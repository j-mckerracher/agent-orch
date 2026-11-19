//! Authority records (DPES-1108 phase 5, migrations 0065 and 0066).
//!
//! - A task the orchestrator started on its own cites its origin: the user turn whose message
//!   asked for it, or the open routed request a coordinator's task serves (DPES-1108
//!   integration). The store refuses an origin that is not a user turn, or a turn that read an
//!   agent's report, so a wake can never be the origin of new work; a routed request was made by
//!   the global orchestrator from a user's word, and is checked against the task's own
//!   `routed_request_id`.
//! - A one-task override records an explicit departure from a project default with the user's
//!   words. It is read only for its own task.
//! - A discard is the user's word that a task's committed candidate is not wanted. It is what
//!   lets the archive guard pass for a candidate that was never delivered. Nothing is deleted.
//!
//! Every row is written once and never changes. Writing the same row again returns it, so an
//! execution that reruns after a crash writes once.

use std::fmt;
use std::io;

use orchestrator_core::orchestrator_action::MAX_AUTONOMOUS_TASKS_PER_REQUEST;
use orchestrator_core::task::TaskState;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection};

use crate::sqlite_archive::ArchiveError;
use crate::sqlite_conversations::corrupt;
use crate::{SqliteStore, StoreError};

/// True (one row) when task `?1` has a committed candidate that is neither delivered nor
/// discarded. A candidate is an attempt that committed changed files, or a verification run of a
/// commit. Landed is a local review ref, a bound pull request, or a merge of it.
pub(crate) const UNLANDED_CANDIDATE_SQL: &str = "SELECT 1 WHERE \
    (EXISTS (SELECT 1 FROM attempts WHERE task_id = ?1 AND changed_files > 0) \
     OR EXISTS (SELECT 1 FROM verification_runs WHERE task_id = ?1 AND \"commit\" <> '')) \
    AND NOT EXISTS (SELECT 1 FROM delivery_steps WHERE task_id = ?1 \
        AND outcome IN ('review_ref_created', 'review_ref_already_matches')) \
    AND NOT EXISTS (SELECT 1 FROM pull_requests WHERE task_id = ?1) \
    AND NOT EXISTS (SELECT 1 FROM pull_request_merges WHERE task_id = ?1 AND state = 'merged') \
    AND NOT EXISTS (SELECT 1 FROM task_discards WHERE task_id = ?1)";

/// Most bytes of a user turn's message an override keeps. The message limit is 8 KiB.
const MAX_USER_WORDS_BYTES: usize = 16 * 1024;

/// What a task's origin names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginKind {
    /// A user turn: the user's own message in a conversation.
    UserTurn,
    /// An open routed request the task's coordinator serves (DPES-1108 phase 7).
    RoutedRequest,
}

impl OriginKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserTurn => "user_turn",
            Self::RoutedRequest => "routed_request",
        }
    }
}

/// The user instruction a task the orchestrator started on its own cites.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskOrigin {
    pub task_id: String,
    pub kind: OriginKind,
    /// The turn id for [`OriginKind::UserTurn`], the request id for
    /// [`OriginKind::RoutedRequest`].
    pub origin_ref: String,
    /// The proposal that made the task.
    pub proposal_id: String,
    pub created_at: String,
}

/// One explicit departure from a project default, for one task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskOverride {
    pub task_id: String,
    /// The proposal field that departs. Only `delivery` today.
    pub field: String,
    pub project_default: String,
    pub chosen: String,
    /// The message of the user turn that asked for it, verbatim.
    pub user_words: String,
    pub turn_id: String,
    pub created_at: String,
}

/// Why the store refused an authority record. Carried inside a `StoreError::Io` of kind
/// `InvalidInput`; [`AuthorityError::of`] reads it back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityError {
    /// The origin is not a user turn, or the turn read an agent's report.
    NotUserTurn,
    /// The routed request is not the one the task serves, or it is answered already.
    NotOpenRequest,
    /// The routed request is already the origin of
    /// [`MAX_AUTONOMOUS_TASKS_PER_REQUEST`] tasks.
    RequestTaskCap,
    /// The task already has a different origin or override for that field.
    AlreadyRecorded,
    /// An override field, value or words outside the table's rules.
    InvalidOverride,
}

impl fmt::Display for AuthorityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotUserTurn => "only a user turn that read no agent report can be an origin",
            Self::NotOpenRequest => "only the open routed request a task serves can be its origin",
            Self::RequestTaskCap => "the routed request already started its most tasks",
            Self::AlreadyRecorded => "the task already has a different record",
            Self::InvalidOverride => "the override is not valid",
        })
    }
}

impl std::error::Error for AuthorityError {}

impl From<AuthorityError> for StoreError {
    fn from(error: AuthorityError) -> Self {
        StoreError::Io(io::Error::new(io::ErrorKind::InvalidInput, error))
    }
}

impl AuthorityError {
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
    /// Records that `task_id` was started on its own from the user turn `turn_id`, by proposal
    /// `proposal_id`. Refused with [`AuthorityError::NotUserTurn`] unless the turn exists, a
    /// user message opened it and it read no agent report. The same record again returns it.
    pub fn record_task_origin(
        &self,
        task_id: &str,
        turn_id: &str,
        proposal_id: &str,
    ) -> Result<TaskOrigin, StoreError> {
        let (task_id, turn_id, proposal_id) = (
            task_id.to_owned(),
            turn_id.to_owned(),
            proposal_id.to_owned(),
        );
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            require_task(&mut transaction, &task_id).await?;
            let turn = sqlx::query_as::<_, (String, i64)>(
                "SELECT trigger_kind, read_report FROM conversation_turns WHERE id = ?",
            )
            .bind(&turn_id)
            .fetch_optional(&mut *transaction)
            .await?;
            if !matches!(&turn, Some((trigger, 0)) if trigger == "user") {
                return Err(AuthorityError::NotUserTurn.into());
            }
            let saved = write_origin(
                &mut transaction,
                &task_id,
                OriginKind::UserTurn,
                &turn_id,
                &proposal_id,
            )
            .await?;
            transaction.commit().await?;
            Ok(saved)
        })
    }

    /// Records that `task_id`, a coordinator's task, was started on its own for the routed
    /// request `request_id`, by proposal `proposal_id`. Refused with
    /// [`AuthorityError::NotOpenRequest`] unless the task's own `routed_request_id` is that
    /// request (written by `set_task_coordinator`, which checks the project) and the request has
    /// no answer yet, and with [`AuthorityError::RequestTaskCap`] once the request is the origin
    /// of [`MAX_AUTONOMOUS_TASKS_PER_REQUEST`] other tasks. The same record again returns it.
    pub fn record_task_routed_origin(
        &self,
        task_id: &str,
        request_id: &str,
        proposal_id: &str,
    ) -> Result<TaskOrigin, StoreError> {
        let (task_id, request_id, proposal_id) = (
            task_id.to_owned(),
            request_id.to_owned(),
            proposal_id.to_owned(),
        );
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            require_task(&mut transaction, &task_id).await?;
            let open = sqlx::query_scalar::<_, i64>(
                "SELECT 1 FROM tasks t WHERE t.id = ?1 AND t.routed_request_id = ?2 \
                 AND NOT EXISTS (SELECT 1 FROM coordinator_reports r \
                     WHERE r.request_id = ?2 AND r.verb = 'answer')",
            )
            .bind(&task_id)
            .bind(&request_id)
            .fetch_optional(&mut *transaction)
            .await?;
            if open.is_none() {
                return Err(AuthorityError::NotOpenRequest.into());
            }
            let others = routed_origins(&mut transaction, &request_id, &task_id).await?;
            if others >= MAX_AUTONOMOUS_TASKS_PER_REQUEST {
                return Err(AuthorityError::RequestTaskCap.into());
            }
            let saved = write_origin(
                &mut transaction,
                &task_id,
                OriginKind::RoutedRequest,
                &request_id,
                &proposal_id,
            )
            .await?;
            transaction.commit().await?;
            Ok(saved)
        })
    }

    /// How many tasks the routed request `request_id` is the origin of.
    pub fn routed_request_origin_count(&self, request_id: &str) -> Result<usize, StoreError> {
        let request_id = request_id.to_owned();
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            routed_origins(&mut connection, &request_id, "").await
        })
    }

    /// The origin of a task the orchestrator started on its own, if it has one.
    pub fn task_origin(&self, task_id: &str) -> Result<Option<TaskOrigin>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            read_origin(&mut connection, &task_id).await
        })
    }

    /// Records one departure from a project default for `record.task_id`. The field must be
    /// `delivery`, the values must differ, and the words must be 1 to 16 KiB. The same record
    /// again returns it; a different one for the same field is refused.
    pub fn record_task_override(&self, record: &TaskOverride) -> Result<TaskOverride, StoreError> {
        let valid = record.field == "delivery"
            && record.project_default != record.chosen
            && (1..=64).contains(&record.project_default.len())
            && (1..=64).contains(&record.chosen.len())
            && (1..=MAX_USER_WORDS_BYTES).contains(&record.user_words.len())
            && (1..=128).contains(&record.turn_id.len());
        if !valid {
            return Err(AuthorityError::InvalidOverride.into());
        }
        let record = record.clone();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            require_task(&mut transaction, &record.task_id).await?;
            let existing = read_overrides(&mut transaction, &record.task_id)
                .await?
                .into_iter()
                .find(|existing| existing.field == record.field);
            if let Some(existing) = existing {
                let same = existing.project_default == record.project_default
                    && existing.chosen == record.chosen
                    && existing.user_words == record.user_words
                    && existing.turn_id == record.turn_id;
                return if same {
                    Ok(existing)
                } else {
                    Err(AuthorityError::AlreadyRecorded.into())
                };
            }
            sqlx::query(
                "INSERT INTO task_overrides \
                 (task_id, field, project_default, chosen, user_words, turn_id) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&record.task_id)
            .bind(&record.field)
            .bind(&record.project_default)
            .bind(&record.chosen)
            .bind(&record.user_words)
            .bind(&record.turn_id)
            .execute(&mut *transaction)
            .await?;
            let saved = read_overrides(&mut transaction, &record.task_id)
                .await?
                .into_iter()
                .find(|saved| saved.field == record.field)
                .ok_or_else(|| corrupt("the task override just written is missing".to_owned()))?;
            transaction.commit().await?;
            Ok(saved)
        })
    }

    /// The overrides of one task, by field. Nothing reads them for another task.
    pub fn task_overrides(&self, task_id: &str) -> Result<Vec<TaskOverride>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            read_overrides(&mut connection, &task_id).await
        })
    }

    /// Records the user's discard of a finished task's candidate. Returns `true` when this call
    /// recorded it and `false` when it was already recorded. A task that is not `Succeeded`,
    /// `Failed`, `VerificationFailed` or `Cancelled` is refused with
    /// [`ArchiveError::NotFinished`]; an unknown one fails with `RecordNotFound`. Nothing on
    /// disk changes: the task's branch stays.
    pub fn discard_task_candidate(&self, task_id: &str) -> Result<bool, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let state: String = sqlx::query_scalar("SELECT state FROM tasks WHERE id = ?")
                .bind(&task_id)
                .fetch_optional(&mut *transaction)
                .await?
                .ok_or_else(|| StoreError::RecordNotFound {
                    entity: "task",
                    id: task_id.clone(),
                })?;
            let state: TaskState = serde_json::from_str(&state)?;
            if !finished(state) {
                return Err(StoreError::from(ArchiveError::NotFinished));
            }
            let written = sqlx::query(
                "INSERT INTO task_discards (task_id, discarded_by) VALUES (?, 'user') \
                 ON CONFLICT(task_id) DO NOTHING",
            )
            .bind(&task_id)
            .execute(&mut *transaction)
            .await?
            .rows_affected()
                > 0;
            transaction.commit().await?;
            Ok(written)
        })
    }

    /// Whether the user discarded the task's candidate.
    pub fn task_discarded(&self, task_id: &str) -> Result<bool, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            Ok(
                sqlx::query_scalar::<_, i64>("SELECT 1 FROM task_discards WHERE task_id = ?")
                    .bind(&task_id)
                    .fetch_optional(&pool)
                    .await?
                    .is_some(),
            )
        })
    }

    /// Whether the task has a committed candidate that is not merged, not delivered as a pull
    /// request or a local review ref, and not discarded by the user. While it does, the task is
    /// not archived and its slot is not returned.
    pub fn has_unlanded_candidate(&self, task_id: &str) -> Result<bool, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            unlanded(&mut connection, &task_id).await
        })
    }
}

/// [`SqliteStore::has_unlanded_candidate`] in the caller's connection or transaction.
pub(crate) async fn unlanded(
    connection: &mut SqliteConnection,
    task_id: &str,
) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar::<_, i64>(UNLANDED_CANDIDATE_SQL)
        .bind(task_id)
        .fetch_optional(connection)
        .await?
        .is_some())
}

fn finished(state: TaskState) -> bool {
    matches!(
        state,
        TaskState::Succeeded
            | TaskState::Failed
            | TaskState::VerificationFailed
            | TaskState::Cancelled
    )
}

/// How many tasks other than `task_id` cite the routed request `request_id` as their origin.
async fn routed_origins(
    connection: &mut SqliteConnection,
    request_id: &str,
    task_id: &str,
) -> Result<usize, StoreError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_origins \
         WHERE origin_kind = 'routed_request' AND origin_ref = ? AND task_id <> ?",
    )
    .bind(request_id)
    .bind(task_id)
    .fetch_one(connection)
    .await?;
    Ok(usize::try_from(count).unwrap_or(usize::MAX))
}

async fn require_task(connection: &mut SqliteConnection, task_id: &str) -> Result<(), StoreError> {
    let found = sqlx::query_scalar::<_, i64>("SELECT 1 FROM tasks WHERE id = ?")
        .bind(task_id)
        .fetch_optional(connection)
        .await?;
    match found {
        Some(_) => Ok(()),
        None => Err(StoreError::RecordNotFound {
            entity: "task",
            id: task_id.to_owned(),
        }),
    }
}

/// Writes a task's one origin, or returns the same one written before. A different origin for
/// the task is refused with [`AuthorityError::AlreadyRecorded`].
async fn write_origin(
    connection: &mut SqliteConnection,
    task_id: &str,
    kind: OriginKind,
    origin_ref: &str,
    proposal_id: &str,
) -> Result<TaskOrigin, StoreError> {
    if let Some(existing) = read_origin(connection, task_id).await? {
        return if existing.kind == kind
            && existing.origin_ref == origin_ref
            && existing.proposal_id == proposal_id
        {
            Ok(existing)
        } else {
            Err(AuthorityError::AlreadyRecorded.into())
        };
    }
    sqlx::query(
        "INSERT INTO task_origins (task_id, origin_kind, origin_ref, proposal_id) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(task_id)
    .bind(kind.as_str())
    .bind(origin_ref)
    .bind(proposal_id)
    .execute(&mut *connection)
    .await?;
    read_origin(connection, task_id)
        .await?
        .ok_or_else(|| corrupt("the task origin just written is missing".to_owned()))
}

async fn read_origin(
    connection: &mut SqliteConnection,
    task_id: &str,
) -> Result<Option<TaskOrigin>, StoreError> {
    sqlx::query(
        "SELECT task_id, origin_kind, origin_ref, proposal_id, created_at \
         FROM task_origins WHERE task_id = ?",
    )
    .bind(task_id)
    .fetch_optional(connection)
    .await?
    .map(|row| origin_from_row(&row))
    .transpose()
}

fn origin_from_row(row: &SqliteRow) -> Result<TaskOrigin, StoreError> {
    let kind: String = row.try_get("origin_kind")?;
    let kind = match kind.as_str() {
        "user_turn" => OriginKind::UserTurn,
        "routed_request" => OriginKind::RoutedRequest,
        other => return Err(corrupt(format!("unknown task origin kind {other}"))),
    };
    Ok(TaskOrigin {
        task_id: row.try_get("task_id")?,
        kind,
        origin_ref: row.try_get("origin_ref")?,
        proposal_id: row.try_get("proposal_id")?,
        created_at: row.try_get("created_at")?,
    })
}

async fn read_overrides(
    connection: &mut SqliteConnection,
    task_id: &str,
) -> Result<Vec<TaskOverride>, StoreError> {
    let rows = sqlx::query(
        "SELECT task_id, field, project_default, chosen, user_words, turn_id, created_at \
         FROM task_overrides WHERE task_id = ? ORDER BY field",
    )
    .bind(task_id)
    .fetch_all(connection)
    .await?;
    rows.iter()
        .map(|row| {
            Ok(TaskOverride {
                task_id: row.try_get("task_id")?,
                field: row.try_get("field")?,
                project_default: row.try_get("project_default")?,
                chosen: row.try_get("chosen")?,
                user_words: row.try_get("user_words")?,
                turn_id: row.try_get("turn_id")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect()
}
