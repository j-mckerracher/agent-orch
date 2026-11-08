//! SQLite persistence operations for orchestrator domain records.
//!
//! Task transitions use the core state table and commit state plus audit event
//! in one transaction. Other records are append-only. Updated: 2026-11-08T16:51:19+00:00.

use orchestrator_core::authorization::Approval;
use orchestrator_core::contract::{validate as validate_contract, TaskContract};
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
