//! M1-05 cleanup-safety contract, exercised through the real CLI binary: `cleanup`
//! must never remove a worktree that holds uncommitted or unlanded work, or one
//! that a live run owner still uses, and must leave the primary checkout alone.
//! Reuses the std-only fake agent (`support/fake_codex.rs`) so the same tests run
//! on macOS and native Windows. Every process the tests start is tracked by its
//! exact identity and is signalled on failure only while that identity still
//! matches, never by name.

use orchestrator_core::identity::{ExactProcessIdentity, ProcessClassification};
use orchestrator_runtime::is_process_running;
use orchestrator_runtime::reconcile::{capture_exact_process_identity, observe_owner_process};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
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

/// Everything about the operator's primary checkout that cleanup must not touch.
#[derive(Debug, PartialEq, Eq)]
struct Primary {
    head: String,
    status: String,
    tracked_dirty: Vec<u8>,
    untracked: Vec<u8>,
}

fn pin_git(command: &mut Command) -> &mut Command {
    command
        .env("GIT_CONFIG_GLOBAL", "/nonexistent-git-global")
        .env("GIT_CONFIG_SYSTEM", "/nonexistent-git-system")
}

fn git(dir: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    let output = pin_git(&mut command)
        .arg("-C")
        .arg(dir)
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

fn fixture() -> Fixture {
    let root = tempfile::tempdir().expect("create unique fixture directory");
    let repo = root.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--initial-branch=main"]);
    git(&repo, &["config", "user.name", "Cleanup safety test"]);
    git(&repo, &["config", "user.email", "cleanup@example.invalid"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    fs::write(repo.join("sample.txt"), "base\n").unwrap();
    fs::write(repo.join("other.txt"), "other base\n").unwrap();
    git(&repo, &["add", "sample.txt", "other.txt"]);
    git(&repo, &["commit", "-m", "initial"]);
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
        repo,
        agent,
        root,
    }
}

fn primary(repo: &Path) -> Primary {
    Primary {
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
        .arg("cleanup safety objective")
        .arg("--base-ref")
        .arg("main")
        .arg("--agent-program")
        .arg(&fixture.agent)
        .env("FAKE_AGENT_SCENARIO", scenario);
    command
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

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_success(output: &Output, what: &str) {
    assert!(
        output.status.success(),
        "{what} failed: {:?}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        stderr(output)
    );
}

fn labeled(output: &Output, label: &str) -> String {
    let text = String::from_utf8_lossy(&output.stdout);
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

fn read_pidfile(path: &Path) -> Option<(u32, u32)> {
    let text = fs::read_to_string(path).ok()?;
    let mut pids = text.lines().map(|line| line.trim().parse::<u32>());
    Some((pids.next()?.ok()?, pids.next()?.ok()?))
}

/// Failure-path cleanup for processes this test started. A process is signalled
/// only while the OS still reports the exact identity captured when it was first
/// seen; a dead, replaced, or unreadable process is never signalled.
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
}

impl OwnedProcesses {
    /// Signals every tracked process whose exact identity is still observed alive.
    fn terminate(&self) {
        for (pid, identity) in &self.0 {
            if observe_owner_process(*identity) != ProcessClassification::Alive {
                continue;
            }
            #[cfg(windows)]
            let _ = Command::new("taskkill")
                .args(["/F", "/PID", &pid.to_string()])
                .output();
            #[cfg(unix)]
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
        }
    }
}

impl Drop for OwnedProcesses {
    fn drop(&mut self) {
        self.terminate();
    }
}

/// Runs a clean worktree's task to the point where `run` finished with a
/// candidate commit, then lands that candidate on `main`, leaving a clean,
/// landed worktree that cleanup would legitimately remove.
fn landed_worktree(fixture: &Fixture, task_id: &str) -> PathBuf {
    let run = run_command(fixture, task_id, "edit")
        .output()
        .expect("run CLI");
    assert_success(&run, "run");
    let worktree = PathBuf::from(labeled(&run, "worktree: "));
    let branch = labeled(&run, "branch: ");
    git(&fixture.repo, &["merge", "--ff-only", &branch]);
    assert_eq!(git(&worktree, &["status", "--porcelain"]), "");
    worktree
}

/// Asserts a refused cleanup left the worktree, its HEAD, and its exact state.
fn assert_refused_and_preserved(
    fixture: &Fixture,
    task_id: &str,
    worktree: &Path,
    reason: &str,
    primary_before: &Primary,
) {
    let head = git(worktree, &["rev-parse", "HEAD"]);
    let status = git(
        worktree,
        &["status", "--porcelain", "--untracked-files=all"],
    );
    let sample = fs::read(worktree.join("sample.txt")).ok();
    let refused = cli(fixture, "cleanup", task_id);
    assert!(
        !refused.status.success(),
        "cleanup must refuse ({reason}); stdout: {}",
        String::from_utf8_lossy(&refused.stdout)
    );
    assert!(
        worktree.exists(),
        "refused cleanup removed worktree ({reason})"
    );
    assert_eq!(git(worktree, &["rev-parse", "HEAD"]), head, "{reason}");
    assert_eq!(
        git(
            worktree,
            &["status", "--porcelain", "--untracked-files=all"]
        ),
        status,
        "{reason}"
    );
    assert_eq!(
        fs::read(worktree.join("sample.txt")).ok(),
        sample,
        "{reason}"
    );
    assert_eq!(
        &primary(&fixture.repo),
        primary_before,
        "refused cleanup changed the primary ({reason})"
    );
}

/// Staged-only, deleted-tracked and committed-but-unlanded work all block cleanup of
/// an otherwise landed worktree; only after the work is landed does cleanup remove it.
#[test]
fn cleanup_refuses_staged_deleted_and_unlanded_work_in_a_landed_worktree() {
    let fixture = fixture();
    let worktree = landed_worktree(&fixture, "retain-variants");
    let before = primary(&fixture.repo);

    fs::write(worktree.join("staged.txt"), "staged only\n").unwrap();
    git(&worktree, &["add", "staged.txt"]);
    assert_refused_and_preserved(&fixture, "retain-variants", &worktree, "staged", &before);
    assert_eq!(
        fs::read(worktree.join("staged.txt")).unwrap(),
        b"staged only\n"
    );
    git(&worktree, &["reset", "-q", "--", "staged.txt"]);
    fs::remove_file(worktree.join("staged.txt")).unwrap();

    fs::remove_file(worktree.join("sample.txt")).unwrap();
    assert_refused_and_preserved(&fixture, "retain-variants", &worktree, "deleted", &before);
    assert!(!worktree.join("sample.txt").exists());
    git(&worktree, &["checkout", "--", "sample.txt"]);

    fs::write(worktree.join("extra.txt"), "committed, not landed\n").unwrap();
    git(&worktree, &["add", "extra.txt"]);
    git(&worktree, &["commit", "-q", "-m", "unlanded follow-up"]);
    assert_refused_and_preserved(&fixture, "retain-variants", &worktree, "unlanded", &before);
    assert_eq!(
        fs::read(worktree.join("extra.txt")).unwrap(),
        b"committed, not landed\n"
    );

    let branch = git(&worktree, &["branch", "--show-current"]);
    git(&fixture.repo, &["merge", "--ff-only", &branch]);
    let cleanup = cli(&fixture, "cleanup", "retain-variants");
    assert_success(&cleanup, "cleanup of clean, landed worktree");
    assert!(!worktree.exists());
    let after = primary(&fixture.repo);
    assert_eq!(after.tracked_dirty, before.tracked_dirty);
    assert_eq!(after.untracked, before.untracked);
}

/// A worktree can be clean and trivially "landed" (HEAD still equals the base) while
/// the run owner and its agent are still using it. Cleanup must refuse it, leave the
/// owner and the agent tree running, and the worktree in place.
#[test]
fn cleanup_refuses_clean_worktree_while_run_owner_is_live() {
    let fixture = fixture();
    let before = primary(&fixture.repo);
    let pidfile = fixture.root.path().join("pids.txt");
    let run_stdout = fixture.root.path().join("run.stdout.log");
    let run_stderr = fixture.root.path().join("run.stderr.log");

    let mut run: Child = run_command(&fixture, "live-owner", "tree")
        .env("FAKE_AGENT_PIDFILE", &pidfile)
        .stdout(File::create(&run_stdout).expect("create run stdout log"))
        .stderr(File::create(&run_stderr).expect("create run stderr log"))
        .spawn()
        .expect("start CLI run");
    let mut guard = OwnedProcesses::default();
    // The run owner is our unreaped child, so its PID cannot have been reused.
    guard.track("run owner", run.id());

    let metadata = fixture.data.join("cli/live-owner/metadata.json");
    let early = |run: &mut Child| {
        if let Some(status) = run.try_wait().expect("poll run") {
            panic!(
                "run exited early with {status}\nstdout:\n{}\nstderr:\n{}",
                fs::read_to_string(&run_stdout).unwrap_or_default(),
                fs::read_to_string(&run_stderr).unwrap_or_default()
            );
        }
    };
    let wrapper_pid = wait_until("wrapper PID in metadata", || {
        early(&mut run);
        read_json_pid(&metadata)
    });
    guard.track("wrapper", wrapper_pid);
    let (agent_pid, descendant_pid) = wait_until("agent and descendant PIDs", || {
        early(&mut run);
        read_pidfile(&pidfile)
    });
    guard.track("agent", agent_pid);
    guard.track("descendant", descendant_pid);
    let tree = [wrapper_pid, agent_pid, descendant_pid];

    let worktree = fixture.data.join("worktrees/live-owner");
    assert!(worktree.exists(), "run must have created its worktree");
    // Precondition: nothing but liveness can justify refusing this cleanup.
    assert_eq!(
        git(
            &worktree,
            &["status", "--porcelain", "--untracked-files=all"]
        ),
        "",
        "worktree must be clean"
    );
    let head = git(&worktree, &["rev-parse", "HEAD"]);
    assert_eq!(head, git(&fixture.repo, &["rev-parse", "main"]));

    let refused = cli(&fixture, "cleanup", "live-owner");
    let still_alive: Vec<bool> = tree.iter().map(|pid| is_process_running(*pid)).collect();
    let worktree_kept = worktree.exists();
    let run_still_running = run.try_wait().expect("poll run").is_none();

    // Stop the run through its own owner before asserting, so a failed assertion
    // cannot leave the fake agent tree running (the guard still backs this up).
    let cancel = cli(&fixture, "cancel", "live-owner");
    let status = wait_until("run to exit after cancel", || {
        run.try_wait().expect("poll run")
    });
    guard.forget(run.id());
    for pid in tree {
        wait_until(&format!("PID {pid} to exit"), || {
            (!is_process_running(pid)).then_some(())
        });
        guard.forget(pid);
    }

    assert!(
        !refused.status.success(),
        "cleanup must refuse a worktree whose run owner is live; stdout: {}",
        String::from_utf8_lossy(&refused.stdout)
    );
    assert!(worktree_kept, "cleanup removed the live owner's worktree");
    assert_eq!(still_alive, [true; 3], "cleanup disturbed the agent tree");
    assert!(run_still_running, "cleanup disturbed the run owner");
    assert_success(&cancel, "cancel");
    assert_eq!(status.code(), Some(130), "run exit after cancel");
    assert_eq!(
        &primary(&fixture.repo),
        &before,
        "cleanup changed the primary"
    );
}

/// An open run owner that is provably exited (our own killed, reaped child) must not
/// block cleanup: a clean, landed worktree follows the existing safe-cleanup contract.
/// The fake agent tree is stopped first so only the owner state decides the outcome.
#[test]
fn cleanup_follows_existing_contract_for_provably_exited_run_owner() {
    let fixture = fixture();
    let before = primary(&fixture.repo);
    let pidfile = fixture.root.path().join("pids.txt");

    let mut run: Child = run_command(&fixture, "exited-owner", "tree")
        .env("FAKE_AGENT_PIDFILE", &pidfile)
        .stdout(File::create(fixture.root.path().join("run.stdout.log")).unwrap())
        .stderr(File::create(fixture.root.path().join("run.stderr.log")).unwrap())
        .spawn()
        .expect("start CLI run");
    let mut guard = OwnedProcesses::default();
    // The run owner is our unreaped child, so its PID cannot have been reused.
    guard.track("run owner", run.id());
    let owner_identity = capture_exact_process_identity(run.id()).expect("run owner identity");

    let metadata = fixture.data.join("cli/exited-owner/metadata.json");
    let wrapper_pid = wait_until("wrapper PID in metadata", || read_json_pid(&metadata));
    guard.track("wrapper", wrapper_pid);
    let (agent_pid, descendant_pid) =
        wait_until("agent and descendant PIDs", || read_pidfile(&pidfile));
    guard.track("agent", agent_pid);
    guard.track("descendant", descendant_pid);
    let tree = [wrapper_pid, agent_pid, descendant_pid];

    run.kill().expect("kill our own run owner");
    run.wait().expect("reap run owner");
    assert_eq!(
        observe_owner_process(owner_identity),
        ProcessClassification::Exited,
        "precondition: the run owner must be provably exited"
    );
    guard.forget(run.id());
    guard.terminate();
    for pid in tree {
        wait_until(&format!("PID {pid} to exit"), || {
            (!is_process_running(pid)).then_some(())
        });
        guard.forget(pid);
    }

    let worktree = fixture.data.join("worktrees/exited-owner");
    assert!(worktree.exists(), "run must have created its worktree");
    assert_eq!(
        git(
            &worktree,
            &["status", "--porcelain", "--untracked-files=all"]
        ),
        "",
        "worktree must be clean"
    );
    let cleanup = cli(&fixture, "cleanup", "exited-owner");
    assert_success(&cleanup, "cleanup with a provably exited run owner");
    assert!(
        !worktree.exists(),
        "clean, landed worktree should be removed"
    );
    assert_eq!(&primary(&fixture.repo), &before);
}

/// Deterministic startup-window state: the run created its worktree, recorded the
/// task as Running, then died before any agent launched or any run owner was
/// registered. Nothing has settled the task, so cleanup must fail closed and keep
/// the worktree (the operator settles the task first). Needs the CLI's existing
/// `fault-injection` feature (same seam as `recovery.rs`); Unix-only because the
/// injected crash is a Unix SIGKILL of the CLI itself.
#[cfg(all(unix, feature = "fault-injection"))]
#[test]
fn cleanup_refuses_running_task_that_never_registered_a_run_owner() {
    let fixture = fixture();
    let before = primary(&fixture.repo);
    let crashed = run_command(&fixture, "startup-crash", "edit")
        .env("AGENT_ORCH_FAULT_INJECTION", "crash-before-agent-launch")
        .output()
        .expect("run CLI");
    assert!(
        !crashed.status.success(),
        "injected crash must kill the run"
    );

    let worktree = fixture.data.join("worktrees/startup-crash");
    assert!(worktree.exists(), "run must have created its worktree");
    assert_eq!(
        git(
            &worktree,
            &["status", "--porcelain", "--untracked-files=all"]
        ),
        "",
        "worktree must be clean"
    );
    assert_eq!(
        git(&worktree, &["rev-parse", "HEAD"]),
        git(&fixture.repo, &["rev-parse", "main"]),
        "worktree must be landed"
    );
    let refused = cli(&fixture, "cleanup", "startup-crash");
    assert!(
        !refused.status.success(),
        "cleanup must not remove the worktree of an unsettled Running task; stdout: {}",
        String::from_utf8_lossy(&refused.stdout)
    );
    assert!(worktree.exists(), "refused cleanup removed the worktree");
    assert_eq!(&primary(&fixture.repo), &before);
}

/// Killing only the CLI run owner (our own child) leaves its agent tree running in
/// the worktree. An Exited owner must not be taken as proof that the worktree
/// writers are gone: while the wrapper, agent or descendant is demonstrably alive,
/// cleanup must refuse and keep the worktree. Unix-only because the owner kill must
/// not take the agent tree with it (Windows `Child::kill` semantics differ).
#[cfg(unix)]
#[test]
fn cleanup_refuses_exited_run_owner_while_its_agent_tree_is_alive() {
    let fixture = fixture();
    let before = primary(&fixture.repo);
    let pidfile = fixture.root.path().join("pids.txt");

    let mut run: Child = run_command(&fixture, "orphaned-tree", "tree")
        .env("FAKE_AGENT_PIDFILE", &pidfile)
        .stdout(File::create(fixture.root.path().join("run.stdout.log")).unwrap())
        .stderr(File::create(fixture.root.path().join("run.stderr.log")).unwrap())
        .spawn()
        .expect("start CLI run");
    let mut guard = OwnedProcesses::default();
    guard.track("run owner", run.id());
    let owner_identity = capture_exact_process_identity(run.id()).expect("run owner identity");
    let metadata = fixture.data.join("cli/orphaned-tree/metadata.json");
    let wrapper_pid = wait_until("wrapper PID in metadata", || read_json_pid(&metadata));
    guard.track("wrapper", wrapper_pid);
    let (agent_pid, descendant_pid) =
        wait_until("agent and descendant PIDs", || read_pidfile(&pidfile));
    guard.track("agent", agent_pid);
    guard.track("descendant", descendant_pid);
    let tree = [wrapper_pid, agent_pid, descendant_pid];
    let identities: Vec<_> = tree
        .iter()
        .map(|pid| capture_exact_process_identity(*pid).expect("tree identity"))
        .collect();

    run.kill().expect("kill our own run owner");
    run.wait().expect("reap run owner");
    assert_eq!(
        observe_owner_process(owner_identity),
        ProcessClassification::Exited,
        "precondition: the run owner must be provably exited"
    );
    // Evidence: classify each tree member by exact identity after the owner is gone.
    let survivors: Vec<_> = tree
        .iter()
        .zip(&identities)
        .map(|(pid, id)| (*pid, observe_owner_process(*id)))
        .collect();
    let any_alive = survivors
        .iter()
        .any(|(_, class)| *class == ProcessClassification::Alive);

    let worktree = fixture.data.join("worktrees/orphaned-tree");
    let refused = cli(&fixture, "cleanup", "orphaned-tree");
    let worktree_kept = worktree.exists();
    // Stop only our own tree before asserting; the guard backs this up.
    guard.forget(run.id());
    guard.terminate();
    for pid in tree {
        wait_until(&format!("PID {pid} to exit"), || {
            (!is_process_running(pid)).then_some(())
        });
        guard.forget(pid);
    }

    // Precondition (observed, not assumed): SIGKILL of the owner does not stop the
    // supervised tree, so the worktree still has live writers when cleanup runs.
    assert!(any_alive, "agent tree unexpectedly gone: {survivors:?}");
    assert!(
        !refused.status.success(),
        "cleanup removed a worktree while its agent tree was alive ({survivors:?}); stdout: {}",
        String::from_utf8_lossy(&refused.stdout)
    );
    assert!(worktree_kept, "refused cleanup removed the worktree");
    assert_eq!(&primary(&fixture.repo), &before);
}
