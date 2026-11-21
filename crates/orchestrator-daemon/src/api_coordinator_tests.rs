//! Project coordinators as domain orchestrators (DPES-1108 phase 7): routing, ownership, the
//! parent channel and the retirement guard, through the daemon's own methods.

use orchestrator_core::orchestrator_action::{parse_block, BlockScope};
use orchestrator_core::permission::PermissionLevel;
use orchestrator_store::{
    ActionProposal, ConversationTarget, ProposalState, TurnBlock, TurnPermission,
};

use super::actions::project_code;
use super::actions_tests::{call, new_api, proposal, resolve};
use super::*;

const PROJECT: &str = "/repo/billing";

fn project_target() -> ConversationTarget {
    ConversationTarget::Project(PROJECT.into())
}

/// Registers the project and adds its coordinator.
fn with_coordinator(api: &DaemonApi) -> String {
    api.store
        .register_project(&orchestrator_core::records::Project {
            id: PROJECT.into(),
            root_path: PROJECT.into(),
            settings: json!({}),
        })
        .unwrap();
    call(
        api,
        "coordinator.restore",
        json!({"project_id": PROJECT, "idempotency_key": "add"}),
    )
    .unwrap();
    project_code(PROJECT)
}

/// Stores one block as the proposals of a Restricted turn of `target`, parsed in `scope` as a
/// finished reply is. Returns the proposal ids.
fn propose(
    api: &DaemonApi,
    target: &ConversationTarget,
    scope: BlockScope<'_>,
    turn: &str,
    actions: &str,
) -> Vec<String> {
    api.store
        .begin_conversation_turn(target, turn, "hi", "test-runtime", "1.0.0")
        .unwrap();
    api.store
        .record_conversation_turn_permission(
            turn,
            TurnPermission {
                level: PermissionLevel::Restricted,
                profile: "test-profile".into(),
            },
        )
        .unwrap();
    let held = format!("```mcao-actions\n{{\"mcao_actions\": 1, \"actions\": [{actions}]}}\n```");
    let block = parse_block(&held, scope).unwrap();
    api.store
        .complete_conversation_turn(turn, TurnBlock::Accepted(block))
        .unwrap()
        .unwrap()
}

fn approve(api: &DaemonApi, id: &str, key: &str) -> ActionProposal {
    resolve(api, id, "approve", key).unwrap();
    proposal(api, id)
}

fn route(api: &DaemonApi, code: &str, turn: &str) -> String {
    let ids = propose(
        api,
        &ConversationTarget::Global,
        BlockScope::Global,
        turn,
        &format!(
            r#"{{"kind": "coordinator.route", "project": "{code}", "work": "Fix the nightly export."}}"#
        ),
    );
    let routed = approve(api, &ids[0], &format!("{turn}-approve"));
    assert_eq!(routed.state, ProposalState::Executed, "{routed:?}");
    routed
        .result_ref
        .unwrap()
        .strip_prefix("request:")
        .unwrap()
        .to_owned()
}

#[test]
fn routed_work_runs_under_its_coordinator_and_returns_only_through_reports() {
    let (_directory, api) = new_api();
    let code = with_coordinator(&api);
    let request = route(&api, &code, "g1");
    assert!(
        orchestrator_core::charter::is_request_id(&request),
        "{request}"
    );

    // The coordinator proposes the task for the request; it owns it, outside "Global tasks".
    let ids = propose(
        &api,
        &project_target(),
        BlockScope::Project(&code),
        "c1",
        &format!(
            r#"{{"kind": "task.propose", "project": "{code}", "objective": "Fix the export.", "agent": "codex", "request": "{request}"}}"#
        ),
    );
    let made = approve(&api, &ids[0], "c1-approve");
    assert_eq!(made.state, ProposalState::Executed, "{made:?}");
    let task_id = made.result_ref.unwrap();
    assert_eq!(
        api.store.task_coordinator(&task_id).unwrap(),
        Some((PROJECT.to_owned(), Some(request.clone())))
    );
    assert!(api.store.workstream_task_link(&task_id).unwrap().is_none());
    assert_eq!(
        api.store.task_wake_facts(&task_id).unwrap().unwrap().scope,
        "conversation:project:/repo/billing"
    );

    // Retiring the coordinator is refused while it owns open work.
    let refused = call(
        &api,
        "coordinator.remove",
        json!({"project_id": PROJECT, "idempotency_key": "remove-1"}),
    );
    assert_eq!(refused.unwrap_err(), RpcError::Conflict);

    // The coordinator answers; the request closes, and a second answer is refused.
    let ids = propose(
        &api,
        &project_target(),
        BlockScope::Project(&code),
        "c2",
        &format!(
            r#"{{"kind": "coordinator.report", "verb": "answer", "request": "{request}", "note": "The export runs again."}}"#
        ),
    );
    let answered = approve(&api, &ids[0], "c2-approve");
    assert_eq!(answered.state, ProposalState::Executed, "{answered:?}");
    assert!(
        api.store
            .routed_request(&request)
            .unwrap()
            .unwrap()
            .answered
    );

    let view = call(
        &api,
        "conversations.get",
        json!({"target": {"kind": "project", "project_id": PROJECT}}),
    )
    .unwrap();
    let coordinator = &view["coordinator"];
    assert_eq!(coordinator["open_tasks"], 1, "{view}");
    assert_eq!(coordinator["open_requests"], 0, "{view}");
    assert_eq!(coordinator["routed_requests"][0]["id"], request.as_str());
    assert_eq!(coordinator["reports"][0]["verb"], "answer");

    // The task finishes, so the coordinator may go.
    call(
        &api,
        "tasks.cancel",
        json!({"task_id": task_id, "idempotency_key": "cancel-1"}),
    )
    .unwrap();
    let removed = call(
        &api,
        "coordinator.remove",
        json!({"project_id": PROJECT, "idempotency_key": "remove-2"}),
    )
    .unwrap();
    assert_eq!(removed["removed"], true, "{removed}");
}

#[test]
fn a_coordinator_never_routes_and_the_global_conversation_never_reports() {
    let (_directory, api) = new_api();
    let code = with_coordinator(&api);
    let request = route(&api, &code, "g1");
    // A route stored under a coordinator's conversation (parsed as global, as a forged store
    // row would be) fails closed at execution: depth stays two.
    let ids = propose(
        &api,
        &project_target(),
        BlockScope::Global,
        "c1",
        &format!(r#"{{"kind": "coordinator.route", "project": "{code}", "work": "Again."}}"#),
    );
    let refused = approve(&api, &ids[0], "c1-approve");
    assert_eq!(refused.state, ProposalState::Failed, "{refused:?}");
    assert_eq!(refused.error_code.as_deref(), Some("invalid"));
    assert_eq!(api.store.routed_requests(PROJECT, 10).unwrap().len(), 1);

    // A report stored under the global conversation fails closed too.
    let ids = propose(
        &api,
        &ConversationTarget::Global,
        BlockScope::Project(&code),
        "g2",
        &format!(
            r#"{{"kind": "coordinator.report", "verb": "answer", "request": "{request}", "note": "Done."}}"#
        ),
    );
    let refused = approve(&api, &ids[0], "g2-approve");
    assert_eq!(refused.state, ProposalState::Failed, "{refused:?}");
    assert!(
        !api.store
            .routed_request(&request)
            .unwrap()
            .unwrap()
            .answered
    );
}

#[test]
fn work_routed_to_a_project_without_a_coordinator_fails_as_not_found() {
    let (_directory, api) = new_api();
    let code = with_coordinator(&api);
    call(
        &api,
        "coordinator.remove",
        json!({"project_id": PROJECT, "idempotency_key": "remove"}),
    )
    .unwrap();
    let ids = propose(
        &api,
        &ConversationTarget::Global,
        BlockScope::Global,
        "g1",
        &format!(r#"{{"kind": "coordinator.route", "project": "{code}", "work": "Work."}}"#),
    );
    let failed = approve(&api, &ids[0], "g1-approve");
    assert_eq!(failed.state, ProposalState::Failed, "{failed:?}");
    assert_eq!(failed.error_code.as_deref(), Some("not_found"));
}

#[test]
fn the_scope_sentence_is_set_by_the_user_and_shown_in_the_coordinator_view() {
    let (_directory, api) = new_api();
    with_coordinator(&api);
    let set = call(
        &api,
        "coordinator.set_scope",
        json!({"project_id": PROJECT, "scope_sentence": "Payments and invoices.", "idempotency_key": "s1"}),
    )
    .unwrap();
    assert_eq!(set["scope_sentence"], "Payments and invoices.");
    let view = call(
        &api,
        "conversations.get",
        json!({"target": {"kind": "project", "project_id": PROJECT}}),
    )
    .unwrap();
    assert_eq!(
        view["coordinator"]["scope_sentence"],
        "Payments and invoices."
    );
    assert_eq!(view["coordinator"]["open_tasks"], 0);

    let two_lines = call(
        &api,
        "coordinator.set_scope",
        json!({"project_id": PROJECT, "scope_sentence": "a\nb", "idempotency_key": "s2"}),
    );
    assert_eq!(two_lines.unwrap_err(), RpcError::InvalidParams);
    let cleared = call(
        &api,
        "coordinator.set_scope",
        json!({"project_id": PROJECT, "scope_sentence": "  ", "idempotency_key": "s3"}),
    )
    .unwrap();
    assert_eq!(cleared["scope_sentence"], Value::Null);
    let unknown = call(
        &api,
        "coordinator.set_scope",
        json!({"project_id": "/repo/none", "scope_sentence": "X.", "idempotency_key": "s4"}),
    );
    assert_eq!(unknown.unwrap_err(), RpcError::NotFound);
}

/// Stores one block of a completed coordinator wake turn (no user words), as a finished reply
/// to a routed-request wake does. Returns the proposal ids.
fn coordinator_wake(api: &DaemonApi, code: &str, turn: &str, actions: &str) -> Vec<String> {
    api.store
        .begin_conversation_turn_as(
            &project_target(),
            turn,
            "Work was routed to you.",
            "test-runtime",
            "1.0.0",
            orchestrator_store::TurnTrigger::Wake,
        )
        .unwrap();
    api.store
        .record_conversation_turn_permission(
            turn,
            TurnPermission {
                level: PermissionLevel::Restricted,
                profile: "test-profile".into(),
            },
        )
        .unwrap();
    let held = format!("```mcao-actions\n{{\"mcao_actions\": 1, \"actions\": [{actions}]}}\n```");
    let block = parse_block(&held, BlockScope::Project(code)).unwrap();
    api.store
        .complete_conversation_turn(turn, TurnBlock::Accepted(block))
        .unwrap()
        .unwrap()
}

/// With the authority model (DPES-1108 integration), Global routes work on its own from the
/// user's turn, but only to a project whose coordinator is active. The coordinator, woken with
/// no user words, starts the task for the open request on its own: the request is the task's
/// origin. It reports on its own too. A task with no request stays a card.
#[test]
fn routed_work_runs_on_its_own_and_the_request_is_the_origin_of_the_coordinators_task() {
    let (_directory, api) = new_api();
    let code = with_coordinator(&api);
    const OTHER: &str = "/repo/other";
    api.store
        .register_project(&orchestrator_core::records::Project {
            id: OTHER.into(),
            root_path: OTHER.into(),
            settings: json!({}),
        })
        .unwrap();
    let ids = propose(
        &api,
        &ConversationTarget::Global,
        BlockScope::Global,
        "g1",
        &format!(
            r#"{{"kind": "coordinator.route", "project": "{code}", "work": "Fix the nightly export."}}, {{"kind": "coordinator.route", "project": "{}", "work": "No coordinator here."}}"#,
            project_code(OTHER)
        ),
    );
    assert_eq!(
        api.run_autonomous("g1", &ids, crate::executor::unix_now_ms()),
        1
    );
    let routed = proposal(&api, &ids[0]);
    assert_eq!(routed.state, ProposalState::Executed, "{routed:?}");
    assert_eq!(
        api.store.proposal_approver(&ids[0]).unwrap().as_deref(),
        Some(super::authority::AUTONOMOUS_ACTOR)
    );
    let request = routed
        .result_ref
        .unwrap()
        .strip_prefix("request:")
        .unwrap()
        .to_owned();
    // The project without an active coordinator keeps its route a card for the user.
    assert_eq!(proposal(&api, &ids[1]).state, ProposalState::Pending);

    let ids = coordinator_wake(
        &api,
        &code,
        "c1",
        &format!(
            r#"{{"kind": "task.propose", "project": "{code}", "objective": "Fix the export.", "agent": "codex", "request": "{request}"}}, {{"kind": "task.propose", "project": "{code}", "objective": "Work of my own.", "agent": "codex"}}, {{"kind": "coordinator.report", "verb": "caveat", "request": "{request}", "note": "CI is slow today."}}"#
        ),
    );
    assert_eq!(
        api.run_autonomous("c1", &ids, crate::executor::unix_now_ms()),
        2
    );
    let made = proposal(&api, &ids[0]);
    assert_eq!(made.state, ProposalState::Executed, "{made:?}");
    let task_id = made.result_ref.unwrap();
    assert_eq!(
        api.store.task_coordinator(&task_id).unwrap(),
        Some((PROJECT.to_owned(), Some(request.clone())))
    );
    let origin = api.store.task_origin(&task_id).unwrap().unwrap();
    assert_eq!(origin.kind, orchestrator_store::OriginKind::RoutedRequest);
    assert_eq!(origin.origin_ref, request);
    assert_eq!(origin.proposal_id, ids[0]);
    // A wake turn never starts work of its own.
    assert_eq!(proposal(&api, &ids[1]).state, ProposalState::Pending);
    assert_eq!(proposal(&api, &ids[2]).state, ProposalState::Executed);
    let reports = api.store.coordinator_reports(PROJECT, 10).unwrap();
    assert!(
        reports.iter().any(|report| report.verb == "caveat"),
        "{reports:?}"
    );
}
