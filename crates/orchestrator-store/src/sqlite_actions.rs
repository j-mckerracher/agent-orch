//! Action proposals parsed from orchestrator replies (DPES-1105).
//!
//! A proposal is a request the user may approve. Storing one touches nothing
//! else: no workstream, objective or task changes, and nothing runs in this
//! phase. Rows are written only by [`SqliteStore::complete_conversation_turn`],
//! inside the transaction that completes the turn, so a turn never completes
//! without the proposals its block produced.
//!
//! Migration 0037 holds the rules that outside callers cannot break: a
//! proposal's parameters are fixed, a proposal is deleted only by a history
//! clear, and events are append-only.

use orchestrator_core::orchestrator_action::{digest_hex, proposal_id, ActionBlock, ActionKind};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection};

use crate::sqlite_conversations::{corrupt, ConversationError, ConversationTarget};
use crate::{SqliteStore, StoreError};

macro_rules! proposal_columns {
    () => {
        "id, scope, turn_id, idx, kind, params_json, digest, subject, depends_on, state, \
         result_ref, error_code, created_at, updated_at"
    };
}

/// What became of a turn's action block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionBlockOutcome {
    /// The block parsed, and its actions are pending proposals.
    Accepted,
    /// The block was malformed or out of scope, so nothing was stored.
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
    /// A block was present and valid. Its actions become pending proposals.
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
}

/// Stores the proposals of an accepted block, inside the transaction that completes
/// its turn. Returns the effective id of each action in block order: an action that
/// matches a pending card gets that card's id and writes nothing.
pub(crate) async fn insert_block_proposals(
    connection: &mut SqliteConnection,
    scope: &str,
    turn_id: &str,
    block: &ActionBlock,
) -> Result<Vec<String>, ConversationError> {
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
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT id FROM action_proposals \
             WHERE scope = ? AND kind = ? AND digest = ? AND state = 'pending'",
        )
        .bind(scope)
        .bind(kind.as_str())
        .bind(&digest)
        .fetch_optional(&mut *connection)
        .await?;
        if let Some(existing) = existing {
            effective.push(existing);
            continue;
        }
        // Every pending card found here differs from this action, because identical
        // ones were matched above. A changed request replaces them.
        let stale = sqlx::query(
            "SELECT id, digest FROM action_proposals \
             WHERE scope = ? AND kind = ? AND subject = ? AND state = 'pending'",
        )
        .bind(scope)
        .bind(kind.as_str())
        .bind(&canonical.subject)
        .fetch_all(&mut *connection)
        .await?;
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
        sqlx::query(
            "INSERT INTO action_proposals \
             (id, scope, turn_id, idx, kind, params_json, digest, subject, depends_on) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .execute(&mut *connection)
        .await?;
        record_event(
            connection,
            &id,
            scope,
            None,
            ProposalState::Pending,
            &digest,
        )
        .await?;
        effective.push(id);
    }
    Ok(effective)
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
        )
        .await?;
    }
    Ok(())
}

/// Appends one event. Phase 1 records only the agent's own actions.
async fn record_event(
    connection: &mut SqliteConnection,
    id: &str,
    scope: &str,
    from: Option<ProposalState>,
    to: ProposalState,
    digest: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO action_proposal_events \
         (proposal_id, scope, from_state, to_state, actor, digest) \
         VALUES (?, ?, ?, ?, 'agent', ?)",
    )
    .bind(id)
    .bind(scope)
    .bind(from.map(ProposalState::as_str))
    .bind(to.as_str())
    .bind(digest)
    .execute(connection)
    .await?;
    Ok(())
}

fn decode_proposal(row: SqliteRow) -> Result<ActionProposal, StoreError> {
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
