use crate::{Client, Server, ServerConfig};
use std::fs;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn wait_for_socket(dir: &Path) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !dir.join("ipc/daemon.sock").exists() {
        assert!(Instant::now() < deadline, "daemon socket not created");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn stale_socket_is_reclaimed_after_refused_probe() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("ipc")).unwrap();
    let stale = dir.path().join("ipc/daemon.sock");
    drop(UnixListener::bind(&stale).unwrap());
    let daemon_dir = dir.path().to_path_buf();
    let daemon =
        thread::spawn(move || Server::serve(ServerConfig::new(daemon_dir, "test")).unwrap());
    wait_for_socket(dir.path());
    let mut client = Client::connect(dir.path()).unwrap();
    assert_eq!(client.health().unwrap().package_version, "test");
    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn live_server_rejects_competing_start_and_idle_cleanup_removes_files() {
    let dir = TempDir::new().unwrap();
    let daemon_dir = dir.path().to_path_buf();
    let daemon =
        thread::spawn(move || Server::serve(ServerConfig::new(daemon_dir, "test")).unwrap());
    wait_for_socket(dir.path());
    assert!(matches!(
        Server::serve(ServerConfig::new(dir.path().to_path_buf(), "second")),
        Err(crate::IpcError::AlreadyRunning)
    ));
    let mut client = Client::connect(dir.path()).unwrap();
    assert_eq!(client.health().unwrap().package_version, "test");
    client.shutdown().unwrap();
    daemon.join().unwrap();

    let idle = TempDir::new().unwrap();
    let idle_dir = idle.path().to_path_buf();
    let daemon = thread::spawn(move || {
        Server::serve(
            ServerConfig::new(idle_dir, "idle").with_idle_timeout(Duration::from_millis(100)),
        )
        .unwrap()
    });
    wait_for_socket(idle.path());
    daemon.join().unwrap();
    assert!(!idle.path().join("daemon.token").exists());
    assert!(!idle.path().join("ipc/daemon.sock").exists());
}

#[test]
fn authenticated_request_resets_idle_deadline() {
    let dir = TempDir::new().unwrap();
    let daemon_dir = dir.path().to_path_buf();
    let daemon = thread::spawn(move || {
        Server::serve(
            ServerConfig::new(daemon_dir, "active").with_idle_timeout(Duration::from_millis(500)),
        )
        .unwrap()
    });
    wait_for_socket(dir.path());
    let mut client = Client::connect(dir.path()).unwrap();
    thread::sleep(Duration::from_millis(300));
    assert_eq!(client.health().unwrap().package_version, "active");
    thread::sleep(Duration::from_millis(300));
    assert_eq!(client.health().unwrap().package_version, "active");
    client.shutdown().unwrap();
    daemon.join().unwrap();
}
