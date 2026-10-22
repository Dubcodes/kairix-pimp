use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{DateTime, Duration, Local, Utc};
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
    #[serde(default)]
    initialized_catalogue_path: String,
    #[serde(default)]
    project_paths: HashMap<String, String>,
}
fn default_date_mode() -> String {
    "original-file-dates".into()
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
            initialized_catalogue_path: String::new(),
            project_paths: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TechnicalMetadata {
    size_bytes: u64,
    duration_seconds: Option<f64>,
    width: Option<u32>,
    height: Option<u32>,
    codec: Option<String>,
    frame_rate: Option<f64>,
    has_audio: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MediaItem {
    id: String,
    relative_path: String,
    original_filename: String,
    sha256: String,
    capture_date: Option<String>,
    capture_date_provenance: String,
    ingest_date: String,
    file_type: String,
    tags: Vec<String>,
    project_ids: Vec<String>,
    notes: String,
    technical: TechnicalMetadata,
    thumbnail_path: Option<String>,
    attached_audio_id: Option<String>,
    available: bool,
    needs_review: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectRecord {
    id: String,
    name: String,
    working_directory: String,
    media_ids: Vec<String>,
    notes: String,
    tags: Vec<String>,
    created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalogue {
    #[serde(default)]
    media: Vec<MediaItem>,
    #[serde(default)]
    projects: Vec<ProjectRecord>,
    #[serde(default)]
    updated_at: String,
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
        return Ok(T::default());
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
    if !path.exists() {
        return fs::rename(&temp, path).map_err(|e| e.to_string());
    }
    let backup = parent.join(format!(
        ".{}.{}.backup",
        path.file_name().unwrap().to_string_lossy(),
        Uuid::new_v4()
    ));
    fs::rename(path, &backup).map_err(|e| e.to_string())?;
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::rename(&backup, path);
        let _ = fs::remove_file(&temp);
        return Err(format!("Could not promote metadata update: {error}"));
    }
    fs::remove_file(backup).map_err(|e| e.to_string())
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
fn catalogue_path(settings: &DeviceSettings) -> Result<PathBuf, String> {
    Ok(catalogue_dir(settings)?.join("catalogue.json"))
}
fn load_catalogue_file(settings: &DeviceSettings) -> Result<Catalogue, String> {
    read_json(&catalogue_path(settings)?)
}
fn save_catalogue_file(settings: &DeviceSettings, catalogue: &mut Catalogue) -> Result<(), String> {
    catalogue.updated_at = Utc::now().to_rfc3339();
    write_json(&catalogue_path(settings)?, catalogue)
}
fn append_event(
    settings: &DeviceSettings,
    event_type: &str,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let path = catalogue_dir(settings)?
        .join("events")
        .join(format!("{}.jsonl", settings.device_id));
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let envelope = serde_json::json!({"schemaVersion":1,"eventId":Uuid::new_v4(),"deviceId":settings.device_id,"occurredAt":Utc::now().to_rfc3339(),"type":event_type,"payload":payload});
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    writeln!(file, "{}", envelope).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())
}

#[tauri::command]
fn load_settings(app: AppHandle) -> Result<DeviceSettings, String> {
    load_settings_file(&app)
}

#[tauri::command]
fn save_settings(app: AppHandle, mut settings: DeviceSettings) -> Result<DeviceSettings, String> {
    if settings.master_bin_path.trim().is_empty()
        || settings.working_projects_path.trim().is_empty()
    {
        return Err("Master Bin and Working Projects directories are required".into());
    }
    fs::create_dir_all(settings.master_bin_path.trim())
        .map_err(|e| format!("Cannot create Master Bin: {e}"))?;
    fs::create_dir_all(settings.working_projects_path.trim())
        .map_err(|e| format!("Cannot create Working Projects directory: {e}"))?;
    let next_catalogue = catalogue_dir(&settings)?.to_string_lossy().to_string();
    let current = load_settings_file(&app)?;
    if !current.initialized_catalogue_path.is_empty()
        && current.initialized_catalogue_path != next_catalogue
        && Path::new(&current.initialized_catalogue_path)
            .join("catalogue.json")
            .exists()
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
    if settings.device_id.is_empty() {
        settings.device_id = current.device_id;
    }
    write_json(&settings_path(&app)?, &settings)?;
    if !catalogue_path(&settings)?.exists() {
        save_catalogue_file(&settings, &mut Catalogue::default())?;
    }
    write_json(
        &catalogue_dir(&settings)?.join("shared-settings.json"),
        &serde_json::json!({"schemaVersion":1,"ingestDateOrganization":settings.ingest_date_organization,"updatedAt":Utc::now().to_rfc3339(),"updatedBy":settings.device_id}),
    )?;
    Ok(settings)
}

#[tauri::command]
fn load_catalogue(app: AppHandle) -> Result<Catalogue, String> {
    let settings = load_settings_file(&app)?;
    let mut catalogue = load_catalogue_file(&settings)?;
    let master = PathBuf::from(&settings.master_bin_path);
    for item in &mut catalogue.media {
        item.available = master.join(&item.relative_path).is_file();
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
) -> (String, Option<String>, String) {
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
    (
        chosen.format("%Y-%m-%d").to_string(),
        capture,
        if modified.is_some() {
            "filesystem".into()
        } else {
            "unknown".into()
        },
    )
}
fn copy_verified(
    app: &AppHandle,
    state: &NativeState,
    job_id: &str,
    source: &Path,
    destination: &Path,
    base_bytes: u64,
    total_bytes: u64,
    file_index: usize,
    file_total: usize,
) -> Result<String, String> {
    let source_hash = sha256(source)?;
    let temp_dir = destination.parent().unwrap().join(".pimp-incoming");
    fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
    let temp = temp_dir.join(format!("{}.part", Uuid::new_v4()));
    let mut input = File::open(source).map_err(|e| e.to_string())?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    let mut copied = 0u64;
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        if state.cancelled_jobs.lock().unwrap().contains(job_id) {
            drop(output);
            let _ = fs::remove_file(&temp);
            return Err("__cancelled__".into());
        }
        let read = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .map_err(|e| e.to_string())?;
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
    output.sync_all().map_err(|e| e.to_string())?;
    drop(output);
    if sha256(&temp)? != source_hash {
        let _ = fs::remove_file(&temp);
        return Err("Independent SHA-256 verification failed".into());
    }
    fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
    if destination.exists() {
        let _ = fs::remove_file(&temp);
        return Err("__race__".into());
    }
    fs::rename(&temp, destination).map_err(|e| format!("Cannot promote verified copy: {e}"))?;
    Ok(source_hash)
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
    let mut catalogue = load_catalogue_file(&settings)?;
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
            destination_date(&settings, source, &request.date_override);
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
        let destination = master.join(&date_folder).join(&filename);
        let relative = destination
            .strip_prefix(&master)
            .map_err(|_| "Destination escaped Master Bin")?
            .to_string_lossy()
            .replace('\\', "/");
        if destination.exists() {
            match(sha256(source),sha256(&destination)){(Ok(a),Ok(b))if a==b=>{result.duplicates.push(source.to_string_lossy().to_string());if !catalogue.media.iter().any(|m|m.relative_path==relative){let item=new_media(&destination,relative,b,capture,provenance,false)?;catalogue.media.push(item.clone());result.imported.push(item)}},_=>result.conflicts.push(IngestConflict{source_path:source.to_string_lossy().to_string(),destination_path:destination.to_string_lossy().to_string(),reason:"A different file already uses this destination name. Skip it, choose another date, or explicitly rename the incoming copy.".into()})}
            completed_bytes += source_size;
            continue;
        }
        match copy_verified(&app,&state,&request.job_id,source,&destination,completed_bytes,total_bytes,index,files.len()){Ok(hash)=>{let item=new_media(&destination,relative,hash,capture,provenance,false)?;append_event(&settings,"media.registered",&serde_json::to_value(&item).unwrap())?;catalogue.media.push(item.clone());result.imported.push(item)},Err(error)if error=="__cancelled__"=>{result.cancelled=true;break},Err(error)if error=="__race__"=>result.conflicts.push(IngestConflict{source_path:source.to_string_lossy().to_string(),destination_path:destination.to_string_lossy().to_string(),reason:"Another operation created the destination while copying; the verified temporary copy was discarded.".into()}),Err(error)=>result.failures.push(format!("{}: {error}",source.display()))}
        completed_bytes += source_size;
    }
    pair_audio(&mut catalogue);
    save_catalogue_file(&settings, &mut catalogue)?;
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
        &directory.join(".pimp-project.json"),
        &serde_json::json!({
            "schemaVersion": 1,
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
    let mut catalogue = load_catalogue_file(&settings)?;
    for item in catalogue
        .media
        .iter_mut()
        .filter(|m| update.media_ids.contains(&m.id))
    {
        if let Some(tags) = &update.tags {
            item.tags = normalized(tags)
        }
        if let Some(projects) = &update.project_ids {
            item.project_ids = normalized(projects)
        }
        if let Some(tags) = &update.add_tags {
            item.tags = normalized(&[item.tags.clone(), tags.clone()].concat())
        }
        if let Some(projects) = &update.add_project_ids {
            item.project_ids = normalized(&[item.project_ids.clone(), projects.clone()].concat())
        }
        if let Some(notes) = &update.notes {
            item.notes = notes.clone()
        }
        item.needs_review = false;
        append_event(
            &settings,
            "media.updated",
            &serde_json::to_value(&item).unwrap(),
        )?;
    }
    let media = &catalogue.media;
    for project in &mut catalogue.projects {
        project.media_ids = media
            .iter()
            .filter(|item| item.project_ids.contains(&project.id))
            .map(|item| item.id.clone())
            .collect();
        if let Some(path) = settings.project_paths.get(&project.id) {
            write_project_manifest(Path::new(path), project, media)?;
        }
    }
    save_catalogue_file(&settings, &mut catalogue)?;
    Ok(catalogue)
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
    let project = ProjectRecord {
        id: Uuid::new_v4().to_string(),
        name: name.trim().into(),
        working_directory: ".".into(),
        media_ids: vec![],
        notes: String::new(),
        tags: vec![],
        created_at: Utc::now().to_rfc3339(),
    };
    write_project_manifest(&directory, &project, &[])?;
    settings
        .project_paths
        .insert(project.id.clone(), working_directory);
    write_json(&settings_path(&app)?, &settings)?;
    let _guard = state.catalogue_write.lock().unwrap();
    let mut catalogue = load_catalogue_file(&settings)?;
    catalogue.projects.push(project.clone());
    append_event(
        &settings,
        "project.upserted",
        &serde_json::to_value(&project).unwrap(),
    )?;
    save_catalogue_file(&settings, &mut catalogue)?;
    Ok(project)
}
fn pair_audio(catalogue: &mut Catalogue) {
    let audios: HashMap<String, String> = catalogue
        .media
        .iter()
        .filter(|m| m.file_type == "wav")
        .map(|m| {
            (
                Path::new(&m.original_filename)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_ascii_lowercase(),
                m.id.clone(),
            )
        })
        .collect();
    for media in catalogue
        .media
        .iter_mut()
        .filter(|m| matches!(m.file_type.as_str(), "mp4" | "mov" | "m4v"))
    {
        let stem = Path::new(&media.original_filename)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        media.attached_audio_id = audios.get(&stem).cloned();
    }
}
#[tauri::command]
async fn rescan(app: AppHandle, state: State<'_, NativeState>) -> Result<usize, String> {
    let settings = load_settings_file(&app)?;
    let master = PathBuf::from(&settings.master_bin_path);
    let _guard = state.catalogue_write.lock().unwrap();
    let mut catalogue = load_catalogue_file(&settings)?;
    let known: HashSet<_> = catalogue
        .media
        .iter()
        .map(|m| m.relative_path.clone())
        .collect();
    let mut added = 0;
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
        if known.contains(&relative) {
            continue;
        }
        let before = path.metadata().map_err(|e| e.to_string())?;
        std::thread::sleep(std::time::Duration::from_millis(250));
        let after = path.metadata().map_err(|e| e.to_string())?;
        if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
            continue;
        }
        let hash = sha256(path)?;
        let (_, capture, provenance) = destination_date(&settings, path, &None);
        let item = new_media(path, relative, hash, capture, provenance, true)?;
        append_event(
            &settings,
            "media.discovered",
            &serde_json::to_value(&item).unwrap(),
        )?;
        catalogue.media.push(item);
        added += 1
    }
    pair_audio(&mut catalogue);
    save_catalogue_file(&settings, &mut catalogue)?;
    Ok(added)
}
#[tauri::command]
async fn enrich_media(app: AppHandle, state: State<'_, NativeState>) -> Result<usize, String> {
    let settings = load_settings_file(&app)?;
    let master = PathBuf::from(&settings.master_bin_path);
    let cache = PathBuf::from(&settings.thumbnail_cache_path);
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let _guard = state.catalogue_write.lock().unwrap();
    let mut catalogue = load_catalogue_file(&settings)?;
    let mut changed = 0;
    for item in &mut catalogue.media {
        let source = master.join(&item.relative_path);
        if !source.is_file() {
            item.available = false;
            continue;
        }
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
                    item.technical.has_audio =
                        Some(streams.iter().any(|stream| stream["codec_type"] == "audio"));
                    item.technical.duration_seconds = value["format"]["duration"]
                        .as_str()
                        .and_then(|value| value.parse().ok());
                    if let Some(video) = video {
                        item.technical.width = video["width"].as_u64().map(|value| value as u32);
                        item.technical.height = video["height"].as_u64().map(|value| value as u32);
                        item.technical.codec = video["codec_name"].as_str().map(String::from);
                        item.technical.frame_rate =
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
        if matches!(item.file_type.as_str(), "mp4" | "mov" | "m4v") {
            let thumbnail = cache.join(format!("{}.jpg", item.id));
            if !thumbnail.exists() {
                let _ = Command::new("ffmpeg")
                    .args(["-y", "-ss", "0.1", "-i"])
                    .arg(&source)
                    .args(["-frames:v", "1", "-vf", "scale=640:-2", "-q:v", "4"])
                    .arg(&thumbnail)
                    .output();
            }
            if thumbnail.is_file() {
                item.thumbnail_path = Some(thumbnail.to_string_lossy().to_string());
            }
        }
        changed += 1;
    }
    save_catalogue_file(&settings, &mut catalogue)?;
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
    Ok(PathBuf::from(settings.master_bin_path)
        .join(&item.relative_path)
        .to_string_lossy()
        .to_string())
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
    let Some(path) = &item.thumbnail_path else {
        return Ok(None);
    };
    let path = PathBuf::from(path);
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
}
