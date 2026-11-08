//! Black-box CLI tests use a fresh Git repository and data directory per case.
//! Updated: 2026-11-07T22:31:40-07:00.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use orchestrator_runtime::is_process_running;
use tempfile::TempDir;

const CLI: &str = env!("CARGO_BIN_EXE_orchestrator-cli");
const THREAD_ID: &str = "01a1b721-1f91-7393-ae1d-6aace8e4646a";

struct Fixture {
    _root: TempDir,
    repo: PathBuf,
    data: PathBuf,
    agent: PathBuf,
}

fn fixture(agent_script: &str) -> Fixture {
    let root = tempfile::tempdir().expect("create unique fixture directory");
    let repo = root.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--initial-branch=main"]);
    git(&repo, &["config", "user.name", "CLI test"]);
    git(&repo, &["config", "user.email", "cli-test@example.invalid"]);
    fs::write(repo.join("sample.txt"), "base\n").unwrap();
    git(&repo, &["add", "sample.txt"]);
    git(&repo, &["commit", "-m", "initial"]);
    let agent = root.path().join("fake-codex");
    fs::write(&agent, agent_script).unwrap();
    let mut permissions = fs::metadata(&agent).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&agent, permissions).unwrap();
    Fixture {
        data: root.path().join("data"),
        _root: root,
        repo,
        agent,
    }
}

fn successful_script() -> &'static str {
    "#!/bin/sh\nprintf '{\"type\":\"thread.started\",\"thread_id\":\"01a1b721-1f91-7393-ae1d-6aace8e4646a\"}\\n'\nprintf '{\"type\":\"turn.started\"}\\n'\nprintf '{\"type\":\"item.completed\",\"item\":{\"id\":\"item_0\",\"type\":\"agent_message\",\"text\":\"edited sample\"}}\\n'\nprintf '{\"type\":\"turn.completed\"}\\n'\nprintf 'candidate\\n' > sample.txt\n"
}

fn run_success(fixture: &Fixture, task_id: &str) -> Output {
    let output = Command::new(CLI)
        .arg("--data-dir")
        .arg(&fixture.data)
        .arg("run")
        .arg("--repo")
        .arg(&fixture.repo)
        .arg("--task-id")
        .arg(task_id)
        .arg("--objective")
        .arg("replace sample.txt with candidate")
        .arg("--base-ref")
        .arg("main")
        .arg("--agent-program")
        .arg(&fixture.agent)
        .output()
        .expect("run CLI");
    assert!(
        output.status.success(),
        "run failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn run_cli(fixture: &Fixture, command: &str, task_id: &str) -> Output {
    Command::new(CLI)
        .arg("--data-dir")
        .arg(&fixture.data)
        .arg(command)
        .arg(task_id)
        .output()
        .expect("run CLI command")
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
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
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn task_worktree(fixture: &Fixture, task_id: &str) -> PathBuf {
    fixture.data.join("worktrees").join(task_id)
}

#[test]
fn run_preserves_primary_checkout_and_records_candidate() {
    let fixture = fixture(successful_script());
    let base_head = git(&fixture.repo, &["rev-parse", "HEAD"]);
    let base_status = git(&fixture.repo, &["status", "--porcelain"]);
    let output = run_success(&fixture, "run-preserves-primary");
    assert!(String::from_utf8_lossy(&output.stdout).contains("status: Verifying"));
    assert_eq!(git(&fixture.repo, &["rev-parse", "HEAD"]), base_head);
    assert_eq!(git(&fixture.repo, &["status", "--porcelain"]), base_status);
    assert_eq!(
        fs::read_to_string(fixture.repo.join("sample.txt")).unwrap(),
        "base\n"
    );
    assert_eq!(
        fs::read_to_string(task_worktree(&fixture, "run-preserves-primary").join("sample.txt"))
            .unwrap(),
        "candidate\n"
    );
    let logs = fixture.data.join("cli/run-preserves-primary");
    let jsonl = fs::read_to_string(logs.join("codex.jsonl")).unwrap();
    assert!(jsonl.contains("\"type\":\"turn.completed\""));
}

#[test]
fn run_rejects_data_directory_inside_primary_checkout_before_writing() {
    let fixture = fixture(successful_script());
    let data_dir = fixture.repo.join(".agent-orch-data");
    let head = git(&fixture.repo, &["rev-parse", "HEAD"]);
    let status = git(&fixture.repo, &["status", "--porcelain"]);
    let output = Command::new(CLI)
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("run")
        .arg("--repo")
        .arg(&fixture.repo)
        .arg("--task-id")
        .arg("unsafe-data-dir")
        .arg("--objective")
        .arg("edit sample")
        .arg("--agent-program")
        .arg(&fixture.agent)
        .output()
        .expect("run CLI with unsafe data directory");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("refusing to write under primary checkout"));
    assert!(!data_dir.exists());
    assert_eq!(git(&fixture.repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&fixture.repo, &["status", "--porcelain"]), status);
}

#[test]
fn tasks_lists_state_from_store() {
    let fixture = fixture(successful_script());
    run_success(&fixture, "listed-task");
    let output = Command::new(CLI)
        .arg("--data-dir")
        .arg(&fixture.data)
        .arg("tasks")
        .output()
        .expect("list tasks");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("listed-task"));
    assert!(stdout.contains("Verifying"));
}

#[test]
fn inspect_shows_attempt_session_and_worktree() {
    let fixture = fixture(successful_script());
    run_success(&fixture, "inspect-task");
    let output = run_cli(&fixture, "inspect", "inspect-task");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("status: Verifying"));
    assert!(stdout.contains("CandidateReady"));
    assert!(stdout.contains("Exited"));
    assert!(stdout.contains(task_worktree(&fixture, "inspect-task").to_str().unwrap()));
}

#[test]
fn diff_includes_candidate_changes_from_base_commit() {
    let fixture = fixture(successful_script());
    run_success(&fixture, "diff-task");
    let output = run_cli(&fixture, "diff", "diff-task");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("diff --git a/sample.txt b/sample.txt"));
    assert!(stdout.contains("+candidate"));
}

#[test]
fn diff_includes_untracked_candidate_files() {
    let fixture = fixture(successful_script());
    run_success(&fixture, "untracked-diff");
    let worktree = task_worktree(&fixture, "untracked-diff");
    fs::write(worktree.join("new.txt"), "untracked candidate\n").unwrap();

    let output = run_cli(&fixture, "diff", "untracked-diff");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("untracked candidate"));
    assert!(stdout.contains("new.txt"));
}

#[test]
fn cleanup_preserves_dirty_and_unlanded_worktrees() {
    let fixture = fixture(successful_script());
    run_success(&fixture, "cleanup-guard");
    let worktree = task_worktree(&fixture, "cleanup-guard");

    let unlanded = run_cli(&fixture, "cleanup", "cleanup-guard");
    assert!(!unlanded.status.success());
    assert!(String::from_utf8_lossy(&unlanded.stderr).contains("not reachable"));
    assert!(worktree.exists());

    fs::write(worktree.join("sample.txt"), "dirty\n").unwrap();
    let dirty = run_cli(&fixture, "cleanup", "cleanup-guard");
    assert!(!dirty.status.success());
    assert!(String::from_utf8_lossy(&dirty.stderr).contains("is dirty"));
    assert!(worktree.exists());
}

#[test]
fn cleanup_removes_only_clean_landed_worktree() {
    let fixture = fixture(successful_script());
    run_success(&fixture, "cleanup-landed");
    let worktree = task_worktree(&fixture, "cleanup-landed");
    let branch = git(&worktree, &["branch", "--show-current"]);
    git(&fixture.repo, &["merge", "--ff-only", &branch]);

    let output = run_cli(&fixture, "cleanup", "cleanup-landed");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!worktree.exists());
}

#[test]
fn cancel_reaps_owned_child_and_records_terminal_state() {
    let script = format!(
        "#!/bin/sh\nprintf '{{\"type\":\"thread.started\",\"thread_id\":\"{THREAD_ID}\"}}\\n'\nprintf '{{\"type\":\"turn.started\"}}\\n'\nexec sleep 60\n"
    );
    let fixture = fixture(&script);
    let mut run = Command::new(CLI)
        .arg("--data-dir")
        .arg(&fixture.data)
        .arg("run")
        .arg("--repo")
        .arg(&fixture.repo)
        .arg("--task-id")
        .arg("cancel-task")
        .arg("--objective")
        .arg("wait until cancelled")
        .arg("--base-ref")
        .arg("main")
        .arg("--agent-program")
        .arg(&fixture.agent)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start CLI run");

    let metadata_path = fixture.data.join("cli/cancel-task/metadata.json");
    let agent_pid = wait_for_agent_pid(&mut run, &metadata_path);
    let cancel = run_cli(&fixture, "cancel", "cancel-task");
    assert!(
        cancel.status.success(),
        "{}",
        String::from_utf8_lossy(&cancel.stderr)
    );
    let output = wait_for_exit(run);
    assert_eq!(output.status.code(), Some(130));
    assert!(
        !is_process_running(agent_pid),
        "owned child must be reaped before run exits"
    );
    assert!(task_worktree(&fixture, "cancel-task").exists());

    let inspect = run_cli(&fixture, "inspect", "cancel-task");
    assert!(inspect.status.success());
    let stdout = String::from_utf8_lossy(&inspect.stdout);
    assert!(stdout.contains("status: Cancelled"));
    assert!(stdout.contains("Cancelled"));
    assert!(stdout.contains("Exited"));
}

#[test]
fn command_help_exposes_all_workflows() {
    let output = Command::new(CLI)
        .arg("--help")
        .output()
        .expect("run CLI help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for name in [
        "run",
        "tasks",
        "inspect",
        "cancel",
        "diff",
        "cleanup",
        "--data-dir",
    ] {
        assert!(stdout.contains(name), "missing {name} in help: {stdout}");
    }
}

fn wait_for_agent_pid(run: &mut Child, metadata_path: &Path) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(bytes) = fs::read(metadata_path) {
            if let Ok(metadata) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if let Some(pid) = metadata
                    .get("agent_pid")
                    .and_then(serde_json::Value::as_u64)
                {
                    return pid as u32;
                }
            }
        }
        if let Some(status) = run.try_wait().expect("poll CLI run") {
            panic!("CLI run exited before agent launch with {status}");
        }
        assert!(
            Instant::now() < deadline,
            "agent PID was not recorded before timeout"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn wait_for_exit(mut run: Child) -> Output {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = run.try_wait().expect("poll CLI run") {
            assert!(
                status.code().is_some(),
                "CLI runner must exit normally after cancellation"
            );
            return run.wait_with_output().expect("collect CLI output");
        }
        assert!(Instant::now() < deadline, "cancelled CLI run did not exit");
        thread::sleep(Duration::from_millis(20));
    }
}
