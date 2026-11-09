//! M2-06 acceptance for the CLI engine's data-directory lock, launch identity
//! capture and reconciliation gate, driven through the real `orchestrator-cli`
//! binary. Nothing here is Unix-gated: the agent is the std-only fake in
//! `tests/support/fake_codex.rs` (compiled with `rustc`), state is seeded through
//! the portable `SqliteStore` API, and processes are only ever signalled after
//! re-observing the exact identity captured when the test first saw them.
//!
//! Unlike `cli.rs`, `recovery.rs` and `multi_task_review.rs` (file-wide
//! `cfg(unix)`), these tests run on native Windows too.
//!
//! Interrupted-task workspace and delivery-journal evidence feeds the scheduling
//! gate per the M2-06 coordinator ruling (plan section 9); see the section
//! comment above those tests for the exact policy they pin.

use orchestrator_core::approval::{ApprovalAction, ApprovalActionKind, ParameterDigest};
use orchestrator_core::authorization::ActionKind;
use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::identity::{
    ExactProcessIdentity, ProcessClassification, ProcessIdentity, ProcessObservation,
};
use orchestrator_core::records::{Project, Workspace};
use orchestrator_core::task::{Attempt, AttemptState, Session, SessionState, TaskEvent, TaskState};
use orchestrator_runtime::is_process_running;
use orchestrator_runtime::reconcile::{
    capture_exact_process_identity, capture_process_identity, observe_owner_process,
    observe_process,
};
use orchestrator_store::{
    DataDirectoryLock, DeliveryOutcome, DeliveryStepRecord, SqliteStore, Store,
};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const CLI: &str = env!("CARGO_BIN_EXE_orchestrator-cli");
const FAKE_AGENT_SOURCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/fake_codex.rs");
const STEP_TIMEOUT: Duration = Duration::from_secs(30);

struct Fixture {
    root: TempDir,
    repo: PathBuf,
    data: PathBuf,
    agent: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
struct PrimarySnapshot {
    head: String,
    status: String,
    tracked_dirty: Vec<u8>,
    untracked: Vec<u8>,
}

fn fixture() -> Fixture {
    let root = tempfile::tempdir().expect("create unique fixture directory");
    let repo = root.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--initial-branch=main"]);
    git(&repo, &["config", "user.name", "Gate test"]);
    git(&repo, &["config", "user.email", "gate@example.invalid"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    fs::write(repo.join("sample.txt"), "base\n").unwrap();
    fs::write(repo.join("other.txt"), "other base\n").unwrap();
    git(&repo, &["add", "sample.txt", "other.txt"]);
    git(&repo, &["commit", "-m", "initial"]);
    // A dirty primary checkout, so every refusal also proves it is untouched.
    fs::write(repo.join("other.txt"), "other base\nlocal edit\n").unwrap();
    fs::write(repo.join("primary-untracked.txt"), "operator scratch\n").unwrap();

    let agent = root.path().join(if cfg!(windows) {
        "fake-codex.exe"
    } else {
        "fake-codex"
    });
    let compile = Command::new("rustc")
        .args(["--edition=2021", "-O"])
        .arg(FAKE_AGENT_SOURCE)
        .arg("-o")
        .arg(&agent)
        .output()
        .expect("run rustc (it must be on PATH)");
    assert!(
        compile.status.success(),
        "compile fake agent: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    Fixture {
        data: root.path().join("data"),
        root,
        repo,
        agent,
    }
}

/// Pins global and system Git config away so a user's settings cannot change
/// the result. The CLI's own Git children inherit it.
fn pin_git(command: &mut Command) -> &mut Command {
    command
        .env("GIT_CONFIG_GLOBAL", "/nonexistent-git-global")
        .env("GIT_CONFIG_SYSTEM", "/nonexistent-git-system")
}

fn git(repo: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    let output = pin_git(&mut command)
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("run Git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string()
}

fn snapshot(repo: &Path) -> PrimarySnapshot {
    PrimarySnapshot {
        head: git(repo, &["rev-parse", "HEAD"]),
        status: git(repo, &["status", "--porcelain", "--untracked-files=all"]),
        tracked_dirty: fs::read(repo.join("other.txt")).unwrap(),
        untracked: fs::read(repo.join("primary-untracked.txt")).unwrap(),
    }
}

fn run_command(fixture: &Fixture, task_id: &str, scenario: &str) -> Command {
    let mut command = Command::new(CLI);
    pin_git(&mut command)
        .arg("--data-dir")
        .arg(&fixture.data)
        .arg("run")
        .arg("--repo")
        .arg(&fixture.repo)
        .arg("--task-id")
        .arg(task_id)
        .arg("--objective")
        .arg("gate objective")
        .arg("--base-ref")
        .arg("main")
        .arg("--agent-program")
        .arg(&fixture.agent)
        .env("FAKE_AGENT_SCENARIO", scenario);
    command
}

fn cli(fixture: &Fixture, args: &[&str]) -> Output {
    let mut command = Command::new(CLI);
    pin_git(&mut command)
        .arg("--data-dir")
        .arg(&fixture.data)
        .args(args)
        .output()
        .expect("run CLI command")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_success(output: &Output, what: &str) {
    assert!(
        output.status.success(),
        "{what} failed: {:?}\nstdout: {}\nstderr: {}",
        output.status,
        stdout(output),
        stderr(output)
    );
}

fn wait_until<T>(what: &str, mut probe: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + STEP_TIMEOUT;
    loop {
        if let Some(value) = probe() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(50));
    }
}

fn read_json_pid(path: &Path) -> Option<u32> {
    let metadata: serde_json::Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    Some(metadata.get("agent_pid")?.as_u64()? as u32)
}

fn read_pidfile(path: &Path) -> Option<(u32, u32)> {
    let text = fs::read_to_string(path).ok()?;
    let mut pids = text.lines().map(|line| line.trim().parse::<u32>());
    Some((pids.next()?.ok()?, pids.next()?.ok()?))
}

fn open_store(fixture: &Fixture) -> SqliteStore {
    SqliteStore::new(&fixture.data.join("store.sqlite")).expect("open SQLite store")
}

/// Failure-path cleanup for processes this test started. A process is signalled
/// only if the OS still reports the exact identity captured when it was first
/// seen; a dead, replaced or unreadable process, and any untracked process, never is.
#[derive(Default)]
struct OwnedProcesses(Vec<(u32, ExactProcessIdentity)>);

impl OwnedProcesses {
    fn track(&mut self, what: &str, pid: u32) {
        let identity = capture_exact_process_identity(pid)
            .unwrap_or_else(|error| panic!("cannot capture identity of {what} PID {pid}: {error}"));
        self.0.push((pid, identity));
    }

    fn forget(&mut self, pid: u32) {
        self.0.retain(|(tracked, _)| *tracked != pid);
    }

    /// Kills one tracked process, only while its exact identity is still alive.
    fn terminate(&mut self, pid: u32) {
        for (tracked, identity) in self.0.iter().filter(|(tracked, _)| *tracked == pid) {
            kill_if_same(*tracked, *identity);
        }
        self.forget(pid);
    }
}

fn kill_if_same(pid: u32, identity: ExactProcessIdentity) {
    if observe_owner_process(identity) != ProcessClassification::Alive {
        return;
    }
    #[cfg(windows)]
    let _ = Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .output();
    #[cfg(unix)]
    let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
}

impl Drop for OwnedProcesses {
    fn drop(&mut self) {
        for (pid, identity) in &self.0 {
            kill_if_same(*pid, *identity);
        }
    }
}

/// Starts a real `run` that stays alive (`tree` scenario) and returns it with the
/// wrapper, agent and descendant PIDs, all tracked by exact identity in `guard`.
struct LiveRun {
    run: Child,
    wrapper: u32,
    agent: u32,
    descendant: u32,
}

fn start_live_run(fixture: &Fixture, task_id: &str, guard: &mut OwnedProcesses) -> LiveRun {
    let pidfile = fixture.root.path().join(format!("{task_id}.pids"));
    let mut run = run_command(fixture, task_id, "tree")
        .env("FAKE_AGENT_PIDFILE", &pidfile)
        .stdout(File::create(fixture.root.path().join(format!("{task_id}.out"))).unwrap())
        .stderr(File::create(fixture.root.path().join(format!("{task_id}.err"))).unwrap())
        .spawn()
        .expect("start CLI run");
    guard.track("run owner", run.id());
    let metadata = fixture.data.join("cli").join(task_id).join("metadata.json");
    let wrapper = wait_until("wrapper PID in metadata", || {
        if let Some(status) = run.try_wait().expect("poll run") {
            panic!("live run {task_id} exited early with {status}");
        }
        read_json_pid(&metadata)
    });
    guard.track("wrapper", wrapper);
    let (agent, descendant) = wait_until("agent and descendant PIDs", || {
        if let Some(status) = run.try_wait().expect("poll run") {
            panic!("live run {task_id} exited early with {status}");
        }
        read_pidfile(&pidfile)
    });
    guard.track("agent", agent);
    guard.track("descendant", descendant);
    LiveRun {
        run,
        wrapper,
        agent,
        descendant,
    }
}

fn project_id(fixture: &Fixture) -> String {
    fs::canonicalize(&fixture.repo)
        .unwrap()
        .display()
        .to_string()
}

/// Seeds a Running task/attempt/session like a run owner that was interrupted
/// mid-flight. `identity` is the session's recorded launch identity, if any.
fn seed_running_session(fixture: &Fixture, task_id: &str, identity: Option<ProcessIdentity>) {
    let project = project_id(fixture);
    seed_running_session_in(fixture, task_id, identity, &project, &project);
}

fn seed_running_session_in(
    fixture: &Fixture,
    task_id: &str,
    identity: Option<ProcessIdentity>,
    project_id: &str,
    project_root: &str,
) {
    let store = open_store(fixture);
    let project_id = project_id.to_owned();
    if store.get_project(&project_id).is_err() {
        store
            .register_project(&Project {
                id: project_id.clone(),
                root_path: project_root.to_owned(),
                settings: serde_json::json!({}),
            })
            .unwrap();
    }
    store
        .create(&TaskContract {
            id: task_id.to_owned(),
            project_id,
            parent_id: None,
            objective: "previous process".to_owned(),
            kind: TaskKind::Implement,
            agent: "codex".to_owned(),
            dependencies: Vec::new(),
            verification: VerificationPolicy::None,
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 0,
            max_repairs: 0,
        })
        .unwrap();
    store.transition(task_id, TaskEvent::Admit).unwrap();
    let attempt = Attempt {
        id: format!("{task_id}-attempt-1"),
        task_id: task_id.to_owned(),
        attempt_number: 1,
        state: AttemptState::Pending,
    };
    store.add_attempt(task_id, &attempt).unwrap();
    store
        .transition(task_id, TaskEvent::ExecutionStarted)
        .unwrap();
    store
        .update_attempt_state(task_id, &attempt.id, AttemptState::Running)
        .unwrap();
    let session = Session {
        id: format!("{task_id}-session-1"),
        attempt_id: attempt.id,
        adapter: "codex".to_owned(),
        state: SessionState::Spawning,
    };
    store.add_session(task_id, &session).unwrap();
    if let Some(identity) = identity {
        store
            .record_session_identity(task_id, &session.id, &identity)
            .unwrap();
    }
    store
        .update_session_state(task_id, &session.id, SessionState::Running)
        .unwrap();
}

/// An identity that belonged to a real agent process that is now provably gone
/// (the process and its descendant were killed by exact identity and reaped).
fn dead_identity(fixture: &Fixture, name: &str) -> ProcessIdentity {
    let pidfile = fixture.root.path().join(format!("{name}.dead.pids"));
    let mut command = Command::new(&fixture.agent);
    command
        .env("FAKE_AGENT_SCENARIO", "tree")
        .env("FAKE_AGENT_PIDFILE", &pidfile)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Like a real launch, lead a new process group (Unix). Otherwise the recorded
    // group is this test process's own, which stays alive, and the gate rightly
    // reports the identity as Unknown instead of provably Exited.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn().expect("start throwaway agent");
    let mut guard = OwnedProcesses::default();
    guard.track("throwaway agent", child.id());
    let (_, descendant) = wait_until("throwaway agent descendant", || read_pidfile(&pidfile));
    guard.track("throwaway descendant", descendant);
    let identity = capture_process_identity(child.id()).expect("capture agent identity");
    child.kill().expect("kill throwaway agent");
    child.wait().expect("reap throwaway agent");
    guard.terminate(descendant);
    wait_until("throwaway descendant to exit", || {
        (!is_process_running(descendant)).then_some(())
    });
    assert_eq!(
        observe_process(identity),
        ProcessObservation::Exited,
        "the throwaway agent must be provably exited, as the gate observes it"
    );
    assert_ne!(
        observe_owner_process(ExactProcessIdentity {
            identity,
            windows_creation_time_100ns_since_unix_epoch: None,
        }),
        ProcessClassification::Alive,
        "the throwaway agent must be dead"
    );
    identity
}

/// Registers a real Git worktree for `task_id` where `run` would put it.
fn registered_worktree(fixture: &Fixture, task_id: &str) -> PathBuf {
    let path = fixture.data.join("worktrees").join(task_id);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let branch = format!("orchestrator/{task_id}");
    git(
        &fixture.repo,
        &[
            "worktree",
            "add",
            "-b",
            &branch,
            path.to_str().unwrap(),
            "main",
        ],
    );
    let path = fs::canonicalize(path).unwrap();
    record_workspace(fixture, task_id, &path, &branch);
    path
}

fn record_workspace(fixture: &Fixture, task_id: &str, path: &Path, branch: &str) {
    open_store(fixture)
        .add_workspace(&Workspace {
            id: format!("{task_id}-workspace-1"),
            task_id: task_id.to_owned(),
            path: path.display().to_string(),
            branch: branch.to_owned(),
            base_commit: git(&fixture.repo, &["rev-parse", "main"]),
        })
        .unwrap();
}

/// A recorded workspace whose path exists but is NOT the registered worktree.
fn foreign_workspace(fixture: &Fixture, task_id: &str) -> PathBuf {
    let path = fixture.data.join("worktrees").join(task_id);
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("precious.txt"), "operator data\n").unwrap();
    let path = fs::canonicalize(path).unwrap();
    record_workspace(fixture, task_id, &path, &format!("orchestrator/{task_id}"));
    path
}

fn record_debug(fixture: &Fixture, task_id: &str) -> String {
    format!("{:?}", open_store(fixture).get(task_id).unwrap())
}

/// Settles a seeded interrupted task the way a finished run does: session
/// Exited, attempt and task Failed.
fn settle_failed(fixture: &Fixture, task_id: &str) {
    let store = open_store(fixture);
    let attempt = format!("{task_id}-attempt-1");
    store
        .update_session_state(
            task_id,
            &format!("{task_id}-session-1"),
            SessionState::Exited,
        )
        .unwrap();
    store
        .update_attempt_state(task_id, &attempt, AttemptState::Failed)
        .unwrap();
    store
        .transition(task_id, TaskEvent::ExecutionFailed)
        .unwrap();
}

/// Settles a seeded task as Succeeded (verified candidate, exited session).
fn settle_succeeded(fixture: &Fixture, task_id: &str) {
    let store = open_store(fixture);
    let attempt = format!("{task_id}-attempt-1");
    store
        .update_session_state(
            task_id,
            &format!("{task_id}-session-1"),
            SessionState::Exited,
        )
        .unwrap();
    store
        .update_attempt_state(task_id, &attempt, AttemptState::CandidateReady)
        .unwrap();
    store
        .transition(task_id, TaskEvent::CandidateReady)
        .unwrap();
    store
        .update_attempt_state(task_id, &attempt, AttemptState::Succeeded)
        .unwrap();
    store
        .transition(task_id, TaskEvent::VerificationOutcome { passed: true })
        .unwrap();
}

fn review_ref(task_id: &str, suffix: &str) -> String {
    format!("refs/heads/orchestrator/review/{task_id}{suffix}")
}

/// A commit object that is not on any branch, to stand for a ref that points at
/// something other than the recorded candidate.
fn unrelated_commit(fixture: &Fixture) -> String {
    let tree = git(&fixture.repo, &["rev-parse", "main^{tree}"]);
    git(
        &fixture.repo,
        &["commit-tree", &tree, "-m", "unrelated review ref target"],
    )
}

/// Persists one local-review delivery journal step for `task_id`. The candidate
/// is `main`'s commit; `target_ref` is only recorded, never created here.
fn record_delivery(fixture: &Fixture, task_id: &str, step_id: &str, target_ref: &str) {
    let candidate = git(&fixture.repo, &["rev-parse", "main"]);
    let action = ApprovalAction::new(
        ApprovalActionKind::Delivery(ActionKind::LocalReviewMerge),
        ParameterDigest::from_sha256([7; 32]),
        candidate,
        task_id,
    )
    .expect("valid delivery action");
    let step_id = format!("{task_id}-{step_id}");
    open_store(fixture)
        .record_delivery_step(&DeliveryStepRecord {
            id: &step_id,
            approval_id: "approval-for-journal",
            verification_run_id: "verification-for-journal",
            action: &action,
            destination: &project_id(fixture),
            target_ref: Some(target_ref),
            outcome: DeliveryOutcome::ReviewRefCreated,
            diff_summary: None,
        })
        .expect("persist delivery journal step");
}

fn point_ref_at_candidate(fixture: &Fixture, target_ref: &str) {
    git(&fixture.repo, &["update-ref", target_ref, "main"]);
}

fn no_unfinished_session(fixture: &Fixture, task_id: &str) -> bool {
    open_store(fixture)
        .get(task_id)
        .unwrap()
        .sessions
        .iter()
        .all(|session| session.state.is_terminal())
}

/// A scheduling refusal: non-success, and no new task, branch, worktree, output
/// directory or agent launch for `new_id`, and an untouched primary checkout.
fn refused_run(fixture: &Fixture, new_id: &str) -> Output {
    let before = snapshot(&fixture.repo);
    let output = run_command(fixture, new_id, "edit")
        .output()
        .expect("run CLI");
    assert!(
        !output.status.success(),
        "run must refuse to schedule\nstdout: {}\nstderr: {}",
        stdout(&output),
        stderr(&output)
    );
    assert_no_new_task(fixture, new_id);
    assert_eq!(
        snapshot(&fixture.repo),
        before,
        "refusal changed the primary"
    );
    output
}

/// The refusal must name the task and the kind of evidence (case-insensitive), so
/// a refusal for an unrelated reason (e.g. an unproven process) cannot pass.
fn assert_refusal_cites(output: &Output, task_id: &str, evidence: &str) {
    let text = stderr(output);
    assert!(
        text.contains(task_id),
        "refusal must name {task_id}: {text}"
    );
    assert!(
        text.to_lowercase().contains(evidence),
        "refusal must cite {evidence} evidence: {text}"
    );
}

fn assert_no_new_task(fixture: &Fixture, new_id: &str) {
    assert!(
        open_store(fixture).get(new_id).is_err(),
        "refused run created task {new_id}"
    );
    assert!(!fixture.data.join("cli").join(new_id).exists());
    assert!(!fixture.data.join("worktrees").join(new_id).exists());
    assert!(
        !git(&fixture.repo, &["branch", "--list"]).contains(new_id),
        "refused run created a branch for {new_id}"
    );
    assert!(
        !git(&fixture.repo, &["worktree", "list", "--porcelain"]).contains(new_id),
        "refused run registered a worktree for {new_id}"
    );
}

// ---- launch identity, SQLite store, JSON not migrated -------------------------------

#[test]
fn run_persists_launch_identity_in_sqlite_and_leaves_legacy_json_unmigrated() {
    let fixture = fixture();
    let legacy = fixture.data.join("tasks/legacy-json-task.json");
    fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    let legacy_bytes = br#"{"legacy":"pre-SQLite record, never migrated"}"#;
    fs::write(&legacy, legacy_bytes).unwrap();

    let run = run_command(&fixture, "ident-run", "edit")
        .output()
        .expect("run CLI");
    assert_success(&run, "run");

    assert!(
        fixture.data.join("store.sqlite").exists(),
        "SQLite is the engine store"
    );
    let wrapper = read_json_pid(&fixture.data.join("cli/ident-run/metadata.json"))
        .expect("metadata records the launched wrapper PID");
    let store = open_store(&fixture);
    let identity = store
        .session_identity("ident-run", "ident-run-session-1")
        .unwrap()
        .expect("launch identity is persisted in SQLite");
    assert_eq!(
        identity.pid, wrapper,
        "recorded identity is the launched process"
    );
    assert!(
        identity.start_time_micros.is_some() || cfg!(windows),
        "identity must carry a start time to defeat PID reuse: {identity:?}"
    );
    assert_eq!(store.get("ident-run").unwrap().state, TaskState::Verifying);

    assert_eq!(
        fs::read(&legacy).unwrap(),
        legacy_bytes,
        "JSON not rewritten"
    );
    let json_files = fs::read_dir(fixture.data.join("tasks"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(
        json_files,
        ["legacy-json-task.json"],
        "run must not write JSON task records"
    );
    let tasks = cli(&fixture, &["tasks"]);
    assert_success(&tasks, "tasks");
    assert!(stdout(&tasks).contains("ident-run\tVerifying"));
    assert!(
        !stdout(&tasks).contains("legacy-json-task"),
        "legacy JSON records are not migrated: {}",
        stdout(&tasks)
    );
}

// ---- lock ---------------------------------------------------------------------------

#[test]
fn live_parallel_run_neither_blocks_scheduling_nor_holds_the_lock() {
    let fixture = fixture();
    let mut guard = OwnedProcesses::default();
    let mut live = start_live_run(&fixture, "par-live", &mut guard);

    let store = open_store(&fixture);
    let recorded = store
        .session_identity("par-live", "par-live-session-1")
        .unwrap()
        .expect("identity is persisted before the agent is running");
    assert_eq!(
        capture_process_identity(live.wrapper).unwrap(),
        recorded,
        "persisted identity must be the live launched process"
    );
    assert_eq!(store.get("par-live").unwrap().state, TaskState::Running);

    // A live run must release the data-directory lock after launch...
    drop(DataDirectoryLock::acquire(&fixture.data).expect("lock is free while a run is live"));
    // ...and its exact-identity-verified live session is not a global blocker.
    let before = snapshot(&fixture.repo);
    let second = run_command(&fixture, "par-second", "edit")
        .output()
        .expect("run CLI");
    assert_success(&second, "second run beside a live task");
    assert!(stdout(&second).contains("status: Verifying"));
    assert_eq!(snapshot(&fixture.repo), before);

    assert!(
        live.run.try_wait().unwrap().is_none(),
        "live run kept going"
    );
    assert_eq!(
        open_store(&fixture).get("par-live").unwrap().state,
        TaskState::Running
    );
    for pid in [live.wrapper, live.agent, live.descendant] {
        assert!(is_process_running(pid), "PID {pid} must still be running");
    }
    assert!(fixture.data.join("worktrees/par-live").exists());

    let cancel = cli(&fixture, &["cancel", "par-live"]);
    assert_success(&cancel, "cancel");
    let status = wait_until("live run to exit after cancel", || {
        live.run.try_wait().expect("poll run")
    });
    guard.forget(live.run.id());
    assert_eq!(status.code(), Some(130));
    for pid in [live.wrapper, live.agent, live.descendant] {
        wait_until(&format!("PID {pid} to exit"), || {
            (!is_process_running(pid)).then_some(())
        });
        guard.forget(pid);
    }
    assert!(fixture.data.join("worktrees/par-live").exists());
}

#[test]
fn held_data_directory_lock_refuses_mutating_commands_and_leaves_readers_alone() {
    let fixture = fixture();
    let run = run_command(&fixture, "locked-prior", "edit")
        .output()
        .expect("run CLI");
    assert_success(&run, "prior run");
    let prior_record = record_debug(&fixture, "locked-prior");
    let prior_worktree = fixture.data.join("worktrees/locked-prior");
    let before = snapshot(&fixture.repo);

    let lock = DataDirectoryLock::acquire(&fixture.data).expect("hold data-directory lock");
    let holder = std::process::id().to_string();

    let refused_run = run_command(&fixture, "locked-new", "edit")
        .output()
        .expect("run CLI");
    let cancel = cli(&fixture, &["cancel", "locked-prior"]);
    let cleanup = cli(&fixture, &["cleanup", "locked-prior"]);
    let recover = cli(
        &fixture,
        &[
            "recover-owner",
            "locked-prior",
            "--attempt-id",
            "locked-prior-attempt-1",
            "--session-id",
            "locked-prior-session-1",
            "--generation",
            "1",
            "--confirm-processes-exited",
            "--reason",
            "lock test",
        ],
    );
    for (what, output) in [
        ("run", &refused_run),
        ("cancel", &cancel),
        ("cleanup", &cleanup),
        ("recover-owner", &recover),
    ] {
        assert!(
            !output.status.success(),
            "{what} must refuse while the lock is held: {}",
            stdout(output)
        );
        assert!(
            stderr(output).contains(&holder),
            "{what} must name the holder PID {holder}: {}",
            stderr(output)
        );
    }

    // Read-only commands take no lock.
    assert_success(&cli(&fixture, &["tasks"]), "tasks");
    assert_success(&cli(&fixture, &["inspect", "locked-prior"]), "inspect");
    assert_success(&cli(&fixture, &["diff", "locked-prior"]), "diff");

    assert_no_new_task(&fixture, "locked-new");
    assert_eq!(record_debug(&fixture, "locked-prior"), prior_record);
    assert!(prior_worktree.exists(), "refused cleanup kept the worktree");
    assert_eq!(snapshot(&fixture.repo), before);
    drop(lock);
}

// ---- the gate: process-level records ------------------------------------------------

#[test]
fn unidentified_running_session_refuses_scheduling_and_preserves_records() {
    let fixture = fixture();
    seed_running_session(&fixture, "unidentified", None);
    let seeded = record_debug(&fixture, "unidentified");

    let output = refused_run(&fixture, "after-unidentified");
    assert!(
        stderr(&output).contains("unidentified"),
        "refusal must name the blocking task: {}",
        stderr(&output)
    );
    assert_eq!(record_debug(&fixture, "unidentified"), seeded);
    // A second run is refused the same way: nothing was laundered into a resolved state.
    refused_run(&fixture, "after-unidentified-again");
    assert_eq!(record_debug(&fixture, "unidentified"), seeded);
}

#[test]
fn running_task_without_an_unfinished_session_refuses_scheduling() {
    let fixture = fixture();
    seed_running_session(&fixture, "orphaned", None);
    open_store(&fixture)
        .update_session_state("orphaned", "orphaned-session-1", SessionState::Exited)
        .unwrap();
    let seeded = record_debug(&fixture, "orphaned");

    let output = refused_run(&fixture, "after-orphaned");
    assert!(
        stderr(&output).contains("orphaned"),
        "refusal must name the blocking task: {}",
        stderr(&output)
    );
    assert_eq!(record_debug(&fixture, "orphaned"), seeded);
}

// ---- the gate: interrupted-task workspace and delivery evidence --------------------
//
// Coordinator ruling for M2-06 (plan section 9): for a task recovered THIS PASS
// (proven-exited unfinished session) the engine inspects the recorded workspace
// and persisted delivery journal, via the runtime's advisory classifier, before
// it turns the task Failed.
// - Foreign or Unknown workspace, Partial or Ambiguous delivery, or Confirmed
//   delivery on a non-Succeeded task: refuse to schedule; keep every record,
//   worktree and file; do not launder the evidence into a settled state.
// - Dirty, Missing, BranchMoved or Matching known-owned workspace with no
//   contradictory delivery: preserve it and resume scheduling, so one ordinary
//   crash never blocks the data directory forever.
// - Persisted delivery journals are inspected for ANY task: Partial, Ambiguous
//   or Confirmed-on-non-Succeeded blocks; consistent Confirmed or NotApplied
//   alone does not. Settled workspaces are never a global blocker.

#[test]
fn interrupted_task_with_known_owned_workspace_resumes_scheduling_and_keeps_evidence() {
    let fixture = fixture();
    let ids = [
        "crash-matching",
        "crash-branch-moved",
        "crash-dirty",
        "crash-missing",
    ];
    for id in ids {
        seed_running_session(&fixture, id, Some(dead_identity(&fixture, id)));
    }
    let matching = registered_worktree(&fixture, "crash-matching");

    let moved = registered_worktree(&fixture, "crash-branch-moved");
    fs::write(moved.join("kept.txt"), "kept\n").unwrap();
    git(&moved, &["add", "kept.txt"]);
    git(&moved, &["commit", "-m", "agent progress"]);
    let moved_head = git(&moved, &["rev-parse", "HEAD"]);

    let dirty = registered_worktree(&fixture, "crash-dirty");
    fs::write(dirty.join("sample.txt"), "uncommitted agent edit\n").unwrap();
    fs::write(dirty.join("unsaved.txt"), "uncommitted new file\n").unwrap();

    let missing = registered_worktree(&fixture, "crash-missing");
    git(
        &fixture.repo,
        &["worktree", "remove", "--force", missing.to_str().unwrap()],
    );
    let before = snapshot(&fixture.repo);

    let run = run_command(&fixture, "after-crashes", "edit")
        .output()
        .expect("run CLI");
    assert_success(&run, "run after recovering known-owned interrupted tasks");
    assert!(stdout(&run).contains("status: Verifying"));

    let store = open_store(&fixture);
    for id in ids {
        let record = store.get(id).unwrap();
        assert_eq!(record.state, TaskState::Failed, "{id}");
        assert_eq!(record.attempts.len(), 1, "{id}: attempt history preserved");
        assert_eq!(record.attempts[0].state, AttemptState::Failed, "{id}");
        assert_eq!(record.sessions.len(), 1, "{id}: session history preserved");
        assert_eq!(record.sessions[0].state, SessionState::Exited, "{id}");
    }
    assert_eq!(
        store.get("after-crashes").unwrap().state,
        TaskState::Verifying
    );
    assert!(matching.exists());
    assert_eq!(
        fs::read_to_string(moved.join("kept.txt")).unwrap(),
        "kept\n"
    );
    assert_eq!(git(&moved, &["rev-parse", "HEAD"]), moved_head);
    assert_eq!(
        fs::read(dirty.join("sample.txt")).unwrap(),
        b"uncommitted agent edit\n",
        "a dirty worktree is preserved, never reset or relabelled clean"
    );
    assert_eq!(
        fs::read(dirty.join("unsaved.txt")).unwrap(),
        b"uncommitted new file\n"
    );
    assert!(
        git(&fixture.repo, &["branch", "--list"]).contains("orchestrator/crash-missing"),
        "the missing worktree's branch is preserved"
    );
    assert_eq!(
        snapshot(&fixture.repo).tracked_dirty,
        before.tracked_dirty,
        "primary checkout untouched"
    );
}

#[test]
fn settled_failed_task_with_dirty_worktree_does_not_block_scheduling() {
    let fixture = fixture();
    seed_running_session(&fixture, "settled-dirty", None);
    let worktree = registered_worktree(&fixture, "settled-dirty");
    settle_failed(&fixture, "settled-dirty");
    fs::write(worktree.join("unsaved.txt"), "uncommitted new file\n").unwrap();
    let settled = record_debug(&fixture, "settled-dirty");

    let run = run_command(&fixture, "after-settled-dirty", "edit")
        .output()
        .expect("run CLI");
    assert_success(
        &run,
        "run beside a settled Failed task with a dirty worktree",
    );
    assert_eq!(record_debug(&fixture, "settled-dirty"), settled);
    assert_eq!(
        fs::read(worktree.join("unsaved.txt")).unwrap(),
        b"uncommitted new file\n"
    );
}

#[test]
fn interrupted_foreign_workspace_refuses_scheduling_and_keeps_all_evidence() {
    let fixture = fixture();
    seed_running_session(
        &fixture,
        "crash-foreign",
        Some(dead_identity(&fixture, "crash-foreign")),
    );
    let foreign = foreign_workspace(&fixture, "crash-foreign");

    for new_id in ["foreign-first", "foreign-second"] {
        let output = refused_run(&fixture, new_id);
        assert_refusal_cites(&output, "crash-foreign", "foreign");
        assert_eq!(
            fs::read(foreign.join("precious.txt")).unwrap(),
            b"operator data\n",
            "foreign files must never be touched"
        );
        let record = open_store(&fixture).get("crash-foreign").unwrap();
        assert_eq!(record.attempts.len(), 1, "attempt history preserved");
        assert_eq!(record.sessions.len(), 1, "session history preserved");
        // The refusal must not become a resolution: neither a Failed task that
        // the next run would treat as settled, nor a Running task whose
        // session was terminated (an irreversible no-session tombstone).
        assert!(
            !(record.state == TaskState::Running
                && no_unfinished_session(&fixture, "crash-foreign")),
            "refusal left a Running task with no unfinished session"
        );
        assert_ne!(
            record.state,
            TaskState::Failed,
            "refusal must not settle the interrupted task as Failed"
        );
    }
}

#[test]
fn interrupted_task_whose_project_cannot_be_inspected_refuses_scheduling() {
    let fixture = fixture();
    // The recorded workspace exists, but its project's repository root is gone, so
    // the workspace cannot be inspected (Unknown, not Missing and not Matching).
    let gone = fixture.root.path().join("vanished-repo");
    let gone = gone.display().to_string();
    seed_running_session_in(
        &fixture,
        "crash-unknown",
        Some(dead_identity(&fixture, "crash-unknown")),
        &gone,
        &gone,
    );
    let worktree = registered_worktree(&fixture, "crash-unknown");
    fs::write(worktree.join("unsaved.txt"), "uncommitted new file\n").unwrap();

    let output = refused_run(&fixture, "after-crash-unknown");
    assert_refusal_cites(&output, "crash-unknown", "unknown");
    assert_eq!(
        fs::read(worktree.join("unsaved.txt")).unwrap(),
        b"uncommitted new file\n"
    );
    let record = open_store(&fixture).get("crash-unknown").unwrap();
    assert_eq!(record.attempts.len(), 1);
    assert_ne!(record.state, TaskState::Failed, "not settled by refusal");
}

#[test]
fn interrupted_task_without_a_recorded_workspace_refuses_scheduling() {
    let fixture = fixture();
    // A proven-dead session whose task never recorded a workspace: nothing can be
    // inspected, so the gate must fail closed (Unknown) rather than settle it.
    seed_running_session(
        &fixture,
        "crash-no-workspace",
        Some(dead_identity(&fixture, "crash-no-workspace")),
    );
    assert!(
        open_store(&fixture)
            .list_workspaces()
            .unwrap()
            .iter()
            .all(|workspace| workspace.task_id != "crash-no-workspace"),
        "fixture must not record a workspace"
    );

    for new_id in ["no-workspace-first", "no-workspace-second"] {
        let output = refused_run(&fixture, new_id);
        assert_refusal_cites(&output, "crash-no-workspace", "unknown");
        let record = open_store(&fixture).get("crash-no-workspace").unwrap();
        assert_eq!(record.attempts.len(), 1, "attempt history preserved");
        assert_eq!(record.sessions.len(), 1, "session history preserved");
        assert_ne!(record.state, TaskState::Failed, "not settled by refusal");
        assert!(
            !(record.state == TaskState::Running
                && no_unfinished_session(&fixture, "crash-no-workspace")),
            "refusal left a Running task with no unfinished session"
        );
    }
}

// ---- the gate: persisted delivery journals ------------------------------------------

#[test]
fn interrupted_task_with_partial_delivery_refuses_scheduling_and_keeps_evidence() {
    let fixture = fixture();
    seed_running_session(
        &fixture,
        "crash-partial",
        Some(dead_identity(&fixture, "crash-partial")),
    );
    let worktree = registered_worktree(&fixture, "crash-partial");
    // One step landed (ref == candidate), the other did not: Partial.
    let landed = review_ref("crash-partial", "");
    let missing = review_ref("crash-partial", "-retry");
    point_ref_at_candidate(&fixture, &landed);
    record_delivery(&fixture, "crash-partial", "step-landed", &landed);
    record_delivery(&fixture, "crash-partial", "step-unlanded", &missing);

    let output = refused_run(&fixture, "after-crash-partial");
    assert_refusal_cites(&output, "crash-partial", "partial");
    assert!(worktree.exists());
    let record = open_store(&fixture).get("crash-partial").unwrap();
    assert_eq!(record.attempts.len(), 1);
    assert_ne!(record.state, TaskState::Failed, "not settled by refusal");
    assert_eq!(
        open_store(&fixture)
            .delivery_steps_for_task("crash-partial")
            .unwrap()
            .len(),
        2,
        "delivery journal untouched"
    );
}

#[test]
fn settled_task_with_partial_delivery_journal_refuses_scheduling() {
    let fixture = fixture();
    seed_running_session(&fixture, "settled-partial", None);
    settle_failed(&fixture, "settled-partial");
    let landed = review_ref("settled-partial", "");
    point_ref_at_candidate(&fixture, &landed);
    record_delivery(&fixture, "settled-partial", "step-landed", &landed);
    record_delivery(
        &fixture,
        "settled-partial",
        "step-unlanded",
        &review_ref("settled-partial", "-retry"),
    );
    let settled = record_debug(&fixture, "settled-partial");

    let output = refused_run(&fixture, "after-settled-partial");
    assert_refusal_cites(&output, "settled-partial", "partial");
    assert_eq!(record_debug(&fixture, "settled-partial"), settled);
}

#[test]
fn settled_task_with_ambiguous_delivery_journal_refuses_scheduling() {
    let fixture = fixture();
    seed_running_session(&fixture, "settled-ambiguous", None);
    settle_failed(&fixture, "settled-ambiguous");
    // The review ref exists but points at something other than the candidate.
    let target = review_ref("settled-ambiguous", "");
    git(
        &fixture.repo,
        &["update-ref", &target, &unrelated_commit(&fixture)],
    );
    record_delivery(&fixture, "settled-ambiguous", "step-1", &target);

    let output = refused_run(&fixture, "after-settled-ambiguous");
    assert_refusal_cites(&output, "settled-ambiguous", "ambiguous");
}

#[test]
fn confirmed_delivery_on_a_failed_task_is_contradictory_and_refuses_scheduling() {
    let fixture = fixture();
    seed_running_session(&fixture, "settled-contradiction", None);
    settle_failed(&fixture, "settled-contradiction");
    let target = review_ref("settled-contradiction", "");
    point_ref_at_candidate(&fixture, &target);
    record_delivery(&fixture, "settled-contradiction", "step-1", &target);

    let output = refused_run(&fixture, "after-settled-contradiction");
    assert_refusal_cites(&output, "settled-contradiction", "confirmed");
}

#[test]
fn consistent_delivery_journals_do_not_block_scheduling() {
    let fixture = fixture();
    // Failed task whose single local-review step never created its ref: NotApplied.
    seed_running_session(&fixture, "journal-not-applied", None);
    settle_failed(&fixture, "journal-not-applied");
    record_delivery(
        &fixture,
        "journal-not-applied",
        "step-1",
        &review_ref("journal-not-applied", ""),
    );
    // Succeeded task whose step landed (ref == candidate): Confirmed and consistent.
    seed_running_session(&fixture, "journal-confirmed", None);
    settle_succeeded(&fixture, "journal-confirmed");
    let landed = review_ref("journal-confirmed", "");
    point_ref_at_candidate(&fixture, &landed);
    record_delivery(&fixture, "journal-confirmed", "step-1", &landed);

    let run = run_command(&fixture, "after-consistent-journals", "edit")
        .output()
        .expect("run CLI");
    assert_success(&run, "run beside consistent delivery journals");
    assert!(stdout(&run).contains("status: Verifying"));
}
