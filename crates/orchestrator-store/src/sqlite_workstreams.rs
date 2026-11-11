//! SQLite persistence for workstreams, objectives, and task links (M7-07).
//!
//! Every write runs in one `BEGIN IMMEDIATE` transaction that also appends the
//! new revision snapshot, so a record and its audit row commit together. Writers
//! present the revision they read; a different stored revision is a
//! [`WorkstreamStoreError::Stale`] rejection and nothing is written.
//!
//! Linking a task never changes the task, its contract, or its state. Project
//! membership of a workstream is derived from its linked tasks.
//!
//! No durable event is emitted for workstream changes: `durable_events` is
//! task-scoped and this slice does not alter existing tables.

use std::fmt;
use std::future::Future;

use orchestrator_core::task::TaskState;
use orchestrator_core::workstream::{
    AcceptanceGate, GateState, Objective, Workstream, WorkstreamRuleError, WorkstreamState,
    INITIAL_REVISION,
};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::{SqliteStore, StoreError};

/// A workstream mutation failed.
#[derive(Debug)]
pub enum WorkstreamStoreError {
    /// The domain rules rejected the change.
    Rule(WorkstreamRuleError),
    /// The caller's revision is not the stored revision.
    Stale {
        entity: &'static str,
        id: String,
        expected: i64,
        actual: i64,
    },
    NotFound {
        entity: &'static str,
        id: String,
    },
    AlreadyExists {
        entity: &'static str,
        id: String,
    },
    /// The task already serves a different workstream or objective.
    TaskAlreadyLinked {
        task_id: String,
        workstream_id: String,
        objective_id: Option<String>,
    },
    /// The objective belongs to another workstream.
    ObjectiveMismatch {
        objective_id: String,
        workstream_id: String,
    },
    /// Gate evidence must be a task linked to the same workstream.
    EvidenceNotLinked {
        task_id: String,
    },
    /// A gate cites the task as evidence, so it cannot be unlinked.
    TaskIsGateEvidence {
        task_id: String,
    },
    Store(StoreError),
}

impl fmt::Display for WorkstreamStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rule(error) => write!(formatter, "{error}"),
            Self::Stale {
                entity,
                id,
                expected,
                actual,
            } => write!(
                formatter,
                "stale revision for {entity} {id}: expected {expected}, stored {actual}"
            ),
            Self::NotFound { entity, id } => write!(formatter, "{entity} {id} not found"),
            Self::AlreadyExists { entity, id } => {
                write!(formatter, "{entity} {id} already exists")
            }
            Self::TaskAlreadyLinked {
                task_id,
                workstream_id,
                ..
            } => write!(
                formatter,
                "task {task_id} already serves workstream {workstream_id}"
            ),
            Self::ObjectiveMismatch {
                objective_id,
                workstream_id,
            } => write!(
                formatter,
                "objective {objective_id} does not belong to workstream {workstream_id}"
            ),
            Self::EvidenceNotLinked { task_id } => write!(
                formatter,
                "evidence task {task_id} is not linked to this workstream"
            ),
            Self::TaskIsGateEvidence { task_id } => {
                write!(formatter, "task {task_id} is cited as gate evidence")
            }
            Self::Store(error) => write!(formatter, "store: {error}"),
        }
    }
}

impl std::error::Error for WorkstreamStoreError {}

impl From<WorkstreamRuleError> for WorkstreamStoreError {
    fn from(error: WorkstreamRuleError) -> Self {
        Self::Rule(error)
    }
}

impl From<StoreError> for WorkstreamStoreError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<sqlx::Error> for WorkstreamStoreError {
    fn from(error: sqlx::Error) -> Self {
        Self::Store(error.into())
    }
}

impl From<serde_json::Error> for WorkstreamStoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::Store(error.into())
    }
}

/// Link between one task and the workstream (and optional objective) it serves.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WorkstreamTask {
    pub task_id: String,
    pub workstream_id: String,
    pub objective_id: Option<String>,
    /// Owning project, read from the task. The task is not modified.
    pub project_id: String,
    /// Current task state, read from the task.
    pub task_state: TaskState,
}

/// One audited revision snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkstreamRevision {
    pub revision: i64,
    pub snapshot: serde_json::Value,
    pub actor: String,
    pub created_at: String,
}

type WResult<T> = Result<T, WorkstreamStoreError>;

impl SqliteStore {
    /// Runs one workstream operation on the store worker.
    fn wcall<T, F, Fut>(&self, operation: F) -> WResult<T>
    where
        T: Send + 'static,
        F: FnOnce(SqlitePool) -> Fut + Send + 'static,
        Fut: Future<Output = WResult<T>> + Send + 'static,
    {
        self.call(move |pool| async move { Ok(operation(pool).await) })?
    }

    /// Creates a workstream at its initial revision.
    pub fn create_workstream(&self, workstream: &Workstream, actor: &str) -> WResult<Workstream> {
        workstream.validate()?;
        if workstream.revision != INITIAL_REVISION {
            return Err(stale_new("workstream", &workstream.id, workstream.revision));
        }
        let (workstream, actor) = (workstream.clone(), actor.to_owned());
        self.wcall(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            if workstream_exists(&mut transaction, &workstream.id).await? {
                return Err(WorkstreamStoreError::AlreadyExists {
                    entity: "workstream",
                    id: workstream.id,
                });
            }
            sqlx::query(
                "INSERT INTO workstreams (id, title, summary, state, revision) \
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&workstream.id)
            .bind(&workstream.title)
            .bind(&workstream.summary)
            .bind(workstream.state.tag())
            .bind(workstream.revision)
            .execute(&mut *transaction)
            .await?;
            record_revision(
                &mut transaction,
                "workstream",
                &workstream.id,
                workstream.revision,
                &serde_json::to_string(&workstream)?,
                &actor,
            )
            .await?;
            transaction.commit().await?;
            Ok(workstream)
        })
    }

    /// Loads one workstream.
    pub fn get_workstream(&self, id: &str) -> WResult<Workstream> {
        let id = id.to_owned();
        self.wcall(move |pool| async move {
            let mut connection = pool.acquire().await?;
            load_workstream(&mut connection, &id).await
        })
    }

    /// Lists workstreams in creation order, optionally of one state.
    pub fn list_workstreams(&self, state: Option<WorkstreamState>) -> WResult<Vec<Workstream>> {
        self.wcall(move |pool| async move {
            let rows = sqlx::query(
                "SELECT id, title, summary, state, revision FROM workstreams \
                 WHERE (?1 IS NULL OR state = ?1) ORDER BY sequence",
            )
            .bind(state.map(WorkstreamState::tag))
            .fetch_all(&pool)
            .await?;
            rows.iter().map(decode_workstream).collect()
        })
    }

    /// Updates a workstream's title, summary, and/or state.
    pub fn update_workstream(
        &self,
        id: &str,
        expected_revision: i64,
        title: Option<&str>,
        summary: Option<&str>,
        state: Option<WorkstreamState>,
        actor: &str,
    ) -> WResult<Workstream> {
        let (id, actor) = (id.to_owned(), actor.to_owned());
        let title = title.map(str::to_owned);
        let summary = summary.map(str::to_owned);
        self.wcall(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let current = load_workstream(&mut transaction, &id).await?;
            if current.revision != expected_revision {
                return Err(WorkstreamStoreError::Stale {
                    entity: "workstream",
                    id,
                    expected: expected_revision,
                    actual: current.revision,
                });
            }
            let next = current.apply_update(title.as_deref(), summary.as_deref(), state)?;
            sqlx::query(
                "UPDATE workstreams SET title = ?, summary = ?, state = ?, revision = ?, \
                 updated_at = CURRENT_TIMESTAMP WHERE id = ? AND revision = ?",
            )
            .bind(&next.title)
            .bind(&next.summary)
            .bind(next.state.tag())
            .bind(next.revision)
            .bind(&next.id)
            .bind(current.revision)
            .execute(&mut *transaction)
            .await?;
            record_revision(
                &mut transaction,
                "workstream",
                &next.id,
                next.revision,
                &serde_json::to_string(&next)?,
                &actor,
            )
            .await?;
            transaction.commit().await?;
            Ok(next)
        })
    }

    /// Adds an objective to an unarchived workstream.
    pub fn add_objective(&self, objective: &Objective, actor: &str) -> WResult<Objective> {
        objective.validate()?;
        if objective.revision != INITIAL_REVISION {
            return Err(stale_new("objective", &objective.id, objective.revision));
        }
        let (objective, actor) = (objective.clone(), actor.to_owned());
        self.wcall(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let workstream = load_workstream(&mut transaction, &objective.workstream_id).await?;
            require_unarchived(&workstream)?;
            if objective_exists(&mut transaction, &objective.id).await? {
                return Err(WorkstreamStoreError::AlreadyExists {
                    entity: "objective",
                    id: objective.id,
                });
            }
            sqlx::query(
                "INSERT INTO objectives \
                 (id, workstream_id, title, description, gates_json, revision) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&objective.id)
            .bind(&objective.workstream_id)
            .bind(&objective.title)
            .bind(&objective.description)
            .bind(serde_json::to_string(&objective.gates)?)
            .bind(objective.revision)
            .execute(&mut *transaction)
            .await?;
            record_revision(
                &mut transaction,
                "objective",
                &objective.id,
                objective.revision,
                &serde_json::to_string(&objective)?,
                &actor,
            )
            .await?;
            transaction.commit().await?;
            Ok(objective)
        })
    }

    /// Loads one objective.
    pub fn get_objective(&self, id: &str) -> WResult<Objective> {
        let id = id.to_owned();
        self.wcall(move |pool| async move {
            let mut connection = pool.acquire().await?;
            load_objective(&mut connection, &id).await
        })
    }

    /// Lists one workstream's objectives in creation order.
    pub fn list_objectives(&self, workstream_id: &str) -> WResult<Vec<Objective>> {
        let workstream_id = workstream_id.to_owned();
        self.wcall(move |pool| async move {
            let mut connection = pool.acquire().await?;
            load_workstream(&mut connection, &workstream_id).await?;
            let rows = sqlx::query(OBJECTIVE_SELECT_BY_WORKSTREAM)
                .bind(&workstream_id)
                .fetch_all(&mut *connection)
                .await?;
            rows.iter().map(decode_objective).collect()
        })
    }

    /// Retitles an objective and/or appends gates. Existing gates are kept.
    pub fn revise_objective(
        &self,
        id: &str,
        expected_revision: i64,
        title: Option<&str>,
        description: Option<&str>,
        new_gates: &[AcceptanceGate],
        actor: &str,
    ) -> WResult<Objective> {
        let (id, actor) = (id.to_owned(), actor.to_owned());
        let title = title.map(str::to_owned);
        let description = description.map(str::to_owned);
        let new_gates = new_gates.to_vec();
        self.wcall(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let current = load_objective(&mut transaction, &id).await?;
            check_objective_revision(&current, expected_revision)?;
            let workstream = load_workstream(&mut transaction, &current.workstream_id).await?;
            require_unarchived(&workstream)?;
            let next =
                current.apply_revision(title.as_deref(), description.as_deref(), &new_gates)?;
            for gate in &next.gates[current.gates.len()..] {
                if let Some(task_id) = &gate.evidence_task_id {
                    require_linked(&mut transaction, task_id, &next.workstream_id).await?;
                }
            }
            save_objective(&mut transaction, &current, &next, &actor).await?;
            transaction.commit().await?;
            Ok(next)
        })
    }

    /// Moves one gate to a new state, optionally citing a linked task.
    pub fn set_gate_state(
        &self,
        objective_id: &str,
        expected_revision: i64,
        gate_id: &str,
        state: GateState,
        evidence_task_id: Option<&str>,
        actor: &str,
    ) -> WResult<Objective> {
        let (objective_id, gate_id, actor) = (
            objective_id.to_owned(),
            gate_id.to_owned(),
            actor.to_owned(),
        );
        let evidence_task_id = evidence_task_id.map(str::to_owned);
        self.wcall(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let current = load_objective(&mut transaction, &objective_id).await?;
            check_objective_revision(&current, expected_revision)?;
            let workstream = load_workstream(&mut transaction, &current.workstream_id).await?;
            require_unarchived(&workstream)?;
            let next = current.apply_gate_state(&gate_id, state, evidence_task_id.as_deref())?;
            if let Some(task_id) = &evidence_task_id {
                require_linked(&mut transaction, task_id, &next.workstream_id).await?;
            }
            save_objective(&mut transaction, &current, &next, &actor).await?;
            transaction.commit().await?;
            Ok(next)
        })
    }

    /// Links a task to a workstream, optionally to one of its objectives.
    ///
    /// Repeating the same link is a no-op. A task serves at most one target;
    /// moving it requires [`Self::unlink_task`] first. Does not bump any
    /// revision and never touches the task.
    pub fn link_task(
        &self,
        workstream_id: &str,
        task_id: &str,
        objective_id: Option<&str>,
    ) -> WResult<WorkstreamTask> {
        let (workstream_id, task_id) = (workstream_id.to_owned(), task_id.to_owned());
        let objective_id = objective_id.map(str::to_owned);
        self.wcall(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let workstream = load_workstream(&mut transaction, &workstream_id).await?;
            let task_exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE id = ?")
                .bind(&task_id)
                .fetch_one(&mut *transaction)
                .await?;
            if task_exists == 0 {
                return Err(WorkstreamStoreError::NotFound {
                    entity: "task",
                    id: task_id,
                });
            }
            if let Some(existing) = load_link(&mut transaction, &task_id).await? {
                if existing.workstream_id == workstream_id && existing.objective_id == objective_id
                {
                    return Ok(existing);
                }
                return Err(WorkstreamStoreError::TaskAlreadyLinked {
                    task_id,
                    workstream_id: existing.workstream_id,
                    objective_id: existing.objective_id,
                });
            }
            require_unarchived(&workstream)?;
            if let Some(objective_id) = &objective_id {
                let objective = load_objective(&mut transaction, objective_id).await?;
                if objective.workstream_id != workstream_id {
                    return Err(WorkstreamStoreError::ObjectiveMismatch {
                        objective_id: objective_id.clone(),
                        workstream_id,
                    });
                }
            }
            sqlx::query(
                "INSERT INTO workstream_tasks (task_id, workstream_id, objective_id) \
                 VALUES (?, ?, ?)",
            )
            .bind(&task_id)
            .bind(&workstream_id)
            .bind(&objective_id)
            .execute(&mut *transaction)
            .await?;
            let link = load_link(&mut transaction, &task_id).await?;
            transaction.commit().await?;
            link.ok_or(WorkstreamStoreError::NotFound {
                entity: "task link",
                id: task_id,
            })
        })
    }

    /// Removes a task's link. Returns whether a link was removed; an absent
    /// link is not an error so a retry is harmless.
    pub fn unlink_task(&self, workstream_id: &str, task_id: &str) -> WResult<bool> {
        let (workstream_id, task_id) = (workstream_id.to_owned(), task_id.to_owned());
        self.wcall(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let workstream = load_workstream(&mut transaction, &workstream_id).await?;
            match load_link(&mut transaction, &task_id).await? {
                Some(link) if link.workstream_id == workstream_id => {}
                _ => return Ok(false),
            }
            require_unarchived(&workstream)?;
            let rows = sqlx::query(OBJECTIVE_SELECT_BY_WORKSTREAM)
                .bind(&workstream_id)
                .fetch_all(&mut *transaction)
                .await?;
            for row in &rows {
                let objective = decode_objective(row)?;
                if objective
                    .gates
                    .iter()
                    .any(|gate| gate.evidence_task_id.as_deref() == Some(task_id.as_str()))
                {
                    return Err(WorkstreamStoreError::TaskIsGateEvidence { task_id });
                }
            }
            sqlx::query("DELETE FROM workstream_tasks WHERE task_id = ?")
                .bind(&task_id)
                .execute(&mut *transaction)
                .await?;
            transaction.commit().await?;
            Ok(true)
        })
    }

    /// Lists a workstream's linked tasks with their owning project and state.
    pub fn list_workstream_tasks(&self, workstream_id: &str) -> WResult<Vec<WorkstreamTask>> {
        let workstream_id = workstream_id.to_owned();
        self.wcall(move |pool| async move {
            let mut connection = pool.acquire().await?;
            load_workstream(&mut connection, &workstream_id).await?;
            let rows = sqlx::query(LINK_SELECT_BY_WORKSTREAM)
                .bind(&workstream_id)
                .fetch_all(&mut *connection)
                .await?;
            rows.iter().map(decode_link).collect()
        })
    }

    /// Finds the workstream a task serves, if any.
    pub fn workstream_task_link(&self, task_id: &str) -> WResult<Option<WorkstreamTask>> {
        let task_id = task_id.to_owned();
        self.wcall(move |pool| async move {
            let mut connection = pool.acquire().await?;
            load_link(&mut connection, &task_id).await
        })
    }

    /// Lists the audit snapshots of one workstream or objective, oldest first.
    pub fn workstream_revisions(
        &self,
        entity_kind: &str,
        entity_id: &str,
    ) -> WResult<Vec<WorkstreamRevision>> {
        let (entity_kind, entity_id) = (entity_kind.to_owned(), entity_id.to_owned());
        self.wcall(move |pool| async move {
            let rows = sqlx::query(
                "SELECT revision, snapshot_json, actor, created_at FROM workstream_revisions \
                 WHERE entity_kind = ? AND entity_id = ? ORDER BY revision",
            )
            .bind(entity_kind)
            .bind(entity_id)
            .fetch_all(&pool)
            .await?;
            rows.iter()
                .map(|row| {
                    Ok(WorkstreamRevision {
                        revision: row.try_get("revision")?,
                        snapshot: serde_json::from_str(
                            &row.try_get::<String, _>("snapshot_json")?,
                        )?,
                        actor: row.try_get("actor")?,
                        created_at: row.try_get("created_at")?,
                    })
                })
                .collect()
        })
    }
}

const OBJECTIVE_SELECT_BY_ID: &str =
    "SELECT id, workstream_id, title, description, gates_json, revision FROM objectives \
     WHERE id = ?";
const OBJECTIVE_SELECT_BY_WORKSTREAM: &str =
    "SELECT id, workstream_id, title, description, gates_json, revision FROM objectives \
     WHERE workstream_id = ? ORDER BY sequence";
const LINK_SELECT_BY_TASK: &str = "SELECT l.task_id, l.workstream_id, l.objective_id, \
     t.project_id, t.state AS task_state FROM workstream_tasks l \
     JOIN tasks t ON t.id = l.task_id WHERE l.task_id = ?";
const LINK_SELECT_BY_WORKSTREAM: &str = "SELECT l.task_id, l.workstream_id, l.objective_id, \
     t.project_id, t.state AS task_state FROM workstream_tasks l \
     JOIN tasks t ON t.id = l.task_id WHERE l.workstream_id = ? ORDER BY l.task_id";

fn stale_new(entity: &'static str, id: &str, revision: i64) -> WorkstreamStoreError {
    WorkstreamStoreError::Stale {
        entity,
        id: id.to_owned(),
        expected: INITIAL_REVISION,
        actual: revision,
    }
}

fn require_unarchived(workstream: &Workstream) -> WResult<()> {
    if workstream.state == WorkstreamState::Archived {
        return Err(WorkstreamRuleError::WorkstreamArchived.into());
    }
    Ok(())
}

fn check_objective_revision(current: &Objective, expected: i64) -> WResult<()> {
    if current.revision != expected {
        return Err(WorkstreamStoreError::Stale {
            entity: "objective",
            id: current.id.clone(),
            expected,
            actual: current.revision,
        });
    }
    Ok(())
}

async fn workstream_exists(connection: &mut SqliteConnection, id: &str) -> WResult<bool> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workstreams WHERE id = ?")
        .bind(id)
        .fetch_one(connection)
        .await?;
    Ok(count > 0)
}

async fn objective_exists(connection: &mut SqliteConnection, id: &str) -> WResult<bool> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM objectives WHERE id = ?")
        .bind(id)
        .fetch_one(connection)
        .await?;
    Ok(count > 0)
}

async fn load_workstream(connection: &mut SqliteConnection, id: &str) -> WResult<Workstream> {
    let row =
        sqlx::query("SELECT id, title, summary, state, revision FROM workstreams WHERE id = ?")
            .bind(id)
            .fetch_optional(connection)
            .await?
            .ok_or_else(|| WorkstreamStoreError::NotFound {
                entity: "workstream",
                id: id.to_owned(),
            })?;
    decode_workstream(&row)
}

async fn load_objective(connection: &mut SqliteConnection, id: &str) -> WResult<Objective> {
    let row = sqlx::query(OBJECTIVE_SELECT_BY_ID)
        .bind(id)
        .fetch_optional(connection)
        .await?
        .ok_or_else(|| WorkstreamStoreError::NotFound {
            entity: "objective",
            id: id.to_owned(),
        })?;
    decode_objective(&row)
}

async fn load_link(
    connection: &mut SqliteConnection,
    task_id: &str,
) -> WResult<Option<WorkstreamTask>> {
    let row = sqlx::query(LINK_SELECT_BY_TASK)
        .bind(task_id)
        .fetch_optional(connection)
        .await?;
    row.as_ref().map(decode_link).transpose()
}

async fn require_linked(
    connection: &mut SqliteConnection,
    task_id: &str,
    workstream_id: &str,
) -> WResult<()> {
    match load_link(connection, task_id).await? {
        Some(link) if link.workstream_id == workstream_id => Ok(()),
        _ => Err(WorkstreamStoreError::EvidenceNotLinked {
            task_id: task_id.to_owned(),
        }),
    }
}

async fn save_objective(
    connection: &mut SqliteConnection,
    current: &Objective,
    next: &Objective,
    actor: &str,
) -> WResult<()> {
    sqlx::query(
        "UPDATE objectives SET title = ?, description = ?, gates_json = ?, revision = ?, \
         updated_at = CURRENT_TIMESTAMP WHERE id = ? AND revision = ?",
    )
    .bind(&next.title)
    .bind(&next.description)
    .bind(serde_json::to_string(&next.gates)?)
    .bind(next.revision)
    .bind(&next.id)
    .bind(current.revision)
    .execute(&mut *connection)
    .await?;
    record_revision(
        connection,
        "objective",
        &next.id,
        next.revision,
        &serde_json::to_string(next)?,
        actor,
    )
    .await
}

async fn record_revision(
    connection: &mut SqliteConnection,
    kind: &str,
    id: &str,
    revision: i64,
    snapshot: &str,
    actor: &str,
) -> WResult<()> {
    sqlx::query(
        "INSERT INTO workstream_revisions (entity_kind, entity_id, revision, snapshot_json, actor) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(kind)
    .bind(id)
    .bind(revision)
    .bind(snapshot)
    .bind(actor)
    .execute(connection)
    .await?;
    Ok(())
}

fn corrupt(detail: String) -> WorkstreamStoreError {
    sqlx::Error::Decode(detail.into()).into()
}

fn decode_workstream(row: &SqliteRow) -> WResult<Workstream> {
    let state: String = row.try_get("state")?;
    Ok(Workstream {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        summary: row.try_get("summary")?,
        state: WorkstreamState::from_tag(&state)
            .ok_or_else(|| corrupt(format!("unknown workstream state {state:?}")))?,
        revision: row.try_get("revision")?,
    })
}

fn decode_objective(row: &SqliteRow) -> WResult<Objective> {
    Ok(Objective {
        id: row.try_get("id")?,
        workstream_id: row.try_get("workstream_id")?,
        title: row.try_get("title")?,
        description: row.try_get("description")?,
        gates: serde_json::from_str(&row.try_get::<String, _>("gates_json")?)?,
        revision: row.try_get("revision")?,
    })
}

fn decode_link(row: &SqliteRow) -> WResult<WorkstreamTask> {
    Ok(WorkstreamTask {
        task_id: row.try_get("task_id")?,
        workstream_id: row.try_get("workstream_id")?,
        objective_id: row.try_get("objective_id")?,
        project_id: row.try_get("project_id")?,
        task_state: serde_json::from_str(&row.try_get::<String, _>("task_state")?)?,
    })
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use orchestrator_core::contract::{
        AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
    };
    use sqlx::migrate::Migrator;
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

    use super::*;
    use crate::Store;

    /// Version of the migration that introduced workstreams.
    const WORKSTREAMS_MIGRATION: i64 = 30;

    static ALL_MIGRATIONS: Migrator = sqlx::migrate!("./migrations");

    /// Builds a data directory exactly as a pre-M7-07 build left it: only the
    /// migrations below 0030 applied, populated through raw SQL.
    fn legacy_database(path: &std::path::Path) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let options = SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal)
                .foreign_keys(true);
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(options)
                .await
                .unwrap();
            let legacy = Migrator {
                migrations: Cow::Owned(
                    ALL_MIGRATIONS
                        .iter()
                        .filter(|migration| migration.version < WORKSTREAMS_MIGRATION)
                        .cloned()
                        .collect(),
                ),
                ..Migrator::DEFAULT
            };
            legacy.run(&pool).await.unwrap();
            sqlx::query(
                "INSERT INTO projects (id, root_path, settings) VALUES ('p1', '/work/p1', '{}')",
            )
            .execute(&pool)
            .await
            .unwrap();
            for (id, parent) in [("t1", None), ("t2", Some("t1"))] {
                let contract = TaskContract {
                    id: id.into(),
                    project_id: "p1".into(),
                    parent_id: parent.map(str::to_owned),
                    objective: format!("legacy {id}"),
                    kind: TaskKind::Implement,
                    agent: "codex".into(),
                    dependencies: Vec::new(),
                    verification: VerificationPolicy::None,
                    delivery: DeliveryPolicy::LocalReview,
                    autonomy: AutonomyPolicy::WorkspaceWrites,
                    max_retries: 0,
                    max_repairs: 0,
                    model_selection: None,
                };
                sqlx::query(
                    "INSERT INTO tasks (id, project_id, parent_id, state, contract_json, version) \
                     VALUES (?, 'p1', ?, '\"Created\"', ?, 1)",
                )
                .bind(id)
                .bind(parent)
                .bind(serde_json::to_string(&contract).unwrap())
                .execute(&pool)
                .await
                .unwrap();
            }
            sqlx::query(
                "INSERT INTO coordinator_entries (id, project_id, kind, author, body) \
                 VALUES ('e1', 'p1', 'message', 'user', 'hello')",
            )
            .execute(&pool)
            .await
            .unwrap();
            let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
                .fetch_one(&pool)
                .await
                .unwrap();
            let legacy_count = ALL_MIGRATIONS
                .iter()
                .filter(|migration| migration.version < WORKSTREAMS_MIGRATION)
                .count();
            assert_eq!(applied as usize, legacy_count);
            pool.close().await;
        });
    }

    #[test]
    fn existing_data_directory_migrates_without_loss() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("store.sqlite");
        legacy_database(&path);

        let store = SqliteStore::new(&path).unwrap();
        let versions: Vec<i64> = store
            .call(|pool| async move {
                Ok(sqlx::query_scalar(
                    "SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version",
                )
                .fetch_all(&pool)
                .await?)
            })
            .unwrap();
        assert!(versions.contains(&WORKSTREAMS_MIGRATION));
        assert_eq!(
            versions,
            ALL_MIGRATIONS
                .iter()
                .map(|migration| migration.version)
                .collect::<Vec<_>>()
        );

        assert_eq!(store.get_project("p1").unwrap().root_path, "/work/p1");
        let tasks = store.list().unwrap();
        assert_eq!(tasks.len(), 2);
        let child = store.get("t2").unwrap();
        assert_eq!(child.contract.objective, "legacy t2");
        assert_eq!(child.contract.parent_id.as_deref(), Some("t1"));
        assert_eq!(
            store.coordinator_entries_after("p1", 0, 10).unwrap()[0].body,
            "hello"
        );
        assert!(store.list_workstreams(None).unwrap().is_empty());

        store
            .create_workstream(&Workstream::new("w1", "Initiative", "").unwrap(), "user")
            .unwrap();
        let link = store.link_task("w1", "t2", None).unwrap();
        assert_eq!(link.project_id, "p1");
        assert_eq!(store.get("t2").unwrap(), child);
    }
}
