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
        assert_eq!(
            api.dispatch(method, params),
            Err(RpcError::InvalidParams),
            "{method}"
        );
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
        api.dispatch(
            "tasks.submit",
            submit_params("request-1", "different objective")
        ),
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

#[test]
fn session_events_replay_task_and_only_selected_session_events() {
    use orchestrator_core::contract::{
        AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
    };
    use orchestrator_core::records::Project;
    use orchestrator_core::task::{Attempt, AttemptState, Session, SessionState, TaskEvent};

    let (_directory, api) = new_api();
    api.store
        .register_project(&Project {
            id: "project-1".into(),
            root_path: "/unused/project".into(),
            settings: json!({}),
        })
        .unwrap();
    api.store
        .create(&TaskContract {
            id: "task-1".into(),
            project_id: "project-1".into(),
            parent_id: None,
            objective: "Replay session history".into(),
            kind: TaskKind::Implement,
            agent: "codex".into(),
            dependencies: Vec::new(),
            verification: VerificationPolicy::None,
            delivery: DeliveryPolicy::ReportOnly,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 0,
            max_repairs: 0,
        })
        .unwrap();
    api.store.transition("task-1", TaskEvent::Admit).unwrap();
    api.store
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
    api.store
        .transition("task-1", TaskEvent::ExecutionStarted)
        .unwrap();
    api.store
        .update_attempt_state("task-1", "attempt-1", AttemptState::Running)
        .unwrap();
    for session_id in ["session-1", "session-2"] {
        api.store
            .add_session(
                "task-1",
                &Session {
                    id: session_id.into(),
                    attempt_id: "attempt-1".into(),
                    adapter: "codex".into(),
                    state: SessionState::Spawning,
                },
            )
            .unwrap();
        api.store
            .update_session_state("task-1", session_id, SessionState::Running)
            .unwrap();
    }

    let replay = api
        .dispatch(
            "sessions.events",
            json!({"task_id": "task-1", "session_id": "session-1", "after_sequence": 0}),
        )
        .unwrap();
    let events = replay["events"].as_array().unwrap();
    let sequences = events
        .iter()
        .map(|event| event["sequence"].as_i64().unwrap())
        .collect::<Vec<_>>();
    assert!(sequences.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(events.iter().any(|event| event["session_id"].is_null()));
    assert!(events
        .iter()
        .any(|event| event["session_id"] == "session-1"));
    assert!(!events
        .iter()
        .any(|event| event["session_id"] == "session-2"));
    assert_eq!(
        api.dispatch("sessions.list", json!({}))
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let session = api
        .dispatch(
            "sessions.get",
            json!({"task_id": "task-1", "session_id": "session-1"}),
        )
        .unwrap();
    assert_eq!(session["session"]["state"], "Running");

    let cursor = sequences[2];
    let resumed = api
        .dispatch(
            "sessions.events",
            json!({
                "task_id": "task-1",
                "session_id": "session-1",
                "after_sequence": cursor
            }),
        )
        .unwrap();
    assert!(resumed["events"]
        .as_array()
        .unwrap()
        .iter()
        .all(|event| event["sequence"].as_i64().unwrap() > cursor));
}

#[test]
fn queued_task_cancellation_is_idempotent() {
    let (_directory, api) = new_api();
    register_project(&api);
    let submitted = api
        .dispatch(
            "tasks.submit",
            submit_params("submit-key", "Cancel this queued task"),
        )
        .unwrap();
    let task_id = submitted["contract"]["id"].as_str().unwrap();
    let params = json!({"task_id": task_id, "idempotency_key": "cancel-key"});
    let cancelled = api.dispatch("tasks.cancel", params.clone()).unwrap();
    assert_eq!(cancelled["state"], "Cancelled");
    assert_eq!(api.dispatch("tasks.cancel", params).unwrap(), cancelled);
    assert_eq!(
        api.dispatch("tasks.get", json!({"task_id": task_id}))
            .unwrap(),
        cancelled
    );
    assert_eq!(
        api.dispatch("tasks.list", Value::Null)
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn approval_resolution_uses_allowlisted_actor_and_is_idempotent() {
    use orchestrator_core::approval::{
        Actor, ApprovalAction, ApprovalActionKind, ApprovalState, ApproverAllowlist,
        ParameterDigest,
    };
    use orchestrator_core::authorization::ActionKind;
    use orchestrator_core::contract::{
        AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
    };

    let (_directory, api) = new_api();
    register_project(&api);
    api.store
        .create(&TaskContract {
            id: "task-1".into(),
            project_id: "project-1".into(),
            parent_id: None,
            objective: "Approve a report".into(),
            kind: TaskKind::Implement,
            agent: "codex".into(),
            dependencies: Vec::new(),
            verification: VerificationPolicy::None,
            delivery: DeliveryPolicy::ReportOnly,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 0,
            max_repairs: 0,
        })
        .unwrap();
    let allowlist = ApproverAllowlist::new("local-user", Vec::<String>::new()).unwrap();
    let action = ApprovalAction::new(
        ApprovalActionKind::Delivery(ActionKind::ReportPublish),
        ParameterDigest::from_sha256([7; 32]),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "task-1",
    )
    .unwrap();
    api.store
        .request_approval(
            "approval-1",
            &action,
            &Actor::Agent("agent-1".into()),
            &allowlist,
        )
        .unwrap();
    assert_eq!(
        api.dispatch("approvals.list", Value::Null)
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let params = json!({
        "approval_id": "approval-1",
        "decision": "approve",
        "idempotency_key": "approval-key"
    });
    let resolved = api.dispatch("approvals.resolve", params.clone()).unwrap();
    assert_eq!(resolved["state"]["state"], "approved");
    assert_eq!(api.dispatch("approvals.resolve", params).unwrap(), resolved);
    assert!(api
        .dispatch("approvals.list", Value::Null)
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        api.store.get_approval("approval-1").unwrap().state,
        ApprovalState::Approved {
            by: Actor::User("local-user".into())
        }
    );
}

#[path = "api_safety_tests.rs"]
mod safety;
