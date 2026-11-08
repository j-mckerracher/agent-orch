//! Read-only SQLite queries that supply reconciliation snapshots.
//!
//! Workspace paths remain opaque to this store. CLI and scheduler wiring is
//! deferred to M2-06. Partial-delivery inspection is deferred to M3-06.
//!
//! Updated: 2026-11-08T07:40:37+00:00.

use orchestrator_core::records::Workspace;
use sqlx::Row;

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
}
