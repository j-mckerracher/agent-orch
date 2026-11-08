use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use orchestrator_store::{DataDirectoryLock, DataDirectoryLockError};

#[test]
fn live_holder_is_reported_and_drop_releases_lock() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let lock = DataDirectoryLock::acquire(directory.path()).expect("first lock");

    assert!(matches!(
        DataDirectoryLock::acquire(directory.path()),
        Err(DataDirectoryLockError::AlreadyHeld { holder_pid: pid })
            if pid == std::process::id()
    ));

    drop(lock);
    let _next_lock = DataDirectoryLock::acquire(directory.path()).expect("lock after drop");
}

#[test]
fn dead_holder_is_reported_as_stale_without_deleting_its_file() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let lock_path = directory.path().join(".orchestrator.lock");
    let dead_pid = i32::MAX as u32;
    fs::write(&lock_path, format!("pid={dead_pid}\nowner=dead\n")).expect("write stale lock");

    match DataDirectoryLock::acquire(directory.path()) {
        Err(DataDirectoryLockError::StaleLock { holder_pid }) => {
            assert_eq!(holder_pid, dead_pid)
        }
        Err(error) => panic!("expected stale-lock error, got {error:?}"),
        Ok(_) => panic!("stale lock was acquired"),
    }
    assert!(lock_path.exists(), "foreign stale lock must not be removed");
}

#[test]
fn unknown_holder_is_blocked_without_replacing_lock_file() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let lock_path = directory.path().join(".orchestrator.lock");
    let original = b"unreadable owner record\n";
    fs::write(&lock_path, original).expect("write unrecognized lock");

    assert!(matches!(
        DataDirectoryLock::acquire(directory.path()),
        Err(DataDirectoryLockError::HolderUnknown)
    ));
    assert_eq!(fs::read(&lock_path).expect("read retained lock"), original);
}

const LOCK_HELPER_DIRECTORY: &str = "ORCHESTRATOR_DATA_LOCK_TEST_DIRECTORY";

#[test]
#[ignore = "invoked as a child by separate_process_cannot_acquire_a_live_lock"]
fn lock_child_process_helper() {
    let Some(directory) = std::env::var_os(LOCK_HELPER_DIRECTORY) else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let _lock = DataDirectoryLock::acquire(&directory).expect("child lock");
    fs::write(directory.join("ready"), b"ready").expect("signal lock ready");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !directory.join("release").exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn separate_process_cannot_acquire_a_live_lock() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let ready = directory.path().join("ready");
    let release = directory.path().join("release");
    let mut child = Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "lock_child_process_helper",
            "--ignored",
            "--nocapture",
        ])
        .env(LOCK_HELPER_DIRECTORY, directory.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn lock holder");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready.exists() {
        if let Some(status) = child.try_wait().expect("check lock holder") {
            panic!("lock holder exited before acquiring lock: {status}");
        }
        assert!(
            Instant::now() < deadline,
            "lock holder did not become ready"
        );
        thread::sleep(Duration::from_millis(5));
    }

    let result = DataDirectoryLock::acquire(directory.path());
    fs::write(&release, b"release").expect("release child lock");
    let status = child.wait().expect("wait for lock holder");
    assert!(status.success(), "lock holder exited with {status}");
    match result {
        Err(DataDirectoryLockError::AlreadyHeld { holder_pid: pid }) => {
            assert_eq!(pid, child.id())
        }
        Err(error) => panic!("expected live-holder conflict, got {error:?}"),
        Ok(lock) => {
            drop(lock);
            panic!("second process acquired live lock");
        }
    }
    let _lock = DataDirectoryLock::acquire(directory.path()).expect("acquire after child exits");
}
