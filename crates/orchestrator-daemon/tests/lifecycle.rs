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
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(30);
/// A client call may span slow daemon work on a slow host; the production
/// default is 2 s. Retry loops below swallow errors, so a short bound would
/// turn one slow call into a 30 s hang.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

fn connect(data: &Path) -> Result<Client, orchestrator_ipc::IpcError> {
    Client::connect_with_timeout(data, &Capability::ALL, CALL_TIMEOUT)
}

// Not `AGENT_ORCH_`-prefixed: the supervisor never forwards that prefix.
const AGENT_ROLE: &str = "LIFECYCLE_AGENT_PID_FILE";

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
/// Its stdout and stderr belong to the daemon, so it leaves breadcrumb files
/// next to the pid file for the failure diagnostics.
fn agent(pid_file: &Path) {
    if std::env::args().any(|arg| arg == "--version") {
        println!("lifecycle-fake-agent 0.0.0");
        return;
    }
    let launch = format!(
        "pid={} cwd={:?} args={:?}",
        std::process::id(),
        std::env::current_dir(),
        std::env::args().collect::<Vec<_>>()
    );
    let _ = fs::write(pid_file.with_extension("launch"), launch);
    if let Err(error) = record_pid(pid_file) {
        let _ = fs::write(pid_file.with_extension("err"), error.to_string());
        return;
    }
    thread::sleep(Duration::from_secs(90));
}

fn record_pid(pid_file: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let mut stdout = std::io::stdout();
    writeln!(
        stdout,
        r#"{{"type":"thread.started","thread_id":"01a1b721-1f91-7393-ae1d-6aace8e4646a"}}"#
    )?;
    writeln!(stdout, r#"{{"type":"turn.started"}}"#)?;
    stdout.flush()?;
    let staged = pid_file.with_extension("tmp");
    fs::write(&staged, std::process::id().to_string())?;
    fs::rename(&staged, pid_file)
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

/// Where the failure report finds the evidence: the daemon's own output and
/// the task being driven. The daemon logs every launch failure to stderr.
struct Evidence {
    base: PathBuf,
    daemon_log: PathBuf,
    task_id: Option<String>,
}

static EVIDENCE: Mutex<Option<Evidence>> = Mutex::new(None);

fn evidence(update: impl FnOnce(&mut Option<Evidence>)) {
    update(&mut EVIDENCE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
}

/// Panics with `what` plus everything needed to explain it without a rerun:
/// daemon output, agent breadcrumbs, task/session state, worktrees and the
/// longest path (Windows rejects paths over 259 characters in many APIs).
fn fail(what: &str) -> ! {
    panic!("{what}\n{}", diagnostics())
}

fn diagnostics() -> String {
    use std::fmt::Write;
    let guard = EVIDENCE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(evidence) = guard.as_ref() else {
        return "no diagnostics: the scenario had not started".to_owned();
    };
    let data = evidence.base.join("data");
    let mut out = String::from("--- lifecycle diagnostics ---\n");
    let _ = writeln!(out, "base: {} ({} chars)", evidence.base.display(), evidence.base.as_os_str().len());
    let _ = writeln!(out, "daemon log {}:\n{}", evidence.daemon_log.display(), tail(&evidence.daemon_log));
    for extension in ["launch", "err"] {
        let file = evidence.base.join("agent").with_extension(extension);
        let _ = writeln!(out, "agent.{extension}: {}", tail(&file));
    }
    let (longest, count) = longest_path(&evidence.base);
    let _ = writeln!(out, "longest path under base ({} chars of {count} entries): {}", longest.as_os_str().len(), longest.display());
    let _ = writeln!(out, "worktrees: {:?}", list(&data.join("worktrees")));
    match connect(&data) {
        Err(error) => {
            let _ = writeln!(out, "daemon not reachable: {error}");
        }
        Ok(mut client) => {
            if let Some(task_id) = &evidence.task_id {
                let _ = writeln!(out, "tasks.get: {:?}", client.tasks_get(task_id));
                let _ = writeln!(out, "sessions.list: {:?}", client.sessions_list(Some(task_id)));
            } else {
                let _ = writeln!(out, "tasks.list: {:?}", client.tasks_list());
            }
        }
    }
    out
}

fn tail(file: &Path) -> String {
    match fs::read_to_string(file) {
        Ok(text) => {
            let start = text.char_indices().rev().nth(8000).map_or(0, |(index, _)| index);
            text[start..].to_owned()
        }
        Err(error) => format!("<unreadable: {error}>"),
    }
}

fn list(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn longest_path(root: &Path) -> (PathBuf, usize) {
    let mut longest = root.to_path_buf();
    let mut count = 0;
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            count += 1;
            if path.as_os_str().len() > longest.as_os_str().len() {
                longest = path.clone();
            }
            if path.is_dir() {
                pending.push(path);
            }
        }
    }
    (longest, count)
}

fn until<T>(what: &str, mut probe: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(found) = probe() {
            return found;
        }
        if Instant::now() >= deadline {
            fail(&format!("timed out after 30 s: {what}"));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Runs one request on a fresh connection: an idle connection without an event
/// subscription is dropped by the daemon after a short timeout.
fn call<T>(data: &Path, what: &str, mut request: impl FnMut(&mut Client) -> Option<T>) -> T {
    until(what, || {
        let mut client = connect(data).ok()?;
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
    match receiver.recv_timeout(WAIT) {
        Ok(RelayEvent::Message(value)) => value,
        Ok(RelayEvent::Closed(reason)) => {
            fail(&format!("relay closed unexpectedly: {}", reason.code()))
        }
        Err(error) => fail(&format!("no relay message within 30 s: {error}")),
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
    git(&repo, &["config", "core.longpaths", "true"]);
    fs::write(repo.join("sample.txt"), "base\n").unwrap();
    git(&repo, &["add", "sample.txt"]);
    git(&repo, &["commit", "-m", "initial"]);

    step("1 start a real daemon with a fake agent binary");
    let agent: PathBuf = std::env::current_exe().unwrap();
    let nonexistent: PathBuf = base.join("no-such-agent");
    // Diagnostics self-check: an unlaunchable agent must fail fast and explain itself.
    let agent = if std::env::var_os("AGENT_ORCH_LIFECYCLE_BREAK_AGENT").is_some() {
        nonexistent.clone()
    } else {
        agent
    };
    let daemon_log = base.join("daemon.log");
    let log = fs::File::create(&daemon_log).unwrap();
    evidence(|slot| {
        *slot = Some(Evidence {
            base: base.to_path_buf(),
            daemon_log: daemon_log.clone(),
            task_id: None,
        });
    });
    let mut daemon = Daemon(
        Command::new(env!("CARGO_BIN_EXE_orchestrator-daemon"))
            .arg("--data-dir")
            .arg(&data)
            .env("AGENT_ORCH_CODEX_PROGRAM", &agent)
            .env("AGENT_ORCH_CLAUDE_PROGRAM", &nonexistent)
            .env("AGENT_ORCH_PI_PROGRAM", &nonexistent)
            .env(AGENT_ROLE, &pid_file)
            .env("AGENT_ORCH_ENV_PASSTHROUGH", AGENT_ROLE)
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
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
    evidence(|slot| {
        if let Some(evidence) = slot {
            evidence.task_id = Some(task_id.clone());
        }
    });
    let agent_pid: u32 = until("agent wrote its pid", || {
        if let Some(pid) = fs::read_to_string(&pid_file)
            .ok()
            .and_then(|text| text.trim().parse().ok())
        {
            return Some(pid);
        }
        // Fail fast with the daemon's own explanation instead of idling 30 s.
        if let Some(status) = daemon.0.try_wait().unwrap() {
            fail(&format!("daemon exited before the agent started: {status}"));
        }
        let task = connect(&data)
            .ok()
            .and_then(|mut client| client.tasks_get(&task_id).ok())?;
        if matches!(task["state"].as_str(), Some("Failed" | "Cancelled")) {
            fail("task ended before the agent wrote its pid");
        }
        None
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
