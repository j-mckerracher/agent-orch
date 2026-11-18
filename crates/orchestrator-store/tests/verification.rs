use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::{Project, VerificationRun};
use orchestrator_core::task::{
    Attempt, AttemptState, AuditEvent, Session, SessionState, TaskEvent, TaskState,
};
use orchestrator_store::{Settlement, SqliteStore, Store, StoreError};
use serde_json::{json, Value};
use tempfile::TempDir;

fn create_task(store: &SqliteStore) {
    store
        .register_project(&Project {
            id: "project-1".into(),
            root_path: "/repo".into(),
            settings: json!({}),
        })
        .unwrap();
    store
        .create(&TaskContract {
            id: "task-1".into(),
            project_id: "project-1".into(),
            parent_id: None,
            objective: "verify candidate".into(),
            kind: TaskKind::Implement,
            agent: "test-agent".into(),
            dependencies: Vec::new(),
            verification: VerificationPolicy::Commands {
                commands: vec![vec!["cargo".into(), "test".into()]],
            },
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 1,
            max_repairs: 1,
            model_selection: None,
            permission: Default::default(),
            work_item: None,
        })
        .unwrap();
}

fn run(id: &str, commit: &str, status: &str) -> VerificationRun {
    VerificationRun {
        id: id.into(),
        task_id: "task-1".into(),
        commit: commit.into(),
        status: status.into(),
        log_path: None,
    }
}

fn details(check: &str, trust: &str) -> Value {
    json!({
        "checks": [{"name": "candidate", "outcome": check}],
        "commands": [{
            "trust": trust,
            "status": check,
            "exit_code": if check == "passed" { Some(0) } else { None },
            "stdout": "captured output",
        }],
    })
}

#[test]
fn appends_and_reopens_results_without_overwriting_failed_runs() {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("store.sqlite");
    let failed_run = run("verification-1", "candidate-a", "failed");
    let failed_details = details("rejected", "denied");
    let passed_run = run("verification-2", "candidate-b", "passed");
    let passed_details = details("passed", "trusted");

    {
        let store = SqliteStore::new(&database).unwrap();
        create_task(&store);
        store
            .record_verification_result(&failed_run, 100, 200, &failed_details)
            .unwrap();
        let attempted_overwrite = run("verification-1", "candidate-changed", "passed");
        assert!(store
            .record_verification_result(
                &attempted_overwrite,
                300,
                400,
                &details("passed", "trusted"),
            )
            .is_err());
        store
            .record_verification_result(&passed_run, 300, 400, &passed_details)
            .unwrap();
    }

    let reopened = SqliteStore::new(&database).unwrap();
    let results = reopened.verification_results_for_task("task-1").unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, failed_run);
    assert_eq!(results[0].1["details"], failed_details);
    assert_eq!(results[0].1["started_at_ms"], json!(100));
    assert_eq!(results[0].1["finished_at_ms"], json!(200));
    assert_eq!(results[1].0, passed_run);
    assert_eq!(results[1].1["details"], passed_details);
    assert_eq!(results[1].1["started_at_ms"], json!(300));
    assert_eq!(results[1].1["finished_at_ms"], json!(400));
}

/// Moves `task-1` from `Created` to `Verifying` with attempt `a1` at `CandidateReady`.
fn drive_to_verifying(store: &SqliteStore) {
    store.transition("task-1", TaskEvent::Admit).unwrap();
    store
        .add_attempt(
            "task-1",
            &Attempt {
                id: "a1".into(),
                task_id: "task-1".into(),
                attempt_number: 1,
                state: AttemptState::Pending,
                failure_reason: None,
                summary: None,
                changed_files: None,
            },
        )
        .unwrap();
    store
        .transition("task-1", TaskEvent::ExecutionStarted)
        .unwrap();
    store
        .update_attempt_state("task-1", "a1", AttemptState::Running)
        .unwrap();
    store
        .transition("task-1", TaskEvent::CandidateReady)
        .unwrap();
    store
        .update_attempt_state("task-1", "a1", AttemptState::CandidateReady)
        .unwrap();
}

#[test]
fn passing_settlement_succeeds_the_attempt_and_records_the_run() {
    let temp = TempDir::new().unwrap();
    let store = SqliteStore::new(&temp.path().join("store.sqlite")).unwrap();
    create_task(&store);
    drive_to_verifying(&store);
    let passed_run = run("verification-1", "candidate-a", "passed");
    let passed_details = details("passed", "trusted");

    let settlement = store
        .settle_verification("task-1", "a1", &passed_run, 100, 200, &passed_details, None)
        .unwrap();

    assert_eq!(settlement, Settlement::Settled { passed: true });
    let record = store.get("task-1").unwrap();
    assert_eq!(record.state, TaskState::Succeeded);
    assert_eq!(record.events.last(), Some(&AuditEvent::VerificationPassed));
    let attempts = store.attempts_for_task("task-1").unwrap();
    assert_eq!(attempts[0].state, AttemptState::Succeeded);
    assert_eq!(attempts[0].failure_reason, None);
    let results = store.verification_results_for_task("task-1").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, passed_run);
    assert_eq!(results[0].1["details"], passed_details);
    assert_eq!(results[0].1["started_at_ms"], json!(100));
    assert_eq!(results[0].1["finished_at_ms"], json!(200));
}

#[test]
fn failing_settlement_fails_the_attempt_with_its_reason() {
    let temp = TempDir::new().unwrap();
    let store = SqliteStore::new(&temp.path().join("store.sqlite")).unwrap();
    create_task(&store);
    drive_to_verifying(&store);
    let failed_run = run("verification-1", "candidate-a", "failed");

    let settlement = store
        .settle_verification(
            "task-1",
            "a1",
            &failed_run,
            100,
            200,
            &details("rejected", "trusted"),
            Some("cargo test exited 101"),
        )
        .unwrap();

    assert_eq!(settlement, Settlement::Settled { passed: false });
    let record = store.get("task-1").unwrap();
    assert_eq!(record.state, TaskState::VerificationFailed);
    assert_eq!(record.events.last(), Some(&AuditEvent::VerificationFailed));
    let attempts = store.attempts_for_task("task-1").unwrap();
    assert_eq!(attempts[0].state, AttemptState::Failed);
    assert_eq!(
        attempts[0].failure_reason.as_deref(),
        Some("cargo test exited 101")
    );
    let results = store.verification_results_for_task("task-1").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, failed_run);
}

#[test]
fn settlement_after_cancel_is_superseded_and_writes_nothing() {
    let temp = TempDir::new().unwrap();
    let store = SqliteStore::new(&temp.path().join("store.sqlite")).unwrap();
    create_task(&store);
    drive_to_verifying(&store);
    store.cancel_verifying_task("task-1").unwrap();
    let late_run = run("verification-1", "candidate-a", "passed");

    let settlement = store
        .settle_verification(
            "task-1",
            "a1",
            &late_run,
            100,
            200,
            &details("passed", "trusted"),
            None,
        )
        .unwrap();

    assert_eq!(settlement, Settlement::Superseded);
    let record = store.get("task-1").unwrap();
    assert_eq!(record.state, TaskState::Cancelled);
    assert_eq!(
        record.events.last(),
        Some(&AuditEvent::VerificationCancelled)
    );
    let attempts = store.attempts_for_task("task-1").unwrap();
    assert_eq!(attempts[0].state, AttemptState::Cancelled);
    assert!(store
        .verification_results_for_task("task-1")
        .unwrap()
        .is_empty());
}

#[test]
fn cancel_verifying_task_requires_verifying_and_no_live_session() {
    let temp = TempDir::new().unwrap();
    let store = SqliteStore::new(&temp.path().join("store.sqlite")).unwrap();
    create_task(&store);
    assert!(matches!(
        store.cancel_verifying_task("task-1"),
        Err(StoreError::IllegalTransition(_))
    ));

    drive_to_verifying(&store);
    store
        .add_session(
            "task-1",
            &Session {
                id: "s1".into(),
                attempt_id: "a1".into(),
                adapter: "test-adapter".into(),
                state: SessionState::Spawning,
            },
        )
        .unwrap();
    assert!(matches!(
        store.cancel_verifying_task("task-1"),
        Err(StoreError::CancellationConflict { .. })
    ));
    assert_eq!(store.get("task-1").unwrap().state, TaskState::Verifying);

    store
        .update_session_state("task-1", "s1", SessionState::Exited)
        .unwrap();
    store.cancel_verifying_task("task-1").unwrap();
    assert_eq!(store.get("task-1").unwrap().state, TaskState::Cancelled);
    assert!(matches!(
        store.cancel_verifying_task("task-1"),
        Err(StoreError::IllegalTransition(_))
    ));
}
