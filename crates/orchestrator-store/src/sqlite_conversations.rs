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
//!   the same history in the same order.
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

use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use sqlx::SqlitePool;

use crate::{SqliteStore, StoreError};

/// Largest accepted user message, in bytes.
pub const MAX_CONVERSATION_MESSAGE_BYTES: usize = 8 * 1024;
/// Largest single reply chunk, in bytes.
pub const MAX_REPLY_CHUNK_BYTES: usize = 16 * 1024;
/// Largest total reply for one turn, in bytes.
pub const MAX_REPLY_BYTES_PER_TURN: usize = 256 * 1024;
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
}

impl ConversationTarget {
    fn id(&self) -> String {
        match self {
            Self::Global => "conversation:global".to_owned(),
            Self::Workstream(id) => format!("conversation:workstream:{id}"),
        }
    }

    fn parts(&self) -> (&'static str, Option<&str>) {
        match self {
            Self::Global => ("global", None),
            Self::Workstream(id) => ("workstream", Some(id.as_str())),
        }
    }

    fn validate(&self) -> Result<(), ConversationError> {
        match self {
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
    /// `user_message`, `reply_chunk`, `turn_completed`, `turn_failed`, or
    /// `turn_cancelled`.
    pub kind: String,
    /// `user`, `orchestrator`, or `system`.
    pub author: String,
    pub body: String,
    pub error_code: Option<String>,
    pub created_at: String,
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
         created_at, updated_at"
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
        target.validate()?;
        if turn_id.is_empty() || runtime_name.is_empty() || runtime_version.is_empty() {
            return Err(ConversationError::InvalidInput("turn or runtime identity"));
        }
        if text.trim().is_empty() || text.len() > MAX_CONVERSATION_MESSAGE_BYTES {
            return Err(ConversationError::InvalidInput("message"));
        }
        let conversation_id = target.id();
        let (kind, workstream_id) = target.parts();
        let (kind, workstream_id) = (kind, workstream_id.map(str::to_owned));
        let (turn_id, text) = (turn_id.to_owned(), text.to_owned());
        let (runtime_name, runtime_version) = (runtime_name.to_owned(), runtime_version.to_owned());
        conversation_call(self, move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
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
                "INSERT INTO conversations (id, target_kind, workstream_id) VALUES (?, ?, ?) \
                 ON CONFLICT(id) DO NOTHING",
            )
            .bind(&conversation_id)
            .bind(kind)
            .bind(&workstream_id)
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
                 (id, conversation_id, state, runtime_name, runtime_version) \
                 VALUES (?, ?, 'running', ?, ?)",
            )
            .bind(&turn_id)
            .bind(&conversation_id)
            .bind(&runtime_name)
            .bind(&runtime_version)
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

    /// End a running turn. Returns the terminal entry, or `None` when the turn
    /// was already terminal (the first terminal state wins).
    pub fn finish_conversation_turn(
        &self,
        turn_id: &str,
        outcome: &TurnOutcome,
    ) -> Result<Option<ConversationEntry>, ConversationError> {
        let turn_id = turn_id.to_owned();
        let outcome = outcome.clone();
        conversation_call(self, move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let entry = finish_turn(&mut transaction, &turn_id, &outcome).await?;
            transaction.commit().await?;
            Ok(entry)
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
            let exists: Option<String> =
                sqlx::query_scalar("SELECT id FROM conversations WHERE id = ?")
                    .bind(&conversation_id)
                    .fetch_optional(&mut *transaction)
                    .await?;
            if exists.is_none() {
                return Ok(ConversationPage {
                    conversation_id: None,
                    entries: Vec::new(),
                    latest_sequence: 0,
                    latest_turn: None,
                });
            }
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
            })
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

fn corrupt(detail: String) -> StoreError {
    StoreError::Corrupt {
        task_id: String::new(),
        detail,
    }
}

async fn finish_turn(
    connection: &mut sqlx::SqliteConnection,
    turn_id: &str,
    outcome: &TurnOutcome,
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
        "",
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
    Ok(ConversationTurn {
        id: row.try_get("id")?,
        conversation_id: row.try_get("conversation_id")?,
        state: TurnState::parse(&state)?,
        error_code: row.try_get("error_code")?,
        runtime_name: row.try_get("runtime_name")?,
        runtime_version: row.try_get("runtime_version")?,
        reply_bytes: u64::try_from(reply_bytes).unwrap_or(0),
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
