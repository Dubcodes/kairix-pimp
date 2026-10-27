use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSnapshot {
    pub length: u64,
    pub modified_millis: Option<u128>,
}

pub fn snapshot(path: &Path) -> Result<FileSnapshot, String> {
    let metadata = path
        .metadata()
        .map_err(|error| format!("Cannot inspect {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    Ok(FileSnapshot {
        length: metadata.len(),
        modified_millis: metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map(|value| value.as_millis()),
    })
}

pub fn validate_relative(relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty() || relative.contains('\0') {
        return Err("Path must be a non-empty relative path".into());
    }
    let path = Path::new(relative);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("Path escapes the authorized root".into());
    }
    let clean: PathBuf = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value),
            _ => None,
        })
        .collect();
    if clean.as_os_str().is_empty() {
        return Err("Path must contain a filename".into());
    }
    for component in clean.components() {
        let Component::Normal(value) = component else {
            continue;
        };
        let value = value
            .to_str()
            .ok_or("Path components must be valid Unicode")?;
        if value.ends_with([' ', '.'])
            || value
                .chars()
                .any(|character| character.is_control() || r#"<>:"/\|?*"#.contains(character))
        {
            return Err(format!("Unsafe Windows path component: {value}"));
        }
        let stem = value
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if matches!(
            stem.as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        ) {
            return Err(format!("Reserved Windows path component: {value}"));
        }
    }
    Ok(clean)
}

pub(crate) fn within(root: &Path, candidate: &Path) -> bool {
    #[cfg(windows)]
    {
        let root = root.to_string_lossy().to_lowercase();
        let candidate = candidate.to_string_lossy().to_lowercase();
        candidate == root
            || candidate
                .strip_prefix(&root)
                .is_some_and(|rest| rest.starts_with('\\') || rest.starts_with('/'))
    }
    #[cfg(not(windows))]
    {
        candidate.starts_with(root)
    }
}

pub fn resolve_project_directory(
    root: &Path,
    requested: &Path,
    create_missing: bool,
) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("Cannot resolve Working Projects root: {error}"))?;
    if requested.as_os_str().is_empty()
        || requested
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("Project path contains traversal outside the authorized root".into());
    }
    let candidate = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        root.join(validate_relative(&requested.to_string_lossy())?)
    };

    let mut existing = candidate.as_path();
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(existing) {
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let name = existing
                    .file_name()
                    .ok_or("Project path has no existing authorized ancestor")?;
                missing.push(name.to_os_string());
                existing = existing
                    .parent()
                    .ok_or("Project path has no existing authorized ancestor")?;
            }
            Err(error) => {
                return Err(format!(
                    "Cannot inspect project path {}: {error}",
                    existing.display()
                ))
            }
        }
    }
    missing.reverse();
    for component in &missing {
        validate_relative(
            component
                .to_str()
                .ok_or("Project path components must be valid Unicode")?,
        )?;
    }

    let mut current = existing
        .canonicalize()
        .map_err(|error| format!("Cannot resolve project path ancestor: {error}"))?;
    if !within(&root, &current) {
        return Err("Project path escapes the configured Working Projects root".into());
    }
    if !current.is_dir() {
        return Err(format!(
            "Project path ancestor is not a directory: {}",
            current.display()
        ));
    }
    if !missing.is_empty() && !create_missing {
        return Err("The authorized project folder is not available locally".into());
    }
    for component in missing {
        let next = current.join(component);
        fs::create_dir(&next).map_err(|error| {
            format!("Cannot create project directory {}: {error}", next.display())
        })?;
        current = next
            .canonicalize()
            .map_err(|error| format!("Cannot resolve created project directory: {error}"))?;
        if !within(&root, &current) {
            return Err("Created project directory escaped the authorized root".into());
        }
    }
    if current == root {
        return Err("Choose a project folder inside the Working Projects root".into());
    }
    if !current.is_dir() {
        return Err("Authorized project path is not a directory".into());
    }
    Ok(current)
}

pub fn resolve_existing(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("Cannot resolve authorized root: {error}"))?;
    let candidate = root.join(validate_relative(relative)?);
    let candidate = candidate
        .canonicalize()
        .map_err(|error| format!("Cannot resolve requested path: {error}"))?;
    if !within(&root, &candidate) {
        return Err("Resolved path escapes the authorized root".into());
    }
    Ok(candidate)
}

pub fn resolve_for_create(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("Cannot resolve authorized root: {error}"))?;
    let relative = validate_relative(relative)?;
    let filename = relative.file_name().ok_or("Destination has no filename")?;
    let mut current = root.clone();
    if let Some(parent) = relative.parent() {
        for component in parent.components() {
            let Component::Normal(component) = component else {
                return Err("Destination parent contains an unsafe component".into());
            };
            let next = current.join(component);
            match fs::symlink_metadata(&next) {
                Ok(metadata) => {
                    if !metadata.is_dir() && !metadata.file_type().is_symlink() {
                        return Err(format!(
                            "Destination parent is not a directory: {}",
                            next.display()
                        ));
                    }
                    let canonical = next.canonicalize().map_err(|error| {
                        format!(
                            "Cannot resolve destination parent {}: {error}",
                            next.display()
                        )
                    })?;
                    if !within(&root, &canonical) {
                        return Err("Destination parent escapes the authorized root".into());
                    }
                    if !canonical.is_dir() {
                        return Err(format!(
                            "Destination parent is not a directory: {}",
                            next.display()
                        ));
                    }
                    current = canonical;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    if !within(&root, &current) {
                        return Err("Destination parent escapes the authorized root".into());
                    }
                    fs::create_dir(&next).map_err(|error| {
                        format!(
                            "Cannot create destination directory {}: {error}",
                            next.display()
                        )
                    })?;
                    let canonical = next.canonicalize().map_err(|error| {
                        format!("Cannot resolve created destination directory: {error}")
                    })?;
                    if !within(&root, &canonical) {
                        return Err(
                            "Created destination directory escaped the authorized root".into()
                        );
                    }
                    current = canonical;
                }
                Err(error) => {
                    return Err(format!(
                        "Cannot inspect destination directory {}: {error}",
                        next.display()
                    ))
                }
            }
        }
    }
    Ok(current.join(filename))
}

pub fn staging_root(master: &Path, device_id: &str) -> Result<PathBuf, String> {
    let master = master
        .canonicalize()
        .map_err(|error| format!("Cannot resolve Master Bin: {error}"))?;
    let parent = master
        .parent()
        .ok_or("Master Bin has no parent directory")?;
    let name = master
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("master-bin");
    let mut hash = Sha256::new();
    hash.update(device_id.as_bytes());
    let suffix = format!("{:x}", hash.finalize());
    let root = parent.join(format!(".{name}.pimp-staging-{}", &suffix[..12]));
    fs::create_dir_all(root.join("files")).map_err(|error| {
        format!("Cannot create safe same-filesystem staging outside Master Bin: {error}")
    })?;
    let canonical = root.canonicalize().map_err(|error| error.to_string())?;
    if within(&master, &canonical) {
        return Err("Safe staging resolved inside the synchronized Master Bin".into());
    }
    Ok(canonical)
}

/// Atomically creates the destination name without replacing anything already
/// present. A hard link is used because it has create-new semantics on the same
/// filesystem; the private staging name is removed only after the link exists.
pub fn promote_no_overwrite(staged: &Path, destination: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn MoveFileW(existing: *const u16, new: *const u16) -> i32;
        }
        let existing: Vec<u16> = staged.as_os_str().encode_wide().chain(Some(0)).collect();
        let new: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        // MoveFileW has no replace-existing flag: creation of the destination
        // name and removal of the staging name are one filesystem operation.
        if unsafe { MoveFileW(existing.as_ptr(), new.as_ptr()) } != 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if matches!(error.raw_os_error(), Some(80) | Some(183)) {
            return Err("__destination_exists__".into());
        }
        return Err(format!(
            "Filesystem cannot safely promote without overwrite: {error}"
        ));
    }
    #[cfg(not(windows))]
    match fs::hard_link(staged, destination) {
        Ok(()) => {
            fs::remove_file(staged).map_err(|error| {
                format!("Verified destination exists, but staging cleanup failed: {error}")
            })?;
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            Err("__destination_exists__".into())
        }
        Err(error) => Err(format!(
            "Filesystem cannot safely promote without overwrite: {error}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temporary(name: &str) -> PathBuf {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".test-tmp")
            .join(format!("pimp-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn rejects_traversal_absolute_and_windows_prefixes() {
        for unsafe_path in [
            "../escape",
            "..\\escape",
            "/escape",
            "C:\\escape",
            "\\\\server\\share",
        ] {
            assert!(validate_relative(unsafe_path).is_err(), "{unsafe_path}");
        }
        assert_eq!(
            validate_relative("日付/clip ü.mp4").unwrap(),
            PathBuf::from("日付/clip ü.mp4")
        );
        for unsafe_name in ["CON.mp4", "clip?.mp4", "trailing. ", "bad:name.mp4"] {
            assert!(validate_relative(unsafe_name).is_err(), "{unsafe_name}");
        }
        assert!(validate_relative("2026 10 23/clip ü 01.mp4").is_ok());
    }

    #[test]
    fn create_validation_rejects_directory_link_escape_before_side_effects() {
        let root = temporary("link-escape");
        let master = root.join("master");
        let outside = root.join("outside");
        fs::create_dir(&master).unwrap();
        fs::create_dir(&outside).unwrap();
        let link = master.join("escape");
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_dir(&outside, &link);
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&outside, &link);
        if let Err(error) = linked {
            if error.kind() == io::ErrorKind::PermissionDenied || error.raw_os_error() == Some(1314)
            {
                eprintln!("directory-link creation is unavailable in this test environment");
                fs::remove_dir_all(root).unwrap();
                return;
            }
            panic!("cannot create test directory link: {error}");
        }
        let result = resolve_for_create(&master, "escape/new/clip.mp4");
        assert!(result.is_err());
        assert!(!outside.join("new").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_directory_authorization_handles_existing_missing_and_nested_paths() {
        let root = temporary("project-directory");
        let projects = root.join("projects");
        fs::create_dir(&projects).unwrap();
        let existing = projects.join("Existing");
        fs::create_dir(&existing).unwrap();
        assert_eq!(
            resolve_project_directory(&projects, &existing, true).unwrap(),
            existing.canonicalize().unwrap()
        );
        let nested = projects.join("Series").join("Episode 1");
        assert_eq!(
            resolve_project_directory(&projects, &nested, true).unwrap(),
            nested.canonicalize().unwrap()
        );
        assert!(resolve_project_directory(&projects, Path::new("../escape"), true).is_err());
        assert!(!root.join("escape").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_directory_authorization_rejects_link_and_junction_escapes() {
        let root = temporary("project-directory-link");
        let projects = root.join("projects");
        let outside = root.join("outside");
        fs::create_dir(&projects).unwrap();
        fs::create_dir(&outside).unwrap();
        let link = projects.join("linked");
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_dir(&outside, &link);
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&outside, &link);
        if let Err(error) = linked {
            if error.kind() == io::ErrorKind::PermissionDenied || error.raw_os_error() == Some(1314)
            {
                eprintln!("directory-link creation is unavailable in this test environment");
                fs::remove_dir_all(root).unwrap();
                return;
            }
            panic!("cannot create project directory link: {error}");
        }
        assert!(resolve_project_directory(&projects, &link.join("Project"), true).is_err());
        assert!(!outside.join("Project").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn promotion_never_replaces_a_concurrent_destination() {
        let root = temporary("promote");
        let staged = root.join("staged");
        let destination = root.join("destination");
        fs::write(&staged, b"incoming").unwrap();
        fs::write(&destination, b"existing").unwrap();
        let result = promote_no_overwrite(&staged, &destination);
        if result
            .as_ref()
            .is_err_and(|error| error.contains("Access is denied"))
        {
            eprintln!("raw Windows rename is blocked by the test sandbox; Windows CI exercises it");
            fs::remove_dir_all(root).unwrap();
            return;
        }
        assert_eq!(result.unwrap_err(), "__destination_exists__");
        assert_eq!(fs::read(&destination).unwrap(), b"existing");
        assert_eq!(fs::read(&staged).unwrap(), b"incoming");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn promotion_exposes_only_complete_bytes() {
        let root = temporary("complete");
        let staged = root.join("staged");
        let destination = root.join("destination");
        let mut file = fs::File::create(&staged).unwrap();
        file.write_all(b"verified").unwrap();
        file.sync_all().unwrap();
        drop(file);
        let result = promote_no_overwrite(&staged, &destination);
        if result
            .as_ref()
            .is_err_and(|error| error.contains("Access is denied"))
        {
            eprintln!("raw Windows rename is blocked by the test sandbox; Windows CI exercises it");
            fs::remove_dir_all(root).unwrap();
            return;
        }
        result.unwrap();
        assert_eq!(fs::read(destination).unwrap(), b"verified");
        assert!(!staged.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn staging_is_a_sibling_not_a_master_bin_child() {
        let root = temporary("staging");
        let master = root.join("master");
        fs::create_dir(&master).unwrap();
        let staging = staging_root(&master, "device-a").unwrap();
        assert!(!staging.starts_with(master.canonicalize().unwrap()));
        fs::remove_dir_all(root).unwrap();
    }
}
