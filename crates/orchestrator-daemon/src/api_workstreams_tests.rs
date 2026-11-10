use super::*;
use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::Project;
use orchestrator_store::Store;
use serde_json::json;

fn new_api() -> (tempfile::TempDir, DaemonApi) {
    let directory = tempfile::tempdir().unwrap();
    let api = DaemonApi::new(directory.path()).unwrap();
    (directory, api)
}

fn register_project(api: &DaemonApi, id: &str) {
    api.store
        .register_project(&Project {
            id: id.into(),
            root_path: format!("/private/{id}"),
            settings: json!({}),
        })
        .unwrap();
}

fn create_task(api: &DaemonApi, project: &str, id: &str) {
    api.store
        .create(&TaskContract {
            id: id.into(),
            project_id: project.into(),
            parent_id: None,
            objective: "Workstream-linked task".into(),
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
}

fn call(api: &DaemonApi, method: &str, params: Value) -> Result<Value, RpcError> {
    api.dispatch(method, params)
}

fn create_workstream(api: &DaemonApi, key: &str) -> Value {
    call(
        api,
        "workstreams.create",
        json!({"title": "Ship M7", "summary": "goal", "idempotency_key": key}),
    )
    .unwrap()
}

fn add_objective(api: &DaemonApi, workstream_id: &str, key: &str, gates: Value) -> Value {
    call(
        api,
        "workstreams.add_objective",
        json!({
            "workstream_id": workstream_id,
            "title": "Objective",
            "gates": gates,
            "idempotency_key": key
        }),
    )
    .unwrap()
}

#[test]
fn capability_is_advertised() {
    let (_dir, api) = new_api();
    assert!(api.capabilities().contains(&Capability::Workstreams));
}

#[test]
fn create_get_list_and_update_round_trip() {
    let (_dir, api) = new_api();
    let created = create_workstream(&api, "k1");
    let id = created["id"].as_str().unwrap().to_owned();
    assert_eq!(created["state"], "active");
    assert_eq!(created["revision"], 1);

    let got = call(&api, "workstreams.get", json!({"workstream_id": id})).unwrap();
    assert_eq!(got["workstream"], created);
    assert_eq!(got["objectives"], json!([]));
    assert_eq!(got["tasks"], json!([]));
    assert_eq!(got["project_ids"], json!([]));

    let updated = call(
        &api,
        "workstreams.update",
        json!({
            "workstream_id": id,
            "expected_revision": 1,
            "title": "Renamed",
            "state": "suspended",
            "idempotency_key": "k2"
        }),
    )
    .unwrap();
    assert_eq!(updated["title"], "Renamed");
    assert_eq!(updated["state"], "suspended");
    assert_eq!(updated["revision"], 2);

    let listed = call(&api, "workstreams.list", json!({})).unwrap();
    assert_eq!(listed.as_array().unwrap().len(), 1);
    let active = call(&api, "workstreams.list", json!({"state": "active"})).unwrap();
    assert_eq!(active, json!([]));
}

#[test]
fn stale_revision_is_a_conflict_and_writes_nothing() {
    let (_dir, api) = new_api();
    let id = create_workstream(&api, "k1")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let update = |key: &str, revision: i64| {
        call(
            &api,
            "workstreams.update",
            json!({
                "workstream_id": id,
                "expected_revision": revision,
                "title": format!("Title {key}"),
                "idempotency_key": key
            }),
        )
    };
    update("k2", 1).unwrap();
    assert_eq!(update("k3", 1), Err(RpcError::Conflict));
    let got = call(&api, "workstreams.get", json!({"workstream_id": id})).unwrap();
    assert_eq!(got["workstream"]["title"], "Title k2");
    assert_eq!(got["workstream"]["revision"], 2);
}

#[test]
fn archived_is_terminal_and_illegal_transitions_conflict() {
    let (_dir, api) = new_api();
    let id = create_workstream(&api, "k1")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let set_state = |key: &str, revision: i64, state: &str| {
        call(
            &api,
            "workstreams.update",
            json!({
                "workstream_id": id,
                "expected_revision": revision,
                "state": state,
                "idempotency_key": key
            }),
        )
    };
    set_state("k2", 1, "archived").unwrap();
    assert_eq!(set_state("k3", 2, "active"), Err(RpcError::Conflict));
}

#[test]
fn invalid_parameters_are_rejected() {
    let (_dir, api) = new_api();
    let invalid = [
        ("workstreams.create", json!({"title": "No key"})),
        (
            "workstreams.create",
            json!({"title": "x", "idempotency_key": "k", "extra": 1}),
        ),
        (
            "workstreams.create",
            json!({"title": "   ", "idempotency_key": "k"}),
        ),
        ("workstreams.get", json!({"workstream_id": 4})),
        ("workstreams.list", json!({"state": "bogus"})),
    ];
    for (method, params) in invalid {
        assert_eq!(
            call(&api, method, params),
            Err(RpcError::InvalidParams),
            "{method}"
        );
    }
    assert_eq!(
        call(&api, "workstreams.list", json!({})).unwrap(),
        json!([])
    );
}

#[test]
fn unknown_records_are_not_found() {
    let (_dir, api) = new_api();
    assert_eq!(
        call(&api, "workstreams.get", json!({"workstream_id": "missing"})),
        Err(RpcError::NotFound)
    );
    assert_eq!(
        call(
            &api,
            "workstreams.add_objective",
            json!({"workstream_id": "missing", "title": "t", "idempotency_key": "k"})
        ),
        Err(RpcError::NotFound)
    );
    assert_eq!(
        call(
            &api,
            "workstreams.link_task",
            json!({"workstream_id": "missing", "task_id": "t", "idempotency_key": "k2"})
        ),
        Err(RpcError::NotFound)
    );
}

#[test]
fn idempotent_replay_returns_the_same_result() {
    let (_dir, api) = new_api();
    let params = json!({"title": "Once", "idempotency_key": "same"});
    let first = call(&api, "workstreams.create", params.clone()).unwrap();
    assert_eq!(call(&api, "workstreams.create", params).unwrap(), first);
    assert_eq!(
        call(&api, "workstreams.list", json!({}))
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        call(
            &api,
            "workstreams.create",
            json!({"title": "Different", "idempotency_key": "same"})
        ),
        Err(RpcError::IdempotencyConflict)
    );
}

#[test]
fn objectives_gates_and_evidence_flow() {
    let (_dir, api) = new_api();
    register_project(&api, "project-1");
    create_task(&api, "project-1", "task-1");
    create_task(&api, "project-1", "task-2");
    let workstream_id = create_workstream(&api, "w")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let objective = add_objective(
        &api,
        &workstream_id,
        "o",
        json!([{"description": "first"}, {"description": "second"}]),
    );
    let objective_id = objective["id"].as_str().unwrap().to_owned();
    assert_eq!(objective["gates"][0]["id"], "gate-1");
    assert_eq!(objective["gates"][1]["id"], "gate-2");
    assert_eq!(objective["gates"][0]["state"], "pending");

    let revised = call(
        &api,
        "workstreams.revise_objective",
        json!({
            "objective_id": objective_id,
            "expected_revision": 1,
            "title": "Objective v2",
            "add_gates": [{"description": "third"}],
            "idempotency_key": "r"
        }),
    )
    .unwrap();
    assert_eq!(revised["revision"], 2);
    assert_eq!(revised["gates"][2]["id"], "gate-3");
    assert_eq!(
        call(
            &api,
            "workstreams.revise_objective",
            json!({
                "objective_id": objective_id,
                "expected_revision": 1,
                "title": "stale",
                "idempotency_key": "r-stale"
            })
        ),
        Err(RpcError::Conflict)
    );

    let gate = |key: &str, revision: i64, gate_id: &str, state: &str, evidence: Value| {
        call(
            &api,
            "workstreams.set_gate",
            json!({
                "objective_id": objective_id,
                "expected_revision": revision,
                "gate_id": gate_id,
                "state": state,
                "evidence_task_id": evidence,
                "idempotency_key": key
            }),
        )
    };
    // Evidence must be a task linked to the same workstream.
    assert_eq!(
        gate("g0", 2, "gate-1", "verified", json!("task-1")),
        Err(RpcError::InvalidParams)
    );
    call(
        &api,
        "workstreams.link_task",
        json!({
            "workstream_id": workstream_id,
            "task_id": "task-1",
            "objective_id": objective_id,
            "idempotency_key": "l1"
        }),
    )
    .unwrap();
    assert_eq!(
        gate("g-missing", 2, "gate-9", "verified", Value::Null),
        Err(RpcError::NotFound)
    );
    let verified = gate("g1", 2, "gate-1", "verified", json!("task-1")).unwrap();
    assert_eq!(verified["revision"], 3);
    assert_eq!(verified["gates"][0]["state"], "verified");
    assert_eq!(verified["gates"][0]["evidence_task_id"], "task-1");
    assert_eq!(
        gate("g2", 3, "gate-1", "failed", Value::Null),
        Err(RpcError::Conflict)
    );

    assert_eq!(
        call(
            &api,
            "workstreams.unlink_task",
            json!({
                "workstream_id": workstream_id,
                "task_id": "task-1",
                "idempotency_key": "u-evidence"
            })
        ),
        Err(RpcError::Conflict)
    );
}

#[test]
fn linking_leaves_tasks_untouched_and_spans_projects() {
    let (_dir, api) = new_api();
    register_project(&api, "project-1");
    register_project(&api, "project-2");
    create_task(&api, "project-1", "task-1");
    create_task(&api, "project-2", "task-2");
    let workstream_id = create_workstream(&api, "w")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let other_id = create_workstream(&api, "w2")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let before = call(&api, "tasks.get", json!({"task_id": "task-1"})).unwrap();

    let link = |key: &str, workstream: &str, task: &str| {
        call(
            &api,
            "workstreams.link_task",
            json!({"workstream_id": workstream, "task_id": task, "idempotency_key": key}),
        )
    };
    let linked = link("l1", &workstream_id, "task-1").unwrap();
    assert_eq!(linked["task_id"], "task-1");
    assert_eq!(linked["project_id"], "project-1");
    link("l2", &workstream_id, "task-2").unwrap();
    assert_eq!(
        call(&api, "tasks.get", json!({"task_id": "task-1"})).unwrap(),
        before
    );

    let got = call(
        &api,
        "workstreams.get",
        json!({"workstream_id": workstream_id}),
    )
    .unwrap();
    assert_eq!(got["project_ids"], json!(["project-1", "project-2"]));
    assert_eq!(got["tasks"].as_array().unwrap().len(), 2);

    // A task serves one workstream; an unknown task cannot be linked.
    assert_eq!(link("l3", &other_id, "task-1"), Err(RpcError::Conflict));
    assert_eq!(
        link("l4", &workstream_id, "no-such-task"),
        Err(RpcError::NotFound)
    );

    let removed = call(
        &api,
        "workstreams.unlink_task",
        json!({
            "workstream_id": workstream_id,
            "task_id": "task-2",
            "idempotency_key": "u1"
        }),
    )
    .unwrap();
    assert_eq!(removed, json!({"removed": true}));
    let got = call(
        &api,
        "workstreams.get",
        json!({"workstream_id": workstream_id}),
    )
    .unwrap();
    assert_eq!(got["project_ids"], json!(["project-1"]));
}

#[test]
fn task_submission_is_unchanged_by_the_workstream_model() {
    let (_dir, api) = new_api();
    register_project(&api, "project-1");
    let params = json!({
        "idempotency_key": "submit",
        "contract": {
            "project_id": "project-1",
            "parent_id": null,
            "objective": "Plain task",
            "kind": "Implement",
            "agent": "codex",
            "dependencies": [],
            "verification": "None",
            "delivery": "ReportOnly",
            "autonomy": "WorkspaceWrites",
            "max_retries": 0,
            "max_repairs": 0
        }
    });
    let submitted = call(&api, "tasks.submit", params).unwrap();
    assert_eq!(submitted["contract"]["objective"], "Plain task");
    assert!(submitted.get("workstream_id").is_none());
}
