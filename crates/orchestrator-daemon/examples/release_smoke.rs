//! Release smoke test against PACKAGED daemon binaries: the `orchestrator-daemon`
//! sidecar found inside an installed app, not the one `cargo test` builds.
//!
//! Usage:
//!   release_smoke --new-daemon <bin> [--old-daemon <bin>] [--work <new dir>]
//!                 [--expect-old-version <v>] [--expect-new-version <v>]
//!                 [--out <result.json>] [--keep-work]
//!
//! With `--old-daemon`, phase A runs the older binary and phase B runs the newer
//! binary over the SAME data directory: that is the upgrade. Without it both
//! phases run the one binary (a reinstall, not an upgrade) and the run says so.
//! `--work` must not exist; it holds the repositories, the data directory and
//! `daemon.log`. A failed run leaves it in place for inspection and stops its fake agents; a
//! passing run deletes it unless `--keep-work` is given.
//!
//! `--out` is written only after every step passed (it must not exist beforehand). It is the
//! structured result that a caller must validate instead of trusting the exit code: `result`,
//! `schema`, the numbered `steps`, both `daemon_start` versions, `upgrade_requested` (an
//! `--old-daemon` was given), `same_version`, and the daemon binaries it ran.
//!
//! What a pass proves, over the real authenticated local transport (Unix socket
//! or Windows named pipe), with a fake `codex` agent that this binary re-executes:
//! 1. install: the packaged daemon starts, authenticates a client and registers a project;
//! 2. cancellation: a running agent is cancelled, the task reaches `Cancelled` and the
//!    agent process exits (phase A, then again on the upgraded daemon);
//! 3. recovery: a daemon killed hard while an agent runs leaves an orphan on Unix (on Windows
//!    the kill-on-close job object ends the agent with the daemon); the next daemon (the
//!    upgraded one) terminates any orphan, marks the task `Failed` without relaunching, and
//!    `tasks.retry` starts a new attempt;
//! 4. upgrade: records written by the old daemon (project, coordinator message, cancelled
//!    task) are read back unchanged by the new one;
//! 5. approvals: a requested approval survives a hard daemon kill and restart; approve
//!    delivers a review ref at exactly the candidate commit; reject delivers nothing and
//!    blocks a later approve or delivery;
//! 6. orderly shutdown removes the token, and the daemon log holds no credential.
//!
//! What it does NOT prove: code signing, notarization or Authenticode (the packaging
//! scripts check those), Gatekeeper or SmartScreen, the installer or the GUI, a pending
//! approval written by the OLD daemon (public RPC cannot create a reviewable task, so the
//! seed is written with this checkout's store code after the upgrade), or Windows
//! behaviour on a machine this has not run on.
//!
//! Every phase prints a `SMOKE` line; the last line is `release_smoke: PASS`.

#[path = "../tests/support/seed.rs"]
mod seed;

use orchestrator_ipc::{Capability, Client, IpcError};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(30);
/// Review calls run several Git processes; the production default is sized for fast calls.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);
/// Not `AGENT_ORCH_`-prefixed: the supervisor never forwards that prefix.
const AGENT_ROLE: &str = "RELEASE_SMOKE_AGENT_PID_FILE";

const RESULT_SCHEMA: u32 = 1;
/// The fake agent exits when this file (next to its pid file) appears, so a failed run leaves
/// no process sleeping out the 90 s backstop.
const AGENT_STOP_EXTENSION: &str = "stop";

static LOG: OnceLock<PathBuf> = OnceLock::new();
static STEPS: Mutex<Vec<(u32, String)>> = Mutex::new(Vec::new());

fn main() {
    if let Some(pid_file) = std::env::var_os(AGENT_ROLE) {
        return agent(Path::new(&pid_file));
    }
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            eprintln!(
                "usage: release_smoke --new-daemon <bin> [--old-daemon <bin>] [--work <new dir>] \
                 [--expect-old-version <v>] [--expect-new-version <v>] [--out <result.json>] \
                 [--keep-work]"
            );
            std::process::exit(2);
        }
    };
    install_redacting_panic_hook();
    let result = scenario(&options);
    if let Some(out) = &options.out {
        let text = serde_json::to_string_pretty(&result).unwrap();
        fs::write(out, format!("{text}\n")).unwrap_or_else(|error| {
            panic!("cannot write the result file {}: {error}", out.display())
        });
    }
    println!("release_smoke: PASS");
}

struct Options {
    new_daemon: PathBuf,
    old_daemon: Option<PathBuf>,
    work: PathBuf,
    expect_old: Option<String>,
    expect_new: Option<String>,
    out: Option<PathBuf>,
    keep_work: bool,
}

impl Options {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let (mut new_daemon, mut old_daemon, mut work) = (None, None, None);
        let (mut expect_old, mut expect_new, mut out) = (None, None, None);
        let mut keep_work = false;
        let mut args = args;
        while let Some(flag) = args.next() {
            if flag == "--keep-work" {
                keep_work = true;
                continue;
            }
            let mut value = || args.next().ok_or(format!("{flag} needs a value"));
            match flag.as_str() {
                "--new-daemon" => new_daemon = Some(PathBuf::from(value()?)),
                "--old-daemon" => old_daemon = Some(PathBuf::from(value()?)),
                "--work" => work = Some(PathBuf::from(value()?)),
                "--expect-old-version" => expect_old = Some(value()?),
                "--expect-new-version" => expect_new = Some(value()?),
                "--out" => out = Some(PathBuf::from(value()?)),
                other => return Err(format!("unknown argument {other}")),
            }
        }
        let new_daemon = new_daemon.ok_or("--new-daemon is required")?;
        let work = work.unwrap_or_else(|| {
            let unique = format!("rs{}{}", std::process::id(), unix_seconds());
            std::env::temp_dir().join(unique)
        });
        if work.exists() {
            return Err(format!(
                "--work exists, refusing to reuse it: {}",
                work.display()
            ));
        }
        // Unix socket paths are limited to about 100 bytes: <work>/data/ipc/daemon.sock.
        if cfg!(unix) && work.join("data/ipc/daemon.sock").as_os_str().len() > 100 {
            return Err(format!(
                "--work is too long for a Unix socket path (use a shorter one): {}",
                work.display()
            ));
        }
        if let Some(out) = &out {
            if out.exists() {
                return Err(format!(
                    "--out exists, refusing to reuse it: {}",
                    out.display()
                ));
            }
        }
        for binary in std::iter::once(&new_daemon).chain(old_daemon.as_ref()) {
            if !binary.is_file() {
                return Err(format!("daemon binary not found: {}", binary.display()));
            }
        }
        Ok(Self {
            new_daemon,
            old_daemon,
            work,
            expect_old,
            expect_new,
            out,
            keep_work,
        })
    }
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// The fake agent: reports a started turn like codex, records its own pid and idles until
/// killed, told to stop by the stop file, or 90 s have passed.
fn agent(pid_file: &Path) {
    if std::env::args().any(|arg| arg == "--version") {
        println!("release-smoke-fake-agent 0.0.0");
        return;
    }
    let mut stdout = std::io::stdout();
    let _ = writeln!(
        stdout,
        r#"{{"type":"thread.started","thread_id":"01a1b721-1f91-7393-ae1d-6aace8e4646a"}}"#
    );
    let _ = writeln!(stdout, r#"{{"type":"turn.started"}}"#);
    let _ = stdout.flush();
    let staged = pid_file.with_extension("tmp");
    if fs::write(&staged, std::process::id().to_string()).is_ok() {
        let _ = fs::rename(&staged, pid_file);
    }
    let stop = pid_file.with_extension(AGENT_STOP_EXTENSION);
    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline && !stop.exists() {
        thread::sleep(Duration::from_millis(200));
    }
}

fn install_redacting_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let mut text = info.to_string();
        if let Some(log) = LOG.get() {
            text.push_str(&format!("\n--- daemon log tail ({}) ---\n", log.display()));
            text.push_str(&tail(log));
        }
        eprintln!("{}", orchestrator_diagnostics::redact(&text));
    }));
}

fn tail(file: &Path) -> String {
    match fs::read_to_string(file) {
        Ok(text) => {
            let start = text
                .char_indices()
                .rev()
                .nth(8000)
                .map_or(0, |(index, _)| index);
            text[start..].to_owned()
        }
        Err(error) => format!("<unreadable: {error}>"),
    }
}

fn step(number: u32, text: &str) {
    println!("SMOKE {number} {text}");
    STEPS.lock().unwrap().push((number, text.to_owned()));
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

fn connect(data: &Path) -> Result<Client, IpcError> {
    Client::connect_with_timeout(data, &Capability::ALL, CALL_TIMEOUT)
}

/// One request per fresh connection: an idle connection without an event subscription is
/// dropped by the daemon after a short timeout.
fn call<T>(data: &Path, what: &str, mut request: impl FnMut(&mut Client) -> Option<T>) -> T {
    until(what, || request(&mut connect(data).ok()?))
}

fn task(data: &Path, task_id: &str) -> Value {
    call(data, "tasks.get", |client| client.tasks_get(task_id).ok())
}

fn wait_state(data: &Path, task_id: &str, wanted: &str) -> Value {
    until(&format!("task {task_id} reaches {wanted}"), || {
        let task = task(data, task_id);
        (task["state"] == wanted).then_some(task)
    })
}

fn process_running(pid: u32) -> bool {
    orchestrator_runtime::is_process_running(pid)
}

fn wait_exit(pid: u32) {
    until(&format!("process {pid} exits"), || {
        (!process_running(pid)).then_some(())
    });
}

struct Daemon(Option<Child>);

impl Daemon {
    fn start(binary: &Path, env: &Env) -> Self {
        let log = fs::File::options()
            .create(true)
            .append(true)
            .open(&env.log)
            .unwrap();
        let child = Command::new(binary)
            .arg("--data-dir")
            .arg(&env.data)
            .env("AGENT_ORCH_CODEX_PROGRAM", &env.agent)
            .env("AGENT_ORCH_CLAUDE_PROGRAM", env.work.join("no-such-agent"))
            .env("AGENT_ORCH_PI_PROGRAM", env.work.join("no-such-agent"))
            .env(AGENT_ROLE, &env.pid_file)
            .env("AGENT_ORCH_ENV_PASSTHROUGH", AGENT_ROLE)
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap_or_else(|error| panic!("cannot start {}: {error}", binary.display()));
        let mut daemon = Self(Some(child));
        until("daemon accepts an authenticated client", || {
            assert!(
                daemon.child().try_wait().unwrap().is_none(),
                "the daemon exited before it was ready"
            );
            connect(&env.data).ok().map(drop)
        });
        daemon
    }

    fn child(&mut self) -> &mut Child {
        self.0.as_mut().expect("daemon already finished")
    }

    /// A crash: SIGKILL on Unix, TerminateProcess on Windows. Nothing runs in the daemon.
    fn crash(mut self) {
        let mut child = self.0.take().unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
    }

    /// An orderly stop through the `shutdown` request.
    fn shutdown(mut self, data: &Path) {
        call(data, "shutdown", |client| client.shutdown().ok());
        let mut child = self.0.take().unwrap();
        let status = until("daemon exits after shutdown", || child.try_wait().unwrap());
        assert!(status.success(), "daemon exit status {status}");
        assert!(
            !data.join("daemon.token").exists(),
            "token removed on orderly shutdown"
        );
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

struct Env {
    work: PathBuf,
    data: PathBuf,
    log: PathBuf,
    pid_file: PathBuf,
    agent: PathBuf,
}

impl Env {
    /// Waits for the fake agent's own pid file and returns the pid, then removes the file so
    /// the next launch is recognisable.
    fn agent_pid(&self) -> u32 {
        let pid = until("agent wrote its pid", || {
            fs::read_to_string(&self.pid_file).ok()?.trim().parse().ok()
        });
        fs::remove_file(&self.pid_file).unwrap();
        pid
    }
}

fn daemon_versions(log: &Path) -> Vec<String> {
    fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|event| event["event"] == "daemon_start")
        .filter_map(|event| event["version"].as_str().map(str::to_owned))
        .collect()
}

fn submit(data: &Path, project_id: &str, key: &str) -> String {
    let submitted = call(data, "tasks.submit", |client| {
        client
            .tasks_submit(json!({
                "idempotency_key": key,
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
                    "max_retries": 1,
                    "max_repairs": 0
                }
            }))
            .ok()
    });
    submitted["contract"]["id"].as_str().unwrap().to_owned()
}

fn target_ref(task_id: &str) -> String {
    format!("refs/heads/orchestrator/review/{task_id}")
}

fn ref_commit(repo: &Path, task_id: &str) -> Option<String> {
    let output = Command::new("git")
        .current_dir(repo)
        .args(["rev-parse", "--verify", "--quiet", &target_ref(task_id)])
        .output()
        .unwrap();
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn review_params(seeded: &Value, key: &str) -> Value {
    json!({
        "task_id": seeded["task_id"],
        "workspace_id": seeded["workspace_id"],
        "verification_run_id": seeded["verification_run_id"],
        "action": "local_review",
        "target_ref": target_ref(seeded["task_id"].as_str().unwrap()),
        "idempotency_key": key,
    })
}

fn delivery_params(seeded: &Value, approval_id: &str, key: &str) -> Value {
    let mut params = review_params(seeded, key);
    params["approval_id"] = json!(approval_id);
    params
}

fn git_init(repo: &Path) {
    seed::init_repo(repo);
    seed::git(repo, &["config", "core.longpaths", "true"]);
}

/// Tells every fake agent to exit when the scenario ends or panics. It is declared before any
/// daemon so it is dropped after the daemons have been killed.
struct StopAgents(PathBuf);

impl Drop for StopAgents {
    fn drop(&mut self) {
        let _ = fs::write(&self.0, b"stop");
    }
}

fn scenario(options: &Options) -> Value {
    fs::create_dir_all(&options.work).unwrap();
    // macOS temp dirs are symlinks; Windows canonical paths gain a `\\?\` prefix that does not
    // belong in pipe names, so only Unix canonicalizes.
    #[cfg(unix)]
    let work = fs::canonicalize(&options.work).unwrap();
    #[cfg(not(unix))]
    let work = options.work.clone();
    let env = Env {
        data: work.join("data"),
        log: work.join("daemon.log"),
        pid_file: work.join("agent.pid"),
        agent: std::env::current_exe().unwrap(),
        work: work.clone(),
    };
    let _stop_agents = StopAgents(env.pid_file.with_extension(AGENT_STOP_EXTENSION));
    LOG.set(env.log.clone()).ok();
    fs::create_dir_all(&env.data).unwrap();
    println!("work directory: {}", work.display());

    let old = options.old_daemon.as_deref().unwrap_or(&options.new_daemon);
    let upgrade = options.old_daemon.is_some();
    let repo = work.join("repo");
    git_init(&repo);

    step(
        1,
        "phase A: start the installed (old) daemon and register a project",
    );
    let daemon = Daemon::start(old, &env);
    let project = call(&env.data, "projects.register", |client| {
        client.projects_register(&repo.display().to_string()).ok()
    });
    let project_id = project["id"].as_str().expect("project id").to_owned();
    let message = call(&env.data, "coordinator.send_message", |client| {
        client
            .coordinator_send_message(&project_id, "Plan it", "smoke-msg-1")
            .ok()
    });
    assert_eq!(message["author"], "user");

    step(2, "phase A: cancel a running agent");
    let cancelled = submit(&env.data, &project_id, "smoke-cancel");
    let agent = env.agent_pid();
    wait_state(&env.data, &cancelled, "Running");
    assert!(process_running(agent), "the agent process itself is alive");
    call(&env.data, "tasks.cancel", |client| {
        client.tasks_cancel(&cancelled, "smoke-cancel-1").ok()
    });
    wait_state(&env.data, &cancelled, "Cancelled");
    wait_exit(agent);
    let cancelled_record = task(&env.data, &cancelled);

    step(
        3,
        "phase A: crash the daemon while an agent runs (Unix: an orphan is left behind)",
    );
    let interrupted = submit(&env.data, &project_id, "smoke-crash");
    let orphan = env.agent_pid();
    wait_state(&env.data, &interrupted, "Running");
    daemon.crash();
    if cfg!(unix) {
        assert!(
            process_running(orphan),
            "a crashed daemon leaves its agent running"
        );
    } else {
        // Windows agents live in a kill-on-close job object: the crash takes the agent with it.
        wait_exit(orphan);
    }

    step(
        4,
        if upgrade {
            "UPGRADE: start the new daemon over the old data directory"
        } else {
            "phase B: restart the same daemon (a reinstall; the upgrade is NOT exercised)"
        },
    );
    let daemon = Daemon::start(&options.new_daemon, &env);
    let versions = daemon_versions(&env.log);
    assert_eq!(versions.len(), 2, "two daemon_start events: {versions:?}");
    println!("UPGRADE: {} -> {}", versions[0], versions[1]);
    if let Some(expected) = &options.expect_old {
        assert_eq!(&versions[0], expected, "old daemon version");
    }
    if let Some(expected) = &options.expect_new {
        assert_eq!(&versions[1], expected, "new daemon version");
    }
    if upgrade && versions[0] == versions[1] {
        println!(
            "UPGRADE WARNING: old and new daemon report the same version; this is a reinstall"
        );
    }

    step(
        5,
        "records written by the old daemon are read back unchanged",
    );
    let projects = call(&env.data, "projects.list", |client| {
        client.projects_list().ok()
    });
    assert!(
        projects
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == project_id.as_str()),
        "project survives: {projects}"
    );
    let page = call(&env.data, "coordinator.get", |client| {
        client.coordinator_get(&project_id, None).ok()
    });
    assert_eq!(page["entries"][0]["body"], "Plan it");
    assert_eq!(
        task(&env.data, &cancelled),
        cancelled_record,
        "the cancelled task is unchanged"
    );

    step(
        6,
        "recovery: no orphan survives, and the interrupted task is Failed without a relaunch",
    );
    wait_exit(orphan);
    let failed = wait_state(&env.data, &interrupted, "Failed");
    assert_eq!(failed["attempts"][0]["state"], "Failed");
    assert!(
        !env.pid_file.exists(),
        "an interrupted task is not relaunched without a retry"
    );
    let retried = call(&env.data, "tasks.retry", |client| {
        client.tasks_retry(&interrupted, "smoke-retry-1").ok()
    });
    assert!(
        retried["state"] == "Queued" || retried["state"] == "Running",
        "{retried}"
    );
    let second = env.agent_pid();
    assert_ne!(second, orphan, "the retry is a new process");
    wait_state(&env.data, &interrupted, "Running");

    step(7, "cancellation on the new daemon");
    call(&env.data, "tasks.cancel", |client| {
        client.tasks_cancel(&interrupted, "smoke-cancel-2").ok()
    });
    wait_state(&env.data, &interrupted, "Cancelled");
    wait_exit(second);
    daemon.shutdown(&env.data);

    step(
        8,
        "approvals: seed two reviewable tasks, restart, request approvals",
    );
    let review_repo = work.join("review-repo");
    git_init(&review_repo);
    let worktrees = work.join("worktrees");
    let approve = seed::seed_review(&env.data, &review_repo, &worktrees, "smoke-approve");
    let reject = seed::seed_review(&env.data, &review_repo, &worktrees, "smoke-reject");
    let approve_id = approve["task_id"].as_str().unwrap();
    let reject_id = reject["task_id"].as_str().unwrap();
    let candidate = approve["candidate_commit"].as_str().unwrap();
    let daemon = Daemon::start(&options.new_daemon, &env);
    let requested = call(&env.data, "review.request_approval", |client| {
        client
            .review_request_approval(review_params(&approve, "smoke-request-a"))
            .ok()
    });
    assert_eq!(requested["approval"]["state"], "requested");
    let approval_id = requested["approval"]["id"].as_str().unwrap().to_owned();
    let rejected_request = call(&env.data, "review.request_approval", |client| {
        client
            .review_request_approval(review_params(&reject, "smoke-request-r"))
            .ok()
    });
    let reject_approval = rejected_request["approval"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    step(
        9,
        "a pending approval survives a hard daemon kill and restart",
    );
    daemon.crash();
    let daemon = Daemon::start(&options.new_daemon, &env);
    let pending = call(&env.data, "approvals.list", |client| {
        client.approvals_list().ok()
    });
    for id in [&approval_id, &reject_approval] {
        assert!(
            pending
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["id"] == id.as_str()),
            "approval {id} still pending after the crash: {pending}"
        );
    }
    let evidence = call(&env.data, "review.evidence", |client| {
        client
            .review_evidence(approve_id, Some(&approval_id), None)
            .ok()
    });
    assert_eq!(evidence["review_state"], "awaiting_decision");
    assert_eq!(evidence["decision_ready"], true);
    assert_eq!(
        ref_commit(&review_repo, approve_id),
        None,
        "nothing delivered before approval"
    );

    step(
        10,
        "approve delivers a review ref at exactly the candidate commit",
    );
    let approved = call(&env.data, "approvals.resolve", |client| {
        client
            .approvals_resolve(&approval_id, "approve", "smoke-resolve-a")
            .ok()
    });
    assert_eq!(approved["state"]["state"], "approved");
    let delivered = call(&env.data, "delivery.request", |client| {
        client
            .delivery_request(delivery_params(&approve, &approval_id, "smoke-deliver-a"))
            .ok()
    });
    assert_eq!(delivered["outcome"], "review_ref_created");
    assert_eq!(
        ref_commit(&review_repo, approve_id).as_deref(),
        Some(candidate)
    );

    step(
        11,
        "reject delivers nothing and blocks a later approve and delivery",
    );
    let mut client = connect(&env.data).unwrap();
    let rejected = client
        .approvals_resolve(&reject_approval, "reject", "smoke-resolve-r")
        .unwrap();
    assert_eq!(rejected["state"]["state"], "rejected");
    assert!(matches!(
        client.approvals_resolve(&reject_approval, "approve", "smoke-resolve-r-late"),
        Err(IpcError::Conflict)
    ));
    assert!(matches!(
        client.delivery_request(delivery_params(
            &reject,
            &reject_approval,
            "smoke-deliver-r"
        )),
        Err(IpcError::Forbidden)
    ));
    assert_eq!(ref_commit(&review_repo, reject_id), None);
    drop(client);

    step(
        12,
        "orderly shutdown removes the token; the log holds no credential",
    );
    let token = fs::read_to_string(env.data.join("daemon.token"))
        .unwrap()
        .trim()
        .to_owned();
    assert_eq!(token.len(), 64);
    daemon.shutdown(&env.data);
    let log = fs::read_to_string(&env.log).unwrap();
    assert!(
        !log.contains(&token),
        "the IPC token was written to daemon.log"
    );
    assert!(!process_running(orphan) && !process_running(second));

    let steps: Vec<Value> = STEPS
        .lock()
        .unwrap()
        .iter()
        .map(|(number, text)| json!({ "n": number, "text": text }))
        .collect();
    let result = json!({
        "schema": RESULT_SCHEMA,
        "result": "PASS",
        "steps": steps,
        "upgrade_requested": upgrade,
        "same_version": versions[0] == versions[1],
        "daemon_versions": versions,
        "old_daemon": old.display().to_string(),
        "new_daemon": options.new_daemon.display().to_string(),
    });
    if !options.keep_work {
        let _ = fs::remove_dir_all(&work);
    }
    result
}
