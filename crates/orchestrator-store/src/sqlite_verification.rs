//! Append-only SQLite persistence for independent verification evidence.
//!
//! The JSON payload retains every check outcome, command trust decision, and
//! captured command result. Start and finish times remain queryable columns.
//! Duplicate run identifiers fail at the primary key; this module never
//! replaces a prior result. It stores evidence only and opens no referenced
//! worktree or log paths.
//!
//! [`SqliteStore::settle_verification`] records a run and settles its candidate
//! attempt and task in one transaction. A result that arrives after the task has
//! left `Verifying` is discarded, so a cancellation always wins.
//!
//! Updated: 2026-11-07T23:46:24-07:00.

use orchestrator_core::records::VerificationRun;
use orchestrator_core::task::{AttemptState, SessionState, TaskEvent, TaskState, TransitionError};
use serde_json::Value;
use sqlx::{Row, Sqlite, Transaction};

use crate::sqlite_events::append_durable_event;
use crate::sqlite_ops::{
    apply_task_event_in, bump_task_version, current_task_state, decode, encode, validate_task_id,
};
use crate::{SqliteStore, StoreError};

/// Outcome of [`SqliteStore::settle_verification`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settlement {
    /// The run was recorded, the attempt was settled, and the task left
    /// `Verifying` with this outcome.
    Settled { passed: bool },
    /// The task or attempt was no longer `Verifying` and `CandidateReady`, for
    /// example because it was cancelled while the check ran. Nothing was written.
    Superseded,
}

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
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            insert_verification_run_in(
                &mut transaction,
                &run,
                started_at_ms,
                finished_at_ms,
                &details,
            )
            .await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    /// Records a verification run and settles its candidate in one `BEGIN IMMEDIATE`
    /// transaction. A run whose `status` is `"passed"` succeeds the attempt and
    /// applies a passing outcome. Any other status fails the attempt and stores
    /// `failure_reason` on it.
    ///
    /// Returns [`Settlement::Superseded`] and writes nothing unless the task is
    /// `Verifying` and the attempt is `CandidateReady`.
    #[allow(clippy::too_many_arguments)]
    pub fn settle_verification(
        &self,
        task_id: &str,
        attempt_id: &str,
        run: &VerificationRun,
        started_at_ms: i64,
        finished_at_ms: i64,
        details: &Value,
        failure_reason: Option<&str>,
    ) -> Result<Settlement, StoreError> {
        validate_task_id(task_id)?;
        let task_id = task_id.to_owned();
        let attempt_id = attempt_id.to_owned();
        let run = run.clone();
        let details = serde_json::to_string(details)?;
        let failure_reason = failure_reason.map(str::to_owned);
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            if current_task_state(&mut transaction, &task_id).await? != TaskState::Verifying {
                return Ok(Settlement::Superseded);
            }
            let attempt_state = sqlx::query_scalar::<_, String>(
                "SELECT state FROM attempts WHERE task_id = ? AND id = ?",
            )
            .bind(&task_id)
            .bind(&attempt_id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| StoreError::NoAttempt {
                attempt_id: attempt_id.clone(),
                task_id: task_id.clone(),
            })?;
            if decode::<AttemptState>(&attempt_state)? != AttemptState::CandidateReady {
                return Ok(Settlement::Superseded);
            }

            let passed = run.status == "passed";
            let settled = if passed {
                AttemptState::Succeeded
            } else {
                AttemptState::Failed
            };
            insert_verification_run_in(
                &mut transaction,
                &run,
                started_at_ms,
                finished_at_ms,
                &details,
            )
            .await?;
            sqlx::query(
                "UPDATE attempts SET state = ?, failure_reason = ? WHERE task_id = ? AND id = ?",
            )
            .bind(encode(&settled)?)
            .bind(failure_reason.as_deref())
            .bind(&task_id)
            .bind(&attempt_id)
            .execute(&mut *transaction)
            .await?;
            bump_task_version(&mut transaction, &task_id).await?;
            let payload = serde_json::json!({
                "attempt_id": &attempt_id,
                "state": settled,
                "failure_reason_available": failure_reason.is_some(),
            });
            append_durable_event(
                &mut transaction,
                &task_id,
                None,
                "attempt.state_changed",
                &payload,
            )
            .await?;
            apply_task_event_in(
                &mut transaction,
                &task_id,
                TaskEvent::VerificationOutcome { passed },
            )
            .await?;
            transaction.commit().await?;
            Ok(Settlement::Settled { passed })
        })
    }

    /// Cancels a task whose candidate is awaiting verification. One `BEGIN IMMEDIATE`
    /// transaction requires the task to be `Verifying` with no live agent session,
    /// moves its non-terminal attempts to `Cancelled`, and applies `CancelRequested`.
    ///
    /// Any other task state fails with `IllegalTransition`. A live session fails
    /// with `CancellationConflict`.
    pub fn cancel_verifying_task(&self, task_id: &str) -> Result<(), StoreError> {
        self.cancel_without_agent(task_id, TaskState::Verifying)
    }

    /// Cancels a task whose worker is waiting (DPES-1108 phase 4), the same way as
    /// [`Self::cancel_verifying_task`]: no process runs, so no owner confirms. The waiting
    /// attempt ends `Cancelled`. An open question it raised stays open: only the user closes it.
    pub fn cancel_waiting_task(&self, task_id: &str) -> Result<(), StoreError> {
        self.cancel_without_agent(task_id, TaskState::Waiting)
    }

    fn cancel_without_agent(&self, task_id: &str, expected: TaskState) -> Result<(), StoreError> {
        validate_task_id(task_id)?;
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let state = current_task_state(&mut transaction, &task_id).await?;
            if state != expected {
                return Err(StoreError::IllegalTransition(TransitionError::Illegal {
                    from: state,
                    event: TaskEvent::CancelRequested,
                }));
            }
            let sessions =
                sqlx::query_scalar::<_, String>("SELECT state FROM sessions WHERE task_id = ?")
                    .bind(&task_id)
                    .fetch_all(&mut *transaction)
                    .await?;
            for session_state in sessions {
                if !decode::<SessionState>(&session_state)?.is_terminal() {
                    return Err(StoreError::CancellationConflict {
                        task_id: task_id.clone(),
                        detail: "an agent session is still live",
                    });
                }
            }
            cancel_non_terminal_attempts(&mut transaction, &task_id).await?;
            apply_task_event_in(&mut transaction, &task_id, TaskEvent::CancelRequested).await?;
            transaction.commit().await?;
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

/// Appends one run row inside the caller's transaction.
async fn insert_verification_run_in(
    transaction: &mut Transaction<'_, Sqlite>,
    run: &VerificationRun,
    started_at_ms: i64,
    finished_at_ms: i64,
    details: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO verification_runs (id, task_id, \"commit\", status, log_path, result_json, started_at_ms, finished_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&run.id)
    .bind(&run.task_id)
    .bind(&run.commit)
    .bind(&run.status)
    .bind(run.log_path.as_deref())
    .bind(details)
    .bind(started_at_ms)
    .bind(finished_at_ms)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

/// Moves every non-terminal attempt of the task to `Cancelled`, one event each.
async fn cancel_non_terminal_attempts(
    transaction: &mut Transaction<'_, Sqlite>,
    task_id: &str,
) -> Result<(), StoreError> {
    let rows = sqlx::query("SELECT id, state FROM attempts WHERE task_id = ?")
        .bind(task_id)
        .fetch_all(&mut **transaction)
        .await?;
    for row in rows {
        let attempt_id: String = row.try_get("id")?;
        let current: AttemptState = decode(&row.try_get::<String, _>("state")?)?;
        if current.is_terminal() {
            continue;
        }
        sqlx::query("UPDATE attempts SET state = ? WHERE task_id = ? AND id = ?")
            .bind(encode(&AttemptState::Cancelled)?)
            .bind(task_id)
            .bind(&attempt_id)
            .execute(&mut **transaction)
            .await?;
        bump_task_version(transaction, task_id).await?;
        let payload = serde_json::json!({
            "attempt_id": &attempt_id,
            "state": AttemptState::Cancelled,
        });
        append_durable_event(
            transaction,
            task_id,
            None,
            "attempt.state_changed",
            &payload,
        )
        .await?;
    }
    Ok(())
}
