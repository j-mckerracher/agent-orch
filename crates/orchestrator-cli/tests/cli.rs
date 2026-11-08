//! Black-box CLI tests use a fresh Git repository and data directory per case.
//! Updated: 2026-11-08T09:04:40+00:00.

#![cfg(unix)]

use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::identity::ProcessIdentity;
#[cfg(target_os = "macos")]
use orchestrator_core::identity::ProcessPlatform;
use orchestrator_core::records::Project;
use orchestrator_core::task::{Attempt, AttemptState, Session, SessionState, TaskEvent, TaskState};
use orchestrator_runtime::reconcile::capture_process_identity;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use orchestrator_runtime::is_process_running;
use orchestrator_store::{DataDirectoryLock, SqliteStore, Store};
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
    String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string()
}

fn seed_running_session(fixture: &Fixture, task_id: &str, identity: Option<ProcessIdentity>) {
    let store = SqliteStore::new(&fixture.data.join("store.sqlite")).unwrap();
    let project_id = fs::canonicalize(&fixture.repo)
        .unwrap()
        .display()
        .to_string();
    store
        .register_project(&Project {
            id: project_id.clone(),
            root_path: project_id.clone(),
            settings: serde_json::json!({}),
        })
        .unwrap();
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

fn task_worktree(fixture: &Fixture, task_id: &str) -> PathBuf {
    fixture.data.join("worktrees").join(task_id)
}

#[test]
fn run_preserves_primary_checkout_and_records_candidate() {
    let fixture = fixture(successful_script());
    let tracked = fixture.repo.join("sample.txt");
    let untracked = fixture.repo.join("primary-untracked.txt");
    fs::write(&tracked, b"primary tracked bytes\n").unwrap();
    fs::write(&untracked, b"primary untracked bytes\n").unwrap();
    let base_head = git(&fixture.repo, &["rev-parse", "HEAD"]);
    let base_status = git(&fixture.repo, &["status", "--porcelain"]);
    let base_tracked = fs::read(&tracked).unwrap();
    let base_untracked = fs::read(&untracked).unwrap();
    let output = run_success(&fixture, "run-preserves-primary");
    assert!(String::from_utf8_lossy(&output.stdout).contains("status: Verifying"));
    assert_eq!(git(&fixture.repo, &["rev-parse", "HEAD"]), base_head);
    assert_eq!(git(&fixture.repo, &["status", "--porcelain"]), base_status);
    assert_eq!(fs::read(&tracked).unwrap(), base_tracked);
    assert_eq!(fs::read(&untracked).unwrap(), base_untracked);
    assert_eq!(
        fs::read(task_worktree(&fixture, "run-preserves-primary").join("sample.txt")).unwrap(),
        b"candidate\n"
    );
    let logs = fixture.data.join("cli/run-preserves-primary");
    let jsonl = fs::read_to_string(logs.join("codex.jsonl")).unwrap();
    assert!(jsonl.contains("\"type\":\"turn.completed\""));
}

#[cfg(target_os = "macos")]
#[test]
fn run_persists_session_identity_after_sqlite_reopen() {
    let fixture = fixture(successful_script());
    run_success(&fixture, "identity-task");

    let reopened = SqliteStore::new(&fixture.data.join("store.sqlite")).unwrap();
    let task = reopened.get("identity-task").unwrap();
    let session = &task.sessions[0];
    let identity = reopened
        .session_identity("identity-task", &session.id)
        .unwrap()
        .expect("launch identity persisted");
    assert_eq!(identity.platform, ProcessPlatform::MacOS);
    assert!(identity.process_group.is_some());
    assert!(identity.start_time_micros.is_some());
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
fn run_refuses_a_held_data_directory_lock_with_holder_pid() {
    let fixture = fixture(successful_script());
    let _lock = DataDirectoryLock::acquire(&fixture.data).unwrap();
    let output = Command::new(CLI)
        .arg("--data-dir")
        .arg(&fixture.data)
        .arg("run")
        .arg("--repo")
        .arg(&fixture.repo)
        .arg("--task-id")
        .arg("locked-run")
        .arg("--objective")
        .arg("do not start")
        .arg("--agent-program")
        .arg(&fixture.agent)
        .output()
        .expect("run CLI against held data directory");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("holder PID {}", std::process::id())),
        "{stderr}"
    );
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
fn tasks_and_inspect_read_while_another_process_holds_the_data_lock() {
    let fixture = fixture(successful_script());
    run_success(&fixture, "readonly-while-locked");
    let _lock = DataDirectoryLock::acquire(&fixture.data).unwrap();

    let tasks = Command::new(CLI)
        .arg("--data-dir")
        .arg(&fixture.data)
        .arg("tasks")
        .output()
        .expect("list tasks while lock is held");
    assert!(
        tasks.status.success(),
        "{}",
        String::from_utf8_lossy(&tasks.stderr)
    );
    assert!(String::from_utf8_lossy(&tasks.stdout).contains("readonly-while-locked"));

    let inspect = run_cli(&fixture, "inspect", "readonly-while-locked");
    assert!(
        inspect.status.success(),
        "{}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    assert!(String::from_utf8_lossy(&inspect.stdout).contains("status: Verifying"));
}

#[test]
fn run_reconciles_an_exited_session_before_starting_new_task() {
    let fixture = fixture(successful_script());
    let mut child = Command::new("sleep")
        .arg("1")
        .process_group(0)
        .spawn()
        .expect("start short-lived child");
    let identity = capture_process_identity(child.id()).unwrap();
    seed_running_session(&fixture, "previous-task", Some(identity));
    assert!(child.wait().unwrap().success());

    run_success(&fixture, "after-interruption");
    let store = SqliteStore::new(&fixture.data.join("store.sqlite")).unwrap();
    let previous = store.get("previous-task").unwrap();
    assert_eq!(previous.state, TaskState::Failed);
    assert_eq!(previous.attempts[0].state, AttemptState::Failed);
    assert_eq!(previous.sessions[0].state, SessionState::Exited);
    assert_ne!(previous.state, TaskState::Succeeded);
}

#[test]
fn run_refuses_an_identity_mismatch_and_names_the_blocking_session() {
    let fixture = fixture(successful_script());
    let mut child = Command::new("sleep")
        .arg("60")
        .process_group(0)
        .spawn()
        .expect("start process with a stable identity");
    let mut identity = capture_process_identity(child.id()).unwrap();
    if let Some(start_time) = identity.start_time_micros {
        identity.start_time_micros = Some(start_time + 1);
    } else {
        identity.process_group = identity.process_group.map(|group| group + 1);
    }
    seed_running_session(&fixture, "ambiguous-task", Some(identity));

    let output = Command::new(CLI)
        .arg("--data-dir")
        .arg(&fixture.data)
        .arg("run")
        .arg("--repo")
        .arg(&fixture.repo)
        .arg("--task-id")
        .arg("blocked-task")
        .arg("--objective")
        .arg("must not start")
        .arg("--agent-program")
        .arg(&fixture.agent)
        .output()
        .expect("run CLI with ambiguous prior session");
    let _ = child.kill();
    let _ = child.wait();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("session ambiguous-task/ambiguous-task-session-1 needs review"),
        "{stderr}"
    );
    let store = SqliteStore::new(&fixture.data.join("store.sqlite")).unwrap();
    assert!(store.get("blocked-task").is_err());
    assert_eq!(
        store.get("ambiguous-task").unwrap().state,
        TaskState::Running
    );
}

#[test]
fn run_refuses_a_session_without_process_identity() {
    let fixture = fixture(successful_script());
    seed_running_session(&fixture, "unidentified-task", None);
    let output = Command::new(CLI)
        .arg("--data-dir")
        .arg(&fixture.data)
        .arg("run")
        .arg("--repo")
        .arg(&fixture.repo)
        .arg("--task-id")
        .arg("blocked-task")
        .arg("--objective")
        .arg("must not start")
        .arg("--agent-program")
        .arg(&fixture.agent)
        .output()
        .expect("run CLI with identity-less prior session");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("session unidentified-task/unidentified-task-session-1 has no recorded process identity"),
        "{stderr}"
    );
    let store = SqliteStore::new(&fixture.data.join("store.sqlite")).unwrap();
    assert!(store.get("blocked-task").is_err());
}

#[test]
fn run_refuses_an_ambiguous_running_task_without_an_unfinished_session() {
    let fixture = fixture(successful_script());
    seed_running_session(&fixture, "orphan-task", None);
    let store = SqliteStore::new(&fixture.data.join("store.sqlite")).unwrap();
    store
        .update_session_state("orphan-task", "orphan-task-session-1", SessionState::Exited)
        .unwrap();
    drop(store);

    let output = Command::new(CLI)
        .arg("--data-dir")
        .arg(&fixture.data)
        .arg("run")
        .arg("--repo")
        .arg(&fixture.repo)
        .arg("--task-id")
        .arg("blocked-task")
        .arg("--objective")
        .arg("must not start")
        .arg("--agent-program")
        .arg(&fixture.agent)
        .output()
        .expect("run CLI with ambiguous task state");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("task orphan-task is Running without an unfinished session"),
        "{stderr}"
    );
    let store = SqliteStore::new(&fixture.data.join("store.sqlite")).unwrap();
    assert!(store.get("blocked-task").is_err());
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
    let unlanded_head = git(&worktree, &["rev-parse", "HEAD"]);
    let unlanded_status = git(&worktree, &["status", "--porcelain"]);
    let unlanded_bytes = fs::read(worktree.join("sample.txt")).unwrap();
    let unlanded = run_cli(&fixture, "cleanup", "cleanup-guard");
    assert!(!unlanded.status.success());
    assert!(String::from_utf8_lossy(&unlanded.stderr).contains("not reachable"));
    assert!(worktree.exists());
    assert_eq!(git(&worktree, &["rev-parse", "HEAD"]), unlanded_head);
    assert_eq!(git(&worktree, &["status", "--porcelain"]), unlanded_status);
    assert_eq!(
        fs::read(worktree.join("sample.txt")).unwrap(),
        unlanded_bytes
    );

    let tracked = worktree.join("sample.txt");
    let untracked = worktree.join("untracked.txt");
    fs::write(&tracked, b"dirty tracked bytes\n").unwrap();
    fs::write(&untracked, b"dirty untracked bytes\n").unwrap();
    let dirty_head = git(&worktree, &["rev-parse", "HEAD"]);
    let dirty_status = git(&worktree, &["status", "--porcelain"]);
    let dirty_tracked = fs::read(&tracked).unwrap();
    let dirty_untracked = fs::read(&untracked).unwrap();
    let dirty = run_cli(&fixture, "cleanup", "cleanup-guard");
    assert!(!dirty.status.success());
    assert!(String::from_utf8_lossy(&dirty.stderr).contains("is dirty"));
    assert!(worktree.exists());
    assert_eq!(git(&worktree, &["rev-parse", "HEAD"]), dirty_head);
    assert_eq!(git(&worktree, &["status", "--porcelain"]), dirty_status);
    assert_eq!(fs::read(&tracked).unwrap(), dirty_tracked);
    assert_eq!(fs::read(&untracked).unwrap(), dirty_untracked);
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
#[cfg(target_os = "macos")]
fn cancel_refuses_missing_run_owner_without_signalling_or_changing_state() {
    let fixture = fixture(successful_script());
    let mut child = Command::new("sleep")
        .arg("60")
        .process_group(0)
        .spawn()
        .expect("start process with a stable identity");
    let mut identity = capture_process_identity(child.id()).unwrap();
    identity.start_time_micros = Some(identity.start_time_micros.unwrap() + 1);
    let pid = identity.pid;
    seed_running_session(&fixture, "mismatch-cancel", Some(identity));

    let output = run_cli(&fixture, "cancel", "mismatch-cancel");
    let remained_alive = is_process_running(pid);
    let _ = child.kill();
    let _ = child.wait();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no active cancellation owner"), "{stderr}");
    assert!(
        remained_alive,
        "cancel signalled a process without its run owner"
    );
    let store = SqliteStore::new(&fixture.data.join("store.sqlite")).unwrap();
    let task = store.get("mismatch-cancel").unwrap();
    assert_eq!(task.state, TaskState::Running);
    assert_eq!(task.attempts[0].state, AttemptState::Running);
    assert_eq!(task.sessions[0].state, SessionState::Running);
}

#[test]
#[cfg(target_os = "macos")]
fn reconciliation_does_not_promote_legacy_cancelled_attempt_without_owner_ack() {
    let fixture = fixture(successful_script());
    let mut child = Command::new("sleep")
        .arg("60")
        .process_group(0)
        .spawn()
        .expect("start process for a dead-session identity");
    let identity = capture_process_identity(child.id()).unwrap();
    let _ = child.kill();
    let _ = child.wait();
    seed_running_session(&fixture, "legacy-cancel", Some(identity));
    let store = SqliteStore::new(&fixture.data.join("store.sqlite")).unwrap();
    store
        .update_attempt_state(
            "legacy-cancel",
            "legacy-cancel-attempt-1",
            AttemptState::Cancelled,
        )
        .unwrap();

    let output = Command::new(CLI)
        .arg("--data-dir")
        .arg(&fixture.data)
        .arg("run")
        .arg("--repo")
        .arg(&fixture.repo)
        .arg("--task-id")
        .arg("blocked-task")
        .arg("--objective")
        .arg("must not start")
        .arg("--agent-program")
        .arg(&fixture.agent)
        .output()
        .expect("run CLI against legacy cancellation state");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cancelled attempt without run-owner confirmation"),
        "{stderr}"
    );
    let task = store.get("legacy-cancel").unwrap();
    assert_eq!(task.state, TaskState::Running);
    assert_eq!(task.attempts[0].state, AttemptState::Cancelled);
    assert_eq!(task.sessions[0].state, SessionState::Exited);
    assert!(store.get("blocked-task").is_err());
}

#[cfg(target_os = "macos")]
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
    let store = SqliteStore::new(&fixture.data.join("store.sqlite")).unwrap();
    let task = store.get("cancel-task").unwrap();
    let identity = store
        .session_identity("cancel-task", &task.sessions[0].id)
        .unwrap()
        .expect("cancelled session identity");
    assert_eq!(identity.pid, agent_pid);
    assert!(identity.process_group.is_some());
    assert!(identity.start_time_micros.is_some());

    let inspect = run_cli(&fixture, "inspect", "cancel-task");
    assert!(inspect.status.success());
    let stdout = String::from_utf8_lossy(&inspect.stdout);
    assert!(stdout.contains("status: Cancelled"));
    assert!(stdout.contains("cancellation request: Confirmed"));
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
