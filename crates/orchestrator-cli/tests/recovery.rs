#![cfg(unix)]

use orchestrator_core::task::{AttemptState, AuditEvent, SessionState, TaskState};
use orchestrator_store::{CancellationStatus, SqliteStore, Store, StoreError};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;
#[link(name = "c")]
unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

const CLI: &str = env!("CARGO_BIN_EXE_orchestrator-cli");
const WAIT_LIMIT: Duration = Duration::from_secs(20);
const BARRIER_AGENT: &str = r##"#!/bin/sh
set -eu
printf '%s\n' '{"type":"thread.started","thread_id":"01a1b721-1f91-7393-ae1d-6aace8e4646a"}'
printf '%s\n' '{"type":"turn.started"}'
printf '%s' "$M2_TASK_CONTENT" > "$M2_TASK_FILE"
: > "$M2_READY_MARKER"
count=0
while [ ! -f "$M2_BARRIER_FILE" ]; do
    count=$((count + 1))
    if [ "$count" -ge 1000 ]; then exit 71; fi
    sleep 0.02
done
printf '%s\n' '{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"finished"}}'
printf '%s\n' '{"type":"turn.completed"}'
"##;

struct Fixture {
    _root: TempDir,
    repo: PathBuf,
    data: PathBuf,
    agent: PathBuf,
}

impl Fixture {
    fn new(agent_script: &str) -> Self {
        let root = tempfile::tempdir().expect("create unique fixture directory");
        let repo = root.path().join("repo");
        fs::create_dir_all(&repo).expect("create repository");
        git(&repo, &["init", "--initial-branch=main"]);
        git(&repo, &["config", "user.name", "recovery test"]);
        git(&repo, &["config", "user.email", "recovery@example.invalid"]);
        fs::write(repo.join("base.txt"), b"base\n").expect("write base file");
        git(&repo, &["add", "base.txt"]);
        git(&repo, &["commit", "-m", "initial"]);
        let agent = root.path().join("fake-codex");
        fs::write(&agent, agent_script).expect("write fake agent");
        let mut permissions = fs::metadata(&agent).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&agent, permissions).unwrap();
        Self {
            data: root.path().join("data"),
            _root: root,
            repo,
            agent,
        }
    }
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

fn run_command(fixture: &Fixture, task_id: &str, objective: &str) -> Command {
    let mut command = Command::new(CLI);
    command
        .arg("--data-dir")
        .arg(&fixture.data)
        .arg("run")
        .arg("--repo")
        .arg(&fixture.repo)
        .arg("--task-id")
        .arg(task_id)
        .arg("--objective")
        .arg(objective)
        .arg("--base-ref")
        .arg("main")
        .arg("--agent-program")
        .arg(&fixture.agent)
        // The supervisor forwards only an allowlisted environment to agents.
        .env(
            "AGENT_ORCH_ENV_PASSTHROUGH",
            "M2_TASK_FILE,M2_TASK_CONTENT,M2_READY_MARKER,M2_BARRIER_FILE,M2_EXIT_CODE",
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    command
}

fn cancel_command(fixture: &Fixture, task_id: &str) -> Command {
    let mut command = Command::new(CLI);
    command
        .arg("--data-dir")
        .arg(&fixture.data)
        .arg("cancel")
        .arg(task_id)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    command
}

struct ChildGuard {
    child: Option<Child>,
    process_group: Option<u32>,
}

impl ChildGuard {
    fn spawn(mut command: Command) -> Self {
        Self {
            child: Some(command.spawn().expect("spawn CLI run")),
            process_group: None,
        }
    }

    fn wait(&mut self) -> Result<Output, String> {
        let deadline = Instant::now() + WAIT_LIMIT;
        loop {
            let child = self.child.as_mut().expect("child still owned");
            if child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_some()
            {
                return self
                    .child
                    .take()
                    .expect("completed child still owned")
                    .wait_with_output()
                    .map_err(|error| error.to_string());
            }
            if Instant::now() >= deadline {
                return Err("CLI run exceeded bounded wait".to_string());
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(group) = self.process_group {
            // SAFETY: This group ID comes from this fixture's persisted agent identity.
            unsafe {
                kill(-(group as i32), 9);
            }
        }
        if let Some(mut child) = self.child.take() {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

fn wait_for_path(path: &Path) {
    let deadline = Instant::now() + WAIT_LIMIT;
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "path did not appear: {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "CLI failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(feature = "fault-injection")]
fn run_bounded(command: Command) -> Output {
    ChildGuard::spawn(command)
        .wait()
        .expect("CLI run finishes before deadline")
}

fn assert_audit_complete(store: &SqliteStore) {
    let journal = store.event_journal(0).expect("read event journal");
    for (index, entry) in journal.iter().enumerate() {
        assert_eq!(entry.sequence, i64::try_from(index + 1).unwrap());
    }
    for task in store.list().expect("list journaled tasks") {
        let task_events = journal
            .iter()
            .filter(|entry| entry.task_id == task.contract.id)
            .map(|entry| entry.event)
            .collect::<Vec<_>>();
        assert_eq!(task.events, task_events, "task {}", task.contract.id);
    }
}

#[test]
fn two_parallel_tasks_preserve_disjoint_worktrees_for_five_runs() {
    for iteration in 1..=5 {
        let fixture = Fixture::new(BARRIER_AGENT);
        let barrier = fixture._root.path().join("release");
        let left_ready = fixture._root.path().join("left-ready");
        let right_ready = fixture._root.path().join("right-ready");
        let mut left_command = run_command(&fixture, "left-task", "write left file");
        left_command
            .env("M2_TASK_CONTENT", "left bytes\n")
            .env("M2_TASK_FILE", "left.txt")
            .env("M2_READY_MARKER", &left_ready)
            .env("M2_BARRIER_FILE", &barrier);
        let mut left = ChildGuard::spawn(left_command);
        wait_for_path(&left_ready);

        let mut right_command = run_command(&fixture, "right-task", "write right file");
        right_command
            .env("M2_TASK_CONTENT", "right bytes\n")
            .env("M2_TASK_FILE", "right.txt")
            .env("M2_READY_MARKER", &right_ready)
            .env("M2_BARRIER_FILE", &barrier);
        let mut right = ChildGuard::spawn(right_command);
        wait_for_path(&right_ready);
        fs::write(&barrier, b"release\n").expect("release both agents");

        assert_success(&left.wait().expect("left CLI exits"));
        assert_success(&right.wait().expect("right CLI exits"));
        let store = SqliteStore::new(&fixture.data.join("store.sqlite")).unwrap();
        for (task_id, file, bytes) in [
            ("left-task", "left.txt", b"left bytes\n".as_slice()),
            ("right-task", "right.txt", b"right bytes\n".as_slice()),
        ] {
            let record = store.get(task_id).unwrap();
            assert_eq!(record.state, TaskState::Verifying);
            assert_eq!(record.attempts[0].state, AttemptState::CandidateReady);
            assert_eq!(record.sessions[0].state, SessionState::Exited);
            assert_eq!(
                fs::read(fixture.data.join("worktrees").join(task_id).join(file)).unwrap(),
                bytes
            );
        }
        assert_audit_complete(&store);
        println!("concurrent repeat {iteration}/5: both tasks Verifying; disjoint files preserved");
    }
}

#[cfg(feature = "fault-injection")]
fn wait_for_process_group_exit(pid: u32) {
    let deadline = Instant::now() + WAIT_LIMIT;
    while orchestrator_runtime::supervisor::process_group_alive(pid) {
        assert!(
            Instant::now() < deadline,
            "owned process group stayed alive"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(feature = "fault-injection")]
fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(feature = "fault-injection")]
#[test]
fn cancel_leaves_a_live_run_owner_active() {
    let fixture = Fixture::new(BARRIER_AGENT);
    let barrier = fixture._root.path().join("live-owner-barrier");
    let ready = fixture._root.path().join("live-owner-ready");
    let mut command = run_command(&fixture, "live-owner-task", "wait for cancellation");
    command
        .env("M2_TASK_CONTENT", "live owner bytes\n")
        .env("M2_TASK_FILE", "live-owner.txt")
        .env("M2_READY_MARKER", &ready)
        .env("M2_BARRIER_FILE", &barrier);
    let mut run = ChildGuard::spawn(command);
    wait_for_path(&ready);

    let store = SqliteStore::new(&fixture.data.join("store.sqlite")).unwrap();
    let record = store.get("live-owner-task").unwrap();
    let identity = store
        .session_identity("live-owner-task", &record.sessions[0].id)
        .unwrap()
        .expect("live agent process identity is recorded");
    run.process_group = identity.process_group;

    let cancel_output = run_bounded(cancel_command(&fixture, "live-owner-task"));
    assert_success(&cancel_output);
    assert!(
        String::from_utf8_lossy(&cancel_output.stdout).contains("cancellation pending"),
        "unexpected cancel output: {}",
        String::from_utf8_lossy(&cancel_output.stdout)
    );
    let run_output = run
        .wait()
        .expect("run owner observes and confirms cancellation");
    assert_eq!(run_output.status.code(), Some(130));
    if let Some(group) = identity.process_group {
        wait_for_process_group_exit(group);
    }
    run.process_group = None;
    assert_eq!(
        store.get("live-owner-task").unwrap().state,
        TaskState::Cancelled
    );
    assert_eq!(
        store
            .cancellation_status_for_task("live-owner-task")
            .unwrap(),
        Some(CancellationStatus::Confirmed)
    );
}

#[cfg(feature = "fault-injection")]
#[test]
fn sigkill_reconciles_dead_task_while_parallel_task_completes() {
    let fixture = Fixture::new(BARRIER_AGENT);
    let crash_ready = fixture._root.path().join("crash-ready");
    let crash_gate = fixture._root.path().join("crash-gate");
    let survivor_ready = fixture._root.path().join("survivor-ready");
    let barrier = fixture._root.path().join("release");

    let mut crash_command = run_command(&fixture, "crashed-task", "write crash file");
    crash_command
        .env("M2_TASK_CONTENT", "uncommitted recovery bytes\n")
        .env("M2_TASK_FILE", "crash-only.txt")
        .env("M2_READY_MARKER", &crash_ready)
        .env("M2_BARRIER_FILE", &barrier)
        .env("AGENT_ORCH_FAULT_INJECTION", "kill-agent-group-and-cli")
        .env("AGENT_ORCH_FAULT_GATE", &crash_gate);
    let mut crash = ChildGuard::spawn(crash_command);
    wait_for_path(&crash_ready);

    let store = SqliteStore::new(&fixture.data.join("store.sqlite")).unwrap();
    assert_eq!(
        store.request_cancellation_for_task("crashed-task").unwrap(),
        CancellationStatus::Pending
    );
    let crashed = store.get("crashed-task").unwrap();
    let identity = store
        .session_identity("crashed-task", &crashed.sessions[0].id)
        .unwrap()
        .expect("process identity was persisted before the fault");
    assert_eq!(identity.process_group, Some(identity.pid));
    crash.process_group = identity.process_group;
    let worktree = fixture.data.join("worktrees/crashed-task");
    let changed_file = worktree.join("crash-only.txt");
    let before = fs::read(&changed_file).expect("agent wrote uncommitted file");
    let before_checksum = checksum(&before);
    assert!(git(&worktree, &["status", "--porcelain"]).contains("crash-only.txt"));

    let mut survivor_command = run_command(&fixture, "survivor-task", "write survivor file");
    survivor_command
        .env("M2_TASK_CONTENT", "survivor bytes\n")
        .env("M2_TASK_FILE", "survivor-only.txt")
        .env("M2_READY_MARKER", &survivor_ready)
        .env("M2_BARRIER_FILE", &barrier);
    let mut survivor = ChildGuard::spawn(survivor_command);
    wait_for_path(&survivor_ready);
    fs::write(&crash_gate, b"kill owned group\n").expect("release process fault");
    let crash_output = crash.wait().expect("faulted CLI exits");
    assert_eq!(
        std::os::unix::process::ExitStatusExt::signal(&crash_output.status),
        Some(9)
    );
    wait_for_process_group_exit(identity.pid);
    crash.process_group = None;

    fs::write(&barrier, b"release survivor\n").expect("release surviving agent");
    assert_success(&survivor.wait().expect("surviving CLI exits"));
    assert_eq!(
        store.get("survivor-task").unwrap().state,
        TaskState::Verifying
    );
    let decisions = orchestrator_runtime::reconcile::reconcile_processes(&[
        orchestrator_runtime::reconcile::RecordedProcess {
            subject: orchestrator_runtime::reconcile::ProcessSubject::Session {
                task_id: "crashed-task".to_string(),
                session_id: crashed.sessions[0].id.clone(),
            },
            identity,
        },
    ]);
    assert_eq!(
        decisions[0].action,
        orchestrator_runtime::reconcile::ProcessAction::MarkInterrupted
    );

    let cancel_output = run_bounded(cancel_command(&fixture, "crashed-task"));
    assert!(!cancel_output.status.success());
    assert!(
        String::from_utf8_lossy(&cancel_output.stderr).contains("no active cancellation owner"),
        "unexpected cancel error: {}",
        String::from_utf8_lossy(&cancel_output.stderr)
    );
    assert_eq!(
        store.cancellation_status_for_task("crashed-task").unwrap(),
        Some(CancellationStatus::Failed)
    );

    let recovery_barrier = fixture._root.path().join("recovery-release");
    let recovery_ready = fixture._root.path().join("recovery-ready");
    fs::write(&recovery_barrier, b"release\n").unwrap();
    let mut recovery_command = run_command(&fixture, "reconcile-task", "write recovery file");
    recovery_command
        .env("M2_TASK_CONTENT", "reconciled bytes\n")
        .env("M2_TASK_FILE", "reconciled.txt")
        .env("M2_READY_MARKER", &recovery_ready)
        .env("M2_BARRIER_FILE", &recovery_barrier);
    assert_success(
        &ChildGuard::spawn(recovery_command)
            .wait()
            .expect("reconciliation run exits"),
    );

    let recovered = store.get("crashed-task").unwrap();
    assert_eq!(recovered.state, TaskState::Failed);
    assert_eq!(recovered.attempts.len(), 1);
    assert_eq!(recovered.attempts[0].state, AttemptState::Failed);
    assert_eq!(recovered.sessions[0].state, SessionState::Exited);
    assert_eq!(
        store.cancellation_status_for_task("crashed-task").unwrap(),
        Some(CancellationStatus::Failed)
    );
    assert!(!recovered.events.contains(&AuditEvent::TaskCancelled));
    assert!(matches!(
        store.request_cancellation_for_task("crashed-task"),
        Err(StoreError::NoActiveCancellationOwner { .. })
    ));
    let after = fs::read(&changed_file).expect("uncommitted file survived reconciliation");
    assert_eq!(checksum(&after), before_checksum);
    assert_eq!(after, before);
    assert!(git(&worktree, &["status", "--porcelain"]).contains("crash-only.txt"));
    assert_audit_complete(&store);
}

#[cfg(feature = "fault-injection")]
#[test]
fn event_insert_failure_rolls_back_transition_then_retry_records_once() {
    let fixture = Fixture::new(BARRIER_AGENT);
    let mut command = run_command(&fixture, "runtime-fault", "injected transition failure");
    command.env("AGENT_ORCH_FAULT_INJECTION", "fail-event-insert");
    let output = run_bounded(command);
    assert!(!output.status.success());

    let store = SqliteStore::new(&fixture.data.join("store.sqlite")).unwrap();
    let failed = store.get("runtime-fault").unwrap();
    assert_eq!(failed.state, TaskState::Created);
    assert!(failed.events.is_empty());
    assert!(store.event_journal(0).unwrap().is_empty());

    let drop_trigger = Command::new("sqlite3")
        .arg("-batch")
        .arg(fixture.data.join("store.sqlite"))
        .arg("DROP TRIGGER agent_orch_m2_05_reject_event;")
        .output()
        .expect("clear injected event-write fault");
    assert!(
        drop_trigger.status.success(),
        "{}",
        String::from_utf8_lossy(&drop_trigger.stderr)
    );
    store
        .transition("runtime-fault", orchestrator_core::task::TaskEvent::Admit)
        .unwrap();
    let retried = store.get("runtime-fault").unwrap();
    assert_eq!(retried.state, TaskState::Queued);
    assert_eq!(retried.events.len(), 1);
    let journal = store.event_journal(0).unwrap();
    assert_eq!(journal.len(), 1);
    assert_eq!(journal[0].sequence, 1);
    assert_eq!(
        journal[0].event,
        orchestrator_core::task::AuditEvent::TaskAdmitted
    );
    assert_audit_complete(&store);
}

#[cfg(feature = "fault-injection")]
#[test]
fn identity_capture_failure_fails_run_without_leaving_running_session() {
    let fixture = Fixture::new(BARRIER_AGENT);
    let mut command = run_command(&fixture, "no-identity", "injected identity failure");
    command.env("AGENT_ORCH_FAULT_INJECTION", "fail-identity-capture");
    let output = run_bounded(command);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("injected identity capture failure"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let store = SqliteStore::new(&fixture.data.join("store.sqlite")).unwrap();
    let failed = store.get("no-identity").unwrap();
    assert_eq!(failed.state, TaskState::Failed);
    assert_eq!(failed.attempts[0].state, AttemptState::Failed);
    assert_eq!(failed.sessions.len(), 1);
    assert_eq!(failed.sessions[0].state, SessionState::Aborted);
    assert!(store.open_cancellation_owners().unwrap().is_empty());
    assert!(
        !fixture.data.join(".orchestrator.lock").exists(),
        "failed run released the data lock"
    );
    assert_audit_complete(&store);
}

#[cfg(feature = "fault-injection")]
#[test]
fn prelaunch_crash_reconciles_before_any_second_attempt() {
    let fixture = Fixture::new(BARRIER_AGENT);
    let mut command = run_command(&fixture, "orphan-task", "crash before launch");
    command.env("AGENT_ORCH_FAULT_INJECTION", "crash-before-agent-launch");
    let output = run_bounded(command);
    assert_eq!(
        std::os::unix::process::ExitStatusExt::signal(&output.status),
        Some(9)
    );

    let store = SqliteStore::new(&fixture.data.join("store.sqlite")).unwrap();
    let orphan = store.get("orphan-task").unwrap();
    assert_eq!(orphan.state, TaskState::Running);
    assert_eq!(orphan.attempts.len(), 1);
    assert_eq!(orphan.attempts[0].state, AttemptState::Running);
    assert_eq!(orphan.sessions.len(), 1);
    assert_eq!(orphan.sessions[0].state, SessionState::Spawning);
    assert!(!orphan
        .events
        .contains(&orchestrator_core::task::AuditEvent::RetryScheduled));

    let recovery = run_bounded(run_command(
        &fixture,
        "must-not-start",
        "must await retry decision",
    ));
    assert!(!recovery.status.success());
    assert!(
        String::from_utf8_lossy(&recovery.stderr).contains("no recorded process identity"),
        "{}",
        String::from_utf8_lossy(&recovery.stderr)
    );
    assert!(
        !fixture.data.join(".orchestrator.lock").exists(),
        "reconciliation run released recovered lock"
    );
    assert_eq!(store.get("orphan-task").unwrap().attempts.len(), 1);
    assert!(store.get("must-not-start").is_err());
    assert_audit_complete(&store);
}

#[cfg(feature = "fault-injection")]
#[test]
fn audit_journal_is_complete_after_concurrent_failure_and_crash_paths() {
    let fixture = Fixture::new(BARRIER_AGENT);
    let barrier = fixture._root.path().join("audit-release");
    fs::write(&barrier, b"release\n").expect("release audit agents");
    for (task_id, file, contents) in [
        ("audit-left", "left.txt", "left bytes\n"),
        ("audit-right", "right.txt", "right bytes\n"),
    ] {
        let ready = fixture._root.path().join(format!("{task_id}-ready"));
        let mut command = run_command(&fixture, task_id, "write audit file");
        command
            .env("M2_TASK_CONTENT", contents)
            .env("M2_TASK_FILE", file)
            .env("M2_READY_MARKER", ready)
            .env("M2_BARRIER_FILE", &barrier);
        assert_success(&run_bounded(command));
    }

    let mut fault_command = run_command(&fixture, "audit-fault", "fail before admit");
    fault_command.env("AGENT_ORCH_FAULT_INJECTION", "fail-event-insert");
    assert!(!run_bounded(fault_command).status.success());
    let store = SqliteStore::new(&fixture.data.join("store.sqlite")).unwrap();
    assert!(store.get("audit-fault").unwrap().events.is_empty());
    assert_eq!(store.event_journal(0).unwrap().len(), 6);
    let clear_fault = Command::new("sqlite3")
        .arg("-batch")
        .arg(fixture.data.join("store.sqlite"))
        .arg("DROP TRIGGER agent_orch_m2_05_reject_event;")
        .output()
        .expect("clear event fault");
    assert!(clear_fault.status.success());
    store
        .transition("audit-fault", orchestrator_core::task::TaskEvent::Admit)
        .unwrap();

    let mut crash_command = run_command(&fixture, "audit-orphan", "crash before launch");
    crash_command.env("AGENT_ORCH_FAULT_INJECTION", "crash-before-agent-launch");
    let crashed = run_bounded(crash_command);
    assert_eq!(
        std::os::unix::process::ExitStatusExt::signal(&crashed.status),
        Some(9)
    );
    let recovery = run_bounded(run_command(
        &fixture,
        "audit-must-not-start",
        "wait for retry decision",
    ));
    assert!(!recovery.status.success());
    assert!(
        String::from_utf8_lossy(&recovery.stderr).contains("no recorded process identity"),
        "{}",
        String::from_utf8_lossy(&recovery.stderr)
    );
    assert_eq!(store.event_journal(0).unwrap().len(), 9);
    assert_audit_complete(&store);
}

#[cfg(feature = "fault-injection")]
#[test]
fn fast_exit_before_identity_preserves_failure_status() {
    let fixture = Fixture::new("#!/bin/sh\nexit 3\n");
    let mut command = run_command(&fixture, "fast-exit", "fast exit");
    command.env(
        "AGENT_ORCH_FAULT_INJECTION",
        "wait-agent-exit-before-identity",
    );
    let output = run_bounded(command);
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("process identity unavailable"));
    let store = SqliteStore::new(&fixture.data.join("store.sqlite")).unwrap();
    let task = store.get("fast-exit").unwrap();
    assert_eq!(task.state, TaskState::Failed);
    assert_eq!(task.attempts[0].state, AttemptState::Failed);
    assert!(store
        .session_identity("fast-exit", "fast-exit-session-1")
        .unwrap()
        .is_none());
    let metadata: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture.data.join("cli/fast-exit/metadata.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(metadata["exit_code"], 3);
    let log = fs::read_to_string(fixture.data.join("cli/fast-exit/session.log")).unwrap();
    assert!(log.contains("identity_skipped_agent_exited"));
}
