#![cfg(unix)]
//! Process-level coordinator conversation. Unix-only: it waits on the Unix
//! socket path, so it is not Windows evidence.

use orchestrator_core::records::Project;
use orchestrator_ipc::{Client, IpcError};
use orchestrator_store::SqliteStore;
use serde_json::json;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Daemon {
    fn wait_for_exit(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.0.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "daemon did not stop");
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn start(dir: &Path) -> Daemon {
    let mut daemon = Daemon(
        Command::new(env!("CARGO_BIN_EXE_orchestrator-daemon"))
            .arg("--data-dir")
            .arg(dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while !dir.join("daemon.token").exists() || !dir.join("ipc/daemon.sock").exists() {
        assert!(daemon.0.try_wait().unwrap().is_none(), "daemon exited");
        assert!(Instant::now() < deadline, "daemon startup timed out");
        thread::sleep(Duration::from_millis(10));
    }
    daemon
}

#[test]
fn coordinator_conversation_survives_daemon_restart_and_grants_no_authority() {
    let dir = TempDir::new().unwrap();
    let store = SqliteStore::new(&dir.path().join("store.sqlite")).unwrap();
    store
        .register_project(&Project {
            id: "project-1".into(),
            root_path: "/private/project-root".into(),
            settings: json!({}),
        })
        .unwrap();
    drop(store);

    let mut first = start(dir.path());
    let mut client = Client::connect(dir.path()).unwrap();
    let message = client
        .coordinator_send_message("project-1", "Plan it", "msg-1")
        .unwrap();
    assert_eq!(message["author"], "user");
    let proposal = client
        .coordinator_propose("project-1", "not a plan", "prop-1")
        .unwrap();
    assert_eq!(proposal["validation"]["status"], "invalid");
    assert_eq!(proposal["validation"]["authority"], "none");
    assert_eq!(client.tasks_list().unwrap(), json!([]));
    assert!(matches!(
        client.coordinator_get("missing", None),
        Err(IpcError::NotFound)
    ));
    assert!(matches!(
        client.review_evidence("missing", None, None),
        Err(IpcError::NotFound)
    ));
    client.shutdown().unwrap();
    first.wait_for_exit();

    let _second = start(dir.path());
    let mut client = Client::connect(dir.path()).unwrap();
    let page = client.coordinator_get("project-1", None).unwrap();
    assert_eq!(page["entries"].as_array().unwrap().len(), 2);
    assert_eq!(page["entries"][0]["body"], "Plan it");
    assert_eq!(
        client
            .coordinator_send_message("project-1", "Plan it", "msg-1")
            .unwrap(),
        message,
        "idempotent replay survives restart"
    );
    assert!(!page.to_string().contains("/private/project-root"));
    client.shutdown().unwrap();
}
