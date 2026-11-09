//! SQLite persistence operations for orchestrator domain records.
//!
//! Task transitions use the core state table and commit state plus audit event
//! in one transaction. Other records are append-only. Updated: 2026-11-08T18:09:45+00:00.

use orchestrator_core::authorization::Approval;
use orchestrator_core::contract::{validate as validate_contract, TaskContract};
use orchestrator_core::identity::{ExactProcessIdentity, ProcessIdentity};
use orchestrator_core::records::{Artifact, Project, VerificationRun, Workspace};
use orchestrator_core::task::{
    attempt_transition, session_transition, transition as task_transition, Attempt, AttemptState,
    AuditEvent, Session, SessionState, TaskEvent, TaskState,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use sqlx::sqlite::Sqlite;
use sqlx::{Row, Transaction};

use crate::sqlite_events::append_durable_event;
use crate::{SqliteStore, Store, StoreError, TaskRecord};

/// One task event in the database-wide append-only journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventJournalEntry {
    /// Monotonic database-wide sequence number.
    pub sequence: i64,
    /// Task that produced this event.
    pub task_id: String,
    /// Core audit event for the accepted task transition.
    pub event: AuditEvent,
    /// UTC timestamp assigned by SQLite.
    pub created_at: String,
}

/// Identity of the process that owns a running session's [`AgentHandle`].
///
/// `generation` is a fresh UUID for each run. It is not a PID: requests are
/// bound to this generation and the exact task/attempt/session, then consumed
/// only by the owner that holds the live handle.
#[derive(Clone, PartialEq, Eq)]
pub struct CancellationOwner {
    pub task_id: String,
    pub attempt_id: String,
    pub session_id: String,
    generation: String,
    process_identity: Option<ExactProcessIdentity>,
}

impl CancellationOwner {
    /// Creates an owner capability for one run generation.
    pub fn new(
        task_id: String,
        attempt_id: String,
        session_id: String,
        generation: String,
    ) -> Result<Self, StoreError> {
        let owner = Self {
            task_id,
            attempt_id,
            session_id,
            generation,
            process_identity: None,
        };
        validate_cancellation_owner(&owner)?;
        Ok(owner)
    }

    /// Associates the exact process identity that owns the live agent handle.
    pub fn with_process_identity(mut self, identity: ProcessIdentity) -> Self {
        self.process_identity = Some(ExactProcessIdentity {
            identity,
            windows_creation_time_100ns_since_unix_epoch: None,
        });
        self
    }

    /// Associates an exact process identity for cancellation-owner recovery.
    pub fn with_exact_process_identity(mut self, identity: ExactProcessIdentity) -> Self {
        self.process_identity = Some(identity);
        self
    }

    /// Returns the recorded identity of the process that owns the live handle.
    pub fn process_identity(&self) -> Option<ProcessIdentity> {
        self.process_identity.map(|exact| exact.identity)
    }

    /// Returns the exact identity used to reconcile this cancellation owner.
    pub fn exact_process_identity(&self) -> Option<ExactProcessIdentity> {
        self.process_identity
    }

    /// Returns the opaque generation token used to identify this exact owner.
    pub fn generation(&self) -> &str {
        &self.generation
    }
}

/// Durable result of a cross-process cancellation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationStatus {
    /// The request exists, but the owner has not acknowledged termination.
    Pending,
    /// The owner waited for its agent handle and confirmed cancellation.
    Confirmed,
    /// The agent exited without being cancelled.
    Rejected,
    /// The owner could not confirm cancellation.
    Failed,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct CancellationReceipt {
    task_id: String,
    attempt_id: String,
    session_id: String,
    generation: String,
    status: CancellationStatus,
}

impl SqliteStore {
    /// Registers a project. Duplicate identifiers return a typed database error.
    pub fn register_project(&self, project: &Project) -> Result<(), StoreError> {
        let project = project.clone();
        self.call(move |pool| async move {
            let settings = serde_json::to_string(&project.settings)?;
            sqlx::query("INSERT INTO projects (id, root_path, settings) VALUES (?, ?, ?)")
                .bind(project.id)
                .bind(project.root_path)
                .bind(settings)
                .execute(&pool)
                .await?;
            Ok(())
        })
    }

    /// Loads one registered project.
    pub fn get_project(&self, project_id: &str) -> Result<Project, StoreError> {
        let project_id = project_id.to_owned();
        self.call(move |pool| async move {
            let row = sqlx::query("SELECT id, root_path, settings FROM projects WHERE id = ?")
                .bind(&project_id)
                .fetch_optional(&pool)
                .await?
                .ok_or_else(|| StoreError::RecordNotFound {
                    entity: "project",
                    id: project_id,
                })?;
            Ok(Project {
                id: row.try_get("id")?,
                root_path: row.try_get("root_path")?,
                settings: decode(&row.try_get::<String, _>("settings")?)?,
            })
        })
    }

    /// Lists registered projects in identifier order.
    pub fn list_projects(&self) -> Result<Vec<Project>, StoreError> {
        self.call(|pool| async move {
            let rows = sqlx::query("SELECT id, root_path, settings FROM projects ORDER BY id")
                .fetch_all(&pool)
                .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(Project {
                        id: row.try_get("id")?,
                        root_path: row.try_get("root_path")?,
                        settings: decode(&row.try_get::<String, _>("settings")?)?,
                    })
                })
                .collect()
        })
    }

    /// Appends a workspace record and a path-free event for its owning task.
    pub fn add_workspace(&self, workspace: &Workspace) -> Result<(), StoreError> {
        let workspace = workspace.clone();
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            sqlx::query(
                "INSERT INTO workspaces (id, task_id, path, branch, base_commit) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&workspace.id)
            .bind(&workspace.task_id)
            .bind(&workspace.path)
            .bind(&workspace.branch)
            .bind(&workspace.base_commit)
            .execute(&mut *transaction)
            .await?;
            let payload = serde_json::json!({"workspace_id": workspace.id});
            append_durable_event(
                &mut transaction,
                &workspace.task_id,
                None,
                "workspace.added",
                &payload,
            )
            .await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    /// Lists a task's workspaces in identifier order.
    pub fn workspaces_for_task(&self, task_id: &str) -> Result<Vec<Workspace>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let rows = sqlx::query(
                "SELECT id, task_id, path, branch, base_commit FROM workspaces WHERE task_id = ? ORDER BY id",
            )
            .bind(task_id)
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

    /// Appends an approval record for its bound task.
    pub fn add_approval(&self, approval: &Approval) -> Result<(), StoreError> {
        let approval = approval.clone();
        self.call(move |pool| async move {
            sqlx::query(
                "INSERT INTO approvals (id, task_id, action, status, granted_by) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(approval.id)
            .bind(&approval.action.task_id)
            .bind(serde_json::to_string(&approval.action)?)
            .bind(serde_json::to_string(&approval.state)?)
            .bind(approval.granted_by)
            .execute(&pool)
            .await?;
            Ok(())
        })
    }

    /// Lists approvals bound to a task in identifier order.
    pub fn approvals_for_task(&self, task_id: &str) -> Result<Vec<Approval>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let rows = sqlx::query(
                "SELECT id, action, status, granted_by FROM approvals WHERE task_id = ? ORDER BY id",
            )
            .bind(task_id)
            .fetch_all(&pool)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(Approval {
                        id: row.try_get("id")?,
                        action: decode(&row.try_get::<String, _>("action")?)?,
                        state: decode(&row.try_get::<String, _>("status")?)?,
                        granted_by: row.try_get("granted_by")?,
                    })
                })
                .collect()
        })
    }

    /// Appends a verification-run record for an existing task.
    pub fn add_verification_run(&self, run: &VerificationRun) -> Result<(), StoreError> {
        let run = run.clone();
        self.call(move |pool| async move {
            sqlx::query(
                "INSERT INTO verification_runs (id, task_id, \"commit\", status, log_path) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(run.id)
            .bind(run.task_id)
            .bind(run.commit)
            .bind(run.status)
            .bind(run.log_path)
            .execute(&pool)
            .await?;
            Ok(())
        })
    }

    /// Lists verification runs for a task in identifier order.
    pub fn verification_runs_for_task(
        &self,
        task_id: &str,
    ) -> Result<Vec<VerificationRun>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let rows = sqlx::query(
                "SELECT id, task_id, \"commit\", status, log_path FROM verification_runs WHERE task_id = ? ORDER BY id",
            )
            .bind(task_id)
            .fetch_all(&pool)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(VerificationRun {
                        id: row.try_get("id")?,
                        task_id: row.try_get("task_id")?,
                        commit: row.try_get("commit")?,
                        status: row.try_get("status")?,
                        log_path: row.try_get("log_path")?,
                    })
                })
                .collect()
        })
    }

    /// Appends an artifact reference. Artifact contents stay outside SQLite.
    pub fn add_artifact(&self, artifact: &Artifact) -> Result<(), StoreError> {
        let artifact = artifact.clone();
        self.call(move |pool| async move {
            sqlx::query(
                "INSERT INTO artifacts (id, task_id, type, path, checksum) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(artifact.id)
            .bind(artifact.task_id)
            .bind(artifact.kind)
            .bind(artifact.path)
            .bind(artifact.checksum)
            .execute(&pool)
            .await?;
            Ok(())
        })
    }

    /// Lists artifact references for a task in identifier order.
    pub fn artifacts_for_task(&self, task_id: &str) -> Result<Vec<Artifact>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let rows = sqlx::query(
                "SELECT id, task_id, type, path, checksum FROM artifacts WHERE task_id = ? ORDER BY id",
            )
            .bind(task_id)
            .fetch_all(&pool)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(Artifact {
                        id: row.try_get("id")?,
                        task_id: row.try_get("task_id")?,
                        kind: row.try_get("type")?,
                        path: row.try_get("path")?,
                        checksum: row.try_get("checksum")?,
                    })
                })
                .collect()
        })
    }

    /// Reads journal entries with sequence greater than `after_sequence`.
    pub fn event_journal(&self, after_sequence: i64) -> Result<Vec<EventJournalEntry>, StoreError> {
        self.call(move |pool| async move {
            let rows = sqlx::query(
                "SELECT seq, task_id, payload, created_at FROM events WHERE seq > ? ORDER BY seq",
            )
            .bind(after_sequence)
            .fetch_all(&pool)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(EventJournalEntry {
                        sequence: row.try_get("seq")?,
                        task_id: row.try_get("task_id")?,
                        event: decode(&row.try_get::<String, _>("payload")?)?,
                        created_at: row.try_get("created_at")?,
                    })
                })
                .collect()
        })
    }
}

impl SqliteStore {
    /// Lists open owner generations, including generations whose task is no
    /// longer running. Reconciliation uses these records to retire only owners
    /// whose exact process identity has been observed as exited.
    pub fn open_cancellation_owners(&self) -> Result<Vec<CancellationOwner>, StoreError> {
        self.call(|pool| async move {
            let mut transaction = pool.begin().await?;
            let owners = read_open_cancellation_owners(&mut transaction, None).await?;
            transaction.commit().await?;
            Ok(owners)
        })
    }

    /// Finds the unique active owner for a running task. A stale owner row is
    /// never surfaced for a task whose durable state is no longer `Running`.
    fn active_cancellation_owner(
        &self,
        task_id: &str,
    ) -> Result<Option<CancellationOwner>, StoreError> {
        validate_task_id(task_id)?;
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            if current_task_state(&mut transaction, &task_id).await? != TaskState::Running {
                transaction.commit().await?;
                return Ok(None);
            }
            let mut owners =
                read_open_cancellation_owners(&mut transaction, Some(&task_id)).await?;
            if owners.len() > 1 {
                return Err(StoreError::CancellationConflict {
                    task_id,
                    detail: "multiple active owners are registered",
                });
            }
            let owner = owners.pop();
            if let Some(owner) = &owner {
                require_running_owner_records(&mut transaction, owner).await?;
            }
            transaction.commit().await?;
            Ok(owner)
        })
    }

    /// Creates or reuses the cancellation request for a task without
    /// disclosing the run owner's generation capability to the caller.
    pub fn request_cancellation_for_task(
        &self,
        task_id: &str,
    ) -> Result<CancellationStatus, StoreError> {
        validate_task_id(task_id)?;
        let Some(owner) = self.active_cancellation_owner(task_id)? else {
            let session_id = self
                .get(task_id)?
                .sessions
                .into_iter()
                .rev()
                .find(|session| !session.state.is_terminal())
                .map(|session| session.id)
                .unwrap_or_else(|| "unknown".to_string());
            return Err(StoreError::NoActiveCancellationOwner {
                task_id: task_id.to_owned(),
                session_id,
            });
        };
        self.request_cancellation(&owner)
    }

    /// Reads the active task owner's request state without exposing its
    /// generation capability.
    pub fn cancellation_status_for_task(
        &self,
        task_id: &str,
    ) -> Result<Option<CancellationStatus>, StoreError> {
        validate_task_id(task_id)?;
        let task = self.get(task_id)?;
        if task.state == TaskState::Running {
            if let Some(owner) = self.active_cancellation_owner(task_id)? {
                if let Some(status) = self.cancellation_request(&owner)? {
                    return Ok(Some(status));
                }
            }
        }
        let Some(session) = task.sessions.last() else {
            return Ok(None);
        };
        let task_id = task_id.to_owned();
        let attempt_id = session.attempt_id.clone();
        let session_id = session.id.clone();
        let method = format!("cli.cancel.request:{task_id}:{attempt_id}:{session_id}");
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            let rows = sqlx::query(
                "SELECT key, fingerprint, result FROM idempotency_records WHERE method = ?",
            )
            .bind(&method)
            .fetch_all(&mut *transaction)
            .await?;
            if rows.is_empty() {
                transaction.commit().await?;
                return Ok(None);
            }
            if rows.len() != 1 {
                return Err(StoreError::CancellationConflict {
                    task_id,
                    detail: "multiple cancellation generations exist for one session",
                });
            }
            let row = &rows[0];
            let key: String = row.try_get("key")?;
            let fingerprint: String = row.try_get("fingerprint")?;
            let result: Option<String> = row.try_get("result")?;
            let Some(generation) = key.strip_prefix("agent-orch-cancel-") else {
                return Err(StoreError::IdempotencyConflict);
            };
            let owner = CancellationOwner::new(
                task_id.clone(),
                attempt_id,
                session_id,
                generation.to_owned(),
            )?;
            let expected_fingerprint = cancellation_fingerprint(&owner);
            verify_cancellation_record(
                &owner,
                &method,
                &expected_fingerprint,
                &method,
                &fingerprint,
            )?;
            let status = cancellation_status(&owner, result.as_deref())?;
            if status != CancellationStatus::Pending {
                let owner_key = cancellation_owner_key(&owner);
                let owner_method = cancellation_owner_method(&owner);
                let Some((stored_method, stored_fingerprint, owner_result)) =
                    read_idempotency_record(&mut transaction, &owner_key).await?
                else {
                    return Err(StoreError::NoActiveCancellationOwner {
                        task_id,
                        session_id: owner.session_id,
                    });
                };
                verify_cancellation_record(
                    &owner,
                    &owner_method,
                    &expected_fingerprint,
                    &stored_method,
                    &stored_fingerprint,
                )?;
                if owner_result.as_deref()
                    != Some(serde_json::json!({"status": "closed"}).to_string().as_str())
                {
                    return Err(StoreError::CancellationConflict {
                        task_id: owner.task_id,
                        detail: "terminal request has no closed owner record",
                    });
                }
            }
            transaction.commit().await?;
            Ok(Some(status))
        })
    }

    /// Registers the current run as owner of a live agent session.
    ///
    /// This uses the existing SQLite idempotency table as a durable local
    /// mailbox. The owner row is namespaced separately from request rows and
    /// stays pending while the run process owns the handle. Process identity
    /// is recorded only to reconcile a stale owner, never as cancellation
    /// authority.
    pub fn register_cancellation_owner(&self, owner: &CancellationOwner) -> Result<(), StoreError> {
        validate_cancellation_owner(owner)?;
        let owner = owner.clone();
        let key = cancellation_owner_key(&owner);
        let method = cancellation_owner_method(&owner);
        let fingerprint = cancellation_fingerprint(&owner);
        let process_identity = owner.process_identity;
        let process_key = cancellation_owner_process_key(&owner);
        let process_method = cancellation_owner_process_method(&owner);
        let process_result = process_identity
            .map(|identity| encode(&identity))
            .transpose()?;
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            require_running_owner_records(&mut transaction, &owner).await?;
            if !read_open_cancellation_owners(&mut transaction, Some(&owner.task_id))
                .await?
                .is_empty()
            {
                return Err(StoreError::CancellationConflict {
                    task_id: owner.task_id,
                    detail: "another cancellation owner generation is still open",
                });
            }
            let inserted = sqlx::query(
                "INSERT INTO idempotency_records (key, method, fingerprint) \
                 VALUES (?, ?, ?) ON CONFLICT(key) DO NOTHING",
            )
            .bind(&key)
            .bind(&method)
            .bind(&fingerprint)
            .execute(&mut *transaction)
            .await?;
            if inserted.rows_affected() != 1 {
                return Err(StoreError::CancellationConflict {
                    task_id: owner.task_id,
                    detail: "owner generation is already registered",
                });
            }
            if let Some(process_result) = process_result {
                let inserted = sqlx::query(
                    "INSERT INTO idempotency_records (key, method, fingerprint, result) \
                     VALUES (?, ?, ?, ?) ON CONFLICT(key) DO NOTHING",
                )
                .bind(&process_key)
                .bind(&process_method)
                .bind(&fingerprint)
                .bind(process_result)
                .execute(&mut *transaction)
                .await?;
                if inserted.rows_affected() != 1 {
                    return Err(StoreError::CancellationConflict {
                        task_id: owner.task_id,
                        detail: "owner process identity is already registered",
                    });
                }
            }
            transaction.commit().await?;
            Ok(())
        })
    }

    /// Resolves a pending request and closes exactly the observed stale
    /// generation. This does not change task, attempt, session, or audit state.
    pub fn retire_stale_cancellation_owner(
        &self,
        owner: &CancellationOwner,
    ) -> Result<(), StoreError> {
        validate_cancellation_owner(owner)?;
        let Some(process_identity) = owner.exact_process_identity() else {
            return Err(StoreError::CancellationConflict {
                task_id: owner.task_id.clone(),
                detail: "owner process identity is unavailable",
            });
        };
        let owner = owner.clone();
        let owner_key = cancellation_owner_key(&owner);
        let owner_method = cancellation_owner_method(&owner);
        let request_key = cancellation_request_key(&owner);
        let request_method = cancellation_request_method(&owner);
        let fingerprint = cancellation_fingerprint(&owner);
        let closed_result = serde_json::json!({"status": "closed"}).to_string();
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            sqlx::query(
                "UPDATE idempotency_records SET result = result WHERE key = ? \
                 AND method = ? AND fingerprint = ?",
            )
            .bind(&owner_key)
            .bind(&owner_method)
            .bind(&fingerprint)
            .execute(&mut *transaction)
            .await?;
            let Some((stored_method, stored_fingerprint, owner_result)) =
                read_idempotency_record(&mut transaction, &owner_key).await?
            else {
                return Err(StoreError::NoActiveCancellationOwner {
                    task_id: owner.task_id,
                    session_id: owner.session_id,
                });
            };
            verify_cancellation_record(
                &owner,
                &owner_method,
                &fingerprint,
                &stored_method,
                &stored_fingerprint,
            )?;
            if owner_result.as_deref() == Some(closed_result.as_str()) {
                transaction.commit().await?;
                return Ok(());
            }
            if owner_result.is_some() {
                return Err(StoreError::CancellationConflict {
                    task_id: owner.task_id,
                    detail: "owner registration already has a result",
                });
            }
            if read_cancellation_owner_process_identity(&mut transaction, &owner).await?
                != Some(process_identity)
            {
                return Err(StoreError::CancellationConflict {
                    task_id: owner.task_id,
                    detail: "owner process identity does not match the recorded generation",
                });
            }
            if current_task_state(&mut transaction, &owner.task_id).await? == TaskState::Cancelled {
                return Err(StoreError::CancellationConflict {
                    task_id: owner.task_id,
                    detail: "task is already Cancelled",
                });
            }
            write_cancellation_receipt(
                &mut transaction,
                &owner,
                &request_key,
                &request_method,
                &fingerprint,
                CancellationStatus::Failed,
            )
            .await?;
            close_cancellation_owner(
                &mut transaction,
                &owner,
                &owner_key,
                &owner_method,
                &fingerprint,
            )
            .await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    /// Manually closes one owner generation after an operator confirms that
    /// its process tree has exited. The target attempt and session must
    /// already be terminal; this never changes task lifecycle or marks a task
    /// cancelled.
    pub fn manually_resolve_cancellation_owner(
        &self,
        owner: &CancellationOwner,
        processes_confirmed_exited: bool,
        reason: &str,
    ) -> Result<(), StoreError> {
        validate_cancellation_owner(owner)?;
        let reason = reason.trim().to_owned();
        if !processes_confirmed_exited || reason.is_empty() || reason.len() > 1024 {
            return Err(StoreError::CancellationConflict {
                task_id: owner.task_id.clone(),
                detail: "manual recovery requires process-exit confirmation and a reason",
            });
        }

        let owner = owner.clone();
        let owner_key = cancellation_owner_key(&owner);
        let owner_method = cancellation_owner_method(&owner);
        let request_key = cancellation_request_key(&owner);
        let request_method = cancellation_request_method(&owner);
        let fingerprint = cancellation_fingerprint(&owner);
        let closed_result = serde_json::json!({"status": "closed"}).to_string();
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            sqlx::query(
                "UPDATE idempotency_records SET result = result WHERE key = ? \
                 AND method = ? AND fingerprint = ?",
            )
            .bind(&owner_key)
            .bind(&owner_method)
            .bind(&fingerprint)
            .execute(&mut *transaction)
            .await?;
            let Some((stored_method, stored_fingerprint, owner_result)) =
                read_idempotency_record(&mut transaction, &owner_key).await?
            else {
                return Err(StoreError::NoActiveCancellationOwner {
                    task_id: owner.task_id,
                    session_id: owner.session_id,
                });
            };
            verify_cancellation_record(
                &owner,
                &owner_method,
                &fingerprint,
                &stored_method,
                &stored_fingerprint,
            )?;
            if owner_result.as_deref() == Some(closed_result.as_str()) {
                transaction.commit().await?;
                return Ok(());
            }
            if owner_result.is_some() {
                return Err(StoreError::CancellationConflict {
                    task_id: owner.task_id,
                    detail: "owner registration already has a result",
                });
            }
            if current_task_state(&mut transaction, &owner.task_id).await? == TaskState::Cancelled {
                return Err(StoreError::CancellationConflict {
                    task_id: owner.task_id,
                    detail: "task is already Cancelled",
                });
            }
            if !attempt_state_in(&mut transaction, &owner)
                .await?
                .is_terminal()
                || !session_state_in(&mut transaction, &owner)
                    .await?
                    .is_terminal()
            {
                return Err(StoreError::CancellationConflict {
                    task_id: owner.task_id,
                    detail: "manual recovery requires a terminal attempt and session",
                });
            }

            let mut request_status = None;
            if let Some((method, stored_fingerprint, result)) =
                read_idempotency_record(&mut transaction, &request_key).await?
            {
                verify_cancellation_record(
                    &owner,
                    &request_method,
                    &fingerprint,
                    &method,
                    &stored_fingerprint,
                )?;
                match cancellation_status(&owner, result.as_deref())? {
                    CancellationStatus::Pending => {
                        write_cancellation_receipt(
                            &mut transaction,
                            &owner,
                            &request_key,
                            &request_method,
                            &fingerprint,
                            CancellationStatus::Failed,
                        )
                        .await?;
                        request_status = Some(CancellationStatus::Failed);
                    }
                    CancellationStatus::Confirmed => {
                        return Err(StoreError::CancellationConflict {
                            task_id: owner.task_id,
                            detail: "confirmed cancellation cannot use manual recovery",
                        });
                    }
                    status @ (CancellationStatus::Rejected | CancellationStatus::Failed) => {
                        request_status = Some(status);
                    }
                }
            }

            append_durable_event(
                &mut transaction,
                &owner.task_id,
                Some(&owner.session_id),
                "cancellation.owner_manually_released",
                &serde_json::json!({
                    "attempt_id": owner.attempt_id,
                    "session_id": owner.session_id,
                    "generation": owner.generation,
                    "reason": reason,
                    "process_exit_confirmed_by_operator": true,
                    "request_status": request_status,
                }),
            )
            .await?;
            close_cancellation_owner(
                &mut transaction,
                &owner,
                &owner_key,
                &owner_method,
                &fingerprint,
            )
            .await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    /// Creates or reuses the one cancellation request bound to this owner.
    ///
    /// The task/session checks and request insertion share one transaction.
    /// Concurrent cancel clients therefore receive the same pending or
    /// completed result instead of creating independent kill attempts.
    fn request_cancellation(
        &self,
        owner: &CancellationOwner,
    ) -> Result<CancellationStatus, StoreError> {
        validate_cancellation_owner(owner)?;
        let owner = owner.clone();
        let owner_key = cancellation_owner_key(&owner);
        let request_key = cancellation_request_key(&owner);
        let owner_method = cancellation_owner_method(&owner);
        let request_method = cancellation_request_method(&owner);
        let fingerprint = cancellation_fingerprint(&owner);
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            // Take SQLite's writer reservation before reading. In WAL mode,
            // two deferred read transactions that both upgrade to a write
            // can fail with SQLITE_BUSY_SNAPSHOT instead of serializing.
            sqlx::query(
                "INSERT INTO idempotency_records (key, method, fingerprint) \
                 VALUES (?, ?, ?) ON CONFLICT(key) DO NOTHING",
            )
            .bind(&request_key)
            .bind(&request_method)
            .bind(&fingerprint)
            .execute(&mut *transaction)
            .await?;
            let Some((stored_request_method, stored_request_fingerprint, result)) =
                read_idempotency_record(&mut transaction, &request_key).await?
            else {
                return Err(StoreError::CancellationConflict {
                    task_id: owner.task_id,
                    detail: "request was not persisted",
                });
            };
            verify_cancellation_record(
                &owner,
                &request_method,
                &fingerprint,
                &stored_request_method,
                &stored_request_fingerprint,
            )?;
            let status = cancellation_status(&owner, result.as_deref())?;
            if status != CancellationStatus::Pending {
                transaction.commit().await?;
                return Ok(status);
            }

            let Some((stored_method, stored_fingerprint, owner_result)) =
                read_idempotency_record(&mut transaction, &owner_key).await?
            else {
                return Err(StoreError::NoActiveCancellationOwner {
                    task_id: owner.task_id,
                    session_id: owner.session_id,
                });
            };
            verify_cancellation_record(
                &owner,
                &owner_method,
                &fingerprint,
                &stored_method,
                &stored_fingerprint,
            )?;
            if owner_result.is_some() {
                return Err(StoreError::NoActiveCancellationOwner {
                    task_id: owner.task_id,
                    session_id: owner.session_id,
                });
            }
            require_running_owner_records(&mut transaction, &owner).await?;
            transaction.commit().await?;
            Ok(status)
        })
    }

    /// Returns the request result without creating a request.
    pub fn cancellation_request(
        &self,
        owner: &CancellationOwner,
    ) -> Result<Option<CancellationStatus>, StoreError> {
        validate_cancellation_owner(owner)?;
        let owner = owner.clone();
        let key = cancellation_request_key(&owner);
        let method = cancellation_request_method(&owner);
        let fingerprint = cancellation_fingerprint(&owner);
        self.call(move |pool| async move {
            let Some((stored_method, stored_fingerprint, result)) = sqlx::query(
                "SELECT method, fingerprint, result FROM idempotency_records WHERE key = ?",
            )
            .bind(&key)
            .fetch_optional(&pool)
            .await?
            .map(|row| {
                Ok::<_, StoreError>((
                    row.try_get::<String, _>("method")?,
                    row.try_get::<String, _>("fingerprint")?,
                    row.try_get::<Option<String>, _>("result")?,
                ))
            })
            .transpose()?
            else {
                return Ok(None);
            };
            verify_cancellation_record(
                &owner,
                &method,
                &fingerprint,
                &stored_method,
                &stored_fingerprint,
            )?;
            Ok(Some(cancellation_status(&owner, result.as_deref())?))
        })
    }

    /// Atomically records confirmed cancellation after the owner has waited
    /// for the agent handle to report its terminal outcome.
    ///
    /// The task/session/attempt transitions, durable acknowledgement, and
    /// owner closure commit together. A request is never acknowledged by the
    /// separate CLI process or by a PID observation.
    pub fn confirm_cancellation(&self, owner: &CancellationOwner) -> Result<(), StoreError> {
        validate_cancellation_owner(owner)?;
        let owner = owner.clone();
        let owner_key = cancellation_owner_key(&owner);
        let owner_method = cancellation_owner_method(&owner);
        let request_key = cancellation_request_key(&owner);
        let request_method = cancellation_request_method(&owner);
        let fingerprint = cancellation_fingerprint(&owner);
        let closed_result = serde_json::json!({"status": "closed"}).to_string();
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            // Reserve the writer before reading state so duplicate requests
            // cannot invalidate this transaction's WAL snapshot.
            sqlx::query(
                "UPDATE idempotency_records SET result = result WHERE key = ? \
                 AND method = ? AND fingerprint = ?",
            )
            .bind(&owner_key)
            .bind(&owner_method)
            .bind(&fingerprint)
            .execute(&mut *transaction)
            .await?;
            let Some((stored_method, stored_fingerprint, owner_result)) =
                read_idempotency_record(&mut transaction, &owner_key).await?
            else {
                return Err(StoreError::NoActiveCancellationOwner {
                    task_id: owner.task_id,
                    session_id: owner.session_id,
                });
            };
            verify_cancellation_record(
                &owner,
                &owner_method,
                &fingerprint,
                &stored_method,
                &stored_fingerprint,
            )?;

            let current = current_task_state(&mut transaction, &owner.task_id).await?;
            if current == TaskState::Cancelled {
                let attempt_state = attempt_state_in(&mut transaction, &owner).await?;
                let session_state = session_state_in(&mut transaction, &owner).await?;
                let request = read_idempotency_record(&mut transaction, &request_key).await?;
                let request_confirmed = match request {
                    Some((method, stored_fingerprint, result)) => {
                        verify_cancellation_record(
                            &owner,
                            &request_method,
                            &fingerprint,
                            &method,
                            &stored_fingerprint,
                        )?;
                        cancellation_status(&owner, result.as_deref())?
                            == CancellationStatus::Confirmed
                    }
                    None => true,
                };
                if attempt_state == AttemptState::Cancelled
                    && session_state == SessionState::Exited
                    && owner_result.as_deref() == Some(closed_result.as_str())
                    && request_confirmed
                {
                    transaction.commit().await?;
                    return Ok(());
                }
                return Err(StoreError::CancellationConflict {
                    task_id: owner.task_id,
                    detail: "task is cancelled without this owner's confirmation",
                });
            }
            if owner_result.is_some() {
                return Err(StoreError::NoActiveCancellationOwner {
                    task_id: owner.task_id,
                    session_id: owner.session_id,
                });
            }
            if current != TaskState::Running {
                return Err(StoreError::CancellationConflict {
                    task_id: owner.task_id,
                    detail: "task is no longer running",
                });
            }
            let Some((request_stored_method, request_fingerprint, request_result)) =
                read_idempotency_record(&mut transaction, &request_key).await?
            else {
                return Err(StoreError::CancellationConflict {
                    task_id: owner.task_id,
                    detail: "there is no cancellation request to confirm",
                });
            };
            verify_cancellation_record(
                &owner,
                &request_method,
                &fingerprint,
                &request_stored_method,
                &request_fingerprint,
            )?;
            if cancellation_status(&owner, request_result.as_deref())?
                != CancellationStatus::Pending
            {
                return Err(StoreError::CancellationConflict {
                    task_id: owner.task_id,
                    detail: "cancellation request is not pending",
                });
            }

            let attempt_state = attempt_state_in(&mut transaction, &owner).await?;
            let session_state = session_state_in(&mut transaction, &owner).await?;
            if attempt_state != AttemptState::Running || session_state != SessionState::Running {
                return Err(StoreError::CancellationConflict {
                    task_id: owner.task_id,
                    detail: "attempt or session is no longer running",
                });
            }
            attempt_transition(attempt_state, AttemptState::Cancelled)?;
            session_transition(session_state, SessionState::Exited)?;
            let (next, audit) = task_transition(current, TaskEvent::CancellationConfirmed)?;

            sqlx::query("UPDATE sessions SET state = ? WHERE task_id = ? AND id = ? AND state = ?")
                .bind(encode(&SessionState::Exited)?)
                .bind(&owner.task_id)
                .bind(&owner.session_id)
                .bind(encode(&session_state)?)
                .execute(&mut *transaction)
                .await?
                .rows_affected()
                .eq(&1)
                .then_some(())
                .ok_or_else(|| StoreError::CancellationConflict {
                    task_id: owner.task_id.clone(),
                    detail: "session changed before confirmation",
                })?;
            bump_task_version(&mut transaction, &owner.task_id).await?;
            append_durable_event(
                &mut transaction,
                &owner.task_id,
                Some(&owner.session_id),
                "session.state_changed",
                &serde_json::json!({
                    "session_id": owner.session_id,
                    "state": SessionState::Exited
                }),
            )
            .await?;

            sqlx::query("UPDATE attempts SET state = ? WHERE task_id = ? AND id = ? AND state = ?")
                .bind(encode(&AttemptState::Cancelled)?)
                .bind(&owner.task_id)
                .bind(&owner.attempt_id)
                .bind(encode(&attempt_state)?)
                .execute(&mut *transaction)
                .await?
                .rows_affected()
                .eq(&1)
                .then_some(())
                .ok_or_else(|| StoreError::CancellationConflict {
                    task_id: owner.task_id.clone(),
                    detail: "attempt changed before confirmation",
                })?;
            bump_task_version(&mut transaction, &owner.task_id).await?;
            append_durable_event(
                &mut transaction,
                &owner.task_id,
                None,
                "attempt.state_changed",
                &serde_json::json!({
                    "attempt_id": owner.attempt_id,
                    "state": AttemptState::Cancelled
                }),
            )
            .await?;

            let updated = sqlx::query(
                "UPDATE tasks SET state = ?, version = version + 1 WHERE id = ? AND state = ?",
            )
            .bind(encode(&next)?)
            .bind(&owner.task_id)
            .bind(encode(&current)?)
            .execute(&mut *transaction)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(StoreError::CancellationConflict {
                    task_id: owner.task_id.clone(),
                    detail: "task changed before confirmation",
                });
            }
            sqlx::query("INSERT INTO events (task_id, kind, payload) VALUES (?, ?, ?)")
                .bind(&owner.task_id)
                .bind(format!("{audit:?}"))
                .bind(encode(&audit)?)
                .execute(&mut *transaction)
                .await?;
            append_durable_event(
                &mut transaction,
                &owner.task_id,
                None,
                "task.transition",
                &serde_json::json!({"audit": audit, "state": next}),
            )
            .await?;

            write_cancellation_receipt(
                &mut transaction,
                &owner,
                &request_key,
                &request_method,
                &fingerprint,
                CancellationStatus::Confirmed,
            )
            .await?;
            close_cancellation_owner(
                &mut transaction,
                &owner,
                &owner_key,
                &owner_method,
                &fingerprint,
            )
            .await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    /// Resolves an unconfirmed request after the owner has finished monitoring
    /// the agent. `Rejected` means the agent exited without cancellation;
    /// `Failed` means the owner could not establish a confirmed cancellation.
    pub fn finish_cancellation_owner(
        &self,
        owner: &CancellationOwner,
        status: CancellationStatus,
    ) -> Result<(), StoreError> {
        validate_cancellation_owner(owner)?;
        if !matches!(
            status,
            CancellationStatus::Rejected | CancellationStatus::Failed
        ) {
            return Err(StoreError::CancellationConflict {
                task_id: owner.task_id.clone(),
                detail: "non-cancelled owner must finish as rejected or failed",
            });
        }
        let owner = owner.clone();
        let owner_key = cancellation_owner_key(&owner);
        let owner_method = cancellation_owner_method(&owner);
        let request_key = cancellation_request_key(&owner);
        let request_method = cancellation_request_method(&owner);
        let fingerprint = cancellation_fingerprint(&owner);
        let closed_result = serde_json::json!({"status": "closed"}).to_string();
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            sqlx::query(
                "UPDATE idempotency_records SET result = result WHERE key = ? \
                 AND method = ? AND fingerprint = ?",
            )
            .bind(&owner_key)
            .bind(&owner_method)
            .bind(&fingerprint)
            .execute(&mut *transaction)
            .await?;
            let Some((stored_method, stored_fingerprint, owner_result)) =
                read_idempotency_record(&mut transaction, &owner_key).await?
            else {
                return Err(StoreError::NoActiveCancellationOwner {
                    task_id: owner.task_id,
                    session_id: owner.session_id,
                });
            };
            verify_cancellation_record(
                &owner,
                &owner_method,
                &fingerprint,
                &stored_method,
                &stored_fingerprint,
            )?;
            let task_state = current_task_state(&mut transaction, &owner.task_id).await?;
            if task_state == TaskState::Running || task_state == TaskState::Cancelled {
                return Err(StoreError::CancellationConflict {
                    task_id: owner.task_id,
                    detail: "task outcome is not ready to resolve cancellation",
                });
            }

            let request = read_idempotency_record(&mut transaction, &request_key).await?;
            if let Some((method, stored_fingerprint, result)) = request {
                verify_cancellation_record(
                    &owner,
                    &request_method,
                    &fingerprint,
                    &method,
                    &stored_fingerprint,
                )?;
                if let Some(result) = result {
                    if cancellation_status(&owner, Some(&result))? != status {
                        return Err(StoreError::CancellationConflict {
                            task_id: owner.task_id,
                            detail: "cancellation request already has a different result",
                        });
                    }
                } else {
                    write_cancellation_receipt(
                        &mut transaction,
                        &owner,
                        &request_key,
                        &request_method,
                        &fingerprint,
                        status,
                    )
                    .await?;
                }
            }

            if let Some(result) = owner_result {
                if result != closed_result {
                    return Err(StoreError::CancellationConflict {
                        task_id: owner.task_id,
                        detail: "owner registration already has a different result",
                    });
                }
            } else {
                close_cancellation_owner(
                    &mut transaction,
                    &owner,
                    &owner_key,
                    &owner_method,
                    &fingerprint,
                )
                .await?;
            }
            transaction.commit().await?;
            Ok(())
        })
    }
}

async fn read_open_cancellation_owners(
    transaction: &mut Transaction<'_, Sqlite>,
    task_id: Option<&str>,
) -> Result<Vec<CancellationOwner>, StoreError> {
    let prefix = task_id
        .map(|task_id| format!("cli.cancel.owner:{task_id}:"))
        .unwrap_or_else(|| "cli.cancel.owner:".to_string());
    let rows = sqlx::query(
        "SELECT key, method, fingerprint FROM idempotency_records \
         WHERE substr(method, 1, ?) = ? AND result IS NULL ORDER BY key",
    )
    .bind(prefix.len() as i64)
    .bind(&prefix)
    .fetch_all(&mut **transaction)
    .await?;
    let mut owners = Vec::with_capacity(rows.len());
    for row in rows {
        let key = row.try_get::<String, _>("key")?;
        let method = row.try_get::<String, _>("method")?;
        let record_task_id = task_id
            .map(str::to_owned)
            .or_else(|| cancellation_owner_task_id(&method))
            .ok_or(StoreError::IdempotencyConflict)?;
        let mut owner = parse_cancellation_owner_record(
            &record_task_id,
            &key,
            &method,
            &row.try_get::<String, _>("fingerprint")?,
        )?;
        owner.process_identity =
            read_cancellation_owner_process_identity(transaction, &owner).await?;
        owners.push(owner);
    }
    Ok(owners)
}

async fn read_cancellation_owner_process_identity(
    transaction: &mut Transaction<'_, Sqlite>,
    owner: &CancellationOwner,
) -> Result<Option<ExactProcessIdentity>, StoreError> {
    let key = cancellation_owner_process_key(owner);
    let Some((method, fingerprint, result)) = read_idempotency_record(transaction, &key).await?
    else {
        return Ok(None);
    };
    if method != cancellation_owner_process_method(owner)
        || fingerprint != cancellation_fingerprint(owner)
    {
        return Err(StoreError::IdempotencyConflict);
    }
    let Some(result) = result else {
        return Err(StoreError::CancellationConflict {
            task_id: owner.task_id.clone(),
            detail: "owner process identity record has no value",
        });
    };
    let identity: ExactProcessIdentity = decode(&result)?;
    if !valid_exact_process_identity(identity) {
        return Err(StoreError::IdempotencyConflict);
    }
    Ok(Some(identity))
}

fn cancellation_owner_task_id(method: &str) -> Option<String> {
    let identity = method.strip_prefix("cli.cancel.owner:")?;
    let (task_id, _) = identity.split_once(':')?;
    (!task_id.is_empty()).then(|| task_id.to_string())
}

async fn require_running_owner_records(
    transaction: &mut Transaction<'_, Sqlite>,
    owner: &CancellationOwner,
) -> Result<(), StoreError> {
    let task_state = current_task_state(transaction, &owner.task_id).await?;
    if task_state != TaskState::Running {
        return Err(StoreError::CancellationConflict {
            task_id: owner.task_id.clone(),
            detail: "task is not running",
        });
    }
    if attempt_state_in(transaction, owner).await? != AttemptState::Running
        || session_state_in(transaction, owner).await? != SessionState::Running
    {
        return Err(StoreError::CancellationConflict {
            task_id: owner.task_id.clone(),
            detail: "attempt or session is not running",
        });
    }
    Ok(())
}

async fn attempt_state_in(
    transaction: &mut Transaction<'_, Sqlite>,
    owner: &CancellationOwner,
) -> Result<AttemptState, StoreError> {
    let row = sqlx::query("SELECT state FROM attempts WHERE task_id = ? AND id = ?")
        .bind(&owner.task_id)
        .bind(&owner.attempt_id)
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or_else(|| StoreError::NoAttempt {
            task_id: owner.task_id.clone(),
            attempt_id: owner.attempt_id.clone(),
        })?;
    decode(&row.try_get::<String, _>("state")?)
}

async fn session_state_in(
    transaction: &mut Transaction<'_, Sqlite>,
    owner: &CancellationOwner,
) -> Result<SessionState, StoreError> {
    let row = sqlx::query("SELECT state FROM sessions WHERE task_id = ? AND id = ?")
        .bind(&owner.task_id)
        .bind(&owner.session_id)
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or_else(|| StoreError::NoSession {
            task_id: owner.task_id.clone(),
            session_id: owner.session_id.clone(),
        })?;
    decode(&row.try_get::<String, _>("state")?)
}

async fn read_idempotency_record(
    transaction: &mut Transaction<'_, Sqlite>,
    key: &str,
) -> Result<Option<(String, String, Option<String>)>, StoreError> {
    let row =
        sqlx::query("SELECT method, fingerprint, result FROM idempotency_records WHERE key = ?")
            .bind(key)
            .fetch_optional(&mut **transaction)
            .await?;
    row.map(|row| {
        Ok((
            row.try_get("method")?,
            row.try_get("fingerprint")?,
            row.try_get("result")?,
        ))
    })
    .transpose()
}

fn validate_cancellation_owner(owner: &CancellationOwner) -> Result<(), StoreError> {
    validate_task_id(&owner.task_id)?;
    validate_task_id(&owner.attempt_id)?;
    validate_task_id(&owner.session_id)?;
    if owner.generation.len() != 32
        || !owner
            .generation
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(StoreError::InvalidCancellationOwner);
    }
    if owner
        .process_identity
        .is_some_and(|identity| !valid_exact_process_identity(identity))
    {
        return Err(StoreError::InvalidCancellationOwner);
    }
    Ok(())
}

fn valid_exact_process_identity(identity: ExactProcessIdentity) -> bool {
    let process = identity.identity;
    if process.pid == 0 || process.process_group == Some(0) {
        return false;
    }
    match (
        process.platform,
        process.start_time_micros,
        identity.windows_creation_time_100ns_since_unix_epoch,
    ) {
        (orchestrator_core::identity::ProcessPlatform::Windows, Some(micros), Some(ticks)) => {
            ticks / 10 == micros
        }
        (_, _, None) => true,
        _ => false,
    }
}

fn cancellation_owner_key(owner: &CancellationOwner) -> String {
    format!("agent-orch-owner-{}", owner.generation)
}

fn cancellation_owner_process_key(owner: &CancellationOwner) -> String {
    format!("agent-orch-owner-process-{}", owner.generation)
}

fn parse_cancellation_owner_record(
    task_id: &str,
    key: &str,
    method: &str,
    fingerprint: &str,
) -> Result<CancellationOwner, StoreError> {
    let prefix = format!("cli.cancel.owner:{task_id}:");
    let Some(identity) = method.strip_prefix(&prefix) else {
        return Err(StoreError::IdempotencyConflict);
    };
    let Some((attempt_id, session_id)) = identity.split_once(':') else {
        return Err(StoreError::IdempotencyConflict);
    };
    let Some(generation) = key.strip_prefix("agent-orch-owner-") else {
        return Err(StoreError::IdempotencyConflict);
    };
    let owner = CancellationOwner {
        task_id: task_id.to_owned(),
        attempt_id: attempt_id.to_owned(),
        session_id: session_id.to_owned(),
        generation: generation.to_owned(),
        process_identity: None,
    };
    validate_cancellation_owner(&owner)?;
    if session_id.contains(':') || cancellation_fingerprint(&owner) != fingerprint {
        return Err(StoreError::IdempotencyConflict);
    }
    Ok(owner)
}

fn cancellation_request_key(owner: &CancellationOwner) -> String {
    format!("agent-orch-cancel-{}", owner.generation)
}

fn cancellation_owner_method(owner: &CancellationOwner) -> String {
    format!(
        "cli.cancel.owner:{}:{}:{}",
        owner.task_id, owner.attempt_id, owner.session_id
    )
}

fn cancellation_owner_process_method(owner: &CancellationOwner) -> String {
    format!(
        "cli.cancel.owner_process:{}:{}:{}",
        owner.task_id, owner.attempt_id, owner.session_id
    )
}

fn cancellation_request_method(owner: &CancellationOwner) -> String {
    format!(
        "cli.cancel.request:{}:{}:{}",
        owner.task_id, owner.attempt_id, owner.session_id
    )
}

fn cancellation_fingerprint(owner: &CancellationOwner) -> String {
    // The generation is random and immutable for this owner. Duplicating it
    // fills the existing idempotency API's 64-hex fingerprint field; the
    // task/attempt/session binding is independently checked in the method.
    format!("{}{}", owner.generation, owner.generation)
}

fn verify_cancellation_record(
    owner: &CancellationOwner,
    expected_method: &str,
    expected_fingerprint: &str,
    stored_method: &str,
    stored_fingerprint: &str,
) -> Result<(), StoreError> {
    let owner_method = cancellation_owner_method(owner);
    let request_method = cancellation_request_method(owner);
    if (expected_method != owner_method && expected_method != request_method)
        || expected_fingerprint != cancellation_fingerprint(owner)
        || stored_method != expected_method
        || stored_fingerprint != expected_fingerprint
    {
        return Err(StoreError::IdempotencyConflict);
    }
    Ok(())
}

fn cancellation_status(
    owner: &CancellationOwner,
    result: Option<&str>,
) -> Result<CancellationStatus, StoreError> {
    let Some(result) = result else {
        return Ok(CancellationStatus::Pending);
    };
    let receipt: CancellationReceipt = decode(result)?;
    if receipt.task_id != owner.task_id
        || receipt.attempt_id != owner.attempt_id
        || receipt.session_id != owner.session_id
        || receipt.generation != owner.generation
    {
        return Err(StoreError::IdempotencyConflict);
    }
    Ok(receipt.status)
}

async fn write_cancellation_receipt(
    transaction: &mut Transaction<'_, Sqlite>,
    owner: &CancellationOwner,
    key: &str,
    method: &str,
    fingerprint: &str,
    status: CancellationStatus,
) -> Result<(), StoreError> {
    if let Some((stored_method, stored_fingerprint, result)) =
        read_idempotency_record(transaction, key).await?
    {
        verify_cancellation_record(
            owner,
            method,
            fingerprint,
            &stored_method,
            &stored_fingerprint,
        )?;
        if let Some(result) = result {
            if cancellation_status(owner, Some(&result))? == status {
                return Ok(());
            }
            return Err(StoreError::CancellationConflict {
                task_id: owner.task_id.clone(),
                detail: "cancellation request already has a different result",
            });
        }
        let receipt = CancellationReceipt {
            task_id: owner.task_id.clone(),
            attempt_id: owner.attempt_id.clone(),
            session_id: owner.session_id.clone(),
            generation: owner.generation.clone(),
            status,
        };
        let result = encode(&receipt)?;
        let updated = sqlx::query(
            "UPDATE idempotency_records SET result = ? WHERE key = ? AND method = ? \
             AND fingerprint = ? AND result IS NULL",
        )
        .bind(result)
        .bind(key)
        .bind(method)
        .bind(fingerprint)
        .execute(&mut **transaction)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(StoreError::CancellationConflict {
                task_id: owner.task_id.clone(),
                detail: "cancellation request changed before acknowledgement",
            });
        }
    }
    Ok(())
}

async fn close_cancellation_owner(
    transaction: &mut Transaction<'_, Sqlite>,
    owner: &CancellationOwner,
    key: &str,
    method: &str,
    fingerprint: &str,
) -> Result<(), StoreError> {
    let Some((stored_method, stored_fingerprint, result)) =
        read_idempotency_record(transaction, key).await?
    else {
        return Err(StoreError::NoActiveCancellationOwner {
            task_id: owner.task_id.clone(),
            session_id: owner.session_id.clone(),
        });
    };
    verify_cancellation_record(
        owner,
        method,
        fingerprint,
        &stored_method,
        &stored_fingerprint,
    )?;
    let closed = serde_json::json!({"status": "closed"}).to_string();
    if result.as_deref() == Some(closed.as_str()) {
        return Ok(());
    }
    if result.is_some() {
        return Err(StoreError::CancellationConflict {
            task_id: owner.task_id.clone(),
            detail: "owner registration already has a result",
        });
    }
    let updated = sqlx::query(
        "UPDATE idempotency_records SET result = ? WHERE key = ? AND method = ? \
         AND fingerprint = ? AND result IS NULL",
    )
    .bind(closed)
    .bind(key)
    .bind(method)
    .bind(fingerprint)
    .execute(&mut **transaction)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(StoreError::CancellationConflict {
            task_id: owner.task_id.clone(),
            detail: "owner registration changed before closure",
        });
    }
    Ok(())
}

impl Store for SqliteStore {
    fn create(&self, contract: &TaskContract) -> Result<(), StoreError> {
        validate_contract(contract)?;
        validate_task_id(&contract.id)?;
        let contract = contract.clone();
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            let state = encode(&TaskState::Created)?;
            let contract_json = encode(&contract)?;
            let inserted = sqlx::query(
                "INSERT INTO tasks (id, project_id, parent_id, state, contract_json, version) VALUES (?, ?, ?, ?, ?, 1) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&contract.id)
            .bind(&contract.project_id)
            .bind(contract.parent_id.as_deref())
            .bind(state)
            .bind(contract_json)
            .execute(&mut *transaction)
            .await?;
            if inserted.rows_affected() == 0 {
                return Err(StoreError::AlreadyExists(contract.id));
            }
            for dependency_id in &contract.dependencies {
                sqlx::query(
                    "INSERT INTO task_dependencies (task_id, depends_on_id) VALUES (?, ?)",
                )
                .bind(&contract.id)
                .bind(dependency_id)
                .execute(&mut *transaction)
                .await?;
            }
            let payload = serde_json::json!({"state": TaskState::Created});
            append_durable_event(
                &mut transaction,
                &contract.id,
                None,
                "task.created",
                &payload,
            )
            .await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    fn get(&self, task_id: &str) -> Result<TaskRecord, StoreError> {
        validate_task_id(task_id)?;
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            let record = load_record(&mut transaction, &task_id).await?;
            transaction.commit().await?;
            Ok(record)
        })
    }

    fn list(&self) -> Result<Vec<TaskRecord>, StoreError> {
        self.call(|pool| async move {
            let mut transaction = pool.begin().await?;
            let task_ids = sqlx::query_scalar::<_, String>("SELECT id FROM tasks ORDER BY id")
                .fetch_all(&mut *transaction)
                .await?;
            let mut records = Vec::with_capacity(task_ids.len());
            for task_id in task_ids {
                records.push(load_record(&mut transaction, &task_id).await?);
            }
            transaction.commit().await?;
            Ok(records)
        })
    }

    fn transition(&self, task_id: &str, event: TaskEvent) -> Result<AuditEvent, StoreError> {
        validate_task_id(task_id)?;
        if event == TaskEvent::CancellationConfirmed {
            return Err(StoreError::CancellationConflict {
                task_id: task_id.to_owned(),
                detail: "confirmation requires the owner acknowledgement API",
            });
        }
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            let current = current_task_state(&mut transaction, &task_id).await?;
            let (next, audit) = task_transition(current, event)?;
            let state = encode(&next)?;
            let updated =
                sqlx::query("UPDATE tasks SET state = ?, version = version + 1 WHERE id = ?")
                    .bind(state)
                    .bind(&task_id)
                    .execute(&mut *transaction)
                    .await?;
            if updated.rows_affected() != 1 {
                return Err(StoreError::NotFound(task_id));
            }
            sqlx::query("INSERT INTO events (task_id, kind, payload) VALUES (?, ?, ?)")
                .bind(&task_id)
                .bind(format!("{audit:?}"))
                .bind(encode(&audit)?)
                .execute(&mut *transaction)
                .await?;
            let payload = serde_json::json!({"audit": &audit, "state": &next});
            append_durable_event(
                &mut transaction,
                &task_id,
                None,
                "task.transition",
                &payload,
            )
            .await?;
            transaction.commit().await?;
            Ok(audit)
        })
    }

    fn add_attempt(&self, task_id: &str, attempt: &Attempt) -> Result<(), StoreError> {
        validate_task_id(task_id)?;
        let task_id = task_id.to_owned();
        let attempt = attempt.clone();
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            let task_state = current_task_state(&mut transaction, &task_id).await?;
            let duplicate = sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS (SELECT 1 FROM attempts WHERE task_id = ? AND id = ?)",
            )
            .bind(&task_id)
            .bind(&attempt.id)
            .fetch_one(&mut *transaction)
            .await?;
            if duplicate != 0 {
                return Err(StoreError::DuplicateAttempt {
                    attempt_id: attempt.id,
                    task_id,
                });
            }
            if task_state != TaskState::Queued {
                return Err(StoreError::NotAcceptingAttempts(task_state));
            }
            if attempt.id.is_empty() {
                return Err(StoreError::EmptyAttemptId {
                    task_id: task_id.clone(),
                });
            }
            if attempt.state != AttemptState::Pending {
                return Err(StoreError::AttemptNotPending {
                    attempt_id: attempt.id,
                    state: attempt.state,
                });
            }
            if attempt.task_id != task_id {
                return Err(StoreError::CrossTaskAttempt {
                    attempt_id: attempt.id,
                    attempt_task: attempt.task_id,
                    record_task: task_id,
                });
            }
            let previous_number = sqlx::query_scalar::<_, i64>(
                "SELECT COALESCE(MAX(attempt_number), 0) FROM attempts WHERE task_id = ?",
            )
            .bind(&task_id)
            .fetch_one(&mut *transaction)
            .await?;
            let expected = u32::try_from(previous_number)
                .ok()
                .and_then(|number| number.checked_add(1))
                .ok_or_else(|| StoreError::Corrupt {
                    task_id: task_id.clone(),
                    detail: "attempt number exceeds u32".to_owned(),
                })?;
            if attempt.attempt_number != expected {
                return Err(StoreError::BadAttemptNumber {
                    expected,
                    got: attempt.attempt_number,
                    task_id,
                });
            }
            sqlx::query(
                "INSERT INTO attempts (id, task_id, attempt_number, state) VALUES (?, ?, ?, ?)",
            )
            .bind(&attempt.id)
            .bind(&task_id)
            .bind(i64::from(attempt.attempt_number))
            .bind(encode(&attempt.state)?)
            .execute(&mut *transaction)
            .await?;
            bump_task_version(&mut transaction, &task_id).await?;
            let payload = serde_json::json!({
                "attempt_id": &attempt.id,
                "attempt_number": attempt.attempt_number,
                "state": attempt.state,
            });
            append_durable_event(
                &mut transaction,
                &task_id,
                None,
                "attempt.created",
                &payload,
            )
            .await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    fn update_attempt_state(
        &self,
        task_id: &str,
        attempt_id: &str,
        state: AttemptState,
    ) -> Result<(), StoreError> {
        validate_task_id(task_id)?;
        let task_id = task_id.to_owned();
        let attempt_id = attempt_id.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            current_task_state(&mut transaction, &task_id).await?;
            let row = sqlx::query("SELECT state FROM attempts WHERE task_id = ? AND id = ?")
                .bind(&task_id)
                .bind(&attempt_id)
                .fetch_optional(&mut *transaction)
                .await?
                .ok_or_else(|| StoreError::NoAttempt {
                    attempt_id: attempt_id.clone(),
                    task_id: task_id.clone(),
                })?;
            let current: AttemptState = decode(&row.try_get::<String, _>("state")?)?;
            if current.is_terminal() {
                return Err(StoreError::TerminalAttempt {
                    attempt_id,
                    state: current,
                });
            }
            attempt_transition(current, state)?;
            sqlx::query("UPDATE attempts SET state = ? WHERE task_id = ? AND id = ?")
                .bind(encode(&state)?)
                .bind(&task_id)
                .bind(&attempt_id)
                .execute(&mut *transaction)
                .await?;
            bump_task_version(&mut transaction, &task_id).await?;
            let payload = serde_json::json!({"attempt_id": &attempt_id, "state": state});
            append_durable_event(
                &mut transaction,
                &task_id,
                None,
                "attempt.state_changed",
                &payload,
            )
            .await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    fn add_session(&self, task_id: &str, session: &Session) -> Result<(), StoreError> {
        validate_task_id(task_id)?;
        let task_id = task_id.to_owned();
        let session = session.clone();
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            current_task_state(&mut transaction, &task_id).await?;
            let duplicate = sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS (SELECT 1 FROM sessions WHERE task_id = ? AND id = ?)",
            )
            .bind(&task_id)
            .bind(&session.id)
            .fetch_one(&mut *transaction)
            .await?;
            if duplicate != 0 {
                return Err(StoreError::DuplicateSession {
                    session_id: session.id,
                    task_id,
                });
            }
            let attempt_exists = sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS (SELECT 1 FROM attempts WHERE task_id = ? AND id = ?)",
            )
            .bind(&task_id)
            .bind(&session.attempt_id)
            .fetch_one(&mut *transaction)
            .await?;
            if attempt_exists == 0 {
                return Err(StoreError::UnknownAttempt {
                    session_id: session.id,
                    attempt_id: session.attempt_id,
                });
            }
            if session.state != SessionState::Spawning {
                return Err(StoreError::SessionNotSpawning {
                    session_id: session.id,
                    state: session.state,
                });
            }
            sqlx::query(
                "INSERT INTO sessions (id, task_id, attempt_id, adapter, state) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&session.id)
            .bind(&task_id)
            .bind(&session.attempt_id)
            .bind(&session.adapter)
            .bind(encode(&session.state)?)
            .execute(&mut *transaction)
            .await?;
            bump_task_version(&mut transaction, &task_id).await?;
            let payload = serde_json::json!({
                "session_id": &session.id,
                "attempt_id": &session.attempt_id,
                "state": session.state,
            });
            append_durable_event(
                &mut transaction,
                &task_id,
                Some(&session.id),
                "session.created",
                &payload,
            )
            .await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    fn update_session_state(
        &self,
        task_id: &str,
        session_id: &str,
        state: SessionState,
    ) -> Result<(), StoreError> {
        validate_task_id(task_id)?;
        let task_id = task_id.to_owned();
        let session_id = session_id.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            current_task_state(&mut transaction, &task_id).await?;
            let row = sqlx::query("SELECT state FROM sessions WHERE task_id = ? AND id = ?")
                .bind(&task_id)
                .bind(&session_id)
                .fetch_optional(&mut *transaction)
                .await?
                .ok_or_else(|| StoreError::NoSession {
                    session_id: session_id.clone(),
                    task_id: task_id.clone(),
                })?;
            let current: SessionState = decode(&row.try_get::<String, _>("state")?)?;
            if current.is_terminal() {
                return Err(StoreError::TerminalSession {
                    session_id,
                    state: current,
                });
            }
            session_transition(current, state)?;
            sqlx::query("UPDATE sessions SET state = ? WHERE task_id = ? AND id = ?")
                .bind(encode(&state)?)
                .bind(&task_id)
                .bind(&session_id)
                .execute(&mut *transaction)
                .await?;
            bump_task_version(&mut transaction, &task_id).await?;
            let payload = serde_json::json!({"session_id": &session_id, "state": state});
            append_durable_event(
                &mut transaction,
                &task_id,
                Some(&session_id),
                "session.state_changed",
                &payload,
            )
            .await?;
            transaction.commit().await?;
            Ok(())
        })
    }
}

async fn load_record(
    transaction: &mut Transaction<'_, Sqlite>,
    task_id: &str,
) -> Result<TaskRecord, StoreError> {
    let row = sqlx::query("SELECT state, contract_json, version FROM tasks WHERE id = ?")
        .bind(task_id)
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or_else(|| StoreError::NotFound(task_id.to_owned()))?;
    let state: TaskState = decode(&row.try_get::<String, _>("state")?)?;
    let contract: TaskContract = decode(&row.try_get::<String, _>("contract_json")?)?;
    let version =
        u64::try_from(row.try_get::<i64, _>("version")?).map_err(|_| StoreError::Corrupt {
            task_id: task_id.to_owned(),
            detail: "task version is negative".to_owned(),
        })?;

    let attempt_rows = sqlx::query(
        "SELECT id, attempt_number, state FROM attempts WHERE task_id = ? ORDER BY attempt_number",
    )
    .bind(task_id)
    .fetch_all(&mut **transaction)
    .await?;
    let attempts =
        attempt_rows
            .into_iter()
            .map(|row| {
                let attempt_number = u32::try_from(row.try_get::<i64, _>("attempt_number")?)
                    .map_err(|_| StoreError::Corrupt {
                        task_id: task_id.to_owned(),
                        detail: "attempt number exceeds u32".to_owned(),
                    })?;
                Ok(Attempt {
                    id: row.try_get("id")?,
                    task_id: task_id.to_owned(),
                    attempt_number,
                    state: decode(&row.try_get::<String, _>("state")?)?,
                })
            })
            .collect::<Result<Vec<_>, StoreError>>()?;

    let session_rows = sqlx::query(
        "SELECT id, attempt_id, adapter, state FROM sessions WHERE task_id = ? ORDER BY sequence",
    )
    .bind(task_id)
    .fetch_all(&mut **transaction)
    .await?;
    let sessions = session_rows
        .into_iter()
        .map(|row| {
            Ok(Session {
                id: row.try_get("id")?,
                attempt_id: row.try_get("attempt_id")?,
                adapter: row.try_get("adapter")?,
                state: decode(&row.try_get::<String, _>("state")?)?,
            })
        })
        .collect::<Result<Vec<_>, StoreError>>()?;

    let event_rows = sqlx::query("SELECT payload FROM events WHERE task_id = ? ORDER BY seq")
        .bind(task_id)
        .fetch_all(&mut **transaction)
        .await?;
    let events = event_rows
        .into_iter()
        .map(|row| decode(&row.try_get::<String, _>("payload")?))
        .collect::<Result<Vec<AuditEvent>, StoreError>>()?;

    Ok(TaskRecord {
        contract,
        state,
        attempts,
        sessions,
        events,
        version,
    })
}

async fn current_task_state(
    transaction: &mut Transaction<'_, Sqlite>,
    task_id: &str,
) -> Result<TaskState, StoreError> {
    let row = sqlx::query("SELECT state FROM tasks WHERE id = ?")
        .bind(task_id)
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or_else(|| StoreError::NotFound(task_id.to_owned()))?;
    decode(&row.try_get::<String, _>("state")?)
}

async fn bump_task_version(
    transaction: &mut Transaction<'_, Sqlite>,
    task_id: &str,
) -> Result<(), StoreError> {
    let updated = sqlx::query("UPDATE tasks SET version = version + 1 WHERE id = ?")
        .bind(task_id)
        .execute(&mut **transaction)
        .await?;
    if updated.rows_affected() != 1 {
        return Err(StoreError::NotFound(task_id.to_owned()));
    }
    Ok(())
}

fn validate_task_id(task_id: &str) -> Result<(), StoreError> {
    if task_id.is_empty()
        || !task_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(StoreError::InvalidTaskId(task_id.to_owned()));
    }
    Ok(())
}

fn encode<T: Serialize>(value: &T) -> Result<String, StoreError> {
    Ok(serde_json::to_string(value)?)
}

fn decode<T: DeserializeOwned>(value: &str) -> Result<T, StoreError> {
    Ok(serde_json::from_str(value)?)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use orchestrator_core::authorization::{Action, ActionKind, ApprovalState, Destination};
    use orchestrator_core::contract::{
        AutonomyPolicy, DeliveryPolicy, TaskKind, VerificationPolicy,
    };
    use orchestrator_core::records::{Artifact, Project, VerificationRun, Workspace};
    use orchestrator_core::task::{
        Attempt, AttemptState, Session, SessionState, TaskEvent, TaskState,
    };

    use super::*;

    fn project() -> Project {
        Project {
            id: "p1".into(),
            root_path: "/work/project".into(),
            settings: serde_json::json!({"branch": "main"}),
        }
    }

    fn contract(id: &str) -> TaskContract {
        TaskContract {
            id: id.into(),
            project_id: "p1".into(),
            parent_id: None,
            objective: "Implement the thing".into(),
            kind: TaskKind::Implement,
            agent: "codex".into(),
            dependencies: Vec::new(),
            verification: VerificationPolicy::Commands {
                commands: vec![vec!["cargo".into(), "test".into()]],
            },
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 1,
            max_repairs: 1,
        }
    }

    fn attempt() -> Attempt {
        Attempt {
            id: "a1".into(),
            task_id: "t1".into(),
            attempt_number: 1,
            state: AttemptState::Pending,
        }
    }

    fn session() -> Session {
        Session {
            id: "s1".into(),
            attempt_id: "a1".into(),
            adapter: "codex".into(),
            state: SessionState::Spawning,
        }
    }

    #[test]
    fn transition_rolls_back_state_when_event_insert_fails() {
        let directory = tempfile::tempdir().unwrap();
        let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
        store.register_project(&project()).unwrap();
        store.create(&contract("t1")).unwrap();
        store
            .call(|pool| async move {
                sqlx::query(
                    "CREATE TRIGGER reject_events BEFORE INSERT ON events BEGIN SELECT RAISE(ABORT, 'forced journal failure'); END",
                )
                .execute(&pool)
                .await?;
                Ok(())
            })
            .unwrap();

        assert!(matches!(
            store.transition("t1", TaskEvent::Admit),
            Err(StoreError::Database(_))
        ));
        let record = store.get("t1").unwrap();
        assert_eq!(record.state, TaskState::Created);
        assert!(record.events.is_empty());
        assert!(store.event_journal(0).unwrap().is_empty());
    }

    #[test]
    fn task_records_and_event_sequence_survive_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("store.sqlite");
        let project = project();
        let workspace = Workspace {
            id: "w1".into(),
            task_id: "t1".into(),
            path: "/worktrees/t1".into(),
            branch: "task/t1".into(),
            base_commit: "abc123".into(),
        };
        let approval = Approval {
            id: "approval1".into(),
            action: Action {
                kind: ActionKind::LocalReviewMerge,
                destination: Destination::LocalBranch {
                    branch: "main".into(),
                },
                parameters: Default::default(),
                candidate_commit: "def456".into(),
                task_id: "t1".into(),
            },
            granted_by: "user".into(),
            state: ApprovalState::Granted,
        };
        let verification_run = VerificationRun {
            id: "v1".into(),
            task_id: "t1".into(),
            commit: "def456".into(),
            status: "passed".into(),
            log_path: Some("/logs/v1.txt".into()),
        };
        let artifact = Artifact {
            id: "artifact1".into(),
            task_id: "t1".into(),
            kind: "report".into(),
            path: "/artifacts/report.md".into(),
            checksum: "sha256:0123".into(),
        };
        let original_record;
        let original_events;
        {
            let store = SqliteStore::new(&path).unwrap();
            store.register_project(&project).unwrap();
            store.create(&contract("t1")).unwrap();
            assert!(matches!(
                store.transition("t1", TaskEvent::ExecutionStarted),
                Err(StoreError::IllegalTransition(_))
            ));
            store.transition("t1", TaskEvent::Admit).unwrap();
            store.add_attempt("t1", &attempt()).unwrap();
            store
                .update_attempt_state("t1", "a1", AttemptState::Running)
                .unwrap();
            store
                .update_attempt_state("t1", "a1", AttemptState::CandidateReady)
                .unwrap();
            store.add_session("t1", &session()).unwrap();
            store
                .update_session_state("t1", "s1", SessionState::Running)
                .unwrap();
            store
                .update_session_state("t1", "s1", SessionState::Exited)
                .unwrap();
            store.add_workspace(&workspace).unwrap();
            store.add_approval(&approval).unwrap();
            store.add_verification_run(&verification_run).unwrap();
            store.add_artifact(&artifact).unwrap();
            original_record = store.get("t1").unwrap();
            original_events = store.event_journal(0).unwrap();
            assert_eq!(original_events.len(), 1);
        }

        let reopened = SqliteStore::new(&path).unwrap();
        assert_eq!(reopened.get_project("p1").unwrap(), project);
        assert_eq!(reopened.list_projects().unwrap(), vec![project]);
        assert_eq!(reopened.get("t1").unwrap(), original_record);
        assert_eq!(reopened.workspaces_for_task("t1").unwrap(), vec![workspace]);
        assert_eq!(reopened.approvals_for_task("t1").unwrap(), vec![approval]);
        assert_eq!(
            reopened.verification_runs_for_task("t1").unwrap(),
            vec![verification_run]
        );
        assert_eq!(reopened.artifacts_for_task("t1").unwrap(), vec![artifact]);
        assert_eq!(reopened.event_journal(0).unwrap(), original_events);

        reopened
            .transition("t1", TaskEvent::ExecutionStarted)
            .unwrap();
        let next = reopened.event_journal(original_events[0].sequence).unwrap();
        assert_eq!(next.len(), 1);
        assert!(next[0].sequence > original_events[0].sequence);
    }

    #[test]
    fn separate_handles_serialize_writes_or_return_database_errors() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("store.sqlite");
        let setup = SqliteStore::new(&path).unwrap();
        setup.register_project(&project()).unwrap();
        drop(setup);

        let first = SqliteStore::new(&path).unwrap();
        let second = SqliteStore::new(&path).unwrap();
        let start = Arc::new(Barrier::new(3));
        let first_start = Arc::clone(&start);
        let first_thread = std::thread::spawn(move || {
            first_start.wait();
            first.create(&contract("t1"))
        });
        let second_start = Arc::clone(&start);
        let second_thread = std::thread::spawn(move || {
            second_start.wait();
            second.create(&contract("t2"))
        });
        start.wait();

        let results = [first_thread.join().unwrap(), second_thread.join().unwrap()];
        assert!(results.iter().any(Result::is_ok));
        for error in results.iter().filter_map(|result| result.as_ref().err()) {
            assert!(
                matches!(error, StoreError::Database(_)),
                "unexpected error: {error:?}"
            );
        }
        let store = SqliteStore::new(&path).unwrap();
        assert_eq!(
            store.list().unwrap().len(),
            results.iter().filter(|r| r.is_ok()).count()
        );
    }
}
