//! Execution of the four task action kinds (DPES-1105): `task.propose`, `task.cancel`,
//! `task.retry` and `workstream.link_task`. Every test drives an in-process `DaemonApi`
//! over a real store. `DaemonApi::new` starts the executor thread, so a task admitted here
//! is picked up by it. These tests never launch an agent CLI because every project root they
//! register is a path that does not exist (`ALPHA` and the like): the launch fails while it
//! prepares the worktree, before any agent process starts. Keep it that way: a test that needs
//! a real repository belongs in `tests/conversation_runtime.rs`, which runs the fake agent.

use orchestrator_core::task::{Attempt, AuditEvent};
use orchestrator_store::{ConversationTarget, ProposalState};

use super::actions::project_code;
use super::actions_tests::{
    call, create_workstream, new_api, proposal, propose, propose_in, resolve,
};
use super::workstreams::LinkTaskParams;
use super::*;

/// The project every test registers. Its id is a repository path, so the leak test can
/// look for the path text.
pub(super) const ALPHA: &str = "/private/canary-root/alpha";
const OBJECTIVE: &str = "Add unit tests for the retry module.";
const CREATE: &str = r#"{"kind": "workstream.create", "ref": "w1", "title": "Alpha"}"#;

pub(super) fn register_project(api: &DaemonApi, id: &str) {
    api.store
        .register_project(&Project {
            id: id.to_owned(),
            root_path: id.to_owned(),
            settings: json!({}),
        })
        .unwrap();
}

/// A `task.propose` action. `extra` is more fields, each with a leading comma.
fn task_block(code: &str, objective: &str, extra: &str) -> String {
    format!(
        r#"{{"kind": "task.propose", "project": "{code}", "objective": "{objective}", "agent": "codex"{extra}}}"#
    )
}

#[test]
fn proposed_tasks_use_dispatch_defaults_and_revisions_keep_recorded_choices() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let defaults = json!({
        "default_agent":"claude-code",
        "providers":{"claude-code":{"model":"sonnet"},"codex":{"model":"gpt-6-luna","effort":"low"}}
    });
    api.store
        .update_orchestrator_settings(&orchestrator_store::SettingsPatch {
            dispatch_defaults: Some(serde_json::from_value(defaults).unwrap()),
            ..Default::default()
        })
        .unwrap();
    let snapshot = super::conversation_snapshot::build_snapshot(
        &api.store,
        &ConversationTarget::Global,
        true,
        &api.config.load.read(std::time::Instant::now()),
    );
    assert!(snapshot.contains("Task dispatch default provider: claude-code"));
    let code = project_code(ALPHA);
    let ids = propose(
        &api,
        "defaults-turn",
        &format!(
            r#"{{"kind":"task.propose","project":"{code}","objective":"Use defaults"}}, {}"#,
            task_block(&code, "Explicit provider", "")
        ),
    );
    for (index, agent, model) in [(0, "claude-code", "sonnet"), (1, "codex", "gpt-6-luna")] {
        let result = resolve(
            &api,
            &ids[index],
            "approve",
            &format!("defaults-approve-{index}"),
        )
        .unwrap();
        let id = proposed_task_id(&ids[index]);
        assert_executed(&result, &id);
        let contract = api.store.get(&id).unwrap().contract;
        assert_eq!(contract.agent, agent);
        assert_eq!(
            contract.model_selection.unwrap().model.as_deref(),
            Some(model)
        );
    }
    api.store
        .update_orchestrator_settings(&orchestrator_store::SettingsPatch {
            dispatch_defaults: Some(
                serde_json::from_value(json!({
                    "default_agent": "codex",
                    "providers": {"claude-code": {"model":"sonnet", "effort":"high"}}
                }))
                .unwrap(),
            ),
            ..Default::default()
        })
        .unwrap();
    let old = proposed_task_id(&ids[0]);
    let revised = propose(
        &api,
        "revise-defaults",
        &format!(
            r#"{{"kind":"task.revise","task_id":"{old}","added_intent":"Add one more case"}}"#
        ),
    );
    let result = resolve(&api, &revised[0], "approve", "defaults-revise").unwrap();
    let next = proposed_task_id(&revised[0]);
    assert_executed(&result, &next);
    assert_eq!(
        api.store.get(&next).unwrap().contract.model_selection,
        api.store.get(&old).unwrap().contract.model_selection
    );
}

fn in_workstream(workstream: &str) -> String {
    format!(r#", "workstream_id": "{workstream}""#)
}

pub(super) fn task_action(kind: &str, task_id: &str) -> String {
    format!(r#"{{"kind": "{kind}", "task_id": "{task_id}"}}"#)
}

fn link_action(workstream: &str, task_id: &str) -> String {
    format!(
        r#"{{"kind": "workstream.link_task", "workstream_id": "{workstream}", "task_id": "{task_id}"}}"#
    )
}

/// The `tasks.submit` request the daemon builds for a proposal under `key`.
pub(super) fn chat_submit(key: &str, objective: &str) -> TaskSubmitParams {
    TaskSubmitParams {
        idempotency_key: key.to_owned(),
        contract: TaskSubmission {
            project_id: ALPHA.to_owned(),
            parent_id: None,
            objective: objective.to_owned(),
            kind: TaskKind::Implement,
            agent: "codex".to_owned(),
            dependencies: Vec::new(),
            verification: VerificationPolicy::None,
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::ApprovalRequired,
            max_retries: 1,
            max_repairs: 1,
            model_selection: None,
            permission: None,
            work_item: None,
            work_class: None,
        },
    }
}

/// The contract the chat envelope fixes for a proposal, with id `id`.
pub(super) fn chat_contract(id: &str, objective: &str) -> TaskContract {
    TaskContract {
        id: id.to_owned(),
        project_id: ALPHA.to_owned(),
        parent_id: None,
        objective: objective.to_owned(),
        kind: TaskKind::Implement,
        agent: "codex".to_owned(),
        dependencies: Vec::new(),
        verification: VerificationPolicy::None,
        delivery: DeliveryPolicy::LocalReview,
        autonomy: AutonomyPolicy::ApprovalRequired,
        max_retries: 1,
        max_repairs: 1,
        model_selection: None,
        permission: PermissionLevel::Restricted,
        work_item: None,
    }
}

/// The id of the task a proposal makes: derived from the proposal id alone.
pub(super) fn proposed_task_id(proposal_id: &str) -> String {
    idempotency::stable_id("task", &idempotency::stable_id("action", proposal_id))
}

/// A queued task, made through the public method, as a user would.
pub(super) fn submit_queued(api: &DaemonApi, key: &str, objective: &str) -> String {
    let submitted = call(
        api,
        "tasks.submit",
        json!({
            "idempotency_key": key,
            "contract": {
                "project_id": ALPHA,
                "parent_id": null,
                "objective": objective,
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
    )
    .unwrap();
    submitted["contract"]["id"].as_str().unwrap().to_owned()
}

fn attempt(task_id: &str) -> Attempt {
    Attempt {
        id: "attempt-1".into(),
        task_id: task_id.into(),
        attempt_number: 1,
        state: AttemptState::Pending,
        failure_reason: None,
        summary: None,
        changed_files: None,
    }
}

/// A task whose first attempt is running.
pub(super) fn running_task(api: &DaemonApi, key: &str) -> String {
    let id = submit_queued(api, key, "Runs now");
    api.store.add_attempt(&id, &attempt(&id)).unwrap();
    api.store
        .transition(&id, TaskEvent::ExecutionStarted)
        .unwrap();
    api.store
        .update_attempt_state(&id, "attempt-1", AttemptState::Running)
        .unwrap();
    id
}

/// A task whose candidate awaits verification, with no session left live.
pub(super) fn verifying_task(api: &DaemonApi, key: &str) -> String {
    let id = running_task(api, key);
    api.store
        .update_attempt_state(&id, "attempt-1", AttemptState::CandidateReady)
        .unwrap();
    api.store
        .transition(&id, TaskEvent::CandidateReady)
        .unwrap();
    id
}

/// A task that ran once and failed, so one retry is allowed.
pub(super) fn failed_task(api: &DaemonApi, key: &str) -> String {
    let id = running_task(api, key);
    api.store
        .record_attempt_failure(&id, "attempt-1", "attempt failed")
        .unwrap();
    api.store
        .transition(&id, TaskEvent::ExecutionFailed)
        .unwrap();
    id
}

fn link_task(api: &DaemonApi, workstream: &str, task_id: &str, key: &str) {
    call(
        api,
        "workstreams.link_task",
        json!({"workstream_id": workstream, "task_id": task_id, "idempotency_key": key}),
    )
    .unwrap();
}

pub(super) fn task_count(api: &DaemonApi) -> usize {
    api.store.list().unwrap().len()
}

fn retries_scheduled(api: &DaemonApi, task_id: &str) -> usize {
    api.store
        .get(task_id)
        .unwrap()
        .events
        .iter()
        .filter(|event| **event == AuditEvent::RetryScheduled)
        .count()
}

/// Claims `key` and runs `effect`, then stops before the claim completes, as a crash would.
pub(super) fn stop_after_effect(
    api: &DaemonApi,
    method: &str,
    key: &str,
    params: &Value,
    effect: impl FnOnce(),
) {
    let held = api.idempotency_gate.lock().unwrap();
    let stopped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        idempotency::run_locked(&held, &api.store, method, key, params, || {
            effect();
            panic!("stopped before the claim completed")
        })
    }));
    assert!(stopped.is_err());
}

pub(super) fn assert_executed(result: &Value, result_ref: &str) {
    assert_eq!(result["proposal"]["state"], "executed", "{result}");
    assert_eq!(result["proposal"]["result_ref"], result_ref, "{result}");
}

pub(super) fn assert_failed(result: &Value, code: &str) {
    assert_eq!(result["proposal"]["state"], "failed", "{result}");
    assert_eq!(result["proposal"]["error_code"], code, "{result}");
}

#[cfg(unix)]
struct TestChild(std::process::Child);

#[cfg(unix)]
impl TestChild {
    fn spawn() -> (Self, orchestrator_core::identity::ProcessIdentity) {
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

/// A running task whose owner is a live process, so a cancellation request can be made.
#[cfg(unix)]
fn running_task_with_live_owner(api: &DaemonApi, key: &str) -> (String, TestChild) {
    use orchestrator_core::task::{Session, SessionState};
    use orchestrator_store::CancellationOwner;

    let id = running_task(api, key);
    api.store
        .add_session(
            &id,
            &Session {
                id: "session-1".into(),
                attempt_id: "attempt-1".into(),
                adapter: "codex".into(),
                state: SessionState::Spawning,
            },
        )
        .unwrap();
    api.store
        .update_session_state(&id, "session-1", SessionState::Running)
        .unwrap();
    let (child, identity) = TestChild::spawn();
    api.store
        .record_session_identity(&id, "session-1", &identity)
        .unwrap();
    let owner = CancellationOwner::new(
        id.clone(),
        "attempt-1".into(),
        "session-1".into(),
        "0123456789abcdef0123456789abcdef".into(),
    )
    .unwrap()
    .with_exact_process_identity(
        orchestrator_runtime::reconcile::capture_exact_process_identity(child.0.id()).unwrap(),
    );
    api.store.register_cancellation_owner(&owner).unwrap();
    (id, child)
}

/// Approving a cancel of a running task only requests the cancellation: the proposal
/// is executed, and the task runs on until its owner confirms.
#[cfg(unix)]
fn assert_a_running_cancel_is_only_requested(api: &DaemonApi) {
    use orchestrator_store::CancellationStatus;

    let (task_id, mut child) = running_task_with_live_owner(api, "s-running");
    let ids = propose(api, "t-running", &task_action("task.cancel", &task_id));
    let result = resolve(api, &ids[0], "approve", "k-running").unwrap();
    assert_executed(&result, &task_id);
    assert_eq!(result["proposal"]["target"]["task_state"], "Running");
    assert_eq!(api.store.get(&task_id).unwrap().state, TaskState::Running);
    assert_eq!(
        api.store.cancellation_status_for_task(&task_id).unwrap(),
        Some(CancellationStatus::Pending)
    );
    assert!(child.0.try_wait().unwrap().is_none());
}

#[test]
fn approving_task_propose_creates_one_admitted_task_with_the_chat_envelope_fields() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let ids = propose(&api, "t1", &task_block(&project_code(ALPHA), OBJECTIVE, ""));

    let approved = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    let task_id = proposed_task_id(&ids[0]);
    assert_executed(&approved, &task_id);
    // The card names the project by its folder, never by its path.
    assert_eq!(
        approved["proposal"]["target"],
        json!({"project_name": "alpha", "machine_busy": false})
    );
    assert_eq!(approved["proposal"]["denial"], Value::Null);

    // Approval fixes autonomy, verification, the attempt limits and the permission; the
    // proposal chose only the objective, kind, agent and delivery.
    let task = api.store.get(&task_id).unwrap();
    assert_eq!(task.contract, chat_contract(&task_id, OBJECTIVE));
    assert_eq!(task.state, TaskState::Queued);
    assert_eq!(task.events, vec![AuditEvent::TaskAdmitted]);
    assert_eq!(task_count(&api), 1);

    // The same key replays the answer, and a second key finds nothing pending.
    assert_eq!(resolve(&api, &ids[0], "approve", "k-1").unwrap(), approved);
    let refused = resolve(&api, &ids[0], "approve", "k-2").unwrap_err();
    assert_eq!(refused, RpcError::ProposalNotPending);
    assert_eq!(task_count(&api), 1);
}

#[test]
fn approving_task_propose_with_a_workstream_links_the_task() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let code = project_code(ALPHA);
    let workstream = create_workstream(&api, "Ship M7", "w-1");
    let objective = call(
        &api,
        "workstreams.add_objective",
        json!({"workstream_id": workstream, "title": "Ship", "idempotency_key": "o-1"}),
    )
    .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let with_objective = format!(
        r#"{}, "objective_id": "{objective}""#,
        in_workstream(&workstream)
    );
    let ids = propose(
        &api,
        "t1",
        &format!(
            "{}, {}",
            task_block(&code, "Wire the first part.", &with_objective),
            task_block(&code, "Wire the second part.", &in_workstream(&workstream))
        ),
    );

    let first = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_executed(&first, &proposed_task_id(&ids[0]));
    let link = api
        .store
        .workstream_task_link(&proposed_task_id(&ids[0]))
        .unwrap()
        .unwrap();
    assert_eq!(link.workstream_id, workstream);
    assert_eq!(link.objective_id.as_deref(), Some(objective.as_str()));
    let second = resolve(&api, &ids[1], "approve", "k-2").unwrap();
    assert_executed(&second, &proposed_task_id(&ids[1]));
    let link = api
        .store
        .workstream_task_link(&proposed_task_id(&ids[1]))
        .unwrap()
        .unwrap();
    assert_eq!(link.workstream_id, workstream);
    assert_eq!(link.objective_id, None);

    // A task may also join the workstream a `workstream.create` of the same block makes.
    let ids = propose(
        &api,
        "t2",
        &format!(
            "{CREATE}, {}",
            task_block(&code, "Wire the third part.", r#", "workstream_ref": "w1""#)
        ),
    );
    let created = resolve(&api, &ids[0], "approve", "k-3").unwrap();
    let new_workstream = created["proposal"]["result_ref"].as_str().unwrap();
    let third = resolve(&api, &ids[1], "approve", "k-4").unwrap();
    assert_executed(&third, &proposed_task_id(&ids[1]));
    let link = api
        .store
        .workstream_task_link(&proposed_task_id(&ids[1]))
        .unwrap()
        .unwrap();
    assert_eq!(link.workstream_id, new_workstream);
    assert_eq!(task_count(&api), 3);
}

#[test]
fn a_global_task_proposal_without_a_workstream_joins_global_tasks_once() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let code = project_code(ALPHA);
    let named = create_workstream(&api, "Specialised", "w-1");
    let ids = propose(
        &api,
        "t1",
        &[
            task_block(&code, "First loose task.", ""),
            task_block(&code, "Second loose task.", ""),
            task_block(&code, "A named task.", &in_workstream(&named)),
        ]
        .join(", "),
    );

    let first = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_executed(&first, &proposed_task_id(&ids[0]));
    let second = resolve(&api, &ids[1], "approve", "k-2").unwrap();
    assert_executed(&second, &proposed_task_id(&ids[1]));
    let third = resolve(&api, &ids[2], "approve", "k-3").unwrap();
    assert_executed(&third, &proposed_task_id(&ids[2]));

    let link_of = |index: usize| {
        api.store
            .workstream_task_link(&proposed_task_id(&ids[index]))
            .unwrap()
            .unwrap()
            .workstream_id
    };
    let global = link_of(0);
    assert_eq!(link_of(1), global);
    assert_eq!(link_of(2), named);
    let workstreams = api.store.list_workstreams(None).unwrap();
    assert_eq!(workstreams.len(), 2);
    let default = workstreams.iter().find(|ws| ws.id == global).unwrap();
    assert_eq!(default.title, "Global tasks");
}

#[test]
fn a_global_task_proposal_stays_unlinked_when_global_tasks_is_archived() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let code = project_code(ALPHA);
    let ids = propose(
        &api,
        "t1",
        &[
            task_block(&code, "Makes the default workstream.", ""),
            task_block(&code, "Runs after it is archived.", ""),
        ]
        .join(", "),
    );
    resolve(&api, &ids[0], "approve", "k-1").unwrap();
    let global = api
        .store
        .workstream_task_link(&proposed_task_id(&ids[0]))
        .unwrap()
        .unwrap()
        .workstream_id;
    call(
        &api,
        "workstreams.update",
        json!({"workstream_id": global, "expected_revision": 1, "state": "archived", "idempotency_key": "w-2"}),
    )
    .unwrap();

    let result = resolve(&api, &ids[1], "approve", "k-2").unwrap();
    assert_executed(&result, &proposed_task_id(&ids[1]));
    assert_eq!(
        api.store
            .workstream_task_link(&proposed_task_id(&ids[1]))
            .unwrap(),
        None
    );
    assert_eq!(task_count(&api), 2);
}

#[test]
fn task_propose_for_an_archived_workstream_fails_conflict_and_creates_no_task() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let workstream = create_workstream(&api, "Old work", "w-1");
    call(
        &api,
        "workstreams.update",
        json!({"workstream_id": workstream, "expected_revision": 1, "state": "archived", "idempotency_key": "w-2"}),
    )
    .unwrap();
    let ids = propose(
        &api,
        "t1",
        &task_block(&project_code(ALPHA), OBJECTIVE, &in_workstream(&workstream)),
    );

    let result = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_failed(&result, "conflict");
    assert_eq!(task_count(&api), 0);
}

#[test]
fn task_propose_for_a_missing_workstream_or_a_foreign_objective_creates_no_task() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let code = project_code(ALPHA);
    let here = create_workstream(&api, "Here", "w-1");
    let there = create_workstream(&api, "There", "w-2");
    let objective = call(
        &api,
        "workstreams.add_objective",
        json!({"workstream_id": there, "title": "Ship", "idempotency_key": "o-1"}),
    )
    .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let foreign = format!(r#"{}, "objective_id": "{objective}""#, in_workstream(&here));
    let missing_objective = format!(
        r#"{}, "objective_id": "objective-missing""#,
        in_workstream(&here)
    );
    let ids = propose(
        &api,
        "t1",
        &[
            task_block(
                &code,
                "Nowhere to go.",
                &in_workstream("workstream-missing"),
            ),
            task_block(&code, "Wrong objective.", &foreign),
            task_block(&code, "No such objective.", &missing_objective),
        ]
        .join(", "),
    );

    // The link target is checked before the task exists, so none is made and left unlinked.
    let results = ["k-1", "k-2", "k-3"]
        .iter()
        .zip(&ids)
        .map(|(key, id)| resolve(&api, id, "approve", key).unwrap())
        .collect::<Vec<_>>();
    assert_failed(&results[0], "not_found");
    assert_failed(&results[1], "invalid");
    assert_failed(&results[2], "not_found");
    assert_eq!(task_count(&api), 0);
}

#[test]
fn a_task_proposal_reports_machine_busy_while_the_gate_is_closed() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let ids = propose(&api, "t1", &task_block(&project_code(ALPHA), OBJECTIVE, ""));
    let target = |api: &DaemonApi| {
        let pending = api.dispatch("actions.pending", Value::Null).unwrap();
        pending["proposals"][0]["target"].clone()
    };
    assert_eq!(target(&api)["machine_busy"], false);

    crate::api::load::tests::close_gate(&api.config.load);
    assert_eq!(target(&api)["machine_busy"], true);
    // Advisory only: approval still goes through.
    let approved = resolve(&api, &ids[0], "approve", "k-busy").unwrap();
    assert_eq!(approved["proposal"]["state"], "executed");
    assert_eq!(approved["proposal"]["target"]["machine_busy"], true);
}

#[test]
fn task_propose_with_an_unknown_or_ambiguous_project_code_fails_not_found_or_conflict() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let unknown = propose(&api, "t1", &task_block("p-00000000", OBJECTIVE, ""));
    let result = resolve(&api, &unknown[0], "approve", "k-1").unwrap();
    assert_failed(&result, "not_found");
    assert_eq!(
        result["proposal"]["target"],
        json!({"project_name": null, "machine_busy": false})
    );

    // Two folders whose ids hash to one code: the proposal cannot say which it means.
    register_project(&api, "/w/45318");
    register_project(&api, "/w/55913");
    assert_eq!(project_code("/w/45318"), project_code("/w/55913"));
    let ambiguous = propose(
        &api,
        "t2",
        &task_block(&project_code("/w/45318"), OBJECTIVE, ""),
    );
    let result = resolve(&api, &ambiguous[0], "approve", "k-2").unwrap();
    assert_failed(&result, "conflict");
    assert_eq!(task_count(&api), 0);
}

#[test]
fn a_pull_request_task_that_was_never_checked_still_cannot_run() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    // The helper stores the block without the envelope check, as a bug would.
    let ids = propose(
        &api,
        "t1",
        &task_block(
            &project_code(ALPHA),
            OBJECTIVE,
            r#", "delivery": "PullRequest""#,
        ),
    );
    assert_eq!(proposal(&api, &ids[0]).state, ProposalState::Pending);

    // The approve-time re-check (Phase 4b) refuses it first, with its reason recorded. The
    // execution-time check in `propose_execution` stays behind it as a second line of defence.
    let result = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_failed(&result, "action_denied");
    assert_eq!(
        result["proposal"]["denial"], "delivery_not_authorized",
        "{result}"
    );
    assert_eq!(task_count(&api), 0);
}

/// A stop after the task was created, before it was admitted. The next start finds the
/// task by its id, admits it once, and completes the claim.
#[test]
fn a_reconcile_after_create_before_admit_admits_the_task_once() {
    let directory = tempfile::tempdir().unwrap();
    let (proposal_id, task_id) = {
        let api = DaemonApi::new(directory.path()).unwrap();
        register_project(&api, ALPHA);
        let ids = propose(&api, "t1", &task_block(&project_code(ALPHA), OBJECTIVE, ""));
        api.store.approve_action_proposal(&ids[0], "user").unwrap();

        let key = idempotency::stable_id("action", &ids[0]);
        let task_id = proposed_task_id(&ids[0]);
        let params = serde_json::to_value(chat_submit(&key, OBJECTIVE)).unwrap();
        let held = api.idempotency_gate.lock().unwrap();
        let stopped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            idempotency::run_locked(&held, &api.store, "tasks.submit", &key, &params, || {
                api.store
                    .create(&chat_contract(&task_id, OBJECTIVE))
                    .unwrap();
                panic!("stopped before the task was admitted")
            })
        }));
        assert!(stopped.is_err());
        drop(held);
        assert_eq!(api.store.get(&task_id).unwrap().state, TaskState::Created);
        (ids[0].clone(), task_id)
    };

    let api = DaemonApi::new(directory.path()).unwrap();
    let settled = proposal(&api, &proposal_id);
    assert_eq!(settled.state, ProposalState::Executed);
    assert_eq!(settled.result_ref.as_deref(), Some(task_id.as_str()));
    let task = api.store.get(&task_id).unwrap();
    assert_eq!(task.state, TaskState::Queued);
    assert_eq!(task.events, vec![AuditEvent::TaskAdmitted]);
    assert_eq!(task_count(&api), 1);
}

/// A stop after the task was created and admitted, before it was linked. The next start
/// links it once and does not admit it again.
#[test]
fn a_reconcile_after_admit_before_link_links_the_task_once() {
    let directory = tempfile::tempdir().unwrap();
    let (proposal_id, task_id, workstream) = {
        let api = DaemonApi::new(directory.path()).unwrap();
        register_project(&api, ALPHA);
        let workstream = create_workstream(&api, "Ship M7", "w-1");
        let ids = propose(
            &api,
            "t1",
            &task_block(&project_code(ALPHA), OBJECTIVE, &in_workstream(&workstream)),
        );
        api.store.approve_action_proposal(&ids[0], "user").unwrap();

        let key = idempotency::stable_id("action", &ids[0]);
        let submit = chat_submit(&key, OBJECTIVE);
        let params = serde_json::to_value(&submit).unwrap();
        let held = api.idempotency_gate.lock().unwrap();
        let stopped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            idempotency::run_locked(&held, &api.store, "tasks.submit", &key, &params, || {
                api.submit_task_inner(&submit, &key, None).unwrap();
                panic!("stopped before the task was linked")
            })
        }));
        assert!(stopped.is_err());
        drop(held);
        let task_id = proposed_task_id(&ids[0]);
        assert_eq!(api.store.workstream_task_link(&task_id).unwrap(), None);
        (ids[0].clone(), task_id, workstream)
    };

    let api = DaemonApi::new(directory.path()).unwrap();
    let settled = proposal(&api, &proposal_id);
    assert_eq!(settled.state, ProposalState::Executed);
    assert_eq!(settled.result_ref.as_deref(), Some(task_id.as_str()));
    let link = api.store.workstream_task_link(&task_id).unwrap().unwrap();
    assert_eq!(link.workstream_id, workstream);
    assert_eq!(
        api.store.get(&task_id).unwrap().events,
        vec![AuditEvent::TaskAdmitted]
    );
    assert_eq!(task_count(&api), 1);
}

#[test]
fn approving_task_cancel_on_a_queued_task_cancels_it_and_on_a_running_task_requests_cancellation() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let queued = submit_queued(&api, "s-queued", "Queued work");
    let ids = propose(&api, "t-queued", &task_action("task.cancel", &queued));

    let result = resolve(&api, &ids[0], "approve", "k-queued").unwrap();
    assert_executed(&result, &queued);
    assert_eq!(result["proposal"]["target"]["task_state"], "Cancelled");
    let task = api.store.get(&queued).unwrap();
    assert_eq!(task.state, TaskState::Cancelled);
    assert_eq!(
        task.events,
        vec![AuditEvent::TaskAdmitted, AuditEvent::TaskCancelled]
    );

    // Needs a live owner process, which only the Unix harness can provide.
    #[cfg(unix)]
    assert_a_running_cancel_is_only_requested(&api);
}

#[test]
fn approving_task_cancel_on_a_finished_task_fails_conflict() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let task_id = submit_queued(&api, "s-1", "Done work");
    call(
        &api,
        "tasks.cancel",
        json!({"task_id": task_id, "idempotency_key": "c-1"}),
    )
    .unwrap();
    let before = api.store.get(&task_id).unwrap();
    let ids = propose(&api, "t1", &task_action("task.cancel", &task_id));

    let result = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_failed(&result, "conflict");
    assert_eq!(api.store.get(&task_id).unwrap(), before);
}

#[test]
fn approving_task_retry_on_a_failed_task_queues_one_attempt() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let task_id = failed_task(&api, "s-1");
    let ids = propose(&api, "t1", &task_action("task.retry", &task_id));

    let result = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_executed(&result, &task_id);
    assert_eq!(api.store.get(&task_id).unwrap().state, TaskState::Queued);
    assert_eq!(retries_scheduled(&api, &task_id), 1);

    // The task no longer failed, so asking again changes nothing.
    let again = propose(&api, "t2", &task_action("task.retry", &task_id));
    let result = resolve(&api, &again[0], "approve", "k-2").unwrap();
    assert_failed(&result, "conflict");
    assert_eq!(retries_scheduled(&api, &task_id), 1);
}

#[test]
fn approving_task_retry_on_a_running_task_fails_conflict_and_starts_no_attempt() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let running = running_task(&api, "s-running");
    let queued = submit_queued(&api, "s-queued", "Queued work");
    let running_before = api.store.get(&running).unwrap();
    let queued_before = api.store.get(&queued).unwrap();
    let ids = propose(
        &api,
        "t1",
        &format!(
            "{}, {}",
            task_action("task.retry", &running),
            task_action("task.retry", &queued)
        ),
    );

    let result = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_failed(&result, "conflict");
    let result = resolve(&api, &ids[1], "approve", "k-2").unwrap();
    assert_failed(&result, "conflict");
    // Neither task changed: no second attempt, no second run, nothing cancelled.
    assert_eq!(api.store.get(&running).unwrap(), running_before);
    assert_eq!(api.store.get(&queued).unwrap(), queued_before);
}

#[test]
fn approving_workstream_link_task_links_it_and_a_second_link_elsewhere_fails_conflict() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let first = create_workstream(&api, "First", "w-1");
    let second = create_workstream(&api, "Second", "w-2");
    let task_id = submit_queued(&api, "s-1", "Loose work");
    let ids = propose(&api, "t1", &link_action(&first, &task_id));

    let linked = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_executed(&linked, &task_id);
    let link = api.store.workstream_task_link(&task_id).unwrap().unwrap();
    assert_eq!(link.workstream_id, first);
    assert_eq!(linked["proposal"]["target"]["task_title"], "Loose work");

    // A task serves one workstream: the second link is refused and the first stays. It is a
    // later turn, because a pending link of the same task would be superseded in the block.
    let again = propose(&api, "t2", &link_action(&second, &task_id));
    let elsewhere = resolve(&api, &again[0], "approve", "k-2").unwrap();
    assert_failed(&elsewhere, "conflict");
    let link = api.store.workstream_task_link(&task_id).unwrap().unwrap();
    assert_eq!(link.workstream_id, first);
}

#[test]
fn a_workstream_conversation_cannot_cancel_or_retry_a_task_linked_elsewhere() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let own = create_workstream(&api, "Own", "w-1");
    let other = create_workstream(&api, "Other", "w-2");
    let foreign = submit_queued(&api, "s-1", "Foreign work");
    let unlinked = failed_task(&api, "s-2");
    let mine = submit_queued(&api, "s-3", "My work");
    link_task(&api, &other, &foreign, "l-1");
    link_task(&api, &own, &mine, "l-2");
    let foreign_before = api.store.get(&foreign).unwrap();
    let unlinked_before = api.store.get(&unlinked).unwrap();

    let ids = propose_in(
        &api,
        &ConversationTarget::Workstream(own),
        "t1",
        PermissionLevel::Restricted,
        &format!(
            "{}, {}, {}",
            task_action("task.cancel", &foreign),
            task_action("task.retry", &unlinked),
            task_action("task.cancel", &mine)
        ),
    );

    let result = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_failed(&result, "invalid");
    let result = resolve(&api, &ids[1], "approve", "k-2").unwrap();
    assert_failed(&result, "invalid");
    assert_eq!(api.store.get(&foreign).unwrap(), foreign_before);
    assert_eq!(api.store.get(&unlinked).unwrap(), unlinked_before);
    // A task of the conversation's own workstream is still its to cancel.
    let result = resolve(&api, &ids[2], "approve", "k-3").unwrap();
    assert_executed(&result, &mine);
    assert_eq!(api.store.get(&mine).unwrap().state, TaskState::Cancelled);
}

#[test]
fn a_task_proposal_outside_its_conversations_workstream_fails_invalid() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let code = project_code(ALPHA);
    let own = create_workstream(&api, "Own", "w-1");
    let other = create_workstream(&api, "Other", "w-2");
    let loose = submit_queued(&api, "s-1", "Loose work");

    let ids = propose_in(
        &api,
        &ConversationTarget::Workstream(own.clone()),
        "t1",
        PermissionLevel::Restricted,
        &[
            task_block(&code, "Into the other one.", &in_workstream(&other)),
            task_block(&code, "Into none at all.", ""),
            link_action(&other, &loose),
            task_block(&code, "Into its own.", &in_workstream(&own)),
        ]
        .join(", "),
    );

    for (index, key) in ["k-1", "k-2", "k-3"].iter().enumerate() {
        let result = resolve(&api, &ids[index], "approve", key).unwrap();
        assert_failed(&result, "invalid");
    }
    // Nothing was made or linked, but the proposal that stays inside runs.
    assert_eq!(task_count(&api), 1);
    assert_eq!(api.store.workstream_task_link(&loose).unwrap(), None);
    let result = resolve(&api, &ids[3], "approve", "k-4").unwrap();
    assert_executed(&result, &proposed_task_id(&ids[3]));
    let link = api
        .store
        .workstream_task_link(&proposed_task_id(&ids[3]))
        .unwrap()
        .unwrap();
    assert_eq!(link.workstream_id, own);
}

/// A stop between approving and executing, once for every kind. The next start runs each
/// exactly once, and a further start finds nothing left to run.
#[test]
fn a_restart_between_approve_and_execute_completes_each_task_kind_once() {
    let directory = tempfile::tempdir().unwrap();
    let (ids, queued, failed, loose, workstream) = {
        let api = DaemonApi::new(directory.path()).unwrap();
        register_project(&api, ALPHA);
        let workstream = create_workstream(&api, "Ship M7", "w-1");
        let queued = submit_queued(&api, "s-1", "Queued work");
        let failed = failed_task(&api, "s-2");
        let loose = submit_queued(&api, "s-3", "Loose work");
        let ids = propose(
            &api,
            "t1",
            &[
                task_block(&project_code(ALPHA), "Brand new work.", ""),
                task_action("task.cancel", &queued),
                task_action("task.retry", &failed),
                link_action(&workstream, &loose),
            ]
            .join(", "),
        );
        // The process stops after every approval committed and before anything ran.
        for id in &ids {
            api.store.approve_action_proposal(id, "user").unwrap();
        }
        (ids, queued, failed, loose, workstream)
    };

    let assert_done = |api: &DaemonApi| {
        for id in &ids {
            assert_eq!(proposal(api, id).state, ProposalState::Executed);
        }
        let made = proposed_task_id(&ids[0]);
        assert_eq!(
            proposal(api, &ids[0]).result_ref.as_deref(),
            Some(made.as_str())
        );
        let task = api.store.get(&made).unwrap();
        assert_eq!(task.state, TaskState::Queued);
        assert_eq!(task.events, vec![AuditEvent::TaskAdmitted]);
        assert_eq!(api.store.get(&queued).unwrap().state, TaskState::Cancelled);
        assert_eq!(api.store.get(&failed).unwrap().state, TaskState::Queued);
        assert_eq!(retries_scheduled(api, &failed), 1);
        let link = api.store.workstream_task_link(&loose).unwrap().unwrap();
        assert_eq!(link.workstream_id, workstream);
        assert_eq!(task_count(api), 4);
    };
    {
        let api = DaemonApi::new(directory.path()).unwrap();
        assert_done(&api);
        let replayed = resolve(&api, &ids[0], "approve", "k-1").unwrap_err();
        assert_eq!(replayed, RpcError::ProposalNotPending);
    }
    let api = DaemonApi::new(directory.path()).unwrap();
    assert_done(&api);
}

/// A stop after a cancel, a retry and a link committed, before any claim completed. The next
/// start finds each effect by the task's own record and runs nothing twice. A retry of a task
/// that never failed has no effect to find, so it is run again and refused.
#[test]
fn an_open_claim_whose_task_effect_committed_is_found_not_rerun_at_startup() {
    let directory = tempfile::tempdir().unwrap();
    let (ids, queued, failed, loose, running, running_before, workstream) = {
        let api = DaemonApi::new(directory.path()).unwrap();
        register_project(&api, ALPHA);
        let workstream = create_workstream(&api, "Ship M7", "w-1");
        let queued = submit_queued(&api, "s-1", "Queued work");
        let failed = failed_task(&api, "s-2");
        let loose = submit_queued(&api, "s-3", "Loose work");
        let running = running_task(&api, "s-4");
        let running_before = api.store.get(&running).unwrap();
        let ids = propose(
            &api,
            "t1",
            &[
                task_action("task.cancel", &queued),
                task_action("task.retry", &failed),
                link_action(&workstream, &loose),
                task_action("task.retry", &running),
            ]
            .join(", "),
        );
        for id in &ids {
            api.store.approve_action_proposal(id, "user").unwrap();
        }
        let key = |index: usize| idempotency::stable_id("action", &ids[index]);

        let cancel = CancelParams {
            task_id: queued.clone(),
            idempotency_key: key(0),
        };
        let params = serde_json::to_value(&cancel).unwrap();
        stop_after_effect(
            &api,
            "tasks.cancel",
            &cancel.idempotency_key,
            &params,
            || {
                api.cancel_task_inner(&queued).unwrap();
            },
        );
        let retry = RetryParams {
            task_id: failed.clone(),
            idempotency_key: key(1),
        };
        let params = serde_json::to_value(&retry).unwrap();
        stop_after_effect(&api, "tasks.retry", &retry.idempotency_key, &params, || {
            api.retry_task_inner(&failed).unwrap();
        });
        let link = LinkTaskParams {
            workstream_id: workstream.clone(),
            task_id: loose.clone(),
            objective_id: None,
            idempotency_key: key(2),
        };
        let params = serde_json::to_value(&link).unwrap();
        stop_after_effect(
            &api,
            "workstreams.link_task",
            &link.idempotency_key,
            &params,
            || {
                api.link_task_inner(&link).unwrap();
            },
        );
        // This retry stopped before it did anything: the task is running, never failed.
        let retry = RetryParams {
            task_id: running.clone(),
            idempotency_key: key(3),
        };
        let params = serde_json::to_value(&retry).unwrap();
        stop_after_effect(&api, "tasks.retry", &retry.idempotency_key, &params, || {});
        (
            ids,
            queued,
            failed,
            loose,
            running,
            running_before,
            workstream,
        )
    };

    let api = DaemonApi::new(directory.path()).unwrap();
    for (id, task_id) in ids.iter().zip([&queued, &failed, &loose]) {
        let settled = proposal(&api, id);
        assert_eq!(settled.state, ProposalState::Executed, "{id}");
        assert_eq!(settled.result_ref.as_deref(), Some(task_id.as_str()));
    }
    assert_eq!(api.store.get(&queued).unwrap().state, TaskState::Cancelled);
    assert_eq!(api.store.get(&failed).unwrap().state, TaskState::Queued);
    assert_eq!(retries_scheduled(&api, &failed), 1);
    let link = api.store.workstream_task_link(&loose).unwrap().unwrap();
    assert_eq!(link.workstream_id, workstream);
    let refused = proposal(&api, &ids[3]);
    assert_eq!(refused.state, ProposalState::Failed);
    assert_eq!(refused.error_code.as_deref(), Some("conflict"));
    assert_eq!(api.store.get(&running).unwrap(), running_before);
}

#[test]
fn no_task_action_stores_or_returns_a_path_or_environment_text() {
    let (directory, api) = new_api();
    register_project(&api, ALPHA);
    let workstream = create_workstream(&api, "Ship M7", "w-1");
    let queued = submit_queued(&api, "s-1", "Queued work");
    let failed = failed_task(&api, "s-2");
    let loose = submit_queued(&api, "s-3", "Loose work");
    let code = project_code(ALPHA);
    let ids = propose(
        &api,
        "t1",
        &[
            task_block(&code, "Brand new work.", &in_workstream(&workstream)),
            task_block(
                &code,
                "Nowhere to go.",
                &in_workstream("workstream-missing"),
            ),
            task_block("p-00000000", "No such project.", ""),
            task_action("task.cancel", &queued),
            task_action("task.retry", &failed),
            link_action(&workstream, &loose),
        ]
        .join(", "),
    );
    let mut seen = String::new();
    for (index, id) in ids.iter().enumerate() {
        let result = resolve(&api, id, "approve", &format!("k-{index}")).unwrap();
        seen.push_str(&result.to_string());
    }
    for stored in api
        .store
        .action_proposals(&ConversationTarget::Global, usize::MAX)
        .unwrap()
    {
        seen.push_str(&format!("{stored:?}"));
        for event in api.store.action_proposal_events(&stored.id).unwrap() {
            seen.push_str(&format!("{event:?}"));
        }
    }
    let page = call(
        &api,
        "conversations.get",
        json!({"target": {"kind": "global"}}),
    )
    .unwrap();
    seen.push_str(&page.to_string());
    seen.push_str(
        &call(&api, "actions.pending", json!({}))
            .unwrap()
            .to_string(),
    );

    // Four of the six ran, and the rest failed with a code, so each kind is in the text.
    assert!(seen.contains("task-"), "{seen}");
    assert!(seen.contains("not_found"), "{seen}");
    let data_directory = directory.path().display().to_string();
    for secret in ["canary-root", data_directory.as_str()] {
        assert!(!seen.contains(secret), "{secret} leaked into {seen}");
    }
}

#[test]
fn actions_pending_lists_global_and_workstream_proposals_pending_only() {
    let (_directory, api) = new_api();
    let workstream = create_workstream(&api, "Ship M7", "w-1");
    let global = propose(&api, "t1", CREATE)[0].clone();
    let scoped = propose_in(
        &api,
        &ConversationTarget::Workstream(workstream.clone()),
        "t2",
        PermissionLevel::Restricted,
        &format!(
            r#"{{"kind": "workstream.update", "workstream_id": "{workstream}", "expected_revision": 1, "title": "Beta"}}"#
        ),
    )[0]
    .clone();
    let rejected = propose(
        &api,
        "t3",
        r#"{"kind": "workstream.create", "title": "Gamma"}"#,
    )[0]
    .clone();
    resolve(&api, &rejected, "reject", "k-1").unwrap();

    let page = call(&api, "actions.pending", json!({})).unwrap();
    let listed = page["proposals"].as_array().unwrap();
    let ids: Vec<&str> = listed
        .iter()
        .map(|proposal| proposal["id"].as_str().unwrap())
        .collect();
    // Newest first; the rejected card is settled and not listed.
    assert_eq!(ids, [scoped.as_str(), global.as_str()], "{page}");
    assert_eq!(
        listed[0]["scope"],
        json!({"kind": "workstream", "workstream_id": workstream})
    );
    assert_eq!(listed[1]["scope"], json!({"kind": "global"}));
    assert_eq!(listed[0]["state"], "pending");
    assert_eq!(page["more"], false);
    // A null `params` is the same as none; any field is refused.
    assert_eq!(call(&api, "actions.pending", Value::Null).unwrap(), page);
    let refused = call(&api, "actions.pending", json!({"limit": 1})).unwrap_err();
    assert_eq!(refused, RpcError::InvalidParams);
}

#[test]
fn actions_pending_stops_at_one_page_and_says_more() {
    let (_directory, api) = new_api();
    // A conversation holds at most 20 pending proposals (the store's cap), so a page of 100
    // is only reached across scopes: Global plus five workstreams fill 120, and 101 of them
    // is more than one page.
    let mut ids: Vec<String> = Vec::new();
    let mut targets = vec![ConversationTarget::Global];
    for number in 0..5 {
        let workstream =
            create_workstream(&api, &format!("Scope {number}"), &format!("ws-{number}"));
        targets.push(ConversationTarget::Workstream(workstream));
    }
    for (index, target) in targets.iter().enumerate() {
        for turn in 0..20 {
            let create =
                format!(r#"{{"kind": "workstream.create", "title": "Pending {index} {turn}"}}"#);
            ids.extend(propose_in(
                &api,
                target,
                &format!("t{index}-{turn}"),
                PermissionLevel::Restricted,
                &create,
            ));
        }
    }
    let page = call(&api, "actions.pending", json!({})).unwrap();
    let listed = page["proposals"].as_array().unwrap();
    assert_eq!(
        listed.len(),
        100,
        "{} are pending, one page is 100",
        ids.len()
    );
    assert_eq!(page["more"], true);
}

/// The transport refuses a session that did not negotiate the capability before any handler
/// runs (`tests/conversation_runtime.rs` drives that), so a session can only call the method
/// when the daemon offers `orchestrator_actions`. The method itself lists nothing it was not
/// asked for: an empty store gives an empty page.
#[test]
fn actions_pending_needs_the_capability() {
    let (_directory, api) = new_api();
    assert!(api
        .capabilities()
        .contains(&Capability::OrchestratorActions));
    let page = call(&api, "actions.pending", json!({})).unwrap();
    assert_eq!(page, json!({"proposals": [], "more": false}));
}
