use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::Project;
use orchestrator_core::task::{Attempt, AttemptState, TaskEvent};
use orchestrator_store::{SqliteStore, Store, StoreError};

fn contract() -> TaskContract {
    TaskContract {
        id: "task-1".into(),
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
        max_retries: 1,
        max_repairs: 1,
        model_selection: None,
    }
}

fn prepare_task(store: &SqliteStore) {
    store
        .register_project(&Project {
            id: "project".into(),
            root_path: "/repo".into(),
            settings: serde_json::json!({}),
        })
        .unwrap();
    store.create(&contract()).unwrap();
    store.transition("task-1", TaskEvent::Admit).unwrap();
}

fn attempt(id: &str, attempt_number: u32, state: AttemptState) -> Attempt {
    Attempt {
        id: id.into(),
        task_id: "task-1".into(),
        attempt_number,
        state,
    }
}

#[test]
fn retry_appends_attempt_and_preserves_failure_after_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("attempts.sqlite");
    {
        let store = SqliteStore::new(&database_path).unwrap();
        prepare_task(&store);
        assert_eq!(
            store.attempts_for_task("task-1").unwrap(),
            Vec::<Attempt>::new()
        );
        store
            .add_attempt("task-1", &attempt("attempt-1", 1, AttemptState::Pending))
            .unwrap();
        store
            .update_attempt_state("task-1", "attempt-1", AttemptState::Running)
            .unwrap();
        store
            .transition("task-1", TaskEvent::ExecutionStarted)
            .unwrap();
        store
            .update_attempt_state("task-1", "attempt-1", AttemptState::Failed)
            .unwrap();
        store
            .transition("task-1", TaskEvent::ExecutionFailed)
            .unwrap();
        store
            .transition("task-1", TaskEvent::RetryRequested)
            .unwrap();
        store
            .add_attempt("task-1", &attempt("attempt-2", 2, AttemptState::Pending))
            .unwrap();
    }

    let store = SqliteStore::new(&database_path).unwrap();
    let history = store.attempts_for_task("task-1").unwrap();
    assert_eq!(
        history,
        vec![
            attempt("attempt-1", 1, AttemptState::Failed),
            attempt("attempt-2", 2, AttemptState::Pending),
        ]
    );
}

#[test]
fn attempt_history_rejects_unknown_task() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(&directory.path().join("attempts.sqlite")).unwrap();
    prepare_task(&store);
    assert!(matches!(
        store.attempts_for_task("missing"),
        Err(StoreError::NotFound(task_id)) if task_id == "missing"
    ));
}
