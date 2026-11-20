//! Worker status, waits and replies (DPES-1108 phase 4, migration 0065).
//!
//! A task worker reports on a status file; each accepted line is a `task_status_events` row,
//! keyed by its line in the attempt's file so a second read adds nothing. A worker that reports
//! `needs-decision` or `blocked` ends its turn, and [`SqliteStore::enter_waiting`] parks the
//! task in one transaction: the session ends, the attempt and the task wait, and the worker's
//! questions are raised. A reply for a waiting task is a `task_messages` row: the user's answer
//! or reconcile (written by the close of a question, in its own transaction) or a
//! `task.message`. [`SqliteStore::begin_resume`] delivers replies once, in the transaction that
//! moves the task back to `Running` with a new session of the same attempt.
//!
//! Every multi-record change is one transaction, so a crash never leaves a half-parked task
//! for the restart gate to refuse.

use std::collections::BTreeSet;
use std::fmt;
use std::io;

use orchestrator_core::task::{
    attempt_transition, session_transition, AttemptState, Session, SessionState, TaskEvent,
    TaskState, TransitionError, WaitReason,
};
use orchestrator_core::worker_status::{StatusLine, StatusVerb};
use sqlx::{Row, Sqlite, Transaction};

use crate::sqlite_conversations::corrupt;
use crate::sqlite_events::append_durable_event;
use crate::sqlite_ops::{
    apply_task_event_in, bump_task_version, current_task_state, decode, encode, validate_task_id,
};
use crate::sqlite_questions::QuestionRaiser;
use crate::{SqliteStore, StoreError};

/// Most `task.message` replies one author holds undelivered on a waiting task. Answers are not
/// counted: a question closes whatever the queue holds. Each author has its own count, so the
/// orchestrator's messages cannot fill the user's.
pub const MAX_QUEUED_MESSAGES: i64 = 2;

/// One status line as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusEvent {
    pub attempt_id: String,
    pub session_id: String,
    pub line_no: u32,
    pub verb: StatusVerb,
    pub key: Option<String>,
    pub note: Option<String>,
    pub recorded_at: String,
}

/// A question a worker raised, to be stored with its wait.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerQuestion {
    pub id: String,
    pub question: String,
    pub key: Option<String>,
}

/// Where a resume continues: the waiting attempt and the agent session it ran last.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResumeTarget {
    pub attempt_id: String,
    pub attempt_number: u32,
    /// The attempt number whose status directory the run reports to: its own, or for a
    /// follow-up the one it continues, so the agent keeps the path it was told.
    pub status_attempt_number: u32,
    /// Whether the attempt is a follow-up to a finished one.
    pub follow_up: bool,
    /// Whether the follow-up needs a new slot, the one it continues having been returned.
    pub fresh_workspace: bool,
    /// Sessions the attempt has had, so the next one can be named.
    pub sessions: u32,
    /// The agent CLI's own id of the attempt's newest session that recorded one.
    pub agent_session_id: Option<String>,
}

/// Who wrote a reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyKind {
    Answer,
    Reconcile,
    Message,
    /// A `tasks.message`: the user's own words.
    UserMessage,
}

impl ReplyKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Answer => "answer",
            Self::Reconcile => "reconcile",
            Self::Message | Self::UserMessage => "message",
        }
    }
}

/// Who queued a `task.message`: the orchestrator's approved action, or the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageAuthor {
    Orchestrator,
    User,
}

impl MessageAuthor {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Orchestrator => "orchestrator",
            Self::User => "user",
        }
    }
}

/// A reply not yet delivered, with the text the worker will read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingReply {
    pub id: i64,
    pub kind: ReplyKind,
    /// The question an answer or a reconcile closed.
    pub question: Option<String>,
    /// The user's words, the reconcile evidence or the message, exactly as stored.
    pub text: String,
}

/// Why the store refused a wait or reply write. Carried inside a `StoreError::Io` of kind
/// `InvalidInput`; [`WaitError::of`] reads it back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitError {
    /// The task is not waiting.
    NotWaiting,
    /// The task already holds [`MAX_QUEUED_MESSAGES`] undelivered messages.
    QueueFull,
    /// The task has an open question, which holds the task until it closes. A user's message
    /// is refused rather than left to wait behind it.
    QuestionOpen,
    /// The task's undelivered replies are not the ones its resume was planned with: one was
    /// queued after they were read. Nothing is written, and the next pass plans the resume again.
    ReplyChanged,
    /// A reply named for delivery is not an undelivered reply of the task.
    UnknownReply,
    /// A worker question or a status line breaks a rule of the core parser.
    BadLine,
    /// A finished task has no agent session to continue: no run of it ever reported one.
    NoSession,
}

impl fmt::Display for WaitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotWaiting => "the task is not waiting for a reply",
            Self::QueueFull => "the task already holds the most messages it can wait with",
            Self::QuestionOpen => "the task is waiting on a question",
            Self::ReplyChanged => "the task's replies changed before its resume started",
            Self::UnknownReply => "a reply is not an undelivered reply of the task",
            Self::BadLine => "a status line or a worker question is not accepted",
            Self::NoSession => "the task has no agent session to continue",
        })
    }
}

impl std::error::Error for WaitError {}

impl From<WaitError> for StoreError {
    fn from(error: WaitError) -> Self {
        StoreError::Io(io::Error::new(io::ErrorKind::InvalidInput, error))
    }
}

impl WaitError {
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

fn verb_from(value: &str) -> Result<StatusVerb, StoreError> {
    StatusVerb::parse(value).ok_or_else(|| corrupt(format!("unknown status verb {value:?}")))
}

impl SqliteStore {
    /// Stores accepted status lines of one session. A line already stored for the attempt is
    /// left as it is, so reading a file twice is harmless. Notes must already be redacted.
    pub fn record_status_events(
        &self,
        task_id: &str,
        attempt_id: &str,
        session_id: &str,
        lines: &[(u32, StatusLine)],
    ) -> Result<(), StoreError> {
        validate_task_id(task_id)?;
        if lines.is_empty() {
            return Ok(());
        }
        let (task_id, attempt_id, session_id) = (
            task_id.to_owned(),
            attempt_id.to_owned(),
            session_id.to_owned(),
        );
        let lines = lines.to_vec();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            for (line_no, line) in &lines {
                sqlx::query(
                    "INSERT OR IGNORE INTO task_status_events \
                     (task_id, attempt_id, session_id, line_no, verb, status_key, note) \
                     VALUES (?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&task_id)
                .bind(&attempt_id)
                .bind(&session_id)
                .bind(i64::from(*line_no))
                .bind(line.verb.as_str())
                .bind(&line.key)
                .bind(&line.note)
                .execute(&mut *transaction)
                .await?;
            }
            transaction.commit().await?;
            Ok(())
        })
    }

    /// The agent session a finished task would continue: the newest attempt's, or the one it
    /// continued. `None` when no run of the task ever reported one.
    pub fn continuable_session(&self, task_id: &str) -> Result<Option<String>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let newest: Option<String> = sqlx::query_scalar(
                "SELECT id FROM attempts WHERE task_id = ? ORDER BY attempt_number DESC LIMIT 1",
            )
            .bind(&task_id)
            .fetch_optional(&pool)
            .await?;
            match newest {
                Some(attempt) => last_agent_session(&pool, &task_id, &attempt).await,
                None => Ok(None),
            }
        })
    }

    /// How many of a task's attempts are follow-ups to a finished attempt. They are not
    /// retries or repairs, so the bounds on those leave them out.
    pub fn follow_up_attempts(&self, task_id: &str) -> Result<u32, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM attempts WHERE task_id = ? AND continues_attempt_id IS NOT NULL",
            )
            .bind(&task_id)
            .fetch_one(&pool)
            .await?;
            Ok(u32::try_from(count).unwrap_or(u32::MAX))
        })
    }

    /// Marks the named replies of a task that has not started as delivered by its launch
    /// session, once the brief that carries them is recorded.
    pub fn mark_launch_replies_delivered(
        &self,
        task_id: &str,
        session_id: &str,
        reply_ids: &[i64],
    ) -> Result<(), StoreError> {
        validate_task_id(task_id)?;
        let (task_id, session_id, reply_ids) = (
            task_id.to_owned(),
            session_id.to_owned(),
            reply_ids.to_vec(),
        );
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            for id in &reply_ids {
                let marked = sqlx::query(
                    "UPDATE task_messages SET delivered_session_id = ? \
                     WHERE id = ? AND task_id = ? AND delivered_session_id IS NULL",
                )
                .bind(&session_id)
                .bind(id)
                .bind(&task_id)
                .execute(&mut *transaction)
                .await?
                .rows_affected();
                if marked != 1 {
                    return Err(WaitError::UnknownReply.into());
                }
            }
            transaction.commit().await?;
            Ok(())
        })
    }

    /// The last stored line number of an attempt's status file, or 0. A new session reads the
    /// file from the line after it.
    pub fn last_status_line(&self, task_id: &str, attempt_id: &str) -> Result<u32, StoreError> {
        let (task_id, attempt_id) = (task_id.to_owned(), attempt_id.to_owned());
        self.call(move |pool| async move {
            // A follow-up shares the status file of the attempt it continues, so it reads on
            // from the lines that attempt, and the ones before it, left.
            let last: i64 = sqlx::query_scalar(
                "WITH RECURSIVE chain(id) AS ( \
                     SELECT ? UNION \
                     SELECT a.continues_attempt_id FROM attempts a JOIN chain c ON a.id = c.id \
                     WHERE a.task_id = ? AND a.continues_attempt_id IS NOT NULL) \
                 SELECT COALESCE(MAX(line_no), 0) FROM task_status_events \
                 WHERE task_id = ? AND attempt_id IN (SELECT id FROM chain)",
            )
            .bind(&attempt_id)
            .bind(&task_id)
            .bind(&task_id)
            .fetch_one(&pool)
            .await?;
            u32::try_from(last).map_err(|_| corrupt("status line number out of range".to_owned()))
        })
    }

    /// Every stored status line of a task, oldest first.
    pub fn status_events(&self, task_id: &str) -> Result<Vec<StatusEvent>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let rows = sqlx::query(
                "SELECT attempt_id, session_id, line_no, verb, status_key, note, recorded_at \
                 FROM task_status_events WHERE task_id = ? ORDER BY id",
            )
            .bind(&task_id)
            .fetch_all(&pool)
            .await?;
            rows.into_iter()
                .map(|row| {
                    let line_no: i64 = row.try_get("line_no")?;
                    Ok(StatusEvent {
                        attempt_id: row.try_get("attempt_id")?,
                        session_id: row.try_get("session_id")?,
                        line_no: u32::try_from(line_no)
                            .map_err(|_| corrupt("status line number out of range".to_owned()))?,
                        verb: verb_from(&row.try_get::<String, _>("verb")?)?,
                        key: row.try_get("status_key")?,
                        note: row.try_get("note")?,
                        recorded_at: crate::instant::instant(&row, "recorded_at")?,
                    })
                })
                .collect()
        })
    }

    /// Records the agent CLI's own id for a session, once. The same id again is accepted.
    pub fn record_agent_session_id(
        &self,
        task_id: &str,
        session_id: &str,
        agent_session_id: &str,
    ) -> Result<(), StoreError> {
        validate_task_id(task_id)?;
        let (task_id, session_id, agent_session_id) = (
            task_id.to_owned(),
            session_id.to_owned(),
            agent_session_id.to_owned(),
        );
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let updated = sqlx::query(
                "UPDATE sessions SET agent_session_id = ? WHERE task_id = ? AND id = ?",
            )
            .bind(&agent_session_id)
            .bind(&task_id)
            .bind(&session_id)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(StoreError::NoSession {
                    session_id,
                    task_id,
                });
            }
            transaction.commit().await?;
            Ok(())
        })
    }

    /// The attempt a waiting task resumes and the agent session it continues. `None` when the
    /// task's newest attempt is not waiting.
    pub fn resume_target(&self, task_id: &str) -> Result<Option<ResumeTarget>, StoreError> {
        let task_id = task_id.to_owned();
        let waiting = encode(&AttemptState::Waiting)?;
        self.call(move |pool| async move {
            let row = sqlx::query(
                "SELECT id, attempt_number, state FROM attempts WHERE task_id = ? \
                 ORDER BY attempt_number DESC LIMIT 1",
            )
            .bind(&task_id)
            .fetch_optional(&pool)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            if row.try_get::<String, _>("state")? != waiting {
                return Ok(None);
            }
            let attempt_id: String = row.try_get("id")?;
            let number: i64 = row.try_get("attempt_number")?;
            let sessions: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sessions WHERE task_id = ? AND attempt_id = ?",
            )
            .bind(&task_id)
            .bind(&attempt_id)
            .fetch_one(&pool)
            .await?;
            let agent_session_id = last_agent_session(&pool, &task_id, &attempt_id).await?;
            let (status_number, follow_up) = status_root(&pool, &task_id, &attempt_id).await?;
            let fresh_workspace = sqlx::query_scalar::<_, i64>(
                "SELECT fresh_workspace FROM attempts WHERE task_id = ? AND id = ?",
            )
            .bind(&task_id)
            .bind(&attempt_id)
            .fetch_one(&pool)
            .await?
                == 1;
            Ok(Some(ResumeTarget {
                attempt_id,
                attempt_number: u32::try_from(number)
                    .map_err(|_| corrupt("attempt number out of range".to_owned()))?,
                status_attempt_number: u32::try_from(status_number)
                    .map_err(|_| corrupt("attempt number out of range".to_owned()))?,
                follow_up,
                fresh_workspace,
                sessions: u32::try_from(sessions).unwrap_or(u32::MAX),
                agent_session_id,
            }))
        })
    }

    /// Parks a task whose worker reported a wait and exited cleanly, in one transaction: the
    /// session ends `Exited`, the attempt and the task go to `Waiting`, and each worker
    /// question is raised (one already open with the same key, or with the same id, is kept).
    pub fn enter_waiting(
        &self,
        task_id: &str,
        attempt_id: &str,
        session_id: &str,
        reason: WaitReason,
        questions: &[WorkerQuestion],
    ) -> Result<(), StoreError> {
        validate_task_id(task_id)?;
        for question in questions {
            orchestrator_core::question::check_question(
                &question.question,
                &[],
                question.key.as_deref(),
            )
            .map_err(|_| WaitError::BadLine)?;
            if !(1..=128).contains(&question.id.len()) {
                return Err(WaitError::BadLine.into());
            }
        }
        let (task_id, attempt_id, session_id) = (
            task_id.to_owned(),
            attempt_id.to_owned(),
            session_id.to_owned(),
        );
        let questions = questions.to_vec();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let state = current_task_state(&mut transaction, &task_id).await?;
            let event = TaskEvent::WorkerWaiting { reason };
            if state != TaskState::Running {
                return Err(StoreError::IllegalTransition(TransitionError::Illegal {
                    from: state,
                    event,
                }));
            }
            set_session_state(
                &mut transaction,
                &task_id,
                &session_id,
                SessionState::Exited,
            )
            .await?;
            set_attempt_state(
                &mut transaction,
                &task_id,
                &attempt_id,
                AttemptState::Waiting,
            )
            .await?;
            for question in &questions {
                raise_worker_question(&mut transaction, &task_id, question).await?;
            }
            apply_task_event_in(&mut transaction, &task_id, event).await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    /// Queues a `task.message` for a waiting task under `key`, the execution key of the
    /// approved action or the client's key for `tasks.message`: a second call with the same key
    /// queues nothing. The text follows the reply rules. Refused unless the task waits, or when
    /// the author's queue is full. The user's own message also reaches a task that has not
    /// started (the launch delivers it) and a finished one (it is reopened to resume).
    pub fn queue_task_message(
        &self,
        task_id: &str,
        key: &str,
        text: &str,
        author: MessageAuthor,
    ) -> Result<(), StoreError> {
        self.queue_message(task_id, key, text, author, false)
    }

    /// [`Self::queue_task_message`] with the caller's word on whether the task's slot has been
    /// returned to the pool (archived, or its pull request merged): a finished task reopened
    /// then needs a new slot.
    pub fn queue_message(
        &self,
        task_id: &str,
        key: &str,
        text: &str,
        author: MessageAuthor,
        slot_returned: bool,
    ) -> Result<(), StoreError> {
        validate_task_id(task_id)?;
        orchestrator_core::question::check_reply(text).map_err(|_| WaitError::BadLine)?;
        if !(1..=160).contains(&key.len()) {
            return Err(WaitError::BadLine.into());
        }
        let (task_id, key, text) = (task_id.to_owned(), key.to_owned(), text.to_owned());
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            if message_queued(&mut transaction, &key).await? {
                return Ok(());
            }
            let state = current_task_state(&mut transaction, &task_id).await?;
            let attempt_id = match state {
                TaskState::Waiting => waiting_attempt(&mut transaction, &task_id).await?,
                // Only the user reaches past a wait: the orchestrator's messages go to a waiting
                // worker alone. A task that has not started has no attempt yet; the launch
                // delivers the message with the brief.
                TaskState::Created | TaskState::Queued if author == MessageAuthor::User => None,
                TaskState::Succeeded
                | TaskState::Failed
                | TaskState::VerificationFailed
                | TaskState::Cancelled
                    if author == MessageAuthor::User =>
                {
                    Some(reopen_finished(&mut transaction, &task_id, slot_returned).await?)
                }
                _ => return Err(WaitError::NotWaiting.into()),
            };
            // A worker resumes only once its questions close (`tasks_ready_to_resume`), so a
            // user's message sent now would wait behind the question. Refuse it instead.
            let asked: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM decision_holds WHERE subject_kind = 'question' \
                 AND state = 'active' AND subject_id = ?)",
            )
            .bind(&task_id)
            .fetch_one(&mut *transaction)
            .await?;
            if asked && author == MessageAuthor::User {
                return Err(WaitError::QuestionOpen.into());
            }
            let queued: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM task_messages WHERE task_id = ? AND kind = 'message' \
                 AND author = ? AND delivered_session_id IS NULL",
            )
            .bind(&task_id)
            .bind(author.as_str())
            .fetch_one(&mut *transaction)
            .await?;
            if queued >= MAX_QUEUED_MESSAGES {
                return Err(WaitError::QueueFull.into());
            }
            sqlx::query(
                "INSERT INTO task_messages (task_id, attempt_id, kind, text, message_key, author) \
                 VALUES (?, ?, 'message', ?, ?, ?)",
            )
            .bind(&task_id)
            .bind(&attempt_id)
            .bind(&text)
            .bind(&key)
            .bind(author.as_str())
            .execute(&mut *transaction)
            .await?;
            bump_task_version(&mut transaction, &task_id).await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    /// Whether a `task.message` was queued under `key`.
    pub fn task_message_queued(&self, key: &str) -> Result<bool, StoreError> {
        let key = key.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin().await?;
            let queued = message_queued(&mut transaction, &key).await?;
            transaction.commit().await?;
            Ok(queued)
        })
    }

    /// The waiting tasks that have a reply to deliver and no open question: the tasks the
    /// scheduler may resume.
    pub fn tasks_ready_to_resume(&self) -> Result<BTreeSet<String>, StoreError> {
        let waiting = encode(&TaskState::Waiting)?;
        self.call(move |pool| async move {
            let ids: Vec<String> = sqlx::query_scalar(
                "SELECT t.id FROM tasks t WHERE t.state = ? \
                 AND EXISTS (SELECT 1 FROM task_messages m \
                     WHERE m.task_id = t.id AND m.delivered_session_id IS NULL) \
                 AND NOT EXISTS (SELECT 1 FROM decision_holds q \
                     WHERE q.subject_kind = 'question' AND q.state = 'active' \
                     AND q.subject_id = t.id)",
            )
            .bind(&waiting)
            .fetch_all(&pool)
            .await?;
            Ok(ids.into_iter().collect())
        })
    }

    /// The undelivered replies of a task, oldest first, with the text the worker reads.
    pub fn pending_replies(&self, task_id: &str) -> Result<Vec<PendingReply>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let rows = sqlx::query(
                "SELECT m.id, m.kind, m.author, m.text AS message, q.question, \
                        (SELECT a.text FROM decision_answers a WHERE a.hold_id = m.question_id \
                          AND a.kind = m.kind ORDER BY a.id LIMIT 1) AS closing \
                 FROM task_messages m LEFT JOIN decision_holds q ON q.id = m.question_id \
                 WHERE m.task_id = ? AND m.delivered_session_id IS NULL ORDER BY m.id",
            )
            .bind(&task_id)
            .fetch_all(&pool)
            .await?;
            rows.into_iter()
                .map(|row| {
                    let kind = match row.try_get::<String, _>("kind")?.as_str() {
                        "answer" => ReplyKind::Answer,
                        "reconcile" => ReplyKind::Reconcile,
                        "message" => match row.try_get::<String, _>("author")?.as_str() {
                            "orchestrator" => ReplyKind::Message,
                            "user" => ReplyKind::UserMessage,
                            other => {
                                return Err(corrupt(format!("unknown message author {other:?}")));
                            }
                        },
                        other => return Err(corrupt(format!("unknown reply kind {other:?}"))),
                    };
                    let text = match kind {
                        ReplyKind::Message | ReplyKind::UserMessage => {
                            row.try_get::<Option<String>, _>("message")?
                        }
                        _ => row.try_get::<Option<String>, _>("closing")?,
                    }
                    .ok_or_else(|| corrupt("a reply has no text".to_owned()))?;
                    Ok(PendingReply {
                        id: row.try_get("id")?,
                        kind,
                        question: row.try_get("question")?,
                        text,
                    })
                })
                .collect()
        })
    }

    /// Starts the resume of a waiting task, in one transaction: a new `Spawning` session of
    /// the waiting attempt that continues `agent_session_id`, the attempt and the task back to
    /// `Running`, and the named replies marked delivered by that session.
    pub fn begin_resume(
        &self,
        task_id: &str,
        session: &Session,
        agent_session_id: &str,
        reply_ids: &[i64],
    ) -> Result<(), StoreError> {
        validate_task_id(task_id)?;
        let (task_id, session, agent_session_id) = (
            task_id.to_owned(),
            session.clone(),
            agent_session_id.to_owned(),
        );
        let reply_ids = reply_ids.to_vec();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            if waiting_attempt(&mut transaction, &task_id)
                .await?
                .as_deref()
                != Some(session.attempt_id.as_str())
            {
                return Err(WaitError::NotWaiting.into());
            }
            if session.state != SessionState::Spawning {
                return Err(StoreError::SessionNotSpawning {
                    session_id: session.id,
                    state: session.state,
                });
            }
            // The caller read the replies before this transaction. One queued since would be
            // left undelivered by this resume, so the resume is refused until it is planned again.
            let undelivered: BTreeSet<i64> = sqlx::query_scalar::<_, i64>(
                "SELECT id FROM task_messages WHERE task_id = ? AND delivered_session_id IS NULL",
            )
            .bind(&task_id)
            .fetch_all(&mut *transaction)
            .await?
            .into_iter()
            .collect();
            if undelivered != reply_ids.iter().copied().collect::<BTreeSet<i64>>() {
                return Err(WaitError::ReplyChanged.into());
            }
            sqlx::query(
                "INSERT INTO sessions (id, task_id, attempt_id, adapter, state, agent_session_id) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&session.id)
            .bind(&task_id)
            .bind(&session.attempt_id)
            .bind(&session.adapter)
            .bind(encode(&session.state)?)
            .bind(&agent_session_id)
            .execute(&mut *transaction)
            .await?;
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
            set_attempt_state(
                &mut transaction,
                &task_id,
                &session.attempt_id,
                AttemptState::Running,
            )
            .await?;
            for id in &reply_ids {
                let marked = sqlx::query(
                    "UPDATE task_messages SET delivered_session_id = ? \
                     WHERE id = ? AND task_id = ? AND delivered_session_id IS NULL",
                )
                .bind(&session.id)
                .bind(id)
                .bind(&task_id)
                .execute(&mut *transaction)
                .await?
                .rows_affected();
                if marked != 1 {
                    return Err(WaitError::UnknownReply.into());
                }
            }
            apply_task_event_in(&mut transaction, &task_id, TaskEvent::ResumeStarted).await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    /// Fails a waiting task that cannot resume, in one transaction: the waiting attempt ends
    /// `Failed` with `reason` and the task `Failed`, so it can be retried. Its replies stay
    /// undelivered and its questions stay as they are.
    pub fn fail_waiting_task(&self, task_id: &str, reason: &str) -> Result<(), StoreError> {
        validate_task_id(task_id)?;
        let (task_id, reason) = (task_id.to_owned(), reason.to_owned());
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let Some(attempt_id) = waiting_attempt(&mut transaction, &task_id).await? else {
                return Err(WaitError::NotWaiting.into());
            };
            attempt_transition(AttemptState::Waiting, AttemptState::Failed)?;
            sqlx::query(
                "UPDATE attempts SET state = ?, failure_reason = ? WHERE task_id = ? AND id = ?",
            )
            .bind(encode(&AttemptState::Failed)?)
            .bind(&reason)
            .bind(&task_id)
            .bind(&attempt_id)
            .execute(&mut *transaction)
            .await?;
            bump_task_version(&mut transaction, &task_id).await?;
            let payload = serde_json::json!({
                "attempt_id": &attempt_id,
                "state": AttemptState::Failed,
                "failure_reason_available": true,
            });
            append_durable_event(
                &mut transaction,
                &task_id,
                None,
                "attempt.state_changed",
                &payload,
            )
            .await?;
            apply_task_event_in(&mut transaction, &task_id, TaskEvent::ExecutionFailed).await?;
            transaction.commit().await?;
            Ok(())
        })
    }
}

/// The attempt number of the status directory an attempt reports to, and whether the attempt is
/// a follow-up: follows `continues_attempt_id` back to the attempt that made the directory.
async fn status_root(
    pool: &sqlx::SqlitePool,
    task_id: &str,
    attempt_id: &str,
) -> Result<(i64, bool), StoreError> {
    let mut attempt = attempt_id.to_owned();
    let mut follow_up = false;
    loop {
        let (number, previous): (i64, Option<String>) = sqlx::query_as(
            "SELECT attempt_number, continues_attempt_id FROM attempts \
             WHERE task_id = ? AND id = ?",
        )
        .bind(task_id)
        .bind(&attempt)
        .fetch_one(pool)
        .await?;
        match previous {
            Some(previous) => {
                follow_up = true;
                attempt = previous;
            }
            None => return Ok((number, follow_up)),
        }
    }
}

/// The agent session an attempt last ran. A follow-up attempt has run nothing yet, so it takes
/// the session of the attempt it continues, and so on back along the chain.
async fn last_agent_session(
    pool: &sqlx::SqlitePool,
    task_id: &str,
    attempt_id: &str,
) -> Result<Option<String>, StoreError> {
    let mut attempt = attempt_id.to_owned();
    loop {
        let session: Option<String> = sqlx::query_scalar(
            "SELECT agent_session_id FROM sessions WHERE task_id = ? AND attempt_id = ? \
             AND agent_session_id IS NOT NULL ORDER BY sequence DESC LIMIT 1",
        )
        .bind(task_id)
        .bind(&attempt)
        .fetch_optional(pool)
        .await?;
        if session.is_some() {
            return Ok(session);
        }
        let previous: Option<Option<String>> = sqlx::query_scalar(
            "SELECT continues_attempt_id FROM attempts WHERE task_id = ? AND id = ?",
        )
        .bind(task_id)
        .bind(&attempt)
        .fetch_optional(pool)
        .await?;
        match previous.flatten() {
            Some(previous) => attempt = previous,
            None => return Ok(None),
        }
    }
}

/// Parks a finished task for a user's message, in the caller's transaction: a new `Waiting`
/// attempt that continues the newest attempt's agent session, the task back to `Waiting`, and
/// the task visible again if it was archived. Returns the new attempt's id. A task whose runs
/// never reported an agent session has nothing to continue ([`WaitError::NoSession`]).
async fn reopen_finished(
    transaction: &mut Transaction<'_, Sqlite>,
    task_id: &str,
    slot_returned: bool,
) -> Result<String, StoreError> {
    let (previous, number): (String, i64) = sqlx::query_as(
        "SELECT id, attempt_number FROM attempts WHERE task_id = ? \
         ORDER BY attempt_number DESC LIMIT 1",
    )
    .bind(task_id)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or(WaitError::NoSession)?;
    let mut cursor = Some(previous.clone());
    let mut has_session = false;
    while let Some(attempt) = cursor {
        has_session = sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS (SELECT 1 FROM sessions WHERE task_id = ? AND attempt_id = ? \
             AND agent_session_id IS NOT NULL)",
        )
        .bind(task_id)
        .bind(&attempt)
        .fetch_one(&mut **transaction)
        .await?
            == 1;
        if has_session {
            break;
        }
        cursor = sqlx::query_scalar::<_, Option<String>>(
            "SELECT continues_attempt_id FROM attempts WHERE task_id = ? AND id = ?",
        )
        .bind(task_id)
        .bind(&attempt)
        .fetch_one(&mut **transaction)
        .await?;
    }
    if !has_session {
        return Err(WaitError::NoSession.into());
    }
    let attempt_id = format!("{task_id}-attempt-{}", number + 1);
    sqlx::query(
        "INSERT INTO attempts \
         (id, task_id, attempt_number, state, continues_attempt_id, fresh_workspace) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&attempt_id)
    .bind(task_id)
    .bind(number + 1)
    .bind(encode(&AttemptState::Waiting)?)
    .bind(&previous)
    .bind(i64::from(slot_returned))
    .execute(&mut **transaction)
    .await?;
    let payload = serde_json::json!({
        "attempt_id": &attempt_id,
        "attempt_number": number + 1,
        "state": AttemptState::Waiting,
    });
    append_durable_event(transaction, task_id, None, "attempt.created", &payload).await?;
    apply_task_event_in(transaction, task_id, TaskEvent::FollowUpRequested).await?;
    sqlx::query("UPDATE tasks SET archived_at = NULL WHERE id = ?")
        .bind(task_id)
        .execute(&mut **transaction)
        .await?;
    Ok(attempt_id)
}

async fn message_queued(
    transaction: &mut Transaction<'_, Sqlite>,
    key: &str,
) -> Result<bool, StoreError> {
    Ok(
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM task_messages WHERE message_key = ?")
            .bind(key)
            .fetch_optional(&mut **transaction)
            .await?
            .is_some(),
    )
}

/// The waiting attempt of a waiting task, or `None` when the task does not wait.
async fn waiting_attempt(
    transaction: &mut Transaction<'_, Sqlite>,
    task_id: &str,
) -> Result<Option<String>, StoreError> {
    if current_task_state(transaction, task_id).await? != TaskState::Waiting {
        return Ok(None);
    }
    let attempt: Option<String> = sqlx::query_scalar(
        "SELECT id FROM attempts WHERE task_id = ? AND state = ? \
         ORDER BY attempt_number DESC LIMIT 1",
    )
    .bind(task_id)
    .bind(encode(&AttemptState::Waiting)?)
    .fetch_optional(&mut **transaction)
    .await?;
    match attempt {
        Some(attempt) => Ok(Some(attempt)),
        None => Err(corrupt(format!(
            "task {task_id:?} waits with no waiting attempt"
        ))),
    }
}

/// Queues the user's reply for a waiting task when a question of that task closes. Called in
/// the closing transaction, so the reply exists exactly when the close does. A task that does
/// not wait gets no reply: its worker is not waiting for one.
pub(crate) async fn queue_reply_if_waiting(
    transaction: &mut Transaction<'_, Sqlite>,
    task_id: &str,
    question_id: &str,
    kind: crate::sqlite_questions::ClosingKind,
) -> Result<(), StoreError> {
    let Some(attempt_id) = waiting_attempt(transaction, task_id).await? else {
        return Ok(());
    };
    let kind = match kind {
        crate::sqlite_questions::ClosingKind::Answer => ReplyKind::Answer,
        crate::sqlite_questions::ClosingKind::Reconcile => ReplyKind::Reconcile,
    };
    // Only the user's own RPCs close a question (`decisions.answer`, `decisions.reconcile`), so
    // the reply is the user's.
    sqlx::query(
        "INSERT INTO task_messages (task_id, attempt_id, kind, question_id, author) \
         VALUES (?, ?, ?, ?, 'user')",
    )
    .bind(task_id)
    .bind(&attempt_id)
    .bind(kind.as_str())
    .bind(question_id)
    .execute(&mut **transaction)
    .await?;
    bump_task_version(transaction, task_id).await
}

async fn raise_worker_question(
    transaction: &mut Transaction<'_, Sqlite>,
    task_id: &str,
    question: &WorkerQuestion,
) -> Result<(), StoreError> {
    let existing: Option<String> =
        sqlx::query_scalar("SELECT subject_id FROM decision_holds WHERE id = ?")
            .bind(&question.id)
            .fetch_optional(&mut **transaction)
            .await?;
    if let Some(subject) = existing {
        return if subject == task_id {
            Ok(())
        } else {
            Err(crate::sqlite_questions::QuestionError::IdInUse.into())
        };
    }
    if let Some(key) = &question.key {
        let open: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM decision_holds WHERE subject_kind = 'question' AND state = 'active' \
             AND subject_id = ? AND question_key = ?",
        )
        .bind(task_id)
        .bind(key)
        .fetch_optional(&mut **transaction)
        .await?;
        if open.is_some() {
            return Ok(());
        }
    }
    sqlx::query(
        "INSERT INTO decision_holds \
         (id, subject_kind, subject_id, set_by, question, options_json, question_key) \
         VALUES (?, 'question', ?, ?, ?, '[]', ?)",
    )
    .bind(&question.id)
    .bind(task_id)
    .bind(QuestionRaiser::Worker.as_str())
    .bind(&question.question)
    .bind(&question.key)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn set_session_state(
    transaction: &mut Transaction<'_, Sqlite>,
    task_id: &str,
    session_id: &str,
    state: SessionState,
) -> Result<(), StoreError> {
    let current: String =
        sqlx::query_scalar("SELECT state FROM sessions WHERE task_id = ? AND id = ?")
            .bind(task_id)
            .bind(session_id)
            .fetch_optional(&mut **transaction)
            .await?
            .ok_or_else(|| StoreError::NoSession {
                session_id: session_id.to_owned(),
                task_id: task_id.to_owned(),
            })?;
    let current: SessionState = decode(&current)?;
    if current.is_terminal() {
        return Err(StoreError::TerminalSession {
            session_id: session_id.to_owned(),
            state: current,
        });
    }
    session_transition(current, state)?;
    sqlx::query("UPDATE sessions SET state = ? WHERE task_id = ? AND id = ?")
        .bind(encode(&state)?)
        .bind(task_id)
        .bind(session_id)
        .execute(&mut **transaction)
        .await?;
    bump_task_version(transaction, task_id).await?;
    let payload = serde_json::json!({"session_id": session_id, "state": state});
    append_durable_event(
        transaction,
        task_id,
        Some(session_id),
        "session.state_changed",
        &payload,
    )
    .await
}

async fn set_attempt_state(
    transaction: &mut Transaction<'_, Sqlite>,
    task_id: &str,
    attempt_id: &str,
    state: AttemptState,
) -> Result<(), StoreError> {
    let current: String =
        sqlx::query_scalar("SELECT state FROM attempts WHERE task_id = ? AND id = ?")
            .bind(task_id)
            .bind(attempt_id)
            .fetch_optional(&mut **transaction)
            .await?
            .ok_or_else(|| StoreError::NoAttempt {
                attempt_id: attempt_id.to_owned(),
                task_id: task_id.to_owned(),
            })?;
    let current: AttemptState = decode(&current)?;
    if current.is_terminal() {
        return Err(StoreError::TerminalAttempt {
            attempt_id: attempt_id.to_owned(),
            state: current,
        });
    }
    attempt_transition(current, state)?;
    sqlx::query("UPDATE attempts SET state = ? WHERE task_id = ? AND id = ?")
        .bind(encode(&state)?)
        .bind(task_id)
        .bind(attempt_id)
        .execute(&mut **transaction)
        .await?;
    bump_task_version(transaction, task_id).await?;
    let payload = serde_json::json!({"attempt_id": attempt_id, "state": state});
    append_durable_event(
        transaction,
        task_id,
        None,
        "attempt.state_changed",
        &payload,
    )
    .await
}
