mod catalogue;
mod discovery;
mod fs_safe;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use catalogue::{
    Catalogue, CatalogueEvent, CatalogueIssue, MediaItem, ProjectRecord, TechnicalMetadata,
};
#[cfg(test)]
use chrono::TimeZone;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;
use tauri_plugin_opener::OpenerExt;
use uuid::Uuid;
use walkdir::WalkDir;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn should_suppress_child_console(is_windows: bool, debug_build: bool) -> bool {
    is_windows && !debug_build
}

fn media_tool_command(program: &str) -> Command {
    let mut command = Command::new(program);
    #[cfg(target_os = "windows")]
    if should_suppress_child_console(true, cfg!(debug_assertions)) {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

#[derive(Default)]
struct NativeState {
    cancelled_jobs: Mutex<HashSet<String>>,
    paused_jobs: Mutex<HashSet<String>>,
    catalogue_write: Mutex<()>,
    project_manifest_write: Mutex<()>,
    discovery_write: Mutex<()>,
    discovery_status: Mutex<DiscoveryStatus>,
    enrichment_write: Mutex<()>,
    job_store: Mutex<()>,
    worker_active: Mutex<bool>,
    availability: Mutex<HashMap<String, (String, bool)>>,
    tray_available: Mutex<bool>,
    update_installing: Mutex<bool>,
    preview_generation: Mutex<()>,
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
    #[serde(default = "default_true")]
    hover_previews_enabled: bool,
    #[serde(default = "default_preview_cache_limit_bytes")]
    preview_cache_limit_bytes: u64,
    #[serde(default = "legacy_date_mode")]
    ingest_date_organization: String,
    #[serde(default = "legacy_date_folder_pattern")]
    date_folder_pattern: String,
    #[serde(default)]
    initialized_catalogue_path: String,
    #[serde(default)]
    project_paths: HashMap<String, String>,
    #[serde(default)]
    start_with_windows: bool,
    #[serde(default = "default_true")]
    close_to_tray: bool,
    #[serde(default = "default_startup_page")]
    startup_page: String,
    #[serde(default = "default_true")]
    check_for_updates: bool,
    #[serde(default = "default_theme")]
    theme: String,
    #[serde(default = "default_density")]
    density: String,
    #[serde(default)]
    show_attached_audio_separately: bool,
    #[serde(default)]
    sidebar_collapsed: bool,
    #[serde(default = "default_library_folder_sort")]
    library_folder_sort: String,
    #[serde(default)]
    library_folder_order: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct PendingProjectManifest {
    project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    directory: Option<String>,
    attempts: u32,
    last_error: String,
    updated_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ProjectManifestSyncState {
    #[serde(default)]
    pending: BTreeMap<String, PendingProjectManifest>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ProjectAsset {
    relative_path: String,
    name: String,
    category: String,
    size_bytes: u64,
    modified_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ProjectInventory {
    project_id: String,
    root_path: String,
    item_count: usize,
    size_bytes: u64,
    main_premiere_project: Option<String>,
    scanned_at: String,
    items: Vec<ProjectAsset>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct HoverPreview {
    media_id: String,
    duration_seconds: f64,
    frame_times: Vec<f64>,
    frames: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct HoverPreviewManifest {
    version: u32,
    media_id: String,
    fingerprint: String,
    duration_seconds: f64,
    frame_times: Vec<f64>,
    frame_files: Vec<String>,
    generated_at: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectUpdate {
    project_id: String,
    notes: String,
    tags: Vec<String>,
}
fn default_true() -> bool {
    true
}
fn default_startup_page() -> String {
    "Library".into()
}
fn default_theme() -> String {
    "graphite".into()
}
fn default_density() -> String {
    "comfortable".into()
}
fn default_library_folder_sort() -> String {
    "newest".into()
}
fn default_preview_cache_limit_bytes() -> u64 {
    512 * 1024 * 1024
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
            hover_previews_enabled: true,
            preview_cache_limit_bytes: default_preview_cache_limit_bytes(),
            ingest_date_organization: "today".into(),
            date_folder_pattern: "yyyymmdd".into(),
            initialized_catalogue_path: String::new(),
            project_paths: HashMap::new(),
            start_with_windows: false,
            close_to_tray: true,
            startup_page: default_startup_page(),
            check_for_updates: true,
            theme: default_theme(),
            density: default_density(),
            show_attached_audio_separately: false,
            sidebar_collapsed: false,
            library_folder_sort: default_library_folder_sort(),
            library_folder_order: vec![],
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
    #[serde(default)]
    planned_relative_path: String,
    size_bytes: u64,
    kind: String,
    analysis_supported: bool,
    preserve_hierarchy: bool,
    #[serde(skip)]
    source_root_path: String,
    #[serde(skip)]
    source_root_label: String,
    #[serde(skip)]
    include_source_root: bool,
    #[serde(skip)]
    package_key: Option<String>,
    #[serde(skip)]
    package_date_source_path: Option<String>,
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

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct RescanResult {
    scanned: usize,
    registered: usize,
    changed: usize,
    pending: usize,
    errors: usize,
    full: bool,
    completed_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct DiscoveryStatus {
    running: bool,
    full: bool,
    files_processed: usize,
    total_files: usize,
    registered: usize,
    changed: usize,
    pending: usize,
    errors: usize,
    started_at: Option<String>,
    last_completed_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MediaToolsStatus {
    ffmpeg_available: bool,
    ffprobe_available: bool,
    ffmpeg_version: Option<String>,
    ffprobe_version: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: String,
    build_commit: String,
    edition: String,
    release_channel: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdaterConfiguration {
    configured: bool,
    channel: String,
    endpoint: String,
    reason: Option<String>,
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

fn discovery_status_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(settings_path(app)?
        .parent()
        .ok_or("Invalid application configuration path")?
        .join("discovery-status.json"))
}

fn project_manifest_sync_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(settings_path(app)?
        .parent()
        .ok_or("Invalid application configuration path")?
        .join("project-manifest-sync.json"))
}

fn diagnostic_log_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = settings_path(app)?
        .parent()
        .ok_or("Invalid application configuration path")?
        .join("logs");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join("pimp.log"))
}

fn diagnostic_log(app: &AppHandle, level: &str, event: &str, detail: &str) {
    let Ok(path) = diagnostic_log_path(app) else {
        return;
    };
    if path.metadata().map(|metadata| metadata.len()).unwrap_or(0) > 2 * 1024 * 1024 {
        let _ = fs::rename(&path, path.with_extension("log.previous"));
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let sanitized = detail.replace(['\r', '\n'], " ");
        let _ = writeln!(
            file,
            "{} [{level}] {event}: {}",
            Utc::now().to_rfc3339(),
            sanitized.chars().take(2000).collect::<String>()
        );
    }
}
fn load_jobs_unlocked(app: &AppHandle) -> Result<Vec<IngestJob>, String> {
    read_json(&jobs_path(app)?)
}
const RETAINED_TERMINAL_JOBS: usize = 100;

fn job_is_fully_finalized(job: &IngestJob) -> bool {
    job.state == "completed"
        && !job.files.is_empty()
        && job.inventory_issues.is_empty()
        && job.failures.is_empty()
        && job.files.iter().all(|file| {
            file.state == "completed"
                && file.pending_metadata.is_none()
                && file.metadata_error.is_none()
                && file.error.is_none()
        })
}

fn prune_terminal_jobs(jobs: &mut Vec<IngestJob>) {
    let finalized_count = jobs
        .iter()
        .filter(|job| job_is_fully_finalized(job))
        .count();
    let mut remove = finalized_count.saturating_sub(RETAINED_TERMINAL_JOBS);
    jobs.retain(|job| {
        if remove > 0 && job_is_fully_finalized(job) {
            remove -= 1;
            false
        } else {
            true
        }
    });
}

fn mutate_jobs<T>(
    app: &AppHandle,
    state: &NativeState,
    operation: impl FnOnce(&mut Vec<IngestJob>) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = state.job_store.lock().unwrap();
    let mut jobs = load_jobs_unlocked(app)?;
    let result = operation(&mut jobs)?;
    prune_terminal_jobs(&mut jobs);
    write_json(&jobs_path(app)?, &jobs)?;
    let _ = app.emit("ingest-jobs-changed", &jobs);
    Ok(result)
}
fn snapshot_jobs(app: &AppHandle, state: &NativeState) -> Result<Vec<IngestJob>, String> {
    let _guard = state.job_store.lock().unwrap();
    load_jobs_unlocked(app)
}

const UPDATE_OPERATION_BLOCKED: &str =
    "A verified update is being installed. Wait for the application to restart before starting or resuming ingest.";

fn job_blocks_update(job: &IngestJob) -> bool {
    !matches!(job.state.as_str(), "completed" | "failed" | "cancelled")
}

fn begin_update_gate(installing: &mut bool, jobs: &[IngestJob]) -> Result<(), String> {
    if *installing {
        return Err("A verified update installation is already in progress.".into());
    }
    if jobs.iter().any(job_blocks_update) {
        return Err(
            "Finish, cancel, or resolve every active ingest job before installing an update."
                .into(),
        );
    }
    *installing = true;
    Ok(())
}

fn end_update_gate(update_installing: &Mutex<bool>) {
    *update_installing.lock().unwrap() = false;
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

fn write_json_if_changed<T: Serialize>(path: &Path, value: &T) -> Result<bool, String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    if path.is_file()
        && fs::read(path)
            .map_err(|error| format!("Cannot read {}: {error}", path.display()))?
            == bytes
    {
        return Ok(false);
    }
    write_json(path, value)?;
    Ok(true)
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
    let _manifest_guard = state.project_manifest_write.lock().unwrap();
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
    if !matches!(
        settings.theme.as_str(),
        "graphite" | "adobe-dark" | "light" | "midnight" | "system" | "high-contrast"
    ) {
        return Err("Choose a supported appearance theme".into());
    }
    if !matches!(
        settings.startup_page.as_str(),
        "Ingest" | "Library" | "Projects" | "Archive"
    ) {
        return Err("Choose a supported startup page".into());
    }
    if !matches!(settings.density.as_str(), "comfortable" | "compact") {
        return Err("Choose a supported layout density".into());
    }
    if !matches!(
        settings.library_folder_sort.as_str(),
        "newest"
            | "oldest"
            | "name-asc"
            | "name-desc"
            | "most-recordings"
            | "fewest-recordings"
            | "custom"
    ) {
        return Err("Choose a supported Library folder ordering".into());
    }
    fs::create_dir_all(settings.master_bin_path.trim())
        .map_err(|e| format!("Cannot create Master Bin: {e}"))?;
    fs::create_dir_all(settings.working_projects_path.trim())
        .map_err(|e| format!("Cannot create Working Projects directory: {e}"))?;
    let next_catalogue = catalogue_dir(&settings)?.to_string_lossy().to_string();
    let current = load_settings_file(&app)?;
    settings.device_id = preserve_device_identity(&current.device_id, &settings.device_id)?;
    settings.project_paths = current.project_paths.clone();
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
    write_json(&settings_path(&app)?, &settings)?;
    let autostart_result = if settings.start_with_windows {
        app.autolaunch().enable()
    } else {
        app.autolaunch().disable()
    };
    if let Err(error) = autostart_result {
        diagnostic_log(
            &app,
            "WARN",
            "autostart",
            &format!("Could not apply startup preference: {error}"),
        );
    }
    diagnostic_log(&app, "INFO", "settings", "Device settings saved");
    Ok(settings)
}

#[tauri::command]
fn load_catalogue(app: AppHandle, state: State<'_, NativeState>) -> Result<Catalogue, String> {
    let _manifest_guard = state.project_manifest_write.lock().unwrap();
    let mut settings = load_settings_file(&app)?;
    let mut catalogue = load_catalogue_file(&settings)?;
    let sync_path = project_manifest_sync_path(&app)?;
    recover_pending_project_paths(
        &sync_path,
        &settings_path(&app)?,
        &mut settings,
        &catalogue,
    )?;
    let drifted_manifests = project_manifest_drift_requests(&settings, &catalogue);
    let manifest_issues = reconcile_project_manifests(
        &app,
        &settings,
        &catalogue,
        drifted_manifests,
    )?;
    drop(_manifest_guard);
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
    catalogue.issues.extend(manifest_issues);
    *state.availability.lock().unwrap() = catalogue
        .media
        .iter()
        .map(|item| {
            (
                item.id.clone(),
                (item.relative_path.clone(), item.available),
            )
        })
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
        "mp4" | "mov" | "m4v" | "mxf" | "h264" | "h265" | "hevc" | "mts" | "m2ts" | "avi"
        | "r3d" | "braw" => "video",
        "wav" | "bwf" | "aif" | "aiff" | "mp3" | "m4a" => "audio",
        "jpg" | "jpeg" | "png" | "tif" | "tiff" | "dng" | "arw" | "cr2" | "cr3" => "image",
        "xml" | "xmp" | "bim" | "thm" | "cpi" | "mpl" | "bdm" | "idx" | "ind" | "pro" | "dat"
        | "bin" | "srt" | "lrv" | "csv" | "json" => "camera-support",
        _ => "unknown",
    }
}

fn supported(path: &Path) -> bool {
    matches!(
        extension(path).as_str(),
        "mp4"
            | "mov"
            | "m4v"
            | "mxf"
            | "h264"
            | "h265"
            | "hevc"
            | "mts"
            | "m2ts"
            | "avi"
            | "wav"
            | "bwf"
            | "aif"
            | "aiff"
            | "mp3"
            | "m4a"
            | "jpg"
            | "jpeg"
            | "png"
            | "tif"
            | "tiff"
            | "dng"
            | "arw"
            | "cr2"
            | "cr3"
            | "r3d"
            | "braw"
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
    if path
        .components()
        .any(|component| component.as_os_str() == ".pimp")
    {
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
            component
                .as_os_str()
                .to_string_lossy()
                .to_ascii_uppercase()
                .as_str(),
            "XDROOT"
                | "PRIVATE"
                | "AVCHD"
                | "BPAV"
                | "CONTENTS"
                | "M4ROOT"
                | "DCIM"
                | "CLIP"
                | "SUB"
                | "GENERAL"
                | "SONY"
        )
    })
}

fn camera_package_root(relative: &Path) -> Option<PathBuf> {
    let mut prefix = PathBuf::new();
    for component in relative.components() {
        let std::path::Component::Normal(value) = component else {
            continue;
        };
        prefix.push(value);
        if matches!(
            value.to_string_lossy().to_ascii_uppercase().as_str(),
            "XDROOT" | "PRIVATE" | "AVCHD" | "BPAV" | "CONTENTS" | "M4ROOT" | "DCIM"
        ) {
            return Some(prefix);
        }
    }
    None
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
    source_root: &Path,
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
    let source_root_path = source_root.to_string_lossy().to_string();
    let source_root_label = source_root
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let package_key = if root_is_package {
        Some(source_root_path.clone())
    } else {
        camera_package_root(relative)
            .map(|package_root| source_root.join(package_root).to_string_lossy().to_string())
    };
    inventory.files.push(InventoryFile {
        source_path: candidate.to_string_lossy().into(),
        source_relative_path: portable_relative(relative),
        planned_relative_path: String::new(),
        size_bytes: size.unwrap_or(0),
        kind: media_kind(candidate).into(),
        analysis_supported: analysis_supported(candidate),
        preserve_hierarchy: root_is_package || package_component(relative),
        source_root_path,
        source_root_label,
        include_source_root: root_is_package,
        package_key,
        package_date_source_path: None,
    });
}

fn build_source_inventory(sources: &[String]) -> SourceInventory {
    let mut inventory = SourceInventory::default();
    for source in sources {
        let path = PathBuf::from(source);
        if path.is_file() {
            let relative = PathBuf::from(path.file_name().unwrap_or_default());
            let root = path.parent().unwrap_or_else(|| Path::new("."));
            inventory_candidate(&mut inventory, &path, &relative, root, false);
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
                            &path,
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
    inventory
        .files
        .sort_by(|left, right| left.source_path.cmp(&right.source_path));
    inventory
        .files
        .dedup_by(|left, right| left.source_path == right.source_path);
    let mut roots = inventory
        .files
        .iter()
        .map(|file| {
            (
                file.source_root_path.clone(),
                file.source_root_label.clone(),
            )
        })
        .collect::<Vec<_>>();
    roots.sort_by(|left, right| left.0.cmp(&right.0));
    roots.dedup_by(|left, right| left.0 == right.0);
    let mut root_labels = HashMap::new();
    let mut used_labels = HashSet::new();
    for (root, label) in roots {
        let base = if label.trim().is_empty() {
            "source".to_string()
        } else {
            label
        };
        let mut candidate = base.clone();
        let mut suffix = 2;
        while !used_labels.insert(candidate.to_ascii_lowercase()) {
            candidate = format!("{base}-{suffix}");
            suffix += 1;
        }
        root_labels.insert(root, candidate);
    }
    let mut relative_paths = HashMap::<String, usize>::new();
    for file in &inventory.files {
        *relative_paths
            .entry(file.source_relative_path.to_ascii_lowercase())
            .or_default() += 1;
    }
    for file in &mut inventory.files {
        if file.include_source_root
            || relative_paths
                .get(&file.source_relative_path.to_ascii_lowercase())
                .copied()
                .unwrap_or(0)
                > 1
        {
            let label = root_labels
                .get(&file.source_root_path)
                .cloned()
                .unwrap_or_else(|| "source".into());
            file.source_relative_path =
                portable_relative(&PathBuf::from(label).join(&file.source_relative_path));
            file.preserve_hierarchy = true;
        }
    }
    let mut package_representatives =
        HashMap::<String, (Option<std::time::SystemTime>, String)>::new();
    for file in &inventory.files {
        let Some(package_key) = &file.package_key else {
            continue;
        };
        let modified = Path::new(&file.source_path)
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok());
        let candidate = (modified, file.source_path.clone());
        package_representatives
            .entry(package_key.clone())
            .and_modify(|current| {
                let candidate_key = (candidate.0.is_none(), candidate.0, &candidate.1);
                let current_key = (current.0.is_none(), current.0, &current.1);
                if candidate_key < current_key {
                    *current = candidate.clone();
                }
            })
            .or_insert(candidate);
    }
    for file in &mut inventory.files {
        if let Some(package_key) = &file.package_key {
            file.package_date_source_path = package_representatives
                .get(package_key)
                .map(|(_, path)| path.clone());
        }
    }
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
fn inventory_sources(
    app: AppHandle,
    sources: Vec<String>,
    date_override: Option<String>,
) -> Result<SourceInventory, String> {
    let settings = load_settings_file(&app)?;
    let master = PathBuf::from(&settings.master_bin_path);
    let batch_timestamp = Local::now();
    let mut inventory = build_source_inventory(&sources);
    for file in &mut inventory.files {
        let filename = Path::new(&file.source_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        file.planned_relative_path = planned_relative_path(
            &settings,
            &master,
            file,
            &date_override,
            &batch_timestamp,
            &filename,
        )?;
    }
    Ok(inventory)
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
        None if settings.ingest_date_organization == "original-file-dates" => modified
            .as_ref()
            .map(|value| (value.date_naive(), value.time()))
            .unwrap_or((batch_timestamp.date_naive(), batch_timestamp.time())),
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

fn planned_relative_path(
    settings: &DeviceSettings,
    master: &Path,
    inventory_file: &InventoryFile,
    override_date: &Option<String>,
    batch_timestamp: &DateTime<Local>,
    filename: &str,
) -> Result<String, String> {
    let date_source = inventory_file
        .package_date_source_path
        .as_deref()
        .unwrap_or(&inventory_file.source_path);
    let (date, _, _) = destination_date(
        settings,
        Path::new(date_source),
        override_date,
        batch_timestamp,
    )?;
    let compact_date = date.replace('-', "");
    let date_folder = if !settings.master_bin_path.trim().is_empty() && master.join(&date).is_dir()
    {
        date
    } else if !settings.master_bin_path.trim().is_empty() && master.join(&compact_date).is_dir() {
        compact_date
    } else {
        date
    };
    let destination_suffix = if inventory_file.preserve_hierarchy {
        let source_relative = PathBuf::from(&inventory_file.source_relative_path);
        source_relative
            .parent()
            .map(|parent| parent.join(filename))
            .unwrap_or_else(|| PathBuf::from(filename))
    } else {
        PathBuf::from(filename)
    };
    Ok(format!(
        "{date_folder}/{}",
        portable_relative(&destination_suffix)
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
            if let Some(existing) = catalogue
                .media
                .iter()
                .find(|item| item.id == media.id || item.relative_path == media.relative_path)
            {
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

fn all_job_files_completed(job: &IngestJob) -> bool {
    job.inventory_issues.is_empty()
        && !job.files.is_empty()
        && job.files.iter().all(|file| file.state == "completed")
}

fn finish_job(app: &AppHandle, state: &NativeState, job_id: &str) -> Result<(), String> {
    mutate_jobs(app, state, |jobs| {
        let job = jobs
            .iter_mut()
            .find(|job| job.id == job_id)
            .ok_or("Ingest job no longer exists")?;
        job.current_file = None;
        job.state = if all_job_files_completed(job) {
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
    allowed
        .then_some(())
        .ok_or_else(|| format!("Cannot {action} an ingest job while it is {state}"))
}

fn actionable_ingest_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    let guidance = if lower.contains("not enough space")
        || lower.contains("no space left")
        || lower.contains("disk full")
    {
        "The destination drive is full. Free space in the Master Bin drive, then retry."
    } else if lower.contains("cannot read source")
        || lower.contains("interrupted source read")
        || lower.contains("access is denied")
        || lower.contains("permission denied")
    {
        "The source file could not be read. Reconnect the card, check permissions, then retry."
    } else if lower.contains("sha-256 verification failed") {
        "The copied bytes did not match the source. The staged copy was rejected; check the source and destination drives before retrying."
    } else if lower.contains("source changed during transfer")
        || lower.contains("source content changed during transfer")
    {
        "The source changed during copying. Stop other software from modifying the card, then retry."
    } else if error == "__race__" || lower.contains("destination exists") {
        "A file appeared at the destination while copying. Nothing was overwritten; review the collision and choose another name."
    } else {
        "The copy could not be completed. Check that the source and Master Bin are connected and writable, then retry."
    };
    format!("{guidance} Technical detail: {error}")
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
                update_job_progress(app, &state, job_id, &planned.id, "cancelled", 0, None)?;
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
                    Some(actionable_ingest_error(&error)),
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
    let update_guard = state.update_installing.lock().unwrap();
    if *update_guard {
        return Err(UPDATE_OPERATION_BLOCKED.into());
    }
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
        let filename = request
            .renamed_destinations
            .get(&source.to_string_lossy().to_string())
            .cloned()
            .unwrap_or_else(|| source.file_name().unwrap().to_string_lossy().to_string());
        if Path::new(&filename)
            .file_name()
            .and_then(|value| value.to_str())
            != Some(filename.as_str())
        {
            return Err(format!(
                "{} has an invalid destination filename",
                source.display()
            ));
        }
        let relative_path = planned_relative_path(
            &settings,
            &master,
            inventory_file,
            &request.date_override,
            &batch_timestamp,
            &filename,
        )?;
        let destination = fs_safe::resolve_for_create(&master, &relative_path)?;
        let destination_path = destination.to_string_lossy().to_string();
        let collision =
            reserved.contains(&destination_path) || !planned_paths.insert(destination_path.clone());
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
            error: collision
                .then(|| "Another queued ingest already reserves this destination".into()),
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
    drop(update_guard);
    kick_ingest_worker(app);
    Ok(job)
}

#[tauri::command]
fn list_ingest_jobs(
    app: AppHandle,
    state: State<'_, NativeState>,
) -> Result<Vec<IngestJob>, String> {
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
        return Err(
            "Catalogue identity or relative path disagrees with the persisted job plan".into(),
        );
    }
    let planned = PathBuf::from(&file.destination_path);
    if !planned.is_file() {
        return Err("The planned destination is missing or incomplete".into());
    }
    let expected = fs_safe::resolve_existing(master, &file.relative_path).map_err(|error| {
        format!("The planned destination is outside the configured root: {error}")
    })?;
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
                if all_job_files_completed(job) {
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

fn apply_metadata_patch(metadata: &mut ProvisionalMetadata, patch: &ProvisionalMetadataPatch) {
    if let Some(tags) = &patch.tags {
        metadata.tags = normalized(tags);
        if !metadata.edited_fields.iter().any(|value| value == "tags") {
            metadata.edited_fields.push("tags".into());
        }
    }
    if let Some(project_ids) = &patch.project_ids {
        metadata.project_ids = normalized(project_ids);
        if !metadata
            .edited_fields
            .iter()
            .any(|value| value == "projectIds")
        {
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
        if !metadata
            .edited_fields
            .iter()
            .any(|value| value == "favourite")
        {
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
fn pause_ingest(
    app: AppHandle,
    state: State<'_, NativeState>,
    job_id: String,
) -> Result<(), String> {
    let result = mutate_jobs(&app, &state, |jobs| {
        let job = jobs
            .iter_mut()
            .find(|job| job.id == job_id)
            .ok_or("Ingest job not found")?;
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
fn resume_ingest(
    app: AppHandle,
    state: State<'_, NativeState>,
    job_id: String,
) -> Result<(), String> {
    let update_guard = state.update_installing.lock().unwrap();
    if *update_guard {
        return Err(UPDATE_OPERATION_BLOCKED.into());
    }
    let current = snapshot_jobs(&app, &state)?
        .into_iter()
        .find(|job| job.id == job_id)
        .ok_or("Ingest job not found")?;
    validate_job_action(&current.state, "resume")?;
    state.paused_jobs.lock().unwrap().remove(&job_id);
    let result = mutate_jobs(&app, &state, |jobs| {
        let job = jobs
            .iter_mut()
            .find(|job| job.id == job_id)
            .ok_or("Ingest job not found")?;
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
    drop(update_guard);
    kick_ingest_worker(app);
    Ok(())
}

#[tauri::command]
fn retry_ingest(
    app: AppHandle,
    state: State<'_, NativeState>,
    job_id: String,
) -> Result<(), String> {
    let update_guard = state.update_installing.lock().unwrap();
    if *update_guard {
        return Err(UPDATE_OPERATION_BLOCKED.into());
    }
    let current = snapshot_jobs(&app, &state)?
        .into_iter()
        .find(|job| job.id == job_id)
        .ok_or("Ingest job not found")?;
    validate_job_action(&current.state, "retry")?;
    state.cancelled_jobs.lock().unwrap().remove(&job_id);
    state.paused_jobs.lock().unwrap().remove(&job_id);
    mutate_jobs(&app, &state, |jobs| {
        let job = jobs
            .iter_mut()
            .find(|job| job.id == job_id)
            .ok_or("Ingest job not found")?;
        validate_job_action(&job.state, "retry")?;
        prepare_job_retry(job);
        Ok(())
    })?;
    drop(update_guard);
    kick_ingest_worker(app);
    Ok(())
}

#[tauri::command]
fn cancel_ingest(app: AppHandle, state: State<NativeState>, job_id: String) -> Result<(), String> {
    let result = mutate_jobs(&app, &state, |jobs| {
        let job = jobs
            .iter_mut()
            .find(|job| job.id == job_id)
            .ok_or("Ingest job not found")?;
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

fn project_manifest_identity(value: &serde_json::Value) -> Result<String, String> {
    let identities = [
        value.get("id").and_then(serde_json::Value::as_str),
        value
            .get("project")
            .and_then(|project| project.get("id"))
            .and_then(serde_json::Value::as_str),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    let Some(first) = identities.first() else {
        return Err("Existing .pimp-project.json has no compatible project identity".into());
    };
    if identities.iter().any(|identity| identity != first) {
        return Err(
            "Existing .pimp-project.json contains conflicting project identities and was not overwritten"
                .into(),
        );
    }
    Ok((*first).to_string())
}

fn prepare_project_manifest(
    directory: &Path,
    project: &ProjectRecord,
    media: &[MediaItem],
) -> Result<(PathBuf, serde_json::Value, bool), String> {
    let manifest_path = directory.join(".pimp-project.json");
    let mut value = if manifest_path.exists() {
        let value: serde_json::Value = read_json(&manifest_path)?;
        let schema = value
            .get("schemaVersion")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1);
        if schema > catalogue::EVENT_SCHEMA_VERSION as u64 {
            return Err(format!(
                "Existing .pimp-project.json uses unsupported schema {schema}; it was not overwritten"
            ));
        }
        let existing_id = project_manifest_identity(&value)?;
        if existing_id != project.id {
            return Err(format!(
                "Existing .pimp-project.json belongs to project {existing_id}; it was not overwritten"
            ));
        }
        value
    } else {
        serde_json::json!({})
    };
    let original = manifest_path.exists().then(|| value.clone());
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
    let object = value
        .as_object_mut()
        .ok_or("Existing .pimp-project.json must contain a JSON object")?;
    object.insert("schemaVersion".into(), serde_json::json!(2));
    object.insert("media".into(), serde_json::json!(references));
    if let Some(project_value) = object.get_mut("project") {
        let project_object = project_value
            .as_object_mut()
            .ok_or("Existing project manifest has an incompatible project record")?;
        project_object.insert("id".into(), serde_json::json!(project.id));
        project_object.insert("name".into(), serde_json::json!(project.name));
        project_object.insert("workingDirectory".into(), serde_json::json!("."));
        project_object.insert("notes".into(), serde_json::json!(project.notes));
        project_object.insert("tags".into(), serde_json::json!(project.tags));
        project_object.insert("mediaIds".into(), serde_json::json!(project.media_ids));
        project_object.insert("createdAt".into(), serde_json::json!(project.created_at));
    } else {
        object.insert("id".into(), serde_json::json!(project.id));
        object.insert("name".into(), serde_json::json!(project.name));
        object.insert("workingDirectory".into(), serde_json::json!("."));
        object.insert("notes".into(), serde_json::json!(project.notes));
        object.insert("tags".into(), serde_json::json!(project.tags));
        object.insert("mediaIds".into(), serde_json::json!(project.media_ids));
        object.insert("createdAt".into(), serde_json::json!(project.created_at));
    }
    let changed = original.as_ref() != Some(&value);
    Ok((manifest_path, value, changed))
}

fn write_project_manifest(
    directory: &Path,
    project: &ProjectRecord,
    media: &[MediaItem],
) -> Result<bool, String> {
    let (manifest_path, value, changed) = prepare_project_manifest(directory, project, media)?;
    if changed {
        write_json(&manifest_path, &value)?;
    }
    Ok(changed)
}

fn project_manifest_drift_requests(
    settings: &DeviceSettings,
    catalogue: &Catalogue,
) -> Vec<(String, Option<String>)> {
    catalogue
        .projects
        .iter()
        .filter_map(|project| {
            let configured = settings.project_paths.get(&project.id)?;
            let requires_sync = match fs_safe::resolve_project_directory(
                Path::new(&settings.working_projects_path),
                Path::new(configured),
                false,
            ) {
                Ok(directory) => prepare_project_manifest(&directory, project, &catalogue.media)
                    .map(|(_, _, changed)| changed)
                    .unwrap_or(true),
                Err(_) => true,
            };
            requires_sync.then(|| (project.id.clone(), Some(configured.clone())))
        })
        .collect()
}

fn persist_project_manifest_intents_with<I, W>(
    state_path: &Path,
    requested: I,
    state_writer: &W,
) -> Result<(), String>
where
    I: IntoIterator<Item = (String, Option<String>)>,
    W: Fn(&Path, &ProjectManifestSyncState) -> Result<bool, String>,
{
    let mut state: ProjectManifestSyncState = read_json(state_path)?;
    let original = state.clone();
    for (project_id, directory) in requested {
        let pending = state
            .pending
            .entry(project_id.clone())
            .or_insert_with(|| PendingProjectManifest {
                project_id,
                directory: directory.clone(),
                attempts: 0,
                last_error: "Manifest synchronization is pending".into(),
                updated_at: Utc::now().to_rfc3339(),
            });
        if pending.directory.is_none() && directory.is_some() {
            pending.directory = directory;
            pending.updated_at = Utc::now().to_rfc3339();
        }
    }
    if state != original {
        state_writer(state_path, &state)?;
    }
    Ok(())
}

fn persist_project_manifest_intents(
    state_path: &Path,
    requested: impl IntoIterator<Item = (String, Option<String>)>,
) -> Result<(), String> {
    persist_project_manifest_intents_with(state_path, requested, &write_json_if_changed)
}

fn recover_pending_project_paths(
    state_path: &Path,
    settings_path: &Path,
    settings: &mut DeviceSettings,
    catalogue: &Catalogue,
) -> Result<bool, String> {
    let state: ProjectManifestSyncState = read_json(state_path)?;
    let mut changed = false;
    for pending in state.pending.values() {
        let Some(directory) = pending.directory.as_deref() else {
            continue;
        };
        if !catalogue
            .projects
            .iter()
            .any(|project| project.id == pending.project_id)
        {
            continue;
        }
        let Ok(recovered) = fs_safe::resolve_project_directory(
            Path::new(&settings.working_projects_path),
            Path::new(directory),
            false,
        ) else {
            continue;
        };
        if let Some(existing) = settings.project_paths.get(&pending.project_id) {
            let Ok(existing) = fs_safe::resolve_project_directory(
                Path::new(&settings.working_projects_path),
                Path::new(existing),
                false,
            ) else {
                continue;
            };
            if existing != recovered {
                continue;
            }
        } else {
            settings.project_paths.insert(
                pending.project_id.clone(),
                recovered.to_string_lossy().into_owned(),
            );
            changed = true;
        }
    }
    if changed {
        write_json_if_changed(settings_path, settings)?;
    }
    Ok(changed)
}

fn reconcile_project_manifests_with<I, M, W>(
    state_path: &Path,
    settings: &DeviceSettings,
    catalogue: &Catalogue,
    requested: I,
    manifest_writer: M,
    state_writer: W,
) -> Result<Vec<CatalogueIssue>, String>
where
    I: IntoIterator<Item = (String, Option<String>)>,
    M: Fn(&Path, &ProjectRecord, &[MediaItem]) -> Result<bool, String>,
    W: Fn(&Path, &ProjectManifestSyncState) -> Result<bool, String>,
{
    persist_project_manifest_intents_with(state_path, requested, &state_writer)?;
    let mut state: ProjectManifestSyncState = read_json(state_path)?;
    let durable_state = state.clone();
    let project_ids: BTreeSet<String> = state.pending.keys().cloned().collect();
    for project_id in project_ids {
        let Some(project) = catalogue.projects.iter().find(|project| project.id == project_id) else {
            state.pending.remove(&project_id);
            continue;
        };
        let pending_directory = state
            .pending
            .get(&project_id)
            .and_then(|pending| pending.directory.as_deref());
        let result = match (
            settings.project_paths.get(&project_id).map(String::as_str),
            pending_directory,
        ) {
            (Some(configured), Some(pending)) => fs_safe::resolve_project_directory(
                Path::new(&settings.working_projects_path),
                Path::new(configured),
                false,
            )
            .and_then(|configured| {
                let pending = fs_safe::resolve_project_directory(
                    Path::new(&settings.working_projects_path),
                    Path::new(pending),
                    false,
                )?;
                if configured != pending {
                    return Err(
                        "Pending project path conflicts with its existing local folder association"
                            .into(),
                    );
                }
                Ok(configured)
            }),
            (Some(configured), None) => fs_safe::resolve_project_directory(
                Path::new(&settings.working_projects_path),
                Path::new(configured),
                false,
            ),
            (None, Some(pending)) => fs_safe::resolve_project_directory(
                Path::new(&settings.working_projects_path),
                Path::new(pending),
                false,
            ),
            (None, None) => Err("Project is not associated with a local folder".into()),
        }
        .and_then(|directory| manifest_writer(&directory, project, &catalogue.media));
        match result {
            Ok(_) => {
                state.pending.remove(&project_id);
            }
            Err(error) => {
                let pending = state.pending.entry(project_id.clone()).or_insert_with(|| {
                    PendingProjectManifest {
                        project_id,
                        directory: None,
                        attempts: 0,
                        last_error: String::new(),
                        updated_at: String::new(),
                    }
                });
                pending.attempts = pending.attempts.saturating_add(1);
                pending.last_error = error;
                pending.updated_at = Utc::now().to_rfc3339();
            }
        }
    }
    if state != durable_state {
        state_writer(state_path, &state)?;
    }
    Ok(state
        .pending
        .values()
        .map(|pending| CatalogueIssue {
            code: "project-manifest-pending".into(),
            source: pending.project_id.clone(),
            detail: format!(
                "Catalogue changes are saved; project manifest retry {} is pending: {}",
                pending.attempts, pending.last_error
            ),
        })
        .collect())
}

fn reconcile_project_manifests(
    app: &AppHandle,
    settings: &DeviceSettings,
    catalogue: &Catalogue,
    requested: impl IntoIterator<Item = (String, Option<String>)>,
) -> Result<Vec<CatalogueIssue>, String> {
    reconcile_project_manifests_with(
        &project_manifest_sync_path(app)?,
        settings,
        catalogue,
        requested,
        write_project_manifest,
        write_json_if_changed,
    )
}

fn commit_project_creation_with<A, R>(
    sync_path: &Path,
    settings_file: &Path,
    settings: &mut DeviceSettings,
    project: &ProjectRecord,
    directory: &Path,
    append: A,
    reconcile: R,
) -> Result<(), String>
where
    A: FnOnce(&DeviceSettings) -> Result<(), String>,
    R: FnOnce(&DeviceSettings) -> Result<Vec<CatalogueIssue>, String>,
{
    persist_project_manifest_intents(
        sync_path,
        [(
            project.id.clone(),
            Some(directory.to_string_lossy().into_owned()),
        )],
    )?;
    append(settings).map_err(|error| {
        format!(
            "Project registration failed before a new manifest was created; the folder was preserved and recovery intent remains durable: {error}"
        )
    })?;
    settings.project_paths.insert(
        project.id.clone(),
        directory.to_string_lossy().into_owned(),
    );
    write_json_if_changed(settings_file, settings).map_err(|error| {
        format!(
            "Project registration was saved, but its local folder association is pending automatic recovery: {error}"
        )
    })?;
    if !reconcile(settings)?.is_empty() {
        return Err(
            "Project registration was saved, but its portable manifest is pending automatic retry; do not create the project again."
                .into(),
        );
    }
    Ok(())
}

fn project_identity_for_directory(
    sync_path: &Path,
    settings: &DeviceSettings,
    directory: &Path,
    manifest_id: Option<&str>,
) -> Result<Option<String>, String> {
    let projects_root = Path::new(&settings.working_projects_path);
    let mut identities = BTreeSet::new();
    for (project_id, configured) in &settings.project_paths {
        if fs_safe::resolve_project_directory(projects_root, Path::new(configured), false)
            .is_ok_and(|resolved| resolved == directory)
        {
            identities.insert(project_id.clone());
        }
    }
    let pending: ProjectManifestSyncState = read_json(sync_path)?;
    for record in pending.pending.values() {
        let Some(configured) = record.directory.as_deref() else {
            continue;
        };
        if fs_safe::resolve_project_directory(projects_root, Path::new(configured), false)
            .is_ok_and(|resolved| resolved == directory)
        {
            identities.insert(record.project_id.clone());
        }
    }
    if let Some(manifest_id) = manifest_id {
        identities.insert(manifest_id.to_string());
    }
    if identities.len() > 1 {
        return Err(format!(
            "The selected folder has conflicting project identities ({}). No files or associations were changed.",
            identities.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    Ok(identities.into_iter().next())
}

fn project_from_manifest(
    value: &serde_json::Value,
    fallback_name: &str,
) -> Result<ProjectRecord, String> {
    let id = project_manifest_identity(value)?;
    let project_value = value.get("project").unwrap_or(value);
    Ok(ProjectRecord {
        id,
        name: project_value
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(fallback_name)
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
    })
}

#[tauri::command]
async fn update_media(
    app: AppHandle,
    state: State<'_, NativeState>,
    update: MediaUpdate,
) -> Result<Catalogue, String> {
    let _manifest_guard = state.project_manifest_write.lock().unwrap();
    let settings = load_settings_file(&app)?;
    let catalogue_guard = state.catalogue_write.lock().unwrap();
    let catalogue = load_catalogue_file(&settings)?;
    let mut requested = BTreeSet::new();
    for item in catalogue
        .media
        .iter()
        .filter(|item| update.media_ids.contains(&item.id))
    {
        requested.extend(item.project_ids.iter().cloned());
    }
    for projects in [
        update.project_ids.as_ref(),
        update.add_project_ids.as_ref(),
        update.remove_project_ids.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        requested.extend(projects.iter().cloned());
    }
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
    drop(catalogue_guard);
    let sync_path = project_manifest_sync_path(&app)?;
    persist_project_manifest_intents(
        &sync_path,
        requested.into_iter().map(|project_id| (project_id, None)),
    )?;
    let catalogue_guard = state.catalogue_write.lock().unwrap();
    append_events(&settings, events)?;
    let mut next = load_catalogue_file(&settings)?;
    drop(catalogue_guard);
    let manifest_issues = reconcile_project_manifests(
        &app,
        &settings,
        &next,
        std::iter::empty::<(String, Option<String>)>(),
    )
        .map_err(|error| {
            format!(
                "Catalogue changes were saved, but manifest recovery state could not be persisted: {error}"
            )
        })?;
    next.issues.extend(manifest_issues);
    Ok(next)
}

#[tauri::command]
fn update_project(
    app: AppHandle,
    state: State<NativeState>,
    update: ProjectUpdate,
) -> Result<Catalogue, String> {
    let _manifest_guard = state.project_manifest_write.lock().unwrap();
    let settings = load_settings_file(&app)?;
    let catalogue = {
        let _catalogue_guard = state.catalogue_write.lock().unwrap();
        load_catalogue_file(&settings)?
    };
    let mut project = catalogue
        .projects
        .iter()
        .find(|project| project.id == update.project_id)
        .cloned()
        .ok_or("Unknown project")?;
    project.notes = update.notes;
    project.tags = normalized(&update.tags);
    let directory = settings.project_paths.get(&project.id).cloned();
    persist_project_manifest_intents(
        &project_manifest_sync_path(&app)?,
        [(project.id.clone(), directory)],
    )?;
    {
        let _catalogue_guard = state.catalogue_write.lock().unwrap();
        append_events(
            &settings,
            vec![CatalogueEvent::ProjectUpserted {
                project: project.clone(),
            }],
        )?;
    }
    let mut next = load_catalogue_file(&settings)?;
    let issues = reconcile_project_manifests(
        &app,
        &settings,
        &next,
        std::iter::empty::<(String, Option<String>)>(),
    )?;
    next.issues.extend(issues);
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
    let _manifest_guard = state.project_manifest_write.lock().unwrap();
    let mut settings = load_settings_file(&app)?;
    if settings.working_projects_path.trim().is_empty() {
        return Err("Configure the Working Projects root first".into());
    }
    let directory = fs_safe::resolve_project_directory(
        Path::new(&settings.working_projects_path),
        Path::new(&working_directory),
        true,
    )?;
    let catalogue = {
        let _catalogue_guard = state.catalogue_write.lock().unwrap();
        load_catalogue_file(&settings)?
    };
    let sync_path = project_manifest_sync_path(&app)?;
    let manifest_path = directory.join(".pimp-project.json");
    let manifest_value = manifest_path
        .exists()
        .then(|| read_json::<serde_json::Value>(&manifest_path))
        .transpose()?;
    let manifest_id = manifest_value
        .as_ref()
        .map(project_manifest_identity)
        .transpose()?;
    let recovered_id = project_identity_for_directory(
        &sync_path,
        &settings,
        &directory,
        manifest_id.as_deref(),
    )?;
    let manifest_project = manifest_value
        .as_ref()
        .map(|value| project_from_manifest(value, name.trim()))
        .transpose()?;
    let project = recovered_id
        .as_ref()
        .and_then(|id| catalogue.projects.iter().find(|project| &project.id == id))
        .cloned()
        .or(manifest_project)
        .unwrap_or_else(|| ProjectRecord {
            id: recovered_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
            name: name.trim().into(),
            working_directory: ".".into(),
            media_ids: vec![],
            notes: String::new(),
            tags: vec![],
            created_at: Utc::now().to_rfc3339(),
        });
    if let Some(existing) = settings.project_paths.get(&project.id) {
        let existing = fs_safe::resolve_project_directory(
            Path::new(&settings.working_projects_path),
            Path::new(existing),
            false,
        )?;
        if existing != directory {
            return Err(format!(
                "Project {} is already associated with a different authorized folder",
                project.id
            ));
        }
    }
    let already_registered = catalogue
        .projects
        .iter()
        .any(|existing| existing.id == project.id);
    let settings_file = settings_path(&app)?;
    commit_project_creation_with(
        &sync_path,
        &settings_file,
        &mut settings,
        &project,
        &directory,
        |current_settings| {
            if already_registered {
                return Ok(());
            }
            let _catalogue_guard = state.catalogue_write.lock().unwrap();
            append_events(
                current_settings,
                vec![CatalogueEvent::ProjectUpserted {
                    project: project.clone(),
                }],
            )
        },
        |current_settings| {
            let catalogue = load_catalogue_file(current_settings)?;
            reconcile_project_manifests(
                &app,
                current_settings,
                &catalogue,
                std::iter::empty::<(String, Option<String>)>(),
            )
        },
    )?;
    Ok(project)
}

#[tauri::command]
fn retry_project_synchronization(
    app: AppHandle,
    state: State<NativeState>,
    project_id: String,
) -> Result<Vec<CatalogueIssue>, String> {
    let _manifest_guard = state.project_manifest_write.lock().unwrap();
    let settings = load_settings_file(&app)?;
    let catalogue = {
        let _catalogue_guard = state.catalogue_write.lock().unwrap();
        load_catalogue_file(&settings)?
    };
    if !catalogue.projects.iter().any(|project| project.id == project_id) {
        return Err("The project is not present in the authoritative catalogue".into());
    }
    let directory = settings.project_paths.get(&project_id).cloned();
    reconcile_project_manifests(&app, &settings, &catalogue, [(project_id, directory)])
}

fn resolve_known_project_directory(
    settings: &DeviceSettings,
    project_id: &str,
) -> Result<PathBuf, String> {
    let configured = settings
        .project_paths
        .get(project_id)
        .ok_or("This project is not associated with a local folder on this device")?;
    fs_safe::resolve_project_directory(
        Path::new(&settings.working_projects_path),
        Path::new(configured),
        false,
    )
}

fn project_asset_category(relative_path: &Path) -> String {
    let lower_path = relative_path.to_string_lossy().replace('\\', "/").to_lowercase();
    let extension = relative_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if lower_path
        .split('/')
        .any(|part| part.contains("auto-save") || part.contains("autosave"))
    {
        return "premiere-auto-save".into();
    }
    if extension == "prproj" {
        return "premiere-project".into();
    }
    if lower_path
        .split('/')
        .any(|part| part == "export" || part == "exports")
    {
        return "exports".into();
    }
    if matches!(
        extension.as_str(),
        "mp4" | "mov" | "mxf" | "mts" | "m2ts" | "avi" | "r3d" | "braw" | "m4v"
    ) {
        return "video".into();
    }
    if matches!(
        extension.as_str(),
        "wav" | "bwf" | "aif" | "aiff" | "mp3" | "m4a" | "aac" | "flac" | "ogg"
    ) {
        return "audio-music".into();
    }
    if matches!(
        extension.as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "tif" | "tiff" | "bmp" | "webp" | "psd" | "ai" | "svg"
    ) {
        return "images-graphics".into();
    }
    if matches!(
        extension.as_str(),
        "txt" | "md" | "pdf" | "doc" | "docx" | "rtf" | "csv" | "xlsx"
    ) {
        return "notes-documentation".into();
    }
    "other".into()
}

fn inventory_project_directory(
    project_id: &str,
    project_name: &str,
    directory: &Path,
) -> Result<ProjectInventory, String> {
    inventory_project_directory_with_limits(
        project_id,
        project_name,
        directory,
        100_000,
        64,
    )
}

fn inventory_project_directory_with_limits(
    project_id: &str,
    project_name: &str,
    directory: &Path,
    max_entries: usize,
    max_depth: usize,
) -> Result<ProjectInventory, String> {
    let mut items = Vec::new();
    for entry in WalkDir::new(directory).follow_links(false).into_iter() {
        let entry = entry.map_err(|error| format!("Project inventory failed: {error}"))?;
        if entry.depth() > max_depth {
            return Err(format!(
                "Project inventory stopped because the folder hierarchy exceeds {max_depth} levels"
            ));
        }
        if !entry.file_type().is_file() {
            continue;
        }
        if items.len() >= max_entries {
            return Err(format!(
                "Project inventory stopped after {max_entries} files; narrow the project folder before scanning again"
            ));
        }
        let path = entry.path();
        let relative = path
            .strip_prefix(directory)
            .map_err(|_| "Project inventory escaped its authorized directory")?;
        let metadata = entry
            .metadata()
            .map_err(|error| format!("Cannot inspect {}: {error}", relative.display()))?;
        items.push(ProjectAsset {
            relative_path: relative.to_string_lossy().replace('\\', "/"),
            name: entry.file_name().to_string_lossy().into_owned(),
            category: project_asset_category(relative),
            size_bytes: metadata.len(),
            modified_at: metadata
                .modified()
                .ok()
                .map(|value| DateTime::<Utc>::from(value).to_rfc3339()),
        });
    }
    items.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let normalized_name = project_name.trim().to_lowercase();
    let main_premiere_project = items
        .iter()
        .filter(|item| item.category == "premiere-project")
        .min_by_key(|item| {
            let stem_matches = Path::new(&item.name)
                .file_stem()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.to_lowercase() == normalized_name);
            (
                !stem_matches,
                item.relative_path.matches('/').count(),
                item.relative_path.clone(),
            )
        })
        .map(|item| item.relative_path.clone());
    let size_bytes = items.iter().try_fold(0_u64, |total, item| {
        total.checked_add(item.size_bytes).ok_or_else(|| {
            "Project inventory size exceeds the supported range".to_string()
        })
    })?;
    Ok(ProjectInventory {
        project_id: project_id.into(),
        root_path: directory.to_string_lossy().into_owned(),
        item_count: items.len(),
        size_bytes,
        main_premiere_project,
        scanned_at: Utc::now().to_rfc3339(),
        items,
    })
}

#[tauri::command]
async fn inventory_project(app: AppHandle, project_id: String) -> Result<ProjectInventory, String> {
    let settings = load_settings_file(&app)?;
    let catalogue = load_catalogue_file(&settings)?;
    let project = catalogue
        .projects
        .iter()
        .find(|project| project.id == project_id)
        .ok_or("Unknown project")?;
    let project_name = project.name.clone();
    let directory = resolve_known_project_directory(&settings, &project_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        inventory_project_directory(&project_id, &project_name, &directory)
    })
    .await
    .map_err(|error| format!("Project inventory task failed: {error}"))?
}

#[tauri::command]
fn open_project_folder(app: AppHandle, project_id: String) -> Result<(), String> {
    let settings = load_settings_file(&app)?;
    let directory = resolve_known_project_directory(&settings, &project_id)?;
    #[cfg(target_os = "windows")]
    Command::new("explorer.exe")
        .arg(&directory)
        .spawn()
        .map_err(|error| format!("Could not open the project folder: {error}"))?;
    #[cfg(not(target_os = "windows"))]
    return Err("Open project folder is available only in the Windows build".into());
    Ok(())
}

#[tauri::command]
fn reveal_project_asset(
    app: AppHandle,
    project_id: String,
    relative_path: String,
) -> Result<(), String> {
    let settings = load_settings_file(&app)?;
    let directory = resolve_known_project_directory(&settings, &project_id)?;
    fs_safe::validate_relative(&relative_path)?;
    let path = fs_safe::resolve_existing(&directory, &relative_path)?;
    if !path.is_file() {
        return Err("The selected project asset is not available locally".into());
    }
    #[cfg(target_os = "windows")]
    reveal_path_in_explorer(&path)?;
    #[cfg(not(target_os = "windows"))]
    return Err("Reveal project asset is available only in the Windows build".into());
    Ok(())
}
fn publish_discovery_status(app: &AppHandle, state: &NativeState, status: &DiscoveryStatus) {
    *state.discovery_status.lock().unwrap() = status.clone();
    if let Ok(path) = discovery_status_path(app) {
        let _ = write_json(&path, status);
    }
    let _ = app.emit("discovery-progress", status);
}

fn fail_discovery_scan(
    app: &AppHandle,
    state: &NativeState,
    status: &mut DiscoveryStatus,
    error: String,
) -> String {
    status.running = false;
    status.errors += 1;
    publish_discovery_status(app, state, status);
    diagnostic_log(app, "ERROR", "library-scan", &error);
    error
}

#[tauri::command]
fn get_discovery_status(
    app: AppHandle,
    state: State<'_, NativeState>,
) -> Result<DiscoveryStatus, String> {
    let current = state.discovery_status.lock().unwrap().clone();
    if current.running || current.last_completed_at.is_some() {
        return Ok(current);
    }
    let path = discovery_status_path(&app)?;
    if !path.exists() {
        return Ok(current);
    }
    let mut persisted: DiscoveryStatus = read_json(&path)?;
    // A process cannot resume an in-memory walk. A new scan safely starts again.
    persisted.running = false;
    *state.discovery_status.lock().unwrap() = persisted.clone();
    Ok(persisted)
}

#[tauri::command]
fn log_ui_error(app: AppHandle, message: String) {
    diagnostic_log(&app, "ERROR", "ui", &message);
}

#[tauri::command]
async fn rescan(
    app: AppHandle,
    state: State<'_, NativeState>,
    full: Option<bool>,
) -> Result<RescanResult, String> {
    let settings = load_settings_file(&app)?;
    let master = PathBuf::from(&settings.master_bin_path);
    let full = full.unwrap_or(false);
    let _discovery_guard = state
        .discovery_write
        .try_lock()
        .map_err(|_| "A library scan is already running".to_string())?;
    let started_at = Utc::now().to_rfc3339();
    let mut status = DiscoveryStatus {
        running: true,
        full,
        started_at: Some(started_at),
        ..get_discovery_status(app.clone(), state.clone())?
    };
    status.running = true;
    status.full = full;
    status.files_processed = 0;
    status.total_files = 0;
    status.registered = 0;
    status.changed = 0;
    status.pending = 0;
    status.errors = 0;
    publish_discovery_status(&app, &state, &status);
    let catalogue = load_catalogue_file(&settings)
        .map_err(|error| fail_discovery_scan(&app, &state, &mut status, error))?;
    let known: HashMap<_, _> = catalogue
        .media
        .iter()
        .map(|item| (item.relative_path.clone(), item.clone()))
        .collect();
    let discovery_path = settings_path(&app)?
        .parent()
        .unwrap()
        .join("discovery-observations.json");
    let mut observations = discovery::load(&discovery_path)
        .map_err(|error| fail_discovery_scan(&app, &state, &mut status, error))?;
    let now = Utc::now().timestamp();
    let mut events = Vec::new();
    let mut result = RescanResult {
        full,
        ..Default::default()
    };
    let batch_timestamp = Local::now();
    let mut entries = Vec::new();
    for entry in WalkDir::new(&master).follow_links(false) {
        match entry {
            Ok(entry)
                if entry.file_type().is_file()
                    && !ignored(entry.path())
                    && supported(entry.path()) =>
            {
                entries.push(entry)
            }
            Ok(_) => {}
            Err(_) => result.errors += 1,
        }
    }
    status.total_files = entries.len();
    status.errors = result.errors;
    publish_discovery_status(&app, &state, &status);
    for entry in entries {
        let path = entry.path();
        result.scanned += 1;
        status.files_processed = result.scanned;
        let relative = path
            .strip_prefix(&master)
            .map_err(|_| "Path escaped Master Bin")?
            .to_string_lossy()
            .replace('\\', "/");
        let snapshot = match fs_safe::snapshot(path) {
            Ok(snapshot) => snapshot,
            Err(_) => {
                result.errors += 1;
                status.errors = result.errors;
                continue;
            }
        };
        if let Some(item) = known.get(&relative) {
            let evidence = item.integrity_by_device.get(&settings.device_id);
            let verified_snapshot = evidence.is_some_and(|evidence| {
                evidence.observed_size == snapshot.length
                    && evidence.observed_modified_millis
                        == snapshot.modified_millis.map(|value| value as u64)
            });
            if !full && verified_snapshot {
                observations.verified(&relative);
                continue;
            }
            if !verified_snapshot
                && discovery::Decision::Wait == observations.observe(&relative, &snapshot, now)
            {
                result.pending += 1;
                status.pending = result.pending;
                continue;
            }
        } else if discovery::Decision::Wait == observations.observe(&relative, &snapshot, now) {
            result.pending += 1;
            status.pending = result.pending;
            continue;
        }
        let hash = match sha256(path) {
            Ok(hash) => hash,
            Err(_) => {
                result.errors += 1;
                status.errors = result.errors;
                continue;
            }
        };
        if fs_safe::snapshot(path).ok().as_ref() != Some(&snapshot) {
            result.pending += 1;
            status.pending = result.pending;
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
                match destination_date(&settings, path, &None, &batch_timestamp) {
                    Ok(value) => value,
                    Err(_) => {
                        result.errors += 1;
                        status.errors = result.errors;
                        continue;
                    }
                };
            let item = match new_media(
                path,
                relative.clone(),
                None,
                hash,
                capture,
                provenance,
                true,
            ) {
                Ok(item) => item,
                Err(_) => {
                    result.errors += 1;
                    status.errors = result.errors;
                    continue;
                }
            };
            events.push(CatalogueEvent::MediaRegistered { media: item });
        }
        observations.verified(&relative);
        if result.scanned % 25 == 0 {
            publish_discovery_status(&app, &state, &status);
        }
    }
    discovery::save(&discovery_path, &observations)
        .map_err(|error| fail_discovery_scan(&app, &state, &mut status, error))?;
    let _guard = state.catalogue_write.lock().unwrap();
    let current = load_catalogue_file(&settings)
        .map_err(|error| fail_discovery_scan(&app, &state, &mut status, error))?;
    events.retain(|event| match event {
        CatalogueEvent::MediaRegistered { media } => !current
            .media
            .iter()
            .any(|item| item.id == media.id || item.relative_path == media.relative_path),
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
    let changed_catalogue = !events.is_empty();
    append_events(&settings, events)
        .map_err(|error| fail_discovery_scan(&app, &state, &mut status, error))?;
    result.completed_at = Utc::now().to_rfc3339();
    status.running = false;
    status.registered = result.registered;
    status.changed = result.changed;
    status.pending = result.pending;
    status.errors = result.errors;
    status.last_completed_at = Some(result.completed_at.clone());
    publish_discovery_status(&app, &state, &status);
    diagnostic_log(
        &app,
        "INFO",
        "library-scan",
        &format!(
            "mode={} scanned={} registered={} changed={} pending={} errors={}",
            if full { "full" } else { "quick" },
            result.scanned,
            result.registered,
            result.changed,
            result.pending,
            result.errors
        ),
    );
    if changed_catalogue {
        let _ = app.emit("catalogue-changed", ());
    }
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
            if let Ok(output) = media_tool_command("ffprobe")
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
            let _ = media_tool_command("ffmpeg")
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
    let relative = fs_safe::validate_relative(&item.relative_path)?;
    let master = Path::new(&settings.master_bin_path)
        .canonicalize()
        .map_err(|error| format!("Cannot resolve authorized Master Bin: {error}"))?;
    if !master.join(relative).is_file() {
        return Err("This recording is not available locally yet".into());
    }
    let path = fs_safe::resolve_existing(&master, &item.relative_path)?;
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

#[cfg(target_os = "windows")]
fn reveal_path_in_explorer(path: &Path) -> Result<(), String> {
    use std::{ffi::c_void, iter::once, os::windows::ffi::OsStrExt, ptr};

    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(reserved: *mut c_void, coinit: u32) -> i32;
        fn CoUninitialize();
    }
    #[link(name = "shell32")]
    extern "system" {
        fn ILCreateFromPathW(path: *const u16) -> *mut c_void;
        fn ILFree(pidl: *mut c_void);
        fn SHOpenFolderAndSelectItems(
            folder: *const c_void,
            item_count: u32,
            items: *const *const c_void,
            flags: u32,
        ) -> i32;
    }

    const COINIT_APARTMENTTHREADED: u32 = 0x2;
    const COINIT_DISABLE_OLE1DDE: u32 = 0x4;
    const RPC_E_CHANGED_MODE: i32 = 0x8001_0106_u32 as i32;

    let wide_path = path.as_os_str().encode_wide().chain(once(0)).collect::<Vec<_>>();
    unsafe {
        let initialized = CoInitializeEx(
            ptr::null_mut(),
            COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE,
        );
        if initialized < 0 && initialized != RPC_E_CHANGED_MODE {
            return Err(format!(
                "Windows Explorer initialization failed (HRESULT 0x{:08X})",
                initialized as u32
            ));
        }

        let item = ILCreateFromPathW(wide_path.as_ptr());
        if item.is_null() {
            if initialized >= 0 {
                CoUninitialize();
            }
            return Err("Windows Explorer could not resolve this recording".into());
        }

        // With item_count zero, the Shell opens the parent and selects this item.
        // This avoids explorer.exe command-line quoting ambiguity for commas,
        // whitespace and Unicode names and lets an existing Explorer window be reused.
        let selected = SHOpenFolderAndSelectItems(item, 0, ptr::null(), 0);
        ILFree(item);
        if initialized >= 0 {
            CoUninitialize();
        }
        if selected < 0 {
            return Err(format!(
                "Windows Explorer could not select this recording (HRESULT 0x{:08X})",
                selected as u32
            ));
        }
    }
    Ok(())
}

#[tauri::command]
fn reveal_media_in_explorer(app: AppHandle, media_id: String) -> Result<(), String> {
    let result: Result<(), String> = (|| {
        let settings = load_settings_file(&app)?;
        let path = resolve_catalogue_media_path(&settings, &media_id)?;
        #[cfg(target_os = "windows")]
        reveal_path_in_explorer(&path)?;
        #[cfg(not(target_os = "windows"))]
        return Err("Reveal in Explorer is available only in the Windows build".into());
        Ok(())
    })();
    if let Err(error) = &result {
        diagnostic_log(&app, "ERROR", "explorer-reveal", error);
    }
    result
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
    let output = media_tool_command(command).arg("-version").output().ok()?;
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

fn support_url(destination: &str) -> Option<&'static str> {
    match destination {
        "github" => Some("https://github.com/Dubcodes/kairix-pimp"),
        "releases" => Some("https://github.com/Dubcodes/kairix-pimp/releases"),
        "coffee" => Some("https://buymeacoffee.com/dubcodes"),
        _ => None,
    }
}

#[tauri::command]
fn open_support_link(app: AppHandle, destination: String) -> Result<(), String> {
    let url = support_url(&destination).ok_or("Unknown support destination")?;
    app.opener()
        .open_url(url, None::<String>)
        .map_err(|error| format!("Could not open the link: {error}"))
}

#[tauri::command]
fn open_logs_folder(app: AppHandle) -> Result<(), String> {
    let path = diagnostic_log_path(&app)?;
    let folder = path.parent().ok_or("Invalid logs folder")?;
    #[cfg(target_os = "windows")]
    Command::new("explorer.exe")
        .arg(folder)
        .spawn()
        .map_err(|error| format!("Could not open the logs folder: {error}"))?;
    #[cfg(not(target_os = "windows"))]
    return Err("Open logs folder is available only in the Windows build".into());
    Ok(())
}

#[tauri::command]
fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        build_commit: option_env!("PIMP_BUILD_COMMIT")
            .unwrap_or("development")
            .to_string(),
        edition: "Kairix PIMP Light".into(),
        release_channel: "Beta".into(),
    }
}

fn updater_configuration_for_key(public_key: Option<&str>) -> UpdaterConfiguration {
    const ENDPOINT: &str =
        "https://github.com/Dubcodes/kairix-pimp/releases/download/beta-channel/latest.json";
    let key = public_key.map(str::trim).filter(|key| !key.is_empty());
    let configured = key.is_some_and(|key| {
        BASE64
            .decode(key)
            .ok()
            .and_then(|decoded| String::from_utf8(decoded).ok())
            .is_some_and(|decoded| minisign_verify::PublicKey::decode(&decoded).is_ok())
    });
    UpdaterConfiguration {
        configured,
        channel: "Beta".into(),
        endpoint: ENDPOINT.into(),
        reason: (!configured).then(|| match key {
            Some(_) => "The embedded updater public key is invalid. Use the reviewed NSIS installer from GitHub Releases.".into(),
            None => "Signed updates are not configured in this build. Use the reviewed NSIS installer from GitHub Releases.".into(),
        }),
    }
}

#[tauri::command]
fn updater_configuration() -> UpdaterConfiguration {
    updater_configuration_for_key(option_env!("PIMP_UPDATER_PUBLIC_KEY"))
}

#[tauri::command]
fn begin_update_installation(app: AppHandle, state: State<'_, NativeState>) -> Result<(), String> {
    let mut installing = state.update_installing.lock().unwrap();
    let jobs = snapshot_jobs(&app, &state)?;
    begin_update_gate(&mut installing, &jobs)
}

#[tauri::command]
fn end_update_installation(state: State<'_, NativeState>) {
    end_update_gate(&state.update_installing);
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

fn hover_preview_root(settings: &DeviceSettings) -> Result<PathBuf, String> {
    let root = PathBuf::from(&settings.thumbnail_cache_path).join("hover-previews");
    fs::create_dir_all(&root)
        .map_err(|error| format!("Cannot create hover preview cache: {error}"))?;
    root.canonicalize()
        .map_err(|error| format!("Cannot resolve hover preview cache: {error}"))
}

fn hover_preview_fingerprint(item: &MediaItem, snapshot: &fs_safe::FileSnapshot) -> String {
    let mut digest = Sha256::new();
    digest.update(b"pimp-hover-preview-v1\0");
    digest.update(item.id.as_bytes());
    digest.update(b"\0");
    digest.update(item.sha256.as_bytes());
    digest.update(b"\0");
    digest.update(snapshot.length.to_le_bytes());
    digest.update(snapshot.modified_millis.unwrap_or_default().to_le_bytes());
    format!("{:x}", digest.finalize())
}

fn hover_preview_frame_times(duration_seconds: f64) -> Result<Vec<f64>, String> {
    if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
        return Err("Hover preview requires a known positive duration".into());
    }
    let count = ((duration_seconds * 2.0).floor() as usize).clamp(1, 16);
    if count == 1 {
        return Ok(vec![duration_seconds / 2.0]);
    }
    Ok((0..count)
        .map(|index| duration_seconds * index as f64 / (count - 1) as f64)
        .collect())
}

fn read_hover_preview(directory: &Path, expected_fingerprint: &str) -> Result<HoverPreview, String> {
    let manifest_path = directory.join("manifest.json");
    let manifest: HoverPreviewManifest = serde_json::from_slice(
        &fs::read(&manifest_path)
            .map_err(|error| format!("Cannot read hover preview manifest: {error}"))?,
    )
    .map_err(|error| format!("Invalid hover preview manifest: {error}"))?;
    if manifest.version != 1 || manifest.fingerprint != expected_fingerprint {
        return Err("Hover preview cache fingerprint does not match the source".into());
    }
    if manifest.frame_files.len() != manifest.frame_times.len() || manifest.frame_files.is_empty() {
        return Err("Hover preview manifest has an invalid frame list".into());
    }
    let canonical_directory = directory
        .canonicalize()
        .map_err(|error| format!("Cannot resolve hover preview directory: {error}"))?;
    let mut frames = Vec::with_capacity(manifest.frame_files.len());
    let mut total_bytes = 0_u64;
    for reference in &manifest.frame_files {
        if reference.contains('/') || reference.contains('\\') || !reference.ends_with(".jpg") {
            return Err("Hover preview manifest contains an unsafe frame reference".into());
        }
        let path = canonical_directory.join(reference);
        let path = path
            .canonicalize()
            .map_err(|error| format!("Cannot resolve hover preview frame: {error}"))?;
        if !path.starts_with(&canonical_directory) || !path.is_file() {
            return Err("Hover preview frame resolved outside its cache entry".into());
        }
        let bytes = fs::read(path).map_err(|error| format!("Cannot read hover preview frame: {error}"))?;
        total_bytes = total_bytes.saturating_add(bytes.len() as u64);
        if total_bytes > 32 * 1024 * 1024 {
            return Err("Hover preview exceeds the 32 MB safety limit".into());
        }
        frames.push(format!("data:image/jpeg;base64,{}", BASE64.encode(bytes)));
    }
    Ok(HoverPreview {
        media_id: manifest.media_id,
        duration_seconds: manifest.duration_seconds,
        frame_times: manifest.frame_times,
        frames,
    })
}

fn preview_cache_usage(root: &Path) -> u64 {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.metadata().ok().map(|metadata| metadata.len()))
        .sum()
}

fn evict_hover_preview_cache(root: &Path, budget: u64, keep: &Path) -> Result<(), String> {
    let mut entries = fs::read_dir(root)
        .map_err(|error| format!("Cannot inspect hover preview cache: {error}"))?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let path = entry.path();
            let manifest = path.join("manifest.json");
            let generated = fs::metadata(&manifest).ok()?.modified().ok()?;
            Some((generated, path))
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.0);
    let mut usage = preview_cache_usage(root);
    for (_, path) in entries {
        if usage <= budget {
            break;
        }
        if path == keep {
            continue;
        }
        let entry_size = preview_cache_usage(&path);
        fs::remove_dir_all(&path)
            .map_err(|error| format!("Cannot evict hover preview cache entry: {error}"))?;
        usage = usage.saturating_sub(entry_size);
    }
    Ok(())
}

fn generate_hover_preview(
    settings: &DeviceSettings,
    item: &MediaItem,
    source: &Path,
) -> Result<HoverPreview, String> {
    let duration_seconds = item
        .technical
        .duration_seconds
        .ok_or("Hover preview requires media duration metadata")?;
    let frame_times = hover_preview_frame_times(duration_seconds)?;
    let before = fs_safe::snapshot(source)?;
    let fingerprint = hover_preview_fingerprint(item, &before);
    let root = hover_preview_root(settings)?;
    let final_directory = root.join(&fingerprint);
    if final_directory.is_dir() {
        if let Ok(preview) = read_hover_preview(&final_directory, &fingerprint) {
            return Ok(preview);
        }
        fs::remove_dir_all(&final_directory)
            .map_err(|error| format!("Cannot replace invalid hover preview cache: {error}"))?;
    }

    let temporary = root.join(format!(".generating-{}", Uuid::new_v4()));
    fs::create_dir(&temporary)
        .map_err(|error| format!("Cannot stage hover preview: {error}"))?;
    let output_pattern = temporary.join("frame-%03d.jpg");
    let filter = format!(
        "fps={}/{duration_seconds},scale=320:-2:force_original_aspect_ratio=decrease",
        frame_times.len()
    );
    let output = media_tool_command("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(source)
        .args(["-an", "-vf", &filter, "-frames:v"])
        .arg(frame_times.len().to_string())
        .args(["-q:v", "7"])
        .arg(&output_pattern)
        .output()
        .map_err(|error| format!("FFmpeg is unavailable for hover previews: {error}"));
    let output = match output {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            let _ = fs::remove_dir_all(&temporary);
            return Err(format!(
                "FFmpeg could not generate a hover preview: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&temporary);
            return Err(error);
        }
    };
    drop(output);
    if fs_safe::snapshot(source)? != before {
        let _ = fs::remove_dir_all(&temporary);
        return Err("Source changed while generating its hover preview".into());
    }
    let mut frame_files = fs::read_dir(&temporary)
        .map_err(|error| format!("Cannot inspect generated hover preview: {error}"))?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|value| value == "jpg"))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    frame_files.sort();
    if frame_files.len() != frame_times.len() {
        let _ = fs::remove_dir_all(&temporary);
        return Err("FFmpeg produced an incomplete hover preview".into());
    }
    let manifest = HoverPreviewManifest {
        version: 1,
        media_id: item.id.clone(),
        fingerprint: fingerprint.clone(),
        duration_seconds,
        frame_times,
        frame_files,
        generated_at: Utc::now().to_rfc3339(),
    };
    fs::write(
        temporary.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("Cannot write hover preview manifest: {error}"))?;
    fs::rename(&temporary, &final_directory)
        .map_err(|error| format!("Cannot commit hover preview cache entry: {error}"))?;
    evict_hover_preview_cache(&root, settings.preview_cache_limit_bytes, &final_directory)?;
    read_hover_preview(&final_directory, &fingerprint)
}

#[tauri::command]
async fn load_hover_preview(app: AppHandle, media_id: String) -> Result<HoverPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<NativeState>();
        let _guard = state.preview_generation.lock().unwrap();
        let settings = load_settings_file(&app)?;
        if !settings.hover_previews_enabled {
            return Err("Hover previews are disabled in Settings".into());
        }
        let catalogue = load_catalogue_file(&settings)?;
        let item = catalogue
            .media
            .iter()
            .find(|item| item.id == media_id)
            .ok_or("Unknown media item")?;
        if !matches!(
            item.file_type.to_ascii_lowercase().as_str(),
            "mp4" | "mov" | "m4v" | "mxf" | "mts" | "m2ts" | "avi"
        ) {
            return Err("Hover previews are available only for supported video files".into());
        }
        let source = resolve_catalogue_media_path(&settings, &media_id)?;
        generate_hover_preview(&settings, item, &source)
    })
    .await
    .map_err(|error| format!("Hover preview task failed: {error}"))?
}

#[tauri::command]
fn hover_preview_cache_usage(app: AppHandle) -> Result<u64, String> {
    let settings = load_settings_file(&app)?;
    Ok(preview_cache_usage(&hover_preview_root(&settings)?))
}

#[tauri::command]
fn clear_hover_preview_cache(app: AppHandle) -> Result<u64, String> {
    let settings = load_settings_file(&app)?;
    let root = hover_preview_root(&settings)?;
    for entry in fs::read_dir(&root)
        .map_err(|error| format!("Cannot inspect hover preview cache: {error}"))?
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_dir())
            && (path.join("manifest.json").is_file()
                || entry.file_name().to_string_lossy().starts_with(".generating-"))
        {
            fs::remove_dir_all(path)
                .map_err(|error| format!("Cannot clear hover preview cache: {error}"))?;
        }
    }
    Ok(preview_cache_usage(&root))
}

fn should_start_hidden(arguments: &[String], tray_available: bool) -> bool {
    tray_available && arguments.iter().any(|argument| argument == "--minimized")
}

fn restore_main_window(app: &AppHandle, source: &str) {
    let Some(window) = app.get_webview_window("main") else {
        diagnostic_log(
            app,
            "ERROR",
            "window-restore",
            &format!("{source}: main window is unavailable"),
        );
        return;
    };
    let mut failures = Vec::new();
    if let Err(error) = window.show() {
        failures.push(format!("show failed: {error}"));
    }
    if let Err(error) = window.unminimize() {
        failures.push(format!("unminimize failed: {error}"));
    }
    if let Err(error) = window.set_focus() {
        failures.push(format!("focus request failed: {error}"));
        let _ = window.request_user_attention(Some(tauri::UserAttentionType::Informational));
    }
    if !failures.is_empty() {
        diagnostic_log(
            app,
            "WARN",
            "window-restore",
            &format!("{source}: {}", failures.join("; ")),
        );
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            restore_main_window(app, "second-instance launch");
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args(["--minimized"])
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(
            tauri_plugin_updater::Builder::new()
                .pubkey(option_env!("PIMP_UPDATER_PUBLIC_KEY").unwrap_or(""))
                .build(),
        )
        .manage(NativeState::default())
        .setup(|app| {
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};

            let show = MenuItem::with_id(app, "show", "Show PIMP", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit PIMP", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = TrayIconBuilder::with_id("pimp-main")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("Kairix PIMP Light")
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => restore_main_window(app, "tray menu"),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            ..
                        } |
                        TrayIconEvent::DoubleClick {
                            button: MouseButton::Left,
                            ..
                        }
                    ) {
                        restore_main_window(tray.app_handle(), "tray icon");
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            match tray.build(app) {
                Ok(_) => *app.state::<NativeState>().tray_available.lock().unwrap() = true,
                Err(error) => diagnostic_log(
                    app.handle(),
                    "WARN",
                    "tray",
                    &format!("Tray icon is unavailable: {error}"),
                ),
            }
            if let Some(window) = app.get_webview_window("main") {
                let close_window = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        let app = close_window.app_handle();
                        let tray_available = *app
                            .state::<NativeState>()
                            .tray_available
                            .lock()
                            .unwrap();
                        let close_to_tray = load_settings_file(&app)
                            .map(|settings| settings.close_to_tray)
                            .unwrap_or(false);
                        if tray_available && close_to_tray {
                            api.prevent_close();
                            let _ = close_window.hide();
                        }
                    }
                });
                let arguments = std::env::args().collect::<Vec<_>>();
                let tray_available = *app
                    .state::<NativeState>()
                    .tray_available
                    .lock()
                    .unwrap();
                if should_start_hidden(&arguments, tray_available) {
                    let _ = window.hide();
                } else {
                    if arguments.iter().any(|argument| argument == "--minimized") {
                        diagnostic_log(
                            app.handle(),
                            "WARN",
                            "autostart",
                            "Minimized startup was requested but no tray is available; showing the main window",
                        );
                    }
                    restore_main_window(app.handle(), "initial launch");
                }
            }
            Ok(())
        })
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
            update_project,
            create_project,
            retry_project_synchronization,
            inventory_project,
            open_project_folder,
            reveal_project_asset,
            rescan,
            get_discovery_status,
            log_ui_error,
            verify_media_integrity,
            enrich_media,
            media_tools_status,
            app_info,
            updater_configuration,
            begin_update_installation,
            end_update_installation,
            open_support_link,
            open_logs_folder,
            resolve_media_path,
            reveal_media_in_explorer,
            open_media_folder,
            load_thumbnail,
            load_hover_preview,
            hover_preview_cache_usage,
            clear_hover_preview_cache
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
        assert!(jobs[0].files[0]
            .error
            .as_deref()
            .unwrap()
            .contains("stopped"));
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
        assert_eq!(
            fs::read(root.join("master/20261024/clip.mp4")).unwrap(),
            b"verified-media"
        );
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
        assert_eq!(
            file.metadata_error.as_deref(),
            Some("simulated append failure")
        );

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
        fs::write(
            card.join("General").join("Sony").join("A001.XML"),
            b"sidecar",
        )
        .unwrap();
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
    fn independent_selected_roots_keep_distinct_destination_hierarchies() {
        let root = temporary("inventory-independent-roots");
        let card_a = root.join("card-a");
        let card_b = root.join("card-b");
        for card in [&card_a, &card_b] {
            fs::create_dir_all(card.join("XDROOT").join("Clip")).unwrap();
            fs::write(card.join("clip.mp4"), b"standalone").unwrap();
            fs::write(
                card.join("XDROOT").join("Clip").join("clip.mxf"),
                b"package",
            )
            .unwrap();
        }

        let inventory = build_source_inventory(&[
            card_a.to_string_lossy().into(),
            card_b.to_string_lossy().into(),
        ]);
        let relative_paths = inventory
            .files
            .iter()
            .map(|file| file.source_relative_path.as_str())
            .collect::<HashSet<_>>();
        assert_eq!(relative_paths.len(), 4);
        assert!(relative_paths.contains("card-a/clip.mp4"));
        assert!(relative_paths.contains("card-b/clip.mp4"));
        assert!(relative_paths.contains("card-a/XDROOT/Clip/clip.mxf"));
        assert!(relative_paths.contains("card-b/XDROOT/Clip/clip.mxf"));

        let settings = DeviceSettings::default();
        let batch = Local.with_ymd_and_hms(2026, 10, 26, 12, 0, 0).unwrap();
        let planned = inventory
            .files
            .iter()
            .map(|file| {
                let filename = Path::new(&file.source_path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy();
                planned_relative_path(&settings, Path::new(""), file, &None, &batch, &filename)
                    .unwrap()
            })
            .collect::<HashSet<_>>();
        assert_eq!(planned.len(), 4);
        assert!(planned.contains("20261026/card-a/clip.mp4"));
        assert!(planned.contains("20261026/card-b/clip.mp4"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn camera_package_uses_earliest_member_date_without_splitting_sidecars() {
        let root = temporary("inventory-package-dates");
        let package = root.join("card").join("XDROOT");
        let clip = package.join("Clip").join("A001.MXF");
        let sidecar = package.join("General").join("Sony").join("A001.XML");
        let standalone = root.join("standalone.mp4");
        fs::create_dir_all(clip.parent().unwrap()).unwrap();
        fs::create_dir_all(sidecar.parent().unwrap()).unwrap();
        fs::write(&clip, b"mxf").unwrap();
        fs::write(&sidecar, b"xml").unwrap();
        fs::write(&standalone, b"mp4").unwrap();
        let day_20 = Local.with_ymd_and_hms(2026, 10, 20, 8, 0, 0).unwrap();
        let day_21 = Local.with_ymd_and_hms(2026, 10, 21, 9, 0, 0).unwrap();
        let day_22 = Local.with_ymd_and_hms(2026, 10, 22, 10, 0, 0).unwrap();
        for (path, modified) in [(&clip, day_20), (&sidecar, day_21), (&standalone, day_22)] {
            File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(
                    std::time::SystemTime::UNIX_EPOCH
                        + std::time::Duration::from_secs(modified.timestamp() as u64),
                ))
                .unwrap();
        }

        let inventory = build_source_inventory(&[root.to_string_lossy().into()]);
        let mut settings = DeviceSettings::default();
        settings.ingest_date_organization = "original-file-dates".into();
        let batch = Local.with_ymd_and_hms(2026, 10, 26, 12, 0, 0).unwrap();
        let planned = inventory
            .files
            .iter()
            .map(|file| {
                let filename = Path::new(&file.source_path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy();
                (
                    filename.to_string(),
                    planned_relative_path(&settings, Path::new(""), file, &None, &batch, &filename)
                        .unwrap(),
                )
            })
            .collect::<HashMap<_, _>>();
        assert_eq!(planned["A001.MXF"], "20261020/card/XDROOT/Clip/A001.MXF");
        assert_eq!(
            planned["A001.XML"],
            "20261020/card/XDROOT/General/Sony/A001.XML"
        );
        assert_eq!(planned["standalone.mp4"], "20261022/standalone.mp4");
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
    fn large_folder_inventory_is_complete_and_deterministic() {
        let root = temporary("inventory-large");
        for directory in 0..8 {
            let folder = root.join(format!("card-{directory:02}"));
            fs::create_dir_all(&folder).unwrap();
            for file in 0..64 {
                fs::write(folder.join(format!("clip-{file:03}.MXF")), [file as u8]).unwrap();
            }
        }
        let first = build_source_inventory(&[root.to_string_lossy().into()]);
        let second = build_source_inventory(&[root.to_string_lossy().into()]);
        assert_eq!(first.copy_file_count, 512);
        assert_eq!(first.total_file_count, 512);
        assert_eq!(first.files, second.files);
        assert!(first.files.iter().all(|file| file.preserve_hierarchy));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ingest_errors_include_recovery_guidance_and_technical_detail() {
        let full = actionable_ingest_error("Interrupted staging write: no space left on device");
        assert!(full.contains("destination drive is full"));
        assert!(full.contains("no space left on device"));
        let unreadable = actionable_ingest_error("Cannot read source: Access is denied");
        assert!(unreadable.contains("Reconnect the card"));
        let mismatch = actionable_ingest_error("Independent SHA-256 verification failed");
        assert!(mismatch.contains("staged copy was rejected"));
        let collision = actionable_ingest_error("__race__");
        assert!(collision.contains("Nothing was overwritten"));
    }

    #[test]
    fn unresolved_inventory_issues_prevent_job_completion() {
        let mut job = ingest_job("copying", "completed");
        assert!(all_job_files_completed(&job));
        job.inventory_issues
            .push("Card folder could not be traversed".into());
        assert!(!all_job_files_completed(&job));
    }

    #[test]
    fn job_history_is_bounded_without_removing_active_work() {
        let mut jobs = Vec::new();
        for index in 0..125 {
            let mut job = ingest_job("completed", "completed");
            job.id = format!("completed-{index}");
            job.failures.clear();
            jobs.push(job);
        }
        let mut active = ingest_job("interrupted", "interrupted");
        active.id = "recoverable-active".into();
        jobs.insert(0, active);
        prune_terminal_jobs(&mut jobs);
        assert_eq!(jobs.len(), RETAINED_TERMINAL_JOBS + 1);
        assert_eq!(jobs[0].id, "recoverable-active");
        assert_eq!(jobs[1].id, "completed-25");
    }

    #[test]
    fn pruning_never_discards_failed_or_cancelled_jobs_with_pending_intent() {
        let mut jobs = Vec::new();
        for index in 0..220 {
            let state = if index % 2 == 0 {
                "failed"
            } else {
                "cancelled"
            };
            let mut job = ingest_job(state, state);
            job.id = format!("recoverable-{index}");
            job.files[0].metadata.tags = vec![format!("tag-{index}")];
            job.files[0].metadata.notes = format!("notes-{index}");
            job.files[0].pending_metadata = Some(job.files[0].metadata.clone());
            job.files[0].metadata_error = Some("Pending catalogue write".into());
            jobs.push(job);
        }
        for index in 0..125 {
            let mut job = ingest_job("completed", "completed");
            job.id = format!("finalized-{index}");
            job.failures.clear();
            jobs.push(job);
        }

        prune_terminal_jobs(&mut jobs);

        assert_eq!(jobs.len(), 220 + RETAINED_TERMINAL_JOBS);
        let recoverable = jobs
            .iter()
            .filter(|job| job.id.starts_with("recoverable-"))
            .collect::<Vec<_>>();
        assert_eq!(recoverable.len(), 220);
        assert!(recoverable.iter().all(|job| {
            !job.files[0].metadata.tags.is_empty()
                && !job.files[0].metadata.notes.is_empty()
                && job.files[0].pending_metadata.is_some()
                && job.files[0].metadata_error.is_some()
        }));
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
        assert!(
            preserve_device_identity(&restarted.device_id, &Uuid::new_v4().to_string()).is_err()
        );

        write_json(&path, &restarted).unwrap();
        fs::remove_file(&path).unwrap();
        let recovered = load_or_initialize_settings(&path).unwrap();
        assert_eq!(recovered.device_id, restarted.device_id);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn new_devices_default_to_recoverable_desktop_preferences() {
        let settings = DeviceSettings::default();
        assert!(settings.close_to_tray);
        assert!(!settings.start_with_windows);
        assert_eq!(settings.startup_page, "Library");
        assert_eq!(settings.theme, "graphite");
        assert_eq!(settings.density, "comfortable");
        assert!(!settings.show_attached_audio_separately);
        assert!(!settings.sidebar_collapsed);
        assert_eq!(settings.library_folder_sort, "newest");
        assert!(settings.library_folder_order.is_empty());
    }

    #[test]
    fn external_support_destinations_are_strictly_allowlisted() {
        assert_eq!(
            support_url("github"),
            Some("https://github.com/Dubcodes/kairix-pimp")
        );
        assert_eq!(
            support_url("coffee"),
            Some("https://buymeacoffee.com/dubcodes")
        );
        assert_eq!(support_url("https://example.invalid"), None);
    }

    #[test]
    fn updater_is_a_higher_beta_and_fails_closed_without_a_public_key() {
        assert_eq!(env!("CARGO_PKG_VERSION"), "0.1.1-2");
        let configuration = updater_configuration();
        assert_eq!(configuration.channel, "Beta");
        assert!(configuration.endpoint.starts_with("https://"));
        if option_env!("PIMP_UPDATER_PUBLIC_KEY").is_none() {
            assert!(!configuration.configured);
            assert!(configuration.reason.is_some());
        } else {
            assert!(
                configuration.configured,
                "release builds must reject an invalid embedded updater public key"
            );
        }
        assert!(!updater_configuration_for_key(Some("not-a-minisign-key")).configured);
        let encoded_public_key = BASE64.encode(
            "untrusted comment: minisign public key\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3\n",
        );
        assert!(updater_configuration_for_key(Some(&encoded_public_key)).configured);
    }

    #[test]
    fn update_gate_serializes_installation_with_ingest_lifecycle() {
        for state in ["queued", "copying", "verifying", "paused", "interrupted"] {
            let mut installing = false;
            assert!(begin_update_gate(&mut installing, &[ingest_job(state, state)]).is_err());
            assert!(!installing);
        }

        for state in ["completed", "failed", "cancelled"] {
            let mut installing = false;
            begin_update_gate(&mut installing, &[ingest_job(state, state)]).unwrap();
            assert!(installing);
            assert!(begin_update_gate(&mut installing, &[]).is_err());
            end_update_gate(&Mutex::new(installing));
        }

        let gate = Mutex::new(false);
        {
            let mut installing = gate.lock().unwrap();
            begin_update_gate(&mut installing, &[]).unwrap();
        }
        assert_eq!(
            *gate.lock().unwrap(),
            true,
            "new ingest commands observe the installation gate"
        );
        end_update_gate(&gate);
        assert!(!*gate.lock().unwrap());
    }

    #[test]
    fn updater_signature_verification_rejects_tampered_bytes() {
        use minisign_verify::{PublicKey, Signature};

        let public_key =
            PublicKey::from_base64("RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3")
                .unwrap();
        let signature = Signature::decode(
            "untrusted comment: signature from minisign secret key\nRUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\ntrusted comment: timestamp:1633700835\tfile:test\tprehashed\nwLMDjy9FLAuxZ3q4NlEvkgtyhrr0gtTu6KC4KBJdITbbOeAi1zBIYo0v4iTgt8jJpIidRJnp94ABQkJAgAooBQ==",
        )
        .unwrap();
        assert!(public_key.verify(b"test", &signature, false).is_ok());
        assert!(public_key.verify(b"tampered", &signature, false).is_err());
    }

    #[test]
    fn autostart_hides_only_when_the_tray_is_available() {
        let minimized = vec!["kairix-pimp.exe".into(), "--minimized".into()];
        assert!(should_start_hidden(&minimized, true));
        assert!(!should_start_hidden(&minimized, false));
        assert!(!should_start_hidden(&["kairix-pimp.exe".into()], true));
    }

    #[test]
    fn child_console_suppression_is_windows_release_only() {
        assert!(should_suppress_child_console(true, false));
        assert!(!should_suppress_child_console(true, true));
        assert!(!should_suppress_child_console(false, false));
    }

    #[test]
    fn known_media_resolution_preserves_spaces_commas_unicode_and_offline_errors() {
        let root = temporary("explorer-special-path");
        let master = root.join("master");
        let path = master
            .join("20261027")
            .join("DJI clip, kōrero 音声.WAV");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"disposable audio").unwrap();
        let media = new_media(
            &path,
            "20261027/DJI clip, kōrero 音声.WAV".into(),
            None,
            sha256(&path).unwrap(),
            None,
            "unknown".into(),
            false,
        )
        .unwrap();
        let media_id = media.id.clone();
        let mut settings = DeviceSettings::default();
        settings.master_bin_path = master.to_string_lossy().into();
        append_events(
            &settings,
            vec![CatalogueEvent::MediaRegistered { media }],
        )
        .unwrap();

        assert_eq!(
            resolve_catalogue_media_path(&settings, &media_id).unwrap(),
            path.canonicalize().unwrap()
        );
        fs::remove_file(&path).unwrap();
        assert!(resolve_catalogue_media_path(&settings, &media_id)
            .unwrap_err()
            .contains("not available locally"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_settings_without_a_date_mode_keep_the_previous_default() {
        let mut value = serde_json::to_value(DeviceSettings::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("ingestDateOrganization");
        value.as_object_mut().unwrap().remove("sidebarCollapsed");
        value.as_object_mut().unwrap().remove("libraryFolderSort");
        value.as_object_mut().unwrap().remove("libraryFolderOrder");
        let settings: DeviceSettings = serde_json::from_value(value).unwrap();
        assert_eq!(settings.ingest_date_organization, "original-file-dates");
        assert!(!settings.sidebar_collapsed);
        assert_eq!(settings.library_folder_sort, "newest");
        assert!(settings.library_folder_order.is_empty());
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

        let today_first =
            destination_date_from_modified(&settings, Some(first.clone()), &None, &batch)
                .unwrap()
                .0;
        let today_second =
            destination_date_from_modified(&settings, Some(second.clone()), &None, &batch)
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

        let recovery = recover_staging(&master, "device-a", Some(&master.join(".pimp"))).unwrap();
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

    #[test]
    fn compatible_project_manifest_updates_preserve_unknown_fields() {
        let root = temporary("manifest-unknown-fields");
        let path = root.join(".pimp-project.json");
        write_json(
            &path,
            &serde_json::json!({
                "schemaVersion": 2,
                "customTopLevel": {"keep": true},
                "project": {
                    "id": "project-a",
                    "name": "Old",
                    "customProjectField": "keep-me"
                },
                "media": []
            }),
        )
        .unwrap();
        let project = ProjectRecord {
            id: "project-a".into(),
            name: "Updated".into(),
            working_directory: ".".into(),
            media_ids: vec![],
            notes: "notes".into(),
            tags: vec!["tag".into()],
            created_at: "2026-10-27T00:00:00Z".into(),
        };
        write_project_manifest(&root, &project, &[]).unwrap();
        let value: serde_json::Value = read_json(&path).unwrap();
        assert_eq!(value["customTopLevel"]["keep"], true);
        assert_eq!(value["project"]["customProjectField"], "keep-me");
        assert_eq!(value["project"]["name"], "Updated");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn repeated_project_creation_reuses_pending_and_persisted_folder_identity() {
        let root = temporary("project-identity-retry");
        let projects_root = root.join("projects");
        let directory = projects_root.join("Project A");
        fs::create_dir_all(&directory).unwrap();
        let directory = directory.canonicalize().unwrap();
        let state_path = root.join("manifest-sync.json");
        let mut settings = DeviceSettings::default();
        settings.working_projects_path = projects_root.to_string_lossy().into_owned();
        persist_project_manifest_intents(
            &state_path,
            [("project-a".into(), Some(directory.to_string_lossy().into_owned()))],
        )
        .unwrap();

        assert_eq!(
            project_identity_for_directory(&state_path, &settings, &directory, None).unwrap(),
            Some("project-a".into())
        );
        settings
            .project_paths
            .insert("project-a".into(), directory.to_string_lossy().into_owned());
        assert_eq!(
            project_identity_for_directory(&state_path, &settings, &directory, None).unwrap(),
            Some("project-a".into())
        );

        let restarted: ProjectManifestSyncState = read_json(&state_path).unwrap();
        assert_eq!(restarted.pending["project-a"].project_id, "project-a");
        settings
            .project_paths
            .insert("project-b".into(), directory.to_string_lossy().into_owned());
        let conflict = project_identity_for_directory(
            &state_path,
            &settings,
            &directory,
            Some("project-c"),
        )
        .unwrap_err();
        assert!(conflict.contains("conflicting project identities"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_two_device_media_alias_in_manifest_reconciles_once() {
        let root = temporary("manifest-media-alias");
        let projects_root = root.join("projects");
        let directory = projects_root.join("Simple Vlog 057");
        fs::create_dir_all(&directory).unwrap();
        let source = root.join("clip.mp4");
        fs::write(&source, b"synthetic").unwrap();
        let mut first = new_media(
            &source,
            "20261028/clip.mp4".into(),
            None,
            "same-hash".into(),
            None,
            "unknown".into(),
            false,
        )
        .unwrap();
        first.id = "media-a".into();
        let mut second = first.clone();
        second.id = "media-b".into();
        let project = ProjectRecord {
            id: "project-a".into(),
            name: "Simple Vlog 057".into(),
            working_directory: ".".into(),
            media_ids: vec![],
            notes: String::new(),
            tags: vec![],
            created_at: "2026-10-28T00:00:00Z".into(),
        };
        let event = |device: &str, sequence: u64, event: CatalogueEvent| {
            catalogue::EventEnvelope {
                schema_version: catalogue::EVENT_SCHEMA_VERSION,
                event_id: format!("{device}-{sequence}"),
                device_id: device.into(),
                sequence,
                occurred_at: "2026-10-28T00:00:00Z".into(),
                observed: BTreeMap::new(),
                event,
            }
        };
        let catalogue = catalogue::rebuild(catalogue::EventRead {
            events: vec![
                event(
                    "device-a",
                    1,
                    CatalogueEvent::MediaRegistered { media: first },
                ),
                event(
                    "device-b",
                    1,
                    CatalogueEvent::MediaRegistered { media: second },
                ),
                event(
                    "device-b",
                    2,
                    CatalogueEvent::MediaProjectsAdded {
                        media_id: "media-b".into(),
                        values: vec!["project-a".into()],
                    },
                ),
                event(
                    "device-c",
                    1,
                    CatalogueEvent::ProjectUpserted {
                        project: project.clone(),
                    },
                ),
            ],
            ..Default::default()
        });
        assert_eq!(catalogue.media.len(), 1);
        let canonical_id = catalogue.media[0].id.clone();
        assert_eq!(catalogue.projects[0].media_ids, vec![canonical_id.clone()]);
        write_json(
            &directory.join(".pimp-project.json"),
            &serde_json::json!({
                "schemaVersion": 2,
                "customField": "preserved",
                "id": "project-a",
                "name": "Simple Vlog 057",
                "workingDirectory": ".",
                "mediaIds": ["media-b"],
                "media": [{
                    "id": "media-b",
                    "relativePath": "20261028/clip.mp4",
                    "sha256": "same-hash",
                    "tags": []
                }]
            }),
        )
        .unwrap();
        let mut settings = DeviceSettings::default();
        settings.working_projects_path = projects_root.to_string_lossy().into_owned();
        settings
            .project_paths
            .insert("project-a".into(), directory.to_string_lossy().into_owned());
        let requests = project_manifest_drift_requests(&settings, &catalogue);
        assert_eq!(requests.len(), 1);
        let state_path = root.join("manifest-sync.json");
        let issues = reconcile_project_manifests_with(
            &state_path,
            &settings,
            &catalogue,
            requests,
            write_project_manifest,
            write_json_if_changed,
        )
        .unwrap();
        assert!(issues.is_empty());
        let manifest: serde_json::Value =
            read_json(&directory.join(".pimp-project.json")).unwrap();
        assert_eq!(manifest["customField"], "preserved");
        assert_eq!(manifest["media"][0]["id"], canonical_id);
        assert!(project_manifest_drift_requests(&settings, &catalogue).is_empty());
        assert!(!write_project_manifest(&directory, &catalogue.projects[0], &catalogue.media)
            .unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_project_inventory_is_read_only_and_categorized() {
        let root = temporary("project-inventory");
        let directory = root.join("projects").join("Simple Vlog 057");
        for folder in [
            directory.join("Adobe Premiere Pro Auto-Save"),
            directory.join("assets").join("video"),
            directory.join("assets").join("audio"),
            directory.join("assets").join("graphics"),
            directory.join("Exports"),
        ] {
            fs::create_dir_all(folder).unwrap();
        }
        let fixtures = [
            ("Simple Vlog 057.prproj", b"project".as_slice()),
            ("description.txt", b"notes".as_slice()),
            ("Adobe Premiere Pro Auto-Save/backup.prproj", b"backup".as_slice()),
            ("assets/video/b-roll.mov", b"video".as_slice()),
            ("assets/audio/music.wav", b"audio".as_slice()),
            ("assets/graphics/title.png", b"graphic".as_slice()),
            ("Exports/final.mp4", b"export".as_slice()),
            ("assets/cache.bin", b"other".as_slice()),
        ];
        for (relative, bytes) in fixtures {
            fs::write(directory.join(relative), bytes).unwrap();
        }

        let inventory =
            inventory_project_directory("project-a", "Simple Vlog 057", &directory).unwrap();
        assert_eq!(inventory.item_count, 8);
        assert_eq!(
            inventory.main_premiere_project.as_deref(),
            Some("Simple Vlog 057.prproj")
        );
        assert_eq!(inventory.items[0].relative_path, "Adobe Premiere Pro Auto-Save/backup.prproj");
        assert!(inventory
            .items
            .iter()
            .any(|item| item.category == "premiere-auto-save"));
        assert!(inventory
            .items
            .iter()
            .any(|item| item.category == "video"));
        assert!(inventory
            .items
            .iter()
            .any(|item| item.category == "audio-music"));
        assert!(inventory
            .items
            .iter()
            .any(|item| item.category == "images-graphics"));
        assert!(inventory
            .items
            .iter()
            .any(|item| item.category == "exports"));
        assert!(inventory
            .items
            .iter()
            .any(|item| item.category == "notes-documentation"));
        assert!(inventory.items.iter().any(|item| item.category == "other"));
        assert!(directory.join("Simple Vlog 057.prproj").is_file());
        fs::write(directory.join("assets/video/new-take.mp4"), b"new").unwrap();
        let refreshed =
            inventory_project_directory("project-a", "Simple Vlog 057", &directory).unwrap();
        assert_eq!(refreshed.item_count, 9);
        assert!(refreshed
            .items
            .iter()
            .any(|item| item.relative_path == "assets/video/new-take.mp4"));
        fs::remove_dir_all(root).unwrap();
        assert!(inventory_project_directory("project-a", "Simple Vlog 057", &directory).is_err());
    }

    #[test]
    fn project_inventory_fails_clearly_when_the_file_budget_is_exceeded() {
        let root = temporary("project-inventory-budget");
        let directory = root.join("project");
        fs::create_dir_all(&directory).unwrap();
        for index in 0..4 {
            fs::write(
                directory.join(format!("asset-{index}.mp4")),
                b"media",
            )
            .unwrap();
        }

        let error = inventory_project_directory_with_limits(
            "project-a",
            "Budget Test",
            &directory,
            3,
            64,
        )
        .unwrap_err();
        assert!(error.contains("stopped after 3 files"));
        assert_eq!(fs::read(directory.join("asset-0.mp4")).unwrap(), b"media");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hover_preview_sampling_is_bounded_and_avoids_short_clip_duplicates() {
        assert_eq!(hover_preview_frame_times(0.4).unwrap().len(), 1);
        assert_eq!(hover_preview_frame_times(3.0).unwrap().len(), 6);
        let long = hover_preview_frame_times(600.0).unwrap();
        assert_eq!(long.len(), 16);
        assert_eq!(long.first().copied(), Some(0.0));
        assert_eq!(long.last().copied(), Some(600.0));
        assert!(hover_preview_frame_times(0.0).is_err());
    }

    #[test]
    fn hover_preview_cache_is_fingerprinted_validated_and_budgeted() {
        let root = temporary("hover-preview-cache");
        let first = root.join("first");
        let second = root.join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        let manifest = HoverPreviewManifest {
            version: 1,
            media_id: "media-a".into(),
            fingerprint: "fingerprint-a".into(),
            duration_seconds: 5.0,
            frame_times: vec![0.0],
            frame_files: vec!["frame-001.jpg".into()],
            generated_at: Utc::now().to_rfc3339(),
        };
        fs::write(first.join("frame-001.jpg"), b"frame-a").unwrap();
        fs::write(
            first.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let preview = read_hover_preview(&first, "fingerprint-a").unwrap();
        assert_eq!(preview.media_id, "media-a");
        assert!(read_hover_preview(&first, "changed-source").is_err());

        let mut second_manifest = manifest.clone();
        second_manifest.fingerprint = "fingerprint-b".into();
        fs::write(second.join("frame-001.jpg"), b"frame-b").unwrap();
        fs::write(
            second.join("manifest.json"),
            serde_json::to_vec(&second_manifest).unwrap(),
        )
        .unwrap();
        evict_hover_preview_cache(&root, 0, &second).unwrap();
        assert!(!first.exists());
        assert!(second.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_project_manifest_sync_is_durable_and_retryable_without_catalogue_events() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };

        let root = temporary("manifest-retry");
        let projects_root = root.join("projects");
        let directory = projects_root.join("Project A");
        fs::create_dir_all(&directory).unwrap();
        let state_path = root.join("manifest-sync.json");
        let mut settings = DeviceSettings::default();
        settings.working_projects_path = projects_root.to_string_lossy().into_owned();
        settings
            .project_paths
            .insert("project-a".into(), directory.to_string_lossy().into_owned());
        let project = ProjectRecord {
            id: "project-a".into(),
            name: "Project A".into(),
            working_directory: ".".into(),
            media_ids: vec!["media-a".into()],
            notes: String::new(),
            tags: vec![],
            created_at: "2026-10-27T00:00:00Z".into(),
        };
        let source = root.join("source.mp4");
        fs::write(&source, b"synthetic").unwrap();
        let mut media = new_media(
            &source,
            "20261027/source.mp4".into(),
            None,
            "hash".into(),
            None,
            "unknown".into(),
            false,
        )
        .unwrap();
        media.id = "media-a".into();
        media.project_ids = vec!["project-a".into()];
        let catalogue = Catalogue {
            media: vec![media],
            projects: vec![project],
            ..Default::default()
        };
        let attempts = Arc::new(AtomicUsize::new(0));
        let failure_counter = attempts.clone();
        let issues = reconcile_project_manifests_with(
            &state_path,
            &settings,
            &catalogue,
            vec![("project-a".into(), None)],
            move |_, _, _| {
                failure_counter.fetch_add(1, Ordering::SeqCst);
                Err("injected unavailable drive".into())
            },
            write_json_if_changed,
        )
        .unwrap();
        assert_eq!(issues.len(), 1);
        let pending: ProjectManifestSyncState = read_json(&state_path).unwrap();
        assert_eq!(pending.pending["project-a"].attempts, 1);
        assert!(!directory.join(".pimp-project.json").exists());

        let issues = reconcile_project_manifests_with(
            &state_path,
            &settings,
            &catalogue,
            std::iter::empty::<(String, Option<String>)>(),
            |_, _, _| Err("injected unavailable drive again".into()),
            write_json_if_changed,
        )
        .unwrap();
        assert_eq!(issues.len(), 1);
        let pending: ProjectManifestSyncState = read_json(&state_path).unwrap();
        assert_eq!(pending.pending["project-a"].attempts, 2);

        let issues = reconcile_project_manifests_with(
            &state_path,
            &settings,
            &catalogue,
            std::iter::empty::<(String, Option<String>)>(),
            write_project_manifest,
            write_json_if_changed,
        )
        .unwrap();
        assert!(issues.is_empty());
        assert!(directory.join(".pimp-project.json").is_file());
        let pending: ProjectManifestSyncState = read_json(&state_path).unwrap();
        assert!(pending.pending.is_empty());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn no_op_manifest_refresh_writes_neither_state_nor_compatible_manifest() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let root = temporary("manifest-no-op");
        let projects_root = root.join("projects");
        let directory = projects_root.join("Project A");
        fs::create_dir_all(&directory).unwrap();
        let state_path = root.join("manifest-sync.json");
        let mut settings = DeviceSettings::default();
        settings.working_projects_path = projects_root.to_string_lossy().into_owned();
        settings
            .project_paths
            .insert("project-a".into(), directory.to_string_lossy().into_owned());
        let project = ProjectRecord {
            id: "project-a".into(),
            name: "Project A".into(),
            working_directory: ".".into(),
            media_ids: vec![],
            notes: String::new(),
            tags: vec![],
            created_at: "2026-10-27T00:00:00Z".into(),
        };
        let catalogue = Catalogue {
            projects: vec![project.clone()],
            ..Default::default()
        };
        assert!(write_project_manifest(&directory, &project, &[]).unwrap());
        assert!(!write_project_manifest(&directory, &project, &[]).unwrap());
        assert!(!directory.join(".pimp-project.json.backup").exists());

        let state_writes = AtomicUsize::new(0);
        let manifest_calls = AtomicUsize::new(0);
        let issues = reconcile_project_manifests_with(
            &state_path,
            &settings,
            &catalogue,
            std::iter::empty::<(String, Option<String>)>(),
            |_, _, _| {
                manifest_calls.fetch_add(1, Ordering::SeqCst);
                Ok(false)
            },
            |path, state| {
                state_writes.fetch_add(1, Ordering::SeqCst);
                write_json_if_changed(path, state)
            },
        )
        .unwrap();
        assert!(issues.is_empty());
        assert_eq!(manifest_calls.load(Ordering::SeqCst), 0);
        assert_eq!(state_writes.load(Ordering::SeqCst), 0);
        assert!(!state_path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_retry_state_clear_preserves_the_durable_intent() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let root = temporary("manifest-state-failure");
        let projects_root = root.join("projects");
        let directory = projects_root.join("Project A");
        fs::create_dir_all(&directory).unwrap();
        let state_path = root.join("manifest-sync.json");
        let mut settings = DeviceSettings::default();
        settings.working_projects_path = projects_root.to_string_lossy().into_owned();
        settings
            .project_paths
            .insert("project-a".into(), directory.to_string_lossy().into_owned());
        let catalogue = Catalogue {
            projects: vec![ProjectRecord {
                id: "project-a".into(),
                name: "Project A".into(),
                working_directory: ".".into(),
                media_ids: vec![],
                notes: String::new(),
                tags: vec![],
                created_at: "2026-10-27T00:00:00Z".into(),
            }],
            ..Default::default()
        };
        let writes = AtomicUsize::new(0);
        let result = reconcile_project_manifests_with(
            &state_path,
            &settings,
            &catalogue,
            [("project-a".into(), None)],
            |_, _, _| Ok(false),
            |path, state| {
                if writes.fetch_add(1, Ordering::SeqCst) == 0 {
                    write_json_if_changed(path, state)
                } else {
                    Err("injected retry-state replacement failure".into())
                }
            },
        );
        assert!(result.is_err());
        let durable: ProjectManifestSyncState = read_json(&state_path).unwrap();
        assert!(durable.pending.contains_key("project-a"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_intent_persistence_stops_before_manifest_work() {
        let root = temporary("manifest-intent-failure");
        let state_path = root.join("manifest-sync.json");
        let result = persist_project_manifest_intents_with(
            &state_path,
            [("project-a".into(), None)],
            &|_, _| Err("injected write-ahead failure".into()),
        );
        assert!(result.is_err());
        assert!(!state_path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pending_project_path_repairs_a_crash_after_registration() {
        let root = temporary("project-path-recovery");
        let projects_root = root.join("projects");
        let directory = projects_root.join("Project A");
        fs::create_dir_all(&directory).unwrap();
        let state_path = root.join("manifest-sync.json");
        let settings_file = root.join("settings.json");
        let mut settings = DeviceSettings::default();
        settings.working_projects_path = projects_root.to_string_lossy().into_owned();
        let project = ProjectRecord {
            id: "project-a".into(),
            name: "Project A".into(),
            working_directory: ".".into(),
            media_ids: vec![],
            notes: String::new(),
            tags: vec![],
            created_at: "2026-10-27T00:00:00Z".into(),
        };
        let catalogue = Catalogue {
            projects: vec![project],
            ..Default::default()
        };
        persist_project_manifest_intents(
            &state_path,
            [(
                "project-a".into(),
                Some(directory.to_string_lossy().into_owned()),
            )],
        )
        .unwrap();

        assert!(recover_pending_project_paths(
            &state_path,
            &settings_file,
            &mut settings,
            &catalogue
        )
        .unwrap());
        let expected = directory.canonicalize().unwrap().to_string_lossy().into_owned();
        assert_eq!(settings.project_paths.get("project-a"), Some(&expected));
        let issues = reconcile_project_manifests_with(
            &state_path,
            &settings,
            &catalogue,
            std::iter::empty::<(String, Option<String>)>(),
            write_project_manifest,
            write_json_if_changed,
        )
        .unwrap();
        assert!(issues.is_empty());
        assert!(directory.join(".pimp-project.json").is_file());
        let durable: ProjectManifestSyncState = read_json(&state_path).unwrap();
        assert!(durable.pending.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_append_failure_creates_no_new_manifest_or_folder_mapping() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let root = temporary("project-append-failure");
        let projects_root = root.join("projects");
        let directory = projects_root.join("Project A");
        fs::create_dir_all(&directory).unwrap();
        let state_path = root.join("manifest-sync.json");
        let settings_file = root.join("settings.json");
        let mut settings = DeviceSettings::default();
        settings.working_projects_path = projects_root.to_string_lossy().into_owned();
        let project = ProjectRecord {
            id: "project-a".into(),
            name: "Project A".into(),
            working_directory: ".".into(),
            media_ids: vec![],
            notes: String::new(),
            tags: vec![],
            created_at: "2026-10-27T00:00:00Z".into(),
        };
        let reconcile_called = AtomicBool::new(false);
        let result = commit_project_creation_with(
            &state_path,
            &settings_file,
            &mut settings,
            &project,
            &directory,
            |_| Err("injected event append failure".into()),
            |_| {
                reconcile_called.store(true, Ordering::SeqCst);
                Ok(vec![])
            },
        );
        assert!(result.is_err());
        assert!(!reconcile_called.load(Ordering::SeqCst));
        assert!(!directory.join(".pimp-project.json").exists());
        assert!(!settings_file.exists());
        assert!(!settings.project_paths.contains_key("project-a"));
        assert!(directory.is_dir());
        let durable: ProjectManifestSyncState = read_json(&state_path).unwrap();
        assert!(durable.pending.contains_key("project-a"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_transaction_lock_serializes_refresh_with_metadata_reconciliation() {
        use std::sync::{mpsc, Arc};
        use std::thread;

        let root = temporary("manifest-concurrency");
        let projects_root = root.join("projects");
        let directory = projects_root.join("Project A");
        fs::create_dir_all(&directory).unwrap();
        let state_path = root.join("manifest-sync.json");
        let mut settings = DeviceSettings::default();
        settings.working_projects_path = projects_root.to_string_lossy().into_owned();
        settings
            .project_paths
            .insert("project-a".into(), directory.to_string_lossy().into_owned());
        let catalogue = Catalogue {
            projects: vec![ProjectRecord {
                id: "project-a".into(),
                name: "Project A".into(),
                working_directory: ".".into(),
                media_ids: vec![],
                notes: String::new(),
                tags: vec![],
                created_at: "2026-10-27T00:00:00Z".into(),
            }],
            ..Default::default()
        };
        let state = Arc::new(NativeState::default());
        let (held_tx, held_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let first_state = state.clone();
        let first_path = state_path.clone();
        let first_settings = settings.clone();
        let first_catalogue = catalogue.clone();
        let updater = thread::spawn(move || {
            let _guard = first_state.project_manifest_write.lock().unwrap();
            persist_project_manifest_intents(
                &first_path,
                [("project-a".into(), None)],
            )
            .unwrap();
            held_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            reconcile_project_manifests_with(
                &first_path,
                &first_settings,
                &first_catalogue,
                std::iter::empty::<(String, Option<String>)>(),
                write_project_manifest,
                write_json_if_changed,
            )
            .unwrap();
        });
        held_rx.recv().unwrap();

        let second_state = state.clone();
        let second_path = state_path.clone();
        let second_settings = settings.clone();
        let second_catalogue = catalogue.clone();
        let (started_tx, started_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();
        let refresher = thread::spawn(move || {
            started_tx.send(()).unwrap();
            let _guard = second_state.project_manifest_write.lock().unwrap();
            let issues = reconcile_project_manifests_with(
                &second_path,
                &second_settings,
                &second_catalogue,
                std::iter::empty::<(String, Option<String>)>(),
                write_project_manifest,
                write_json_if_changed,
            )
            .unwrap();
            finished_tx.send(issues).unwrap();
        });
        started_rx.recv().unwrap();
        assert!(finished_rx
            .recv_timeout(std::time::Duration::from_millis(50))
            .is_err());
        release_tx.send(()).unwrap();
        updater.join().unwrap();
        assert!(finished_rx.recv().unwrap().is_empty());
        refresher.join().unwrap();
        let durable: ProjectManifestSyncState = read_json(&state_path).unwrap();
        assert!(durable.pending.is_empty());
        assert!(directory.join(".pimp-project.json").is_file());
        fs::remove_dir_all(root).unwrap();
    }
}
