use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::Project;
use orchestrator_core::task::{
    Attempt, AttemptState, AuditEvent, Session, SessionState, TaskEvent, WaitReason,
};
use orchestrator_engine::reconcile::reconcile_before_scheduling;
use orchestrator_store::{SqliteStore, Store};
use serde_json::json;
use tempfile::TempDir;

/// A `Running` task with one live session, `s1`.
fn running_task_with_live_session() -> (TempDir, SqliteStore) {
    let temp = TempDir::new().unwrap();
    let store = SqliteStore::new(&temp.path().join("store.sqlite")).unwrap();
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
            verification: VerificationPolicy::None,
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 1,
            max_repairs: 1,
            model_selection: None,
            permission: Default::default(),
            work_item: None,
        })
        .unwrap();
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
    (temp, store)
}

#[test]
fn cancelled_verifying_task_does_not_block_reconcile() {
    let (_temp, store) = running_task_with_live_session();
    store
        .update_session_state("task-1", "s1", SessionState::Exited)
        .unwrap();
    store
        .transition("task-1", TaskEvent::CandidateReady)
        .unwrap();
    store
        .update_attempt_state("task-1", "a1", AttemptState::CandidateReady)
        .unwrap();
    store.cancel_verifying_task("task-1").unwrap();
    assert_eq!(
        store.get("task-1").unwrap().events.last(),
        Some(&AuditEvent::VerificationCancelled)
    );

    reconcile_before_scheduling(&store)
        .expect("a verification cancel needs no run-owner confirmation");
}

#[test]
fn cancelled_waiting_task_does_not_block_reconcile() {
    let (_temp, store) = running_task_with_live_session();
    store
        .enter_waiting("task-1", "a1", "s1", WaitReason::Blocker, &[])
        .unwrap();
    store.cancel_waiting_task("task-1").unwrap();

    reconcile_before_scheduling(&store)
        .expect("a waiting worker has no process, so its cancel needs no run-owner confirmation");
}
