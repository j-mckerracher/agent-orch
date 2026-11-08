use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::{Project, Workspace};
use orchestrator_store::{SqliteStore, Store};
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
