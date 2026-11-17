//! The grant approval claim and the revoke that stops approved proposals (DPES-1105 phase 5a).
//!
//! Every test uses the store's public API. Time is injected: the grant is compared with the
//! `now_ms` each call passes, so no test sleeps.

use std::path::Path;

use orchestrator_core::orchestrator_action::{parse_block, ActionKind, BlockScope};
use orchestrator_core::standing_grant::{GrantScope, GrantTerms};
use orchestrator_store::{
    ConversationTarget, GrantClaimError, GrantError, GrantEventKind, ProposalState, ResolveError,
    SqliteStore, TurnBlock,
};

/// 2026-11-17 in the afternoon, UTC.
const NOW: u64 = 1_794_901_798_000;
const TASK: &str = r#"{"kind": "task.propose", "project": "p-3fa9c1d2", "objective": "Add unit tests for the retry module.", "agent": "codex"}"#;
const OTHER_TASK: &str = r#"{"kind": "task.propose", "project": "p-3fa9c1d2", "objective": "Document the retry module.", "agent": "codex"}"#;
const WRONG_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn open(directory: &Path) -> SqliteStore {
    SqliteStore::new(&directory.join("store.sqlite")).unwrap()
}

fn terms(max_uses: u32) -> GrantTerms {
    GrantTerms {
        scope: GrantScope::Project("p-3fa9c1d2".to_owned()),
        kinds: vec![ActionKind::TaskPropose],
        expires_in_secs: 3600,
        max_uses,
        token_budget: 1000,
    }
}

/// One pending proposal made by `turn`, and its digest.
fn pending(store: &SqliteStore, turn: &str, action: &str) -> (String, String) {
    store
        .begin_conversation_turn(
            &ConversationTarget::Global,
            turn,
            "hi",
            "test-runtime",
            "1.0.0",
        )
        .unwrap();
    let held = format!("```mcao-actions\n{{\"mcao_actions\": 1, \"actions\": [{action}]}}\n```");
    let block = parse_block(&held, BlockScope::Global).unwrap();
    let ids = store
        .complete_conversation_turn(turn, TurnBlock::Accepted(block))
        .unwrap()
        .unwrap();
    let proposal = store.action_proposal(&ids[0]).unwrap().unwrap();
    (proposal.id, proposal.digest)
}

fn event_kinds(store: &SqliteStore, grant: &str) -> Vec<GrantEventKind> {
    store
        .grant_events(grant, 0, 100)
        .unwrap()
        .into_iter()
        .map(|event| event.event)
        .collect()
}

#[test]
fn a_claim_approves_the_proposal_and_spends_one_use_together() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let grant = store
        .create_standing_grant("grant-1", &terms(2), NOW)
        .unwrap();
    let (id, digest) = pending(&store, "t1", TASK);

    let approved = store
        .approve_action_proposal_by_grant(&id, &digest, &grant.id, NOW + 1)
        .unwrap();

    assert_eq!(approved.state, ProposalState::Approved);
    assert_eq!(
        store.standing_grant("grant-1").unwrap().unwrap().uses_used,
        1
    );
    assert_eq!(
        event_kinds(&store, "grant-1"),
        [GrantEventKind::Created, GrantEventKind::Used]
    );
    let events = store.action_proposal_events(&id).unwrap();
    let last = events.last().unwrap();
    assert_eq!(last.to_state, ProposalState::Approved);
    assert_eq!(last.actor, "grant:grant-1");
    assert_eq!(
        store.proposal_approver(&id).unwrap().as_deref(),
        Some("grant:grant-1")
    );
    let by_grant = store.approved_by_grant().unwrap();
    assert_eq!(by_grant.len(), 1);
    assert_eq!(by_grant[0].0.id, id);
    assert_eq!(by_grant[0].1, "grant-1");
}

#[test]
fn a_claim_that_fails_spends_no_use_and_writes_no_event() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    store
        .create_standing_grant("grant-1", &terms(2), NOW)
        .unwrap();
    let (id, digest) = pending(&store, "t1", TASK);

    // A digest the proposal does not have.
    let stale = store.approve_action_proposal_by_grant(&id, WRONG_DIGEST, "grant-1", NOW + 1);
    assert!(matches!(stale, Err(GrantClaimError::DigestChanged)));
    // An expired grant.
    let expired = store.approve_action_proposal_by_grant(&id, &digest, "grant-1", NOW + 3_600_000);
    assert!(matches!(
        expired,
        Err(GrantClaimError::Grant(GrantError::Expired))
    ));
    // An unknown grant.
    let unknown = store.approve_action_proposal_by_grant(&id, &digest, "grant-9", NOW + 1);
    assert!(matches!(
        unknown,
        Err(GrantClaimError::Grant(GrantError::NotFound))
    ));
    // A proposal a user rejected first.
    store.reject_action_proposal(&id, "user").unwrap();
    let rejected = store.approve_action_proposal_by_grant(&id, &digest, "grant-1", NOW + 1);
    assert!(matches!(
        rejected,
        Err(GrantClaimError::Proposal(ResolveError::NotPending(
            ProposalState::Rejected
        )))
    ));
    let missing = store.approve_action_proposal_by_grant("nope", &digest, "grant-1", NOW + 1);
    assert!(matches!(
        missing,
        Err(GrantClaimError::Proposal(ResolveError::NotFound))
    ));

    assert_eq!(
        store.standing_grant("grant-1").unwrap().unwrap().uses_used,
        0
    );
    assert_eq!(event_kinds(&store, "grant-1"), [GrantEventKind::Created]);
}

#[test]
fn a_claim_for_the_last_use_leaves_the_next_proposal_pending() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    store
        .create_standing_grant("grant-1", &terms(1), NOW)
        .unwrap();
    let (first, first_digest) = pending(&store, "t1", TASK);
    let (second, second_digest) = pending(&store, "t2", OTHER_TASK);

    store
        .approve_action_proposal_by_grant(&first, &first_digest, "grant-1", NOW + 1)
        .unwrap();
    let spent = store.approve_action_proposal_by_grant(&second, &second_digest, "grant-1", NOW + 2);

    assert!(matches!(
        spent,
        Err(GrantClaimError::Grant(GrantError::UsesExhausted))
    ));
    let still = store.action_proposal(&second).unwrap().unwrap();
    assert_eq!(still.state, ProposalState::Pending);
    assert_eq!(store.action_proposal_events(&second).unwrap().len(), 1);
}

#[test]
fn a_revoke_fails_the_proposals_a_grant_approved_and_closes_their_uses() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    store
        .create_standing_grant("grant-1", &terms(5), NOW)
        .unwrap();
    let (waiting, waiting_digest) = pending(&store, "t1", TASK);
    let (user, _) = pending(&store, "t2", OTHER_TASK);
    store
        .approve_action_proposal_by_grant(&waiting, &waiting_digest, "grant-1", NOW + 1)
        .unwrap();
    // A user approval is not the grant's to stop.
    store.approve_action_proposal(&user, "user").unwrap();

    let (grant, stopped) = store
        .revoke_standing_grant_and_stop("grant-1", NOW + 2)
        .unwrap();

    assert_eq!(stopped, 1);
    assert_eq!(grant.revoked_at_ms, Some(NOW + 2));
    let failed = store.action_proposal(&waiting).unwrap().unwrap();
    assert_eq!(failed.state, ProposalState::Failed);
    assert_eq!(failed.error_code.as_deref(), Some("grant_revoked"));
    assert_eq!(
        store.action_proposal(&user).unwrap().unwrap().state,
        ProposalState::Approved
    );
    let events = store.grant_events("grant-1", 0, 100).unwrap();
    let stopped_event = events.last().unwrap();
    assert_eq!(stopped_event.event, GrantEventKind::Stopped);
    assert_eq!(stopped_event.code.as_deref(), Some("grant_revoked"));
    assert_eq!(stopped_event.proposal_id.as_deref(), Some(waiting.as_str()));

    // A second revoke succeeds and stops nothing more.
    let (_, again) = store
        .revoke_standing_grant_and_stop("grant-1", NOW + 3)
        .unwrap();
    assert_eq!(again, 0);
    // The revoked grant takes no new claim, and the use already spent is not refunded.
    let (late, late_digest) = pending(&store, "t3", "{\"kind\": \"task.propose\", \"project\": \"p-3fa9c1d2\", \"objective\": \"Third.\", \"agent\": \"codex\"}");
    let refused = store.approve_action_proposal_by_grant(&late, &late_digest, "grant-1", NOW + 4);
    assert!(matches!(
        refused,
        Err(GrantClaimError::Grant(GrantError::Revoked))
    ));
    assert_eq!(
        store.standing_grant("grant-1").unwrap().unwrap().uses_used,
        1
    );
    assert!(store
        .revoke_standing_grant_and_stop("grant-9", NOW)
        .is_err());
}

#[test]
fn the_matcher_reads_only_untried_proposals_created_after_the_floor() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let (old, _) = pending(&store, "t1", TASK);
    let grant = store
        .create_standing_grant("grant-1", &terms(5), NOW)
        .unwrap();
    let (new, _) = pending(&store, "t2", OTHER_TASK);

    let found = store
        .untried_pending_proposals(grant.proposal_floor, 10)
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0.id, new);
    assert!(found[0].1 > grant.proposal_floor);
    // Without a floor the older one is listed too, with a lower sequence.
    let all = store.untried_pending_proposals(0, 10).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].0.id, old);
    assert!(all[0].1 <= grant.proposal_floor);

    store.mark_grant_tried(&new).unwrap();
    assert!(store
        .untried_pending_proposals(grant.proposal_floor, 10)
        .unwrap()
        .is_empty());
}
