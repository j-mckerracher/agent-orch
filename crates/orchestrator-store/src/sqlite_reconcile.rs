//! SQLite persistence and queries used during reconciliation.
//!
//! Process identity is stored separately from the core session lifecycle.
//! Workspace paths remain opaque. CLI/scheduler wiring is deferred to M2-06;
//! partial-delivery inspection is deferred to M3-06.
//!
//! Updated: 2026-11-08T08:14:55+00:00.

use orchestrator_core::identity::{ProcessIdentity, ProcessPlatform};
use orchestrator_core::records::Workspace;
use sqlx::{sqlite::SqliteRow, Row};

use crate::{SqliteStore, StoreError};

impl SqliteStore {
    /// Lists every persisted workspace in stable task and workspace order.
    pub fn list_workspaces(&self) -> Result<Vec<Workspace>, StoreError> {
        self.call(|pool| async move {
            let rows = sqlx::query(
                "SELECT id, task_id, path, branch, base_commit FROM workspaces \
                 ORDER BY task_id, id",
            )
            .fetch_all(&pool)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(Workspace {
                        id: row.try_get("id")?,
                        task_id: row.try_get("task_id")?,
                        path: row.try_get("path")?,
                        branch: row.try_get("branch")?,
                        base_commit: row.try_get("base_commit")?,
                    })
                })
                .collect()
        })
    }
    /// Records a launch-time identity for an existing session.
    ///
    /// Unknown sessions fail with `StoreError::NoSession`. Repeating the same
    /// identity is a no-op; conflicting identities fail and never overwrite.
    pub fn record_session_identity(
        &self,
        task_id: &str,
        session_id: &str,
        identity: &ProcessIdentity,
    ) -> Result<(), StoreError> {
        let task_id = task_id.to_owned();
        let session_id = session_id.to_owned();
        let identity = *identity;
        if identity.pid == 0 || identity.process_group == Some(0) {
            return Err(invalid_identity("PID and process group must be nonzero"));
        }
        let start_time_micros = identity
            .start_time_micros
            .map(i64::try_from)
            .transpose()
            .map_err(|_| invalid_identity("start time exceeds SQLite integer range"))?;
        let platform = identity.platform.tag().to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            let session_exists = sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS (SELECT 1 FROM sessions WHERE task_id = ? AND id = ?)",
            )
            .bind(&task_id)
            .bind(&session_id)
            .fetch_one(&mut *transaction)
            .await?;
            if session_exists == 0 {
                return Err(StoreError::NoSession {
                    task_id,
                    session_id,
                });
            }

            let existing = sqlx::query(
                "SELECT pid, process_group, start_time_micros, platform \
                 FROM session_process_identities WHERE task_id = ? AND session_id = ?",
            )
            .bind(&task_id)
            .bind(&session_id)
            .fetch_optional(&mut *transaction)
            .await?;
            if let Some(row) = existing {
                if decode_identity(row)? == identity {
                    transaction.commit().await?;
                    return Ok(());
                }
                return Err(invalid_identity("session identity already recorded"));
            }

            sqlx::query(
                "INSERT INTO session_process_identities \
                 (task_id, session_id, pid, process_group, start_time_micros, platform) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&task_id)
            .bind(&session_id)
            .bind(i64::from(identity.pid))
            .bind(identity.process_group.map(i64::from))
            .bind(start_time_micros)
            .bind(platform)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    /// Loads a session identity. Returns `None` when no identity was recorded.
    pub fn session_identity(
        &self,
        task_id: &str,
        session_id: &str,
    ) -> Result<Option<ProcessIdentity>, StoreError> {
        let task_id = task_id.to_owned();
        let session_id = session_id.to_owned();
        self.call(move |pool| async move {
            let row = sqlx::query(
                "SELECT pid, process_group, start_time_micros, platform \
                 FROM session_process_identities WHERE task_id = ? AND session_id = ?",
            )
            .bind(task_id)
            .bind(session_id)
            .fetch_optional(&pool)
            .await?;
            row.map(decode_identity).transpose()
        })
    }
}

fn decode_identity(row: SqliteRow) -> Result<ProcessIdentity, StoreError> {
    let pid: i64 = row.try_get("pid")?;
    let process_group: Option<i64> = row.try_get("process_group")?;
    let start_time_micros: Option<i64> = row.try_get("start_time_micros")?;
    let platform: String = row.try_get("platform")?;
    let pid = u32::try_from(pid).map_err(|_| invalid_identity("invalid stored PID"))?;
    let process_group = process_group
        .map(|group| u32::try_from(group).map_err(|_| invalid_identity("invalid process group")))
        .transpose()?;
    if pid == 0 || process_group == Some(0) {
        return Err(invalid_identity(
            "stored PID and process group must be nonzero",
        ));
    }
    Ok(ProcessIdentity {
        pid,
        process_group,
        start_time_micros: start_time_micros
            .map(|time| {
                u64::try_from(time).map_err(|_| invalid_identity("invalid process start time"))
            })
            .transpose()?,
        platform: ProcessPlatform::from_tag(&platform)
            .ok_or_else(|| invalid_identity("invalid stored platform tag"))?,
    })
}
fn invalid_identity(message: &str) -> StoreError {
    StoreError::Database(sqlx::Error::Protocol(message.to_owned()))
}
