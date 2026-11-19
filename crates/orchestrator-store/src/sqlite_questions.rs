//! User decisions (DPES-1108 phase 3, migration 0064).
//!
//! A question for the user is a `decision_holds` row of subject kind `question`, bound to a
//! task. The orchestrator raises one through an approved `decision.ask`; a later phase raises
//! one when a worker reports that it needs a decision ([`QuestionRaiser::Worker`]). While a
//! question is open its task does not start (the scheduler reads [`SqliteStore::tasks_with_open_questions`]).
//!
//! What the user does with a question is an append-only `decision_answers` record: an answer
//! (the user's exact words), a reconcile (evidence that the question no longer matters) or a
//! deferral (a day, as a hold names one). Only an answer or a reconcile closes a question, and
//! the database refuses any other close. A deferral leaves it open and only hides it from the
//! list until its day ends, by the caller's clock: the store reads no clock for logic.

use std::fmt;
use std::io;

use orchestrator_core::question::{check_question, check_reply};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection};

use crate::sqlite_conversations::corrupt;
use crate::sqlite_holds::end_of_day_ms;
use crate::{SqliteStore, StoreError};

/// 2100-01-01 in milliseconds; migration 0064 bounds `until_ms` by the same number.
const MAX_UNTIL_MS: i64 = 4_102_444_800_000;
const COLUMNS: &str =
    "id, subject_id, set_by, state, created_at, question, options_json, question_key";

/// Who raised a question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionRaiser {
    /// The orchestrator, through an approved `decision.ask`.
    Orchestrator,
    /// The task's own worker, when it reports that it needs a decision.
    Worker,
}

impl QuestionRaiser {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Orchestrator => "orchestrator",
            Self::Worker => "worker",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "orchestrator" => Some(Self::Orchestrator),
            "worker" => Some(Self::Worker),
            _ => None,
        }
    }
}

/// How a question was closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClosingKind {
    /// The user's own words.
    Answer,
    /// Evidence that the question no longer matters. Never reads as the user's answer.
    Reconcile,
}

impl ClosingKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Answer => "answer",
            Self::Reconcile => "reconcile",
        }
    }
}

/// The record that closed a question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestionClosing {
    pub kind: ClosingKind,
    /// Exactly as given: the answer or the evidence.
    pub text: String,
    pub recorded_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionQuestion {
    pub id: String,
    pub task_id: String,
    pub question: String,
    pub options: Vec<String>,
    pub key: Option<String>,
    pub raised_by: QuestionRaiser,
    pub created_at: String,
    /// `None` while the question is open.
    pub closing: Option<QuestionClosing>,
    /// The newest deferral, as `YYYY-MM-DD` and the end of that UTC day in milliseconds. It
    /// hides an open question while `until_ms` is after the caller's clock.
    pub deferred_until: Option<(String, i64)>,
}

impl DecisionQuestion {
    pub fn is_open(&self) -> bool {
        self.closing.is_none()
    }

    /// Whether a deferral hides the question at `now_ms`.
    pub fn is_deferred_at(&self, now_ms: i64) -> bool {
        self.is_open()
            && self
                .deferred_until
                .as_ref()
                .is_some_and(|(_, until_ms)| *until_ms > now_ms)
    }
}

/// Why the store refused a question write. Carried inside a `StoreError::Io` of kind
/// `InvalidInput`; [`QuestionError::of`] reads it back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionError {
    /// The question id or the task id is empty or too long.
    BadId,
    /// The question, its options or its key break a rule of `orchestrator_core::question`.
    BadQuestion,
    /// The answer or the evidence is blank, too long or holds a refused character.
    BadReply,
    /// The deferral date is not a real `YYYY-MM-DD` before 2100 with its UTC day end.
    BadDate,
    /// The deferral ends before the caller's clock.
    NotInFuture,
    /// The task does not exist.
    NoTask,
    /// The question is closed already.
    Closed,
    /// A question with this id exists and is not the one being raised.
    IdInUse,
    /// The task has a question its worker raised that is still open, so its report may not
    /// be relayed yet.
    WorkerQuestionOpen,
}

impl fmt::Display for QuestionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::BadId => {
                "the question id and the task id must be 1 to 128 and 1 to 160 characters"
            }
            Self::BadQuestion => "the question, its options or its key are not accepted",
            Self::BadReply => "the answer or the evidence is not accepted",
            Self::BadDate => "the date must be a real YYYY-MM-DD before 2100 with its UTC day end",
            Self::NotInFuture => "the deferral must end after now",
            Self::NoTask => "the task does not exist",
            Self::Closed => "the question is closed",
            Self::IdInUse => "a question with this id already exists",
            Self::WorkerQuestionOpen => "the task has an open question for the user",
        })
    }
}

impl std::error::Error for QuestionError {}

impl From<QuestionError> for StoreError {
    fn from(error: QuestionError) -> Self {
        StoreError::Io(io::Error::new(io::ErrorKind::InvalidInput, error))
    }
}

impl QuestionError {
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
    /// Raises a question for the user on `task_id`, in one transaction. The id names the
    /// question; a repeat with the same id and task returns the stored question unchanged.
    /// With a `key`, an open question of the same task and key is returned unchanged
    /// whatever its wording, so asking again does not ask twice. The task may be in any
    /// state; while the question is open the task does not start.
    pub fn raise_question(
        &self,
        id: &str,
        task_id: &str,
        question: &str,
        options: &[String],
        key: Option<&str>,
        raised_by: QuestionRaiser,
    ) -> Result<DecisionQuestion, StoreError> {
        if !(1..=128).contains(&id.chars().count()) || !(1..=160).contains(&task_id.chars().count())
        {
            return Err(QuestionError::BadId.into());
        }
        check_question(question, options, key).map_err(|_| QuestionError::BadQuestion)?;
        let options_json = serde_json::to_string(options)
            .map_err(|_| corrupt("question options do not serialize".to_owned()))?;
        let (id, task_id, question) = (id.to_owned(), task_id.to_owned(), question.to_owned());
        let key = key.map(str::to_owned);
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            if let Some(found) = read_question(&mut transaction, &id).await? {
                return if found.task_id == task_id {
                    Ok(found)
                } else {
                    Err(QuestionError::IdInUse.into())
                };
            }
            if let Some(key) = &key {
                let open = sqlx::query_scalar::<_, String>(
                    "SELECT id FROM decision_holds WHERE subject_kind = 'question' \
                     AND state = 'active' AND subject_id = ? AND question_key = ?",
                )
                .bind(&task_id)
                .bind(key)
                .fetch_optional(&mut *transaction)
                .await?;
                if let Some(open) = open {
                    return read_question(&mut transaction, &open)
                        .await?
                        .ok_or_else(|| corrupt("the open question is missing".to_owned()));
                }
            }
            let task = sqlx::query_scalar::<_, i64>("SELECT 1 FROM tasks WHERE id = ?")
                .bind(&task_id)
                .fetch_optional(&mut *transaction)
                .await?;
            if task.is_none() {
                return Err(QuestionError::NoTask.into());
            }
            sqlx::query(
                "INSERT INTO decision_holds \
                 (id, subject_kind, subject_id, set_by, question, options_json, question_key) \
                 VALUES (?, 'question', ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&task_id)
            .bind(raised_by.as_str())
            .bind(&question)
            .bind(&options_json)
            .bind(&key)
            .execute(&mut *transaction)
            .await?;
            let saved = read_question(&mut transaction, &id)
                .await?
                .ok_or_else(|| corrupt("the question just raised is missing".to_owned()))?;
            transaction.commit().await?;
            Ok(saved)
        })
    }

    /// Closes an open question with the user's answer, stored exactly as given. A closed
    /// question is `Closed`; an unknown id is `RecordNotFound`.
    pub fn answer_question(
        &self,
        question_id: &str,
        answer: &str,
    ) -> Result<DecisionQuestion, StoreError> {
        self.close_question(question_id, ClosingKind::Answer, answer)
    }

    /// Closes an open question as moot, with the evidence that it no longer matters. The
    /// record says reconcile, never answer.
    pub fn reconcile_question(
        &self,
        question_id: &str,
        evidence: &str,
    ) -> Result<DecisionQuestion, StoreError> {
        self.close_question(question_id, ClosingKind::Reconcile, evidence)
    }

    /// Defers an open question to the end of `until_date` (UTC), which `until_ms` must be and
    /// which must be after `now_ms`. The question stays open and its task stays held; it is
    /// only hidden from the list until then. A newer deferral replaces an older one by being
    /// newer; none is changed. Range limits (today to 60 days) belong to the caller.
    pub fn defer_question(
        &self,
        question_id: &str,
        until_date: &str,
        until_ms: i64,
        now_ms: i64,
    ) -> Result<DecisionQuestion, StoreError> {
        if end_of_day_ms(until_date) != Some(until_ms) || until_ms > MAX_UNTIL_MS {
            return Err(QuestionError::BadDate.into());
        }
        if until_ms <= now_ms {
            return Err(QuestionError::NotInFuture.into());
        }
        let (question_id, until_date) = (question_id.to_owned(), until_date.to_owned());
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            open_question(&mut transaction, &question_id).await?;
            sqlx::query(
                "INSERT INTO decision_answers (hold_id, kind, until_date, until_ms, recorded_by) \
                 VALUES (?, 'defer', ?, ?, 'user')",
            )
            .bind(&question_id)
            .bind(&until_date)
            .bind(until_ms)
            .execute(&mut *transaction)
            .await?;
            let saved = read_question(&mut transaction, &question_id)
                .await?
                .ok_or_else(|| corrupt("the deferred question is missing".to_owned()))?;
            transaction.commit().await?;
            Ok(saved)
        })
    }

    /// One question by id, open or closed.
    pub fn decision_question(
        &self,
        question_id: &str,
    ) -> Result<Option<DecisionQuestion>, StoreError> {
        let question_id = question_id.to_owned();
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            read_question(&mut connection, &question_id).await
        })
    }

    /// Every open question, deferred or not, oldest first. This is `decisions.list`.
    pub fn open_questions(&self) -> Result<Vec<DecisionQuestion>, StoreError> {
        self.call(|pool| async move {
            let mut connection = pool.acquire().await?;
            let ids = sqlx::query_scalar::<_, String>(
                "SELECT id FROM decision_holds WHERE subject_kind = 'question' \
                 AND state = 'active' ORDER BY rowid",
            )
            .fetch_all(&mut *connection)
            .await?;
            let mut questions = Vec::with_capacity(ids.len());
            for id in ids {
                questions.push(
                    read_question(&mut connection, &id)
                        .await?
                        .ok_or_else(|| corrupt("an open question is missing".to_owned()))?,
                );
            }
            Ok(questions)
        })
    }

    /// The ids of the tasks that have an open question, deferred or not: the tasks the
    /// scheduler holds back.
    pub fn tasks_with_open_questions(
        &self,
    ) -> Result<std::collections::BTreeSet<String>, StoreError> {
        self.call(|pool| async move {
            let ids = sqlx::query_scalar::<_, String>(
                "SELECT DISTINCT subject_id FROM decision_holds \
                 WHERE subject_kind = 'question' AND state = 'active'",
            )
            .fetch_all(&pool)
            .await?;
            Ok(ids.into_iter().collect())
        })
    }

    fn close_question(
        &self,
        question_id: &str,
        kind: ClosingKind,
        text: &str,
    ) -> Result<DecisionQuestion, StoreError> {
        check_reply(text).map_err(|_| QuestionError::BadReply)?;
        let (question_id, text) = (question_id.to_owned(), text.to_owned());
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            open_question(&mut transaction, &question_id).await?;
            sqlx::query(
                "INSERT INTO decision_answers (hold_id, kind, text, recorded_by) \
                 VALUES (?, ?, ?, 'user')",
            )
            .bind(&question_id)
            .bind(kind.as_str())
            .bind(&text)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                "UPDATE decision_holds SET state = 'released', released_at = CURRENT_TIMESTAMP \
                 WHERE id = ? AND subject_kind = 'question' AND state = 'active'",
            )
            .bind(&question_id)
            .execute(&mut *transaction)
            .await?;
            let saved = read_question(&mut transaction, &question_id)
                .await?
                .ok_or_else(|| corrupt("the closed question is missing".to_owned()))?;
            // A waiting worker gets the reply when its task resumes (DPES-1108 phase 4).
            crate::sqlite_worker_status::queue_reply_if_waiting(
                &mut transaction,
                &saved.task_id,
                &question_id,
                kind,
            )
            .await?;
            transaction.commit().await?;
            Ok(saved)
        })
    }
}

/// Whether `task_id` has an open question its worker raised. A scout's report is not relayed
/// while it has one (`mark_task_report_relayed`), and no candidate is recorded while it has one
/// (`apply_task_event_in`, DPES-1108 phase 4).
pub(crate) async fn worker_question_open(
    connection: &mut SqliteConnection,
    task_id: &str,
) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM decision_holds WHERE subject_kind = 'question' AND state = 'active' \
         AND set_by = 'worker' AND subject_id = ? LIMIT 1",
    )
    .bind(task_id)
    .fetch_optional(connection)
    .await?
    .is_some())
}

/// Fails unless `question_id` names an open question: `RecordNotFound` for no question,
/// `Closed` for a closed one.
async fn open_question(
    connection: &mut SqliteConnection,
    question_id: &str,
) -> Result<(), StoreError> {
    match read_question(connection, question_id).await? {
        None => Err(StoreError::RecordNotFound {
            entity: "decision question",
            id: question_id.to_owned(),
        }),
        Some(question) if !question.is_open() => Err(QuestionError::Closed.into()),
        Some(_) => Ok(()),
    }
}

async fn read_question(
    connection: &mut SqliteConnection,
    question_id: &str,
) -> Result<Option<DecisionQuestion>, StoreError> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        // Safe assert: `COLUMNS` is a fixed list of column names; the id is bound.
        "SELECT {COLUMNS} FROM decision_holds WHERE id = ? AND subject_kind = 'question'"
    )))
    .bind(question_id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let mut question = question_from_row(&row)?;
    let closing = sqlx::query(
        "SELECT kind, text, recorded_at FROM decision_answers \
         WHERE hold_id = ? AND kind IN ('answer', 'reconcile') ORDER BY id LIMIT 1",
    )
    .bind(question_id)
    .fetch_optional(&mut *connection)
    .await?;
    question.closing = closing
        .map(|row| -> Result<QuestionClosing, StoreError> {
            let kind: String = row.try_get("kind")?;
            Ok(QuestionClosing {
                kind: if kind == "answer" {
                    ClosingKind::Answer
                } else {
                    ClosingKind::Reconcile
                },
                text: row.try_get("text")?,
                recorded_at: row.try_get("recorded_at")?,
            })
        })
        .transpose()?;
    let state: String = row.try_get("state")?;
    if (state == "active") != question.closing.is_none() {
        return Err(corrupt(format!(
            "question {question_id:?} state and record disagree"
        )));
    }
    let deferral = sqlx::query(
        "SELECT until_date, until_ms FROM decision_answers \
         WHERE hold_id = ? AND kind = 'defer' ORDER BY id DESC LIMIT 1",
    )
    .bind(question_id)
    .fetch_optional(&mut *connection)
    .await?;
    question.deferred_until = deferral
        .map(|row| -> Result<(String, i64), StoreError> {
            Ok((row.try_get("until_date")?, row.try_get("until_ms")?))
        })
        .transpose()?;
    Ok(Some(question))
}

fn question_from_row(row: &SqliteRow) -> Result<DecisionQuestion, StoreError> {
    let set_by: String = row.try_get("set_by")?;
    let options_json: String = row.try_get("options_json")?;
    Ok(DecisionQuestion {
        id: row.try_get("id")?,
        task_id: row.try_get("subject_id")?,
        question: row.try_get("question")?,
        options: serde_json::from_str(&options_json)
            .map_err(|_| corrupt("question options are not a list of text".to_owned()))?,
        key: row.try_get("question_key")?,
        raised_by: QuestionRaiser::parse(&set_by)
            .ok_or_else(|| corrupt(format!("unknown question raiser {set_by:?}")))?,
        created_at: row.try_get("created_at")?,
        closing: None,
        deferred_until: None,
    })
}
