//! Held decisions (DPES-1105 phase 4d, migration 0045).
//!
//! A hold hides a decision from the actionable queue until a date. It never approves, rejects
//! or expires anything, and it holds no text. The caller names the date (`YYYY-MM-DD`) and its
//! end in UTC milliseconds; the store checks that the two agree and that the end is after the
//! caller's `now_ms`. A hold "expires" by that clock alone: [`SqliteStore::decision_holds_in_force`]
//! compares `until_ms` with the `now_ms` it is given, so nothing runs and nothing is written
//! when a date passes. The store reads no clock for logic.
//!
//! A hold is never deleted. A new date is a release plus a new hold, in one transaction.
//!
//! Rows of the subject kind `question` (migration 0064) share the table but are not holds:
//! every read here leaves them out, and `sqlite_questions.rs` owns them.

use std::fmt;
use std::io;

use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection};

use crate::sqlite_conversations::corrupt;
use crate::{SqliteStore, StoreError};

const MS_PER_DAY: i64 = 86_400_000;
/// 2100-01-01 in milliseconds; migration 0045 bounds `until_ms` by the same number.
const MAX_UNTIL_MS: i64 = 4_102_444_800_000;
const COLUMNS: &str =
    "id, subject_kind, subject_id, until_date, until_ms, state, created_at, released_at";

/// What a hold covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldSubject {
    /// Every pending approval of one task; the subject id is the task id.
    Approval,
    /// One action card; the subject id is the proposal id.
    Proposal,
}

impl HoldSubject {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approval => "approval",
            Self::Proposal => "proposal",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "approval" => Some(Self::Approval),
            "proposal" => Some(Self::Proposal),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldState {
    Active,
    Released,
}

impl HoldState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Released => "released",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "released" => Some(Self::Released),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionHold {
    pub id: String,
    pub subject_kind: HoldSubject,
    pub subject_id: String,
    /// `YYYY-MM-DD`.
    pub until_date: String,
    /// End of `until_date` in UTC milliseconds.
    pub until_ms: i64,
    pub state: HoldState,
    pub created_at: String,
    pub released_at: Option<String>,
}

/// Why the store refused a hold. Carried inside a `StoreError::Io` of kind `InvalidInput`;
/// [`HoldError::of`] reads it back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldError {
    /// The hold id or the subject id is empty or too long.
    BadId,
    /// The date is not a real `YYYY-MM-DD` before 2100, or `until_ms` is not its UTC day end.
    BadDate,
    /// The end of the day is not after the caller's clock.
    NotInFuture,
    /// The task has no requested approval, or the proposal is not pending.
    NothingOpen,
    /// A hold with this id already exists and is not the one being asked for.
    IdInUse,
}

impl fmt::Display for HoldError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::BadId => {
                "the hold id and the subject id must be 1 to 128 and 1 to 160 characters"
            }
            Self::BadDate => {
                "the hold date must be a real YYYY-MM-DD before 2100 with its UTC day end"
            }
            Self::NotInFuture => "the hold must end after now",
            Self::NothingOpen => "there is no open decision to hold",
            Self::IdInUse => "a hold with this id already exists",
        })
    }
}

impl std::error::Error for HoldError {}

impl From<HoldError> for StoreError {
    fn from(error: HoldError) -> Self {
        StoreError::Io(io::Error::new(io::ErrorKind::InvalidInput, error))
    }
}

impl HoldError {
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
    /// Holds a subject until `until_date`, in one transaction. An active hold for the same
    /// subject with the same date is returned unchanged (a repeat writes nothing). One with a
    /// different date is released and replaced by this one. `until_ms` must be the end of
    /// `until_date` in UTC and after `now_ms`. The subject must still be open, as the store
    /// reads it: for [`HoldSubject::Approval`] the task has a `requested` approval, for
    /// [`HoldSubject::Proposal`] the proposal is `pending`; otherwise `NothingOpen`. Range
    /// limits (today to 60 days) belong to the caller.
    pub fn set_decision_hold(
        &self,
        id: &str,
        subject_kind: HoldSubject,
        subject_id: &str,
        until_date: &str,
        until_ms: i64,
        now_ms: i64,
    ) -> Result<DecisionHold, StoreError> {
        if !(1..=128).contains(&id.chars().count())
            || !(1..=160).contains(&subject_id.chars().count())
        {
            return Err(HoldError::BadId.into());
        }
        if end_of_day_ms(until_date) != Some(until_ms) || until_ms > MAX_UNTIL_MS {
            return Err(HoldError::BadDate.into());
        }
        if until_ms <= now_ms {
            return Err(HoldError::NotInFuture.into());
        }
        let id = id.to_owned();
        let subject_id = subject_id.to_owned();
        let until_date = until_date.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let same = active_for(&mut transaction, subject_kind, &subject_id)
                .await?
                .filter(|current| current.until_date == until_date);
            if let Some(current) = same {
                return Ok(current);
            }
            if read_hold(&mut transaction, &id).await?.is_some() {
                return Err(StoreError::from(HoldError::IdInUse));
            }
            if !subject_is_open(&mut transaction, subject_kind, &subject_id).await? {
                return Err(StoreError::from(HoldError::NothingOpen));
            }
            release_active(&mut transaction, subject_kind, &subject_id).await?;
            sqlx::query(
                "INSERT INTO decision_holds (id, subject_kind, subject_id, until_date, until_ms, set_by) \
                 VALUES (?, ?, ?, ?, ?, 'user')",
            )
            .bind(&id)
            .bind(subject_kind.as_str())
            .bind(&subject_id)
            .bind(&until_date)
            .bind(until_ms)
            .execute(&mut *transaction)
            .await?;
            let saved = read_hold(&mut transaction, &id)
                .await?
                .ok_or_else(|| corrupt("the decision hold just written is missing".to_owned()))?;
            transaction.commit().await?;
            Ok(saved)
        })
    }

    /// Releases one hold by id. A hold already released is returned unchanged. An unknown id
    /// fails with `RecordNotFound`.
    pub fn release_decision_hold(&self, hold_id: &str) -> Result<DecisionHold, StoreError> {
        let hold_id = hold_id.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let hold = read_hold(&mut transaction, &hold_id)
                .await?
                .ok_or_else(|| StoreError::RecordNotFound {
                    entity: "decision hold",
                    id: hold_id.clone(),
                })?;
            if hold.state == HoldState::Released {
                return Ok(hold);
            }
            release_active(&mut transaction, hold.subject_kind, &hold.subject_id).await?;
            let saved = read_hold(&mut transaction, &hold_id)
                .await?
                .ok_or_else(|| corrupt("the decision hold just released is missing".to_owned()))?;
            transaction.commit().await?;
            Ok(saved)
        })
    }

    /// Releases the active hold of a subject, if any, and returns it as released.
    pub fn clear_decision_hold(
        &self,
        subject_kind: HoldSubject,
        subject_id: &str,
    ) -> Result<Option<DecisionHold>, StoreError> {
        let subject_id = subject_id.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let Some(current) = active_for(&mut transaction, subject_kind, &subject_id).await?
            else {
                return Ok(None);
            };
            release_active(&mut transaction, subject_kind, &subject_id).await?;
            let saved = read_hold(&mut transaction, &current.id).await?;
            transaction.commit().await?;
            Ok(saved)
        })
    }

    /// Releases the active `proposal` holds of the given proposals. For history clear, which
    /// scrubs the proposal ids. Returns how many holds were released.
    pub fn release_proposal_holds(&self, proposal_ids: &[String]) -> Result<usize, StoreError> {
        let proposal_ids = proposal_ids.to_vec();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let released = release_proposal_holds_in(&mut transaction, &proposal_ids).await?;
            transaction.commit().await?;
            Ok(released)
        })
    }

    /// One hold by id, released or not.
    pub fn decision_hold(&self, hold_id: &str) -> Result<Option<DecisionHold>, StoreError> {
        let hold_id = hold_id.to_owned();
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            read_hold(&mut connection, &hold_id).await
        })
    }

    /// Every active hold, expired or not, oldest first. This is `holds.list`.
    pub fn active_decision_holds(&self) -> Result<Vec<DecisionHold>, StoreError> {
        self.call(|pool| async move {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                // Safe assert: `COLUMNS` is a fixed list of column names; nothing is bound in.
                "SELECT {COLUMNS} FROM decision_holds WHERE state = 'active' \
                 AND subject_kind <> 'question' ORDER BY rowid"
            )))
            .fetch_all(&pool)
            .await?;
            rows.iter().map(hold_from_row).collect()
        })
    }

    /// The active holds whose end is after `now_ms`: the decisions that are hidden right now.
    /// A hold past its date drops out by this comparison alone.
    pub fn decision_holds_in_force(&self, now_ms: i64) -> Result<Vec<DecisionHold>, StoreError> {
        self.call(move |pool| async move {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                // Safe assert: `COLUMNS` is a fixed list of column names; `now_ms` is bound.
                "SELECT {COLUMNS} FROM decision_holds WHERE state = 'active' AND until_ms > ? \
                 ORDER BY rowid"
            )))
            .bind(now_ms)
            .fetch_all(&pool)
            .await?;
            rows.iter().map(hold_from_row).collect()
        })
    }
}

/// Releases the active proposal holds of `proposal_ids` on `connection`, so a caller already in
/// a transaction (history clear) can do it in that transaction.
pub(crate) async fn release_proposal_holds_in(
    connection: &mut SqliteConnection,
    proposal_ids: &[String],
) -> Result<usize, StoreError> {
    let mut released = 0;
    for proposal_id in proposal_ids {
        let result = sqlx::query(
            "UPDATE decision_holds SET state = 'released', released_at = CURRENT_TIMESTAMP \
             WHERE subject_kind = 'proposal' AND subject_id = ? AND state = 'active'",
        )
        .bind(proposal_id)
        .execute(&mut *connection)
        .await?;
        released += usize::try_from(result.rows_affected()).unwrap_or(0);
    }
    Ok(released)
}

async fn release_active(
    connection: &mut SqliteConnection,
    subject_kind: HoldSubject,
    subject_id: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE decision_holds SET state = 'released', released_at = CURRENT_TIMESTAMP \
         WHERE subject_kind = ? AND subject_id = ? AND state = 'active'",
    )
    .bind(subject_kind.as_str())
    .bind(subject_id)
    .execute(connection)
    .await?;
    Ok(())
}

async fn subject_is_open(
    connection: &mut SqliteConnection,
    subject_kind: HoldSubject,
    subject_id: &str,
) -> Result<bool, StoreError> {
    let sql = match subject_kind {
        HoldSubject::Approval => {
            "SELECT 1 FROM approvals WHERE task_id = ? AND status = 'requested' LIMIT 1"
        }
        HoldSubject::Proposal => {
            "SELECT 1 FROM action_proposals WHERE id = ? AND state = 'pending'"
        }
    };
    Ok(sqlx::query_scalar::<_, i64>(sql)
        .bind(subject_id)
        .fetch_optional(connection)
        .await?
        .is_some())
}

async fn active_for(
    connection: &mut SqliteConnection,
    subject_kind: HoldSubject,
    subject_id: &str,
) -> Result<Option<DecisionHold>, StoreError> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        // Safe assert: `COLUMNS` is a fixed list of column names; the values are bound.
        "SELECT {COLUMNS} FROM decision_holds \
         WHERE subject_kind = ? AND subject_id = ? AND state = 'active'"
    )))
    .bind(subject_kind.as_str())
    .bind(subject_id)
    .fetch_optional(connection)
    .await?;
    row.as_ref().map(hold_from_row).transpose()
}

async fn read_hold(
    connection: &mut SqliteConnection,
    hold_id: &str,
) -> Result<Option<DecisionHold>, StoreError> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        // Safe assert: `COLUMNS` is a fixed list of column names; the id is bound.
        "SELECT {COLUMNS} FROM decision_holds WHERE id = ? AND subject_kind <> 'question'"
    )))
    .bind(hold_id)
    .fetch_optional(connection)
    .await?;
    row.as_ref().map(hold_from_row).transpose()
}

fn hold_from_row(row: &SqliteRow) -> Result<DecisionHold, StoreError> {
    let kind: String = row.try_get("subject_kind")?;
    let state: String = row.try_get("state")?;
    Ok(DecisionHold {
        id: row.try_get("id")?,
        subject_kind: HoldSubject::parse(&kind)
            .ok_or_else(|| corrupt(format!("unknown hold subject kind {kind:?}")))?,
        subject_id: row.try_get("subject_id")?,
        until_date: row.try_get("until_date")?,
        until_ms: row.try_get("until_ms")?,
        state: HoldState::parse(&state)
            .ok_or_else(|| corrupt(format!("unknown hold state {state:?}")))?,
        created_at: crate::instant::instant(&row, "created_at")?,
        released_at: crate::instant::opt_instant(&row, "released_at")?,
    })
}

/// The last millisecond of `date` (`YYYY-MM-DD`, a real calendar day) in UTC, or `None`.
pub(crate) fn end_of_day_ms(date: &str) -> Option<i64> {
    let shaped = date.len() == 10
        && date.bytes().enumerate().all(|(index, byte)| {
            if index == 4 || index == 7 {
                byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        });
    if !shaped {
        return None;
    }
    let year: i64 = date.get(0..4)?.parse().ok()?;
    let month: i64 = date.get(5..7)?.parse().ok()?;
    let day: i64 = date.get(8..10)?.parse().ok()?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if !(1..=month_days).contains(&day) {
        return None;
    }
    // Days since 1970-01-01 for a civil date (proleptic Gregorian).
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = shifted_year.div_euclid(400);
    let year_of_era = shifted_year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    Some(days * MS_PER_DAY + MS_PER_DAY - 1)
}
