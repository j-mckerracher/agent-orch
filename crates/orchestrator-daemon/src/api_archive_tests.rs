//! `tasks.archive` (DPES-1106): an archived task leaves `tasks.list` and the workstream view,
//! `tasks.get` still answers for it, and a task that is not finished refuses the whole call.

use super::actions_task_tests::{register_project, submit_queued, ALPHA};
use super::actions_tests::{call, create_workstream, new_api};
use super::*;
use serde_json::json;

/// A task cancelled while queued: a finished task that archiving accepts.
fn cancelled_task(api: &DaemonApi, key: &str) -> String {
    let id = submit_queued(api, key, "Cancel me");
    call(
        api,
        "tasks.cancel",
        json!({"task_id": id, "idempotency_key": format!("cancel-{key}")}),
    )
    .unwrap();
    id
}

fn listed_ids(api: &DaemonApi) -> Vec<String> {
    call(api, "tasks.list", Value::Null)
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["contract"]["id"].as_str().unwrap().to_owned())
        .collect()
}

fn archive(api: &DaemonApi, ids: &[&str], key: &str) -> Result<Value, RpcError> {
    call(
        api,
        "tasks.archive",
        json!({"task_ids": ids, "idempotency_key": key}),
    )
}

#[test]
fn archived_tasks_leave_the_list_and_tasks_get_still_answers() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let done = cancelled_task(&api, "k-done");
    let open = submit_queued(&api, "k-open", "Still queued");

    assert_eq!(
        archive(&api, &[done.as_str()], "archive-1").unwrap(),
        json!({"archived": [done]})
    );
    assert_eq!(listed_ids(&api), vec![open]);
    assert_eq!(
        call(&api, "tasks.get", json!({"task_id": done})).unwrap()["state"],
        "Cancelled"
    );
}

#[test]
fn archiving_again_is_a_no_op_and_an_unknown_task_is_not_found() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let done = cancelled_task(&api, "k-done");

    archive(&api, &[done.as_str()], "archive-1").unwrap();
    assert_eq!(
        archive(&api, &[done.as_str()], "archive-2").unwrap(),
        json!({"archived": []})
    );
    assert_eq!(
        archive(&api, &["missing"], "archive-3"),
        Err(RpcError::NotFound)
    );
}

#[test]
fn a_task_that_is_not_finished_refuses_the_whole_archive_and_writes_nothing() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let done = cancelled_task(&api, "k-done");
    let queued = submit_queued(&api, "k-queued", "Waiting");

    assert_eq!(
        archive(&api, &[done.as_str(), queued.as_str()], "archive-1"),
        Err(RpcError::Conflict)
    );
    assert_eq!(listed_ids(&api).len(), 2);
    assert!(api.store.archived_task_ids().unwrap().is_empty());
    assert_eq!(
        call(&api, "tasks.get", json!({"task_id": queued})).unwrap()["state"],
        "Queued"
    );
}

#[test]
fn an_archived_task_drops_out_of_its_workstream_view_but_not_the_store_links() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let workstream = create_workstream(&api, "Ship", "w-1");
    let done = cancelled_task(&api, "k-done");
    let open = submit_queued(&api, "k-open", "Still queued");
    for (task, key) in [(&done, "link-done"), (&open, "link-open")] {
        call(
            &api,
            "workstreams.link_task",
            json!({"workstream_id": workstream, "task_id": task, "idempotency_key": key}),
        )
        .unwrap();
    }

    archive(&api, &[done.as_str()], "archive-1").unwrap();

    let got = call(
        &api,
        "workstreams.get",
        json!({"workstream_id": workstream}),
    )
    .unwrap();
    let linked: Vec<&str> = got["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|link| link["task_id"].as_str().unwrap())
        .collect();
    assert_eq!(linked, vec![open.as_str()]);

    // The gates and the scheduler read the store directly, so the link is still there for them.
    assert_eq!(
        api.store.list_workstream_tasks(&workstream).unwrap().len(),
        2
    );
    let overview = api.store.workstream_task_overview(&workstream, 10).unwrap();
    let newest: Vec<&str> = overview
        .newest
        .iter()
        .map(|task| task.id.as_str())
        .collect();
    assert_eq!(newest, vec![open.as_str()]);
}
