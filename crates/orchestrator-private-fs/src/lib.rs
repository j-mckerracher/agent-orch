//! Owner-only permissions for the data directory and its files.
//!
//! The data directory holds the task database (objectives, agent event
//! streams), the IPC token, and the daemon log, so other local users must not
//! read it.
//!
//! On Unix the directory is created `0700` and files `0600`, and a directory
//! or file that already exists with group or other access is tightened.
//!
//! Windows has no mode bits. There the same calls set a protected DACL that
//! grants full control to the current user's SID only: inheritance from the
//! parent is disabled (so a `CodexSandboxUsers` or profile ACE on the parent
//! does not apply) and a directory's ACE is inheritable, so files created in
//! it later, such as SQLite's `-wal` and `-shm`, start owner-only. SYSTEM and
//! Administrators are deliberately not granted access. A path that already
//! has exactly that DACL is left untouched, so a restart does not re-walk a
//! large worktree tree.
//!
//! A failure to apply the permissions is returned, not ignored: a path that
//! cannot be made private must not be used.

use std::io;
use std::path::Path;

#[cfg(windows)]
mod windows;

/// Creates `path` and any missing parents owner-only, then removes other
/// users' access from `path` itself if it already existed.
pub fn create_private_dir(path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        windows::create_private_dir(path)
    }
    #[cfg(not(windows))]
    {
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(path)?;
        restrict_to_owner(path)
    }
}

/// Creates `path` empty and owner-only if it does not exist, and removes other
/// users' access if it does. Never truncates.
pub fn create_private_file(path: &Path) -> io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    match options.open(path) {
        Ok(_) => restrict_new_file(path),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => restrict_to_owner(path),
        Err(error) => Err(error),
    }
}

/// Removes other users' access from an existing file or directory. A missing
/// path is not an error (SQLite's `-wal` and `-shm` may not exist).
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
        Ok(())
    }
    #[cfg(windows)]
    {
        windows::restrict_to_owner(path)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        Ok(())
    }
}

/// A file created with the owner-only mode already needs nothing on Unix; on
/// Windows it still has the parent's inherited ACL until the DACL is applied.
fn restrict_new_file(path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        windows::restrict_to_owner(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Ok(())
    }
}

/// Reads an SDDL DACL string such as `D:P(A;OICI;FA;;;S-1-5-21-1-2-3-1001)`.
/// Returns `Some(protected)` when it holds exactly one allow ACE and
/// `is_owner` accepts its trustee, and `None` for any other shape (extra
/// ACEs, deny ACEs, another trustee, or text this parser does not know).
#[cfg(any(windows, test))]
fn sole_owner_ace(sddl: &str, is_owner: impl Fn(&str) -> bool) -> Option<bool> {
    let dacl = sddl.strip_prefix("D:")?;
    let first_ace = dacl.find('(')?;
    let protected = dacl[..first_ace].contains('P');
    let mut aces = dacl[first_ace..].split(')');
    let ace = aces.next()?.strip_prefix('(')?;
    if aces.any(|rest| !rest.is_empty()) {
        return None;
    }
    let fields: Vec<&str> = ace.split(';').collect();
    (fields.len() == 6 && fields[0] == "A" && is_owner(fields[5])).then_some(protected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_path_is_not_an_error_to_restrict() {
        let root = tempfile::tempdir().unwrap();
        restrict_to_owner(&root.path().join("absent")).unwrap();
    }

    #[test]
    fn creating_a_file_never_truncates_an_existing_one() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("store.sqlite");
        std::fs::write(&file, b"kept").unwrap();
        create_private_file(&file).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"kept");
    }

    #[test]
    fn the_dacl_reader_accepts_only_one_owner_allow_ace() {
        let owner = |trustee: &str| trustee == "S-1-5-21-1-2-3-1001";
        assert_eq!(
            sole_owner_ace("D:P(A;OICI;FA;;;S-1-5-21-1-2-3-1001)", owner),
            Some(true)
        );
        assert_eq!(
            sole_owner_ace("D:AI(A;ID;FA;;;S-1-5-21-1-2-3-1001)", owner),
            Some(false)
        );
        assert_eq!(
            sole_owner_ace("D:PAI(A;;FA;;;S-1-5-21-1-2-3-1001)", owner),
            Some(true)
        );
        for rejected in [
            "D:P(A;OICI;FA;;;S-1-5-21-1-2-3-1001)(A;OICI;0x1301bf;;;S-1-5-21-9-9-9-1002)",
            "D:P(A;OICI;FA;;;S-1-5-21-1-2-3-1001)(A;OICI;FA;;;SY)",
            "D:P(A;OICI;FA;;;S-1-5-21-9-9-9-1002)",
            "D:P(D;OICI;FA;;;S-1-5-21-1-2-3-1001)",
            "D:P",
            "D:P()",
            "O:SYD:P(A;OICI;FA;;;S-1-5-21-1-2-3-1001)",
            "",
        ] {
            assert_eq!(sole_owner_ace(rejected, owner), None, "{rejected}");
        }
    }
}

#[cfg(all(test, unix))]
mod unix_tests {
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
    fn restricting_never_widens() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("f");
        std::fs::write(&file, b"x").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o400)).unwrap();
        restrict_to_owner(&file).unwrap();
        assert_eq!(mode(&file), 0o400);
    }
}
