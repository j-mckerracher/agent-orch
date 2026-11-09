#![cfg(unix)]

use orchestrator_core::records::Project;
use orchestrator_ipc::{Client, IpcError};
use orchestrator_store::SqliteStore;
use serde_json::json;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
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

/// Builds the CLI next to the daemon test binary so a real run owner can be
/// launched. Cargo only provides `CARGO_BIN_EXE_*` for this package, so the
/// sibling package is built with the same profile and target directory. The
/// build is incremental and a no-op when the binary is already current.
fn build_cli_binary() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let profile_dir = exe.parent().and_then(Path::parent).unwrap().to_path_buf();
    let target_dir = profile_dir.parent().unwrap();
    let mut build = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    build
        .args([
            "build",
            "--locked",
            "-p",
            "orchestrator-cli",
            "--manifest-path",
        ])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml"))
        .arg("--target-dir")
        .arg(target_dir);
    if profile_dir
        .file_name()
        .is_some_and(|name| name == "release")
    {
        build.arg("--release");
    }
    let status = build.status().expect("run cargo build for the CLI");
    assert!(status.success(), "cannot build orchestrator-cli");
    let cli = profile_dir.join("orchestrator-cli");
    assert!(cli.exists(), "missing {}", cli.display());
    cli
}

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn wait_for_agent_pid(run: &mut Child, metadata: &Path) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(pid) = fs::read(metadata)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| value["agent_pid"].as_u64())
        {
            return pid as u32;
        }
        if let Some(status) = run.try_wait().unwrap() {
            panic!("CLI run exited before agent launch with {status}");
        }
        assert!(Instant::now() < deadline, "agent PID was not recorded");
        thread::sleep(Duration::from_millis(20));
    }
}

/// A real run owner (not a fake) must be able to confirm a cancellation that
/// the daemon queued, even though the daemon holds the data lock for its whole
/// lifetime. This is the live path that previously left the task Running and
/// the next cancel returning Conflict.
#[test]
fn daemon_cancel_reaches_cancelled_through_real_run_owner() {
    let cli = build_cli_binary();
    let root = TempDir::new().unwrap();
    let repo = root.path().join("repo");
    let data = root.path().join("data");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--initial-branch=main"]);
    git(&repo, &["config", "user.name", "daemon test"]);
    git(
        &repo,
        &["config", "user.email", "daemon-test@example.invalid"],
    );
    fs::write(repo.join("sample.txt"), "base\n").unwrap();
    git(&repo, &["add", "sample.txt"]);
    git(&repo, &["commit", "-m", "initial"]);
    let agent = root.path().join("fake-codex");
    fs::write(
        &agent,
        "#!/bin/sh\nprintf '{\"type\":\"thread.started\",\"thread_id\":\"01a1b721-1f91-7393-ae1d-6aace8e4646a\"}\\n'\nprintf '{\"type\":\"turn.started\"}\\n'\nexec sleep 60\n",
    )
    .unwrap();
    fs::set_permissions(&agent, fs::Permissions::from_mode(0o755)).unwrap();

    // The run owner needs the data lock for setup, so it starts first. The
    // daemon starts after the agent is live and then holds the lock until exit.
    let mut run = Command::new(&cli)
        .arg("--data-dir")
        .arg(&data)
        .args(["run", "--repo"])
        .arg(&repo)
        .args(["--task-id", "live-cancel", "--objective", "wait for cancel"])
        .args(["--base-ref", "main", "--agent-program"])
        .arg(&agent)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let agent_pid = wait_for_agent_pid(&mut run, &data.join("cli/live-cancel/metadata.json"));

    let store = SqliteStore::new(&data.join("store.sqlite")).unwrap();
    let owners = store.open_cancellation_owners().unwrap();
    assert_eq!(owners.len(), 1, "real run owner must be registered");
    let owner = owners.into_iter().next().unwrap();

    let mut daemon = Daemon::start(&data);
    wait_for_ipc(&data, &mut daemon);
    let mut client = Client::connect(&data).unwrap();
    let cancelled = client.tasks_cancel("live-cancel", "live-cancel-1");
    let cancelled = cancelled.unwrap_or_else(|error| {
        let _ = run.kill();
        panic!("daemon cancel failed: {error:?}");
    });
    // The request is queued for the owner; the owner confirms it afterwards.
    assert!(cancelled["state"] == "Running" || cancelled["state"] == "Cancelled");

    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = run.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "run owner did not exit after cancel"
        );
        thread::sleep(Duration::from_millis(20));
    };
    let output = run.wait_with_output().unwrap();
    assert_eq!(
        status.code(),
        Some(130),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!orchestrator_runtime::is_process_running(agent_pid));

    let task = client.tasks_get("live-cancel").unwrap();
    assert_eq!(task["state"], "Cancelled");
    assert!(store.open_cancellation_owners().unwrap().is_empty());
    assert_eq!(
        store.cancellation_request(&owner).unwrap(),
        Some(orchestrator_store::CancellationStatus::Confirmed)
    );

    client.shutdown().unwrap();
    assert!(daemon.finish().status.success());
}
