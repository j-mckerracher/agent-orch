use std::sync::{Arc, Barrier};
use std::thread;

use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::Project;
use orchestrator_core::task::{Attempt, AttemptState, Session, SessionState, TaskEvent, TaskState};
use orchestrator_store::{CancellationOwner, CancellationStatus, SqliteStore, Store, StoreError};

fn running_task(store: &SqliteStore, task_id: &str, register_owner: bool) -> CancellationOwner {
    store
        .register_project(&Project {
            id: "project".into(),
            root_path: "/repo".into(),
            settings: serde_json::json!({}),
        })
        .unwrap();
    store
        .create(&TaskContract {
            id: task_id.into(),
            project_id: "project".into(),
            parent_id: None,
            objective: "run task".into(),
            kind: TaskKind::Implement,
            agent: "codex".into(),
            dependencies: Vec::new(),
            verification: VerificationPolicy::Commands {
                commands: vec![vec!["true".into()]],
            },
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 0,
            max_repairs: 0,
        })
        .unwrap();
    store.transition(task_id, TaskEvent::Admit).unwrap();

    let attempt_id = format!("{task_id}-attempt-1");
    let session_id = format!("{task_id}-session-1");
    store
        .add_attempt(
            task_id,
            &Attempt {
                id: attempt_id.clone(),
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
        .update_attempt_state(task_id, &attempt_id, AttemptState::Running)
        .unwrap();
    store
        .add_session(
            task_id,
            &Session {
                id: session_id.clone(),
                attempt_id: attempt_id.clone(),
                adapter: "codex".into(),
                state: SessionState::Spawning,
            },
        )
        .unwrap();
    store
        .update_session_state(task_id, &session_id, SessionState::Running)
        .unwrap();

    let owner = CancellationOwner::new(
        task_id.into(),
        attempt_id,
        session_id,
        "0123456789abcdef0123456789abcdef".into(),
    )
    .unwrap();
    if register_owner {
        store.register_cancellation_owner(&owner).unwrap();
    }
    owner
}

fn assert_running(store: &SqliteStore, task_id: &str) {
    let record = store.get(task_id).unwrap();
    assert_eq!(record.state, TaskState::Running);
    assert_eq!(record.attempts[0].state, AttemptState::Running);
    assert_eq!(record.sessions[0].state, SessionState::Running);
}

#[test]
fn request_stays_pending_until_owner_confirms_reaped_handle() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("store.sqlite");
    let store = SqliteStore::new(&database).unwrap();
    let owner = running_task(&store, "cancel-pending", true);

    assert!(matches!(
        store.confirm_cancellation(&owner),
        Err(StoreError::CancellationConflict { .. })
    ));
    assert_running(&store, "cancel-pending");
    assert_eq!(
        store
            .request_cancellation_for_task("cancel-pending")
            .unwrap(),
        CancellationStatus::Pending
    );
    assert_eq!(
        store
            .cancellation_status_for_task("cancel-pending")
            .unwrap(),
        Some(CancellationStatus::Pending)
    );
    assert_running(&store, "cancel-pending");
    assert!(matches!(
        store.transition("cancel-pending", TaskEvent::CancellationConfirmed),
        Err(StoreError::CancellationConflict { .. })
    ));

    store.confirm_cancellation(&owner).unwrap();
    store.confirm_cancellation(&owner).unwrap();
    let record = store.get("cancel-pending").unwrap();
    assert_eq!(record.state, TaskState::Cancelled);
    assert_eq!(record.attempts[0].state, AttemptState::Cancelled);
    assert_eq!(record.sessions[0].state, SessionState::Exited);
    assert_eq!(
        store.cancellation_request(&owner).unwrap(),
        Some(CancellationStatus::Confirmed)
    );
}

#[test]
fn absent_or_stale_owner_cannot_acknowledge_cancellation() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
    let owner = running_task(&store, "cancel-no-owner", false);

    assert!(matches!(
        store.request_cancellation_for_task("cancel-no-owner"),
        Err(StoreError::NoActiveCancellationOwner { .. })
    ));
    assert_running(&store, "cancel-no-owner");

    let stale = CancellationOwner::new(
        owner.task_id.clone(),
        owner.attempt_id.clone(),
        owner.session_id.clone(),
        "fedcba9876543210fedcba9876543210".into(),
    )
    .unwrap();
    assert!(matches!(
        store.confirm_cancellation(&stale),
        Err(StoreError::NoActiveCancellationOwner { .. })
    ));
    assert_running(&store, "cancel-no-owner");
}

#[test]
fn duplicate_concurrent_requests_share_one_pending_result() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("store.sqlite");
    let store = SqliteStore::new(&database).unwrap();
    let owner = running_task(&store, "cancel-duplicates", true);
    let count = 4;
    let barrier = Arc::new(Barrier::new(count));
    let workers = (0..count)
        .map(|_| {
            let database = database.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let store = SqliteStore::new(&database).unwrap();
                barrier.wait();
                store.request_cancellation_for_task("cancel-duplicates")
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        assert_eq!(worker.join().unwrap().unwrap(), CancellationStatus::Pending);
    }
    assert_running(&store, "cancel-duplicates");
    store.confirm_cancellation(&owner).unwrap();
    assert_eq!(
        store.cancellation_request(&owner).unwrap(),
        Some(CancellationStatus::Confirmed)
    );
}

#[test]
fn crashed_owner_leaves_durable_request_pending_not_cancelled() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("store.sqlite");
    let owner = {
        let store = SqliteStore::new(&database).unwrap();
        let owner = running_task(&store, "cancel-owner-crash", true);
        assert_eq!(
            store
                .request_cancellation_for_task("cancel-owner-crash")
                .unwrap(),
            CancellationStatus::Pending
        );
        owner
    };

    let reopened = SqliteStore::new(&database).unwrap();
    assert_eq!(
        reopened
            .cancellation_status_for_task("cancel-owner-crash")
            .unwrap(),
        Some(CancellationStatus::Pending)
    );
    assert_eq!(
        reopened
            .request_cancellation_for_task("cancel-owner-crash")
            .unwrap(),
        CancellationStatus::Pending
    );
    assert_running(&reopened, "cancel-owner-crash");
    assert!(matches!(
        reopened.finish_cancellation_owner(&owner, CancellationStatus::Confirmed),
        Err(StoreError::CancellationConflict { .. })
    ));
    assert_running(&reopened, "cancel-owner-crash");
}

#[test]
fn non_cancelled_owner_exit_resolves_request_without_cancelling_task() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
    let owner = running_task(&store, "cancel-rejected", true);
    store
        .request_cancellation_for_task("cancel-rejected")
        .unwrap();

    store
        .update_session_state("cancel-rejected", &owner.session_id, SessionState::Exited)
        .unwrap();
    store
        .update_attempt_state("cancel-rejected", &owner.attempt_id, AttemptState::Failed)
        .unwrap();
    store
        .transition("cancel-rejected", TaskEvent::ExecutionFailed)
        .unwrap();
    store
        .finish_cancellation_owner(&owner, CancellationStatus::Rejected)
        .unwrap();

    let record = store.get("cancel-rejected").unwrap();
    assert_eq!(record.state, TaskState::Failed);
    assert_eq!(record.attempts[0].state, AttemptState::Failed);
    assert_eq!(record.sessions[0].state, SessionState::Exited);
    assert_eq!(
        store.cancellation_request(&owner).unwrap(),
        Some(CancellationStatus::Rejected)
    );
}
