//! Soft archive of a project (DPES-1105 phase 4d, `projects.archived_at` from migration 0043).
//!
//! `project.remove` never deletes a project: tasks and history reference it. Archiving sets
//! `archived_at`; registering the same path again clears it. Nothing on disk is touched here.
//!
//! The marker is only a mark, so its text comes from the database and nothing reads it for
//! logic other than "set or not".

use std::fmt;
use std::io;

use crate::{SqliteStore, StoreError};

/// Why the store refused to archive a project. Carried inside a `StoreError::Io` of kind
/// `InvalidInput`; [`ArchiveError::of`] reads it back so the daemon can fail the proposal
/// closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveError {
    /// A task of the project is `Queued`, `Running` or `Verifying`.
    ActiveTasks,
    /// An approval of one of the project's tasks is still `requested`.
    PendingApprovals,
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ActiveTasks => "the project has a task that is queued, running or verifying",
            Self::PendingApprovals => "the project has an approval that is still requested",
        })
    }
}

impl std::error::Error for ArchiveError {}

impl From<ArchiveError> for StoreError {
    fn from(error: ArchiveError) -> Self {
        StoreError::Io(io::Error::new(io::ErrorKind::InvalidInput, error))
    }
}

impl ArchiveError {
    /// The refusal inside `error`, if it is one.
    pub fn of(error: &StoreError) -> Option<Self> {
        match error {
            StoreError::Io(cause) if cause.kind() == io::ErrorKind::InvalidInput => cause
                .get_ref()
                .and_then(|inner| inner.downcast_ref::<Self>())
                .copied(),
            _ => None,
        }
    }
}

impl SqliteStore {
    /// Archives a project, in one transaction: refuses with [`ArchiveError`] while any of its
    /// tasks is created, queued, running or verifying or any approval of its tasks is requested; else
    /// sets `archived_at`. Returns `true` when this call archived it and `false` when it was
    /// already archived (nothing written, and no refusal: the call is idempotent). An unknown
    /// project fails with `RecordNotFound`. Rows of tasks, approvals and history stay.
    pub fn archive_project(&self, project_id: &str) -> Result<bool, StoreError> {
        let project_id = project_id.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let archived = sqlx::query_scalar::<_, Option<String>>(
                "SELECT archived_at FROM projects WHERE id = ?",
            )
            .bind(&project_id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| StoreError::RecordNotFound {
                entity: "project",
                id: project_id.clone(),
            })?;
            if archived.is_some() {
                return Ok(false);
            }
            // Task state is stored as its JSON text, so the names carry quotes.
            let active = sqlx::query_scalar::<_, i64>(
                "SELECT 1 FROM tasks WHERE project_id = ? \
                 AND state IN ('\"Created\"', '\"Queued\"', '\"Running\"', '\"Verifying\"') LIMIT 1",
            )
            .bind(&project_id)
            .fetch_optional(&mut *transaction)
            .await?;
            if active.is_some() {
                return Err(StoreError::from(ArchiveError::ActiveTasks));
            }
            let pending = sqlx::query_scalar::<_, i64>(
                "SELECT 1 FROM approvals WHERE status = 'requested' AND task_id IN \
                 (SELECT id FROM tasks WHERE project_id = ?) LIMIT 1",
            )
            .bind(&project_id)
            .fetch_optional(&mut *transaction)
            .await?;
            if pending.is_some() {
                return Err(StoreError::from(ArchiveError::PendingApprovals));
            }
            sqlx::query("UPDATE projects SET archived_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(&project_id)
                .execute(&mut *transaction)
                .await?;
            transaction.commit().await?;
            Ok(true)
        })
    }

    /// Clears the archive marker: the undo, called when the same path is registered again.
    /// Returns `true` when a marker was cleared, `false` when the project was not archived.
    /// An unknown project fails with `RecordNotFound`.
    pub fn restore_project(&self, project_id: &str) -> Result<bool, StoreError> {
        let project_id = project_id.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let cleared = sqlx::query(
                "UPDATE projects SET archived_at = NULL WHERE id = ? AND archived_at IS NOT NULL",
            )
            .bind(&project_id)
            .execute(&mut *transaction)
            .await?
            .rows_affected()
                > 0;
            if !cleared {
                let found = sqlx::query_scalar::<_, i64>("SELECT 1 FROM projects WHERE id = ?")
                    .bind(&project_id)
                    .fetch_optional(&mut *transaction)
                    .await?;
                if found.is_none() {
                    return Err(StoreError::RecordNotFound {
                        entity: "project",
                        id: project_id,
                    });
                }
            }
            transaction.commit().await?;
            Ok(cleared)
        })
    }

    /// The ids of archived projects, in id order. The daemon leaves these out of
    /// `projects.list`, the snapshot and the PR poller, and refuses new tasks for them.
    pub fn archived_project_ids(&self) -> Result<Vec<String>, StoreError> {
        self.call(|pool| async move {
            Ok(sqlx::query_scalar::<_, String>(
                "SELECT id FROM projects WHERE archived_at IS NOT NULL ORDER BY id",
            )
            .fetch_all(&pool)
            .await?)
        })
    }

    /// Whether the project is archived. An unknown project fails with `RecordNotFound`.
    pub fn project_is_archived(&self, project_id: &str) -> Result<bool, StoreError> {
        let project_id = project_id.to_owned();
        self.call(move |pool| async move {
            sqlx::query_scalar::<_, Option<String>>("SELECT archived_at FROM projects WHERE id = ?")
                .bind(&project_id)
                .fetch_optional(&pool)
                .await?
                .map(|archived| archived.is_some())
                .ok_or(StoreError::RecordNotFound {
                    entity: "project",
                    id: project_id,
                })
        })
    }
}
