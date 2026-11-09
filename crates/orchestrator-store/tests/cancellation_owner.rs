use std::sync::{Arc, Barrier};
use std::thread;

use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::identity::{ExactProcessIdentity, ProcessIdentity, ProcessPlatform};
use orchestrator_core::records::Project;
use orchestrator_core::task::{
    Attempt, AttemptState, AuditEvent, Session, SessionState, TaskEvent, TaskState,
};
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

fn process_identity(pid: u32, start_time_micros: u64) -> ProcessIdentity {
    ProcessIdentity {
        pid,
        process_group: Some(pid),
        start_time_micros: Some(start_time_micros),
        platform: ProcessPlatform::MacOS,
    }
}

#[test]
fn windows_owner_creation_time_precision_survives_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("store.sqlite");
    let identity = ProcessIdentity {
        pid: 101,
        process_group: None,
        start_time_micros: Some(1_700_000_000_123_456),
        platform: ProcessPlatform::Windows,
    };
    let exact_identity = ExactProcessIdentity {
        identity,
        windows_creation_time_100ns_since_unix_epoch: Some(1_700_000_000_123_456_u64 * 10 + 7),
    };
    let owner = {
        let store = SqliteStore::new(&database).unwrap();
        let owner = running_task(&store, "cancel-windows-identity", false)
            .with_exact_process_identity(exact_identity);
        store.register_cancellation_owner(&owner).unwrap();
        owner
    };

    let reopened = SqliteStore::new(&database).unwrap();
    assert_eq!(
        reopened.open_cancellation_owners().unwrap()[0].exact_process_identity(),
        Some(exact_identity)
    );
    assert_eq!(owner.process_identity(), Some(identity));
    assert_eq!(owner.exact_process_identity(), Some(exact_identity));
}

fn fail_running_owner_task(store: &SqliteStore, owner: &CancellationOwner) {
    store
        .update_session_state(&owner.task_id, &owner.session_id, SessionState::Exited)
        .unwrap();
    store
        .update_attempt_state(&owner.task_id, &owner.attempt_id, AttemptState::Failed)
        .unwrap();
    store
        .transition(&owner.task_id, TaskEvent::ExecutionFailed)
        .unwrap();
}

fn retry_owner(store: &SqliteStore, task_id: &str, identity: ProcessIdentity) -> CancellationOwner {
    store
        .transition(task_id, TaskEvent::RetryRequested)
        .unwrap();
    let attempt_id = format!("{task_id}-attempt-2");
    let session_id = format!("{task_id}-session-2");
    store
        .add_attempt(
            task_id,
            &Attempt {
                id: attempt_id.clone(),
                task_id: task_id.into(),
                attempt_number: 2,
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
    CancellationOwner::new(
        task_id.into(),
        attempt_id,
        session_id,
        "fedcba9876543210fedcba9876543210".into(),
    )
    .unwrap()
    .with_process_identity(identity)
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
fn stale_owner_recovery_resolves_request_without_changing_lifecycle() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
    let owner = running_task(&store, "cancel-stale-owner", false)
        .with_process_identity(process_identity(101, 10_000));
    store.register_cancellation_owner(&owner).unwrap();
    assert_eq!(
        store
            .request_cancellation_for_task("cancel-stale-owner")
            .unwrap(),
        CancellationStatus::Pending
    );
    fail_running_owner_task(&store, &owner);

    store.retire_stale_cancellation_owner(&owner).unwrap();

    let record = store.get("cancel-stale-owner").unwrap();
    assert_eq!(record.state, TaskState::Failed);
    assert_eq!(record.attempts[0].state, AttemptState::Failed);
    assert_eq!(record.sessions[0].state, SessionState::Exited);
    assert!(!record.events.contains(&AuditEvent::TaskCancelled));
    assert_eq!(
        store.cancellation_request(&owner).unwrap(),
        Some(CancellationStatus::Failed)
    );
    assert_eq!(
        store
            .cancellation_status_for_task("cancel-stale-owner")
            .unwrap(),
        Some(CancellationStatus::Failed)
    );
    assert!(store.open_cancellation_owners().unwrap().is_empty());
}

#[test]
fn stale_owner_recovery_requires_the_recorded_process_identity() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
    let identity = process_identity(201, 20_000);
    let owner =
        running_task(&store, "cancel-identity-mismatch", false).with_process_identity(identity);
    store.register_cancellation_owner(&owner).unwrap();
    store
        .request_cancellation_for_task("cancel-identity-mismatch")
        .unwrap();
    let wrong_identity = owner
        .clone()
        .with_process_identity(process_identity(201, 21_000));

    assert!(matches!(
        store.retire_stale_cancellation_owner(&wrong_identity),
        Err(StoreError::CancellationConflict { .. })
    ));
    assert_eq!(
        store.cancellation_request(&owner).unwrap(),
        Some(CancellationStatus::Pending)
    );
    assert_eq!(
        store.open_cancellation_owners().unwrap()[0].process_identity(),
        Some(identity)
    );
    assert_running(&store, "cancel-identity-mismatch");
}

#[test]
fn stale_owner_without_process_identity_is_not_retired() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
    let owner = running_task(&store, "cancel-unknown-owner", true);
    store
        .request_cancellation_for_task("cancel-unknown-owner")
        .unwrap();

    assert!(matches!(
        store.retire_stale_cancellation_owner(&owner),
        Err(StoreError::CancellationConflict { .. })
    ));
    assert_eq!(
        store.cancellation_request(&owner).unwrap(),
        Some(CancellationStatus::Pending)
    );
    assert_eq!(store.open_cancellation_owners().unwrap().len(), 1);
    assert_running(&store, "cancel-unknown-owner");
}

#[test]
fn stale_generation_is_retired_before_retry_owner_can_register() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
    let first_identity = process_identity(301, 30_000);
    let first_owner =
        running_task(&store, "cancel-retry", false).with_process_identity(first_identity);
    store.register_cancellation_owner(&first_owner).unwrap();
    store.request_cancellation_for_task("cancel-retry").unwrap();
    fail_running_owner_task(&store, &first_owner);

    let second_identity = process_identity(302, 31_000);
    let second_owner = retry_owner(&store, "cancel-retry", second_identity);
    assert!(matches!(
        store.register_cancellation_owner(&second_owner),
        Err(StoreError::CancellationConflict { .. })
    ));

    store.retire_stale_cancellation_owner(&first_owner).unwrap();
    store.register_cancellation_owner(&second_owner).unwrap();
    store.retire_stale_cancellation_owner(&first_owner).unwrap();
    assert!(matches!(
        store.confirm_cancellation(&first_owner),
        Err(StoreError::NoActiveCancellationOwner { .. })
    ));

    assert_eq!(
        store.request_cancellation_for_task("cancel-retry").unwrap(),
        CancellationStatus::Pending
    );
    assert_eq!(
        store.cancellation_request(&first_owner).unwrap(),
        Some(CancellationStatus::Failed)
    );
    assert_eq!(
        store.cancellation_request(&second_owner).unwrap(),
        Some(CancellationStatus::Pending)
    );
    let open_owners = store.open_cancellation_owners().unwrap();
    assert_eq!(open_owners.len(), 1);
    assert_eq!(open_owners[0].session_id, second_owner.session_id);
    assert_eq!(open_owners[0].process_identity(), Some(second_identity));
    let record = store.get("cancel-retry").unwrap();
    assert_eq!(record.state, TaskState::Running);
    assert_eq!(record.attempts[0].state, AttemptState::Failed);
    assert_eq!(record.attempts[1].state, AttemptState::Running);
    assert_eq!(record.sessions[0].state, SessionState::Exited);
    assert_eq!(record.sessions[1].state, SessionState::Running);
    assert!(!record.events.contains(&AuditEvent::TaskCancelled));
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

#[test]
fn manual_owner_recovery_is_auditable_and_preserves_lifecycle() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
    let owner = running_task(&store, "cancel-manual-recovery", false);
    store.register_cancellation_owner(&owner).unwrap();
    store
        .request_cancellation_for_task("cancel-manual-recovery")
        .unwrap();
    fail_running_owner_task(&store, &owner);

    let wrong_generation = CancellationOwner::new(
        owner.task_id.clone(),
        owner.attempt_id.clone(),
        owner.session_id.clone(),
        "fedcba9876543210fedcba9876543210".into(),
    )
    .unwrap();
    assert!(store
        .manually_resolve_cancellation_owner(&wrong_generation, true, "confirmed processes exited")
        .is_err());
    assert!(store
        .manually_resolve_cancellation_owner(&owner, false, "confirmed processes exited")
        .is_err());
    assert_eq!(
        store.cancellation_request(&owner).unwrap(),
        Some(CancellationStatus::Pending)
    );
    assert_eq!(store.open_cancellation_owners().unwrap().len(), 1);

    store
        .manually_resolve_cancellation_owner(
            &owner,
            true,
            "confirmed owner and managed process tree exited",
        )
        .unwrap();

    let record = store.get("cancel-manual-recovery").unwrap();
    assert_eq!(record.state, TaskState::Failed);
    assert_eq!(record.attempts[0].state, AttemptState::Failed);
    assert_eq!(record.sessions[0].state, SessionState::Exited);
    assert!(!record.events.contains(&AuditEvent::TaskCancelled));
    assert_eq!(
        store.cancellation_request(&owner).unwrap(),
        Some(CancellationStatus::Failed)
    );
    assert!(store.open_cancellation_owners().unwrap().is_empty());

    let event = store
        .stream_events_after(0, 100)
        .unwrap()
        .into_iter()
        .find(|event| event.kind == "cancellation.owner_manually_released")
        .expect("manual recovery audit event");
    assert_eq!(event.payload["generation"], owner.generation());
    assert_eq!(
        event.payload["reason"],
        "confirmed owner and managed process tree exited"
    );
    assert_eq!(event.payload["process_exit_confirmed_by_operator"], true);
    assert_eq!(event.payload["request_status"], "failed");
}

#[test]
fn manual_owner_recovery_requires_terminal_target_lifecycle() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
    let owner = running_task(&store, "cancel-manual-active", true);
    store
        .request_cancellation_for_task("cancel-manual-active")
        .unwrap();

    assert!(matches!(
        store.manually_resolve_cancellation_owner(
            &owner,
            true,
            "confirmed owner and managed process tree exited"
        ),
        Err(StoreError::CancellationConflict { .. })
    ));
    assert_eq!(
        store.cancellation_request(&owner).unwrap(),
        Some(CancellationStatus::Pending)
    );
    assert_running(&store, "cancel-manual-active");
    assert_eq!(store.open_cancellation_owners().unwrap().len(), 1);
}

#[test]
fn manual_recovery_releases_only_stale_generation_before_successor() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
    let first_owner = running_task(&store, "cancel-manual-retry", false);
    store.register_cancellation_owner(&first_owner).unwrap();
    store
        .request_cancellation_for_task("cancel-manual-retry")
        .unwrap();
    fail_running_owner_task(&store, &first_owner);

    let second_identity = process_identity(402, 40_000);
    let second_owner = retry_owner(&store, "cancel-manual-retry", second_identity);
    assert!(store.register_cancellation_owner(&second_owner).is_err());

    store
        .manually_resolve_cancellation_owner(
            &first_owner,
            true,
            "confirmed first generation and its managed process tree exited",
        )
        .unwrap();
    store.register_cancellation_owner(&second_owner).unwrap();

    assert_eq!(
        store.cancellation_request(&first_owner).unwrap(),
        Some(CancellationStatus::Failed)
    );
    assert_eq!(
        store
            .request_cancellation_for_task("cancel-manual-retry")
            .unwrap(),
        CancellationStatus::Pending
    );
    let record = store.get("cancel-manual-retry").unwrap();
    assert_eq!(record.state, TaskState::Running);
    assert_eq!(record.attempts[0].state, AttemptState::Failed);
    assert_eq!(record.attempts[1].state, AttemptState::Running);
    assert_eq!(record.sessions[0].state, SessionState::Exited);
    assert_eq!(record.sessions[1].state, SessionState::Running);
    assert!(!record.events.contains(&AuditEvent::TaskCancelled));
    let owners = store.open_cancellation_owners().unwrap();
    assert_eq!(owners.len(), 1);
    assert_eq!(owners[0].generation(), second_owner.generation());
}
