//! M2-05 portable acceptance: two concurrent `run`s, interrupted processes and
//! runtime failures, restart reconciliation, and no silently lost changes or
//! audit events. Nothing here is Unix-gated, so every test executes on macOS and
//! native Windows.
//!
//! The agent is the std-only `tests/support/m2_agent.rs` (compiled with `rustc`):
//! it writes one uncommitted file, publishes its PID, and holds on a barrier
//! file. Synchronisation is by probes (`wait_until`) and barrier files, never by
//! fixed sleeps. Processes are signalled only after re-observing the exact
//! identity (PID plus start time) captured when the test first saw them, and the
//! test never signals a process group.
//!
//! Platform notes:
//! - The crash is `Child::kill()` on the run owner (SIGKILL / TerminateProcess).
//!   On Unix the agent tree outlives the owner by design and is killed by exact
//!   identity; on Windows the owner's kill-on-close Job Object must end it, which
//!   the test proves with a bounded wait and no kill of its own.
//! - Event-insert rollback installs its trigger through SQLx (a dev-dependency
//!   at the store's pin), so it needs no `sqlite3` binary. The older
//!   `sqlite3`-hook cases, and `recovery.rs` as a whole, remain Unix-only.

use orchestrator_core::identity::{ExactProcessIdentity, ProcessClassification};
use orchestrator_core::task::{AttemptState, AuditEvent, SessionState, TaskState};
use orchestrator_runtime::is_process_running;
use orchestrator_runtime::reconcile::{capture_exact_process_identity, observe_owner_process};
use orchestrator_store::{SqliteStore, Store};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const CLI: &str = env!("CARGO_BIN_EXE_orchestrator-cli");
const AGENT_SOURCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/m2_agent.rs");
const STEP_TIMEOUT: Duration = Duration::from_secs(30);

const SUCCEEDED: [AuditEvent; 3] = [
    AuditEvent::TaskAdmitted,
    AuditEvent::ExecutionStarted,
    AuditEvent::CandidateRecorded,
];
const RUNNING: [AuditEvent; 2] = [AuditEvent::TaskAdmitted, AuditEvent::ExecutionStarted];
const FAILED: [AuditEvent; 3] = [
    AuditEvent::TaskAdmitted,
    AuditEvent::ExecutionStarted,
    AuditEvent::ExecutionFailed,
];
const CANCELLED: [AuditEvent; 3] = [
    AuditEvent::TaskAdmitted,
    AuditEvent::ExecutionStarted,
    AuditEvent::TaskCancelled,
];

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
    git(&repo, &["config", "user.name", "M2 test"]);
    git(&repo, &["config", "user.email", "m2@example.invalid"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    fs::write(repo.join("sample.txt"), "base\n").unwrap();
    fs::write(repo.join("other.txt"), "other base\n").unwrap();
    git(&repo, &["add", "sample.txt", "other.txt"]);
    git(&repo, &["commit", "-m", "initial"]);
    // Dirty primary: invariance covers tracked and untracked operator work.
    fs::write(repo.join("other.txt"), "other base\nlocal edit\n").unwrap();
    fs::write(repo.join("primary-untracked.txt"), "operator scratch\n").unwrap();

    let agent = root.path().join(if cfg!(windows) {
        "m2-agent.exe"
    } else {
        "m2-agent"
    });
    let compile = Command::new("rustc")
        .args(["--edition=2021", "-O"])
        .arg(AGENT_SOURCE)
        .arg("-o")
        .arg(&agent)
        .output()
        .expect("run rustc (it must be on PATH)");
    assert!(
        compile.status.success(),
        "compile m2 agent: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    Fixture {
        data: root.path().join("data"),
        root,
        repo,
        agent,
    }
}

/// Pins global and system Git config away so a user's gpgsign, hooks path, or
/// autocrlf cannot change the result; CLI children inherit it.
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

fn open_store(fixture: &Fixture) -> SqliteStore {
    SqliteStore::new(&fixture.data.join("store.sqlite")).expect("open SQLite store")
}

fn cli(fixture: &Fixture, command: &str, task_id: &str) -> Output {
    let mut cli = Command::new(CLI);
    pin_git(&mut cli)
        .arg("--data-dir")
        .arg(&fixture.data)
        .arg(command)
        .arg(task_id)
        .output()
        .expect("run CLI command")
}

fn labeled(text: &str, label: &str) -> String {
    text.lines()
        .find_map(|line| line.strip_prefix(label))
        .unwrap_or_else(|| panic!("missing {label:?} in output:\n{text}"))
        .trim()
        .to_string()
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

fn read_ready_pid(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
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

/// Failure-path cleanup for processes this test started. A process is signalled
/// only while the OS still reports the exact identity captured when first seen;
/// a dead, replaced or unreadable process, and any untracked process, never is.
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

    /// Kills one tracked process (only if its exact identity is still alive),
    /// waits for it to be gone, and stops tracking it.
    fn terminate(&mut self, pid: u32) {
        for (tracked, identity) in self.0.iter().filter(|(tracked, _)| *tracked == pid) {
            kill_if_same(*tracked, *identity);
        }
        wait_gone(pid);
        self.forget(pid);
    }
}

impl Drop for OwnedProcesses {
    fn drop(&mut self) {
        for (pid, identity) in &self.0 {
            kill_if_same(*pid, *identity);
        }
    }
}

/// After a run owner has died: when `reaped_by_owner_death` the tree must be
/// gone on its own (Windows Job Object kill-on-close, or the Unix hook's group
/// kill), proven by a bounded wait with no kill issued by the test. Otherwise
/// (Unix `Child::kill`, where the tree outlives the owner by design) the
/// leftovers are killed by exact identity.
fn settle_tree(guard: &mut OwnedProcesses, live: &Live, reaped_by_owner_death: bool) {
    for pid in [live.wrapper, live.agent] {
        if reaped_by_owner_death {
            wait_gone(pid);
            guard.forget(pid);
        } else {
            guard.terminate(pid);
        }
    }
}

fn wait_gone(pid: u32) {
    wait_until(&format!("PID {pid} to exit"), || {
        (!is_process_running(pid)).then_some(())
    });
}

/// One spawned `run` and its barrier. `stdout`/`stderr` are files, not pipes, so
/// nothing can fill a pipe and block the run.
struct Run {
    id: &'static str,
    child: Child,
    stdout: PathBuf,
    stderr: PathBuf,
    ready: PathBuf,
    barrier: PathBuf,
}

/// A `run` whose agent is live: wrapper and agent PIDs are known and tracked.
struct Live {
    run: Run,
    wrapper: u32,
    agent: u32,
}

fn spawn_run(
    fixture: &Fixture,
    guard: &mut OwnedProcesses,
    id: &'static str,
    extra_env: &[(&str, &str)],
) -> Run {
    let dir = fixture.root.path();
    let ready = dir.join(format!("{id}.ready"));
    let barrier = dir.join(format!("{id}.barrier"));
    let stdout = dir.join(format!("{id}.run.stdout.log"));
    let stderr = dir.join(format!("{id}.run.stderr.log"));
    let mut command = Command::new(CLI);
    pin_git(&mut command)
        .arg("--data-dir")
        .arg(&fixture.data)
        .arg("run")
        .arg("--repo")
        .arg(&fixture.repo)
        .arg("--task-id")
        .arg(id)
        .arg("--objective")
        .arg("m2 objective")
        .arg("--base-ref")
        .arg("main")
        .arg("--agent-program")
        .arg(&fixture.agent)
        .env("M2_TASK_FILE", task_file(id))
        .env("M2_TASK_CONTENT", task_content(id))
        .env("M2_READY_MARKER", &ready)
        .env("M2_BARRIER_FILE", &barrier);
    for (key, value) in extra_env {
        command.env(key, value);
    }
    let child = command
        .stdout(File::create(&stdout).expect("create run stdout log"))
        .stderr(File::create(&stderr).expect("create run stderr log"))
        .spawn()
        .expect("start CLI run");
    // The run owner is our unreaped child, so its PID cannot have been reused.
    guard.track("run owner", child.id());
    Run {
        id,
        child,
        stdout,
        stderr,
        ready,
        barrier,
    }
}

fn task_file(id: &str) -> String {
    format!("{id}.txt")
}

fn task_content(id: &str) -> String {
    format!("uncommitted bytes of {id}\n")
}

fn logs(run: &Run) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&fs::read(&run.stdout).unwrap_or_default()),
        String::from_utf8_lossy(&fs::read(&run.stderr).unwrap_or_default())
    )
}

/// Waits until the agent is live and its tree PIDs are known, tracking them.
fn start(
    fixture: &Fixture,
    guard: &mut OwnedProcesses,
    id: &'static str,
    extra_env: &[(&str, &str)],
) -> Live {
    let mut run = spawn_run(fixture, guard, id, extra_env);
    let metadata = fixture.data.join("cli").join(id).join("metadata.json");
    let agent = wait_until(&format!("{id} agent ready marker"), || {
        if let Some(status) = run.child.try_wait().expect("poll run") {
            panic!("{id} run exited early with {status}\n{}", logs(&run));
        }
        read_ready_pid(&run.ready)
    });
    let wrapper = wait_until(&format!("{id} wrapper PID in metadata"), || {
        read_json_pid(&metadata)
    });
    guard.track("wrapper", wrapper);
    guard.track("agent", agent);
    assert_ne!(wrapper, agent, "{id}: wrapper and agent must be distinct");
    assert!(is_process_running(wrapper) && is_process_running(agent));
    Live {
        run,
        wrapper,
        agent,
    }
}

fn release(run: &Run) {
    fs::write(&run.barrier, b"release\n").expect("release barrier");
}

fn wait_exit(guard: &mut OwnedProcesses, run: &mut Run) -> ExitStatus {
    let status = wait_until(&format!("{} run to exit", run.id), || {
        run.child.try_wait().expect("poll run")
    });
    guard.forget(run.child.id());
    status
}

/// Waits for a live run's whole tree to be gone, then stops tracking it.
fn wait_tree_gone(guard: &mut OwnedProcesses, live: &Live) {
    for pid in [live.wrapper, live.agent] {
        wait_gone(pid);
        guard.forget(pid);
    }
}

/// Releases `live`, requires a clean `Verifying` exit, and returns the candidate.
fn complete(guard: &mut OwnedProcesses, live: &mut Live) -> String {
    release(&live.run);
    let status = wait_exit(guard, &mut live.run);
    assert!(
        status.success(),
        "{} failed: {status}\n{}",
        live.run.id,
        logs(&live.run)
    );
    wait_tree_gone(guard, live);
    let out = fs::read_to_string(&live.run.stdout).unwrap();
    assert!(out.contains("status: Verifying"), "{out}");
    labeled(&out, "candidate: ")
}

fn worktree(fixture: &Fixture, id: &str) -> PathBuf {
    fixture.data.join("worktrees").join(id)
}

/// The worktree still holds the task's uncommitted file, byte for byte, and Git
/// still reports it as changed.
fn assert_uncommitted_bytes(fixture: &Fixture, id: &str) {
    let tree = worktree(fixture, id);
    assert_eq!(
        fs::read(tree.join(task_file(id))).expect("task file survived"),
        task_content(id).into_bytes(),
        "{id}: uncommitted bytes changed"
    );
    assert!(
        git(&tree, &["status", "--porcelain"]).contains(&task_file(id)),
        "{id}: file is no longer reported as changed"
    );
}

/// The candidate commit is the worktree head, sits directly on `base`, and
/// contains exactly this task's file with its bytes.
fn assert_candidate(fixture: &Fixture, id: &str, candidate: &str, base: &str) {
    let tree = worktree(fixture, id);
    assert_eq!(git(&tree, &["rev-parse", "HEAD"]), candidate, "{id}");
    assert_eq!(git(&tree, &["rev-parse", "HEAD^"]), base, "{id}");
    assert_eq!(
        git(
            &tree,
            &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"]
        ),
        task_file(id),
        "{id}: candidate must contain only its own file"
    );
    assert_eq!(
        git(&tree, &["show", &format!("HEAD:{}", task_file(id))]),
        task_content(id).trim_end(),
        "{id}"
    );
}

fn assert_states(
    store: &SqliteStore,
    id: &str,
    state: TaskState,
    attempt: AttemptState,
    session: SessionState,
    events: &[AuditEvent],
) {
    let task = store.get(id).unwrap();
    assert_eq!(task.state, state, "{id} state");
    assert_eq!(
        task.attempts.iter().map(|a| a.state).collect::<Vec<_>>(),
        [attempt],
        "{id} attempts"
    );
    assert_eq!(
        task.sessions.iter().map(|s| s.state).collect::<Vec<_>>(),
        [session],
        "{id} sessions"
    );
    assert_eq!(task.events.as_slice(), events, "{id} events");
}

/// The journal is contiguous, holds exactly the expected events per task and in
/// total (a silently dropped tail row fails the total), and each task's own
/// event list agrees with it.
fn assert_journal(store: &SqliteStore, expected: &[(&str, &[AuditEvent])]) {
    let journal = store.event_journal(0).expect("read event journal");
    for (index, entry) in journal.iter().enumerate() {
        assert_eq!(entry.sequence, i64::try_from(index + 1).unwrap());
    }
    let total: usize = expected.iter().map(|(_, events)| events.len()).sum();
    assert_eq!(journal.len(), total, "journal length");
    for (task, events) in expected {
        let journaled = journal
            .iter()
            .filter(|entry| entry.task_id == *task)
            .map(|entry| entry.event)
            .collect::<Vec<_>>();
        assert_eq!(journaled.as_slice(), *events, "journal for {task}");
        assert_eq!(
            store.get(task).unwrap().events.as_slice(),
            *events,
            "task events for {task}"
        );
    }
}

/// Both agents are live and both run owners are registered.
fn wait_two_owners(store: &SqliteStore) {
    wait_until("two open cancellation owners", || {
        (store.open_cancellation_owners().unwrap().len() == 2).then_some(())
    });
}

#[test]
fn two_concurrent_runs_overlap_and_keep_their_changes() {
    let fixture = fixture();
    let before = snapshot(&fixture.repo);
    let base = git(&fixture.repo, &["rev-parse", "main"]);
    let mut guard = OwnedProcesses::default();
    let mut a = start(&fixture, &mut guard, "task-a", &[]);
    let mut b = start(&fixture, &mut guard, "task-b", &[]);
    let store = open_store(&fixture);
    wait_two_owners(&store);

    // Genuine overlap: both agents are alive at once, in disjoint worktrees.
    assert_ne!(a.agent, b.agent);
    assert!(is_process_running(a.agent) && is_process_running(b.agent));
    for id in ["task-a", "task-b"] {
        assert_eq!(store.get(id).unwrap().state, TaskState::Running);
        assert_uncommitted_bytes(&fixture, id);
    }
    assert!(!worktree(&fixture, "task-a").join("task-b.txt").exists());
    assert!(!worktree(&fixture, "task-b").join("task-a.txt").exists());

    let candidate_a = complete(&mut guard, &mut a);
    let candidate_b = complete(&mut guard, &mut b);
    assert_candidate(&fixture, "task-a", &candidate_a, &base);
    assert_candidate(&fixture, "task-b", &candidate_b, &base);
    for id in ["task-a", "task-b"] {
        assert_states(
            &store,
            id,
            TaskState::Verifying,
            AttemptState::CandidateReady,
            SessionState::Exited,
            &SUCCEEDED,
        );
    }
    assert!(store.open_cancellation_owners().unwrap().is_empty());
    assert_journal(&store, &[("task-a", &SUCCEEDED), ("task-b", &SUCCEEDED)]);
    assert_eq!(snapshot(&fixture.repo), before, "runs changed the primary");
}

#[test]
fn killed_run_owner_is_reconciled_while_parallel_task_completes() {
    crash_victim_scenario(false);
}

/// Same scenario, but the run owner crashes itself at the in-process
/// `kill-agent-group-and-cli` hook (SIGKILLs its owned group first on Unix; on
/// Windows the owner's death alone ends the Job Object tree).
#[cfg(feature = "fault-injection")]
#[test]
fn injected_owner_crash_is_reconciled_while_parallel_task_completes() {
    crash_victim_scenario(true);
}

fn crash_victim_scenario(use_hook: bool) {
    let fixture = fixture();
    let before = snapshot(&fixture.repo);
    let base = git(&fixture.repo, &["rev-parse", "main"]);
    let gate = fixture.root.path().join("victim.gate");
    let gate_text = gate.to_str().expect("UTF-8 temp path").to_string();
    let hook_env = [
        ("AGENT_ORCH_FAULT_INJECTION", "kill-agent-group-and-cli"),
        ("AGENT_ORCH_FAULT_GATE", gate_text.as_str()),
    ];
    let victim_env: &[(&str, &str)] = if use_hook { &hook_env } else { &[] };
    let mut guard = OwnedProcesses::default();
    let mut victim = start(&fixture, &mut guard, "victim", victim_env);
    let mut survivor = start(&fixture, &mut guard, "survivor", &[]);
    let store = open_store(&fixture);
    wait_two_owners(&store);
    assert_uncommitted_bytes(&fixture, "victim");
    assert_uncommitted_bytes(&fixture, "survivor");

    // Crash the victim's run owner, then make sure its leftover tree is gone by
    // exact identity before restarting anything.
    let crashed = if use_hook {
        fs::write(&gate, b"crash\n").expect("release crash gate");
        wait_exit(&mut guard, &mut victim.run)
    } else {
        victim.run.child.kill().expect("kill victim run owner");
        let status = victim.run.child.wait().expect("reap victim run owner");
        guard.forget(victim.run.child.id());
        status
    };
    assert!(
        !crashed.success(),
        "crashed run owner must not exit cleanly"
    );
    settle_tree(&mut guard, &victim, use_hook || cfg!(windows));

    // Nothing owns the victim any more: cancel must refuse, not fake a result.
    let cancel = cli(&fixture, "cancel", "victim");
    assert!(!cancel.status.success());
    let stderr = String::from_utf8_lossy(&cancel.stderr).into_owned();
    assert!(stderr.contains("no active cancellation owner"), "{stderr}");
    let after_cancel = store.get("victim").unwrap();
    assert_eq!(after_cancel.state, TaskState::Running);
    assert_eq!(after_cancel.events.as_slice(), &RUNNING);
    assert!(
        is_process_running(survivor.agent),
        "cancel must not touch the survivor"
    );

    // Restart: the next run reconciles the dead victim while the survivor is live.
    let mut recovery = spawn_run(&fixture, &mut guard, "recovery", &[]);
    release(&recovery);
    let status = wait_exit(&mut guard, &mut recovery);
    assert!(
        status.success(),
        "recovery run failed: {status}\n{}",
        logs(&recovery)
    );

    assert_states(
        &store,
        "victim",
        TaskState::Failed,
        AttemptState::Failed,
        SessionState::Exited,
        &FAILED,
    );
    assert_uncommitted_bytes(&fixture, "victim");
    assert_eq!(
        git(&worktree(&fixture, "victim"), &["rev-parse", "HEAD"]),
        base,
        "interrupted work must not be promoted to a candidate"
    );
    assert_eq!(store.get("survivor").unwrap().state, TaskState::Running);
    assert!(is_process_running(survivor.agent));
    assert_uncommitted_bytes(&fixture, "survivor");
    let owners = store.open_cancellation_owners().unwrap();
    assert_eq!(
        owners
            .iter()
            .map(|o| o.task_id.as_str())
            .collect::<Vec<_>>(),
        ["survivor"],
        "only the live survivor may still own a task"
    );

    let candidate = complete(&mut guard, &mut survivor);
    assert_candidate(&fixture, "survivor", &candidate, &base);
    assert_states(
        &store,
        "survivor",
        TaskState::Verifying,
        AttemptState::CandidateReady,
        SessionState::Exited,
        &SUCCEEDED,
    );
    assert_states(
        &store,
        "recovery",
        TaskState::Verifying,
        AttemptState::CandidateReady,
        SessionState::Exited,
        &SUCCEEDED,
    );
    assert!(store.open_cancellation_owners().unwrap().is_empty());
    // Reopen the store: the journal on disk, not this connection's view, must
    // hold every transition.
    assert_journal(
        &open_store(&fixture),
        &[
            ("victim", &FAILED),
            ("survivor", &SUCCEEDED),
            ("recovery", &SUCCEEDED),
        ],
    );
    assert_uncommitted_bytes(&fixture, "victim");
    assert_eq!(
        snapshot(&fixture.repo),
        before,
        "recovery changed the primary"
    );
}

/// Hook-driven pre-launch crash: the run owner dies holding the data lock with
/// a `Spawning` session and no recorded process identity, while a parallel task
/// is live. The stale lock is taken over, the scheduling gate fails closed
/// ("no recorded process identity"), and the parallel task is unaffected.
#[cfg(feature = "fault-injection")]
#[test]
fn prelaunch_crash_fails_closed_without_disturbing_the_parallel_task() {
    let fixture = fixture();
    let before = snapshot(&fixture.repo);
    let base = git(&fixture.repo, &["rev-parse", "main"]);
    let mut guard = OwnedProcesses::default();
    let mut survivor = start(&fixture, &mut guard, "survivor", &[]);
    let store = open_store(&fixture);
    wait_until("survivor owner", || {
        (store.open_cancellation_owners().unwrap().len() == 1).then_some(())
    });

    let mut crash = spawn_run(
        &fixture,
        &mut guard,
        "orphan",
        &[("AGENT_ORCH_FAULT_INJECTION", "crash-before-agent-launch")],
    );
    let status = wait_exit(&mut guard, &mut crash);
    assert!(!status.success(), "crashed run must not exit cleanly");
    assert_states(
        &store,
        "orphan",
        TaskState::Running,
        AttemptState::Running,
        SessionState::Spawning,
        &RUNNING,
    );

    let mut blocked = spawn_run(&fixture, &mut guard, "must-not-start", &[]);
    release(&blocked);
    let status = wait_exit(&mut guard, &mut blocked);
    assert!(!status.success(), "gate must refuse scheduling");
    assert!(
        logs(&blocked).contains("no recorded process identity"),
        "{}",
        logs(&blocked)
    );
    assert!(
        !fixture.data.join(".orchestrator.lock").exists(),
        "reconciliation run must release the recovered lock"
    );
    assert!(store.get("must-not-start").is_err());

    assert_eq!(store.get("survivor").unwrap().state, TaskState::Running);
    assert!(is_process_running(survivor.agent));
    assert_uncommitted_bytes(&fixture, "survivor");
    let candidate = complete(&mut guard, &mut survivor);
    assert_candidate(&fixture, "survivor", &candidate, &base);
    assert_journal(
        &open_store(&fixture),
        &[("orphan", &RUNNING), ("survivor", &SUCCEEDED)],
    );
    assert_eq!(snapshot(&fixture.repo), before, "crash changed the primary");
}

#[test]
fn cancel_targets_one_run_and_leaves_the_parallel_task_untouched() {
    let fixture = fixture();
    let before = snapshot(&fixture.repo);
    let base = git(&fixture.repo, &["rev-parse", "main"]);
    let mut guard = OwnedProcesses::default();
    let mut a = start(&fixture, &mut guard, "task-a", &[]);
    let mut b = start(&fixture, &mut guard, "task-b", &[]);
    let store = open_store(&fixture);
    wait_two_owners(&store);

    let cancel = cli(&fixture, "cancel", "task-a");
    assert!(
        cancel.status.success(),
        "cancel failed: {}",
        String::from_utf8_lossy(&cancel.stderr)
    );
    let status = wait_exit(&mut guard, &mut a.run);
    assert_eq!(status.code(), Some(130), "{}", logs(&a.run));
    wait_tree_gone(&mut guard, &a);

    assert_states(
        &store,
        "task-a",
        TaskState::Cancelled,
        AttemptState::Cancelled,
        SessionState::Exited,
        &CANCELLED,
    );
    let inspect = cli(&fixture, "inspect", "task-a");
    let inspect = String::from_utf8_lossy(&inspect.stdout).into_owned();
    assert!(
        inspect.contains("cancellation request: Confirmed"),
        "{inspect}"
    );
    assert_uncommitted_bytes(&fixture, "task-a");

    // The parallel task never saw the cancellation.
    assert_eq!(store.get("task-b").unwrap().state, TaskState::Running);
    assert!(is_process_running(b.agent));
    assert_uncommitted_bytes(&fixture, "task-b");

    let candidate = complete(&mut guard, &mut b);
    assert_candidate(&fixture, "task-b", &candidate, &base);
    assert!(store.open_cancellation_owners().unwrap().is_empty());
    assert_journal(&store, &[("task-a", &CANCELLED), ("task-b", &SUCCEEDED)]);
    assert_eq!(
        snapshot(&fixture.repo),
        before,
        "cancel changed the primary"
    );
}

#[test]
fn agent_runtime_failure_keeps_changes_and_parallel_task_and_a_new_run_succeeds() {
    let fixture = fixture();
    let before = snapshot(&fixture.repo);
    let base = git(&fixture.repo, &["rev-parse", "main"]);
    let mut guard = OwnedProcesses::default();
    let mut failing = start(&fixture, &mut guard, "failing", &[("M2_EXIT_CODE", "3")]);
    let mut healthy = start(&fixture, &mut guard, "healthy", &[]);
    let store = open_store(&fixture);
    wait_two_owners(&store);

    // The failing agent exits nonzero while its sibling is still live.
    release(&failing.run);
    let status = wait_exit(&mut guard, &mut failing.run);
    assert!(!status.success(), "failing run must report failure");
    wait_tree_gone(&mut guard, &failing);
    assert_states(
        &store,
        "failing",
        TaskState::Failed,
        AttemptState::Failed,
        SessionState::Exited,
        &FAILED,
    );
    assert_uncommitted_bytes(&fixture, "failing");
    assert_eq!(
        git(&worktree(&fixture, "failing"), &["rev-parse", "HEAD"]),
        base,
        "a failed attempt must not produce a candidate"
    );
    assert_eq!(store.get("healthy").unwrap().state, TaskState::Running);
    assert!(is_process_running(healthy.agent));
    assert_uncommitted_bytes(&fixture, "healthy");

    // Retry is a fresh run (the CLI has no retry command): it is admitted while
    // the healthy sibling is still live and does not disturb the failed task.
    let mut retry = spawn_run(&fixture, &mut guard, "retry", &[]);
    release(&retry);
    let status = wait_exit(&mut guard, &mut retry);
    assert!(
        status.success(),
        "retry run failed: {status}\n{}",
        logs(&retry)
    );
    let retry_out = fs::read_to_string(&retry.stdout).unwrap();
    assert_candidate(
        &fixture,
        "retry",
        &labeled(&retry_out, "candidate: "),
        &base,
    );

    let candidate = complete(&mut guard, &mut healthy);
    assert_candidate(&fixture, "healthy", &candidate, &base);
    assert_uncommitted_bytes(&fixture, "failing");
    assert!(store.open_cancellation_owners().unwrap().is_empty());
    assert_journal(
        &store,
        &[
            ("failing", &FAILED),
            ("healthy", &SUCCEEDED),
            ("retry", &SUCCEEDED),
        ],
    );
    assert_eq!(
        snapshot(&fixture.repo),
        before,
        "failure changed the primary"
    );
}

/// Runs one fixed SQL statement against the fixture store with SQLx (the store's
/// own driver), so fault triggers need no `sqlite3` binary on any OS.
fn exec_sql(fixture: &Fixture, sql: &'static str) {
    use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection};
    let options = SqliteConnectOptions::new().filename(fixture.data.join("store.sqlite"));
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build SQLx runtime")
        .block_on(async {
            let mut connection = SqliteConnection::connect_with(&options)
                .await
                .expect("connect to store");
            sqlx::query(sql)
                .execute(&mut connection)
                .await
                .unwrap_or_else(|error| panic!("{sql}: {error}"));
            connection.close().await.expect("close SQLx connection");
        });
}

const REJECT_ROLLBACK_EVENTS: &str = "CREATE TRIGGER m2_05_reject_rollback_event \
    BEFORE INSERT ON events WHEN NEW.task_id = 'rollback' \
    BEGIN SELECT RAISE(ABORT, 'M2-05 injected event write failure'); END";
const ALLOW_ROLLBACK_EVENTS: &str = "DROP TRIGGER m2_05_reject_rollback_event";

#[test]
fn event_write_failure_rolls_back_one_task_while_parallel_task_stays_live() {
    let fixture = fixture();
    let base = git(&fixture.repo, &["rev-parse", "main"]);
    let mut guard = OwnedProcesses::default();
    let mut survivor = start(&fixture, &mut guard, "survivor", &[]);
    let store = open_store(&fixture);
    assert_journal(&store, &[("survivor", &RUNNING)]);

    // Every event insert for `rollback` now fails inside the store transaction,
    // while the survivor's agent is live and its own writes stay allowed.
    exec_sql(&fixture, REJECT_ROLLBACK_EVENTS);
    let mut failing = spawn_run(&fixture, &mut guard, "rollback", &[]);
    let status = wait_exit(&mut guard, &mut failing);
    assert!(!status.success(), "{}", logs(&failing));
    let stderr = fs::read_to_string(&failing.stderr).unwrap();
    assert!(stderr.contains("cannot admit task rollback"), "{stderr}");

    // Rolled back, not half-written: no event, no worktree, no owner, no lock;
    // the survivor's agent, journal rows and bytes are untouched.
    let rolled_back = store.get("rollback").unwrap();
    assert_eq!(rolled_back.state, TaskState::Created);
    assert!(rolled_back.events.is_empty());
    assert!(!worktree(&fixture, "rollback").exists());
    assert!(!fixture.data.join(".orchestrator.lock").exists());
    assert_eq!(store.open_cancellation_owners().unwrap().len(), 1);
    assert!(is_process_running(survivor.agent));
    assert_uncommitted_bytes(&fixture, "survivor");
    assert_journal(&store, &[("survivor", &RUNNING), ("rollback", &[])]);

    // Clearing the fault lets the same transition record exactly once.
    exec_sql(&fixture, ALLOW_ROLLBACK_EVENTS);
    store
        .transition("rollback", orchestrator_core::task::TaskEvent::Admit)
        .unwrap();
    let candidate = complete(&mut guard, &mut survivor);
    assert_candidate(&fixture, "survivor", &candidate, &base);
    let reopened = open_store(&fixture);
    assert_eq!(reopened.get("rollback").unwrap().state, TaskState::Queued);
    assert_journal(
        &reopened,
        &[
            ("survivor", &SUCCEEDED),
            ("rollback", &[AuditEvent::TaskAdmitted]),
        ],
    );
}

#[test]
fn running_task_with_no_live_runs_left_is_reconciled_by_the_next_run() {
    // Both run owners die together (a machine-level interruption): nothing is
    // live, so the next run must fail both tasks closed and keep both changes.
    let fixture = fixture();
    let base = git(&fixture.repo, &["rev-parse", "main"]);
    let mut guard = OwnedProcesses::default();
    let mut first = start(&fixture, &mut guard, "first", &[]);
    let mut second = start(&fixture, &mut guard, "second", &[]);
    let store = open_store(&fixture);
    wait_two_owners(&store);

    for live in [&mut first, &mut second] {
        live.run.child.kill().expect("kill run owner");
        live.run.child.wait().expect("reap run owner");
        guard.forget(live.run.child.id());
        settle_tree(&mut guard, live, cfg!(windows));
    }

    let mut next = spawn_run(&fixture, &mut guard, "next", &[]);
    release(&next);
    let status = wait_exit(&mut guard, &mut next);
    assert!(
        status.success(),
        "next run failed: {status}\n{}",
        logs(&next)
    );
    for id in ["first", "second"] {
        assert_states(
            &store,
            id,
            TaskState::Failed,
            AttemptState::Failed,
            SessionState::Exited,
            &FAILED,
        );
        assert_uncommitted_bytes(&fixture, id);
        assert_eq!(git(&worktree(&fixture, id), &["rev-parse", "HEAD"]), base);
    }
    assert!(store.open_cancellation_owners().unwrap().is_empty());
    assert_journal(
        &store,
        &[
            ("first", &FAILED),
            ("second", &FAILED),
            ("next", &SUCCEEDED),
        ],
    );
}
