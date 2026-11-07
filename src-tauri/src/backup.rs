use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::{fs_safe, sha256};

pub const BACKUP_SCHEMA_VERSION: u32 = 1;
pub const BACKUP_MANIFEST_NAME: &str = "kairix-pimp-backup-manifest.json";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub schema_version: u32,
    pub backup_id: String,
    pub project_id: String,
    pub created_at: String,
    pub source_file_count: usize,
    pub source_bytes: u64,
    #[serde(default)]
    pub collection_complete: bool,
    #[serde(default)]
    pub assets: Vec<BackupAsset>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackupAsset {
    pub relative_path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackupJob {
    pub schema_version: u32,
    pub id: String,
    pub project_id: String,
    pub project_name: String,
    pub source_root: String,
    pub destination_root: String,
    pub generation_path: String,
    #[serde(default)]
    pub generation_relative_path: String,
    pub destination_volume_id: String,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub files_completed: usize,
    #[serde(default)]
    pub bytes_completed: u64,
    #[serde(default)]
    pub current_file: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub manifest: BackupManifest,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RestoreTestResult {
    pub restore_path: String,
    pub files_verified: usize,
    pub bytes_verified: u64,
    pub verified_at: String,
}

pub fn plan_backup(
    project_id: &str,
    project_name: &str,
    source_root: &Path,
    destination_root: &Path,
) -> Result<BackupJob, String> {
    let source_root = source_root
        .canonicalize()
        .map_err(|error| format!("Cannot resolve project source: {error}"))?;
    let destination_root = destination_root
        .canonicalize()
        .map_err(|error| format!("Cannot resolve backup destination: {error}"))?;
    if !destination_root.is_dir() {
        return Err("Backup destination is not a directory".into());
    }
    if fs_safe::within(&source_root, &destination_root)
        || fs_safe::within(&destination_root, &source_root)
    {
        return Err("Backup source and destination must not overlap".into());
    }

    let backup_id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();
    let mut assets = Vec::new();
    for entry in WalkDir::new(&source_root).follow_links(false).into_iter() {
        let entry = entry.map_err(|error| format!("Cannot inventory project backup: {error}"))?;
        if entry.file_type().is_symlink() {
            return Err(format!(
                "Project backup refuses symbolic links: {}",
                entry.path().display()
            ));
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(&source_root)
            .map_err(|_| "Project backup inventory escaped its source root")?;
        let relative_path = relative.to_string_lossy().replace('\\', "/");
        fs_safe::validate_relative(&relative_path)?;
        let size_bytes = entry
            .metadata()
            .map_err(|error| format!("Cannot inspect {relative_path}: {error}"))?
            .len();
        assets.push(BackupAsset {
            relative_path,
            size_bytes,
            sha256: sha256(entry.path())?,
        });
    }
    assets.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    if assets.is_empty() {
        return Err("The project folder contains no files to back up".into());
    }
    let source_bytes = assets.iter().try_fold(0_u64, |total, asset| {
        total
            .checked_add(asset.size_bytes)
            .ok_or_else(|| "Backup size exceeds the supported range".to_string())
    })?;
    let safe_project_id: String = project_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let generation = format!(
        "{}-{}",
        Utc::now().format("%Y%m%dT%H%M%SZ"),
        &backup_id[..8]
    );
    let generation_relative_path = PathBuf::from("Kairix PIMP Backups")
        .join(safe_project_id)
        .join(generation);
    let generation_path = destination_root.join(&generation_relative_path);
    let volume_id = destination_volume_identity(&destination_root)?;
    Ok(BackupJob {
        schema_version: BACKUP_SCHEMA_VERSION,
        id: backup_id.clone(),
        project_id: project_id.into(),
        project_name: project_name.into(),
        source_root: source_root.to_string_lossy().into_owned(),
        destination_root: destination_root.to_string_lossy().into_owned(),
        generation_path: generation_path.to_string_lossy().into_owned(),
        generation_relative_path: generation_relative_path
            .to_string_lossy()
            .replace('\\', "/"),
        destination_volume_id: volume_id,
        state: "prepared".into(),
        created_at: created_at.clone(),
        updated_at: created_at.clone(),
        files_completed: 0,
        bytes_completed: 0,
        current_file: None,
        error: None,
        manifest: BackupManifest {
            schema_version: BACKUP_SCHEMA_VERSION,
            backup_id,
            project_id: project_id.into(),
            created_at,
            source_file_count: assets.len(),
            source_bytes,
            collection_complete: false,
            assets,
        },
    })
}

pub fn execute_backup(
    job: &mut BackupJob,
    mut checkpoint: impl FnMut(&BackupJob) -> Result<(), String>,
) -> Result<(), String> {
    let source_root = PathBuf::from(&job.source_root)
        .canonicalize()
        .map_err(|error| format!("Project source is unavailable: {error}"))?;
    let destination_root = PathBuf::from(&job.destination_root)
        .canonicalize()
        .map_err(|error| format!("Backup destination is unavailable: {error}"))?;
    if destination_volume_identity(&destination_root)? != job.destination_volume_id {
        return fail_job(
            job,
            "needs-attention",
            "The selected path now resolves to a different destination volume",
            &mut checkpoint,
        );
    }
    let generation_relative = if job.generation_relative_path.is_empty() {
        PathBuf::from(&job.generation_path)
            .strip_prefix(&destination_root)
            .map_err(|_| "Legacy backup generation path is unavailable at this destination")?
            .to_path_buf()
    } else {
        fs_safe::validate_relative(&job.generation_relative_path)?
    };
    let generation_marker_relative = generation_relative
        .join(".pimp-generation-root")
        .to_string_lossy()
        .replace('\\', "/");
    let generation_marker = fs_safe::resolve_for_create(
        &destination_root,
        &generation_marker_relative,
    )?;
    let generation = generation_marker
        .parent()
        .ok_or("Backup generation has no parent directory")?
        .to_path_buf();
    job.generation_path = generation.to_string_lossy().into_owned();
    let staging_marker = fs_safe::resolve_for_create(
        &destination_root,
        &format!(".kairix-pimp-backup-staging/{}/.pimp-staging-root", job.id),
    )?;
    let staging = staging_marker
        .parent()
        .ok_or("Backup staging path has no parent directory")?
        .to_path_buf();

    job.state = "copying".into();
    job.error = None;
    touch(job);
    checkpoint(job)?;
    job.files_completed = 0;
    job.bytes_completed = 0;

    for asset in job.manifest.assets.clone() {
        job.current_file = Some(asset.relative_path.clone());
        touch(job);
        checkpoint(job)?;
        let source = fs_safe::resolve_existing(&source_root, &asset.relative_path)?;
        let before = fs_safe::snapshot(&source)?;
        if before.length != asset.size_bytes || sha256(&source)? != asset.sha256 {
            return fail_job(
                job,
                "needs-attention",
                &format!("Source changed after backup planning: {}", asset.relative_path),
                &mut checkpoint,
            );
        }
        let destination = fs_safe::resolve_for_create(&generation, &asset.relative_path)?;
        if destination.exists() {
            if destination.is_file()
                && destination.metadata().map(|value| value.len()).ok() == Some(asset.size_bytes)
                && sha256(&destination)? == asset.sha256
            {
                job.files_completed += 1;
                job.bytes_completed += asset.size_bytes;
                checkpoint(job)?;
                continue;
            }
            return fail_job(
                job,
                "needs-attention",
                &format!(
                    "Backup destination collision was left untouched: {}",
                    asset.relative_path
                ),
                &mut checkpoint,
            );
        }
        let staged = fs_safe::resolve_for_create(&staging, &asset.relative_path)?;
        if staged.exists() {
            if !staged.is_file() || sha256(&staged)? != asset.sha256 {
                return fail_job(
                    job,
                    "needs-attention",
                    &format!("Staged backup evidence conflicts: {}", asset.relative_path),
                    &mut checkpoint,
                );
            }
        } else {
            copy_and_flush(&source, &staged)?;
        }
        if sha256(&staged)? != asset.sha256 || fs_safe::snapshot(&source)? != before {
            return fail_job(
                job,
                "needs-attention",
                &format!("Copy verification failed: {}", asset.relative_path),
                &mut checkpoint,
            );
        }
        fs_safe::promote_no_overwrite(&staged, &destination).map_err(|error| {
            if error == "__destination_exists__" {
                format!(
                    "Backup destination collision was left untouched: {}",
                    asset.relative_path
                )
            } else {
                error
            }
        })?;
        job.files_completed += 1;
        job.bytes_completed += asset.size_bytes;
        checkpoint(job)?;
    }

    job.state = "verifying".into();
    job.current_file = None;
    touch(job);
    checkpoint(job)?;
    for asset in &job.manifest.assets {
        let source = fs_safe::resolve_existing(&source_root, &asset.relative_path)?;
        if source.metadata().map_err(|error| error.to_string())?.len() != asset.size_bytes
            || sha256(&source)? != asset.sha256
        {
            return fail_job(
                job,
                "needs-attention",
                &format!(
                    "Source changed before final verification: {}",
                    asset.relative_path
                ),
                &mut checkpoint,
            );
        }
    }
    verify_backup(job)?;
    let manifest_path = generation.join(BACKUP_MANIFEST_NAME);
    if manifest_path.exists() {
        return fail_job(
            job,
            "needs-attention",
            "Backup manifest destination already exists and was left untouched",
            &mut checkpoint,
        );
    }
    write_new_json(&manifest_path, &job.manifest)?;
    job.state = "verified".into();
    job.current_file = None;
    touch(job);
    checkpoint(job)
}

pub fn verify_backup(job: &BackupJob) -> Result<(), String> {
    let destination_root = PathBuf::from(&job.destination_root)
        .canonicalize()
        .map_err(|error| format!("Backup destination is unavailable: {error}"))?;
    if destination_volume_identity(&destination_root)? != job.destination_volume_id {
        return Err("Backup destination volume identity does not match this job".into());
    }
    let generation = PathBuf::from(&job.generation_path)
        .canonicalize()
        .map_err(|error| format!("Backup generation is unavailable: {error}"))?;
    if !fs_safe::within(&destination_root, &generation) {
        return Err("Backup generation escaped the destination root".into());
    }
    for asset in &job.manifest.assets {
        let destination = fs_safe::resolve_existing(&generation, &asset.relative_path)?;
        if destination.metadata().map_err(|error| error.to_string())?.len() != asset.size_bytes
            || sha256(&destination)? != asset.sha256
        {
            return Err(format!(
                "Backup verification failed for {}",
                asset.relative_path
            ));
        }
    }
    Ok(())
}

pub fn restore_backup_test(
    job: &BackupJob,
    restore_root: &Path,
) -> Result<RestoreTestResult, String> {
    if job.state != "verified" {
        return Err("Only a verified backup can be used for a restore test".into());
    }
    verify_backup(job)?;
    let generation = PathBuf::from(&job.generation_path)
        .canonicalize()
        .map_err(|error| format!("Backup generation is unavailable: {error}"))?;
    let restore_root = restore_root
        .canonicalize()
        .map_err(|error| format!("Cannot resolve restore-test destination: {error}"))?;
    if !restore_root.is_dir() {
        return Err("Restore-test destination is not a directory".into());
    }
    if fs::read_dir(&restore_root)
        .map_err(|error| format!("Cannot inspect restore-test destination: {error}"))?
        .next()
        .is_some()
    {
        return Err("Choose an empty folder for the restore test".into());
    }
    if fs_safe::within(&generation, &restore_root)
        || fs_safe::within(&restore_root, &generation)
    {
        return Err("Restore-test destination must be independent of the backup".into());
    }
    let staging_marker = fs_safe::resolve_for_create(
        &restore_root,
        ".kairix-pimp-restore-staging/.pimp-staging-root",
    )?;
    let staging = staging_marker
        .parent()
        .ok_or("Restore-test staging has no parent directory")?
        .to_path_buf();
    for asset in &job.manifest.assets {
        let source = fs_safe::resolve_existing(&generation, &asset.relative_path)?;
        let staged = fs_safe::resolve_for_create(&staging, &asset.relative_path)?;
        copy_and_flush(&source, &staged)?;
        if sha256(&staged)? != asset.sha256 {
            return Err(format!(
                "Restore-test staging verification failed for {}",
                asset.relative_path
            ));
        }
        let destination = fs_safe::resolve_for_create(&restore_root, &asset.relative_path)?;
        if destination.exists() {
            return Err(format!(
                "Restore-test destination collision was left untouched: {}",
                asset.relative_path
            ));
        }
        fs_safe::promote_no_overwrite(&staged, &destination).map_err(|error| {
            if error == "__destination_exists__" {
                format!(
                    "Restore-test destination collision was left untouched: {}",
                    asset.relative_path
                )
            } else {
                error
            }
        })?;
    }
    for asset in &job.manifest.assets {
        let restored = fs_safe::resolve_existing(&restore_root, &asset.relative_path)?;
        if restored.metadata().map_err(|error| error.to_string())?.len() != asset.size_bytes
            || sha256(&restored)? != asset.sha256
        {
            return Err(format!(
                "Independent restore verification failed for {}",
                asset.relative_path
            ));
        }
    }
    Ok(RestoreTestResult {
        restore_path: restore_root.to_string_lossy().into_owned(),
        files_verified: job.manifest.source_file_count,
        bytes_verified: job.manifest.source_bytes,
        verified_at: Utc::now().to_rfc3339(),
    })
}

fn copy_and_flush(source: &Path, destination: &Path) -> Result<(), String> {
    let mut input = File::open(source)
        .map_err(|error| format!("Cannot open backup source {}: {error}", source.display()))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| format!("Cannot create backup staging file: {error}"))?;
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = input.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .map_err(|error| error.to_string())?;
    }
    output.sync_all().map_err(|error| error.to_string())
}

fn write_new_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("Cannot create immutable backup manifest: {error}"))?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())
}

fn fail_job(
    job: &mut BackupJob,
    state: &str,
    message: &str,
    checkpoint: &mut impl FnMut(&BackupJob) -> Result<(), String>,
) -> Result<(), String> {
    job.state = state.into();
    job.error = Some(message.into());
    touch(job);
    checkpoint(job)?;
    Err(message.into())
}

fn touch(job: &mut BackupJob) {
    job.updated_at = Utc::now().to_rfc3339();
}

#[cfg(windows)]
fn destination_volume_identity(path: &Path) -> Result<String, String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetVolumePathNameW(file_name: *const u16, volume_path: *mut u16, length: u32) -> i32;
        fn GetVolumeInformationW(
            root: *const u16,
            volume_name: *mut u16,
            volume_name_size: u32,
            serial: *mut u32,
            max_component: *mut u32,
            flags: *mut u32,
            filesystem_name: *mut u16,
            filesystem_name_size: u32,
        ) -> i32;
    }
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut root = vec![0_u16; 1024];
    if unsafe { GetVolumePathNameW(wide.as_ptr(), root.as_mut_ptr(), root.len() as u32) } == 0 {
        return Err(format!("Cannot identify backup volume: {}", std::io::Error::last_os_error()));
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
        return Err(format!("Cannot read backup volume identity: {}", std::io::Error::last_os_error()));
    }
    let fs_end = filesystem
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(filesystem.len());
    Ok(format!(
        "windows:{serial:08X}:{}",
        String::from_utf16_lossy(&filesystem[..fs_end]).to_uppercase()
    ))
}

#[cfg(not(windows))]
fn destination_volume_identity(path: &Path) -> Result<String, String> {
    Ok(format!("path:{}", path.canonicalize().map_err(|error| error.to_string())?.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".test-tmp")
            .join(format!("pimp-backup-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn backup_preserves_hierarchy_unicode_and_verifies_every_file() {
        let root = temporary("verified");
        let source = root.join("source");
        let destination = root.join("external");
        fs::create_dir_all(source.join("Media/カード 1")).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(source.join("project.prproj"), b"project").unwrap();
        fs::write(source.join("Media/カード 1/clip.mxf"), b"synthetic mxf").unwrap();
        fs::write(source.join("Media/カード 1/clip.xml"), b"<xml />").unwrap();
        let source_before = sha256(&source.join("Media/カード 1/clip.mxf")).unwrap();

        let mut job = plan_backup("project-1", "Proof", &source, &destination).unwrap();
        let mut checkpoints = Vec::new();
        execute_backup(&mut job, |job| {
            checkpoints.push(job.state.clone());
            Ok(())
        })
        .unwrap();

        assert_eq!(job.state, "verified");
        assert!(!job.manifest.collection_complete);
        assert_eq!(job.files_completed, 3);
        assert!(Path::new(&job.generation_path).join(BACKUP_MANIFEST_NAME).is_file());
        assert_eq!(sha256(&source.join("Media/カード 1/clip.mxf")).unwrap(), source_before);
        assert!(checkpoints.contains(&"copying".into()));
        assert!(checkpoints.contains(&"verifying".into()));
        verify_backup(&job).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_destination_is_never_replaced() {
        let root = temporary("collision");
        let source = root.join("source");
        let destination = root.join("external");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(source.join("clip.mp4"), b"incoming").unwrap();
        let mut job = plan_backup("project-1", "Proof", &source, &destination).unwrap();
        let generation = PathBuf::from(&job.generation_path);
        fs::create_dir_all(&generation).unwrap();
        fs::write(generation.join("clip.mp4"), b"existing").unwrap();
        assert!(execute_backup(&mut job, |_| Ok(())).is_err());
        assert_eq!(job.state, "needs-attention");
        assert_eq!(fs::read(generation.join("clip.mp4")).unwrap(), b"existing");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tampering_fails_independent_verification() {
        let root = temporary("tamper");
        let source = root.join("source");
        let destination = root.join("external");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(source.join("clip.mp4"), b"original").unwrap();
        let mut job = plan_backup("project-1", "Proof", &source, &destination).unwrap();
        execute_backup(&mut job, |_| Ok(())).unwrap();
        fs::write(Path::new(&job.generation_path).join("clip.mp4"), b"tampered").unwrap();
        assert!(verify_backup(&job).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_change_before_final_verification_blocks_manifest() {
        let root = temporary("source-change");
        let source = root.join("source");
        let destination = root.join("external");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(source.join("clip.mp4"), b"planned").unwrap();
        let mut job = plan_backup("project-1", "Proof", &source, &destination).unwrap();
        let result = execute_backup(&mut job, |snapshot| {
            if snapshot.state == "verifying" {
                fs::write(Path::new(&snapshot.source_root).join("clip.mp4"), b"changed")
                    .unwrap();
            }
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(job.state, "needs-attention");
        assert!(!Path::new(&job.generation_path).join(BACKUP_MANIFEST_NAME).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn full_restore_test_copies_to_an_independent_empty_folder() {
        let root = temporary("restore");
        let source = root.join("source");
        let destination = root.join("external");
        let restore = root.join("restore");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::create_dir(&restore).unwrap();
        fs::write(source.join("nested/clip.mp4"), b"restore proof").unwrap();
        fs::write(source.join("project.prproj"), b"project proof").unwrap();
        let mut job = plan_backup("project-1", "Proof", &source, &destination).unwrap();
        execute_backup(&mut job, |_| Ok(())).unwrap();

        let result = restore_backup_test(&job, &restore).unwrap();
        assert_eq!(result.files_verified, 2);
        assert_eq!(
            sha256(&source.join("nested/clip.mp4")).unwrap(),
            sha256(&restore.join("nested/clip.mp4")).unwrap()
        );
        assert!(restore_backup_test(&job, &restore).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_and_destination_may_not_overlap() {
        let root = temporary("overlap");
        let source = root.join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("clip.mp4"), b"clip").unwrap();
        assert!(plan_backup("project-1", "Proof", &source, &source).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
