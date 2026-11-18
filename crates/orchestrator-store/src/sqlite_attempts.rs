//! Direct SQLite reads for append-only execution attempt history.
//!
//! Attempt state lives in `attempts`, not in the event journal.
//! Updated: 2026-11-08T09:09:22Z.

use orchestrator_core::task::Attempt;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

use crate::{SqliteStore, StoreError};

/// Decodes one `attempts` row. The select must name `id, attempt_number, state, failure_reason,
/// summary, changed_files`. Shared by every reader of attempts so they agree on the columns.
pub(crate) fn attempt_from_row(task_id: &str, row: &SqliteRow) -> Result<Attempt, StoreError> {
    let attempt_number = u32::try_from(row.try_get::<i64, _>("attempt_number")?).map_err(|_| {
        StoreError::Corrupt {
            task_id: task_id.to_owned(),
            detail: "attempt number exceeds u32".to_owned(),
        }
    })?;
    let changed_files = row
        .try_get::<Option<i64>, _>("changed_files")?
        .map(|count| {
            u32::try_from(count).map_err(|_| StoreError::Corrupt {
                task_id: task_id.to_owned(),
                detail: "changed file count exceeds u32".to_owned(),
            })
        })
        .transpose()?;
    let encoded_state: String = row.try_get("state")?;
    Ok(Attempt {
        id: row.try_get("id")?,
        task_id: task_id.to_owned(),
        attempt_number,
        state: serde_json::from_str(&encoded_state)?,
        failure_reason: row.try_get("failure_reason")?,
        summary: row.try_get("summary")?,
        changed_files,
    })
}

impl SqliteStore {
    /// Returns durable attempts ordered by attempt number. Unknown tasks return `NotFound`.
    pub fn attempts_for_task(&self, task_id: &str) -> Result<Vec<Attempt>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let exists =
                sqlx::query_scalar::<_, i64>("SELECT EXISTS (SELECT 1 FROM tasks WHERE id = ?)")
                    .bind(&task_id)
                    .fetch_one(&pool)
                    .await?;
            if exists == 0 {
                return Err(StoreError::NotFound(task_id));
            }

            let rows = sqlx::query(
                "SELECT id, attempt_number, state, failure_reason, summary, changed_files \
                 FROM attempts WHERE task_id = ? ORDER BY attempt_number",
            )
            .bind(&task_id)
            .fetch_all(&pool)
            .await?;
            rows.iter()
                .map(|row| attempt_from_row(&task_id, row))
                .collect()
        })
    }
}
