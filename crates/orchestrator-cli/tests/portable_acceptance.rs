//! Cross-platform (macOS and native Windows) acceptance for the single-task CLI
//! workflow: a real `run` in an isolated worktree, `tasks`/`inspect`/`diff`/`cleanup`,
//! primary-checkout invariance, and `cancel` of a task-owned process tree.
//! The agent is a std-only program compiled with `rustc` (must be on PATH), so no
//! shell, `sleep`, or POSIX permission bits are involved.

use orchestrator_core::identity::{ExactProcessIdentity, ProcessClassification};
use orchestrator_runtime::is_process_running;
use orchestrator_runtime::reconcile::{capture_exact_process_identity, observe_owner_process};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const CLI: &str = env!("CARGO_BIN_EXE_orchestrator-cli");
const FAKE_AGENT_SOURCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/fake_codex.rs");
const STEP_TIMEOUT: Duration = Duration::from_secs(30);

struct Fixture {
    _root: TempDir,
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
    git(&repo, &["config", "user.name", "Acceptance test"]);
    git(
        &repo,
        &["config", "user.email", "acceptance@example.invalid"],
    );
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    fs::write(repo.join("sample.txt"), "base\n").unwrap();
    fs::write(repo.join("other.txt"), "other base\n").unwrap();
    git(&repo, &["add", "sample.txt", "other.txt"]);
    git(&repo, &["commit", "-m", "initial"]);
    // Leave the primary checkout dirty so invariance covers tracked and untracked work.
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
        _root: root,
        repo,
        agent,
    }
}

/// Pins global and system Git config away (as the other crates' tests do) so a
/// user's `commit.gpgsign`, hooks path, or autocrlf cannot change the result.
/// Child processes of the CLI inherit it, so production Git calls are pinned too.
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
        .arg("acceptance objective")
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

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn assert_success(output: &Output, what: &str) {
    assert!(
        output.status.success(),
        "{what} failed: {:?}\nstdout: {}\nstderr: {}",
        output.status,
        stdout(output),
        String::from_utf8_lossy(&output.stderr)
    );
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

fn read_pidfile(path: &Path) -> Option<(u32, u32)> {
    let text = fs::read_to_string(path).ok()?;
    let mut pids = text.lines().map(|line| line.trim().parse::<u32>());
    Some((pids.next()?.ok()?, pids.next()?.ok()?))
}

#[test]
fn run_produces_isolated_diff_and_cleanup_preserves_primary() {
    let fixture = fixture();
    let before = snapshot(&fixture.repo);

    let run = run_command(&fixture, "accept-run", "edit")
        .output()
        .expect("run CLI");
    assert_success(&run, "run");
    let run_out = stdout(&run);
    assert!(run_out.contains("status: Verifying"), "{run_out}");
    let worktree = PathBuf::from(labeled(&run_out, "worktree: "));
    let branch = labeled(&run_out, "branch: ");
    let candidate = labeled(&run_out, "candidate: ");

    let primary = fs::canonicalize(&fixture.repo).unwrap();
    let worktree_canonical = fs::canonicalize(&worktree).expect("worktree exists");
    assert!(
        !worktree_canonical.starts_with(&primary),
        "worktree {} must be outside primary {}",
        worktree_canonical.display(),
        primary.display()
    );
    assert_eq!(
        fs::read_to_string(worktree.join("sample.txt")).unwrap(),
        "candidate\n"
    );
    assert_eq!(
        git(&worktree, &["rev-parse", "HEAD"]),
        candidate,
        "candidate commit is the worktree head"
    );
    assert_eq!(snapshot(&fixture.repo), before, "run changed the primary");
    assert_eq!(
        fs::read_to_string(fixture.repo.join("sample.txt")).unwrap(),
        "base\n"
    );
    assert!(!fixture.repo.join("new-file.txt").exists());

    let mut tasks = Command::new(CLI);
    let tasks = pin_git(&mut tasks)
        .arg("--data-dir")
        .arg(&fixture.data)
        .arg("tasks")
        .output()
        .expect("run tasks");
    assert_success(&tasks, "tasks");
    assert!(
        stdout(&tasks).contains("accept-run\tVerifying"),
        "{}",
        stdout(&tasks)
    );

    let inspect = cli(&fixture, "inspect", "accept-run");
    assert_success(&inspect, "inspect");
    let inspect_out = stdout(&inspect);
    assert!(inspect_out.contains("status: Verifying"), "{inspect_out}");
    assert!(inspect_out.contains(&branch), "{inspect_out}");

    let diff = cli(&fixture, "diff", "accept-run");
    assert_success(&diff, "diff");
    let diff_out = stdout(&diff);
    assert!(
        diff_out.contains("diff --git a/sample.txt b/sample.txt")
            && diff_out.contains("+candidate"),
        "{diff_out}"
    );
    assert!(
        diff_out.contains("new-file.txt") && diff_out.contains("+untracked candidate"),
        "diff must include the agent's new file: {diff_out}"
    );
    assert!(!diff_out.contains("primary-untracked.txt"), "{diff_out}");

    let refused = cli(&fixture, "cleanup", "accept-run");
    assert!(
        !refused.status.success(),
        "cleanup must refuse unlanded work"
    );
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("not reachable"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(worktree.exists(), "refused cleanup must keep the worktree");
    assert_eq!(
        snapshot(&fixture.repo),
        before,
        "refused cleanup changed the primary"
    );

    // Land the candidate; the operator's dirty files do not overlap it.
    git(&fixture.repo, &["merge", "--ff-only", &branch]);
    let cleanup = cli(&fixture, "cleanup", "accept-run");
    assert_success(&cleanup, "cleanup");
    assert!(!worktree.exists(), "cleanup must remove the worktree");

    let after = snapshot(&fixture.repo);
    assert_eq!(after.head, candidate, "primary advanced only by the merge");
    assert_eq!(after.tracked_dirty, before.tracked_dirty);
    assert_eq!(after.untracked, before.untracked);
    assert_eq!(
        after.status, before.status,
        "operator's tracked and untracked changes must survive run, merge and cleanup"
    );
}

#[test]
fn cancel_terminates_agent_and_descendant() {
    let fixture = fixture();
    let before = snapshot(&fixture.repo);
    let pidfile = fixture._root.path().join("pids.txt");

    let mut run = run_command(&fixture, "accept-cancel", "tree")
        .env("FAKE_AGENT_PIDFILE", &pidfile)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start CLI run");
    let mut guard = OwnedProcesses::default();
    // The run owner is our unreaped child, so its PID cannot have been reused.
    guard.track("run owner", run.id());

    let metadata = fixture.data.join("cli/accept-cancel/metadata.json");
    let wrapper_pid = wait_until("wrapper PID in metadata", || {
        if let Some(status) = run.try_wait().expect("poll run") {
            panic!("run exited early with {status}");
        }
        read_json_pid(&metadata)
    });
    guard.track("wrapper", wrapper_pid);
    let (agent_pid, descendant_pid) = wait_until("agent and descendant PIDs", || {
        if let Some(status) = run.try_wait().expect("poll run") {
            panic!("run exited early with {status}");
        }
        read_pidfile(&pidfile)
    });
    guard.track("agent", agent_pid);
    guard.track("descendant", descendant_pid);
    let tree = [wrapper_pid, agent_pid, descendant_pid];
    assert!(
        tree[0] != tree[1] && tree[0] != tree[2] && tree[1] != tree[2],
        "expected three distinct processes: {tree:?}"
    );
    for pid in tree {
        assert!(
            is_process_running(pid),
            "PID {pid} must be running before cancel"
        );
    }

    let cancel = cli(&fixture, "cancel", "accept-cancel");
    assert_success(&cancel, "cancel");
    let status = wait_until("run to exit after cancel", || {
        run.try_wait().expect("poll run")
    });
    guard.forget(run.id());
    assert_eq!(status.code(), Some(130), "run exit after cancel");
    let _ = run.wait_with_output();

    for (name, pid) in [
        ("wrapper", wrapper_pid),
        ("agent", agent_pid),
        ("descendant", descendant_pid),
    ] {
        wait_until(&format!("{name} PID {pid} to exit"), || {
            (!is_process_running(pid)).then_some(())
        });
        guard.forget(pid);
    }

    let inspect = cli(&fixture, "inspect", "accept-cancel");
    assert_success(&inspect, "inspect");
    let inspect_out = stdout(&inspect);
    assert!(inspect_out.contains("status: Cancelled"), "{inspect_out}");
    assert!(
        inspect_out.contains("cancellation request: Confirmed"),
        "{inspect_out}"
    );
    assert_eq!(
        listed_states(&inspect_out, "attempts:"),
        ["Cancelled"],
        "{inspect_out}"
    );
    assert_eq!(
        listed_states(&inspect_out, "sessions:"),
        ["Exited"],
        "{inspect_out}"
    );
    let worktree = fixture.data.join("worktrees/accept-cancel");
    assert!(worktree.exists(), "cancel must preserve the worktree");
    assert_eq!(
        snapshot(&fixture.repo),
        before,
        "cancel changed the primary"
    );
}

/// States from the tab-separated rows (two-space indent, state last) that
/// `inspect` prints under `header`.
fn listed_states(inspect: &str, header: &str) -> Vec<String> {
    inspect
        .lines()
        .skip_while(|line| *line != header)
        .skip(1)
        .take_while(|line| line.starts_with("  "))
        .filter(|line| !line.starts_with("    "))
        .map(|line| line.rsplit('\t').next().unwrap().to_string())
        .collect()
}

/// Failure-path cleanup for processes this test started. A process is signalled
/// only if the OS still reports the exact identity (PID plus start time, plus the
/// Windows creation time) captured when it was first seen. A dead, replaced, or
/// unreadable process is never signalled, and untracked processes never are.
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

impl Drop for OwnedProcesses {
    fn drop(&mut self) {
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
