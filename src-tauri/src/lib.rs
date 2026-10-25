mod catalogue;
mod discovery;
mod fs_safe;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use catalogue::{Catalogue, CatalogueEvent, MediaItem, ProjectRecord, TechnicalMetadata};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Timelike, Utc};
#[cfg(test)]
use chrono::TimeZone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Default)]
struct NativeState {
    cancelled_jobs: Mutex<HashSet<String>>,
    paused_jobs: Mutex<HashSet<String>>,
    catalogue_write: Mutex<()>,
    discovery_write: Mutex<()>,
    enrichment_write: Mutex<()>,
    job_store: Mutex<()>,
    worker_active: Mutex<bool>,
    availability: Mutex<HashMap<String, (String, bool)>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceSettings {
    device_id: String,
    master_bin_path: String,
    working_projects_path: String,
    catalogue_in_master_bin: bool,
    shared_catalogue_path: String,
    thumbnail_cache_path: String,
    thumbnail_cache_limit_bytes: u64,
    #[serde(default = "legacy_date_mode")]
    ingest_date_organization: String,
    #[serde(default = "legacy_date_folder_pattern")]
    date_folder_pattern: String,
    #[serde(default)]
    initialized_catalogue_path: String,
    #[serde(default)]
    project_paths: HashMap<String, String>,
}
fn legacy_date_mode() -> String {
    "original-file-dates".into()
}
fn legacy_date_folder_pattern() -> String {
    "yyyy-MM-dd".into()
}
impl Default for DeviceSettings {
    fn default() -> Self {
        Self {
            device_id: Uuid::new_v4().to_string(),
            master_bin_path: String::new(),
            working_projects_path: String::new(),
            catalogue_in_master_bin: true,
            shared_catalogue_path: String::new(),
            thumbnail_cache_path: String::new(),
            thumbnail_cache_limit_bytes: 2 * 1024 * 1024 * 1024,
            ingest_date_organization: "today".into(),
            date_folder_pattern: "yyyymmdd".into(),
            initialized_catalogue_path: String::new(),
            project_paths: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IngestRequest {
    job_id: String,
    sources: Vec<String>,
    date_override: Option<String>,
    #[serde(default)]
    renamed_destinations: HashMap<String, String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct InventoryFile {
    source_path: String,
    source_relative_path: String,
    size_bytes: u64,
    kind: String,
    analysis_supported: bool,
    preserve_hierarchy: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct InventoryExclusion {
    path: String,
    reason: String,
    size_bytes: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SourceInventory {
    files: Vec<InventoryFile>,
    excluded: Vec<InventoryExclusion>,
    issues: Vec<String>,
    total_file_count: usize,
    total_bytes: u64,
    copy_file_count: usize,
    copy_bytes: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ProvisionalMetadata {
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    project_ids: Vec<String>,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    favourite: bool,
    #[serde(default)]
    edited_fields: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProvisionalMetadataPatch {
    tags: Option<Vec<String>>,
    project_ids: Option<Vec<String>>,
    notes: Option<String>,
    favourite: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct IngestJobFile {
    id: String,
    media_id: String,
    source_path: String,
    #[serde(default)]
    source_relative_path: String,
    destination_path: String,
    relative_path: String,
    original_filename: String,
    size_bytes: u64,
    bytes_transferred: u64,
    state: String,
    error: Option<String>,
    metadata: ProvisionalMetadata,
    #[serde(default)]
    pending_metadata: Option<ProvisionalMetadata>,
    #[serde(default)]
    metadata_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct IngestJob {
    schema_version: u32,
    id: String,
    state: String,
    created_at: String,
    updated_at: String,
    batch_timestamp: String,
    date_override: Option<String>,
    files: Vec<IngestJobFile>,
    #[serde(default)]
    unsupported: Vec<String>,
    #[serde(default)]
    failures: Vec<String>,
    #[serde(default)]
    inventory_excluded: Vec<InventoryExclusion>,
    #[serde(default)]
    inventory_issues: Vec<String>,
    #[serde(default)]
    inventory_total_file_count: usize,
    #[serde(default)]
    inventory_total_bytes: u64,
    current_file: Option<String>,
    bytes_total: u64,
    bytes_transferred: u64,
    files_completed: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct IngestProgress {
    job_id: String,
    phase: String,
    current_file: String,
    files_complete: usize,
    files_total: usize,
    bytes_complete: u64,
    bytes_total: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct RescanResult {
    registered: usize,
    changed: usize,
    pending: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MediaToolsStatus {
    ffmpeg_available: bool,
    ffprobe_available: bool,
    ffmpeg_version: Option<String>,
    ffprobe_version: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MediaUpdate {
    media_ids: Vec<String>,
    tags: Option<Vec<String>>,
    project_ids: Option<Vec<String>>,
    add_tags: Option<Vec<String>>,
    add_project_ids: Option<Vec<String>>,
    remove_tags: Option<Vec<String>>,
    remove_project_ids: Option<Vec<String>>,
    notes: Option<String>,
    favourite: Option<bool>,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("device-settings.json"))
}
fn jobs_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(settings_path(app)?
        .parent()
        .ok_or("Invalid application configuration path")?
        .join("ingest-jobs.json"))
}
fn load_jobs_unlocked(app: &AppHandle) -> Result<Vec<IngestJob>, String> {
    read_json(&jobs_path(app)?)
}
fn mutate_jobs<T>(
    app: &AppHandle,
    state: &NativeState,
    operation: impl FnOnce(&mut Vec<IngestJob>) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = state.job_store.lock().unwrap();
    let mut jobs = load_jobs_unlocked(app)?;
    let result = operation(&mut jobs)?;
    write_json(&jobs_path(app)?, &jobs)?;
    let _ = app.emit("ingest-jobs-changed", &jobs);
    Ok(result)
}
fn snapshot_jobs(app: &AppHandle, state: &NativeState) -> Result<Vec<IngestJob>, String> {
    let _guard = state.job_store.lock().unwrap();
    load_jobs_unlocked(app)
}
fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> Result<T, String> {
    if !path.exists() {
        let backup = path.with_file_name(format!(
            "{}.backup",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
        if backup.is_file() {
            fs::copy(&backup, path).map_err(|error| {
                format!(
                    "Cannot recover {} from {}: {error}",
                    path.display(),
                    backup.display()
                )
            })?;
        } else {
            return Ok(T::default());
        }
    }
    let data = fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    serde_json::from_slice(&data).map_err(|e| format!("Invalid {}: {e}", path.display()))
}
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path.parent().ok_or("Invalid metadata path")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap().to_string_lossy(),
        Uuid::new_v4()
    ));
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    }
    if path.exists() {
        let backup = path.with_file_name(format!(
            "{}.backup",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
        fs::copy(path, &backup)
            .map_err(|error| format!("Cannot create recoverable metadata backup: {error}"))?;
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&backup)
            .and_then(|file| file.sync_all())
            .map_err(|error| format!("Cannot flush metadata backup: {error}"))?;
        let retired = path.with_file_name(format!(
            "{}.retired",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
        if retired.exists() {
            fs::remove_file(&retired).map_err(|error| error.to_string())?;
        }
        fs::rename(path, &retired)
            .map_err(|error| format!("Cannot prepare metadata replacement: {error}"))?;
        if let Err(error) = fs::rename(&temp, path) {
            let _ = fs::rename(&retired, path);
            let _ = fs::remove_file(&temp);
            return Err(format!("Could not promote metadata update: {error}"));
        }
        fs::remove_file(retired).map_err(|error| error.to_string())?;
        return Ok(());
    }
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(format!("Could not promote metadata update: {error}"));
    }
    Ok(())
}
fn load_settings_file(app: &AppHandle) -> Result<DeviceSettings, String> {
    load_or_initialize_settings(&settings_path(app)?)
}

fn load_or_initialize_settings(path: &Path) -> Result<DeviceSettings, String> {
    if path.exists()
        || path
            .with_file_name(format!(
                "{}.backup",
                path.file_name().unwrap_or_default().to_string_lossy()
            ))
            .exists()
    {
        return read_json(path);
    }
    let settings = DeviceSettings::default();
    write_json(path, &settings)?;
    Ok(settings)
}
fn catalogue_dir(settings: &DeviceSettings) -> Result<PathBuf, String> {
    if settings.master_bin_path.trim().is_empty() {
        return Err("Configure a Master Bin first".into());
    }
    if settings.catalogue_in_master_bin {
        Ok(PathBuf::from(&settings.master_bin_path).join(".pimp"))
    } else if settings.shared_catalogue_path.trim().is_empty() {
        Err("Choose an alternative shared catalogue location".into())
    } else {
        Ok(PathBuf::from(&settings.shared_catalogue_path))
    }
}
fn load_catalogue_file(settings: &DeviceSettings) -> Result<Catalogue, String> {
    let root = catalogue_dir(settings)?;
    catalogue::migrate_legacy_snapshot(&root, &settings.device_id)?;
    catalogue::load(&root)
}
fn append_events(settings: &DeviceSettings, events: Vec<CatalogueEvent>) -> Result<(), String> {
    catalogue::append(&catalogue_dir(settings)?, &settings.device_id, events)
}

#[tauri::command]
fn load_settings(app: AppHandle) -> Result<DeviceSettings, String> {
    load_settings_file(&app)
}

fn preserve_device_identity(current: &str, requested: &str) -> Result<String, String> {
    if !current.is_empty() {
        if requested.is_empty() || requested == current {
            return Ok(current.to_string());
        }
        return Err("Device identity is immutable after initialization".into());
    }
    Uuid::parse_str(requested)
        .map(|value| value.to_string())
        .map_err(|_| "Device identity must be a UUID generated for this device".into())
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<'_, NativeState>,
    mut settings: DeviceSettings,
) -> Result<DeviceSettings, String> {
    if settings.master_bin_path.trim().is_empty()
        || settings.working_projects_path.trim().is_empty()
    {
        return Err("Master Bin and Working Projects directories are required".into());
    }
    format_date_folder(
        &settings.date_folder_pattern,
        Local::now().date_naive(),
        Local::now().time(),
    )?;
    fs::create_dir_all(settings.master_bin_path.trim())
        .map_err(|e| format!("Cannot create Master Bin: {e}"))?;
    fs::create_dir_all(settings.working_projects_path.trim())
        .map_err(|e| format!("Cannot create Working Projects directory: {e}"))?;
    let next_catalogue = catalogue_dir(&settings)?.to_string_lossy().to_string();
    let current = load_settings_file(&app)?;
    settings.device_id = preserve_device_identity(&current.device_id, &settings.device_id)?;
    if !current.initialized_catalogue_path.is_empty()
        && current.initialized_catalogue_path != next_catalogue
        && Path::new(&current.initialized_catalogue_path).exists()
    {
        return Err("Catalogue migration is not implemented. Keep the initialized catalogue location, or move it manually with PIMP closed before changing this setting.".into());
    }
    fs::create_dir_all(&next_catalogue)
        .map_err(|e| format!("Cannot create catalogue directory: {e}"))?;
    if settings.thumbnail_cache_path.trim().is_empty() {
        settings.thumbnail_cache_path = settings_path(&app)?
            .parent()
            .unwrap()
            .join("thumbnails")
            .to_string_lossy()
            .to_string();
    }
    fs::create_dir_all(&settings.thumbnail_cache_path)
        .map_err(|e| format!("Cannot create thumbnail cache: {e}"))?;
    settings.initialized_catalogue_path = next_catalogue;
    let _guard = state.catalogue_write.lock().unwrap();
    let root = catalogue_dir(&settings)?;
    catalogue::migrate_legacy_snapshot(&root, &settings.device_id)?;
    write_json(&settings_path(&app)?, &settings)?;
    Ok(settings)
}

#[tauri::command]
fn load_catalogue(app: AppHandle, state: State<'_, NativeState>) -> Result<Catalogue, String> {
    let settings = load_settings_file(&app)?;
    let mut catalogue = load_catalogue_file(&settings)?;
    let master = PathBuf::from(&settings.master_bin_path);
    for item in &mut catalogue.media {
        item.available = fs_safe::resolve_existing(&master, &item.relative_path)
            .map(|path| path.is_file())
            .unwrap_or(false);
        if let Some(evidence) = item.integrity_by_device.get(&settings.device_id) {
            item.observed_size = Some(evidence.observed_size);
            item.observed_modified_millis = evidence.observed_modified_millis;
            item.needs_review = evidence.needs_review;
        }
    }
    for project in &mut catalogue.projects {
        if let Some(path) = settings.project_paths.get(&project.id) {
            project.working_directory = path.clone();
        }
    }
    *state.availability.lock().unwrap() = catalogue
        .media
        .iter()
        .map(|item| (item.id.clone(), (item.relative_path.clone(), item.available)))
        .collect();
    Ok(catalogue)
}

#[tauri::command]
fn refresh_media_availability(
    app: AppHandle,
    state: State<'_, NativeState>,
) -> Result<bool, String> {
    let settings = load_settings_file(&app)?;
    let master = PathBuf::from(&settings.master_bin_path);
    let mut availability = state.availability.lock().unwrap();
    let mut changed = false;
    for (_, (relative_path, available)) in availability.iter_mut() {
        let next = fs_safe::resolve_existing(&master, relative_path)
            .map(|path| path.is_file())
            .unwrap_or(false);
        if next != *available {
            *available = next;
            changed = true;
        }
    }
    Ok(changed)
}

#[tauri::command]
fn catalogue_signature(app: AppHandle) -> Result<String, String> {
    let settings = load_settings_file(&app)?;
    catalogue::event_signature(&catalogue_dir(&settings)?)
}
fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn media_kind(path: &Path) -> &'static str {
    match extension(path).as_str() {
        "mp4" | "mov" | "m4v" | "mxf" | "h264" | "h265" | "hevc" | "mts"
        | "m2ts" | "avi" | "r3d" | "braw" => "video",
        "wav" | "bwf" | "aif" | "aiff" | "mp3" | "m4a" => "audio",
        "jpg" | "jpeg" | "png" | "tif" | "tiff" | "dng" | "arw" | "cr2"
        | "cr3" => "image",
        "xml" | "xmp" | "bim" | "thm" | "cpi" | "mpl" | "bdm" | "idx"
        | "ind" | "pro" | "dat" | "bin" | "srt" | "lrv" | "csv" | "json" => {
            "camera-support"
        }
        _ => "unknown",
    }
}

fn supported(path: &Path) -> bool {
    matches!(
        extension(path).as_str(),
        "mp4" | "mov" | "m4v" | "mxf" | "h264" | "h265" | "hevc" | "mts"
            | "m2ts" | "avi" | "wav" | "bwf" | "aif" | "aiff" | "mp3" | "m4a"
            | "jpg" | "jpeg" | "png" | "tif" | "tiff" | "dng" | "arw" | "cr2"
            | "cr3" | "r3d" | "braw"
    )
}

fn analysis_supported(path: &Path) -> bool {
    matches!(extension(path).as_str(), "mp4" | "mov" | "m4v" | "wav")
}

fn exclusion_reason(path: &Path) -> Option<&'static str> {
    let name = path
        .file_name()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if path.components().any(|component| component.as_os_str() == ".pimp") {
        Some("Kairix PIMP application data")
    } else if name.starts_with(".syncthing.") {
        Some("Syncthing temporary file")
    } else if name.contains(".sync-conflict-") {
        Some("Syncthing conflict copy")
    } else if name.ends_with(".tmp")
        || name.ends_with(".part")
        || name.ends_with(".partial")
        || name.ends_with(".crdownload")
        || name.contains(".pimp-part-")
    {
        Some("Incomplete or temporary file")
    } else {
        None
    }
}

fn ignored(path: &Path) -> bool {
    exclusion_reason(path).is_some()
}

fn package_component(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component.as_os_str().to_string_lossy().to_ascii_uppercase().as_str(),
            "XDROOT" | "PRIVATE" | "AVCHD" | "BPAV" | "CONTENTS" | "M4ROOT" | "DCIM"
                | "CLIP" | "SUB" | "GENERAL" | "SONY"
        )
    })
}

fn portable_relative(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn inventory_candidate(
    inventory: &mut SourceInventory,
    candidate: &Path,
    relative: &Path,
    root_is_package: bool,
) {
    let size = candidate.metadata().ok().map(|metadata| metadata.len());
    inventory.total_file_count += 1;
    inventory.total_bytes += size.unwrap_or(0);
    if let Some(reason) = exclusion_reason(candidate) {
        inventory.excluded.push(InventoryExclusion {
            path: candidate.to_string_lossy().into(),
            reason: reason.into(),
            size_bytes: size,
        });
        return;
    }
    if let Err(error) = File::open(candidate) {
        inventory.issues.push(format!(
            "Cannot read discovered file {}: {error}",
            candidate.display()
        ));
        return;
    }
    let mut source_relative = relative.to_path_buf();
    if root_is_package {
        if let Some(root_name) = candidate
            .ancestors()
            .nth(relative.components().count())
            .and_then(Path::file_name)
        {
            source_relative = PathBuf::from(root_name).join(relative);
        }
    }
    inventory.files.push(InventoryFile {
        source_path: candidate.to_string_lossy().into(),
        source_relative_path: portable_relative(&source_relative),
        size_bytes: size.unwrap_or(0),
        kind: media_kind(candidate).into(),
        analysis_supported: analysis_supported(candidate),
        preserve_hierarchy: root_is_package || package_component(relative),
    });
}

fn build_source_inventory(sources: &[String]) -> SourceInventory {
    let mut inventory = SourceInventory::default();
    for source in sources {
        let path = PathBuf::from(source);
        if path.is_file() {
            let relative = PathBuf::from(path.file_name().unwrap_or_default());
            inventory_candidate(&mut inventory, &path, &relative, false);
        } else if path.is_dir() {
            let root_is_package = package_component(&path);
            for result in WalkDir::new(&path).follow_links(false) {
                match result {
                    Ok(entry) if entry.file_type().is_file() => {
                        let relative = entry.path().strip_prefix(&path).unwrap_or(entry.path());
                        inventory_candidate(
                            &mut inventory,
                            entry.path(),
                            relative,
                            root_is_package,
                        );
                    }
                    Ok(_) => {}
                    Err(error) => inventory.issues.push(format!(
                        "Cannot inventory {}: {error}",
                        error.path().unwrap_or(&path).display()
                    )),
                }
            }
        } else {
            inventory
                .issues
                .push(format!("Source is unavailable: {}", path.display()));
        }
    }
    inventory.files.sort_by(|left, right| left.source_path.cmp(&right.source_path));
    inventory.files.dedup_by(|left, right| left.source_path == right.source_path);
    let mut names = HashMap::<String, usize>::new();
    for file in &inventory.files {
        let name = Path::new(&file.source_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        *names.entry(name).or_default() += 1;
    }
    for file in &mut inventory.files {
        let name = Path::new(&file.source_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        file.preserve_hierarchy |= names.get(&name).copied().unwrap_or(0) > 1;
    }
    inventory.copy_file_count = inventory.files.len();
    inventory.copy_bytes = inventory.files.iter().map(|file| file.size_bytes).sum();
    inventory
}

#[tauri::command]
fn inventory_sources(sources: Vec<String>) -> SourceInventory {
    build_source_inventory(&sources)
}
fn sha256(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn destination_date(
    settings: &DeviceSettings,
    source: &Path,
    override_date: &Option<String>,
    batch_timestamp: &DateTime<Local>,
) -> Result<(String, Option<String>, String), String> {
    let modified = source
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .map(DateTime::<Local>::from);
    destination_date_from_modified(settings, modified, override_date, batch_timestamp)
}

fn destination_date_from_modified(
    settings: &DeviceSettings,
    modified: Option<DateTime<Local>>,
    override_date: &Option<String>,
    batch_timestamp: &DateTime<Local>,
) -> Result<(String, Option<String>, String), String> {
    let has_modified = modified.is_some();
    let capture = modified
        .as_ref()
        .map(|d| d.format("%Y-%m-%dT%H:%M:%S%:z").to_string());
    let (date, time) = match override_date.as_deref() {
        Some("today") => (batch_timestamp.date_naive(), batch_timestamp.time()),
        Some("yesterday") => (
            batch_timestamp.date_naive() - Duration::days(1),
            batch_timestamp.time(),
        ),
        Some(date) => {
            let date = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|_| "Date override must use YYYY-MM-DD".to_string())?;
            (date, batch_timestamp.time())
        }
        None if settings.ingest_date_organization == "original-file-dates" => {
            modified
                .as_ref()
                .map(|value| (value.date_naive(), value.time()))
                .unwrap_or((batch_timestamp.date_naive(), batch_timestamp.time()))
        }
        None => (batch_timestamp.date_naive(), batch_timestamp.time()),
    };
    Ok((
        format_date_folder(&settings.date_folder_pattern, date, time)?,
        capture,
        if has_modified {
            "filesystem".into()
        } else {
            "unknown".into()
        },
    ))
}

fn format_date_folder(
    pattern: &str,
    date: NaiveDate,
    time: chrono::NaiveTime,
) -> Result<String, String> {
    let pattern = if pattern == "yyyymmdd" {
        "yyyyMMdd"
    } else {
        pattern
    };
    if pattern.trim().is_empty()
        || pattern.contains(['/', '\\'])
        || pattern.chars().any(char::is_control)
    {
        return Err("Date folder pattern must be a non-empty filename, not a path".into());
    }
    let mut output = String::new();
    let mut offset = 0;
    let tokens = ["yyyy", "yy", "MM", "dd", "HH", "hh", "mm", "ss", "a"];
    while offset < pattern.len() {
        let tail = &pattern[offset..];
        if let Some(token) = tokens.iter().find(|token| tail.starts_with(**token)) {
            let value = match *token {
                "yyyy" => format!("{:04}", date.year()),
                "yy" => format!("{:02}", date.year().rem_euclid(100)),
                "MM" => format!("{:02}", date.month()),
                "dd" => format!("{:02}", date.day()),
                "HH" => format!("{:02}", time.hour()),
                "hh" => format!(
                    "{:02}",
                    match time.hour() % 12 {
                        0 => 12,
                        hour => hour,
                    }
                ),
                "mm" => format!("{:02}", time.minute()),
                "ss" => format!("{:02}", time.second()),
                "a" => if time.hour() < 12 { "AM" } else { "PM" }.into(),
                _ => unreachable!(),
            };
            output.push_str(&value);
            offset += token.len();
        } else {
            let character = tail.chars().next().unwrap();
            if r#"<>:\"|?*"#.contains(character) {
                return Err(format!(
                    "Illegal character in date folder pattern: {character}"
                ));
            }
            output.push(character);
            offset += character.len_utf8();
        }
    }
    fs_safe::validate_relative(&output)?;
    Ok(output)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IngestJournal {
    job_id: String,
    source_path: String,
    destination_path: String,
    staged_name: String,
    status: String,
    detail: Option<String>,
    #[serde(default)]
    media: Option<MediaItem>,
}

struct VerifiedCopy {
    hash: String,
    journal_path: PathBuf,
}

#[derive(Default)]
struct RecoverySummary {
    recovered: usize,
    needs_review: usize,
}

fn update_job_progress(
    app: &AppHandle,
    state: &NativeState,
    job_id: &str,
    file_id: &str,
    phase: &str,
    file_bytes: u64,
    error: Option<String>,
) -> Result<(), String> {
    mutate_jobs(app, state, |jobs| {
        let job = jobs
            .iter_mut()
            .find(|job| job.id == job_id)
            .ok_or("Ingest job no longer exists")?;
        let file = job
            .files
            .iter_mut()
            .find(|file| file.id == file_id)
            .ok_or("Ingest job file no longer exists")?;
        file.state = phase.into();
        file.bytes_transferred = file_bytes.min(file.size_bytes);
        file.error = error;
        job.state = phase.into();
        job.current_file = Some(file.original_filename.clone());
        job.bytes_transferred = job.files.iter().map(|file| file.bytes_transferred).sum();
        job.files_completed = job
            .files
            .iter()
            .filter(|file| file.state == "completed")
            .count();
        job.updated_at = Utc::now().to_rfc3339();
        Ok(())
    })
}

fn remove_journal(path: &Path) -> Result<(), String> {
    if path.is_file() {
        fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    let backup = path.with_file_name(format!(
        "{}.backup",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    if backup.is_file() {
        fs::remove_file(backup).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn recover_staging(
    master: &Path,
    device_id: &str,
    catalogue_root: Option<&Path>,
) -> Result<RecoverySummary, String> {
    let root = fs_safe::staging_root(master, device_id)?;
    let jobs = root.join("jobs");
    fs::create_dir_all(&jobs).map_err(|error| error.to_string())?;
    let mut summary = RecoverySummary::default();
    for entry in fs::read_dir(&jobs).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let journal: IngestJournal = match read_json(&entry.path()) {
            Ok(journal) => journal,
            Err(error) => {
                return Err(format!(
                    "Invalid ingest recovery record {}; staging was left untouched: {error}",
                    entry.path().display()
                ))
            }
        };
        if Path::new(&journal.staged_name)
            .file_name()
            .and_then(|value| value.to_str())
            != Some(journal.staged_name.as_str())
        {
            return Err(format!(
                "Unsafe staging filename in {}; staging was left untouched",
                entry.path().display()
            ));
        }
        if !matches!(
            journal.status.as_str(),
            "copying" | "verified" | "committed" | "failed" | "cancelled" | "interrupted"
        ) {
            return Err(format!(
                "Unknown ingest lifecycle state in {}; staging was left untouched",
                entry.path().display()
            ));
        }
        let staged = root.join("files").join(&journal.staged_name);
        let destination = PathBuf::from(&journal.destination_path);
        if destination.exists() {
            let master = master
                .canonicalize()
                .map_err(|error| format!("Cannot resolve Master Bin during recovery: {error}"))?;
            let destination = destination
                .canonicalize()
                .map_err(|error| error.to_string())?;
            if !destination.starts_with(&master) {
                return Err(format!(
                    "Unsafe recovery destination in {}; staging was left untouched",
                    entry.path().display()
                ));
            }
        }
        if journal.status == "verified" {
            // A verified staged file is valuable and may be awaiting promotion.
            // Without a complete committed record, preserve both files and journal.
            summary.needs_review += 1;
            continue;
        }
        if journal.status == "committed" {
            let (Some(media), Some(catalogue_root)) = (&journal.media, catalogue_root) else {
                summary.needs_review += 1;
                continue;
            };
            if !destination.is_file() {
                summary.needs_review += 1;
                continue;
            }
            let expected = match fs_safe::resolve_existing(master, &media.relative_path) {
                Ok(expected) => expected,
                Err(_) => {
                    summary.needs_review += 1;
                    continue;
                }
            };
            let actual = destination
                .canonicalize()
                .map_err(|error| error.to_string())?;
            if actual != expected
                || actual.metadata().map_err(|error| error.to_string())?.len()
                    != media.technical.size_bytes
                || sha256(&actual)? != media.sha256
            {
                summary.needs_review += 1;
                continue;
            }
            let catalogue = catalogue::load(catalogue_root)?;
            if let Some(existing) = catalogue.media.iter().find(|item| {
                item.id == media.id || item.relative_path == media.relative_path
            }) {
                if existing.id != media.id
                    || existing.relative_path != media.relative_path
                    || existing.sha256 != media.sha256
                {
                    summary.needs_review += 1;
                    continue;
                }
            } else {
                catalogue::append(
                    catalogue_root,
                    device_id,
                    vec![CatalogueEvent::MediaRegistered {
                        media: media.clone(),
                    }],
                )?;
            }
        }
        if staged.is_file() {
            fs::remove_file(&staged).map_err(|error| error.to_string())?;
        }
        // A committed journal owns only its staging artifact. The promoted
        // destination is durable user media and is reconciled by Rescan.
        remove_journal(&entry.path())?;
        summary.recovered += 1;
    }
    Ok(summary)
}

fn copy_verified(
    app: &AppHandle,
    state: &NativeState,
    settings: &DeviceSettings,
    job_id: &str,
    file_id: &str,
    source: &Path,
    destination: &Path,
    base_bytes: u64,
    total_bytes: u64,
    file_index: usize,
    file_total: usize,
) -> Result<VerifiedCopy, String> {
    let before = fs_safe::snapshot(source)?;
    let staging = fs_safe::staging_root(Path::new(&settings.master_bin_path), &settings.device_id)?;
    let staged_name = format!("{}.part", Uuid::new_v4());
    let temp = staging.join("files").join(&staged_name);
    let journal_path = staging
        .join("jobs")
        .join(format!("{}.json", Uuid::new_v4()));
    fs::create_dir_all(journal_path.parent().unwrap()).map_err(|error| error.to_string())?;
    let mut journal = IngestJournal {
        job_id: job_id.into(),
        source_path: source.to_string_lossy().to_string(),
        destination_path: destination.to_string_lossy().to_string(),
        staged_name,
        status: "copying".into(),
        detail: None,
        media: None,
    };
    write_json(&journal_path, &journal)?;
    let operation: Result<String, String> = (|| {
        let mut input =
            File::open(source).map_err(|error| format!("Cannot read source: {error}"))?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| format!("Cannot create staging file: {error}"))?;
        let mut copied = 0u64;
        let mut source_hash = Sha256::new();
        let mut buffer = vec![0u8; 1024 * 1024];
        let mut next_persist = 0u64;
        loop {
            if state.cancelled_jobs.lock().unwrap().contains(job_id) {
                return Err("__cancelled__".into());
            }
            let read = input
                .read(&mut buffer)
                .map_err(|error| format!("Interrupted source read: {error}"))?;
            if read == 0 {
                break;
            }
            source_hash.update(&buffer[..read]);
            output
                .write_all(&buffer[..read])
                .map_err(|error| format!("Interrupted staging write: {error}"))?;
            copied += read as u64;
            if copied >= next_persist {
                update_job_progress(app, state, job_id, file_id, "copying", copied, None)?;
                next_persist = copied.saturating_add(8 * 1024 * 1024);
            }
            let _ = app.emit(
                "ingest-progress",
                IngestProgress {
                    job_id: job_id.into(),
                    phase: "copying".into(),
                    current_file: source.to_string_lossy().to_string(),
                    files_complete: file_index,
                    files_total: file_total,
                    bytes_complete: base_bytes + copied,
                    bytes_total: total_bytes,
                },
            );
        }
        output.sync_all().map_err(|error| error.to_string())?;
        drop(output);
        update_job_progress(app, state, job_id, file_id, "verifying", copied, None)?;
        let _ = app.emit(
            "ingest-progress",
            IngestProgress {
                job_id: job_id.into(),
                phase: "verifying".into(),
                current_file: source.to_string_lossy().to_string(),
                files_complete: file_index,
                files_total: file_total,
                bytes_complete: base_bytes + copied,
                bytes_total: total_bytes,
            },
        );
        let copied_source_hash = format!("{:x}", source_hash.finalize());
        let after = fs_safe::snapshot(source)?;
        if before != after || copied != before.length {
            return Err("Source changed during transfer; staged copy was rejected".into());
        }
        let post_copy_source_hash = sha256(source)?;
        if post_copy_source_hash != copied_source_hash {
            return Err("Source content changed during transfer; staged copy was rejected".into());
        }
        if sha256(&temp)? != copied_source_hash {
            return Err("Independent SHA-256 verification failed".into());
        }
        journal.status = "verified".into();
        write_json(&journal_path, &journal)?;
        fs_safe::promote_no_overwrite(&temp, destination)?;
        journal.status = "committed".into();
        write_json(&journal_path, &journal)?;
        Ok(copied_source_hash)
    })();
    match operation {
        Ok(hash) => Ok(VerifiedCopy { hash, journal_path }),
        Err(error) => {
            let _ = fs::remove_file(&temp);
            journal.status = if error == "__cancelled__" {
                "cancelled".into()
            } else {
                "failed".into()
            };
            journal.detail = Some(error.clone());
            let _ = write_json(&journal_path, &journal);
            if error == "__destination_exists__" {
                Err("__race__".into())
            } else {
                Err(error)
            }
        }
    }
}
fn new_media(
    source: &Path,
    relative: String,
    source_relative_path: Option<String>,
    hash: String,
    capture: Option<String>,
    provenance: String,
    needs_review: bool,
) -> Result<MediaItem, String> {
    let metadata = source.metadata().map_err(|e| e.to_string())?;
    let file_type = source
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    Ok(MediaItem {
        id: Uuid::new_v4().to_string(),
        relative_path: relative,
        source_relative_path,
        original_filename: source.file_name().unwrap().to_string_lossy().to_string(),
        sha256: hash,
        capture_date: capture,
        capture_date_provenance: provenance,
        ingest_date: Utc::now().to_rfc3339(),
        file_type,
        tags: vec![],
        project_ids: vec![],
        notes: String::new(),
        favourite: false,
        technical: TechnicalMetadata {
            size_bytes: metadata.len(),
            ..Default::default()
        },
        thumbnail_path: None,
        attached_audio_id: None,
        available: true,
        needs_review,
        observed_size: Some(metadata.len()),
        observed_modified_millis: metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_millis() as u64),
        integrity_by_device: Default::default(),
    })
}

fn metadata_update_events(item: &MediaItem, metadata: &ProvisionalMetadata) -> Vec<CatalogueEvent> {
    let desired_tags = normalized(&metadata.tags);
    let desired_projects = normalized(&metadata.project_ids);
    let edited = |field: &str| metadata.edited_fields.iter().any(|value| value == field);
    let mut events = Vec::new();
    if edited("tags") {
        events.push(CatalogueEvent::MediaTagsAdded {
            media_id: item.id.clone(),
            values: desired_tags
                .iter()
                .filter(|value| !contains_normalized(&item.tags, value))
                .cloned()
                .collect(),
        });
        events.push(CatalogueEvent::MediaTagsRemoved {
            media_id: item.id.clone(),
            values: item
                .tags
                .iter()
                .filter(|value| !contains_normalized(&desired_tags, value))
                .cloned()
                .collect(),
        });
    }
    if edited("projectIds") {
        events.push(CatalogueEvent::MediaProjectsAdded {
            media_id: item.id.clone(),
            values: desired_projects
                .iter()
                .filter(|value| !contains_normalized(&item.project_ids, value))
                .cloned()
                .collect(),
        });
        events.push(CatalogueEvent::MediaProjectsRemoved {
            media_id: item.id.clone(),
            values: item
                .project_ids
                .iter()
                .filter(|value| !contains_normalized(&desired_projects, value))
                .cloned()
                .collect(),
        });
    }
    if edited("notes") && item.notes != metadata.notes {
        events.push(CatalogueEvent::MediaNotesSet {
            media_id: item.id.clone(),
            value: metadata.notes.clone(),
        });
    }
    if edited("favourite") && item.favourite != metadata.favourite {
        events.push(CatalogueEvent::MediaFavouriteSet {
            media_id: item.id.clone(),
            value: metadata.favourite,
        });
    }
    events.retain(|event| match event {
        CatalogueEvent::MediaTagsAdded { values, .. }
        | CatalogueEvent::MediaTagsRemoved { values, .. }
        | CatalogueEvent::MediaProjectsAdded { values, .. }
        | CatalogueEvent::MediaProjectsRemoved { values, .. } => !values.is_empty(),
        _ => true,
    });
    events
}

fn commit_job_file(
    app: &AppHandle,
    state: &NativeState,
    settings: &DeviceSettings,
    job_id: &str,
    file_id: &str,
    hash: String,
    journal_path: Option<&Path>,
) -> Result<(), String> {
    let _catalogue_guard = state.catalogue_write.lock().unwrap();
    let _jobs_guard = state.job_store.lock().unwrap();
    let mut jobs = load_jobs_unlocked(app)?;
    let job = jobs
        .iter_mut()
        .find(|job| job.id == job_id)
        .ok_or("Ingest job no longer exists")?;
    let file = job
        .files
        .iter_mut()
        .find(|file| file.id == file_id)
        .ok_or("Ingest job file no longer exists")?;
    let destination = PathBuf::from(&file.destination_path);
    let (_, capture, provenance) = destination_date(
        settings,
        &PathBuf::from(&file.source_path),
        &job.date_override,
        &DateTime::parse_from_rfc3339(&job.batch_timestamp)
            .map_err(|error| error.to_string())?
            .with_timezone(&Local),
    )?;
    let mut item = new_media(
        &destination,
        file.relative_path.clone(),
        Some(file.source_relative_path.clone()),
        hash,
        capture,
        provenance,
        false,
    )?;
    item.id = file.media_id.clone();
    item.tags = normalized(&file.metadata.tags);
    item.project_ids = normalized(&file.metadata.project_ids);
    item.notes = file.metadata.notes.clone();
    item.favourite = file.metadata.favourite;
    let catalogue = load_catalogue_file(settings)?;
    let mut events = Vec::new();
    if let Some(existing) = catalogue
        .media
        .iter()
        .find(|existing| existing.id == item.id || existing.relative_path == item.relative_path)
    {
        if existing.sha256 != item.sha256 || existing.relative_path != item.relative_path {
            return Err("Destination conflicts with an existing catalogue record".into());
        }
        file.media_id = existing.id.clone();
        item.id = existing.id.clone();
        events.extend(metadata_update_events(existing, &file.metadata));
    } else {
        events.push(CatalogueEvent::MediaRegistered {
            media: item.clone(),
        });
    }
    if let Some(journal_path) = journal_path {
        let mut journal: IngestJournal = read_json(journal_path)?;
        journal.media = Some(item.clone());
        write_json(journal_path, &journal)?;
    }
    append_events(settings, events)?;
    file.state = "completed".into();
    file.bytes_transferred = file.size_bytes;
    file.error = None;
    job.files_completed = job
        .files
        .iter()
        .filter(|file| file.state == "completed")
        .count();
    job.bytes_transferred = job.files.iter().map(|file| file.bytes_transferred).sum();
    job.updated_at = Utc::now().to_rfc3339();
    write_json(&jobs_path(app)?, &jobs)?;
    if let Some(journal_path) = journal_path {
        remove_journal(journal_path)?;
    }
    let _ = app.emit("catalogue-changed", ());
    let _ = app.emit("ingest-jobs-changed", &jobs);
    Ok(())
}

fn finish_job(app: &AppHandle, state: &NativeState, job_id: &str) -> Result<(), String> {
    mutate_jobs(app, state, |jobs| {
        let job = jobs
            .iter_mut()
            .find(|job| job.id == job_id)
            .ok_or("Ingest job no longer exists")?;
        job.current_file = None;
        job.state = if job.inventory_issues.is_empty()
            && !job.files.is_empty()
            && job.files.iter().all(|file| file.state == "completed")
        {
            "completed"
        } else if job.files.iter().any(|file| file.state == "failed") {
            "failed"
        } else if job.files.iter().any(|file| file.state == "cancelled") {
            "cancelled"
        } else if job.files.iter().any(|file| file.state == "paused") {
            "paused"
        } else {
            "failed"
        }
        .into();
        job.updated_at = Utc::now().to_rfc3339();
        Ok(())
    })
}

fn validate_job_action(state: &str, action: &str) -> Result<(), String> {
    let allowed = match action {
        "pause" => matches!(state, "queued" | "copying" | "verifying"),
        "resume" => state == "paused",
        "retry" => matches!(state, "failed" | "cancelled" | "interrupted"),
        "cancel" => matches!(
            state,
            "queued" | "copying" | "verifying" | "paused" | "interrupted"
        ),
        _ => false,
    };
    allowed.then_some(()).ok_or_else(|| {
        format!("Cannot {action} an ingest job while it is {state}")
    })
}

fn process_ingest_job(app: &AppHandle, job_id: &str) -> Result<(), String> {
    let state = app.state::<NativeState>();
    let settings = load_settings_file(app)?;
    let job = snapshot_jobs(app, &state)?
        .into_iter()
        .find(|job| job.id == job_id)
        .ok_or("Ingest job no longer exists")?;
    let mut completed_bytes = job.bytes_transferred;
    for (index, planned) in job.files.iter().enumerate() {
        if matches!(planned.state.as_str(), "completed" | "failed" | "cancelled") {
            continue;
        }
        if state.cancelled_jobs.lock().unwrap().contains(job_id) {
            update_job_progress(
                app,
                &state,
                job_id,
                &planned.id,
                "cancelled",
                planned.bytes_transferred,
                None,
            )?;
            break;
        }
        if state.paused_jobs.lock().unwrap().contains(job_id) {
            update_job_progress(
                app,
                &state,
                job_id,
                &planned.id,
                "paused",
                planned.bytes_transferred,
                None,
            )?;
            break;
        }
        let source = PathBuf::from(&planned.source_path);
        let destination = PathBuf::from(&planned.destination_path);
        if !source.is_file() {
            update_job_progress(
                app,
                &state,
                job_id,
                &planned.id,
                "failed",
                0,
                Some("Source media is unavailable; reconnect it and retry".into()),
            )?;
            continue;
        }
        if destination.exists() {
            match (sha256(&source), sha256(&destination)) {
                (Ok(source_hash), Ok(destination_hash)) if source_hash == destination_hash => {
                    commit_job_file(
                        app,
                        &state,
                        &settings,
                        job_id,
                        &planned.id,
                        destination_hash,
                        None,
                    )?;
                    completed_bytes = completed_bytes.saturating_add(planned.size_bytes);
                    continue;
                }
                _ => {
                    update_job_progress(
                        app,
                        &state,
                        job_id,
                        &planned.id,
                        "failed",
                        0,
                        Some("A different file already uses the planned destination".into()),
                    )?;
                    continue;
                }
            }
        }
        match copy_verified(
            app,
            &state,
            &settings,
            job_id,
            &planned.id,
            &source,
            &destination,
            completed_bytes,
            job.bytes_total,
            index,
            job.files.len(),
        ) {
            Ok(copy) => {
                if let Err(error) = commit_job_file(
                    app,
                    &state,
                    &settings,
                    job_id,
                    &planned.id,
                    copy.hash,
                    Some(&copy.journal_path),
                ) {
                    update_job_progress(
                        app,
                        &state,
                        job_id,
                        &planned.id,
                        "failed",
                        planned.size_bytes,
                        Some(format!(
                            "Media was promoted but catalogue persistence needs retry: {error}"
                        )),
                    )?;
                } else {
                    completed_bytes = completed_bytes.saturating_add(planned.size_bytes);
                }
            }
            Err(error) if error == "__cancelled__" => {
                update_job_progress(
                    app,
                    &state,
                    job_id,
                    &planned.id,
                    "cancelled",
                    0,
                    None,
                )?;
                break;
            }
            Err(error) => {
                update_job_progress(
                    app,
                    &state,
                    job_id,
                    &planned.id,
                    "failed",
                    0,
                    Some(error),
                )?;
            }
        }
    }
    state.cancelled_jobs.lock().unwrap().remove(job_id);
    finish_job(app, &state, job_id)
}

fn kick_ingest_worker(app: AppHandle) {
    {
        let state = app.state::<NativeState>();
        let mut active = state.worker_active.lock().unwrap();
        if *active {
            return;
        }
        *active = true;
    }
    std::thread::spawn(move || {
        loop {
            let state = app.state::<NativeState>();
            let next = mutate_jobs(&app, &state, |jobs| {
                let Some(job) = jobs.iter_mut().find(|job| job.state == "queued") else {
                    return Ok(None);
                };
                job.state = "copying".into();
                job.updated_at = Utc::now().to_rfc3339();
                Ok(Some(job.id.clone()))
            })
            .ok()
            .flatten();
            let Some(job_id) = next else {
                // Close the enqueue/worker-exit race while holding the same
                // job-store lock used by start_ingest. If a producer queued
                // work after the first snapshot it observed worker_active=true;
                // keep this worker alive to claim it.
                let queued_after_snapshot = {
                    let _jobs_guard = state.job_store.lock().unwrap();
                    let queued = load_jobs_unlocked(&app)
                        .map(|jobs| jobs.iter().any(|job| job.state == "queued"))
                        .unwrap_or(false);
                    if !queued {
                        *state.worker_active.lock().unwrap() = false;
                    }
                    queued
                };
                if queued_after_snapshot {
                    continue;
                }
                break;
            };
            if let Err(error) = process_ingest_job(&app, &job_id) {
                let _ = mutate_jobs(&app, &state, |jobs| {
                    if let Some(job) = jobs.iter_mut().find(|job| job.id == job_id) {
                        job.state = "failed".into();
                        job.failures.push(error);
                        job.updated_at = Utc::now().to_rfc3339();
                    }
                    Ok(())
                });
            }
        }
    });
}

fn active_destination_reservations(jobs: &[IngestJob]) -> HashSet<String> {
    jobs.iter()
        .filter(|job| !matches!(job.state.as_str(), "completed" | "cancelled"))
        .flat_map(|job| job.files.iter().map(|file| file.destination_path.clone()))
        .collect()
}

#[tauri::command]
fn start_ingest(
    app: AppHandle,
    state: State<'_, NativeState>,
    request: IngestRequest,
) -> Result<IngestJob, String> {
    let settings = load_settings_file(&app)?;
    let master = PathBuf::from(&settings.master_bin_path);
    let inventory = build_source_inventory(&request.sources);
    let unsupported = inventory
        .files
        .iter()
        .filter(|file| !file.analysis_supported)
        .map(|file| file.source_path.clone())
        .collect();
    let batch_timestamp = Local::now();
    let reserved = active_destination_reservations(&snapshot_jobs(&app, &state)?);
    let mut planned_paths = HashSet::new();
    let mut files = Vec::new();
    for inventory_file in &inventory.files {
        let source = PathBuf::from(&inventory_file.source_path);
        let (date, _, _) = destination_date(
            &settings,
            &source,
            &request.date_override,
            &batch_timestamp,
        )?;
        let compact_date = date.replace('-', "");
        let date_folder = if master.join(&date).is_dir() {
            date
        } else if master.join(&compact_date).is_dir() {
            compact_date
        } else {
            date
        };
        let filename = request
            .renamed_destinations
            .get(&source.to_string_lossy().to_string())
            .cloned()
            .unwrap_or_else(|| source.file_name().unwrap().to_string_lossy().to_string());
        if Path::new(&filename).file_name().and_then(|value| value.to_str())
            != Some(filename.as_str())
        {
            return Err(format!("{} has an invalid destination filename", source.display()));
        }
        let destination_suffix = if inventory_file.preserve_hierarchy {
            let source_relative = PathBuf::from(&inventory_file.source_relative_path);
            source_relative
                .parent()
                .map(|parent| parent.join(&filename))
                .unwrap_or_else(|| PathBuf::from(&filename))
        } else {
            PathBuf::from(&filename)
        };
        let relative_path = format!(
            "{date_folder}/{}",
            portable_relative(&destination_suffix)
        );
        let destination = fs_safe::resolve_for_create(&master, &relative_path)?;
        let destination_path = destination.to_string_lossy().to_string();
        let collision = reserved.contains(&destination_path) || !planned_paths.insert(destination_path.clone());
        files.push(IngestJobFile {
            id: Uuid::new_v4().to_string(),
            media_id: Uuid::new_v4().to_string(),
            source_path: source.to_string_lossy().to_string(),
            source_relative_path: inventory_file.source_relative_path.clone(),
            destination_path,
            relative_path,
            original_filename: source.file_name().unwrap().to_string_lossy().to_string(),
            size_bytes: inventory_file.size_bytes,
            bytes_transferred: 0,
            state: if collision { "failed" } else { "queued" }.into(),
            error: collision.then(|| "Another queued ingest already reserves this destination".into()),
            metadata: ProvisionalMetadata::default(),
            pending_metadata: None,
            metadata_error: None,
        });
    }
    let now = Utc::now().to_rfc3339();
    let job = IngestJob {
        schema_version: 3,
        id: request.job_id,
        state: if files.iter().any(|file| file.state == "queued") {
            "queued"
        } else {
            "failed"
        }
        .into(),
        created_at: now.clone(),
        updated_at: now,
        batch_timestamp: batch_timestamp.to_rfc3339(),
        date_override: request.date_override,
        bytes_total: files.iter().map(|file| file.size_bytes).sum(),
        bytes_transferred: 0,
        files_completed: 0,
        current_file: None,
        files,
        unsupported,
        failures: Vec::new(),
        inventory_excluded: inventory.excluded,
        inventory_issues: inventory.issues,
        inventory_total_file_count: inventory.total_file_count,
        inventory_total_bytes: inventory.total_bytes,
    };
    mutate_jobs(&app, &state, |jobs| {
        if jobs.iter().any(|existing| existing.id == job.id) {
            return Err("An ingest job with this ID already exists".into());
        }
        jobs.push(job.clone());
        Ok(())
    })?;
    kick_ingest_worker(app);
    Ok(job)
}

#[tauri::command]
fn list_ingest_jobs(app: AppHandle, state: State<'_, NativeState>) -> Result<Vec<IngestJob>, String> {
    snapshot_jobs(&app, &state)
}

fn mark_active_jobs_interrupted(jobs: &mut [IngestJob]) {
    for job in jobs {
        if matches!(job.state.as_str(), "queued" | "copying" | "verifying") {
            job.state = "interrupted".into();
            job.current_file = None;
            job.updated_at = Utc::now().to_rfc3339();
            for file in &mut job.files {
                if matches!(file.state.as_str(), "queued" | "copying" | "verifying") {
                    file.state = "interrupted".into();
                    file.error = Some("Application stopped before this file completed".into());
                }
            }
        }
    }
}

fn prepare_job_retry(job: &mut IngestJob) {
    for file in &mut job.files {
        if matches!(file.state.as_str(), "failed" | "cancelled" | "interrupted") {
            file.state = "queued".into();
            file.error = None;
            file.bytes_transferred = 0;
        }
    }
    job.state = "queued".into();
    job.failures.clear();
    job.bytes_transferred = job.files.iter().map(|file| file.bytes_transferred).sum();
    job.updated_at = Utc::now().to_rfc3339();
}

fn verified_completed_media(
    master: &Path,
    file: &IngestJobFile,
    catalogue: &Catalogue,
) -> Result<String, String> {
    let candidates: Vec<_> = catalogue
        .media
        .iter()
        .filter(|media| media.id == file.media_id || media.relative_path == file.relative_path)
        .collect();
    if candidates.len() != 1 {
        return Err(if candidates.is_empty() {
            "No authoritative catalogue registration matches this job file".into()
        } else {
            "Conflicting catalogue registrations match this job file".into()
        });
    }
    let media = candidates[0];
    if media.id != file.media_id || media.relative_path != file.relative_path {
        return Err("Catalogue identity or relative path disagrees with the persisted job plan".into());
    }
    let planned = PathBuf::from(&file.destination_path);
    if !planned.is_file() {
        return Err("The planned destination is missing or incomplete".into());
    }
    let expected = fs_safe::resolve_existing(master, &file.relative_path)
        .map_err(|error| format!("The planned destination is outside the configured root: {error}"))?;
    let actual = planned
        .canonicalize()
        .map_err(|error| format!("Cannot resolve the planned destination: {error}"))?;
    if actual != expected {
        return Err("The persisted destination path disagrees with its relative path".into());
    }
    let size = actual
        .metadata()
        .map_err(|error| format!("Cannot inspect the planned destination: {error}"))?
        .len();
    if size != file.size_bytes || size != media.technical.size_bytes {
        return Err("Destination size disagrees with the job or catalogue record".into());
    }
    if sha256(&actual)? != media.sha256 {
        return Err("Destination SHA-256 disagrees with the catalogue record".into());
    }
    Ok(media.id.clone())
}

#[tauri::command]
fn initialize_ingest_jobs(
    app: AppHandle,
    state: State<'_, NativeState>,
) -> Result<Vec<IngestJob>, String> {
    if *state.worker_active.lock().unwrap() {
        return Err("Cannot run startup recovery while an ingest worker is active".into());
    }
    mutate_jobs(&app, &state, |jobs| {
        mark_active_jobs_interrupted(jobs);
        Ok(())
    })?;
    let settings = load_settings_file(&app)?;
    if !settings.master_bin_path.trim().is_empty() {
        let _catalogue_guard = state.catalogue_write.lock().unwrap();
        let catalogue_root = catalogue_dir(&settings)?;
        let recovery = recover_staging(
            Path::new(&settings.master_bin_path),
            &settings.device_id,
            Some(&catalogue_root),
        )?;
        let catalogue = load_catalogue_file(&settings)?;
        let master = Path::new(&settings.master_bin_path);
        mutate_jobs(&app, &state, |jobs| {
            for job in jobs {
                for file in &mut job.files {
                    if file.state == "completed" {
                        continue;
                    }
                    let has_catalogue_candidate = catalogue.media.iter().any(|media| {
                        media.id == file.media_id || media.relative_path == file.relative_path
                    });
                    if file.state != "interrupted"
                        && !Path::new(&file.destination_path).exists()
                        && !has_catalogue_candidate
                    {
                        continue;
                    }
                    match verified_completed_media(master, file, &catalogue) {
                        Ok(media_id) => {
                            file.media_id = media_id;
                            file.state = "completed".into();
                            file.bytes_transferred = file.size_bytes;
                            file.error = None;
                        }
                        Err(error) => {
                            file.error = Some(format!("Startup reconciliation: {error}"));
                        }
                    }
                }
                job.files_completed = job
                    .files
                    .iter()
                    .filter(|file| file.state == "completed")
                    .count();
                job.bytes_transferred = job.files.iter().map(|file| file.bytes_transferred).sum();
                if job.files_completed == job.files.len() && !job.files.is_empty() {
                    job.state = "completed".into();
                }
                if recovery.needs_review > 0 && job.state == "interrupted" {
                    job.failures.push(format!(
                        "{} staging operation(s) require manual review",
                        recovery.needs_review
                    ));
                }
            }
            Ok(())
        })?;
    }
    recover_pending_metadata(&app, &state)?;
    snapshot_jobs(&app, &state)
}

fn apply_metadata_patch(
    metadata: &mut ProvisionalMetadata,
    patch: &ProvisionalMetadataPatch,
) {
    if let Some(tags) = &patch.tags {
        metadata.tags = normalized(tags);
        if !metadata.edited_fields.iter().any(|value| value == "tags") {
            metadata.edited_fields.push("tags".into());
        }
    }
    if let Some(project_ids) = &patch.project_ids {
        metadata.project_ids = normalized(project_ids);
        if !metadata.edited_fields.iter().any(|value| value == "projectIds") {
            metadata.edited_fields.push("projectIds".into());
        }
    }
    if let Some(notes) = &patch.notes {
        metadata.notes = notes.clone();
        if !metadata.edited_fields.iter().any(|value| value == "notes") {
            metadata.edited_fields.push("notes".into());
        }
    }
    if let Some(favourite) = patch.favourite {
        metadata.favourite = favourite;
        if !metadata.edited_fields.iter().any(|value| value == "favourite") {
            metadata.edited_fields.push("favourite".into());
        }
    }
}

fn apply_pending_metadata_write(
    file: &mut IngestJobFile,
    catalogue: &Catalogue,
    append: impl FnOnce(Vec<CatalogueEvent>) -> Result<(), String>,
) -> Result<(), String> {
    let pending = file
        .pending_metadata
        .clone()
        .ok_or("No pending metadata write exists")?;
    let outcome = (|| {
        if file.state != "completed" {
            return Err("Metadata cannot become authoritative before ingest completion".into());
        }
        let media = catalogue
            .media
            .iter()
            .find(|item| item.id == file.media_id)
            .ok_or("Completed ingest media is missing from the catalogue")?;
        append(metadata_update_events(media, &pending))
    })();
    match outcome {
        Ok(()) => {
            file.pending_metadata = None;
            file.metadata_error = None;
            Ok(())
        }
        Err(error) => {
            file.metadata_error = Some(error.clone());
            Err(format!(
                "Metadata is saved locally but catalogue persistence failed: {error}"
            ))
        }
    }
}

fn flush_pending_metadata(
    app: &AppHandle,
    state: &NativeState,
    job_id: &str,
    file_id: &str,
) -> Result<IngestJob, String> {
    let settings = load_settings_file(app)?;
    let _catalogue_guard = state.catalogue_write.lock().unwrap();
    let _jobs_guard = state.job_store.lock().unwrap();
    let mut jobs = load_jobs_unlocked(app)?;
    let job_index = jobs
        .iter()
        .position(|job| job.id == job_id)
        .ok_or("Ingest job not found")?;
    let file_index = jobs[job_index]
        .files
        .iter()
        .position(|file| file.id == file_id)
        .ok_or("Ingest job file not found")?;
    let catalogue = load_catalogue_file(&settings)?;
    let outcome = apply_pending_metadata_write(
        &mut jobs[job_index].files[file_index],
        &catalogue,
        |events| append_events(&settings, events),
    );
    if let Err(error) = outcome {
        write_json(&jobs_path(app)?, &jobs)?;
        let _ = app.emit("ingest-jobs-changed", &jobs);
        return Err(error);
    }
    jobs[job_index].updated_at = Utc::now().to_rfc3339();
    let updated = jobs[job_index].clone();
    write_json(&jobs_path(app)?, &jobs)?;
    let _ = app.emit("ingest-jobs-changed", &jobs);
    let _ = app.emit("catalogue-changed", ());
    Ok(updated)
}

fn recover_pending_metadata(app: &AppHandle, state: &NativeState) -> Result<(), String> {
    let pending: Vec<_> = snapshot_jobs(app, state)?
        .into_iter()
        .flat_map(|job| {
            let job_id = job.id;
            job.files
                .into_iter()
                .filter(|file| file.state == "completed" && file.pending_metadata.is_some())
                .map(move |file| (job_id.clone(), file.id))
        })
        .collect();
    for (job_id, file_id) in pending {
        let _ = flush_pending_metadata(app, state, &job_id, &file_id);
    }
    Ok(())
}

#[tauri::command]
fn update_ingest_file_metadata(
    app: AppHandle,
    state: State<'_, NativeState>,
    job_id: String,
    file_id: String,
    patch: ProvisionalMetadataPatch,
) -> Result<IngestJob, String> {
    let (job, completed) = mutate_jobs(&app, &state, |jobs| {
        let job = jobs
            .iter_mut()
            .find(|job| job.id == job_id)
            .ok_or("Ingest job not found")?;
        let file = job
            .files
            .iter_mut()
            .find(|file| file.id == file_id)
            .ok_or("Ingest job file not found")?;
        apply_metadata_patch(&mut file.metadata, &patch);
        let completed = if file.state == "completed" {
            let pending = file.pending_metadata.get_or_insert_with(Default::default);
            apply_metadata_patch(pending, &patch);
            file.metadata_error = None;
            Some(())
        } else {
            None
        };
        job.updated_at = Utc::now().to_rfc3339();
        Ok((job.clone(), completed))
    })?;
    if completed.is_some() {
        return flush_pending_metadata(&app, &state, &job_id, &file_id);
    }
    Ok(job)
}

#[tauri::command]
fn pause_ingest(app: AppHandle, state: State<'_, NativeState>, job_id: String) -> Result<(), String> {
    let result = mutate_jobs(&app, &state, |jobs| {
        let job = jobs.iter_mut().find(|job| job.id == job_id).ok_or("Ingest job not found")?;
        validate_job_action(&job.state, "pause")?;
        if job.state == "queued" {
            job.state = "paused".into();
        }
        job.updated_at = Utc::now().to_rfc3339();
        Ok(())
    });
    if result.is_ok() {
        state.paused_jobs.lock().unwrap().insert(job_id);
    }
    result
}

#[tauri::command]
fn resume_ingest(app: AppHandle, state: State<'_, NativeState>, job_id: String) -> Result<(), String> {
    let current = snapshot_jobs(&app, &state)?
        .into_iter()
        .find(|job| job.id == job_id)
        .ok_or("Ingest job not found")?;
    validate_job_action(&current.state, "resume")?;
    state.paused_jobs.lock().unwrap().remove(&job_id);
    let result = mutate_jobs(&app, &state, |jobs| {
        let job = jobs.iter_mut().find(|job| job.id == job_id).ok_or("Ingest job not found")?;
        validate_job_action(&job.state, "resume")?;
        job.state = "queued".into();
        for file in &mut job.files {
            if file.state == "paused" {
                file.state = "queued".into();
            }
        }
        Ok(())
    });
    if let Err(error) = result {
        state.paused_jobs.lock().unwrap().insert(job_id);
        return Err(error);
    }
    kick_ingest_worker(app);
    Ok(())
}

#[tauri::command]
fn retry_ingest(app: AppHandle, state: State<'_, NativeState>, job_id: String) -> Result<(), String> {
    let current = snapshot_jobs(&app, &state)?
        .into_iter()
        .find(|job| job.id == job_id)
        .ok_or("Ingest job not found")?;
    validate_job_action(&current.state, "retry")?;
    state.cancelled_jobs.lock().unwrap().remove(&job_id);
    state.paused_jobs.lock().unwrap().remove(&job_id);
    mutate_jobs(&app, &state, |jobs| {
        let job = jobs.iter_mut().find(|job| job.id == job_id).ok_or("Ingest job not found")?;
        validate_job_action(&job.state, "retry")?;
        prepare_job_retry(job);
        Ok(())
    })?;
    kick_ingest_worker(app);
    Ok(())
}

#[tauri::command]
fn cancel_ingest(app: AppHandle, state: State<NativeState>, job_id: String) -> Result<(), String> {
    let result = mutate_jobs(&app, &state, |jobs| {
        let job = jobs.iter_mut().find(|job| job.id == job_id).ok_or("Ingest job not found")?;
        validate_job_action(&job.state, "cancel")?;
        if matches!(job.state.as_str(), "queued" | "paused" | "interrupted") {
            job.state = "cancelled".into();
            for file in &mut job.files {
                if matches!(file.state.as_str(), "queued" | "paused" | "interrupted") {
                    file.state = "cancelled".into();
                }
            }
        }
        job.updated_at = Utc::now().to_rfc3339();
        Ok(())
    });
    if result.is_ok() {
        state.cancelled_jobs.lock().unwrap().insert(job_id);
    }
    result
}
fn normalized(values: &[String]) -> Vec<String> {
    let mut out: Vec<_> = values
        .iter()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect();
    out.sort_by(|a, b| {
        a.to_lowercase()
            .cmp(&b.to_lowercase())
            .then_with(|| a.cmp(b))
    });
    out.dedup_by(|a, b| a.to_lowercase() == b.to_lowercase());
    out
}

fn contains_normalized(values: &[String], candidate: &str) -> bool {
    let key = candidate.trim().to_lowercase();
    values
        .iter()
        .any(|value| value.trim().to_lowercase() == key)
}

fn write_project_manifest(
    directory: &Path,
    project: &ProjectRecord,
    media: &[MediaItem],
) -> Result<(), String> {
    let manifest_path = directory.join(".pimp-project.json");
    if manifest_path.exists() {
        let value: serde_json::Value = read_json(&manifest_path)?;
        let existing_id = value
            .get("id")
            .or_else(|| value.get("project").and_then(|project| project.get("id")))
            .and_then(serde_json::Value::as_str)
            .ok_or("Existing .pimp-project.json has no compatible project identity")?;
        if existing_id != project.id {
            return Err(format!(
                "Existing .pimp-project.json belongs to project {existing_id}; it was not overwritten"
            ));
        }
    }
    let references: Vec<_> = media
        .iter()
        .filter(|item| item.project_ids.contains(&project.id))
        .map(|item| {
            serde_json::json!({
                "id": item.id,
                "relativePath": item.relative_path,
                "sha256": item.sha256,
                "tags": item.tags
            })
        })
        .collect();
    write_json(
        &manifest_path,
        &serde_json::json!({
            "schemaVersion": 2,
            "id": project.id,
            "name": project.name,
            "workingDirectory": ".",
            "notes": project.notes,
            "tags": project.tags,
            "media": references,
            "createdAt": project.created_at
        }),
    )
}

#[tauri::command]
async fn update_media(
    app: AppHandle,
    state: State<'_, NativeState>,
    update: MediaUpdate,
) -> Result<Catalogue, String> {
    let settings = load_settings_file(&app)?;
    let _guard = state.catalogue_write.lock().unwrap();
    let catalogue = load_catalogue_file(&settings)?;
    let mut events = Vec::new();
    for item in catalogue
        .media
        .iter()
        .filter(|item| update.media_ids.contains(&item.id))
    {
        if let Some(tags) = &update.tags {
            let desired = normalized(tags);
            events.push(CatalogueEvent::MediaTagsAdded {
                media_id: item.id.clone(),
                values: desired
                    .iter()
                    .filter(|value| !contains_normalized(&item.tags, value))
                    .cloned()
                    .collect(),
            });
            events.push(CatalogueEvent::MediaTagsRemoved {
                media_id: item.id.clone(),
                values: item
                    .tags
                    .iter()
                    .filter(|value| !contains_normalized(&desired, value))
                    .cloned()
                    .collect(),
            });
        }
        if let Some(projects) = &update.project_ids {
            let desired = normalized(projects);
            events.push(CatalogueEvent::MediaProjectsAdded {
                media_id: item.id.clone(),
                values: desired
                    .iter()
                    .filter(|value| !contains_normalized(&item.project_ids, value))
                    .cloned()
                    .collect(),
            });
            events.push(CatalogueEvent::MediaProjectsRemoved {
                media_id: item.id.clone(),
                values: item
                    .project_ids
                    .iter()
                    .filter(|value| !contains_normalized(&desired, value))
                    .cloned()
                    .collect(),
            });
        }
        if let Some(tags) = &update.add_tags {
            events.push(CatalogueEvent::MediaTagsAdded {
                media_id: item.id.clone(),
                values: normalized(tags),
            });
        }
        if let Some(projects) = &update.add_project_ids {
            events.push(CatalogueEvent::MediaProjectsAdded {
                media_id: item.id.clone(),
                values: normalized(projects),
            });
        }
        if let Some(tags) = &update.remove_tags {
            events.push(CatalogueEvent::MediaTagsRemoved {
                media_id: item.id.clone(),
                values: normalized(tags),
            });
        }
        if let Some(projects) = &update.remove_project_ids {
            events.push(CatalogueEvent::MediaProjectsRemoved {
                media_id: item.id.clone(),
                values: normalized(projects),
            });
        }
        if let Some(notes) = &update.notes {
            events.push(CatalogueEvent::MediaNotesSet {
                media_id: item.id.clone(),
                value: notes.clone(),
            });
        }
        if let Some(favourite) = update.favourite {
            events.push(CatalogueEvent::MediaFavouriteSet {
                media_id: item.id.clone(),
                value: favourite,
            });
        }
    }
    events.retain(|event| match event {
        CatalogueEvent::MediaTagsAdded { values, .. }
        | CatalogueEvent::MediaTagsRemoved { values, .. }
        | CatalogueEvent::MediaProjectsAdded { values, .. }
        | CatalogueEvent::MediaProjectsRemoved { values, .. } => !values.is_empty(),
        _ => true,
    });
    append_events(&settings, events)?;
    let next = load_catalogue_file(&settings)?;
    for project in &next.projects {
        if let Some(path) = settings.project_paths.get(&project.id) {
            write_project_manifest(Path::new(path), project, &next.media)?;
        }
    }
    Ok(next)
}
#[tauri::command]
fn create_project(
    app: AppHandle,
    state: State<NativeState>,
    name: String,
    working_directory: String,
) -> Result<ProjectRecord, String> {
    if name.trim().is_empty() {
        return Err("Project name is required".into());
    }
    let mut settings = load_settings_file(&app)?;
    let directory = PathBuf::from(&working_directory);
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let manifest_path = directory.join(".pimp-project.json");
    let project = if manifest_path.exists() {
        let value: serde_json::Value = read_json(&manifest_path)?;
        let project_value = value.get("project").unwrap_or(&value);
        let id = project_value
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or("Existing .pimp-project.json is incompatible and was not overwritten")?;
        ProjectRecord {
            id: id.into(),
            name: project_value
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(name.trim())
                .into(),
            working_directory: ".".into(),
            media_ids: vec![],
            notes: project_value
                .get("notes")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .into(),
            tags: serde_json::from_value(project_value.get("tags").cloned().unwrap_or_default())
                .unwrap_or_default(),
            created_at: project_value
                .get("createdAt")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("1970-01-01T00:00:00Z")
                .into(),
        }
    } else {
        ProjectRecord {
            id: Uuid::new_v4().to_string(),
            name: name.trim().into(),
            working_directory: ".".into(),
            media_ids: vec![],
            notes: String::new(),
            tags: vec![],
            created_at: Utc::now().to_rfc3339(),
        }
    };
    settings
        .project_paths
        .insert(project.id.clone(), working_directory);
    let _guard = state.catalogue_write.lock().unwrap();
    append_events(
        &settings,
        vec![CatalogueEvent::ProjectUpserted {
            project: project.clone(),
        }],
    )?;
    write_json(&settings_path(&app)?, &settings)?;
    let catalogue = load_catalogue_file(&settings)?;
    write_project_manifest(&directory, &project, &catalogue.media)?;
    Ok(project)
}
#[tauri::command]
async fn rescan(app: AppHandle, state: State<'_, NativeState>) -> Result<RescanResult, String> {
    let settings = load_settings_file(&app)?;
    let master = PathBuf::from(&settings.master_bin_path);
    let _discovery_guard = state.discovery_write.lock().unwrap();
    let catalogue = load_catalogue_file(&settings)?;
    let known: HashMap<_, _> = catalogue
        .media
        .iter()
        .map(|item| (item.relative_path.clone(), item.clone()))
        .collect();
    let discovery_path = settings_path(&app)?
        .parent()
        .unwrap()
        .join("discovery-observations.json");
    let mut observations = discovery::load(&discovery_path)?;
    let now = Utc::now().timestamp();
    let mut events = Vec::new();
    let mut result = RescanResult::default();
    let batch_timestamp = Local::now();
    for entry in WalkDir::new(&master)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if !entry.file_type().is_file() || ignored(path) || !supported(path) {
            continue;
        }
        let relative = path
            .strip_prefix(&master)
            .map_err(|_| "Path escaped Master Bin")?
            .to_string_lossy()
            .replace('\\', "/");
        let snapshot = match fs_safe::snapshot(path) {
            Ok(snapshot) => snapshot,
            Err(_) => continue,
        };
        if let Some(item) = known.get(&relative) {
            let evidence = item.integrity_by_device.get(&settings.device_id);
            if evidence.is_some_and(|evidence| {
                evidence.observed_size == snapshot.length
                    && evidence.observed_modified_millis
                        == snapshot.modified_millis.map(|value| value as u64)
            }) {
                observations.verified(&relative);
                continue;
            }
        }
        if discovery::Decision::Wait == observations.observe(&relative, &snapshot, now) {
            result.pending += 1;
            continue;
        }
        let hash = sha256(path)?;
        if fs_safe::snapshot(path).ok().as_ref() != Some(&snapshot) {
            result.pending += 1;
            continue;
        }
        if let Some(item) = known.get(&relative) {
            events.push(CatalogueEvent::MediaEvidenceObserved {
                media_id: item.id.clone(),
                observed_size: snapshot.length,
                observed_modified_millis: snapshot.modified_millis.map(|value| value as u64),
                observed_sha256: Some(hash.clone()),
                needs_review: hash != item.sha256,
            });
        } else {
            let (_, capture, provenance) =
                destination_date(&settings, path, &None, &batch_timestamp)?;
            let item = new_media(path, relative.clone(), None, hash, capture, provenance, true)?;
            events.push(CatalogueEvent::MediaRegistered { media: item });
        }
        observations.verified(&relative);
    }
    discovery::save(&discovery_path, &observations)?;
    let _guard = state.catalogue_write.lock().unwrap();
    let current = load_catalogue_file(&settings)?;
    events.retain(|event| match event {
        CatalogueEvent::MediaRegistered { media } => !current.media.iter().any(|item| {
            item.id == media.id || item.relative_path == media.relative_path
        }),
        CatalogueEvent::MediaEvidenceObserved { media_id, .. } => {
            current.media.iter().any(|item| item.id == *media_id)
        }
        _ => true,
    });
    result.registered = events
        .iter()
        .filter(|event| matches!(event, CatalogueEvent::MediaRegistered { .. }))
        .count();
    result.changed = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                CatalogueEvent::MediaEvidenceObserved {
                    needs_review: true,
                    ..
                }
            )
        })
        .count();
    append_events(&settings, events)?;
    Ok(result)
}

#[tauri::command]
fn verify_media_integrity(
    app: AppHandle,
    state: State<'_, NativeState>,
    media_id: String,
) -> Result<bool, String> {
    let settings = load_settings_file(&app)?;
    let _guard = state.catalogue_write.lock().unwrap();
    let catalogue = load_catalogue_file(&settings)?;
    let item = catalogue
        .media
        .iter()
        .find(|item| item.id == media_id)
        .ok_or("Unknown media item")?;
    let path =
        fs_safe::resolve_existing(Path::new(&settings.master_bin_path), &item.relative_path)?;
    let snapshot = fs_safe::snapshot(&path)?;
    let observed_sha256 = sha256(&path)?;
    let matches = observed_sha256 == item.sha256;
    append_events(
        &settings,
        vec![CatalogueEvent::MediaEvidenceObserved {
            media_id,
            observed_size: snapshot.length,
            observed_modified_millis: snapshot.modified_millis.map(|value| value as u64),
            observed_sha256: Some(observed_sha256),
            needs_review: !matches,
        }],
    )?;
    let _ = app.emit("catalogue-changed", ());
    Ok(matches)
}
#[tauri::command]
async fn enrich_media(app: AppHandle, state: State<'_, NativeState>) -> Result<usize, String> {
    let settings = load_settings_file(&app)?;
    let _enrichment_guard = state.enrichment_write.lock().unwrap();
    let master = PathBuf::from(&settings.master_bin_path);
    let cache = PathBuf::from(&settings.thumbnail_cache_path);
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let catalogue = load_catalogue_file(&settings)?;
    let mut candidates = Vec::new();
    for item in &catalogue.media {
        let source = match fs_safe::resolve_existing(&master, &item.relative_path) {
            Ok(path) if path.is_file() => path,
            _ => continue,
        };
        let source_before = match fs_safe::snapshot(&source) {
            Ok(snapshot) => snapshot,
            Err(_) => continue,
        };
        let thumbnail_ref = format!("{}.jpg", item.id);
        if !catalogue::safe_cache_reference(&thumbnail_ref) {
            continue;
        }
        let thumbnail = cache.join(&thumbnail_ref);
        let needs_probe = item.technical.codec.is_none()
            && matches!(item.file_type.as_str(), "mp4" | "mov" | "m4v");
        let needs_thumbnail =
            matches!(item.file_type.as_str(), "mp4" | "mov" | "m4v") && !thumbnail.is_file();
        if !needs_probe && !needs_thumbnail {
            continue;
        }
        let mut technical = item.technical.clone();
        if needs_probe {
            if let Ok(output) = Command::new("ffprobe")
                .args([
                    "-v",
                    "quiet",
                    "-print_format",
                    "json",
                    "-show_streams",
                    "-show_format",
                ])
                .arg(&source)
                .output()
            {
                if output.status.success() {
                    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                        let streams = value["streams"].as_array().cloned().unwrap_or_default();
                        let video = streams
                            .iter()
                            .find(|stream| stream["codec_type"] == "video");
                        technical.has_audio =
                            Some(streams.iter().any(|stream| stream["codec_type"] == "audio"));
                        technical.duration_seconds = value["format"]["duration"]
                            .as_str()
                            .and_then(|value| value.parse().ok());
                        if let Some(video) = video {
                            technical.width = video["width"].as_u64().map(|value| value as u32);
                            technical.height = video["height"].as_u64().map(|value| value as u32);
                            technical.codec = video["codec_name"].as_str().map(String::from);
                            technical.frame_rate =
                                video["avg_frame_rate"].as_str().and_then(|rate| {
                                    let mut parts = rate.split('/');
                                    let numerator: f64 = parts.next()?.parse().ok()?;
                                    let denominator: f64 = parts.next()?.parse().ok()?;
                                    (denominator != 0.0).then_some(numerator / denominator)
                                });
                        }
                    }
                }
            }
        }
        if needs_thumbnail {
            let _ = Command::new("ffmpeg")
                .args(["-y", "-ss", "0.1", "-i"])
                .arg(&source)
                .args(["-frames:v", "1", "-vf", "scale=640:-2", "-q:v", "4"])
                .arg(&thumbnail)
                .output();
        }
        if fs_safe::snapshot(&source).ok().as_ref() != Some(&source_before) {
            continue;
        }
        candidates.push((
            item.id.clone(),
            item.relative_path.clone(),
            item.sha256.clone(),
            item.technical.clone(),
            item.thumbnail_path.clone(),
            CatalogueEvent::MediaEnriched {
                media_id: item.id.clone(),
                technical,
                thumbnail_ref: thumbnail.is_file().then_some(thumbnail_ref),
            },
        ));
    }
    let _guard = state.catalogue_write.lock().unwrap();
    let current = load_catalogue_file(&settings)?;
    let events: Vec<_> = candidates
        .into_iter()
        .filter_map(
            |(id, relative_path, sha256, technical, thumbnail_path, event)| {
                current
                    .media
                    .iter()
                    .find(|item| item.id == id)
                    .filter(|item| {
                        item.relative_path == relative_path
                            && item.sha256 == sha256
                            && item.technical == technical
                            && item.thumbnail_path == thumbnail_path
                    })
                    .map(|_| event)
            },
        )
        .collect();
    let changed = events.len();
    append_events(&settings, events)?;
    Ok(changed)
}
fn resolve_catalogue_media_path(
    settings: &DeviceSettings,
    media_id: &str,
) -> Result<PathBuf, String> {
    let catalogue = load_catalogue_file(settings)?;
    let item = catalogue
        .media
        .iter()
        .find(|m| m.id == media_id)
        .ok_or("Unknown media item")?;
    let path =
        fs_safe::resolve_existing(Path::new(&settings.master_bin_path), &item.relative_path)?;
    if !path.is_file() {
        return Err("This recording is not available locally yet".into());
    }
    Ok(path)
}

#[tauri::command]
fn resolve_media_path(app: AppHandle, media_id: String) -> Result<String, String> {
    let settings = load_settings_file(&app)?;
    Ok(resolve_catalogue_media_path(&settings, &media_id)?
        .to_string_lossy()
        .to_string())
}

#[tauri::command]
fn reveal_media_in_explorer(app: AppHandle, media_id: String) -> Result<(), String> {
    let settings = load_settings_file(&app)?;
    let path = resolve_catalogue_media_path(&settings, &media_id)?;
    #[cfg(target_os = "windows")]
    Command::new("explorer.exe")
        .arg(format!("/select,{}", path.to_string_lossy()))
        .spawn()
        .map_err(|error| format!("Could not open Windows Explorer: {error}"))?;
    #[cfg(not(target_os = "windows"))]
    return Err("Reveal in Explorer is available only in the Windows build".into());
    Ok(())
}

#[tauri::command]
fn open_media_folder(app: AppHandle, media_id: String) -> Result<(), String> {
    let settings = load_settings_file(&app)?;
    let path = resolve_catalogue_media_path(&settings, &media_id)?;
    let folder = path
        .parent()
        .ok_or("The recording does not have a containing folder")?;
    #[cfg(target_os = "windows")]
    Command::new("explorer.exe")
        .arg(folder)
        .spawn()
        .map_err(|error| format!("Could not open Windows Explorer: {error}"))?;
    #[cfg(not(target_os = "windows"))]
    return Err("Open containing folder is available only in the Windows build".into());
    Ok(())
}

fn tool_version(command: &str) -> Option<String> {
    let output = Command::new(command).arg("-version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(String::from)
}

#[tauri::command]
fn media_tools_status() -> MediaToolsStatus {
    let ffmpeg_version = tool_version("ffmpeg");
    let ffprobe_version = tool_version("ffprobe");
    MediaToolsStatus {
        ffmpeg_available: ffmpeg_version.is_some(),
        ffprobe_available: ffprobe_version.is_some(),
        ffmpeg_version,
        ffprobe_version,
    }
}

#[tauri::command]
fn load_thumbnail(app: AppHandle, media_id: String) -> Result<Option<String>, String> {
    let settings = load_settings_file(&app)?;
    let catalogue = load_catalogue_file(&settings)?;
    let item = catalogue
        .media
        .iter()
        .find(|item| item.id == media_id)
        .ok_or("Unknown media item")?;
    let Some(reference) = &item.thumbnail_path else {
        return Ok(None);
    };
    if !catalogue::safe_cache_reference(reference) || reference != &format!("{}.jpg", item.id) {
        return Err("Catalogue contains an unsafe thumbnail reference".into());
    }
    let cache = PathBuf::from(&settings.thumbnail_cache_path);
    fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
    let cache = cache
        .canonicalize()
        .map_err(|error| format!("Cannot resolve thumbnail cache: {error}"))?;
    let path = cache.join(reference);
    if !path.is_file() {
        return Ok(None);
    }
    let path = path
        .canonicalize()
        .map_err(|error| format!("Cannot resolve thumbnail: {error}"))?;
    if !path.starts_with(&cache) {
        return Err("Thumbnail resolved outside the configured cache".into());
    }
    let metadata = path.metadata().map_err(|e| e.to_string())?;
    if metadata.len() > 10 * 1024 * 1024 {
        return Err("Thumbnail exceeds the 10 MB safety limit".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    Ok(Some(format!(
        "data:image/jpeg;base64,{}",
        BASE64.encode(bytes)
    )))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .manage(NativeState::default())
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            load_catalogue,
            catalogue_signature,
            refresh_media_availability,
            inventory_sources,
            start_ingest,
            initialize_ingest_jobs,
            list_ingest_jobs,
            update_ingest_file_metadata,
            pause_ingest,
            resume_ingest,
            retry_ingest,
            cancel_ingest,
            update_media,
            create_project,
            rescan,
            verify_media_integrity,
            enrich_media,
            media_tools_status,
            resolve_media_path,
            reveal_media_in_explorer,
            open_media_folder,
            load_thumbnail
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kairix PIMP");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".test-tmp")
            .join(format!("pimp-native-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn ingest_job(state: &str, file_state: &str) -> IngestJob {
        IngestJob {
            schema_version: 2,
            id: "job-1".into(),
            state: state.into(),
            created_at: "2026-10-24T00:00:00Z".into(),
            updated_at: "2026-10-24T00:00:00Z".into(),
            batch_timestamp: "2026-10-24T00:00:00+00:00".into(),
            date_override: None,
            files: vec![IngestJobFile {
                id: "file-1".into(),
                media_id: "media-1".into(),
                source_path: "missing-source.mp4".into(),
                source_relative_path: "missing-source.mp4".into(),
                destination_path: "master/20261024/clip.mp4".into(),
                relative_path: "20261024/clip.mp4".into(),
                original_filename: "clip.mp4".into(),
                size_bytes: 12,
                bytes_transferred: 5,
                state: file_state.into(),
                error: None,
                metadata: ProvisionalMetadata {
                    tags: vec!["Interview".into()],
                    project_ids: vec!["project-1".into()],
                    notes: "Retain through restart".into(),
                    favourite: true,
                    edited_fields: vec![
                        "tags".into(),
                        "projectIds".into(),
                        "notes".into(),
                        "favourite".into(),
                    ],
                },
                pending_metadata: None,
                metadata_error: None,
            }],
            unsupported: Vec::new(),
            failures: vec!["previous failure".into()],
            inventory_excluded: Vec::new(),
            inventory_issues: Vec::new(),
            inventory_total_file_count: 1,
            inventory_total_bytes: 12,
            current_file: Some("clip.mp4".into()),
            bytes_total: 12,
            bytes_transferred: 5,
            files_completed: 0,
        }
    }

    fn completion_fixture(name: &str) -> (PathBuf, PathBuf, IngestJobFile, Catalogue) {
        let root = temporary(name);
        let master = root.join("master");
        let destination = master.join("20261024").join("clip.mp4");
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(&destination, b"verified-media").unwrap();
        let media = new_media(
            &destination,
            "20261024/clip.mp4".into(),
            None,
            sha256(&destination).unwrap(),
            None,
            "unknown".into(),
            false,
        )
        .unwrap();
        let file = IngestJobFile {
            id: "file-1".into(),
            media_id: media.id.clone(),
            source_path: root.join("missing-card.mp4").to_string_lossy().into(),
            source_relative_path: "missing-card.mp4".into(),
            destination_path: destination.to_string_lossy().into(),
            relative_path: media.relative_path.clone(),
            original_filename: "clip.mp4".into(),
            size_bytes: media.technical.size_bytes,
            bytes_transferred: media.technical.size_bytes,
            state: "interrupted".into(),
            error: None,
            metadata: ProvisionalMetadata::default(),
            pending_metadata: None,
            metadata_error: None,
        };
        let catalogue = Catalogue {
            media: vec![media],
            ..Default::default()
        };
        (root, master, file, catalogue)
    }

    #[test]
    fn persistent_jobs_round_trip_provisional_metadata() {
        let root = temporary("job-round-trip");
        let path = root.join("ingest-jobs.json");
        let jobs = vec![ingest_job("failed", "failed")];
        write_json(&path, &jobs).unwrap();
        assert_eq!(read_json::<Vec<IngestJob>>(&path).unwrap(), jobs);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_marks_active_work_interrupted_without_auto_resuming_missing_sources() {
        let mut jobs = vec![ingest_job("copying", "verifying")];
        mark_active_jobs_interrupted(&mut jobs);
        assert_eq!(jobs[0].state, "interrupted");
        assert_eq!(jobs[0].files[0].state, "interrupted");
        assert_eq!(jobs[0].bytes_transferred, 5);
        assert_eq!(jobs[0].files[0].bytes_transferred, 5);
        assert_eq!(jobs[0].files[0].metadata.notes, "Retain through restart");
        assert!(jobs[0].files[0].error.as_deref().unwrap().contains("stopped"));
    }

    #[test]
    fn retry_preserves_completed_files_and_metadata_but_restarts_partial_files() {
        let mut job = ingest_job("failed", "interrupted");
        let mut completed = job.files[0].clone();
        completed.id = "file-2".into();
        completed.media_id = "media-2".into();
        completed.state = "completed".into();
        completed.bytes_transferred = completed.size_bytes;
        job.files.push(completed);
        prepare_job_retry(&mut job);
        assert_eq!(job.state, "queued");
        assert_eq!(job.files[0].state, "queued");
        assert_eq!(job.files[0].bytes_transferred, 0);
        assert_eq!(job.files[0].metadata.tags, vec!["Interview"]);
        assert_eq!(job.files[1].state, "completed");
        assert_eq!(job.files[1].bytes_transferred, 12);
        assert_eq!(job.bytes_transferred, 12);
        assert!(job.failures.is_empty());
    }

    #[test]
    fn native_job_actions_reject_illegal_lifecycle_transitions() {
        assert!(validate_job_action("copying", "retry").is_err());
        assert!(validate_job_action("verifying", "retry").is_err());
        assert!(validate_job_action("completed", "retry").is_err());
        assert!(validate_job_action("failed", "retry").is_ok());
        assert!(validate_job_action("interrupted", "retry").is_ok());
        assert!(validate_job_action("copying", "pause").is_ok());
        assert!(validate_job_action("failed", "pause").is_err());
        assert!(validate_job_action("paused", "resume").is_ok());
        assert!(validate_job_action("queued", "resume").is_err());
        assert!(validate_job_action("completed", "cancel").is_err());
        assert!(validate_job_action("queued", "cancel").is_ok());
    }

    #[test]
    fn queued_jobs_reserve_destinations_until_completion_or_cancellation() {
        let queued = ingest_job("queued", "queued");
        let destination = queued.files[0].destination_path.clone();
        assert!(active_destination_reservations(&[queued.clone()]).contains(&destination));

        let mut completed = queued.clone();
        completed.state = "completed".into();
        let mut cancelled = queued;
        cancelled.state = "cancelled".into();
        assert!(active_destination_reservations(&[completed, cancelled]).is_empty());
    }

    #[test]
    fn duplicate_with_unspecified_metadata_preserves_the_existing_record() {
        let (root, _master, _file, mut catalogue) = completion_fixture("duplicate-metadata");
        let media = &mut catalogue.media[0];
        media.tags = vec!["Race".into()];
        media.project_ids = vec!["project-1".into(), "project-2".into()];
        media.notes = "Existing note".into();
        media.favourite = true;
        let expected = media.clone();
        let catalogue_root = root.join("catalogue");
        catalogue::append(
            &catalogue_root,
            "device-a",
            vec![CatalogueEvent::MediaRegistered {
                media: expected.clone(),
            }],
        )
        .unwrap();
        let registered = catalogue::load(&catalogue_root).unwrap();
        let duplicate_events =
            metadata_update_events(&registered.media[0], &ProvisionalMetadata::default());
        assert!(duplicate_events.is_empty());
        catalogue::append(&catalogue_root, "device-a", duplicate_events).unwrap();
        let after_duplicate = catalogue::load(&catalogue_root).unwrap();
        assert_eq!(after_duplicate.media[0], expected);
        assert_eq!(fs::read(root.join("master/20261024/clip.mp4")).unwrap(), b"verified-media");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_reconciliation_accepts_only_a_fully_verified_promoted_copy() {
        let (root, master, file, catalogue) = completion_fixture("valid-completion");
        assert_eq!(
            verified_completed_media(&master, &file, &catalogue).unwrap(),
            file.media_id
        );
        assert!(!Path::new(&file.source_path).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_reconciliation_rejects_same_path_with_a_different_hash() {
        let (root, master, file, mut catalogue) = completion_fixture("wrong-hash");
        catalogue.media[0].sha256 = "different".into();
        assert!(verified_completed_media(&master, &file, &catalogue)
            .unwrap_err()
            .contains("SHA-256"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_reconciliation_rejects_same_id_with_a_different_path() {
        let (root, master, file, mut catalogue) = completion_fixture("wrong-path");
        catalogue.media[0].relative_path = "20261024/other.mp4".into();
        assert!(verified_completed_media(&master, &file, &catalogue)
            .unwrap_err()
            .contains("relative path"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_reconciliation_rejects_missing_and_partial_destinations() {
        let (root, master, file, catalogue) = completion_fixture("missing-partial");
        fs::write(&file.destination_path, b"partial").unwrap();
        assert!(verified_completed_media(&master, &file, &catalogue)
            .unwrap_err()
            .contains("size"));
        fs::remove_file(&file.destination_path).unwrap();
        assert!(verified_completed_media(&master, &file, &catalogue)
            .unwrap_err()
            .contains("missing"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_reconciliation_rejects_conflicting_registrations() {
        let (root, master, file, mut catalogue) = completion_fixture("conflicting-completion");
        let mut conflict = catalogue.media[0].clone();
        conflict.id = "other-media".into();
        catalogue.media.push(conflict);
        assert!(verified_completed_media(&master, &file, &catalogue)
            .unwrap_err()
            .contains("Conflicting"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn completed_job_metadata_becomes_authoritative_events() {
        let metadata = ingest_job("queued", "queued")
            .files
            .into_iter()
            .next()
            .unwrap()
            .metadata;
        let root = temporary("metadata-add-events");
        let path = root.join("clip.mp4");
        fs::write(&path, b"media").unwrap();
        let media = new_media(
            &path,
            "20261024/clip.mp4".into(),
            None,
            sha256(&path).unwrap(),
            None,
            "unknown".into(),
            false,
        )
        .unwrap();
        assert_eq!(
            metadata_update_events(&media, &metadata),
            vec![
                CatalogueEvent::MediaTagsAdded {
                    media_id: media.id.clone(),
                    values: vec!["Interview".into()],
                },
                CatalogueEvent::MediaProjectsAdded {
                    media_id: media.id.clone(),
                    values: vec!["project-1".into()],
                },
                CatalogueEvent::MediaNotesSet {
                    media_id: media.id.clone(),
                    value: "Retain through restart".into(),
                },
                CatalogueEvent::MediaFavouriteSet {
                    media_id: media.id.clone(),
                    value: true,
                },
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn completed_job_metadata_can_be_cleared_without_replacing_concurrent_additions() {
        let root = temporary("metadata-update-events");
        let path = root.join("clip.mp4");
        fs::write(&path, b"media").unwrap();
        let mut media = new_media(
            &path,
            "20261024/clip.mp4".into(),
            None,
            sha256(&path).unwrap(),
            None,
            "unknown".into(),
            false,
        )
        .unwrap();
        media.tags = vec!["Race".into()];
        media.project_ids = vec!["project-1".into()];
        media.notes = "Old note".into();
        media.favourite = true;
        let cleared = ProvisionalMetadata {
            edited_fields: vec![
                "tags".into(),
                "projectIds".into(),
                "notes".into(),
                "favourite".into(),
            ],
            ..Default::default()
        };
        assert_eq!(
            metadata_update_events(&media, &cleared),
            vec![
                CatalogueEvent::MediaTagsRemoved {
                    media_id: media.id.clone(),
                    values: vec!["Race".into()],
                },
                CatalogueEvent::MediaProjectsRemoved {
                    media_id: media.id.clone(),
                    values: vec!["project-1".into()],
                },
                CatalogueEvent::MediaNotesSet {
                    media_id: media.id.clone(),
                    value: String::new(),
                },
                CatalogueEvent::MediaFavouriteSet {
                    media_id: media.id.clone(),
                    value: false,
                },
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_completed_metadata_write_remains_durable_and_recovers_idempotently() {
        let (root, _master, mut file, mut catalogue) = completion_fixture("pending-metadata");
        file.state = "completed".into();
        catalogue.media[0].notes = "Old note".into();
        let patch = ProvisionalMetadataPatch {
            notes: Some(String::new()),
            ..Default::default()
        };
        apply_metadata_patch(&mut file.metadata, &patch);
        let mut pending = ProvisionalMetadata::default();
        apply_metadata_patch(&mut pending, &patch);
        file.pending_metadata = Some(pending);

        let error = apply_pending_metadata_write(&mut file, &catalogue, |_| {
            Err("simulated append failure".into())
        })
        .unwrap_err();
        assert!(error.contains("saved locally"));
        assert!(file.pending_metadata.is_some());
        assert_eq!(file.metadata_error.as_deref(), Some("simulated append failure"));

        let path = root.join("failed-job.json");
        write_json(&path, &file).unwrap();
        let mut restarted: IngestJobFile =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        // Simulate the uncertain case where the event reached disk before the
        // caller observed an error. Recovery must not emit it a second time.
        catalogue.media[0].notes.clear();
        let mut recovered_events = None;
        apply_pending_metadata_write(&mut restarted, &catalogue, |events| {
            recovered_events = Some(events);
            Ok(())
        })
        .unwrap();
        assert!(recovered_events.unwrap().is_empty());
        assert!(restarted.pending_metadata.is_none());
        assert!(restarted.metadata_error.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn accepts_common_production_media_extensions() {
        assert!(supported(Path::new("clip.MP4")));
        assert!(supported(Path::new("clip.MXF")));
        assert!(supported(Path::new("camera.MTS")));
        assert!(supported(Path::new("audio.wav")));
        assert!(supported(Path::new("audio.BWF")));
        assert!(supported(Path::new("still.CR3")));
        assert!(!supported(Path::new("project.prproj")));
    }

    #[test]
    fn inventories_camera_packages_unknown_files_and_exclusions_without_silent_loss() {
        let root = temporary("inventory-package");
        let card = root.join("XDROOT");
        fs::create_dir_all(card.join("Clip")).unwrap();
        fs::create_dir_all(card.join("General").join("Sony")).unwrap();
        fs::write(card.join("Clip").join("A001.MXF"), b"video").unwrap();
        fs::write(card.join("General").join("Sony").join("A001.XML"), b"sidecar").unwrap();
        fs::write(card.join("vendor.future"), b"future").unwrap();
        fs::write(card.join(".syncthing.A002.MXF.tmp"), b"partial").unwrap();

        let inventory = build_source_inventory(&[card.to_string_lossy().into()]);
        assert_eq!(inventory.total_file_count, 4);
        assert_eq!(inventory.copy_file_count, 3);
        assert_eq!(inventory.excluded.len(), 1);
        assert!(inventory.issues.is_empty());
        assert!(inventory.files.iter().all(|file| file.preserve_hierarchy));
        assert!(inventory
            .files
            .iter()
            .any(|file| file.source_relative_path == "XDROOT/Clip/A001.MXF"));
        assert!(inventory
            .files
            .iter()
            .any(|file| file.kind == "unknown" && !file.analysis_supported));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn duplicate_basenames_preserve_their_relative_paths() {
        let root = temporary("inventory-duplicates");
        fs::create_dir_all(root.join("card-a")).unwrap();
        fs::create_dir_all(root.join("card-b")).unwrap();
        fs::write(root.join("card-a").join("clip.mp4"), b"one").unwrap();
        fs::write(root.join("card-b").join("clip.mp4"), b"two").unwrap();

        let inventory = build_source_inventory(&[root.to_string_lossy().into()]);
        assert_eq!(inventory.copy_file_count, 2);
        assert!(inventory.files.iter().all(|file| file.preserve_hierarchy));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_sources_are_reported_as_inventory_issues() {
        let root = temporary("inventory-missing");
        let missing = root.join("removed-card");
        let inventory = build_source_inventory(&[missing.to_string_lossy().into()]);
        assert_eq!(inventory.copy_file_count, 0);
        assert_eq!(inventory.issues.len(), 1);
        assert!(inventory.issues[0].contains("unavailable"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ignores_partial_transfer_names_and_private_metadata() {
        assert!(ignored(Path::new(".syncthing.clip.mp4.tmp")));
        assert!(ignored(Path::new("2026-10-22/.pimp/catalogue.json")));
        assert!(!ignored(Path::new("2026-10-22/clip.mp4")));
    }

    #[test]
    fn additive_metadata_is_normalized_without_losing_values() {
        let existing = vec!["Race".to_string(), "Workshop".to_string()];
        let added = vec!["race".to_string(), "Night".to_string()];
        assert_eq!(
            normalized(&[existing, added].concat()),
            vec!["Night", "Race", "Workshop"]
        );
    }

    #[test]
    fn device_identity_cannot_be_replaced_after_initialization() {
        let identity = Uuid::new_v4().to_string();
        assert_eq!(preserve_device_identity(&identity, "").unwrap(), identity);
        assert!(preserve_device_identity(&identity, &Uuid::new_v4().to_string()).is_err());
        assert!(preserve_device_identity("", "desktop").is_err());
    }

    #[test]
    fn first_launch_persists_one_immutable_device_identity_across_restart() {
        let root = temporary("first-launch-identity");
        let path = root.join("empty-config").join("device-settings.json");
        let first = load_or_initialize_settings(&path).unwrap();
        assert!(Uuid::parse_str(&first.device_id).is_ok());
        assert_eq!(first.ingest_date_organization, "today");
        assert!(path.is_file());

        let restarted = load_or_initialize_settings(&path).unwrap();
        assert_eq!(restarted.device_id, first.device_id);
        assert_eq!(
            preserve_device_identity(&restarted.device_id, &first.device_id).unwrap(),
            first.device_id
        );
        assert!(preserve_device_identity(&restarted.device_id, &Uuid::new_v4().to_string()).is_err());

        write_json(&path, &restarted).unwrap();
        fs::remove_file(&path).unwrap();
        let recovered = load_or_initialize_settings(&path).unwrap();
        assert_eq!(recovered.device_id, restarted.device_id);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_settings_without_a_date_mode_keep_the_previous_default() {
        let mut value = serde_json::to_value(DeviceSettings::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("ingestDateOrganization");
        let settings: DeviceSettings = serde_json::from_value(value).unwrap();
        assert_eq!(settings.ingest_date_organization, "original-file-dates");
    }

    #[test]
    fn date_folder_patterns_are_unambiguous_and_path_safe() {
        let date = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        let time = chrono::NaiveTime::from_hms_opt(17, 42, 9).unwrap();
        assert_eq!(
            format_date_folder("yyyymmdd", date, time).unwrap(),
            "20261023"
        );
        assert_eq!(
            format_date_folder("yyyy-MM-dd HH-mm", date, time).unwrap(),
            "2026-10-23 17-42"
        );
        assert_eq!(
            format_date_folder("yyyy MM dd hh-mm a", date, time).unwrap(),
            "2026 10 23 05-42 PM"
        );
        assert!(format_date_folder("yyyy/MM/dd", date, time).is_err());
        assert!(format_date_folder("yyyy:MM:dd", date, time).is_err());
    }

    #[test]
    fn date_time_folders_use_one_batch_time_or_each_original_file_time() {
        let batch = Local.with_ymd_and_hms(2026, 10, 24, 12, 34, 56).unwrap();
        let first = Local.with_ymd_and_hms(2026, 10, 20, 8, 15, 1).unwrap();
        let second = Local.with_ymd_and_hms(2026, 10, 21, 19, 45, 2).unwrap();
        let mut settings = DeviceSettings::default();
        settings.date_folder_pattern = "yyyyMMdd-HHmmss".into();
        settings.ingest_date_organization = "today".into();

        let today_first = destination_date_from_modified(
            &settings,
            Some(first.clone()),
            &None,
            &batch,
        )
        .unwrap()
        .0;
        let today_second = destination_date_from_modified(
            &settings,
            Some(second.clone()),
            &None,
            &batch,
        )
        .unwrap()
        .0;
        assert_eq!(today_first, "20261024-123456");
        assert_eq!(today_first, today_second);

        settings.ingest_date_organization = "original-file-dates".into();
        assert_eq!(
            destination_date_from_modified(&settings, Some(first.clone()), &None, &batch)
                .unwrap()
                .0,
            "20261020-081501"
        );
        assert_eq!(
            destination_date_from_modified(&settings, Some(second.clone()), &None, &batch)
                .unwrap()
                .0,
            "20261021-194502"
        );
        assert_eq!(
            destination_date_from_modified(
                &settings,
                Some(first.clone()),
                &Some("2026-10-22".into()),
                &batch,
            )
            .unwrap()
            .0,
            "20261022-123456"
        );
        assert!(destination_date_from_modified(
            &settings,
            Some(first),
            &Some("2026-99-99".into()),
            &batch,
        )
        .is_err());
    }

    #[test]
    fn older_settings_keep_the_existing_dashed_date_pattern() {
        let value = serde_json::json!({
            "deviceId": Uuid::new_v4().to_string(),
            "masterBinPath": "master",
            "workingProjectsPath": "projects",
            "catalogueInMasterBin": true,
            "sharedCataloguePath": "",
            "thumbnailCachePath": "cache",
            "thumbnailCacheLimitBytes": 1,
            "ingestDateOrganization": "original-file-dates"
        });
        let settings: DeviceSettings = serde_json::from_value(value).unwrap();
        assert_eq!(settings.date_folder_pattern, "yyyy-MM-dd");
        assert_eq!(DeviceSettings::default().date_folder_pattern, "yyyymmdd");
    }

    #[test]
    fn metadata_write_retains_a_recoverable_previous_version() {
        let root = temporary("metadata");
        let path = root.join("state.json");
        write_json(&path, &serde_json::json!({"version":1})).unwrap();
        write_json(&path, &serde_json::json!({"version":2})).unwrap();
        assert_eq!(read_json::<serde_json::Value>(&path).unwrap()["version"], 2);
        fs::remove_file(&path).unwrap();
        assert_eq!(read_json::<serde_json::Value>(&path).unwrap()["version"], 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_recovery_journal_fails_closed_without_deleting_staging() {
        let root = temporary("bad-recovery");
        let master = root.join("master");
        fs::create_dir(&master).unwrap();
        let staging = fs_safe::staging_root(&master, "device-a").unwrap();
        let staged = staging.join("files").join("keep.part");
        let journal = staging.join("jobs").join("bad.json");
        fs::write(&staged, b"incomplete").unwrap();
        fs::create_dir_all(journal.parent().unwrap()).unwrap();
        fs::write(&journal, b"{not-json}").unwrap();
        assert!(recover_staging(&master, "device-a", None).is_err());
        assert_eq!(fs::read(&staged).unwrap(), b"incomplete");
        assert!(journal.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn catalogue_refresh_does_not_touch_active_staging() {
        let root = temporary("refresh-active-staging");
        let master = root.join("master");
        let catalogue_root = master.join(".pimp");
        fs::create_dir_all(&catalogue_root).unwrap();
        let staging = fs_safe::staging_root(&master, "device-a").unwrap();
        let active = staging.join("files").join("active.part");
        fs::write(&active, b"still-copying").unwrap();
        for _ in 0..10 {
            catalogue::load(&catalogue_root).unwrap();
        }
        assert_eq!(fs::read(&active).unwrap(), b"still-copying");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn long_background_coordination_does_not_own_the_catalogue_write_lock() {
        let state = NativeState::default();
        let _discovery = state.discovery_write.lock().unwrap();
        let _enrichment = state.enrichment_write.lock().unwrap();
        assert!(state.catalogue_write.try_lock().is_ok());
    }

    #[test]
    fn recovery_removes_only_the_file_named_by_a_valid_journal() {
        let root = temporary("owned-recovery");
        let master = root.join("master");
        fs::create_dir(&master).unwrap();
        let staging = fs_safe::staging_root(&master, "device-a").unwrap();
        let owned = staging.join("files").join("owned.part");
        let unrelated = staging.join("files").join("unrelated.part");
        fs::write(&owned, b"owned").unwrap();
        fs::write(&unrelated, b"unrelated").unwrap();
        let journal_path = staging.join("jobs").join("owned.json");
        write_json(
            &journal_path,
            &IngestJournal {
                job_id: "job".into(),
                source_path: "source".into(),
                destination_path: "destination".into(),
                staged_name: "owned.part".into(),
                status: "copying".into(),
                detail: None,
                media: None,
            },
        )
        .unwrap();
        assert_eq!(
            recover_staging(&master, "device-a", None)
                .unwrap()
                .recovered,
            1
        );
        assert!(!owned.exists());
        assert!(!journal_path.exists());
        assert_eq!(fs::read(&unrelated).unwrap(), b"unrelated");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_never_deletes_a_promoted_destination() {
        let root = temporary("committed-recovery");
        let master = root.join("master");
        fs::create_dir(&master).unwrap();
        let destination = master.join("2026-10-23").join("clip.mp4");
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(&destination, b"verified-media").unwrap();
        let staging = fs_safe::staging_root(&master, "device-a").unwrap();
        let staged = staging.join("files").join("already-promoted.part");
        fs::write(&staged, b"stale-stage").unwrap();
        let journal_path = staging.join("jobs").join("committed.json");
        let media = new_media(
            &destination,
            "2026-10-23/clip.mp4".into(),
            None,
            sha256(&destination).unwrap(),
            None,
            "unknown".into(),
            false,
        )
        .unwrap();
        let catalogue_root = master.join(".pimp");
        write_json(
            &journal_path,
            &IngestJournal {
                job_id: "job".into(),
                source_path: "source".into(),
                destination_path: destination.to_string_lossy().into(),
                staged_name: "already-promoted.part".into(),
                status: "committed".into(),
                detail: None,
                media: Some(media.clone()),
            },
        )
        .unwrap();
        let recovery = recover_staging(&master, "device-a", Some(&catalogue_root)).unwrap();
        assert_eq!(recovery.recovered, 1);
        assert_eq!(recovery.needs_review, 0);
        assert_eq!(fs::read(&destination).unwrap(), b"verified-media");
        assert!(!staged.exists());
        assert!(!journal_path.exists());
        assert_eq!(
            catalogue::load(&catalogue_root).unwrap().media[0].id,
            media.id
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_preserves_verified_work_when_promotion_is_uncertain() {
        let root = temporary("verified-recovery");
        let master = root.join("master");
        fs::create_dir(&master).unwrap();
        let staging = fs_safe::staging_root(&master, "device-a").unwrap();
        let staged = staging.join("files").join("verified.part");
        fs::write(&staged, b"verified-bytes").unwrap();
        let journal_path = staging.join("jobs").join("verified.json");
        write_json(
            &journal_path,
            &IngestJournal {
                job_id: "job".into(),
                source_path: "source".into(),
                destination_path: master
                    .join("2026-10-23")
                    .join("clip.mp4")
                    .to_string_lossy()
                    .into(),
                staged_name: "verified.part".into(),
                status: "verified".into(),
                detail: None,
                media: None,
            },
        )
        .unwrap();
        let recovery = recover_staging(&master, "device-a", None).unwrap();
        assert_eq!(recovery.recovered, 0);
        assert_eq!(recovery.needs_review, 1);
        assert_eq!(fs::read(&staged).unwrap(), b"verified-bytes");
        assert!(journal_path.is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_preserves_a_committed_journal_when_promoted_bytes_changed() {
        let root = temporary("corrupt-promoted-recovery");
        let master = root.join("master");
        fs::create_dir(&master).unwrap();
        let destination = master.join("2026-10-23").join("clip.mp4");
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(&destination, b"original-media").unwrap();
        let mut media = new_media(
            &destination,
            "2026-10-23/clip.mp4".into(),
            None,
            sha256(&destination).unwrap(),
            None,
            "unknown".into(),
            false,
        )
        .unwrap();
        fs::write(&destination, b"modified-media").unwrap();
        media.technical.size_bytes = fs::metadata(&destination).unwrap().len();

        let staging = fs_safe::staging_root(&master, "device-a").unwrap();
        let staged = staging.join("files").join("keep.part");
        fs::write(&staged, b"staged-copy").unwrap();
        let journal_path = staging.join("jobs").join("committed.json");
        write_json(
            &journal_path,
            &IngestJournal {
                job_id: "job".into(),
                source_path: "source".into(),
                destination_path: destination.to_string_lossy().into(),
                staged_name: "keep.part".into(),
                status: "committed".into(),
                detail: None,
                media: Some(media),
            },
        )
        .unwrap();

        let catalogue_root = master.join(".pimp");
        let recovery = recover_staging(&master, "device-a", Some(&catalogue_root)).unwrap();
        assert_eq!(recovery.recovered, 0);
        assert_eq!(recovery.needs_review, 1);
        assert_eq!(fs::read(&destination).unwrap(), b"modified-media");
        assert!(staged.is_file());
        assert!(journal_path.is_file());
        assert!(catalogue::load(&catalogue_root).unwrap().media.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_rejects_a_destination_that_disagrees_with_the_media_path() {
        let root = temporary("wrong-destination-recovery");
        let master = root.join("master");
        fs::create_dir(&master).unwrap();
        let expected = master.join("expected").join("clip.mp4");
        let destination = master.join("other").join("clip.mp4");
        fs::create_dir_all(expected.parent().unwrap()).unwrap();
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(&expected, b"same-media").unwrap();
        fs::write(&destination, b"same-media").unwrap();
        let media = new_media(
            &expected,
            "expected/clip.mp4".into(),
            None,
            sha256(&expected).unwrap(),
            None,
            "unknown".into(),
            false,
        )
        .unwrap();
        let staging = fs_safe::staging_root(&master, "device-a").unwrap();
        let staged = staging.join("files").join("keep.part");
        fs::write(&staged, b"same-media").unwrap();
        let journal_path = staging.join("jobs").join("committed.json");
        write_json(
            &journal_path,
            &IngestJournal {
                job_id: "job".into(),
                source_path: "source".into(),
                destination_path: destination.to_string_lossy().into(),
                staged_name: "keep.part".into(),
                status: "committed".into(),
                detail: None,
                media: Some(media),
            },
        )
        .unwrap();

        let recovery =
            recover_staging(&master, "device-a", Some(&master.join(".pimp"))).unwrap();
        assert_eq!(recovery.recovered, 0);
        assert_eq!(recovery.needs_review, 1);
        assert!(journal_path.is_file());
        assert!(staged.is_file());
        assert_eq!(fs::read(&destination).unwrap(), b"same-media");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_preserves_a_journal_that_conflicts_with_existing_registration() {
        let root = temporary("conflicting-registration-recovery");
        let master = root.join("master");
        fs::create_dir(&master).unwrap();
        let destination = master.join("2026-10-23").join("clip.mp4");
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(&destination, b"verified-media").unwrap();
        let media = new_media(
            &destination,
            "2026-10-23/clip.mp4".into(),
            None,
            sha256(&destination).unwrap(),
            None,
            "unknown".into(),
            false,
        )
        .unwrap();
        let mut conflicting = media.clone();
        conflicting.id = "different-media-id".into();
        let catalogue_root = master.join(".pimp");
        catalogue::append(
            &catalogue_root,
            "device-a",
            vec![CatalogueEvent::MediaRegistered { media: conflicting }],
        )
        .unwrap();

        let staging = fs_safe::staging_root(&master, "device-a").unwrap();
        let staged = staging.join("files").join("keep.part");
        fs::write(&staged, b"verified-media").unwrap();
        let journal_path = staging.join("jobs").join("committed.json");
        write_json(
            &journal_path,
            &IngestJournal {
                job_id: "job".into(),
                source_path: "source".into(),
                destination_path: destination.to_string_lossy().into(),
                staged_name: "keep.part".into(),
                status: "committed".into(),
                detail: None,
                media: Some(media),
            },
        )
        .unwrap();

        let recovery = recover_staging(&master, "device-a", Some(&catalogue_root)).unwrap();
        assert_eq!(recovery.recovered, 0);
        assert_eq!(recovery.needs_review, 1);
        assert!(journal_path.is_file());
        assert!(staged.is_file());
        assert_eq!(catalogue::load(&catalogue_root).unwrap().media.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interrupted_journal_rewrite_leaves_the_last_complete_state_recoverable() {
        let root = temporary("interrupted-journal-rewrite");
        let master = root.join("master");
        fs::create_dir(&master).unwrap();
        let staging = fs_safe::staging_root(&master, "device-a").unwrap();
        let staged = staging.join("files").join("keep.part");
        fs::write(&staged, b"verified-media").unwrap();
        let journal_path = staging.join("jobs").join("job.json");
        let journal = IngestJournal {
            job_id: "job".into(),
            source_path: "source".into(),
            destination_path: master
                .join("2026-10-23")
                .join("clip.mp4")
                .to_string_lossy()
                .into(),
            staged_name: "keep.part".into(),
            status: "verified".into(),
            detail: None,
            media: None,
        };
        write_json(&journal_path, &journal).unwrap();
        fs::write(staging.join("jobs").join(".job.json.interrupted.tmp"), b"{").unwrap();

        let recovery = recover_staging(&master, "device-a", None).unwrap();
        assert_eq!(recovery.recovered, 0);
        assert_eq!(recovery.needs_review, 1);
        assert!(journal_path.is_file());
        assert!(staged.is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn incompatible_project_manifest_is_never_overwritten() {
        let root = temporary("manifest");
        let path = root.join(".pimp-project.json");
        let original = br#"{"schemaVersion":1,"id":"existing","name":"Existing"}"#;
        fs::write(&path, original).unwrap();
        let project = ProjectRecord {
            id: "incoming".into(),
            name: "Incoming".into(),
            working_directory: ".".into(),
            media_ids: vec![],
            notes: String::new(),
            tags: vec![],
            created_at: "2026-10-22T00:00:00Z".into(),
        };
        assert!(write_project_manifest(&root, &project, &[]).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::remove_dir_all(root).unwrap();
    }
}
