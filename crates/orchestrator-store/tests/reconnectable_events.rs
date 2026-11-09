use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::Project;
use orchestrator_core::task::{Attempt, AttemptState, Session, SessionState, TaskEvent};
use orchestrator_store::{IdempotencyStart, SqliteStore, Store, StoreError};
use serde_json::json;

fn project() -> Project {
    Project {
        id: "project-1".into(),
        root_path: "/unused/project".into(),
        settings: json!({}),
    }
}

fn contract() -> TaskContract {
    TaskContract {
        id: "task-1".into(),
        project_id: "project-1".into(),
        parent_id: None,
        objective: "Record session events".into(),
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

fn add_session_history(store: &SqliteStore) {
    store.register_project(&project()).unwrap();
    store.create(&contract()).unwrap();
    store.transition("task-1", TaskEvent::Admit).unwrap();
    store
        .add_attempt(
            "task-1",
            &Attempt {
                id: "attempt-1".into(),
                task_id: "task-1".into(),
                attempt_number: 1,
                state: AttemptState::Pending,
            },
        )
        .unwrap();
    store
        .transition("task-1", TaskEvent::ExecutionStarted)
        .unwrap();
    store
        .update_attempt_state("task-1", "attempt-1", AttemptState::Running)
        .unwrap();
    store
        .add_session(
            "task-1",
            &Session {
                id: "session-1".into(),
                attempt_id: "attempt-1".into(),
                adapter: "codex".into(),
                state: SessionState::Spawning,
            },
        )
        .unwrap();
    store
        .update_session_state("task-1", "session-1", SessionState::Running)
        .unwrap();
}

#[test]
fn session_replay_is_global_sequence_ordered_and_reconnectable() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let first_page;
    let all;
    {
        let store = SqliteStore::new(&path).unwrap();
        add_session_history(&store);
        all = store.stream_events_after(0, 100).unwrap();
        assert_eq!(
            all.iter().map(|event| event.sequence).collect::<Vec<_>>(),
            (1..=all.len() as i64).collect::<Vec<_>>()
        );
        assert_eq!(all[0].kind, "task.created");
        let session_events = store
            .session_events_after("task-1", "session-1", 0, 100)
            .unwrap();
        assert_eq!(session_events, all);
        first_page = all[2].sequence;
    }

    let reopened = SqliteStore::new(&path).unwrap();
    assert_eq!(
        reopened.stream_events_after(first_page, 100).unwrap(),
        all.iter()
            .filter(|event| event.sequence > first_page)
            .cloned()
            .collect::<Vec<_>>()
    );
    assert!(matches!(
        reopened.stream_events_after(-1, 10),
        Err(StoreError::InvalidEventCursor)
    ));
}

#[test]
fn idempotency_result_and_parameter_binding_survive_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let fingerprint = "a".repeat(64);
    {
        let store = SqliteStore::new(&path).unwrap();
        assert_eq!(
            store
                .begin_idempotency("key-1", "tasks.submit", &fingerprint)
                .unwrap(),
            IdempotencyStart::Started
        );
        store
            .complete_idempotency(
                "key-1",
                "tasks.submit",
                &fingerprint,
                &json!({"task_id": "task-1"}),
            )
            .unwrap();
    }

    let reopened = SqliteStore::new(&path).unwrap();
    assert_eq!(
        reopened
            .begin_idempotency("key-1", "tasks.submit", &fingerprint)
            .unwrap(),
        IdempotencyStart::Completed(json!({"task_id": "task-1"}))
    );
    assert!(matches!(
        reopened.begin_idempotency("key-1", "tasks.submit", &"b".repeat(64)),
        Err(StoreError::IdempotencyConflict)
    ));
}

#[test]
fn store_epoch_is_stable_across_reopen_and_unique_per_database() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let first = SqliteStore::new(&path).unwrap();
    let epoch = first.store_epoch().unwrap();
    assert_eq!(epoch.len(), 32);
    assert!(epoch.bytes().all(|byte| byte.is_ascii_hexdigit()));
    add_session_history(&first);
    assert_eq!(first.store_epoch().unwrap(), epoch);
    drop(first);
    assert_eq!(
        SqliteStore::new(&path).unwrap().store_epoch().unwrap(),
        epoch
    );

    let other = SqliteStore::new(&directory.path().join("replacement.sqlite")).unwrap();
    assert_ne!(other.store_epoch().unwrap(), epoch);
}
