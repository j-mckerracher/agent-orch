//! Global durable event replay and mutation deduplication.
//!
//! Event sequences are assigned in the transaction that records each event.
//! A page is bounded so a slow subscriber cannot retain unbounded memory.
//! Updated: 2026-11-08T17:19:10+00:00.

use serde_json::Value;
use sqlx::Row;

use crate::{SqliteStore, StoreError};

const MAX_EVENT_PAGE: usize = 1024;

/// One globally sequenced task, session, approval, or delivery event.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct StreamEvent {
    pub sequence: i64,
    pub task_id: String,
    pub session_id: Option<String>,
    pub kind: String,
    pub payload: Value,
    pub created_at: String,
}

/// Result of claiming a persistent idempotency key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdempotencyStart {
    Started,
    InProgress,
    Completed(Value),
}

impl SqliteStore {
    /// Loads a bounded page of events after a global sequence cursor.
    pub fn stream_events_after(
        &self,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<StreamEvent>, StoreError> {
        validate_page(after_sequence, limit)?;
        let limit = i64::try_from(limit).map_err(|_| StoreError::InvalidEventLimit)?;
        self.call(move |pool| async move {
            let rows = sqlx::query(
                "SELECT sequence, task_id, session_id, kind, payload, created_at \
                 FROM durable_events WHERE sequence > ? ORDER BY sequence LIMIT ?",
            )
            .bind(after_sequence)
            .bind(limit)
            .fetch_all(&pool)
            .await?;
            rows.into_iter().map(decode_event).collect()
        })
    }

    /// Replays task events and events belonging to one task session.
    pub fn session_events_after(
        &self,
        task_id: &str,
        session_id: &str,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<StreamEvent>, StoreError> {
        validate_page(after_sequence, limit)?;
        let task_id = task_id.to_owned();
        let session_id = session_id.to_owned();
        let limit = i64::try_from(limit).map_err(|_| StoreError::InvalidEventLimit)?;
        self.call(move |pool| async move {
            let exists = sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS (SELECT 1 FROM sessions WHERE task_id = ? AND id = ?)",
            )
            .bind(&task_id)
            .bind(&session_id)
            .fetch_one(&pool)
            .await?;
            if exists == 0 {
                return Err(StoreError::NoSession {
                    task_id,
                    session_id,
                });
            }
            let rows = sqlx::query(
                "SELECT sequence, task_id, session_id, kind, payload, created_at \
                 FROM durable_events WHERE task_id = ? AND sequence > ? \
                 AND (session_id IS NULL OR session_id = ?) ORDER BY sequence LIMIT ?",
            )
            .bind(&task_id)
            .bind(after_sequence)
            .bind(&session_id)
            .bind(limit)
            .fetch_all(&pool)
            .await?;
            rows.into_iter().map(decode_event).collect()
        })
    }
    /// Returns the current global event cursor, or zero for an empty journal.
    pub fn latest_event_sequence(&self) -> Result<i64, StoreError> {
        self.call(|pool| async move {
            Ok(sqlx::query_scalar::<_, i64>(
                "SELECT COALESCE(MAX(sequence), 0) FROM durable_events",
            )
            .fetch_one(&pool)
            .await?)
        })
    }

    /// Returns the identifier of this database's journal. It is created with
    /// the database and changes when the database file is replaced, so a resume
    /// cursor issued by another journal can be detected.
    pub fn store_epoch(&self) -> Result<String, StoreError> {
        self.call(|pool| async move {
            Ok(
                sqlx::query_scalar::<_, String>("SELECT epoch FROM store_identity WHERE id = 1")
                    .fetch_one(&pool)
                    .await?,
            )
        })
    }

    /// Reads an existing claim without creating one, before fallible request preparation.
    pub fn lookup_idempotency(
        &self,
        key: &str,
        method: &str,
        fingerprint: &str,
    ) -> Result<Option<IdempotencyStart>, StoreError> {
        validate_idempotency(key, method, fingerprint)?;
        let (key, method, fingerprint) =
            (key.to_owned(), method.to_owned(), fingerprint.to_owned());
        self.call(move |pool| async move {
            let row = sqlx::query(
                "SELECT method, fingerprint, result FROM idempotency_records WHERE key = ?",
            )
            .bind(key)
            .fetch_optional(&pool)
            .await?;
            let Some(row) = row else { return Ok(None) };
            let stored_method: String = row.try_get("method")?;
            let stored_fingerprint: String = row.try_get("fingerprint")?;
            let cleared = stored_fingerprint == crate::sqlite_history::CLEARED_FINGERPRINT;
            if stored_method != method || (stored_fingerprint != fingerprint && !cleared) {
                return Err(StoreError::IdempotencyConflict);
            }
            let result: Option<String> = row.try_get("result")?;
            Ok(Some(match result {
                Some(value) => IdempotencyStart::Completed(serde_json::from_str(&value)?),
                None => IdempotencyStart::InProgress,
            }))
        })
    }

    /// Claims a key, or returns its completed result without repeating work.
    pub fn begin_idempotency(
        &self,
        key: &str,
        method: &str,
        fingerprint: &str,
    ) -> Result<IdempotencyStart, StoreError> {
        validate_idempotency(key, method, fingerprint)?;
        let key = key.to_owned();
        let method = method.to_owned();
        let fingerprint = fingerprint.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let inserted = sqlx::query(
                "INSERT INTO idempotency_records (key, method, fingerprint) \
                 VALUES (?, ?, ?) ON CONFLICT(key) DO NOTHING",
            )
            .bind(&key)
            .bind(&method)
            .bind(&fingerprint)
            .execute(&mut *transaction)
            .await?;
            let row = sqlx::query(
                "SELECT method, fingerprint, result FROM idempotency_records WHERE key = ?",
            )
            .bind(&key)
            .fetch_one(&mut *transaction)
            .await?;
            let stored_method: String = row.try_get("method")?;
            let stored_fingerprint: String = row.try_get("fingerprint")?;
            // A send whose message was cleared keeps its key spent with a tombstone
            // fingerprint. The text it would be compared with is gone, so the stored
            // result is returned, and that result is references only (DPES-1105).
            let cleared = stored_fingerprint == crate::sqlite_history::CLEARED_FINGERPRINT;
            if stored_method != method || (stored_fingerprint != fingerprint && !cleared) {
                return Err(StoreError::IdempotencyConflict);
            }
            let result: Option<String> = row.try_get("result")?;
            let state = match result {
                Some(value) => IdempotencyStart::Completed(serde_json::from_str(&value)?),
                None if inserted.rows_affected() == 1 => IdempotencyStart::Started,
                None => IdempotencyStart::InProgress,
            };
            transaction.commit().await?;
            Ok(state)
        })
    }

    /// Persists the reply for a previously claimed key.
    pub fn complete_idempotency(
        &self,
        key: &str,
        method: &str,
        fingerprint: &str,
        result: &Value,
    ) -> Result<(), StoreError> {
        validate_idempotency(key, method, fingerprint)?;
        let key = key.to_owned();
        let method = method.to_owned();
        let fingerprint = fingerprint.to_owned();
        let result = serde_json::to_string(result)?;
        self.call(move |pool| async move {
            let updated = sqlx::query(
                "UPDATE idempotency_records SET result = ? WHERE key = ? AND method = ? \
                 AND fingerprint = ? AND result IS NULL",
            )
            .bind(&result)
            .bind(&key)
            .bind(&method)
            .bind(&fingerprint)
            .execute(&pool)
            .await?;
            if updated.rows_affected() == 1 {
                return Ok(());
            }
            let stored: Option<String> = sqlx::query_scalar(
                "SELECT result FROM idempotency_records WHERE key = ? AND method = ? AND fingerprint = ?",
            )
            .bind(&key)
            .bind(&method)
            .bind(&fingerprint)
            .fetch_optional(&pool)
            .await?
            .flatten();
            if stored.as_deref() == Some(result.as_str()) {
                Ok(())
            } else {
                Err(StoreError::IdempotencyConflict)
            }
        })
    }
}

pub(crate) async fn append_durable_event(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    task_id: &str,
    session_id: Option<&str>,
    kind: &str,
    payload: &Value,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO durable_events (task_id, session_id, kind, payload) VALUES (?, ?, ?, ?)",
    )
    .bind(task_id)
    .bind(session_id)
    .bind(kind)
    .bind(serde_json::to_string(payload)?)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn validate_page(cursor: i64, limit: usize) -> Result<(), StoreError> {
    if cursor < 0 {
        return Err(StoreError::InvalidEventCursor);
    }
    if limit == 0 || limit > MAX_EVENT_PAGE {
        return Err(StoreError::InvalidEventLimit);
    }
    Ok(())
}

fn validate_idempotency(key: &str, method: &str, fingerprint: &str) -> Result<(), StoreError> {
    if key.is_empty() || key.len() > 128 || method.is_empty() {
        return Err(StoreError::InvalidIdempotencyKey);
    }
    if fingerprint.len() != 64
        || !fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(StoreError::InvalidIdempotencyKey);
    }
    Ok(())
}

fn decode_event(row: sqlx::sqlite::SqliteRow) -> Result<StreamEvent, StoreError> {
    Ok(StreamEvent {
        sequence: row.try_get("sequence")?,
        task_id: row.try_get("task_id")?,
        session_id: row.try_get("session_id")?,
        kind: row.try_get("kind")?,
        payload: serde_json::from_str(&row.try_get::<String, _>("payload")?)?,
        created_at: crate::instant::instant(&row, "created_at")?,
    })
}

/// One task-scoped journal row. Delivery rows carry the approval and candidate they recorded.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TaskDurableEvent {
    /// Global journal cursor: the `durable_events` AUTOINCREMENT position, shared by every task
    /// in the store. A task's results skip the positions of other tasks' rows, so a gap in them
    /// does not mean a row was deleted. This is not the audit envelope `seq`; that is per home,
    /// writer-assigned, and held in `audit_envelope::WriterFields::seq`.
    pub sequence: i64,
    /// Owning task. Non-null in the journal.
    pub task_id: String,
    /// Owning agent session, when the event belongs to one.
    pub session_id: Option<String>,
    /// Stable event kind, for example `session.created` or `approval.approved`.
    pub kind: String,
    /// Payload exactly as recorded. Readers ignore keys they do not know.
    pub payload: Value,
    /// SQLite `CURRENT_TIMESTAMP` at journal write, read as RFC 3339 UTC (`YYYY-MM-DDTHH:MM:SSZ`).
    /// Write time, not the event instant. Projection never uses it as `occurred_at_utc`.
    pub created_at: String,
    /// Lineage joined through `payload.step_id`; `None` for other kinds.
    pub delivery: Option<DeliveryLineage>,
}

/// Approval and candidate identifiers of one recorded delivery step.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeliveryLineage {
    pub approval_id: String,
    pub verification_run_id: String,
    pub candidate_sha: String,
}

impl SqliteStore {
    /// Returns one task's journal rows after `after_sequence`, in journal order.
    ///
    /// This single query correlates dispatch (`attempt.created`, `session.created`), session
    /// (`session.*`), approval (`approval.*`, `payload.approval_id`), and delivery (`delivery.*`,
    /// joined to `delivery_steps` through `payload.step_id`) rows for one task.
    pub fn task_events_after(
        &self,
        task_id: &str,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<TaskDurableEvent>, StoreError> {
        validate_page(after_sequence, limit)?;
        let task_id = task_id.to_owned();
        let limit = i64::try_from(limit).map_err(|_| StoreError::InvalidEventLimit)?;
        self.call(move |pool| async move {
            let rows = sqlx::query(
                "SELECT e.sequence, e.task_id, e.session_id, e.kind, e.payload, e.created_at, d.approval_id, d.verification_run_id, d.candidate_sha FROM durable_events e LEFT JOIN delivery_steps d ON d.task_id = e.task_id AND d.id = json_extract(e.payload, '$.step_id') WHERE e.task_id = ? AND e.sequence > ? ORDER BY e.sequence LIMIT ?",
            )
            .bind(&task_id)
            .bind(after_sequence)
            .bind(limit)
            .fetch_all(&pool)
            .await?;
            rows.into_iter().map(decode_task_event).collect()
        })
    }
}

fn decode_task_event(row: sqlx::sqlite::SqliteRow) -> Result<TaskDurableEvent, StoreError> {
    let approval_id: Option<String> = row.try_get("approval_id")?;
    let delivery = match approval_id {
        Some(approval_id) => Some(DeliveryLineage {
            approval_id,
            verification_run_id: row.try_get("verification_run_id")?,
            candidate_sha: row.try_get("candidate_sha")?,
        }),
        None => None,
    };
    Ok(TaskDurableEvent {
        sequence: row.try_get("sequence")?,
        task_id: row.try_get("task_id")?,
        session_id: row.try_get("session_id")?,
        kind: row.try_get("kind")?,
        payload: serde_json::from_str(&row.try_get::<String, _>("payload")?)?,
        created_at: crate::instant::instant(&row, "created_at")?,
        delivery,
    })
}
