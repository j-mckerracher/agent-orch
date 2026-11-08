//! Append-only SQLite persistence for independent verification evidence.
//!
//! The JSON payload retains every check outcome, command trust decision, and
//! captured command result. Start and finish times remain queryable columns.
//! Duplicate run identifiers fail at the primary key; this module never
//! replaces a prior result. It stores evidence only and opens no referenced
//! worktree or log paths.
//!
//! Updated: 2026-11-07T23:46:24-07:00.

use orchestrator_core::records::VerificationRun;
use serde_json::Value;
use sqlx::Row;

use crate::{SqliteStore, StoreError};

impl SqliteStore {
    /// Appends verifier-owned JSON evidence and timestamps for one run.
    ///
    /// `details` contains the run's individual checks, trust decisions, and
    /// command outputs. The run identifier is unique and is never upserted.
    pub fn record_verification_result(
        &self,
        run: &VerificationRun,
        started_at_ms: i64,
        finished_at_ms: i64,
        details: &Value,
    ) -> Result<(), StoreError> {
        let run = run.clone();
        let details = serde_json::to_string(details)?;
        self.call(move |pool| async move {
            sqlx::query(
                "INSERT INTO verification_runs (id, task_id, \"commit\", status, log_path, result_json, started_at_ms, finished_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(run.id)
            .bind(run.task_id)
            .bind(run.commit)
            .bind(run.status)
            .bind(run.log_path)
            .bind(details)
            .bind(started_at_ms)
            .bind(finished_at_ms)
            .execute(&pool)
            .await?;
            Ok(())
        })
    }

    /// Loads all runs and their retained evidence in identifier order.
    ///
    /// The second tuple item is a JSON envelope with `details`,
    /// `started_at_ms`, and `finished_at_ms`. Legacy rows keep null timestamps.
    pub fn verification_results_for_task(
        &self,
        task_id: &str,
    ) -> Result<Vec<(VerificationRun, Value)>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let rows = sqlx::query(
                "SELECT id, task_id, \"commit\", status, log_path, result_json, started_at_ms, finished_at_ms FROM verification_runs WHERE task_id = ? ORDER BY id",
            )
            .bind(task_id)
            .fetch_all(&pool)
            .await?;
            rows.into_iter()
                .map(|row| {
                    let details: String = row.try_get("result_json")?;
                    let started_at_ms: Option<i64> = row.try_get("started_at_ms")?;
                    let finished_at_ms: Option<i64> = row.try_get("finished_at_ms")?;
                    Ok((
                        VerificationRun {
                            id: row.try_get("id")?,
                            task_id: row.try_get("task_id")?,
                            commit: row.try_get("commit")?,
                            status: row.try_get("status")?,
                            log_path: row.try_get("log_path")?,
                        },
                        serde_json::json!({
                            "details": serde_json::from_str::<Value>(&details)?,
                            "started_at_ms": started_at_ms,
                            "finished_at_ms": finished_at_ms,
                        }),
                    ))
                })
                .collect()
        })
    }
}
