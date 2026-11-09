//! Executor behavior against real git repositories and real child processes.
//! The fake agents are shell scripts, so these tests run on Unix only and are
//! not Windows evidence.

use super::*;
use crate::api::DaemonApi;
use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::Project;
use orchestrator_ipc::{RpcError, RpcHandler};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::time::Instant;
use tempfile::TempDir;

const THREAD_STARTED: &str = r#"printf '{"type":"thread.started","thread_id":"01a1b721-1f91-7393-ae1d-6aace8e4646a"}\n'
printf '{"type":"turn.started"}\n'
"#;
const TURN_COMPLETED: &str = "printf '{\"type\":\"turn.completed\"}\\n'\n";

struct Fixture {
    root: TempDir,
    repo: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        let repo = fs::canonicalize(root.path()).unwrap().join("repo");
        fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init", "--initial-branch=main"]);
        run_git(&repo, &["config", "user.name", "executor test"]);
        run_git(&repo, &["config", "user.email", "executor@example.invalid"]);
        fs::write(repo.join("sample.txt"), "base\n").unwrap();
        run_git(&repo, &["add", "sample.txt"]);
        run_git(&repo, &["commit", "-m", "initial"]);
        Self { root, repo }
    }

    fn data(&self) -> PathBuf {
        let data = self.root.path().join("data");
        fs::create_dir_all(&data).unwrap();
        data
    }

    fn project_id(&self) -> String {
        self.repo.display().to_string()
    }

    fn register(&self, store: &SqliteStore) {
        // Registering twice (a second daemon over the same data) is harmless.
        let _ = store.register_project(&Project {
            id: self.project_id(),
            root_path: self.project_id(),
            settings: json!({}),
        });
    }

    fn script(&self, name: &str, body: &str) -> PathBuf {
        let path = self.root.path().join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn marker(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    fn config(&self, data: &Path, program: &Path) -> ExecutorConfig {
        ExecutorConfig {
            worktrees_root: data.join("worktrees"),
            codex_program: Some(program.to_owned()),
            claude_program: None,
            limits: SchedulerLimits::default(),
            poll: Duration::from_millis(25),
        }
    }

    fn api(&self, program: &Path) -> DaemonApi {
        self.api_with(program, |_| {})
    }

    fn api_with(&self, program: &Path, adjust: impl FnOnce(&mut ExecutorConfig)) -> DaemonApi {
        let data = self.data();
        self.register(&SqliteStore::new(&data.join("store.sqlite")).unwrap());
        let mut config = self.config(&data, program);
        adjust(&mut config);
        DaemonApi::with_executor(&data, config).unwrap()
    }

    fn submit(&self, api: &DaemonApi, key: &str, max_retries: u32) -> String {
        self.submit_full(api, key, "codex", max_retries, &[])
    }

    fn submit_full(
        &self,
        api: &DaemonApi,
        key: &str,
        agent: &str,
        max_retries: u32,
        dependencies: &[&str],
    ) -> String {
        let task = api
            .dispatch(
                "tasks.submit",
                json!({
                    "idempotency_key": key,
                    "contract": {
                        "project_id": self.project_id(),
                        "parent_id": null,
                        "objective": format!("objective of {key}"),
                        "kind": "Implement",
                        "agent": agent,
                        "dependencies": dependencies,
                        "verification": "None",
                        "delivery": "ReportOnly",
                        "autonomy": "WorkspaceWrites",
                        "max_retries": max_retries,
                        "max_repairs": 0
                    }
                }),
            )
            .unwrap();
        task["contract"]["id"].as_str().unwrap().to_owned()
    }
}

fn run_git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn task(api: &DaemonApi, task_id: &str) -> Value {
    api.dispatch("tasks.get", json!({"task_id": task_id}))
        .unwrap()
}

fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(20));
    }
}

fn wait_state(api: &DaemonApi, task_id: &str, state: &str) -> Value {
    wait_until(&format!("task {task_id} to be {state}"), || {
        task(api, task_id)["state"] == state
    });
    task(api, task_id)
}

fn succeeding_agent(fixture: &Fixture) -> PathBuf {
    fixture.script(
        "ok-agent",
        &format!("{THREAD_STARTED}echo produced > agent-output.txt\n{TURN_COMPLETED}"),
    )
}

fn waiting_agent(fixture: &Fixture, release: &Path) -> PathBuf {
    fixture.script(
        "waiting-agent",
        &format!(
            "{THREAD_STARTED}while [ ! -f '{}' ]; do sleep 0.05; done\n{TURN_COMPLETED}",
            release.display()
        ),
    )
}

#[test]
fn submitted_task_runs_to_candidate_ready_with_every_transition_journaled() {
    let fixture = Fixture::new();
    let api = fixture.api(&succeeding_agent(&fixture));
    let task_id = fixture.submit(&api, "run-1", 0);

    let done = wait_state(&api, &task_id, "Verifying");
    assert_eq!(done["attempts"][0]["state"], "CandidateReady");
    assert_eq!(done["sessions"][0]["state"], "Exited");
    let store = SqliteStore::new(&fixture.data().join("store.sqlite")).unwrap();
    let workspaces: Vec<_> = store
        .list_workspaces()
        .unwrap()
        .into_iter()
        .filter(|workspace| workspace.task_id == task_id)
        .collect();
    assert_eq!(workspaces.len(), 1);
    let worktree = PathBuf::from(&workspaces[0].path);
    // The candidate is a commit on the task branch, not an uncommitted file.
    let log = Command::new("git")
        .arg("-C")
        .arg(&worktree)
        .args(["show", "--stat", "--format=%s", "HEAD"])
        .output()
        .unwrap();
    let log = String::from_utf8(log.stdout).unwrap();
    assert!(log.contains("agent-orch candidate"), "{log}");
    assert!(log.contains("agent-output.txt"), "{log}");
    // The primary checkout is untouched.
    assert!(!fixture.repo.join("agent-output.txt").exists());

    // Every state change is already a durable, streamable journal event.
    let kinds: Vec<String> = store
        .stream_events_after(0, 200)
        .unwrap()
        .into_iter()
        .filter(|event| event.task_id == task_id)
        .map(|event| event.kind)
        .collect();
    let transitions = kinds
        .iter()
        .filter(|kind| kind.as_str() == "task.transition")
        .count();
    // Admit, ExecutionStarted, CandidateReady.
    assert_eq!(transitions, 3, "{kinds:?}");
    for expected in ["task.created", "workspace.added"] {
        assert!(
            kinds.iter().any(|kind| kind == expected),
            "{expected}: {kinds:?}"
        );
    }
    assert!(
        kinds
            .iter()
            .filter(|kind| kind.starts_with("attempt."))
            .count()
            >= 3,
        "attempt created, running and candidate-ready: {kinds:?}"
    );
    assert!(
        kinds
            .iter()
            .filter(|kind| kind.starts_with("session."))
            .count()
            >= 3,
        "session created, running and exited: {kinds:?}"
    );
    api.shutdown();
}

#[test]
fn failed_agent_fails_the_task_and_retry_appends_a_second_attempt() {
    let fixture = Fixture::new();
    let first_ran = fixture.marker("first-ran");
    let agent = fixture.script(
        "flaky-agent",
        &format!(
            "{THREAD_STARTED}if [ -f '{0}' ]; then {TURN_COMPLETED} exit 0; fi\ntouch '{0}'\nexit 3\n",
            first_ran.display()
        ),
    );
    let api = fixture.api(&agent);
    let task_id = fixture.submit(&api, "flaky-1", 1);

    let failed = wait_state(&api, &task_id, "Failed");
    assert_eq!(failed["attempts"].as_array().unwrap().len(), 1);
    assert_eq!(failed["attempts"][0]["state"], "Failed");

    let retry = json!({"task_id": task_id, "idempotency_key": "retry-1"});
    let queued = api.dispatch("tasks.retry", retry.clone()).unwrap();
    assert!(queued["state"] == "Queued" || queued["state"] == "Running");
    let done = wait_state(&api, &task_id, "Verifying");
    let attempts = done["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 2, "the failed attempt is never erased");
    assert_eq!(attempts[0]["state"], "Failed");
    assert_eq!(attempts[1]["state"], "CandidateReady");
    assert_eq!(attempts[1]["attempt_number"], 2);

    // Replaying the same key returns the recorded answer and starts nothing.
    assert_eq!(api.dispatch("tasks.retry", retry).unwrap(), queued);
    assert_eq!(
        task(&api, &task_id)["attempts"].as_array().unwrap().len(),
        2
    );
    api.shutdown();
}

#[test]
fn retry_is_bounded_by_the_contract_and_only_for_failed_tasks() {
    let fixture = Fixture::new();
    let agent = fixture.script("failing-agent", &format!("{THREAD_STARTED}exit 3\n"));
    let api = fixture.api(&agent);
    let task_id = fixture.submit(&api, "bounded-1", 1);
    wait_state(&api, &task_id, "Failed");

    api.dispatch(
        "tasks.retry",
        json!({"task_id": task_id, "idempotency_key": "retry-a"}),
    )
    .unwrap();
    wait_until("second attempt to fail", || {
        let record = task(&api, &task_id);
        record["state"] == "Failed" && record["attempts"].as_array().unwrap().len() == 2
    });
    // max_retries is 1 and two attempts exist: the budget is spent.
    assert_eq!(
        api.dispatch(
            "tasks.retry",
            json!({"task_id": task_id, "idempotency_key": "retry-b"})
        ),
        Err(RpcError::Conflict)
    );
    assert_eq!(
        task(&api, &task_id)["attempts"].as_array().unwrap().len(),
        2
    );

    // A task that has not failed (it waits on the failed one) cannot be retried.
    let second = fixture.submit_full(&api, "not-failed-1", "codex", 5, &[&task_id]);
    assert_eq!(task(&api, &second)["state"], "Queued");
    assert_eq!(
        api.dispatch(
            "tasks.retry",
            json!({"task_id": second, "idempotency_key": "retry-c"})
        ),
        Err(RpcError::Conflict)
    );
    assert_eq!(
        api.dispatch(
            "tasks.retry",
            json!({"task_id": "missing", "idempotency_key": "retry-d"})
        ),
        Err(RpcError::NotFound)
    );
    assert_eq!(
        api.dispatch("tasks.retry", json!({"task_id": second})),
        Err(RpcError::InvalidParams)
    );
    api.shutdown();
}

#[test]
fn scheduler_limits_hold_and_a_finished_task_releases_its_slot() {
    let fixture = Fixture::new();
    let release = fixture.marker("release");
    let api = fixture.api_with(&waiting_agent(&fixture, &release), |config| {
        config.limits = SchedulerLimits::new(Some(1), Some(1)).unwrap();
    });
    let first = fixture.submit(&api, "limit-1", 0);
    let second = fixture.submit(&api, "limit-2", 0);

    wait_state(&api, &first, "Running");
    // The second task would fit in the queue but never exceeds the limit.
    thread::sleep(Duration::from_millis(400));
    assert_eq!(task(&api, &second)["state"], "Queued");
    let running = [&first, &second]
        .into_iter()
        .filter(|id| task(&api, id)["state"] == "Running")
        .count();
    assert_eq!(running, 1);

    fs::write(&release, "go").unwrap();
    wait_state(&api, &first, "Verifying");
    wait_state(&api, &second, "Verifying");
    api.shutdown();
}

#[test]
fn cancelling_a_running_task_stops_the_agent_through_its_owner() {
    let fixture = Fixture::new();
    let pid_file = fixture.marker("agent.pid");
    let agent = fixture.script(
        "sleeping-agent",
        &format!(
            "{THREAD_STARTED}echo $$ > '{}'\nexec sleep 60\n",
            pid_file.display()
        ),
    );
    let api = fixture.api(&agent);
    let task_id = fixture.submit(&api, "cancel-1", 0);
    wait_state(&api, &task_id, "Running");
    wait_until("agent to record its pid", || pid_file.exists());
    let pid: u32 = fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    wait_until("cancellation owner to register", || {
        api.dispatch(
            "tasks.cancel",
            json!({"task_id": task_id, "idempotency_key": "cancel-key"}),
        )
        .is_ok()
    });
    wait_state(&api, &task_id, "Cancelled");
    wait_until("agent process to exit", || {
        !orchestrator_runtime::is_process_running(pid)
    });
    api.shutdown();
}

#[test]
fn unlaunchable_task_fails_with_a_recorded_attempt_instead_of_hanging() {
    let fixture = Fixture::new();
    let api = fixture.api(&succeeding_agent(&fixture));
    let unlaunchable = fixture.submit_full(&api, "pi-1", "pi", 0, &[]);
    let failed = wait_state(&api, &unlaunchable, "Failed");
    assert_eq!(failed["attempts"][0]["state"], "Failed");
    api.shutdown();
}

#[test]
fn agents_capabilities_reports_installed_state_without_paths() {
    let fixture = Fixture::new();
    let api = fixture.api(&succeeding_agent(&fixture));
    let report = api.dispatch("agents.capabilities", Value::Null).unwrap();
    let report = report.as_array().unwrap();
    let find = |name: &str| report.iter().find(|entry| entry["agent"] == name).unwrap();
    let codex = find("codex");
    assert_eq!(codex["installed"], true);
    assert_eq!(codex["launchable"], true);
    assert_eq!(codex["capabilities"]["streaming"], true);
    assert_eq!(codex["capabilities"]["cancellation"], true);
    assert_eq!(codex["capabilities"]["interactive_input"], false);
    assert_eq!(find("pi")["launchable"], false);
    let text = serde_json::to_string(report).unwrap();
    assert!(
        !text.contains(&fixture.root.path().display().to_string()),
        "capabilities must not expose local paths: {text}"
    );
    assert_eq!(
        api.dispatch("agents.capabilities", json!({"x": 1})),
        Err(RpcError::InvalidParams)
    );
    api.shutdown();

    let absent = fixture.api_with(&fixture.marker("no-such-codex"), |_| {});
    let report = absent.dispatch("agents.capabilities", Value::Null).unwrap();
    let codex = report
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["agent"] == "codex")
        .unwrap();
    assert_eq!(codex["installed"], false);
    assert_eq!(codex["capabilities"], Value::Null);
    absent.shutdown();
}

fn queued_contract(fixture: &Fixture, id: &str) -> TaskContract {
    TaskContract {
        id: id.into(),
        project_id: fixture.project_id(),
        parent_id: None,
        objective: format!("objective of {id}"),
        kind: TaskKind::Implement,
        agent: "codex".into(),
        dependencies: Vec::new(),
        verification: VerificationPolicy::None,
        delivery: DeliveryPolicy::ReportOnly,
        autonomy: AutonomyPolicy::WorkspaceWrites,
        max_retries: 0,
        max_repairs: 0,
    }
}

/// Leaves `task_id` Running with a Running session, as a daemon that died
/// mid-run would. `identity` is the recorded agent process, if any.
fn seed_interrupted_run(store: &SqliteStore, fixture: &Fixture, task_id: &str, pid: Option<u32>) {
    store.create(&queued_contract(fixture, task_id)).unwrap();
    store.transition(task_id, TaskEvent::Admit).unwrap();
    let worktrees = fixture.data().join("worktrees");
    fs::create_dir_all(&worktrees).unwrap();
    let worktree = create_task_worktree(
        &Repository::new(&fixture.repo).unwrap(),
        task_id,
        "HEAD",
        &fs::canonicalize(worktrees).unwrap(),
    )
    .unwrap();
    store
        .add_workspace(&Workspace {
            id: format!("{task_id}-workspace-1"),
            task_id: task_id.into(),
            path: worktree.path().display().to_string(),
            branch: worktree.branch().to_string(),
            base_commit: worktree.base_sha().to_string(),
        })
        .unwrap();
    let attempt = format!("{task_id}-attempt-1");
    let session = format!("{task_id}-session-1");
    store
        .add_attempt(
            task_id,
            &Attempt {
                id: attempt.clone(),
                task_id: task_id.into(),
                attempt_number: 1,
                state: AttemptState::Pending,
            },
        )
        .unwrap();
    store
        .transition(task_id, TaskEvent::ExecutionStarted)
        .unwrap();
    store
        .update_attempt_state(task_id, &attempt, AttemptState::Running)
        .unwrap();
    store
        .add_session(
            task_id,
            &Session {
                id: session.clone(),
                attempt_id: attempt,
                adapter: "codex".into(),
                state: SessionState::Spawning,
            },
        )
        .unwrap();
    if let Some(pid) = pid {
        let identity = capture_process_identity(pid).unwrap();
        store
            .record_session_identity(task_id, &session, &identity)
            .unwrap();
    }
    store
        .update_session_state(task_id, &session, SessionState::Running)
        .unwrap();
}

#[test]
fn restart_terminates_an_orphaned_agent_then_fails_its_task_and_resumes_scheduling() {
    let fixture = Fixture::new();
    let data = fixture.data();
    let store = Arc::new(SqliteStore::new(&data.join("store.sqlite")).unwrap());
    fixture.register(&store);

    let mut orphan = Command::new("/bin/sleep")
        .arg("300")
        .process_group(0)
        .spawn()
        .unwrap();
    let orphan_pid = orphan.id();
    let (exited_tx, exited_rx) = std::sync::mpsc::channel();
    let reaper = thread::spawn(move || exited_tx.send(orphan.wait().unwrap()).unwrap());
    seed_interrupted_run(&store, &fixture, "orphaned", Some(orphan_pid));
    store.create(&queued_contract(&fixture, "waiting")).unwrap();
    store.transition("waiting", TaskEvent::Admit).unwrap();

    let executor = Executor::start(
        store.clone(),
        fixture.config(&data, &succeeding_agent(&fixture)),
    )
    .unwrap();

    let status = exited_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("orphaned agent was not terminated");
    assert_eq!(status.signal(), Some(libc::SIGTERM));
    reaper.join().unwrap();
    wait_until("interrupted task to be failed", || {
        store.get("orphaned").unwrap().state == TaskState::Failed
    });
    let orphaned = store.get("orphaned").unwrap();
    assert_eq!(orphaned.attempts[0].state, AttemptState::Failed);
    assert_eq!(orphaned.sessions[0].state, SessionState::Exited);
    // Scheduling resumed only after the gate passed.
    wait_until("queued task to run", || {
        store.get("waiting").unwrap().state == TaskState::Verifying
    });
    assert_eq!(executor.blocked_reason(), None);
    executor.shutdown();
}

#[test]
fn restart_fails_a_run_whose_agent_already_exited() {
    let fixture = Fixture::new();
    let data = fixture.data();
    let store = Arc::new(SqliteStore::new(&data.join("store.sqlite")).unwrap());
    fixture.register(&store);
    let mut gone = Command::new("/bin/sleep")
        .arg("300")
        .process_group(0)
        .spawn()
        .unwrap();
    let identity = capture_process_identity(gone.id()).unwrap();
    gone.kill().unwrap();
    gone.wait().unwrap();
    seed_interrupted_run(&store, &fixture, "dead-agent", None);
    store
        .record_session_identity("dead-agent", "dead-agent-session-1", &identity)
        .unwrap();

    let executor = Executor::start(
        store.clone(),
        fixture.config(&data, &succeeding_agent(&fixture)),
    )
    .unwrap();
    wait_until("dead agent's task to fail", || {
        store.get("dead-agent").unwrap().state == TaskState::Failed
    });
    assert_eq!(
        store.get("dead-agent").unwrap().attempts[0].state,
        AttemptState::Failed
    );
    executor.shutdown();
}

#[test]
fn restart_gate_blocks_launches_while_a_run_cannot_be_proven_dead() {
    let fixture = Fixture::new();
    let data = fixture.data();
    let store = Arc::new(SqliteStore::new(&data.join("store.sqlite")).unwrap());
    fixture.register(&store);
    // Running with no recorded process identity: nothing can prove it exited.
    seed_interrupted_run(&store, &fixture, "unknown", None);
    store
        .create(&queued_contract(&fixture, "held-back"))
        .unwrap();
    store.transition("held-back", TaskEvent::Admit).unwrap();

    let executor = Executor::start(
        store.clone(),
        fixture.config(&data, &succeeding_agent(&fixture)),
    )
    .unwrap();
    wait_until("gate to report a blocker", || {
        executor.blocked_reason().is_some()
    });
    thread::sleep(Duration::from_millis(300));
    assert_eq!(store.get("held-back").unwrap().state, TaskState::Queued);
    assert_eq!(store.get("unknown").unwrap().state, TaskState::Running);
    executor.shutdown();
}
