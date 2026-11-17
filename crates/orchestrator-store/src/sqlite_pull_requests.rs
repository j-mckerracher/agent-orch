//! Pull requests the daemon watches and the merge ledger (DPES-1105 phase 4c, migration 0051).
//!
//! `pull_requests` holds the last read of one pull request per task: its head, state, the
//! REQUIRED checks and whether GitHub calls it mergeable. A row is written only by
//! [`SqliteStore::observe_pull_request`], which appends the durable event `pr.changed`
//! (the task id only) when something that matters changed. Merged and closed are final.
//!
//! `pull_request_merges` is the ledger of merge attempts, keyed by the approved proposal, so a
//! proposal claims a merge at most once, ever. While one attempt on a pull request is open
//! (claimed, issued or unknown) no second attempt can be claimed for it. The state machine
//! is `claimed -> issued -> merged | unknown | refused`, `claimed -> refused` and
//! `unknown -> merged | refused`; the migration's triggers refuse every other edge, also from
//! another connection. The caller commits `issued` BEFORE the remote call.
//!
//! The store holds no token, URL, diff or command output. A title and a check name come from
//! the remote: this layer strips control characters and caps them before they are written.
//! Time is passed in by the caller.

use std::fmt;
use std::io;

use orchestrator_core::landing::{CheckState, MergeMethod, PrObservation, PrState};
use serde_json::{json, Value};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection};

use crate::sqlite_conversations::corrupt;
use crate::sqlite_events::append_durable_event;
use crate::{SqliteStore, StoreError};

/// Most required checks one row keeps. More is refused, never cut: a cut could drop a red one.
pub const MAX_STORED_CHECKS: usize = 100;
const MAX_TITLE_CHARS: usize = 200;
const MAX_CHECK_NAME_CHARS: usize = 80;
const MAX_REF_CHARS: usize = 255;
const MAX_NUMBER: u64 = 999_999;
const MAX_REFUSAL_CHARS: usize = 64;
const MAX_POLL_FAILURES: i64 = 1_000_000;

const PULL_REQUEST_COLUMNS: &str = "task_id, project_id, number, branch, base_branch, state, \
     head_sha, mergeable, required_total, required_passed, required_failed, required_unknown, \
     checks_json, title, observed_ms, changed_ms, poll_failures";
const MERGE_COLUMNS: &str = "proposal_id, task_id, number, head_sha, method, state, refusal, \
     merge_sha, created_at, updated_at";

/// One required check as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredCheck {
    pub name: String,
    pub state: CheckState,
}

/// What the poller read of one pull request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequestSeen {
    pub task_id: String,
    pub project_id: String,
    pub number: u64,
    pub branch: String,
    pub base_branch: String,
    pub state: PrState,
    /// 40 lower-case hex characters.
    pub head_sha: String,
    /// `None`: not computed yet, or the required checks could not be read.
    pub mergeable: Option<bool>,
    /// The required checks. Empty means none, or unreadable; the merge rule refuses both.
    pub checks: Vec<StoredCheck>,
    /// Remote text; the store strips controls and caps it at 200 characters.
    pub title: String,
}

/// One stored pull request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequestRow {
    pub task_id: String,
    pub project_id: String,
    pub number: u64,
    pub branch: String,
    pub base_branch: String,
    pub state: PrState,
    pub head_sha: String,
    pub mergeable: Option<bool>,
    pub required_total: u32,
    pub required_passed: u32,
    pub required_failed: u32,
    pub required_unknown: u32,
    pub checks: Vec<StoredCheck>,
    pub title: String,
    pub observed_ms: i64,
    pub changed_ms: i64,
    pub poll_failures: u32,
}

impl PullRequestRow {
    /// The row as the pure merge rule reads it.
    pub fn observation(&self) -> PrObservation {
        PrObservation {
            state: self.state,
            head_sha: self.head_sha.clone(),
            mergeable: self.mergeable,
            required_total: self.required_total,
            required_passed: self.required_passed,
            required_failed: self.required_failed,
            required_unknown: self.required_unknown,
            observed_ms: self.observed_ms,
        }
    }
}

/// What [`SqliteStore::observe_pull_request`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PullRequestChange {
    /// First read of this task's pull request. Event appended.
    Created,
    /// Head, state, base, mergeable or the checks changed. Event appended.
    Changed,
    /// Nothing that matters changed; the read time was recorded.
    Unchanged,
    /// The stored pull request is merged or closed, which is final. Nothing was written.
    Settled,
}

/// Why the store refused a pull request or a merge attempt. Carried inside a
/// `StoreError::Io` of kind `InvalidInput`; [`PullRequestError::of`] reads it back. Names no
/// value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PullRequestError {
    /// The number is not 1 to 999999.
    BadNumber,
    /// The head or merge commit is not 40 lower-case hex characters.
    BadSha,
    /// A branch name is empty, over 255 characters or holds a control character.
    BadBranch,
    /// The time is not positive.
    BadTime,
    /// More than 100 required checks.
    TooManyChecks,
    /// The task belongs to another project than the one given.
    ProjectMismatch,
    /// The task already follows another open pull request.
    Retargeted,
    /// Another task already holds this pull request number in the project.
    NumberTaken,
    /// The proposal id is empty or over 128 characters.
    BadProposal,
    /// A refusal needs a code of 1 to 64 characters of `a-z`, `0-9` and `_`, and only a
    /// refused attempt has one.
    BadRefusal,
    /// Only a merged attempt has a merge commit.
    BadMergeSha,
    /// The state change is not an edge of the merge state machine.
    IllegalTransition,
}

impl fmt::Display for PullRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::BadNumber => "a pull request number is 1 to 999999",
            Self::BadSha => "a commit is 40 lower-case hex characters",
            Self::BadBranch => "a branch name is 1 to 255 characters without controls",
            Self::BadTime => "a pull request time must be positive",
            Self::TooManyChecks => "at most 100 required checks are stored",
            Self::ProjectMismatch => "the task belongs to another project",
            Self::Retargeted => "the task already follows another open pull request",
            Self::NumberTaken => "another task already holds this pull request",
            Self::BadProposal => "a merge attempt needs a proposal id of 1 to 128 characters",
            Self::BadRefusal => "a refused merge attempt needs a short refusal code, others none",
            Self::BadMergeSha => "only a merged attempt has a merge commit",
            Self::IllegalTransition => "illegal merge attempt transition",
        })
    }
}

impl std::error::Error for PullRequestError {}

impl From<PullRequestError> for StoreError {
    fn from(error: PullRequestError) -> Self {
        StoreError::Io(io::Error::new(io::ErrorKind::InvalidInput, error))
    }
}

impl PullRequestError {
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

/// State of one merge attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeState {
    /// Recorded, nothing sent.
    Claimed,
    /// Committed before the remote call: the call may have been sent.
    Issued,
    Merged,
    Refused,
    /// The call's reply was lost; only a read of the remote settles it.
    Unknown,
}

impl MergeState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Claimed => "claimed",
            Self::Issued => "issued",
            Self::Merged => "merged",
            Self::Refused => "refused",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [
            Self::Claimed,
            Self::Issued,
            Self::Merged,
            Self::Refused,
            Self::Unknown,
        ]
        .into_iter()
        .find(|state| state.as_str() == value)
    }

    /// Whether the attempt can still change.
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Claimed | Self::Issued | Self::Unknown)
    }

    /// The edges of the state machine; the migration's trigger refuses the rest.
    pub const fn can_become(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Claimed, Self::Issued | Self::Refused)
                | (Self::Issued, Self::Merged | Self::Unknown | Self::Refused)
                | (Self::Unknown, Self::Merged | Self::Refused)
        )
    }
}

/// One merge attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergeAttempt {
    pub proposal_id: String,
    pub task_id: String,
    pub number: u64,
    pub head_sha: String,
    pub method: MergeMethod,
    pub state: MergeState,
    /// Set when refused.
    pub refusal: Option<String>,
    /// Set when merged and known.
    pub merge_sha: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Result of [`SqliteStore::claim_pull_request_merge`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeClaim {
    /// This call recorded the attempt, in state `claimed`. Only the caller of this call may
    /// go on to merge.
    Claimed(MergeAttempt),
    /// The proposal already had an attempt (a second approval, a replay). Do not merge.
    Existing(MergeAttempt),
    /// Another proposal has an open attempt on the same pull request. Do not merge.
    Busy(MergeAttempt),
}

fn valid_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn valid_number(number: u64) -> Result<i64, PullRequestError> {
    if (1..=MAX_NUMBER).contains(&number) {
        i64::try_from(number).map_err(|_| PullRequestError::BadNumber)
    } else {
        Err(PullRequestError::BadNumber)
    }
}

fn valid_ref(value: &str) -> Result<(), PullRequestError> {
    if value.is_empty()
        || value.chars().count() > MAX_REF_CHARS
        || value.chars().any(char::is_control)
    {
        Err(PullRequestError::BadBranch)
    } else {
        Ok(())
    }
}

/// Plain text: controls stripped, cut to `max` characters.
fn plain(text: &str, max: usize) -> String {
    text.chars().filter(|c| !c.is_control()).take(max).collect()
}

fn count(checks: &[StoredCheck], state: CheckState) -> i64 {
    // At most 100 checks, so the count fits.
    i64::try_from(checks.iter().filter(|check| check.state == state).count()).unwrap_or(i64::MAX)
}

fn mergeable_text(mergeable: Option<bool>) -> &'static str {
    match mergeable {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    }
}

fn checks_json(checks: &[StoredCheck]) -> Value {
    Value::Array(
        checks
            .iter()
            .map(|check| json!({ "name": check.name, "result": check.state.as_str() }))
            .collect(),
    )
}

impl SqliteStore {
    /// Records what the poller read of a task's pull request, in one transaction. The first
    /// read inserts the row; a later read updates it and, when the head, state, base,
    /// mergeable answer or checks changed, stamps `changed_ms` and appends the durable event
    /// `pr.changed` (the task id only). A stored pull request that is merged or closed is
    /// final: the call writes nothing and returns `Settled`. A successful read resets the
    /// failure count.
    ///
    /// Refused with a [`PullRequestError`]: a bad number, commit or branch, a non-positive
    /// time, more than 100 checks, a task of another project, a task that already follows a
    /// different open pull request, or a number another task holds. Title and check names
    /// are cleaned and capped here. An unknown task fails with `RecordNotFound`.
    pub fn observe_pull_request(
        &self,
        seen: &PullRequestSeen,
        now_ms: i64,
    ) -> Result<PullRequestChange, StoreError> {
        let number = valid_number(seen.number)?;
        if !valid_sha(&seen.head_sha) {
            return Err(PullRequestError::BadSha.into());
        }
        valid_ref(&seen.branch)?;
        valid_ref(&seen.base_branch)?;
        if now_ms <= 0 {
            return Err(PullRequestError::BadTime.into());
        }
        if seen.checks.len() > MAX_STORED_CHECKS {
            return Err(PullRequestError::TooManyChecks.into());
        }
        let mut seen = seen.clone();
        seen.title = plain(&seen.title, MAX_TITLE_CHARS);
        for check in &mut seen.checks {
            check.name = plain(&check.name, MAX_CHECK_NAME_CHARS);
        }
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let project =
                sqlx::query_scalar::<_, String>("SELECT project_id FROM tasks WHERE id = ?")
                    .bind(&seen.task_id)
                    .fetch_optional(&mut *transaction)
                    .await?
                    .ok_or_else(|| StoreError::RecordNotFound {
                        entity: "task",
                        id: seen.task_id.clone(),
                    })?;
            if project != seen.project_id {
                return Err(StoreError::from(PullRequestError::ProjectMismatch));
            }
            let existing = read_pull_request(&mut transaction, &seen.task_id).await?;
            if let Some(existing) = &existing {
                if existing.state != PrState::Open {
                    return Ok(PullRequestChange::Settled);
                }
                if existing.number != seen.number {
                    return Err(StoreError::from(PullRequestError::Retargeted));
                }
            }
            let checks_text = serde_json::to_string(&checks_json(&seen.checks))?;
            let changed = match &existing {
                None => true,
                Some(row) => {
                    row.state != seen.state
                        || row.head_sha != seen.head_sha
                        || row.base_branch != seen.base_branch
                        || row.mergeable != seen.mergeable
                        || row.checks != seen.checks
                }
            };
            let total = i64::try_from(seen.checks.len()).unwrap_or(i64::MAX);
            let (passed, failed, unknown) = (
                count(&seen.checks, CheckState::Passed),
                count(&seen.checks, CheckState::Failed),
                count(&seen.checks, CheckState::Unknown),
            );
            if existing.is_none() {
                let added = sqlx::query(
                    "INSERT INTO pull_requests \
                     (task_id, project_id, number, branch, base_branch, state, head_sha, \
                      mergeable, required_total, required_passed, required_failed, \
                      required_unknown, checks_json, title, observed_ms, changed_ms) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
                     ON CONFLICT DO NOTHING",
                )
                .bind(&seen.task_id)
                .bind(&seen.project_id)
                .bind(number)
                .bind(&seen.branch)
                .bind(&seen.base_branch)
                .bind(seen.state.as_str())
                .bind(&seen.head_sha)
                .bind(mergeable_text(seen.mergeable))
                .bind(total)
                .bind(passed)
                .bind(failed)
                .bind(unknown)
                .bind(&checks_text)
                .bind(&seen.title)
                .bind(now_ms)
                .bind(now_ms)
                .execute(&mut *transaction)
                .await?
                .rows_affected();
                if added == 0 {
                    return Err(StoreError::from(PullRequestError::NumberTaken));
                }
            } else {
                sqlx::query(
                    "UPDATE pull_requests SET base_branch = ?, state = ?, head_sha = ?, \
                     mergeable = ?, required_total = ?, required_passed = ?, \
                     required_failed = ?, required_unknown = ?, checks_json = ?, title = ?, \
                     observed_ms = ?, changed_ms = CASE WHEN ? THEN ? ELSE changed_ms END, \
                     poll_failures = 0 WHERE task_id = ?",
                )
                .bind(&seen.base_branch)
                .bind(seen.state.as_str())
                .bind(&seen.head_sha)
                .bind(mergeable_text(seen.mergeable))
                .bind(total)
                .bind(passed)
                .bind(failed)
                .bind(unknown)
                .bind(&checks_text)
                .bind(&seen.title)
                .bind(now_ms)
                .bind(changed)
                .bind(now_ms)
                .bind(&seen.task_id)
                .execute(&mut *transaction)
                .await?;
            }
            if changed {
                append_durable_event(
                    &mut transaction,
                    &seen.task_id,
                    None,
                    "pr.changed",
                    &json!({ "task_id": seen.task_id }),
                )
                .await?;
            }
            transaction.commit().await?;
            Ok(match (&existing, changed) {
                (None, _) => PullRequestChange::Created,
                (Some(_), true) => PullRequestChange::Changed,
                (Some(_), false) => PullRequestChange::Unchanged,
            })
        })
    }

    /// Counts one failed read of the task's open pull request and returns the new count, which
    /// stops growing at one million. `None` when the task has no open pull request row.
    pub fn record_pull_request_poll_failure(
        &self,
        task_id: &str,
    ) -> Result<Option<u32>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let changed = sqlx::query(
                "UPDATE pull_requests SET poll_failures = MIN(poll_failures + 1, ?) \
                 WHERE task_id = ? AND state = 'open'",
            )
            .bind(MAX_POLL_FAILURES)
            .bind(&task_id)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
            let failures = if changed == 0 {
                None
            } else {
                let value = sqlx::query_scalar::<_, i64>(
                    "SELECT poll_failures FROM pull_requests WHERE task_id = ?",
                )
                .bind(&task_id)
                .fetch_one(&mut *transaction)
                .await?;
                Some(u32::try_from(value).unwrap_or(u32::MAX))
            };
            transaction.commit().await?;
            Ok(failures)
        })
    }

    /// The pull request of a task, or `None`. Writes nothing.
    pub fn pull_request(&self, task_id: &str) -> Result<Option<PullRequestRow>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            read_pull_request(&mut connection, &task_id).await
        })
    }

    /// Pull requests, most recently changed first, of one project or of all. `limit` bounds
    /// the rows. Writes nothing.
    pub fn list_pull_requests(
        &self,
        project_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<PullRequestRow>, StoreError> {
        let project_id = project_id.map(str::to_owned);
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.call(move |pool| async move {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                // Safe assert: `PULL_REQUEST_COLUMNS` is a fixed list of column names; the
                // project and the limit are bound.
                "SELECT {PULL_REQUEST_COLUMNS} FROM pull_requests \
                 WHERE (?1 IS NULL OR project_id = ?1) \
                 ORDER BY changed_ms DESC, task_id LIMIT ?2"
            )))
            .bind(&project_id)
            .bind(limit)
            .fetch_all(&pool)
            .await?;
            rows.iter().map(pull_request_from_row).collect()
        })
    }

    /// Records a merge attempt for an approved proposal, in state `claimed`. The proposal id
    /// is the key: a second claim of the same proposal returns `Existing` and writes nothing,
    /// and a claim while another proposal has an open attempt on the same pull request returns
    /// `Busy`. Only `Claimed` lets the caller go on to merge.
    ///
    /// The task must have a stored pull request with this number (`RecordNotFound`
    /// otherwise). `head_sha` is the commit the approver bound.
    pub fn claim_pull_request_merge(
        &self,
        proposal_id: &str,
        task_id: &str,
        number: u64,
        head_sha: &str,
        method: MergeMethod,
    ) -> Result<MergeClaim, StoreError> {
        if proposal_id.is_empty() || proposal_id.len() > 128 {
            return Err(PullRequestError::BadProposal.into());
        }
        let number_bound = valid_number(number)?;
        if !valid_sha(head_sha) {
            return Err(PullRequestError::BadSha.into());
        }
        let (proposal_id, task_id, head_sha) = (
            proposal_id.to_owned(),
            task_id.to_owned(),
            head_sha.to_owned(),
        );
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let stored = read_pull_request(&mut transaction, &task_id).await?;
            if stored.is_none_or(|row| row.number != number) {
                return Err(StoreError::RecordNotFound {
                    entity: "pull request",
                    id: task_id,
                });
            }
            let added = sqlx::query(
                "INSERT INTO pull_request_merges (proposal_id, task_id, number, head_sha, method, state) \
                 VALUES (?, ?, ?, ?, ?, 'claimed') ON CONFLICT DO NOTHING",
            )
            .bind(&proposal_id)
            .bind(&task_id)
            .bind(number_bound)
            .bind(&head_sha)
            .bind(method.as_str())
            .execute(&mut *transaction)
            .await?
            .rows_affected();
            let claim = if added > 0 {
                let attempt = read_merge(&mut transaction, &proposal_id)
                    .await?
                    .ok_or_else(|| corrupt("the merge attempt just written is missing".to_owned()))?;
                MergeClaim::Claimed(attempt)
            } else if let Some(attempt) = read_merge(&mut transaction, &proposal_id).await? {
                MergeClaim::Existing(attempt)
            } else {
                let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                    // Safe assert: `MERGE_COLUMNS` is a fixed list of column names.
                    "SELECT {MERGE_COLUMNS} FROM pull_request_merges \
                     WHERE task_id = ? AND number = ? \
                     AND state IN ('claimed', 'issued', 'unknown') ORDER BY rowid LIMIT 1"
                )))
                .bind(&task_id)
                .bind(number_bound)
                .fetch_optional(&mut *transaction)
                .await?;
                let blocking = row
                    .as_ref()
                    .map(merge_from_row)
                    .transpose()?
                    .ok_or_else(|| corrupt("a merge claim conflicted with nothing".to_owned()))?;
                MergeClaim::Busy(blocking)
            };
            transaction.commit().await?;
            Ok(claim)
        })
    }

    /// Moves an attempt to `to` and returns it. `Refused` needs a refusal code (1 to 64
    /// characters of `a-z`, `0-9`, `_`), no other state takes one, and only `Merged` may carry
    /// a merge commit. Moving to the state the attempt already has returns it unchanged. An
    /// edge the state machine does not have, including any move to `Claimed`, is refused with
    /// `IllegalTransition`. An unknown proposal fails with `RecordNotFound`.
    pub fn advance_pull_request_merge(
        &self,
        proposal_id: &str,
        to: MergeState,
        refusal: Option<&str>,
        merge_sha: Option<&str>,
    ) -> Result<MergeAttempt, StoreError> {
        let refusal_ok = refusal.is_some_and(|code| {
            (1..=MAX_REFUSAL_CHARS).contains(&code.len())
                && code
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        });
        if (to == MergeState::Refused) != refusal_ok
            || (refusal.is_some() && to != MergeState::Refused)
        {
            return Err(PullRequestError::BadRefusal.into());
        }
        if let Some(sha) = merge_sha {
            if to != MergeState::Merged || !valid_sha(sha) {
                return Err(PullRequestError::BadMergeSha.into());
            }
        }
        let proposal_id = proposal_id.to_owned();
        let refusal = refusal.map(str::to_owned);
        let merge_sha = merge_sha.map(str::to_owned);
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let current = read_merge(&mut transaction, &proposal_id)
                .await?
                .ok_or_else(|| StoreError::RecordNotFound {
                    entity: "pull request merge",
                    id: proposal_id.clone(),
                })?;
            if current.state == to {
                return Ok(current);
            }
            if !current.state.can_become(to) {
                return Err(StoreError::from(PullRequestError::IllegalTransition));
            }
            sqlx::query(
                "UPDATE pull_request_merges SET state = ?, refusal = ?, merge_sha = ?, \
                 updated_at = CURRENT_TIMESTAMP WHERE proposal_id = ?",
            )
            .bind(to.as_str())
            .bind(&refusal)
            .bind(&merge_sha)
            .bind(&proposal_id)
            .execute(&mut *transaction)
            .await?;
            let saved = read_merge(&mut transaction, &proposal_id)
                .await?
                .ok_or_else(|| corrupt("the merge attempt just updated is missing".to_owned()))?;
            transaction.commit().await?;
            Ok(saved)
        })
    }

    /// The attempt of a proposal, or `None`. Writes nothing.
    pub fn pull_request_merge(
        &self,
        proposal_id: &str,
    ) -> Result<Option<MergeAttempt>, StoreError> {
        let proposal_id = proposal_id.to_owned();
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            read_merge(&mut connection, &proposal_id).await
        })
    }

    /// The most recent attempt on a task's pull request, or `None`. Writes nothing.
    pub fn latest_pull_request_merge(
        &self,
        task_id: &str,
    ) -> Result<Option<MergeAttempt>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                // Safe assert: `MERGE_COLUMNS` is a fixed list of column names; the id is bound.
                "SELECT {MERGE_COLUMNS} FROM pull_request_merges WHERE task_id = ? \
                 ORDER BY rowid DESC LIMIT 1"
            )))
            .bind(&task_id)
            .fetch_optional(&pool)
            .await?;
            row.as_ref().map(merge_from_row).transpose()
        })
    }

    /// The attempts that are still open (claimed, issued or unknown), oldest first, for the
    /// restart reconcile. Writes nothing.
    pub fn open_pull_request_merges(&self) -> Result<Vec<MergeAttempt>, StoreError> {
        self.call(move |pool| async move {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                // Safe assert: `MERGE_COLUMNS` is a fixed list of column names.
                "SELECT {MERGE_COLUMNS} FROM pull_request_merges \
                 WHERE state IN ('claimed', 'issued', 'unknown') ORDER BY rowid"
            )))
            .fetch_all(&pool)
            .await?;
            rows.iter().map(merge_from_row).collect()
        })
    }
}

async fn read_pull_request(
    connection: &mut SqliteConnection,
    task_id: &str,
) -> Result<Option<PullRequestRow>, StoreError> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        // Safe assert: `PULL_REQUEST_COLUMNS` is a fixed list of column names; the id is bound.
        "SELECT {PULL_REQUEST_COLUMNS} FROM pull_requests WHERE task_id = ?"
    )))
    .bind(task_id)
    .fetch_optional(connection)
    .await?;
    row.as_ref().map(pull_request_from_row).transpose()
}

async fn read_merge(
    connection: &mut SqliteConnection,
    proposal_id: &str,
) -> Result<Option<MergeAttempt>, StoreError> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        // Safe assert: `MERGE_COLUMNS` is a fixed list of column names; the id is bound.
        "SELECT {MERGE_COLUMNS} FROM pull_request_merges WHERE proposal_id = ?"
    )))
    .bind(proposal_id)
    .fetch_optional(connection)
    .await?;
    row.as_ref().map(merge_from_row).transpose()
}

fn small(row: &SqliteRow, column: &str) -> Result<u32, StoreError> {
    let value: i64 = row.try_get(column)?;
    u32::try_from(value).map_err(|_| corrupt(format!("{column} is out of range")))
}

fn pull_request_from_row(row: &SqliteRow) -> Result<PullRequestRow, StoreError> {
    let state: String = row.try_get("state")?;
    let state = PrState::parse(&state)
        .ok_or_else(|| corrupt("a stored pull request state is unknown".to_owned()))?;
    let mergeable: String = row.try_get("mergeable")?;
    let mergeable = match mergeable.as_str() {
        "yes" => Some(true),
        "no" => Some(false),
        "unknown" => None,
        _ => return Err(corrupt("a stored mergeable value is unknown".to_owned())),
    };
    let number: i64 = row.try_get("number")?;
    let checks_text: String = row.try_get("checks_json")?;
    Ok(PullRequestRow {
        task_id: row.try_get("task_id")?,
        project_id: row.try_get("project_id")?,
        number: u64::try_from(number).map_err(|_| corrupt("number is out of range".to_owned()))?,
        branch: row.try_get("branch")?,
        base_branch: row.try_get("base_branch")?,
        state,
        head_sha: row.try_get("head_sha")?,
        mergeable,
        required_total: small(row, "required_total")?,
        required_passed: small(row, "required_passed")?,
        required_failed: small(row, "required_failed")?,
        required_unknown: small(row, "required_unknown")?,
        checks: parse_checks(&checks_text)?,
        title: row.try_get("title")?,
        observed_ms: row.try_get("observed_ms")?,
        changed_ms: row.try_get("changed_ms")?,
        poll_failures: small(row, "poll_failures")?,
    })
}

fn parse_checks(text: &str) -> Result<Vec<StoredCheck>, StoreError> {
    let bad = || corrupt("stored pull request checks are unreadable".to_owned());
    let value: Value = serde_json::from_str(text).map_err(|_| bad())?;
    value
        .as_array()
        .ok_or_else(bad)?
        .iter()
        .map(|item| {
            let name = item.get("name").and_then(Value::as_str).ok_or_else(bad)?;
            let state = item
                .get("result")
                .and_then(Value::as_str)
                .and_then(CheckState::parse)
                .ok_or_else(bad)?;
            Ok::<_, StoreError>(StoredCheck {
                name: name.to_owned(),
                state,
            })
        })
        .collect()
}

fn merge_from_row(row: &SqliteRow) -> Result<MergeAttempt, StoreError> {
    let state: String = row.try_get("state")?;
    let method: String = row.try_get("method")?;
    let number: i64 = row.try_get("number")?;
    Ok(MergeAttempt {
        proposal_id: row.try_get("proposal_id")?,
        task_id: row.try_get("task_id")?,
        number: u64::try_from(number).map_err(|_| corrupt("number is out of range".to_owned()))?,
        head_sha: row.try_get("head_sha")?,
        method: MergeMethod::parse(&method)
            .ok_or_else(|| corrupt("a stored merge method is unknown".to_owned()))?,
        state: MergeState::parse(&state)
            .ok_or_else(|| corrupt("a stored merge state is unknown".to_owned()))?,
        refusal: row.try_get("refusal")?,
        merge_sha: row.try_get("merge_sha")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
