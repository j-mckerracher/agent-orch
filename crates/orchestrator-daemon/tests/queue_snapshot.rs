//! Native proof of `queue.snapshot`: a real daemon at the default 2/2 limits
//! runs two fake agent processes at once, queues a third task, and reports
//! both over the platform transport (Unix socket, Windows named pipe).
//!
//! Run with `cargo test -p orchestrator-daemon --test queue_snapshot -- --nocapture`.
//! It has no libtest harness because it re-executes its own binary as the fake
//! agent (`AGENT_ORCH_CODEX_PROGRAM`), which makes it identical on Unix and
//! Windows. Every step prints a `STEP` line; the last line is
//! `queue_snapshot: PASS`.
//!
//! Proof of simultaneous execution: each agent process wrote its own pid file
//! and the OS reports both pids alive while the snapshot lists two running
//! tasks, one queued task waiting on the global cap, and the same task states
//! `tasks.list` reports at the same event sequence.

use orchestrator_ipc::{Capability, Client, IpcError};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(30);
/// Retry loops swallow errors, so a short call bound would turn one slow call
/// into a 30 s hang; the production default is 2 s.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);
const AGENT_ROLE: &str = "AGENT_ORCH_QUEUE_AGENT_DIR";

fn connect(data: &Path) -> Result<Client, IpcError> {
    Client::connect_with_timeout(data, &Capability::ALL, CALL_TIMEOUT)
}

fn main() {
    if let Some(dir) = std::env::var_os(AGENT_ROLE) {
        return agent(Path::new(&dir));
    }
    let root = tempfile::TempDir::new().unwrap();
    // macOS temp dirs are symlinks; Windows canonical paths gain a `\\?\` prefix
    // that does not belong in pipe names, so only Unix canonicalizes.
    #[cfg(unix)]
    let base = fs::canonicalize(root.path()).unwrap();
    #[cfg(not(unix))]
    let base = root.path().to_path_buf();
    scenario(&base);
    println!("queue_snapshot: PASS");
}

/// The fake agent: reports a started turn like codex, records its own pid in
/// the shared directory, and idles until killed (or 90 s, so a failed run
/// cannot leave it behind).
fn agent(dir: &Path) {
    use std::io::Write;
    if std::env::args().any(|arg| arg == "--version") {
        println!("queue-fake-agent 0.0.0");
        return;
    }
    let mut stdout = std::io::stdout();
    let _ = writeln!(
        stdout,
        r#"{{"type":"thread.started","thread_id":"01a1b721-1f91-7393-ae1d-6aace8e4646a"}}"#
    );
    let _ = writeln!(stdout, r#"{{"type":"turn.started"}}"#);
    let _ = stdout.flush();
    let pid = std::process::id();
    let staged = dir.join(format!("{pid}.tmp"));
    if fs::write(&staged, pid.to_string()).is_ok() {
        let _ = fs::rename(&staged, dir.join(format!("{pid}.pid")));
    }
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

struct Evidence {
    data: PathBuf,
    daemon_log: PathBuf,
}

static EVIDENCE: Mutex<Option<Evidence>> = Mutex::new(None);

/// Panics with `what` plus the daemon's own output and the live queue state.
fn fail(what: &str) -> ! {
    panic!("{what}\n{}", diagnostics())
}

fn diagnostics() -> String {
    use std::fmt::Write;
    let guard = EVIDENCE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(evidence) = guard.as_ref() else {
        return "no diagnostics: the scenario had not started".to_owned();
    };
    let mut out = String::from("--- queue_snapshot diagnostics ---\n");
    let log = fs::read_to_string(&evidence.daemon_log)
        .unwrap_or_else(|error| format!("<unreadable: {error}>"));
    let _ = writeln!(out, "daemon log:\n{log}");
    match connect(&evidence.data) {
        Err(error) => {
            let _ = writeln!(out, "daemon not reachable: {error}");
        }
        Ok(mut client) => {
            let _ = writeln!(out, "queue.snapshot: {:?}", client.queue_snapshot());
            let _ = writeln!(out, "tasks.list: {:?}", client.tasks_list());
        }
    }
    out
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

fn agent_pids(dir: &Path) -> Vec<u32> {
    let mut pids: Vec<u32> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension()? == "pid").then_some(path)
        })
        .filter_map(|path| fs::read_to_string(path).ok()?.trim().parse().ok())
        .collect();
    pids.sort_unstable();
    pids
}

fn submit(data: &Path, project_id: &str, number: usize) -> String {
    let task = call(data, "tasks.submit", |client| {
        client
            .tasks_submit(json!({
                "idempotency_key": format!("queue-{number}"),
                "contract": {
                    "project_id": project_id,
                    "parent_id": null,
                    "objective": format!("run until cancelled {number}"),
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
    task["contract"]["id"].as_str().unwrap().to_owned()
}

fn ids(entries: &Value) -> Vec<String> {
    let mut ids: Vec<String> = entries
        .as_array()
        .unwrap_or_else(|| fail(&format!("not a list: {entries}")))
        .iter()
        .map(|entry| entry["task_id"].as_str().unwrap().to_owned())
        .collect();
    ids.sort_unstable();
    ids
}

/// A snapshot and `tasks.list` read around it, accepted only when the event
/// sequence did not move, so both describe the same point of the journal.
fn consistent_views(data: &Path) -> (Value, Value) {
    until("snapshot and tasks.list at one sequence", || {
        let first = connect(data).ok()?.queue_snapshot().ok()?;
        let tasks = connect(data).ok()?.tasks_list().ok()?;
        let second = connect(data).ok()?.queue_snapshot().ok()?;
        (first["as_of_sequence"] == second["as_of_sequence"]).then_some((second, tasks))
    })
}

fn tasks_in_state(tasks: &Value, state: &str) -> Vec<String> {
    let mut found: Vec<String> = tasks
        .as_array()
        .unwrap()
        .iter()
        .filter(|task| task["state"] == state)
        .map(|task| task["contract"]["id"].as_str().unwrap().to_owned())
        .collect();
    found.sort_unstable();
    found
}

fn scenario(base: &Path) {
    let repo = base.join("repo");
    let data = base.join("data");
    let agents = base.join("agents");
    for dir in [&repo, &data, &agents] {
        fs::create_dir_all(dir).unwrap();
    }
    git(&repo, &["init", "--initial-branch=main"]);
    git(&repo, &["config", "user.name", "queue test"]);
    git(&repo, &["config", "user.email", "queue@example.invalid"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["config", "core.longpaths", "true"]);
    fs::write(repo.join("sample.txt"), "base\n").unwrap();
    git(&repo, &["add", "sample.txt"]);
    git(&repo, &["commit", "-m", "initial"]);

    step("1 start a real daemon (default limits 2/2) with a fake agent binary");
    let agent: PathBuf = std::env::current_exe().unwrap();
    let nonexistent: PathBuf = base.join("no-such-agent");
    let daemon_log = base.join("daemon.log");
    let log = fs::File::create(&daemon_log).unwrap();
    *EVIDENCE.lock().unwrap() = Some(Evidence {
        data: data.clone(),
        daemon_log: daemon_log.clone(),
    });
    let mut daemon = Daemon(
        Command::new(env!("CARGO_BIN_EXE_orchestrator-daemon"))
            .arg("--data-dir")
            .arg(&data)
            .env("AGENT_ORCH_CODEX_PROGRAM", &agent)
            .env("AGENT_ORCH_CLAUDE_PROGRAM", &nonexistent)
            .env("AGENT_ORCH_PI_PROGRAM", &nonexistent)
            .env(AGENT_ROLE, &agents)
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap(),
    );
    let project = call(&data, "projects.register", |client| {
        client.projects_register(&repo.display().to_string()).ok()
    });
    let project_id = project["id"].as_str().unwrap().to_owned();

    step("2 an empty daemon reports its limits and an empty queue");
    let empty = call(&data, "queue.snapshot", |client| {
        client.queue_snapshot().ok()
    });
    assert_eq!(empty["limits"]["max_concurrent_workers"], 2, "{empty}");
    assert_eq!(empty["limits"]["max_workers_per_project"], 2, "{empty}");
    assert_eq!(empty["running"], json!([]), "{empty}");
    assert_eq!(empty["queued"], json!([]), "{empty}");

    step("3 a client that did not negotiate the queue capability is refused");
    let mut narrow = Client::connect_with_timeout(&data, &[Capability::Tasks], CALL_TIMEOUT)
        .unwrap_or_else(|error| fail(&format!("narrow connect failed: {error}")));
    assert!(matches!(
        narrow.queue_snapshot(),
        Err(IpcError::UnsupportedCapability)
    ));
    drop(narrow);

    step("4 submit three tasks: two run at once, the third waits");
    let tasks: Vec<String> = (1..=3).map(|n| submit(&data, &project_id, n)).collect();
    let pids = until("both agents wrote their pids", || {
        if let Some(status) = daemon.0.try_wait().unwrap() {
            fail(&format!("daemon exited: {status}"));
        }
        let pids = agent_pids(&agents);
        (pids.len() == 2).then_some(pids)
    });
    for pid in &pids {
        assert!(orchestrator_runtime::is_process_running(*pid), "pid {pid}");
    }
    let (snapshot, listed) = until("two running, one queued", || {
        let (snapshot, listed) = consistent_views(&data);
        (snapshot["running"].as_array()?.len() == 2).then_some((snapshot, listed))
    });
    assert_eq!(ids(&snapshot["running"]).len(), 2, "{snapshot}");
    let queued = snapshot["queued"].as_array().unwrap();
    assert_eq!(queued.len(), 1, "{snapshot}");
    assert_eq!(queued[0]["waiting_reason"], "global_cap", "{snapshot}");
    assert_eq!(queued[0]["position"], 1, "{snapshot}");
    assert_eq!(queued[0]["project_id"], project_id.as_str(), "{snapshot}");

    step("5 the snapshot agrees with tasks.list at the same event sequence");
    assert_eq!(
        ids(&snapshot["running"]),
        tasks_in_state(&listed, "Running")
    );
    assert_eq!(ids(&snapshot["queued"]), tasks_in_state(&listed, "Queued"));
    let mut everything = ids(&snapshot["running"]);
    everything.extend(ids(&snapshot["queued"]));
    everything.sort_unstable();
    let mut submitted = tasks.clone();
    submitted.sort_unstable();
    assert_eq!(everything, submitted);
    assert!(
        snapshot["as_of_sequence"].as_i64().unwrap() > 0,
        "{snapshot}"
    );

    step("6 cancelling a running task promotes the queued one");
    let waiting = ids(&snapshot["queued"]).remove(0);
    let victim = ids(&snapshot["running"]).remove(0);
    call(&data, "tasks.cancel", |client| {
        client.tasks_cancel(&victim, "queue-cancel-1").ok()
    });
    let (promoted, _) = until("queued task promoted", || {
        let (snapshot, listed) = consistent_views(&data);
        (snapshot["queued"] == json!([]) && ids(&snapshot["running"]).contains(&waiting))
            .then_some((snapshot, listed))
    });
    assert_eq!(
        promoted["running"].as_array().unwrap().len(),
        2,
        "{promoted}"
    );

    step("7 cancel the rest and shut the daemon down");
    for (number, task_id) in tasks.iter().enumerate() {
        if *task_id != victim {
            call(&data, "tasks.cancel", |client| {
                client
                    .tasks_cancel(task_id, &format!("queue-cancel-final-{number}"))
                    .ok()
            });
        }
    }
    until("all agents exited", || {
        pids.iter()
            .chain(agent_pids(&agents).iter())
            .all(|pid| !orchestrator_runtime::is_process_running(*pid))
            .then_some(())
    });
    call(&data, "shutdown", |client| client.shutdown().ok());
    let status = until("daemon exit", || daemon.0.try_wait().unwrap());
    assert!(status.success());
}
