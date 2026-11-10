//! Native lifecycle proof: a real daemon owns a real task process, a UI-side
//! relay (the transport the Tauri host uses) closes and reopens, and the run
//! and the event stream survive.
//!
//! Run with `cargo test -p orchestrator-daemon --test lifecycle -- --nocapture`.
//! It has no libtest harness because it re-executes its own binary as the fake
//! agent (`AGENT_ORCH_CODEX_PROGRAM`), which makes it identical on Unix and
//! Windows. Every step prints a `STEP` line; the last line is `lifecycle: PASS`.
//!
//! What counts as proof of execution: the agent process itself wrote its own
//! pid to a file, the OS reports that pid alive after the client closed, and a
//! separate fresh connection reads the task as `Running`. Transport
//! availability or a surviving client alone proves nothing, and nothing here
//! uses fixtures.

use orchestrator_ipc::{Capability, Client, CloseReason, Relay, RelayEvent};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(30);
const AGENT_ROLE: &str = "AGENT_ORCH_LIFECYCLE_AGENT_PID_FILE";

fn main() {
    if let Some(pid_file) = std::env::var_os(AGENT_ROLE) {
        return agent(Path::new(&pid_file));
    }
    let root = tempfile::TempDir::new().unwrap();
    // macOS temp dirs are symlinks; Windows canonical paths gain a `\\?\` prefix
    // that does not belong in pipe names, so only Unix canonicalizes.
    #[cfg(unix)]
    let base = fs::canonicalize(root.path()).unwrap();
    #[cfg(not(unix))]
    let base = root.path().to_path_buf();
    scenario(&base);
    println!("lifecycle: PASS");
}

/// The fake agent: reports a started turn like codex, records its own pid, and
/// idles until killed (or 90 s, so a failed run cannot leave it behind).
fn agent(pid_file: &Path) {
    if std::env::args().any(|arg| arg == "--version") {
        println!("lifecycle-fake-agent 0.0.0");
        return;
    }
    println!(r#"{{"type":"thread.started","thread_id":"01a1b721-1f91-7393-ae1d-6aace8e4646a"}}"#);
    println!(r#"{{"type":"turn.started"}}"#);
    let staged = pid_file.with_extension("tmp");
    fs::write(&staged, std::process::id().to_string()).unwrap();
    fs::rename(&staged, pid_file).unwrap();
    thread::sleep(Duration::from_secs(90));
}

struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn step(text: &str) {
    println!("STEP {text}");
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

fn until<T>(what: &str, mut probe: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(found) = probe() {
            return found;
        }
        assert!(Instant::now() < deadline, "timed out after 30 s: {what}");
        thread::sleep(Duration::from_millis(50));
    }
}

/// Runs one request on a fresh connection: an idle connection without an event
/// subscription is dropped by the daemon after a short timeout.
fn call<T>(data: &Path, what: &str, mut request: impl FnMut(&mut Client) -> Option<T>) -> T {
    until(what, || {
        let mut client = Client::connect(data).ok()?;
        request(&mut client)
    })
}

fn task_state(data: &Path, task_id: &str) -> String {
    call(data, "tasks.get", |client| {
        let task = client.tasks_get(task_id).ok()?;
        Some(task["state"].as_str().unwrap_or_default().to_owned())
    })
}

fn open_relay(data: &Path) -> (Relay, Value, Receiver<RelayEvent>) {
    until("relay open", || {
        let (sender, receiver) = mpsc::channel();
        Relay::open(
            data,
            &[Capability::Events, Capability::Tasks],
            move |event| {
                let _ = sender.send(event);
            },
        )
        .ok()
        .map(|(relay, welcome)| (relay, welcome, receiver))
    })
}

fn message(receiver: &Receiver<RelayEvent>) -> Value {
    match receiver
        .recv_timeout(WAIT)
        .expect("relay message within 30 s")
    {
        RelayEvent::Message(value) => value,
        RelayEvent::Closed(reason) => panic!("relay closed unexpectedly: {}", reason.code()),
    }
}

fn subscribe(
    relay: &Relay,
    receiver: &Receiver<RelayEvent>,
    after: Option<i64>,
    epoch: Option<&str>,
) -> Value {
    let mut params = json!({});
    if let Some(after) = after {
        params["after_sequence"] = json!(after);
    }
    if let Some(epoch) = epoch {
        params["epoch"] = json!(epoch);
    }
    relay
        .send(json!({"jsonrpc": "2.0", "id": 1, "method": "events.subscribe", "params": params}))
        .unwrap();
    let response = message(receiver);
    assert_eq!(response["id"], 1, "{response}");
    assert!(
        response.get("error").is_none(),
        "subscribe failed: {response}"
    );
    response["result"].clone()
}

fn event_sequence(value: &Value) -> i64 {
    assert_eq!(value["method"], "events.event", "{value}");
    value["params"]["event"]["sequence"].as_i64().unwrap()
}

fn scenario(base: &Path) {
    let repo = base.join("repo");
    let data = base.join("data");
    let pid_file = base.join("agent.pid");
    fs::create_dir_all(&repo).unwrap();
    fs::create_dir_all(&data).unwrap();
    git(&repo, &["init", "--initial-branch=main"]);
    git(&repo, &["config", "user.name", "lifecycle test"]);
    git(
        &repo,
        &["config", "user.email", "lifecycle@example.invalid"],
    );
    git(&repo, &["config", "core.autocrlf", "false"]);
    fs::write(repo.join("sample.txt"), "base\n").unwrap();
    git(&repo, &["add", "sample.txt"]);
    git(&repo, &["commit", "-m", "initial"]);

    step("1 start a real daemon with a fake agent binary");
    let agent: PathBuf = std::env::current_exe().unwrap();
    let nonexistent: PathBuf = base.join("no-such-agent");
    let mut daemon = Daemon(
        Command::new(env!("CARGO_BIN_EXE_orchestrator-daemon"))
            .arg("--data-dir")
            .arg(&data)
            .env("AGENT_ORCH_CODEX_PROGRAM", &agent)
            .env("AGENT_ORCH_CLAUDE_PROGRAM", &nonexistent)
            .env("AGENT_ORCH_PI_PROGRAM", &nonexistent)
            .env(AGENT_ROLE, &pid_file)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let project = call(&data, "projects.register", |client| {
        client.projects_register(&repo.display().to_string()).ok()
    });
    let project_id = project["id"]
        .as_str()
        .expect("registered project id")
        .to_owned();

    step("2 submit a task; the daemon launches the agent process");
    let task = call(&data, "tasks.submit", |client| {
        client
            .tasks_submit(json!({
                "idempotency_key": "lifecycle-1",
                "contract": {
                    "project_id": project_id,
                    "parent_id": null,
                    "objective": "run until cancelled",
                    "kind": "Implement",
                    "agent": "codex",
                    "dependencies": [],
                    "verification": "None",
                    "delivery": "ReportOnly",
                    "autonomy": "WorkspaceWrites",
                    "max_retries": 0,
                    "max_repairs": 0
                }
            }))
            .ok()
    });
    let task_id = task["contract"]["id"].as_str().unwrap().to_owned();
    let agent_pid: u32 = until("agent wrote its pid", || {
        fs::read_to_string(&pid_file).ok()?.trim().parse().ok()
    });
    until("task Running", || {
        (task_state(&data, &task_id) == "Running").then_some(())
    });
    assert!(orchestrator_runtime::is_process_running(agent_pid));

    step("3 open the UI-side relay and subscribe to the event stream");
    let (relay, welcome, events) = open_relay(&data);
    assert_eq!(welcome["version"], 1);
    let subscribed = subscribe(&relay, &events, None, None);
    let epoch = subscribed["epoch"]
        .as_str()
        .expect("journal epoch")
        .to_owned();
    let first = event_sequence(&message(&events));
    assert_eq!(
        first, 1,
        "a fresh subscription replays the journal from the start"
    );
    let mut last = first;
    // Drain what the journal already holds; the stream is contiguous.
    while let Ok(RelayEvent::Message(next)) = events.recv_timeout(Duration::from_secs(2)) {
        let sequence = event_sequence(&next);
        assert_eq!(sequence, last + 1, "contiguous sequences");
        last = sequence;
    }
    assert!(last >= 1);

    step("4 close the UI client: this must not stop the daemon or the agent");
    relay.close();
    thread::sleep(Duration::from_secs(1));
    assert!(
        daemon.0.try_wait().unwrap().is_none(),
        "daemon exited when the client closed"
    );
    assert!(
        orchestrator_runtime::is_process_running(agent_pid),
        "agent died when the client closed"
    );
    assert_eq!(
        task_state(&data, &task_id),
        "Running",
        "read over a fresh connection"
    );

    step("5 reopen: authenticate again and resume the stream at the saved cursor and epoch");
    let (relay, welcome, events) = open_relay(&data);
    assert_eq!(welcome["version"], 1);
    assert_eq!(
        welcome["journal_epoch"],
        epoch.as_str(),
        "same journal after reopen"
    );
    subscribe(&relay, &events, Some(last), Some(&epoch));
    assert!(
        orchestrator_runtime::is_process_running(agent_pid),
        "agent is still the same live process after reconnect"
    );

    step("6 cancel through another client; the reopened stream sees the transition with no gap");
    call(&data, "tasks.cancel", |client| {
        client.tasks_cancel(&task_id, "lifecycle-cancel").ok()
    });
    let mut saw_cancel = false;
    let deadline = Instant::now() + WAIT;
    while !saw_cancel {
        assert!(
            Instant::now() < deadline,
            "no cancel event arrived over the resumed stream"
        );
        let next = message(&events);
        let sequence = event_sequence(&next);
        assert_eq!(
            sequence,
            last + 1,
            "resumed stream continues exactly after the cursor"
        );
        last = sequence;
        let event = &next["params"]["event"];
        saw_cancel =
            event["task_id"] == task_id.as_str() && event.to_string().contains("Cancelled");
    }
    until("task Cancelled", || {
        (task_state(&data, &task_id) == "Cancelled").then_some(())
    });
    until("agent process exited", || {
        (!orchestrator_runtime::is_process_running(agent_pid)).then_some(())
    });

    step(
        "7 a relay on a stopped daemon reports a remote close, then the daemon shuts down cleanly",
    );
    call(&data, "shutdown", |client| client.shutdown().ok());
    // Events still in flight (the cancel's follow-ups) may precede the close.
    loop {
        match events.recv_timeout(WAIT).expect("close notice") {
            RelayEvent::Closed(reason) => {
                assert_eq!(reason.code(), CloseReason::Remote.code());
                break;
            }
            RelayEvent::Message(next) => {
                assert_eq!(next["method"], "events.event", "{next}");
            }
        }
    }
    relay.close();
    let status = until("daemon exit", || daemon.0.try_wait().unwrap());
    assert!(status.success());
    assert!(
        !data.join("daemon.token").exists(),
        "token removed on orderly shutdown"
    );
}
