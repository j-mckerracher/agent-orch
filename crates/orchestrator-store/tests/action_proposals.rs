//! Action proposals stored when a turn completes (DPES-1105).
//!
//! These tests use the store's public API. A raw connection moves a card to
//! `executed`, because no API does that before phase 1c.

use std::path::Path;

use orchestrator_core::orchestrator_action::{parse_block, proposal_id, ActionKind, BlockScope};
use orchestrator_store::{
    ActionBlockOutcome, ConversationTarget, ProposalState, SqliteStore, TurnBlock, TurnOutcome,
    TurnState,
};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

const CREATE_ALPHA: &str = r#"{"kind": "workstream.create", "ref": "w1", "title": "Alpha"}"#;
const PLAN_ALPHA: &str = r#"{"kind": "objective.add", "workstream_ref": "w1", "title": "Plan"}"#;

fn open(directory: &Path) -> SqliteStore {
    SqliteStore::new(&directory.join("store.sqlite")).unwrap()
}

fn begin(store: &SqliteStore, target: &ConversationTarget, turn: &str) {
    store
        .begin_conversation_turn(target, turn, "hi", "test-runtime", "1.0.0")
        .unwrap();
}

/// An accepted block, parsed from the fenced text a reply would carry.
fn accepted(scope: BlockScope<'_>, actions: &str) -> TurnBlock {
    let held = format!("```mcao-actions\n{{\"mcao_actions\": 1, \"actions\": [{actions}]}}\n```");
    TurnBlock::Accepted(parse_block(&held, scope).unwrap())
}

/// Runs one statement on a second connection, as another process would, with
/// `binds` bound in order.
fn run_sql(path: &Path, sql: &'static str, binds: &[&str]) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        let mut query = sqlx::query(sql);
        for value in binds {
            query = query.bind(*value);
        }
        query.execute(&mut connection).await.unwrap();
    });
}

#[test]
fn an_accepted_block_stores_pending_proposals_and_events_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    begin(&store, &ConversationTarget::Global, "t1");
    let ids = store
        .complete_conversation_turn(
            "t1",
            accepted(BlockScope::Global, &format!("{CREATE_ALPHA}, {PLAN_ALPHA}")),
        )
        .unwrap()
        .unwrap();
    assert_eq!(ids, vec![proposal_id("t1", 0), proposal_id("t1", 1)]);
    assert_eq!(
        store.conversation_turn("t1").unwrap().unwrap().action_block,
        Some(ActionBlockOutcome::Accepted)
    );

    // Newest first: the objective was inserted after the workstream it joins.
    let proposals = store
        .action_proposals(&ConversationTarget::Global, 10)
        .unwrap();
    assert_eq!(proposals.len(), 2);
    let objective = &proposals[0];
    let create = &proposals[1];
    assert_eq!(create.id, proposal_id("t1", 0));
    assert_eq!(create.kind, ActionKind::WorkstreamCreate);
    assert_eq!(create.state, ProposalState::Pending);
    assert_eq!(objective.kind, ActionKind::ObjectiveAdd);
    assert_eq!(objective.depends_on.as_deref(), Some(create.id.as_str()));

    let events = store.action_proposal_events(&create.id).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].from_state, None);
    assert_eq!(events[0].to_state, ProposalState::Pending);
    assert_eq!(events[0].actor, "agent");
    assert_eq!(events[0].digest, create.digest);
}

#[test]
fn the_same_request_twice_returns_the_existing_cards_including_dependents() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let both = format!("{CREATE_ALPHA}, {PLAN_ALPHA}");

    begin(&store, &ConversationTarget::Global, "t1");
    let first = store
        .complete_conversation_turn("t1", accepted(BlockScope::Global, &both))
        .unwrap()
        .unwrap();
    begin(&store, &ConversationTarget::Global, "t2");
    let second = store
        .complete_conversation_turn("t2", accepted(BlockScope::Global, &both))
        .unwrap()
        .unwrap();

    // The child matches because its digest binds the parent's existing id.
    assert_eq!(second, first);
    assert_eq!(
        store
            .action_proposals(&ConversationTarget::Global, 10)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(store.action_proposal_events(&first[1]).unwrap().len(), 1);
    assert_eq!(
        store.conversation_turn("t2").unwrap().unwrap().action_block,
        Some(ActionBlockOutcome::Accepted)
    );
}

#[test]
fn a_changed_request_supersedes_the_older_pending_one_with_an_event() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let target = ConversationTarget::Global;

    begin(&store, &target, "t1");
    let first = store
        .complete_conversation_turn(
            "t1",
            accepted(BlockScope::Global, &format!("{CREATE_ALPHA}, {PLAN_ALPHA}")),
        )
        .unwrap()
        .unwrap();
    // Same title, so the same subject, but a different summary, so a different digest.
    let revised =
        r#"{"kind": "workstream.create", "ref": "w1", "title": "Alpha", "summary": "Revised"}"#;
    begin(&store, &target, "t2");
    let second = store
        .complete_conversation_turn(
            "t2",
            accepted(BlockScope::Global, &format!("{revised}, {PLAN_ALPHA}")),
        )
        .unwrap()
        .unwrap();

    assert_eq!(
        store.action_proposal(&first[0]).unwrap().unwrap().state,
        ProposalState::Superseded
    );
    let events = store.action_proposal_events(&first[0]).unwrap();
    let transitions: Vec<ProposalState> = events.iter().map(|event| event.to_state).collect();
    assert_eq!(
        transitions,
        vec![ProposalState::Pending, ProposalState::Superseded]
    );
    assert_eq!(events[1].from_state, Some(ProposalState::Pending));
    assert_eq!(events[1].actor, "agent");

    // The objective built on the superseded workstream goes with it (Q3). Its
    // replacement is built on the new workstream and stays pending.
    assert_eq!(
        store.action_proposal(&first[1]).unwrap().unwrap().state,
        ProposalState::Superseded
    );
    assert_eq!(
        store.action_proposal(&second[0]).unwrap().unwrap().state,
        ProposalState::Pending
    );
    assert_eq!(
        store.action_proposal(&second[1]).unwrap().unwrap().state,
        ProposalState::Pending
    );
}

#[test]
fn non_pending_proposals_are_never_deduplicated_or_superseded() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let path = directory.path().join("store.sqlite");

    begin(&store, &ConversationTarget::Global, "t1");
    let first = store
        .complete_conversation_turn("t1", accepted(BlockScope::Global, CREATE_ALPHA))
        .unwrap()
        .unwrap();
    // No API moves a card past pending yet, so an outside writer does it, one legal
    // transition at a time.
    run_sql(
        &path,
        "UPDATE action_proposals SET state = 'approved' WHERE id = ?",
        &[&first[0]],
    );
    run_sql(
        &path,
        "UPDATE action_proposals SET state = 'executed' WHERE id = ?",
        &[&first[0]],
    );

    // The same request is a new card, and the executed one keeps its state and history.
    begin(&store, &ConversationTarget::Global, "t2");
    let second = store
        .complete_conversation_turn("t2", accepted(BlockScope::Global, CREATE_ALPHA))
        .unwrap()
        .unwrap();
    assert_eq!(second, vec![proposal_id("t2", 0)]);
    assert_eq!(
        store.action_proposal(&first[0]).unwrap().unwrap().state,
        ProposalState::Executed
    );
    assert_eq!(store.action_proposal_events(&first[0]).unwrap().len(), 1);
    assert_eq!(
        store.action_proposal(&second[0]).unwrap().unwrap().state,
        ProposalState::Pending
    );
}

#[test]
fn a_turn_already_cancelled_or_failed_records_no_block() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let target = ConversationTarget::Global;

    begin(&store, &target, "t1");
    store
        .finish_conversation_turn("t1", &TurnOutcome::Cancelled)
        .unwrap();
    let cancelled = store
        .complete_conversation_turn("t1", accepted(BlockScope::Global, CREATE_ALPHA))
        .unwrap();
    assert_eq!(cancelled, None);

    begin(&store, &target, "t2");
    store
        .finish_conversation_turn("t2", &TurnOutcome::Failed("runtime_failed".to_owned()))
        .unwrap();
    let failed = store
        .complete_conversation_turn("t2", accepted(BlockScope::Global, CREATE_ALPHA))
        .unwrap();
    assert_eq!(failed, None);

    assert!(store.action_proposals(&target, 10).unwrap().is_empty());
    assert_eq!(
        store.conversation_turn("t1").unwrap().unwrap().action_block,
        None
    );
    assert_eq!(
        store.conversation_turn("t2").unwrap().unwrap().action_block,
        None
    );
}

#[test]
fn stripped_rejected_and_absent_blocks_set_the_outcome_without_proposals() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let target = ConversationTarget::Global;

    begin(&store, &target, "t1");
    let stripped = store
        .complete_conversation_turn("t1", TurnBlock::Stripped)
        .unwrap();
    begin(&store, &target, "t2");
    let rejected = store
        .complete_conversation_turn("t2", TurnBlock::Rejected)
        .unwrap();
    begin(&store, &target, "t3");
    let absent = store
        .complete_conversation_turn("t3", TurnBlock::None)
        .unwrap();

    assert_eq!(stripped, Some(Vec::new()));
    assert_eq!(rejected, Some(Vec::new()));
    assert_eq!(absent, Some(Vec::new()));
    let turn = |id: &str| store.conversation_turn(id).unwrap().unwrap();
    assert_eq!(turn("t1").action_block, Some(ActionBlockOutcome::Stripped));
    assert_eq!(turn("t2").action_block, Some(ActionBlockOutcome::Rejected));
    assert_eq!(turn("t3").action_block, None);
    assert_eq!(turn("t3").state, TurnState::Completed);
    assert!(store.action_proposals(&target, 10).unwrap().is_empty());
}

#[test]
fn proposals_list_by_scope_newest_first_and_get_by_id() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let global = ConversationTarget::Global;
    let workstream = ConversationTarget::Workstream("ws-1".to_owned());

    begin(&store, &global, "t1");
    store
        .complete_conversation_turn("t1", accepted(BlockScope::Global, CREATE_ALPHA))
        .unwrap();
    begin(&store, &workstream, "t2");
    store
        .complete_conversation_turn(
            "t2",
            accepted(
                BlockScope::Workstream("ws-1"),
                r#"{"kind": "objective.add", "workstream_id": "ws-1", "title": "Plan"}"#,
            ),
        )
        .unwrap();
    begin(&store, &global, "t3");
    store
        .complete_conversation_turn(
            "t3",
            accepted(
                BlockScope::Global,
                r#"{"kind": "workstream.create", "title": "Beta"}"#,
            ),
        )
        .unwrap();

    let listed = store.action_proposals(&global, 10).unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].turn_id, "t3");
    assert_eq!(listed[1].turn_id, "t1");
    assert_eq!(store.action_proposals(&global, 1).unwrap().len(), 1);

    let in_workstream = store.action_proposals(&workstream, 10).unwrap();
    assert_eq!(in_workstream.len(), 1);
    assert_eq!(in_workstream[0].kind, ActionKind::ObjectiveAdd);
    assert_eq!(in_workstream[0].scope, "conversation:workstream:ws-1");

    let by_id = store
        .action_proposal(&proposal_id("t2", 0))
        .unwrap()
        .unwrap();
    assert_eq!(by_id, in_workstream[0]);
    assert!(store.action_proposal("proposal-missing").unwrap().is_none());
}
