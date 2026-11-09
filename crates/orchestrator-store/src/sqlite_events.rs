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
            if stored_method != method || stored_fingerprint != fingerprint {
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
        created_at: row.try_get("created_at")?,
    })
}
