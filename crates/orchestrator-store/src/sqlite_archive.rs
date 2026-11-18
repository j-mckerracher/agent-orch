//! Soft archive of a project (DPES-1105 phase 4d, `projects.archived_at` from migration 0043).
//!
//! `project.remove` never deletes a project: tasks and history reference it. Archiving sets
//! `archived_at`; registering the same path again clears it. Nothing on disk is touched here.
//!
//! The marker is only a mark, so its text comes from the database and nothing reads it for
//! logic other than "set or not".
//!
//! Finished tasks archive the same way (DPES-1106, migration 0062, `tasks.archived_at`): the row
//! stays, and the daemon leaves it out of the task list and the workstream views.

use std::collections::HashSet;
use std::fmt;
use std::io;

use orchestrator_core::task::TaskState;
use serde_json::json;

use crate::sqlite_events::append_durable_event;
use crate::{SqliteStore, StoreError};

/// Why the store refused an archive. Carried inside a `StoreError::Io` of kind
/// `InvalidInput`; [`ArchiveError::of`] reads it back so the daemon can fail the proposal
/// closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveError {
    /// A task of the project is `Queued`, `Running` or `Verifying`.
    ActiveTasks,
    /// An approval of one of the project's tasks is still `requested`.
    PendingApprovals,
    /// A task named for archiving is not `Succeeded`, `Failed`, `VerificationFailed` or `Cancelled`.
    NotFinished,
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ActiveTasks => "the project has a task that is queued, running or verifying",
            Self::PendingApprovals => "the project has an approval that is still requested",
            Self::NotFinished => "a task is not finished",
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

impl SqliteStore {
    /// Archives finished tasks, in one transaction. A task that is not `Succeeded`, `Failed`,
    /// `VerificationFailed` or `Cancelled` refuses the whole call with [`ArchiveError::NotFinished`],
    /// and an unknown id fails with `RecordNotFound`; either way nothing is written. A task that is
    /// already archived is skipped. Each task this call archives gets one `task.archived` durable
    /// event, in the same transaction. Returns the ids this call archived, in request order.
    pub fn archive_tasks(&self, task_ids: &[String]) -> Result<Vec<String>, StoreError> {
        let task_ids = task_ids.to_vec();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let mut archived = Vec::new();
            for task_id in task_ids {
                let (state, archived_at) = sqlx::query_as::<_, (String, Option<String>)>(
                    "SELECT state, archived_at FROM tasks WHERE id = ?",
                )
                .bind(&task_id)
                .fetch_optional(&mut *transaction)
                .await?
                .ok_or_else(|| StoreError::RecordNotFound {
                    entity: "task",
                    id: task_id.clone(),
                })?;
                if archived_at.is_some() {
                    continue;
                }
                let state: TaskState = serde_json::from_str(&state)?;
                if !matches!(
                    state,
                    TaskState::Succeeded
                        | TaskState::Failed
                        | TaskState::VerificationFailed
                        | TaskState::Cancelled
                ) {
                    return Err(StoreError::from(ArchiveError::NotFinished));
                }
                sqlx::query("UPDATE tasks SET archived_at = CURRENT_TIMESTAMP WHERE id = ?")
                    .bind(&task_id)
                    .execute(&mut *transaction)
                    .await?;
                append_durable_event(
                    &mut transaction,
                    &task_id,
                    None,
                    "task.archived",
                    &json!({}),
                )
                .await?;
                archived.push(task_id);
            }
            transaction.commit().await?;
            Ok(archived)
        })
    }

    /// The ids of archived tasks. The daemon leaves these out of `tasks.list` and the workstream
    /// views; `tasks.get` still answers for them.
    pub fn archived_task_ids(&self) -> Result<HashSet<String>, StoreError> {
        self.call(|pool| async move {
            Ok(sqlx::query_scalar::<_, String>(
                "SELECT id FROM tasks WHERE archived_at IS NOT NULL",
            )
            .fetch_all(&pool)
            .await?
            .into_iter()
            .collect())
        })
    }
}
