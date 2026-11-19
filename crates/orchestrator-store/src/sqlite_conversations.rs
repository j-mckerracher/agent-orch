//! SQLite persistence for orchestrator conversations (M7-08).
//!
//! A conversation is a transcript between the user and the orchestrator, either
//! global or scoped to one workstream. The transcript is data only: this module
//! never touches tasks, approvals, deliveries, or policy, and nothing stored
//! here is read back as a command.
//!
//! # Ordering and recovery
//!
//! - `conversation_entries.sequence` is the replay cursor. Entries are
//!   append-only (enforced by triggers), so a cursor read after a restart sees
//!   the same history in the same order. The one exception is a history clear
//!   (DPES-1105, see [`SqliteStore::clear_conversation_history`]), which deletes
//!   a conversation's rows and advances its `clear_generation`.
//! - A conversation has at most one `running` turn. A message that arrives while
//!   a turn runs is refused ([`ConversationError::TurnRunning`]); that keeps a
//!   reply correlated with exactly one message.
//! - Every turn ends in exactly one terminal state. The state change and its
//!   terminal entry commit together, so a late reply chunk after cancel or
//!   failure is refused rather than appended.
//! - The store owns the transcript. It does not own the runtime that produced a
//!   reply. [`SqliteStore::interrupt_open_conversation_turns`] marks any turn
//!   still `running` after a restart as failed with `interrupted_by_restart`;
//!   runtime state is never trusted or resumed.
//!
//! # Workstream seam (M7-07)
//!
//! M7-07 owns the workstream entity. This module stores `workstream_id` as an
//! opaque, non-empty string and does not check that it exists. The daemon
//! resolves it through `WorkstreamDirectory` before a conversation is created.
//! M7-07 may add a foreign key in its own migration; nothing here changes.

use std::fmt;
use std::future::Future;

use orchestrator_core::memory::MemoryScope;
use orchestrator_core::orchestrator_action::{Action, ActionBlock};
use orchestrator_core::permission::PermissionLevel;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use sqlx::SqlitePool;

use crate::sqlite_actions::{
    delete_scope_proposals, insert_block_proposals, ActionBlockOutcome, TurnBlock,
};
use crate::sqlite_conversation_usage::TurnTrigger;
use crate::sqlite_history::{
    bump_coordinator, coordinator_generation, coordinator_removed, coordinator_scope_id,
    count_coordinator_entries, record_audit, require_project, set_clear_gate,
    tombstone_conversation_sends, wipe_coordinator, ClearOutcome,
};
use crate::sqlite_thinking::THINKING_TRUNCATED_MARKER;
use crate::sqlite_turn_links::record_turn_links;
use crate::sqlite_wake::delete_scope_wake_records;
use crate::{SqliteStore, StoreError};

/// Largest accepted user message, in bytes.
pub const MAX_CONVERSATION_MESSAGE_BYTES: usize = 8 * 1024;
/// Largest single reply chunk, in bytes.
pub const MAX_REPLY_CHUNK_BYTES: usize = 16 * 1024;
/// Largest total reply for one turn, in bytes.
pub const MAX_REPLY_BYTES_PER_TURN: usize = 256 * 1024;
/// Largest total thinking for one turn, in bytes. Thinking has its own budget so it never
/// takes room from the reply.
pub const MAX_THINKING_BYTES_PER_TURN: usize = 1024 * 1024;
/// Largest accepted workstream identifier, in bytes.
pub const MAX_WORKSTREAM_ID_BYTES: usize = 128;

/// Error code recorded when a restart interrupted a running turn.
pub const INTERRUPTED_BY_RESTART: &str = "interrupted_by_restart";

/// What a conversation is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationTarget {
    Global,
    /// Opaque reference resolved by M7-07.
    Workstream(String),
    Project(String),
}

impl ConversationTarget {
    pub(crate) fn id(&self) -> String {
        match self {
            Self::Global => "conversation:global".to_owned(),
            Self::Workstream(id) => format!("conversation:workstream:{id}"),
            Self::Project(id) => format!("conversation:project:{id}"),
        }
    }

    /// The scope of this conversation's preferences and learnings (migration 0067).
    pub fn memory_scope(&self) -> MemoryScope {
        match self {
            Self::Global => MemoryScope::Global,
            Self::Workstream(id) => MemoryScope::Workstream(id.clone()),
            Self::Project(id) => MemoryScope::Project(id.clone()),
        }
    }

    fn parts(&self) -> (&'static str, Option<&str>, Option<&str>) {
        match self {
            Self::Global => ("global", None, None),
            Self::Workstream(id) => ("workstream", Some(id.as_str()), None),
            Self::Project(id) => ("project", None, Some(id.as_str())),
        }
    }

    fn validate(&self) -> Result<(), ConversationError> {
        match self {
            Self::Project(id)
                if id.is_empty() || id.len() > 4096 || id.chars().any(char::is_control) =>
            {
                Err(ConversationError::InvalidInput("project id"))
            }
            Self::Workstream(id)
                if id.is_empty()
                    || id.len() > MAX_WORKSTREAM_ID_BYTES
                    || id.chars().any(char::is_control) =>
            {
                Err(ConversationError::InvalidInput("workstream id"))
            }
            _ => Ok(()),
        }
    }
}

/// Lifecycle of one turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnState {
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl TurnState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(corrupt(format!("unknown turn state {other}"))),
        }
    }
}

/// How a running turn ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnOutcome {
    Completed,
    /// Stable, machine-readable code (1-64 bytes). Never free-form runtime text.
    Failed(String),
    Cancelled,
}

/// One message or reply fragment in a transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationEntry {
    pub sequence: i64,
    pub id: String,
    pub conversation_id: String,
    pub turn_id: String,
    /// `user_message`, `reply_chunk`, `thinking_chunk`, `turn_completed`,
    /// `turn_failed`, or `turn_cancelled`.
    pub kind: String,
    /// `user`, `orchestrator`, or `system`.
    pub author: String,
    pub body: String,
    pub error_code: Option<String>,
    pub created_at: String,
}

/// The permission level a turn was started with and the argument profile it
/// ran with (DPES-1105). A turn from before permission levels has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnPermission {
    pub level: PermissionLevel,
    pub profile: &'static str,
}

/// One user message and the reply it produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationTurn {
    pub id: String,
    pub conversation_id: String,
    pub state: TurnState,
    pub error_code: Option<String>,
    pub runtime_name: String,
    pub runtime_version: String,
    pub reply_bytes: u64,
    /// The level the turn ran with. `None` for a turn from before permission levels.
    pub permission_level: Option<PermissionLevel>,
    /// The argument profile the turn ran with, such as `codex:auto:v1`.
    pub permission_profile: Option<String>,
    /// What the reply's action block did. `None` when the reply had no block, or the
    /// turn predates migration 0037.
    pub action_block: Option<ActionBlockOutcome>,
    /// What opened the turn: a user message, or an automatic wake (DPES-1105 phase 3).
    pub trigger: TurnTrigger,
    pub created_at: String,
    pub updated_at: String,
}

/// A new turn and the user message that opened it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BegunTurn {
    pub turn: ConversationTurn,
    pub user_entry: ConversationEntry,
    /// False when `turn_id` already existed and the stored turn is returned.
    pub created: bool,
}

/// One bounded read of a transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationPage {
    /// Absent until the first message creates the conversation.
    pub conversation_id: Option<String>,
    pub entries: Vec<ConversationEntry>,
    /// Newest sequence in this conversation; 0 when it is empty.
    pub latest_sequence: i64,
    pub latest_turn: Option<ConversationTurn>,
    /// Incremented by every history clear of this conversation; 0 before the first.
    pub clear_generation: i64,
}

/// A conversation mutation failed.
#[derive(Debug)]
pub enum ConversationError {
    Store(StoreError),
    /// The conversation already has a running turn.
    TurnRunning {
        turn_id: String,
    },
    TurnNotFound,
    /// The turn is no longer running, so it accepts no more output.
    TurnNotRunning(TurnState),
    /// The reply would exceed the per-turn or per-chunk bound.
    ReplyTooLarge,
    /// The turn's thinking reached its bound. The turn itself is unaffected.
    ThinkingFull,
    InvalidInput(&'static str),
}

impl fmt::Display for ConversationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "{error}"),
            Self::TurnRunning { turn_id } => {
                write!(formatter, "conversation already has running turn {turn_id}")
            }
            Self::TurnNotFound => formatter.write_str("conversation turn not found"),
            Self::TurnNotRunning(state) => {
                write!(
                    formatter,
                    "turn is {} and accepts no output",
                    state.as_str()
                )
            }
            Self::ReplyTooLarge => formatter.write_str("reply exceeds its size bound"),
            Self::ThinkingFull => formatter.write_str("thinking exceeds its size bound"),
            Self::InvalidInput(field) => write!(formatter, "invalid {field}"),
        }
    }
}

impl std::error::Error for ConversationError {}

impl From<StoreError> for ConversationError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<sqlx::Error> for ConversationError {
    fn from(error: sqlx::Error) -> Self {
        Self::Store(StoreError::Database(error))
    }
}

macro_rules! entry_columns {
    () => {
        "sequence, id, conversation_id, turn_id, kind, author, body, error_code, created_at"
    };
}
macro_rules! turn_columns {
    () => {
        "id, conversation_id, state, error_code, runtime_name, runtime_version, reply_bytes, \
         permission_level, permission_profile, action_block, trigger_kind, created_at, updated_at"
    };
}

impl SqliteStore {
    /// Record a user message and open the turn that will answer it.
    ///
    /// The conversation is created on first use. The conversation row, turn row
    /// and user entry commit together. Calling again with the same `turn_id`
    /// returns the stored turn unchanged (`created == false`).
    pub fn begin_conversation_turn(
        &self,
        target: &ConversationTarget,
        turn_id: &str,
        text: &str,
        runtime_name: &str,
        runtime_version: &str,
    ) -> Result<BegunTurn, ConversationError> {
        self.begin_conversation_turn_as(
            target,
            turn_id,
            text,
            runtime_name,
            runtime_version,
            TurnTrigger::User,
        )
    }

    /// Like [`Self::begin_conversation_turn`], and records what opened the turn. A wake
    /// turn (DPES-1105 phase 3) stores its opening text, written by the daemon, as the
    /// opening entry and is marked by [`TurnTrigger::Wake`].
    pub fn begin_conversation_turn_as(
        &self,
        target: &ConversationTarget,
        turn_id: &str,
        text: &str,
        runtime_name: &str,
        runtime_version: &str,
        trigger: TurnTrigger,
    ) -> Result<BegunTurn, ConversationError> {
        target.validate()?;
        if turn_id.is_empty() || runtime_name.is_empty() || runtime_version.is_empty() {
            return Err(ConversationError::InvalidInput("turn or runtime identity"));
        }
        if text.trim().is_empty() || text.len() > MAX_CONVERSATION_MESSAGE_BYTES {
            return Err(ConversationError::InvalidInput("message"));
        }
        let conversation_id = target.id();
        let (kind, workstream_id, project_id) = target.parts();
        let (workstream_id, project_id) = (
            workstream_id.map(str::to_owned),
            project_id.map(str::to_owned),
        );
        let (turn_id, text) = (turn_id.to_owned(), text.to_owned());
        let (runtime_name, runtime_version) = (runtime_name.to_owned(), runtime_version.to_owned());
        conversation_call(self, move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            if let Some(id) = &project_id {
                require_project(&mut transaction, id).await?;
                if coordinator_removed(&mut transaction, id).await? {
                    return Err(StoreError::RecordNotFound {
                        entity: "coordinator",
                        id: id.clone(),
                    }
                    .into());
                }
            }
            if let Some(turn) = load_turn(&mut transaction, &turn_id).await? {
                if turn.conversation_id != conversation_id {
                    return Err(ConversationError::InvalidInput("turn id"));
                }
                let user_entry = load_entry(&mut transaction, &format!("{turn_id}:user")).await?;
                transaction.commit().await?;
                return Ok(BegunTurn {
                    turn,
                    user_entry,
                    created: false,
                });
            }
            sqlx::query(
                "INSERT INTO conversations (id, target_kind, workstream_id, project_id) VALUES (?, ?, ?, ?) \
                 ON CONFLICT(id) DO NOTHING",
            )
            .bind(&conversation_id)
            .bind(kind)
            .bind(&workstream_id)
            .bind(&project_id)
            .execute(&mut *transaction)
            .await?;
            let running: Option<String> = sqlx::query_scalar(
                "SELECT id FROM conversation_turns WHERE conversation_id = ? AND state = 'running'",
            )
            .bind(&conversation_id)
            .fetch_optional(&mut *transaction)
            .await?;
            if let Some(running) = running {
                return Err(ConversationError::TurnRunning { turn_id: running });
            }
            sqlx::query(
                "INSERT INTO conversation_turns \
                 (id, conversation_id, state, runtime_name, runtime_version, trigger_kind) \
                 VALUES (?, ?, 'running', ?, ?, ?)",
            )
            .bind(&turn_id)
            .bind(&conversation_id)
            .bind(&runtime_name)
            .bind(&runtime_version)
            .bind(trigger.as_str())
            .execute(&mut *transaction)
            .await?;
            let user_entry = insert_entry(
                &mut transaction,
                &format!("{turn_id}:user"),
                &conversation_id,
                &turn_id,
                "user_message",
                "user",
                &text,
                None,
            )
            .await?;
            let turn = load_turn(&mut transaction, &turn_id)
                .await?
                .ok_or_else(|| corrupt("turn missing after insert".to_owned()))?;
            transaction.commit().await?;
            Ok(BegunTurn {
                turn,
                user_entry,
                created: true,
            })
        })
    }

    /// Append one reply fragment to a running turn.
    ///
    /// Refused once the turn is terminal, so output that arrives after cancel,
    /// failure, or a restart never enters the transcript.
    pub fn append_reply_chunk(
        &self,
        turn_id: &str,
        text: &str,
    ) -> Result<ConversationEntry, ConversationError> {
        if text.is_empty() {
            return Err(ConversationError::InvalidInput("reply chunk"));
        }
        if text.len() > MAX_REPLY_CHUNK_BYTES {
            return Err(ConversationError::ReplyTooLarge);
        }
        let (turn_id, text) = (turn_id.to_owned(), text.to_owned());
        conversation_call(self, move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let turn = load_turn(&mut transaction, &turn_id)
                .await?
                .ok_or(ConversationError::TurnNotFound)?;
            if turn.state != TurnState::Running {
                return Err(ConversationError::TurnNotRunning(turn.state));
            }
            let total = turn.reply_bytes + text.len() as u64;
            if total > MAX_REPLY_BYTES_PER_TURN as u64 {
                return Err(ConversationError::ReplyTooLarge);
            }
            let chunks: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM conversation_entries WHERE turn_id = ? AND kind = 'reply_chunk'",
            )
            .bind(&turn_id)
            .fetch_one(&mut *transaction)
            .await?;
            let entry = insert_entry(
                &mut transaction,
                &format!("{turn_id}:chunk:{}", chunks + 1),
                &turn.conversation_id,
                &turn_id,
                "reply_chunk",
                "orchestrator",
                &text,
                None,
            )
            .await?;
            sqlx::query(
                "UPDATE conversation_turns SET reply_bytes = ?, updated_at = CURRENT_TIMESTAMP \
                 WHERE id = ?",
            )
            .bind(i64::try_from(total).unwrap_or(i64::MAX))
            .bind(&turn_id)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            Ok(entry)
        })
    }

    /// Append one thinking fragment to a running turn, mirroring [`Self::append_reply_chunk`]
    /// with its own budget: `thinking_bytes` counts it and `reply_bytes` never does.
    ///
    /// When the fragment would pass [`MAX_THINKING_BYTES_PER_TURN`], the first such call stores
    /// one final `[thinking truncated: limit reached]` entry; that call and every later one
    /// return [`ConversationError::ThinkingFull`]. The turn stays running.
    pub fn append_thinking_chunk(
        &self,
        turn_id: &str,
        text: &str,
    ) -> Result<ConversationEntry, ConversationError> {
        if text.is_empty() {
            return Err(ConversationError::InvalidInput("thinking chunk"));
        }
        if text.len() > MAX_REPLY_CHUNK_BYTES {
            return Err(ConversationError::ReplyTooLarge);
        }
        let (turn_id, text) = (turn_id.to_owned(), text.to_owned());
        conversation_call(self, move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let turn = load_turn(&mut transaction, &turn_id)
                .await?
                .ok_or(ConversationError::TurnNotFound)?;
            if turn.state != TurnState::Running {
                return Err(ConversationError::TurnNotRunning(turn.state));
            }
            let marker_id = format!("{turn_id}:think:truncated");
            let (used, marked): (i64, bool) = sqlx::query_as(
                "SELECT thinking_bytes, EXISTS (SELECT 1 FROM conversation_entries WHERE id = ?) \
                 FROM conversation_turns WHERE id = ?",
            )
            .bind(&marker_id)
            .bind(&turn_id)
            .fetch_one(&mut *transaction)
            .await?;
            if marked {
                return Err(ConversationError::ThinkingFull);
            }
            let total = used + text.len() as i64;
            if total > MAX_THINKING_BYTES_PER_TURN as i64 {
                insert_entry(
                    &mut transaction,
                    &marker_id,
                    &turn.conversation_id,
                    &turn_id,
                    "thinking_chunk",
                    "orchestrator",
                    THINKING_TRUNCATED_MARKER,
                    None,
                )
                .await?;
                transaction.commit().await?;
                return Err(ConversationError::ThinkingFull);
            }
            let chunks: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM conversation_entries WHERE turn_id = ? AND kind = 'thinking_chunk'",
            )
            .bind(&turn_id)
            .fetch_one(&mut *transaction)
            .await?;
            let entry = insert_entry(
                &mut transaction,
                &format!("{turn_id}:think:{}", chunks + 1),
                &turn.conversation_id,
                &turn_id,
                "thinking_chunk",
                "orchestrator",
                &text,
                None,
            )
            .await?;
            sqlx::query(
                "UPDATE conversation_turns SET thinking_bytes = ?, updated_at = CURRENT_TIMESTAMP \
                 WHERE id = ?",
            )
            .bind(total)
            .bind(&turn_id)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            Ok(entry)
        })
    }

    /// Record the permission level and profile a turn launched with. Called once,
    /// after the agent started, so a turn that never launched records none.
    /// Recording does not depend on the turn's state: the agent may finish
    /// before this call returns.
    pub fn record_conversation_turn_permission(
        &self,
        turn_id: &str,
        permission: TurnPermission,
    ) -> Result<(), ConversationError> {
        let turn_id = turn_id.to_owned();
        conversation_call(self, move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let result = sqlx::query(
                "UPDATE conversation_turns \
                 SET permission_level = ?, permission_profile = ? \
                 WHERE id = ? AND permission_level IS NULL",
            )
            .bind(permission.level.as_str())
            .bind(permission.profile)
            .bind(&turn_id)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            if result.rows_affected() == 1 {
                Ok(())
            } else {
                Err(ConversationError::TurnNotFound)
            }
        })
    }

    /// Marks a turn as one the user's relay handed a scout report to (DPES-1105 phase 4a,
    /// column `read_report` of migration 0044), so a proposal made in it can say so. Safe
    /// to repeat. A turn that does not exist fails with `TurnNotFound`.
    pub fn mark_turn_read_report(&self, turn_id: &str) -> Result<(), ConversationError> {
        let turn_id = turn_id.to_owned();
        conversation_call(self, move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let result = sqlx::query("UPDATE conversation_turns SET read_report = 1 WHERE id = ?")
                .bind(&turn_id)
                .execute(&mut *transaction)
                .await?;
            transaction.commit().await?;
            if result.rows_affected() == 1 {
                Ok(())
            } else {
                Err(ConversationError::TurnNotFound)
            }
        })
    }

    /// Whether the turn was handed a scout report. `false` for a turn that does not exist.
    pub fn turn_read_report(&self, turn_id: &str) -> Result<bool, ConversationError> {
        let turn_id = turn_id.to_owned();
        conversation_call(self, move |pool| async move {
            let flag: Option<i64> =
                sqlx::query_scalar("SELECT read_report FROM conversation_turns WHERE id = ?")
                    .bind(&turn_id)
                    .fetch_optional(&pool)
                    .await?;
            Ok(flag.is_some_and(|flag| flag != 0))
        })
    }

    /// End a running turn. Returns the terminal entry, or `None` when the turn
    /// was already terminal (the first terminal state wins).
    pub fn finish_conversation_turn(
        &self,
        turn_id: &str,
        outcome: &TurnOutcome,
    ) -> Result<Option<ConversationEntry>, ConversationError> {
        self.finish_conversation_turn_with_detail(turn_id, outcome, "")
    }

    /// Like [`Self::finish_conversation_turn`], and the terminal entry's body is
    /// `detail`. The detail is a daemon-chosen token of at most 64 bytes, such as
    /// the flag a refused agent no longer lists; it is never runtime text.
    pub fn finish_conversation_turn_with_detail(
        &self,
        turn_id: &str,
        outcome: &TurnOutcome,
        detail: &str,
    ) -> Result<Option<ConversationEntry>, ConversationError> {
        if detail.len() > 64 {
            return Err(ConversationError::InvalidInput("turn detail"));
        }
        let turn_id = turn_id.to_owned();
        let outcome = outcome.clone();
        let detail = detail.to_owned();
        conversation_call(self, move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let entry = finish_turn(&mut transaction, &turn_id, &outcome, &detail).await?;
            transaction.commit().await?;
            Ok(entry)
        })
    }

    /// Completes a running turn and records what its reply's action block did, in one
    /// transaction (DPES-1105). The terminal state, the block outcome and any proposals
    /// commit together, so a turn never completes without the proposals it produced.
    ///
    /// `None` means the turn was already terminal, and nothing was recorded. Otherwise
    /// the effective proposal id of each action comes back in block order. An action
    /// that matches a pending card returns that card's id.
    ///
    /// A block is stored whole or not at all. One that would leave the conversation with
    /// more than 20 pending proposals stores nothing and is recorded as
    /// [`ActionBlockOutcome::Rejected`], and the list that comes back is empty, as it is
    /// for a block refused before it got here. The turn still completes.
    ///
    /// This is a trusted-caller API. It does not check the session's negotiated
    /// capabilities. The daemon applies the `orchestrator_actions` gate, and a caller
    /// must pass [`TurnBlock::Stripped`] when the session did not negotiate it.
    pub fn complete_conversation_turn(
        &self,
        turn_id: &str,
        block: TurnBlock,
    ) -> Result<Option<Vec<String>>, ConversationError> {
        let turn_id = turn_id.to_owned();
        conversation_call(self, move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            if finish_turn(&mut transaction, &turn_id, &TurnOutcome::Completed, "")
                .await?
                .is_none()
            {
                transaction.commit().await?;
                return Ok(None);
            }
            let turn = load_turn(&mut transaction, &turn_id)
                .await?
                .ok_or(ConversationError::TurnNotFound)?;
            let block_tasks = match &block {
                TurnBlock::Accepted(action_block) => block_task_ids(action_block),
                _ => Vec::new(),
            };
            let (outcome, effective) = match block {
                TurnBlock::None => (None, Vec::new()),
                TurnBlock::Stripped => (Some(ActionBlockOutcome::Stripped), Vec::new()),
                TurnBlock::Rejected => (Some(ActionBlockOutcome::Rejected), Vec::new()),
                TurnBlock::Accepted(action_block) => {
                    let stored = insert_block_proposals(
                        &mut transaction,
                        &turn.conversation_id,
                        &turn_id,
                        &action_block,
                    )
                    .await?;
                    // `None`: the block would have left the conversation over its limit of
                    // pending proposals. Nothing of it was kept, so the turn records it as
                    // rejected here, in the same transaction that completes the turn.
                    match stored {
                        Some(effective) => (Some(ActionBlockOutcome::Accepted), effective),
                        None => (Some(ActionBlockOutcome::Rejected), Vec::new()),
                    }
                }
            };
            if let Some(outcome) = outcome {
                sqlx::query("UPDATE conversation_turns SET action_block = ? WHERE id = ?")
                    .bind(outcome.as_str())
                    .bind(&turn_id)
                    .execute(&mut *transaction)
                    .await?;
            }
            record_turn_links(&mut transaction, &turn_id, &block_tasks).await?;
            transaction.commit().await?;
            Ok(Some(effective))
        })
    }

    /// Fail every turn left `running` by a previous daemon process.
    ///
    /// Returns the ids of the interrupted turns.
    pub fn interrupt_open_conversation_turns(&self) -> Result<Vec<String>, ConversationError> {
        conversation_call(self, move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let open: Vec<String> = sqlx::query_scalar(
                "SELECT id FROM conversation_turns WHERE state = 'running' ORDER BY created_at, id",
            )
            .fetch_all(&mut *transaction)
            .await?;
            for turn_id in &open {
                finish_turn(
                    &mut transaction,
                    turn_id,
                    &TurnOutcome::Failed(INTERRUPTED_BY_RESTART.to_owned()),
                    "",
                )
                .await?;
            }
            transaction.commit().await?;
            Ok(open)
        })
    }

    /// Load one turn by id.
    pub fn conversation_turn(
        &self,
        turn_id: &str,
    ) -> Result<Option<ConversationTurn>, ConversationError> {
        let turn_id = turn_id.to_owned();
        conversation_call(self, move |pool| async move {
            let mut connection = pool.acquire().await?;
            Ok(load_turn(&mut connection, &turn_id).await?)
        })
    }

    /// The turn of `target` that is running now, if any.
    pub fn running_conversation_turn(
        &self,
        target: &ConversationTarget,
    ) -> Result<Option<ConversationTurn>, ConversationError> {
        target.validate()?;
        let conversation_id = target.id();
        conversation_call(self, move |pool| async move {
            let mut connection = pool.acquire().await?;
            Ok(sqlx::query(concat!(
                "SELECT ",
                turn_columns!(),
                " FROM conversation_turns WHERE conversation_id = ? AND state = 'running'"
            ))
            .bind(&conversation_id)
            .fetch_optional(&mut *connection)
            .await?
            .map(decode_turn)
            .transpose()?)
        })
    }

    /// The reply text of a turn, cut to at most `max_bytes` on a character boundary: its
    /// `reply_chunk` entries joined in order. Empty when the turn wrote no reply. The daemon
    /// uses it to recognize the quiet token without reading a whole reply.
    pub fn turn_reply_text(
        &self,
        turn_id: &str,
        max_bytes: usize,
    ) -> Result<String, ConversationError> {
        let turn_id = turn_id.to_owned();
        conversation_call(self, move |pool| async move {
            let mut connection = pool.acquire().await?;
            let chunks: Vec<String> = sqlx::query_scalar(
                "SELECT body FROM conversation_entries \
                 WHERE turn_id = ? AND kind = 'reply_chunk' ORDER BY sequence",
            )
            .bind(&turn_id)
            .fetch_all(&mut *connection)
            .await?;
            let mut reply = String::new();
            for chunk in chunks {
                if reply.len() >= max_bytes {
                    break;
                }
                reply.push_str(&chunk);
            }
            if reply.len() > max_bytes {
                let mut end = max_bytes;
                while !reply.is_char_boundary(end) {
                    end -= 1;
                }
                reply.truncate(end);
            }
            Ok(reply)
        })
    }

    /// Read a bounded page of a transcript after `after_sequence`.
    ///
    /// A target without messages yields an empty page rather than an error.
    pub fn conversation_page(
        &self,
        target: &ConversationTarget,
        after_sequence: i64,
        limit: usize,
    ) -> Result<ConversationPage, ConversationError> {
        target.validate()?;
        if after_sequence < 0 {
            return Err(StoreError::InvalidEventCursor.into());
        }
        let limit = i64::try_from(limit).map_err(|_| StoreError::InvalidEventLimit)?;
        let conversation_id = target.id();
        conversation_call(self, move |pool| async move {
            let mut transaction = pool.begin().await?;
            let generation: Option<i64> =
                sqlx::query_scalar("SELECT clear_generation FROM conversations WHERE id = ?")
                    .bind(&conversation_id)
                    .fetch_optional(&mut *transaction)
                    .await?;
            let Some(clear_generation) = generation else {
                return Ok(ConversationPage {
                    conversation_id: None,
                    entries: Vec::new(),
                    latest_sequence: 0,
                    latest_turn: None,
                    clear_generation: 0,
                });
            };
            let rows = sqlx::query(concat!(
                "SELECT ",
                entry_columns!(),
                " FROM conversation_entries \
                 WHERE conversation_id = ? AND sequence > ? ORDER BY sequence LIMIT ?"
            ))
            .bind(&conversation_id)
            .bind(after_sequence)
            .bind(limit)
            .fetch_all(&mut *transaction)
            .await?;
            let entries = rows
                .into_iter()
                .map(decode_entry)
                .collect::<Result<Vec<_>, _>>()?;
            let latest_sequence: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(sequence), 0) FROM conversation_entries WHERE conversation_id = ?",
            )
            .bind(&conversation_id)
            .fetch_one(&mut *transaction)
            .await?;
            let latest_turn = sqlx::query(concat!(
                "SELECT ",
                turn_columns!(),
                " FROM conversation_turns WHERE conversation_id = ? \
                 ORDER BY rowid DESC LIMIT 1"
            ))
            .bind(&conversation_id)
            .fetch_optional(&mut *transaction)
            .await?
            .map(decode_turn)
            .transpose()?;
            Ok(ConversationPage {
                conversation_id: Some(conversation_id),
                entries,
                latest_sequence,
                latest_turn,
                clear_generation,
            })
        })
    }
}

impl SqliteStore {
    /// Permanently delete one conversation's messages, replies, turns, action
    /// proposals and the user's notes for it (DPES-1105).
    ///
    /// A running turn refuses the clear with [`ConversationError::TurnRunning`], and
    /// nothing is deleted. The check and the delete share one `BEGIN IMMEDIATE`
    /// transaction, so a turn cannot start between them. Other conversations are
    /// untouched. A conversation with nothing in it is left as it is.
    pub fn clear_conversation_history(
        &self,
        target: &ConversationTarget,
    ) -> Result<ClearOutcome, ConversationError> {
        self.clear_conversation_history_as(target, false)
    }

    pub(crate) fn clear_conversation_history_as(
        &self,
        target: &ConversationTarget,
        remove: bool,
    ) -> Result<ClearOutcome, ConversationError> {
        target.validate()?;
        let conversation_id = target.id();
        let (kind, workstream_id, project_id) = target.parts();
        let (workstream_id, project_id) = (
            workstream_id.map(str::to_owned),
            project_id.map(str::to_owned),
        );
        let outcome = conversation_call(self, move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let legacy_entries = if let Some(id) = &project_id {
                require_project(&mut transaction, id).await?;
                if coordinator_removed(&mut transaction, id).await? {
                    if !remove {
                        return Err(StoreError::RecordNotFound {
                            entity: "coordinator",
                            id: id.clone(),
                        }
                        .into());
                    }
                    return Ok(ClearOutcome {
                        cleared: false,
                        entries_deleted: 0,
                        turns_deleted: 0,
                        clear_generation: coordinator_generation(&mut transaction, id).await?,
                    });
                }
                // A coordinator that owns unfinished work or an unanswered routed request stays
                // (DPES-1108 phase 7). Migration 0070 holds the same rule as a trigger.
                if remove
                    && crate::sqlite_coordinators::coordinator_has_open_work(&mut transaction, id)
                        .await?
                {
                    return Err(StoreError::from(
                        crate::sqlite_coordinators::CoordinatorError::OpenWork,
                    )
                    .into());
                }
                count_coordinator_entries(&mut transaction, id).await?
            } else {
                0
            };
            let running: Option<String> = sqlx::query_scalar(
                "SELECT id FROM conversation_turns WHERE conversation_id = ? AND state = 'running'",
            )
            .bind(&conversation_id)
            .fetch_optional(&mut *transaction)
            .await?;
            if let Some(turn_id) = running {
                return Err(ConversationError::TurnRunning { turn_id });
            }
            // Grants and their events stay, and so do this conversation's preferences and
            // learnings: the user manages them in Settings, and none is ever deleted silently
            // (DPES-1108 phase 6).
            let entries: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM conversation_entries WHERE conversation_id = ?",
            )
            .bind(&conversation_id)
            .fetch_one(&mut *transaction)
            .await?;
            let turns: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM conversation_turns WHERE conversation_id = ?",
            )
            .bind(&conversation_id)
            .fetch_one(&mut *transaction)
            .await?;
            let wake_records: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM wake_records WHERE scope = ?")
                    .bind(&conversation_id)
                    .fetch_one(&mut *transaction)
                    .await?;
            if entries == 0 && turns == 0 && wake_records == 0 && legacy_entries == 0 && !remove {
                let clear_generation: Option<i64> =
                    sqlx::query_scalar("SELECT clear_generation FROM conversations WHERE id = ?")
                        .bind(&conversation_id)
                        .fetch_optional(&mut *transaction)
                        .await?;
                let clear_generation = if let Some(id) = &project_id {
                    coordinator_generation(&mut transaction, id).await?
                } else {
                    clear_generation.unwrap_or(0)
                };
                transaction.commit().await?;
                return Ok(ClearOutcome {
                    cleared: false,
                    entries_deleted: 0,
                    turns_deleted: 0,
                    clear_generation,
                });
            }
            let entries = u64::try_from(entries).unwrap_or(0) + legacy_entries;
            let turns = u64::try_from(turns).unwrap_or(0);
            if let Some(id) = &project_id {
                wipe_coordinator(&mut transaction, id).await?;
            }
            set_clear_gate(&mut transaction, true).await?;
            delete_scope_proposals(&mut transaction, &conversation_id).await?;
            // The wake turn ledger and the usage receipts stay: they hold no text, and a
            // clear must not reset a cap or the token budget.
            delete_scope_wake_records(&mut transaction, &conversation_id).await?;
            tombstone_conversation_sends(&mut transaction, &conversation_id).await?;
            sqlx::query("DELETE FROM conversation_entries WHERE conversation_id = ?")
                .bind(&conversation_id)
                .execute(&mut *transaction)
                .await?;
            sqlx::query("DELETE FROM conversation_turns WHERE conversation_id = ?")
                .bind(&conversation_id)
                .execute(&mut *transaction)
                .await?;
            set_clear_gate(&mut transaction, false).await?;
            let clear_generation: i64 = sqlx::query_scalar(
                "INSERT INTO conversations (id, target_kind, workstream_id, project_id) VALUES (?, ?, ?, ?) \
                 ON CONFLICT(id) DO UPDATE SET clear_generation = clear_generation + 1 \
                 RETURNING clear_generation",
            )
            .bind(&conversation_id)
            .bind(kind)
            .bind(&workstream_id)
            .bind(&project_id)
            .fetch_one(&mut *transaction)
            .await?;
            let (clear_generation, audit_scope) = if let Some(id) = &project_id {
                let generation = bump_coordinator(&mut transaction, id, remove).await?;
                sqlx::query("UPDATE conversations SET clear_generation = ? WHERE id = ?")
                    .bind(generation)
                    .bind(&conversation_id)
                    .execute(&mut *transaction)
                    .await?;
                (generation, coordinator_scope_id(id))
            } else {
                (clear_generation, conversation_id)
            };
            record_audit(
                &mut transaction,
                if remove { "remove" } else { "clear" },
                &audit_scope,
                entries,
                turns,
            )
            .await?;
            transaction.commit().await?;
            Ok(ClearOutcome {
                cleared: true,
                entries_deleted: entries,
                turns_deleted: turns,
                clear_generation,
            })
        })?;
        self.checkpoint_wal()?;
        Ok(outcome)
    }

    /// One entry by id, or `None` once a history clear has deleted it.
    pub fn conversation_entry(&self, id: &str) -> Result<Option<ConversationEntry>, StoreError> {
        let id = id.to_owned();
        self.call(move |pool| async move {
            sqlx::query(concat!(
                "SELECT ",
                entry_columns!(),
                " FROM conversation_entries WHERE id = ?"
            ))
            .bind(id)
            .fetch_optional(&pool)
            .await?
            .map(decode_entry)
            .transpose()
        })
    }
}

fn conversation_call<T, F, Fut>(store: &SqliteStore, operation: F) -> Result<T, ConversationError>
where
    T: Send + 'static,
    F: FnOnce(SqlitePool) -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, ConversationError>> + Send + 'static,
{
    store
        .call(move |pool| async move { Ok(operation(pool).await) })
        .map_err(ConversationError::Store)?
}

/// The tasks an accepted block names, in block order: the target of each action, the task a
/// `workstream.link_task` links, and the routed request a `task.propose` or a
/// `coordinator.report` names, which [`record_turn_links`] reads as the tasks serving it
/// (DPES-1108 integration). Their recorded pull requests become the turn's links.
fn block_task_ids(block: &ActionBlock) -> Vec<String> {
    block
        .actions()
        .iter()
        .filter_map(|action| match action {
            Action::WorkstreamLinkTask(link) => Some(link.task_id.clone()),
            other => other
                .target_task()
                .or_else(|| other.routed_request())
                .map(str::to_owned),
        })
        .collect()
}

pub(crate) fn corrupt(detail: String) -> StoreError {
    StoreError::Corrupt {
        task_id: String::new(),
        detail,
    }
}

async fn finish_turn(
    connection: &mut sqlx::SqliteConnection,
    turn_id: &str,
    outcome: &TurnOutcome,
    detail: &str,
) -> Result<Option<ConversationEntry>, ConversationError> {
    let (state, error_code, kind, author) = match outcome {
        TurnOutcome::Completed => ("completed", None, "turn_completed", "system"),
        TurnOutcome::Failed(code) => {
            if code.is_empty() || code.len() > 64 {
                return Err(ConversationError::InvalidInput("error code"));
            }
            ("failed", Some(code.as_str()), "turn_failed", "system")
        }
        TurnOutcome::Cancelled => ("cancelled", None, "turn_cancelled", "system"),
    };
    let updated = sqlx::query(
        "UPDATE conversation_turns SET state = ?, error_code = ?, updated_at = CURRENT_TIMESTAMP \
         WHERE id = ? AND state = 'running'",
    )
    .bind(state)
    .bind(error_code)
    .bind(turn_id)
    .execute(&mut *connection)
    .await?;
    if updated.rows_affected() == 0 {
        return match load_turn(connection, turn_id).await? {
            Some(_) => Ok(None),
            None => Err(ConversationError::TurnNotFound),
        };
    }
    let conversation_id: String =
        sqlx::query_scalar("SELECT conversation_id FROM conversation_turns WHERE id = ?")
            .bind(turn_id)
            .fetch_one(&mut *connection)
            .await?;
    insert_entry(
        connection,
        &format!("{turn_id}:end"),
        &conversation_id,
        turn_id,
        kind,
        author,
        detail,
        error_code,
    )
    .await
    .map(Some)
    .map_err(ConversationError::Store)
}

#[allow(clippy::too_many_arguments)]
async fn insert_entry(
    connection: &mut sqlx::SqliteConnection,
    id: &str,
    conversation_id: &str,
    turn_id: &str,
    kind: &str,
    author: &str,
    body: &str,
    error_code: Option<&str>,
) -> Result<ConversationEntry, StoreError> {
    let row = sqlx::query(concat!(
        "INSERT INTO conversation_entries \
         (id, conversation_id, turn_id, kind, author, body, error_code) \
         VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING ",
        entry_columns!()
    ))
    .bind(id)
    .bind(conversation_id)
    .bind(turn_id)
    .bind(kind)
    .bind(author)
    .bind(body)
    .bind(error_code)
    .fetch_one(connection)
    .await?;
    decode_entry(row)
}

async fn load_entry(
    connection: &mut sqlx::SqliteConnection,
    id: &str,
) -> Result<ConversationEntry, StoreError> {
    let row = sqlx::query(concat!(
        "SELECT ",
        entry_columns!(),
        " FROM conversation_entries WHERE id = ?"
    ))
    .bind(id)
    .fetch_one(connection)
    .await?;
    decode_entry(row)
}

async fn load_turn(
    connection: &mut sqlx::SqliteConnection,
    turn_id: &str,
) -> Result<Option<ConversationTurn>, StoreError> {
    sqlx::query(concat!(
        "SELECT ",
        turn_columns!(),
        " FROM conversation_turns WHERE id = ?"
    ))
    .bind(turn_id)
    .fetch_optional(connection)
    .await?
    .map(decode_turn)
    .transpose()
}

fn decode_entry(row: SqliteRow) -> Result<ConversationEntry, StoreError> {
    Ok(ConversationEntry {
        sequence: row.try_get("sequence")?,
        id: row.try_get("id")?,
        conversation_id: row.try_get("conversation_id")?,
        turn_id: row.try_get("turn_id")?,
        kind: row.try_get("kind")?,
        author: row.try_get("author")?,
        body: row.try_get("body")?,
        error_code: row.try_get("error_code")?,
        created_at: row.try_get("created_at")?,
    })
}

fn decode_turn(row: SqliteRow) -> Result<ConversationTurn, StoreError> {
    let state: String = row.try_get("state")?;
    let reply_bytes: i64 = row.try_get("reply_bytes")?;
    let permission_level: Option<String> = row.try_get("permission_level")?;
    let permission_level = permission_level
        .map(|name| {
            PermissionLevel::parse(&name)
                .ok_or_else(|| corrupt(format!("unknown permission level {name}")))
        })
        .transpose()?;
    let action_block: Option<String> = row.try_get("action_block")?;
    let action_block = action_block
        .map(|name| ActionBlockOutcome::parse(&name))
        .transpose()?;
    let trigger: String = row.try_get("trigger_kind")?;
    Ok(ConversationTurn {
        id: row.try_get("id")?,
        conversation_id: row.try_get("conversation_id")?,
        state: TurnState::parse(&state)?,
        error_code: row.try_get("error_code")?,
        runtime_name: row.try_get("runtime_name")?,
        runtime_version: row.try_get("runtime_version")?,
        reply_bytes: u64::try_from(reply_bytes).unwrap_or(0),
        permission_level,
        permission_profile: row.try_get("permission_profile")?,
        action_block,
        trigger: TurnTrigger::from_column(&trigger)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open() -> (tempfile::TempDir, SqliteStore) {
        let directory = tempfile::tempdir().unwrap();
        let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
        (directory, store)
    }

    /// Runs `sql` against the store's own database and returns the database's
    /// own answer, so a CHECK violation shows as `Err`.
    fn execute(store: &SqliteStore, sql: &'static str) -> Result<(), sqlx::Error> {
        store
            .call(move |pool| async move { Ok(sqlx::query(sql).execute(&pool).await.map(|_| ())) })
            .unwrap()
    }

    #[test]
    fn a_turn_from_before_permission_levels_reads_null_and_a_launched_turn_records_its_level() {
        let (_directory, store) = open();
        let target = ConversationTarget::Global;
        store
            .begin_conversation_turn(&target, "turn-old", "hi", "codex", "0.167.2")
            .unwrap();
        let old = store.conversation_turn("turn-old").unwrap().unwrap();
        assert_eq!(old.permission_level, None);
        assert_eq!(old.permission_profile, None);
        store
            .finish_conversation_turn("turn-old", &TurnOutcome::Completed)
            .unwrap();

        store
            .begin_conversation_turn(&target, "turn-new", "hi", "codex", "0.167.2")
            .unwrap();
        store
            .record_conversation_turn_permission(
                "turn-new",
                TurnPermission {
                    level: PermissionLevel::Auto,
                    profile: "codex:auto:v1",
                },
            )
            .unwrap();
        let recorded = store.conversation_turn("turn-new").unwrap().unwrap();
        assert_eq!(recorded.permission_level, Some(PermissionLevel::Auto));
        assert_eq!(
            recorded.permission_profile.as_deref(),
            Some("codex:auto:v1")
        );

        // The first recorded level stands: a second record changes nothing.
        let second = store.record_conversation_turn_permission(
            "turn-new",
            TurnPermission {
                level: PermissionLevel::Full,
                profile: "codex:full:v1",
            },
        );
        assert!(matches!(second, Err(ConversationError::TurnNotFound)));
        let kept = store.conversation_turn("turn-new").unwrap().unwrap();
        assert_eq!(kept.permission_level, Some(PermissionLevel::Auto));
    }

    #[test]
    fn recording_a_level_for_an_unknown_turn_is_refused() {
        let (_directory, store) = open();
        let result = store.record_conversation_turn_permission(
            "no-such-turn",
            TurnPermission {
                level: PermissionLevel::Restricted,
                profile: "codex:restricted:v1",
            },
        );
        assert!(matches!(result, Err(ConversationError::TurnNotFound)));
    }

    #[test]
    fn the_database_refuses_a_level_outside_the_four_and_a_blank_profile() {
        let (_directory, store) = open();
        store
            .begin_conversation_turn(
                &ConversationTarget::Global,
                "turn-1",
                "hi",
                "codex",
                "0.1.0",
            )
            .unwrap();
        assert!(execute(
            &store,
            "UPDATE conversation_turns SET permission_level = 'sandboxed' WHERE id = 'turn-1'",
        )
        .is_err());
        assert!(execute(
            &store,
            "UPDATE conversation_turns SET permission_profile = '' WHERE id = 'turn-1'",
        )
        .is_err());
        assert!(execute(
            &store,
            "UPDATE conversation_turns SET permission_level = 'extended', \
             permission_profile = 'codex:extended:v1' WHERE id = 'turn-1'",
        )
        .is_ok());
    }
}
