use std::fs;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use orchestrator_store::{DataDirectoryLock, DataDirectoryLockError};

fn lock_record(pid: u32, start: &str, token: &str) -> String {
    format!("pid={pid}\nstart={start}\ntoken={token}\n")
}

fn lock_field<'a>(record: &'a str, key: &str) -> &'a str {
    record
        .lines()
        .find_map(|line| {
            line.strip_prefix(key)
                .and_then(|value| value.strip_prefix('='))
        })
        .expect("lock field")
}

#[test]
fn live_holder_is_refused_with_pid_and_drop_releases_lock() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let lock = DataDirectoryLock::acquire(directory.path()).expect("first lock");
    let holder_pid = std::process::id();
    let error = match DataDirectoryLock::acquire(directory.path()) {
        Err(error) => error,
        Ok(next) => {
            drop(next);
            panic!("live lock was acquired twice");
        }
    };
    assert!(matches!(
        &error,
        DataDirectoryLockError::AlreadyHeld { holder_pid: pid } if *pid == holder_pid
    ));
    assert!(error.to_string().contains(&holder_pid.to_string()));

    drop(lock);
    let _next_lock = DataDirectoryLock::acquire(directory.path()).expect("lock after drop");
}

#[test]
fn dead_pid_lock_is_taken_over_and_new_holder_is_recorded() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let lock_path = directory.path().join(".orchestrator.lock");
    let dead_pid = i32::MAX as u32;
    let old = format!("pid={dead_pid}\nowner=dead-token\n");
    fs::write(&lock_path, &old).expect("write legacy dead holder lock");

    let lock = DataDirectoryLock::acquire(directory.path()).expect("take over dead lock");
    let current = fs::read_to_string(&lock_path).expect("read new holder");
    assert_eq!(lock_field(&current, "pid"), std::process::id().to_string());
    assert!(!lock_field(&current, "start").is_empty());
    assert_ne!(lock_field(&current, "token"), "dead-token");
    drop(lock);
    assert!(!lock_path.exists());
}

#[test]
fn live_legacy_lock_is_refused_with_clear_error() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let lock_path = directory.path().join(".orchestrator.lock");
    let legacy = format!("pid={}\nowner=legacy-token\n", std::process::id());
    fs::write(&lock_path, &legacy).expect("write legacy live lock");

    let error = match DataDirectoryLock::acquire(directory.path()) {
        Err(error) => error,
        Ok(lock) => {
            drop(lock);
            panic!("legacy live lock was acquired twice");
        }
    };
    assert!(matches!(
        &error,
        DataDirectoryLockError::LegacyLockHeld { holder_pid }
            if *holder_pid == std::process::id()
    ));
    assert!(error.to_string().contains("no start identity"));
    assert_eq!(fs::read_to_string(&lock_path).unwrap(), legacy);
}

#[test]
fn live_pid_with_different_start_identity_is_taken_over() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let lock_path = directory.path().join(".orchestrator.lock");
    let reused_pid = std::process::id();
    fs::write(
        &lock_path,
        lock_record(reused_pid, "different-start", "reused-token"),
    )
    .expect("write PID-reuse lock");

    let lock = DataDirectoryLock::acquire(directory.path()).expect("take over reused PID");
    let current = fs::read_to_string(&lock_path).expect("read new holder");
    assert_eq!(lock_field(&current, "pid"), reused_pid.to_string());
    assert_ne!(lock_field(&current, "start"), "different-start");
    assert_ne!(lock_field(&current, "token"), "reused-token");
    drop(lock);
}

#[test]
fn foreign_non_lock_file_is_never_removed() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let lock_path = directory.path().join(".orchestrator.lock");
    let original = b"unreadable owner record\n";
    fs::write(&lock_path, original).expect("write foreign file");

    assert!(matches!(
        DataDirectoryLock::acquire(directory.path()),
        Err(DataDirectoryLockError::HolderUnknown)
    ));
    assert_eq!(fs::read(&lock_path).expect("read retained file"), original);
}

#[test]
fn concurrent_takeovers_have_one_holder_and_one_named_conflict() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let lock_path = directory.path().join(".orchestrator.lock");
    let dead_pid = i32::MAX as u32;
    fs::write(
        &lock_path,
        lock_record(dead_pid, "dead-start", "concurrent-token"),
    )
    .expect("write stale lock");
    let barrier = Arc::new(Barrier::new(3));
    let (send, receive) = mpsc::channel();
    let mut threads = Vec::new();
    for _ in 0..2 {
        let directory = directory.path().to_path_buf();
        let barrier = Arc::clone(&barrier);
        let send = send.clone();
        threads.push(thread::spawn(move || {
            barrier.wait();
            send.send(DataDirectoryLock::acquire(directory)).unwrap();
        }));
    }
    barrier.wait();

    let first = receive
        .recv_timeout(Duration::from_secs(5))
        .expect("first takeover outcome");
    let second = receive
        .recv_timeout(Duration::from_secs(5))
        .expect("second takeover outcome");
    for thread in threads {
        thread.join().expect("takeover thread");
    }

    let mut holders = Vec::new();
    let mut conflicts = Vec::new();
    for result in [first, second] {
        match result {
            Ok(lock) => holders.push(lock),
            Err(DataDirectoryLockError::AlreadyHeld { holder_pid }) => conflicts.push(holder_pid),
            Err(error) => panic!("unexpected takeover result: {error}"),
        }
    }
    assert_eq!(holders.len(), 1);
    assert_eq!(conflicts.len(), 1);
    let current = fs::read_to_string(&lock_path).expect("read winning holder");
    assert_eq!(lock_field(&current, "pid"), conflicts[0].to_string());
    drop(holders);
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
