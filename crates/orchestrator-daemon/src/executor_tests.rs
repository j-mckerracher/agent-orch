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
            pi_program: None,
            limits: SchedulerLimits::default(),
            poll: Duration::from_millis(25),
            probe_timeout: Duration::from_secs(5),
            diagnostics: orchestrator_diagnostics::Diagnostics::memory(),
            catalog_timeout: Duration::from_secs(5),
            catalog_wait: Duration::from_secs(5),
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
fn usage_reported_by_the_agent_is_recorded_and_silence_stays_unknown() {
    let fixture = Fixture::new();
    let usage_turn = r#"printf '{"type":"turn.completed","usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":50,"reasoning_output_tokens":20}}\n'
"#;
    let reporting = fixture.script("usage-agent", &format!("{THREAD_STARTED}{usage_turn}"));
    let api = fixture.api(&reporting);
    let task_id = fixture.submit(&api, "usage-1", 0);
    wait_state(&api, &task_id, "Verifying");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let params = json!({
        "timezone": "UTC",
        "utc_offset_seconds": 0,
        "window_start_unix_ms": now - 3_600_000,
        "window_end_unix_ms": now + 3_600_000,
    });
    let summary = api.dispatch("usage.summary", params.clone()).unwrap();
    let codex = &summary["providers"][0];
    assert_eq!(codex["provider"], "codex");
    assert_eq!(codex["reported"]["input_tokens"], 600);
    assert_eq!(codex["reported"]["cache_read_tokens"], 400);
    assert_eq!(codex["reported"]["output_tokens"], 50);
    assert_eq!(codex["cost_status"], "unknown");
    api.shutdown();

    let quiet_fixture = Fixture::new();
    let api = quiet_fixture.api(&succeeding_agent(&quiet_fixture));
    let task_id = quiet_fixture.submit(&api, "usage-2", 0);
    wait_state(&api, &task_id, "Verifying");
    let summary = api.dispatch("usage.summary", params).unwrap();
    let codex = &summary["providers"][0];
    assert_eq!(codex["sessions"], 1);
    assert_eq!(codex["token_status"], "unknown");
    assert!(codex["reported"]["input_tokens"].is_null());
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
        model_selection: None,
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

fn events(diag: &orchestrator_diagnostics::Diagnostics, name: &str) -> Vec<Value> {
    diag.lines()
        .iter()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|line| line["event"] == name)
        .collect()
}

#[test]
fn failed_agent_is_reported_with_its_exit_and_the_preserved_worktree() {
    let fixture = Fixture::new();
    let agent = fixture.script(
        "partial-agent",
        &format!("{THREAD_STARTED}echo half > partial.txt\nexit 3\n"),
    );
    let diag = Diagnostics::memory();
    let api = fixture.api_with(&agent, |config| config.diagnostics = diag.clone());
    let task_id = fixture.submit(&api, "partial-1", 0);
    wait_state(&api, &task_id, "Failed");
    wait_until("failure event", || {
        !events(&diag, "task_attempt_failed").is_empty()
    });

    let exited = &events(&diag, "agent_exited")[0];
    assert_eq!(exited["task_id"], task_id.as_str());
    assert_eq!(exited["outcome"], "failed");
    assert_eq!(exited["exit_code"], "3");
    let failed = &events(&diag, "task_attempt_failed")[0];
    assert_eq!(failed["records_complete"], "true");
    assert_eq!(failed["uncommitted_paths"], "1");
    let worktree = PathBuf::from(failed["worktree"].as_str().unwrap());
    assert_eq!(
        fs::read_to_string(worktree.join("partial.txt")).unwrap(),
        "half\n",
        "a failed run must keep what the agent wrote"
    );
    api.shutdown();
}

#[test]
fn launch_failure_is_reported_without_leaking_the_credential_in_the_program_path() {
    let fixture = Fixture::new();
    let diag = Diagnostics::memory();
    let missing = fixture
        .root
        .path()
        .join("token=SECRETVALUE0123456789")
        .join("codex");
    let api = fixture.api_with(&missing, |config| config.diagnostics = diag.clone());
    let task_id = fixture.submit(&api, "missing-1", 0);
    wait_state(&api, &task_id, "Failed");
    wait_until("launch failure event", || {
        !events(&diag, "agent_launch_failed").is_empty()
    });

    let launch = &events(&diag, "agent_launch_failed")[0];
    assert_eq!(launch["task_id"], task_id.as_str());
    assert_eq!(launch["agent"], "codex");
    assert!(launch["error"].as_str().unwrap().contains("[REDACTED]"));
    for line in diag.lines() {
        assert!(!line.contains("SECRETVALUE"), "credential leaked: {line}");
    }
    api.shutdown();
}

#[test]
fn rpc_failures_are_reported_by_method_and_class_but_never_with_parameters() {
    let fixture = Fixture::new();
    let diag = Diagnostics::memory();
    let api = fixture.api_with(&succeeding_agent(&fixture), |config| {
        config.diagnostics = diag.clone()
    });
    let waiting = fixture.submit_full(&api, "dep-parent", "pi", 0, &[]);
    let second = fixture.submit_full(&api, "dep-child", "codex", 5, &[&waiting]);
    assert_eq!(
        api.dispatch(
            "tasks.retry",
            json!({"task_id": second, "idempotency_key": "retry-hunter2hunter2"})
        ),
        Err(RpcError::Conflict)
    );
    assert_eq!(
        api.dispatch("tasks.retry", json!({"task_id": "missing", "idempotency_key": "k"})),
        Err(RpcError::NotFound)
    );

    let failed = events(&diag, "rpc_failed");
    assert_eq!(failed.len(), 1, "caller mistakes such as NotFound stay quiet");
    assert_eq!(failed[0]["method"], "tasks.retry");
    assert_eq!(failed[0]["level"], "warn");
    assert_eq!(failed[0]["error"], "Conflict");
    assert!(!diag.lines().join("\n").contains("hunter2"));
    api.shutdown();
}

#[test]
fn restart_reports_the_orphan_it_stopped_and_the_runs_it_interrupted() {
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
    let reaper = thread::spawn(move || orphan.wait().unwrap());
    seed_interrupted_run(&store, &fixture, "orphaned", Some(orphan_pid));

    let config = fixture.config(&data, &succeeding_agent(&fixture));
    let diag = config.diagnostics.clone();
    let executor = Executor::start(store.clone(), config).unwrap();
    wait_until("gate to open", || {
        !events(&diag, "recovery_gate_open").is_empty()
    });
    reaper.join().unwrap();

    let stopped = &events(&diag, "orphan_terminated")[0];
    assert_eq!(stopped["task_id"], "orphaned");
    assert_eq!(stopped["pid"], orphan_pid.to_string().as_str());
    let opened = &events(&diag, "recovery_gate_open")[0];
    assert_eq!(opened["interrupted_task_ids"], "orphaned");
    executor.shutdown();
}

#[test]
fn restart_gate_refusal_is_reported_once_with_its_blocker() {
    let fixture = Fixture::new();
    let data = fixture.data();
    let store = Arc::new(SqliteStore::new(&data.join("store.sqlite")).unwrap());
    fixture.register(&store);
    seed_interrupted_run(&store, &fixture, "unknown", None);

    let config = fixture.config(&data, &succeeding_agent(&fixture));
    let diag = config.diagnostics.clone();
    let executor = Executor::start(store.clone(), config).unwrap();
    wait_until("gate refusal event", || {
        !events(&diag, "recovery_gate_blocked").is_empty()
    });
    thread::sleep(Duration::from_millis(300));
    executor.shutdown();

    let blocked = events(&diag, "recovery_gate_blocked");
    assert_eq!(blocked.len(), 1, "an unchanged refusal is not repeated");
    assert!(blocked[0]["reason"].as_str().unwrap().contains("unknown"));
    assert_eq!(
        events(&diag, "shutdown_with_running_agents")[0]["task_ids"],
        "unknown"
    );
}

fn spawn_group_leader() -> std::process::Child {
    Command::new("/bin/sleep")
        .arg("300")
        .process_group(0)
        .spawn()
        .unwrap()
}

#[test]
fn restart_never_signals_an_agent_whose_owner_daemon_is_still_alive() {
    let fixture = Fixture::new();
    let data = fixture.data();
    let store = SqliteStore::new(&data.join("store.sqlite")).unwrap();
    fixture.register(&store);
    let mut agent = spawn_group_leader();
    seed_interrupted_run(&store, &fixture, "owned", Some(agent.id()));
    // This test process stands in for a live owning daemon.
    let owner = CancellationOwner::new(
        "owned".into(),
        "owned-attempt-1".into(),
        "owned-session-1".into(),
        uuid::Uuid::new_v4().simple().to_string(),
    )
    .unwrap()
    .with_exact_process_identity(capture_exact_process_identity(std::process::id()).unwrap());
    store.register_cancellation_owner(&owner).unwrap();

    terminate_orphaned_agents(&store, &Diagnostics::memory());

    assert!(
        agent.try_wait().unwrap().is_none(),
        "an agent with a live owner must not be signalled"
    );
    agent.kill().unwrap();
    agent.wait().unwrap();
}

#[test]
fn restart_never_signals_a_process_whose_identity_differs_from_the_recorded_one() {
    let fixture = Fixture::new();
    let data = fixture.data();
    let store = SqliteStore::new(&data.join("store.sqlite")).unwrap();
    fixture.register(&store);
    // The live process now at the recorded pid started at a different time,
    // as after PID reuse. It is a group leader, so only the identity check
    // stands between it and SIGTERM.
    let mut reused = spawn_group_leader();
    let mut recorded = capture_process_identity(reused.id()).unwrap();
    recorded.start_time_micros = Some(
        recorded
            .start_time_micros
            .map_or(1, |started| started + 3_600_000_000),
    );
    seed_interrupted_run(&store, &fixture, "reused", None);
    store
        .record_session_identity("reused", "reused-session-1", &recorded)
        .unwrap();

    terminate_orphaned_agents(&store, &Diagnostics::memory());

    assert!(
        reused.try_wait().unwrap().is_none(),
        "a process that does not match the recorded identity must not be signalled"
    );
    reused.kill().unwrap();
    reused.wait().unwrap();
}

#[test]
fn capabilities_probe_is_bounded_and_kills_a_wedged_agent_by_pid() {
    let fixture = Fixture::new();
    let pid_file = fixture.marker("wedged.pids");
    // Wedges on the first probe command and leaves a grandchild behind.
    let pi = fixture.script(
        "wedged-pi",
        &format!(
            "echo $$ > '{0}'\nsleep 300 &\necho $! >> '{0}'\nwait\n",
            pid_file.display()
        ),
    );
    let api = fixture.api_with(&succeeding_agent(&fixture), |config| {
        config.pi_program = Some(pi.clone());
        // Generous enough that the script has started even on a loaded host.
        config.probe_timeout = Duration::from_secs(2);
    });

    // A script written moments ago can fail to exec with ETXTBSY while another
    // test thread forks; such a failed spawn is quick and leaves no pid file,
    // so retry until the wedged script has actually run.
    let mut attempts = 0;
    let (agents, elapsed) = loop {
        let started = Instant::now();
        let agents = api.dispatch("agents.capabilities", json!({})).unwrap();
        let elapsed = started.elapsed();
        attempts += 1;
        if pid_file.exists() || attempts == 20 {
            break (agents, elapsed);
        }
        thread::sleep(Duration::from_millis(50));
    };

    assert!(elapsed < Duration::from_secs(10), "probe took {elapsed:?}");
    let pi_entry = agents
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["agent"] == "pi")
        .unwrap();
    assert_eq!(pi_entry["installed"], false);
    assert_eq!(pi_entry["launchable"], false);
    let pids = fs::read_to_string(&pid_file).unwrap();
    for pid in pids.split_whitespace() {
        let pid: i32 = pid.parse().unwrap();
        wait_until("wedged probe process to be killed", || unsafe {
            libc::kill(pid, 0) != 0
        });
    }
}

fn queue_snapshot(api: &DaemonApi) -> Value {
    api.dispatch("queue.snapshot", json!({})).unwrap()
}

fn queue_ids(entries: &Value) -> Vec<String> {
    entries
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["task_id"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn queue_snapshot_reports_a_closed_restart_gate_from_the_running_executor() {
    let fixture = Fixture::new();
    let data = fixture.data();
    {
        let store = SqliteStore::new(&data.join("store.sqlite")).unwrap();
        fixture.register(&store);
        // Running with no recorded process identity: the gate cannot open.
        seed_interrupted_run(&store, &fixture, "unknown", None);
        store
            .create(&queued_contract(&fixture, "held-back"))
            .unwrap();
        store.transition("held-back", TaskEvent::Admit).unwrap();
    }
    let api = DaemonApi::with_executor(&data, fixture.config(&data, &succeeding_agent(&fixture)))
        .unwrap();

    wait_until("snapshot to report the closed gate", || {
        queue_snapshot(&api)["dispatch_blocked"] == true
    });

    let result = queue_snapshot(&api);
    assert_eq!(queue_ids(&result["running"]), ["unknown"]);
    let held_back = &result["queued"][0];
    assert_eq!(held_back["task_id"], "held-back");
    assert_eq!(held_back["waiting_reason"], "other");
    assert_eq!(held_back["waiting_detail"], "dispatch_blocked");
    api.shutdown();
}

#[test]
fn queue_snapshot_orders_the_queue_from_the_executors_fairness_cursor() {
    let fixture = Fixture::new();
    let data = fixture.data();
    let release = fixture.marker("release");
    {
        let store = SqliteStore::new(&data.join("store.sqlite")).unwrap();
        fixture.register(&store);
        // Sorts after the fixture project, which is a filesystem path.
        store
            .register_project(&Project {
                id: "zz-other".into(),
                root_path: "/zz-other".into(),
                settings: json!({}),
            })
            .unwrap();
        for (id, project) in [("p1", None), ("p2", None), ("q1", Some("zz-other"))] {
            let mut contract = queued_contract(&fixture, id);
            if let Some(project) = project {
                contract.project_id = project.into();
            }
            store.create(&contract).unwrap();
            store.transition(id, TaskEvent::Admit).unwrap();
        }
    }
    let mut config = fixture.config(&data, &waiting_agent(&fixture, &release));
    config.limits = SchedulerLimits::new(Some(1), Some(1)).unwrap();
    let api = DaemonApi::with_executor(&data, config).unwrap();

    // The first pass starts p1 and records its project as the last started,
    // so the next pass would serve the other project first.
    wait_until("queue to follow the fairness cursor", || {
        let result = queue_snapshot(&api);
        queue_ids(&result["running"]) == ["p1"] && queue_ids(&result["queued"]) == ["q1", "p2"]
    });

    let result = queue_snapshot(&api);
    assert_eq!(result["dispatch_blocked"], false);
    assert_eq!(result["queued"][0]["position"], 1);
    assert_eq!(result["queued"][0]["waiting_reason"], "global_cap");
    api.shutdown();
    fs::write(&release, "go").unwrap();
    wait_state(&api, "p1", "Verifying");
}

mod usage_paths {
    use super::*;
    use orchestrator_agents::{ReportedCost, UsageReport};
    use std::future::{ready, Future};
    use std::pin::Pin;

    type Next = orchestrator_agents::Result<Option<AgentEvent>>;

    /// A handle whose stream is scripted; usage is handed out once.
    struct ScriptedHandle {
        events: Vec<Next>,
        usage: Vec<UsageReport>,
    }

    impl AgentHandle for ScriptedHandle {
        fn pid(&self) -> u32 {
            0
        }
        fn next_event<'a, 'b>(&'a mut self) -> Pin<Box<dyn Future<Output = Next> + Send + 'b>>
        where
            'a: 'b,
            Self: 'b,
        {
            let next = if self.events.is_empty() {
                Ok(None)
            } else {
                self.events.remove(0)
            };
            Box::pin(ready(next))
        }
        fn take_usage(&mut self) -> Vec<UsageReport> {
            std::mem::take(&mut self.usage)
        }
        fn cancel<'a, 'b>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = orchestrator_agents::Result<()>> + Send + 'b>>
        where
            'a: 'b,
            Self: 'b,
        {
            Box::pin(ready(Ok(())))
        }
        fn wait<'a, 'b>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = orchestrator_agents::Result<AgentExit>> + Send + 'b>>
        where
            'a: 'b,
            Self: 'b,
        {
            Box::pin(ready(Ok(AgentExit::Cancelled {
                code: None,
                last_message: None,
            })))
        }
    }

    fn cost_only_report(occurred_at_ms: Option<u64>) -> UsageReport {
        UsageReport {
            receipt_key: "result".into(),
            model: Some("model-a".into()),
            input_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            output_tokens: None,
            reasoning_tokens: None,
            cost: Some(ReportedCost {
                nanos: 8_638_800,
                currency: "USD".into(),
            }),
            occurred_at_ms,
        }
    }

    fn now_ms() -> u64 {
        unix_now_ms()
    }

    /// A Running task, its registered owner and the store holding both.
    fn running_owner(fixture: &Fixture, task_id: &str) -> (SqliteStore, CancellationOwner) {
        let store = SqliteStore::new(&fixture.data().join("store.sqlite")).unwrap();
        fixture.register(&store);
        seed_interrupted_run(&store, fixture, task_id, None);
        let owner = CancellationOwner::new(
            task_id.into(),
            format!("{task_id}-attempt-1"),
            format!("{task_id}-session-1"),
            uuid::Uuid::new_v4().simple().to_string(),
        )
        .unwrap();
        store.register_cancellation_owner(&owner).unwrap();
        (store, owner)
    }

    fn run(
        handle: ScriptedHandle,
        store: &SqliteStore,
        owner: &CancellationOwner,
    ) -> Result<MonitorOutcome, String> {
        let job = Job {
            task_id: owner.task_id.clone(),
            attempt_id: owner.attempt_id.clone(),
            session_id: owner.session_id.clone(),
            agent: "codex".into(),
            objective: "objective".into(),
            model_selection: None,
            worktree: PathBuf::new(),
            diag: orchestrator_diagnostics::Diagnostics::memory(),
        };
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(monitor_agent(Box::new(handle), store, owner, &job))
    }

    fn assert_cost_recorded_and_tokens_unknown(store: &SqliteStore) {
        let now = now_ms();
        let summary = store
            .usage_window_summary(now - 3_600_000, now + 3_600_000)
            .unwrap();
        let reported = &summary.providers[0].bucket.reported;
        assert_eq!(reported.costs.get("USD"), Some(&8_638_800));
        assert_eq!(
            reported.input_tokens, None,
            "unknown is never recorded as 0"
        );
        assert_eq!(reported.output_tokens, None);
    }

    #[test]
    fn usage_is_recorded_when_the_event_stream_fails() {
        let fixture = Fixture::new();
        let (store, owner) = running_owner(&fixture, "stream-error");
        let handle = ScriptedHandle {
            events: vec![Err(orchestrator_agents::AgentError::Protocol(
                "broken stream".into(),
            ))],
            usage: vec![cost_only_report(None)],
        };
        let outcome = run(handle, &store, &owner).unwrap();
        assert!(outcome.supervision_error.is_some());
        assert_cost_recorded_and_tokens_unknown(&store);
    }

    #[test]
    fn usage_is_recorded_when_the_run_is_cancelled_before_any_event() {
        let fixture = Fixture::new();
        let (store, owner) = running_owner(&fixture, "early-cancel");
        store.request_cancellation_for_task("early-cancel").unwrap();
        let handle = ScriptedHandle {
            events: Vec::new(),
            usage: vec![cost_only_report(None)],
        };
        let outcome = run(handle, &store, &owner).unwrap();
        assert!(outcome.cancellation_succeeded);
        assert_cost_recorded_and_tokens_unknown(&store);
    }

    #[test]
    fn a_silent_cancelled_run_records_nothing() {
        let fixture = Fixture::new();
        let (store, owner) = running_owner(&fixture, "silent-cancel");
        store
            .request_cancellation_for_task("silent-cancel")
            .unwrap();
        let handle = ScriptedHandle {
            events: Vec::new(),
            usage: Vec::new(),
        };
        run(handle, &store, &owner).unwrap();
        let now = now_ms();
        let summary = store
            .usage_window_summary(now - 3_600_000, now + 3_600_000)
            .unwrap();
        assert!(summary.providers[0].bucket.reported.costs.is_empty());
        assert_eq!(summary.providers[0].bucket.sessions_with_cost, 0);
    }

    #[test]
    fn the_provider_timestamp_sets_the_receipt_window_end() {
        let fixture = Fixture::new();
        let (store, owner) = running_owner(&fixture, "provider-time");
        let now = now_ms();
        let started = now - 60_000;
        let occurred = now - 30_000;
        let mut handle = ScriptedHandle {
            events: Vec::new(),
            usage: vec![cost_only_report(Some(occurred))],
        };
        record_usage(&mut handle, &store, &owner, started, None);
        let at_provider_time = store
            .usage_window_summary(occurred, occurred + 1_000)
            .unwrap();
        assert_eq!(
            at_provider_time.totals.reported.costs.get("USD"),
            Some(&8_638_800),
            "attributed to the provider's own time, not the time it was observed"
        );
        let at_observation = store
            .usage_window_summary(now - 1_000, now + 3_600_000)
            .unwrap();
        assert!(at_observation.totals.reported.costs.is_empty());
    }

    fn token_report(model: Option<&str>) -> UsageReport {
        UsageReport {
            receipt_key: "turn.completed:1".into(),
            model: model.map(str::to_owned),
            input_tokens: Some(1_000),
            cache_read_tokens: Some(200),
            cache_write_tokens: Some(0),
            output_tokens: Some(100),
            reasoning_tokens: None,
            cost: None,
            occurred_at_ms: None,
        }
    }

    fn estimated_usd(store: &SqliteStore) -> Option<u128> {
        let now = now_ms();
        store
            .usage_window_summary(now - 3_600_000, now + 3_600_000)
            .unwrap()
            .totals
            .estimated
            .costs
            .get("USD")
            .copied()
    }

    #[test]
    fn the_requested_model_prices_a_report_that_names_none() {
        let fixture = Fixture::new();
        let (store, owner) = running_owner(&fixture, "requested-model");
        let mut handle = ScriptedHandle {
            events: Vec::new(),
            usage: vec![token_report(None)],
        };
        record_usage(
            &mut handle,
            &store,
            &owner,
            now_ms() - 1_000,
            Some("gpt-6-luna"),
        );
        assert_eq!(estimated_usd(&store), Some(152_000));
    }

    #[test]
    fn a_run_without_a_model_stays_unpriced_and_a_reported_model_wins() {
        let fixture = Fixture::new();
        let (store, owner) = running_owner(&fixture, "no-model");
        let mut handle = ScriptedHandle {
            events: Vec::new(),
            usage: vec![token_report(None)],
        };
        record_usage(&mut handle, &store, &owner, now_ms() - 1_000, None);
        assert_eq!(estimated_usd(&store), None);

        let fixture = Fixture::new();
        let (store, owner) = running_owner(&fixture, "reported-wins");
        let mut handle = ScriptedHandle {
            events: Vec::new(),
            usage: vec![token_report(Some("gpt-6-sol"))],
        };
        record_usage(
            &mut handle,
            &store,
            &owner,
            now_ms() - 1_000,
            Some("gpt-6-luna"),
        );
        assert_eq!(estimated_usd(&store), Some(3_040_000));
    }

    #[test]
    fn only_codex_falls_back_to_the_requested_model() {
        let job = |agent: &str, model: Option<&str>| Job {
            task_id: "t".into(),
            attempt_id: "a".into(),
            session_id: "s".into(),
            agent: agent.into(),
            objective: "o".into(),
            model_selection: Some(ModelSelection {
                model: model.map(str::to_owned),
                effort: None,
            }),
            worktree: PathBuf::from("."),
            diag: orchestrator_diagnostics::Diagnostics::memory(),
        };
        assert_eq!(
            requested_codex_model(&job(CODEX_AGENT, Some("gpt-6-luna"))).as_deref(),
            Some("gpt-6-luna")
        );
        assert_eq!(requested_codex_model(&job(CODEX_AGENT, None)), None);
        assert_eq!(
            requested_codex_model(&job(CLAUDE_AGENT, Some("claude-opus-5-5"))),
            None
        );
    }
}

#[path = "api_models_tests.rs"]
mod model_selection;
