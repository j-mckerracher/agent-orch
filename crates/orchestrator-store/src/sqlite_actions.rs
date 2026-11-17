//! Action proposals parsed from orchestrator replies (DPES-1105).
//!
//! A proposal is a request the user may approve. Storing one touches nothing
//! else: no workstream, objective or task changes, and nothing runs in this
//! phase. Rows are written only by [`SqliteStore::complete_conversation_turn`],
//! inside the transaction that completes the turn, so a turn never completes
//! without the proposals its block produced.
//!
//! Migrations 0037 and 0038 hold the rules that outside callers cannot break: a
//! proposal's parameters are fixed, a proposal is deleted only by a history
//! clear, and events are append-only.
//!
//! The store holds no policy. A task the envelope refused arrives from the block
//! with its denial already decided, and is stored as a terminal `failed` card that
//! no one can approve.
//!
//! A pending proposal moves only through [`SqliteStore::approve_action_proposal`],
//! [`SqliteStore::reject_action_proposal`] and [`SqliteStore::record_action_outcome`].
//! Each is a compare-and-set from the state it read, and its event commits with it.

use std::future::Future;

use orchestrator_core::orchestrator_action::{digest_hex, proposal_id, ActionBlock, ActionKind};
use orchestrator_core::task_envelope::{Denial, ACTION_DENIED};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::sqlite_conversations::{corrupt, ConversationError, ConversationTarget};
use crate::{SqliteStore, StoreError};

macro_rules! proposal_columns {
    () => {
        "id, scope, turn_id, idx, kind, params_json, digest, subject, depends_on, state, \
         result_ref, error_code, denial, created_at, updated_at"
    };
}

/// What became of a turn's action block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionBlockOutcome {
    /// The block parsed, and its actions are pending proposals.
    Accepted,
    /// The block was malformed, out of scope, or would have left the conversation over
    /// its limit of pending proposals, so nothing was stored.
    Rejected,
    /// The session did not negotiate actions, so the block was dropped unparsed.
    Stripped,
}

impl ActionBlockOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Stripped => "stripped",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "accepted" => Ok(Self::Accepted),
            "rejected" => Ok(Self::Rejected),
            "stripped" => Ok(Self::Stripped),
            other => Err(corrupt(format!("unknown action block outcome {other}"))),
        }
    }
}

/// The block a completed turn's reply carried, as
/// [`SqliteStore::complete_conversation_turn`] records it.
pub enum TurnBlock {
    /// The reply had no block. Nothing is recorded.
    None,
    /// A block was present, but the session did not negotiate actions.
    Stripped,
    /// A block was present and refused. Nothing is stored.
    Rejected,
    /// A block was present and valid. Its actions become pending proposals, unless that
    /// would leave the conversation over its limit: then nothing is stored and the turn
    /// records [`ActionBlockOutcome::Rejected`].
    Accepted(ActionBlock),
}

/// Where a proposal stands. Only `pending` cards are deduplicated or superseded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProposalState {
    Pending,
    Approved,
    Executed,
    Failed,
    Rejected,
    Superseded,
    Expired,
}

impl ProposalState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Executed => "executed",
            Self::Failed => "failed",
            Self::Rejected => "rejected",
            Self::Superseded => "superseded",
            Self::Expired => "expired",
        }
    }

    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "executed" => Ok(Self::Executed),
            "failed" => Ok(Self::Failed),
            "rejected" => Ok(Self::Rejected),
            "superseded" => Ok(Self::Superseded),
            "expired" => Ok(Self::Expired),
            other => Err(corrupt(format!("unknown proposal state {other}"))),
        }
    }
}

/// One proposed action. Its parameters never change; only its state does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionProposal {
    pub id: String,
    pub scope: String,
    pub turn_id: String,
    /// Position of the action in its block, 0 to 9.
    pub idx: u32,
    pub kind: ActionKind,
    /// Canonical JSON of the action, the text the digest binds.
    pub params_json: String,
    /// Lowercase hex SHA-256 of `params_json`.
    pub digest: String,
    /// What a later pending request must match to supersede this card.
    pub subject: String,
    /// The proposal this one was built on, such as the workstream an objective joins.
    pub depends_on: Option<String>,
    pub state: ProposalState,
    pub result_ref: Option<String>,
    pub error_code: Option<String>,
    /// Why the envelope refused a task proposal: set, once, exactly when `error_code`
    /// is [`ACTION_DENIED`]. Null for every other proposal.
    pub denial: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One state change of a proposal. Holds digests, never parameters, and survives a
/// history clear of its conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalEvent {
    pub sequence: i64,
    pub proposal_id: String,
    pub scope: String,
    /// `None` only for the event that created the proposal.
    pub from_state: Option<ProposalState>,
    pub to_state: ProposalState,
    pub actor: String,
    pub digest: String,
    pub error_code: Option<String>,
    pub result_ref: Option<String>,
    pub created_at: String,
}

/// Event `error_code` of a pending proposal failed because its parent did not execute.
pub const DEPENDENCY_NOT_EXECUTED: &str = "dependency_not_executed";

/// Most pending proposals one conversation may hold. A block that would leave its
/// conversation above this stores nothing, and its turn records
/// [`ActionBlockOutcome::Rejected`]. A pending card is at most about 17 KiB, so 20 of
/// them stay under the 1 MiB IPC body limit, and the user can always open the
/// conversation and reject.
const MAX_PENDING_PROPOSALS_PER_SCOPE: usize = 20;

/// Why a resolve call was refused. A refused call writes nothing.
#[derive(Debug)]
pub enum ResolveError {
    /// No proposal has this id.
    NotFound,
    /// The proposal is not in the state the transition needs.
    NotPending(ProposalState),
    /// An approved dependent needs its parent executed first.
    ParentNotExecuted,
    Store(StoreError),
}

impl From<StoreError> for ResolveError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<sqlx::Error> for ResolveError {
    fn from(error: sqlx::Error) -> Self {
        Self::Store(StoreError::Database(error))
    }
}

/// What an execution produced, recorded by [`SqliteStore::record_action_outcome`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionOutcome<'a> {
    Executed { result_ref: &'a str },
    Failed { error_code: &'a str },
}

/// A task the global conversation routed to a workstream, read from the proposal ledger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutedTask {
    pub task_id: String,
    /// When the proposal executed (`updated_at`).
    pub routed_at: String,
}

impl ActionProposal {
    /// The workstream id of a workstream-scoped proposal.
    pub fn scope_workstream(&self) -> Option<&str> {
        self.scope.strip_prefix("conversation:workstream:")
    }
}

impl SqliteStore {
    /// Proposals of one conversation, newest first, at most `limit` of them.
    pub fn action_proposals(
        &self,
        target: &ConversationTarget,
        limit: usize,
    ) -> Result<Vec<ActionProposal>, StoreError> {
        let scope = target.id();
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.call(move |pool| async move {
            let rows = sqlx::query(concat!(
                "SELECT ",
                proposal_columns!(),
                " FROM action_proposals WHERE scope = ? ORDER BY rowid DESC LIMIT ?"
            ))
            .bind(scope)
            .bind(limit)
            .fetch_all(&pool)
            .await?;
            rows.into_iter()
                .map(decode_proposal)
                .collect::<Result<Vec<_>, _>>()
        })
    }

    /// Tasks the GLOBAL conversation proposed against `workstream_id` that have executed,
    /// newest first, at most `limit`. Derived from the proposal ledger: nothing is stored.
    /// A history clear of the global conversation deletes these rows.
    pub fn routed_in(
        &self,
        workstream_id: &str,
        limit: usize,
    ) -> Result<Vec<RoutedTask>, StoreError> {
        let workstream_id = workstream_id.to_owned();
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.call(move |pool| async move {
            let rows = sqlx::query(
                "SELECT p.result_ref, p.updated_at FROM action_proposals p \
                 JOIN workstream_tasks w ON w.task_id = p.result_ref AND w.workstream_id = ? \
                 WHERE p.scope = 'conversation:global' AND p.kind = 'task.propose' \
                 AND p.state = 'executed' AND p.result_ref IS NOT NULL \
                 AND json_extract(p.params_json, '$.workstream_id') = w.workstream_id \
                 ORDER BY p.updated_at DESC, p.rowid DESC LIMIT ?",
            )
            .bind(workstream_id)
            .bind(limit)
            .fetch_all(&pool)
            .await?;
            rows.into_iter()
                .map(|row| -> Result<RoutedTask, StoreError> {
                    Ok(RoutedTask {
                        task_id: row.try_get("result_ref")?,
                        routed_at: row.try_get("updated_at")?,
                    })
                })
                .collect()
        })
    }

    /// Pending proposals of every conversation, newest first, at most `limit` of them.
    pub fn pending_action_proposals(
        &self,
        limit: usize,
    ) -> Result<Vec<ActionProposal>, StoreError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.call(move |pool| async move {
            let rows = sqlx::query(concat!(
                "SELECT ",
                proposal_columns!(),
                " FROM action_proposals WHERE state = 'pending' \
                 ORDER BY created_at DESC, rowid DESC LIMIT ?"
            ))
            .bind(limit)
            .fetch_all(&pool)
            .await?;
            rows.into_iter()
                .map(decode_proposal)
                .collect::<Result<Vec<_>, _>>()
        })
    }

    /// One proposal by id, or `None` when no such proposal exists.
    pub fn action_proposal(&self, id: &str) -> Result<Option<ActionProposal>, StoreError> {
        let id = id.to_owned();
        self.call(move |pool| async move {
            sqlx::query(concat!(
                "SELECT ",
                proposal_columns!(),
                " FROM action_proposals WHERE id = ?"
            ))
            .bind(id)
            .fetch_optional(&pool)
            .await?
            .map(decode_proposal)
            .transpose()
        })
    }

    /// Every state change of one proposal, oldest first.
    pub fn action_proposal_events(&self, id: &str) -> Result<Vec<ProposalEvent>, StoreError> {
        let id = id.to_owned();
        self.call(move |pool| async move {
            let rows = sqlx::query(
                "SELECT sequence, proposal_id, scope, from_state, to_state, actor, digest, \
                 error_code, result_ref, created_at \
                 FROM action_proposal_events WHERE proposal_id = ? ORDER BY sequence",
            )
            .bind(id)
            .fetch_all(&pool)
            .await?;
            rows.into_iter()
                .map(decode_event)
                .collect::<Result<Vec<_>, _>>()
        })
    }

    /// Pending proposals no grant matcher has tried yet, oldest first, at most `limit`, each
    /// with the sequence of its creation event. Only proposals created after event sequence
    /// `min_floor` are listed, so nothing that existed before the oldest open grant is read.
    pub fn untried_pending_proposals(
        &self,
        min_floor: i64,
        limit: usize,
    ) -> Result<Vec<(ActionProposal, i64)>, StoreError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            let created = sqlx::query(
                "SELECT proposal_id, sequence FROM action_proposal_events \
                 WHERE from_state IS NULL AND sequence > ? AND proposal_id IN \
                   (SELECT id FROM action_proposals WHERE state = 'pending' AND grant_tried = 0) \
                 ORDER BY sequence LIMIT ?",
            )
            .bind(min_floor)
            .bind(limit)
            .fetch_all(&mut *connection)
            .await?;
            let mut found = Vec::with_capacity(created.len());
            for row in created {
                let id: String = row.try_get("proposal_id")?;
                let sequence: i64 = row.try_get("sequence")?;
                if let Some(proposal) = load_proposal(&mut connection, &id).await? {
                    found.push((proposal, sequence));
                }
            }
            Ok(found)
        })
    }

    /// Marks one proposal as tried by the grant matcher, so it is never tried again.
    pub fn mark_grant_tried(&self, id: &str) -> Result<(), StoreError> {
        let id = id.to_owned();
        self.call(move |pool| async move {
            sqlx::query("UPDATE action_proposals SET grant_tried = 1 WHERE id = ?")
                .bind(id)
                .execute(&pool)
                .await?;
            Ok(())
        })
    }

    /// The actor that approved one proposal (`user`, or `grant:<id>`), or `None` while it
    /// has not been approved.
    pub fn proposal_approver(&self, id: &str) -> Result<Option<String>, StoreError> {
        let id = id.to_owned();
        self.call(move |pool| async move {
            let actor: Option<String> = sqlx::query_scalar(
                "SELECT actor FROM action_proposal_events \
                 WHERE proposal_id = ? AND to_state = 'approved' ORDER BY sequence DESC LIMIT 1",
            )
            .bind(id)
            .fetch_optional(&pool)
            .await?;
            Ok(actor)
        })
    }

    /// Approved proposals a grant approved, in the order stored, each with the grant id.
    pub fn approved_by_grant(&self) -> Result<Vec<(ActionProposal, String)>, StoreError> {
        let mut found = Vec::new();
        for proposal in self.approved_action_proposals()? {
            let approver = self.proposal_approver(&proposal.id)?;
            if let Some(grant_id) = approver
                .as_deref()
                .and_then(|actor| actor.strip_prefix("grant:"))
            {
                found.push((proposal, grant_id.to_owned()));
            }
        }
        Ok(found)
    }

    /// Approved proposals, in the order they were stored. Startup reconcile executes
    /// these; a pending proposal is never returned.
    pub fn approved_action_proposals(&self) -> Result<Vec<ActionProposal>, StoreError> {
        self.call(move |pool| async move {
            let rows = sqlx::query(concat!(
                "SELECT ",
                proposal_columns!(),
                " FROM action_proposals WHERE state = 'approved' ORDER BY rowid"
            ))
            .fetch_all(&pool)
            .await?;
            rows.into_iter()
                .map(decode_proposal)
                .collect::<Result<Vec<_>, _>>()
        })
    }

    /// Moves a pending proposal to `approved`. A parent it depends on must already be
    /// `executed`.
    pub fn approve_action_proposal(
        &self,
        id: &str,
        actor: &str,
    ) -> Result<ActionProposal, ResolveError> {
        let (id, actor) = (id.to_owned(), actor.to_owned());
        resolve_call(self, move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let proposal = load_proposal(&mut transaction, &id)
                .await?
                .ok_or(ResolveError::NotFound)?;
            let approved = approve_pending(&mut transaction, &proposal, &actor).await?;
            transaction.commit().await?;
            Ok(approved)
        })
    }

    /// Moves a pending proposal to `failed` with `action_denied` and `denial`, in one
    /// transaction, records the event, and fails its pending dependents. Used by the
    /// approve-time envelope re-check. A proposal that is not pending is refused with
    /// [`ResolveError::NotPending`] and nothing is written.
    pub fn deny_action_proposal(
        &self,
        id: &str,
        denial: &str,
        actor: &str,
    ) -> Result<ActionProposal, ResolveError> {
        let (id, denial, actor) = (id.to_owned(), denial.to_owned(), actor.to_owned());
        resolve_call(self, move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let proposal = load_proposal(&mut transaction, &id)
                .await?
                .ok_or(ResolveError::NotFound)?;
            if proposal.state != ProposalState::Pending {
                return Err(ResolveError::NotPending(proposal.state));
            }
            move_state(
                &mut transaction,
                &proposal,
                ProposalState::Failed,
                None,
                Some(ACTION_DENIED),
                Some(denial.as_str()),
                &actor,
            )
            .await?;
            fail_pending_dependents(&mut transaction, &id, &actor).await?;
            let denied = reload_proposal(&mut transaction, &id).await?;
            transaction.commit().await?;
            Ok(denied)
        })
    }

    /// Moves a pending proposal to `rejected`, and fails its pending dependents.
    pub fn reject_action_proposal(
        &self,
        id: &str,
        actor: &str,
    ) -> Result<ActionProposal, ResolveError> {
        let (id, actor) = (id.to_owned(), actor.to_owned());
        resolve_call(self, move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let proposal = load_proposal(&mut transaction, &id)
                .await?
                .ok_or(ResolveError::NotFound)?;
            if proposal.state != ProposalState::Pending {
                return Err(ResolveError::NotPending(proposal.state));
            }
            move_state(
                &mut transaction,
                &proposal,
                ProposalState::Rejected,
                None,
                None,
                None,
                &actor,
            )
            .await?;
            fail_pending_dependents(&mut transaction, &id, &actor).await?;
            let rejected = reload_proposal(&mut transaction, &id).await?;
            transaction.commit().await?;
            Ok(rejected)
        })
    }

    /// Records how an approved proposal ended: `executed`, or `failed` with its code,
    /// which also fails its pending dependents. The same outcome again returns the
    /// proposal unchanged, and writes nothing.
    pub fn record_action_outcome(
        &self,
        id: &str,
        outcome: ActionOutcome<'_>,
        actor: &str,
    ) -> Result<ActionProposal, ResolveError> {
        let (id, actor) = (id.to_owned(), actor.to_owned());
        let (to, result_ref, error_code) = match outcome {
            ActionOutcome::Executed { result_ref } => {
                (ProposalState::Executed, Some(result_ref.to_owned()), None)
            }
            ActionOutcome::Failed { error_code } => {
                (ProposalState::Failed, None, Some(error_code.to_owned()))
            }
        };
        resolve_call(self, move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let proposal = load_proposal(&mut transaction, &id)
                .await?
                .ok_or(ResolveError::NotFound)?;
            if proposal.state == to
                && proposal.result_ref == result_ref
                && proposal.error_code == error_code
            {
                return Ok(proposal);
            }
            if proposal.state != ProposalState::Approved {
                return Err(ResolveError::NotPending(proposal.state));
            }
            move_state(
                &mut transaction,
                &proposal,
                to,
                result_ref.as_deref(),
                error_code.as_deref(),
                None,
                &actor,
            )
            .await?;
            if to == ProposalState::Failed {
                fail_pending_dependents(&mut transaction, &id, &actor).await?;
            }
            let recorded = reload_proposal(&mut transaction, &id).await?;
            transaction.commit().await?;
            Ok(recorded)
        })
    }
}

/// Runs one resolve transaction. Refusals are the closure's own `Err`; storage
/// failures arrive as [`ResolveError::Store`].
pub(crate) fn resolve_call<T, F, Fut>(store: &SqliteStore, operation: F) -> Result<T, ResolveError>
where
    T: Send + 'static,
    F: FnOnce(SqlitePool) -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, ResolveError>> + Send + 'static,
{
    store
        .call(move |pool| async move { Ok::<_, StoreError>(operation(pool).await) })
        .map_err(ResolveError::Store)?
}

/// Moves the pending `proposal`, read in the caller's transaction, to `approved` by `actor`.
/// A parent it depends on must already be `executed`. The caller commits.
pub(crate) async fn approve_pending(
    connection: &mut SqliteConnection,
    proposal: &ActionProposal,
    actor: &str,
) -> Result<ActionProposal, ResolveError> {
    if proposal.state != ProposalState::Pending {
        return Err(ResolveError::NotPending(proposal.state));
    }
    if let Some(parent_id) = &proposal.depends_on {
        let parent = load_proposal(connection, parent_id).await?;
        if parent.map(|parent| parent.state) != Some(ProposalState::Executed) {
            return Err(ResolveError::ParentNotExecuted);
        }
    }
    move_state(
        connection,
        proposal,
        ProposalState::Approved,
        None,
        None,
        None,
        actor,
    )
    .await?;
    reload_proposal(connection, &proposal.id).await
}

/// Moves the `approved` `proposal`, read in the caller's transaction, to `failed` with
/// `error_code` and fails its pending dependents. The caller commits.
pub(crate) async fn fail_approved(
    connection: &mut SqliteConnection,
    proposal: &ActionProposal,
    error_code: &str,
    actor: &str,
) -> Result<(), ResolveError> {
    if proposal.state != ProposalState::Approved {
        return Err(ResolveError::NotPending(proposal.state));
    }
    move_state(
        connection,
        proposal,
        ProposalState::Failed,
        None,
        Some(error_code),
        None,
        actor,
    )
    .await?;
    fail_pending_dependents(connection, &proposal.id, actor).await
}

/// Moves `proposal` from its current state to `to` by compare-and-set, and appends its
/// event in the same transaction. The update touches only the state, the outcome and
/// `updated_at`, which migration 0037 allows. `denial` is written only by an approve-time
/// denial; the other statement leaves the column out, because a trigger fires on a column
/// named in `SET` even when its value is unchanged.
async fn move_state(
    connection: &mut SqliteConnection,
    proposal: &ActionProposal,
    to: ProposalState,
    result_ref: Option<&str>,
    error_code: Option<&str>,
    denial: Option<&str>,
    actor: &str,
) -> Result<(), ResolveError> {
    let from = proposal.state;
    let updated = match denial {
        Some(denial) => {
            sqlx::query(
                "UPDATE action_proposals \
                 SET state = ?, result_ref = ?, error_code = ?, denial = ?, \
                     updated_at = CURRENT_TIMESTAMP \
                 WHERE id = ? AND state = ?",
            )
            .bind(to.as_str())
            .bind(result_ref)
            .bind(error_code)
            .bind(denial)
            .bind(&proposal.id)
            .bind(from.as_str())
            .execute(&mut *connection)
            .await?
        }
        None => {
            sqlx::query(
                "UPDATE action_proposals \
                 SET state = ?, result_ref = ?, error_code = ?, updated_at = CURRENT_TIMESTAMP \
                 WHERE id = ? AND state = ?",
            )
            .bind(to.as_str())
            .bind(result_ref)
            .bind(error_code)
            .bind(&proposal.id)
            .bind(from.as_str())
            .execute(&mut *connection)
            .await?
        }
    };
    // The row was read in this same immediate transaction, so this only fails if
    // another writer changed it in between, which the transaction rules out.
    if updated.rows_affected() != 1 {
        return Err(ResolveError::NotPending(from));
    }
    sqlx::query(
        "INSERT INTO action_proposal_events \
         (proposal_id, scope, from_state, to_state, actor, digest, error_code, result_ref) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&proposal.id)
    .bind(&proposal.scope)
    .bind(from.as_str())
    .bind(to.as_str())
    .bind(actor)
    .bind(&proposal.digest)
    .bind(error_code)
    .bind(result_ref)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

/// Fails every pending proposal that depends on `parent_id`. Only pending dependents
/// move: an approved one needed its parent executed, so it cannot still depend on it.
async fn fail_pending_dependents(
    connection: &mut SqliteConnection,
    parent_id: &str,
    actor: &str,
) -> Result<(), ResolveError> {
    let rows = sqlx::query(concat!(
        "SELECT ",
        proposal_columns!(),
        " FROM action_proposals WHERE depends_on = ? AND state = 'pending' ORDER BY rowid"
    ))
    .bind(parent_id)
    .fetch_all(&mut *connection)
    .await?;
    for row in rows {
        let dependent = decode_proposal(row)?;
        move_state(
            connection,
            &dependent,
            ProposalState::Failed,
            None,
            Some(DEPENDENCY_NOT_EXECUTED),
            None,
            actor,
        )
        .await?;
    }
    Ok(())
}

pub(crate) async fn load_proposal(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<Option<ActionProposal>, StoreError> {
    sqlx::query(concat!(
        "SELECT ",
        proposal_columns!(),
        " FROM action_proposals WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(&mut *connection)
    .await?
    .map(decode_proposal)
    .transpose()
}

/// Reads a proposal written earlier in the same transaction.
async fn reload_proposal(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<ActionProposal, ResolveError> {
    load_proposal(connection, id)
        .await?
        .ok_or(ResolveError::NotFound)
}

/// Stores the proposals of an accepted block, inside the transaction that completes
/// its turn. Returns the effective id of each action in block order: an action that
/// matches a pending card gets that card's id and writes nothing.
///
/// An action the envelope refused (`block.denial`) is stored already `failed`, with
/// `error_code = action_denied` and its denial key, and gets two events: created
/// `pending`, then `pending` to `failed`. It is never pending, so it matches no card
/// and a repeat of the same refused request is a second `failed` card. It supersedes
/// nothing: a pending card with the same subject stays pending.
///
/// A block is stored whole or not at all. If it would leave more than
/// [`MAX_PENDING_PROPOSALS_PER_SCOPE`] cards pending in `scope`, counted after its own
/// supersedes, everything it wrote is undone (cards, supersedes and events) and the
/// result is `None`. The transaction goes on, so the caller can still complete the
/// turn and record the block as [`ActionBlockOutcome::Rejected`].
pub(crate) async fn insert_block_proposals(
    connection: &mut SqliteConnection,
    scope: &str,
    turn_id: &str,
    block: &ActionBlock,
) -> Result<Option<Vec<String>>, ConversationError> {
    // The cap can only be counted after the last insert, so an over-cap block is undone
    // back to here, including the cards it superseded.
    sqlx::query("SAVEPOINT block_proposals")
        .execute(&mut *connection)
        .await?;
    let mut effective: Vec<String> = Vec::with_capacity(block.actions().len());
    for (index, action) in block.actions().iter().enumerate() {
        let parent = block
            .parent(index)
            .map(|parent| {
                effective
                    .get(parent)
                    .cloned()
                    .ok_or(ConversationError::InvalidInput("action parent"))
            })
            .transpose()?;
        let canonical = action.canonical(parent.as_deref());
        let kind = action.kind();
        let digest = digest_hex(&canonical.digest);
        let denial = block.denial(index);
        let existing: Option<String> = if denial.is_some() {
            None
        } else {
            sqlx::query_scalar(
                "SELECT id FROM action_proposals \
                 WHERE scope = ? AND kind = ? AND digest = ? AND state = 'pending'",
            )
            .bind(scope)
            .bind(kind.as_str())
            .bind(&digest)
            .fetch_optional(&mut *connection)
            .await?
        };
        if let Some(existing) = existing {
            effective.push(existing);
            continue;
        }
        // Every pending card found here differs from this action, because identical
        // ones were matched above. A changed request replaces them. A denied card
        // replaces nothing: it can never be approved, so it must not take away a
        // card the user still can.
        let stale = if denial.is_some() {
            Vec::new()
        } else {
            sqlx::query(
                "SELECT id, digest FROM action_proposals \
                 WHERE scope = ? AND kind = ? AND subject = ? AND state = 'pending'",
            )
            .bind(scope)
            .bind(kind.as_str())
            .bind(&canonical.subject)
            .fetch_all(&mut *connection)
            .await?
        };
        for row in stale {
            let old_id: String = row.try_get("id")?;
            let old_digest: String = row.try_get("digest")?;
            supersede(connection, scope, &old_id, &old_digest).await?;
            // A dependent was built on the card it names, so it goes with that card.
            // Only a create has dependents and a dependent is never a parent, so one
            // level is enough.
            let dependents = sqlx::query(
                "SELECT id, digest FROM action_proposals \
                 WHERE depends_on = ? AND state = 'pending'",
            )
            .bind(&old_id)
            .fetch_all(&mut *connection)
            .await?;
            for dependent in dependents {
                let dependent_id: String = dependent.try_get("id")?;
                let dependent_digest: String = dependent.try_get("digest")?;
                supersede(connection, scope, &dependent_id, &dependent_digest).await?;
            }
        }
        let id = proposal_id(turn_id, index);
        let (state, error_code) = match denial {
            Some(_) => (ProposalState::Failed, Some(ACTION_DENIED)),
            None => (ProposalState::Pending, None),
        };
        sqlx::query(
            "INSERT INTO action_proposals \
             (id, scope, turn_id, idx, kind, params_json, digest, subject, depends_on, \
              state, error_code, denial) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(scope)
        .bind(turn_id)
        .bind(i64::try_from(index).unwrap_or(i64::MAX))
        .bind(kind.as_str())
        .bind(&canonical.json)
        .bind(&digest)
        .bind(&canonical.subject)
        .bind(parent.as_deref())
        .bind(state.as_str())
        .bind(error_code)
        .bind(denial.map(Denial::as_str))
        .execute(&mut *connection)
        .await?;
        record_event(
            connection,
            &id,
            scope,
            None,
            ProposalState::Pending,
            &digest,
            None,
        )
        .await?;
        if state == ProposalState::Failed {
            record_event(
                connection,
                &id,
                scope,
                Some(ProposalState::Pending),
                ProposalState::Failed,
                &digest,
                error_code,
            )
            .await?;
        }
        effective.push(id);
    }
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM action_proposals WHERE scope = ? AND state = 'pending'",
    )
    .bind(scope)
    .fetch_one(&mut *connection)
    .await?;
    let over = usize::try_from(pending).unwrap_or(usize::MAX) > MAX_PENDING_PROPOSALS_PER_SCOPE;
    if over {
        sqlx::query("ROLLBACK TO block_proposals")
            .execute(&mut *connection)
            .await?;
    }
    sqlx::query("RELEASE block_proposals")
        .execute(&mut *connection)
        .await?;
    Ok((!over).then_some(effective))
}

/// Deletes one conversation's proposals. Runs in the clear transaction with the gate
/// open, before its turns are deleted, because a proposal references its turn.
pub(crate) async fn delete_scope_proposals(
    connection: &mut SqliteConnection,
    scope: &str,
) -> Result<u64, StoreError> {
    let deleted = sqlx::query("DELETE FROM action_proposals WHERE scope = ?")
        .bind(scope)
        .execute(connection)
        .await?;
    Ok(deleted.rows_affected())
}

/// Marks one pending proposal superseded and records the event. A row that is no
/// longer pending is left alone and records nothing.
async fn supersede(
    connection: &mut SqliteConnection,
    scope: &str,
    id: &str,
    digest: &str,
) -> Result<(), ConversationError> {
    let updated = sqlx::query(
        "UPDATE action_proposals SET state = 'superseded', updated_at = CURRENT_TIMESTAMP \
         WHERE id = ? AND state = 'pending'",
    )
    .bind(id)
    .execute(&mut *connection)
    .await?;
    if updated.rows_affected() == 1 {
        record_event(
            connection,
            id,
            scope,
            Some(ProposalState::Pending),
            ProposalState::Superseded,
            digest,
            None,
        )
        .await?;
    }
    Ok(())
}

/// Appends one agent event: creation, supersession or denial (`error_code`). Resolve
/// events go through `move_state`, which also records the user's actor and outcome.
async fn record_event(
    connection: &mut SqliteConnection,
    id: &str,
    scope: &str,
    from: Option<ProposalState>,
    to: ProposalState,
    digest: &str,
    error_code: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO action_proposal_events \
         (proposal_id, scope, from_state, to_state, actor, digest, error_code) \
         VALUES (?, ?, ?, ?, 'agent', ?, ?)",
    )
    .bind(id)
    .bind(scope)
    .bind(from.map(ProposalState::as_str))
    .bind(to.as_str())
    .bind(digest)
    .bind(error_code)
    .execute(connection)
    .await?;
    Ok(())
}

pub(crate) fn decode_proposal(row: SqliteRow) -> Result<ActionProposal, StoreError> {
    let id: String = row.try_get("id")?;
    let idx: i64 = row.try_get("idx")?;
    let kind: String = row.try_get("kind")?;
    let state: String = row.try_get("state")?;
    Ok(ActionProposal {
        idx: u32::try_from(idx).map_err(|_| corrupt(format!("proposal {id} index {idx}")))?,
        kind: ActionKind::parse(&kind)
            .ok_or_else(|| corrupt(format!("unknown action kind {kind}")))?,
        state: ProposalState::parse(&state)?,
        scope: row.try_get("scope")?,
        turn_id: row.try_get("turn_id")?,
        params_json: row.try_get("params_json")?,
        digest: row.try_get("digest")?,
        subject: row.try_get("subject")?,
        depends_on: row.try_get("depends_on")?,
        result_ref: row.try_get("result_ref")?,
        error_code: row.try_get("error_code")?,
        denial: row.try_get("denial")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        id,
    })
}

fn decode_event(row: SqliteRow) -> Result<ProposalEvent, StoreError> {
    let from_state: Option<String> = row.try_get("from_state")?;
    let to_state: String = row.try_get("to_state")?;
    Ok(ProposalEvent {
        sequence: row.try_get("sequence")?,
        proposal_id: row.try_get("proposal_id")?,
        scope: row.try_get("scope")?,
        from_state: from_state
            .map(|state| ProposalState::parse(&state))
            .transpose()?,
        to_state: ProposalState::parse(&to_state)?,
        actor: row.try_get("actor")?,
        digest: row.try_get("digest")?,
        error_code: row.try_get("error_code")?,
        result_ref: row.try_get("result_ref")?,
        created_at: row.try_get("created_at")?,
    })
}
