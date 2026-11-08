use super::*;
use orchestrator_core::records::Project;
use orchestrator_store::Store;
use serde_json::json;

fn new_api() -> (tempfile::TempDir, DaemonApi) {
    let directory = tempfile::tempdir().unwrap();
    let api = DaemonApi::new(directory.path()).unwrap();
    (directory, api)
}

fn register_project(api: &DaemonApi) {
    api.store
        .register_project(&Project {
            id: "project-1".into(),
            root_path: "/private/project-root".into(),
            settings: json!({}),
        })
        .unwrap();
}

fn submit_params(key: &str, objective: &str) -> Value {
    json!({
        "idempotency_key": key,
        "contract": {
            "project_id": "project-1",
            "parent_id": null,
            "objective": objective,
            "kind": "Implement",
            "agent": "codex",
            "dependencies": [],
            "verification": "None",
            "delivery": "ReportOnly",
            "autonomy": "WorkspaceWrites",
            "max_retries": 0,
            "max_repairs": 0
        }
    })
}

#[test]
fn every_rpc_method_rejects_malformed_parameters() {
    let (_directory, api) = new_api();
    let invalid = [
        ("projects.list", json!({"extra": true})),
        ("tasks.submit", json!({"idempotency_key": 4})),
        ("tasks.list", json!({"extra": true})),
        ("tasks.get", json!({"task_id": 4})),
        ("tasks.cancel", json!({"task_id": "task-1"})),
        ("sessions.list", json!({"task_id": 4})),
        ("sessions.get", json!({"task_id": "task-1"})),
        ("sessions.events", json!({"task_id": "task-1"})),
        ("approvals.list", json!({"extra": true})),
        ("approvals.resolve", json!({"approval_id": "approval-1"})),
        ("workspaces.diff", json!({"task_id": 4})),
        ("delivery.request", json!({"task_id": "task-1"})),
    ];
    for (method, params) in invalid {
        assert_eq!(api.dispatch(method, params), Err(RpcError::InvalidParams), "{method}");
    }
}

#[test]
fn task_submission_is_deduplicated_and_hides_project_path() {
    let (directory, api) = new_api();
    register_project(&api);
    let project_list = api.dispatch("projects.list", Value::Null).unwrap();
    assert_eq!(project_list, json!([{"id": "project-1"}]));
    assert!(!project_list.to_string().contains("/private/project-root"));

    let params = submit_params("request-1", "Implement one safe task");
    let first = api.dispatch("tasks.submit", params.clone()).unwrap();
    assert_eq!(first["state"], "Queued");
    assert_eq!(api.dispatch("tasks.submit", params.clone()).unwrap(), first);
    assert_eq!(api.store.list().unwrap().len(), 1);
    assert_eq!(api.store.stream_events_after(0, 20).unwrap().len(), 2);
    assert_eq!(
        api.dispatch("tasks.submit", submit_params("request-1", "different objective")),
        Err(RpcError::IdempotencyConflict)
    );

    drop(api);
    let reopened = DaemonApi::new(directory.path()).unwrap();
    assert_eq!(reopened.dispatch("tasks.submit", params).unwrap(), first);
    assert_eq!(reopened.store.list().unwrap().len(), 1);
}

#[test]
fn missing_task_returns_typed_not_found() {
    let (_directory, api) = new_api();
    assert_eq!(
        api.dispatch("tasks.get", json!({"task_id": "missing"})),
        Err(RpcError::NotFound)
    );
}
