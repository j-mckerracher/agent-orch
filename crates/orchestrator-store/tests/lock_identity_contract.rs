//! M2-04 contract tests: the data-directory lock fails closed.
//!
//! `tests/data_lock.rs` covers live-holder refusal, token takeover of a dead
//! holder, PID reuse, and concurrent takeovers. These tests cover records the
//! lock must never remove or reinterpret: tokenless stale records, malformed
//! records, and non-file lock paths. Every case asserts the lock path is left
//! byte-for-byte untouched.
//!
//! Windows file-identity API failures cannot be provoked from a portable
//! test; see the M2-04 contract report for the native-only gap.

use std::fs;
use std::path::PathBuf;

use orchestrator_store::{DataDirectoryLock, DataDirectoryLockError};

const LOCK_FILE: &str = ".orchestrator.lock";

fn dead_pid() -> u32 {
    // Larger than any PID the supported platforms assign.
    i32::MAX as u32
}

fn lock_path(directory: &tempfile::TempDir) -> PathBuf {
    directory.path().join(LOCK_FILE)
}

#[test]
fn stale_record_without_a_token_is_reported_and_preserved() {
    for record in [
        format!("pid={}\nstart=dead-start\n", dead_pid()),
        format!("pid={}\n", dead_pid()),
    ] {
        let directory = tempfile::tempdir().expect("temporary data directory");
        let path = lock_path(&directory);
        fs::write(&path, &record).expect("write tokenless stale lock");

        match DataDirectoryLock::acquire(directory.path()) {
            Err(DataDirectoryLockError::StaleLock { holder_pid }) => {
                assert_eq!(holder_pid, dead_pid())
            }
            Err(error) => panic!("expected StaleLock for {record:?}, got {error:?}"),
            Ok(lock) => {
                drop(lock);
                panic!("tokenless stale lock was taken over: {record:?}");
            }
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), record);
    }
}

#[test]
fn malformed_records_are_never_taken_over() {
    let dead = dead_pid();
    let records = [
        // Duplicate fields are ambiguous, not "last one wins".
        format!("pid={dead}\npid={dead}\nstart=s\ntoken=t\n"),
        format!("pid={dead}\nstart=a\nstart=b\ntoken=t\n"),
        format!("pid={dead}\nstart=s\ntoken=a\ntoken=b\n"),
        // Empty identity fields.
        format!("pid={dead}\nstart=\ntoken=t\n"),
        format!("pid={dead}\nstart=s\ntoken=\n"),
        // Mixing the legacy owner field with a token.
        format!("pid={dead}\nstart=s\ntoken=t\nowner=o\n"),
        // Missing, zero, or non-numeric PIDs.
        "start=s\ntoken=t\n".to_owned(),
        "pid=0\nstart=s\ntoken=t\n".to_owned(),
        "pid=not-a-number\nstart=s\ntoken=t\n".to_owned(),
        String::new(),
    ];
    for record in records {
        let directory = tempfile::tempdir().expect("temporary data directory");
        let path = lock_path(&directory);
        fs::write(&path, &record).expect("write malformed lock");

        match DataDirectoryLock::acquire(directory.path()) {
            Err(DataDirectoryLockError::HolderUnknown) => {}
            Err(error) => panic!("expected HolderUnknown for {record:?}, got {error:?}"),
            Ok(lock) => {
                drop(lock);
                panic!("malformed lock was taken over: {record:?}");
            }
        }
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            record,
            "malformed lock must be preserved"
        );
    }
}

#[test]
fn directory_at_the_lock_path_is_never_removed() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let path = lock_path(&directory);
    fs::create_dir(&path).expect("create directory at lock path");
    let marker = path.join("keep");
    fs::write(&marker, b"keep").expect("write marker");

    assert!(matches!(
        DataDirectoryLock::acquire(directory.path()),
        Err(DataDirectoryLockError::HolderUnknown)
    ));
    assert_eq!(fs::read(&marker).expect("marker survives"), b"keep");
}

#[cfg(unix)]
#[test]
fn symlink_at_the_lock_path_is_neither_followed_nor_removed() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let path = lock_path(&directory);
    let target = directory.path().join("victim");
    let record = format!("pid={}\nstart=dead-start\ntoken=victim-token\n", dead_pid());
    fs::write(&target, &record).expect("write symlink target");
    std::os::unix::fs::symlink(&target, &path).expect("create lock symlink");

    assert!(matches!(
        DataDirectoryLock::acquire(directory.path()),
        Err(DataDirectoryLockError::HolderUnknown)
    ));
    assert!(fs::symlink_metadata(&path)
        .expect("symlink survives")
        .file_type()
        .is_symlink());
    assert_eq!(fs::read_to_string(&target).unwrap(), record);
}
