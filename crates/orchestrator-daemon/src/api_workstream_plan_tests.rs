//! Plan editing of a workstream's tasks: `workstreams.add_task`, `split_task`,
//! `combine_tasks`, `remove_task`, `reassign_task` and `set_task_policy`. The API runs
//! without an executor, so a queued task stays queued.

use orchestrator_store::LineageRelation;

use super::actions_task_tests::{
    failed_task, register_project, running_task, submit_queued, task_count, verifying_task, ALPHA,
};
use super::actions_tests::{call, create_workstream, new_api};
use super::*;

fn link(api: &DaemonApi, workstream: &str, task_id: &str, objective: Option<&str>) {
    call(
        api,
        "workstreams.link_task",
        json!({
            "workstream_id": workstream,
            "task_id": task_id,
            "objective_id": objective,
            "idempotency_key": format!("link-{task_id}"),
        }),
    )
    .unwrap();
}

fn add_objective(api: &DaemonApi, workstream: &str, key: &str) -> String {
    let objective = call(
        api,
        "workstreams.add_objective",
        json!({
            "workstream_id": workstream,
            "title": "Objective",
            "gates": [{"description": "done"}],
            "idempotency_key": key,
        }),
    )
    .unwrap();
    objective["id"].as_str().unwrap().to_owned()
}

/// A workstream with one queued task linked to one objective.
fn plan(api: &DaemonApi) -> (String, String, String) {
    register_project(api, ALPHA);
    let workstream = create_workstream(api, "Plan", "ws");
    let objective = add_objective(api, &workstream, "obj");
    let task = submit_queued(api, "s-1", "Original work");
    link(api, &workstream, &task, Some(&objective));
    (workstream, objective, task)
}

fn ids(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect()
}

fn link_of(api: &DaemonApi, task_id: &str) -> Option<orchestrator_store::WorkstreamTask> {
    api.store.workstream_task_link(task_id).unwrap()
}

fn parent_of(api: &DaemonApi, task_id: &str) -> (String, Option<String>) {
    let lineage = api.store.task_lineage(task_id).unwrap().unwrap();
    assert_eq!(lineage.relation, LineageRelation::RevisedFrom);
    (lineage.parent_task_id, lineage.added_intent)
}

fn split(api: &DaemonApi, workstream: &str, task: &str, key: &str) -> Result<Value, RpcError> {
    call(
        api,
        "workstreams.split_task",
        json!({
            "workstream_id": workstream,
            "task_id": task,
            "objectives": ["First half", "Second half"],
            "idempotency_key": key,
        }),
    )
}

#[test]
fn add_task_admits_and_links_one_task_and_a_replay_returns_the_same_task() {
    let (_directory, api) = new_api();
    let (workstream, objective, _) = plan(&api);
    let request = json!({
        "workstream_id": workstream,
        "objective_id": objective,
        "contract": {
            "project_id": ALPHA,
            "objective": "New work",
            "kind": "Implement",
            "agent": "codex",
            "dependencies": [],
            "verification": "None",
            "delivery": "ReportOnly",
            "autonomy": "WorkspaceWrites",
            "max_retries": 0,
            "max_repairs": 0,
        },
        "idempotency_key": "add-1",
    });
    let added = call(&api, "workstreams.add_task", request.clone()).unwrap();
    let created = ids(&added["created"]);
    assert_eq!(created.len(), 1);
    assert_eq!(added["replaced"], json!([]));
    assert_eq!(api.store.get(&created[0]).unwrap().state, TaskState::Queued);
    let linked = link_of(&api, &created[0]).unwrap();
    assert_eq!(linked.workstream_id, workstream);
    assert_eq!(linked.objective_id.as_deref(), Some(objective.as_str()));

    assert_eq!(call(&api, "workstreams.add_task", request).unwrap(), added);
    assert_eq!(task_count(&api), 2);
}

#[test]
fn add_task_to_a_foreign_objective_or_an_archived_workstream_creates_no_task() {
    let (_directory, api) = new_api();
    let (workstream, _, _) = plan(&api);
    let other = create_workstream(&api, "Other", "ws-2");
    let foreign = add_objective(&api, &other, "obj-2");
    let request = |workstream: &str, objective: Option<&str>, key: &str| {
        json!({
            "workstream_id": workstream,
            "objective_id": objective,
            "contract": {
                "project_id": ALPHA,
                "objective": "New work",
                "kind": "Implement",
                "agent": "codex",
                "dependencies": [],
                "verification": "None",
                "delivery": "ReportOnly",
                "autonomy": "WorkspaceWrites",
                "max_retries": 0,
                "max_repairs": 0,
            },
            "idempotency_key": key,
        })
    };
    assert_eq!(
        call(
            &api,
            "workstreams.add_task",
            request(&workstream, Some(&foreign), "a1")
        ),
        Err(RpcError::InvalidParams)
    );
    call(
        &api,
        "workstreams.update",
        json!({
            "workstream_id": other,
            "expected_revision": 1,
            "state": "archived",
            "idempotency_key": "archive",
        }),
    )
    .unwrap();
    assert_eq!(
        call(&api, "workstreams.add_task", request(&other, None, "a2")),
        Err(RpcError::Conflict)
    );
    assert_eq!(task_count(&api), 1);
}

#[test]
fn split_replaces_a_queued_task_with_linked_parts_and_records_lineage() {
    let (_directory, api) = new_api();
    let (workstream, objective, task) = plan(&api);
    let result = split(&api, &workstream, &task, "split-1").unwrap();
    let created = ids(&result["created"]);
    assert_eq!(created.len(), 2);
    assert_eq!(ids(&result["replaced"]), vec![task.clone()]);
    assert_eq!(api.store.get(&task).unwrap().state, TaskState::Cancelled);
    for (index, id) in created.iter().enumerate() {
        let record = api.store.get(id).unwrap();
        assert_eq!(record.state, TaskState::Queued);
        assert_eq!(record.contract.agent, "codex");
        let linked = link_of(&api, id).unwrap();
        assert_eq!(linked.objective_id.as_deref(), Some(objective.as_str()));
        let (parent, note) = parent_of(&api, id);
        assert_eq!(parent, task);
        assert_eq!(
            note.unwrap(),
            format!("split into 2 tasks: part {}", index + 1)
        );
    }
    assert_eq!(
        api.store.get(&created[0]).unwrap().contract.objective,
        "First half"
    );
    // The original stays linked as history.
    assert!(link_of(&api, &task).is_some());

    assert_eq!(split(&api, &workstream, &task, "split-1").unwrap(), result);
    assert_eq!(
        split(&api, &workstream, &task, "split-2"),
        Err(RpcError::Conflict)
    );
    assert_eq!(task_count(&api), 3);
}

#[test]
fn split_refuses_bad_shapes_running_tasks_unlinked_tasks_and_gate_evidence() {
    let (_directory, api) = new_api();
    let (workstream, objective, task) = plan(&api);
    let one_part = call(
        &api,
        "workstreams.split_task",
        json!({
            "workstream_id": workstream,
            "task_id": task,
            "objectives": ["Only one"],
            "idempotency_key": "bad",
        }),
    );
    assert_eq!(one_part, Err(RpcError::InvalidParams));

    let running = running_task(&api, "s-running");
    link(&api, &workstream, &running, None);
    assert_eq!(
        split(&api, &workstream, &running, "k1"),
        Err(RpcError::Conflict)
    );

    let unlinked = submit_queued(&api, "s-unlinked", "Elsewhere");
    assert_eq!(
        split(&api, &workstream, &unlinked, "k2"),
        Err(RpcError::NotFound)
    );

    let failed = failed_task(&api, "s-failed");
    link(&api, &workstream, &failed, Some(&objective));
    call(
        &api,
        "workstreams.set_gate",
        json!({
            "objective_id": objective,
            "expected_revision": 1,
            "gate_id": "gate-1",
            "state": "failed",
            "evidence_task_id": failed,
            "idempotency_key": "gate",
        }),
    )
    .unwrap();
    assert_eq!(
        split(&api, &workstream, &failed, "k3"),
        Err(RpcError::Conflict)
    );
    assert_eq!(task_count(&api), 4);
}

#[test]
fn split_refuses_a_task_an_unfinished_task_depends_on() {
    let (_directory, api) = new_api();
    let (workstream, _, task) = plan(&api);
    call(
        &api,
        "tasks.submit",
        json!({
            "idempotency_key": "dependent",
            "contract": {
                "project_id": ALPHA,
                "objective": "Needs the original",
                "kind": "Implement",
                "agent": "codex",
                "dependencies": [task],
                "verification": "None",
                "delivery": "ReportOnly",
                "autonomy": "WorkspaceWrites",
                "max_retries": 0,
                "max_repairs": 0,
            },
        }),
    )
    .unwrap();
    assert_eq!(
        split(&api, &workstream, &task, "k"),
        Err(RpcError::Conflict)
    );
    assert_eq!(api.store.get(&task).unwrap().state, TaskState::Queued);
}

#[test]
fn combine_merges_tasks_into_one_with_their_outside_dependencies() {
    let (_directory, api) = new_api();
    let (workstream, objective, first) = plan(&api);
    let outside = submit_queued(&api, "s-outside", "Outside work");
    let second = call(
        &api,
        "tasks.submit",
        json!({
            "idempotency_key": "s-2",
            "contract": {
                "project_id": ALPHA,
                "objective": "Second",
                "kind": "Implement",
                "agent": "codex",
                "dependencies": [first, outside],
                "verification": "None",
                "delivery": "ReportOnly",
                "autonomy": "WorkspaceWrites",
                "max_retries": 0,
                "max_repairs": 0,
            },
        }),
    )
    .unwrap()["contract"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    link(&api, &workstream, &second, Some(&objective));

    let combine = |task_ids: Value, key: &str| {
        call(
            &api,
            "workstreams.combine_tasks",
            json!({
                "workstream_id": workstream,
                "task_ids": task_ids,
                "objective": "Both at once",
                "idempotency_key": key,
            }),
        )
    };
    assert_eq!(
        combine(json!([first, first]), "dup"),
        Err(RpcError::InvalidParams)
    );
    let result = combine(json!([first, second]), "c-1").unwrap();
    let created = ids(&result["created"]);
    assert_eq!(created.len(), 1);
    assert_eq!(
        ids(&result["replaced"]),
        vec![first.clone(), second.clone()]
    );
    let record = api.store.get(&created[0]).unwrap();
    assert_eq!(record.contract.objective, "Both at once");
    assert_eq!(record.contract.dependencies, vec![outside]);
    assert_eq!(api.store.get(&first).unwrap().state, TaskState::Cancelled);
    assert_eq!(api.store.get(&second).unwrap().state, TaskState::Cancelled);
    assert_eq!(
        link_of(&api, &created[0]).unwrap().objective_id.as_deref(),
        Some(objective.as_str())
    );
    let (parent, note) = parent_of(&api, &created[0]);
    assert_eq!(parent, first);
    assert_eq!(note.unwrap(), format!("combined from {first}, {second}"));
}

#[test]
fn remove_cancels_and_unlinks_a_queued_task_and_unlinks_a_failed_one() {
    let (_directory, api) = new_api();
    let (workstream, _, task) = plan(&api);
    let remove = |task_id: &str, key: &str| {
        call(
            &api,
            "workstreams.remove_task",
            json!({"workstream_id": workstream, "task_id": task_id, "idempotency_key": key}),
        )
    };
    let removed = remove(&task, "r-1").unwrap();
    assert_eq!(
        removed,
        json!({"task_id": task, "cancelled": true, "removed": true})
    );
    assert_eq!(api.store.get(&task).unwrap().state, TaskState::Cancelled);
    assert!(link_of(&api, &task).is_none());
    assert_eq!(remove(&task, "r-1").unwrap(), removed);
    assert_eq!(remove(&task, "r-2"), Err(RpcError::NotFound));

    let failed = failed_task(&api, "s-failed");
    link(&api, &workstream, &failed, None);
    assert_eq!(
        remove(&failed, "r-3").unwrap(),
        json!({"task_id": failed, "cancelled": false, "removed": true})
    );
    assert_eq!(api.store.get(&failed).unwrap().state, TaskState::Failed);

    let running = running_task(&api, "s-running");
    link(&api, &workstream, &running, None);
    assert_eq!(remove(&running, "r-4"), Err(RpcError::Conflict));
    assert!(link_of(&api, &running).is_some());
}

#[test]
fn remove_cancels_and_unlinks_a_verifying_task() {
    use orchestrator_core::task::AttemptState;

    let (_directory, api) = new_api();
    let (workstream, _, _) = plan(&api);
    let verifying = verifying_task(&api, "s-verifying");
    link(&api, &workstream, &verifying, None);

    let removed = call(
        &api,
        "workstreams.remove_task",
        json!({"workstream_id": workstream, "task_id": verifying, "idempotency_key": "r-v"}),
    );

    assert_eq!(
        removed,
        Ok(json!({"task_id": verifying, "cancelled": true, "removed": true}))
    );
    let task = api.store.get(&verifying).unwrap();
    assert_eq!(task.state, TaskState::Cancelled);
    assert_eq!(task.attempts[0].state, AttemptState::Cancelled);
    assert!(link_of(&api, &verifying).is_none());
}

#[test]
fn reassign_replaces_a_failed_task_with_one_for_the_new_agent() {
    let (_directory, api) = new_api();
    let (workstream, _, _) = plan(&api);
    let failed = failed_task(&api, "s-failed");
    link(&api, &workstream, &failed, None);
    let reassign = |agent: &str, key: &str| {
        call(
            &api,
            "workstreams.reassign_task",
            json!({
                "workstream_id": workstream,
                "task_id": failed,
                "agent": agent,
                "idempotency_key": key,
            }),
        )
    };
    assert_eq!(reassign("codex", "same"), Err(RpcError::InvalidParams));
    assert_eq!(reassign("nobody", "unknown"), Err(RpcError::InvalidParams));
    let result = reassign("claude-code", "re-1").unwrap();
    let created = ids(&result["created"]);
    let record = api.store.get(&created[0]).unwrap();
    assert_eq!(record.contract.agent, "claude-code");
    assert_eq!(record.contract.objective, "Runs now");
    assert_eq!(record.state, TaskState::Queued);
    // A failed task is history already; it is not cancelled.
    assert_eq!(api.store.get(&failed).unwrap().state, TaskState::Failed);
    let (parent, note) = parent_of(&api, &created[0]);
    assert_eq!(parent, failed);
    assert_eq!(note.unwrap(), "reassigned from codex to claude-code");
}

#[test]
fn set_task_policy_replaces_the_task_and_refuses_an_empty_or_unchanged_patch() {
    let (_directory, api) = new_api();
    let (workstream, _, task) = plan(&api);
    let policy = |patch: Value, key: &str| {
        let mut params = json!({
            "workstream_id": workstream,
            "task_id": task,
            "idempotency_key": key,
        });
        params
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        call(&api, "workstreams.set_task_policy", params)
    };
    assert_eq!(policy(json!({}), "empty"), Err(RpcError::InvalidParams));
    assert_eq!(
        policy(json!({"delivery": "ReportOnly", "max_retries": 1}), "same"),
        Err(RpcError::InvalidParams)
    );
    assert_eq!(
        policy(json!({"autonomy": "ReadOnly"}), "read-only"),
        Err(RpcError::Forbidden)
    );
    let result = policy(json!({"delivery": "LocalReview", "max_repairs": 3}), "p-1").unwrap();
    let created = ids(&result["created"]);
    let record = api.store.get(&created[0]).unwrap();
    assert_eq!(record.contract.delivery, DeliveryPolicy::LocalReview);
    assert_eq!(record.contract.max_repairs, 3);
    assert_eq!(record.contract.max_retries, 1);
    assert_eq!(api.store.get(&task).unwrap().state, TaskState::Cancelled);
    assert_eq!(task_count(&api), 2);
}
