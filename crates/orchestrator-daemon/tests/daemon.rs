#![cfg(unix)]

use orchestrator_core::records::Project;
use orchestrator_ipc::{Client, IpcError};
use orchestrator_store::SqliteStore;
use serde_json::json;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

struct Daemon(Option<Child>);

impl Daemon {
    fn start(data_dir: &Path) -> Self {
        Self(Some(
            Command::new(env!("CARGO_BIN_EXE_orchestrator-daemon"))
                .arg("--data-dir")
                .arg(data_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        ))
    }

    fn child(&mut self) -> &mut Child {
        self.0.as_mut().unwrap()
    }

    fn pid(&mut self) -> u32 {
        self.child().id()
    }

    fn finish(mut self) -> Output {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.child().try_wait().unwrap().is_none() {
            assert!(
                Instant::now() < deadline,
                "daemon did not stop after shutdown"
            );
            thread::sleep(Duration::from_millis(10));
        }
        self.0.take().unwrap().wait_with_output().unwrap()
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn wait_for_ipc(dir: &Path, daemon: &mut Daemon) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !dir.join("daemon.token").exists() || !dir.join("ipc/daemon.sock").exists() {
        assert!(
            daemon.child().try_wait().unwrap().is_none(),
            "daemon exited before IPC startup"
        );
        assert!(Instant::now() < deadline, "daemon IPC startup timed out");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn rejected_authentication_does_not_log_either_token() {
    let dir = TempDir::new().unwrap();
    let mut daemon = Daemon::start(dir.path());
    wait_for_ipc(dir.path(), &mut daemon);
    let token = std::fs::read_to_string(dir.path().join("daemon.token")).unwrap();
    let mut wrong = token.clone();
    wrong.replace_range(..1, if token.starts_with('0') { "1" } else { "0" });

    let mut stream = UnixStream::connect(dir.path().join("ipc/daemon.sock")).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let body = format!("{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"handshake\",\"params\":{{\"token\":\"{wrong}\",\"versions\":[1],\"capabilities\":[]}}}}");
    stream.write_all(&1u16.to_be_bytes()).unwrap();
    stream
        .write_all(&(body.len() as u32).to_be_bytes())
        .unwrap();
    stream.write_all(body.as_bytes()).unwrap();
    let mut header = [0; 6];
    stream.read_exact(&mut header).unwrap();
    let length = u32::from_be_bytes(header[2..6].try_into().unwrap()) as usize;
    let mut response = vec![0; length];
    stream.read_exact(&mut response).unwrap();
    drop(stream);

    let mut client = Client::connect(dir.path()).unwrap();
    client.shutdown().unwrap();
    let output = daemon.finish();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success());
    assert!(!stdout.contains(&token) && !stdout.contains(&wrong));
    assert!(!stderr.contains(&token) && !stderr.contains(&wrong));
}
#[test]
fn second_daemon_reports_holder_and_shutdown_removes_resources() {
    let dir = TempDir::new().unwrap();
    let mut daemon = Daemon::start(dir.path());
    wait_for_ipc(dir.path(), &mut daemon);
    let holder = daemon.pid();

    let second = Command::new(env!("CARGO_BIN_EXE_orchestrator-daemon"))
        .arg("--data-dir")
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(!second.status.success());
    assert!(String::from_utf8_lossy(&second.stderr).contains(&holder.to_string()));

    let mut client = Client::connect(dir.path()).unwrap();
    let health = client.health().unwrap();
    assert_eq!(health.package_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(health.data_directory.as_path(), dir.path());
    client.shutdown().unwrap();

    let output = daemon.finish();
    assert!(output.status.success());
    assert!(!dir.path().join("daemon.token").exists());
    assert!(!dir.path().join("ipc/daemon.sock").exists());
}

#[test]
fn daemon_clients_submit_cancel_and_resume_durable_events() {
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

    let mut daemon = Daemon::start(dir.path());
    wait_for_ipc(dir.path(), &mut daemon);
    let mut client = Client::connect(dir.path()).unwrap();
    assert_eq!(
        client.projects_list().unwrap(),
        json!([{"id": "project-1"}])
    );
    client.events_subscribe(None).unwrap();
    let params = json!({
        "idempotency_key": "client-submit-1",
        "contract": {
            "project_id": "project-1",
            "parent_id": null,
            "objective": "Persist this task across reconnects",
            "kind": "Implement",
            "agent": "codex",
            "dependencies": [],
            "verification": "None",
            "delivery": "ReportOnly",
            "autonomy": "WorkspaceWrites",
            "max_retries": 0,
            "max_repairs": 0
        }
    });
    let submitted = client.tasks_submit(params.clone()).unwrap();
    assert_eq!(client.tasks_submit(params).unwrap(), submitted);
    let task_id = submitted["contract"]["id"].as_str().unwrap().to_owned();
    assert_eq!(client.tasks_get(&task_id).unwrap(), submitted);
    assert_eq!(client.tasks_list().unwrap().as_array().unwrap().len(), 1);
    assert!(client
        .sessions_list(None)
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
    assert!(client
        .approvals_list()
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
    assert!(matches!(
        client.sessions_events(&task_id, "missing", None),
        Err(IpcError::NotFound)
    ));
    assert!(matches!(
        client.workspaces_diff(&task_id, None, None),
        Err(IpcError::NotFound)
    ));
    assert!(matches!(
        client.approvals_resolve("missing", "invalid", "client-approval-1"),
        Err(IpcError::InvalidParams)
    ));
    assert!(matches!(
        client.delivery_request(serde_json::Value::Null),
        Err(IpcError::InvalidParams)
    ));

    assert_eq!(client.next_event().unwrap().sequence, 1);
    assert_eq!(client.next_event().unwrap().sequence, 2);
    assert_eq!(
        client.tasks_cancel(&task_id, "client-cancel-1").unwrap()["state"],
        "Cancelled"
    );
    assert_eq!(client.next_event().unwrap().sequence, 3);
    client.shutdown().unwrap();
    assert!(daemon.finish().status.success());
}
