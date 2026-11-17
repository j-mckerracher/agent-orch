//! Scout reports (DPES-1105 phase 4a, migration 0044).
//!
//! A finished scout (`Investigate` with `ReportOnly`) leaves one report: the text of the agent's
//! last message. The text is untrusted agent output. The daemon redacts it before it calls
//! [`SqliteStore::capture_task_report`]; this layer stores no path, token or environment value
//! of its own and never puts the text in a prompt. It is data: the daemon decides what a model
//! sees, and only the relay the user asks for hands it to one turn.
//!
//! A report is written once: a second capture changes nothing. Its text and times never change
//! (the migration's triggers refuse an update, also from another connection), `relayed_at` is
//! set once, and a report is never deleted. Clearing conversation history does not touch it.
//! Time is passed in by the caller.

use std::fmt;
use std::io;

use serde_json::json;
use sqlx::{Row, SqliteConnection};

use crate::sqlite_events::append_durable_event;
use crate::{SqliteStore, StoreError};

/// The most bytes of report text the store keeps; a longer text is cut at a character boundary.
pub const MAX_REPORT_BYTES: usize = 16 * 1024;

const COLUMNS: &str = "task_id, attempt_id, body, truncated, captured_at, relayed_at";

/// One stored report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskReport {
    pub task_id: String,
    pub attempt_id: String,
    /// Untrusted agent output.
    pub body: String,
    /// True when the text was cut to fit the cap.
    pub truncated: bool,
    pub captured_at: String,
    pub relayed_at: Option<String>,
}

/// A report without its text, for lists and snapshots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskReportSummary {
    pub task_id: String,
    pub attempt_id: String,
    pub truncated: bool,
    pub captured_at: String,
    pub relayed_at: Option<String>,
}

/// Which reports a list covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportScope<'a> {
    /// Tasks of one project.
    Project(&'a str),
    /// Tasks linked to one workstream.
    Workstream(&'a str),
}

/// Why the store refused a report. Carried inside a `StoreError::Io` of kind `InvalidInput`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskReportError {
    EmptyBody,
    EmptyAttempt,
    EmptyTime,
}

impl fmt::Display for TaskReportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EmptyBody => "a task report needs text",
            Self::EmptyAttempt => "a task report needs the attempt it came from",
            Self::EmptyTime => "a task report needs a time",
        })
    }
}

impl std::error::Error for TaskReportError {}

impl From<TaskReportError> for StoreError {
    fn from(error: TaskReportError) -> Self {
        StoreError::Io(io::Error::new(io::ErrorKind::InvalidInput, error))
    }
}

/// The text cut to [`MAX_REPORT_BYTES`] at a character boundary, and whether it was cut.
fn capped(body: &str) -> (&str, bool) {
    if body.len() <= MAX_REPORT_BYTES {
        return (body, false);
    }
    let mut end = MAX_REPORT_BYTES;
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    (&body[..end], true)
}

impl SqliteStore {
    /// Stores the report of `task_id`, once, and appends the durable event `task.report_ready`
    /// (no text in it) in the same transaction. Returns `true` when it stored the report and
    /// `false` when the task already has one: a second capture never overwrites. Text over
    /// [`MAX_REPORT_BYTES`] is cut at a character boundary and recorded as truncated;
    /// `already_truncated` carries a cut the caller made first. Empty text, an empty attempt or
    /// an empty time is refused with a [`TaskReportError`]; a task that is not stored fails with
    /// `RecordNotFound`.
    pub fn capture_task_report(
        &self,
        task_id: &str,
        attempt_id: &str,
        body: &str,
        already_truncated: bool,
        captured_at: &str,
    ) -> Result<bool, StoreError> {
        if body.is_empty() {
            return Err(TaskReportError::EmptyBody.into());
        }
        if attempt_id.is_empty() {
            return Err(TaskReportError::EmptyAttempt.into());
        }
        if captured_at.is_empty() {
            return Err(TaskReportError::EmptyTime.into());
        }
        let (kept, cut) = capped(body);
        let kept = kept.to_owned();
        let truncated = cut || already_truncated;
        let (task_id, attempt_id, captured_at) = (
            task_id.to_owned(),
            attempt_id.to_owned(),
            captured_at.to_owned(),
        );
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let task = sqlx::query_scalar::<_, i64>("SELECT 1 FROM tasks WHERE id = ?")
                .bind(&task_id)
                .fetch_optional(&mut *transaction)
                .await?;
            if task.is_none() {
                return Err(StoreError::RecordNotFound {
                    entity: "task",
                    id: task_id,
                });
            }
            let added = sqlx::query(
                "INSERT INTO task_reports \
                 (task_id, attempt_id, body, truncated, captured_at) VALUES (?, ?, ?, ?, ?) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(&task_id)
            .bind(&attempt_id)
            .bind(&kept)
            .bind(i64::from(truncated))
            .bind(&captured_at)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
            if added == 0 {
                return Ok(false);
            }
            append_durable_event(
                &mut transaction,
                &task_id,
                None,
                "task.report_ready",
                &json!({ "task_id": task_id }),
            )
            .await?;
            transaction.commit().await?;
            Ok(true)
        })
    }

    /// The report of a task, or `None`. Writes nothing.
    pub fn task_report(&self, task_id: &str) -> Result<Option<TaskReport>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            read_report(&mut connection, &task_id).await
        })
    }

    /// Records that the user's relay handed the report to a turn, once. Returns `true` when it
    /// set `relayed_at` and `false` when the report was already relayed. A task with no report
    /// fails with `RecordNotFound`.
    pub fn mark_task_report_relayed(
        &self,
        task_id: &str,
        relayed_at: &str,
    ) -> Result<bool, StoreError> {
        if relayed_at.is_empty() {
            return Err(TaskReportError::EmptyTime.into());
        }
        let (task_id, relayed_at) = (task_id.to_owned(), relayed_at.to_owned());
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let changed = sqlx::query(
                "UPDATE task_reports SET relayed_at = ? WHERE task_id = ? AND relayed_at IS NULL",
            )
            .bind(&relayed_at)
            .bind(&task_id)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
            if changed == 0 {
                let found =
                    sqlx::query_scalar::<_, i64>("SELECT 1 FROM task_reports WHERE task_id = ?")
                        .bind(&task_id)
                        .fetch_optional(&mut *transaction)
                        .await?;
                if found.is_none() {
                    return Err(StoreError::RecordNotFound {
                        entity: "task report",
                        id: task_id,
                    });
                }
            }
            transaction.commit().await?;
            Ok(changed > 0)
        })
    }

    /// Reports in `scope`, newest capture first and then by task identifier, without their
    /// text. `limit` bounds the rows. Writes nothing.
    pub fn list_task_reports(
        &self,
        scope: ReportScope<'_>,
        limit: usize,
    ) -> Result<Vec<TaskReportSummary>, StoreError> {
        let (sql, id) = match scope {
            ReportScope::Project(id) => (
                "SELECT r.task_id, r.attempt_id, r.truncated, r.captured_at, r.relayed_at \
                 FROM task_reports r JOIN tasks t ON t.id = r.task_id \
                 WHERE t.project_id = ? ORDER BY r.captured_at DESC, r.task_id LIMIT ?",
                id.to_owned(),
            ),
            ReportScope::Workstream(id) => (
                "SELECT r.task_id, r.attempt_id, r.truncated, r.captured_at, r.relayed_at \
                 FROM task_reports r JOIN workstream_tasks w ON w.task_id = r.task_id \
                 WHERE w.workstream_id = ? ORDER BY r.captured_at DESC, r.task_id LIMIT ?",
                id.to_owned(),
            ),
        };
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.call(move |pool| async move {
            let rows = sqlx::query(sql)
                .bind(id)
                .bind(limit)
                .fetch_all(&pool)
                .await?;
            rows.iter()
                .map(|row| {
                    Ok(TaskReportSummary {
                        task_id: row.try_get("task_id")?,
                        attempt_id: row.try_get("attempt_id")?,
                        truncated: row.try_get::<i64, _>("truncated")? != 0,
                        captured_at: row.try_get("captured_at")?,
                        relayed_at: row.try_get("relayed_at")?,
                    })
                })
                .collect()
        })
    }
}

async fn read_report(
    connection: &mut SqliteConnection,
    task_id: &str,
) -> Result<Option<TaskReport>, StoreError> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        // Safe to assert: a constant list of column names.
        "SELECT {COLUMNS} FROM task_reports WHERE task_id = ?"
    )))
    .bind(task_id)
    .fetch_optional(connection)
    .await?;
    row.map(|row| {
        Ok(TaskReport {
            task_id: row.try_get("task_id")?,
            attempt_id: row.try_get("attempt_id")?,
            body: row.try_get("body")?,
            truncated: row.try_get::<i64, _>("truncated")? != 0,
            captured_at: row.try_get("captured_at")?,
            relayed_at: row.try_get("relayed_at")?,
        })
    })
    .transpose()
}
