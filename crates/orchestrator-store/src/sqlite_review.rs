//! SQLite persistence for the coordinator conversation and review decisions.
//!
//! Coordinator entries are project-scoped and never change task state. A change
//! request is feedback bound to one approval and its candidate commit. It does
//! not approve, reject, or supersede the approval. Approving while a change
//! request exists is refused in the same transaction that would approve.

use std::fmt;

use orchestrator_core::approval::{
    self, Actor, Approval, ApprovalError, ApprovalEvent, ApprovalState, ApproverAllowlist,
};
use serde_json::Value;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

use crate::sqlite_approvals::{
    approval_call, authorize_decision, decode_approval, load_approval, persistence,
    update_approval, validate_actor, validate_approval_id,
};
use crate::sqlite_events::append_durable_event;
use crate::{SqliteStore, StoreError};

/// Largest accepted change-request feedback, in bytes.
pub const MAX_FEEDBACK_BYTES: usize = 8 * 1024;

/// One coordinator message or proposal in a project conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinatorEntry {
    pub sequence: i64,
    pub id: String,
    pub project_id: String,
    /// `message` or `proposal`.
    pub kind: String,
    /// `user` or `coordinator`.
    pub author: String,
    pub body: String,
    /// Structured validation outcome; present exactly for proposals.
    pub validation: Option<Value>,
    pub created_at: String,
}

/// Reviewer feedback bound to one approval and candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeRequest {
    pub id: String,
    pub task_id: String,
    pub approval_id: String,
    pub candidate_commit: String,
    pub feedback: String,
    pub requested_by: String,
    pub created_at: String,
}

/// A review mutation failed.
#[derive(Debug)]
pub enum ReviewError {
    Approval(ApprovalError),
    Store(StoreError),
    /// The approval has an open change request, so it cannot be approved.
    ChangesRequested {
        approval_id: String,
    },
    /// The approval already carries a change request.
    AlreadyRequested {
        approval_id: String,
    },
    /// The named candidate is not the approval's candidate commit.
    CandidateMismatch,
    /// Feedback is empty, blank, or larger than [`MAX_FEEDBACK_BYTES`].
    InvalidFeedback,
}

impl fmt::Display for ReviewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Approval(error) => write!(formatter, "approval: {error}"),
            Self::Store(error) => write!(formatter, "store: {error}"),
            Self::ChangesRequested { approval_id } => {
                write!(formatter, "approval {approval_id} has requested changes")
            }
            Self::AlreadyRequested { approval_id } => {
                write!(
                    formatter,
                    "approval {approval_id} already has a change request"
                )
            }
            Self::CandidateMismatch => formatter.write_str("candidate commit does not match"),
            Self::InvalidFeedback => formatter.write_str("feedback is invalid"),
        }
    }
}

impl std::error::Error for ReviewError {}

impl From<ApprovalError> for ReviewError {
    fn from(error: ApprovalError) -> Self {
        Self::Approval(error)
    }
}

impl From<StoreError> for ReviewError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl SqliteStore {
    /// Append one entry to a project's coordinator conversation.
    ///
    /// `validation` must be present exactly for `proposal` entries. The project
    /// must exist; the foreign key rejects an unknown project.
    pub fn append_coordinator_entry(
        &self,
        id: &str,
        project_id: &str,
        kind: &str,
        author: &str,
        body: &str,
        validation: Option<&Value>,
    ) -> Result<CoordinatorEntry, StoreError> {
        let (id, project_id, kind, author, body) = (
            id.to_owned(),
            project_id.to_owned(),
            kind.to_owned(),
            author.to_owned(),
            body.to_owned(),
        );
        let validation = validation.map(serde_json::to_string).transpose()?;
        self.call(move |pool| async move {
            let row = sqlx::query(
                "INSERT INTO coordinator_entries (id, project_id, kind, author, body, validation) \
                 VALUES (?, ?, ?, ?, ?, ?) \
                 RETURNING sequence, id, project_id, kind, author, body, validation, created_at",
            )
            .bind(&id)
            .bind(&project_id)
            .bind(&kind)
            .bind(&author)
            .bind(&body)
            .bind(validation)
            .fetch_one(&pool)
            .await?;
            decode_entry(row)
        })
    }

    /// Load a bounded page of one project's conversation in sequence order.
    pub fn coordinator_entries_after(
        &self,
        project_id: &str,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<CoordinatorEntry>, StoreError> {
        if after_sequence < 0 {
            return Err(StoreError::InvalidEventCursor);
        }
        let limit = i64::try_from(limit).map_err(|_| StoreError::InvalidEventLimit)?;
        let project_id = project_id.to_owned();
        self.call(move |pool| async move {
            let rows = sqlx::query(
                "SELECT sequence, id, project_id, kind, author, body, validation, created_at \
                 FROM coordinator_entries WHERE project_id = ? AND sequence > ? \
                 ORDER BY sequence LIMIT ?",
            )
            .bind(project_id)
            .bind(after_sequence)
            .bind(limit)
            .fetch_all(&pool)
            .await?;
            rows.into_iter().map(decode_entry).collect()
        })
    }

    /// Load the readable parameters stored with an approval, when present.
    pub fn approval_parameters(&self, approval_id: &str) -> Result<Option<Value>, StoreError> {
        let approval_id = approval_id.to_owned();
        self.call(move |pool| async move {
            let stored: Option<String> = sqlx::query_scalar(
                "SELECT parameters FROM approval_parameters WHERE approval_id = ?",
            )
            .bind(approval_id)
            .fetch_optional(&pool)
            .await?;
            stored
                .map(|text| serde_json::from_str(&text).map_err(StoreError::from))
                .transpose()
        })
    }

    /// Load every approval bound to a task, with its action and state.
    pub fn approval_records_for_task(&self, task_id: &str) -> Result<Vec<Approval>, ApprovalError> {
        let task_id = task_id.to_owned();
        approval_call(self, move |pool| async move {
            let rows = sqlx::query(
                "SELECT id, task_id, action, status, granted_by FROM approvals \
                 WHERE task_id = ? ORDER BY id",
            )
            .bind(task_id)
            .fetch_all(&pool)
            .await
            .map_err(persistence)?;
            rows.into_iter().map(decode_approval).collect()
        })
    }

    /// Approve a pending action unless the reviewer asked for changes.
    ///
    /// The change-request check and the approval commit in one immediate
    /// transaction, so a concurrent request cannot slip between them.
    pub fn approve_reviewed_approval(
        &self,
        id: &str,
        actor: &Actor,
        allowlist: &ApproverAllowlist,
    ) -> Result<Approval, ReviewError> {
        validate_approval_id(id)?;
        validate_actor(actor)?;
        authorize_decision(actor, allowlist)?;
        let id = id.to_owned();
        let actor = actor.clone();
        approval_call_review(self, move |pool| async move {
            let mut transaction = pool
                .begin_with("BEGIN IMMEDIATE")
                .await
                .map_err(persistence)?;
            let current = load_approval(&mut transaction, &id).await?;
            let open: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM change_requests WHERE approval_id = ?")
                    .bind(&id)
                    .fetch_one(&mut *transaction)
                    .await
                    .map_err(persistence)?;
            if open > 0 {
                return Err(ReviewError::ChangesRequested { approval_id: id });
            }
            let event = ApprovalEvent::Approved { by: actor };
            let next = approval::transition(Some(&current.state), &event)?;
            let updated = update_approval(&mut transaction, &current, &event, &next).await?;
            transaction.commit().await.map_err(persistence)?;
            Ok(updated)
        })
    }

    /// Record reviewer feedback against a pending approval and its candidate.
    ///
    /// The approval stays `Requested`: this is neither a rejection nor an
    /// authorization. A durable `review.changes_requested` event with
    /// identifiers only commits in the same transaction.
    pub fn request_changes(
        &self,
        change_id: &str,
        approval_id: &str,
        candidate_commit: &str,
        feedback: &str,
        actor: &Actor,
        allowlist: &ApproverAllowlist,
    ) -> Result<ChangeRequest, ReviewError> {
        validate_approval_id(approval_id)?;
        validate_actor(actor)?;
        authorize_decision(actor, allowlist)?;
        if feedback.trim().is_empty() || feedback.len() > MAX_FEEDBACK_BYTES {
            return Err(ReviewError::InvalidFeedback);
        }
        let change_id = change_id.to_owned();
        let approval_id = approval_id.to_owned();
        let candidate_commit = candidate_commit.to_owned();
        let feedback = feedback.to_owned();
        let requested_by = actor.id().to_owned();
        approval_call_review(self, move |pool| async move {
            let mut transaction = pool
                .begin_with("BEGIN IMMEDIATE")
                .await
                .map_err(persistence)?;
            let current = load_approval(&mut transaction, &approval_id).await?;
            if current.state != ApprovalState::Requested {
                return Err(ApprovalError::IllegalTransition {
                    from: Some(current.state.kind()),
                    event: approval::ApprovalEventKind::Requested,
                }
                .into());
            }
            if current.action.candidate_commit().as_str() != candidate_commit {
                return Err(ReviewError::CandidateMismatch);
            }
            let existing: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM change_requests WHERE approval_id = ?")
                    .bind(&approval_id)
                    .fetch_one(&mut *transaction)
                    .await
                    .map_err(persistence)?;
            if existing > 0 {
                return Err(ReviewError::AlreadyRequested { approval_id });
            }
            let task_id = current.action.task_id().to_owned();
            let row = sqlx::query(
                "INSERT INTO change_requests \
                 (id, task_id, approval_id, candidate_commit, feedback, requested_by) \
                 VALUES (?, ?, ?, ?, ?, ?) \
                 RETURNING id, task_id, approval_id, candidate_commit, feedback, requested_by, created_at",
            )
            .bind(&change_id)
            .bind(&task_id)
            .bind(&approval_id)
            .bind(&candidate_commit)
            .bind(&feedback)
            .bind(&requested_by)
            .fetch_one(&mut *transaction)
            .await
            .map_err(persistence)?;
            let request = decode_change_request(row)?;
            append_durable_event(
                &mut transaction,
                &task_id,
                None,
                "review.changes_requested",
                &serde_json::json!({
                    "change_request_id": change_id,
                    "approval_id": approval_id,
                    "candidate_commit": candidate_commit,
                }),
            )
            .await
            .map_err(ReviewError::Store)?;
            transaction.commit().await.map_err(persistence)?;
            Ok(request)
        })
    }

    /// Load the change request bound to an approval, when one exists.
    pub fn change_request_for_approval(
        &self,
        approval_id: &str,
    ) -> Result<Option<ChangeRequest>, StoreError> {
        let approval_id = approval_id.to_owned();
        self.call(move |pool| async move {
            let row = sqlx::query(
                "SELECT id, task_id, approval_id, candidate_commit, feedback, requested_by, created_at \
                 FROM change_requests WHERE approval_id = ?",
            )
            .bind(approval_id)
            .fetch_optional(&pool)
            .await?;
            row.map(|row| decode_change_request(row).map_err(|error| match error {
                ReviewError::Store(error) => error,
                other => StoreError::Io(std::io::Error::other(other.to_string())),
            }))
            .transpose()
        })
    }
}

fn approval_call_review<T, F, Fut>(store: &SqliteStore, operation: F) -> Result<T, ReviewError>
where
    T: Send + 'static,
    F: FnOnce(sqlx::sqlite::SqlitePool) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<T, ReviewError>> + Send + 'static,
{
    store
        .call(move |pool| async move { Ok(operation(pool).await) })
        .map_err(ReviewError::Store)?
}

fn decode_entry(row: SqliteRow) -> Result<CoordinatorEntry, StoreError> {
    let validation: Option<String> = row.try_get("validation")?;
    Ok(CoordinatorEntry {
        sequence: row.try_get("sequence")?,
        id: row.try_get("id")?,
        project_id: row.try_get("project_id")?,
        kind: row.try_get("kind")?,
        author: row.try_get("author")?,
        body: row.try_get("body")?,
        validation: validation
            .map(|text| serde_json::from_str(&text))
            .transpose()?,
        created_at: row.try_get("created_at")?,
    })
}

fn decode_change_request(row: SqliteRow) -> Result<ChangeRequest, ReviewError> {
    let field = |name: &str| -> Result<String, ReviewError> {
        row.try_get(name)
            .map_err(|error| ReviewError::Store(error.into()))
    };
    Ok(ChangeRequest {
        id: field("id")?,
        task_id: field("task_id")?,
        approval_id: field("approval_id")?,
        candidate_commit: field("candidate_commit")?,
        feedback: field("feedback")?,
        requested_by: field("requested_by")?,
        created_at: field("created_at")?,
    })
}
