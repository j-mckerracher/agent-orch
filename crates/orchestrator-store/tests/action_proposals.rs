//! Action proposals stored when a turn completes (DPES-1105).
//!
//! These tests use the store's public API. A raw connection moves a card to
//! `executed`, because no API does that before phase 1c.

use std::fs;
use std::path::{Path, PathBuf};

use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::orchestrator_action::{
    parse_block, proposal_id, ActionBlock, ActionKind, BlockScope,
};
use orchestrator_core::records::Project;
use orchestrator_core::task_envelope::{chat_task_envelope, ACTION_DENIED};
use orchestrator_core::workstream::Workstream;
use orchestrator_store::{
    ActionBlockOutcome, ActionOutcome, ConversationTarget, ProposalState, ResolveError,
    SqliteStore, Store, TurnBlock, TurnOutcome, TurnState, DEPENDENCY_NOT_EXECUTED,
};
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

const CREATE_ALPHA: &str = r#"{"kind": "workstream.create", "ref": "w1", "title": "Alpha"}"#;
const PLAN_ALPHA: &str = r#"{"kind": "objective.add", "workstream_ref": "w1", "title": "Plan"}"#;

// The four task kinds. PROPOSE_TASK passes the chat envelope; the pull request variants do
// not, because the envelope grants report-only and local-review delivery only.
const PROPOSE_TASK: &str = r#"{"kind": "task.propose", "project": "p-3fa9c1d2", "objective": "Add unit tests for the retry module.", "agent": "codex"}"#;
const PROPOSE_TASK_AS_PULL_REQUEST: &str = r#"{"kind": "task.propose", "project": "p-3fa9c1d2", "objective": "Add unit tests for the retry module.", "agent": "codex", "delivery": "PullRequest"}"#;
const PROPOSE_PULL_REQUEST: &str = r#"{"kind": "task.propose", "project": "p-3fa9c1d2", "objective": "Document the retry module.", "agent": "codex", "delivery": "PullRequest"}"#;
const CANCEL_TASK: &str = r#"{"kind": "task.cancel", "task_id": "task-aaaa0001"}"#;
const RETRY_TASK: &str = r#"{"kind": "task.retry", "task_id": "task-aaaa0002"}"#;
const LINK_TASK: &str = r#"{"kind": "workstream.link_task", "workstream_id": "workstream-1", "task_id": "task-aaaa0003"}"#;
const NEW_KINDS: [&str; 4] = [PROPOSE_TASK, CANCEL_TASK, RETRY_TASK, LINK_TASK];
const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn open(directory: &Path) -> SqliteStore {
    SqliteStore::new(&directory.join("store.sqlite")).unwrap()
}

fn begin(store: &SqliteStore, target: &ConversationTarget, turn: &str) {
    store
        .begin_conversation_turn(target, turn, "hi", "test-runtime", "1.0.0")
        .unwrap();
}

/// A block parsed from the fenced text a reply would carry.
fn parsed(scope: BlockScope<'_>, actions: &str) -> ActionBlock {
    let held = format!("```mcao-actions\n{{\"mcao_actions\": 1, \"actions\": [{actions}]}}\n```");
    parse_block(&held, scope).unwrap()
}

/// An accepted block that no envelope has looked at, so nothing in it is denied.
fn accepted(scope: BlockScope<'_>, actions: &str) -> TurnBlock {
    TurnBlock::Accepted(parsed(scope, actions))
}

/// An accepted block after the chat envelope checked each task proposal, as the daemon
/// does before it hands the block to the store.
fn checked(scope: BlockScope<'_>, actions: &str) -> TurnBlock {
    TurnBlock::Accepted(parsed(scope, actions).checked(&chat_task_envelope()))
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Runs one statement on a second connection, as another process would, with
/// `binds` bound in order, and reports whether the database took it.
fn try_sql(path: &Path, sql: &'static str, binds: &[&str]) -> Result<(), sqlx::Error> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
        let mut query = sqlx::query(sql);
        for value in binds {
            query = query.bind(*value);
        }
        query.execute(&mut connection).await?;
        Ok::<(), sqlx::Error>(())
    })
}

fn run_sql(path: &Path, sql: &'static str, binds: &[&str]) {
    try_sql(path, sql, binds).unwrap();
}

/// The first column of every row `sql` returns, as text, read on a second connection.
fn scalars(path: &Path, sql: &'static str) -> Vec<String> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        sqlx::query_scalar::<_, String>(sql)
            .fetch_all(&mut connection)
            .await
            .unwrap()
    })
}

/// One `task.propose` row for turn t1 with the given state, error code and denial, for
/// checking which combinations the database takes. Binds: id, index, digest.
macro_rules! raw_task_row {
    ($tail:literal) => {
        concat!(
            "INSERT INTO action_proposals \
             (id, scope, turn_id, idx, kind, params_json, digest, subject, state, error_code, denial) \
             VALUES (?, 'conversation:global', 't1', CAST(? AS INTEGER), 'task.propose', '{}', ?, 'x', ",
            $tail,
            ")"
        )
    };
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

/// One pending proposal, stored by its own turn.
fn one_pending(store: &SqliteStore, turn: &str, request: &str) -> String {
    begin(store, &ConversationTarget::Global, turn);
    store
        .complete_conversation_turn(turn, accepted(BlockScope::Global, request))
        .unwrap()
        .unwrap()
        .remove(0)
}

/// A parent workstream and the objective built on it, both pending, in one turn.
fn parent_and_dependent(store: &SqliteStore) -> (String, String) {
    begin(store, &ConversationTarget::Global, "t1");
    let both = format!("{CREATE_ALPHA}, {PLAN_ALPHA}");
    let ids = store
        .complete_conversation_turn("t1", accepted(BlockScope::Global, &both))
        .unwrap()
        .unwrap();
    (ids[0].clone(), ids[1].clone())
}

#[test]
fn approve_is_a_compare_and_set_from_pending() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let id = one_pending(&store, "t1", CREATE_ALPHA);

    // An outcome needs an approved proposal, so a pending one is refused.
    assert!(matches!(
        store.record_action_outcome(
            &id,
            ActionOutcome::Executed {
                result_ref: "workstream-1",
            },
            "user",
        ),
        Err(ResolveError::NotPending(ProposalState::Pending))
    ));

    let approved = store.approve_action_proposal(&id, "user").unwrap();
    assert_eq!(approved.state, ProposalState::Approved);
    // A second approve, and a reject after approve, both find the row approved.
    assert!(matches!(
        store.approve_action_proposal(&id, "user"),
        Err(ResolveError::NotPending(ProposalState::Approved))
    ));
    assert!(matches!(
        store.reject_action_proposal(&id, "user"),
        Err(ResolveError::NotPending(ProposalState::Approved))
    ));
    assert!(matches!(
        store.approve_action_proposal("proposal-missing", "user"),
        Err(ResolveError::NotFound)
    ));

    let events = store.action_proposal_events(&id).unwrap();
    let transitions: Vec<ProposalState> = events.iter().map(|event| event.to_state).collect();
    assert_eq!(
        transitions,
        vec![ProposalState::Pending, ProposalState::Approved]
    );
    assert_eq!(events[1].from_state, Some(ProposalState::Pending));
    assert_eq!(events[1].actor, "user");
    assert_eq!(events[1].digest, approved.digest);
}

#[test]
fn a_dependent_is_approved_only_after_its_parent_executed() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let (parent, child) = parent_and_dependent(&store);

    assert!(matches!(
        store.approve_action_proposal(&child, "user"),
        Err(ResolveError::ParentNotExecuted)
    ));
    store.approve_action_proposal(&parent, "user").unwrap();
    // Approved is not enough: the parent must have executed.
    assert!(matches!(
        store.approve_action_proposal(&child, "user"),
        Err(ResolveError::ParentNotExecuted)
    ));
    store
        .record_action_outcome(
            &parent,
            ActionOutcome::Executed {
                result_ref: "workstream-1",
            },
            "user",
        )
        .unwrap();

    let approved = store.approve_action_proposal(&child, "user").unwrap();
    assert_eq!(approved.state, ProposalState::Approved);
    assert_eq!(store.action_proposal_events(&child).unwrap().len(), 2);
}

#[test]
fn outcomes_record_once_and_repeat_is_idempotent() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let id = one_pending(&store, "t1", CREATE_ALPHA);
    store.approve_action_proposal(&id, "user").unwrap();

    let executed = store
        .record_action_outcome(
            &id,
            ActionOutcome::Executed {
                result_ref: "workstream-1",
            },
            "user",
        )
        .unwrap();
    assert_eq!(executed.state, ProposalState::Executed);
    assert_eq!(executed.result_ref.as_deref(), Some("workstream-1"));
    assert_eq!(executed.error_code, None);

    // The same outcome again returns the proposal unchanged and writes no event.
    let again = store
        .record_action_outcome(
            &id,
            ActionOutcome::Executed {
                result_ref: "workstream-1",
            },
            "user",
        )
        .unwrap();
    assert_eq!(again, executed);
    assert_eq!(store.action_proposal_events(&id).unwrap().len(), 3);

    // A different outcome, or a failure after execution, is refused.
    assert!(matches!(
        store.record_action_outcome(
            &id,
            ActionOutcome::Executed {
                result_ref: "workstream-2",
            },
            "user",
        ),
        Err(ResolveError::NotPending(ProposalState::Executed))
    ));
    assert!(matches!(
        store.record_action_outcome(
            &id,
            ActionOutcome::Failed {
                error_code: "conflict",
            },
            "user",
        ),
        Err(ResolveError::NotPending(ProposalState::Executed))
    ));
    assert_eq!(store.action_proposal(&id).unwrap().unwrap(), executed);
}

#[test]
fn reject_fails_pending_dependents_with_events() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let (parent, child) = parent_and_dependent(&store);

    let rejected = store.reject_action_proposal(&parent, "user").unwrap();
    assert_eq!(rejected.state, ProposalState::Rejected);
    assert_eq!(
        store
            .action_proposal_events(&parent)
            .unwrap()
            .last()
            .unwrap()
            .actor,
        "user"
    );

    let dependent = store.action_proposal(&child).unwrap().unwrap();
    assert_eq!(dependent.state, ProposalState::Failed);
    assert_eq!(
        dependent.error_code.as_deref(),
        Some(DEPENDENCY_NOT_EXECUTED)
    );
    let events = store.action_proposal_events(&child).unwrap();
    let transitions: Vec<ProposalState> = events.iter().map(|event| event.to_state).collect();
    assert_eq!(
        transitions,
        vec![ProposalState::Pending, ProposalState::Failed]
    );
    assert_eq!(events[1].actor, "user");
    assert_eq!(
        events[1].error_code.as_deref(),
        Some(DEPENDENCY_NOT_EXECUTED)
    );
}

#[test]
fn a_failed_parent_fails_pending_dependents_with_events() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let (parent, child) = parent_and_dependent(&store);
    store.approve_action_proposal(&parent, "user").unwrap();

    let failed = store
        .record_action_outcome(
            &parent,
            ActionOutcome::Failed {
                error_code: "stale_revision",
            },
            "user",
        )
        .unwrap();
    assert_eq!(failed.state, ProposalState::Failed);
    assert_eq!(failed.error_code.as_deref(), Some("stale_revision"));

    let dependent = store.action_proposal(&child).unwrap().unwrap();
    assert_eq!(dependent.state, ProposalState::Failed);
    assert_eq!(
        dependent.error_code.as_deref(),
        Some(DEPENDENCY_NOT_EXECUTED)
    );
    let events = store.action_proposal_events(&child).unwrap();
    assert_eq!(events.last().unwrap().actor, "user");
    assert_eq!(events.last().unwrap().to_state, ProposalState::Failed);
}

#[test]
fn approved_proposals_list_only_approved_oldest_first() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let first = one_pending(&store, "t1", CREATE_ALPHA);
    let second = one_pending(
        &store,
        "t2",
        r#"{"kind": "workstream.create", "title": "Beta"}"#,
    );
    let third = one_pending(
        &store,
        "t3",
        r#"{"kind": "workstream.create", "title": "Gamma"}"#,
    );
    assert!(store.approved_action_proposals().unwrap().is_empty());

    // Approval order differs from storage order; the listing follows storage.
    store.approve_action_proposal(&third, "user").unwrap();
    store.approve_action_proposal(&first, "user").unwrap();
    store.reject_action_proposal(&second, "user").unwrap();

    let approved: Vec<String> = store
        .approved_action_proposals()
        .unwrap()
        .into_iter()
        .map(|proposal| proposal.id)
        .collect();
    assert_eq!(approved, vec![first, third]);
}

#[test]
fn scope_workstream_reads_the_workstream_id_from_the_scope() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let workstream = ConversationTarget::Workstream("ws-1".to_owned());

    begin(&store, &workstream, "t1");
    let id = store
        .complete_conversation_turn(
            "t1",
            accepted(
                BlockScope::Workstream("ws-1"),
                r#"{"kind": "objective.add", "workstream_id": "ws-1", "title": "Plan"}"#,
            ),
        )
        .unwrap()
        .unwrap()
        .remove(0);
    let proposal = store.action_proposal(&id).unwrap().unwrap();
    assert_eq!(proposal.scope_workstream(), Some("ws-1"));

    let global = one_pending(&store, "t2", CREATE_ALPHA);
    let global = store.action_proposal(&global).unwrap().unwrap();
    assert_eq!(global.scope_workstream(), None);
}

const LEGACY_STATES: [&str; 7] = [
    "pending",
    "approved",
    "executed",
    "failed",
    "rejected",
    "superseded",
    "expired",
];

const PROPOSAL_DUMP: &str = "SELECT json_array(rowid, id, scope, turn_id, idx, kind, params_json, \
    digest, subject, depends_on, state, result_ref, error_code, created_at, updated_at) \
    FROM action_proposals ORDER BY rowid";
const EVENT_DUMP: &str = "SELECT json_array(sequence, proposal_id, scope, from_state, to_state, \
    actor, digest, error_code, result_ref, created_at) \
    FROM action_proposal_events ORDER BY sequence";

/// Builds the database as 0037 left it: only the migrations up to 0037 applied, holding
/// one workstream proposal in each state, each with the events its history would have
/// written. Returns the proposals and events as dumped before 0038 runs.
fn database_at_0037(directory: &Path) -> (Vec<String>, Vec<String>) {
    let legacy_dir = directory.join("legacy-migrations");
    fs::create_dir(&legacy_dir).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for entry in fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap().path();
        let version: u32 = entry
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .split('_')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        if version <= 37 {
            fs::copy(&entry, legacy_dir.join(entry.file_name().unwrap())).unwrap();
        }
    }
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(directory.join("store.sqlite"))
            .create_if_missing(true)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        Migrator::new(legacy_dir.as_path())
            .await
            .unwrap()
            .run(&mut connection)
            .await
            .unwrap();
        let newest: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
            .fetch_one(&mut connection)
            .await
            .unwrap();
        assert_eq!(newest, 37);
        sqlx::query(
            "INSERT INTO conversations (id, target_kind) VALUES ('conversation:global', 'global')",
        )
        .execute(&mut connection)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversation_turns (id, conversation_id, state, runtime_name, runtime_version) \
             VALUES ('t1', 'conversation:global', 'completed', 'test-runtime', '1.0.0')",
        )
        .execute(&mut connection)
        .await
        .unwrap();
        for (idx, state) in LEGACY_STATES.iter().enumerate() {
            let id = format!("proposal-legacy-{state}");
            let digest = format!("{:064x}", idx + 1);
            let params = format!(r#"{{"kind":"workstream.create","title":"Legacy {state}"}}"#);
            let (result_ref, error_code, depends_on) = match *state {
                "executed" => (Some("workstream-1"), None, None),
                "failed" => (
                    None,
                    Some("dependency_not_executed"),
                    Some("proposal-legacy-pending"),
                ),
                _ => (None, None, None),
            };
            sqlx::query(
                "INSERT INTO action_proposals \
                 (id, scope, turn_id, idx, kind, params_json, digest, subject, depends_on, \
                  state, result_ref, error_code) \
                 VALUES (?, 'conversation:global', 't1', ?, 'workstream.create', ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(i64::try_from(idx).unwrap())
            .bind(&params)
            .bind(&digest)
            .bind(format!("Legacy {state}"))
            .bind(depends_on)
            .bind(*state)
            .bind(result_ref)
            .bind(error_code)
            .execute(&mut connection)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO action_proposal_events \
                 (proposal_id, scope, from_state, to_state, actor, digest) \
                 VALUES (?, 'conversation:global', NULL, 'pending', 'agent', ?)",
            )
            .bind(&id)
            .bind(&digest)
            .execute(&mut connection)
            .await
            .unwrap();
            if *state != "pending" {
                let from = if *state == "executed" {
                    "approved"
                } else {
                    "pending"
                };
                sqlx::query(
                    "INSERT INTO action_proposal_events \
                     (proposal_id, scope, from_state, to_state, actor, digest, error_code, result_ref) \
                     VALUES (?, 'conversation:global', ?, ?, 'user', ?, ?, ?)",
                )
                .bind(&id)
                .bind(from)
                .bind(*state)
                .bind(&digest)
                .bind(error_code)
                .bind(result_ref)
                .execute(&mut connection)
                .await
                .unwrap();
            }
        }
        let proposals = sqlx::query_scalar::<_, String>(PROPOSAL_DUMP)
            .fetch_all(&mut connection)
            .await
            .unwrap();
        let events = sqlx::query_scalar::<_, String>(EVENT_DUMP)
            .fetch_all(&mut connection)
            .await
            .unwrap();
        connection.close().await.unwrap();
        (proposals, events)
    })
}

#[test]
fn migration_0038_applies_to_a_0037_database_with_proposals_and_keeps_them() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let (proposals_before, events_before) = database_at_0037(directory.path());
    assert_eq!(proposals_before.len(), LEGACY_STATES.len());
    assert_eq!(events_before.len(), 2 * LEGACY_STATES.len() - 1);

    // Opening the store applies 0038 to the database that already holds them.
    let store = open(directory.path());
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(success AS TEXT) FROM _sqlx_migrations WHERE version = 38"
        ),
        vec!["1"]
    );

    // Every row and event is byte for byte what it was, in the same order, and none has
    // a denial.
    assert_eq!(scalars(&path, PROPOSAL_DUMP), proposals_before);
    assert_eq!(scalars(&path, EVENT_DUMP), events_before);
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(COUNT(*) AS TEXT) FROM action_proposals WHERE denial IS NOT NULL"
        ),
        vec!["0"]
    );
    assert!(scalars(&path, "PRAGMA foreign_key_check").is_empty());

    // The rules survived the rebuild: the same triggers and indexes exist.
    assert_eq!(
        scalars(
            &path,
            "SELECT name FROM sqlite_master WHERE type = 'trigger' \
             AND tbl_name IN ('action_proposals', 'action_proposal_events') ORDER BY name"
        ),
        vec![
            "action_proposal_events_no_delete",
            "action_proposal_events_no_update",
            // Added by 0042: the store opens every migration, so a 0037 database ends with it too.
            "action_proposals_denial_once",
            "action_proposals_fixed_parameters",
            "action_proposals_gated_delete",
            "action_proposals_state_transitions",
        ]
    );
    assert_eq!(
        scalars(
            &path,
            "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'action_proposals' \
             AND name NOT LIKE 'sqlite_autoindex%' ORDER BY name"
        ),
        vec![
            "action_proposals_by_scope",
            // Added by 0047, which this database also ends with.
            "action_proposals_grant_untried",
            "action_proposals_one_pending_digest",
            "action_proposals_pending_subject",
        ]
    );

    // The store reads the migrated rows, newest first, and the one pending proposal is
    // the only one the pending read lists.
    let listed = store
        .action_proposals(&ConversationTarget::Global, 20)
        .unwrap();
    let states: Vec<&str> = listed
        .iter()
        .rev()
        .map(|proposal| proposal.state.as_str())
        .collect();
    assert_eq!(states, LEGACY_STATES);
    assert!(listed.iter().all(|proposal| proposal.denial.is_none()));
    let pending = store.pending_action_proposals(10).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, "proposal-legacy-pending");

    // A migrated proposal still moves through its legal edges, with its event.
    store
        .approve_action_proposal("proposal-legacy-pending", "user")
        .unwrap();
    assert_eq!(
        store
            .action_proposal_events("proposal-legacy-pending")
            .unwrap()
            .len(),
        2
    );

    // And the migrated table takes a new kind and a denied task.
    begin(&store, &ConversationTarget::Global, "t2");
    let ids = store
        .complete_conversation_turn(
            "t2",
            checked(
                BlockScope::Global,
                &format!("{PROPOSE_TASK}, {PROPOSE_PULL_REQUEST}"),
            ),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        store.action_proposal(&ids[0]).unwrap().unwrap().state,
        ProposalState::Pending
    );
    assert_eq!(
        store
            .action_proposal(&ids[1])
            .unwrap()
            .unwrap()
            .denial
            .as_deref(),
        Some("delivery_not_authorized")
    );
}

#[test]
fn after_the_rebuild_parameters_never_change_state_transitions_are_legal_only_and_deletes_need_the_clear_gate(
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let store = open(directory.path());
    begin(&store, &ConversationTarget::Global, "t1");
    let ids = store
        .complete_conversation_turn("t1", checked(BlockScope::Global, &NEW_KINDS.join(", ")))
        .unwrap()
        .unwrap();
    assert_eq!(ids.len(), 4);

    // Each statement names a column that never changes. Some of them would pass every
    // CHECK, so only the trigger can be what refuses them.
    const FIXED: [&str; 9] = [
        "UPDATE action_proposals SET params_json = '{}' WHERE id = ?",
        "UPDATE action_proposals SET digest = '0000000000000000000000000000000000000000000000000000000000000000' WHERE id = ?",
        "UPDATE action_proposals SET subject = 'x' WHERE id = ?",
        "UPDATE action_proposals SET kind = 'workstream.create' WHERE id = ?",
        "UPDATE action_proposals SET idx = 9 WHERE id = ?",
        "UPDATE action_proposals SET depends_on = 'x' WHERE id = ?",
        "UPDATE action_proposals SET denial = 'contract_invalid' WHERE id = ?",
        "UPDATE action_proposals SET created_at = '2000-01-01 00:00:00' WHERE id = ?",
        "DELETE FROM action_proposals WHERE id = ?",
    ];
    for id in &ids {
        for sql in FIXED {
            assert!(try_sql(&path, sql, &[id]).is_err(), "{sql}");
        }
        // Only pending -> approved -> executed is legal here: a skipped edge, a repeat,
        // a backward edge and any change to a terminal row are all refused.
        assert!(try_sql(
            &path,
            "UPDATE action_proposals SET state = 'executed' WHERE id = ?",
            &[id]
        )
        .is_err());
        assert!(try_sql(
            &path,
            "UPDATE action_proposals SET state = 'pending' WHERE id = ?",
            &[id]
        )
        .is_err());
        store.approve_action_proposal(id, "user").unwrap();
        assert!(try_sql(
            &path,
            "UPDATE action_proposals SET state = 'pending' WHERE id = ?",
            &[id]
        )
        .is_err());
        assert!(try_sql(
            &path,
            "UPDATE action_proposals SET state = 'rejected' WHERE id = ?",
            &[id]
        )
        .is_err());
        store
            .record_action_outcome(
                id,
                ActionOutcome::Executed {
                    result_ref: "task-1",
                },
                "user",
            )
            .unwrap();
        for sql in [
            "UPDATE action_proposals SET state = 'failed', error_code = 'conflict' WHERE id = ?",
            "UPDATE action_proposals SET state = 'approved' WHERE id = ?",
        ] {
            assert!(try_sql(&path, sql, &[id]).is_err(), "{sql}");
        }
        assert_eq!(
            store.action_proposal(id).unwrap().unwrap().state,
            ProposalState::Executed
        );
    }
    // Events are append-only, whatever the kind.
    assert!(try_sql(&path, "DELETE FROM action_proposal_events", &[]).is_err());
    assert!(try_sql(
        &path,
        "UPDATE action_proposal_events SET to_state = 'approved'",
        &[]
    )
    .is_err());

    // Only a history clear deletes proposals, and it leaves their events.
    assert!(
        store
            .clear_conversation_history(&ConversationTarget::Global)
            .unwrap()
            .cleared
    );
    for id in &ids {
        assert!(store.action_proposal(id).unwrap().is_none());
        assert_eq!(store.action_proposal_events(id).unwrap().len(), 3);
    }
}

#[test]
fn the_new_kinds_pass_the_kind_check_and_an_unknown_kind_does_not() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let store = open(directory.path());
    begin(&store, &ConversationTarget::Global, "t1");
    let ids = store
        .complete_conversation_turn("t1", checked(BlockScope::Global, &NEW_KINDS.join(", ")))
        .unwrap()
        .unwrap();
    let kinds: Vec<ActionKind> = ids
        .iter()
        .map(|id| store.action_proposal(id).unwrap().unwrap().kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            ActionKind::TaskPropose,
            ActionKind::TaskCancel,
            ActionKind::TaskRetry,
            ActionKind::WorkstreamLinkTask,
        ]
    );

    // Core knows exactly these seventeen kinds, and the database takes each of them.
    const KINDS: [&str; 17] = [
        "workstream.create",
        "workstream.update",
        "objective.add",
        "objective.revise",
        "task.propose",
        "task.cancel",
        "task.retry",
        "workstream.link_task",
        "task.revise",
        "delivery.merge",
        "project.remove",
        "decision.hold",
        "decision.ask",
        "task.message",
        "memory.set",
        "coordinator.route",
        "coordinator.report",
    ];
    const RAW_KIND: &str = "INSERT INTO action_proposals \
        (id, scope, turn_id, idx, kind, params_json, digest, subject) \
        VALUES (?, 'conversation:global', ?, CAST(? AS INTEGER), ?, '{}', ?, 'x')";
    assert_eq!(ActionKind::ALL.len(), KINDS.len());
    begin(&store, &ConversationTarget::Global, "t2");
    for (position, kind) in KINDS.into_iter().enumerate() {
        assert!(ActionKind::parse(kind).is_some(), "{kind}");
        // A turn holds at most ten actions (`idx BETWEEN 0 AND 9`), so the eleventh kind is
        // stored on a second turn, once the first has completed.
        if position == 10 {
            store
                .complete_conversation_turn("t2", TurnBlock::None)
                .unwrap();
            begin(&store, &ConversationTarget::Global, "t3");
        }
        let turn = if position < 10 { "t2" } else { "t3" };
        let (id, idx) = (format!("raw-{position}"), (position % 10).to_string());
        run_sql(&path, RAW_KIND, &[&id, turn, &idx, kind, DIGEST]);
    }
    let current = "t3";
    for unknown in [
        "task.delete",
        "task.create",
        "workstream.link",
        "",
        "TASK.PROPOSE",
    ] {
        assert!(
            try_sql(
                &path,
                RAW_KIND,
                &["raw-unknown", current, "9", unknown, DIGEST]
            )
            .is_err(),
            "{unknown}"
        );
    }
}

#[test]
fn a_denied_task_is_stored_failed_with_its_reason_and_two_events() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let target = ConversationTarget::Global;
    begin(&store, &target, "t1");
    let ids = store
        .complete_conversation_turn(
            "t1",
            checked(
                BlockScope::Global,
                &format!("{PROPOSE_TASK}, {PROPOSE_PULL_REQUEST}"),
            ),
        )
        .unwrap()
        .unwrap();
    assert_eq!(ids, vec![proposal_id("t1", 0), proposal_id("t1", 1)]);
    assert_eq!(
        store.conversation_turn("t1").unwrap().unwrap().action_block,
        Some(ActionBlockOutcome::Accepted)
    );

    // The allowed task is an ordinary pending card with one event and no denial.
    let allowed = store.action_proposal(&ids[0]).unwrap().unwrap();
    assert_eq!(allowed.state, ProposalState::Pending);
    assert_eq!(allowed.error_code, None);
    assert_eq!(allowed.denial, None);
    assert_eq!(store.action_proposal_events(&ids[0]).unwrap().len(), 1);

    // The refused one is stored whole, terminal from the start, with its reason.
    let denied = store.action_proposal(&ids[1]).unwrap().unwrap();
    assert_eq!(denied.kind, ActionKind::TaskPropose);
    assert_eq!(denied.state, ProposalState::Failed);
    assert_eq!(denied.error_code.as_deref(), Some(ACTION_DENIED));
    assert_eq!(denied.denial.as_deref(), Some("delivery_not_authorized"));
    assert_eq!(denied.result_ref, None);
    assert!(denied.params_json.contains("PullRequest"));

    let events = store.action_proposal_events(&ids[1]).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].from_state, None);
    assert_eq!(events[0].to_state, ProposalState::Pending);
    assert_eq!(events[0].error_code, None);
    assert_eq!(events[1].from_state, Some(ProposalState::Pending));
    assert_eq!(events[1].to_state, ProposalState::Failed);
    assert_eq!(events[1].error_code.as_deref(), Some(ACTION_DENIED));
    for event in &events {
        assert_eq!(event.actor, "agent");
        assert_eq!(event.digest, denied.digest);
        assert_eq!(event.result_ref, None);
    }

    // The listing shows both, newest first, with the denial where it was stored.
    let listed = store.action_proposals(&target, 10).unwrap();
    assert_eq!(listed, vec![denied, allowed]);
}

#[test]
fn the_denial_column_is_set_exactly_when_the_code_is_action_denied() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let store = open(directory.path());
    begin(&store, &ConversationTarget::Global, "t1");

    // A code of action_denied needs a denial, and a denial needs that code.
    assert!(try_sql(
        &path,
        raw_task_row!("'failed', 'action_denied', NULL"),
        &["raw-a", "0", DIGEST]
    )
    .is_err());
    assert!(try_sql(
        &path,
        raw_task_row!("'failed', 'conflict', 'delivery_not_authorized'"),
        &["raw-b", "1", DIGEST]
    )
    .is_err());
    assert!(try_sql(
        &path,
        raw_task_row!("'pending', NULL, 'delivery_not_authorized'"),
        &["raw-c", "2", DIGEST]
    )
    .is_err());
    assert!(try_sql(
        &path,
        raw_task_row!("'failed', 'action_denied', ''"),
        &["raw-d", "3", DIGEST]
    )
    .is_err());
    run_sql(
        &path,
        raw_task_row!("'failed', 'action_denied', 'delivery_not_authorized'"),
        &["raw-denied", "4", DIGEST],
    );
    run_sql(
        &path,
        raw_task_row!("'failed', 'conflict', NULL"),
        &["raw-failed", "5", DIGEST],
    );
    run_sql(
        &path,
        raw_task_row!("'pending', NULL, NULL"),
        &["raw-pending", "6", DIGEST],
    );
    let denied = store.action_proposal("raw-denied").unwrap().unwrap();
    assert_eq!(denied.denial.as_deref(), Some("delivery_not_authorized"));
    assert_eq!(
        store.action_proposal("raw-failed").unwrap().unwrap().denial,
        None
    );
    assert_eq!(
        store
            .action_proposal("raw-pending")
            .unwrap()
            .unwrap()
            .denial,
        None
    );

    // The reason is written once. Changing it passes the CHECK, so the trigger refuses it.
    assert!(try_sql(
        &path,
        "UPDATE action_proposals SET denial = 'contract_invalid' WHERE id = ?",
        &["raw-denied"]
    )
    .is_err());
    assert!(try_sql(
        &path,
        "UPDATE action_proposals SET denial = 'delivery_not_authorized' WHERE id = ?",
        &["raw-pending"]
    )
    .is_err());

    // An execution cannot end in action_denied: no denial was recorded, so the database
    // refuses the outcome, and the proposal stays approved.
    store
        .approve_action_proposal("raw-pending", "user")
        .unwrap();
    assert!(matches!(
        store.record_action_outcome(
            "raw-pending",
            ActionOutcome::Failed {
                error_code: ACTION_DENIED,
            },
            "user",
        ),
        Err(ResolveError::Store(_))
    ));
    assert_eq!(
        store.action_proposal("raw-pending").unwrap().unwrap().state,
        ProposalState::Approved
    );
    let failed = store
        .record_action_outcome(
            "raw-pending",
            ActionOutcome::Failed {
                error_code: "conflict",
            },
            "user",
        )
        .unwrap();
    assert_eq!(failed.denial, None);
}

#[test]
fn a_denied_proposal_is_not_approvable_and_creates_no_pending_card() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let target = ConversationTarget::Global;

    // The same task is pending first. A later request for it that the envelope refuses
    // is stored failed and takes nothing away: the earlier card stays pending.
    begin(&store, &target, "t1");
    let pending = store
        .complete_conversation_turn("t1", checked(BlockScope::Global, PROPOSE_TASK))
        .unwrap()
        .unwrap();
    assert_eq!(store.pending_action_proposals(10).unwrap().len(), 1);
    begin(&store, &target, "t2");
    let denied = store
        .complete_conversation_turn(
            "t2",
            checked(BlockScope::Global, PROPOSE_TASK_AS_PULL_REQUEST),
        )
        .unwrap()
        .unwrap()
        .remove(0);
    let still_pending = store.pending_action_proposals(10).unwrap();
    assert_eq!(still_pending.len(), 1);
    assert_eq!(still_pending[0].id, pending[0]);
    assert_eq!(
        store.action_proposal(&pending[0]).unwrap().unwrap().state,
        ProposalState::Pending
    );

    // Nothing moves a denied proposal: not approve, not reject, not an outcome.
    assert!(matches!(
        store.approve_action_proposal(&denied, "user"),
        Err(ResolveError::NotPending(ProposalState::Failed))
    ));
    assert!(matches!(
        store.reject_action_proposal(&denied, "user"),
        Err(ResolveError::NotPending(ProposalState::Failed))
    ));
    assert!(matches!(
        store.record_action_outcome(
            &denied,
            ActionOutcome::Executed {
                result_ref: "task-1",
            },
            "user",
        ),
        Err(ResolveError::NotPending(ProposalState::Failed))
    ));
    let after = store.action_proposal(&denied).unwrap().unwrap();
    assert_eq!(after.state, ProposalState::Failed);
    assert_eq!(after.denial.as_deref(), Some("delivery_not_authorized"));
    assert_eq!(store.action_proposal_events(&denied).unwrap().len(), 2);

    // A denied task built on a pending workstream is just as terminal, and rejecting
    // that workstream records nothing more on it.
    begin(&store, &target, "t3");
    let ids = store
        .complete_conversation_turn(
            "t3",
            checked(
                BlockScope::Global,
                &format!(
                    r#"{CREATE_ALPHA}, {{"kind": "task.propose", "project": "p-3fa9c1d2", "objective": "Document the retry module.", "agent": "codex", "delivery": "PullRequest", "workstream_ref": "w1"}}"#
                ),
            ),
        )
        .unwrap()
        .unwrap();
    let child = store.action_proposal(&ids[1]).unwrap().unwrap();
    assert_eq!(child.depends_on.as_deref(), Some(ids[0].as_str()));
    assert_eq!(child.state, ProposalState::Failed);
    assert!(matches!(
        store.approve_action_proposal(&ids[1], "user"),
        Err(ResolveError::NotPending(ProposalState::Failed))
    ));
    store.reject_action_proposal(&ids[0], "user").unwrap();
    assert_eq!(store.action_proposal_events(&ids[1]).unwrap().len(), 2);
    // Only the card from the first turn is left pending.
    let left = store.pending_action_proposals(10).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, pending[0]);
}

#[test]
fn a_denied_proposal_never_supersedes_a_pending_one() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let target = ConversationTarget::Global;

    // One pending card, and a changed request for the same subject that the envelope
    // refuses. They share a subject (same project, objective, agent, no workstream)
    // and differ only in delivery, so the digests differ.
    begin(&store, &target, "t1");
    let pending = store
        .complete_conversation_turn("t1", checked(BlockScope::Global, PROPOSE_TASK))
        .unwrap()
        .unwrap();
    begin(&store, &target, "t2");
    let denied = store
        .complete_conversation_turn(
            "t2",
            checked(BlockScope::Global, PROPOSE_TASK_AS_PULL_REQUEST),
        )
        .unwrap()
        .unwrap();
    assert_eq!(denied, vec![proposal_id("t2", 0)]);

    // The denied card is stored failed. The pending one is untouched: still pending,
    // with its single creation event and no superseded event.
    let denied = store.action_proposal(&denied[0]).unwrap().unwrap();
    assert_eq!(denied.state, ProposalState::Failed);
    assert_eq!(denied.denial.as_deref(), Some("delivery_not_authorized"));
    let kept = store.action_proposal(&pending[0]).unwrap().unwrap();
    assert_eq!(kept.state, ProposalState::Pending);
    assert_eq!(store.action_proposal_events(&pending[0]).unwrap().len(), 1);
    assert_eq!(store.pending_action_proposals(10).unwrap().len(), 1);

    // The user can still approve the card the denied one did not replace.
    store.approve_action_proposal(&pending[0], "user").unwrap();

    // The same holds inside one block: a refused request after a pending one with the
    // same subject leaves that card pending.
    begin(&store, &target, "t3");
    let both = format!("{PROPOSE_TASK}, {PROPOSE_TASK_AS_PULL_REQUEST}");
    let ids = store
        .complete_conversation_turn("t3", checked(BlockScope::Global, &both))
        .unwrap()
        .unwrap();
    assert_eq!(ids.len(), 2);
    assert_eq!(
        store.action_proposal(&ids[0]).unwrap().unwrap().state,
        ProposalState::Pending
    );
    assert_eq!(
        store.action_proposal(&ids[1]).unwrap().unwrap().state,
        ProposalState::Failed
    );
    assert_eq!(store.pending_action_proposals(10).unwrap().len(), 1);
}

/// `count` `task.propose` actions with distinct objectives, numbered from `first`, so
/// none matches or replaces another.
fn task_requests(first: usize, count: usize) -> String {
    (first..first + count)
        .map(|number| {
            format!(
                r#"{{"kind": "task.propose", "project": "p-3fa9c1d2", "objective": "Task number {number}.", "agent": "codex"}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// How many proposals of `target` are pending.
fn pending_in(store: &SqliteStore, target: &ConversationTarget) -> usize {
    store
        .action_proposals(target, 100)
        .unwrap()
        .iter()
        .filter(|proposal| proposal.state == ProposalState::Pending)
        .count()
}

/// Fills the global conversation with 20 pending task proposals, the most it may hold
/// (`MAX_PENDING_PROPOSALS_PER_SCOPE`), in two turns because a block holds at most 10
/// actions. Returns their ids, numbered from 0.
fn fill_the_global_conversation(store: &SqliteStore) -> Vec<String> {
    let mut ids = Vec::new();
    for (turn, first) in [("fill-1", 0), ("fill-2", 10)] {
        begin(store, &ConversationTarget::Global, turn);
        let block = accepted(BlockScope::Global, &task_requests(first, 10));
        ids.extend(
            store
                .complete_conversation_turn(turn, block)
                .unwrap()
                .unwrap(),
        );
    }
    assert_eq!(ids.len(), 20);
    assert_eq!(pending_in(store, &ConversationTarget::Global), 20);
    ids
}

#[test]
fn a_scope_with_20_pending_proposals_stores_no_more_and_the_block_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let global = ConversationTarget::Global;
    let ids = fill_the_global_conversation(&store);

    // A block that would take the conversation to 21 stores nothing. The turn still
    // completes, and it records the block as rejected in the same transaction, so the
    // daemon does not need a second attempt.
    begin(&store, &global, "t3");
    let over = accepted(BlockScope::Global, &task_requests(20, 1));
    assert_eq!(
        store.complete_conversation_turn("t3", over).unwrap(),
        Some(Vec::new())
    );
    let turn = store.conversation_turn("t3").unwrap().unwrap();
    assert_eq!(turn.state, TurnState::Completed);
    assert_eq!(turn.action_block, Some(ActionBlockOutcome::Rejected));
    assert!(store
        .action_proposal(&proposal_id("t3", 0))
        .unwrap()
        .is_none());
    assert_eq!(store.action_proposals(&global, 100).unwrap().len(), 20);
    assert_eq!(pending_in(&store, &global), 20);

    // A block that replaces a pending card leaves the count where it was, so it fits at
    // the cap. Same project, objective and agent as task 1, another delivery.
    begin(&store, &global, "t4");
    let replacement = r#"{"kind": "task.propose", "project": "p-3fa9c1d2", "objective": "Task number 1.", "agent": "codex", "delivery": "ReportOnly"}"#;
    let stored = store
        .complete_conversation_turn("t4", accepted(BlockScope::Global, replacement))
        .unwrap()
        .unwrap();
    assert_eq!(stored, vec![proposal_id("t4", 0)]);
    assert_eq!(
        store.action_proposal(&ids[1]).unwrap().unwrap().state,
        ProposalState::Superseded
    );
    assert_eq!(pending_in(&store, &global), 20);

    // A block of two stores neither when only the second would not fit. One slot is
    // free here, because a card was rejected.
    store.reject_action_proposal(&ids[0], "user").unwrap();
    begin(&store, &global, "t5");
    let two = accepted(BlockScope::Global, &task_requests(21, 2));
    assert_eq!(
        store.complete_conversation_turn("t5", two).unwrap(),
        Some(Vec::new())
    );
    assert_eq!(
        store.conversation_turn("t5").unwrap().unwrap().action_block,
        Some(ActionBlockOutcome::Rejected)
    );
    assert!(store
        .action_proposal(&proposal_id("t5", 0))
        .unwrap()
        .is_none());
    assert_eq!(pending_in(&store, &global), 19);

    // The cap is per conversation: a workstream conversation is not held back by the
    // full global one.
    let workstream = ConversationTarget::Workstream("ws-1".to_owned());
    begin(&store, &workstream, "t6");
    let linked = r#"{"kind": "task.propose", "project": "p-3fa9c1d2", "objective": "Task number 0.", "agent": "codex", "workstream_id": "ws-1"}"#;
    let stored = store
        .complete_conversation_turn("t6", accepted(BlockScope::Workstream("ws-1"), linked))
        .unwrap()
        .unwrap();
    assert_eq!(stored, vec![proposal_id("t6", 0)]);
    assert_eq!(pending_in(&store, &workstream), 1);
}

#[test]
fn a_decided_proposal_frees_a_slot_under_the_pending_cap() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let global = ConversationTarget::Global;
    let ids = fill_the_global_conversation(&store);

    // Full: one more is refused.
    begin(&store, &global, "t3");
    let next = accepted(BlockScope::Global, &task_requests(20, 1));
    assert_eq!(
        store.complete_conversation_turn("t3", next).unwrap(),
        Some(Vec::new())
    );
    assert_eq!(
        store.conversation_turn("t3").unwrap().unwrap().action_block,
        Some(ActionBlockOutcome::Rejected)
    );

    // Each way a card stops being pending frees one slot, and only one.
    store.reject_action_proposal(&ids[0], "user").unwrap();
    store.approve_action_proposal(&ids[1], "user").unwrap();
    assert_eq!(pending_in(&store, &global), 18);
    begin(&store, &global, "t4");
    let stored = store
        .complete_conversation_turn("t4", accepted(BlockScope::Global, &task_requests(20, 2)))
        .unwrap()
        .unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(pending_in(&store, &global), 20);

    // Full again: a decided card never counts, and a new one does.
    begin(&store, &global, "t5");
    let over = accepted(BlockScope::Global, &task_requests(22, 1));
    assert_eq!(
        store.complete_conversation_turn("t5", over).unwrap(),
        Some(Vec::new())
    );
    assert_eq!(
        store.conversation_turn("t5").unwrap().unwrap().action_block,
        Some(ActionBlockOutcome::Rejected)
    );
    assert_eq!(pending_in(&store, &global), 20);
}

#[test]
fn an_over_the_cap_block_records_the_turn_as_rejected_in_one_transaction() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let store = open(directory.path());
    let global = ConversationTarget::Global;
    let ids = fill_the_global_conversation(&store);
    let proposals_before = scalars(&path, PROPOSAL_DUMP);
    let events_before = scalars(&path, EVENT_DUMP);
    assert_eq!(proposals_before.len(), 20);

    // The first action replaces pending task 1, which leaves the count at 20. The other
    // two are new, so the block would end at 22. Everything it wrote is undone, the
    // replacement included, and one call both completes the turn and records the outcome.
    begin(&store, &global, "t3");
    let replacement = r#"{"kind": "task.propose", "project": "p-3fa9c1d2", "objective": "Task number 1.", "agent": "codex", "delivery": "ReportOnly"}"#;
    let over = format!("{replacement}, {}", task_requests(20, 2));
    let stored = store
        .complete_conversation_turn("t3", accepted(BlockScope::Global, &over))
        .unwrap();
    assert_eq!(stored, Some(Vec::new()));
    let turn = store.conversation_turn("t3").unwrap().unwrap();
    assert_eq!(turn.state, TurnState::Completed);
    assert_eq!(turn.action_block, Some(ActionBlockOutcome::Rejected));

    // No row of the turn exists, and the 20 cards and their events are byte for byte
    // what they were: the replaced card is still pending, with no superseded event.
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(COUNT(*) AS TEXT) FROM action_proposals WHERE turn_id = 't3'"
        ),
        vec!["0"]
    );
    assert_eq!(scalars(&path, PROPOSAL_DUMP), proposals_before);
    assert_eq!(scalars(&path, EVENT_DUMP), events_before);
    assert_eq!(
        store.action_proposal(&ids[1]).unwrap().unwrap().state,
        ProposalState::Pending
    );
    assert_eq!(pending_in(&store, &global), 20);
}

#[test]
fn pending_action_proposals_lists_newest_first_and_honors_the_limit() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let global = ConversationTarget::Global;
    let workstream = ConversationTarget::Workstream("ws-1".to_owned());
    assert!(store.pending_action_proposals(10).unwrap().is_empty());

    begin(&store, &global, "t1");
    let approved = store
        .complete_conversation_turn("t1", accepted(BlockScope::Global, CREATE_ALPHA))
        .unwrap()
        .unwrap()
        .remove(0);
    begin(&store, &workstream, "t2");
    let in_workstream = store
        .complete_conversation_turn(
            "t2",
            accepted(
                BlockScope::Workstream("ws-1"),
                r#"{"kind": "objective.add", "workstream_id": "ws-1", "title": "Plan"}"#,
            ),
        )
        .unwrap()
        .unwrap()
        .remove(0);
    begin(&store, &global, "t3");
    let task = store
        .complete_conversation_turn("t3", checked(BlockScope::Global, PROPOSE_TASK))
        .unwrap()
        .unwrap()
        .remove(0);
    // A denied task and an approved one are not pending, so neither is listed.
    begin(&store, &global, "t4");
    store
        .complete_conversation_turn("t4", checked(BlockScope::Global, PROPOSE_PULL_REQUEST))
        .unwrap();
    store.approve_action_proposal(&approved, "user").unwrap();

    // Newest first, across both conversations.
    let listed: Vec<String> = store
        .pending_action_proposals(10)
        .unwrap()
        .into_iter()
        .map(|proposal| proposal.id)
        .collect();
    assert_eq!(listed, vec![task.clone(), in_workstream]);
    let newest = store.pending_action_proposals(1).unwrap();
    assert_eq!(newest.len(), 1);
    assert_eq!(newest[0].id, task);
    assert_eq!(newest[0].kind, ActionKind::TaskPropose);
    assert!(store.pending_action_proposals(0).unwrap().is_empty());
}

#[test]
fn a_turn_completed_twice_stores_each_task_proposal_and_its_events_once() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let target = ConversationTarget::Global;
    let both = format!("{PROPOSE_TASK}, {PROPOSE_PULL_REQUEST}");

    begin(&store, &target, "t1");
    let first = store
        .complete_conversation_turn("t1", checked(BlockScope::Global, &both))
        .unwrap()
        .unwrap();
    // The turn is already completed, so a second completion stores nothing.
    assert_eq!(
        store
            .complete_conversation_turn("t1", checked(BlockScope::Global, &both))
            .unwrap(),
        None
    );
    assert_eq!(store.action_proposals(&target, 10).unwrap().len(), 2);
    assert_eq!(store.action_proposal_events(&first[0]).unwrap().len(), 1);
    assert_eq!(store.action_proposal_events(&first[1]).unwrap().len(), 2);

    // The same request in a later turn matches the pending card and writes nothing for
    // it. The refused one is never pending, so it is a second failed card with its own
    // two events.
    begin(&store, &target, "t2");
    let second = store
        .complete_conversation_turn("t2", checked(BlockScope::Global, &both))
        .unwrap()
        .unwrap();
    assert_eq!(second, vec![first[0].clone(), proposal_id("t2", 1)]);
    assert_eq!(store.action_proposals(&target, 10).unwrap().len(), 3);
    assert_eq!(store.action_proposal_events(&first[0]).unwrap().len(), 1);
    assert_eq!(store.action_proposal_events(&second[1]).unwrap().len(), 2);
    assert_eq!(
        store.action_proposal(&second[1]).unwrap().unwrap().state,
        ProposalState::Failed
    );
    assert_eq!(store.pending_action_proposals(10).unwrap().len(), 1);
}

/// Every regular file under `directory` whose bytes contain `needle`.
fn files_containing(directory: &Path, needle: &str) -> Vec<PathBuf> {
    fn walk(path: &Path, needle: &[u8], found: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, needle, found);
            } else if fs::read(&path)
                .unwrap()
                .windows(needle.len())
                .any(|window| window == needle)
            {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(directory, needle.as_bytes(), &mut found);
    found
}

#[test]
fn a_clear_scrubs_task_proposal_parameters_and_leaves_only_digests_in_events() {
    const MARKER: &str = "MARKER-TASK-OBJECTIVE-6b1e";
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let target = ConversationTarget::Global;
    let task = |delivery: &str| {
        format!(
            r#"{{"kind": "task.propose", "project": "p-3fa9c1d2", "objective": "{MARKER} {delivery}", "agent": "codex", "delivery": "{delivery}"}}"#
        )
    };
    begin(&store, &target, "t1");
    let ids = store
        .complete_conversation_turn(
            "t1",
            checked(
                BlockScope::Global,
                &format!("{}, {}", task("LocalReview"), task("PullRequest")),
            ),
        )
        .unwrap()
        .unwrap();
    // Control: the objective is on disk before the clear, so the search below means something.
    assert!(!files_containing(directory.path(), MARKER).is_empty());

    assert!(store.clear_conversation_history(&target).unwrap().cleared);
    assert!(store.action_proposals(&target, 10).unwrap().is_empty());
    assert!(store.pending_action_proposals(10).unwrap().is_empty());
    for id in &ids {
        assert!(store.action_proposal(id).unwrap().is_none());
    }
    // The events outlive the clear. They hold digests and codes, never the objective.
    let pending = store.action_proposal_events(&ids[0]).unwrap();
    let denied = store.action_proposal_events(&ids[1]).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(denied.len(), 2);
    assert_eq!(denied[1].error_code.as_deref(), Some(ACTION_DENIED));
    for event in pending.iter().chain(&denied) {
        assert_eq!(event.digest.len(), 64);
    }
    assert!(files_containing(directory.path(), MARKER).is_empty());
}

fn route_request(workstream: &str, number: usize) -> String {
    format!(
        r#"{{"kind": "task.propose", "project": "p-3fa9c1d2", "objective": "Routed task number {number}.", "agent": "codex", "workstream_id": "{workstream}"}}"#
    )
}

/// One approved proposal in `target`'s turn, recorded as executed with `result_ref`.
fn executed_route(
    store: &SqliteStore,
    target: &ConversationTarget,
    scope: BlockScope<'_>,
    turn: &str,
    request: &str,
    result_ref: &str,
) {
    begin(store, target, turn);
    let id = store
        .complete_conversation_turn(turn, checked(scope, request))
        .unwrap()
        .unwrap()
        .remove(0);
    store.approve_action_proposal(&id, "user").unwrap();
    store
        .record_action_outcome(&id, ActionOutcome::Executed { result_ref }, "user")
        .unwrap();
}

/// Creates the task `task_id` and links it to `workstream_id`, which is created on first use,
/// so a routed task is one the workstream still holds.
fn link(store: &SqliteStore, workstream_id: &str, task_id: &str) {
    // The project and the workstream exist after the first call; a repeat is refused.
    let _ = store.register_project(&Project {
        id: "p-route".into(),
        root_path: "/work/p-route".into(),
        settings: serde_json::json!({}),
    });
    let _ = store.create_workstream(
        &Workstream::new(workstream_id, "Title", "").unwrap(),
        "user",
    );
    store
        .create(&TaskContract {
            id: task_id.into(),
            project_id: "p-route".into(),
            parent_id: None,
            objective: format!("work for {task_id}"),
            kind: TaskKind::Implement,
            agent: "codex".into(),
            dependencies: Vec::new(),
            verification: VerificationPolicy::None,
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 0,
            max_repairs: 0,
            model_selection: None,
            permission: Default::default(),
            work_item: None,
        })
        .unwrap();
    store.link_task(workstream_id, task_id, None).unwrap();
}

#[test]
fn a_task_relinked_away_from_the_workstream_is_not_routed_in() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    executed_route(
        &store,
        &ConversationTarget::Global,
        BlockScope::Global,
        "t1",
        &route_request("ws-1", 1),
        "task-r1",
    );
    link(&store, "ws-1", "task-r1");
    assert_eq!(store.routed_in("ws-1", 10).unwrap().len(), 1);

    store.unlink_task("ws-1", "task-r1").unwrap();
    assert!(store.routed_in("ws-1", 10).unwrap().is_empty());

    let _ = store.create_workstream(&Workstream::new("ws-2", "Other", "").unwrap(), "user");
    store.link_task("ws-2", "task-r1", None).unwrap();
    assert!(store.routed_in("ws-1", 10).unwrap().is_empty());
}

#[test]
fn routed_in_returns_only_executed_global_task_proposals_for_that_workstream() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let global = ConversationTarget::Global;
    for (turn, workstream, task) in [
        ("t1", "ws-1", "task-r1"),
        ("t2", "ws-2", "task-r2"),
        ("t3", "ws-1", "task-r3"),
    ] {
        executed_route(
            &store,
            &global,
            BlockScope::Global,
            turn,
            &route_request(workstream, 1),
            task,
        );
        link(&store, workstream, task);
    }
    let routed = store.routed_in("ws-1", 10).unwrap();
    let ids: Vec<&str> = routed.iter().map(|r| r.task_id.as_str()).collect();
    assert_eq!(ids, ["task-r3", "task-r1"]);
    assert!(!routed[0].routed_at.is_empty());
    assert_eq!(store.routed_in("ws-1", 1).unwrap().len(), 1);
    assert_eq!(store.routed_in("ws-2", 10).unwrap().len(), 1);
    assert!(store.routed_in("ws-3", 10).unwrap().is_empty());
}

#[test]
fn routed_in_ignores_pending_failed_and_other_workstream_proposals() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let global = ConversationTarget::Global;

    // Pending: proposed, never approved.
    begin(&store, &global, "t1");
    store
        .complete_conversation_turn("t1", checked(BlockScope::Global, &route_request("ws-1", 1)))
        .unwrap();
    // Failed: approved, then the run failed.
    begin(&store, &global, "t2");
    let failed = store
        .complete_conversation_turn("t2", checked(BlockScope::Global, &route_request("ws-1", 2)))
        .unwrap()
        .unwrap()
        .remove(0);
    store.approve_action_proposal(&failed, "user").unwrap();
    store
        .record_action_outcome(
            &failed,
            ActionOutcome::Failed {
                error_code: "execution_failed",
            },
            "user",
        )
        .unwrap();
    // Executed, but proposed by the workstream conversation itself, not by Global.
    let workstream = ConversationTarget::Workstream("ws-1".to_owned());
    executed_route(
        &store,
        &workstream,
        BlockScope::Workstream("ws-1"),
        "t3",
        &route_request("ws-1", 3),
        "task-own",
    );
    // Executed in Global, but for another workstream.
    executed_route(
        &store,
        &global,
        BlockScope::Global,
        "t4",
        &route_request("ws-9", 4),
        "task-other",
    );

    assert!(store.routed_in("ws-1", 10).unwrap().is_empty());
}

#[test]
fn routed_in_is_empty_after_the_global_history_is_cleared() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let global = ConversationTarget::Global;
    executed_route(
        &store,
        &global,
        BlockScope::Global,
        "t1",
        &route_request("ws-1", 1),
        "task-r1",
    );
    link(&store, "ws-1", "task-r1");
    assert_eq!(store.routed_in("ws-1", 10).unwrap().len(), 1);

    assert!(store.clear_conversation_history(&global).unwrap().cleared);
    assert!(store.routed_in("ws-1", 10).unwrap().is_empty());
}
