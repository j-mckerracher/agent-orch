use orchestrator_core::orchestrator_action::{parse_block, BlockScope};
use orchestrator_store::{
    ActionProposal, ConversationTarget, ProposalState, TurnBlock, TurnPermission,
    DEPENDENCY_NOT_EXECUTED,
};

use super::conversation_agent::build_prompt;
use super::conversation_runtime::{turn_actions, RuntimeIdentity, TurnRequest};
use super::workstreams::{ReviseObjectiveParams, UpdateParams};
use super::*;

const CREATE: &str = r#"{"kind": "workstream.create", "ref": "w1", "title": "Alpha"}"#;
const PLAN: &str = r#"{"kind": "objective.add", "workstream_ref": "w1", "title": "Plan"}"#;

pub(super) fn new_api() -> (tempfile::TempDir, DaemonApi) {
    let directory = tempfile::tempdir().unwrap();
    let api = DaemonApi::new(directory.path()).unwrap();
    (directory, api)
}

pub(super) fn call(api: &DaemonApi, method: &str, params: Value) -> Result<Value, RpcError> {
    api.dispatch(method, params)
}

/// Stores one accepted block as the proposals of a global turn run at `level`, as a
/// finished reply does.
fn propose_at(api: &DaemonApi, turn: &str, level: PermissionLevel, actions: &str) -> Vec<String> {
    propose_in(api, &ConversationTarget::Global, turn, level, actions)
}

/// `propose_at` for a turn of any conversation. The block is parsed as a global one, so it
/// may hold actions that a workstream conversation would have refused when parsing.
pub(super) fn propose_in(
    api: &DaemonApi,
    target: &ConversationTarget,
    turn: &str,
    level: PermissionLevel,
    actions: &str,
) -> Vec<String> {
    api.store
        .begin_conversation_turn(target, turn, "hi", "test-runtime", "1.0.0")
        .unwrap();
    api.store
        .record_conversation_turn_permission(
            turn,
            TurnPermission {
                level,
                profile: "test-profile",
            },
        )
        .unwrap();
    let held = format!("```mcao-actions\n{{\"mcao_actions\": 1, \"actions\": [{actions}]}}\n```");
    let block = parse_block(&held, BlockScope::Global).unwrap();
    api.store
        .complete_conversation_turn(turn, TurnBlock::Accepted(block))
        .unwrap()
        .unwrap()
}

/// The proposals of a turn at `Restricted`, the only level that carries them.
pub(super) fn propose(api: &DaemonApi, turn: &str, actions: &str) -> Vec<String> {
    propose_at(api, turn, PermissionLevel::Restricted, actions)
}

/// Resolves with the digest the proposal is stored under, as the desktop sends it.
pub(super) fn resolve(
    api: &DaemonApi,
    id: &str,
    decision: &str,
    key: &str,
) -> Result<Value, RpcError> {
    let digest = proposal(api, id).digest;
    call(
        api,
        "actions.resolve",
        json!({"proposal_id": id, "decision": decision, "digest": digest, "idempotency_key": key}),
    )
}

pub(super) fn proposal(api: &DaemonApi, id: &str) -> ActionProposal {
    api.store.action_proposal(id).unwrap().unwrap()
}

pub(super) fn create_workstream(api: &DaemonApi, title: &str, key: &str) -> String {
    let created = call(
        api,
        "workstreams.create",
        json!({"title": title, "summary": "", "idempotency_key": key}),
    )
    .unwrap();
    created["id"].as_str().unwrap().to_owned()
}

#[test]
fn approve_executes_each_kind_once_and_records_the_result() {
    let (_directory, api) = new_api();
    let ids = propose(&api, "t1", &format!("{CREATE}, {PLAN}"));

    let created = resolve(&api, &ids[0], "approve", "k-create").unwrap();
    assert_eq!(created["proposal"]["state"], "executed", "{created}");
    let workstreams = api.store.list_workstreams(None).unwrap();
    assert_eq!(workstreams.len(), 1);
    assert_eq!(created["proposal"]["result_ref"], workstreams[0].id);

    let added = resolve(&api, &ids[1], "approve", "k-plan").unwrap();
    assert_eq!(added["proposal"]["state"], "executed", "{added}");
    let objectives = api.store.list_objectives(&workstreams[0].id).unwrap();
    assert_eq!(objectives.len(), 1);
    assert_eq!(added["proposal"]["result_ref"], objectives[0].id);

    // The first event is the proposal itself; the rest are the approver's.
    let events = api.store.action_proposal_events(&ids[0]).unwrap();
    let transitions: Vec<(ProposalState, &str)> = events
        .iter()
        .skip(1)
        .map(|event| (event.to_state, event.actor.as_str()))
        .collect();
    assert_eq!(
        transitions,
        [
            (ProposalState::Approved, "user"),
            (ProposalState::Executed, "user")
        ]
    );
}

#[test]
fn an_approved_update_and_revision_run_once_and_name_the_action_as_actor() {
    let (_directory, api) = new_api();
    let workstream = create_workstream(&api, "Ship M7", "w-1");
    let objective = call(
        &api,
        "workstreams.add_objective",
        json!({"workstream_id": workstream, "title": "Ship", "idempotency_key": "o-1"}),
    )
    .unwrap();
    let objective_id = objective["id"].as_str().unwrap().to_owned();
    let ids = propose(
        &api,
        "t2",
        &format!(
            r#"{{"kind": "workstream.update", "workstream_id": "{workstream}", "expected_revision": 1, "title": "Beta"}}, {{"kind": "objective.revise", "objective_id": "{objective_id}", "expected_revision": 1, "title": "Sharper"}}"#
        ),
    );

    let updated = resolve(&api, &ids[0], "approve", "k-update").unwrap();
    assert_eq!(updated["proposal"]["state"], "executed", "{updated}");
    let revisions = api
        .store
        .workstream_revisions("workstream", &workstream)
        .unwrap();
    let last = revisions.last().unwrap();
    assert_eq!(last.revision, 2);
    assert_eq!(last.actor, format!("action:{}", ids[0]));
    assert_eq!(api.store.get_workstream(&workstream).unwrap().title, "Beta");

    let revised = resolve(&api, &ids[1], "approve", "k-revise").unwrap();
    assert_eq!(revised["proposal"]["state"], "executed", "{revised}");
    let revisions = api
        .store
        .workstream_revisions("objective", &objective_id)
        .unwrap();
    let last = revisions.last().unwrap();
    assert_eq!(last.revision, 2);
    assert_eq!(last.actor, format!("action:{}", ids[1]));
    assert_eq!(
        api.store.get_objective(&objective_id).unwrap().title,
        "Sharper"
    );
}

#[test]
fn the_same_key_twice_returns_the_same_result_and_writes_once() {
    let (_directory, api) = new_api();
    let ids = propose(&api, "t1", CREATE);
    let first = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    let second = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_eq!(first, second);
    assert_eq!(api.store.list_workstreams(None).unwrap().len(), 1);
}

#[test]
fn a_second_key_for_the_same_proposal_gets_proposal_not_pending() {
    let (_directory, api) = new_api();
    let ids = propose(&api, "t1", CREATE);
    resolve(&api, &ids[0], "approve", "k-1").unwrap();
    let refused = resolve(&api, &ids[0], "approve", "k-2").unwrap_err();
    assert_eq!(refused, RpcError::ProposalNotPending);
    assert_eq!(api.store.list_workstreams(None).unwrap().len(), 1);
}

#[test]
fn reject_then_approve_gets_proposal_not_pending() {
    let (_directory, api) = new_api();
    let ids = propose(&api, "t1", CREATE);
    resolve(&api, &ids[0], "reject", "k-1").unwrap();
    assert_eq!(proposal(&api, &ids[0]).state, ProposalState::Rejected);
    let refused = resolve(&api, &ids[0], "approve", "k-2").unwrap_err();
    assert_eq!(refused, RpcError::ProposalNotPending);
    assert!(api.store.list_workstreams(None).unwrap().is_empty());
}

#[test]
fn approve_after_clear_gets_proposal_not_found_and_so_does_its_replay() {
    let (_directory, api) = new_api();
    let ids = propose(&api, "t1", CREATE);
    // The desktop holds the digest it showed; the clear removes the proposal.
    let digest = proposal(&api, &ids[0]).digest;
    call(
        &api,
        "conversations.clear",
        json!({"target": {"kind": "global"}, "idempotency_key": "clear-1"}),
    )
    .unwrap();
    let approve = || {
        call(
            &api,
            "actions.resolve",
            json!({"proposal_id": ids[0], "decision": "approve", "digest": digest, "idempotency_key": "k-1"}),
        )
    };
    let refused = approve().unwrap_err();
    assert_eq!(refused, RpcError::ProposalNotFound);
    let replayed = approve().unwrap_err();
    assert_eq!(replayed, RpcError::ProposalNotFound);
    assert!(api.store.list_workstreams(None).unwrap().is_empty());
}

/// Only the allowlist check: `resolve_as` refuses an agent actor. Says nothing about
/// how a wire caller is identified, which the daemon does not do yet.
#[test]
fn the_approval_allowlist_refuses_an_agent_actor() {
    let (_directory, api) = new_api();
    let ids = propose(&api, "t1", CREATE);
    let digest = proposal(&api, &ids[0]).digest;
    let refused = api
        .resolve_as(
            &Actor::Agent("codex".to_owned()),
            json!({"proposal_id": ids[0], "decision": "approve", "digest": digest, "idempotency_key": "k-1"}),
        )
        .unwrap_err();
    assert_eq!(refused, RpcError::Forbidden);
    assert_eq!(proposal(&api, &ids[0]).state, ProposalState::Pending);
    assert!(api.store.list_workstreams(None).unwrap().is_empty());
}

#[test]
fn a_digest_that_does_not_match_refuses_and_changes_nothing() {
    let (_directory, api) = new_api();
    let ids = propose(&api, "t1", CREATE);
    let refused = call(
        &api,
        "actions.resolve",
        json!({"proposal_id": ids[0], "decision": "approve", "digest": "0".repeat(64), "idempotency_key": "k-bad"}),
    )
    .unwrap_err();
    assert_eq!(refused, RpcError::ProposalInvalid);
    assert_eq!(proposal(&api, &ids[0]).state, ProposalState::Pending);
    assert!(api.store.list_workstreams(None).unwrap().is_empty());

    // The matching digest still approves the same proposal under a new key.
    let approved = resolve(&api, &ids[0], "approve", "k-good").unwrap();
    assert_eq!(approved["proposal"]["state"], "executed", "{approved}");
}

#[test]
fn a_missing_digest_is_a_bad_request() {
    let (_directory, api) = new_api();
    let ids = propose(&api, "t1", CREATE);
    let refused = call(
        &api,
        "actions.resolve",
        json!({"proposal_id": ids[0], "decision": "approve", "idempotency_key": "k-1"}),
    )
    .unwrap_err();
    assert_eq!(refused, RpcError::InvalidParams);
    assert_eq!(proposal(&api, &ids[0]).state, ProposalState::Pending);
}

#[test]
fn unknown_proposal_bad_decision_and_bad_key_are_refused_without_writes() {
    let (_directory, api) = new_api();
    let ids = propose(&api, "t1", CREATE);
    let missing = call(
        &api,
        "actions.resolve",
        json!({"proposal_id": "proposal-missing", "decision": "approve", "digest": "0".repeat(64), "idempotency_key": "k-1"}),
    )
    .unwrap_err();
    assert_eq!(missing, RpcError::ProposalNotFound);
    let decision = resolve(&api, &ids[0], "maybe", "k-2").unwrap_err();
    assert_eq!(decision, RpcError::InvalidParams);
    let key = resolve(&api, &ids[0], "approve", "bad key").unwrap_err();
    assert_eq!(key, RpcError::InvalidParams);
    assert_eq!(proposal(&api, &ids[0]).state, ProposalState::Pending);
}

#[test]
fn a_stale_revision_fails_the_card_with_stale_revision() {
    let (_directory, api) = new_api();
    let workstream = create_workstream(&api, "Ship M7", "w-1");
    let ids = propose(
        &api,
        "t1",
        &format!(
            r#"{{"kind": "workstream.update", "workstream_id": "{workstream}", "expected_revision": 1, "title": "Beta"}}"#
        ),
    );
    // Another write moves the workstream past the revision the proposal read.
    call(
        &api,
        "workstreams.update",
        json!({"workstream_id": workstream, "expected_revision": 1, "title": "Gamma", "idempotency_key": "w-2"}),
    )
    .unwrap();

    let result = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_eq!(result["proposal"]["state"], "failed", "{result}");
    assert_eq!(
        result["proposal"]["error_code"], "stale_revision",
        "{result}"
    );
    assert_eq!(
        api.store.get_workstream(&workstream).unwrap().title,
        "Gamma"
    );
}

#[test]
fn a_dependent_approved_before_its_parent_executed_gets_proposal_invalid() {
    let (_directory, api) = new_api();
    let ids = propose(&api, "t1", &format!("{CREATE}, {PLAN}"));
    let refused = resolve(&api, &ids[1], "approve", "k-1").unwrap_err();
    assert_eq!(refused, RpcError::ProposalInvalid);
    assert_eq!(proposal(&api, &ids[1]).state, ProposalState::Pending);
    assert!(api.store.list_workstreams(None).unwrap().is_empty());
}

#[test]
fn a_rejected_parent_fails_its_dependent_with_dependency_not_executed() {
    let (_directory, api) = new_api();
    let ids = propose(&api, "t1", &format!("{CREATE}, {PLAN}"));
    resolve(&api, &ids[0], "reject", "k-1").unwrap();
    let dependent = proposal(&api, &ids[1]);
    assert_eq!(dependent.state, ProposalState::Failed);
    assert_eq!(
        dependent.error_code.as_deref(),
        Some(DEPENDENCY_NOT_EXECUTED)
    );
    assert!(api.store.list_workstreams(None).unwrap().is_empty());
}

#[test]
fn a_restart_between_approve_and_execute_completes_once() {
    let directory = tempfile::tempdir().unwrap();
    let ids = {
        let api = DaemonApi::new(directory.path()).unwrap();
        let ids = propose(&api, "t1", &format!("{CREATE}, {PLAN}"));
        // The process stops after the approval commits and before anything runs.
        api.store.approve_action_proposal(&ids[0], "user").unwrap();
        ids
    };

    let api = DaemonApi::new(directory.path()).unwrap();
    assert_eq!(proposal(&api, &ids[0]).state, ProposalState::Executed);
    assert_eq!(proposal(&api, &ids[1]).state, ProposalState::Pending);
    assert_eq!(api.store.list_workstreams(None).unwrap().len(), 1);
    let replayed = resolve(&api, &ids[0], "approve", "k-1").unwrap_err();
    assert_eq!(replayed, RpcError::ProposalNotPending);
    assert_eq!(api.store.list_workstreams(None).unwrap().len(), 1);
}

#[test]
fn a_pending_proposal_stays_listed_past_the_settled_cap() {
    let (_directory, api) = new_api();
    let old = propose(&api, "t0", CREATE)[0].clone();
    for turn in 1..=110 {
        let settled = format!(r#"{{"kind": "workstream.create", "title": "Settled {turn}"}}"#);
        let id = propose(&api, &format!("t{turn}"), &settled)[0].clone();
        api.store.reject_action_proposal(&id, "user").unwrap();
    }
    let page = call(
        &api,
        "conversations.get",
        json!({"target": {"kind": "global"}}),
    )
    .unwrap();
    let listed: Vec<&str> = page["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|proposal| proposal["id"].as_str().unwrap())
        .collect();
    assert!(listed.contains(&old.as_str()), "{page}");
    // One pending card, plus the 100 newest settled cards.
    assert_eq!(listed.len(), 101, "{page}");
    assert_eq!(page["proposals_more"], true);
}

/// A stop after an update and a revise committed, before either claim completed. The
/// next start finds each change by its record, completes its claim, and runs nothing twice.
#[test]
fn an_update_and_a_revise_whose_effect_committed_are_found_not_rerun_at_startup() {
    let directory = tempfile::tempdir().unwrap();
    let (workstream, objective_id, ids) = {
        let api = DaemonApi::new(directory.path()).unwrap();
        let workstream = create_workstream(&api, "Ship M7", "w-1");
        let objective = call(
            &api,
            "workstreams.add_objective",
            json!({"workstream_id": workstream, "title": "Ship", "idempotency_key": "o-1"}),
        )
        .unwrap();
        let objective_id = objective["id"].as_str().unwrap().to_owned();
        let ids = propose(
            &api,
            "t1",
            &format!(
                r#"{{"kind": "workstream.update", "workstream_id": "{workstream}", "expected_revision": 1, "title": "Beta"}}, {{"kind": "objective.revise", "objective_id": "{objective_id}", "expected_revision": 1, "title": "Sharper"}}"#
            ),
        );
        for id in &ids {
            api.store.approve_action_proposal(id, "user").unwrap();
        }

        // Each change commits, then the process stops before its claim completes.
        let held = api.idempotency_gate.lock().unwrap();
        let update = UpdateParams {
            workstream_id: workstream.clone(),
            expected_revision: 1,
            title: Some("Beta".to_owned()),
            summary: None,
            state: None,
            idempotency_key: idempotency::stable_id("action", &ids[0]),
        };
        let update_params = serde_json::to_value(&update).unwrap();
        let stopped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            idempotency::run_locked(
                &held,
                &api.store,
                "workstreams.update",
                &update.idempotency_key,
                &update_params,
                || {
                    api.update_inner(&update, &format!("action:{}", ids[0]))
                        .unwrap();
                    panic!("stopped before the claim completed")
                },
            )
        }));
        assert!(stopped.is_err());

        let revise = ReviseObjectiveParams {
            objective_id: objective_id.clone(),
            expected_revision: 1,
            title: Some("Sharper".to_owned()),
            description: None,
            add_gates: Vec::new(),
            idempotency_key: idempotency::stable_id("action", &ids[1]),
        };
        let revise_params = serde_json::to_value(&revise).unwrap();
        let stopped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            idempotency::run_locked(
                &held,
                &api.store,
                "workstreams.revise_objective",
                &revise.idempotency_key,
                &revise_params,
                || {
                    api.revise_objective_inner(&revise, &format!("action:{}", ids[1]))
                        .unwrap();
                    panic!("stopped before the claim completed")
                },
            )
        }));
        assert!(stopped.is_err());
        drop(held);
        (workstream, objective_id, ids)
    };

    // The next start sees both claims open, finds each change, and completes it.
    let api = DaemonApi::new(directory.path()).unwrap();
    assert_eq!(proposal(&api, &ids[0]).state, ProposalState::Executed);
    assert_eq!(proposal(&api, &ids[1]).state, ProposalState::Executed);
    assert_eq!(
        proposal(&api, &ids[0]).result_ref.as_deref(),
        Some(workstream.as_str())
    );
    assert_eq!(
        proposal(&api, &ids[1]).result_ref.as_deref(),
        Some(objective_id.as_str())
    );
    let revisions = api
        .store
        .workstream_revisions("workstream", &workstream)
        .unwrap();
    assert_eq!(revisions.len(), 2);
    assert_eq!(revisions[1].revision, 2);
    assert_eq!(revisions[1].actor, format!("action:{}", ids[0]));
    let revisions = api
        .store
        .workstream_revisions("objective", &objective_id)
        .unwrap();
    assert_eq!(revisions.len(), 2);
    assert_eq!(revisions[1].revision, 2);
    assert_eq!(revisions[1].actor, format!("action:{}", ids[1]));
    assert_eq!(api.store.get_workstream(&workstream).unwrap().title, "Beta");
    assert_eq!(
        api.store.get_objective(&objective_id).unwrap().title,
        "Sharper"
    );
}

/// A proposal on a turn that ran at Extended or Full is refused, even though the turn
/// stored it. Nothing executes and the card stays pending.
#[test]
fn a_proposal_on_a_full_or_extended_turn_is_refused_and_never_executes() {
    let (_directory, api) = new_api();
    for (position, level) in [PermissionLevel::Extended, PermissionLevel::Full]
        .into_iter()
        .enumerate()
    {
        let ids = propose_at(&api, &format!("t-{position}"), level, CREATE);
        let refused = resolve(&api, &ids[0], "approve", &format!("k-{position}")).unwrap_err();
        assert_eq!(refused, RpcError::ProposalInvalid, "{level:?}");
        assert_eq!(
            proposal(&api, &ids[0]).state,
            ProposalState::Pending,
            "{level:?}"
        );
    }
    assert!(api.store.list_workstreams(None).unwrap().is_empty());
}

/// Only a Restricted or Auto turn in a session that negotiated the capability gets the
/// block format. Extended and Full never do, so their preamble cannot ask for proposals.
#[test]
fn a_turn_above_auto_never_gets_the_block_format() {
    for level in [PermissionLevel::Restricted, PermissionLevel::Auto] {
        assert!(turn_actions(true, level), "{level:?}");
        assert!(!turn_actions(false, level), "{level:?}");
    }
    for level in [PermissionLevel::Extended, PermissionLevel::Full] {
        // Even with the capability negotiated.
        let actions = turn_actions(true, level);
        assert!(!actions, "{level:?}");
        let request = TurnRequest {
            target: ConversationTarget::Global,
            text: "hi".into(),
            identity: RuntimeIdentity {
                name: "codex".into(),
                version: "0.0.0".into(),
            },
            choice: None,
            level,
            history: Vec::new(),
            context: None,
            actions,
            wake: None,
            mcp: None,
        };
        let prompt = build_prompt(&request);
        assert!(!prompt.instructions.contains("mcao-actions"), "{level:?}");
        assert!(
            !prompt.instructions.contains("\"mcao_actions\""),
            "{level:?}"
        );
    }
}
