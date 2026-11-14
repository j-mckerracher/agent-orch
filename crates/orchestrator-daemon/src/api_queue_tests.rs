use super::*;
use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::task::{Attempt, TaskEvent};
use orchestrator_store::Store;
use serde_json::json;

fn new_api() -> (tempfile::TempDir, DaemonApi) {
    let directory = tempfile::tempdir().unwrap();
    let api = DaemonApi::new(directory.path()).unwrap();
    (directory, api)
}

fn register(api: &DaemonApi, project_id: &str) {
    api.store
        .register_project(&Project {
            id: project_id.into(),
            root_path: format!("/private/{project_id}"),
            settings: json!({}),
        })
        .unwrap();
}

fn create(api: &DaemonApi, task_id: &str, project_id: &str, dependencies: &[&str]) {
    api.store
        .create(&TaskContract {
            id: task_id.into(),
            project_id: project_id.into(),
            parent_id: None,
            objective: "Exercise the queue snapshot".into(),
            kind: TaskKind::Implement,
            agent: "codex".into(),
            dependencies: dependencies.iter().map(|id| (*id).into()).collect(),
            verification: VerificationPolicy::None,
            delivery: DeliveryPolicy::ReportOnly,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 0,
            max_repairs: 0,
            model_selection: None,
        })
        .unwrap();
}

fn queue(api: &DaemonApi, task_id: &str, project_id: &str, dependencies: &[&str]) {
    create(api, task_id, project_id, dependencies);
    api.store.transition(task_id, TaskEvent::Admit).unwrap();
}

fn run(api: &DaemonApi, task_id: &str, project_id: &str) {
    queue(api, task_id, project_id, &[]);
    api.store
        .add_attempt(
            task_id,
            &Attempt {
                id: format!("{task_id}-attempt-1"),
                task_id: task_id.into(),
                attempt_number: 1,
                state: AttemptState::Pending,
            },
        )
        .unwrap();
    api.store
        .transition(task_id, TaskEvent::ExecutionStarted)
        .unwrap();
}

fn snapshot(api: &DaemonApi) -> Value {
    api.dispatch("queue.snapshot", Value::Null).unwrap()
}

fn ids(entries: &Value) -> Vec<&str> {
    entries
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["task_id"].as_str().unwrap())
        .collect()
}

fn entry<'a>(snapshot: &'a Value, task_id: &str) -> &'a Value {
    snapshot["queued"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["task_id"] == task_id)
        .unwrap_or_else(|| panic!("{task_id} is queued"))
}

#[test]
fn empty_daemon_reports_limits_and_nothing_else() {
    let (_directory, api) = new_api();

    let result = snapshot(&api);

    assert_eq!(result["limits"]["max_concurrent_workers"], 2);
    assert_eq!(result["limits"]["max_workers_per_project"], 2);
    assert_eq!(result["running"], json!([]));
    assert_eq!(result["queued"], json!([]));
    assert_eq!(result["dispatch_blocked"], false);
    assert_eq!(
        result["as_of_sequence"],
        api.store.latest_event_sequence().unwrap()
    );
    assert_eq!(result["epoch"], api.store.store_epoch().unwrap());
}

#[test]
fn queue_snapshot_rejects_params() {
    let (_directory, api) = new_api();

    assert!(matches!(
        api.dispatch("queue.snapshot", json!({"extra": 1})),
        Err(RpcError::InvalidParams)
    ));
    assert!(api.dispatch("queue.snapshot", json!({})).is_ok());
}

#[test]
fn queue_snapshot_is_advertised_as_a_capability() {
    let (_directory, api) = new_api();

    assert!(api.capabilities().contains(&Capability::Queue));
}

#[test]
fn global_cap_blocks_queued_tasks_behind_a_full_worker_pool() {
    let (_directory, mut api) = new_api();
    api.config.limits = SchedulerLimits::new(Some(2), Some(5)).unwrap();
    register(&api, "a");
    register(&api, "b");
    run(&api, "run-1", "a");
    run(&api, "run-2", "b");
    queue(&api, "wait-1", "a", &[]);

    let result = snapshot(&api);

    assert_eq!(ids(&result["running"]), ["run-1", "run-2"]);
    let waiting = entry(&result, "wait-1");
    assert_eq!(waiting["waiting_reason"], "global_cap");
    assert_eq!(waiting["position"], 1);
    assert_eq!(waiting["waiting_on"], Value::Null);
}

#[test]
fn project_cap_blocks_only_the_full_project() {
    let (_directory, mut api) = new_api();
    api.config.limits = SchedulerLimits::new(Some(4), Some(1)).unwrap();
    register(&api, "a");
    register(&api, "b");
    run(&api, "run-a", "a");
    queue(&api, "wait-a", "a", &[]);
    queue(&api, "wait-b", "b", &[]);

    let result = snapshot(&api);

    assert_eq!(entry(&result, "wait-a")["waiting_reason"], "project_cap");
    let free = entry(&result, "wait-b");
    assert_eq!(free["waiting_reason"], "other");
    assert_eq!(free["waiting_detail"], "dispatch_pending");
    assert_eq!(free["position"], 1);
    assert_eq!(entry(&result, "wait-a")["position"], 2);
    assert_eq!(ids(&result["queued"]), ["wait-b", "wait-a"]);
}

#[test]
fn dependency_blocked_task_names_its_dependency_and_has_no_position() {
    let (_directory, api) = new_api();
    register(&api, "a");
    queue(&api, "parent", "a", &[]);
    queue(&api, "child", "a", &["parent"]);

    let result = snapshot(&api);

    let child = entry(&result, "child");
    assert_eq!(child["waiting_reason"], "dependency");
    assert_eq!(child["waiting_on"], "parent");
    assert_eq!(child["position"], Value::Null);
    assert_eq!(entry(&result, "parent")["position"], 1);
    assert_eq!(ids(&result["queued"]), ["parent", "child"]);
}

#[test]
fn withheld_start_authorization_waits_for_approval() {
    let (_directory, api) = new_api();
    register(&api, "a");
    queue(&api, "ok", "a", &[]);
    queue(&api, "held", "a", &[]);
    let reading = api.consistent_reading().unwrap();
    let deny_held = |_: &str, task_id: &str| task_id != "held";

    let result = serde_json::to_value(
        super::queue::build_snapshot(
            &reading,
            "epoch".into(),
            SchedulerLimits::default(),
            None,
            false,
            &deny_held,
        )
        .unwrap(),
    )
    .unwrap();

    let blocked = entry(&result, "held");
    assert_eq!(blocked["waiting_reason"], "approval");
    assert_eq!(blocked["position"], Value::Null);
    assert_eq!(entry(&result, "ok")["position"], 1);
}

#[test]
fn restart_gate_marks_startable_tasks_as_blocked() {
    let (_directory, api) = new_api();
    register(&api, "a");
    queue(&api, "gated", "a", &[]);
    let reading = api.consistent_reading().unwrap();

    let result = serde_json::to_value(
        super::queue::build_snapshot(
            &reading,
            "epoch".into(),
            SchedulerLimits::default(),
            None,
            true,
            &|_: &str, _: &str| true,
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(result["dispatch_blocked"], true);
    let gated = entry(&result, "gated");
    assert_eq!(gated["waiting_reason"], "other");
    assert_eq!(gated["waiting_detail"], "dispatch_blocked");
}

#[test]
fn created_tasks_are_not_part_of_the_queue() {
    let (_directory, api) = new_api();
    register(&api, "a");
    create(&api, "draft", "a", &[]);

    let result = snapshot(&api);

    assert_eq!(result["queued"], json!([]));
    assert_eq!(result["running"], json!([]));
}

#[test]
fn up_next_order_round_robins_across_projects() {
    let (_directory, api) = new_api();
    register(&api, "a");
    register(&api, "b");
    queue(&api, "a1", "a", &[]);
    queue(&api, "a2", "a", &[]);
    queue(&api, "b1", "b", &[]);

    let result = snapshot(&api);

    assert_eq!(ids(&result["queued"]), ["a1", "b1", "a2"]);
    assert_eq!(entry(&result, "b1")["position"], 2);
    assert_eq!(entry(&result, "a2")["position"], 3);
}

#[test]
fn snapshot_agrees_with_tasks_list_at_its_sequence() {
    let (_directory, api) = new_api();
    register(&api, "a");
    run(&api, "run-1", "a");
    queue(&api, "wait-1", "a", &[]);
    queue(&api, "wait-2", "a", &[]);

    let result = snapshot(&api);
    let tasks = api.dispatch("tasks.list", Value::Null).unwrap();

    assert_eq!(
        result["as_of_sequence"],
        api.store.latest_event_sequence().unwrap()
    );
    let in_state = |state: &str| {
        let mut found: Vec<&str> = tasks
            .as_array()
            .unwrap()
            .iter()
            .filter(|task| task["state"] == state)
            .map(|task| task["contract"]["id"].as_str().unwrap())
            .collect();
        found.sort_unstable();
        found
    };
    assert_eq!(ids(&result["running"]), in_state("Running"));
    let mut queued = ids(&result["queued"]);
    queued.sort_unstable();
    assert_eq!(queued, in_state("Queued"));
}

fn build(reading: &super::queue::Reading, cursor: Option<&str>, dispatch_blocked: bool) -> Value {
    serde_json::to_value(
        super::queue::build_snapshot(
            reading,
            "epoch".into(),
            SchedulerLimits::default(),
            cursor,
            dispatch_blocked,
            &|_: &str, _: &str| true,
        )
        .unwrap(),
    )
    .unwrap()
}

fn add_workspace(api: &DaemonApi, id: &str, task_id: &str, path: &str) {
    api.store
        .add_workspace(&Workspace {
            id: id.into(),
            task_id: task_id.into(),
            path: path.into(),
            branch: format!("branch-{id}"),
            base_commit: "0".repeat(40),
        })
        .unwrap();
}

#[test]
fn limits_are_reported_in_their_own_wire_fields() {
    let (_directory, mut api) = new_api();
    api.config.limits = SchedulerLimits::new(Some(3), Some(1)).unwrap();

    let result = snapshot(&api);

    assert_eq!(
        result["limits"],
        json!({"max_concurrent_workers": 3, "max_workers_per_project": 1})
    );
}

#[test]
fn fairness_cursor_moves_the_up_next_order_past_the_last_started_project() {
    let (_directory, api) = new_api();
    register(&api, "a");
    register(&api, "b");
    queue(&api, "a1", "a", &[]);
    queue(&api, "a2", "a", &[]);
    queue(&api, "b1", "b", &[]);
    let reading = api.consistent_reading().unwrap();

    let fresh = build(&reading, None, false);
    let after_a = build(&reading, Some("a"), false);

    assert_eq!(ids(&fresh["queued"]), ["a1", "b1", "a2"]);
    assert_eq!(ids(&after_a["queued"]), ["b1", "a1", "a2"]);
    assert_eq!(entry(&after_a, "b1")["position"], 1);
    assert_eq!(entry(&after_a, "a1")["position"], 2);
}

#[test]
fn running_and_queued_are_sorted_whatever_order_the_records_arrive_in() {
    let (_directory, api) = new_api();
    register(&api, "a");
    run(&api, "run-a", "a");
    run(&api, "run-b", "a");
    queue(&api, "q-a", "a", &[]);
    queue(&api, "q-b", "a", &[]);
    queue(&api, "child-a", "a", &["q-a"]);
    queue(&api, "child-z", "a", &["q-a"]);
    let mut reading = api.consistent_reading().unwrap();
    reading.tasks.reverse();

    let result = build(&reading, None, false);

    assert_eq!(ids(&result["running"]), ["run-a", "run-b"]);
    assert_eq!(ids(&result["queued"]), ["q-a", "q-b", "child-a", "child-z"]);
}

#[test]
fn a_retried_task_is_judged_by_its_newest_workspace() {
    let (_directory, api) = new_api();
    register(&api, "a");
    run(&api, "owner", "a");
    add_workspace(&api, "owner-w1", "owner", "/worktrees/shared");
    queue(&api, "retry", "a", &[]);
    add_workspace(&api, "retry-w1", "retry", "/worktrees/old");
    add_workspace(&api, "retry-w2", "retry", "/worktrees/shared");

    let result = snapshot(&api);

    let retry = entry(&result, "retry");
    assert_eq!(retry["waiting_reason"], "other");
    assert_eq!(retry["waiting_detail"], "worktree_owned");
    assert_eq!(retry["waiting_on"], "owner");
    assert_eq!(retry["position"], Value::Null);
}

#[test]
fn verifying_tasks_are_in_neither_list_and_hold_no_capacity() {
    let (_directory, mut api) = new_api();
    api.config.limits = SchedulerLimits::new(Some(1), Some(1)).unwrap();
    register(&api, "a");
    run(&api, "verifying-1", "a");
    api.store
        .update_attempt_state(
            "verifying-1",
            "verifying-1-attempt-1",
            AttemptState::Running,
        )
        .unwrap();
    api.store
        .update_attempt_state(
            "verifying-1",
            "verifying-1-attempt-1",
            AttemptState::CandidateReady,
        )
        .unwrap();
    api.store
        .transition("verifying-1", TaskEvent::CandidateReady)
        .unwrap();
    queue(&api, "next", "a", &[]);

    let result = snapshot(&api);

    assert_eq!(
        api.store.get("verifying-1").unwrap().state,
        TaskState::Verifying
    );
    assert_eq!(result["running"], json!([]));
    assert_eq!(ids(&result["queued"]), ["next"]);
    let next = entry(&result, "next");
    assert_eq!(next["position"], 1);
    assert_eq!(next["waiting_detail"], "dispatch_pending");
}

#[test]
fn consistent_reading_retries_a_moving_journal_and_keeps_the_settled_reading() {
    let mut sequences = [1, 2, 2, 2].into_iter();
    let mut reads = 0;

    let reading = super::queue::settle(
        4,
        || Ok(sequences.next().unwrap()),
        |sequence| {
            reads += 1;
            Ok(sequence)
        },
    )
    .unwrap();

    assert_eq!((reading, reads), (2, 2));
}

#[test]
fn consistent_reading_gives_up_after_its_attempts_with_the_last_reading() {
    let mut sequence = 0;
    let mut reads = 0;

    let reading = super::queue::settle(
        4,
        || {
            sequence += 1;
            Ok(sequence)
        },
        |before| {
            reads += 1;
            Ok(before)
        },
    )
    .unwrap();

    // Calls alternate before/after, so the fourth attempt started at 7.
    assert_eq!((reading, reads), (7, 4));
}

#[test]
fn consistent_reading_reads_once_when_the_journal_is_still() {
    let mut reads = 0;

    super::queue::settle(
        4,
        || Ok(9),
        |_| {
            reads += 1;
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(reads, 1);
}
