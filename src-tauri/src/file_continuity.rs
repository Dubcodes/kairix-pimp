use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::Path,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ContinuityEvidence {
    pub(crate) identity: String,
    pub(crate) length: u64,
    pub(crate) modified_millis: Option<u128>,
}

pub(crate) fn metadata_modified_millis(metadata: &fs::Metadata) -> Option<u128> {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_millis())
}

#[cfg(target_os = "windows")]
fn identity_for_handle(file: &File) -> Result<String, String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe {
        GetFileInformationByHandle(
            file.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE,
            &mut information,
        )
    } == 0
    {
        return Err(format!(
            "Cannot read Windows file identity: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(format!(
        "win:{:08x}:{:08x}{:08x}",
        information.dwVolumeSerialNumber, information.nFileIndexHigh, information.nFileIndexLow
    ))
}

#[cfg(all(not(target_os = "windows"), unix))]
fn identity_for_handle(file: &File) -> Result<String, String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    Ok(format!("unix:{}:{}", metadata.dev(), metadata.ino()))
}

#[cfg(all(not(target_os = "windows"), not(unix)))]
fn identity_for_handle(file: &File) -> Result<String, String> {
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    Ok(format!(
        "metadata:{}:{}",
        metadata.len(),
        metadata_modified_millis(&metadata).unwrap_or_default()
    ))
}

pub(crate) fn evidence_for_handle(file: &File) -> Result<ContinuityEvidence, String> {
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("Continuity evidence requires a regular file".into());
    }
    Ok(ContinuityEvidence {
        identity: identity_for_handle(file)?,
        length: metadata.len(),
        modified_millis: metadata_modified_millis(&metadata),
    })
}

#[cfg(target_os = "windows")]
pub(crate) fn open_protected_read(path: &Path) -> Result<File, String> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
        .map_err(|error| format!("Cannot protect {} for reading: {error}", path.display()))
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn open_protected_read(path: &Path) -> Result<File, String> {
    File::open(path).map_err(|error| format!("Cannot open {}: {error}", path.display()))
}

#[cfg(target_os = "windows")]
pub(crate) fn create_staging_file(path: &Path) -> Result<File, String> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_DELETE, FILE_SHARE_READ};
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
        .open(path)
        .map_err(|error| format!("Cannot create staging file: {error}"))
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn create_staging_file(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("Cannot create staging file: {error}"))
}

pub(crate) fn sha256_for_handle(file: &mut File) -> Result<String, String> {
    file.seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(crate) fn hash_with_evidence(path: &Path) -> Result<(String, ContinuityEvidence), String> {
    let mut file = open_protected_read(path)?;
    let before = evidence_for_handle(&file)?;
    let hash = sha256_for_handle(&mut file)?;
    if evidence_for_handle(&file)? != before {
        return Err("File identity or snapshot changed while hashing".into());
    }
    Ok((hash, before))
}

pub(crate) fn verify_approved_source_hash(actual: &str, approved: &str) -> Result<(), String> {
    if actual == approved {
        Ok(())
    } else {
        Err(
            "Source bytes no longer match the approved inventory SHA-256; staged copy was rejected"
                .into(),
        )
    }
}

pub(crate) fn validated_ingest_verification_mode(value: &str) -> Result<&str, String> {
    match value {
        "fast" | "deep" => Ok(value),
        _ => Err("Ingest verification mode must be Fast or Deep".into()),
    }
}

pub(crate) fn verify_staged_handle(
    staged: &mut File,
    staged_evidence: &ContinuityEvidence,
    expected_size: u64,
    expected_sha256: &str,
    verification_mode: &str,
) -> Result<u64, String> {
    let verification_mode = validated_ingest_verification_mode(verification_mode)?;
    if staged_evidence.length != expected_size {
        return Err("Staged destination size does not match the completed source read".into());
    }
    if verification_mode == "fast" {
        return Ok(0);
    }
    if sha256_for_handle(staged)? != expected_sha256 {
        return Err("Independent SHA-256 verification failed".into());
    }
    if evidence_for_handle(staged)? != *staged_evidence {
        return Err("Staged file identity changed during independent verification".into());
    }
    Ok(staged_evidence.length)
}

#[cfg(test)]
pub(crate) fn verify_staged_copy(
    staged: &Path,
    expected_size: u64,
    expected_sha256: &str,
    verification_mode: &str,
) -> Result<u64, String> {
    let mut file = open_protected_read(staged)?;
    let evidence = evidence_for_handle(&file)?;
    verify_staged_handle(
        &mut file,
        &evidence,
        expected_size,
        expected_sha256,
        verification_mode,
    )
}

pub(crate) fn file_identity(path: &Path) -> Result<String, String> {
    let file = open_protected_read(path)?;
    Ok(evidence_for_handle(&file)?.identity)
}

pub(crate) fn open_validated_destination(
    path: &Path,
    expected_size: u64,
    expected_modified_millis: Option<u128>,
    expected_identity: &str,
) -> Result<File, String> {
    let file = open_protected_read(path)
        .map_err(|error| format!("Cannot protect the verified destination: {error}"))?;
    let evidence = evidence_for_handle(&file)?;
    if evidence.length != expected_size
        || evidence.modified_millis != expected_modified_millis
        || evidence.identity != expected_identity
    {
        return Err("Destination identity or snapshot changed before cleanup".into());
    }
    Ok(file)
}

fn confirm_path_absent(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Source removal outcome is indeterminate because the path cannot be inspected: {error}"
        )),
        Ok(_) => Err(
            "Source removal outcome is uncertain because the path remains present or was replaced"
                .into(),
        ),
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn remove_exact_source(
    path: &Path,
    expected_size: u64,
    expected_modified_millis: Option<u128>,
    expected_identity: &str,
) -> Result<(), String> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        FileDispositionInfo, SetFileInformationByHandle, DELETE, FILE_DISPOSITION_INFO,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
    };

    let file = OpenOptions::new()
        .read(true)
        .access_mode(DELETE | windows_sys::Win32::Foundation::GENERIC_READ)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|error| format!("Cannot exclusively open approved source: {error}"))?;
    let evidence = evidence_for_handle(&file)
        .map_err(|error| format!("Cannot inspect approved source handle: {error}"))?;
    if evidence.length != expected_size
        || evidence.modified_millis != expected_modified_millis
        || evidence.identity != expected_identity
    {
        return Err("Approved source handle no longer identifies the expected regular file".into());
    }
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    let removed = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE,
            FileDispositionInfo,
            &disposition as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    };
    if removed == 0 {
        return Err(format!(
            "Windows refused handle-bound source removal: {}",
            std::io::Error::last_os_error()
        ));
    }
    drop(file);
    confirm_path_absent(path)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn remove_exact_source(
    path: &Path,
    expected_size: u64,
    expected_modified_millis: Option<u128>,
    expected_identity: &str,
) -> Result<(), String> {
    let file = open_protected_read(path)?;
    let evidence = evidence_for_handle(&file)?;
    if evidence.length != expected_size
        || evidence.modified_millis != expected_modified_millis
        || evidence.identity != expected_identity
    {
        return Err("Approved source content changed before removal".into());
    }
    fs::remove_file(path).map_err(|error| error.to_string())?;
    confirm_path_absent(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn handle_hash_and_identity_describe_the_open_file() {
        let root = std::env::temp_dir().join(format!("pimp-continuity-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("fixture.bin");
        fs::write(&path, b"continuity").unwrap();
        let mut file = open_protected_read(&path).unwrap();
        let before = evidence_for_handle(&file).unwrap();
        assert_eq!(before.length, 10);
        assert_eq!(
            sha256_for_handle(&mut file).unwrap(),
            format!("{:x}", Sha256::digest(b"continuity"))
        );
        assert_eq!(evidence_for_handle(&file).unwrap(), before);
        drop(file);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn staging_handle_remains_bound_to_written_bytes() {
        let root = std::env::temp_dir().join(format!("pimp-staging-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("fixture.part");
        let mut file = create_staging_file(&path).unwrap();
        file.write_all(b"staged").unwrap();
        file.sync_all().unwrap();
        let evidence = evidence_for_handle(&file).unwrap();
        assert_eq!(evidence.length, 6);
        assert_eq!(
            sha256_for_handle(&mut file).unwrap(),
            format!("{:x}", Sha256::digest(b"staged"))
        );
        drop(file);
        fs::remove_dir_all(root).unwrap();
    }
}
