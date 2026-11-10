//! Owner-only permissions for the data directory and its files.
//!
//! The data directory holds the task database (objectives, agent event
//! streams), the IPC token, and the daemon log, so other local users must not
//! read it. On Unix the directory is created `0700` and files `0600`, and a
//! directory or file that already exists with group or other access is
//! tightened. A chmod failure is returned, not ignored: an unowned directory
//! cannot be made private, and the caller should refuse to use it.
//!
//! Windows has no mode bits and this module sets no DACL. A directory under
//! `%LOCALAPPDATA%` inherits the user profile's ACL (the user, SYSTEM and
//! Administrators); a `--data-dir` elsewhere inherits whatever its parent
//! grants. That is not verified or enforced here.

use std::io;
use std::path::Path;

/// Creates `path` and any missing parents, owner-only on Unix, then removes
/// group and other access from `path` itself if it already existed.
pub fn create_private_dir(path: &Path) -> io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path)?;
    restrict_to_owner(path)
}

/// Creates `path` empty and owner-only if it does not exist, and removes group
/// and other access if it does. Never truncates.
pub fn create_private_file(path: &Path) -> io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    match options.open(path) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => restrict_to_owner(path),
        Err(error) => Err(error),
    }
}

/// Removes group and other access from an existing file or directory. A
/// missing path is not an error (SQLite's `-wal` and `-shm` may not exist).
pub fn restrict_to_owner(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let mode = metadata.permissions().mode();
        if mode & 0o077 != 0 {
            let private = if metadata.is_dir() { 0o700 } else { 0o600 };
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & private))?;
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn a_new_directory_tree_and_file_are_owner_only() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("a/b");
        create_private_dir(&nested).unwrap();
        assert_eq!(mode(&nested), 0o700);
        assert_eq!(mode(&root.path().join("a")), 0o700);
        let file = nested.join("store.sqlite");
        create_private_file(&file).unwrap();
        assert_eq!(mode(&file), 0o600);
    }

    #[test]
    fn existing_wide_open_paths_are_tightened_without_losing_content() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("data");
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let file = dir.join("daemon.log");
        std::fs::write(&file, b"kept").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o664)).unwrap();
        create_private_dir(&dir).unwrap();
        create_private_file(&file).unwrap();
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&file), 0o600);
        assert_eq!(std::fs::read(&file).unwrap(), b"kept");
    }

    #[test]
    fn restricting_never_widens_and_ignores_a_missing_path() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("f");
        std::fs::write(&file, b"x").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o400)).unwrap();
        restrict_to_owner(&file).unwrap();
        assert_eq!(mode(&file), 0o400);
        restrict_to_owner(&root.path().join("absent")).unwrap();
    }
}
