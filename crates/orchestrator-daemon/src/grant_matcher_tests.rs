//! The standing-grant matcher (DPES-1105 phase 5a). Every test drives an in-process
//! `DaemonApi` over a real store and calls `grant_matcher_tick` itself with a clock reading it
//! chooses: no thread runs and no test sleeps. The project roots are paths that do not exist,
//! so no agent CLI can start.

use orchestrator_core::orchestrator_action::ActionKind;
use orchestrator_core::standing_grant::GRANTABLE_KINDS;
use orchestrator_core::task::{Attempt, AttemptState, Session, SessionState};
use orchestrator_store::{
    ConversationTarget, GrantClaimError, GrantError, GrantEventKind, ProposalState, UsageReceipt,
    UsageSource,
};

use super::actions::{project_code, FORCED_GRANT_READ_ERRORS};
use super::actions_task_tests::{
    proposed_task_id, register_project, submit_queued, task_count, ALPHA,
};
use super::actions_tests::{
    call, create_workstream, new_api, proposal, propose, propose_in, resolve,
};
use super::grant_matcher::MatchReport;
use super::*;

const BETA: &str = "/private/canary-root/beta";

fn now() -> u64 {
    crate::executor::unix_now_ms()
}

fn task_action(code: &str, objective: &str) -> String {
    json!({"kind": "task.propose", "project": code, "objective": objective, "agent": "codex"})
        .to_string()
}

fn grant_with(api: &DaemonApi, scope: Value, kinds: Value, key: &str, extra: Value) -> String {
    let mut params = json!({
        "scope": scope,
        "kinds": kinds,
        "token_budget": 1_000_000,
        "idempotency_key": key,
    });
    for (name, value) in extra.as_object().unwrap() {
        params[name] = value.clone();
    }
    let created = call(api, "grants.create", params).unwrap();
    created["grant"]["id"].as_str().unwrap().to_owned()
}

/// A grant on the project `root` for `task.propose`.
fn project_grant(api: &DaemonApi, root: &str, key: &str) -> String {
    grant_with(
        api,
        json!({"kind": "project", "id": project_code(root)}),
        json!(["task.propose"]),
        key,
        json!({}),
    )
}

fn tick(api: &DaemonApi) -> MatchReport {
    api.grant_matcher_tick(now())
}

fn uses_used(api: &DaemonApi, grant_id: &str) -> u32 {
    api.store
        .standing_grant(grant_id)
        .unwrap()
        .unwrap()
        .uses_used
}

fn state_of(api: &DaemonApi, id: &str) -> ProposalState {
    proposal(api, id).state
}

fn event_kinds(api: &DaemonApi, grant_id: &str) -> Vec<GrantEventKind> {
    api.store
        .grant_events(grant_id, 0, 100)
        .unwrap()
        .into_iter()
        .map(|event| event.event)
        .collect()
}

/// A claim made straight on the store, as the matcher makes it, without running the proposal:
/// the state a stop between the claim and the execution leaves behind.
fn claim_only(api: &DaemonApi, proposal_id: &str, grant_id: &str) {
    let digest = proposal(api, proposal_id).digest;
    api.store
        .approve_action_proposal_by_grant(proposal_id, &digest, grant_id, now())
        .unwrap();
}

#[test]
fn a_matching_grant_approves_and_executes_with_a_grant_actor() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let grant = project_grant(&api, ALPHA, "g-1");
    let ids = propose(
        &api,
        "t1",
        &task_action(&project_code(ALPHA), "Add unit tests."),
    );

    let report = tick(&api);

    assert_eq!(
        report,
        MatchReport {
            reconciled: 0,
            tried: 1,
            approved: 1
        }
    );
    let task_id = proposed_task_id(&ids[0]);
    let done = proposal(&api, &ids[0]);
    assert_eq!(done.state, ProposalState::Executed);
    assert_eq!(done.result_ref.as_deref(), Some(task_id.as_str()));
    assert_eq!(api.store.get(&task_id).unwrap().state, TaskState::Queued);
    let actor = format!("grant:{grant}");
    let transitions: Vec<(ProposalState, String)> = api
        .store
        .action_proposal_events(&ids[0])
        .unwrap()
        .into_iter()
        .skip(1)
        .map(|event| (event.to_state, event.actor))
        .collect();
    assert_eq!(
        transitions,
        [
            (ProposalState::Approved, actor.clone()),
            (ProposalState::Executed, actor.clone())
        ]
    );
    assert_eq!(
        event_kinds(&api, &grant),
        [
            GrantEventKind::Created,
            GrantEventKind::Used,
            GrantEventKind::Executed
        ]
    );
    let executed = api
        .store
        .grant_events(&grant, 0, 100)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(executed.task_id.as_deref(), Some(task_id.as_str()));
    assert_eq!(executed.proposal_id.as_deref(), Some(ids[0].as_str()));
    assert_eq!(uses_used(&api, &grant), 1);
    // A second pass finds nothing left to do.
    assert_eq!(tick(&api), MatchReport::default());
    assert_eq!(task_count(&api), 1);
}

#[test]
fn a_grant_never_approves_an_auto_turns_proposal() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let grant = project_grant(&api, ALPHA, "g-1");
    let code = project_code(ALPHA);
    let auto = propose_in(
        &api,
        &ConversationTarget::Global,
        "t1",
        PermissionLevel::Auto,
        &task_action(&code, "Auto work."),
    );
    let full = propose_in(
        &api,
        &ConversationTarget::Global,
        "t2",
        PermissionLevel::Full,
        &task_action(&code, "Full work."),
    );

    let report = tick(&api);

    assert_eq!(report.approved, 0);
    assert_eq!(state_of(&api, &auto[0]), ProposalState::Pending);
    assert_eq!(state_of(&api, &full[0]), ProposalState::Pending);
    assert_eq!(uses_used(&api, &grant), 0);
    assert_eq!(task_count(&api), 0);
}

#[test]
fn a_grant_never_covers_a_never_grantable_kind() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let code = project_code(ALPHA);
    // Creating a grant for a kind is refused unless it is on the grantable list.
    for kind in ActionKind::ALL {
        let created = call(
            &api,
            "grants.create",
            json!({
                "scope": {"kind": "project", "id": code},
                "kinds": [kind.as_str()],
                "token_budget": 1000,
                "idempotency_key": format!("walk-{}", kind.as_str()),
            }),
        );
        if kind == ActionKind::DeliveryMerge {
            // Grantable only through a merge grant: alone, confirmed.
            assert_eq!(created.unwrap_err(), RpcError::GrantInvalid, "{kind:?}");
        } else if GRANTABLE_KINDS.contains(&kind) {
            assert!(created.is_ok(), "{kind:?}");
        } else {
            assert_eq!(created.unwrap_err(), RpcError::GrantInvalid, "{kind:?}");
        }
    }
    // And a proposal of such a kind stays a pending card under a grant that covers every
    // ordinary grantable kind in its project (the merge kind cannot share a grant).
    let ordinary: Vec<&str> = GRANTABLE_KINDS
        .iter()
        .filter(|kind| **kind != ActionKind::DeliveryMerge)
        .map(|kind| kind.as_str())
        .collect();
    let grant = grant_with(
        &api,
        json!({"kind": "project", "id": code}),
        json!(ordinary),
        "g-all",
        json!({}),
    );
    let actions = [
        r#"{"kind": "workstream.create", "title": "Beta"}"#.to_owned(),
        r#"{"kind": "task.cancel", "task_id": "task-1"}"#.to_owned(),
        r#"{"kind": "task.revise", "task_id": "task-1", "added_intent": "More."}"#.to_owned(),
        format!(r#"{{"kind": "project.remove", "project": "{code}"}}"#),
        r#"{"kind": "decision.hold", "task_id": "task-1", "until": "2026-12-01"}"#.to_owned(),
        r#"{"kind": "decision.ask", "task_id": "task-1", "question": "Which?"}"#.to_owned(),
        r#"{"kind": "task.message", "task_id": "task-1", "text": "Go on."}"#.to_owned(),
        format!(
            r#"{{"kind": "delivery.merge", "task_id": "task-1", "pr_number": 42, "head_sha": "{}"}}"#,
            "a".repeat(40)
        ),
    ];
    let ids = propose(&api, "t1", &actions.join(", "));
    assert_eq!(ids.len(), actions.len());

    let report = tick(&api);

    assert_eq!(report.approved, 0);
    for id in &ids {
        assert_eq!(state_of(&api, id), ProposalState::Pending);
    }
    assert_eq!(uses_used(&api, &grant), 0);
}

#[test]
fn a_grant_never_covers_a_workstream_update_that_sets_state() {
    let (_directory, api) = new_api();
    let workstream = create_workstream(&api, "Ship M7", "w-1");
    let revision = api.store.get_workstream(&workstream).unwrap().revision;
    let grant = grant_with(
        &api,
        json!({"kind": "workstream", "id": workstream}),
        json!(["workstream.update"]),
        "g-1",
        json!({}),
    );
    let update = |extra: Value| {
        let mut action = json!({
            "kind": "workstream.update",
            "workstream_id": workstream,
            "expected_revision": revision,
        });
        for (name, value) in extra.as_object().unwrap() {
            action[name] = value.clone();
        }
        action.to_string()
    };
    let pausing = propose(&api, "t1", &update(json!({"state": "suspended"})));

    let report = tick(&api);

    assert_eq!(report.approved, 0);
    assert_eq!(state_of(&api, &pausing[0]), ProposalState::Pending);
    assert_eq!(uses_used(&api, &grant), 0);
    assert_eq!(
        api.store.get_workstream(&workstream).unwrap().state,
        orchestrator_core::workstream::WorkstreamState::Active
    );
    // The same grant covers a title change in that workstream.
    let renaming = propose(&api, "t2", &update(json!({"title": "Ship M8"})));
    assert_eq!(tick(&api).approved, 1);
    assert_eq!(state_of(&api, &renaming[0]), ProposalState::Executed);
    assert_eq!(
        api.store.get_workstream(&workstream).unwrap().title,
        "Ship M8"
    );
}

#[test]
fn a_workstream_grant_cannot_start_a_task_on_a_foreign_project() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    register_project(&api, BETA);
    let workstream = create_workstream(&api, "Ship M7", "w-1");
    let member = submit_queued(&api, "s-1", "Already in the workstream.");
    call(
        &api,
        "workstreams.link_task",
        json!({"workstream_id": workstream, "task_id": member, "idempotency_key": "l-1"}),
    )
    .unwrap();
    let grant = grant_with(
        &api,
        json!({"kind": "workstream", "id": workstream}),
        json!(["task.propose"]),
        "g-1",
        json!({}),
    );
    let in_workstream = |root: &str, objective: &str| {
        json!({
            "kind": "task.propose",
            "project": project_code(root),
            "objective": objective,
            "agent": "codex",
            "workstream_id": workstream,
        })
        .to_string()
    };
    let foreign = propose(
        &api,
        "t1",
        &in_workstream(BETA, "Work on the other project."),
    );
    let own = propose(
        &api,
        "t2",
        &in_workstream(ALPHA, "Work on the member project."),
    );

    let report = tick(&api);

    assert_eq!(report.tried, 2);
    assert_eq!(report.approved, 1);
    assert_eq!(state_of(&api, &foreign[0]), ProposalState::Pending);
    assert_eq!(state_of(&api, &own[0]), ProposalState::Executed);
    assert_eq!(uses_used(&api, &grant), 1);
    // One task for the member, one the grant started; nothing on the foreign project.
    assert_eq!(task_count(&api), 2);
    assert!(api
        .store
        .list()
        .unwrap()
        .iter()
        .all(|task| task.contract.project_id == ALPHA));
}

#[test]
fn a_grant_ignores_proposals_older_than_the_grant() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let code = project_code(ALPHA);
    let before = propose(&api, "t1", &task_action(&code, "Asked before the grant."));
    project_grant(&api, ALPHA, "g-1");
    let after = propose(&api, "t2", &task_action(&code, "Asked after the grant."));

    let report = tick(&api);

    assert_eq!(report.tried, 1);
    assert_eq!(report.approved, 1);
    assert_eq!(state_of(&api, &before[0]), ProposalState::Pending);
    assert_eq!(state_of(&api, &after[0]), ProposalState::Executed);
}

#[test]
fn an_expired_grant_matches_nothing() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let grant = grant_with(
        &api,
        json!({"kind": "project", "id": project_code(ALPHA)}),
        json!(["task.propose"]),
        "g-1",
        json!({"expires_in_secs": 60}),
    );
    let ids = propose(&api, "t1", &task_action(&project_code(ALPHA), "Too late."));

    let report = api.grant_matcher_tick(now() + 61_000);

    assert_eq!(report, MatchReport::default());
    assert_eq!(state_of(&api, &ids[0]), ProposalState::Pending);
    assert_eq!(uses_used(&api, &grant), 0);
    assert_eq!(task_count(&api), 0);
}

#[test]
fn a_grant_stops_at_its_use_cap() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let grant = grant_with(
        &api,
        json!({"kind": "project", "id": project_code(ALPHA)}),
        json!(["task.propose"]),
        "g-1",
        json!({"max_uses": 1}),
    );
    let code = project_code(ALPHA);
    let first = propose(&api, "t1", &task_action(&code, "First."));
    let second = propose(&api, "t2", &task_action(&code, "Second."));

    let report = tick(&api);

    assert_eq!(report.approved, 1);
    assert_eq!(state_of(&api, &first[0]), ProposalState::Executed);
    assert_eq!(state_of(&api, &second[0]), ProposalState::Pending);
    assert_eq!(uses_used(&api, &grant), 1);
    let listed = call(&api, "grants.list", json!({"include_ended": true})).unwrap();
    assert_eq!(listed["grants"][0]["status"], "exhausted", "{listed}");
}

#[test]
fn a_grant_stops_when_its_token_budget_is_spent_and_cancels_nothing() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let grant = grant_with(
        &api,
        json!({"kind": "project", "id": project_code(ALPHA)}),
        json!(["task.propose"]),
        "g-1",
        json!({"token_budget": 100}),
    );
    let code = project_code(ALPHA);
    let first = propose(&api, "t1", &task_action(&code, "First."));
    assert_eq!(tick(&api).approved, 1);
    let task_id = proposed_task_id(&first[0]);
    // The task the grant started reports more tokens than the whole budget.
    api.store
        .add_attempt(
            &task_id,
            &Attempt {
                id: "attempt-1".into(),
                task_id: task_id.clone(),
                attempt_number: 1,
                state: AttemptState::Pending,
                failure_reason: None,
                summary: None,
                changed_files: None,
            },
        )
        .unwrap();
    api.store
        .add_session(
            &task_id,
            &Session {
                id: "session-1".into(),
                attempt_id: "attempt-1".into(),
                adapter: "codex".into(),
                state: SessionState::Spawning,
            },
        )
        .unwrap();
    let at = now() + 1;
    api.store
        .record_usage_receipt(&UsageReceipt {
            task_id: task_id.clone(),
            session_id: "session-1".into(),
            source: UsageSource::ProviderReported,
            receipt_key: "turn:1".into(),
            window_start_ms: at - 1,
            window_end_ms: at,
            reported_at_ms: at,
            input_tokens: Some(150),
            cache_read_tokens: None,
            cache_write_tokens: None,
            output_tokens: Some(20),
            reasoning_tokens: None,
            cost: None,
            model: None,
        })
        .unwrap();
    let second = propose(&api, "t2", &task_action(&code, "Second."));

    let report = tick(&api);

    assert_eq!(report.approved, 0);
    assert_eq!(state_of(&api, &second[0]), ProposalState::Pending);
    assert_eq!(uses_used(&api, &grant), 1);
    // The budget blocks new uses only: the task it started is not cancelled.
    assert_eq!(api.store.get(&task_id).unwrap().state, TaskState::Queued);
    let listed = call(&api, "grants.list", json!({"include_ended": true})).unwrap();
    assert_eq!(
        listed["grants"][0]["status"], "budget_exhausted",
        "{listed}"
    );
    assert_eq!(listed["grants"][0]["tokens_spent"], 170, "{listed}");
}

#[test]
fn a_revoked_grant_stops_before_execution_and_fails_its_approved_proposals() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let code = project_code(ALPHA);

    // Revoke through the method: the proposal the grant approved and never started fails.
    let grant = project_grant(&api, ALPHA, "g-1");
    let first = propose(&api, "t1", &task_action(&code, "First."));
    claim_only(&api, &first[0], &grant);
    let revoked = call(
        &api,
        "grants.revoke",
        json!({"grant_id": grant, "idempotency_key": "r-1"}),
    )
    .unwrap();
    assert_eq!(revoked["stopped"], 1, "{revoked}");
    assert_eq!(revoked["grant"]["status"], "revoked", "{revoked}");
    let stopped = proposal(&api, &first[0]);
    assert_eq!(stopped.state, ProposalState::Failed);
    assert_eq!(stopped.error_code.as_deref(), Some("grant_revoked"));
    assert_eq!(task_count(&api), 0);
    assert_eq!(
        event_kinds(&api, &grant),
        [
            GrantEventKind::Created,
            GrantEventKind::Used,
            GrantEventKind::Revoked,
            GrantEventKind::Stopped
        ]
    );
    assert_eq!(tick(&api), MatchReport::default());

    // A revoke that did not stop it (the store call alone) is still caught before the start.
    let other = project_grant(&api, ALPHA, "g-2");
    let second = propose(&api, "t2", &task_action(&code, "Second."));
    claim_only(&api, &second[0], &other);
    api.store.revoke_standing_grant(&other, now()).unwrap();

    let report = tick(&api);

    assert_eq!(report.reconciled, 1);
    let failed = proposal(&api, &second[0]);
    assert_eq!(failed.state, ProposalState::Failed);
    assert_eq!(failed.error_code.as_deref(), Some("grant_revoked"));
    assert_eq!(task_count(&api), 0);
    let actors: Vec<String> = api
        .store
        .action_proposal_events(&second[0])
        .unwrap()
        .into_iter()
        .skip(1)
        .map(|event| event.actor)
        .collect();
    assert_eq!(actors, [format!("grant:{other}"), format!("grant:{other}")]);
}

#[test]
fn a_grant_approval_still_hits_the_project_defaults_recheck() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let code = project_code(ALPHA);
    let grant = project_grant(&api, ALPHA, "g-1");
    // A scout may only deliver a report, so the envelope refuses a local review of one. The
    // card was stored unchecked here, so the approve-time re-check is the only thing that stops it.
    let denied = propose(
        &api,
        "t1",
        &json!({
            "kind": "task.propose",
            "project": code,
            "objective": "Scout then review.",
            "agent": "codex",
            "task_kind": "Investigate",
            "delivery": "LocalReview",
        })
        .to_string(),
    );
    // A pull request is outside what a grant covers, whatever the project's mode and setting.
    api.store
        .update_orchestrator_settings(&orchestrator_store::SettingsPatch {
            chat_pr_delivery: Some(true),
            ..orchestrator_store::SettingsPatch::default()
        })
        .unwrap();
    api.store
        .set_project_defaults(&orchestrator_store::ProjectDefaultsRow {
            project_id: ALPHA.to_owned(),
            mode: "no-mistakes".to_owned(),
            base_branch: None,
            branch_prefix: None,
            checks: vec![vec!["cargo".to_owned(), "test".to_owned()]],
            merge_method: "squash".to_owned(),
            updated_at: String::new(),
        })
        .unwrap();
    let pull_request = propose(
        &api,
        "t2",
        &json!({
            "kind": "task.propose",
            "project": code,
            "objective": "Open a pull request.",
            "agent": "codex",
            "delivery": "PullRequest",
        })
        .to_string(),
    );

    let report = tick(&api);

    assert_eq!(report.approved, 0);
    let failed = proposal(&api, &denied[0]);
    assert_eq!(failed.state, ProposalState::Failed);
    assert_eq!(failed.error_code.as_deref(), Some("action_denied"));
    assert_eq!(failed.denial.as_deref(), Some("delivery_not_authorized"));
    assert_eq!(state_of(&api, &pull_request[0]), ProposalState::Pending);
    // No use was spent and no task made.
    assert_eq!(uses_used(&api, &grant), 0);
    assert_eq!(event_kinds(&api, &grant), [GrantEventKind::Created]);
    assert_eq!(task_count(&api), 0);
}

#[test]
fn a_failed_claim_leaves_no_use_and_a_refused_use_leaves_no_approval() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let code = project_code(ALPHA);
    let grant = project_grant(&api, ALPHA, "g-1");
    let rejected = propose(&api, "t1", &task_action(&code, "A user said no."));
    resolve(&api, &rejected[0], "reject", "k-1").unwrap();

    // The approval cannot happen, so no use is spent.
    let digest = proposal(&api, &rejected[0]).digest;
    let claim = api
        .store
        .approve_action_proposal_by_grant(&rejected[0], &digest, &grant, now());
    assert!(matches!(claim, Err(GrantClaimError::Proposal(_))));
    assert_eq!(uses_used(&api, &grant), 0);
    assert_eq!(event_kinds(&api, &grant), [GrantEventKind::Created]);

    // The use cannot be spent, so nothing is approved.
    let spent = grant_with(
        &api,
        json!({"kind": "project", "id": code}),
        json!(["task.propose"]),
        "g-2",
        json!({"max_uses": 1}),
    );
    api.store.revoke_standing_grant(&grant, now()).unwrap();
    let fresh = propose(&api, "t2", &task_action(&code, "Fresh."));
    let digest = proposal(&api, &fresh[0]).digest;
    api.store
        .approve_action_proposal_by_grant(&fresh[0], &digest, &spent, now())
        .unwrap();
    let other = propose(&api, "t3", &task_action(&code, "Other."));
    let digest = proposal(&api, &other[0]).digest;
    let refused = api
        .store
        .approve_action_proposal_by_grant(&other[0], &digest, &spent, now());
    assert!(matches!(
        refused,
        Err(GrantClaimError::Grant(GrantError::UsesExhausted))
    ));
    assert_eq!(state_of(&api, &other[0]), ProposalState::Pending);

    // The matcher's claim writes the use and the approval together.
    let events = api.store.action_proposal_events(&fresh[0]).unwrap();
    assert_eq!(events.last().unwrap().actor, format!("grant:{spent}"));
    let used = api
        .store
        .grant_events(&spent, 0, 100)
        .unwrap()
        .into_iter()
        .filter(|event| event.event == GrantEventKind::Used)
        .count();
    assert_eq!(used, 1);
    assert_eq!(uses_used(&api, &spent), 1);
}

#[test]
fn a_restart_between_consume_and_execute_finishes_without_a_second_use() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let grant = project_grant(&api, ALPHA, "g-1");
    let ids = propose(
        &api,
        "t1",
        &task_action(&project_code(ALPHA), "Interrupted."),
    );
    claim_only(&api, &ids[0], &grant);
    assert_eq!(state_of(&api, &ids[0]), ProposalState::Approved);
    assert_eq!(task_count(&api), 0);

    // The next start finishes it, as it finishes a user's approved proposal.
    api.reconcile_actions();

    let task_id = proposed_task_id(&ids[0]);
    let done = proposal(&api, &ids[0]);
    assert_eq!(done.state, ProposalState::Executed);
    assert_eq!(done.result_ref.as_deref(), Some(task_id.as_str()));
    assert_eq!(uses_used(&api, &grant), 1);
    assert_eq!(
        event_kinds(&api, &grant),
        [
            GrantEventKind::Created,
            GrantEventKind::Used,
            GrantEventKind::Executed
        ]
    );
    let actors: Vec<String> = api
        .store
        .action_proposal_events(&ids[0])
        .unwrap()
        .into_iter()
        .skip(1)
        .map(|event| event.actor)
        .collect();
    assert_eq!(actors, [format!("grant:{grant}"), format!("grant:{grant}")]);
    // Another start, and another pass, change nothing.
    api.reconcile_actions();
    assert_eq!(tick(&api), MatchReport::default());
    assert_eq!(uses_used(&api, &grant), 1);
    assert_eq!(task_count(&api), 1);
}

#[test]
fn a_user_approval_keeps_the_user_actor_and_is_never_a_grant_use() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let ids = propose(&api, "t1", &task_action(&project_code(ALPHA), "By hand."));
    let result = resolve(&api, &ids[0], "approve", "k-1").unwrap();
    assert_eq!(result["proposal"]["state"], "executed", "{result}");
    assert_eq!(result["proposal"]["grant_id"], Value::Null, "{result}");
    let actors: Vec<String> = api
        .store
        .action_proposal_events(&ids[0])
        .unwrap()
        .into_iter()
        .skip(1)
        .map(|event| event.actor)
        .collect();
    assert_eq!(actors, ["user", "user"]);
}

#[test]
fn a_grant_event_error_or_card_never_contains_a_path_or_token() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let code = project_code(ALPHA);
    let grant = project_grant(&api, ALPHA, "g-1");
    let ids = propose(&api, "t1", &task_action(&code, "Add unit tests."));
    tick(&api);
    let mut seen = Vec::new();
    seen.push(
        call(&api, "grants.list", json!({"include_ended": true}))
            .unwrap()
            .to_string(),
    );
    seen.push(format!(
        "{:?}",
        api.store.grant_events(&grant, 0, 100).unwrap()
    ));
    seen.push(format!(
        "{:?}",
        api.store.action_proposal_events(&ids[0]).unwrap()
    ));
    seen.push(format!("{:?}", proposal(&api, &ids[0])));
    seen.push(
        call(&api, "actions.pending", json!({}))
            .unwrap()
            .to_string(),
    );
    seen.push(
        call(
            &api,
            "grants.revoke",
            json!({"grant_id": grant, "idempotency_key": "r-1"}),
        )
        .unwrap()
        .to_string(),
    );
    let failures = [
        call(
            &api,
            "grants.create",
            json!({
                "scope": {"kind": "project", "id": "p-00000000"},
                "kinds": ["task.propose"],
                "token_budget": 1000,
                "idempotency_key": "g-2",
            }),
        ),
        call(
            &api,
            "grants.create",
            json!({
                "scope": {"kind": "project", "id": code},
                "kinds": ["project.remove"],
                "token_budget": 1000,
                "idempotency_key": "g-3",
            }),
        ),
        call(
            &api,
            "grants.revoke",
            json!({"grant_id": "grant-missing", "idempotency_key": "r-2"}),
        ),
    ];
    for failure in failures {
        seen.push(format!("{:?}", failure.unwrap_err()));
    }
    for text in seen {
        assert!(!text.contains("canary-root"), "{text}");
        assert!(!text.contains(ALPHA), "{text}");
    }
}

#[test]
fn a_proposal_that_keeps_failing_to_match_stops_blocking_newer_ones() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let code = project_code(ALPHA);
    let grant = project_grant(&api, ALPHA, "g-1");
    let stuck = propose(&api, "t1", &task_action(&code, "Cannot be matched."));
    api.grant_try_state
        .lock()
        .unwrap()
        .forced_errors
        .insert(stuck[0].clone());
    let untried = |api: &DaemonApi| api.store.untried_pending_proposals(0, 50).unwrap().len();

    // Two failures leave it untried: it is tried again.
    assert_eq!(tick(&api), MatchReport::default());
    assert_eq!(tick(&api), MatchReport::default());
    assert_eq!(untried(&api), 1);
    // The third marks it tried: later passes no longer read it.
    assert_eq!(tick(&api), MatchReport::default());
    assert_eq!(untried(&api), 0);
    assert_eq!(state_of(&api, &stuck[0]), ProposalState::Pending);

    // A newer proposal is matched, and the failed one is left to the user.
    let newer = propose(&api, "t3", &task_action(&code, "Newer."));
    let report = tick(&api);
    assert_eq!(report.approved, 1);
    assert_eq!(state_of(&api, &newer[0]), ProposalState::Executed);
    assert_eq!(state_of(&api, &stuck[0]), ProposalState::Pending);
    assert_eq!(uses_used(&api, &grant), 1);
}

#[test]
fn a_transient_grant_read_error_leaves_the_approved_proposal_for_a_retry() {
    let (_directory, api) = new_api();
    register_project(&api, ALPHA);
    let code = project_code(ALPHA);
    let grant = project_grant(&api, ALPHA, "g-1");
    let first = propose(&api, "t1", &task_action(&code, "First."));
    claim_only(&api, &first[0], &grant);

    // The grant cannot be read: the proposal is neither failed nor started.
    FORCED_GRANT_READ_ERRORS
        .lock()
        .unwrap()
        .insert(grant.clone());
    let report = tick(&api);
    assert_eq!(report.reconciled, 0);
    assert_eq!(state_of(&api, &first[0]), ProposalState::Approved);
    assert_eq!(task_count(&api), 0);
    assert_eq!(
        event_kinds(&api, &grant),
        [GrantEventKind::Created, GrantEventKind::Used]
    );

    // The read works again: the next pass runs it.
    FORCED_GRANT_READ_ERRORS.lock().unwrap().remove(&grant);
    let report = tick(&api);
    assert_eq!(report.reconciled, 1);
    assert_eq!(state_of(&api, &first[0]), ProposalState::Executed);
    assert_eq!(task_count(&api), 1);
}
