mod backup;
mod catalogue;
mod discovery;
mod fs_safe;
mod premiere_index;

use backup::BackupJob;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use catalogue::{
    Catalogue, CatalogueEvent, CatalogueIssue, MediaItem, ProjectRecord, TechnicalMetadata,
};
use premiere_index::PremiereProjectIndex;
#[cfg(test)]
use chrono::TimeZone;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration as StdDuration, Instant},
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
    enrichment_worker_active: Mutex<bool>,
    enrichment_requested: Mutex<bool>,
    enrichment_status: Mutex<Option<BackgroundOperation>>,
    job_store: Mutex<()>,
    worker_active: Mutex<bool>,
    availability: Mutex<HashMap<String, (String, bool)>>,
    tray_available: Mutex<bool>,
    operations: Mutex<OperationCoordinator>,
    preview_generation: Mutex<()>,
    static_thumbnail_generation: Mutex<()>,
    thumbnail_generations: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    playback_proxy_generations: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    playback_proxy_status: Mutex<HashMap<String, BackgroundOperation>>,
    playback_proxy_cancelled: Mutex<HashSet<String>>,
    active_playback_proxies: Mutex<HashMap<String, PathBuf>>,
    playback_proxy_temporaries: Mutex<HashSet<PathBuf>>,
    playback_proxy_processes: Mutex<HashMap<String, u32>>,
    backup_job_store: Mutex<()>,
    approved_plan_store: Mutex<()>,
    inventory_worker: Mutex<()>,
    inventory_cancelled: Mutex<HashSet<String>>,
    inventory_status: Mutex<Option<BackgroundOperation>>,
    project_inventory_status: Mutex<HashMap<String, BackgroundOperation>>,
}

#[derive(Default)]
struct OperationCoordinator {
    update_installing: bool,
    active: BTreeMap<&'static str, usize>,
}

struct OperationPermit<'a> {
    coordinator: &'a Mutex<OperationCoordinator>,
    name: &'static str,
}

impl Drop for OperationPermit<'_> {
    fn drop(&mut self) {
        let mut coordinator = self.coordinator.lock().unwrap();
        if let Some(count) = coordinator.active.get_mut(self.name) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                coordinator.active.remove(self.name);
            }
        }
    }
}

fn begin_critical_operation<'a>(
    coordinator: &'a Mutex<OperationCoordinator>,
    name: &'static str,
) -> Result<OperationPermit<'a>, String> {
    let mut state = coordinator.lock().unwrap();
    if state.update_installing {
        return Err(UPDATE_OPERATION_BLOCKED.into());
    }
    *state.active.entry(name).or_default() += 1;
    drop(state);
    Ok(OperationPermit { coordinator, name })
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
    #[serde(default = "default_playback_proxy_cache_limit_bytes")]
    playback_proxy_cache_limit_bytes: u64,
    #[serde(default = "default_thumbnail_hold_frame")]
    thumbnail_hold_frame: String,
    #[serde(default = "default_preview_preloading")]
    preview_preloading: String,
    #[serde(default)]
    preview_ram_cache_limit_bytes: u64,
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
    #[serde(default = "default_true")]
    sd_grabber_enabled: bool,
    #[serde(default = "default_true")]
    sd_grabber_auto_detect: bool,
    #[serde(default = "default_sd_grabber_extensions")]
    sd_grabber_extensions: Vec<String>,
    #[serde(default = "default_true")]
    sd_grabber_search_subfolders: bool,
    #[serde(default = "default_true")]
    sd_grabber_remember_devices: bool,
    #[serde(default)]
    sd_grabber_recognized_device_ids: Vec<String>,
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
    #[serde(default)]
    premiere_index: Option<PremiereProjectIndex>,
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

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct PremiereImportManifest {
    schema_version: u32,
    kind: String,
    created_at: String,
    device_id: String,
    items: Vec<PremiereImportItem>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct PremiereImportItem {
    media_id: String,
    absolute_path: String,
    relative_path: String,
    original_filename: String,
    sha256: String,
    file_type: String,
    bin_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    attached_to_media_id: Option<String>,
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

fn default_playback_proxy_cache_limit_bytes() -> u64 {
    2 * 1024 * 1024 * 1024
}
fn default_thumbnail_hold_frame() -> String {
    "first".into()
}
fn default_preview_preloading() -> String {
    "visible-nearby".into()
}
fn default_sd_grabber_extensions() -> Vec<String> {
    vec!["mp4".into(), "wav".into()]
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
            playback_proxy_cache_limit_bytes: default_playback_proxy_cache_limit_bytes(),
            thumbnail_hold_frame: default_thumbnail_hold_frame(),
            preview_preloading: default_preview_preloading(),
            preview_ram_cache_limit_bytes: 0,
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
            sd_grabber_enabled: true,
            sd_grabber_auto_detect: true,
            sd_grabber_extensions: default_sd_grabber_extensions(),
            sd_grabber_search_subfolders: true,
            sd_grabber_remember_devices: true,
            sd_grabber_recognized_device_ids: vec![],
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
    #[serde(default)]
    batch_tags: Vec<String>,
    #[serde(default)]
    approved_plan_id: Option<String>,
    #[serde(default)]
    allow_duplicate_copies: bool,
    #[serde(default)]
    allow_unverified_reimport: bool,
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
    #[serde(default)]
    disposition: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    existing_relative_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
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
struct InventoryConflict {
    source_path: String,
    destination_path: String,
    reason: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SourceInventory {
    plan_id: String,
    files: Vec<InventoryFile>,
    excluded: Vec<InventoryExclusion>,
    issues: Vec<String>,
    #[serde(default)]
    conflicts: Vec<InventoryConflict>,
    total_file_count: usize,
    total_bytes: u64,
    copy_file_count: usize,
    copy_bytes: u64,
    #[serde(default)]
    already_imported_file_count: usize,
    #[serde(default)]
    already_imported_bytes: u64,
    #[serde(default)]
    attention_file_count: usize,
    #[serde(default)]
    attention_bytes: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct RemovableVolume {
    root_path: String,
    label: String,
    identity: String,
    capacity_bytes: u64,
    available_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ApprovedIngestFile {
    inventory: InventoryFile,
    source_size: u64,
    source_modified_millis: Option<u128>,
    #[serde(default)]
    planning_modified_millis: Option<u128>,
    source_sha256: Option<String>,
    #[serde(default)]
    source_volume_identity: Option<String>,
    existing_media_id: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ApprovedIngestPlan {
    schema_version: u32,
    id: String,
    #[serde(default)]
    lineage_id: String,
    #[serde(default)]
    revision: u64,
    created_at: String,
    batch_timestamp: String,
    master_bin_path: String,
    sources: Vec<String>,
    date_override: Option<String>,
    files: Vec<ApprovedIngestFile>,
    inventory: SourceInventory,
    sd_grabber: bool,
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
    #[serde(default)]
    source_modified_millis: Option<u128>,
    #[serde(default)]
    approved_sha256: Option<String>,
    #[serde(default)]
    source_volume_identity: Option<String>,
    #[serde(default)]
    duplicate_existing_relative_path: Option<String>,
    #[serde(default)]
    intentional_duplicate: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct IngestJob {
    schema_version: u32,
    id: String,
    #[serde(default)]
    approved_plan_id: Option<String>,
    #[serde(default)]
    approved_plan_lineage_id: Option<String>,
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
struct OperationCounts {
    new_files: usize,
    duplicate_files: usize,
    attention_files: usize,
    failed_files: usize,
    #[serde(default)]
    attempted_files: usize,
    #[serde(default)]
    existing_files: usize,
    #[serde(default)]
    pending_files: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct BackgroundOperation {
    id: String,
    kind: String,
    title: String,
    state: String,
    stage: String,
    current_item: Option<String>,
    completed_items: usize,
    total_items: Option<usize>,
    bytes_completed: u64,
    bytes_total: Option<u64>,
    counts: OperationCounts,
    cancellable: bool,
    pausable: bool,
    retryable: bool,
    error: Option<String>,
    started_at: String,
    updated_at: String,
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

fn approved_plan_path(app: &AppHandle, plan_id: &str) -> Result<PathBuf, String> {
    let id = Uuid::parse_str(plan_id).map_err(|_| "Approved ingest plan ID is invalid")?;
    Ok(settings_path(app)?
        .parent()
        .ok_or("Invalid application configuration path")?
        .join("approved-ingest-plans")
        .join(format!("{id}.json")))
}

fn save_approved_plan(app: &AppHandle, plan: &ApprovedIngestPlan) -> Result<(), String> {
    write_json(&approved_plan_path(app, &plan.id)?, plan)
}

fn load_approved_plan(app: &AppHandle, plan_id: &str) -> Result<ApprovedIngestPlan, String> {
    let path = approved_plan_path(app, plan_id)?;
    if !path.is_file() {
        return Err("Approved ingest plan is missing; scan the source again".into());
    }
    read_json(&path)
}

fn approved_plan_lineage(plan: &ApprovedIngestPlan) -> &str {
    if plan.lineage_id.is_empty() {
        &plan.id
    } else {
        &plan.lineage_id
    }
}

fn current_approved_plan(
    app: &AppHandle,
    requested_id: &str,
) -> Result<ApprovedIngestPlan, String> {
    let requested = load_approved_plan(app, requested_id)?;
    let lineage = approved_plan_lineage(&requested).to_owned();
    let directory = approved_plan_path(app, requested_id)?
        .parent()
        .ok_or("Invalid approved plan directory")?
        .to_path_buf();
    let mut current = requested;
    if !directory.is_dir() {
        return Ok(current);
    }
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(candidate) = read_json::<ApprovedIngestPlan>(&path) else {
            continue;
        };
        if approved_plan_lineage(&candidate) == lineage && candidate.revision > current.revision {
            current = candidate;
        }
    }
    Ok(current)
}

fn begin_approved_plan_revision(mut plan: ApprovedIngestPlan) -> ApprovedIngestPlan {
    let lineage = approved_plan_lineage(&plan).to_owned();
    plan.id = Uuid::new_v4().to_string();
    plan.lineage_id = lineage;
    plan.revision = plan.revision.saturating_add(1);
    plan.created_at = Utc::now().to_rfc3339();
    plan.inventory.plan_id = plan.id.clone();
    plan
}

fn remove_approved_plan_lineage(app: &AppHandle, lineage: &str) -> Result<(), String> {
    let directory = approved_plan_path(app, lineage)?
        .parent()
        .ok_or("Invalid approved plan directory")?
        .to_path_buf();
    if !directory.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(plan) = read_json::<ApprovedIngestPlan>(&path) else {
            continue;
        };
        if approved_plan_lineage(&plan) == lineage {
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
}

#[tauri::command]
fn reapprove_ingest_destinations(
    app: AppHandle,
    plan_id: String,
    renamed_destinations: HashMap<String, String>,
) -> Result<SourceInventory, String> {
    if renamed_destinations.is_empty() {
        return Err("Enter at least one explicit replacement filename".into());
    }
    let state = app.state::<NativeState>();
    let _plan_guard = state.approved_plan_store.lock().unwrap();
    let settings = load_settings_file(&app)?;
    let master = PathBuf::from(&settings.master_bin_path);
    let mut plan = current_approved_plan(&app, &plan_id)?;
    if plan.schema_version != 1 || plan.master_bin_path != settings.master_bin_path {
        return Err("The approved plan is stale; scan the source again".into());
    }
    let known_sources = plan
        .files
        .iter()
        .map(|file| file.inventory.source_path.clone())
        .collect::<HashSet<_>>();
    if renamed_destinations
        .keys()
        .any(|source| !known_sources.contains(source))
    {
        return Err("A destination rename does not belong to the approved plan".into());
    }
    let mut planned = HashSet::new();
    for approved in &mut plan.files {
        if let Some(filename) = renamed_destinations.get(&approved.inventory.source_path) {
            let filename = fs_safe::validate_relative(filename.trim())?;
            if filename.components().count() != 1 {
                return Err("Conflict resolution accepts a filename, not a folder path".into());
            }
            let current = Path::new(&approved.inventory.planned_relative_path);
            let parent = current.parent().unwrap_or_else(|| Path::new(""));
            approved.inventory.planned_relative_path = portable_relative(&parent.join(filename));
            approved.inventory.disposition = "new".into();
            approved.inventory.detail = None;
            approved.inventory.existing_relative_path = None;
            approved.existing_media_id = None;
        }
        let destination =
            fs_safe::resolve_for_create(&master, &approved.inventory.planned_relative_path)?;
        if destination.exists() {
            return Err(format!(
                "The reviewed destination already exists: {}",
                approved.inventory.planned_relative_path
            ));
        }
        if !planned.insert(
            approved
                .inventory
                .planned_relative_path
                .to_ascii_lowercase(),
        ) {
            return Err("Two reviewed files still use the same destination".into());
        }
    }
    plan = begin_approved_plan_revision(plan);
    plan.inventory.files = plan
        .files
        .iter()
        .map(|file| file.inventory.clone())
        .collect();
    plan.inventory.conflicts.clear();
    plan.inventory.copy_file_count = plan
        .inventory
        .files
        .iter()
        .filter(|file| file.disposition == "new")
        .count();
    plan.inventory.copy_bytes = plan
        .inventory
        .files
        .iter()
        .filter(|file| file.disposition == "new")
        .map(|file| file.size_bytes)
        .sum();
    plan.inventory.attention_file_count = plan
        .inventory
        .files
        .iter()
        .filter(|file| file.disposition == "attention")
        .count();
    plan.inventory.attention_bytes = plan
        .inventory
        .files
        .iter()
        .filter(|file| file.disposition == "attention")
        .map(|file| file.size_bytes)
        .sum();
    save_approved_plan(&app, &plan)?;
    Ok(plan.inventory)
}

#[tauri::command]
async fn replan_ingest_destinations(
    app: AppHandle,
    plan_id: String,
    date_override: String,
) -> Result<SourceInventory, String> {
    tauri::async_runtime::spawn_blocking(move || {
        replan_ingest_destinations_blocking(app, plan_id, date_override)
    })
    .await
    .map_err(|error| format!("Date reapproval worker failed: {error}"))?
}

fn local_datetime_from_millis(millis: Option<u128>) -> Option<DateTime<Local>> {
    let millis = u64::try_from(millis?).ok()?;
    let time =
        std::time::SystemTime::UNIX_EPOCH.checked_add(std::time::Duration::from_millis(millis))?;
    Some(DateTime::<Local>::from(time))
}

fn replan_approved_relative_path(
    settings: &DeviceSettings,
    master: &Path,
    approved: &ApprovedIngestFile,
    date_override: &str,
    batch_timestamp: &DateTime<Local>,
) -> Result<String, String> {
    let current = fs_safe::validate_relative(&approved.inventory.planned_relative_path)?;
    let suffix = current.components().skip(1).collect::<PathBuf>();
    if suffix.as_os_str().is_empty() {
        return Err("Approved destination is missing its filename; scan the source again".into());
    }
    let original_dates = date_override == "original-file-dates";
    if original_dates
        && approved.planning_modified_millis.is_none()
        && package_component(Path::new(&approved.inventory.source_relative_path))
    {
        return Err(
            "This camera-package plan predates safe date reapproval; scan the source again".into(),
        );
    }
    let planning_millis = approved
        .planning_modified_millis
        .or(approved.source_modified_millis);
    let (date, _, _) = destination_date_from_modified(
        settings,
        local_datetime_from_millis(planning_millis),
        &Some(date_override.to_owned()),
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
    let relative = PathBuf::from(date_folder).join(suffix);
    fs_safe::validate_relative(&relative.to_string_lossy())?;
    Ok(portable_relative(&relative))
}

fn replan_ingest_destinations_blocking(
    app: AppHandle,
    plan_id: String,
    date_override: String,
) -> Result<SourceInventory, String> {
    let state = app.state::<NativeState>();
    let _plan_guard = state.approved_plan_store.lock().unwrap();
    let settings = load_settings_file(&app)?;
    let master = PathBuf::from(&settings.master_bin_path);
    let mut plan = current_approved_plan(&app, &plan_id)?;
    if plan.schema_version != 1 || plan.master_bin_path != settings.master_bin_path {
        return Err("The approved plan is stale; scan the source again".into());
    }
    let batch_timestamp = DateTime::parse_from_rfc3339(&plan.batch_timestamp)
        .map_err(|_| "The approved plan has an invalid batch timestamp; scan again")?
        .with_timezone(&Local);
    let catalogue = load_catalogue_file(&settings)?;
    let mut hash_index = HashMap::<String, Vec<&MediaItem>>::new();
    for item in &catalogue.media {
        hash_index.entry(item.sha256.clone()).or_default().push(item);
    }
    let mut selection_hashes = HashMap::<String, String>::new();
    let mut planned_destinations = HashSet::new();
    let mut conflicts = Vec::new();
    for approved in &mut plan.files {
        let mut inventory = approved.inventory.clone();
        let source = PathBuf::from(&inventory.source_path);
        if !approved_source_snapshot_matches(&source, approved) {
            return Err(format!(
                "Source changed since approval: {}; scan the source again",
                source.display()
            ));
        }
        let hash = approved
            .source_sha256
            .as_ref()
            .ok_or("The approved plan predates safe date reapproval; scan the source again")?;
        inventory.planned_relative_path = replan_approved_relative_path(
            &settings,
            &master,
            approved,
            &date_override,
            &batch_timestamp,
        )?;
        approved.existing_media_id = None;
        if let Some(relative_path) = selection_hashes.get(hash) {
            inventory.disposition = "already-imported".into();
            inventory.existing_relative_path = Some(relative_path.clone());
            inventory.detail = Some("Duplicate recording within this approved selection".into());
        } else {
            let candidates = hash_index.get(hash).map(Vec::as_slice).unwrap_or(&[]);
            let (disposition, media_id, relative_path, detail) =
                classify_global_duplicate(&settings, candidates, approved.source_size, hash);
            inventory.disposition = disposition;
            inventory.existing_relative_path = relative_path;
            inventory.detail = detail;
            approved.existing_media_id = media_id;
            if inventory.disposition == "new" {
                selection_hashes.insert(hash.clone(), inventory.planned_relative_path.clone());
            }
        }
        if inventory.disposition != "already-imported" {
            let destination =
                fs_safe::resolve_for_create(&master, &inventory.planned_relative_path)?;
            let unique = planned_destinations
                .insert(inventory.planned_relative_path.to_ascii_lowercase());
            if destination.exists() || !unique {
                let reason = if destination.exists() {
                    "A different file already occupies the planned destination"
                } else {
                    "Two selected files resolve to the same planned destination"
                };
                conflicts.push(InventoryConflict {
                    source_path: inventory.source_path.clone(),
                    destination_path: inventory.planned_relative_path.clone(),
                    reason: reason.into(),
                });
                inventory.disposition = "attention".into();
                inventory.detail = Some(format!(
                    "{reason}. Enter a new filename and approve the revised plan."
                ));
            }
        }
        approved.inventory = inventory;
    }
    plan = begin_approved_plan_revision(plan);
    plan.date_override = Some(date_override);
    plan.inventory.files = plan
        .files
        .iter()
        .map(|file| file.inventory.clone())
        .collect();
    plan.inventory.conflicts = conflicts;
    plan.inventory.copy_file_count = plan
        .inventory
        .files
        .iter()
        .filter(|file| file.disposition == "new")
        .count();
    plan.inventory.copy_bytes = plan
        .inventory
        .files
        .iter()
        .filter(|file| file.disposition == "new")
        .map(|file| file.size_bytes)
        .sum();
    plan.inventory.already_imported_file_count = plan
        .inventory
        .files
        .iter()
        .filter(|file| file.disposition == "already-imported")
        .count();
    plan.inventory.already_imported_bytes = plan
        .inventory
        .files
        .iter()
        .filter(|file| file.disposition == "already-imported")
        .map(|file| file.size_bytes)
        .sum();
    plan.inventory.attention_file_count = plan
        .inventory
        .files
        .iter()
        .filter(|file| file.disposition == "attention")
        .count();
    plan.inventory.attention_bytes = plan
        .inventory
        .files
        .iter()
        .filter(|file| file.disposition == "attention")
        .map(|file| file.size_bytes)
        .sum();
    save_approved_plan(&app, &plan)?;
    Ok(plan.inventory)
}

fn backup_jobs_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(settings_path(app)?
        .parent()
        .ok_or("Invalid application configuration path")?
        .join("backup-jobs.json"))
}

fn load_backup_jobs_unlocked(app: &AppHandle) -> Result<Vec<BackupJob>, String> {
    read_json(&backup_jobs_path(app)?)
}

fn save_backup_job(app: &AppHandle, job: &BackupJob) -> Result<(), String> {
    let state = app.state::<NativeState>();
    let _guard = state.backup_job_store.lock().unwrap();
    let mut jobs = load_backup_jobs_unlocked(app)?;
    if let Some(existing) = jobs.iter_mut().find(|existing| existing.id == job.id) {
        *existing = job.clone();
    } else {
        jobs.push(job.clone());
    }
    write_json(&backup_jobs_path(app)?, &jobs)?;
    let _ = app.emit("backup-jobs-changed", &jobs);
    Ok(())
}

fn mark_interrupted_backup_jobs(jobs: &mut [BackupJob]) -> bool {
    let mut changed = false;
    for job in jobs {
        if matches!(job.state.as_str(), "prepared" | "copying" | "verifying") {
            job.state = "interrupted".into();
            job.error = Some(
                "The application stopped before this backup completed. Review and retry it.".into(),
            );
            job.updated_at = Utc::now().to_rfc3339();
            changed = true;
        }
    }
    changed
}

#[tauri::command]
fn initialize_backup_jobs(app: AppHandle) -> Result<Vec<BackupJob>, String> {
    let state = app.state::<NativeState>();
    let _guard = state.backup_job_store.lock().unwrap();
    let mut jobs = load_backup_jobs_unlocked(&app)?;
    let changed = mark_interrupted_backup_jobs(&mut jobs);
    if changed {
        write_json(&backup_jobs_path(&app)?, &jobs)?;
    }
    Ok(jobs)
}

#[tauri::command]
fn list_backup_jobs(app: AppHandle) -> Result<Vec<BackupJob>, String> {
    let state = app.state::<NativeState>();
    let _guard = state.backup_job_store.lock().unwrap();
    load_backup_jobs_unlocked(&app)
}

#[tauri::command]
async fn start_project_backup(
    app: AppHandle,
    state: State<'_, NativeState>,
    project_id: String,
    destination_root: String,
) -> Result<BackupJob, String> {
    let _operation = begin_critical_operation(&state.operations, "project backup")?;
    let settings = load_settings_file(&app)?;
    let catalogue = load_catalogue_file(&settings)?;
    let project = catalogue
        .projects
        .iter()
        .find(|project| project.id == project_id)
        .ok_or("Unknown project")?;
    let source = resolve_known_project_directory(&settings, &project_id)?;
    let destination = PathBuf::from(destination_root);
    let project_name = project.name.clone();
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut job = backup::plan_backup(&project_id, &project_name, &source, &destination)?;
        save_backup_job(&worker_app, &job)?;
        if let Err(error) =
            backup::execute_backup(&mut job, |snapshot| save_backup_job(&worker_app, snapshot))
        {
            if !matches!(job.state.as_str(), "needs-attention" | "failed") {
                job.state = "failed".into();
                job.error = Some(error.clone());
                job.updated_at = Utc::now().to_rfc3339();
                save_backup_job(&worker_app, &job)?;
            }
            return Err(error);
        }
        Ok(job)
    })
    .await
    .map_err(|error| format!("Backup worker failed: {error}"))?
}

#[tauri::command]
async fn retry_project_backup(
    app: AppHandle,
    state: State<'_, NativeState>,
    job_id: String,
    destination_root: Option<String>,
) -> Result<BackupJob, String> {
    let _operation = begin_critical_operation(&state.operations, "project backup")?;
    let mut job = {
        let state = app.state::<NativeState>();
        let _guard = state.backup_job_store.lock().unwrap();
        load_backup_jobs_unlocked(&app)?
            .into_iter()
            .find(|job| job.id == job_id)
            .ok_or("Backup job no longer exists")?
    };
    if job.state == "verified" {
        return Err("This backup is already verified".into());
    }
    if let Some(destination_root) = destination_root {
        job.destination_root = destination_root;
    }
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut job = job;
        job.error = None;
        if let Err(error) =
            backup::execute_backup(&mut job, |snapshot| save_backup_job(&worker_app, snapshot))
        {
            if !matches!(job.state.as_str(), "needs-attention" | "failed") {
                job.state = "failed".into();
                job.error = Some(error.clone());
                job.updated_at = Utc::now().to_rfc3339();
                save_backup_job(&worker_app, &job)?;
            }
            return Err(error);
        }
        Ok(job)
    })
    .await
    .map_err(|error| format!("Backup worker failed: {error}"))?
}

#[tauri::command]
async fn verify_project_backup(
    app: AppHandle,
    state: State<'_, NativeState>,
    job_id: String,
) -> Result<BackupJob, String> {
    let _operation = begin_critical_operation(&state.operations, "backup verification")?;
    let mut job = {
        let state = app.state::<NativeState>();
        let _guard = state.backup_job_store.lock().unwrap();
        load_backup_jobs_unlocked(&app)?
            .into_iter()
            .find(|job| job.id == job_id)
            .ok_or("Backup job no longer exists")?
    };
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        match backup::verify_backup(&job) {
            Ok(()) => {
                job.state = "verified".into();
                job.error = None;
            }
            Err(error) => {
                job.state = "needs-attention".into();
                job.error = Some(error.clone());
                job.updated_at = Utc::now().to_rfc3339();
                save_backup_job(&worker_app, &job)?;
                return Err(error);
            }
        }
        job.updated_at = Utc::now().to_rfc3339();
        save_backup_job(&worker_app, &job)?;
        Ok(job)
    })
    .await
    .map_err(|error| format!("Backup verification worker failed: {error}"))?
}

#[tauri::command]
async fn test_project_backup_restore(
    app: AppHandle,
    state: State<'_, NativeState>,
    job_id: String,
    restore_root: String,
) -> Result<backup::RestoreTestResult, String> {
    let _operation = begin_critical_operation(&state.operations, "backup restore test")?;
    let job = {
        let state = app.state::<NativeState>();
        let _guard = state.backup_job_store.lock().unwrap();
        load_backup_jobs_unlocked(&app)?
            .into_iter()
            .find(|job| job.id == job_id)
            .ok_or("Backup job no longer exists")?
    };
    tauri::async_runtime::spawn_blocking(move || {
        backup::restore_backup_test(&job, Path::new(&restore_root))
    })
    .await
    .map_err(|error| format!("Restore-test worker failed: {error}"))?
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

fn clear_finalized_jobs(
    jobs: &mut Vec<IngestJob>,
    job_id: Option<&str>,
    recovery_job_ids: &HashSet<String>,
) -> Result<usize, String> {
    if let Some(job_id) = job_id {
        let job = jobs
            .iter()
            .find(|job| job.id == job_id)
            .ok_or("Ingest history entry no longer exists")?;
        if !job_is_fully_finalized(job) || recovery_job_ids.contains(job_id) {
            return Err(
                "This ingest still contains recovery, error, inventory, or metadata evidence and cannot be cleared"
                    .into(),
            );
        }
    }
    let before = jobs.len();
    jobs.retain(|job| {
        let selected = job_id.map_or(true, |job_id| job.id == job_id);
        !(selected && job_is_fully_finalized(job) && !recovery_job_ids.contains(job.id.as_str()))
    });
    Ok(before - jobs.len())
}

fn recovery_job_ids(settings: &DeviceSettings) -> Result<HashSet<String>, String> {
    if settings.master_bin_path.trim().is_empty() {
        return Ok(HashSet::new());
    }
    let root = fs_safe::staging_root(Path::new(&settings.master_bin_path), &settings.device_id)?;
    let directory = root.join("jobs");
    if !directory.exists() {
        return Ok(HashSet::new());
    }
    let mut job_ids = HashSet::new();
    for entry in fs::read_dir(&directory)
        .map_err(|error| format!("Cannot inspect ingest recovery records: {error}"))?
    {
        let entry = entry.map_err(|error| format!("Cannot inspect recovery record: {error}"))?;
        let path = entry.path();
        if !entry
            .file_type()
            .map_err(|error| format!("Cannot inspect recovery record type: {error}"))?
            .is_file()
            || !matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("json") | Some("backup")
            )
        {
            continue;
        }
        let journal: IngestJournal = read_json(&path).map_err(|error| {
            format!(
                "Cannot safely clear ingest history while recovery record {} is unreadable: {error}",
                path.display()
            )
        })?;
        job_ids.insert(journal.job_id);
    }
    Ok(job_ids)
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

fn has_active_ingest(jobs: &[IngestJob]) -> bool {
    jobs.iter()
        .any(|job| matches!(job.state.as_str(), "queued" | "copying" | "verifying"))
}

const UPDATE_OPERATION_BLOCKED: &str = "A verified update is being installed. Wait for the application to restart before starting another filesystem operation.";

fn job_blocks_update(job: &IngestJob) -> bool {
    !job_is_fully_finalized(job)
}

fn begin_update_gate(
    coordinator: &mut OperationCoordinator,
    ingest_jobs: &[IngestJob],
    backup_jobs: &[BackupJob],
) -> Result<(), String> {
    if coordinator.update_installing {
        return Err("A verified update installation is already in progress.".into());
    }
    if !coordinator.active.is_empty() {
        let operations = coordinator
            .active
            .keys()
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Wait for active filesystem work to finish before installing the update: {operations}."
        ));
    }
    if ingest_jobs.iter().any(job_blocks_update) {
        return Err(
            "Resolve every active or recoverable ingest job before installing an update.".into(),
        );
    }
    if backup_jobs
        .iter()
        .any(|job| matches!(job.state.as_str(), "prepared" | "copying" | "verifying"))
    {
        return Err(
            "Wait for every active project backup to finish before installing an update.".into(),
        );
    }
    coordinator.update_installing = true;
    Ok(())
}

fn end_update_gate(operations: &Mutex<OperationCoordinator>) {
    operations.lock().unwrap().update_installing = false;
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
        && fs::read(path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?
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
        settings.thumbnail_hold_frame.as_str(),
        "first" | "10-percent" | "20-percent" | "middle" | "80-percent" | "last" | "last-hovered"
    ) {
        return Err("Choose a supported thumbnail hold frame".into());
    }
    if !matches!(
        settings.preview_preloading.as_str(),
        "off" | "hovered" | "visible" | "visible-nearby" | "idle"
    ) {
        return Err("Choose a supported preview preloading mode".into());
    }
    if settings.preview_ram_cache_limit_bytes != 0
        && !matches!(
            settings.preview_ram_cache_limit_bytes,
            67_108_864 | 134_217_728 | 268_435_456 | 536_870_912
        )
    {
        return Err("Choose a supported preview RAM cache allowance".into());
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
    settings.sd_grabber_extensions = settings
        .sd_grabber_extensions
        .iter()
        .map(|value| value.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if settings.sd_grabber_extensions.is_empty()
        || settings.sd_grabber_extensions.len() > 16
        || settings.sd_grabber_extensions.iter().any(|value| {
            value.len() > 12
                || !value
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        })
    {
        return Err("SD Card Grabber extensions must be simple file extensions".into());
    }
    settings.sd_grabber_recognized_device_ids = settings
        .sd_grabber_recognized_device_ids
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if settings.sd_grabber_recognized_device_ids.len() > 64
        || settings
            .sd_grabber_recognized_device_ids
            .iter()
            .any(|value| value.len() > 160 || value.chars().any(char::is_control))
    {
        return Err("Recognised SD device identities are invalid".into());
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
    recover_pending_project_paths(&sync_path, &settings_path(&app)?, &mut settings, &catalogue)?;
    let drifted_manifests = project_manifest_drift_requests(&settings, &catalogue);
    let manifest_issues =
        reconcile_project_manifests(&app, &settings, &catalogue, drifted_manifests)?;
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
    allowed_extensions: Option<&HashSet<String>>,
) {
    let size = candidate.metadata().ok().map(|metadata| metadata.len());
    inventory.total_file_count += 1;
    inventory.total_bytes += size.unwrap_or(0);
    if allowed_extensions.is_some_and(|allowed| {
        candidate
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| !allowed.contains(&value.to_ascii_lowercase()))
            .unwrap_or(true)
    }) {
        inventory.excluded.push(InventoryExclusion {
            path: candidate.to_string_lossy().into(),
            reason: "Not included by the SD Card Grabber profile".into(),
            size_bytes: size,
        });
        return;
    }
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
        disposition: "new".into(),
        existing_relative_path: None,
        detail: None,
        source_root_path,
        source_root_label,
        include_source_root: root_is_package,
        package_key,
        package_date_source_path: None,
    });
}

#[cfg(test)]
fn build_source_inventory(sources: &[String]) -> SourceInventory {
    build_source_inventory_filtered(sources, None, true)
}

#[cfg(test)]
fn build_source_inventory_filtered(
    sources: &[String],
    allowed_extensions: Option<&HashSet<String>>,
    recursive: bool,
) -> SourceInventory {
    build_source_inventory_filtered_with_progress(sources, allowed_extensions, recursive, |_, _| {
        Ok(())
    })
    .expect("no-op inventory progress cannot fail")
}

fn build_source_inventory_filtered_with_progress(
    sources: &[String],
    allowed_extensions: Option<&HashSet<String>>,
    recursive: bool,
    mut progress: impl FnMut(&SourceInventory, &Path) -> Result<(), String>,
) -> Result<SourceInventory, String> {
    let mut inventory = SourceInventory::default();
    for source in sources {
        let path = PathBuf::from(source);
        progress(&inventory, &path)?;
        if path.is_file() {
            let relative = PathBuf::from(path.file_name().unwrap_or_default());
            let root = path.parent().unwrap_or_else(|| Path::new("."));
            inventory_candidate(
                &mut inventory,
                &path,
                &relative,
                root,
                false,
                allowed_extensions,
            );
            progress(&inventory, &path)?;
        } else if path.is_dir() {
            let root_is_package = package_component(&path);
            let walker = if recursive {
                WalkDir::new(&path)
            } else {
                WalkDir::new(&path).max_depth(1)
            };
            for result in walker.follow_links(false) {
                match result {
                    Ok(entry) if entry.file_type().is_file() => {
                        let relative = entry.path().strip_prefix(&path).unwrap_or(entry.path());
                        inventory_candidate(
                            &mut inventory,
                            entry.path(),
                            relative,
                            &path,
                            root_is_package,
                            allowed_extensions,
                        );
                        progress(&inventory, entry.path())?;
                    }
                    Ok(entry) => progress(&inventory, entry.path())?,
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
    Ok(inventory)
}

fn classify_global_duplicate(
    settings: &DeviceSettings,
    candidates: &[&MediaItem],
    expected_size: u64,
    source_hash: &str,
) -> (String, Option<String>, Option<String>, Option<String>) {
    // Stored size observations are useful lookup hints, but they are not
    // authoritative and older compatible records may not contain one. The
    // fresh local snapshot and SHA-256 check below decide whether a copy is
    // safe to trust.
    let candidates = candidates
        .iter()
        .copied()
        .filter(|item| item.sha256 == source_hash)
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return ("new".into(), None, None, None);
    }
    let master = PathBuf::from(&settings.master_bin_path);
    let mut failures = Vec::new();
    for item in candidates {
        let candidate = match fs_safe::resolve_existing(&master, &item.relative_path) {
            Ok(path) if path.is_file() => path,
            Ok(_) => {
                failures.push(format!("{} is not a local file", item.relative_path));
                continue;
            }
            Err(error) => {
                failures.push(format!("{} is unavailable: {error}", item.relative_path));
                continue;
            }
        };
        let before = match fs_safe::snapshot(&candidate) {
            Ok(value) if value.length == expected_size => value,
            Ok(_) => {
                failures.push(format!("{} has a different size", item.relative_path));
                continue;
            }
            Err(error) => {
                failures.push(format!("{} cannot be read: {error}", item.relative_path));
                continue;
            }
        };
        let hash = sha256(&candidate);
        let after = fs_safe::snapshot(&candidate);
        if hash.as_deref() == Ok(source_hash) && after.as_ref() == Ok(&before) {
            return (
                "already-imported".into(),
                Some(item.id.clone()),
                Some(item.relative_path.clone()),
                Some("A local Master Bin copy was independently SHA-256 verified".into()),
            );
        }
        failures.push(format!(
            "{} failed fresh integrity verification",
            item.relative_path
        ));
    }
    (
        "attention".into(),
        None,
        None,
        Some(format!(
            "Catalogue content match is not a trustworthy local copy: {}. The incoming source was not skipped.",
            failures.join("; ")
        )),
    )
}

fn approved_source_snapshot_matches(path: &Path, approved: &ApprovedIngestFile) -> bool {
    let Ok(snapshot) = fs_safe::snapshot(path) else {
        return false;
    };
    if snapshot.length != approved.source_size
        || snapshot.modified_millis != approved.source_modified_millis
    {
        return false;
    }
    if let Some(expected_volume) = approved.source_volume_identity.as_deref() {
        if volume_identity_for_path(path).ok().flatten().as_deref() != Some(expected_volume) {
            return false;
        }
    }
    fs_safe::snapshot(path).as_ref() == Ok(&snapshot)
}

fn create_approved_inventory(
    app: &AppHandle,
    state: &NativeState,
    operation_id: &str,
    sources: Vec<String>,
    date_override: Option<String>,
    sd_grabber: bool,
) -> Result<SourceInventory, String> {
    check_inventory_cancelled(state, operation_id)?;
    let settings = load_settings_file(&app)?;
    if sd_grabber && !settings.sd_grabber_enabled {
        return Err("Enable SD Card Grabber in Settings before scanning a card".into());
    }
    let master = PathBuf::from(&settings.master_bin_path);
    let master_canonical = master
        .canonicalize()
        .map_err(|error| format!("Cannot resolve Master Bin: {error}"))?;
    let config_root = settings_path(app)?
        .parent()
        .ok_or("Invalid application configuration path")?
        .canonicalize()
        .map_err(|error| format!("Cannot resolve application working directory: {error}"))?;
    let staging = fs_safe::staging_root(&master, &settings.device_id)?;
    for source in &sources {
        let canonical = Path::new(source)
            .canonicalize()
            .map_err(|error| format!("Cannot resolve selected source {source}: {error}"))?;
        if paths_overlap(&master_canonical, &canonical)
            || paths_overlap(&staging, &canonical)
            || paths_overlap(&config_root, &canonical)
        {
            return Err(format!(
                "Selected source {} overlaps the Master Bin or a protected PIMP working directory",
                canonical.display()
            ));
        }
    }
    let batch_timestamp = Local::now();
    let allowed = settings
        .sd_grabber_extensions
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    update_inventory_operation(state, operation_id, |operation| {
        operation.state = "running".into();
        operation.stage = "Enumerating".into();
        operation.current_item = None;
        operation.completed_items = 0;
        operation.total_items = None;
        operation.bytes_completed = 0;
        operation.bytes_total = None;
    });
    let mut inventory = build_source_inventory_filtered_with_progress(
        &sources,
        sd_grabber.then_some(&allowed),
        !sd_grabber || settings.sd_grabber_search_subfolders,
        |progress, path| {
            check_inventory_cancelled(state, operation_id)?;
            update_inventory_operation(state, operation_id, |operation| {
                operation.completed_items = progress.total_file_count;
                operation.current_item = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string());
            });
            Ok(())
        },
    )?;
    check_inventory_cancelled(state, operation_id)?;
    update_inventory_operation(state, operation_id, |operation| {
        operation.stage = "Inspecting files".into();
        operation.completed_items = 0;
        operation.total_items = Some(inventory.files.len());
        operation.bytes_total = Some(inventory.files.iter().map(|file| file.size_bytes).sum());
    });
    let catalogue = load_catalogue_file(&settings)?;
    let mut hash_index = HashMap::<String, Vec<&MediaItem>>::new();
    for item in &catalogue.media {
        hash_index
            .entry(item.sha256.clone())
            .or_default()
            .push(item);
    }
    let mut snapshots = Vec::with_capacity(inventory.files.len());
    let mut volume_identities = HashMap::<String, Option<String>>::new();
    for (index, file) in inventory.files.iter_mut().enumerate() {
        check_inventory_cancelled(state, operation_id)?;
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
        let source = PathBuf::from(&file.source_path);
        let snapshot = fs_safe::snapshot(&source)?;
        let volume_identity = volume_identities
            .entry(file.source_root_path.clone())
            .or_insert_with(|| volume_identity_for_path(&source).ok().flatten())
            .clone();
        snapshots.push((snapshot, volume_identity));
        update_inventory_operation(state, operation_id, |operation| {
            operation.completed_items = index + 1;
            operation.current_item = Some(filename);
        });
    }

    update_inventory_operation(state, operation_id, |operation| {
        operation.stage = "Calculating checksums".into();
        operation.completed_items = 0;
        operation.bytes_completed = 0;
    });
    let mut source_hashes = Vec::with_capacity(inventory.files.len());
    let mut bytes_read = 0_u64;
    for (index, file) in inventory.files.iter().enumerate() {
        check_inventory_cancelled(state, operation_id)?;
        let source = PathBuf::from(&file.source_path);
        let hash = sha256_inventory(&source, state, operation_id, &mut bytes_read)?;
        source_hashes.push(hash);
        update_inventory_operation(state, operation_id, |operation| {
            operation.completed_items = index + 1;
            operation.current_item = source
                .file_name()
                .map(|name| name.to_string_lossy().to_string());
        });
    }

    update_inventory_operation(state, operation_id, |operation| {
        operation.stage = "Checking duplicates".into();
        operation.completed_items = 0;
    });
    let mut approved_files = Vec::new();
    let mut selection_hashes = HashMap::<String, String>::new();
    for (index, file) in inventory.files.iter_mut().enumerate() {
        check_inventory_cancelled(state, operation_id)?;
        let (snapshot, source_volume_identity) = &snapshots[index];
        let hash = &source_hashes[index];
        let source = PathBuf::from(&file.source_path);
        let mut existing_media_id = None;
        if fs_safe::snapshot(&source)? != *snapshot {
            file.disposition = "attention".into();
            file.detail =
                Some("Source changed while the approved inventory was being built".into());
        } else if let Some(relative_path) = selection_hashes.get(hash) {
            file.disposition = "already-imported".into();
            file.existing_relative_path = Some(relative_path.clone());
            file.detail = Some("Duplicate recording within this approved selection".into());
        } else {
            let candidates = hash_index.get(hash).map(Vec::as_slice).unwrap_or(&[]);
            let (disposition, media_id, relative_path, detail) =
                classify_global_duplicate(&settings, candidates, snapshot.length, hash);
            file.disposition = disposition;
            file.existing_relative_path = relative_path;
            file.detail = detail;
            existing_media_id = media_id;
            if file.disposition == "new" {
                selection_hashes.insert(hash.clone(), file.planned_relative_path.clone());
            }
        }
        if file.disposition != "already-imported" {
            let destination = fs_safe::resolve_for_create(&master, &file.planned_relative_path)?;
            if destination.exists() {
                let reason = "A different file already occupies the planned destination";
                inventory.conflicts.push(InventoryConflict {
                    source_path: file.source_path.clone(),
                    destination_path: file.planned_relative_path.clone(),
                    reason: reason.into(),
                });
                file.disposition = "attention".into();
                file.detail = Some(format!(
                    "{reason}. Enter a new filename and approve the revised plan."
                ));
            }
        }
        let disposition = file.disposition.clone();
        approved_files.push(ApprovedIngestFile {
            inventory: file.clone(),
            source_size: snapshot.length,
            source_modified_millis: snapshot.modified_millis,
            planning_modified_millis: file
                .package_date_source_path
                .as_deref()
                .and_then(|path| fs_safe::snapshot(Path::new(path)).ok())
                .and_then(|snapshot| snapshot.modified_millis)
                .or(snapshot.modified_millis),
            source_sha256: Some(hash.clone()),
            source_volume_identity: source_volume_identity.clone(),
            existing_media_id,
        });
        update_inventory_operation(state, operation_id, |operation| {
            operation.completed_items = index + 1;
            operation.current_item = source
                .file_name()
                .map(|name| name.to_string_lossy().to_string());
            match disposition.as_str() {
                "new" => operation.counts.new_files += 1,
                "already-imported" => operation.counts.duplicate_files += 1,
                "attention" => operation.counts.attention_files += 1,
                _ => operation.counts.failed_files += 1,
            }
        });
    }
    inventory.copy_file_count = inventory
        .files
        .iter()
        .filter(|file| file.disposition == "new")
        .count();
    inventory.copy_bytes = inventory
        .files
        .iter()
        .filter(|file| file.disposition == "new")
        .map(|file| file.size_bytes)
        .sum();
    inventory.already_imported_file_count = inventory
        .files
        .iter()
        .filter(|file| file.disposition == "already-imported")
        .count();
    inventory.already_imported_bytes = inventory
        .files
        .iter()
        .filter(|file| file.disposition == "already-imported")
        .map(|file| file.size_bytes)
        .sum();
    inventory.attention_file_count = inventory
        .files
        .iter()
        .filter(|file| file.disposition == "attention")
        .count();
    inventory.attention_bytes = inventory
        .files
        .iter()
        .filter(|file| file.disposition == "attention")
        .map(|file| file.size_bytes)
        .sum();
    inventory.plan_id = Uuid::new_v4().to_string();
    let _plan_guard = state.approved_plan_store.lock().unwrap();
    save_approved_plan(
        app,
        &ApprovedIngestPlan {
            schema_version: 1,
            id: inventory.plan_id.clone(),
            lineage_id: inventory.plan_id.clone(),
            revision: 0,
            created_at: Utc::now().to_rfc3339(),
            batch_timestamp: batch_timestamp.to_rfc3339(),
            master_bin_path: settings.master_bin_path,
            sources,
            date_override,
            files: approved_files,
            inventory: inventory.clone(),
            sd_grabber,
        },
    )?;
    update_inventory_operation(state, operation_id, |operation| {
        operation.state = "completed".into();
        operation.stage = "Ready for review".into();
        operation.current_item = None;
        operation.cancellable = false;
        operation.completed_items = inventory.files.len();
        operation.total_items = Some(inventory.files.len());
        operation.bytes_completed = operation.bytes_total.unwrap_or(0);
    });
    Ok(inventory)
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    fs_safe::within(left, right) || fs_safe::within(right, left)
}

fn begin_inventory_operation(
    state: &NativeState,
    operation_id: &str,
    title: &str,
) -> Result<(), String> {
    Uuid::parse_str(operation_id).map_err(|_| "Inventory operation ID must be a UUID")?;
    let now = Utc::now().to_rfc3339();
    let previous = state
        .inventory_status
        .lock()
        .unwrap()
        .replace(BackgroundOperation {
            id: operation_id.into(),
            kind: "source-inventory".into(),
            title: title.into(),
            state: "queued".into(),
            stage: "Waiting for the inventory worker".into(),
            current_item: None,
            completed_items: 0,
            total_items: None,
            bytes_completed: 0,
            bytes_total: None,
            counts: OperationCounts::default(),
            cancellable: true,
            pausable: false,
            retryable: false,
            error: None,
            started_at: now.clone(),
            updated_at: now,
        });
    if let Some(previous) = previous.filter(|operation| {
        matches!(
            operation.state.as_str(),
            "queued" | "running" | "cancelling"
        )
    }) {
        state
            .inventory_cancelled
            .lock()
            .unwrap()
            .insert(previous.id);
    }
    state
        .inventory_cancelled
        .lock()
        .unwrap()
        .remove(operation_id);
    Ok(())
}

async fn run_inventory_command(
    app: AppHandle,
    operation_id: String,
    sources: Vec<String>,
    date_override: Option<String>,
    sd_grabber: bool,
) -> Result<SourceInventory, String> {
    let started = Instant::now();
    {
        let state = app.state::<NativeState>();
        begin_inventory_operation(
            &state,
            &operation_id,
            if sd_grabber {
                "SD-card inventory and duplicate verification"
            } else {
                "Source inventory and duplicate verification"
            },
        )?;
    }
    let worker_app = app.clone();
    let worker_operation_id = operation_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = worker_app.state::<NativeState>();
        let _serial = state.inventory_worker.lock().unwrap();
        check_inventory_cancelled(&state, &worker_operation_id)?;
        diagnostic_log(
            &worker_app,
            "INFO",
            "inventory",
            &format!("{} started", worker_operation_id),
        );
        create_approved_inventory(
            &worker_app,
            &state,
            &worker_operation_id,
            sources,
            date_override,
            sd_grabber,
        )
    })
    .await
    .map_err(|error| format!("Inventory worker failed: {error}"))?;
    if let Err(error) = &result {
        let state = app.state::<NativeState>();
        let cancelled = inventory_was_cancelled(&state, &operation_id);
        update_inventory_operation(&state, &operation_id, |operation| {
            operation.state = if cancelled { "cancelled" } else { "failed" }.into();
            operation.stage = if cancelled {
                "Cancelled safely"
            } else {
                "Inventory failed"
            }
            .into();
            operation.error = (!cancelled).then(|| error.clone());
            operation.cancellable = false;
            operation.retryable = !cancelled;
        });
        diagnostic_log(&app, "ERROR", "inventory", error);
    } else if let Ok(inventory) = &result {
        let seconds = started.elapsed().as_secs_f64().max(0.001);
        diagnostic_log(
            &app,
            "INFO",
            "inventory",
            &format!(
                "{} completed: {} files, {} source bytes read in {:.3}s ({:.1} MiB/s)",
                operation_id,
                inventory.total_file_count,
                inventory.total_bytes,
                seconds,
                inventory.total_bytes as f64 / 1024.0 / 1024.0 / seconds
            ),
        );
    }
    app.state::<NativeState>()
        .inventory_cancelled
        .lock()
        .unwrap()
        .remove(&operation_id);
    result
}

#[tauri::command]
async fn inventory_sources(
    app: AppHandle,
    operation_id: String,
    sources: Vec<String>,
    date_override: Option<String>,
) -> Result<SourceInventory, String> {
    run_inventory_command(app, operation_id, sources, date_override, false).await
}

#[tauri::command]
async fn inventory_sd_card(
    app: AppHandle,
    operation_id: String,
    sources: Vec<String>,
    date_override: Option<String>,
) -> Result<SourceInventory, String> {
    run_inventory_command(app, operation_id, sources, date_override, true).await
}

#[cfg(target_os = "windows")]
fn removable_volumes() -> Vec<RemovableVolume> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };
    use windows_sys::Win32::System::WindowsProgramming::DRIVE_REMOVABLE;
    let mut volumes = Vec::new();
    let drives = unsafe { GetLogicalDrives() };
    for index in 0..26_u32 {
        if drives & (1 << index) == 0 {
            continue;
        }
        let root = format!("{}:\\", (b'A' + index as u8) as char);
        let wide = std::ffi::OsStr::new(&root)
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        if unsafe { GetDriveTypeW(wide.as_ptr()) } != DRIVE_REMOVABLE {
            continue;
        }
        let mut label = vec![0_u16; 261];
        let mut filesystem = vec![0_u16; 64];
        let mut serial = 0_u32;
        let information_ok = unsafe {
            GetVolumeInformationW(
                wide.as_ptr(),
                label.as_mut_ptr(),
                label.len() as u32,
                &mut serial,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                filesystem.as_mut_ptr(),
                filesystem.len() as u32,
            )
        } != 0;
        if !information_ok {
            continue;
        }
        let mut available = 0_u64;
        let mut capacity = 0_u64;
        if unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut available,
                &mut capacity,
                std::ptr::null_mut(),
            )
        } == 0
        {
            continue;
        }
        let label_end = label
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(label.len());
        let filesystem_end = filesystem
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(filesystem.len());
        let filesystem = String::from_utf16_lossy(&filesystem[..filesystem_end]);
        volumes.push(RemovableVolume {
            root_path: root.clone(),
            label: {
                let value = String::from_utf16_lossy(&label[..label_end]);
                if value.trim().is_empty() {
                    "Removable media".into()
                } else {
                    value
                }
            },
            identity: format!("volume:{serial:08x}:{filesystem}:{capacity}"),
            capacity_bytes: capacity,
            available_bytes: available,
        });
    }
    volumes
}

#[cfg(not(target_os = "windows"))]
fn removable_volumes() -> Vec<RemovableVolume> {
    Vec::new()
}

#[tauri::command]
fn list_removable_volumes() -> Vec<RemovableVolume> {
    removable_volumes()
}

#[cfg(target_os = "windows")]
fn volume_identity_for_path(path: &Path) -> Result<Option<String>, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetVolumeInformationW, GetVolumePathNameW,
    };
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut root = vec![0_u16; 261];
    if unsafe { GetVolumePathNameW(wide.as_ptr(), root.as_mut_ptr(), root.len() as u32) } == 0 {
        return Err(format!(
            "Cannot identify source volume for {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    let mut serial = 0_u32;
    let mut filesystem = vec![0_u16; 64];
    if unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            std::ptr::null_mut(),
            0,
            &mut serial,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            filesystem.as_mut_ptr(),
            filesystem.len() as u32,
        )
    } == 0
    {
        return Err(format!(
            "Cannot read source-volume identity for {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    let mut capacity = 0_u64;
    if unsafe {
        GetDiskFreeSpaceExW(
            root.as_ptr(),
            std::ptr::null_mut(),
            &mut capacity,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(format!(
            "Cannot read source-volume capacity for {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    let end = filesystem
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(filesystem.len());
    Ok(Some(format!(
        "volume:{serial:08X}:{}:{capacity}",
        String::from_utf16_lossy(&filesystem[..end])
    )))
}

#[cfg(not(target_os = "windows"))]
fn volume_identity_for_path(_path: &Path) -> Result<Option<String>, String> {
    Ok(None)
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

fn inventory_was_cancelled(state: &NativeState, operation_id: &str) -> bool {
    state
        .inventory_cancelled
        .lock()
        .unwrap()
        .contains(operation_id)
}

fn update_inventory_operation(
    state: &NativeState,
    operation_id: &str,
    update: impl FnOnce(&mut BackgroundOperation),
) {
    let mut status = state.inventory_status.lock().unwrap();
    if let Some(operation) = status
        .as_mut()
        .filter(|operation| operation.id == operation_id)
    {
        update(operation);
        operation.updated_at = Utc::now().to_rfc3339();
    }
}

fn check_inventory_cancelled(state: &NativeState, operation_id: &str) -> Result<(), String> {
    if inventory_was_cancelled(state, operation_id) {
        Err("Inventory cancelled safely".into())
    } else {
        Ok(())
    }
}

fn sha256_inventory(
    path: &Path,
    state: &NativeState,
    operation_id: &str,
    total_bytes_read: &mut u64,
) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| {
        format!(
            "Cannot read source while calculating its checksum {}: {error}",
            path.display()
        )
    })?;
    let mut next_update = 0_u64;
    let hash = sha256_reader_cancellable(
        &mut file,
        || inventory_was_cancelled(state, operation_id),
        |read| {
            *total_bytes_read = total_bytes_read.saturating_add(read);
            if *total_bytes_read >= next_update {
                update_inventory_operation(state, operation_id, |operation| {
                    operation.bytes_completed = *total_bytes_read;
                    operation.current_item = path
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string());
                });
                next_update = total_bytes_read.saturating_add(8 * 1024 * 1024);
            }
        },
    )
    .map_err(|error| {
        if error == "Inventory cancelled safely" {
            error
        } else {
            format!("Source read failed for {}: {error}", path.display())
        }
    })?;
    update_inventory_operation(state, operation_id, |operation| {
        operation.bytes_completed = *total_bytes_read;
    });
    Ok(hash)
}

fn sha256_reader_cancellable(
    reader: &mut impl Read,
    mut cancelled: impl FnMut() -> bool,
    mut progress: impl FnMut(u64),
) -> Result<String, String> {
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        if cancelled() {
            return Err("Inventory cancelled safely".into());
        }
        let read = reader
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
        progress(read as u64);
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[tauri::command]
fn cancel_inventory(state: State<'_, NativeState>, operation_id: String) -> Result<(), String> {
    let active = state
        .inventory_status
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|operation| {
            operation.id == operation_id
                && matches!(
                    operation.state.as_str(),
                    "queued" | "running" | "cancelling"
                )
        });
    if !active {
        return Ok(());
    }
    state
        .inventory_cancelled
        .lock()
        .unwrap()
        .insert(operation_id.clone());
    update_inventory_operation(&state, &operation_id, |operation| {
        if matches!(operation.state.as_str(), "queued" | "running") {
            operation.state = "cancelling".into();
            operation.stage = "Stopping after the current read".into();
            operation.cancellable = false;
        }
    });
    Ok(())
}

#[tauri::command]
fn list_background_operations(
    app: AppHandle,
    state: State<'_, NativeState>,
) -> Vec<BackgroundOperation> {
    let mut operations = Vec::new();
    if let Some(inventory) = state.inventory_status.lock().unwrap().clone() {
        operations.push(inventory);
    }
    operations.extend(
        state
            .playback_proxy_status
            .lock()
            .unwrap()
            .values()
            .cloned(),
    );
    operations.extend(
        state
            .project_inventory_status
            .lock()
            .unwrap()
            .values()
            .cloned(),
    );
    let discovery = state.discovery_status.lock().unwrap().clone();
    if discovery.running {
        operations.push(BackgroundOperation {
            id: "library-discovery".into(),
            kind: "library-scan".into(),
            title: if discovery.full {
                "Full Library integrity verification".into()
            } else {
                "Library scan".into()
            },
            state: "running".into(),
            stage: "Inspecting Library files".into(),
            current_item: None,
            completed_items: discovery.files_processed,
            total_items: Some(discovery.total_files),
            bytes_completed: 0,
            bytes_total: None,
            counts: OperationCounts {
                new_files: discovery.registered,
                attention_files: discovery.errors,
                ..Default::default()
            },
            cancellable: false,
            pausable: false,
            retryable: false,
            error: None,
            started_at: discovery.started_at.unwrap_or_default(),
            updated_at: Utc::now().to_rfc3339(),
        });
    }
    let enrichment = state.enrichment_status.lock().unwrap().clone();
    let enrichment_active = *state.enrichment_worker_active.lock().unwrap();
    if let Some(enrichment) = enrichment {
        if enrichment.state != "completed" || enrichment_active {
            operations.push(enrichment);
        }
    }
    let backup_jobs = {
        let _guard = state.backup_job_store.lock().unwrap();
        load_backup_jobs_unlocked(&app).unwrap_or_default()
    };
    for job in backup_jobs
        .into_iter()
        .filter(|job| matches!(job.state.as_str(), "prepared" | "copying" | "verifying"))
    {
        operations.push(BackgroundOperation {
            id: format!("backup:{}", job.id),
            kind: "backup".into(),
            title: format!("Project backup: {}", job.project_name),
            state: "running".into(),
            stage: job.state.clone(),
            current_item: job.current_file,
            completed_items: job.files_completed,
            total_items: Some(job.manifest.source_file_count),
            bytes_completed: job.bytes_completed,
            bytes_total: Some(job.manifest.source_bytes),
            counts: OperationCounts::default(),
            cancellable: false,
            pausable: false,
            retryable: false,
            error: job.error,
            started_at: job.created_at,
            updated_at: job.updated_at,
        });
    }
    operations
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
        Some("original-file-dates") => modified
            .as_ref()
            .map(|value| (value.date_naive(), value.time()))
            .unwrap_or((batch_timestamp.date_naive(), batch_timestamp.time())),
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

fn verify_approved_source_hash(actual: &str, approved: &str) -> Result<(), String> {
    if actual == approved {
        Ok(())
    } else {
        Err(
            "Source bytes no longer match the approved inventory SHA-256; staged copy was rejected"
                .into(),
        )
    }
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
    approved_hash: &str,
    source_volume_identity: Option<&str>,
) -> Result<VerifiedCopy, String> {
    if let Some(expected_volume) = source_volume_identity {
        if volume_identity_for_path(source)?.as_deref() != Some(expected_volume) {
            return Err("The source volume changed after approval; rescan the card".into());
        }
    }
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
        verify_approved_source_hash(&copied_source_hash, approved_hash)?;
        let after = fs_safe::snapshot(source)?;
        if before != after || copied != before.length {
            return Err("Source changed during transfer; staged copy was rejected".into());
        }
        let post_copy_source_hash = sha256(source)?;
        if post_copy_source_hash != approved_hash {
            return Err("Source content changed during transfer; staged copy was rejected".into());
        }
        if let Some(expected_volume) = source_volume_identity {
            if volume_identity_for_path(source)?.as_deref() != Some(expected_volume) {
                return Err(
                    "The source volume changed during transfer; staged copy was rejected".into(),
                );
            }
        }
        if sha256(&temp)? != approved_hash {
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

fn complete_duplicate_job_file(
    app: &AppHandle,
    state: &NativeState,
    job_id: &str,
    file_id: &str,
    media_id: &str,
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
        file.media_id = media_id.into();
        file.state = "completed".into();
        file.bytes_transferred = 0;
        file.error = None;
        job.files_completed = job
            .files
            .iter()
            .filter(|file| file.state == "completed")
            .count();
        job.bytes_transferred = job.files.iter().map(|file| file.bytes_transferred).sum();
        job.updated_at = Utc::now().to_rfc3339();
        Ok(())
    })
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
        if planned.duplicate_existing_relative_path.is_some() {
            let expected_hash = planned
                .approved_sha256
                .as_deref()
                .ok_or("Approved duplicate is missing its source hash")?;
            let source_before = fs_safe::snapshot(&source);
            let destination_before = fs_safe::snapshot(&destination);
            let source_volume_before = volume_identity_for_path(&source).ok().flatten();
            let source_hash = sha256(&source);
            let destination_hash = sha256(&destination);
            let volume_matches = planned
                .source_volume_identity
                .as_deref()
                .is_none_or(|expected| source_volume_before.as_deref() == Some(expected));
            let trustworthy = volume_matches
                && source_before.as_ref().is_ok_and(|snapshot| {
                    snapshot.length == planned.size_bytes
                        && snapshot.modified_millis == planned.source_modified_millis
                })
                && source_hash.as_deref() == Ok(expected_hash)
                && fs_safe::snapshot(&source).as_ref() == source_before.as_ref()
                && planned
                    .source_volume_identity
                    .as_deref()
                    .is_none_or(|expected| {
                        volume_identity_for_path(&source).ok().flatten().as_deref()
                            == Some(expected)
                    })
                && destination_before
                    .as_ref()
                    .is_ok_and(|snapshot| snapshot.length == planned.size_bytes)
                && destination_hash.as_deref() == Ok(expected_hash)
                && fs_safe::snapshot(&destination).as_ref() == destination_before.as_ref();
            let existing = load_catalogue_file(&settings)?
                .media
                .into_iter()
                .find(|item| {
                    item.id == planned.media_id
                        && item.relative_path == planned.relative_path
                        && item.sha256 == expected_hash
                });
            if trustworthy && existing.is_some() {
                complete_duplicate_job_file(app, &state, job_id, &planned.id, &planned.media_id)?;
                continue;
            }
            update_job_progress(
                app,
                &state,
                job_id,
                &planned.id,
                "failed",
                0,
                Some("The previously verified duplicate is no longer trustworthy; no file was skipped".into()),
            )?;
            continue;
        }
        if destination.exists() {
            match (sha256(&source), sha256(&destination)) {
                (Ok(source_hash), Ok(destination_hash)) if source_hash == destination_hash => {
                    let catalogue = load_catalogue_file(&settings)?;
                    if let Some(existing) = catalogue.media.iter().find(|item| {
                        item.relative_path == planned.relative_path
                            && item.sha256 == destination_hash
                    }) {
                        complete_duplicate_job_file(
                            app,
                            &state,
                            job_id,
                            &planned.id,
                            &existing.id,
                        )?;
                    } else {
                        commit_job_file(
                            app,
                            &state,
                            &settings,
                            job_id,
                            &planned.id,
                            destination_hash,
                            None,
                        )?;
                        kick_enrichment_worker(app.clone());
                    }
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
            planned
                .approved_sha256
                .as_deref()
                .ok_or("Approved ingest file is missing its SHA-256")?,
            planned.source_volume_identity.as_deref(),
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
                    kick_enrichment_worker(app.clone());
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

fn active_content_reservations(jobs: &[IngestJob]) -> HashMap<String, (String, String)> {
    jobs.iter()
        .filter(|job| !matches!(job.state.as_str(), "completed" | "cancelled"))
        .flat_map(|job| &job.files)
        .filter(|file| !file.intentional_duplicate && file.state != "cancelled")
        .filter_map(|file| {
            file.approved_sha256.as_ref().map(|hash| {
                (
                    hash.clone(),
                    (file.relative_path.clone(), file.media_id.clone()),
                )
            })
        })
        .collect()
}

fn initial_batch_metadata(kind: &str, batch_tags: &[String]) -> ProvisionalMetadata {
    if matches!(kind, "video" | "audio" | "image") && !batch_tags.is_empty() {
        ProvisionalMetadata {
            tags: batch_tags.to_vec(),
            edited_fields: vec!["tags".into()],
            ..Default::default()
        }
    } else {
        ProvisionalMetadata::default()
    }
}

#[tauri::command]
fn start_ingest(
    app: AppHandle,
    state: State<'_, NativeState>,
    request: IngestRequest,
) -> Result<IngestJob, String> {
    let _operation = begin_critical_operation(&state.operations, "ingest start")?;
    let _plan_guard = state.approved_plan_store.lock().unwrap();
    let settings = load_settings_file(&app)?;
    let master = PathBuf::from(&settings.master_bin_path);
    let plan_id = request
        .approved_plan_id
        .as_deref()
        .ok_or("Scan and review the source before starting ingest")?;
    let plan = current_approved_plan(&app, plan_id)?;
    if plan.id != plan_id {
        return Err("This approved ingest plan was superseded; review the latest plan".into());
    }
    let plan_lineage = approved_plan_lineage(&plan).to_owned();
    if plan.schema_version != 1
        || plan.master_bin_path != settings.master_bin_path
        || plan.sources != request.sources
        || plan.date_override != request.date_override
    {
        return Err("The approved ingest plan no longer matches this request; scan again".into());
    }
    if !request.renamed_destinations.is_empty() {
        return Err("Destination renames require a new reviewed ingest plan".into());
    }
    let inventory = plan.inventory.clone();
    let unsupported = inventory
        .files
        .iter()
        .filter(|file| !file.analysis_supported)
        .map(|file| file.source_path.clone())
        .collect();
    let batch_timestamp = DateTime::parse_from_rfc3339(&plan.batch_timestamp)
        .map_err(|_| "Approved ingest plan has an invalid timestamp")?
        .with_timezone(&Local);
    let existing_jobs = snapshot_jobs(&app, &state)?;
    let reserved = active_destination_reservations(&existing_jobs);
    let reserved_content = active_content_reservations(&existing_jobs);
    let mut planned_paths = HashSet::new();
    let mut files = Vec::new();
    let batch_tags = normalized(&request.batch_tags);
    let media_ids_by_relative = plan
        .files
        .iter()
        .map(|file| {
            (
                file.inventory.planned_relative_path.clone(),
                Uuid::new_v4().to_string(),
            )
        })
        .collect::<HashMap<_, _>>();
    for approved in &plan.files {
        let inventory_file = &approved.inventory;
        let source = PathBuf::from(&inventory_file.source_path);
        let source_changed = !approved_source_snapshot_matches(&source, approved);
        let active_duplicate = approved
            .source_sha256
            .as_ref()
            .and_then(|hash| reserved_content.get(hash));
        let intentional_duplicate = request.allow_duplicate_copies
            && (inventory_file.disposition == "already-imported" || active_duplicate.is_some());
        let is_duplicate = (inventory_file.disposition == "already-imported"
            || active_duplicate.is_some())
            && !intentional_duplicate;
        let relative_path = if is_duplicate {
            active_duplicate
                .map(|value| value.0.clone())
                .or_else(|| inventory_file.existing_relative_path.clone())
                .ok_or("Approved duplicate is missing its verified destination")?
        } else {
            inventory_file.planned_relative_path.clone()
        };
        let destination = fs_safe::resolve_for_create(&master, &relative_path)?;
        let destination_path = destination.to_string_lossy().to_string();
        let collision = !is_duplicate
            && (reserved.contains(&destination_path)
                || !planned_paths.insert(destination_path.clone()));
        let attention =
            inventory_file.disposition == "attention" && !request.allow_unverified_reimport;
        let failed = collision || source_changed || attention;
        let media_id = if is_duplicate {
            active_duplicate
                .map(|value| value.1.clone())
                .or_else(|| approved.existing_media_id.clone())
                .or_else(|| media_ids_by_relative.get(&relative_path).cloned())
                .ok_or("Approved duplicate has no stable media identity")?
        } else {
            media_ids_by_relative
                .get(&inventory_file.planned_relative_path)
                .cloned()
                .unwrap_or_else(|| Uuid::new_v4().to_string())
        };
        files.push(IngestJobFile {
            id: Uuid::new_v4().to_string(),
            media_id,
            source_path: source.to_string_lossy().to_string(),
            source_relative_path: inventory_file.source_relative_path.clone(),
            destination_path,
            relative_path: relative_path.clone(),
            original_filename: source.file_name().unwrap().to_string_lossy().to_string(),
            size_bytes: inventory_file.size_bytes,
            bytes_transferred: 0,
            state: if failed { "failed" } else { "queued" }.into(),
            error: if source_changed {
                Some("Source changed or disappeared after the approved scan; scan again".into())
            } else if attention {
                inventory_file.detail.clone().or_else(|| {
                    Some("This file requires review before it can be transferred".into())
                })
            } else if collision {
                Some("Another queued ingest already reserves this destination".into())
            } else {
                None
            },
            metadata: initial_batch_metadata(&inventory_file.kind, &batch_tags),
            pending_metadata: None,
            metadata_error: None,
            source_modified_millis: approved.source_modified_millis,
            approved_sha256: approved.source_sha256.clone(),
            source_volume_identity: approved.source_volume_identity.clone(),
            duplicate_existing_relative_path: is_duplicate.then_some(relative_path.clone()),
            intentional_duplicate,
        });
    }
    let now = Utc::now().to_rfc3339();
    let bytes_total = files
        .iter()
        .filter(|file| file.duplicate_existing_relative_path.is_none() && file.state == "queued")
        .map(|file| file.size_bytes)
        .sum();
    let job = IngestJob {
        schema_version: 3,
        id: request.job_id,
        approved_plan_id: Some(plan_id.into()),
        approved_plan_lineage_id: Some(plan_lineage.clone()),
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
        bytes_total,
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
        if jobs.iter().any(|existing| {
            existing.approved_plan_lineage_id.as_deref() == Some(plan_lineage.as_str())
                || (existing.approved_plan_lineage_id.is_none()
                    && existing.approved_plan_id.as_deref() == Some(plan_id))
        })
        {
            return Err("This approved ingest plan has already been used".into());
        }
        jobs.push(job.clone());
        Ok(())
    })?;
    if let Err(error) = remove_approved_plan_lineage(&app, &plan_lineage) {
        diagnostic_log(
            &app,
            "WARN",
            "approved-plan-cleanup",
            &format!("Ingest started, but consumed plan cleanup was incomplete: {error}"),
        );
    }
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

#[tauri::command]
fn clear_completed_ingest_jobs(
    app: AppHandle,
    state: State<'_, NativeState>,
    job_id: Option<String>,
) -> Result<Vec<IngestJob>, String> {
    let _guard = state.job_store.lock().unwrap();
    let settings = load_settings_file(&app)?;
    let recovery_job_ids = recovery_job_ids(&settings)?;
    let mut jobs = load_jobs_unlocked(&app)?;
    clear_finalized_jobs(&mut jobs, job_id.as_deref(), &recovery_job_ids)?;
    write_json(&jobs_path(&app)?, &jobs)?;
    let _ = app.emit("ingest-jobs-changed", &jobs);
    Ok(jobs)
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
    let _operation = begin_critical_operation(&state.operations, "ingest resume")?;
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
    kick_ingest_worker(app);
    Ok(())
}

#[tauri::command]
fn retry_ingest(
    app: AppHandle,
    state: State<'_, NativeState>,
    job_id: String,
) -> Result<(), String> {
    let _operation = begin_critical_operation(&state.operations, "ingest retry")?;
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
        let pending =
            state
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
        let Some(project) = catalogue
            .projects
            .iter()
            .find(|project| project.id == project_id)
        else {
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
    settings
        .project_paths
        .insert(project.id.clone(), directory.to_string_lossy().into_owned());
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
            if &item.notes != notes {
                events.push(CatalogueEvent::MediaNotesSet {
                    media_id: item.id.clone(),
                    value: notes.clone(),
                });
            }
        }
        if let Some(favourite) = update.favourite {
            if item.favourite != favourite {
                events.push(CatalogueEvent::MediaFavouriteSet {
                    media_id: item.id.clone(),
                    value: favourite,
                });
            }
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
    let recovered_id =
        project_identity_for_directory(&sync_path, &settings, &directory, manifest_id.as_deref())?;
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
    if !catalogue
        .projects
        .iter()
        .any(|project| project.id == project_id)
    {
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
    let lower_path = relative_path
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();
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
    inventory_project_directory_with_limits(project_id, project_name, directory, 100_000, 64)
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
        total
            .checked_add(item.size_bytes)
            .ok_or_else(|| "Project inventory size exceeds the supported range".to_string())
    })?;
    Ok(ProjectInventory {
        project_id: project_id.into(),
        root_path: directory.to_string_lossy().into_owned(),
        item_count: items.len(),
        size_bytes,
        main_premiere_project,
        scanned_at: Utc::now().to_rfc3339(),
        items,
        premiere_index: None,
    })
}

fn premiere_index_path(app: &AppHandle, project_id: &str) -> Result<PathBuf, String> {
    if project_id.is_empty()
        || !project_id
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'))
    {
        return Err("Project identifier is not safe for local index storage".into());
    }
    Ok(settings_path(app)?
        .parent()
        .ok_or("Application settings path has no parent")?
        .join("premiere-indexes")
        .join(format!("{project_id}.json")))
}

fn associate_premiere_media(
    index: &mut PremiereProjectIndex,
    project_directory: &Path,
    settings: &DeviceSettings,
    catalogue: &Catalogue,
) {
    let Ok(master) = Path::new(&settings.master_bin_path).canonicalize() else {
        return;
    };
    let known = catalogue
        .media
        .iter()
        .filter_map(|media| {
            fs_safe::resolve_existing(&master, &media.relative_path)
                .ok()
                .and_then(|path| path.canonicalize().ok())
                .map(|path| (path, media.id.clone()))
        })
        .collect::<HashMap<_, _>>();
    for reference in &mut index.media_references {
        let path = PathBuf::from(&reference.path);
        let candidate = if path.is_absolute() {
            path
        } else {
            project_directory.join(path)
        };
        if let Ok(canonical) = candidate.canonicalize() {
            reference.media_id = known.get(&canonical).cloned();
        }
    }
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
    let operation_id = format!("project-index:{project_id}");
    let now = Utc::now().to_rfc3339();
    app.state::<NativeState>()
        .project_inventory_status
        .lock()
        .unwrap()
        .insert(
            operation_id.clone(),
            BackgroundOperation {
                id: operation_id.clone(),
                kind: "premiere-index".into(),
                title: format!("Premiere project index: {project_name}"),
                state: "running".into(),
                stage: "Reading project folder".into(),
                current_item: None,
                completed_items: 0,
                total_items: None,
                bytes_completed: 0,
                bytes_total: None,
                counts: OperationCounts::default(),
                cancellable: false,
                pausable: false,
                retryable: false,
                error: None,
                started_at: now.clone(),
                updated_at: now,
            },
        );

    let result: Result<ProjectInventory, String> = async {
        let directory = resolve_known_project_directory(&settings, &project_id)?;
        let task_directory = directory.clone();
        let task_project_id = project_id.clone();
        let task_operation_id = operation_id.clone();
        let task_app = app.clone();
        let (mut inventory, parsed_index) = tauri::async_runtime::spawn_blocking(move || {
            let inventory = inventory_project_directory(
                &task_project_id,
                &project_name,
                &task_directory,
            )?;
            update_project_inventory_operation(
                &task_app.state::<NativeState>(),
                &task_operation_id,
                |operation| {
                    operation.completed_items = inventory.items.len();
                    operation.stage = if inventory.main_premiere_project.is_some() {
                        "Reading saved Premiere project".into()
                    } else {
                        "Project folder indexed".into()
                    };
                    operation.current_item = inventory.main_premiere_project.clone();
                },
            );
            let parsed = inventory.main_premiere_project.as_ref().map(|relative| {
                fs_safe::resolve_existing(&task_directory, relative).and_then(|path| {
                    premiere_index::index_project(&path, relative, Utc::now().to_rfc3339())
                })
            });
            Ok::<_, String>((inventory, parsed))
        })
        .await
        .map_err(|error| format!("Project inventory task failed: {error}"))??;
        if let Some(parsed_index) = parsed_index {
            update_project_inventory_operation(
                &app.state::<NativeState>(),
                &operation_id,
                |operation| {
                    operation.stage = "Resolving saved media references".into();
                },
            );
            let relative = inventory.main_premiere_project.clone().unwrap_or_default();
            match parsed_index {
                Ok(mut index) => {
                    let path = premiere_index_path(&app, &project_id)?;
                    if path.is_file() {
                        if let Ok(previous) = read_json::<PremiereProjectIndex>(&path) {
                            premiere_index::carry_forward_historical_evidence(
                                &mut index, &previous,
                            );
                        }
                    }
                    associate_premiere_media(&mut index, &directory, &settings, &catalogue);
                    write_json(&path, &index)?;
                    inventory.premiere_index = Some(index);
                }
                Err(error) => {
                    inventory.premiere_index = Some(PremiereProjectIndex {
                        parser_version: premiere_index::PARSER_VERSION.into(),
                        source_relative_path: relative,
                        indexed_at: Utc::now().to_rfc3339(),
                        status: "unsupported".into(),
                        issues: vec![error],
                        ..Default::default()
                    });
                }
            }
        }
        Ok(inventory)
    }
    .await;

    if let Err(error) = &result {
        update_project_inventory_operation(
            &app.state::<NativeState>(),
            &operation_id,
            |operation| {
                operation.state = "failed".into();
                operation.stage = "Project indexing failed".into();
                operation.error = Some(error.clone());
                operation.retryable = true;
            },
        );
    } else {
        app.state::<NativeState>()
            .project_inventory_status
            .lock()
            .unwrap()
            .remove(&operation_id);
    }
    result
}

fn update_project_inventory_operation(
    state: &NativeState,
    operation_id: &str,
    update: impl FnOnce(&mut BackgroundOperation),
) {
    if let Some(operation) = state
        .project_inventory_status
        .lock()
        .unwrap()
        .get_mut(operation_id)
    {
        update(operation);
        operation.updated_at = Utc::now().to_rfc3339();
    }
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

fn evidence_matches(
    item: &MediaItem,
    device_id: &str,
    observed_size: u64,
    observed_modified_millis: Option<u64>,
    observed_sha256: Option<&str>,
    needs_review: bool,
) -> bool {
    item.integrity_by_device
        .get(device_id)
        .is_some_and(|evidence| {
            evidence.observed_size == observed_size
                && evidence.observed_modified_millis == observed_modified_millis
                && evidence.observed_sha256.as_deref() == observed_sha256
                && evidence.needs_review == needs_review
        })
}

fn evidence_observation_event(
    item: &MediaItem,
    device_id: &str,
    snapshot: &fs_safe::FileSnapshot,
    observed_sha256: String,
) -> Option<CatalogueEvent> {
    let observed_modified_millis = snapshot.modified_millis.map(|value| value as u64);
    let needs_review = observed_sha256 != item.sha256;
    (!evidence_matches(
        item,
        device_id,
        snapshot.length,
        observed_modified_millis,
        Some(&observed_sha256),
        needs_review,
    ))
    .then(|| CatalogueEvent::MediaEvidenceObserved {
        media_id: item.id.clone(),
        observed_size: snapshot.length,
        observed_modified_millis,
        observed_sha256: Some(observed_sha256),
        needs_review,
    })
}

#[tauri::command]
fn get_discovery_status(
    app: AppHandle,
    state: State<'_, NativeState>,
) -> Result<DiscoveryStatus, String> {
    load_discovery_status(&app, &state)
}

fn load_discovery_status(app: &AppHandle, state: &NativeState) -> Result<DiscoveryStatus, String> {
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
async fn rescan(app: AppHandle, full: Option<bool>) -> Result<RescanResult, String> {
    let full = full.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<NativeState>();
        let _operation = begin_critical_operation(&state.operations, "library scan")?;
        run_rescan(&app, &state, full)
    })
    .await
    .map_err(|error| format!("Library scan worker failed: {error}"))?
}

fn run_rescan(app: &AppHandle, state: &NativeState, full: bool) -> Result<RescanResult, String> {
    let settings = load_settings_file(&app)?;
    let master = PathBuf::from(&settings.master_bin_path);
    let _discovery_guard = state
        .discovery_write
        .try_lock()
        .map_err(|_| "A library scan is already running".to_string())?;
    let started_at = Utc::now().to_rfc3339();
    let mut status = DiscoveryStatus {
        running: true,
        full,
        started_at: Some(started_at),
        ..load_discovery_status(app, state)?
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
    let mut verified_paths = Vec::new();
    let mut result = RescanResult {
        full,
        ..Default::default()
    };
    let batch_timestamp = Local::now();
    for entry in WalkDir::new(&master).follow_links(false) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                result.errors += 1;
                status.errors = result.errors;
                continue;
            }
        };
        if !entry.file_type().is_file() || ignored(entry.path()) || !supported(entry.path()) {
            continue;
        }
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
            if let Some(event) =
                evidence_observation_event(item, &settings.device_id, &snapshot, hash.clone())
            {
                events.push(event);
            }
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
        verified_paths.push(relative);
        if result.scanned % 25 == 0 {
            publish_discovery_status(&app, &state, &status);
        }
    }
    status.total_files = result.scanned;
    publish_discovery_status(&app, &state, &status);
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
        CatalogueEvent::MediaEvidenceObserved {
            media_id,
            observed_size,
            observed_modified_millis,
            observed_sha256,
            needs_review,
        } => current
            .media
            .iter()
            .find(|item| item.id == *media_id)
            .is_some_and(|item| {
                !evidence_matches(
                    item,
                    &settings.device_id,
                    *observed_size,
                    *observed_modified_millis,
                    observed_sha256.as_deref(),
                    *needs_review,
                )
            }),
        _ => true,
    });
    result.registered = events
        .iter()
        .filter(|event| matches!(event, CatalogueEvent::MediaRegistered { .. }))
        .count();
    result.changed = events
        .iter()
        .filter(|event| matches!(event, CatalogueEvent::MediaEvidenceObserved { .. }))
        .count();
    let changed_catalogue = !events.is_empty();
    append_events(&settings, events)
        .map_err(|error| fail_discovery_scan(&app, &state, &mut status, error))?;
    for relative in verified_paths {
        observations.verified(&relative);
    }
    discovery::save(&discovery_path, &observations)
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
    let _operation = begin_critical_operation(&state.operations, "integrity verification")?;
    let settings = load_settings_file(&app)?;
    let catalogue = load_catalogue_file(&settings)?;
    let item = catalogue
        .media
        .iter()
        .find(|item| item.id == media_id)
        .cloned()
        .ok_or("Unknown media item")?;
    let path =
        fs_safe::resolve_existing(Path::new(&settings.master_bin_path), &item.relative_path)?;
    let snapshot = fs_safe::snapshot(&path)?;
    let observed_sha256 = sha256(&path)?;
    let matches = observed_sha256 == item.sha256;
    if fs_safe::snapshot(&path)? != snapshot {
        return Err("Media changed while its integrity was being verified".into());
    }
    let _guard = state.catalogue_write.lock().unwrap();
    let current = load_catalogue_file(&settings)?;
    let current_item = current
        .media
        .iter()
        .find(|candidate| candidate.id == media_id)
        .ok_or("Media changed while its integrity was being verified")?;
    if let Some(event) = evidence_observation_event(
        current_item,
        &settings.device_id,
        &snapshot,
        observed_sha256,
    ) {
        append_events(&settings, vec![event])?;
        let _ = app.emit("catalogue-changed", ());
    }
    Ok(matches)
}
#[tauri::command]
async fn enrich_media(app: AppHandle) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<NativeState>();
        let _operation = begin_critical_operation(&state.operations, "media enrichment")?;
        let now = Utc::now().to_rfc3339();
        *state.enrichment_status.lock().unwrap() = Some(BackgroundOperation {
            id: "media-enrichment".into(),
            kind: "media-enrichment".into(),
            title: "Technical metadata and thumbnail generation".into(),
            state: "queued".into(),
            stage: "Waiting for background capacity".into(),
            current_item: None,
            completed_items: 0,
            total_items: None,
            bytes_completed: 0,
            bytes_total: None,
            counts: OperationCounts::default(),
            cancellable: false,
            pausable: false,
            retryable: false,
            error: None,
            started_at: now.clone(),
            updated_at: now,
        });
        run_enrichment(&app, &state)
    })
    .await
    .map_err(|error| format!("Media enrichment worker failed: {error}"))?
}

fn kick_enrichment_worker(app: AppHandle) {
    let state = app.state::<NativeState>();
    let mut active = state.enrichment_worker_active.lock().unwrap();
    *state.enrichment_requested.lock().unwrap() = true;
    if *active {
        return;
    }
    *active = true;
    let now = Utc::now().to_rfc3339();
    *state.enrichment_status.lock().unwrap() = Some(BackgroundOperation {
        id: "media-enrichment".into(),
        kind: "media-enrichment".into(),
        title: "Technical metadata and thumbnail generation".into(),
        state: "queued".into(),
        stage: "Waiting for background capacity".into(),
        current_item: None,
        completed_items: 0,
        total_items: None,
        bytes_completed: 0,
        bytes_total: None,
        counts: OperationCounts::default(),
        cancellable: false,
        pausable: false,
        retryable: false,
        error: None,
        started_at: now.clone(),
        updated_at: now,
    });
    drop(active);
    std::thread::spawn(move || loop {
        let state = app.state::<NativeState>();
        {
            let _active = state.enrichment_worker_active.lock().unwrap();
            *state.enrichment_requested.lock().unwrap() = false;
        }
        if let Ok(_operation) = begin_critical_operation(&state.operations, "media enrichment") {
            if let Err(error) = run_enrichment(&app, &state) {
                if let Some(status) = state.enrichment_status.lock().unwrap().as_mut() {
                    status.state = "failed".into();
                    status.stage = "Enrichment failed".into();
                    status.error = Some(error.clone());
                    status.retryable = true;
                    status.updated_at = Utc::now().to_rfc3339();
                }
                diagnostic_log(&app, "ERROR", "media-enrichment", &error);
            }
        }
        let mut active = state.enrichment_worker_active.lock().unwrap();
        if *state.enrichment_requested.lock().unwrap() {
            drop(active);
            continue;
        }
        *active = false;
        break;
    });
}

fn thumbnail_generation_lock(state: &NativeState, media_id: &str) -> Arc<Mutex<()>> {
    let mut locks = state.thumbnail_generations.lock().unwrap();
    if locks.len() > 2_048 {
        locks.retain(|_, lock| Arc::strong_count(lock) > 1);
    }
    locks
        .entry(media_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn enforce_thumbnail_cache_limit(
    cache: &Path,
    limit_bytes: u64,
    protected: Option<&Path>,
) -> Result<u64, String> {
    if limit_bytes == 0 || !cache.is_dir() {
        return Ok(0);
    }
    let mut entries = fs::read_dir(cache)
        .map_err(|error| format!("Cannot inspect thumbnail cache: {error}"))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if !entry.file_type().ok()?.is_file() {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            (metadata.is_file()
                && path
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.eq_ignore_ascii_case("jpg")))
            .then(|| (metadata.modified().ok(), path, metadata.len()))
        })
        .collect::<Vec<_>>();
    let mut total = entries.iter().map(|entry| entry.2).sum::<u64>();
    entries.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    let mut removed = 0;
    for (_, path, size) in entries {
        if total <= limit_bytes {
            break;
        }
        if protected.is_some_and(|protected| path == protected) {
            continue;
        }
        fs::remove_file(&path)
            .map_err(|error| format!("Cannot evict thumbnail {}: {error}", path.display()))?;
        total = total.saturating_sub(size);
        removed += 1;
    }
    Ok(removed)
}

fn valid_cached_thumbnail(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() || metadata.len() < 4 || metadata.len() > 10 * 1024 * 1024 {
        return false;
    }
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut header = [0_u8; 3];
    file.read_exact(&mut header).is_ok() && header == [0xFF, 0xD8, 0xFF]
}

fn thumbnail_reference_is_available(cache: &Path, item: &MediaItem) -> bool {
    item.thumbnail_path.as_deref() == Some(&format!("{}.jpg", item.id))
        && valid_cached_thumbnail(&cache.join(format!("{}.jpg", item.id)))
}

fn enrichment_video_type(file_type: &str) -> bool {
    matches!(
        file_type.to_ascii_lowercase().as_str(),
        "mp4"
            | "mov"
            | "m4v"
            | "mxf"
            | "mts"
            | "m2ts"
            | "avi"
            | "h264"
            | "h265"
            | "hevc"
            | "r3d"
            | "braw"
    )
}

fn validate_thumbnail_frame(seconds: f64, duration: Option<f64>) -> Result<(), String> {
    if !seconds.is_finite() || seconds < 0.0 {
        return Err("Thumbnail frame time must be a finite non-negative value".into());
    }
    if duration.is_some_and(|duration| seconds > duration) {
        return Err("Thumbnail frame time is beyond the recording duration".into());
    }
    Ok(())
}

fn generate_thumbnail_atomic(
    source: &Path,
    destination: &Path,
    seek_seconds: f64,
) -> Result<(), String> {
    validate_thumbnail_frame(seek_seconds, None)?;
    let parent = destination
        .parent()
        .ok_or("Thumbnail destination has no cache directory")?;
    let temporary = parent.join(format!(".thumbnail-{}.jpg", Uuid::new_v4()));
    let seek = format!("{seek_seconds:.3}");
    let mut child = media_tool_command("ffmpeg")
        .args(["-y", "-nostdin", "-ss", &seek, "-i"])
        .arg(source)
        .args(["-frames:v", "1", "-vf", "scale=640:-2", "-q:v", "4"])
        .arg(&temporary)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not start FFmpeg thumbnail generation: {error}"))?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Could not monitor FFmpeg thumbnail generation: {error}"))?
        {
            break status;
        }
        if started.elapsed() >= StdDuration::from_secs(60) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_file(&temporary);
            return Err("FFmpeg thumbnail generation timed out after 60 seconds".into());
        }
        std::thread::sleep(StdDuration::from_millis(25));
    };
    if !status.success() || !valid_cached_thumbnail(&temporary) {
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "FFmpeg did not produce a valid thumbnail (exit status {})",
            status
        ));
    }
    if destination.exists() {
        if valid_cached_thumbnail(destination) {
            let _ = fs::remove_file(&temporary);
            return Ok(());
        }
        fs::remove_file(destination)
            .map_err(|error| format!("Cannot replace invalid cached thumbnail: {error}"))?;
    }
    fs::rename(&temporary, destination)
        .map_err(|error| format!("Cannot commit generated thumbnail: {error}"))?;
    Ok(())
}

fn run_enrichment(app: &AppHandle, state: &NativeState) -> Result<usize, String> {
    let settings = load_settings_file(&app)?;
    let _enrichment_guard = state.enrichment_write.lock().unwrap();
    let master = PathBuf::from(&settings.master_bin_path);
    let cache = PathBuf::from(&settings.thumbnail_cache_path);
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let catalogue = load_catalogue_file(&settings)?;
    let total = catalogue
        .media
        .iter()
        .filter(|item| {
            enrichment_video_type(&item.file_type)
                && (item.technical.codec.is_none()
                    || !thumbnail_reference_is_available(&cache, item))
        })
        .count();
    let existing = catalogue
        .media
        .iter()
        .filter(|item| {
            enrichment_video_type(&item.file_type)
                && item.technical.codec.is_some()
                && thumbnail_reference_is_available(&cache, item)
        })
        .count();
    if let Some(status) = state.enrichment_status.lock().unwrap().as_mut() {
        status.state = "running".into();
        status.stage = "Inspecting technical metadata and thumbnails".into();
        status.completed_items = 0;
        status.total_items = Some(total);
        status.counts = OperationCounts {
            existing_files: existing,
            pending_files: total,
            ..OperationCounts::default()
        };
        status.error = None;
        status.updated_at = Utc::now().to_rfc3339();
    }
    let mut candidates = Vec::new();
    let mut processed = 0_usize;
    for item in &catalogue.media {
        let potentially_eligible = enrichment_video_type(&item.file_type)
            && (item.technical.codec.is_none() || !thumbnail_reference_is_available(&cache, item));
        if !potentially_eligible {
            continue;
        }
        let generation_lock = thumbnail_generation_lock(state, &item.id);
        let _generation_guard = generation_lock.lock().unwrap();
        if let Some(status) = state.enrichment_status.lock().unwrap().as_mut() {
            status.stage = format!(
                "Generating thumbnails and metadata ({}/{total})",
                processed + 1
            );
            status.current_item = Some(item.original_filename.clone());
            status.updated_at = Utc::now().to_rfc3339();
        }
        let source = match fs_safe::resolve_existing(&master, &item.relative_path) {
            Ok(path) if path.is_file() => path,
            _ => {
                processed += 1;
                if let Some(status) = state.enrichment_status.lock().unwrap().as_mut() {
                    status.completed_items = processed;
                    status.counts.failed_files += 1;
                    status.counts.pending_files = total.saturating_sub(processed);
                }
                continue;
            }
        };
        let source_before = match fs_safe::snapshot(&source) {
            Ok(snapshot) => snapshot,
            Err(_) => {
                processed += 1;
                if let Some(status) = state.enrichment_status.lock().unwrap().as_mut() {
                    status.completed_items = processed;
                    status.counts.failed_files += 1;
                    status.counts.pending_files = total.saturating_sub(processed);
                }
                continue;
            }
        };
        let thumbnail_ref = format!("{}.jpg", item.id);
        if !catalogue::safe_cache_reference(&thumbnail_ref) {
            continue;
        }
        let thumbnail = cache.join(&thumbnail_ref);
        let needs_probe = item.technical.codec.is_none();
        let needs_thumbnail = !valid_cached_thumbnail(&thumbnail);
        if !needs_thumbnail {
            if let Some(status) = state.enrichment_status.lock().unwrap().as_mut() {
                status.counts.existing_files += 1;
            }
        }
        if !needs_probe && !needs_thumbnail {
            processed += 1;
            if let Some(status) = state.enrichment_status.lock().unwrap().as_mut() {
                status.completed_items = processed;
            }
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
        let mut thumbnail_failed = false;
        if needs_thumbnail {
            if let Some(status) = state.enrichment_status.lock().unwrap().as_mut() {
                status.counts.attempted_files += 1;
            }
            let _static_lane = state.static_thumbnail_generation.lock().unwrap();
            if let Err(error) = generate_thumbnail_atomic(&source, &thumbnail, 0.1) {
                thumbnail_failed = true;
                diagnostic_log(
                    app,
                    "ERROR",
                    "thumbnail-generation",
                    &format!("{}: {error}", item.relative_path),
                );
            }
        }
        if fs_safe::snapshot(&source).ok().as_ref() != Some(&source_before) {
            if needs_thumbnail {
                let _ = fs::remove_file(&thumbnail);
            }
            processed += 1;
            if let Some(status) = state.enrichment_status.lock().unwrap().as_mut() {
                status.completed_items = processed;
                status.counts.attention_files += 1;
                status.counts.pending_files = total.saturating_sub(processed);
            }
            continue;
        }
        let generated_thumbnail_ref = valid_cached_thumbnail(&thumbnail).then_some(thumbnail_ref);
        let generated_thumbnail = needs_thumbnail && generated_thumbnail_ref.is_some();
        if thumbnail_failed {
            if let Some(status) = state.enrichment_status.lock().unwrap().as_mut() {
                status.counts.failed_files += 1;
            }
        }
        if technical == item.technical && generated_thumbnail_ref == item.thumbnail_path {
            processed += 1;
            if let Some(status) = state.enrichment_status.lock().unwrap().as_mut() {
                status.completed_items = processed;
                status.counts.pending_files = total.saturating_sub(processed);
            }
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
                thumbnail_ref: generated_thumbnail_ref,
            },
        ));
        processed += 1;
        if let Some(status) = state.enrichment_status.lock().unwrap().as_mut() {
            status.completed_items = processed;
            status.counts.new_files += usize::from(generated_thumbnail);
            status.counts.pending_files = total.saturating_sub(processed);
        }
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
    if changed > 0 {
        let _ = app.emit("catalogue-changed", ());
    }
    enforce_thumbnail_cache_limit(&cache, settings.thumbnail_cache_limit_bytes, None)?;
    if let Some(status) = state.enrichment_status.lock().unwrap().as_mut() {
        status.state = "completed".into();
        status.stage = "Thumbnail and metadata pass complete".into();
        status.current_item = None;
        status.completed_items = total;
        status.counts.pending_files = 0;
        status.retryable = status.counts.failed_files > 0;
        status.updated_at = Utc::now().to_rfc3339();
    }
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

fn compatible_windows_handoff_path(value: &str) -> String {
    let Some(without_prefix) = value.strip_prefix(r"\\?\") else {
        return value.to_string();
    };
    let bytes = without_prefix.as_bytes();
    let ordinary_drive_path = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    let legacy_safe_segments = without_prefix
        .split(['\\', '/'])
        .all(|segment| !segment.ends_with(' ') && !segment.ends_with('.'));
    if ordinary_drive_path && legacy_safe_segments && without_prefix.encode_utf16().count() < 260 {
        without_prefix.to_string()
    } else {
        value.to_string()
    }
}

fn explorer_boundary_path(value: &str) -> Result<String, String> {
    let candidate = if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else if let Some(drive) = value.strip_prefix(r"\\?\") {
        drive.to_string()
    } else {
        value.to_string()
    };
    if candidate.encode_utf16().count() >= 260 {
        return Err(
            "Windows Explorer cannot reliably select this path because it exceeds the Shell's supported path length; open its containing folder manually"
                .into(),
        );
    }
    if candidate
        .split(['\\', '/'])
        .any(|segment| segment.ends_with(' ') || segment.ends_with('.'))
    {
        return Err(
            "Windows Explorer cannot safely select a path whose name ends in a space or period"
                .into(),
        );
    }
    Ok(candidate)
}

fn handoff_path(path: &Path) -> String {
    compatible_windows_handoff_path(&path.to_string_lossy())
}

#[tauri::command]
fn resolve_media_path(app: AppHandle, media_id: String) -> Result<String, String> {
    let settings = load_settings_file(&app)?;
    Ok(handoff_path(&resolve_catalogue_media_path(
        &settings, &media_id,
    )?))
}

#[derive(Debug, Deserialize)]
struct PlaybackProbeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    pix_fmt: Option<String>,
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PlaybackProbeFormat {
    format_name: Option<String>,
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PlaybackProbe {
    #[serde(default)]
    streams: Vec<PlaybackProbeStream>,
    format: Option<PlaybackProbeFormat>,
}

fn probe_playback_media(path: &Path) -> Result<PlaybackProbe, String> {
    let output = media_tool_command("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,codec_name,pix_fmt,duration:format=format_name,duration",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|error| format!("Could not start FFprobe: {error}"))?;
    if !output.status.success() {
        return Err("FFprobe could not read the playback proxy".into());
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("FFprobe returned invalid proxy metadata: {error}"))
}

fn parsed_duration_millis(value: Option<&str>) -> Option<u64> {
    let seconds = value?.parse::<f64>().ok()?;
    (seconds.is_finite() && seconds > 0.0).then(|| (seconds * 1_000.0).round() as u64)
}

fn probe_duration_millis(probe: &PlaybackProbe) -> Option<u64> {
    parsed_duration_millis(
        probe
            .format
            .as_ref()
            .and_then(|format| format.duration.as_deref()),
    )
    .or_else(|| {
        probe.streams.iter().find_map(|stream| {
            (stream.codec_type.as_deref() == Some("video"))
                .then(|| parsed_duration_millis(stream.duration.as_deref()))
                .flatten()
        })
    })
}

fn playback_proxy_probe_is_valid(probe: &PlaybackProbe) -> bool {
    let Some(format) = &probe.format else {
        return false;
    };
    let mp4_container = format
        .format_name
        .as_deref()
        .is_some_and(|names| names.split(',').any(|name| matches!(name, "mov" | "mp4")));
    let video = probe.streams.iter().find(|stream| {
        stream.codec_type.as_deref() == Some("video")
            && stream.codec_name.as_deref() == Some("h264")
            && stream
                .pix_fmt
                .as_deref()
                .is_some_and(|format| format.starts_with("yuv420p"))
    });
    let audio_valid = probe
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("audio"))
        .all(|stream| stream.codec_name.as_deref() == Some("aac"));
    mp4_container && video.is_some() && audio_valid && probe_duration_millis(probe).is_some()
}

fn valid_playback_proxy(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.file_type().is_file() && metadata.len() > 1_024
    }) && probe_playback_media(path).is_ok_and(|probe| playback_proxy_probe_is_valid(&probe))
}

fn playback_proxy_root(settings: &DeviceSettings) -> Result<PathBuf, String> {
    let cache = PathBuf::from(&settings.thumbnail_cache_path);
    fs::create_dir_all(&cache)
        .map_err(|error| format!("Cannot create device cache root: {error}"))?;
    let root = cache.join("playback-proxies");
    fs::create_dir_all(&root)
        .map_err(|error| format!("Cannot create playback proxy cache: {error}"))?;
    if fs::symlink_metadata(&root)
        .map_err(|error| error.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Playback proxy cache may not be a link or junction".into());
    }
    let canonical_cache = cache
        .canonicalize()
        .map_err(|error| format!("Cannot resolve device cache root: {error}"))?;
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("Cannot resolve playback proxy cache: {error}"))?;
    if !fs_safe::within(&canonical_cache, &canonical_root) {
        return Err("Playback proxy cache resolved outside PIMP's device cache".into());
    }
    Ok(canonical_root)
}

fn enforce_playback_proxy_limit(
    root: &Path,
    limit: u64,
    protected: &HashSet<PathBuf>,
) -> Result<(), String> {
    let mut entries = fs::read_dir(root)
        .map_err(|error| format!("Cannot inspect playback proxy cache: {error}"))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            let metadata = entry.metadata().ok()?;
            (file_type.is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|value| value.eq_ignore_ascii_case("mp4")))
                .then(|| (entry.path(), metadata.len(), metadata.modified().ok()))
        })
        .collect::<Vec<_>>();
    let mut total = entries.iter().map(|entry| entry.1).sum::<u64>();
    entries.sort_by_key(|entry| entry.2);
    for (path, size, _) in entries {
        if total <= limit {
            break;
        }
        if protected.contains(&path) {
            continue;
        }
        fs::remove_file(&path).map_err(|error| format!("Cannot evict playback proxy: {error}"))?;
        total = total.saturating_sub(size);
    }
    Ok(())
}

fn update_playback_proxy_status(
    state: &NativeState,
    operation_id: &str,
    update: impl FnOnce(&mut BackgroundOperation),
) {
    if let Some(operation) = state
        .playback_proxy_status
        .lock()
        .unwrap()
        .get_mut(operation_id)
    {
        update(operation);
        operation.updated_at = Utc::now().to_rfc3339();
    }
}

fn playback_progress_millis(line: &str) -> Option<u64> {
    if let Some(value) = line.strip_prefix("out_time_us=") {
        return value.parse::<u64>().ok().map(|value| value / 1_000);
    }
    let value = line.strip_prefix("out_time=")?;
    let mut parts = value.split(':');
    let hours = parts.next()?.parse::<f64>().ok()?;
    let minutes = parts.next()?.parse::<f64>().ok()?;
    let seconds = parts.next()?.parse::<f64>().ok()?;
    Some(((hours * 3_600.0 + minutes * 60.0 + seconds) * 1_000.0) as u64)
}

struct ManagedChild(std::process::Child);

impl Drop for ManagedChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

struct PlaybackProcessGuard<'a> {
    processes: &'a Mutex<HashMap<String, u32>>,
    operation_id: String,
}

impl Drop for PlaybackProcessGuard<'_> {
    fn drop(&mut self) {
        self.processes.lock().unwrap().remove(&self.operation_id);
    }
}

fn terminate_playback_proxy_process(pid: u32) {
    #[cfg(target_os = "windows")]
    {
        let _ = media_tool_command("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = Command::new("kill").arg(pid.to_string()).status();
    }
}

fn cancel_all_playback_proxy_processes(state: &NativeState) {
    let processes = state.playback_proxy_processes.lock().unwrap().clone();
    for (operation_id, pid) in processes {
        state
            .playback_proxy_cancelled
            .lock()
            .unwrap()
            .insert(operation_id.clone());
        update_playback_proxy_status(state, &operation_id, |operation| {
            operation.state = "cancelling".into();
            operation.stage = "Application is closing; stopping FFmpeg".into();
            operation.cancellable = false;
        });
        terminate_playback_proxy_process(pid);
    }
}

fn protected_playback_proxy_paths(state: &NativeState, current: &Path) -> HashSet<PathBuf> {
    let mut paths = state
        .active_playback_proxies
        .lock()
        .unwrap()
        .values()
        .cloned()
        .collect::<HashSet<_>>();
    paths.insert(current.to_path_buf());
    paths.extend(
        state
            .playback_proxy_temporaries
            .lock()
            .unwrap()
            .iter()
            .cloned(),
    );
    paths
}

struct PlaybackTemporaryGuard<'a> {
    paths: &'a Mutex<HashSet<PathBuf>>,
    path: PathBuf,
}

impl Drop for PlaybackTemporaryGuard<'_> {
    fn drop(&mut self) {
        self.paths.lock().unwrap().remove(&self.path);
    }
}

fn protect_playback_temporary<'a>(
    state: &'a NativeState,
    path: &Path,
) -> PlaybackTemporaryGuard<'a> {
    state
        .playback_proxy_temporaries
        .lock()
        .unwrap()
        .insert(path.to_path_buf());
    PlaybackTemporaryGuard {
        paths: &state.playback_proxy_temporaries,
        path: path.to_path_buf(),
    }
}

fn cleanup_abandoned_playback_temporaries(
    state: &NativeState,
    root: &Path,
) -> Result<(), String> {
    let active = state.playback_proxy_temporaries.lock().unwrap().clone();
    for entry in fs::read_dir(root)
        .map_err(|error| format!("Cannot inspect playback proxy cache: {error}"))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if active.contains(&path)
            || !entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_file()
            || !entry.file_name().to_string_lossy().starts_with(".proxy-")
        {
            continue;
        }
        fs::remove_file(path)
            .map_err(|error| format!("Cannot remove abandoned playback proxy output: {error}"))?;
    }
    Ok(())
}

fn fail_playback_proxy_status(state: &NativeState, operation_id: &str, error: &str) {
    update_playback_proxy_status(state, operation_id, |operation| {
        if operation.state != "cancelled" {
            operation.state = "failed".into();
            operation.stage = "Proxy generation failed".into();
            operation.error = Some(error.to_owned());
            operation.cancellable = false;
            operation.retryable = true;
        }
    });
}

fn run_playback_proxy_generation(
    state: &NativeState,
    operation_id: &str,
    source: &Path,
    temporary: &Path,
    duration_millis: Option<u64>,
) -> Result<(), String> {
    let mut command = media_tool_command("ffmpeg");
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(source)
        .args([
            "-map",
            "0:v:0",
            "-map",
            "0:a?",
            "-vf",
            "scale=min(1280\\,iw):-2",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "23",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-movflags",
            "+faststart",
            "-progress",
            "pipe:1",
            "-nostats",
            "-n",
        ])
        .arg(temporary)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = ManagedChild(
        command
            .spawn()
            .map_err(|error| format!("Could not start FFmpeg playback proxy generation: {error}"))?,
    );
    state
        .playback_proxy_processes
        .lock()
        .unwrap()
        .insert(operation_id.to_owned(), child.0.id());
    let _process_guard = PlaybackProcessGuard {
        processes: &state.playback_proxy_processes,
        operation_id: operation_id.to_owned(),
    };
    let stdout = child
        .0
        .stdout
        .take()
        .ok_or("FFmpeg progress stream is unavailable")?;
    let stderr = child
        .0
        .stderr
        .take()
        .ok_or("FFmpeg diagnostic stream is unavailable")?;
    let (sender, receiver) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let mut processed = 0_u64;
    let status = loop {
        while let Ok(line) = receiver.try_recv() {
            if let Some(value) = playback_progress_millis(&line) {
                processed = processed.max(value);
                update_playback_proxy_status(state, operation_id, |operation| {
                    operation.completed_items = usize::try_from(processed / 1_000).unwrap_or(usize::MAX);
                });
            }
        }
        if state
            .playback_proxy_cancelled
            .lock()
            .unwrap()
            .contains(operation_id)
        {
            child
                .0
                .kill()
                .map_err(|error| format!("Could not stop FFmpeg safely: {error}"))?;
            let _ = child.0.wait();
            update_playback_proxy_status(state, operation_id, |operation| {
                operation.state = "cancelled".into();
                operation.stage = "Cancelled; partial output removed".into();
                operation.cancellable = false;
            });
            return Err("Playback proxy generation was cancelled safely".into());
        }
        if let Some(status) = child
            .0
            .try_wait()
            .map_err(|error| format!("Could not monitor FFmpeg: {error}"))?
        {
            break status;
        }
        std::thread::sleep(StdDuration::from_millis(100));
    };
    let mut diagnostics = String::new();
    BufReader::new(stderr)
        .read_to_string(&mut diagnostics)
        .map_err(|error| format!("Could not read FFmpeg diagnostics: {error}"))?;
    if !status.success() {
        return Err(format!(
            "FFmpeg could not create a compatible playback proxy (exit status {status}): {}",
            diagnostics.trim()
        ));
    }
    update_playback_proxy_status(state, operation_id, |operation| {
        operation.completed_items = duration_millis
            .map(|value| usize::try_from(value / 1_000).unwrap_or(usize::MAX))
            .unwrap_or(operation.completed_items);
        operation.stage = "Validating completed proxy".into();
        operation.cancellable = false;
    });
    Ok(())
}

#[tauri::command]
async fn prepare_playback_proxy(app: AppHandle, media_id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<NativeState>();
        let _operation = begin_critical_operation(&state.operations, "playback proxy generation")?;
        let settings = load_settings_file(&app)?;
        let catalogue = load_catalogue_file(&settings)?;
        let item = catalogue.media.iter().find(|item| item.id == media_id)
            .cloned().ok_or("Unknown media item")?;
        if !enrichment_video_type(&item.file_type) {
            return Err("Playback proxies are available only for supported video recordings".into());
        }
        let source = resolve_catalogue_media_path(&settings, &media_id)?;
        let before = fs_safe::snapshot(&source)?;
        let source_probe = probe_playback_media(&source)?;
        let duration_millis = probe_duration_millis(&source_probe);
        let mut digest = Sha256::new();
        digest.update(b"pimp-playback-proxy-v1\0");
        digest.update(item.id.as_bytes());
        digest.update(item.sha256.as_bytes());
        digest.update(before.length.to_le_bytes());
        digest.update([u8::from(before.modified_millis.is_some())]);
        digest.update(before.modified_millis.unwrap_or_default().to_le_bytes());
        digest.update(b"1280-h264-aac-crf23");
        let fingerprint = format!("{:x}", digest.finalize());
        let operation_id = format!("playback-proxy:{}", &fingerprint[..24]);
        let generation = {
            let mut generations = state.playback_proxy_generations.lock().unwrap();
            generations
                .entry(operation_id.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _generation_guard = generation.lock().unwrap();
        state
            .playback_proxy_cancelled
            .lock()
            .unwrap()
            .remove(&operation_id);
        let root = playback_proxy_root(&settings)?;
        cleanup_abandoned_playback_temporaries(&state, &root)?;
        let destination = root.join(format!("{}-{}.mp4", item.id, &fingerprint[..24]));
        if valid_playback_proxy(&destination) {
            state
                .active_playback_proxies
                .lock()
                .unwrap()
                .insert(media_id.clone(), destination.clone());
            enforce_playback_proxy_limit(
                &root,
                settings.playback_proxy_cache_limit_bytes,
                &protected_playback_proxy_paths(&state, &destination),
            )?;
            return Ok(destination.to_string_lossy().into_owned());
        }
        if fs::symlink_metadata(&destination).is_ok() {
            fs::remove_file(&destination)
                .map_err(|error| format!("Cannot remove invalid cached proxy: {error}"))?;
        }
        let temporary = root.join(format!(".proxy-{}.mp4", Uuid::new_v4()));
        let _temporary_guard = protect_playback_temporary(&state, &temporary);
        let now = Utc::now().to_rfc3339();
        state.playback_proxy_status.lock().unwrap().insert(
            operation_id.clone(),
            BackgroundOperation {
                id: operation_id.clone(),
                kind: "playback-proxy".into(),
                title: format!("Playback proxy: {}", item.original_filename),
                state: "running".into(),
                stage: "Transcoding H.264/AAC preview".into(),
                current_item: Some(item.original_filename.clone()),
                completed_items: 0,
                total_items: duration_millis
                    .map(|value| usize::try_from(value / 1_000).unwrap_or(usize::MAX)),
                bytes_completed: 0,
                bytes_total: None,
                counts: OperationCounts::default(),
                cancellable: true,
                pausable: false,
                retryable: false,
                error: None,
                started_at: now.clone(),
                updated_at: now,
            },
        );
        let generated = run_playback_proxy_generation(
            &state,
            &operation_id,
            &source,
            &temporary,
            duration_millis,
        );
        if let Err(error) = generated {
            let _ = fs::remove_file(&temporary);
            fail_playback_proxy_status(&state, &operation_id, &error);
            return Err(error);
        }
        if !valid_playback_proxy(&temporary) {
            let _ = fs::remove_file(&temporary);
            let error = "The generated playback proxy failed codec/container validation";
            fail_playback_proxy_status(&state, &operation_id, error);
            return Err(error.into());
        }
        let after = match fs_safe::snapshot(&source) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                fail_playback_proxy_status(&state, &operation_id, &error);
                return Err(error);
            }
        };
        if after != before {
            let _ = fs::remove_file(&temporary);
            let error = "Source changed while its playback proxy was being generated";
            fail_playback_proxy_status(&state, &operation_id, error);
            return Err(error.into());
        }
        if destination.exists() {
            let _ = fs::remove_file(&temporary);
        } else {
            if let Err(error) = fs::rename(&temporary, &destination) {
                let error = format!("Cannot commit playback proxy: {error}");
                fail_playback_proxy_status(&state, &operation_id, &error);
                return Err(error);
            }
        }
        if !valid_playback_proxy(&destination) {
            let error = "The generated playback proxy failed validation";
            fail_playback_proxy_status(&state, &operation_id, error);
            return Err(error.into());
        }
        state
            .active_playback_proxies
            .lock()
            .unwrap()
            .insert(media_id, destination.clone());
        if let Err(error) = enforce_playback_proxy_limit(
            &root,
            settings.playback_proxy_cache_limit_bytes,
            &protected_playback_proxy_paths(&state, &destination),
        ) {
            fail_playback_proxy_status(&state, &operation_id, &error);
            return Err(error);
        }
        update_playback_proxy_status(&state, &operation_id, |operation| {
            operation.state = "completed".into();
            operation.stage = "Compatible proxy ready".into();
            operation.cancellable = false;
        });
        Ok(destination.to_string_lossy().into_owned())
    }).await.map_err(|error| format!("Playback proxy worker failed: {error}"))?
}

#[tauri::command]
fn release_playback_proxy(state: State<'_, NativeState>, media_id: String) {
    state.active_playback_proxies.lock().unwrap().remove(&media_id);
}

#[tauri::command]
fn cancel_background_operation(
    state: State<'_, NativeState>,
    operation_id: String,
) -> Result<(), String> {
    if operation_id.starts_with("playback-proxy:") {
        state
            .playback_proxy_cancelled
            .lock()
            .unwrap()
            .insert(operation_id.clone());
        update_playback_proxy_status(&state, &operation_id, |operation| {
            if operation.state == "running" {
                operation.state = "cancelling".into();
                operation.stage = "Stopping FFmpeg and removing partial output".into();
                operation.cancellable = false;
            }
        });
        Ok(())
    } else {
        cancel_inventory(state, operation_id)
    }
}

fn build_premiere_import_manifest(
    settings: &DeviceSettings,
    catalogue: &Catalogue,
    media_ids: &[String],
) -> Result<PremiereImportManifest, String> {
    if media_ids.is_empty() || media_ids.len() > 5_000 {
        return Err("Select between 1 and 5,000 media files for Premiere".into());
    }
    let master = Path::new(&settings.master_bin_path)
        .canonicalize()
        .map_err(|error| format!("Cannot resolve authorized Master Bin: {error}"))?;
    let mut seen = HashSet::new();
    let mut items = Vec::with_capacity(media_ids.len());
    for media_id in media_ids {
        if !seen.insert(media_id) {
            continue;
        }
        let item = catalogue
            .media
            .iter()
            .find(|item| item.id == *media_id)
            .ok_or_else(|| format!("Unknown media item {media_id}"))?;
        let relative = fs_safe::validate_relative(&item.relative_path)?;
        let absolute = fs_safe::resolve_existing(&master, &item.relative_path)?;
        if !absolute.is_file() {
            return Err(format!(
                "{} is not available locally",
                item.original_filename
            ));
        }
        let bin_path = relative
            .parent()
            .map(|parent| parent.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let attached_to_media_id = catalogue
            .media
            .iter()
            .find(|candidate| candidate.attached_audio_id.as_deref() == Some(item.id.as_str()))
            .map(|candidate| candidate.id.clone());
        items.push(PremiereImportItem {
            media_id: item.id.clone(),
            absolute_path: handoff_path(&absolute),
            relative_path: item.relative_path.replace('\\', "/"),
            original_filename: item.original_filename.clone(),
            sha256: item.sha256.clone(),
            file_type: item.file_type.clone(),
            bin_path,
            attached_to_media_id,
        });
    }
    Ok(PremiereImportManifest {
        schema_version: 1,
        kind: "kairix-pimp-premiere-import".into(),
        created_at: Utc::now().to_rfc3339(),
        device_id: settings.device_id.clone(),
        items,
    })
}

fn write_new_premiere_manifest(
    destination: &Path,
    manifest: &PremiereImportManifest,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                "The selected manifest already exists; choose a new filename so it is not silently replaced".to_string()
            } else {
                format!("Cannot create Premiere manifest: {error}")
            }
        })?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("Cannot finish Premiere manifest: {error}"))
}

#[tauri::command]
fn export_premiere_manifest(
    app: AppHandle,
    media_ids: Vec<String>,
    destination_path: String,
) -> Result<usize, String> {
    let destination = PathBuf::from(destination_path);
    if !destination
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        return Err("Premiere interchange manifests must use the .json extension".into());
    }
    let parent = destination
        .parent()
        .ok_or("Choose a valid manifest destination")?;
    if !parent.is_dir() {
        return Err("The selected manifest destination folder is unavailable".into());
    }
    let settings = load_settings_file(&app)?;
    let catalogue = load_catalogue_file(&settings)?;
    let manifest = build_premiere_import_manifest(&settings, &catalogue, &media_ids)?;
    write_new_premiere_manifest(&destination, &manifest)?;
    Ok(manifest.items.len())
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
        fn ILFindLastID(pidl: *const c_void) -> *const c_void;
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

    let shell_path = explorer_boundary_path(&path.to_string_lossy())?;
    let shell_path = PathBuf::from(shell_path);
    let parent = shell_path
        .parent()
        .ok_or("Windows Explorer could not determine the containing folder")?;
    let wide_path = shell_path
        .as_os_str()
        .encode_wide()
        .chain(once(0))
        .collect::<Vec<_>>();
    let wide_parent = parent
        .as_os_str()
        .encode_wide()
        .chain(once(0))
        .collect::<Vec<_>>();
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

        let folder = ILCreateFromPathW(wide_parent.as_ptr());
        let item = ILCreateFromPathW(wide_path.as_ptr());
        if folder.is_null() || item.is_null() {
            if !folder.is_null() {
                ILFree(folder);
            }
            if !item.is_null() {
                ILFree(item);
            }
            if initialized >= 0 {
                CoUninitialize();
            }
            return Err("Windows Explorer could not resolve this recording".into());
        }
        let child = ILFindLastID(item);
        let children = [child];
        let selected = SHOpenFolderAndSelectItems(folder, 1, children.as_ptr(), 0);
        ILFree(item);
        ILFree(folder);
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
        "test-builds" => {
            Some("https://github.com/Dubcodes/kairix-pimp/actions/workflows/windows-beta.yml")
        }
        "website" => Some("https://www.dubcodesmedia.com/kairix/kairix-pimp"),
        "coffee" => Some("https://buymeacoffee.com/dubcodes"),
        "degoo" => Some("https://app.degoo.com/"),
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
    let mut coordinator = state.operations.lock().unwrap();
    let ingest_jobs = snapshot_jobs(&app, &state)?;
    let backup_jobs = {
        let _guard = state.backup_job_store.lock().unwrap();
        load_backup_jobs_unlocked(&app)?
    };
    begin_update_gate(&mut coordinator, &ingest_jobs, &backup_jobs)
}

#[tauri::command]
fn end_update_installation(state: State<'_, NativeState>) {
    end_update_gate(&state.operations);
}

#[tauri::command]
fn set_media_thumbnail_frame(
    app: AppHandle,
    state: State<'_, NativeState>,
    media_id: String,
    seconds: f64,
) -> Result<String, String> {
    let settings = load_settings_file(&app)?;
    let catalogue = load_catalogue_file(&settings)?;
    let item = catalogue
        .media
        .iter()
        .find(|item| item.id == media_id)
        .cloned()
        .ok_or("Unknown media item")?;
    if !enrichment_video_type(&item.file_type) {
        return Err("Custom thumbnail frames are supported only for video recordings".into());
    }
    validate_thumbnail_frame(seconds, item.technical.duration_seconds)?;
    let source = resolve_catalogue_media_path(&settings, &media_id)?;
    let before = fs_safe::snapshot(&source)?;
    let cache = PathBuf::from(&settings.thumbnail_cache_path);
    fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
    let cache = cache
        .canonicalize()
        .map_err(|error| format!("Cannot resolve thumbnail cache: {error}"))?;
    let thumbnail_ref = format!("{}.jpg", item.id);
    let destination = cache.join(&thumbnail_ref);
    let generation_lock = thumbnail_generation_lock(&state, &media_id);
    let _generation_guard = generation_lock.lock().unwrap();
    {
        let _static_lane = state.static_thumbnail_generation.lock().unwrap();
        generate_thumbnail_atomic(&source, &destination, seconds)?;
    }
    if fs_safe::snapshot(&source).ok().as_ref() != Some(&before) {
        let _ = fs::remove_file(&destination);
        return Err(
            "Source changed while selecting the thumbnail frame; retry after writes finish".into(),
        );
    }
    if item.thumbnail_path.as_deref() != Some(&thumbnail_ref) {
        let _catalogue_guard = state.catalogue_write.lock().unwrap();
        append_events(
            &settings,
            vec![CatalogueEvent::MediaEnriched {
                media_id: item.id,
                technical: item.technical,
                thumbnail_ref: Some(thumbnail_ref),
            }],
        )?;
    }
    enforce_thumbnail_cache_limit(
        &cache,
        settings.thumbnail_cache_limit_bytes,
        Some(&destination),
    )?;
    let bytes = fs::read(&destination)
        .map_err(|error| format!("Cannot load selected thumbnail: {error}"))?;
    let _ = app.emit("catalogue-changed", ());
    Ok(format!("data:image/jpeg;base64,{}", BASE64.encode(bytes)))
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
    if !valid_cached_thumbnail(&path) {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    Ok(Some(format!(
        "data:image/jpeg;base64,{}",
        BASE64.encode(bytes)
    )))
}

#[tauri::command]
fn load_thumbnail_sources(
    app: AppHandle,
    media_ids: Vec<String>,
) -> Result<HashMap<String, String>, String> {
    if media_ids.len() > 10_000 {
        return Err("Thumbnail source request exceeds the 10,000 item safety limit".into());
    }
    let requested = media_ids.into_iter().collect::<HashSet<_>>();
    let settings = load_settings_file(&app)?;
    let catalogue = load_catalogue_file(&settings)?;
    let cache = PathBuf::from(&settings.thumbnail_cache_path);
    fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
    let cache = cache
        .canonicalize()
        .map_err(|error| format!("Cannot resolve thumbnail cache: {error}"))?;
    let mut sources = HashMap::new();
    for item in catalogue
        .media
        .iter()
        .filter(|item| requested.contains(&item.id))
    {
        let Some(reference) = &item.thumbnail_path else {
            continue;
        };
        if !catalogue::safe_cache_reference(reference) || reference != &format!("{}.jpg", item.id) {
            continue;
        }
        let path = cache.join(reference);
        let Ok(path) = path.canonicalize() else {
            continue;
        };
        if path.starts_with(&cache) && valid_cached_thumbnail(&path) {
            sources.insert(item.id.clone(), path.to_string_lossy().into_owned());
        }
    }
    Ok(sources)
}

#[tauri::command]
async fn prepare_media_thumbnail(
    app: AppHandle,
    media_id: String,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<NativeState>();
        let _operation =
            begin_critical_operation(&state.operations, "visible thumbnail generation")?;
        let generation_lock = thumbnail_generation_lock(&state, &media_id);
        let _generation_guard = generation_lock.lock().unwrap();
        let settings = load_settings_file(&app)?;
        let catalogue = load_catalogue_file(&settings)?;
        let item = catalogue
            .media
            .iter()
            .find(|item| item.id == media_id)
            .cloned()
            .ok_or("Unknown media item")?;
        if !enrichment_video_type(&item.file_type) {
            return Ok(None);
        }
        let cache = PathBuf::from(&settings.thumbnail_cache_path);
        fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
        let thumbnail_ref = format!("{}.jpg", item.id);
        if !catalogue::safe_cache_reference(&thumbnail_ref) {
            return Err("Media identity is not safe for the thumbnail cache".into());
        }
        let thumbnail = cache.join(&thumbnail_ref);
        if !valid_cached_thumbnail(&thumbnail) {
            let master = PathBuf::from(&settings.master_bin_path);
            let source = fs_safe::resolve_existing(&master, &item.relative_path)?;
            let before = fs_safe::snapshot(&source)?;
            {
                let _static_lane = state.static_thumbnail_generation.lock().unwrap();
                generate_thumbnail_atomic(&source, &thumbnail, 0.1)?;
            }
            if fs_safe::snapshot(&source)? != before {
                let _ = fs::remove_file(&thumbnail);
                return Err("Media changed while its thumbnail was being generated".into());
            }
        }
        if item.thumbnail_path.as_deref() != Some(&thumbnail_ref) {
            let _catalogue_guard = state.catalogue_write.lock().unwrap();
            let current = load_catalogue_file(&settings)?;
            let current_item = current
                .media
                .iter()
                .find(|candidate| candidate.id == media_id)
                .ok_or("Media changed while its thumbnail was being generated")?;
            if current_item.relative_path != item.relative_path
                || current_item.sha256 != item.sha256
            {
                return Err("Media changed while its thumbnail was being generated".into());
            }
            append_events(
                &settings,
                vec![CatalogueEvent::MediaEnriched {
                    media_id: item.id.clone(),
                    technical: current_item.technical.clone(),
                    thumbnail_ref: Some(thumbnail_ref),
                }],
            )?;
            let _ = app.emit("catalogue-changed", ());
        }
        enforce_thumbnail_cache_limit(
            &cache,
            settings.thumbnail_cache_limit_bytes,
            Some(&thumbnail),
        )?;
        if !valid_cached_thumbnail(&thumbnail) {
            return Ok(None);
        }
        Ok(Some(thumbnail.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|error| format!("Visible thumbnail worker failed: {error}"))?
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
    digest.update(b"pimp-hover-preview-v2\0");
    digest.update(item.id.as_bytes());
    digest.update(b"\0");
    digest.update(item.sha256.as_bytes());
    digest.update(b"\0");
    digest.update(snapshot.length.to_le_bytes());
    digest.update(snapshot.modified_millis.unwrap_or_default().to_le_bytes());
    format!("{:x}", digest.finalize())
}

fn parse_hover_preview_frame_times(stderr: &[u8]) -> Vec<f64> {
    String::from_utf8_lossy(stderr)
        .lines()
        .filter(|line| line.contains("Parsed_showinfo"))
        .filter_map(|line| {
            let value = line.split("pts_time:").nth(1)?.split_whitespace().next()?;
            value
                .parse::<f64>()
                .ok()
                .filter(|timestamp| timestamp.is_finite())
        })
        .collect()
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

fn validate_hover_preview(
    directory: &Path,
    expected_fingerprint: &str,
) -> Result<HoverPreviewManifest, String> {
    let manifest_path = directory.join("manifest.json");
    let manifest: HoverPreviewManifest = serde_json::from_slice(
        &fs::read(&manifest_path)
            .map_err(|error| format!("Cannot read hover preview manifest: {error}"))?,
    )
    .map_err(|error| format!("Invalid hover preview manifest: {error}"))?;
    if manifest.version != 2 || manifest.fingerprint != expected_fingerprint {
        return Err("Hover preview cache fingerprint does not match the source".into());
    }
    if manifest.frame_files.len() != manifest.frame_times.len() || manifest.frame_files.is_empty() {
        return Err("Hover preview manifest has an invalid frame list".into());
    }
    let canonical_directory = directory
        .canonicalize()
        .map_err(|error| format!("Cannot resolve hover preview directory: {error}"))?;
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
        total_bytes = total_bytes.saturating_add(
            path.metadata()
                .map_err(|error| format!("Cannot inspect hover preview frame: {error}"))?
                .len(),
        );
        if total_bytes > 32 * 1024 * 1024 {
            return Err("Hover preview exceeds the 32 MB safety limit".into());
        }
    }
    Ok(manifest)
}

#[tauri::command]
fn load_hover_preview_statuses(
    app: AppHandle,
    media_ids: Vec<String>,
) -> Result<Vec<String>, String> {
    if media_ids.len() > 10_000 {
        return Err("Hover preview status request exceeds the 10,000 item safety limit".into());
    }
    let requested = media_ids.into_iter().collect::<HashSet<_>>();
    let settings = load_settings_file(&app)?;
    let catalogue = load_catalogue_file(&settings)?;
    let master = PathBuf::from(&settings.master_bin_path);
    let root = hover_preview_root(&settings)?;
    let mut ready = Vec::new();
    for item in catalogue.media.iter().filter(|item| {
        requested.contains(&item.id)
            && matches!(
                item.file_type.to_ascii_lowercase().as_str(),
                "mp4" | "mov" | "m4v" | "mxf" | "mts" | "m2ts" | "avi"
            )
    }) {
        let Ok(source) = fs_safe::resolve_existing(&master, &item.relative_path) else {
            continue;
        };
        let Ok(snapshot) = fs_safe::snapshot(&source) else {
            continue;
        };
        let fingerprint = hover_preview_fingerprint(item, &snapshot);
        if validate_hover_preview(&root.join(&fingerprint), &fingerprint).is_ok() {
            ready.push(item.id.clone());
        }
    }
    Ok(ready)
}

fn read_hover_preview(
    directory: &Path,
    expected_fingerprint: &str,
) -> Result<HoverPreview, String> {
    let manifest = validate_hover_preview(directory, expected_fingerprint)?;
    let mut frames = Vec::with_capacity(manifest.frame_files.len());
    for reference in &manifest.frame_files {
        let bytes = fs::read(directory.join(reference))
            .map_err(|error| format!("Cannot read hover preview frame: {error}"))?;
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

fn ensure_hover_preview(
    settings: &DeviceSettings,
    item: &MediaItem,
    source: &Path,
) -> Result<(PathBuf, String), String> {
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
        if validate_hover_preview(&final_directory, &fingerprint).is_ok() {
            return Ok((final_directory, fingerprint));
        }
        fs::remove_dir_all(&final_directory)
            .map_err(|error| format!("Cannot replace invalid hover preview cache: {error}"))?;
    }

    let temporary = root.join(format!(".generating-{}", Uuid::new_v4()));
    fs::create_dir(&temporary).map_err(|error| format!("Cannot stage hover preview: {error}"))?;
    let output_pattern = temporary.join("frame-%03d.jpg");
    let filter = format!(
        "fps={}/{duration_seconds},scale=320:-2:force_original_aspect_ratio=decrease,showinfo",
        frame_times.len()
    );
    let output = media_tool_command("ffmpeg")
        .args(["-v", "info", "-nostdin", "-i"])
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
    let actual_frame_times = parse_hover_preview_frame_times(&output.stderr);
    if actual_frame_times.len() != frame_times.len() {
        let _ = fs::remove_dir_all(&temporary);
        return Err("FFmpeg did not report every sampled hover-preview timestamp".into());
    }
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
        version: 2,
        media_id: item.id.clone(),
        fingerprint: fingerprint.clone(),
        duration_seconds,
        frame_times: actual_frame_times,
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
    Ok((final_directory, fingerprint))
}

fn generate_hover_preview(
    settings: &DeviceSettings,
    item: &MediaItem,
    source: &Path,
) -> Result<HoverPreview, String> {
    let (directory, fingerprint) = ensure_hover_preview(settings, item, source)?;
    read_hover_preview(&directory, &fingerprint)
}

#[tauri::command]
async fn load_hover_preview(app: AppHandle, media_id: String) -> Result<HoverPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<NativeState>();
        let _operation = begin_critical_operation(&state.operations, "hover preview generation")?;
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
async fn prepare_hover_preview(app: AppHandle, media_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<NativeState>();
        let _operation = begin_critical_operation(&state.operations, "hover preview preparation")?;
        if has_active_ingest(&snapshot_jobs(&app, &state)?) {
            return Err("Background preview preparation is deferred while ingest is active".into());
        }
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
        ensure_hover_preview(&settings, item, &source).map(|_| ())
    })
    .await
    .map_err(|error| format!("Hover preview preparation task failed: {error}"))?
}

#[tauri::command]
fn hover_preview_cache_usage(app: AppHandle) -> Result<u64, String> {
    let settings = load_settings_file(&app)?;
    Ok(preview_cache_usage(&hover_preview_root(&settings)?))
}

#[tauri::command]
async fn clear_hover_preview_cache(app: AppHandle) -> Result<u64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<NativeState>();
        let _operation = begin_critical_operation(&state.operations, "preview cache clear")?;
        let settings = load_settings_file(&app)?;
        let root = hover_preview_root(&settings)?;
        clear_hover_preview_cache_synchronized(&state.preview_generation, &root)
    })
    .await
    .map_err(|error| format!("Hover preview cache clear task failed: {error}"))?
}

fn clear_hover_preview_cache_synchronized(
    preview_generation: &Mutex<()>,
    root: &Path,
) -> Result<u64, String> {
    let _guard = preview_generation.lock().unwrap();
    clear_hover_preview_cache_contents(root)
}

fn clear_hover_preview_cache_contents(root: &Path) -> Result<u64, String> {
    for entry in fs::read_dir(&root)
        .map_err(|error| format!("Cannot inspect hover preview cache: {error}"))?
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_dir())
            && (path.join("manifest.json").is_file()
                || entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".generating-"))
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
                    "quit" => {
                        cancel_all_playback_proxy_processes(&app.state::<NativeState>());
                        app.exit(0);
                    }
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
                        } else {
                            let state = app.state::<NativeState>();
                            cancel_all_playback_proxy_processes(&state);
                            let inventory_operation = {
                                state
                                    .inventory_status
                                    .lock()
                                    .unwrap()
                                    .clone()
                                    .filter(|operation| {
                                    matches!(
                                        operation.state.as_str(),
                                        "queued" | "running" | "cancelling"
                                    )
                                })
                            };
                            if let Some(operation) = inventory_operation {
                                state
                                    .inventory_cancelled
                                    .lock()
                                    .unwrap()
                                    .insert(operation.id.clone());
                                diagnostic_log(
                                    &app,
                                    "INFO",
                                    "inventory-shutdown",
                                    &format!(
                                        "Cancellation requested for {} before application close",
                                        operation.id
                                    ),
                                );
                            }
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
            inventory_sd_card,
            cancel_inventory,
            cancel_background_operation,
            list_background_operations,
            reapprove_ingest_destinations,
            replan_ingest_destinations,
            list_removable_volumes,
            start_ingest,
            initialize_ingest_jobs,
            list_ingest_jobs,
            clear_completed_ingest_jobs,
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
            initialize_backup_jobs,
            list_backup_jobs,
            start_project_backup,
            retry_project_backup,
            verify_project_backup,
            test_project_backup_restore,
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
            prepare_playback_proxy,
            release_playback_proxy,
            export_premiere_manifest,
            reveal_media_in_explorer,
            open_media_folder,
            set_media_thumbnail_frame,
            load_thumbnail,
            load_thumbnail_sources,
            prepare_media_thumbnail,
            load_hover_preview_statuses,
            load_hover_preview,
            prepare_hover_preview,
            hover_preview_cache_usage,
            clear_hover_preview_cache
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kairix PIMP");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_backup_jobs_become_explicitly_interrupted_after_restart() {
        let mut jobs = [BackupJob {
            state: "copying".into(),
            ..BackupJob::default()
        }];
        assert!(mark_interrupted_backup_jobs(&mut jobs));
        assert_eq!(jobs[0].state, "interrupted");
        assert!(jobs[0]
            .error
            .as_deref()
            .is_some_and(|error| error.contains("retry")));
        assert!(!mark_interrupted_backup_jobs(&mut jobs));
    }

    fn temporary(name: &str) -> PathBuf {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".test-tmp")
            .join(format!("pimp-native-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn approved_file_for_replan(
        relative_path: &str,
        source_modified_millis: u128,
        planning_modified_millis: Option<u128>,
    ) -> ApprovedIngestFile {
        ApprovedIngestFile {
            inventory: InventoryFile {
                source_path: format!("card/{relative_path}"),
                source_relative_path: relative_path.into(),
                planned_relative_path: format!("2026-01-01/{relative_path}"),
                ..InventoryFile::default()
            },
            source_size: 64,
            source_modified_millis: Some(source_modified_millis),
            planning_modified_millis,
            source_sha256: Some("approved-hash".into()),
            source_volume_identity: Some("volume-a".into()),
            existing_media_id: None,
        }
    }

    #[test]
    fn date_reapproval_preserves_package_suffix_and_approved_evidence() {
        let root = temporary("date-reapproval-package");
        let mut settings = DeviceSettings {
            master_bin_path: root.to_string_lossy().into_owned(),
            date_folder_pattern: "yyyy-MM-dd".into(),
            ..DeviceSettings::default()
        };
        let first_millis = 1_767_268_800_000_u128;
        let package_millis = 1_767_355_200_000_u128;
        let mxf = approved_file_for_replan(
            "PRIVATE/AVCHD/BDMV/STREAM/clip.mxf",
            first_millis,
            Some(package_millis),
        );
        let xml = approved_file_for_replan(
            "PRIVATE/AVCHD/BDMV/CLIPINF/clip.xml",
            first_millis + 172_800_000,
            Some(package_millis),
        );
        let batch = Local::now();
        let expected_date = local_datetime_from_millis(Some(package_millis))
            .unwrap()
            .format("%Y-%m-%d")
            .to_string();
        let mxf_path = replan_approved_relative_path(
            &settings,
            &root,
            &mxf,
            "original-file-dates",
            &batch,
        )
        .unwrap();
        let xml_path = replan_approved_relative_path(
            &settings,
            &root,
            &xml,
            "original-file-dates",
            &batch,
        )
        .unwrap();
        assert!(mxf_path.starts_with(&expected_date));
        assert!(xml_path.starts_with(&expected_date));
        assert!(mxf_path.ends_with("PRIVATE/AVCHD/BDMV/STREAM/clip.mxf"));
        assert!(xml_path.ends_with("PRIVATE/AVCHD/BDMV/CLIPINF/clip.xml"));
        assert_eq!(mxf.source_sha256.as_deref(), Some("approved-hash"));
        assert_eq!(mxf.source_volume_identity.as_deref(), Some("volume-a"));

        settings.date_folder_pattern = "yyyyMMdd".into();
        assert!(replan_approved_relative_path(
            &settings,
            &root,
            &approved_file_for_replan("PRIVATE/clip.mxf", first_millis, None),
            "original-file-dates",
            &batch,
        )
        .unwrap_err()
        .contains("predates safe date reapproval"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn date_reapproval_keeps_independent_clip_dates_separate() {
        let root = temporary("date-reapproval-independent");
        let settings = DeviceSettings {
            master_bin_path: root.to_string_lossy().into_owned(),
            date_folder_pattern: "yyyy-MM-dd".into(),
            ..DeviceSettings::default()
        };
        let first = approved_file_for_replan("card-a/clip.mp4", 1_767_268_800_000, None);
        let second = approved_file_for_replan("card-b/clip.mp4", 1_767_441_600_000, None);
        let batch = Local::now();
        let first_path = replan_approved_relative_path(
            &settings,
            &root,
            &first,
            "original-file-dates",
            &batch,
        )
        .unwrap();
        let second_path = replan_approved_relative_path(
            &settings,
            &root,
            &second,
            "original-file-dates",
            &batch,
        )
        .unwrap();
        assert_ne!(Path::new(&first_path).components().next(), Path::new(&second_path).components().next());
        assert!(first_path.ends_with("card-a/clip.mp4"));
        assert!(second_path.ends_with("card-b/clip.mp4"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn approved_plan_revisions_preserve_lineage_and_leave_prior_plan_immutable() {
        let original_id = Uuid::new_v4().to_string();
        let original = ApprovedIngestPlan {
            schema_version: 1,
            id: original_id.clone(),
            lineage_id: original_id.clone(),
            revision: 0,
            ..ApprovedIngestPlan::default()
        };
        let revised = begin_approved_plan_revision(original.clone());
        assert_eq!(original.id, original_id);
        assert_eq!(original.revision, 0);
        assert_ne!(revised.id, original.id);
        assert_eq!(revised.lineage_id, original_id);
        assert_eq!(revised.revision, 1);
        assert_eq!(revised.inventory.plan_id, revised.id);
    }

    fn playback_probe(json: &str) -> PlaybackProbe {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn playback_proxy_validation_requires_finalized_h264_yuv420_and_aac() {
        let valid = playback_probe(
            r#"{"streams":[{"codec_type":"video","codec_name":"h264","pix_fmt":"yuv420p"},{"codec_type":"audio","codec_name":"aac"}],"format":{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"12.500"}}"#,
        );
        assert!(playback_proxy_probe_is_valid(&valid));

        let wrong_video = playback_probe(
            r#"{"streams":[{"codec_type":"video","codec_name":"hevc","pix_fmt":"yuv420p10le"}],"format":{"format_name":"mov,mp4","duration":"12.5"}}"#,
        );
        assert!(!playback_proxy_probe_is_valid(&wrong_video));
        let wrong_audio = playback_probe(
            r#"{"streams":[{"codec_type":"video","codec_name":"h264","pix_fmt":"yuv420p"},{"codec_type":"audio","codec_name":"pcm_s16le"}],"format":{"format_name":"mov,mp4","duration":"12.5"}}"#,
        );
        assert!(!playback_proxy_probe_is_valid(&wrong_audio));
        let unfinished = playback_probe(
            r#"{"streams":[{"codec_type":"video","codec_name":"h264","pix_fmt":"yuv420p"}],"format":{"format_name":"mov,mp4","duration":"0"}}"#,
        );
        assert!(!playback_proxy_probe_is_valid(&unfinished));
    }

    #[test]
    fn playback_proxy_progress_parses_ffmpeg_machine_output() {
        assert_eq!(playback_progress_millis("out_time_us=13450000"), Some(13_450));
        assert_eq!(playback_progress_millis("out_time=00:02:14.250000"), Some(134_250));
        assert_eq!(playback_progress_millis("progress=continue"), None);
    }

    #[test]
    fn playback_proxy_cache_honors_budget_and_protects_active_and_partial_files() {
        let root = temporary("playback-proxy-cache");
        let old = root.join("old.mp4");
        let protected = root.join("protected.mp4");
        let abandoned = root.join(".proxy-abandoned.mp4");
        let active_partial = root.join(".proxy-active.mp4");
        fs::write(&old, vec![1_u8; 32]).unwrap();
        fs::write(&protected, vec![2_u8; 32]).unwrap();
        fs::write(&abandoned, vec![3_u8; 32]).unwrap();
        fs::write(&active_partial, vec![4_u8; 32]).unwrap();
        let state = NativeState::default();
        state
            .playback_proxy_temporaries
            .lock()
            .unwrap()
            .insert(active_partial.clone());
        cleanup_abandoned_playback_temporaries(&state, &root).unwrap();
        assert!(!abandoned.exists());
        assert!(active_partial.exists());

        let protected_paths = HashSet::from([protected.clone(), active_partial.clone()]);
        enforce_playback_proxy_limit(&root, 1, &protected_paths).unwrap();
        assert!(!old.exists());
        assert!(protected.exists());
        assert!(active_partial.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn managed_playback_proxy_transcodes_hevc_and_cancels_without_touching_source() {
        if tool_version("ffmpeg").is_none() || tool_version("ffprobe").is_none() {
            return;
        }
        let root = temporary("playback-proxy-managed");
        let source = root.join("synthetic-hevc.mp4");
        let generated = media_tool_command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x180:rate=30",
                "-t",
                "1",
                "-c:v",
                "libx265",
                "-threads",
                "1",
                "-pix_fmt",
                "yuv420p",
                "-y",
            ])
            .arg(&source)
            .status();
        if !generated.is_ok_and(|status| status.success()) {
            let _ = fs::remove_dir_all(root);
            return;
        }
        let source_hash = sha256(&source).unwrap();
        let state = NativeState::default();
        let operation_id = "playback-proxy:managed-test";
        let operation = BackgroundOperation {
            id: operation_id.into(),
            kind: "playback-proxy".into(),
            title: "Playback proxy: synthetic-hevc.mp4".into(),
            state: "running".into(),
            stage: "Transcoding H.264/AAC preview".into(),
            current_item: Some("synthetic-hevc.mp4".into()),
            completed_items: 0,
            total_items: Some(1),
            bytes_completed: 0,
            bytes_total: None,
            counts: OperationCounts::default(),
            cancellable: true,
            pausable: false,
            retryable: false,
            error: None,
            started_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        state
            .playback_proxy_status
            .lock()
            .unwrap()
            .insert(operation_id.into(), operation.clone());
        let output = root.join("managed.mp4");
        run_playback_proxy_generation(&state, operation_id, &source, &output, Some(1_000))
            .unwrap();
        assert!(valid_playback_proxy(&output));
        assert_eq!(sha256(&source).unwrap(), source_hash);
        assert!(state.playback_proxy_processes.lock().unwrap().is_empty());

        let cancel_id = "playback-proxy:cancel-test";
        state
            .playback_proxy_status
            .lock()
            .unwrap()
            .insert(cancel_id.into(), BackgroundOperation { id: cancel_id.into(), ..operation });
        state
            .playback_proxy_cancelled
            .lock()
            .unwrap()
            .insert(cancel_id.into());
        let cancelled_output = root.join("cancelled.mp4");
        let error = run_playback_proxy_generation(
            &state,
            cancel_id,
            &source,
            &cancelled_output,
            Some(1_000),
        )
        .unwrap_err();
        assert!(error.contains("cancelled safely"));
        assert_eq!(
            state
                .playback_proxy_status
                .lock()
                .unwrap()
                .get(cancel_id)
                .unwrap()
                .state,
            "cancelled"
        );
        assert!(state.playback_proxy_processes.lock().unwrap().is_empty());
        assert_eq!(sha256(&source).unwrap(), source_hash);
        let _ = fs::remove_file(cancelled_output);
        fs::remove_dir_all(root).unwrap();
    }

    fn ingest_job(state: &str, file_state: &str) -> IngestJob {
        IngestJob {
            schema_version: 2,
            id: "job-1".into(),
            approved_plan_id: None,
            approved_plan_lineage_id: None,
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
                source_modified_millis: None,
                approved_sha256: None,
                source_volume_identity: None,
                duplicate_existing_relative_path: None,
                intentional_duplicate: false,
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
            source_modified_millis: None,
            approved_sha256: None,
            source_volume_identity: None,
            duplicate_existing_relative_path: None,
            intentional_duplicate: false,
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
    fn batch_tags_apply_only_to_catalogued_media_and_survive_file_edits() {
        let tags = normalized(&[" Race Day ".into(), "race day".into(), "Vlog".into()]);
        for kind in ["video", "audio", "image"] {
            let mut metadata = initial_batch_metadata(kind, &tags);
            assert_eq!(metadata.tags, vec!["Race Day", "Vlog"]);
            assert_eq!(metadata.edited_fields, vec!["tags"]);
            apply_metadata_patch(
                &mut metadata,
                &ProvisionalMetadataPatch {
                    tags: Some(vec!["Race Day".into(), "Vlog".into(), "Interview".into()]),
                    ..Default::default()
                },
            );
            assert_eq!(metadata.tags, vec!["Interview", "Race Day", "Vlog"]);
        }
        assert_eq!(
            initial_batch_metadata("camera-support", &tags),
            ProvisionalMetadata::default()
        );
        assert_eq!(
            initial_batch_metadata("unknown", &tags),
            ProvisionalMetadata::default()
        );
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
    fn background_preview_preparation_defers_for_active_ingest_only() {
        assert!(has_active_ingest(&[ingest_job("queued", "queued")]));
        assert!(has_active_ingest(&[ingest_job("copying", "copying")]));
        assert!(has_active_ingest(&[ingest_job("verifying", "verifying")]));
        assert!(!has_active_ingest(&[ingest_job("paused", "queued")]));
        assert!(!has_active_ingest(&[ingest_job("completed", "completed")]));
    }

    #[test]
    fn premiere_manifest_uses_validated_master_paths_and_attached_audio() {
        let (root, master, _, mut catalogue) = completion_fixture("premiere-manifest");
        let audio_path = master.join("20261024").join("clip.wav");
        fs::write(&audio_path, b"verified-audio").unwrap();
        let mut audio = new_media(
            &audio_path,
            "20261024/clip.wav".into(),
            None,
            sha256(&audio_path).unwrap(),
            None,
            "unknown".into(),
            false,
        )
        .unwrap();
        audio.id = "audio-a".into();
        catalogue.media[0].id = "video-a".into();
        catalogue.media[0].attached_audio_id = Some(audio.id.clone());
        catalogue.media.push(audio);
        let mut settings = DeviceSettings::default();
        settings.master_bin_path = master.to_string_lossy().into_owned();

        let manifest = build_premiere_import_manifest(
            &settings,
            &catalogue,
            &["video-a".into(), "audio-a".into()],
        )
        .unwrap();
        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.items.len(), 2);
        assert_eq!(manifest.items[0].bin_path, "20261024");
        assert_eq!(
            manifest.items[1].attached_to_media_id.as_deref(),
            Some("video-a")
        );
        assert!(Path::new(&manifest.items[0].absolute_path).is_absolute());
        let output = root.join("selection.pimp-premiere.json");
        write_new_premiere_manifest(&output, &manifest).unwrap();
        let first_bytes = fs::read(&output).unwrap();
        assert!(write_new_premiere_manifest(&output, &manifest).is_err());
        assert_eq!(fs::read(&output).unwrap(), first_bytes);

        fs::remove_file(&audio_path).unwrap();
        assert!(
            build_premiere_import_manifest(&settings, &catalogue, &["audio-a".into()]).is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn premiere_handoff_paths_strip_only_safe_short_drive_prefixes() {
        assert_eq!(
            compatible_windows_handoff_path(r"C:\Media\Unicode カード\clip 1.mp4"),
            r"C:\Media\Unicode カード\clip 1.mp4"
        );
        assert_eq!(
            compatible_windows_handoff_path(r"\\server\share\clip 1.mp4"),
            r"\\server\share\clip 1.mp4"
        );
        assert_eq!(
            compatible_windows_handoff_path(r"\\?\C:\Media\Unicode カード\clip 1.mp4"),
            r"C:\Media\Unicode カード\clip 1.mp4"
        );
        assert_eq!(
            compatible_windows_handoff_path(r"\\?\UNC\server\share\clip.mp4"),
            r"\\?\UNC\server\share\clip.mp4"
        );
        let long = format!(r"\\?\C:\Media\{}\clip.mp4", "x".repeat(260));
        assert_eq!(compatible_windows_handoff_path(&long), long);
        assert_eq!(
            compatible_windows_handoff_path(r"\\?\C:\Media\legacy.\clip.mp4"),
            r"\\?\C:\Media\legacy.\clip.mp4"
        );
    }

    #[test]
    fn explorer_boundary_converts_short_drive_and_unc_paths_only() {
        assert_eq!(
            explorer_boundary_path(r"\\?\C:\Media\Unicode カード, 1\clip.mp4").unwrap(),
            r"C:\Media\Unicode カード, 1\clip.mp4"
        );
        assert_eq!(
            explorer_boundary_path(r"\\?\UNC\server\share\clip.wav").unwrap(),
            r"\\server\share\clip.wav"
        );
        assert_eq!(
            explorer_boundary_path(r"\\server\share\clip.wav").unwrap(),
            r"\\server\share\clip.wav"
        );
        let long = format!(r"\\?\C:\Media\{}\clip.mp4", "x".repeat(260));
        assert!(explorer_boundary_path(&long)
            .unwrap_err()
            .contains("path length"));
        assert!(explorer_boundary_path(r"\\?\C:\Media\legacy.\clip.mp4").is_err());
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
    fn sd_grabber_filters_case_insensitive_mp4_and_wav_recursively() {
        let root = temporary("sd-filter");
        let card = root.join("card");
        fs::create_dir_all(card.join("PRIVATE/deep")).unwrap();
        fs::write(card.join("CLIP.MP4"), b"video").unwrap();
        fs::write(card.join("PRIVATE/deep/audio.WaV"), b"audio").unwrap();
        fs::write(card.join("PRIVATE/deep/metadata.XML"), b"metadata").unwrap();
        let allowed = HashSet::from(["mp4".to_string(), "wav".to_string()]);
        let inventory = build_source_inventory_filtered(
            &[card.to_string_lossy().into_owned()],
            Some(&allowed),
            true,
        );
        assert_eq!(inventory.files.len(), 2);
        assert!(inventory
            .files
            .iter()
            .any(|file| file.source_relative_path.ends_with("CLIP.MP4")));
        assert!(inventory
            .files
            .iter()
            .any(|file| file.source_relative_path.ends_with("audio.WaV")));
        assert!(inventory
            .excluded
            .iter()
            .any(|file| file.path.ends_with("metadata.XML")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn global_duplicate_requires_a_fresh_verified_local_copy() {
        let root = temporary("global-duplicate");
        let master = root.join("master");
        let existing = master.join("20261001/original.mp4");
        let incoming = root.join("renamed.mp4");
        fs::create_dir_all(existing.parent().unwrap()).unwrap();
        fs::write(&existing, b"same-bytes").unwrap();
        fs::write(&incoming, b"same-bytes").unwrap();
        let hash = sha256(&incoming).unwrap();
        let mut item = new_media(
            &existing,
            "20261001/original.mp4".into(),
            None,
            hash.clone(),
            None,
            "filesystem-modified".into(),
            false,
        )
        .unwrap();
        item.tags = vec!["keep-me".into()];
        item.project_ids = vec!["project-a".into()];
        let mut catalogue = Catalogue {
            media: vec![item.clone()],
            ..Default::default()
        };
        let mut settings = DeviceSettings::default();
        settings.master_bin_path = master.to_string_lossy().into_owned();
        let verified = classify_global_duplicate(
            &settings,
            &[&catalogue.media[0]],
            fs::metadata(&incoming).unwrap().len(),
            &hash,
        );
        assert_eq!(verified.0, "already-imported");
        assert_eq!(verified.1.as_deref(), Some(item.id.as_str()));
        assert_eq!(item.tags, vec!["keep-me"]);
        assert_eq!(item.project_ids, vec!["project-a"]);

        catalogue.media[0].observed_size = None;
        assert_eq!(
            classify_global_duplicate(
                &settings,
                &[&catalogue.media[0]],
                fs::metadata(&incoming).unwrap().len(),
                &hash,
            )
            .0,
            "already-imported"
        );

        fs::write(&incoming, b"other-data").unwrap();
        let different_hash = sha256(&incoming).unwrap();
        assert_eq!(
            classify_global_duplicate(
                &settings,
                &[&catalogue.media[0]],
                fs::metadata(&incoming).unwrap().len(),
                &different_hash,
            )
            .0,
            "new"
        );
        fs::write(&incoming, b"same-bytes").unwrap();

        fs::write(&existing, b"bad!-bytes").unwrap();
        assert_eq!(
            classify_global_duplicate(
                &settings,
                &[&catalogue.media[0]],
                fs::metadata(&incoming).unwrap().len(),
                &hash,
            )
            .0,
            "attention"
        );
        fs::remove_file(&existing).unwrap();
        assert_eq!(
            classify_global_duplicate(
                &settings,
                &[&catalogue.media[0]],
                fs::metadata(&incoming).unwrap().len(),
                &hash,
            )
            .0,
            "attention"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn approved_hash_rejects_same_size_content_changes_before_promotion() {
        let root = temporary("approved-plan-change");
        let source = root.join("clip.mp4");
        fs::write(&source, b"expected").unwrap();
        let snapshot = fs_safe::snapshot(&source).unwrap();
        let approved = ApprovedIngestFile {
            inventory: InventoryFile::default(),
            source_size: snapshot.length,
            source_modified_millis: snapshot.modified_millis,
            planning_modified_millis: snapshot.modified_millis,
            source_sha256: Some(sha256(&source).unwrap()),
            source_volume_identity: None,
            existing_media_id: None,
        };
        assert!(approved_source_snapshot_matches(&source, &approved));
        fs::write(&source, b"modified").unwrap();
        let changed_hash = sha256(&source).unwrap();
        assert!(verify_approved_source_hash(
            &changed_hash,
            approved.source_sha256.as_deref().unwrap()
        )
        .is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checksum_inventory_cancels_before_another_large_read() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let cancelled = AtomicBool::new(false);
        let mut source = std::io::Cursor::new(vec![7_u8; 3 * 1024 * 1024]);
        let result = sha256_reader_cancellable(
            &mut source,
            || cancelled.load(Ordering::SeqCst),
            |_| cancelled.store(true, Ordering::SeqCst),
        );
        assert_eq!(result.unwrap_err(), "Inventory cancelled safely");
        assert_eq!(source.position(), 1024 * 1024);
    }

    #[test]
    fn newer_inventory_cancels_the_stale_request_before_queueing() {
        let state = NativeState::default();
        let first = Uuid::new_v4().to_string();
        let second = Uuid::new_v4().to_string();
        begin_inventory_operation(&state, &first, "First scan").unwrap();
        update_inventory_operation(&state, &first, |operation| {
            operation.state = "running".into();
        });
        begin_inventory_operation(&state, &second, "Second scan").unwrap();
        assert!(state.inventory_cancelled.lock().unwrap().contains(&first));
        assert_eq!(
            state.inventory_status.lock().unwrap().as_ref().unwrap().id,
            second
        );
    }

    #[test]
    fn source_overlap_detection_is_bidirectional() {
        let root = temporary("source-overlap");
        let master = root.join("master");
        let child = master.join("DCIM");
        let separate = root.join("card");
        fs::create_dir_all(&child).unwrap();
        fs::create_dir_all(&separate).unwrap();
        let master = master.canonicalize().unwrap();
        let child = child.canonicalize().unwrap();
        let separate = separate.canonicalize().unwrap();
        assert!(paths_overlap(&master, &child));
        assert!(paths_overlap(&child, &master));
        assert!(!paths_overlap(&master, &separate));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn active_content_reservation_prevents_concurrent_duplicate_copy() {
        let mut job = ingest_job("copying", "copying");
        job.files[0].approved_sha256 = Some("same-hash".into());
        job.files[0].relative_path = "20261030/first.mp4".into();
        job.files[0].media_id = "media-first".into();
        let reservations = active_content_reservations(&[job]);
        assert_eq!(
            reservations.get("same-hash"),
            Some(&("20261030/first.mp4".into(), "media-first".into()))
        );
        let mut intentional = ingest_job("copying", "copying");
        intentional.files[0].approved_sha256 = Some("same-hash".into());
        intentional.files[0].intentional_duplicate = true;
        assert!(active_content_reservations(&[intentional]).is_empty());
    }

    #[test]
    fn ordinary_thumbnail_cache_evicts_oldest_files_only() {
        let root = temporary("thumbnail-limit");
        let cache = root.join("cache");
        fs::create_dir_all(cache.join("hover-previews/keep")).unwrap();
        fs::write(cache.join("a.jpg"), vec![1_u8; 8]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(cache.join("b.jpg"), vec![2_u8; 8]).unwrap();
        fs::write(cache.join("hover-previews/keep/frame.jpg"), b"preview").unwrap();
        assert_eq!(enforce_thumbnail_cache_limit(&cache, 8, None).unwrap(), 1);
        assert!(!cache.join("a.jpg").exists());
        assert!(cache.join("b.jpg").is_file());
        assert!(cache.join("hover-previews/keep/frame.jpg").is_file());
        fs::write(cache.join("older.jpg"), vec![3_u8; 8]).unwrap();
        let protected = cache.join("visible.jpg");
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&protected, vec![4_u8; 8]).unwrap();
        enforce_thumbnail_cache_limit(&cache, 8, Some(&protected)).unwrap();
        assert!(protected.is_file());
        assert!(!cache.join("older.jpg").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn thumbnail_generation_locks_are_per_media_identity() {
        let state = NativeState::default();
        let first = thumbnail_generation_lock(&state, "media-a");
        let same = thumbnail_generation_lock(&state, "media-a");
        let different = thumbnail_generation_lock(&state, "media-b");
        assert!(Arc::ptr_eq(&first, &same));
        assert!(!Arc::ptr_eq(&first, &different));
        let _first_guard = first.lock().unwrap();
        assert!(different.try_lock().is_ok());
    }

    #[test]
    fn missing_empty_and_invalid_thumbnail_cache_entries_require_regeneration() {
        let (root, _, _, mut catalogue) = completion_fixture("thumbnail-validity");
        let cache = root.join("cache");
        fs::create_dir_all(&cache).unwrap();
        let mut item = catalogue.media.remove(0);
        item.id = "media-a".into();
        item.thumbnail_path = Some("media-a.jpg".into());
        let thumbnail = cache.join("media-a.jpg");
        assert!(!thumbnail_reference_is_available(&cache, &item));
        fs::write(&thumbnail, []).unwrap();
        assert!(!thumbnail_reference_is_available(&cache, &item));
        fs::write(&thumbnail, b"not-a-jpeg").unwrap();
        assert!(!thumbnail_reference_is_available(&cache, &item));
        fs::write(&thumbnail, [0xFF, 0xD8, 0xFF, 0xD9]).unwrap();
        assert!(thumbnail_reference_is_available(&cache, &item));
        fs::remove_file(&thumbnail).unwrap();
        assert!(!thumbnail_reference_is_available(&cache, &item));
        item.thumbnail_path = Some("different.jpg".into());
        fs::write(&thumbnail, [0xFF, 0xD8, 0xFF, 0xD9]).unwrap();
        assert!(!thumbnail_reference_is_available(&cache, &item));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn custom_thumbnail_frame_rejects_invalid_or_out_of_range_times() {
        assert!(validate_thumbnail_frame(0.0, Some(10.0)).is_ok());
        assert!(validate_thumbnail_frame(10.0, Some(10.0)).is_ok());
        assert!(validate_thumbnail_frame(-0.1, Some(10.0)).is_err());
        assert!(validate_thumbnail_frame(10.1, Some(10.0)).is_err());
        assert!(validate_thumbnail_frame(f64::NAN, None).is_err());
        assert!(validate_thumbnail_frame(f64::INFINITY, None).is_err());
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
    fn completed_history_can_be_cleared_individually_or_together() {
        let mut jobs = (0..3)
            .map(|index| {
                let mut job = ingest_job("completed", "completed");
                job.id = format!("completed-{index}");
                job.failures.clear();
                job
            })
            .collect::<Vec<_>>();
        let recovery = HashSet::new();

        assert_eq!(
            clear_finalized_jobs(&mut jobs, Some("completed-1"), &recovery).unwrap(),
            1
        );
        assert_eq!(jobs.len(), 2);
        assert!(jobs.iter().all(|job| job.id != "completed-1"));
        assert_eq!(clear_finalized_jobs(&mut jobs, None, &recovery).unwrap(), 2);
        assert!(jobs.is_empty());
        assert_eq!(clear_finalized_jobs(&mut jobs, None, &recovery).unwrap(), 0);
    }

    #[test]
    fn clearing_history_preserves_every_recoverable_or_unresolved_job() {
        let mut finalized = ingest_job("completed", "completed");
        finalized.id = "safe-completed".into();
        finalized.failures.clear();

        let mut pending_metadata = ingest_job("completed", "completed");
        pending_metadata.id = "pending-metadata".into();
        pending_metadata.failures.clear();
        pending_metadata.files[0].pending_metadata = Some(ProvisionalMetadata {
            notes: "must survive".into(),
            ..Default::default()
        });

        let mut failed = ingest_job("failed", "failed");
        failed.id = "failed".into();
        let mut cancelled = ingest_job("cancelled", "cancelled");
        cancelled.id = "cancelled-retryable".into();
        cancelled.files[0].pending_metadata = Some(ProvisionalMetadata {
            tags: vec!["retry".into()],
            ..Default::default()
        });
        let mut interrupted = ingest_job("interrupted", "interrupted");
        interrupted.id = "interrupted-with-journal".into();
        let mut active = ingest_job("copying", "copying");
        active.id = "active".into();
        let mut recovery = HashSet::new();
        recovery.insert(interrupted.id.clone());
        let mut jobs = vec![
            finalized,
            pending_metadata,
            failed,
            cancelled,
            interrupted,
            active,
        ];

        assert_eq!(clear_finalized_jobs(&mut jobs, None, &recovery).unwrap(), 1);
        assert_eq!(jobs.len(), 5);
        assert!(jobs.iter().any(|job| job.id == "pending-metadata"));
        assert!(jobs.iter().any(|job| job.id == "failed"));
        assert!(jobs.iter().any(|job| job.id == "cancelled-retryable"));
        assert!(jobs.iter().any(|job| job.id == "interrupted-with-journal"));
        assert!(jobs.iter().any(|job| job.id == "active"));
    }

    #[test]
    fn clearing_rechecks_eligibility_and_persists_the_remaining_store() {
        let root = temporary("clear-completed-history");
        let store = root.join("ingest-jobs.json");
        let mut completing = ingest_job("verifying", "verifying");
        completing.id = "race".into();
        let mut completed = ingest_job("completed", "completed");
        completed.id = "complete".into();
        completed.failures.clear();
        let mut jobs = vec![completing.clone(), completed];
        let recovery = HashSet::new();

        assert!(clear_finalized_jobs(&mut jobs, Some("race"), &recovery).is_err());
        assert_eq!(clear_finalized_jobs(&mut jobs, None, &recovery).unwrap(), 1);
        completing.state = "completed".into();
        completing.files[0].state = "completed".into();
        completing.failures.clear();
        jobs[0] = completing;
        assert_eq!(clear_finalized_jobs(&mut jobs, None, &recovery).unwrap(), 1);
        write_json(&store, &jobs).unwrap();

        let restarted: Vec<IngestJob> = read_json(&store).unwrap();
        assert!(restarted.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_evidence_blocks_an_otherwise_completed_job() {
        let mut completed = ingest_job("completed", "completed");
        completed.id = "completed-with-journal".into();
        completed.failures.clear();
        let mut jobs = vec![completed];
        let recovery = HashSet::from(["completed-with-journal".to_string()]);

        assert!(
            clear_finalized_jobs(&mut jobs, Some("completed-with-journal"), &recovery).is_err()
        );
        assert_eq!(clear_finalized_jobs(&mut jobs, None, &recovery).unwrap(), 0);
        assert_eq!(jobs.len(), 1);
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
        assert_eq!(
            support_url("test-builds"),
            Some("https://github.com/Dubcodes/kairix-pimp/actions/workflows/windows-beta.yml")
        );
        assert_eq!(
            support_url("website"),
            Some("https://www.dubcodesmedia.com/kairix/kairix-pimp")
        );
        assert_eq!(support_url("https://example.invalid"), None);
    }

    #[test]
    fn updater_is_a_higher_beta_and_fails_closed_without_a_public_key() {
        assert_eq!(env!("CARGO_PKG_VERSION"), "0.1.1-6");
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
            let mut coordinator = OperationCoordinator::default();
            assert!(begin_update_gate(&mut coordinator, &[ingest_job(state, state)], &[]).is_err());
            assert!(!coordinator.update_installing);
        }

        for state in ["failed", "cancelled"] {
            let mut coordinator = OperationCoordinator::default();
            assert!(begin_update_gate(&mut coordinator, &[ingest_job(state, state)], &[]).is_err());
            assert!(!coordinator.update_installing);
        }

        let mut completed = ingest_job("completed", "completed");
        completed.failures.clear();
        let gate = Mutex::new(OperationCoordinator::default());
        {
            let mut coordinator = gate.lock().unwrap();
            begin_update_gate(&mut coordinator, &[completed], &[]).unwrap();
        }
        assert!(gate.lock().unwrap().update_installing);
        assert!(begin_critical_operation(&gate, "library scan").is_err());
        end_update_gate(&gate);
        assert!(!gate.lock().unwrap().update_installing);
    }

    #[test]
    fn update_gate_waits_for_every_critical_filesystem_operation() {
        let gate = Mutex::new(OperationCoordinator::default());
        let operation = begin_critical_operation(&gate, "project backup").unwrap();
        assert!(begin_update_gate(&mut gate.lock().unwrap(), &[], &[]).is_err());
        drop(operation);
        begin_update_gate(&mut gate.lock().unwrap(), &[], &[]).unwrap();
        assert!(gate.lock().unwrap().update_installing);
    }

    #[test]
    fn update_gate_rejects_a_durable_active_backup_job() {
        let root = temporary("update-active-backup");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("project.prproj"), b"project").unwrap();
        let job = backup::plan_backup("project-a", "Project A", &source, &destination).unwrap();
        let mut coordinator = OperationCoordinator::default();
        assert!(begin_update_gate(&mut coordinator, &[], &[job]).is_err());
        assert!(!coordinator.update_installing);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unchanged_full_verification_emits_no_redundant_evidence() {
        let root = temporary("unchanged-full-verification");
        let source = root.join("clip.mp4");
        fs::write(&source, b"verified bytes").unwrap();
        let snapshot = fs_safe::snapshot(&source).unwrap();
        let hash = sha256(&source).unwrap();
        let item = new_media(
            &source,
            "clip.mp4".into(),
            None,
            hash.clone(),
            None,
            "filesystem-modified".into(),
            false,
        )
        .unwrap();
        let catalogue_root = root.join("catalogue");
        catalogue::append(
            &catalogue_root,
            "desktop",
            vec![CatalogueEvent::MediaRegistered {
                media: item.clone(),
            }],
        )
        .unwrap();
        let first_observation =
            evidence_observation_event(&item, "desktop", &snapshot, hash.clone()).unwrap();
        catalogue::append(&catalogue_root, "desktop", vec![first_observation]).unwrap();
        let event_stream = catalogue_root.join("events").join("desktop.jsonl");
        let before = event_stream.metadata().unwrap().len();
        let current = catalogue::load(&catalogue_root).unwrap();
        let current_item = current
            .media
            .iter()
            .find(|media| media.id == item.id)
            .unwrap();
        let repeated = evidence_observation_event(current_item, "desktop", &snapshot, hash);
        assert!(repeated.is_none());
        catalogue::append(&catalogue_root, "desktop", repeated.into_iter().collect()).unwrap();
        assert_eq!(event_stream.metadata().unwrap().len(), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn full_verification_records_same_size_changes_and_recovery() {
        let root = temporary("full-verification-change-recovery");
        let source = root.join("clip.mp4");
        fs::write(&source, b"expected").unwrap();
        let expected_hash = sha256(&source).unwrap();
        let mut item = new_media(
            &source,
            "clip.mp4".into(),
            None,
            expected_hash.clone(),
            None,
            "filesystem-modified".into(),
            false,
        )
        .unwrap();
        fs::write(&source, b"modified").unwrap();
        let changed_snapshot = fs_safe::snapshot(&source).unwrap();
        let changed_hash = sha256(&source).unwrap();
        let changed =
            evidence_observation_event(&item, "desktop", &changed_snapshot, changed_hash.clone())
                .unwrap();
        assert!(matches!(
            changed,
            CatalogueEvent::MediaEvidenceObserved {
                needs_review: true,
                ..
            }
        ));
        item.integrity_by_device.insert(
            "desktop".into(),
            catalogue::IntegrityEvidence {
                observed_size: changed_snapshot.length,
                observed_modified_millis: changed_snapshot
                    .modified_millis
                    .map(|value| value as u64),
                observed_sha256: Some(changed_hash),
                needs_review: true,
            },
        );
        fs::write(&source, b"expected").unwrap();
        let recovered_snapshot = fs_safe::snapshot(&source).unwrap();
        let recovered =
            evidence_observation_event(&item, "desktop", &recovered_snapshot, expected_hash)
                .unwrap();
        assert!(matches!(
            recovered,
            CatalogueEvent::MediaEvidenceObserved {
                needs_review: false,
                ..
            }
        ));
        fs::remove_dir_all(root).unwrap();
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
        let path = master.join("20261027").join("DJI clip, kōrero 音声.WAV");
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
        append_events(&settings, vec![CatalogueEvent::MediaRegistered { media }]).unwrap();

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
    fn every_explicit_ingest_date_choice_overrides_both_saved_modes() {
        let batch = Local.with_ymd_and_hms(2026, 11, 5, 12, 34, 56).unwrap();
        let modified = Local.with_ymd_and_hms(2026, 10, 20, 8, 15, 1).unwrap();
        for saved_mode in ["today", "original-file-dates"] {
            let mut settings = DeviceSettings::default();
            settings.ingest_date_organization = saved_mode.into();
            settings.date_folder_pattern = "yyyymmdd".into();
            for (choice, expected) in [
                ("today", "20261105"),
                ("yesterday", "20261104"),
                ("2026-10-31", "20261031"),
                ("original-file-dates", "20261020"),
            ] {
                assert_eq!(
                    destination_date_from_modified(
                        &settings,
                        Some(modified),
                        &Some(choice.into()),
                        &batch,
                    )
                    .unwrap()
                    .0,
                    expected,
                    "saved mode {saved_mode}, explicit choice {choice}",
                );
            }
        }
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
            [(
                "project-a".into(),
                Some(directory.to_string_lossy().into_owned()),
            )],
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
        let conflict =
            project_identity_for_directory(&state_path, &settings, &directory, Some("project-c"))
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
        let event = |device: &str, sequence: u64, event: CatalogueEvent| catalogue::EventEnvelope {
            schema_version: catalogue::EVENT_SCHEMA_VERSION,
            event_id: format!("{device}-{sequence}"),
            device_id: device.into(),
            sequence,
            occurred_at: "2026-10-28T00:00:00Z".into(),
            observed: BTreeMap::new(),
            event,
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
        let manifest: serde_json::Value = read_json(&directory.join(".pimp-project.json")).unwrap();
        assert_eq!(manifest["customField"], "preserved");
        assert_eq!(manifest["media"][0]["id"], canonical_id);
        assert!(project_manifest_drift_requests(&settings, &catalogue).is_empty());
        assert!(
            !write_project_manifest(&directory, &catalogue.projects[0], &catalogue.media).unwrap()
        );
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
            (
                "Adobe Premiere Pro Auto-Save/backup.prproj",
                b"backup".as_slice(),
            ),
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
        assert_eq!(
            inventory.items[0].relative_path,
            "Adobe Premiere Pro Auto-Save/backup.prproj"
        );
        assert!(inventory
            .items
            .iter()
            .any(|item| item.category == "premiere-auto-save"));
        assert!(inventory.items.iter().any(|item| item.category == "video"));
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
            fs::write(directory.join(format!("asset-{index}.mp4")), b"media").unwrap();
        }

        let error =
            inventory_project_directory_with_limits("project-a", "Budget Test", &directory, 3, 64)
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
    fn hover_preview_uses_actual_ffmpeg_sample_timestamps() {
        let stderr = br#"
[Parsed_showinfo_2] n:   0 pts: 1001 pts_time:10.0100 duration: 1001
[Parsed_showinfo_2] n:   1 pts: 4338 pts_time:43.3804 duration: 1001
[Parsed_showinfo_2] n:   2 pts: 7675 pts_time:76.7508 duration: 1001
"#;
        assert_eq!(
            parse_hover_preview_frame_times(stderr),
            vec![10.01, 43.3804, 76.7508]
        );
        assert!(
            parse_hover_preview_frame_times(b"[Parsed_showinfo_2] pts_time:not-a-number")
                .is_empty()
        );
    }

    #[test]
    fn hover_preview_cache_is_fingerprinted_validated_and_budgeted() {
        let root = temporary("hover-preview-cache");
        let first = root.join("first");
        let second = root.join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        let manifest = HoverPreviewManifest {
            version: 2,
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
    fn hover_preview_cache_clear_waits_for_generation_and_stays_scoped() {
        let parent = temporary("hover-preview-clear");
        let root = parent.join("cache");
        let generating = root.join(".generating-active");
        let complete = root.join("complete");
        let unrelated_cache_directory = root.join("unrelated");
        let master_media = parent.join("master-clip.mp4");
        fs::create_dir_all(&generating).unwrap();
        fs::create_dir_all(&complete).unwrap();
        fs::create_dir_all(&unrelated_cache_directory).unwrap();
        fs::write(generating.join("frame-001.jpg"), b"temporary").unwrap();
        fs::write(complete.join("manifest.json"), b"{}").unwrap();
        fs::write(master_media.clone(), b"original-media").unwrap();

        let lock = std::sync::Arc::new(Mutex::new(()));
        let generation_guard = lock.lock().unwrap();
        let thread_lock = lock.clone();
        let thread_root = root.clone();
        let clear = std::thread::spawn(move || {
            clear_hover_preview_cache_synchronized(&thread_lock, &thread_root).unwrap()
        });
        std::thread::sleep(std::time::Duration::from_millis(25));
        assert!(generating.exists());
        drop(generation_guard);
        assert_eq!(clear.join().unwrap(), 0);

        assert!(!generating.exists());
        assert!(!complete.exists());
        assert!(unrelated_cache_directory.exists());
        assert_eq!(fs::read(master_media).unwrap(), b"original-media");
        fs::remove_dir_all(parent).unwrap();
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
        let expected = directory
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
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
            persist_project_manifest_intents(&first_path, [("project-a".into(), None)]).unwrap();
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
