//! Wake records and the wake turn ledger (DPES-1105 phase 3b, migration 0041).
//!
//! A wake record is one classified event. The daemon writes its notice from a fixed template
//! (ids, a short title, a state, never agent text) and the store keeps it. A record that needs
//! a model turn moves `pending` to `running` to a final state, and migration 0041 holds the
//! only legal edges as triggers. `wake_turn_ledger` has one row per wake turn started, so the
//! hourly and daily caps are counted from it and survive a history clear.
//!
//! Exactly-once rules:
//!
//! - A record's id is a hash of its scope and dedupe key, and the pair is unique, so recording
//!   an event again changes nothing.
//! - `record_wake_events` moves the wake cursor in the same transaction that inserts the
//!   records for the events before it.
//! - `claim_wake_batch` is idempotent on the batch id, and the turn id (the ledger primary key)
//!   derives from it, so a crash after the claim never produces a second turn or ledger row.

use std::io;

use orchestrator_core::supervision::{
    wake_record_id, MonitoringMode, WakeUsage, DAY_MS, HOUR_MS, MAX_BATCH,
};
use sqlx::sqlite::{Sqlite, SqliteRow};
use sqlx::{Executor, Row, SqliteConnection};

use crate::sqlite_conversations::corrupt;
use crate::{SqliteStore, StoreError};

/// Records of one scope that `wake_records_page` returns at most.
pub const WAKE_PAGE_SIZE: usize = 50;
/// Most stale proposals `stale_pending_proposals` returns.
const STALE_PROPOSAL_LIMIT: i64 = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeKind {
    TaskFailed,
    TaskStalled,
    DeliveryWaiting,
    ProposalWaiting,
    SignInNeeded,
    DaemonStart,
    WorkCheck,
    ScoutReportReady,
    /// The default agent failed wake turns in a row. A notice only, it never starts a turn.
    AgentFailing,
    /// A task's agent exited with changes ready for review. A wake turn reports it.
    CandidateReady,
    /// A task's worker asked a question only the user can answer and waits (DPES-1108 phase 4).
    WorkerNeedsDecision,
    /// A task's worker reported that it is blocked and waits for help.
    WorkerBlocked,
    /// Work the global orchestrator routed to a project's coordinator (DPES-1108 phase 7). Its
    /// record is in the coordinator's scope.
    RoutedRequest,
    /// A coordinator's report for the global orchestrator: a fact the daemon wrote or the
    /// coordinator's own judgment. Its record is in the global scope.
    CoordinatorReport,
}

impl WakeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TaskFailed => "task_failed",
            Self::TaskStalled => "task_stalled",
            Self::DeliveryWaiting => "delivery_waiting",
            Self::ProposalWaiting => "proposal_waiting",
            Self::SignInNeeded => "sign_in_needed",
            Self::DaemonStart => "daemon_start",
            Self::WorkCheck => "work_check",
            Self::ScoutReportReady => "scout_report_ready",
            Self::AgentFailing => "agent_failing",
            Self::CandidateReady => "candidate_ready",
            Self::WorkerNeedsDecision => "worker_needs_decision",
            Self::WorkerBlocked => "worker_blocked",
            Self::RoutedRequest => "routed_request",
            Self::CoordinatorReport => "coordinator_report",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "task_failed" => Some(Self::TaskFailed),
            "task_stalled" => Some(Self::TaskStalled),
            "delivery_waiting" => Some(Self::DeliveryWaiting),
            "proposal_waiting" => Some(Self::ProposalWaiting),
            "sign_in_needed" => Some(Self::SignInNeeded),
            "daemon_start" => Some(Self::DaemonStart),
            "work_check" => Some(Self::WorkCheck),
            "scout_report_ready" => Some(Self::ScoutReportReady),
            "agent_failing" => Some(Self::AgentFailing),
            "candidate_ready" => Some(Self::CandidateReady),
            "worker_needs_decision" => Some(Self::WorkerNeedsDecision),
            "worker_blocked" => Some(Self::WorkerBlocked),
            "routed_request" => Some(Self::RoutedRequest),
            "coordinator_report" => Some(Self::CoordinatorReport),
            _ => None,
        }
    }

    /// Whether this kind of event is interpreted by a wake turn. Every other kind is a notice.
    /// A stall is a notice only: the stall check never starts a model turn and never counts
    /// against the wake caps (decisions-3).
    pub const fn needs_model(self) -> bool {
        matches!(
            self,
            Self::TaskFailed
                | Self::DaemonStart
                | Self::WorkCheck
                | Self::ScoutReportReady
                | Self::CandidateReady
                | Self::WorkerNeedsDecision
                | Self::WorkerBlocked
                | Self::RoutedRequest
                | Self::CoordinatorReport
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeState {
    Noticed,
    Pending,
    Running,
    Handled,
    Skipped,
    Failed,
}

impl WakeState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Noticed => "noticed",
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Handled => "handled",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "noticed" => Some(Self::Noticed),
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "handled" => Some(Self::Handled),
            "skipped" => Some(Self::Skipped),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeOutcome {
    Replied,
    Quiet,
    /// The wake turn ended without an answer the user can read. The user is told.
    Unanswered,
}

impl WakeOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Replied => "replied",
            Self::Quiet => "quiet",
            Self::Unanswered => "unanswered",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "replied" => Some(Self::Replied),
            "quiet" => Some(Self::Quiet),
            "unanswered" => Some(Self::Unanswered),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    ModeOff,
    LimitHourly,
    LimitDaily,
    LimitTokens,
    /// The default agent is failing: automatic updates are paused.
    AgentFailing,
}

impl SkipReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ModeOff => "mode_off",
            Self::LimitHourly => "limit_hourly",
            Self::LimitDaily => "limit_daily",
            Self::LimitTokens => "limit_tokens",
            Self::AgentFailing => "agent_failing",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "mode_off" => Some(Self::ModeOff),
            "limit_hourly" => Some(Self::LimitHourly),
            "limit_daily" => Some(Self::LimitDaily),
            "limit_tokens" => Some(Self::LimitTokens),
            "agent_failing" => Some(Self::AgentFailing),
            _ => None,
        }
    }
}

/// An event to record. The notice, subject and dedupe key are written by the daemon from fixed
/// templates and ids; never agent text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewWakeRecord {
    pub scope: String,
    pub kind: WakeKind,
    pub subject: String,
    pub dedupe_key: String,
    pub notice: String,
    /// Normally `kind.needs_model()`. False for the notice-only copy that a workstream event
    /// leaves in Global.
    pub needs_model: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WakeRecord {
    pub id: String,
    pub scope: String,
    pub kind: WakeKind,
    pub subject: String,
    pub dedupe_key: String,
    pub notice: String,
    pub needs_model: bool,
    pub state: WakeState,
    pub outcome: Option<WakeOutcome>,
    pub skip_reason: Option<SkipReason>,
    pub batch_id: Option<String>,
    pub turn_id: Option<String>,
    pub attempts: u32,
    pub anchor_sequence: i64,
    pub notify_reply: bool,
    pub seen: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// How a running batch ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Settle {
    Handled(WakeOutcome),
    Failed,
    /// Back to pending with no batch. `count_attempt` adds one to the attempt count (at most 2).
    Requeue {
        count_attempt: bool,
    },
    Skipped(SkipReason),
}

/// A pending proposal that has waited past a cutoff and has no reminder yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaleProposal {
    pub id: String,
    pub scope: String,
    /// The proposal kind label, for example `workstream.create`.
    pub kind: String,
    pub created_ms: u64,
}

macro_rules! wake_columns {
    () => {
        "id, scope, event_kind, subject, dedupe_key, notice, needs_model, state, outcome, \
         skip_reason, batch_id, turn_id, attempts, anchor_sequence, notify_reply, seen, \
         created_at, updated_at"
    };
}

/// A failed wake turn, as `consecutive_failed_wake_turns` reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WakeFailure {
    pub turn_id: String,
    pub error_code: String,
    /// When the turn's batch was claimed, on the caller's clock. `None` once the ledger row
    /// has been pruned.
    pub started_ms: Option<u64>,
}

fn decode_failure(row: &SqliteRow) -> Result<WakeFailure, StoreError> {
    let error_code: Option<String> = row.try_get("error_code")?;
    let started_ms: Option<i64> = row.try_get("started_ms")?;
    let started_ms = started_ms
        .map(|value| u64::try_from(value).map_err(|_| corrupt("negative wake start".to_owned())))
        .transpose()?;
    Ok(WakeFailure {
        turn_id: row.try_get("id")?,
        error_code: error_code.ok_or_else(|| corrupt("failed wake turn has no code".to_owned()))?,
        started_ms,
    })
}

fn invalid(detail: &'static str) -> StoreError {
    StoreError::Io(io::Error::new(io::ErrorKind::InvalidInput, detail))
}

fn to_i64(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| invalid("time exceeds the storable range"))
}

fn decode_wake(row: SqliteRow) -> Result<WakeRecord, StoreError> {
    let kind: String = row.try_get("event_kind")?;
    let state: String = row.try_get("state")?;
    let outcome: Option<String> = row.try_get("outcome")?;
    let skip_reason: Option<String> = row.try_get("skip_reason")?;
    let attempts: i64 = row.try_get("attempts")?;
    let bad = |what: &str, value: &str| corrupt(format!("unknown wake {what} {value}"));
    Ok(WakeRecord {
        id: row.try_get("id")?,
        scope: row.try_get("scope")?,
        kind: WakeKind::parse(&kind).ok_or_else(|| bad("kind", &kind))?,
        subject: row.try_get("subject")?,
        dedupe_key: row.try_get("dedupe_key")?,
        notice: row.try_get("notice")?,
        needs_model: row.try_get("needs_model")?,
        state: WakeState::parse(&state).ok_or_else(|| bad("state", &state))?,
        outcome: outcome
            .map(|value| WakeOutcome::parse(&value).ok_or_else(|| bad("outcome", &value)))
            .transpose()?,
        skip_reason: skip_reason
            .map(|value| SkipReason::parse(&value).ok_or_else(|| bad("skip reason", &value)))
            .transpose()?,
        batch_id: row.try_get("batch_id")?,
        turn_id: row.try_get("turn_id")?,
        attempts: u32::try_from(attempts).unwrap_or(0),
        anchor_sequence: row.try_get("anchor_sequence")?,
        notify_reply: row.try_get("notify_reply")?,
        seen: row.try_get("seen")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

async fn load_wake(connection: &mut SqliteConnection, id: &str) -> Result<WakeRecord, StoreError> {
    let row = sqlx::query(concat!(
        "SELECT ",
        wake_columns!(),
        " FROM wake_records WHERE id = ?"
    ))
    .bind(id)
    .fetch_one(connection)
    .await?;
    decode_wake(row)
}

/// Ends the running records whose `column` equals `key`, one way. Returns how many changed.
/// Every settle is one UPDATE whose only edge is `running`, so batch and record settles share it.
async fn settle_running<'c, E>(
    executor: E,
    settle: Settle,
    column: &str,
    key: &str,
) -> Result<u64, StoreError>
where
    E: Executor<'c, Database = Sqlite>,
{
    let set = match settle {
        Settle::Handled(_) => "state = 'handled', outcome = ?, updated_at = CURRENT_TIMESTAMP",
        Settle::Failed => "state = 'failed', updated_at = CURRENT_TIMESTAMP",
        Settle::Requeue { .. } => {
            "state = 'pending', batch_id = NULL, turn_id = NULL, \
             attempts = MIN(attempts + ?, 2), updated_at = CURRENT_TIMESTAMP"
        }
        Settle::Skipped(_) => "state = 'skipped', skip_reason = ?, updated_at = CURRENT_TIMESTAMP",
    };
    // Only fixed fragments are interpolated; the key and the outcome, reason or count are bound.
    let sql = format!("UPDATE wake_records SET {set} WHERE {column} = ? AND state = 'running'");
    let query = sqlx::query(sqlx::AssertSqlSafe(sql));
    let query = match settle {
        Settle::Handled(outcome) => query.bind(outcome.as_str()),
        Settle::Failed => query,
        Settle::Requeue { count_attempt } => query.bind(i64::from(count_attempt)),
        Settle::Skipped(reason) => query.bind(reason.as_str()),
    };
    Ok(query.bind(key).execute(executor).await?.rows_affected())
}

async fn load_batch(
    connection: &mut SqliteConnection,
    batch_id: &str,
) -> Result<Vec<WakeRecord>, StoreError> {
    sqlx::query(concat!(
        "SELECT ",
        wake_columns!(),
        " FROM wake_records WHERE batch_id = ? ORDER BY rowid"
    ))
    .bind(batch_id)
    .fetch_all(connection)
    .await?
    .into_iter()
    .map(decode_wake)
    .collect()
}

/// Deletes a scope's wake records. Runs inside the history clear while its gate is open; the
/// delete trigger refuses it any other time. The ledger is never touched.
pub(crate) async fn delete_scope_wake_records(
    connection: &mut SqliteConnection,
    scope: &str,
) -> Result<u64, StoreError> {
    let deleted = sqlx::query("DELETE FROM wake_records WHERE scope = ?")
        .bind(scope)
        .execute(connection)
        .await?;
    Ok(deleted.rows_affected())
}

impl SqliteStore {
    /// ONE transaction: inserts each new record (id is a hash of scope and dedupe key;
    /// `INSERT OR IGNORE` on the unique pair, so a replay is a no-op), then moves the wake
    /// cursor to `cursor_to` (never backwards). Initial state follows the mode: a record that
    /// needs no model is `Noticed`; one that needs a model while the mode runs no wake turns
    /// is `Skipped(ModeOff)`; any other is `Pending`. `anchor_sequence` is the scope's newest
    /// transcript sequence at that moment, 0 when it has none. Returns only the records this
    /// call inserted. Any failure rolls back the records and the cursor together.
    pub fn record_wake_events(
        &self,
        records: &[NewWakeRecord],
        mode: MonitoringMode,
        cursor_to: i64,
    ) -> Result<Vec<WakeRecord>, StoreError> {
        if cursor_to < 0 {
            return Err(StoreError::InvalidEventCursor);
        }
        let records = records.to_vec();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let mut inserted = Vec::new();
            for record in &records {
                let id = wake_record_id(&record.scope, &record.dedupe_key);
                let (state, skip_reason) = if !record.needs_model {
                    (WakeState::Noticed, None)
                } else if !mode.runs_wake_turns() {
                    (WakeState::Skipped, Some(SkipReason::ModeOff))
                } else {
                    (WakeState::Pending, None)
                };
                let anchor: i64 = sqlx::query_scalar(
                    "SELECT COALESCE(MAX(sequence), 0) FROM conversation_entries \
                     WHERE conversation_id = ?",
                )
                .bind(&record.scope)
                .fetch_one(&mut *transaction)
                .await?;
                let added = sqlx::query(
                    // `ON CONFLICT DO NOTHING` skips only a replayed event (the primary key or the
                    // unique pair). `OR IGNORE` would also swallow a CHECK failure and lose the event
                    // while the cursor moved past it.
                    "INSERT INTO wake_records \
                     (id, scope, event_kind, subject, dedupe_key, notice, needs_model, state, \
                      skip_reason, anchor_sequence) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT DO NOTHING",
                )
                .bind(&id)
                .bind(&record.scope)
                .bind(record.kind.as_str())
                .bind(&record.subject)
                .bind(&record.dedupe_key)
                .bind(&record.notice)
                .bind(record.needs_model)
                .bind(state.as_str())
                .bind(skip_reason.map(SkipReason::as_str))
                .bind(anchor)
                .execute(&mut *transaction)
                .await?;
                if added.rows_affected() == 1 {
                    inserted.push(load_wake(&mut transaction, &id).await?);
                }
            }
            sqlx::query(
                "UPDATE orchestrator_settings SET wake_cursor = MAX(wake_cursor, ?) WHERE id = 1",
            )
            .bind(cursor_to)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            Ok(inserted)
        })
    }

    /// The last durable event sequence the classifier has handled.
    pub fn wake_cursor(&self) -> Result<i64, StoreError> {
        self.call(|pool| async move {
            Ok(
                sqlx::query_scalar("SELECT wake_cursor FROM orchestrator_settings WHERE id = 1")
                    .fetch_one(&pool)
                    .await?,
            )
        })
    }

    /// Records waiting for a wake turn, oldest first.
    pub fn pending_wake_records(&self) -> Result<Vec<WakeRecord>, StoreError> {
        self.wake_records_in_state("pending")
    }

    /// Records in a claimed batch, oldest first.
    pub fn running_wake_records(&self) -> Result<Vec<WakeRecord>, StoreError> {
        self.wake_records_in_state("running")
    }

    fn wake_records_in_state(&self, state: &'static str) -> Result<Vec<WakeRecord>, StoreError> {
        self.call(move |pool| async move {
            sqlx::query(concat!(
                "SELECT ",
                wake_columns!(),
                " FROM wake_records WHERE state = ? ORDER BY rowid"
            ))
            .bind(state)
            .fetch_all(&pool)
            .await?
            .into_iter()
            .map(decode_wake)
            .collect()
        })
    }

    /// ONE transaction: moves the pending records `ids` (all in `scope`) to running, sets
    /// their batch id, turn id and notify flag, and inserts the ledger row
    /// `(turn_id, scope, now_ms)`. Idempotent on `batch_id`: a second call returns the same
    /// records and adds no ledger row. Returns nothing, and claims nothing, when the scope
    /// already has a running batch under another id (one wake turn per scope at a time) or
    /// none of `ids` is still pending.
    pub fn claim_wake_batch(
        &self,
        scope: &str,
        ids: &[String],
        batch_id: &str,
        turn_id: &str,
        notify_reply: bool,
        now_ms: u64,
    ) -> Result<Vec<WakeRecord>, StoreError> {
        if batch_id.is_empty() || turn_id.is_empty() || scope.is_empty() {
            return Err(invalid("wake batch needs a scope, batch id and turn id"));
        }
        if ids.len() > MAX_BATCH {
            return Err(invalid("wake batch is over the batch limit"));
        }
        let started = to_i64(now_ms)?;
        let (scope, ids) = (scope.to_owned(), ids.to_vec());
        let (batch_id, turn_id) = (batch_id.to_owned(), turn_id.to_owned());
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let existing = load_batch(&mut transaction, &batch_id).await?;
            if !existing.is_empty() {
                transaction.commit().await?;
                return Ok(existing);
            }
            let busy: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM wake_records WHERE scope = ? AND state = 'running'",
            )
            .bind(&scope)
            .fetch_one(&mut *transaction)
            .await?;
            if busy > 0 {
                transaction.commit().await?;
                return Ok(Vec::new());
            }
            let mut claimed = 0;
            for id in &ids {
                claimed += sqlx::query(
                    "UPDATE wake_records SET state = 'running', batch_id = ?, turn_id = ?, \
                     notify_reply = ?, updated_at = CURRENT_TIMESTAMP \
                     WHERE id = ? AND scope = ? AND state = 'pending'",
                )
                .bind(&batch_id)
                .bind(&turn_id)
                .bind(notify_reply)
                .bind(id)
                .bind(&scope)
                .execute(&mut *transaction)
                .await?
                .rows_affected();
            }
            if claimed == 0 {
                transaction.commit().await?;
                return Ok(Vec::new());
            }
            sqlx::query(
                // Only a replayed claim is skipped; a bad row must fail, because the caps count this ledger.
                "INSERT INTO wake_turn_ledger (turn_id, scope, started_ms) \
                 VALUES (?, ?, ?) ON CONFLICT DO NOTHING",
            )
            .bind(&turn_id)
            .bind(&scope)
            .bind(started)
            .execute(&mut *transaction)
            .await?;
            let batch = load_batch(&mut transaction, &batch_id).await?;
            transaction.commit().await?;
            Ok(batch)
        })
    }

    /// Ends the running records of `batch_id` one way. Returns how many records changed; a
    /// batch that is not running changes nothing.
    pub fn settle_wake_batch(&self, batch_id: &str, settle: Settle) -> Result<usize, StoreError> {
        let batch_id = batch_id.to_owned();
        self.call(move |pool| async move {
            let changed = settle_running(&pool, settle, "batch_id", &batch_id).await?;
            Ok(usize::try_from(changed).unwrap_or(usize::MAX))
        })
    }

    /// Ends the running records `ids` one way, in one transaction. A record that is not running
    /// changes nothing. Returns how many records changed.
    pub fn settle_wake_records(&self, ids: &[String], settle: Settle) -> Result<usize, StoreError> {
        let ids = ids.to_vec();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let mut changed = 0_u64;
            for id in &ids {
                changed += settle_running(&mut *transaction, settle, "id", id).await?;
            }
            transaction.commit().await?;
            Ok(usize::try_from(changed).unwrap_or(usize::MAX))
        })
    }

    /// Fails the running records `ids` and gives them `notice`, in one transaction. A failed turn
    /// is the only time a notice changes, so the notice says why the turn failed. A record that
    /// is not running changes nothing. Returns how many records changed.
    pub fn fail_wake_records_with_notice(
        &self,
        ids: &[String],
        notice: &str,
    ) -> Result<usize, StoreError> {
        let ids = ids.to_vec();
        let notice = notice.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let mut failed = 0_u64;
            for id in &ids {
                failed += sqlx::query(
                    "UPDATE wake_records SET state = 'failed', notice = ?, \
                     updated_at = CURRENT_TIMESTAMP WHERE id = ? AND state = 'running'",
                )
                .bind(&notice)
                .bind(id)
                .execute(&mut *transaction)
                .await?
                .rows_affected();
            }
            transaction.commit().await?;
            Ok(usize::try_from(failed).unwrap_or(usize::MAX))
        })
    }

    /// Skips the pending records `ids` with `reason`. A record that is not pending is left
    /// alone. Returns how many were skipped.
    pub fn skip_wake_records(
        &self,
        ids: &[String],
        reason: SkipReason,
    ) -> Result<usize, StoreError> {
        let ids = ids.to_vec();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let mut skipped = 0_u64;
            for id in &ids {
                skipped += sqlx::query(
                    "UPDATE wake_records SET state = 'skipped', skip_reason = ?, \
                     updated_at = CURRENT_TIMESTAMP WHERE id = ? AND state = 'pending'",
                )
                .bind(reason.as_str())
                .bind(id)
                .execute(&mut *transaction)
                .await?
                .rows_affected();
            }
            transaction.commit().await?;
            Ok(usize::try_from(skipped).unwrap_or(usize::MAX))
        })
    }

    /// Skips every pending record with `reason` (mode set to off). Returns their ids.
    pub fn skip_pending_wake_records(&self, reason: SkipReason) -> Result<Vec<String>, StoreError> {
        self.call(move |pool| async move {
            Ok(sqlx::query_scalar(
                "UPDATE wake_records SET state = 'skipped', skip_reason = ?, \
                 updated_at = CURRENT_TIMESTAMP WHERE state = 'pending' RETURNING id",
            )
            .bind(reason.as_str())
            .fetch_all(&pool)
            .await?)
        })
    }

    /// Ledger counts over the last hour and day, and wake tokens over the last day. A turn
    /// started exactly one hour (or day) ago has left its window.
    pub fn wake_usage(&self, scope: &str, now_ms: u64) -> Result<WakeUsage, StoreError> {
        let now = to_i64(now_ms)?;
        let owned_scope = scope.to_owned();
        let (hour_count, oldest_hour_ms, day_count, oldest_day_ms) =
            self.call(move |pool| async move {
                let mut windows = Vec::new();
                for window_ms in [HOUR_MS, DAY_MS] {
                    let (count, oldest): (i64, Option<i64>) = sqlx::query_as(
                        "SELECT COUNT(*), MIN(started_ms) FROM wake_turn_ledger \
                         WHERE scope = ? AND started_ms > ? - ?",
                    )
                    .bind(&owned_scope)
                    .bind(now)
                    .bind(i64::try_from(window_ms).unwrap_or(i64::MAX))
                    .fetch_one(&pool)
                    .await?;
                    windows.push((
                        u32::try_from(count).unwrap_or(u32::MAX),
                        oldest.and_then(|value| u64::try_from(value).ok()),
                    ));
                }
                Ok((windows[0].0, windows[0].1, windows[1].0, windows[1].1))
            })?;
        // Tokens use the same open edge as the counts: a receipt exactly a day old is out.
        let day_start = now_ms
            .checked_sub(DAY_MS)
            .map_or(0, |start| start.saturating_add(1));
        Ok(WakeUsage {
            hour_count,
            day_count,
            oldest_hour_ms,
            oldest_day_ms,
            day_tokens: self.wake_tokens_since(scope, day_start)?,
        })
    }

    /// The failed wake turns `runtime_name` has in a row. The run is the failed wake turns
    /// newer than the newest completed or cancelled wake turn of that runtime, so a running
    /// turn does not end it. The first element is the streak length, and the first row is the
    /// newest failure.
    pub fn consecutive_failed_wake_turns(
        &self,
        runtime_name: &str,
    ) -> Result<(u32, Option<WakeFailure>), StoreError> {
        let runtime = runtime_name.to_owned();
        self.call(move |pool| async move {
            let rows = sqlx::query(
                "WITH last_settled AS ( \
                     SELECT COALESCE(MAX(rowid), 0) AS last_rowid FROM conversation_turns \
                     WHERE trigger_kind = 'wake' AND runtime_name = ? \
                       AND state IN ('completed', 'cancelled')) \
                 SELECT t.id, t.error_code, l.started_ms \
                 FROM conversation_turns t \
                 LEFT JOIN wake_turn_ledger l ON l.turn_id = t.id \
                 WHERE t.trigger_kind = 'wake' AND t.runtime_name = ? AND t.state = 'failed' \
                   AND t.rowid > (SELECT last_rowid FROM last_settled) \
                 ORDER BY t.rowid DESC",
            )
            .bind(&runtime)
            .bind(&runtime)
            .fetch_all(&pool)
            .await?;
            let count = u32::try_from(rows.len()).unwrap_or(u32::MAX);
            let newest = rows.first().map(decode_failure).transpose()?;
            Ok((count, newest))
        })
    }

    /// Deletes ledger rows that started before `before_ms`. Returns how many.
    pub fn prune_wake_ledger(&self, before_ms: u64) -> Result<usize, StoreError> {
        let before = to_i64(before_ms)?;
        self.call(move |pool| async move {
            let deleted = sqlx::query("DELETE FROM wake_turn_ledger WHERE started_ms < ?")
                .bind(before)
                .execute(&pool)
                .await?;
            Ok(usize::try_from(deleted.rows_affected()).unwrap_or(usize::MAX))
        })
    }

    /// The newest `limit` records of a scope, oldest first, and whether older ones exist.
    pub fn wake_records_page(
        &self,
        scope: &str,
        limit: usize,
    ) -> Result<(Vec<WakeRecord>, bool), StoreError> {
        let scope = scope.to_owned();
        let fetch = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
        self.call(move |pool| async move {
            let mut records = sqlx::query(concat!(
                "SELECT ",
                wake_columns!(),
                " FROM wake_records WHERE scope = ? ORDER BY rowid DESC LIMIT ?"
            ))
            .bind(&scope)
            .bind(fetch)
            .fetch_all(&pool)
            .await?
            .into_iter()
            .map(decode_wake)
            .collect::<Result<Vec<_>, _>>()?;
            let more = records.len() > limit;
            records.truncate(limit);
            records.reverse();
            Ok((records, more))
        })
    }

    /// Unseen count per scope: `seen = 0` and the state is noticed, skipped or failed, or
    /// handled with a reply the mode wanted announced, or handled with an unanswered turn. A
    /// quiet outcome is never unseen. Scopes with none are left out.
    pub fn unseen_wake_counts(&self) -> Result<Vec<(String, u32)>, StoreError> {
        self.call(|pool| async move {
            let rows: Vec<(String, i64)> = sqlx::query_as(
                "SELECT scope, COUNT(*) FROM wake_records WHERE seen = 0 AND \
                 (state IN ('noticed', 'skipped', 'failed') \
                  OR (state = 'handled' AND outcome = 'unanswered') \
                  OR (state = 'handled' AND outcome = 'replied' AND notify_reply = 1)) \
                 GROUP BY scope ORDER BY scope",
            )
            .fetch_all(&pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|(scope, count)| (scope, u32::try_from(count).unwrap_or(u32::MAX)))
                .collect())
        })
    }

    /// Marks a scope's records seen. Returns how many changed.
    pub fn ack_wake_records(&self, scope: &str) -> Result<usize, StoreError> {
        let scope = scope.to_owned();
        self.call(move |pool| async move {
            let changed =
                sqlx::query("UPDATE wake_records SET seen = 1 WHERE scope = ? AND seen = 0")
                    .bind(&scope)
                    .execute(&pool)
                    .await?;
            Ok(usize::try_from(changed.rows_affected()).unwrap_or(usize::MAX))
        })
    }

    /// Pending action proposals created at or before `cutoff_ms` that have no reminder yet,
    /// newest first, at most 100. A reminder is a wake record in the proposal's scope whose
    /// dedupe key is `proposal:` followed by the proposal id. It reads `action_proposals`
    /// only. The cutoff comes from the caller's injected clock.
    pub fn stale_pending_proposals(
        &self,
        cutoff_ms: u64,
    ) -> Result<Vec<StaleProposal>, StoreError> {
        let cutoff = to_i64(cutoff_ms)?;
        self.call(move |pool| async move {
            let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
                "SELECT p.id, p.scope, p.kind, \
                        CAST(strftime('%s', p.created_at) AS INTEGER) * 1000 \
                 FROM action_proposals p \
                 WHERE p.state = 'pending' \
                   AND CAST(strftime('%s', p.created_at) AS INTEGER) * 1000 <= ? \
                   AND NOT EXISTS (SELECT 1 FROM wake_records w WHERE w.scope = p.scope \
                                   AND w.dedupe_key = 'proposal:' || p.id) \
                 ORDER BY p.created_at DESC, p.rowid DESC LIMIT ?",
            )
            .bind(cutoff)
            .bind(STALE_PROPOSAL_LIMIT)
            .fetch_all(&pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|(id, scope, kind, created_ms)| StaleProposal {
                    id,
                    scope,
                    kind,
                    created_ms: u64::try_from(created_ms).unwrap_or(0),
                })
                .collect())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConversationTarget, TurnOutcome, TurnTrigger};

    const GLOBAL: &str = "conversation:global";

    fn open() -> (tempfile::TempDir, SqliteStore) {
        let directory = tempfile::tempdir().unwrap();
        let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
        (directory, store)
    }

    /// Runs `sql` with `binds` on the store's own database and returns the database's own
    /// answer, so a trigger or CHECK shows as `Err`.
    fn execute(store: &SqliteStore, sql: &'static str, binds: &[&str]) -> Result<(), sqlx::Error> {
        let binds: Vec<String> = binds.iter().map(|value| (*value).to_owned()).collect();
        store
            .call(move |pool| async move {
                let mut query = sqlx::query(sql);
                for value in &binds {
                    query = query.bind(value.as_str());
                }
                Ok(query.execute(&pool).await.map(|_| ()))
            })
            .unwrap()
    }

    fn scalar(store: &SqliteStore, sql: &'static str) -> i64 {
        store
            .call(move |pool| async move { Ok(sqlx::query_scalar(sql).fetch_one(&pool).await?) })
            .unwrap()
    }

    fn new_record(scope: &str, kind: WakeKind, key: &str) -> NewWakeRecord {
        NewWakeRecord {
            scope: scope.to_owned(),
            kind,
            subject: "task-1".to_owned(),
            dedupe_key: key.to_owned(),
            notice: "Task task-1 failed.".to_owned(),
            needs_model: kind.needs_model(),
        }
    }

    fn pending(store: &SqliteStore, key: &str) -> WakeRecord {
        store
            .record_wake_events(
                &[new_record(GLOBAL, WakeKind::TaskFailed, key)],
                MonitoringMode::On,
                1,
            )
            .unwrap()
            .remove(0)
    }

    fn claim(store: &SqliteStore, ids: &[String], batch: &str, now_ms: u64) -> Vec<WakeRecord> {
        store
            .claim_wake_batch(GLOBAL, ids, batch, &format!("turn-{batch}"), true, now_ms)
            .unwrap()
    }

    #[test]
    fn adaptive_wake_migration_preserves_records_and_replays_unrelayed_reports() {
        use sqlx::migrate::Migrator;
        use std::borrow::Cow;

        static MIGRATIONS: Migrator = sqlx::migrate!("./migrations");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await.unwrap();
            let legacy = Migrator {
                migrations: Cow::Owned(MIGRATIONS.iter().filter(|m| m.version < 57).cloned().collect()),
                ..Migrator::DEFAULT
            };
            legacy.run(&pool).await.unwrap();
            sqlx::raw_sql(
                "INSERT INTO wake_records (rowid, id, scope, event_kind, subject, dedupe_key, notice,
                    needs_model, state, batch_id, turn_id, attempts, anchor_sequence, notify_reply)
                 VALUES (9, 'old', 'conversation:global', 'task_failed', 't', 'failed:t:7', 'Failed.',
                    1, 'running', 'batch', 'turn', 1, 42, 1);
                 INSERT INTO projects (id, root_path, settings) VALUES ('p', '/repo', '{}');
                 INSERT INTO tasks (id, project_id, state, contract_json, version)
                    VALUES ('t', 'p', '\"Succeeded\"', '{}', 1);
                 INSERT INTO task_reports (task_id, attempt_id, body, captured_at)
                    VALUES ('t', 'a', 'Finding.', '2026-11-17T00:00:00Z');
                 INSERT INTO durable_events (sequence, task_id, kind, payload)
                    VALUES (7, 't', 'task.report_ready', '{}');
                 UPDATE orchestrator_settings SET wake_cursor = 10 WHERE id = 1;"
            ).execute(&pool).await.unwrap();
            MIGRATIONS.run(&pool).await.unwrap();
            let row = sqlx::query("SELECT rowid, state, batch_id, turn_id, attempts, anchor_sequence, notify_reply
                FROM wake_records WHERE id = 'old'").fetch_one(&pool).await.unwrap();
            assert_eq!(row.get::<i64, _>("rowid"), 9);
            assert_eq!(row.get::<String, _>("state"), "running");
            assert_eq!(row.get::<String, _>("batch_id"), "batch");
            assert_eq!(row.get::<String, _>("turn_id"), "turn");
            assert_eq!(row.get::<i64, _>("attempts"), 1);
            assert_eq!(row.get::<i64, _>("anchor_sequence"), 42);
            assert_eq!(row.get::<i64, _>("notify_reply"), 1);
            let cursor: i64 = sqlx::query_scalar("SELECT wake_cursor FROM orchestrator_settings WHERE id = 1")
                .fetch_one(&pool).await.unwrap();
            assert_eq!(cursor, 6);
            for statement in [
                "DELETE FROM wake_records",
                "UPDATE wake_records SET notice = 'changed'",
                "UPDATE wake_records SET state = 'noticed'",
            ] {
                assert!(sqlx::query(statement).execute(&pool).await.is_err(), "{statement}");
            }
            for kind in ["work_check", "scout_report_ready"] {
                sqlx::query("INSERT INTO wake_records (id, scope, event_kind, subject, dedupe_key, notice, needs_model, state)
                    VALUES (?, 'conversation:global', ?, 't', ?, 'Check.', 1, 'pending')")
                    .bind(kind).bind(kind).bind(kind).execute(&pool).await.unwrap();
            }
            pool.close().await;
        });
    }

    fn failed(code: &str) -> TurnOutcome {
        TurnOutcome::Failed(code.to_owned())
    }

    /// A finished wake turn of `runtime`, with its ledger row when `started_ms` is given.
    fn wake_turn(
        store: &SqliteStore,
        id: &str,
        runtime: &str,
        outcome: TurnOutcome,
        started_ms: Option<i64>,
    ) {
        store
            .begin_conversation_turn_as(
                &ConversationTarget::Global,
                id,
                "Update.",
                runtime,
                "1",
                TurnTrigger::Wake,
            )
            .unwrap();
        store.finish_conversation_turn(id, &outcome).unwrap();
        if let Some(started) = started_ms {
            let started = started.to_string();
            execute(
                store,
                "INSERT INTO wake_turn_ledger (turn_id, scope, started_ms) VALUES (?, ?, ?)",
                &[id, GLOBAL, started.as_str()],
            )
            .unwrap();
        }
    }

    #[test]
    fn agent_failing_is_a_notice_kind_with_its_own_skip_reason() {
        assert!(!WakeKind::AgentFailing.needs_model());
        assert_eq!(WakeKind::AgentFailing.as_str(), "agent_failing");
        assert_eq!(
            WakeKind::parse("agent_failing"),
            Some(WakeKind::AgentFailing)
        );
        assert_eq!(SkipReason::AgentFailing.as_str(), "agent_failing");
        assert_eq!(
            SkipReason::parse("agent_failing"),
            Some(SkipReason::AgentFailing)
        );
    }

    #[test]
    fn consecutive_failed_wake_turns_count_from_the_last_settled_turn() {
        let (_directory, store) = open();
        wake_turn(&store, "w-1", "codex", TurnOutcome::Completed, Some(1_000));
        wake_turn(
            &store,
            "w-2",
            "codex",
            failed("runtime_failed"),
            Some(2_000),
        );
        wake_turn(
            &store,
            "w-3",
            "codex",
            failed("runtime_provider_limit"),
            Some(3_000),
        );
        let (count, newest) = store.consecutive_failed_wake_turns("codex").unwrap();
        assert_eq!(count, 2);
        assert_eq!(
            newest,
            Some(WakeFailure {
                turn_id: "w-3".to_owned(),
                error_code: "runtime_provider_limit".to_owned(),
                started_ms: Some(3_000),
            })
        );

        wake_turn(&store, "w-4", "codex", TurnOutcome::Cancelled, Some(4_000));
        assert_eq!(
            store.consecutive_failed_wake_turns("codex").unwrap(),
            (0, None)
        );
    }

    #[test]
    fn consecutive_failed_wake_turns_ignore_user_turns_and_other_runtimes() {
        let (_directory, store) = open();
        store
            .begin_conversation_turn_as(
                &ConversationTarget::Global,
                "u-1",
                "Hi.",
                "codex",
                "1",
                TurnTrigger::User,
            )
            .unwrap();
        store
            .finish_conversation_turn("u-1", &failed("runtime_failed"))
            .unwrap();
        wake_turn(
            &store,
            "w-1",
            "claude-code",
            failed("runtime_failed"),
            Some(1_000),
        );

        assert_eq!(
            store.consecutive_failed_wake_turns("codex").unwrap(),
            (0, None)
        );
        assert_eq!(
            store
                .consecutive_failed_wake_turns("claude-code")
                .unwrap()
                .0,
            1
        );
    }

    #[test]
    fn a_failure_whose_ledger_row_was_pruned_has_no_start_time() {
        let (_directory, store) = open();
        wake_turn(
            &store,
            "w-1",
            "codex",
            failed("runtime_failed"),
            Some(1_000),
        );
        wake_turn(&store, "w-2", "codex", failed("runtime_failed"), None);

        let (count, newest) = store.consecutive_failed_wake_turns("codex").unwrap();
        assert_eq!(count, 2);
        assert_eq!(newest.unwrap().started_ms, None);
    }

    #[test]
    fn agent_failing_migration_keeps_records_and_accepts_the_new_kind_and_skip_reason() {
        use sqlx::migrate::Migrator;
        use std::borrow::Cow;

        static MIGRATIONS: Migrator = sqlx::migrate!("./migrations");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap();
            let before = Migrator {
                migrations: Cow::Owned(
                    MIGRATIONS
                        .iter()
                        .filter(|m| m.version < 58)
                        .cloned()
                        .collect(),
                ),
                ..Migrator::DEFAULT
            };
            before.run(&pool).await.unwrap();
            sqlx::raw_sql(
                "INSERT INTO wake_records (id, scope, event_kind, subject, dedupe_key, notice,
                    needs_model, state, skip_reason)
                 VALUES ('kept', 'conversation:global', 'task_failed', 't', 'failed:t:1',
                    'Failed.', 1, 'skipped', 'limit_hourly')",
            )
            .execute(&pool)
            .await
            .unwrap();
            MIGRATIONS.run(&pool).await.unwrap();

            let kept: String =
                sqlx::query_scalar("SELECT skip_reason FROM wake_records WHERE id = 'kept'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(kept, "limit_hourly");
            sqlx::raw_sql(
                "INSERT INTO wake_records (id, scope, event_kind, subject, dedupe_key, notice,
                    needs_model, state, skip_reason)
                 VALUES ('notice', 'conversation:global', 'agent_failing', 'agent',
                    'agent_failing:codex:w-1', 'Paused.', 0, 'noticed', NULL),
                    ('held', 'conversation:global', 'task_failed', 't', 'failed:t:2',
                    'Failed.', 1, 'skipped', 'agent_failing')",
            )
            .execute(&pool)
            .await
            .unwrap();
            assert!(sqlx::query(
                "INSERT INTO wake_records (id, scope, event_kind, subject, dedupe_key, notice,
                    needs_model, state) VALUES ('bogus', 'conversation:global', 'bogus', 't',
                    'bogus:1', 'Bogus.', 0, 'noticed')",
            )
            .execute(&pool)
            .await
            .is_err());
            for statement in [
                "DELETE FROM wake_records WHERE id = 'kept'",
                "UPDATE wake_records SET notice = 'changed' WHERE id = 'kept'",
            ] {
                assert!(
                    sqlx::query(statement).execute(&pool).await.is_err(),
                    "{statement}"
                );
            }
            pool.close().await;
        });
    }

    #[test]
    fn record_wake_events_is_exactly_once() {
        let (_directory, store) = open();
        let events = [new_record(GLOBAL, WakeKind::TaskFailed, "failed:t1:5")];
        let first = store
            .record_wake_events(&events, MonitoringMode::On, 5)
            .unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].id, wake_record_id(GLOBAL, "failed:t1:5"));
        assert_eq!(store.wake_cursor().unwrap(), 5);

        let again = store
            .record_wake_events(&events, MonitoringMode::On, 5)
            .unwrap();
        assert!(again.is_empty());
        assert_eq!(scalar(&store, "SELECT COUNT(*) FROM wake_records"), 1);
        assert_eq!(store.wake_cursor().unwrap(), 5);
        // The cursor never moves backwards.
        store
            .record_wake_events(&[], MonitoringMode::On, 2)
            .unwrap();
        assert_eq!(store.wake_cursor().unwrap(), 5);
    }

    #[test]
    fn record_wake_events_rolls_back_with_the_cursor() {
        let (_directory, store) = open();
        store
            .record_wake_events(&[], MonitoringMode::On, 3)
            .unwrap();
        let mut bad = new_record(GLOBAL, WakeKind::TaskFailed, "failed:t2:9");
        bad.notice = String::new();
        let events = [new_record(GLOBAL, WakeKind::TaskFailed, "failed:t1:8"), bad];
        assert!(store
            .record_wake_events(&events, MonitoringMode::On, 9)
            .is_err());
        assert_eq!(store.wake_cursor().unwrap(), 3);
        assert_eq!(scalar(&store, "SELECT COUNT(*) FROM wake_records"), 0);
    }

    #[test]
    fn initial_state_follows_needs_model_and_mode() {
        let (_directory, store) = open();
        let notice = new_record(GLOBAL, WakeKind::DeliveryWaiting, "approval:a1");
        let model = new_record(GLOBAL, WakeKind::TaskFailed, "failed:a1:7");
        let mut copy = new_record(GLOBAL, WakeKind::TaskFailed, "up:failed:t1:5");
        copy.needs_model = false;
        let on = store
            .record_wake_events(&[notice, model.clone(), copy], MonitoringMode::On, 1)
            .unwrap();
        let states: Vec<_> = on.iter().map(|record| record.state).collect();
        assert_eq!(
            states,
            vec![WakeState::Noticed, WakeState::Pending, WakeState::Noticed]
        );
        assert!(!on[0].needs_model && on[1].needs_model);

        let mut off_record = model;
        off_record.dedupe_key = "failed:a2:8".to_owned();
        let off = store
            .record_wake_events(&[off_record], MonitoringMode::Off, 2)
            .unwrap();
        assert_eq!(off[0].state, WakeState::Skipped);
        assert_eq!(off[0].skip_reason, Some(SkipReason::ModeOff));

        let mut quiet_record = new_record(GLOBAL, WakeKind::TaskFailed, "failed:t3:1");
        quiet_record.subject = "task-3".to_owned();
        let quiet = store
            .record_wake_events(&[quiet_record], MonitoringMode::Quiet, 3)
            .unwrap();
        assert_eq!(quiet[0].state, WakeState::Pending);
    }

    #[test]
    fn the_anchor_is_the_transcript_sequence_at_record_time() {
        let (_directory, store) = open();
        let first = pending(&store, "failed:t1:1");
        assert_eq!(first.anchor_sequence, 0);

        let target = ConversationTarget::Global;
        let begun = store
            .begin_conversation_turn(&target, "turn-1", "hello", "codex", "1")
            .unwrap();
        store
            .finish_conversation_turn("turn-1", &TurnOutcome::Completed)
            .unwrap();
        let newest = store
            .conversation_page(&target, 0, 50)
            .unwrap()
            .latest_sequence;
        assert!(newest >= begun.user_entry.sequence);
        let second = pending(&store, "failed:t1:2");
        assert_eq!(second.anchor_sequence, newest);
        // Another scope's empty transcript still anchors at 0.
        let other = store
            .record_wake_events(
                &[new_record(
                    "conversation:workstream:w1",
                    WakeKind::TaskFailed,
                    "k",
                )],
                MonitoringMode::On,
                3,
            )
            .unwrap();
        assert_eq!(other[0].anchor_sequence, 0);
    }

    #[test]
    fn claim_is_idempotent_on_the_batch_id() {
        let (_directory, store) = open();
        let a = pending(&store, "failed:t1:1");
        let b = pending(&store, "failed:t1:2");
        let ids = vec![a.id.clone(), b.id.clone()];
        let first = claim(&store, &ids, "batch-1", 1_000);
        assert_eq!(first.len(), 2);
        assert!(first
            .iter()
            .all(|record| record.state == WakeState::Running));
        assert!(first
            .iter()
            .all(|record| record.turn_id.as_deref() == Some("turn-batch-1")));
        assert!(first.iter().all(|record| record.notify_reply));

        let second = claim(&store, &ids, "batch-1", 9_000);
        assert_eq!(second, first);
        assert_eq!(scalar(&store, "SELECT COUNT(*) FROM wake_turn_ledger"), 1);
        assert_eq!(
            scalar(&store, "SELECT started_ms FROM wake_turn_ledger"),
            1_000
        );

        // A different batch for a scope that already runs one claims nothing.
        let c = pending(&store, "failed:t1:3");
        assert!(claim(&store, &[c.id], "batch-2", 2_000).is_empty());
        assert_eq!(scalar(&store, "SELECT COUNT(*) FROM wake_turn_ledger"), 1);
    }

    #[test]
    fn claim_counts_the_turn_even_if_it_never_runs() {
        let (_directory, store) = open();
        let a = pending(&store, "failed:t1:1");
        claim(&store, std::slice::from_ref(&a.id), "batch-1", 5_000);
        // No turn row was ever created; the ledger row exists and counts toward the cap.
        assert_eq!(scalar(&store, "SELECT COUNT(*) FROM conversation_turns"), 0);
        let usage = store.wake_usage(GLOBAL, 6_000).unwrap();
        assert_eq!((usage.hour_count, usage.day_count), (1, 1));

        // A requeue puts the record back; the ledger row stays counted.
        let changed = store
            .settle_wake_batch(
                "batch-1",
                Settle::Requeue {
                    count_attempt: true,
                },
            )
            .unwrap();
        assert_eq!(changed, 1);
        let back = store.pending_wake_records().unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(
            (
                back[0].attempts,
                back[0].batch_id.clone(),
                back[0].turn_id.clone()
            ),
            (1, None, None)
        );
        assert_eq!(store.wake_usage(GLOBAL, 6_000).unwrap().hour_count, 1);
    }

    #[test]
    fn settle_ends_a_running_batch_one_way_and_only_once() {
        let (_directory, store) = open();
        let a = pending(&store, "failed:t1:1");
        claim(&store, std::slice::from_ref(&a.id), "batch-1", 1_000);
        assert_eq!(
            store
                .settle_wake_batch("batch-1", Settle::Handled(WakeOutcome::Quiet))
                .unwrap(),
            1
        );
        // Final: a second settle finds nothing running.
        assert_eq!(
            store.settle_wake_batch("batch-1", Settle::Failed).unwrap(),
            0
        );
        let (records, more) = store.wake_records_page(GLOBAL, 10).unwrap();
        assert!(!more);
        assert_eq!(records[0].state, WakeState::Handled);
        assert_eq!(records[0].outcome, Some(WakeOutcome::Quiet));

        let b = pending(&store, "failed:t1:2");
        claim(&store, std::slice::from_ref(&b.id), "batch-2", 2_000);
        store
            .settle_wake_batch("batch-2", Settle::Skipped(SkipReason::ModeOff))
            .unwrap();
        let c = pending(&store, "failed:t1:3");
        claim(&store, std::slice::from_ref(&c.id), "batch-3", 3_000);
        store.settle_wake_batch("batch-3", Settle::Failed).unwrap();
        let d = pending(&store, "failed:t1:4");
        claim(&store, std::slice::from_ref(&d.id), "batch-4", 4_000);
        // The attempt count stops at 2.
        for _ in 0..3 {
            store
                .settle_wake_batch(
                    "batch-4",
                    Settle::Requeue {
                        count_attempt: true,
                    },
                )
                .unwrap();
            let again = store.pending_wake_records().unwrap();
            assert!(again[0].attempts <= 2);
            claim(&store, &[again[0].id.clone()], "batch-4", 4_000);
        }
        assert_eq!(store.running_wake_records().unwrap()[0].attempts, 2);
    }

    #[test]
    fn illegal_wake_state_edges_are_refused() {
        let (_directory, store) = open();
        let noticed = store
            .record_wake_events(
                &[new_record(GLOBAL, WakeKind::DeliveryWaiting, "approval:a1")],
                MonitoringMode::On,
                1,
            )
            .unwrap()
            .remove(0);
        let waiting = pending(&store, "failed:t1:1");

        // noticed to anything
        for state in ["pending", "running", "failed"] {
            let sql: &'static str = match state {
                "pending" => "UPDATE wake_records SET state = 'pending' WHERE id = ?",
                "running" => "UPDATE wake_records SET state = 'running' WHERE id = ?",
                _ => "UPDATE wake_records SET state = 'failed' WHERE id = ?",
            };
            assert!(
                execute(&store, sql, &[noticed.id.as_str()]).is_err(),
                "noticed to {state}"
            );
        }
        // pending to handled, pending to failed
        assert!(execute(
            &store,
            "UPDATE wake_records SET state = 'handled', outcome = 'replied' WHERE id = ?",
            &[waiting.id.as_str()]
        )
        .is_err());
        assert!(execute(
            &store,
            "UPDATE wake_records SET state = 'failed' WHERE id = ?",
            &[waiting.id.as_str()]
        )
        .is_err());
        // handled to anything
        claim(&store, std::slice::from_ref(&waiting.id), "batch-1", 1_000);
        store
            .settle_wake_batch("batch-1", Settle::Handled(WakeOutcome::Replied))
            .unwrap();
        assert!(execute(
            &store,
            "UPDATE wake_records SET state = 'pending', outcome = NULL WHERE id = ?",
            &[waiting.id.as_str()]
        )
        .is_err());
        assert!(execute(
            &store,
            "UPDATE wake_records SET state = 'running', outcome = NULL WHERE id = ?",
            &[waiting.id.as_str()]
        )
        .is_err());
    }

    #[test]
    fn wake_identity_columns_never_change() {
        let (_directory, store) = open();
        let record = pending(&store, "failed:t1:1");
        for sql in [
            "UPDATE wake_records SET notice = 'rewritten' WHERE id = ?",
            "UPDATE wake_records SET scope = 'conversation:workstream:x' WHERE id = ?",
            "UPDATE wake_records SET dedupe_key = 'other' WHERE id = ?",
            "UPDATE wake_records SET subject = 'other' WHERE id = ?",
            "UPDATE wake_records SET event_kind = 'task_stalled' WHERE id = ?",
            "UPDATE wake_records SET anchor_sequence = 99 WHERE id = ?",
        ] {
            assert!(
                execute(&store, sql, &[record.id.as_str()]).is_err(),
                "{sql}"
            );
        }
        // The fields that may change still do.
        execute(
            &store,
            "UPDATE wake_records SET seen = 1 WHERE id = ?",
            &[record.id.as_str()],
        )
        .unwrap();
    }

    #[test]
    fn wake_records_delete_only_through_a_history_clear() {
        let (_directory, store) = open();
        let target = ConversationTarget::Global;
        store
            .begin_conversation_turn(&target, "turn-1", "hello", "codex", "1")
            .unwrap();
        store
            .finish_conversation_turn("turn-1", &TurnOutcome::Completed)
            .unwrap();
        let record = pending(&store, "failed:t1:1");
        let other = store
            .record_wake_events(
                &[new_record(
                    "conversation:workstream:w1",
                    WakeKind::TaskFailed,
                    "k",
                )],
                MonitoringMode::On,
                2,
            )
            .unwrap()
            .remove(0);
        claim(&store, std::slice::from_ref(&record.id), "batch-1", 1_000);
        assert!(execute(
            &store,
            "DELETE FROM wake_records WHERE id = ?",
            &[record.id.as_str()]
        )
        .is_err());

        store.clear_conversation_history(&target).unwrap();
        let (left, _) = store.wake_records_page(GLOBAL, 10).unwrap();
        assert!(left.is_empty());
        // Another scope's records and the ledger are untouched.
        let (kept, _) = store
            .wake_records_page("conversation:workstream:w1", 10)
            .unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, other.id);
        assert_eq!(scalar(&store, "SELECT COUNT(*) FROM wake_turn_ledger"), 1);
    }

    #[test]
    fn a_clear_removes_notices_of_a_conversation_that_has_no_turns() {
        let (_directory, store) = open();
        store
            .record_wake_events(
                &[new_record(GLOBAL, WakeKind::DeliveryWaiting, "approval:a1")],
                MonitoringMode::On,
                1,
            )
            .unwrap();
        let outcome = store
            .clear_conversation_history(&ConversationTarget::Global)
            .unwrap();
        assert!(outcome.cleared);
        assert_eq!(scalar(&store, "SELECT COUNT(*) FROM wake_records"), 0);
    }

    #[test]
    fn the_ledger_refuses_updates_and_prunes_by_age() {
        let (_directory, store) = open();
        for (index, started) in [1_000_u64, 2_000, 3_000].into_iter().enumerate() {
            let record = pending(&store, &format!("failed:t1:{index}"));
            claim(&store, &[record.id], &format!("b{index}"), started);
            store
                .settle_wake_batch(&format!("b{index}"), Settle::Failed)
                .unwrap();
        }
        assert!(execute(&store, "UPDATE wake_turn_ledger SET started_ms = 0", &[]).is_err());
        assert_eq!(store.prune_wake_ledger(2_000).unwrap(), 1);
        assert_eq!(scalar(&store, "SELECT COUNT(*) FROM wake_turn_ledger"), 2);
        assert_eq!(store.prune_wake_ledger(10_000).unwrap(), 2);
    }

    #[test]
    fn wake_usage_counts_the_last_hour_and_day() {
        let (_directory, store) = open();
        let now = 2 * DAY_MS;
        // Started 25 h, 24 h exactly, 23 h, 1 h exactly, 59 min and 1 min ago.
        let ages = [
            25 * HOUR_MS,
            DAY_MS,
            23 * HOUR_MS,
            HOUR_MS,
            59 * 60_000,
            60_000,
        ];
        for (index, age) in ages.into_iter().enumerate() {
            let record = pending(&store, &format!("failed:t1:{index}"));
            claim(&store, &[record.id], &format!("b{index}"), now - age);
            store
                .settle_wake_batch(&format!("b{index}"), Settle::Failed)
                .unwrap();
        }
        let usage = store.wake_usage(GLOBAL, now).unwrap();
        // Hour: 59 min and 1 min ago (exactly 1 h is out).
        assert_eq!(usage.hour_count, 2);
        assert_eq!(usage.oldest_hour_ms, Some(now - 59 * 60_000));
        // Day: 23 h, 1 h, 59 min, 1 min (exactly 24 h is out).
        assert_eq!(usage.day_count, 4);
        assert_eq!(usage.oldest_day_ms, Some(now - 23 * HOUR_MS));
        assert_eq!(usage.day_tokens, 0);
        // Another scope is counted apart.
        let other = store.wake_usage("conversation:workstream:w1", now).unwrap();
        assert_eq!(other.day_count, 0);
        assert_eq!(other.oldest_hour_ms, None);
    }

    #[test]
    fn unseen_counts_and_acknowledgement_follow_the_state() {
        let (_directory, store) = open();
        store
            .record_wake_events(
                &[new_record(GLOBAL, WakeKind::DeliveryWaiting, "approval:a1")],
                MonitoringMode::On,
                1,
            )
            .unwrap();
        let replied = pending(&store, "failed:t1:1");
        let quiet = pending(&store, "failed:t1:2");
        claim(&store, std::slice::from_ref(&replied.id), "b1", 1_000);
        store
            .settle_wake_batch("b1", Settle::Handled(WakeOutcome::Replied))
            .unwrap();
        claim(&store, std::slice::from_ref(&quiet.id), "b2", 2_000);
        store
            .settle_wake_batch("b2", Settle::Handled(WakeOutcome::Quiet))
            .unwrap();
        // The notice and the announced reply count; the quiet outcome does not.
        assert_eq!(
            store.unseen_wake_counts().unwrap(),
            vec![(GLOBAL.to_owned(), 2)]
        );
        assert_eq!(store.ack_wake_records(GLOBAL).unwrap(), 3);
        assert!(store.unseen_wake_counts().unwrap().is_empty());
    }

    /// Each record of the global scope with its state, oldest first.
    fn states(store: &SqliteStore) -> Vec<(String, WakeState)> {
        store
            .wake_records_page(GLOBAL, 10)
            .unwrap()
            .0
            .into_iter()
            .map(|record| (record.id, record.state))
            .collect()
    }

    #[test]
    fn settle_wake_records_changes_only_the_named_running_records() {
        let (_directory, store) = open();
        let a = pending(&store, "failed:t1:1");
        let b = pending(&store, "failed:t1:2");
        let c = pending(&store, "failed:t1:3");
        claim(&store, &[a.id.clone(), b.id.clone()], "batch-1", 1_000);
        // a is running and named; b is running but not named; c is named but pending.
        let changed = store
            .settle_wake_records(
                &[a.id.clone(), c.id.clone()],
                Settle::Handled(WakeOutcome::Replied),
            )
            .unwrap();
        assert_eq!(changed, 1);
        assert_eq!(
            states(&store),
            vec![
                (a.id.clone(), WakeState::Handled),
                (b.id.clone(), WakeState::Running),
                (c.id.clone(), WakeState::Pending),
            ]
        );
        // A second settle of the same ids finds nothing running.
        assert_eq!(
            store
                .settle_wake_records(&[a.id.clone(), b.id.clone()], Settle::Failed)
                .unwrap(),
            1
        );
        assert_eq!(
            states(&store)[..2],
            [
                (a.id.clone(), WakeState::Handled),
                (b.id.clone(), WakeState::Failed),
            ]
        );
    }

    #[test]
    fn fail_wake_records_with_notice_rewrites_the_notice() {
        let (_directory, store) = open();
        let a = pending(&store, "failed:t1:1");
        let b = pending(&store, "failed:t1:2");
        let c = pending(&store, "failed:t1:3");
        claim(&store, &[a.id.clone(), b.id.clone()], "batch-1", 1_000);
        let reason = "The wake turn failed: the agent timed out.";
        assert_eq!(
            store
                .fail_wake_records_with_notice(&[a.id.clone(), c.id.clone()], reason)
                .unwrap(),
            1
        );
        let (records, _) = store.wake_records_page(GLOBAL, 10).unwrap();
        let [first, second, third] = &records[..] else {
            panic!("three records expected");
        };
        assert_eq!(
            (first.state, first.notice.as_str()),
            (WakeState::Failed, reason)
        );
        // The running record that was not named keeps its state and notice.
        assert_eq!(
            (second.state, second.notice.as_str()),
            (WakeState::Running, "Task task-1 failed.")
        );
        // A pending record is not running, so it keeps its notice, and the database refuses the
        // rewrite on its own.
        assert_eq!(
            (third.state, third.notice.as_str()),
            (WakeState::Pending, "Task task-1 failed.")
        );
        assert!(execute(
            &store,
            "UPDATE wake_records SET notice = 'rewritten' WHERE id = ?",
            &[c.id.as_str()]
        )
        .is_err());
    }

    #[test]
    fn unseen_wake_counts_include_unanswered_outcomes() {
        let (_directory, store) = open();
        let unanswered = pending(&store, "failed:t1:1");
        let quiet = pending(&store, "failed:t1:2");
        claim(&store, std::slice::from_ref(&unanswered.id), "b1", 1_000);
        store
            .settle_wake_batch("b1", Settle::Handled(WakeOutcome::Unanswered))
            .unwrap();
        claim(&store, std::slice::from_ref(&quiet.id), "b2", 2_000);
        store
            .settle_wake_batch("b2", Settle::Handled(WakeOutcome::Quiet))
            .unwrap();
        // The unanswered turn is unseen; the quiet one is not.
        assert_eq!(
            store.unseen_wake_counts().unwrap(),
            vec![(GLOBAL.to_owned(), 1)]
        );
        assert_eq!(store.ack_wake_records(GLOBAL).unwrap(), 2);
        assert!(store.unseen_wake_counts().unwrap().is_empty());
    }

    #[test]
    fn candidate_ready_migration_keeps_records_and_accepts_the_new_kind_and_outcome() {
        use sqlx::migrate::Migrator;
        use std::borrow::Cow;

        static MIGRATIONS: Migrator = sqlx::migrate!("./migrations");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap();
            let before = Migrator {
                migrations: Cow::Owned(
                    MIGRATIONS
                        .iter()
                        .filter(|m| m.version < 60)
                        .cloned()
                        .collect(),
                ),
                ..Migrator::DEFAULT
            };
            before.run(&pool).await.unwrap();
            sqlx::raw_sql(
                "INSERT INTO wake_records (id, scope, event_kind, subject, dedupe_key, notice,
                    needs_model, state, outcome, skip_reason, notify_reply)
                 VALUES ('replied', 'conversation:global', 'task_failed', 't', 'failed:t:1',
                    'Failed.', 1, 'handled', 'replied', NULL, 1),
                    ('skipped', 'conversation:global', 'task_failed', 't', 'failed:t:2',
                    'Failed.', 1, 'skipped', NULL, 'mode_off', 0)",
            )
            .execute(&pool)
            .await
            .unwrap();
            MIGRATIONS.run(&pool).await.unwrap();

            let kept: String = sqlx::query_scalar(
                "SELECT outcome || ':' || notice FROM wake_records WHERE id = 'replied'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(kept, "replied:Failed.");
            sqlx::raw_sql(
                "INSERT INTO wake_records (id, scope, event_kind, subject, dedupe_key, notice,
                    needs_model, state, outcome, notify_reply)
                 VALUES ('ready', 'conversation:global', 'candidate_ready', 't', 'candidate:t:1',
                    'Ready.', 1, 'pending', NULL, 0),
                    ('unanswered', 'conversation:global', 'task_failed', 't', 'failed:t:3',
                    'Failed.', 1, 'handled', 'unanswered', 0),
                    ('running', 'conversation:global', 'task_failed', 't', 'failed:t:4',
                    'Failed.', 1, 'running', NULL, 0)",
            )
            .execute(&pool)
            .await
            .unwrap();
            // A running turn that fails may say why; nothing else may change a notice.
            sqlx::query(
                "UPDATE wake_records SET state = 'failed', notice = 'Timed out.' WHERE id = 'running'",
            )
            .execute(&pool)
            .await
            .unwrap();
            for statement in [
                "UPDATE wake_records SET notice = 'changed' WHERE id = 'skipped'",
                "UPDATE wake_records SET notice = 'changed' WHERE id = 'replied'",
                "INSERT INTO wake_records (id, scope, event_kind, subject, dedupe_key, notice,
                    needs_model, state, outcome, notify_reply)
                 VALUES ('bogus', 'conversation:global', 'bogus', 't', 'bogus:1', 'Bogus.', 0,
                    'handled', 'maybe', 0)",
            ] {
                assert!(
                    sqlx::query(statement).execute(&pool).await.is_err(),
                    "{statement}"
                );
            }
            pool.close().await;
        });
    }

    #[test]
    fn skipping_changes_only_pending_records() {
        let (_directory, store) = open();
        let a = pending(&store, "failed:t1:1");
        let b = pending(&store, "failed:t1:2");
        claim(&store, std::slice::from_ref(&a.id), "b1", 1_000);
        assert_eq!(
            store
                .skip_wake_records(&[a.id.clone(), b.id.clone()], SkipReason::LimitHourly)
                .unwrap(),
            1
        );
        let c = pending(&store, "failed:t1:3");
        assert_eq!(
            store
                .skip_pending_wake_records(SkipReason::ModeOff)
                .unwrap(),
            vec![c.id]
        );
        assert!(store.pending_wake_records().unwrap().is_empty());
        assert_eq!(store.running_wake_records().unwrap().len(), 1);
    }

    #[test]
    fn a_page_returns_the_newest_records_oldest_first() {
        let (_directory, store) = open();
        for index in 0..5 {
            pending(&store, &format!("failed:t1:{index}"));
        }
        let (page, more) = store.wake_records_page(GLOBAL, 3).unwrap();
        assert!(more);
        let keys: Vec<_> = page
            .iter()
            .map(|record| record.dedupe_key.as_str())
            .collect();
        assert_eq!(keys, vec!["failed:t1:2", "failed:t1:3", "failed:t1:4"]);
        assert!(!store.wake_records_page(GLOBAL, 5).unwrap().1);
    }
}
