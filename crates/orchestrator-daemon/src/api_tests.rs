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

fn create_running_task(api: &DaemonApi, task_id: &str) {
    use orchestrator_core::contract::{
        AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
    };
    use orchestrator_core::task::{Attempt, AttemptState, Session, SessionState, TaskEvent};

    api.store
        .create(&TaskContract {
            id: task_id.into(),
            project_id: "project-1".into(),
            parent_id: None,
            objective: "Exercise daemon cancellation".into(),
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
    api.store.transition(task_id, TaskEvent::Admit).unwrap();
    api.store
        .add_attempt(
            task_id,
            &Attempt {
                id: "attempt-1".into(),
                task_id: task_id.into(),
                attempt_number: 1,
                state: AttemptState::Pending,
            },
        )
        .unwrap();
    api.store
        .transition(task_id, TaskEvent::ExecutionStarted)
        .unwrap();
    api.store
        .update_attempt_state(task_id, "attempt-1", AttemptState::Running)
        .unwrap();
    api.store
        .add_session(
            task_id,
            &Session {
                id: "session-1".into(),
                attempt_id: "attempt-1".into(),
                adapter: "codex".into(),
                state: SessionState::Spawning,
            },
        )
        .unwrap();
    api.store
        .update_session_state(task_id, "session-1", SessionState::Running)
        .unwrap();
}

#[cfg(unix)]
struct TestChild(std::process::Child);

#[cfg(unix)]
impl TestChild {
    fn spawn_process_group() -> (Self, orchestrator_core::identity::ProcessIdentity) {
        use std::os::unix::process::CommandExt;

        let child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .expect("spawn cancellation test process");
        let child = Self(child);
        let identity = orchestrator_runtime::reconcile::capture_process_identity(child.0.id())
            .expect("capture cancellation test process identity");
        (child, identity)
    }

    fn is_running(&mut self) -> bool {
        self.0
            .try_wait()
            .expect("observe cancellation test process")
            .is_none()
    }
}

#[cfg(unix)]
impl Drop for TestChild {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
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
fn running_task_cancellation_requests_owner_without_process_identity() {
    use orchestrator_core::task::{AttemptState, SessionState, TaskState};
    use orchestrator_store::{CancellationOwner, CancellationStatus};

    let (_directory, api) = new_api();
    register_project(&api);
    let task_id = "running-with-owner-no-identity";
    create_running_task(&api, task_id);
    let owner = CancellationOwner::new(
        task_id.into(),
        "attempt-1".into(),
        "session-1".into(),
        "0123456789abcdef0123456789abcdef".into(),
    )
    .unwrap();
    api.store.register_cancellation_owner(&owner).unwrap();
    let before = api.store.get(task_id).unwrap();
    let audit_before = api.store.event_journal(0).unwrap();
    let stream_before = api.store.stream_events_after(0, 100).unwrap();

    let result = api.dispatch(
        "tasks.cancel",
        json!({"task_id": task_id, "idempotency_key": "cancel-running-no-identity"}),
    );
    let after = api.store.get(task_id).unwrap();
    let audit_after = api.store.event_journal(0).unwrap();
    let stream_after = api.store.stream_events_after(0, 100).unwrap();
    let cancellation = api.store.cancellation_status_for_task(task_id);
    let response_state = result
        .as_ref()
        .ok()
        .and_then(|value| value["state"].as_str());
    let cancellation_is_pending = matches!(&cancellation, Ok(Some(CancellationStatus::Pending)));

    assert!(
        response_state == Some("Running")
            && before == after
            && after.state == TaskState::Running
            && after.attempts[0].state == AttemptState::Running
            && after.sessions[0].state == SessionState::Running
            && audit_before == audit_after
            && stream_before == stream_after
            && cancellation_is_pending,
        "result={result:?}, task={:?}, attempt={:?}, session={:?}, \
         audit_events={}->{}, durable_events={}->{}, cancellation={cancellation:?}",
        after.state,
        after.attempts[0].state,
        after.sessions[0].state,
        audit_before.len(),
        audit_after.len(),
        stream_before.len(),
        stream_after.len()
    );
}

#[cfg(unix)]
#[test]
fn running_task_cancellation_requests_owner_without_mutating_run_state() {
    use orchestrator_core::task::{AttemptState, SessionState, TaskState};
    use orchestrator_store::{CancellationOwner, CancellationStatus};

    let (_directory, api) = new_api();
    register_project(&api);
    let task_id = "running-with-owner";
    create_running_task(&api, task_id);
    let (mut child, identity) = TestChild::spawn_process_group();
    api.store
        .record_session_identity(task_id, "session-1", &identity)
        .unwrap();
    let owner = CancellationOwner::new(
        task_id.into(),
        "attempt-1".into(),
        "session-1".into(),
        "0123456789abcdef0123456789abcdef".into(),
    )
    .unwrap();
    api.store.register_cancellation_owner(&owner).unwrap();
    let before = api.store.get(task_id).unwrap();
    let audit_before = api.store.event_journal(0).unwrap();
    let stream_before = api.store.stream_events_after(0, 100).unwrap();

    let (result, _) = std::thread::scope(|scope| {
        let (sender, receiver) = std::sync::mpsc::channel();
        let api = &api;
        scope.spawn(move || {
            sender
                .send(api.dispatch(
                    "tasks.cancel",
                    json!({"task_id": task_id, "idempotency_key": "cancel-running"}),
                ))
                .unwrap();
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if child.0.try_wait().unwrap().is_some() {
                let result = receiver
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .expect("cancellation response after reaping process");
                return (result, true);
            }
            if let Ok(result) = receiver.try_recv() {
                return (result, false);
            }
            assert!(
                std::time::Instant::now() < deadline,
                "cancellation did not finish or stop the process"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    });
    let running = child.is_running();
    let after = api.store.get(task_id).unwrap();
    let audit_after = api.store.event_journal(0).unwrap();
    let stream_after = api.store.stream_events_after(0, 100).unwrap();
    let cancellation = api.store.cancellation_status_for_task(task_id);
    let response_state = result
        .as_ref()
        .ok()
        .and_then(|value| value["state"].as_str());
    let cancellation_is_pending = matches!(&cancellation, Ok(Some(CancellationStatus::Pending)));

    assert!(
        response_state == Some("Running")
            && before == after
            && after.state == TaskState::Running
            && after.attempts[0].state == AttemptState::Running
            && after.sessions[0].state == SessionState::Running
            && audit_before == audit_after
            && stream_before == stream_after
            && cancellation_is_pending
            && running,
        "result={result:?}, task={:?}, attempt={:?}, session={:?}, \
         audit_events={}->{}, durable_events={}->{}, \
         cancellation={cancellation:?}, process_running={running}",
        after.state,
        after.attempts[0].state,
        after.sessions[0].state,
        audit_before.len(),
        audit_after.len(),
        stream_before.len(),
        stream_after.len()
    );
}

#[cfg(unix)]
#[test]
fn running_task_without_owner_rejects_cancellation_without_partial_updates() {
    use orchestrator_core::task::TaskState;

    let (_directory, api) = new_api();
    register_project(&api);
    let task_id = "running-without-owner";
    create_running_task(&api, task_id);
    let (mut child, identity) = TestChild::spawn_process_group();
    api.store
        .record_session_identity(task_id, "session-1", &identity)
        .unwrap();
    let before = api.store.get(task_id).unwrap();
    let audit_before = api.store.event_journal(0).unwrap();
    let stream_before = api.store.stream_events_after(0, 100).unwrap();

    let result = api.dispatch(
        "tasks.cancel",
        json!({"task_id": task_id, "idempotency_key": "cancel-without-owner"}),
    );
    let running = child.is_running();
    let after = api.store.get(task_id).unwrap();
    let audit_after = api.store.event_journal(0).unwrap();
    let stream_after = api.store.stream_events_after(0, 100).unwrap();

    assert!(
        matches!(&result, Err(RpcError::Conflict))
            && before == after
            && after.state == TaskState::Running
            && audit_before == audit_after
            && stream_before == stream_after
            && running,
        "result={result:?}, task={:?}, attempt={:?}, session={:?}, \
         audit_events={}->{}, durable_events={}->{}, process_running={running}",
        after.state,
        after.attempts[0].state,
        after.sessions[0].state,
        audit_before.len(),
        audit_after.len(),
        stream_before.len(),
        stream_after.len()
    );
}

#[test]
fn verifying_task_cancellation_is_rejected_without_partial_attempt_update() {
    use orchestrator_core::task::{AttemptState, SessionState, TaskEvent, TaskState};

    let (_directory, api) = new_api();
    register_project(&api);
    let task_id = "verifying-task";
    create_running_task(&api, task_id);
    api.store
        .update_session_state(task_id, "session-1", SessionState::Exited)
        .unwrap();
    api.store
        .update_attempt_state(task_id, "attempt-1", AttemptState::CandidateReady)
        .unwrap();
    api.store
        .transition(task_id, TaskEvent::CandidateReady)
        .unwrap();
    let before = api.store.get(task_id).unwrap();
    let audit_before = api.store.event_journal(0).unwrap();
    let stream_before = api.store.stream_events_after(0, 100).unwrap();

    let result = api.dispatch(
        "tasks.cancel",
        json!({"task_id": task_id, "idempotency_key": "cancel-verifying"}),
    );
    let after = api.store.get(task_id).unwrap();
    let audit_after = api.store.event_journal(0).unwrap();
    let stream_after = api.store.stream_events_after(0, 100).unwrap();

    assert!(
        matches!(&result, Err(RpcError::Conflict))
            && before == after
            && after.state == TaskState::Verifying
            && after.attempts[0].state == AttemptState::CandidateReady
            && after.sessions[0].state == SessionState::Exited
            && audit_before == audit_after
            && stream_before == stream_after,
        "result={result:?}, task={:?}, attempt={:?}, session={:?}, \
         audit_events={}->{}, durable_events={}->{}",
        after.state,
        after.attempts[0].state,
        after.sessions[0].state,
        audit_before.len(),
        audit_after.len(),
        stream_before.len(),
        stream_after.len()
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

#[test]
fn running_task_keeps_daemon_alive_without_connected_clients() {
    use orchestrator_core::task::TaskEvent;

    let (_directory, api) = new_api();
    register_project(&api);
    let queued = api
        .dispatch(
            "tasks.submit",
            submit_params("lifecycle-key", "Keep running"),
        )
        .unwrap();
    assert!(!api.keep_alive());
    let task_id = queued["contract"]["id"].as_str().unwrap();
    api.store
        .transition(task_id, TaskEvent::ExecutionStarted)
        .unwrap();
    assert!(api.keep_alive());
}

#[test]
fn idempotency_fingerprint_normalizes_omitted_optional_fields() {
    let (_directory, api) = new_api();
    let omitted = json!({
        "task_id": "missing",
        "approval_id": "approval-1",
        "verification_run_id": "verification-1",
        "action": "report_only",
        "idempotency_key": "normalized-key"
    });
    assert_eq!(
        api.dispatch("delivery.request", omitted),
        Err(RpcError::NotFound)
    );
    let explicit_null = json!({
        "task_id": "missing",
        "workspace_id": null,
        "approval_id": "approval-1",
        "verification_run_id": "verification-1",
        "action": "report_only",
        "target_ref": null,
        "idempotency_key": "normalized-key"
    });
    assert_eq!(
        api.dispatch("delivery.request", explicit_null),
        Err(RpcError::NotFound)
    );
}

#[path = "api_safety_tests.rs"]
mod safety;
