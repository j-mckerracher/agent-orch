//! Executor behavior against real git repositories and real child processes.
//! The fake agents are shell scripts, so these tests run on Unix only and are
//! not Windows evidence.

use super::*;
use crate::api::admission::AdmissionConfig;
use crate::api::DaemonApi;
use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::Project;
use orchestrator_host_metrics::{
    Meter, MeterId, MeterKind, MeterState, Snapshot, Thresholds, Unit,
};
use orchestrator_ipc::{RpcError, RpcHandler};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::sync::atomic::AtomicU32;
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
        let help = if body.contains("--help") {
            ""
        } else {
            "if [ \"$1\" = '--help' ] || { [ \"$1\" = 'exec' ] && [ \"$2\" = '--help' ]; }; then\n\
              printf '%s\\n' --json --sandbox --ignore-user-config --skip-git-repo-check --approve-for-me --ephemeral -m -c \
              -p --verbose --output-format --include-partial-messages --permission-mode --permission-prompts --restricted \
              --strict-mcp-config --tools --allowedTools --disallowedTools --allow-dangerously-skip-permissions --model --effort \
              --plugin-dir --agent \
              --prompt --log-level --no-auto-update --no-ask-user --no-custom-instructions --disable-builtin-mcps --disallow-temp-dir \
              --usage-output-file -C --allow-tool --deny-tool --allow-all --assisted-approval --max-ai-credits\n\
              exit 0\nfi\n"
        };
        fs::write(&path, format!("#!/bin/sh\n{help}{body}")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn marker(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    fn config(&self, data: &Path, program: &Path) -> ExecutorConfig {
        ExecutorConfig {
            worktrees_root: data.join("worktrees"),
            data_dir: data.to_owned(),
            codex_program: Some(program.to_owned()),
            claude_program: None,
            pi_program: None,
            copilot_credit_cap: false,
            // Never probe a real copilot from a test: a missing path is a fact.
            copilot_program: Some(self.root.path().join("no-such-copilot")),
            treehouse_program: test_treehouse_program(),
            copilot_sign_in: Default::default(),
            compatibility: Default::default(),
            limits: SchedulerLimits::default(),
            admission: Default::default(),
            poll: Duration::from_millis(25),
            wait_for_exit_before_identity: false,
            probe_timeout: Duration::from_secs(5),
            diagnostics: orchestrator_diagnostics::Diagnostics::memory(),
            catalog_timeout: Duration::from_secs(5),
            catalog_wait: Duration::from_secs(5),
            load: Default::default(),
            load_feed: false,
            locate_workflow_agent: fixture_workflow_agent,
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

/// The workflow agent every launch gets unless a test overrides the seam.
fn fixture_workflow_agent() -> Option<WorkflowAgent> {
    Some(WorkflowAgent {
        plugin_dir: PathBuf::from("/fixture/pstack"),
        codex_instructions: "fixture instructions".to_owned(),
    })
}

fn no_workflow_agent() -> Option<WorkflowAgent> {
    None
}

/// What the fixture workflow agent adds to the Claude and Copilot task argv, after the level's own
/// flags.
const FIXTURE_AGENT_FLAGS: [&str; 4] = [
    "--plugin-dir",
    "/fixture/pstack",
    "--agent",
    "pstack:workflow-agent",
];

fn with_flags<'a>(mut argv: Vec<&'a str>, flags: &[&'a str]) -> Vec<&'a str> {
    argv.extend_from_slice(flags);
    argv
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
            "{THREAD_STARTED}while [ ! -f '{}' ]; do sleep 0.05; done\necho produced > agent-output.txt\n{TURN_COMPLETED}",
            release.display()
        ),
    )
}

#[test]
fn submitted_task_runs_to_succeeded_with_every_transition_journaled() {
    let fixture = Fixture::new();
    let api = fixture.api(&succeeding_agent(&fixture));
    let task_id = fixture.submit(&api, "run-1", 0);

    let done = wait_state(&api, &task_id, "Succeeded");
    assert_eq!(done["attempts"][0]["state"], "Succeeded");
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
    // Admit, ExecutionStarted, CandidateReady, VerificationPassed.
    assert_eq!(transitions, 4, "{kinds:?}");
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
fn a_missing_workflow_agent_warns_on_the_attempt_and_still_launches_plain() {
    let fixture = Fixture::new();
    let mut diagnostics = None;
    let api = fixture.api_with(&succeeding_agent(&fixture), |config| {
        config.locate_workflow_agent = no_workflow_agent;
        diagnostics = Some(config.diagnostics.clone());
    });
    let task_id = fixture.submit(&api, "no-workflow-agent", 0);

    let done = wait_state(&api, &task_id, "Succeeded");
    assert_eq!(done["attempts"][0]["state"], "Succeeded");
    let lines = diagnostics.unwrap().lines().join("\n");
    assert!(
        lines.contains("\"event\":\"workflow_agent_missing\"")
            && lines.contains(&format!("\"task_id\":\"{task_id}\"")),
        "{lines}"
    );
}

#[test]
fn a_claude_cli_without_the_agent_flags_launches_plain_and_warns_naming_the_flag() {
    let fixture = Fixture::new();
    let record = fixture.marker("argv");
    // An older Claude CLI: its help offers none of the agent's flags.
    let program = fixture.script(
        "old-claude",
        &format!(
            "if [ \"$1\" = \"--help\" ]; then\n\
             printf '%s\\n' -p --verbose --output-format --include-partial-messages --permission-mode \\\n\
             --permission-prompts --restricted --strict-mcp-config --tools --allowedTools \\\n\
             --disallowedTools --model --effort\n\
             exit 0\nfi\n\
             printf '%s\\n' \"$@\" > '{0}.tmp' && mv '{0}.tmp' '{0}'\n\
             {THREAD_STARTED}{TURN_COMPLETED}",
            record.display()
        ),
    );
    let mut diagnostics = None;
    let api = fixture.api_with(&program, |config| {
        config.claude_program = Some(program.clone());
        diagnostics = Some(config.diagnostics.clone());
    });
    // The submit-time check measures the plain argv, so the submission is accepted.
    let task_id = fixture.submit_full(&api, "old-claude", "claude-code", 0, &[]);

    wait_until("the agent to record its argv", || record.exists());
    let argv = fs::read_to_string(&record).unwrap();
    assert!(
        !argv.contains("--plugin-dir") && !argv.contains("--agent"),
        "{argv}"
    );
    let lines = diagnostics.unwrap().lines().join("\n");
    assert!(
        lines.contains("\"event\":\"workflow_agent_unsupported\"")
            && lines.contains("--plugin-dir")
            && lines.contains(&format!("\"task_id\":\"{task_id}\"")),
        "{lines}"
    );
}

/// A scripted agent that prints one final message before it finishes, in the event shape the
/// codex adapter reads its `last_message` from.
fn messaging_agent(fixture: &Fixture, message: &str) -> PathBuf {
    let event = format!(
        r#"{{"type":"item.completed","item":{{"id":"item_0","type":"agent_message","text":"{message}"}}}}"#
    );
    fixture.script(
        "messaging-agent",
        &format!("{THREAD_STARTED}printf '%s\\n' '{event}'\n{TURN_COMPLETED}"),
    )
}

/// `kind` of the contract `Fixture::submit_full` builds is fixed to `Implement`; a scout needs
/// the other kind, so this submits the same contract with `Investigate`.
fn submit_kind(fixture: &Fixture, api: &DaemonApi, key: &str, kind: &str) -> String {
    let task = api
        .dispatch(
            "tasks.submit",
            json!({
                "idempotency_key": key,
                "contract": {
                    "project_id": fixture.project_id(),
                    "parent_id": null,
                    "objective": format!("objective of {key}"),
                    "kind": kind,
                    "agent": "codex",
                    "dependencies": [],
                    "verification": "None",
                    "delivery": "ReportOnly",
                    "autonomy": "WorkspaceWrites",
                    "max_retries": 0,
                    "max_repairs": 0
                }
            }),
        )
        .unwrap();
    task["contract"]["id"].as_str().unwrap().to_owned()
}

#[test]
fn a_finished_scout_leaves_its_last_message_as_a_redacted_report_and_a_ship_leaves_none() {
    let fixture = Fixture::new();
    // A credential shape the diagnostics redactor masks; a bare long word is masked only in the
    // copy a relay hands a model (`report::relay_block`), not in the stored report.
    let secret = "SECRETVALUE1234";
    let api = fixture.api(&messaging_agent(
        &fixture,
        &format!("The slow query is in the loader, password={secret}"),
    ));
    let scout = submit_kind(&fixture, &api, "scout-1", "Investigate");
    wait_state(&api, &scout, "Succeeded");
    let store = SqliteStore::new(&fixture.data().join("store.sqlite")).unwrap();
    let report = store
        .task_report(&scout)
        .unwrap()
        .expect("a scout leaves a report");
    assert!(
        report.body.contains("The slow query is in the loader"),
        "{}",
        report.body
    );
    assert!(!report.body.contains(secret), "{}", report.body);
    assert!(!report.truncated && report.relayed_at.is_none());

    // This agent changes nothing, so an implement task ends Failed; it still leaves no report.
    let ship = submit_kind(&fixture, &api, "ship-1", "Implement");
    wait_state(&api, &ship, "Failed");
    assert_eq!(store.task_report(&ship).unwrap(), None);
    api.shutdown();
}

#[test]
fn an_implement_agent_that_changes_nothing_fails_with_the_empty_change_set_reason() {
    let fixture = Fixture::new();
    let api = fixture.api(&messaging_agent(
        &fixture,
        "I read the loader and found nothing to change.",
    ));
    let task_id = fixture.submit(&api, "no-op-1", 0);

    let failed = wait_state(&api, &task_id, "Failed");
    assert_eq!(failed["attempts"][0]["state"], "Failed");
    let store = SqliteStore::new(&fixture.data().join("store.sqlite")).unwrap();
    let attempt = &store.attempts_for_task(&task_id).unwrap()[0];
    assert_eq!(
        attempt.failure_reason.as_deref(),
        Some("The agent exited without changing any files. Last message: I read the loader and found nothing to change.")
    );
    assert_eq!(
        attempt.summary.as_deref(),
        Some("I read the loader and found nothing to change.")
    );
    assert_eq!(attempt.changed_files, Some(0));
    api.shutdown();
}

#[test]
fn an_investigate_agent_that_changes_nothing_still_produces_a_candidate() {
    let fixture = Fixture::new();
    let api = fixture.api(&messaging_agent(&fixture, "The loader is the slow part."));
    let scout = submit_kind(&fixture, &api, "scout-no-change", "Investigate");

    let done = wait_state(&api, &scout, "Succeeded");
    assert_eq!(done["attempts"][0]["state"], "Succeeded");
    let store = SqliteStore::new(&fixture.data().join("store.sqlite")).unwrap();
    let attempt = &store.attempts_for_task(&scout).unwrap()[0];
    assert_eq!(attempt.failure_reason, None);
    assert_eq!(attempt.changed_files, Some(0));
    assert_eq!(
        attempt.summary.as_deref(),
        Some("The loader is the slow part.")
    );
    api.shutdown();
}

#[test]
fn every_agent_exit_failure_stores_a_reason() {
    // The agent exits without ever completing a turn.
    let fixture = Fixture::new();
    let agent = fixture.script("silent-agent", &format!("{THREAD_STARTED}exit 3\n"));
    let api = fixture.api(&agent);
    let task_id = fixture.submit(&api, "silent-1", 0);
    wait_state(&api, &task_id, "Failed");
    let store = SqliteStore::new(&fixture.data().join("store.sqlite")).unwrap();
    let attempt = &store.attempts_for_task(&task_id).unwrap()[0];
    assert_eq!(
        attempt.failure_reason.as_deref(),
        Some("The agent failed: agent exited with code 3.")
    );
    assert_eq!(attempt.summary, None);
    api.shutdown();

    // The agent completes a turn with a message, then exits non-zero: the message is kept.
    let fixture = Fixture::new();
    let event = r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"Stopped halfway."}}"#;
    let agent = fixture.script(
        "stops-after-turn",
        &format!("{THREAD_STARTED}printf '%s\\n' '{event}'\n{TURN_COMPLETED}exit 3\n"),
    );
    let api = fixture.api(&agent);
    let task_id = fixture.submit(&api, "stopped-1", 0);
    wait_state(&api, &task_id, "Failed");
    let store = SqliteStore::new(&fixture.data().join("store.sqlite")).unwrap();
    let attempt = &store.attempts_for_task(&task_id).unwrap()[0];
    // A non-zero exit reaches the store as a protocol failure, which carries the message only
    // inside its reason, so the attempt has no separate summary.
    assert_eq!(
        attempt.failure_reason.as_deref(),
        Some("The agent failed: agent exited with code 3; last message: Stopped halfway.")
    );
    assert_eq!(attempt.summary, None);
    assert_eq!(attempt.changed_files, None);
    api.shutdown();
}

#[test]
fn a_candidate_records_its_changed_file_count_and_last_message() {
    let fixture = Fixture::new();
    let event = r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"Added both files."}}"#;
    let agent = fixture.script(
        "two-file-agent",
        &format!(
            "{THREAD_STARTED}echo one > one.txt\necho two > two.txt\nprintf '%s\\n' '{event}'\n{TURN_COMPLETED}"
        ),
    );
    let api = fixture.api(&agent);
    let task_id = fixture.submit(&api, "two-files-1", 0);

    wait_state(&api, &task_id, "Succeeded");
    let store = SqliteStore::new(&fixture.data().join("store.sqlite")).unwrap();
    let attempt = &store.attempts_for_task(&task_id).unwrap()[0];
    assert_eq!(attempt.changed_files, Some(2));
    assert_eq!(attempt.summary.as_deref(), Some("Added both files."));
    api.shutdown();
}

#[test]
fn usage_reported_by_the_agent_is_recorded_and_silence_stays_unknown() {
    let fixture = Fixture::new();
    let usage_turn = r#"printf '{"type":"turn.completed","usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":50,"reasoning_output_tokens":20}}\n'
"#;
    let reporting = fixture.script(
        "usage-agent",
        &format!("{THREAD_STARTED}echo produced > agent-output.txt\n{usage_turn}"),
    );
    let api = fixture.api(&reporting);
    let task_id = fixture.submit(&api, "usage-1", 0);
    wait_state(&api, &task_id, "Succeeded");
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
    wait_state(&api, &task_id, "Succeeded");
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
            "{THREAD_STARTED}if [ -f '{0}' ]; then echo produced > agent-output.txt; {TURN_COMPLETED} exit 0; fi\ntouch '{0}'\nexit 3\n",
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
    let done = wait_state(&api, &task_id, "Succeeded");
    let attempts = done["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 2, "the failed attempt is never erased");
    assert_eq!(attempts[0]["state"], "Failed");
    assert_eq!(attempts[1]["state"], "Succeeded");
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
    wait_state(&api, &first, "Succeeded");
    wait_state(&api, &second, "Succeeded");
    api.shutdown();
}

#[test]
fn a_closed_load_gate_keeps_a_queued_task_queued_and_leaves_a_running_task_alone() {
    use crate::api::load::tests::close_gate;
    let fixture = Fixture::new();
    let release = fixture.marker("release");
    let load = Arc::new(crate::api::load::LoadHandle::default());
    let shared = load.clone();
    let api = fixture.api_with(&waiting_agent(&fixture, &release), move |config| {
        config.load = shared;
    });
    let running = fixture.submit(&api, "load-running", 0);
    wait_state(&api, &running, "Running");

    close_gate(&load);
    let queued = fixture.submit(&api, "load-queued", 0);
    thread::sleep(Duration::from_millis(300));
    assert_eq!(task(&api, &queued)["state"], "Queued");
    assert_eq!(task(&api, &running)["state"], "Running");

    // The running task finishes as usual; the closed gate still holds the queued one.
    fs::write(&release, "go").unwrap();
    wait_state(&api, &running, "Succeeded");
    thread::sleep(Duration::from_millis(300));
    assert_eq!(task(&api, &queued)["state"], "Queued");
    api.shutdown();
}

#[test]
fn opening_the_load_gate_starts_a_queued_task_without_waiting_for_the_poll() {
    use crate::api::load::tests::{at, close_gate, cpu};
    let fixture = Fixture::new();
    let release = fixture.marker("release");
    let load = Arc::new(crate::api::load::LoadHandle::default());
    let shared = load.clone();
    // The poll is far longer than the wait below, so only the wake can start the task.
    let last = close_gate(&load);
    let api = fixture.api_with(&waiting_agent(&fixture, &release), move |config| {
        config.load = shared;
        config.poll = Duration::from_secs(60);
    });
    let queued = fixture.submit(&api, "load-wake", 0);
    thread::sleep(Duration::from_millis(300));
    assert_eq!(task(&api, &queued)["state"], "Queued");

    for n in 1..=8 {
        load.record(cpu(0.0), at(last, n));
    }

    wait_state(&api, &queued, "Running");
    fs::write(&release, "go").unwrap();
    wait_state(&api, &queued, "Succeeded");
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

/// A task whose candidate must pass `commands` before it succeeds.
fn checked_contract(fixture: &Fixture, id: &str, commands: Vec<Vec<String>>) -> TaskContract {
    TaskContract {
        verification: VerificationPolicy::Commands { commands },
        ..queued_contract(fixture, id)
    }
}

/// The argument vector of a check that runs `script` with `sh -c`.
fn sh_check(script: &str) -> Vec<String> {
    vec!["/bin/sh".into(), "-c".into(), script.into()]
}

/// Stores `contract` and admits it, as the API does, so the executor picks it up.
fn admit(store: &SqliteStore, contract: &TaskContract) {
    store.create(contract).unwrap();
    store.transition(&contract.id, TaskEvent::Admit).unwrap();
}

#[test]
fn candidate_that_passes_its_checks_succeeds_with_a_passed_run() {
    let fixture = Fixture::new();
    let data = fixture.data();
    let store = Arc::new(SqliteStore::new(&data.join("store.sqlite")).unwrap());
    fixture.register(&store);
    let contract = checked_contract(
        &fixture,
        "checked-pass",
        vec![sh_check("test -f agent-output.txt")],
    );
    admit(&store, &contract);
    let executor = Executor::start(
        store.clone(),
        fixture.config(&data, &succeeding_agent(&fixture)),
    )
    .unwrap();

    wait_until("checked task to succeed", || {
        store.get("checked-pass").unwrap().state == TaskState::Succeeded
    });
    let task = store.get("checked-pass").unwrap();
    assert_eq!(task.attempts[0].state, AttemptState::Succeeded);
    let runs = store.verification_runs_for_task("checked-pass").unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].status, "passed");
    executor.shutdown();
}

#[test]
fn candidate_that_fails_its_checks_is_verification_failed_with_the_reason() {
    let fixture = Fixture::new();
    let data = fixture.data();
    let store = Arc::new(SqliteStore::new(&data.join("store.sqlite")).unwrap());
    fixture.register(&store);
    let contract = checked_contract(&fixture, "checked-fail", vec![sh_check("exit 3")]);
    admit(&store, &contract);
    let executor = Executor::start(
        store.clone(),
        fixture.config(&data, &succeeding_agent(&fixture)),
    )
    .unwrap();

    wait_until("checked task to fail verification", || {
        store.get("checked-fail").unwrap().state == TaskState::VerificationFailed
    });
    let task = store.get("checked-fail").unwrap();
    assert_eq!(task.attempts[0].state, AttemptState::Failed);
    let reason = task.attempts[0].failure_reason.as_deref().unwrap();
    assert!(!reason.is_empty());
    let runs = store.verification_runs_for_task("checked-fail").unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].status, "failed");
    executor.shutdown();
}

#[test]
fn cancelling_a_slow_verification_kills_its_check_and_cancels_the_task() {
    let fixture = Fixture::new();
    let data = fixture.data();
    let store = Arc::new(SqliteStore::new(&data.join("store.sqlite")).unwrap());
    fixture.register(&store);
    let pid_file = fixture.marker("check.pid");
    let slow = sh_check(&format!(
        "echo $$ > '{}'; exec sleep 60",
        pid_file.display()
    ));
    admit(
        &store,
        &checked_contract(&fixture, "checked-cancel", vec![slow]),
    );
    let executor = Executor::start(
        store.clone(),
        fixture.config(&data, &succeeding_agent(&fixture)),
    )
    .unwrap();

    wait_until("check to record its pid", || {
        fs::read_to_string(&pid_file).is_ok_and(|text| text.trim().parse::<u32>().is_ok())
    });
    let pid: u32 = fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    store.cancel_verifying_task("checked-cancel").unwrap();
    executor.cancel_verification("checked-cancel");

    wait_until("check process to exit", || {
        !orchestrator_runtime::is_process_running(pid)
    });
    assert_eq!(
        store.get("checked-cancel").unwrap().state,
        TaskState::Cancelled
    );
    executor.shutdown();
}

#[test]
fn restart_resumes_a_candidate_left_verifying_and_settles_it() {
    let fixture = Fixture::new();
    let data = fixture.data();
    let store = Arc::new(SqliteStore::new(&data.join("store.sqlite")).unwrap());
    fixture.register(&store);
    // A stopped daemon finished the agent: the session exited and the candidate
    // was recorded, but verification never ran.
    seed_interrupted_run(&store, &fixture, "resumed", None);
    store
        .update_session_state("resumed", "resumed-session-1", SessionState::Exited)
        .unwrap();
    store
        .update_attempt_state("resumed", "resumed-attempt-1", AttemptState::CandidateReady)
        .unwrap();
    store
        .transition("resumed", TaskEvent::CandidateReady)
        .unwrap();
    assert_eq!(store.get("resumed").unwrap().state, TaskState::Verifying);

    let executor = Executor::start(
        store.clone(),
        fixture.config(&data, &succeeding_agent(&fixture)),
    )
    .unwrap();

    wait_until("resumed candidate to succeed", || {
        store.get("resumed").unwrap().state == TaskState::Succeeded
    });
    assert_eq!(
        store.get("resumed").unwrap().attempts[0].state,
        AttemptState::Succeeded
    );
    executor.shutdown();
}

fn message_line(text: &str) -> String {
    format!(
        "printf '{{\"type\":\"item.completed\",\"item\":{{\"id\":\"i\",\"type\":\"agent_message\",\"text\":\"{text}\\\\n\"}}}}\\n'\n"
    )
}

fn output_tail(api: &DaemonApi, task_id: &str, extra: Value) -> Result<Value, RpcError> {
    let mut params = json!({"task_id": task_id, "session_id": format!("{task_id}-session-1")});
    params
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    api.dispatch("sessions.output_tail", params)
}

fn tail_texts(tail: &Value) -> Vec<String> {
    tail["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line["text"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn output_tail_is_read_only_and_leaves_a_running_agent_and_its_exit_untouched() {
    let fixture = Fixture::new();
    let release = fixture.marker("release");
    let agent = fixture.script(
        "chatty-agent",
        &format!(
            "{THREAD_STARTED}{}while [ ! -f '{}' ]; do sleep 0.05; done\n{}echo produced > agent-output.txt\n{TURN_COMPLETED}",
            message_line("before"),
            release.display(),
            message_line("after"),
        ),
    );
    let api = fixture.api(&agent);
    let task_id = fixture.submit(&api, "tail-1", 0);
    wait_state(&api, &task_id, "Running");
    wait_until("the first output line", || {
        output_tail(&api, &task_id, json!({})).is_ok_and(|tail| tail_texts(&tail) == ["before"])
    });

    for _ in 0..50 {
        let tail = output_tail(&api, &task_id, json!({})).unwrap();
        assert_eq!(tail["available"], true);
        assert_eq!(tail["ended"], false);
        assert_eq!(tail["lines"][0]["stream"], "stdout");
        assert_eq!(tail["lines"][0]["sequence"], 1);
        assert_eq!(tail["lines"][0]["line_truncated"], false);
        assert_eq!(tail["truncated_lines"], false);
    }
    assert_eq!(task(&api, &task_id)["sessions"][0]["state"], "Running");

    fs::write(&release, "").unwrap();
    let done = wait_state(&api, &task_id, "Succeeded");
    assert_eq!(done["attempts"][0]["state"], "Succeeded");
    assert_eq!(done["sessions"][0]["state"], "Exited");

    let tail = output_tail(&api, &task_id, json!({})).unwrap();
    assert_eq!(tail_texts(&tail), ["before", "after"]);
    assert_eq!(tail["ended"], true);
    assert_eq!(tail["last_sequence"], 2);
    let newer = output_tail(&api, &task_id, json!({"after_sequence": 1})).unwrap();
    assert_eq!(tail_texts(&newer), ["after"]);
    assert_eq!(newer["truncated_lines"], false);
    api.shutdown();
}

#[test]
fn output_tail_flood_stays_within_the_ring_and_line_caps_and_the_run_still_succeeds() {
    use crate::executor::output_ring::{MAX_LINE_BYTES, MAX_TAIL_LINES, RING_LINES};
    let fixture = Fixture::new();
    let agent = fixture.script(
        "flood-agent",
        &format!(
            "{THREAD_STARTED}pad=$(head -c {} /dev/zero | tr '\\0' x)\ni=0\nwhile [ $i -lt {} ]; do\nprintf '{{\"type\":\"item.completed\",\"item\":{{\"id\":\"i\",\"type\":\"agent_message\",\"text\":\"%s %s\\\\n\"}}}}\\n' $i \"$pad\"\ni=$((i+1))\ndone\necho produced > agent-output.txt\n{TURN_COMPLETED}",
            MAX_LINE_BYTES * 2,
            RING_LINES * 3,
        ),
    );
    let api = fixture.api(&agent);
    let task_id = fixture.submit(&api, "flood-1", 0);
    let done = wait_state(&api, &task_id, "Succeeded");
    assert_eq!(done["attempts"][0]["state"], "Succeeded");

    let tail = output_tail(&api, &task_id, json!({"limit": 100_000})).unwrap();
    assert_eq!(tail["limit"], MAX_TAIL_LINES);
    let lines = tail["lines"].as_array().unwrap();
    assert_eq!(lines.len(), MAX_TAIL_LINES);
    assert_eq!(tail["truncated_lines"], true);
    assert_eq!(tail["last_sequence"], RING_LINES * 3);
    for line in lines {
        assert!(line["text"].as_str().unwrap().len() <= MAX_LINE_BYTES);
        assert_eq!(line["line_truncated"], true);
    }
    // Everything older than the ring is gone; the ring holds RING_LINES.
    let oldest = output_tail(&api, &task_id, json!({"after_sequence": 0, "limit": 100})).unwrap();
    assert!(oldest["lines"][0]["sequence"].as_u64().unwrap() > (RING_LINES * 2) as u64);
    api.shutdown();
}

#[test]
fn output_tail_defaults_clamps_and_rejects_bad_input_and_unknown_sessions() {
    use crate::executor::output_ring::{DEFAULT_TAIL_LINES, MAX_TAIL_LINES};
    let fixture = Fixture::new();
    let body: String = (0..150)
        .map(|number| message_line(&format!("row {number}")))
        .collect();
    let agent = fixture.script(
        "rows-agent",
        &format!("{THREAD_STARTED}{body}echo produced > agent-output.txt\n{TURN_COMPLETED}"),
    );
    let api = fixture.api(&agent);
    let task_id = fixture.submit(&api, "rows-1", 0);
    wait_state(&api, &task_id, "Succeeded");

    let default = output_tail(&api, &task_id, json!({})).unwrap();
    assert_eq!(default["limit"], DEFAULT_TAIL_LINES);
    let rows = tail_texts(&default);
    assert_eq!(rows.len(), DEFAULT_TAIL_LINES);
    assert_eq!(rows.last().unwrap(), "row 149");
    assert_eq!(default["truncated_lines"], true);

    let clamped = output_tail(&api, &task_id, json!({"limit": 101})).unwrap();
    assert_eq!(clamped["limit"], MAX_TAIL_LINES);
    assert_eq!(tail_texts(&clamped).len(), MAX_TAIL_LINES);
    let exact = output_tail(&api, &task_id, json!({"limit": 100})).unwrap();
    assert_eq!(tail_texts(&exact).len(), 100);
    let three = output_tail(&api, &task_id, json!({"limit": 3})).unwrap();
    assert_eq!(tail_texts(&three), ["row 147", "row 148", "row 149"]);

    for bad in [
        json!({"limit": 0}),
        json!({"limit": -5}),
        json!({"limit": "ten"}),
        json!({"after_sequence": -1}),
        json!({"extra": true}),
    ] {
        assert_eq!(
            output_tail(&api, &task_id, bad.clone()),
            Err(RpcError::InvalidParams),
            "{bad}"
        );
    }
    assert_eq!(
        api.dispatch(
            "sessions.output_tail",
            json!({"task_id": task_id, "session_id": "no-such-session"})
        ),
        Err(RpcError::NotFound)
    );
    assert_eq!(
        api.dispatch(
            "sessions.output_tail",
            json!({"task_id": "no-such-task", "session_id": "s"})
        ),
        Err(RpcError::NotFound)
    );
    assert_eq!(
        api.dispatch("sessions.output_tail", Value::Null),
        Err(RpcError::InvalidParams)
    );
    api.shutdown();
}

#[test]
fn output_tail_of_a_session_the_daemon_has_no_output_for_is_empty_and_flagged() {
    let fixture = Fixture::new();
    let agent = succeeding_agent(&fixture);
    let api = fixture.api(&agent);
    let task_id = fixture.submit(&api, "restart-1", 0);
    wait_state(&api, &task_id, "Succeeded");
    api.shutdown();

    // A new daemon over the same data has the session on record but no ring.
    let restarted = fixture.api(&fixture.script("idle-agent", "exit 0\n"));
    let tail = output_tail(&restarted, &task_id, json!({})).unwrap();
    assert_eq!(tail["available"], false);
    assert_eq!(tail["ended"], true);
    assert_eq!(tail["lines"], json!([]));
    assert_eq!(tail["last_sequence"], 0);
    restarted.shutdown();
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
fn an_agent_without_a_launcher_is_accepted_at_submit_when_its_cli_is_absent() {
    let fixture = Fixture::new();
    let api = fixture.api_with(&succeeding_agent(&fixture), |config| {
        config.pi_program = Some(fixture.root.path().join("no-such-pi"));
    });
    // Pi has no daemon launcher, so the submit-time check has no CLI to read or probe.
    // submit_full panics on a refusal, so a returned id is the acceptance.
    fixture.submit_full(&api, "no-launcher", "pi", 0, &[]);
    api.shutdown();
}

#[test]
fn extended_codex_is_invalid_params_even_when_its_cli_cannot_be_probed() {
    let fixture = Fixture::new();
    // A Codex CLI whose help cannot be read, so a probe of it is unconfirmed.
    let silent = fixture.script(
        "silent-codex",
        "if [ \"$1\" = '--help' ]; then exit 1; fi\nexit 1\n",
    );
    let api = fixture.api(&silent);
    let mut contract = launch_contract(&fixture, "codex");
    contract["permission"] = json!("extended");
    // Codex builds no argv for Extended, so the submit-time CLI check is skipped and the level
    // is refused as invalid, not as a CLI conflict.
    let refused = api.dispatch(
        "tasks.submit",
        json!({"idempotency_key": "extended-silent", "contract": contract}),
    );
    assert_eq!(refused, Err(RpcError::InvalidParams));
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
        permission: Default::default(),
        work_item: None,
    }
}

/// A fake CLI that records its argv, one argument per line, to `record`, then
/// finishes as a successful run would. It answers `--version` as Copilot does.
/// The record is written under a temporary name and renamed, so a reader that
/// sees the file sees all of it.
fn recording_agent(fixture: &Fixture, record: &Path) -> PathBuf {
    fixture.script(
        "recording-agent",
        &format!(
            "if [ \"$1\" = \"--version\" ]; then echo 'GitHub Copilot CLI 1.0.109'; exit 0; fi\n\
             printf '%s\\n' \"$@\" > '{0}.tmp' && mv '{0}.tmp' '{0}'\n\
             {THREAD_STARTED}{TURN_COMPLETED}",
            record.display()
        ),
    )
}

fn strs(argv: &[String]) -> Vec<&str> {
    argv.iter().map(String::as_str).collect()
}

/// Submits one task for `agent` at `permission` (`None` leaves the field out)
/// and returns the argv the run launched with. The Copilot usage file and
/// worktree differ per run, so they are replaced by fixed names here.
fn launched_argv(
    agent: &str,
    permission: Option<&str>,
    adjust: impl FnOnce(&mut ExecutorConfig),
) -> Vec<String> {
    let fixture = Fixture::new();
    let record = fixture.marker("argv");
    let program = recording_agent(&fixture, &record);
    let api = fixture.api_with(&program, |config| {
        config.claude_program = Some(program.clone());
        config.copilot_program = Some(program.clone());
        adjust(config);
    });
    let mut contract = launch_contract(&fixture, agent);
    if let Some(level) = permission {
        contract["permission"] = json!(level);
    }
    api.dispatch(
        "tasks.submit",
        json!({"idempotency_key": "launch", "contract": contract}),
    )
    .unwrap();
    wait_until("the agent to record its argv", || record.exists());
    let mut argv: Vec<String> = fs::read_to_string(&record)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    api.shutdown();
    if let Some(i) = argv.iter().position(|arg| arg == "--usage-output-file") {
        argv[i + 1] = "<usage>".to_owned();
    }
    if let Some(i) = argv.iter().position(|arg| arg == "-C") {
        let worktree = argv[i + 1].clone();
        argv = argv
            .into_iter()
            .map(|arg| arg.replace(&worktree, "<worktree>"))
            .collect();
    }
    argv
}

fn launch_contract(fixture: &Fixture, agent: &str) -> Value {
    json!({
        "project_id": fixture.project_id(),
        "parent_id": null,
        "objective": "objective of launch",
        "kind": "Implement",
        "agent": agent,
        "dependencies": [],
        "verification": "None",
        "delivery": "ReportOnly",
        "autonomy": "WorkspaceWrites",
        "max_retries": 0,
        "max_repairs": 0
    })
}

/// The head every Claude task vector shares, up to its tool flags.
const CLAUDE_TASK_HEAD: [&str; 13] = [
    "-p",
    "objective of launch",
    "--verbose",
    "--output-format",
    "stream-json",
    "--include-partial-messages",
    "--permission-mode",
    "dontAsk",
    "--permission-prompts",
    "none",
    "--restricted",
    "--strict-mcp-config",
    "--tools",
];

#[test]
fn codex_task_argv_per_level_is_exact() {
    let objective = "objective of launch";
    let argv = |permission| launched_argv("codex", permission, |_| {});
    // The fixture workflow agent is sent as developer instructions before the objective.
    let restricted = vec![
        "exec",
        "--json",
        "--sandbox",
        "workspace-write",
        "--ignore-user-config",
        "-c",
        "developer_instructions=\"fixture instructions\"",
        "-c",
        "show_raw_agent_reasoning=true",
        "-c",
        "model_reasoning_summary=\"detailed\"",
        "--",
        objective,
    ];
    // A task with no level is Restricted, so an existing Codex task changes.
    assert_eq!(strs(&argv(None)), restricted);
    assert_eq!(strs(&argv(Some("restricted"))), restricted);
    assert_eq!(
        strs(&argv(Some("auto"))),
        [
            "exec",
            "--json",
            "--sandbox",
            "workspace-write",
            "--approve-for-me",
            "--ignore-user-config",
            "-c",
            "developer_instructions=\"fixture instructions\"",
            "-c",
            "show_raw_agent_reasoning=true",
            "-c",
            "model_reasoning_summary=\"detailed\"",
            "--",
            objective,
        ]
    );
    assert_eq!(
        strs(&argv(Some("full"))),
        [
            "exec",
            "--json",
            "-c",
            "developer_instructions=\"fixture instructions\"",
            "-c",
            "show_raw_agent_reasoning=true",
            "-c",
            "model_reasoning_summary=\"detailed\"",
            "--",
            objective,
        ]
    );
    // Without a workflow agent the vector is the one Codex launched before.
    let plain = launched_argv("codex", None, |config| {
        config.locate_workflow_agent = no_workflow_agent
    });
    assert_eq!(
        strs(&plain),
        [
            "exec",
            "--json",
            "--sandbox",
            "workspace-write",
            "--ignore-user-config",
            "-c",
            "show_raw_agent_reasoning=true",
            "-c",
            "model_reasoning_summary=\"detailed\"",
            "--",
            objective,
        ]
    );
}

#[test]
fn extended_and_unknown_levels_are_refused_at_submit_and_create_nothing() {
    let fixture = Fixture::new();
    let mut diagnostics = None;
    let api = fixture.api_with(&succeeding_agent(&fixture), |config| {
        diagnostics = Some(config.diagnostics.clone());
    });
    for level in ["extended", "sandboxed"] {
        let mut contract = launch_contract(&fixture, "codex");
        contract["permission"] = json!(level);
        let refused = api.dispatch(
            "tasks.submit",
            json!({"idempotency_key": level, "contract": contract}),
        );
        assert_eq!(refused, Err(RpcError::InvalidParams), "{level}");
    }
    let store = SqliteStore::new(&fixture.data().join("store.sqlite")).unwrap();
    assert!(store.list().unwrap().is_empty());
    let lines = diagnostics.unwrap().lines();
    assert!(
        lines
            .iter()
            .any(|line| line.contains("permission_level_unsupported")),
        "{lines:?}"
    );
    api.shutdown();
}

#[test]
fn claude_task_argv_per_level_is_exact() {
    let claude = |permission, adjust: fn(&mut ExecutorConfig)| {
        launched_argv("claude-code", permission, adjust)
    };
    let mut restricted = CLAUDE_TASK_HEAD.to_vec();
    restricted.extend(["Read,Edit,Write", "--allowedTools", "Read,Edit,Write"]);
    // Every level ends with the fixture workflow agent's flags. Without one, Absent and
    // Restricted launch the vector a Claude task has always had.
    let restricted_agent = with_flags(restricted.clone(), &FIXTURE_AGENT_FLAGS);
    assert_eq!(strs(&claude(None, |_| {})), restricted_agent);
    assert_eq!(strs(&claude(Some("restricted"), |_| {})), restricted_agent);
    assert_eq!(
        strs(&claude(Some("restricted"), |config| {
            config.locate_workflow_agent = no_workflow_agent
        })),
        restricted
    );

    let mut extended = CLAUDE_TASK_HEAD.to_vec();
    extended.extend([
        "Read,Edit,Write,Bash",
        "--allowedTools",
        "Read,Edit,Write",
        "Bash(git status *)",
        "Bash(git diff *)",
        "Bash(git log *)",
        "Bash(git show *)",
    ]);
    assert_eq!(
        strs(&claude(Some("extended"), |_| {})),
        with_flags(extended, &FIXTURE_AGENT_FLAGS)
    );

    let auto = [
        "-p",
        "objective of launch",
        "--verbose",
        "--output-format",
        "stream-json",
        "--include-partial-messages",
        "--permission-mode",
        "auto",
        "--permission-prompts",
        "none",
        "--restricted",
        "--strict-mcp-config",
        "--tools",
        "Read,Edit,Write,Bash",
        "--allowedTools",
        "Read,Edit,Write",
        "--disallowedTools",
        "Bash(git push *)",
        "Bash(gh *)",
    ];
    assert_eq!(
        strs(&claude(Some("auto"), |_| {})),
        with_flags(auto.to_vec(), &FIXTURE_AGENT_FLAGS)
    );

    let full = [
        "-p",
        "objective of launch",
        "--verbose",
        "--output-format",
        "stream-json",
        "--include-partial-messages",
        "--permission-mode",
        "bypassPermissions",
        "--permission-prompts",
        "none",
        "--allow-dangerously-skip-permissions",
    ];
    assert_eq!(
        strs(&claude(Some("full"), |_| {})),
        with_flags(full.to_vec(), &FIXTURE_AGENT_FLAGS)
    );
}

#[test]
fn copilot_task_argv_per_level_is_exact() {
    let prompt = "--prompt=objective of launch";
    let mut restricted = vec![
        prompt,
        "--output-format",
        "json",
        "--log-level",
        "none",
        "--no-auto-update",
        "--no-ask-user",
        "--no-custom-instructions",
        "--disable-builtin-mcps",
        "--disallow-temp-dir",
        "--usage-output-file",
        "<usage>",
        "-C",
        "<worktree>",
    ];
    restricted.extend([
        "--allow-tool",
        "read",
        "--allow-tool",
        "write(<worktree>/**)",
        "--deny-tool",
        "shell",
        "--deny-tool",
        "url",
        "--deny-tool",
        "memory",
    ]);
    let head = restricted[..restricted.len() - 10].to_vec();
    assert_eq!(
        strs(&launched_argv(COPILOT_AGENT, None, |_| {})),
        with_flags(restricted.clone(), &FIXTURE_AGENT_FLAGS)
    );
    assert_eq!(
        strs(&launched_argv(COPILOT_AGENT, Some("restricted"), |_| {})),
        with_flags(restricted.clone(), &FIXTURE_AGENT_FLAGS)
    );
    let plain = launched_argv(COPILOT_AGENT, None, |config| {
        config.locate_workflow_agent = no_workflow_agent
    });
    assert_eq!(strs(&plain), restricted);

    let mut extended = head.clone();
    extended.extend([
        "--allow-tool",
        "read",
        "--allow-tool",
        "write(<worktree>/**)",
        "--allow-tool",
        "shell(git status)",
        "--allow-tool",
        "shell(git diff)",
        "--allow-tool",
        "shell(git log)",
        "--allow-tool",
        "shell(git show)",
        "--deny-tool",
        "url",
        "--deny-tool",
        "memory",
    ]);
    assert_eq!(
        strs(&launched_argv(COPILOT_AGENT, Some("extended"), |_| {})),
        with_flags(extended, &FIXTURE_AGENT_FLAGS)
    );

    let mut auto = head.clone();
    auto.push("--assisted-approval");
    auto.extend([
        "--allow-tool",
        "read",
        "--allow-tool",
        "write(<worktree>/**)",
        "--deny-tool",
        "url",
        "--deny-tool",
        "memory",
        "--deny-tool",
        "shell(git push)",
    ]);
    assert_eq!(
        strs(&launched_argv(COPILOT_AGENT, Some("auto"), |_| {})),
        with_flags(auto, &FIXTURE_AGENT_FLAGS)
    );

    let full = [
        prompt,
        "--output-format",
        "json",
        "--log-level",
        "none",
        "--no-auto-update",
        "--no-ask-user",
        "--disable-builtin-mcps",
        "--usage-output-file",
        "<usage>",
        "-C",
        "<worktree>",
        "--allow-all",
    ];
    assert_eq!(
        strs(&launched_argv(COPILOT_AGENT, Some("full"), |_| {})),
        with_flags(full.to_vec(), &FIXTURE_AGENT_FLAGS)
    );
}

/// The credit cap reaches a Copilot task launch only when the daemon turns it on.
#[test]
fn copilot_task_passes_the_credit_cap_only_when_it_is_on() {
    let off = launched_argv(COPILOT_AGENT, Some("auto"), |_| {});
    assert!(!off.iter().any(|arg| arg == "--max-ai-credits"), "{off:?}");
    let on = launched_argv(COPILOT_AGENT, Some("auto"), |config| {
        config.copilot_credit_cap = true;
    });
    let at = on
        .iter()
        .position(|arg| *arg == "--max-ai-credits")
        .unwrap();
    assert_eq!(on[at + 1], "30");
    assert_eq!(
        on.iter().filter(|arg| **arg == "--max-ai-credits").count(),
        1
    );
}

#[test]
fn launch_event_and_task_record_report_the_level() {
    let fixture = Fixture::new();
    let record = fixture.marker("argv");
    let program = recording_agent(&fixture, &record);
    let mut diagnostics = None;
    let api = fixture.api_with(&program, |config| {
        config.claude_program = Some(program.clone());
        diagnostics = Some(config.diagnostics.clone());
    });
    let diagnostics = diagnostics.unwrap();
    let mut contract = launch_contract(&fixture, "claude-code");
    contract["permission"] = json!("auto");
    let submitted = api
        .dispatch(
            "tasks.submit",
            json!({"idempotency_key": "level-report", "contract": contract}),
        )
        .unwrap();
    assert_eq!(submitted["contract"]["permission"], "auto");
    wait_until("the launch event", || {
        diagnostics
            .lines()
            .iter()
            .any(|line| line.contains("agent_launched"))
    });
    let lines = diagnostics.lines();
    assert!(
        lines.iter().any(|line| line.contains("agent_launched")
            && line.contains("claude-code:auto:task:v1")
            && line.contains("auto")),
        "{lines:?}"
    );
    api.shutdown();
}

/// Leaves `task_id` Running with a Running session, as a daemon that died
/// mid-run would. `identity` is the recorded agent process, if any.
fn seed_interrupted_run(store: &SqliteStore, fixture: &Fixture, task_id: &str, pid: Option<u32>) {
    store.create(&queued_contract(fixture, task_id)).unwrap();
    store.transition(task_id, TaskEvent::Admit).unwrap();
    let worktrees = fixture.data().join("worktrees");
    fs::create_dir_all(&worktrees).unwrap();
    let treehouse = Treehouse::new(test_treehouse_program(), &fixture.data()).unwrap();
    let workspace_id = format!("{task_id}-workspace-1");
    let worktree = create_task_worktree(
        &Repository::new(&fixture.repo).unwrap(),
        task_id,
        "HEAD",
        &fs::canonicalize(worktrees).unwrap(),
        &BranchPrefix::default(),
        &treehouse,
        &workspace_id,
    )
    .unwrap();
    store
        .add_workspace(&Workspace {
            id: workspace_id,
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
                failure_reason: None,
                summary: None,
                changed_files: None,
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
        store.get("waiting").unwrap().state == TaskState::Succeeded
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
fn fast_exit_before_identity_preserves_failure_status() {
    let fixture = Fixture::new();
    let agent = fixture.script("fast-agent", &format!("{THREAD_STARTED}exit 3\n"));
    let diag = Diagnostics::memory();
    let api = fixture.api_with(&agent, |config| {
        config.diagnostics = diag.clone();
        config.wait_for_exit_before_identity = true;
    });
    let task_id = fixture.submit(&api, "fast-exit", 0);
    let failed = wait_state(&api, &task_id, "Failed");
    assert_eq!(failed["attempts"][0]["state"], "Failed");
    wait_until("exit diagnostic", || {
        !events(&diag, "agent_exited").is_empty()
    });
    assert_eq!(events(&diag, "agent_exited")[0]["exit_code"], "3");
    assert_eq!(events(&diag, "identity_skipped_agent_exited").len(), 1);
    let session = failed["sessions"][0]["id"].as_str().unwrap();
    let store = SqliteStore::new(&fixture.data().join("store.sqlite")).unwrap();
    assert!(store.session_identity(&task_id, session).unwrap().is_none());
    api.shutdown();
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
fn task_launch_refuses_a_replaced_cli_without_required_permission_options() {
    use crate::api::load::tests::{at, close_gate, cpu};

    let fixture = Fixture::new();
    let launched = fixture.marker("inference-started");
    let program = fixture.script("fake-codex", &format!("touch '{}'\n", launched.display()));
    let load = Arc::new(crate::api::load::LoadHandle::default());
    let last = close_gate(&load);
    let api = fixture.api_with(&program, |config| config.load = load.clone());
    // Prime the help cache via discovery so a stale cache would permit this task.
    api.dispatch("agents.capabilities", Value::Null).unwrap();
    let task_id = fixture.submit(&api, "changed-cli", 0);
    assert_eq!(task(&api, &task_id)["state"], "Queued");
    let replacement = fixture.script(
        "new-codex",
        &format!(
            "if [ \"$1\" = exec ] && [ \"$2\" = --help ]; then\n\
             printf '%s\\n' --json --ignore-user-config\nexit 0\nfi\n\
             touch '{}'\n",
            launched.display(),
        ),
    );
    fs::rename(replacement, &program).unwrap();
    assert_eq!(
        api.dispatch(
            "tasks.submit",
            json!({
                "idempotency_key": "refuse-replacement",
                "contract": {
                    "project_id": fixture.project_id(),
                    "parent_id": null,
                    "objective": "never launch this task",
                    "agent": "codex",
                    "kind": "Implement",
                    "dependencies": [],
                    "verification": "None",
                    "delivery": "ReportOnly",
                    "autonomy": "WorkspaceWrites",
                    "max_retries": 0,
                    "max_repairs": 0
                }
            }),
        ),
        Err(RpcError::Conflict)
    );
    for n in 1..=8 {
        load.record(cpu(0.0), at(last, n));
    }
    wait_state(&api, &task_id, "Failed");
    assert!(task(&api, &task_id)["attempts"][0]["failure_reason"]
        .as_str()
        .unwrap()
        .contains("--sandbox"));
    assert!(!launched.exists());
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
    assert_eq!(launch["error"], "the agent executable was not found");
    for line in diag.lines() {
        assert!(!line.contains("SECRETVALUE"), "credential leaked: {line}");
    }
    api.shutdown();
}

#[test]
fn launch_failures_are_stored_on_their_attempt_and_survive_retry() {
    let fixture = Fixture::new();
    let empty_repo = fixture.root.path().join("empty-repo");
    fs::create_dir_all(&empty_repo).unwrap();
    run_git(&empty_repo, &["init", "--initial-branch=main"]);
    let project_id = fs::canonicalize(&empty_repo).unwrap().display().to_string();

    let data = fixture.data();
    let store = SqliteStore::new(&data.join("store.sqlite")).unwrap();
    store
        .register_project(&Project {
            id: project_id.clone(),
            root_path: project_id.clone(),
            settings: json!({}),
        })
        .unwrap();
    drop(store);
    let missing_agent = fixture.root.path().join("missing-codex");
    let api = fixture.api_with(&missing_agent, |_| {});
    let submitted = api
        .dispatch(
            "tasks.submit",
            json!({
                "idempotency_key": "empty-head-launch",
                "contract": {
                    "project_id": project_id,
                    "parent_id": null,
                    "objective": "Fail before launching in an empty repository",
                    "kind": "Implement",
                    "agent": "codex",
                    "dependencies": [],
                    "verification": "None",
                    "delivery": "ReportOnly",
                    "autonomy": "WorkspaceWrites",
                    "max_retries": 1,
                    "max_repairs": 0
                }
            }),
        )
        .unwrap();
    let task_id = submitted["contract"]["id"].as_str().unwrap();
    let first = wait_state(&api, task_id, "Failed");
    assert_eq!(
        first["attempts"][0]["failure_reason"],
        "The repository has no commit at HEAD. Create an initial commit in the registered repository, then retry."
    );
    assert_eq!(first["sessions"], json!([]));

    run_git(&empty_repo, &["config", "user.name", "executor test"]);
    run_git(
        &empty_repo,
        &["config", "user.email", "executor@example.invalid"],
    );
    fs::write(empty_repo.join("initial.txt"), "initial\n").unwrap();
    run_git(&empty_repo, &["add", "initial.txt"]);
    run_git(&empty_repo, &["commit", "-m", "initial"]);

    api.dispatch(
        "tasks.retry",
        json!({"task_id": task_id, "idempotency_key": "empty-head-retry"}),
    )
    .unwrap();
    wait_until("second launch failure", || {
        let record = task(&api, task_id);
        record["state"] == "Failed" && record["attempts"].as_array().unwrap().len() == 2
    });
    let retried = task(&api, task_id);
    let attempts = retried["attempts"].as_array().unwrap();
    assert_eq!(attempts[0]["state"], "Failed");
    assert_eq!(attempts[1]["state"], "Failed");
    assert_eq!(
        attempts[1]["failure_reason"],
        "The daemon could not start the selected agent. Check that its CLI is installed and signed in, then retry; see the daemon log for details."
    );
    assert_ne!(attempts[0]["failure_reason"], attempts[1]["failure_reason"]);
    assert_eq!(retried["sessions"][0]["attempt_id"], attempts[1]["id"]);
    api.shutdown();
}

/// The defaults row a project with a base branch `base_branch` and the prefix `team/` has.
fn project_defaults_row(
    fixture: &Fixture,
    base_branch: &str,
) -> orchestrator_store::ProjectDefaultsRow {
    orchestrator_store::ProjectDefaultsRow {
        project_id: fixture.project_id(),
        mode: "local-only".to_owned(),
        base_branch: Some(base_branch.to_owned()),
        branch_prefix: Some("team/".to_owned()),
        checks: Vec::new(),
        merge_method: "squash".to_owned(),
        updated_at: String::new(),
    }
}

#[test]
fn a_task_is_created_from_the_projects_base_branch_and_prefix() {
    let fixture = Fixture::new();
    // `release-1` is one commit ahead of `main`, and `main` stays checked out.
    run_git(&fixture.repo, &["checkout", "-q", "-b", "release-1"]);
    fs::write(fixture.repo.join("release.txt"), "release\n").unwrap();
    run_git(&fixture.repo, &["add", "release.txt"]);
    run_git(&fixture.repo, &["commit", "-q", "-m", "release"]);
    run_git(&fixture.repo, &["checkout", "-q", "main"]);
    let api = fixture.api(&succeeding_agent(&fixture));
    let store = SqliteStore::new(&fixture.data().join("store.sqlite")).unwrap();
    store
        .set_project_defaults(&project_defaults_row(&fixture, "release-1"))
        .unwrap();
    let task_id = fixture.submit(&api, "base-1", 0);

    wait_state(&api, &task_id, "Succeeded");
    let workspace = store
        .list_workspaces()
        .unwrap()
        .into_iter()
        .find(|workspace| workspace.task_id == task_id)
        .unwrap();
    assert!(
        workspace.branch.starts_with("team/"),
        "{}",
        workspace.branch
    );
    let tip = Command::new("git")
        .arg("-C")
        .arg(&fixture.repo)
        .args(["rev-parse", "release-1"])
        .output()
        .unwrap();
    assert_eq!(
        workspace.base_commit,
        String::from_utf8(tip.stdout).unwrap().trim()
    );
    api.shutdown();
}

/// The worktree path a task's first workspace records.
fn workspace_path(fixture: &Fixture, task_id: &str) -> String {
    let store = SqliteStore::new(&fixture.data().join("store.sqlite")).unwrap();
    store
        .list_workspaces()
        .unwrap()
        .into_iter()
        .find(|workspace| workspace.task_id == task_id)
        .unwrap()
        .path
}

/// Leases one more worktree from the same pool and returns its path. A slot that was
/// returned is handed out again, so the path matches it; a slot still leased is not.
fn lease_one_more(fixture: &Fixture) -> String {
    let treehouse = Treehouse::new(test_treehouse_program(), &fixture.data()).unwrap();
    let worktrees = fs::canonicalize(fixture.data().join("worktrees")).unwrap();
    let worktree = create_task_worktree(
        &Repository::new(&fixture.repo).unwrap(),
        "lease-probe",
        "HEAD",
        &worktrees,
        &BranchPrefix::default(),
        &treehouse,
        "lease-probe-workspace-1",
    )
    .unwrap();
    worktree.path().display().to_string()
}

#[test]
fn archiving_a_finished_task_returns_its_worktree_slot_to_the_pool() {
    let fixture = Fixture::new();
    let api = fixture.api(&succeeding_agent(&fixture));
    let task_id = fixture.submit(&api, "archive-release", 0);
    wait_state(&api, &task_id, "Succeeded");
    let leased = workspace_path(&fixture, &task_id);

    api.dispatch(
        "tasks.archive",
        json!({"task_ids": [task_id], "idempotency_key": "archive-release-call"}),
    )
    .unwrap();

    assert_eq!(lease_one_more(&fixture), leased);
    // The path may now belong to another task, so it is no longer shown as this task's diff.
    assert_eq!(
        api.dispatch("workspaces.diff", json!({"task_id": task_id})),
        Err(RpcError::NotFound)
    );
    api.shutdown();
}

#[test]
fn archiving_keeps_the_slot_of_a_task_whose_worktree_is_dirty() {
    let fixture = Fixture::new();
    let api = fixture.api(&succeeding_agent(&fixture));
    let task_id = fixture.submit(&api, "archive-dirty", 0);
    wait_state(&api, &task_id, "Succeeded");
    let leased = workspace_path(&fixture, &task_id);
    fs::write(Path::new(&leased).join("scratch.txt"), "uncommitted\n").unwrap();

    let archived = api
        .dispatch(
            "tasks.archive",
            json!({"task_ids": [task_id], "idempotency_key": "archive-dirty-call"}),
        )
        .unwrap();

    assert_eq!(archived, json!({"archived": [task_id]}));
    assert_eq!(
        fs::read_to_string(Path::new(&leased).join("scratch.txt")).unwrap(),
        "uncommitted\n"
    );
    assert_ne!(lease_one_more(&fixture), leased);
    api.shutdown();
}

#[test]
fn a_missing_treehouse_fails_the_attempt_with_a_stable_reason() {
    let fixture = Fixture::new();
    let api = fixture.api_with(&fixture.root.path().join("missing-codex"), |config| {
        config.treehouse_program = fixture.root.path().join("no-such-treehouse");
    });
    let task_id = fixture.submit(&api, "no-treehouse", 0);

    let failed = wait_state(&api, &task_id, "Failed");
    assert_eq!(
        failed["attempts"][0]["failure_reason"],
        WORKTREE_MANAGER_MISSING
    );
    assert_eq!(failed["sessions"], json!([]));
    api.shutdown();
}

#[test]
fn a_missing_base_branch_fails_the_attempt_with_a_stable_reason() {
    let fixture = Fixture::new();
    // No agent can start: the worktree fails first.
    let api = fixture.api(&fixture.root.path().join("missing-codex"));
    let store = SqliteStore::new(&fixture.data().join("store.sqlite")).unwrap();
    store
        .set_project_defaults(&project_defaults_row(&fixture, "deleted-branch"))
        .unwrap();
    let task_id = fixture.submit(&api, "base-2", 0);

    let failed = wait_state(&api, &task_id, "Failed");
    assert_eq!(failed["attempts"][0]["failure_reason"], BASE_BRANCH_MISSING);
    assert!(!BASE_BRANCH_MISSING.contains("deleted-branch"));
    assert_eq!(failed["sessions"], json!([]));
    api.shutdown();
}

#[test]
fn a_tag_with_the_base_branchs_name_never_decides_the_base() {
    let fixture = Fixture::new();
    run_git(&fixture.repo, &["checkout", "-q", "-b", "release-1"]);
    fs::write(fixture.repo.join("release.txt"), "release\n").unwrap();
    run_git(&fixture.repo, &["add", "release.txt"]);
    run_git(&fixture.repo, &["commit", "-q", "-m", "release"]);
    run_git(&fixture.repo, &["checkout", "-q", "main"]);
    // The tag has the branch's name but points at the older commit on `main`.
    run_git(&fixture.repo, &["tag", "release-1", "main"]);
    let api = fixture.api(&succeeding_agent(&fixture));
    let store = SqliteStore::new(&fixture.data().join("store.sqlite")).unwrap();
    store
        .set_project_defaults(&project_defaults_row(&fixture, "release-1"))
        .unwrap();
    let task_id = fixture.submit(&api, "base-3", 0);

    wait_state(&api, &task_id, "Succeeded");
    let workspace = store
        .list_workspaces()
        .unwrap()
        .into_iter()
        .find(|workspace| workspace.task_id == task_id)
        .unwrap();
    let tip = Command::new("git")
        .arg("-C")
        .arg(&fixture.repo)
        .args(["rev-parse", "refs/heads/release-1"])
        .output()
        .unwrap();
    assert_eq!(
        workspace.base_commit,
        String::from_utf8(tip.stdout).unwrap().trim()
    );
    api.shutdown();
}

#[test]
fn an_unsafe_stored_base_branch_fails_the_attempt_with_a_stable_reason() {
    let fixture = Fixture::new();
    let api = fixture.api(&fixture.root.path().join("missing-codex"));
    let store = SqliteStore::new(&fixture.data().join("store.sqlite")).unwrap();
    store
        .set_project_defaults(&project_defaults_row(&fixture, "main~1"))
        .unwrap();
    let task_id = fixture.submit(&api, "base-4", 0);

    let failed = wait_state(&api, &task_id, "Failed");
    assert_eq!(failed["attempts"][0]["failure_reason"], BASE_BRANCH_MISSING);
    assert_eq!(failed["sessions"], json!([]));
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
        api.dispatch(
            "tasks.retry",
            json!({"task_id": "missing", "idempotency_key": "k"})
        ),
        Err(RpcError::NotFound)
    );

    let failed = events(&diag, "rpc_failed");
    assert_eq!(
        failed.len(),
        1,
        "caller mistakes such as NotFound stay quiet"
    );
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
        "an agent with a live owner must not be signaled"
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
        "a process that does not match the recorded identity must not be signaled"
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
    wait_state(&api, "p1", "Succeeded");
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
        fn try_exited(&self) -> std::io::Result<bool> {
            Ok(false)
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
            permission: Default::default(),
            worktree: PathBuf::new(),
            in_place: false,
            diag: orchestrator_diagnostics::Diagnostics::memory(),
            output: Default::default(),
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
            permission: PermissionLevel::Restricted,
            worktree: PathBuf::from("."),
            in_place: false,
            diag: orchestrator_diagnostics::Diagnostics::memory(),
            output: Default::default(),
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

fn metric(meter: MeterId, value: f64, total: Option<f64>) -> Meter {
    let (kind, unit) = match meter {
        MeterId::Cpu | MeterId::Gpu => (MeterKind::Utilization, Unit::Percent),
        MeterId::Memory | MeterId::Disk => (MeterKind::Capacity, Unit::Bytes),
    };
    Meter {
        meter,
        state: MeterState::Ok,
        kind,
        unit,
        scope: Some("test".into()),
        value: Some(value),
        total,
        observed_at: None,
        sample_interval_ms: 0,
        reason: None,
    }
}

fn cpu_snapshot(percent: f64) -> Snapshot {
    Snapshot {
        schema_version: 1,
        sample_interval_ms: 1000,
        meters: vec![metric(MeterId::Cpu, percent, None)],
    }
}

fn admission(api: &DaemonApi) -> Value {
    api.dispatch("host.metrics", Value::Null).unwrap()["admission"].clone()
}

#[test]
fn a_start_over_the_cpu_threshold_is_deferred_with_a_reason_then_admitted_when_it_drops() {
    let fixture = Fixture::new();
    let cpu = Arc::new(AtomicU32::new(90));
    let reading = cpu.clone();
    let api = fixture.api_with(&succeeding_agent(&fixture), |config| {
        config.admission = AdmissionConfig {
            thresholds: Thresholds {
                max_cpu_percent: Some(50.0),
                ..Thresholds::default()
            },
            source: Some(Arc::new(move || {
                cpu_snapshot(f64::from(reading.load(Ordering::SeqCst)))
            })),
        };
    });
    let task_id = fixture.submit(&api, "admit-cpu", 0);

    wait_until("the start to be deferred", || {
        admission(&api)["decision"]["reason_code"] == "cpu_over_threshold"
    });
    let state = admission(&api);
    assert_eq!(state["enabled"], true);
    assert_eq!(state["deferred_task_ids"], json!([task_id]));
    assert_eq!(state["decision"]["admitted"], false);
    assert_eq!(state["decision"]["breaches"][0]["threshold"], 50.0);
    assert_eq!(task(&api, &task_id)["state"], "Queued");

    cpu.store(10, Ordering::SeqCst);
    wait_state(&api, &task_id, "Succeeded");
    let state = admission(&api);
    assert_eq!(state["decision"]["reason_code"], "admitted");
    assert_eq!(state["deferred_task_ids"], json!([]));
    api.shutdown();
}

#[test]
fn an_unreadable_meter_never_blocks_a_start_and_is_reported_unknown() {
    let fixture = Fixture::new();
    let api = fixture.api_with(&succeeding_agent(&fixture), |config| {
        config.admission = AdmissionConfig {
            thresholds: Thresholds {
                max_memory_percent: Some(1.0),
                ..Thresholds::default()
            },
            source: Some(Arc::new(|| cpu_snapshot(99.0))),
        };
    });
    let task_id = fixture.submit(&api, "admit-unknown", 0);
    wait_state(&api, &task_id, "Succeeded");
    let state = admission(&api);
    assert_eq!(state["decision"]["reason_code"], "admitted_metric_unknown");
    assert_eq!(state["decision"]["unknown"], json!(["memory"]));
    api.shutdown();
}

#[test]
fn the_running_agent_threshold_defers_the_next_start_without_touching_running_work() {
    let fixture = Fixture::new();
    let release = fixture.marker("release");
    let api = fixture.api_with(&waiting_agent(&fixture, &release), |config| {
        config.admission.thresholds.max_running_agents = Some(1);
    });
    let first = fixture.submit(&api, "agents-1", 0);
    wait_state(&api, &first, "Running");
    let second = fixture.submit(&api, "agents-2", 0);

    wait_until("the second start to be deferred", || {
        admission(&api)["decision"]["reason_code"] == "running_agents_at_limit"
    });
    assert_eq!(admission(&api)["deferred_task_ids"], json!([second]));
    assert_eq!(task(&api, &first)["state"], "Running");
    assert_eq!(task(&api, &second)["state"], "Queued");

    fs::write(&release, "go").unwrap();
    wait_state(&api, &first, "Succeeded");
    wait_state(&api, &second, "Succeeded");
    api.shutdown();
}

#[test]
fn without_thresholds_admission_is_off_and_starts_are_unchanged() {
    let fixture = Fixture::new();
    let api = fixture.api(&succeeding_agent(&fixture));
    let task_id = fixture.submit(&api, "admit-off", 0);
    wait_state(&api, &task_id, "Succeeded");
    let state = admission(&api);
    assert_eq!(state["enabled"], false);
    assert_eq!(state["deferred_task_ids"], json!([]));
    assert_eq!(state["decision"]["admitted"], true);
    api.shutdown();
}

#[test]
fn a_task_for_an_in_place_project_runs_in_the_directory_without_a_worktree_or_commit() {
    let fixture = Fixture::new();
    let home = fs::canonicalize(fixture.root.path()).unwrap().join("home");
    fs::create_dir_all(&home).unwrap();
    let record = fixture.marker("argv");
    let event = r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"Wrote the notes."}}"#;
    let agent = fixture.script(
        "in-place-agent",
        &format!(
            "printf '%s\\n' \"$@\" > '{0}.tmp' && mv '{0}.tmp' '{0}'\n\
             {THREAD_STARTED}echo produced > agent-output.txt\nprintf '%s\\n' '{event}'\n{TURN_COMPLETED}",
            record.display()
        ),
    );
    let api = fixture.api(&agent);
    let store = SqliteStore::new(&fixture.data().join("store.sqlite")).unwrap();
    store
        .register_project(&Project {
            id: home.display().to_string(),
            root_path: home.display().to_string(),
            settings: json!({"in_place": true}),
        })
        .unwrap();
    let mut contract = launch_contract(&fixture, "codex");
    contract["project_id"] = json!(home.display().to_string());
    let task_id = api
        .dispatch(
            "tasks.submit",
            json!({"idempotency_key": "in-place-1", "contract": contract}),
        )
        .unwrap()["contract"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    wait_state(&api, &task_id, "Succeeded");

    // The agent ran in the directory, and nothing turned it into a repository.
    assert_eq!(
        fs::read_to_string(home.join("agent-output.txt")).unwrap(),
        "produced\n"
    );
    assert!(!home.join(".git").exists());
    assert!(!fixture.data().join("worktrees").exists());
    let argv = fs::read_to_string(&record).unwrap();
    assert!(
        argv.lines().any(|arg| arg == "--skip-git-repo-check"),
        "{argv}"
    );
    let workspace = store
        .list_workspaces()
        .unwrap()
        .into_iter()
        .find(|workspace| workspace.task_id == task_id)
        .unwrap();
    assert_eq!(workspace.path, home.display().to_string());
    assert!(workspace.is_in_place());
    assert_eq!(
        (workspace.branch.as_str(), workspace.base_commit.as_str()),
        ("", "")
    );
    // Nothing to review: the last message is the record of what the task did.
    let report = store.task_report(&task_id).unwrap().expect("a report");
    assert!(report.body.contains("Wrote the notes."), "{}", report.body);
    api.shutdown();
}

#[test]
fn a_task_for_an_in_place_project_whose_directory_is_gone_fails_with_a_stable_reason() {
    let fixture = Fixture::new();
    let gone = fixture.root.path().join("gone");
    let api = fixture.api(&succeeding_agent(&fixture));
    SqliteStore::new(&fixture.data().join("store.sqlite"))
        .unwrap()
        .register_project(&Project {
            id: gone.display().to_string(),
            root_path: gone.display().to_string(),
            settings: json!({"in_place": true}),
        })
        .unwrap();
    let mut contract = launch_contract(&fixture, "codex");
    contract["project_id"] = json!(gone.display().to_string());
    let task_id = api
        .dispatch(
            "tasks.submit",
            json!({"idempotency_key": "in-place-2", "contract": contract}),
        )
        .unwrap()["contract"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let failed = wait_state(&api, &task_id, "Failed");
    assert_eq!(
        failed["attempts"][0]["failure_reason"],
        HOME_DIRECTORY_MISSING
    );
    assert_eq!(failed["sessions"], json!([]));
    assert!(!gone.exists());
    api.shutdown();
}

#[test]
fn an_in_place_home_that_became_a_repository_runs_in_a_worktree() {
    let fixture = Fixture::new();
    let home = fs::canonicalize(fixture.root.path()).unwrap().join("home");
    fs::create_dir_all(&home).unwrap();
    let api = fixture.api(&succeeding_agent(&fixture));
    let store = SqliteStore::new(&fixture.data().join("store.sqlite")).unwrap();
    store
        .register_project(&Project {
            id: home.display().to_string(),
            root_path: home.display().to_string(),
            settings: json!({"in_place": true}),
        })
        .unwrap();
    // `git init` runs after the flag was set.
    run_git(&home, &["init", "--initial-branch=main"]);
    run_git(&home, &["config", "user.name", "executor test"]);
    run_git(&home, &["config", "user.email", "executor@example.invalid"]);
    fs::write(home.join("sample.txt"), "base\n").unwrap();
    run_git(&home, &["add", "sample.txt"]);
    run_git(&home, &["commit", "-m", "initial"]);
    let mut contract = launch_contract(&fixture, "codex");
    contract["project_id"] = json!(home.display().to_string());
    let task_id = api
        .dispatch(
            "tasks.submit",
            json!({"idempotency_key": "in-place-3", "contract": contract}),
        )
        .unwrap()["contract"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    wait_state(&api, &task_id, "Succeeded");

    assert!(!store
        .get_project(&home.display().to_string())
        .unwrap()
        .is_in_place());
    let workspace = store
        .list_workspaces()
        .unwrap()
        .into_iter()
        .find(|workspace| workspace.task_id == task_id)
        .unwrap();
    assert!(!workspace.is_in_place());
    assert!(!workspace.branch.is_empty());
    api.shutdown();
}

#[path = "api_models_tests.rs"]
mod model_selection;

#[test]
fn task_thinking_is_stored_and_served_by_sessions_thinking_not_the_output() {
    let fixture = Fixture::new();
    let agent = fixture.script(
        "thinking-agent",
        &format!(
            "{THREAD_STARTED}printf '{{\"type\":\"item.completed\",\"item\":{{\"id\":\"r\",\"type\":\"reasoning\",\"text\":\"weighing options\"}}}}\\n'\n{}echo produced > agent-output.txt\n{TURN_COMPLETED}",
            message_line("done"),
        ),
    );
    let api = fixture.api(&agent);
    let task_id = fixture.submit(&api, "think-1", 0);
    wait_state(&api, &task_id, "Succeeded");
    let session_id = format!("{task_id}-session-1");
    let page = api
        .dispatch("sessions.thinking", json!({"session_id": session_id}))
        .unwrap();
    assert_eq!(page["chunks"].as_array().unwrap().len(), 1, "{page}");
    assert_eq!(page["chunks"][0]["body"], "weighing options");
    assert_eq!(page["truncated"], false);
    let cursor = page["next_cursor"].clone();
    let after = api
        .dispatch(
            "sessions.thinking",
            json!({"session_id": session_id, "after": cursor}),
        )
        .unwrap();
    assert_eq!(after["chunks"], json!([]));
    assert!(
        !tail_texts(&output_tail(&api, &task_id, json!({})).unwrap())
            .iter()
            .any(|line| line.contains("weighing"))
    );
}
