mod catalogue;
mod discovery;
mod fs_safe;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use catalogue::{Catalogue, CatalogueEvent, MediaItem, ProjectRecord, TechnicalMetadata};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Timelike, Utc};
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
    catalogue_write: Mutex<()>,
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
    #[serde(default = "default_date_mode")]
    ingest_date_organization: String,
    #[serde(default = "legacy_date_folder_pattern")]
    date_folder_pattern: String,
    #[serde(default)]
    initialized_catalogue_path: String,
    #[serde(default)]
    project_paths: HashMap<String, String>,
}
fn default_date_mode() -> String {
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
            ingest_date_organization: default_date_mode(),
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
struct IngestResult {
    imported: Vec<MediaItem>,
    duplicates: Vec<String>,
    conflicts: Vec<IngestConflict>,
    unsupported: Vec<String>,
    failures: Vec<String>,
    cancelled: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct IngestConflict {
    source_path: String,
    destination_path: String,
    reason: String,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct RescanResult {
    registered: usize,
    changed: usize,
    pending: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MediaUpdate {
    media_ids: Vec<String>,
    tags: Option<Vec<String>>,
    project_ids: Option<Vec<String>>,
    add_tags: Option<Vec<String>>,
    add_project_ids: Option<Vec<String>>,
    notes: Option<String>,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("device-settings.json"))
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
    read_json(&settings_path(app)?)
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
fn load_catalogue(app: AppHandle) -> Result<Catalogue, String> {
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
    Ok(catalogue)
}
fn supported(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "mp4" | "mov" | "m4v" | "h264" | "h265" | "hevc" | "wav"
    )
}
fn ignored(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    name.starts_with(".syncthing.")
        || name.ends_with(".tmp")
        || name.ends_with(".part")
        || name.ends_with(".partial")
        || name.ends_with(".crdownload")
        || name.contains(".pimp-part-")
        || name.contains(".sync-conflict-")
        || path.components().any(|c| c.as_os_str() == ".pimp")
}
fn source_files(sources: &[String], unsupported: &mut Vec<String>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for source in sources {
        let path = PathBuf::from(source);
        if path.is_file() {
            if supported(&path) {
                files.push(path)
            } else {
                unsupported.push(source.clone())
            }
        } else if path.is_dir() {
            for entry in WalkDir::new(&path)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
            {
                let candidate = entry.path();
                if entry.file_type().is_file() && !ignored(candidate) {
                    if supported(candidate) {
                        files.push(candidate.to_path_buf())
                    } else {
                        unsupported.push(candidate.to_string_lossy().to_string())
                    }
                }
            }
        } else {
            unsupported.push(source.clone())
        }
    }
    files.sort();
    files.dedup();
    files
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
) -> Result<(String, Option<String>, String), String> {
    let modified = source
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .map(DateTime::<Local>::from);
    let capture = modified.map(|d| d.format("%Y-%m-%dT%H:%M:%S%:z").to_string());
    let today = Local::now().date_naive();
    let chosen = match override_date.as_deref() {
        Some("today") => today,
        Some("yesterday") => today - Duration::days(1),
        Some(date) if date.len() == 10 => {
            chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap_or(today)
        }
        _ if settings.ingest_date_organization == "original-file-dates" => {
            modified.map(|d| d.date_naive()).unwrap_or(today)
        }
        _ => today,
    };
    let time = modified
        .map(|value| value.time())
        .unwrap_or_else(|| Local::now().time());
    Ok((
        format_date_folder(&settings.date_folder_pattern, chosen, time)?,
        capture,
        if modified.is_some() {
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
}

struct VerifiedCopy {
    hash: String,
    journal_path: PathBuf,
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

fn recover_staging(master: &Path, device_id: &str) -> Result<usize, String> {
    let root = fs_safe::staging_root(master, device_id)?;
    let jobs = root.join("jobs");
    fs::create_dir_all(&jobs).map_err(|error| error.to_string())?;
    let mut recovered = 0;
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
        if staged.is_file() {
            fs::remove_file(&staged).map_err(|error| error.to_string())?;
        }
        // A committed journal owns only its staging artifact. The promoted
        // destination is durable user media and is reconciled by Rescan.
        remove_journal(&entry.path())?;
        recovered += 1;
    }
    Ok(recovered)
}

fn copy_verified(
    app: &AppHandle,
    state: &NativeState,
    settings: &DeviceSettings,
    job_id: &str,
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
        original_filename: source.file_name().unwrap().to_string_lossy().to_string(),
        sha256: hash,
        capture_date: capture,
        capture_date_provenance: provenance,
        ingest_date: Utc::now().to_rfc3339(),
        file_type,
        tags: vec![],
        project_ids: vec![],
        notes: String::new(),
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

#[tauri::command]
async fn start_ingest(
    app: AppHandle,
    state: State<'_, NativeState>,
    request: IngestRequest,
) -> Result<IngestResult, String> {
    let settings = load_settings_file(&app)?;
    let master = PathBuf::from(&settings.master_bin_path);
    let mut result = IngestResult::default();
    let files = source_files(&request.sources, &mut result.unsupported);
    let total_bytes = files
        .iter()
        .filter_map(|p| p.metadata().ok().map(|m| m.len()))
        .sum();
    let mut completed_bytes = 0u64;
    state.cancelled_jobs.lock().unwrap().remove(&request.job_id);
    let _guard = state.catalogue_write.lock().unwrap();
    let recovered = recover_staging(&master, &settings.device_id)?;
    if recovered > 0 {
        result.failures.push(format!(
            "Recovered {recovered} interrupted staging operation(s) before ingest"
        ));
    }
    let catalogue = load_catalogue_file(&settings)?;
    let mut known_paths: HashSet<String> = catalogue
        .media
        .iter()
        .map(|item| item.relative_path.clone())
        .collect();
    for (index, source) in files.iter().enumerate() {
        if state
            .cancelled_jobs
            .lock()
            .unwrap()
            .contains(&request.job_id)
        {
            result.cancelled = true;
            break;
        }
        let source_size = source.metadata().map(|m| m.len()).unwrap_or(0);
        let (date, capture, provenance) =
            destination_date(&settings, source, &request.date_override)?;
        let compact_date = date.replace('-', "");
        let date_folder = if master.join(&date).is_dir() {
            date.clone()
        } else if master.join(&compact_date).is_dir() {
            compact_date
        } else {
            date
        };
        let requested_name = request
            .renamed_destinations
            .get(&source.to_string_lossy().to_string())
            .cloned();
        let filename = requested_name
            .unwrap_or_else(|| source.file_name().unwrap().to_string_lossy().to_string());
        if Path::new(&filename).file_name().and_then(|x| x.to_str()) != Some(filename.as_str()) {
            result.failures.push(format!(
                "{}: invalid destination filename",
                source.display()
            ));
            continue;
        }
        let relative = format!("{date_folder}/{filename}").replace('\\', "/");
        let destination = match fs_safe::resolve_for_create(&master, &relative) {
            Ok(path) => path,
            Err(error) => {
                result
                    .failures
                    .push(format!("{}: {error}", source.display()));
                continue;
            }
        };
        if destination.exists() {
            match (sha256(source), sha256(&destination)) {
                (Ok(a), Ok(b)) if a == b => {
                    result.duplicates.push(source.to_string_lossy().to_string());
                    if !known_paths.contains(&relative) {
                        let item = new_media(
                            &destination,
                            relative,
                            b,
                            capture,
                            provenance,
                            false,
                        )?;
                        append_events(
                            &settings,
                            vec![CatalogueEvent::MediaRegistered {
                                media: item.clone(),
                            }],
                        )?;
                        known_paths.insert(item.relative_path.clone());
                        result.imported.push(item);
                    }
                }
                _ => result.conflicts.push(IngestConflict {
                    source_path: source.to_string_lossy().to_string(),
                    destination_path: destination.to_string_lossy().to_string(),
                    reason: "A different file already uses this destination name. Skip it, choose another date, or explicitly rename the incoming copy.".into(),
                }),
            }
            completed_bytes += source_size;
            continue;
        }
        match copy_verified(
            &app,
            &state,
            &settings,
            &request.job_id,
            source,
            &destination,
            completed_bytes,
            total_bytes,
            index,
            files.len(),
        ) {
            Ok(copy) => {
                let item = new_media(
                    &destination,
                    relative,
                    copy.hash,
                    capture,
                    provenance,
                    false,
                )?;
                if let Err(error) = append_events(
                    &settings,
                    vec![CatalogueEvent::MediaRegistered {
                        media: item.clone(),
                    }],
                ) {
                    result.failures.push(format!(
                        "{} was safely copied but catalogue persistence failed: {error}. Rescan can recover it.",
                        destination.display()
                    ));
                } else {
                    remove_journal(&copy.journal_path)?;
                    known_paths.insert(item.relative_path.clone());
                    result.imported.push(item);
                }
            }
            Err(error) if error == "__cancelled__" => {
                result.cancelled = true;
                break;
            }
            Err(error) if error == "__race__" => result.conflicts.push(IngestConflict {
                source_path: source.to_string_lossy().to_string(),
                destination_path: destination.to_string_lossy().to_string(),
                reason: "Another operation created the destination while copying; the verified staged copy was discarded without replacing it.".into(),
            }),
            Err(error) => result
                .failures
                .push(format!("{}: {error}", source.display())),
        }
        completed_bytes += source_size;
    }
    state.cancelled_jobs.lock().unwrap().remove(&request.job_id);
    let _ = app.emit("catalogue-changed", ());
    Ok(result)
}
#[tauri::command]
fn cancel_ingest(state: State<NativeState>, job_id: String) {
    state.cancelled_jobs.lock().unwrap().insert(job_id);
}
fn normalized(values: &[String]) -> Vec<String> {
    let mut out: Vec<_> = values
        .iter()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect();
    out.sort_by_key(|a| a.to_lowercase());
    out.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    out
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
                    .filter(|value| !item.tags.contains(value))
                    .cloned()
                    .collect(),
            });
            events.push(CatalogueEvent::MediaTagsRemoved {
                media_id: item.id.clone(),
                values: item
                    .tags
                    .iter()
                    .filter(|value| !desired.contains(value))
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
                    .filter(|value| !item.project_ids.contains(value))
                    .cloned()
                    .collect(),
            });
            events.push(CatalogueEvent::MediaProjectsRemoved {
                media_id: item.id.clone(),
                values: item
                    .project_ids
                    .iter()
                    .filter(|value| !desired.contains(value))
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
        if let Some(notes) = &update.notes {
            events.push(CatalogueEvent::MediaNotesSet {
                media_id: item.id.clone(),
                value: notes.clone(),
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
    let _guard = state.catalogue_write.lock().unwrap();
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
        if let Some(item) = known.get(&relative) {
            events.push(CatalogueEvent::MediaEvidenceObserved {
                media_id: item.id.clone(),
                observed_size: snapshot.length,
                observed_modified_millis: snapshot.modified_millis.map(|value| value as u64),
                observed_sha256: Some(hash.clone()),
                needs_review: hash != item.sha256,
            });
            if hash != item.sha256 {
                result.changed += 1;
            }
        } else {
            let (_, capture, provenance) = destination_date(&settings, path, &None)?;
            let item = new_media(path, relative.clone(), hash, capture, provenance, true)?;
            events.push(CatalogueEvent::MediaRegistered { media: item });
            result.registered += 1;
        }
        observations.verified(&relative);
    }
    append_events(&settings, events)?;
    discovery::save(&discovery_path, &observations)?;
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
    let master = PathBuf::from(&settings.master_bin_path);
    let cache = PathBuf::from(&settings.thumbnail_cache_path);
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let _guard = state.catalogue_write.lock().unwrap();
    let catalogue = load_catalogue_file(&settings)?;
    let mut events = Vec::new();
    let mut changed = 0;
    for item in &catalogue.media {
        let source = match fs_safe::resolve_existing(&master, &item.relative_path) {
            Ok(path) if path.is_file() => path,
            _ => continue,
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
        events.push(CatalogueEvent::MediaEnriched {
            media_id: item.id.clone(),
            technical,
            thumbnail_ref: thumbnail.is_file().then_some(thumbnail_ref),
        });
        changed += 1;
    }
    append_events(&settings, events)?;
    Ok(changed)
}
#[tauri::command]
fn resolve_media_path(app: AppHandle, media_id: String) -> Result<String, String> {
    let settings = load_settings_file(&app)?;
    let catalogue = load_catalogue_file(&settings)?;
    let item = catalogue
        .media
        .iter()
        .find(|m| m.id == media_id)
        .ok_or("Unknown media item")?;
    Ok(
        fs_safe::resolve_existing(Path::new(&settings.master_bin_path), &item.relative_path)?
            .to_string_lossy()
            .to_string(),
    )
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
            start_ingest,
            cancel_ingest,
            update_media,
            create_project,
            rescan,
            verify_media_integrity,
            enrich_media,
            resolve_media_path,
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

    #[test]
    fn accepts_only_initial_media_extensions() {
        assert!(supported(Path::new("clip.MP4")));
        assert!(supported(Path::new("audio.wav")));
        assert!(!supported(Path::new("project.prproj")));
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
        assert!(recover_staging(&master, "device-a").is_err());
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
            },
        )
        .unwrap();
        assert_eq!(recover_staging(&master, "device-a").unwrap(), 1);
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
        write_json(
            &journal_path,
            &IngestJournal {
                job_id: "job".into(),
                source_path: "source".into(),
                destination_path: destination.to_string_lossy().into(),
                staged_name: "already-promoted.part".into(),
                status: "committed".into(),
                detail: None,
            },
        )
        .unwrap();
        assert_eq!(recover_staging(&master, "device-a").unwrap(), 1);
        assert_eq!(fs::read(&destination).unwrap(), b"verified-media");
        assert!(!staged.exists());
        assert!(!journal_path.exists());
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
