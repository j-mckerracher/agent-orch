use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::identity::{
    plan_process_reconciliation, ProcessAction, ProcessClassification, ProcessIdentity,
    ProcessObservation, ProcessPlatform,
};
use orchestrator_core::records::{Project, Workspace};
use orchestrator_core::task::{Attempt, AttemptState, Session, SessionState, TaskEvent};
use orchestrator_store::{SqliteStore, Store, StoreError};
use tempfile::TempDir;

fn project() -> Project {
    Project {
        id: "project".into(),
        root_path: "/repo".into(),
        settings: serde_json::json!({}),
    }
}

fn contract(id: &str) -> TaskContract {
    TaskContract {
        id: id.into(),
        project_id: "project".into(),
        parent_id: None,
        objective: "reconcile workspace".into(),
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
    }
}

fn open_store(directory: &TempDir) -> SqliteStore {
    let store = SqliteStore::new(&directory.path().join("store.sqlite")).expect("open SQLite");
    store
        .register_project(&project())
        .expect("register project");
    store
        .create(&contract("task-1"))
        .expect("create first task");
    store
        .create(&contract("task-2"))
        .expect("create second task");
    store
}

fn add_session(store: &SqliteStore, task_id: &str) {
    store
        .transition(task_id, TaskEvent::Admit)
        .expect("admit task");
    let attempt_id = format!("{task_id}-attempt");
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
        .expect("add attempt");
    store
        .add_session(
            task_id,
            &Session {
                id: format!("{task_id}-session"),
                attempt_id,
                adapter: "codex".into(),
                state: SessionState::Spawning,
            },
        )
        .expect("add session");
}

fn sample_identity() -> ProcessIdentity {
    ProcessIdentity {
        pid: std::process::id(),
        process_group: Some(std::process::id()),
        start_time_micros: Some(1_700_000_000_123_456),
        platform: ProcessPlatform::MacOS,
    }
}

fn current_platform() -> ProcessPlatform {
    if cfg!(target_os = "macos") {
        ProcessPlatform::MacOS
    } else if cfg!(target_os = "windows") {
        ProcessPlatform::Windows
    } else if cfg!(unix) {
        ProcessPlatform::Unix
    } else {
        ProcessPlatform::Other
    }
}

#[test]
fn list_workspaces_returns_all_rows_in_stable_order() {
    let directory = tempfile::tempdir().expect("unique SQLite directory");
    let store = open_store(&directory);
    let second = Workspace {
        id: "workspace-b".into(),
        task_id: "task-2".into(),
        path: "/worktrees/b".into(),
        branch: "task/b".into(),
        base_commit: "b".repeat(40),
    };
    let first = Workspace {
        id: "workspace-a".into(),
        task_id: "task-1".into(),
        path: "/worktrees/a".into(),
        branch: "task/a".into(),
        base_commit: "a".repeat(40),
    };
    store
        .add_workspace(&second)
        .expect("write second workspace");
    store.add_workspace(&first).expect("write first workspace");

    assert_eq!(
        store.list_workspaces().expect("list workspaces"),
        [first, second]
    );
}

#[test]
fn session_identity_round_trips_after_reopen() {
    let directory = tempfile::tempdir().expect("unique SQLite directory");
    let path = directory.path().join("store.sqlite");
    let store = open_store(&directory);
    add_session(&store, "task-1");
    let identity = sample_identity();
    store
        .record_session_identity("task-1", "task-1-session", &identity)
        .expect("record identity");
    drop(store);

    let reopened = SqliteStore::new(&path).expect("reopen SQLite");
    assert_eq!(
        reopened
            .session_identity("task-1", "task-1-session")
            .expect("load identity"),
        Some(identity)
    );
}

#[test]
fn identity_for_unknown_session_is_rejected() {
    let directory = tempfile::tempdir().expect("unique SQLite directory");
    let store = open_store(&directory);

    assert!(matches!(
        store.record_session_identity("task-1", "missing", &sample_identity()),
        Err(StoreError::NoSession {
            task_id,
            session_id
        }) if task_id == "task-1" && session_id == "missing"
    ));
}

#[test]
fn conflicting_identity_does_not_replace_first_record() {
    let directory = tempfile::tempdir().expect("unique SQLite directory");
    let store = open_store(&directory);
    add_session(&store, "task-1");
    let original = sample_identity();
    store
        .record_session_identity("task-1", "task-1-session", &original)
        .expect("record identity");
    store
        .record_session_identity("task-1", "task-1-session", &original)
        .expect("repeat identity without replacement");
    let conflicting = ProcessIdentity {
        start_time_micros: Some(1_700_000_000_123_457),
        ..original
    };

    assert!(store
        .record_session_identity("task-1", "task-1-session", &conflicting)
        .is_err());
    assert_eq!(
        store
            .session_identity("task-1", "task-1-session")
            .expect("load identity"),
        Some(original)
    );
}

#[cfg(unix)]
#[test]
fn real_sqlite_process_exit_is_interrupted_not_succeeded() {
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    let directory = tempfile::tempdir().expect("unique SQLite directory");
    let store = open_store(&directory);
    add_session(&store, "task-1");
    let mut child = Command::new("/bin/sleep")
        .arg("0.5")
        .process_group(0)
        .spawn()
        .expect("spawn short-lived process");
    let identity = ProcessIdentity {
        pid: child.id(),
        process_group: Some(child.id()),
        start_time_micros: None,
        platform: current_platform(),
    };
    store
        .record_session_identity("task-1", "task-1-session", &identity)
        .expect("record child identity");
    assert!(child.wait().expect("wait for child").success());

    let recorded = store
        .session_identity("task-1", "task-1-session")
        .expect("load child identity")
        .expect("identity recorded");
    let plan = plan_process_reconciliation(recorded, ProcessObservation::Exited);

    assert_eq!(plan.classification, ProcessClassification::Exited);
    assert_eq!(plan.action, ProcessAction::MarkInterrupted);
}
