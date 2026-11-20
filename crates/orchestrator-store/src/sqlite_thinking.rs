//! Agent activity of a task session (DPES-1106 and DPES-1044, table `session_activity`,
//! migrations 0063 and 0074). Rows are `thinking`, `tool_started`, or `tool_finished`.
//!
//! Display data only: nothing reads it back as a command or into a prompt. A session keeps at
//! most [`MAX_THINKING_BYTES_PER_SESSION`] bytes; past that one marker row is written and
//! further text is dropped silently, because thinking must never fail the run that produced it.
//! A chat turn keeps its thinking as `thinking_chunk` entries instead (see
//! `sqlite_conversations`).

use sqlx::Row;

use crate::sqlite_conversations::MAX_TOOL_STARTS;
use crate::{SqliteStore, StoreError};

/// Largest accepted tool id, name, or summary field, in bytes. The daemon bounds them far
/// lower; this keeps a caller's slip from filling the store.
const MAX_TOOL_FIELD_BYTES: usize = 2048;

/// Body of a `tool_started` row; `{"truncated": true}` is the cap marker.
const TOOL_TRUNCATED_BODY: &str = r#"{"truncated":true}"#;

pub(crate) fn tool_started_body(tool_id: &str, name: &str, summary: &str) -> Option<String> {
    (tool_fields_ok(tool_id, summary) && name.len() <= MAX_TOOL_FIELD_BYTES)
        .then(|| serde_json::json!({"id": tool_id, "name": name, "summary": summary}).to_string())
}

pub(crate) fn tool_finished_body(tool_id: &str, ok: bool, summary: &str) -> Option<String> {
    tool_fields_ok(tool_id, summary)
        .then(|| serde_json::json!({"id": tool_id, "ok": ok, "summary": summary}).to_string())
}

fn tool_fields_ok(tool_id: &str, summary: &str) -> bool {
    !tool_id.is_empty()
        && tool_id.len() <= MAX_TOOL_FIELD_BYTES
        && summary.len() <= MAX_TOOL_FIELD_BYTES
}

/// Largest single stored thinking piece, in bytes.
pub const MAX_THINKING_CHUNK_BYTES: usize = 16 * 1024;
/// Largest total thinking kept for one session, in bytes.
pub const MAX_THINKING_BYTES_PER_SESSION: usize = 8 * 1024 * 1024;
/// Largest page [`SqliteStore::session_thinking`] returns.
const MAX_PAGE_CHUNKS: usize = 500;
/// The last thinking row of a session or turn that hit its cap.
pub(crate) const THINKING_TRUNCATED_MARKER: &str = "[thinking truncated: limit reached]";

/// One stored piece of a session's thinking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkingChunk {
    pub sequence: u64,
    pub body: String,
}

/// One bounded read of a session's thinking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionThinkingPage {
    /// Ordered by sequence; all newer than the `after` cursor.
    pub chunks: Vec<ThinkingChunk>,
    /// The last returned sequence, or `after` when nothing was returned.
    pub next_cursor: u64,
    /// The session hit its cap; its last chunk is the truncation marker.
    pub truncated: bool,
}

/// One stored activity row of a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityEntry {
    pub sequence: u64,
    /// `thinking`, `tool_started`, or `tool_finished`.
    pub kind: String,
    pub body: String,
}

/// One bounded read of a session's activity, every kind in sequence order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionActivityPage {
    pub entries: Vec<ActivityEntry>,
    /// The last returned sequence, or `after` when nothing was returned.
    pub next_cursor: u64,
    /// The session hit its thinking or tool cap.
    pub truncated: bool,
}

/// Splits `text` into pieces of at most `max` bytes, cutting only on character boundaries.
/// `max` must be at least 4 so every character fits in a piece.
pub(crate) fn split_chunks(text: &str, max: usize) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let mut end = max.min(rest.len());
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let (piece, tail) = rest.split_at(end);
        pieces.push(piece);
        rest = tail;
    }
    pieces
}

impl SqliteStore {
    /// Append thinking text to a session. Empty text is a no-op. Once the session total would
    /// pass [`MAX_THINKING_BYTES_PER_SESSION`], one marker row is written and later text is
    /// dropped without error.
    pub fn append_session_thinking(&self, session_id: &str, text: &str) -> Result<(), StoreError> {
        if text.is_empty() {
            return Ok(());
        }
        let session_id = session_id.to_owned();
        let pieces: Vec<String> = split_chunks(text, MAX_THINKING_CHUNK_BYTES)
            .into_iter()
            .map(str::to_owned)
            .collect();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            // ponytail: sums the session's rows on every append; a counter column if sessions
            // ever stream enough tiny pieces for this to show up in a profile.
            let (mut total, truncated): (i64, bool) = {
                let row = sqlx::query(
                    "SELECT COALESCE(SUM(length(CAST(body AS BLOB))), 0) AS total, \
                     COALESCE(MAX(body = ?), 0) AS truncated \
                     FROM session_activity \
                     WHERE session_id = ? AND kind = 'thinking'",
                )
                .bind(THINKING_TRUNCATED_MARKER)
                .bind(&session_id)
                .fetch_one(&mut *transaction)
                .await?;
                (
                    row.try_get("total")?,
                    row.try_get::<i64, _>("truncated")? != 0,
                )
            };
            if !truncated {
                for piece in &pieces {
                    let full = total + piece.len() as i64 > MAX_THINKING_BYTES_PER_SESSION as i64;
                    sqlx::query("INSERT INTO session_activity (session_id, kind, body) VALUES (?, 'thinking', ?)")
                        .bind(&session_id)
                        .bind(if full {
                            THINKING_TRUNCATED_MARKER
                        } else {
                            piece.as_str()
                        })
                        .execute(&mut *transaction)
                        .await?;
                    if full {
                        break;
                    }
                    total += piece.len() as i64;
                }
            }
            transaction.commit().await?;
            Ok(())
        })
    }

    /// Read a session's thinking after the `after` cursor: at most `limit` chunks (never more
    /// than 500), oldest first.
    pub fn session_thinking(
        &self,
        session_id: &str,
        after: u64,
        limit: usize,
    ) -> Result<SessionThinkingPage, StoreError> {
        let session_id = session_id.to_owned();
        let after_sequence = i64::try_from(after).unwrap_or(i64::MAX);
        let limit = i64::try_from(limit.min(MAX_PAGE_CHUNKS)).unwrap_or(0);
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            let rows = sqlx::query(
                "SELECT sequence, body FROM session_activity \
                 WHERE session_id = ? AND kind = 'thinking' AND sequence > ? \
                 ORDER BY sequence LIMIT ?",
            )
            .bind(&session_id)
            .bind(after_sequence)
            .bind(limit)
            .fetch_all(&mut *transaction)
            .await?;
            let truncated: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM session_activity \
                 WHERE session_id = ? AND kind = 'thinking' AND body = ?)",
            )
            .bind(&session_id)
            .bind(THINKING_TRUNCATED_MARKER)
            .fetch_one(&mut *transaction)
            .await?;
            let chunks = rows
                .iter()
                .map(|row| {
                    Ok(ThinkingChunk {
                        sequence: u64::try_from(row.try_get::<i64, _>("sequence")?).unwrap_or(0),
                        body: row.try_get("body")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?;
            Ok(SessionThinkingPage {
                next_cursor: chunks.last().map_or(after, |chunk| chunk.sequence),
                chunks,
                truncated,
            })
        })
    }

    /// Append a `tool_started` row. Returns whether it was stored: not when the id was
    /// already started, or after [`MAX_TOOL_STARTS`] (the first start past it stores one
    /// `{"truncated": true}` marker row and returns `true`).
    pub fn append_session_tool_started(
        &self,
        session_id: &str,
        tool_id: &str,
        name: &str,
        summary: &str,
    ) -> Result<bool, StoreError> {
        let Some(body) = tool_started_body(tool_id, name, summary) else {
            return Ok(false);
        };
        let (session_id, tool_id) = (session_id.to_owned(), tool_id.to_owned());
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM session_activity \
                 WHERE session_id = ? AND kind = 'tool_started'",
            )
            .bind(&session_id)
            .fetch_one(&mut *transaction)
            .await?;
            let limit = MAX_TOOL_STARTS as i64;
            let stored = count <= limit
                && (count == limit
                    || !tool_started_exists(&mut transaction, &session_id, &tool_id).await?);
            if stored {
                let body = if count == limit {
                    TOOL_TRUNCATED_BODY
                } else {
                    &body
                };
                sqlx::query(
                    "INSERT INTO session_activity (session_id, kind, body) \
                     VALUES (?, 'tool_started', ?)",
                )
                .bind(&session_id)
                .bind(body)
                .execute(&mut *transaction)
                .await?;
            }
            transaction.commit().await?;
            Ok(stored)
        })
    }

    /// Append a `tool_finished` row. Returns whether it was stored: only when the matching
    /// `tool_started` row exists and has no finish yet.
    pub fn append_session_tool_finished(
        &self,
        session_id: &str,
        tool_id: &str,
        ok: bool,
        summary: &str,
    ) -> Result<bool, StoreError> {
        let Some(body) = tool_finished_body(tool_id, ok, summary) else {
            return Ok(false);
        };
        let (session_id, tool_id) = (session_id.to_owned(), tool_id.to_owned());
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let finished: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM session_activity WHERE session_id = ? \
                 AND kind = 'tool_finished' AND CASE WHEN kind = 'tool_finished' AND json_valid(body) \
                 THEN json_extract(body, '$.id') END = ?)",
            )
            .bind(&session_id)
            .bind(&tool_id)
            .fetch_one(&mut *transaction)
            .await?;
            let stored =
                !finished && tool_started_exists(&mut transaction, &session_id, &tool_id).await?;
            if stored {
                sqlx::query(
                    "INSERT INTO session_activity (session_id, kind, body) \
                     VALUES (?, 'tool_finished', ?)",
                )
                .bind(&session_id)
                .bind(&body)
                .execute(&mut *transaction)
                .await?;
            }
            transaction.commit().await?;
            Ok(stored)
        })
    }

    /// Read a session's activity of every kind after the `after` cursor: at most `limit`
    /// rows (never more than 500), oldest first.
    pub fn session_activity(
        &self,
        session_id: &str,
        after: u64,
        limit: usize,
    ) -> Result<SessionActivityPage, StoreError> {
        let session_id = session_id.to_owned();
        let after_sequence = i64::try_from(after).unwrap_or(i64::MAX);
        let limit = i64::try_from(limit.min(MAX_PAGE_CHUNKS)).unwrap_or(0);
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            let rows = sqlx::query(
                "SELECT sequence, kind, body FROM session_activity \
                 WHERE session_id = ? AND sequence > ? ORDER BY sequence LIMIT ?",
            )
            .bind(&session_id)
            .bind(after_sequence)
            .bind(limit)
            .fetch_all(&mut *transaction)
            .await?;
            let truncated: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM session_activity WHERE session_id = ? \
                 AND ((kind = 'thinking' AND body = ?) OR (kind = 'tool_started' AND body = ?)))",
            )
            .bind(&session_id)
            .bind(THINKING_TRUNCATED_MARKER)
            .bind(TOOL_TRUNCATED_BODY)
            .fetch_one(&mut *transaction)
            .await?;
            let entries = rows
                .iter()
                .map(|row| {
                    Ok(ActivityEntry {
                        sequence: u64::try_from(row.try_get::<i64, _>("sequence")?).unwrap_or(0),
                        kind: row.try_get("kind")?,
                        body: row.try_get("body")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?;
            Ok(SessionActivityPage {
                next_cursor: entries.last().map_or(after, |entry| entry.sequence),
                entries,
                truncated,
            })
        })
    }
}

async fn tool_started_exists(
    connection: &mut sqlx::SqliteConnection,
    session_id: &str,
    tool_id: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM session_activity WHERE session_id = ? \
         AND kind = 'tool_started' AND CASE WHEN kind = 'tool_started' AND json_valid(body) \
         THEN json_extract(body, '$.id') END = ?)",
    )
    .bind(session_id)
    .bind(tool_id)
    .fetch_one(connection)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_chunks_cuts_on_char_boundaries_and_loses_nothing() {
        let text = "é".repeat(10) + "abc";
        let pieces = split_chunks(&text, 5);
        assert!(pieces.iter().all(|piece| piece.len() <= 5));
        assert_eq!(pieces.concat(), text);
        assert!(split_chunks("", 5).is_empty());
    }
}
