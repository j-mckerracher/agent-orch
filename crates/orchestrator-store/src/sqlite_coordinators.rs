//! Project coordinators as domain orchestrators (DPES-1108 phase 7, migration 0071).
//!
//! A coordinator owns the tasks made in its conversation and answers to the global orchestrator
//! only through `coordinator_reports`. Work reaches it as a routed request whose id is the
//! correlation id. The rules the migration's triggers hold are checked here first, so a refusal
//! is a typed [`CoordinatorError`] and not a bare database error:
//!
//! - work is routed only from the global conversation, to an active coordinator;
//! - a coordinator report comes only from that coordinator's own conversation;
//! - a routed request is open until a report with verb `answer` names it, and has one answer;
//! - a task's owner is its own project's coordinator, written once;
//! - a coordinator with open work cannot be removed (see `sqlite_history`).

use std::fmt;
use std::io;

use orchestrator_core::charter::{self, ReportVerb};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection};

use crate::sqlite_conversations::{corrupt, ConversationTarget};
use crate::sqlite_history::{coordinator_removed, require_project};
use crate::{SqliteStore, StoreError};

/// Task states that count as open work a coordinator owns, as stored (JSON text). A failed task
/// runs nothing until a retry, so it is not open.
const OPEN_TASK_STATES: &str =
    "('\"Created\"', '\"Queued\"', '\"Running\"', '\"Waiting\"', '\"Verifying\"')";

/// Why the store refused a coordinator write. Carried inside a `StoreError::Io` of kind
/// `InvalidInput`; [`CoordinatorError::of`] reads it back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoordinatorError {
    /// The project has no active coordinator.
    NotActive,
    /// The coordinator owns unfinished tasks or an unanswered routed request.
    OpenWork,
    /// The request is unknown, of another project, or already answered.
    RequestNotOpen,
    /// The proposal named as the origin is not an approved proposal of the conversation that may
    /// make this record.
    WrongOrigin,
    /// A text field breaks a rule of `orchestrator_core::charter`.
    BadText,
    /// The task is of another project, or already has another owner.
    WrongOwner,
}

impl fmt::Display for CoordinatorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotActive => "the project has no active coordinator",
            Self::OpenWork => "the coordinator owns unfinished work",
            Self::RequestNotOpen => "the routed request is not open in this project",
            Self::WrongOrigin => "the record does not come from a conversation that may make it",
            Self::BadText => "a coordinator text field is not accepted",
            Self::WrongOwner => "the task cannot be owned by this coordinator",
        })
    }
}

impl std::error::Error for CoordinatorError {}

impl From<CoordinatorError> for StoreError {
    fn from(error: CoordinatorError) -> Self {
        StoreError::Io(io::Error::new(io::ErrorKind::InvalidInput, error))
    }
}

impl CoordinatorError {
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

/// A durable fact the daemon writes for a coordinator-owned task. Never agent text: the daemon
/// renders each from its verb and the task when it is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoordinatorFact {
    /// The task left a candidate for review.
    TaskReady,
    /// The task left a report.
    ReportReady,
    TaskDone,
    TaskFailed,
    /// The task's worker is blocked and waits for help.
    WorkerBlocked,
    /// A question about the task waits for the user.
    QuestionRaised,
    /// The user answered or settled a question about the task.
    QuestionAnswered,
    /// The task's pull request merged.
    Merged,
}

impl CoordinatorFact {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TaskReady => "task_ready",
            Self::ReportReady => "report_ready",
            Self::TaskDone => "task_done",
            Self::TaskFailed => "task_failed",
            Self::WorkerBlocked => "worker_blocked",
            Self::QuestionRaised => "question_raised",
            Self::QuestionAnswered => "question_answered",
            Self::Merged => "merged",
        }
    }
}

/// One routed request. `answered` is whether a report with verb `answer` names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutedRequest {
    pub id: String,
    pub project_id: String,
    pub work: String,
    pub origin_proposal_id: String,
    pub answered: bool,
    pub created_at: String,
}

/// One report on the parent channel. `author` is `daemon` or `coordinator`; `verb` is a
/// [`CoordinatorFact`] name for the daemon and a [`ReportVerb`] name for the coordinator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoordinatorReport {
    pub id: String,
    pub project_id: String,
    pub author: String,
    pub verb: String,
    pub request_id: Option<String>,
    pub task_id: Option<String>,
    pub key: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
}

/// What the global orchestrator may know about one active coordinator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoordinatorSummary {
    pub project_id: String,
    pub scope_sentence: Option<String>,
    /// Owned tasks that are not finished, failed or cancelled.
    pub open_tasks: u32,
    /// Routed requests with no answer yet.
    pub open_requests: u32,
}

/// A durable fact to record. `id` is derived from the event that caused it, so recording it
/// again changes nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewCoordinatorFact {
    pub id: String,
    pub project_id: String,
    pub fact: CoordinatorFact,
    pub task_id: String,
    pub key: Option<String>,
}

/// The conversation id a project's coordinator writes proposals under.
fn project_scope(project_id: &str) -> String {
    ConversationTarget::Project(project_id.to_owned()).id()
}

fn decode_request(row: &SqliteRow) -> Result<RoutedRequest, StoreError> {
    Ok(RoutedRequest {
        id: row.try_get("id")?,
        project_id: row.try_get("project_id")?,
        work: row.try_get("work")?,
        origin_proposal_id: row.try_get("origin_proposal_id")?,
        answered: row.try_get::<i64, _>("answered")? == 1,
        created_at: crate::instant::instant(&row, "created_at")?,
    })
}

fn decode_report(row: &SqliteRow) -> Result<CoordinatorReport, StoreError> {
    Ok(CoordinatorReport {
        id: row.try_get("id")?,
        project_id: row.try_get("project_id")?,
        author: row.try_get("author")?,
        verb: row.try_get("verb")?,
        request_id: row.try_get("request_id")?,
        task_id: row.try_get("task_id")?,
        key: row.try_get("report_key")?,
        note: row.try_get("note")?,
        created_at: crate::instant::instant(&row, "created_at")?,
    })
}

const REQUEST_COLUMNS: &str = "r.id, r.project_id, r.work, r.origin_proposal_id, r.created_at, \
     EXISTS (SELECT 1 FROM coordinator_reports c WHERE c.request_id = r.id AND c.verb = 'answer') \
     AS answered";

const REPORT_COLUMNS: &str =
    "id, project_id, author, verb, request_id, task_id, report_key, note, created_at";

async fn load_request(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<Option<RoutedRequest>, StoreError> {
    let sql = format!("SELECT {REQUEST_COLUMNS} FROM routed_requests r WHERE r.id = ?");
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(connection)
        .await?
        .as_ref()
        .map(decode_request)
        .transpose()
}

/// Refuses unless `proposal_id` is an approved or executed proposal of `kind` in `scope`.
async fn require_origin(
    connection: &mut SqliteConnection,
    proposal_id: &str,
    kind: &str,
    scope: &str,
) -> Result<(), StoreError> {
    let found: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM action_proposals WHERE id = ? AND kind = ? AND scope = ? \
         AND state IN ('approved', 'executed')",
    )
    .bind(proposal_id)
    .bind(kind)
    .bind(scope)
    .fetch_optional(&mut *connection)
    .await?;
    found
        .map(|_| ())
        .ok_or_else(|| CoordinatorError::WrongOrigin.into())
}

async fn require_active(
    connection: &mut SqliteConnection,
    project_id: &str,
) -> Result<(), StoreError> {
    require_project(&mut *connection, project_id).await?;
    if coordinator_removed(&mut *connection, project_id).await? {
        return Err(CoordinatorError::NotActive.into());
    }
    Ok(())
}

/// Whether the coordinator of `project_id` owns unfinished tasks or an unanswered request. The
/// same test as the 0071 retirement trigger.
pub(crate) async fn coordinator_has_open_work(
    connection: &mut SqliteConnection,
    project_id: &str,
) -> Result<bool, StoreError> {
    let sql = format!(
        "SELECT EXISTS (SELECT 1 FROM tasks WHERE coordinator_project_id = ?1 \
                          AND state IN {OPEN_TASK_STATES}) \
             OR EXISTS (SELECT 1 FROM routed_requests r WHERE r.project_id = ?1 \
                          AND NOT EXISTS (SELECT 1 FROM coordinator_reports c \
                                          WHERE c.request_id = r.id AND c.verb = 'answer'))"
    );
    let open: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(project_id)
        .fetch_one(connection)
        .await?;
    Ok(open == 1)
}

impl SqliteStore {
    /// Sets or clears the scope sentence of an active coordinator. The text must pass
    /// [`charter::check_scope_sentence`]; it is stored as given.
    pub fn set_coordinator_scope_sentence(
        &self,
        project_id: &str,
        sentence: Option<&str>,
    ) -> Result<(), StoreError> {
        if sentence.is_some_and(|sentence| !charter::check_scope_sentence(sentence)) {
            return Err(CoordinatorError::BadText.into());
        }
        let (project_id, sentence) = (project_id.to_owned(), sentence.map(str::to_owned));
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            require_active(&mut transaction, &project_id).await?;
            sqlx::query(
                "UPDATE coordinator_scopes SET scope_sentence = ?, changed_at = CURRENT_TIMESTAMP \
                 WHERE project_id = ?",
            )
            .bind(&sentence)
            .bind(&project_id)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    /// The scope sentence of a project's coordinator, active or not. `None` when none is set.
    pub fn coordinator_scope_sentence(
        &self,
        project_id: &str,
    ) -> Result<Option<String>, StoreError> {
        let project_id = project_id.to_owned();
        self.call(move |pool| async move {
            let sentence: Option<Option<String>> = sqlx::query_scalar(
                "SELECT scope_sentence FROM coordinator_scopes WHERE project_id = ?",
            )
            .bind(&project_id)
            .fetch_optional(&pool)
            .await?;
            Ok(sentence.flatten())
        })
    }

    /// Records work routed to a project's coordinator. `origin_proposal_id` must be an approved
    /// `coordinator.route` proposal of the global conversation, so a coordinator never routes.
    /// Idempotent on `id`: a second call returns the stored request.
    pub fn create_routed_request(
        &self,
        id: &str,
        project_id: &str,
        work: &str,
        origin_proposal_id: &str,
    ) -> Result<RoutedRequest, StoreError> {
        if !charter::is_request_id(id) || !charter::check_routed_work(work) {
            return Err(CoordinatorError::BadText.into());
        }
        let (id, project_id, work, origin) = (
            id.to_owned(),
            project_id.to_owned(),
            work.to_owned(),
            origin_proposal_id.to_owned(),
        );
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            if let Some(found) = load_request(&mut transaction, &id).await? {
                transaction.commit().await?;
                return Ok(found);
            }
            let global = ConversationTarget::Global.id();
            require_origin(&mut transaction, &origin, "coordinator.route", &global).await?;
            require_active(&mut transaction, &project_id).await?;
            sqlx::query(
                "INSERT INTO routed_requests (id, project_id, work, origin_proposal_id) \
                 VALUES (?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&project_id)
            .bind(&work)
            .bind(&origin)
            .execute(&mut *transaction)
            .await?;
            let stored = load_request(&mut transaction, &id)
                .await?
                .ok_or_else(|| corrupt("routed request missing after insert".to_owned()))?;
            transaction.commit().await?;
            Ok(stored)
        })
    }

    /// One routed request, or `None`.
    pub fn routed_request(&self, id: &str) -> Result<Option<RoutedRequest>, StoreError> {
        let id = id.to_owned();
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            load_request(&mut connection, &id).await
        })
    }

    /// The newest `limit` routed requests of a project, newest first, open and answered.
    pub fn routed_requests(
        &self,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<RoutedRequest>, StoreError> {
        let project_id = project_id.to_owned();
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.call(move |pool| async move {
            let sql = format!(
                "SELECT {REQUEST_COLUMNS} FROM routed_requests r WHERE r.project_id = ? \
                 ORDER BY r.rowid DESC LIMIT ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(&project_id)
                .bind(limit)
                .fetch_all(&pool)
                .await?
                .iter()
                .map(decode_request)
                .collect()
        })
    }

    /// Writes the owner of a task: the coordinator of its own project, and the routed request it
    /// serves, if any. Idempotent: the same owner again changes nothing; another owner is
    /// refused. A request must be open and of the same project.
    pub fn set_task_coordinator(
        &self,
        task_id: &str,
        project_id: &str,
        request_id: Option<&str>,
    ) -> Result<(), StoreError> {
        let (task_id, project_id, request_id) = (
            task_id.to_owned(),
            project_id.to_owned(),
            request_id.map(str::to_owned),
        );
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let row = sqlx::query(
                "SELECT project_id, coordinator_project_id, routed_request_id FROM tasks \
                 WHERE id = ?",
            )
            .bind(&task_id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| StoreError::NotFound(task_id.clone()))?;
            let own: String = row.try_get("project_id")?;
            let owner: Option<String> = row.try_get("coordinator_project_id")?;
            let request: Option<String> = row.try_get("routed_request_id")?;
            if own != project_id
                || owner.as_ref().is_some_and(|owner| *owner != project_id)
                || (request.is_some() && request != request_id)
            {
                return Err(CoordinatorError::WrongOwner.into());
            }
            if owner.is_some() && request == request_id {
                transaction.commit().await?;
                return Ok(());
            }
            if let Some(id) = &request_id {
                let open = load_request(&mut transaction, id)
                    .await?
                    .is_some_and(|found| found.project_id == project_id && !found.answered);
                if !open {
                    return Err(CoordinatorError::RequestNotOpen.into());
                }
            }
            sqlx::query(
                "UPDATE tasks SET coordinator_project_id = ?, routed_request_id = ? WHERE id = ?",
            )
            .bind(&project_id)
            .bind(&request_id)
            .bind(&task_id)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    /// The coordinator that owns a task (its project id) and the request it serves. `None` for
    /// a task the global orchestrator supervises, or an unknown task.
    pub fn task_coordinator(
        &self,
        task_id: &str,
    ) -> Result<Option<(String, Option<String>)>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
                "SELECT coordinator_project_id, routed_request_id FROM tasks WHERE id = ?",
            )
            .bind(&task_id)
            .fetch_optional(&pool)
            .await?;
            Ok(row.and_then(|(owner, request)| owner.map(|owner| (owner, request))))
        })
    }

    /// Records durable facts for owned tasks. Each id is derived from its cause, so a replay
    /// inserts nothing. A fact for a task with no owner, or whose owner is another project, is
    /// skipped: only the daemon writes facts, from the task's own record. Returns how many were
    /// inserted.
    pub fn record_coordinator_facts(
        &self,
        facts: &[NewCoordinatorFact],
    ) -> Result<usize, StoreError> {
        let facts = facts.to_vec();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let mut inserted = 0;
            for fact in &facts {
                let added = sqlx::query(
                    "INSERT INTO coordinator_reports \
                     (id, project_id, author, verb, request_id, task_id, report_key) \
                     SELECT ?, t.coordinator_project_id, 'daemon', ?, t.routed_request_id, t.id, ? \
                     FROM tasks t WHERE t.id = ? AND t.coordinator_project_id = ? \
                     ON CONFLICT DO NOTHING",
                )
                .bind(&fact.id)
                .bind(fact.fact.as_str())
                .bind(&fact.key)
                .bind(&fact.task_id)
                .bind(&fact.project_id)
                .execute(&mut *transaction)
                .await?;
                inserted += usize::try_from(added.rows_affected()).unwrap_or(0);
            }
            transaction.commit().await?;
            Ok(inserted)
        })
    }

    /// Records a coordinator's own report. `origin_proposal_id` must be an approved
    /// `coordinator.report` proposal of that coordinator's conversation. An `answer` needs an open
    /// request of the same project; it closes the request. Idempotent on `id`.
    #[allow(clippy::too_many_arguments)]
    pub fn record_coordinator_report(
        &self,
        id: &str,
        project_id: &str,
        verb: ReportVerb,
        request_id: Option<&str>,
        key: Option<&str>,
        note: &str,
        origin_proposal_id: &str,
    ) -> Result<CoordinatorReport, StoreError> {
        if !charter::check_report_note(note)
            || request_id.is_some_and(|request| !charter::is_request_id(request))
            || (verb == ReportVerb::Answer && request_id.is_none())
        {
            return Err(CoordinatorError::BadText.into());
        }
        let (id, project_id, request_id, key, note, origin) = (
            id.to_owned(),
            project_id.to_owned(),
            request_id.map(str::to_owned),
            key.map(str::to_owned),
            note.to_owned(),
            origin_proposal_id.to_owned(),
        );
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let sql = format!("SELECT {REPORT_COLUMNS} FROM coordinator_reports WHERE id = ?");
            if let Some(row) = sqlx::query(sqlx::AssertSqlSafe(sql.clone()))
                .bind(&id)
                .fetch_optional(&mut *transaction)
                .await?
            {
                transaction.commit().await?;
                return decode_report(&row);
            }
            let scope = project_scope(&project_id);
            require_origin(&mut transaction, &origin, "coordinator.report", &scope).await?;
            require_project(&mut transaction, &project_id).await?;
            if let Some(request) = &request_id {
                let found = load_request(&mut transaction, request).await?;
                let usable = found.is_some_and(|found| {
                    found.project_id == project_id
                        && !(verb == ReportVerb::Answer && found.answered)
                });
                if !usable {
                    return Err(CoordinatorError::RequestNotOpen.into());
                }
            }
            sqlx::query(
                "INSERT INTO coordinator_reports \
                 (id, project_id, author, verb, request_id, report_key, note, origin_proposal_id) \
                 VALUES (?, ?, 'coordinator', ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&project_id)
            .bind(verb.as_str())
            .bind(&request_id)
            .bind(&key)
            .bind(&note)
            .bind(&origin)
            .execute(&mut *transaction)
            .await?;
            let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(&id)
                .fetch_one(&mut *transaction)
                .await?;
            transaction.commit().await?;
            decode_report(&row)
        })
    }

    /// One report, or `None`.
    pub fn coordinator_report(&self, id: &str) -> Result<Option<CoordinatorReport>, StoreError> {
        let id = id.to_owned();
        self.call(move |pool| async move {
            let sql = format!("SELECT {REPORT_COLUMNS} FROM coordinator_reports WHERE id = ?");
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(&id)
                .fetch_optional(&pool)
                .await?
                .as_ref()
                .map(decode_report)
                .transpose()
        })
    }

    /// The newest `limit` reports of a project, newest first.
    pub fn coordinator_reports(
        &self,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<CoordinatorReport>, StoreError> {
        let project_id = project_id.to_owned();
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.call(move |pool| async move {
            let sql = format!(
                "SELECT {REPORT_COLUMNS} FROM coordinator_reports WHERE project_id = ? \
                 ORDER BY rowid DESC LIMIT ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(&project_id)
                .bind(limit)
                .fetch_all(&pool)
                .await?
                .iter()
                .map(decode_report)
                .collect()
        })
    }

    /// Reports whose global wake record is not written yet, oldest first, at most `limit`.
    pub fn unwoken_coordinator_reports(
        &self,
        limit: usize,
    ) -> Result<Vec<CoordinatorReport>, StoreError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.call(move |pool| async move {
            let sql = format!(
                "SELECT {REPORT_COLUMNS} FROM coordinator_reports WHERE woken = 0 \
                 ORDER BY rowid LIMIT ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(limit)
                .fetch_all(&pool)
                .await?
                .iter()
                .map(decode_report)
                .collect()
        })
    }

    /// Routed requests whose coordinator wake record is not written yet, oldest first.
    pub fn unwoken_routed_requests(&self, limit: usize) -> Result<Vec<RoutedRequest>, StoreError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.call(move |pool| async move {
            let sql = format!(
                "SELECT {REQUEST_COLUMNS} FROM routed_requests r WHERE r.woken = 0 \
                 ORDER BY r.rowid LIMIT ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(limit)
                .fetch_all(&pool)
                .await?
                .iter()
                .map(decode_request)
                .collect()
        })
    }

    /// Marks reports and requests as having their wake records. Called after the records are
    /// written, which is idempotent, so a crash between the two only writes nothing twice.
    pub fn mark_coordinator_wakes_written(
        &self,
        report_ids: &[String],
        request_ids: &[String],
    ) -> Result<(), StoreError> {
        let (reports, requests) = (report_ids.to_vec(), request_ids.to_vec());
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            for id in &reports {
                sqlx::query("UPDATE coordinator_reports SET woken = 1 WHERE id = ? AND woken = 0")
                    .bind(id)
                    .execute(&mut *transaction)
                    .await?;
            }
            for id in &requests {
                sqlx::query("UPDATE routed_requests SET woken = 1 WHERE id = ? AND woken = 0")
                    .bind(id)
                    .execute(&mut *transaction)
                    .await?;
            }
            transaction.commit().await?;
            Ok(())
        })
    }

    /// Every active coordinator with its scope sentence and open work, by project id.
    pub fn coordinator_summaries(&self) -> Result<Vec<CoordinatorSummary>, StoreError> {
        self.call(|pool| async move {
            let sql = format!(
                "SELECT s.project_id, s.scope_sentence, \
                   (SELECT COUNT(*) FROM tasks t WHERE t.coordinator_project_id = s.project_id \
                      AND t.state IN {OPEN_TASK_STATES}) AS open_tasks, \
                   (SELECT COUNT(*) FROM routed_requests r WHERE r.project_id = s.project_id \
                      AND NOT EXISTS (SELECT 1 FROM coordinator_reports c \
                                      WHERE c.request_id = r.id AND c.verb = 'answer')) \
                     AS open_requests \
                 FROM coordinator_scopes s JOIN projects p ON p.id = s.project_id \
                 WHERE s.removed = 0 ORDER BY s.project_id"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
                .fetch_all(&pool)
                .await?;
            rows.iter()
                .map(|row| {
                    let count = |name: &str| -> Result<u32, StoreError> {
                        Ok(u32::try_from(row.try_get::<i64, _>(name)?).unwrap_or(u32::MAX))
                    };
                    Ok(CoordinatorSummary {
                        project_id: row.try_get("project_id")?,
                        scope_sentence: row.try_get("scope_sentence")?,
                        open_tasks: count("open_tasks")?,
                        open_requests: count("open_requests")?,
                    })
                })
                .collect()
        })
    }
}
