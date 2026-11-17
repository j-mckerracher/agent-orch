//! Execution of the Phase 4d action kinds (DPES-1105): `task.revise`, `decision.hold` and
//! `project.remove`. Every test drives an in-process `DaemonApi` over a real store. The
//! project roots are paths that do not exist (`ALPHA`), so no agent CLI can start; a test
//! that needs one belongs in `tests/conversation_runtime.rs`, which runs the fake agent. The
//! one test that registers a real repository (`registering_the_same_path_clears_the_archive`)
//! never submits a task.

use orchestrator_core::approval::{
    Actor, ApprovalAction, ApprovalActionKind, ApprovalState, ApproverAllowlist, ParameterDigest,
};
use orchestrator_core::authorization::ActionKind as ApprovalKind;
use orchestrator_core::orchestrator_action::{parse_block, BlockScope, ReviseTask};
use orchestrator_store::{
    ConversationTarget, HoldSubject, LineageError, LineageRelation, ProposalState, StoreError,
};

use super::actions::{lineage_write_error, project_code, ActionError};
use super::actions_task_tests::{
    assert_executed, assert_failed, chat_submit, failed_task, proposed_task_id, register_project,
    running_task, submit_queued, task_count, ALPHA,
};
use super::actions_tests::{call, create_workstream, new_api, proposal, propose, resolve};
use super::holds::civil_from_days;
use super::*;

const MS_PER_DAY: i64 = 86_400_000;
const OLD_OBJECTIVE: &str = "Add unit tests for the retry module.";
const INTENT: &str = "Also cover the error path.";

fn revise_action(task_id: &str, intent: &str) -> String {
    json!({"kind": "task.revise", "task_id": task_id, "added_intent": intent}).to_string()
}

fn hold_action(task_id: &str, until: &str) -> String {
    json!({"kind": "decision.hold", "task_id": task_id, "until": until}).to_string()
}

fn remove_action(project: &str) -> String {
    json!({"kind": "project.remove", "project": project}).to_string()
}

/// The UTC day `days` after today, by the clock the daemon uses when no clock is injected.
fn day_number(days: i64) -> i64 {
    i64::try_from(crate::executor::unix_now_ms()).unwrap() / MS_PER_DAY + days
}

fn date_in(days: i64) -> String {
    let (year, month, day) = civil_from_days(day_number(days));
    format!("{year:04}-{month:02}-{day:02}")
}

fn allowlist() -> ApproverAllowlist {
    ApproverAllowlist::new("user-1", ["reviewer-1"]).unwrap()
}

/// A pending approval of `task_id`, as a delivery request makes one.
fn request_approval(api: &DaemonApi, approval_id: &str, task_id: &str) {
    let action = ApprovalAction::new(
        ApprovalActionKind::Delivery(ApprovalKind::LocalReviewMerge),
        ParameterDigest::from_sha256([1; 32]),
        "a".repeat(40),
        task_id,
    )
    .unwrap();
    api.store
        .request_approval(approval_id, &action, &Actor::Agent("agent-1".into()), &allowlist())
        .unwrap();
}

fn cancel(api: &DaemonApi, task_id: &str, key: &str) {
    call(
        api,
        "tasks.cancel",
        json!({"task_id": task_id, "idempotency_key": key}),
    )
    .unwrap();
}

/// A git repository the daemon can register, and the project code its snapshot gives it.
fn register_real_project(api: &DaemonApi) -> (tempfile::TempDir, String, String) {
    let repository = tempfile::tempdir().unwrap();
    let status = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(repository.path())
        .status()
        .unwrap();
    assert!(status.success());
    let registered = call(
        api,
        "projects.register",
        json!({"path": repository.path().to_str().unwrap()}),
    )
    .unwrap();
    let id = registered["id"].as_str().unwrap().to_owned();
    let code = project_code(&id);
    (repository, id, code)
}

fn listed_ids(api: &DaemonApi, params: Value) -> Vec<String> {
    call(api, "projects.list", params)
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|project| project["id"].as_str().unwrap().to_owned())
        .collect()
}

/// Approving a revise stops the old task, makes one new task from the old objective plus
/// the added instruction, links the two, and joins the new task to the old one's workstream.
#[test]
fn approving_task_revise_stops_the_old_task_and_queues_a_linked_new_one() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let workstream = create_workstream(&api, "Ship M7", "w-1");
    let old = submit_queued(&api, "s-1", OLD_OBJECTIVE);
    call(
        &api,
        "workstreams.link_task",
        json!({"workstream_id": workstream, "task_id": old, "idempotency_key": "l-1"}),
    )
    .unwrap();
    let ids = propose(&api, "t1", &revise_action(&old, INTENT));

    let result = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    let new = proposed_task_id(&ids[0]);
    assert_executed(&result, &new);
    assert_eq!(result["proposal"]["target"]["task_title"], OLD_OBJECTIVE);

    // The old task stopped and was kept.
    assert_eq!(api.store.get(&old).unwrap().state, TaskState::Cancelled);
    assert_eq!(task_count(&api), 2);

    // The new objective is the old one, a fixed heading and the instruction. The new task
    // takes the daemon default permission, whatever the old task ran at.
    let task = api.store.get(&new).unwrap();
    assert_eq!(
        task.contract.objective,
        format!("{OLD_OBJECTIVE}\n\nAdded instruction from the user:\n{INTENT}")
    );
    assert_eq!(task.contract.permission, PermissionLevel::Restricted);
    assert_eq!(task.contract.project_id, ALPHA);
    assert_eq!(task.contract.agent, "codex");

    let lineage = api.store.task_lineage(&new).unwrap().unwrap();
    assert_eq!(lineage.parent_task_id, old);
    assert_eq!(lineage.relation, LineageRelation::RevisedFrom);
    assert_eq!(lineage.added_intent.as_deref(), Some(INTENT));
    let link = api.store.workstream_task_link(&new).unwrap().unwrap();
    assert_eq!(link.workstream_id, workstream);

    // The same key replays the answer and makes nothing more.
    assert_eq!(resolve(&api, &ids[0], "approve", "k-1").unwrap(), result);
    assert_eq!(task_count(&api), 2);
}

/// A task that already finished is not revised: nothing is stopped, made or linked.
#[test]
fn a_revise_of_a_finished_task_fails_conflict() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let cancelled = submit_queued(&api, "s-1", "Cancelled work");
    cancel(&api, &cancelled, "c-1");
    let failed = failed_task(&api, "s-2");
    let before = (
        api.store.get(&cancelled).unwrap(),
        api.store.get(&failed).unwrap(),
    );
    let ids = propose(
        &api,
        "t1",
        &[revise_action(&cancelled, INTENT), revise_action(&failed, INTENT)].join(", "),
    );

    for (index, id) in ids.iter().enumerate() {
        let result = resolve(&api, id, "approve", &format!("k-{index}")).unwrap();
        assert_failed(&result, "conflict");
        assert_eq!(api.store.task_lineage(&proposed_task_id(id)).unwrap(), None);
    }
    assert_eq!(api.store.get(&cancelled).unwrap(), before.0);
    assert_eq!(api.store.get(&failed).unwrap(), before.1);
    assert_eq!(task_count(&api), 2);
}

/// A revise of a running task whose owner is not alive cannot ask it to stop, so it fails
/// before the new task is made.
#[test]
fn a_revise_of_a_task_nothing_can_stop_makes_no_new_task() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let running = running_task(&api, "s-1");
    let ids = propose(&api, "t1", &revise_action(&running, INTENT));

    let result = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_failed(&result, "conflict");
    assert_eq!(task_count(&api), 1);
    assert_eq!(api.store.get(&running).unwrap().state, TaskState::Running);
}

/// A stop after the approval committed, before the revise ran. The next start finishes it
/// once, and a second start changes nothing.
#[test]
fn a_restart_between_approve_and_execute_revises_once() {
    let directory = tempfile::tempdir().unwrap();
    let (proposal_id, old) = {
        let api = DaemonApi::new(directory.path()).unwrap();
        register_project(&api, ALPHA);
        let old = submit_queued(&api, "s-1", OLD_OBJECTIVE);
        let ids = propose(&api, "t1", &revise_action(&old, INTENT));
        api.store.approve_action_proposal(&ids[0], "user").unwrap();
        (ids[0].clone(), old)
    };

    let assert_done = |api: &DaemonApi| {
        let new = proposed_task_id(&proposal_id);
        let settled = proposal(api, &proposal_id);
        assert_eq!(settled.state, ProposalState::Executed);
        assert_eq!(settled.result_ref.as_deref(), Some(new.as_str()));
        assert_eq!(api.store.get(&old).unwrap().state, TaskState::Cancelled);
        assert_eq!(task_count(api), 2);
        let lineage = api.store.task_lineage(&new).unwrap().unwrap();
        assert_eq!(lineage.parent_task_id, old);
    };
    {
        let api = DaemonApi::new(directory.path()).unwrap();
        assert_done(&api);
    }
    let api = DaemonApi::new(directory.path()).unwrap();
    assert_done(&api);
}

/// A stop after the new task was made, before the old one was asked to stop or the link was
/// written. The rerun finds the task by its id and does only what is missing.
#[test]
fn a_revise_rerun_after_the_new_task_exists_stops_and_links_once() {
    let directory = tempfile::tempdir().unwrap();
    let (proposal_id, old) = {
        let api = DaemonApi::new(directory.path()).unwrap();
        register_project(&api, ALPHA);
        let old = submit_queued(&api, "s-1", OLD_OBJECTIVE);
        let ids = propose(&api, "t1", &revise_action(&old, INTENT));
        api.store.approve_action_proposal(&ids[0], "user").unwrap();
        let key = idempotency::stable_id("action", &ids[0]);
        {
            let _held = api.idempotency_gate.lock().unwrap();
            api.submit_task_inner(
                &chat_submit(&key, "Old objective plus the instruction"),
                &key,
                None,
            )
            .unwrap();
        }
        (ids[0].clone(), old)
    };

    let api = DaemonApi::new(directory.path()).unwrap();
    let new = proposed_task_id(&proposal_id);
    let settled = proposal(&api, &proposal_id);
    assert_eq!(settled.state, ProposalState::Executed);
    assert_eq!(settled.result_ref.as_deref(), Some(new.as_str()));
    assert_eq!(api.store.get(&old).unwrap().state, TaskState::Cancelled);
    assert_eq!(task_count(&api), 2);
    assert_eq!(
        api.store.task_lineage(&new).unwrap().unwrap().parent_task_id,
        old
    );
}

/// The old task cannot be asked to stop (it runs with no owner to take the request), after
/// the new task was made. The new task is cancelled and the proposal fails, so no state has
/// both tasks live beside a failed proposal.
#[test]
fn a_failed_stop_of_the_old_task_cancels_the_new_one_and_fails_the_revise() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let old = running_task(&api, "s-1");
    let ids = propose(&api, "t1", &revise_action(&old, INTENT));
    // The new task exists already, as after a stop between its making and the old task's stop,
    // so the preflight that would refuse this revise is not run.
    let key = idempotency::stable_id("action", &ids[0]);
    {
        let _held = api.idempotency_gate.lock().unwrap();
        api.submit_task_inner(&chat_submit(&key, "Old objective plus the instruction"), &key, None)
            .unwrap();
    }
    let new = proposed_task_id(&ids[0]);
    assert_eq!(api.store.get(&new).unwrap().state, TaskState::Queued);

    let result = resolve(&api, &ids[0], "approve", "k-1").unwrap();

    assert_failed(&result, "conflict");
    assert_eq!(api.store.get(&new).unwrap().state, TaskState::Cancelled);
    assert_eq!(api.store.get(&old).unwrap().state, TaskState::Running);
    assert_eq!(api.store.task_lineage(&new).unwrap(), None);
}

/// A lineage write that failed for any reason but a refusal is retried by the next start: the
/// old task is already stopped, and the rerun finds the new task by its id.
#[test]
fn a_failed_lineage_write_after_the_stop_leaves_the_revise_for_a_retry() {
    let failed = lineage_write_error(StoreError::Io(std::io::Error::other("disk")));
    assert!(matches!(failed, ActionError::Retry), "{failed:?}");
    let refused = lineage_write_error(LineageError::SelfParent.into());
    assert!(matches!(refused, ActionError::Rpc(RpcError::Conflict)), "{refused:?}");
}

/// The combined objective is checked when the block is stored, so the user is never asked to
/// approve a revise that cannot run.
#[test]
fn a_revise_whose_combined_objective_is_too_long_is_denied_at_the_card() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let nearly_full = "x".repeat(orchestrator_core::contract::limits::MAX_OBJECTIVE_BYTES - 8);
    let old = submit_queued(&api, "s-1", &nearly_full);
    let revise = |added_intent: &str| ReviseTask {
        task_id: old.clone(),
        added_intent: added_intent.to_owned(),
    };

    assert_eq!(
        super::projects_defaults::revise_denial(&api.store, &revise(INTENT)),
        Some(orchestrator_core::task_envelope::Denial::Contract)
    );
    // The same task with an instruction that still fits is not refused.
    let short = submit_queued(&api, "s-2", OLD_OBJECTIVE);
    let fits = ReviseTask { task_id: short, added_intent: INTENT.to_owned() };
    assert_eq!(super::projects_defaults::revise_denial(&api.store, &fits), None);
}

#[test]
fn approving_a_hold_hides_the_decision_until_its_date() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let task = submit_queued(&api, "s-1", "Needs a decision");
    request_approval(&api, "approval-1", &task);
    let until = date_in(3);
    let ids = propose(&api, "t1", &hold_action(&task, &until));

    let result = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_eq!(result["proposal"]["state"], "executed", "{result}");
    let result_ref = result["proposal"]["result_ref"].as_str().unwrap();
    assert!(result_ref.starts_with("hold:"), "{result}");

    // The hold is in force now and ends with the named day, in UTC, with no write.
    let listed = call(&api, "holds.list", json!({})).unwrap();
    assert_eq!(listed["holds"].as_array().unwrap().len(), 1, "{listed}");
    let hold = &listed["holds"][0];
    assert_eq!(hold["subject_kind"], "approval");
    assert_eq!(hold["subject_id"], task.as_str());
    assert_eq!(hold["until_date"], until.as_str());
    assert_eq!(hold["state"], "active");
    assert_eq!(result_ref, format!("hold:{}", hold["id"].as_str().unwrap()));
    let end_of_day = (day_number(3) + 1) * MS_PER_DAY - 1;
    let stored = api.store.active_decision_holds().unwrap();
    assert_eq!(stored[0].until_ms, end_of_day);
    assert_eq!(api.store.decision_holds_in_force(end_of_day - 1).unwrap().len(), 1);
    assert!(api.store.decision_holds_in_force(end_of_day).unwrap().is_empty());

    // A replay of the same key holds once.
    assert_eq!(resolve(&api, &ids[0], "approve", "k-1").unwrap(), result);
    assert_eq!(api.store.active_decision_holds().unwrap().len(), 1);
}

#[test]
fn a_hold_date_outside_the_range_fails_closed() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let task = submit_queued(&api, "s-1", "Needs a decision");
    request_approval(&api, "approval-1", &task);

    // Yesterday, one day past the range and a day that is not on the calendar all fail.
    let outside = [date_in(-1), date_in(61), "2026-02-31".to_owned()];
    for (index, until) in outside.iter().enumerate() {
        let ids = propose(&api, &format!("t-{index}"), &hold_action(&task, until));
        let result = resolve(&api, &ids[0], "approve", &format!("k-{index}")).unwrap();
        assert_failed(&result, "invalid");
    }
    assert!(api.store.active_decision_holds().unwrap().is_empty());

    // The last allowed day, 60 days out, is held.
    let ids = propose(&api, "t-last", &hold_action(&task, &date_in(60)));
    let result = resolve(&api, &ids[0], "approve", "k-last").unwrap();
    assert_eq!(result["proposal"]["state"], "executed", "{result}");
    assert_eq!(api.store.active_decision_holds().unwrap().len(), 1);

    // The direct method answers the same way.
    let refused = call(
        &api,
        "holds.set",
        json!({
            "subject_kind": "approval",
            "subject_id": task,
            "until": date_in(61),
            "idempotency_key": "h-1",
        }),
    );
    assert_eq!(refused, Err(RpcError::InvalidParams));
}

#[test]
fn a_hold_never_approves_or_rejects_anything() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let task = submit_queued(&api, "s-1", "Needs a decision");
    let settled = submit_queued(&api, "s-2", "Already decided");
    let bare = submit_queued(&api, "s-3", "Nothing to decide");
    request_approval(&api, "approval-1", &task);
    request_approval(&api, "approval-2", &settled);
    api.store
        .reject_approval("approval-2", &Actor::User("user-1".into()), &allowlist())
        .unwrap();
    let until = date_in(2);
    let ids = propose(
        &api,
        "t1",
        &[
            hold_action(&task, &until),
            hold_action(&settled, &until),
            hold_action(&bare, &until),
        ]
        .join(", "),
    );

    let held = resolve(&api, &ids[0], "approve", "k-0").unwrap();
    assert_eq!(held["proposal"]["state"], "executed", "{held}");
    // The decision is still open, unchanged and still pending.
    assert_eq!(
        api.store.get_approval("approval-1").unwrap().state,
        ApprovalState::Requested
    );
    assert_eq!(api.store.pending_approvals().unwrap().len(), 1);

    // A settled approval and a task with nothing open cannot be held.
    for (index, id) in ids.iter().enumerate().skip(1) {
        let result = resolve(&api, id, "approve", &format!("k-{index}")).unwrap();
        assert_failed(&result, "not_found");
    }
    assert_eq!(api.store.active_decision_holds().unwrap().len(), 1);
    assert_eq!(
        api.store.get_approval("approval-2").unwrap().state,
        ApprovalState::Rejected
    );
}

/// A removed project takes no new work, and a retry is new work.
#[test]
fn retrying_a_task_of_an_archived_project_is_refused() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let failed = failed_task(&api, "s-1");
    assert!(api.store.archive_project(ALPHA).unwrap());

    let result = call(
        &api,
        "tasks.retry",
        json!({"task_id": failed, "idempotency_key": "r-1"}),
    );

    assert_eq!(result, Err(RpcError::Conflict));
    assert_eq!(api.store.get(&failed).unwrap().state, TaskState::Failed);
}

#[test]
fn approving_project_remove_archives_and_keeps_every_task() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    register_project(&api, "/private/canary-root/beta");
    let cancelled = submit_queued(&api, "s-1", "Cancelled work");
    cancel(&api, &cancelled, "c-1");
    let failed = failed_task(&api, "s-2");
    let tasks_before = (
        api.store.get(&cancelled).unwrap(),
        api.store.get(&failed).unwrap(),
    );
    let code = project_code(ALPHA);
    let ids = propose(&api, "t1", &remove_action(&code));
    let pending = call(&api, "actions.pending", json!({})).unwrap();
    assert_eq!(
        pending["proposals"][0]["target"],
        json!({"project_name": "alpha", "task_count": 2, "running_tasks": 0})
    );

    let result = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_executed(&result, &code);
    assert!(api.store.project_is_archived(ALPHA).unwrap());
    assert!(!api.store.project_is_archived("/private/canary-root/beta").unwrap());

    // Every task and the project row stay. The project is hidden, not deleted.
    assert_eq!(task_count(&api), 2);
    assert_eq!(api.store.get(&cancelled).unwrap(), tasks_before.0);
    assert_eq!(api.store.get(&failed).unwrap(), tasks_before.1);
    assert_eq!(api.store.get_project(ALPHA).unwrap().root_path, ALPHA);
    assert_eq!(
        listed_ids(&api, Value::Null),
        ["/private/canary-root/beta".to_owned()]
    );
    assert_eq!(listed_ids(&api, json!({"include_archived": true})).len(), 2);

    // A removed project takes no new task.
    let refused = call(
        &api,
        "tasks.submit",
        json!({
            "idempotency_key": "s-3",
            "contract": {
                "project_id": ALPHA,
                "parent_id": null,
                "objective": "More work",
                "kind": "Implement",
                "agent": "codex",
                "dependencies": [],
                "verification": "None",
                "delivery": "ReportOnly",
                "autonomy": "WorkspaceWrites",
                "max_retries": 1,
                "max_repairs": 1
            }
        }),
    );
    assert_eq!(refused, Err(RpcError::Conflict));

    // The same key replays, and asking again later is a no-op that still succeeds.
    assert_eq!(resolve(&api, &ids[0], "approve", "k-1").unwrap(), result);
    let again = propose(&api, "t2", &remove_action(&code));
    let result = resolve(&api, &again[0], "approve", "k-2").unwrap();
    assert_executed(&result, &code);
}

#[test]
fn project_remove_fails_while_a_task_is_active() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    register_project(&api, "/private/canary-root/beta");
    let running = running_task(&api, "s-1");
    let decided = failed_task(&api, "s-2");
    let before = api.store.get(&running).unwrap();

    // A running task is never touched, and the project stays.
    let ids = propose(&api, "t1", &remove_action(&project_code(ALPHA)));
    let result = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_failed(&result, "conflict");
    assert!(!api.store.project_is_archived(ALPHA).unwrap());
    assert_eq!(api.store.get(&running).unwrap(), before);

    // A pending approval also stops it. The running task finished first.
    api.store
        .record_attempt_failure(&running, "attempt-1", "attempt failed")
        .unwrap();
    api.store
        .transition(&running, TaskEvent::ExecutionFailed)
        .unwrap();
    request_approval(&api, "approval-1", &decided);
    let again = propose(&api, "t2", &remove_action(&project_code(ALPHA)));
    let result = resolve(&api, &again[0], "approve", "k-2").unwrap();
    assert_failed(&result, "conflict");
    assert!(!api.store.project_is_archived(ALPHA).unwrap());
    assert_eq!(listed_ids(&api, Value::Null).len(), 2);
}

#[test]
fn project_remove_is_named_by_code_and_never_shows_a_path() {
    let (directory, api) = new_api();
    register_project(&api, ALPHA);

    // A path is not a project name: the block is not accepted.
    let held = format!(
        "```mcao-actions\n{{\"mcao_actions\": 1, \"actions\": [{}]}}\n```",
        remove_action(ALPHA)
    );
    assert!(parse_block(&held, BlockScope::Global).is_err());

    let ids = propose(&api, "t1", &remove_action(&project_code(ALPHA)));
    let missing = propose(&api, "t2", &remove_action("p-00000000"));
    let mut seen = String::new();
    seen.push_str(&call(&api, "actions.pending", json!({})).unwrap().to_string());
    let archived = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_executed(&archived, &project_code(ALPHA));
    let unknown = resolve(&api, &missing[0], "approve", "k-2").unwrap();
    assert_failed(&unknown, "not_found");
    seen.push_str(&archived.to_string());
    seen.push_str(&unknown.to_string());
    for stored in api
        .store
        .action_proposals(&ConversationTarget::Global, usize::MAX)
        .unwrap()
    {
        seen.push_str(&format!("{stored:?}"));
    }
    seen.push_str(
        &call(
            &api,
            "conversations.get",
            json!({"target": {"kind": "global"}}),
        )
        .unwrap()
        .to_string(),
    );
    assert!(seen.contains("alpha"), "{seen}");
    let data_directory = directory.path().display().to_string();
    for secret in ["canary-root", data_directory.as_str()] {
        assert!(!seen.contains(secret), "{secret} leaked into {seen}");
    }
}

#[test]
fn registering_the_same_path_clears_the_archive() {
    let (_directory, api) = new_api();
    let (repository, id, code) = register_real_project(&api);
    let ids = propose(&api, "t1", &remove_action(&code));
    let result = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_executed(&result, &code);
    assert!(api.store.project_is_archived(&id).unwrap());
    assert!(listed_ids(&api, Value::Null).is_empty());
    // The repository on disk is untouched.
    assert!(repository.path().join(".git").exists());

    // Registering the same path again is the undo.
    let again = call(
        &api,
        "projects.register",
        json!({"path": repository.path().to_str().unwrap()}),
    )
    .unwrap();
    assert_eq!(again, json!({"id": id}));
    assert!(!api.store.project_is_archived(&id).unwrap());
    assert_eq!(listed_ids(&api, Value::Null), [id]);
}

/// A stopped process left both approved cards. The next start runs each once.
#[test]
fn a_restart_between_approve_and_execute_completes_a_hold_and_a_remove_once() {
    let directory = tempfile::tempdir().unwrap();
    let (ids, task) = {
        let api = DaemonApi::new(directory.path()).unwrap();
        register_project(&api, ALPHA);
        register_project(&api, "/private/canary-root/beta");
        let task = submit_queued(&api, "s-1", "Needs a decision");
        let until = date_in(5);
        request_approval(&api, "approval-1", &task);
        // The task belongs to ALPHA and has a pending approval, so only the hold can run.
        let beta = propose(&api, "t0", &remove_action(&project_code("/private/canary-root/beta")));
        let held = propose(&api, "t1", &hold_action(&task, &until));
        for id in beta.iter().chain(&held) {
            api.store.approve_action_proposal(id, "user").unwrap();
        }
        ([held[0].clone(), beta[0].clone()], task)
    };

    let assert_done = |api: &DaemonApi| {
        for id in &ids {
            assert_eq!(proposal(api, id).state, ProposalState::Executed);
        }
        let holds = api.store.active_decision_holds().unwrap();
        assert_eq!(holds.len(), 1);
        assert_eq!(holds[0].subject_kind, HoldSubject::Approval);
        assert_eq!(holds[0].subject_id, task);
        assert_eq!(
            api.store.archived_project_ids().unwrap(),
            ["/private/canary-root/beta".to_owned()]
        );
    };
    {
        let api = DaemonApi::new(directory.path()).unwrap();
        assert_done(&api);
    }
    let api = DaemonApi::new(directory.path()).unwrap();
    assert_done(&api);
}
