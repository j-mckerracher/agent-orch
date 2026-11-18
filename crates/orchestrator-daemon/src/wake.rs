//! The wake scheduler (DPES-1105 phase 3c).
//!
//! Watches durable events and the running attempts, writes a deterministic notice for each
//! finding, and starts a Restricted model turn (a wake turn) only when interpretation is
//! needed. The notice is committed before any turn starts for its event, so a failed or skipped
//! turn never hides it. Limits, a 60 second debounce, user-turn precedence and the machine load
//! gate bound the rate. Work checks use the workload-scaled interval. Stall findings remain
//! deterministic notices; the separate work-check turn interprets the current snapshot.
//!
//! Every timer reads the injected [`Clock`], so a test moves time by hand and calls
//! [`WakeService::tick`] directly. The loop thread only calls `tick` every two seconds.
//!
//! Nothing here logs a path, a token or an environment value: log fields are counts and words.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use orchestrator_core::scheduler::StartGate;
use orchestrator_core::supervision::{
    admit, stalled, Activity, Admission, Clock, Debounce, MonitoringMode, PauseReason, Stall,
    HOUR_MS, STALL_QUIET_FLOOR,
};
use orchestrator_core::task::TaskState;
use orchestrator_diagnostics::Diagnostics;
use orchestrator_store::{
    ConversationError, ConversationTarget, NewWakeRecord, OrchestratorSettings, Settle, SkipReason,
    SqliteStore, Store, TurnState, WakeKind, WakeOutcome, WakeRecord, WakeState,
};
use sha2::{Digest, Sha256};

use super::conversation_runtime::{ConversationService, WakeBrief, QUIET_TOKEN};
use super::load::LoadHandle;

/// The daemon's wall clock, in Unix milliseconds. Core defines the `Clock` trait and a manual clock
/// for tests but not this one: the daemon owns the one real clock. A clock set before 1970 reads 0.
pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| {
                u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
            })
    }
}
use super::wake_events::{classify, proposal_notice, stall_candidate, Candidate};
use crate::executor::SessionPulse;

const LOG: &str = "wake";
const GLOBAL_SCOPE: &str = "conversation:global";
const WORKSTREAM_PREFIX: &str = "conversation:workstream:";
/// How often the loop thread runs a pass.
const TICK: Duration = Duration::from_secs(2);
/// Durable events read per pass.
const EVENT_PAGE: usize = 256;
/// A pending proposal this old gets a reminder notice.
const PROPOSAL_WAIT_MS: u64 = 30 * 60 * 1000;
/// Wake ledger rows older than this are pruned. Twice the longest window.
const LEDGER_KEEP_MS: u64 = 48 * HOUR_MS;
/// Bytes of a reply read to recognize the quiet token.
const QUIET_PROBE_BYTES: usize = 64;

/// The conversation id the wake tables use for a target.
pub(crate) fn scope_of(target: &ConversationTarget) -> String {
    match target {
        ConversationTarget::Global => GLOBAL_SCOPE.to_owned(),
        ConversationTarget::Workstream(id) => format!("{WORKSTREAM_PREFIX}{id}"),
        ConversationTarget::Project(id) => format!("conversation:project:{id}"),
    }
}

/// The target of a scope string, or `None` for a string that is not one.
pub(crate) fn target_of(scope: &str) -> Option<ConversationTarget> {
    if scope == GLOBAL_SCOPE {
        return Some(ConversationTarget::Global);
    }
    scope
        .strip_prefix(WORKSTREAM_PREFIX)
        .filter(|id| !id.is_empty())
        .map(|id| ConversationTarget::Workstream(id.to_owned()))
}

/// A stable id: the same prefix and seed always give the same value.
fn stable_id(prefix: &str, seed: &str) -> String {
    let digest = Sha256::digest(format!("{prefix}\n{seed}").as_bytes());
    let hex: String = digest
        .iter()
        .take(12)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("{prefix}-{hex}")
}

fn skip_reason(reason: PauseReason) -> SkipReason {
    match reason {
        PauseReason::Hourly => SkipReason::LimitHourly,
        PauseReason::Daily => SkipReason::LimitDaily,
        PauseReason::Tokens => SkipReason::LimitTokens,
    }
}

#[derive(Default)]
struct State {
    debounce: Debounce,
    last_check_ms: HashMap<String, u64>,
    last_prune_ms: u64,
}

/// What one pass did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TickReport {
    pub recorded: usize,
    pub started: usize,
    pub skipped: usize,
    pub settled: usize,
}

pub(crate) struct WakeService {
    store: Arc<SqliteStore>,
    conversations: ConversationService,
    pulse: Arc<SessionPulse>,
    clock: Arc<dyn Clock>,
    load: Arc<LoadHandle>,
    diagnostics: Diagnostics,
    boot_ms: u64,
    state: Mutex<State>,
    stop: Arc<(Mutex<bool>, Condvar)>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

impl WakeService {
    pub(crate) fn new(
        store: Arc<SqliteStore>,
        conversations: ConversationService,
        pulse: Arc<SessionPulse>,
        clock: Arc<dyn Clock>,
        load: Arc<LoadHandle>,
        diagnostics: Diagnostics,
    ) -> Self {
        let boot_ms = clock.now_ms();
        Self {
            store,
            conversations,
            pulse,
            clock,
            load,
            diagnostics,
            boot_ms,
            state: Mutex::new(State::default()),
            stop: Arc::new((Mutex::new(false), Condvar::new())),
            thread: Mutex::new(None),
        }
    }

    /// The injected clock's time, in Unix ms. `monitoring.get` reads pauses against it.
    pub(crate) fn now_ms(&self) -> u64 {
        self.clock.now_ms()
    }

    fn state(&self) -> MutexGuard<'_, State> {
        lock(&self.state)
    }

    /// Startup, before any request is served and after `ConversationService::recover` failed
    /// the turns a crash left open. A batch whose turn did not complete is tried once more
    /// under a new batch id, and failed when it already had its second try. The old turn id is
    /// never reused, so recovery cannot make a second turn for one record. Then the pending
    /// records join the debounce, and the first observation pass runs here so a catch-up
    /// notice belongs to the same batch.
    pub(crate) fn recover(&self) -> Result<(), String> {
        let recovered = self.recover_once();
        self.after_recover(recovered)
    }

    /// A store that cannot be read right now must not stop the daemon: one diagnostics line,
    /// then the pending records join the debounce and the loop observes again on its first
    /// pass.
    fn after_recover(&self, recovered: Result<(), String>) -> Result<(), String> {
        if recovered.is_err() {
            self.diagnostics.warn(LOG, "wake_recover_failed", &[]);
            self.reoffer_pending();
        }
        Ok(())
    }

    /// Offers every pending record to the debounce. A read error leaves them to a later pass.
    fn reoffer_pending(&self) {
        let now = self.boot_ms;
        if let Ok(pending) = self.store.pending_wake_records() {
            let mut state = self.state();
            for record in &pending {
                state.debounce.offer(&record.scope, &record.id, now);
            }
        }
    }

    fn recover_once(&self) -> Result<(), String> {
        let now = self.boot_ms;
        let running = self
            .store
            .running_wake_records()
            .map_err(|_| "cannot read wake records".to_owned())?;
        for batch in batches(&running) {
            let completed = self.completed_outcome(&batch);
            let settle = match completed {
                Some(Ok(settle)) => settle,
                Some(Err(())) => continue,
                None if batch.records[0].attempts >= 1 => Settle::Failed,
                None => Settle::Requeue {
                    count_attempt: true,
                },
            };
            self.settle(&batch.batch_id, settle)?;
        }
        let pending = self
            .store
            .pending_wake_records()
            .map_err(|_| "cannot read wake records".to_owned())?;
        {
            let mut state = self.state();
            for record in &pending {
                state.debounce.offer(&record.scope, &record.id, now);
            }
        }
        let settings = self.settings()?;
        let recorded = self.observe(&settings, now)?;
        if recorded > 0 {
            let candidate = NewWakeRecord {
                scope: GLOBAL_SCOPE.to_owned(),
                kind: WakeKind::DaemonStart,
                subject: "daemon".to_owned(),
                dedupe_key: format!("start:{}", self.boot_ms),
                notice: "The daemon restarted. Checking what changed.".to_owned(),
                needs_model: true,
            };
            self.record(&settings, &[candidate], now)?;
        }
        Ok(())
    }

    /// One pass. A pure function of the store and the clock.
    pub(crate) fn tick(&self) -> Result<TickReport, String> {
        let now = self.clock.now_ms();
        let mut report = TickReport::default();
        let settings = self.settings()?;
        report.settled = self.settle_running(&settings, now)?;
        report.recorded += self.observe(&settings, now)?;
        report.recorded += self.proposal_scan(&settings, now)?;
        self.start_ready(now, &mut report)?;
        // Drain due batches before arming the next check. At the one-minute floor the
        // interval equals the debounce: checking first would skip every other wake.
        report.recorded += self.supervision_scan(&settings, now)?;
        self.prune(now);
        Ok(report)
    }

    /// Spawns the thread "wake-scheduler": tick, then sleep two seconds or until stopped. The
    /// thread holds only a weak reference, so a dropped service ends it.
    pub(crate) fn start(self: &Arc<Self>) {
        let weak: Weak<Self> = Arc::downgrade(self);
        let stop = self.stop.clone();
        let spawned = thread::Builder::new()
            .name("wake-scheduler".into())
            .spawn(move || loop {
                let Some(service) = weak.upgrade() else {
                    return;
                };
                let ticked =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| service.tick()));
                match ticked {
                    Ok(Ok(_)) => {}
                    Ok(Err(reason)) => {
                        service
                            .diagnostics
                            .warn(LOG, "wake_tick_failed", &[("reason", &reason)]);
                    }
                    Err(_) => {
                        service.diagnostics.error(LOG, "wake_tick_panicked", &[]);
                    }
                }
                drop(service);
                let (flag, signal) = &*stop;
                let stopped = lock(flag);
                let (stopped, _) = signal
                    .wait_timeout_while(stopped, TICK, |stopped| !*stopped)
                    .unwrap_or_else(|error| error.into_inner());
                if *stopped {
                    return;
                }
            });
        match spawned {
            Ok(handle) => *lock(&self.thread) = Some(handle),
            Err(_) => self.diagnostics.error(LOG, "wake_thread_not_started", &[]),
        }
    }

    /// Stops the loop thread and waits for it.
    pub(crate) fn stop(&self) {
        let (flag, signal) = &*self.stop;
        *lock(flag) = true;
        signal.notify_all();
        if let Some(handle) = lock(&self.thread).take() {
            let _ = handle.join();
        }
    }

    /// Called by `monitoring.set` after a mode change was saved. To Off: skip the pending model
    /// records (ModeOff), forget the debounce windows and cancel the running wake turns; the
    /// next pass settles their records as skipped.
    pub(crate) fn mode_changed(&self, to: MonitoringMode) {
        if to.runs_wake_turns() {
            return;
        }
        if self
            .store
            .skip_pending_wake_records(SkipReason::ModeOff)
            .is_err()
        {
            self.diagnostics.warn(LOG, "mode_skip_failed", &[]);
        }
        self.state().debounce = Debounce::default();
        let Ok(running) = self.store.running_wake_records() else {
            return;
        };
        let scopes: BTreeSet<&str> = running.iter().map(|record| record.scope.as_str()).collect();
        for target in scopes.into_iter().filter_map(target_of) {
            if self.conversations.cancel_wake_turn(&target, None).is_err() {
                self.diagnostics.warn(LOG, "mode_cancel_failed", &[]);
            }
        }
    }

    fn settings(&self) -> Result<OrchestratorSettings, String> {
        self.store
            .orchestrator_settings()
            .map_err(|_| "cannot read settings".to_owned())
    }

    fn settle(&self, batch_id: &str, settle: Settle) -> Result<usize, String> {
        if matches!(settle, Settle::Handled(WakeOutcome::Replied)) {
            let records = self
                .store
                .running_wake_records()
                .map_err(|_| "cannot read wake records".to_owned())?;
            for record in records.iter().filter(|record| {
                record.batch_id.as_deref() == Some(batch_id)
                    && record.kind == WakeKind::ScoutReportReady
            }) {
                let turn_id = record
                    .turn_id
                    .as_deref()
                    .ok_or_else(|| "report wake has no turn".to_owned())?;
                let reply = self
                    .store
                    .turn_reply_text(turn_id, QUIET_PROBE_BYTES)
                    .map_err(|_| "cannot read report wake reply".to_owned())?;
                if reply.trim().is_empty() {
                    continue;
                }
                self.store
                    .mark_task_report_relayed(
                        &record.subject,
                        &super::report::utc_text(self.clock.now_ms()),
                    )
                    .map_err(|_| "cannot mark relayed report".to_owned())?;
            }
        }
        self.store
            .settle_wake_batch(batch_id, settle)
            .map_err(|_| "cannot settle wake batch".to_owned())
    }

    /// For a batch whose turn is completed: `Some(Ok(settle))`. Failed or cancelled or missing
    /// turn: `None`. A turn that is still running: `Some(Err(()))`, leave it.
    fn completed_outcome(&self, batch: &Batch) -> Option<Result<Settle, ()>> {
        let turn = self
            .store
            .conversation_turn(&batch.turn_id)
            .ok()
            .flatten()?;
        match turn.state {
            TurnState::Completed => {
                let reply = self
                    .store
                    .turn_reply_text(&batch.turn_id, QUIET_PROBE_BYTES)
                    .unwrap_or_default();
                let outcome = if reply.trim() == QUIET_TOKEN {
                    WakeOutcome::Quiet
                } else {
                    WakeOutcome::Replied
                };
                Some(Ok(Settle::Handled(outcome)))
            }
            TurnState::Running => Some(Err(())),
            TurnState::Failed | TurnState::Cancelled => None,
        }
    }

    /// Step 1: settle the running batches whose turns ended.
    fn settle_running(&self, settings: &OrchestratorSettings, now: u64) -> Result<usize, String> {
        let running = self
            .store
            .running_wake_records()
            .map_err(|_| "cannot read wake records".to_owned())?;
        let mut settled = 0;
        for batch in batches(&running) {
            let Some(turn) = self.store.conversation_turn(&batch.turn_id).ok().flatten() else {
                continue;
            };
            let settle = match turn.state {
                TurnState::Running => continue,
                TurnState::Completed => match self.completed_outcome(&batch) {
                    Some(Ok(settle)) => settle,
                    _ => continue,
                },
                TurnState::Failed => Settle::Failed,
                // Nothing but a user turn or the mode cancels a wake turn.
                TurnState::Cancelled if !settings.mode.runs_wake_turns() => {
                    Settle::Skipped(SkipReason::ModeOff)
                }
                TurnState::Cancelled => Settle::Requeue {
                    count_attempt: false,
                },
            };
            let requeue = matches!(settle, Settle::Requeue { .. });
            settled += self.settle(&batch.batch_id, settle)?;
            if requeue {
                // The 60 second window restarts.
                let mut state = self.state();
                for record in &batch.records {
                    state.debounce.offer(&record.scope, &record.id, now);
                }
            }
        }
        Ok(settled)
    }

    /// Writes candidates and offers the new pending records to the debounce.
    fn record(
        &self,
        settings: &OrchestratorSettings,
        records: &[NewWakeRecord],
        now: u64,
    ) -> Result<usize, String> {
        let cursor = self
            .store
            .wake_cursor()
            .map_err(|_| "cannot read wake cursor".to_owned())?;
        self.record_at(settings, records, cursor, now)
    }

    fn record_at(
        &self,
        settings: &OrchestratorSettings,
        records: &[NewWakeRecord],
        cursor: i64,
        now: u64,
    ) -> Result<usize, String> {
        let inserted = self
            .store
            .record_wake_events(records, settings.mode, cursor)
            .map_err(|_| "cannot record wake events".to_owned())?;
        let mut state = self.state();
        for record in inserted.iter().filter(|r| r.state == WakeState::Pending) {
            state.debounce.offer(&record.scope, &record.id, now);
        }
        Ok(inserted.len())
    }

    /// Task events are interpreted in their workstream with a notice in Global. Reports
    /// instead relay in Global, with a notice in the workstream.
    fn expand(candidate: Candidate, scope: &str, out: &mut Vec<NewWakeRecord>) {
        out.push(NewWakeRecord {
            scope: scope.to_owned(),
            kind: candidate.kind,
            subject: candidate.subject.clone(),
            dedupe_key: candidate.dedupe_key.clone(),
            notice: candidate.notice.clone(),
            needs_model: candidate.kind.needs_model()
                && (candidate.kind != WakeKind::ScoutReportReady || scope == GLOBAL_SCOPE),
        });
        if scope != GLOBAL_SCOPE {
            out.push(NewWakeRecord {
                scope: GLOBAL_SCOPE.to_owned(),
                kind: candidate.kind,
                subject: candidate.subject,
                dedupe_key: format!("up:{}", candidate.dedupe_key),
                notice: candidate.notice,
                needs_model: candidate.kind == WakeKind::ScoutReportReady,
            });
        }
    }

    /// Step 2: classify the durable events after the wake cursor. The cursor moves in the same
    /// transaction as the records, so a crash between them replays the events and loses none.
    fn observe(&self, settings: &OrchestratorSettings, now: u64) -> Result<usize, String> {
        let cursor = self
            .store
            .wake_cursor()
            .map_err(|_| "cannot read wake cursor".to_owned())?;
        let events = self
            .store
            .stream_events_after(cursor, EVENT_PAGE)
            .map_err(|_| "cannot read events".to_owned())?;
        let Some(last) = events.last().map(|event| event.sequence) else {
            return Ok(0);
        };
        let mut records = Vec::new();
        for event in &events {
            if event.kind == "task.report_ready"
                && self
                    .store
                    .task_report(&event.task_id)
                    .map_err(|_| "cannot read task report".to_owned())?
                    .is_none_or(|report| report.relayed_at.is_some())
            {
                continue;
            }
            let facts = if event.task_id.is_empty() {
                None
            } else {
                self.store
                    .task_wake_facts(&event.task_id)
                    .map_err(|_| "cannot read task facts".to_owned())?
            };
            // Only a delivery approval for a task that succeeded is a delivery notice.
            if event.kind == "approval.requested"
                && !self
                    .store
                    .get(&event.task_id)
                    .is_ok_and(|task| task.state == TaskState::Succeeded)
            {
                continue;
            }
            if let Some(candidate) = classify(event, facts.as_ref()) {
                let scope = facts
                    .as_ref()
                    .map_or(GLOBAL_SCOPE, |facts| facts.scope.as_str());
                Self::expand(candidate, scope, &mut records);
            }
        }
        self.record_at(settings, &records, last, now)
    }

    /// Adaptive work checks and deterministic stall notices.
    fn supervision_scan(&self, settings: &OrchestratorSettings, now: u64) -> Result<usize, String> {
        let loads = self
            .store
            .supervision_load()
            .map_err(|_| "cannot read supervision load".to_owned())?;
        let running = self
            .store
            .running_attempts()
            .map_err(|_| "cannot read running attempts".to_owned())?;
        let policy = settings.interval_policy();
        let wall = Duration::from_secs(u64::from(settings.supervision.attempt_wall_minutes) * 60);
        let mut found: Vec<(String, String, String, Stall)> = Vec::new();
        let mut records = Vec::new();
        let outstanding: BTreeSet<String> = self
            .store
            .pending_wake_records()
            .map_err(|_| "cannot read outstanding work checks".to_owned())?
            .into_iter()
            .filter(|record| record.kind == WakeKind::WorkCheck)
            .map(|record| record.scope)
            .collect();

        // Over wall time: every tick, not delayed by the interval.
        for attempt in &running {
            let activity = self.activity(attempt);
            if let Some(stall @ Stall::OverWallTime { .. }) =
                stalled(&activity, now, Duration::MAX, wall)
            {
                found.push((
                    attempt.scope.clone(),
                    attempt.task_id.clone(),
                    attempt.attempt_id.clone(),
                    stall,
                ));
            }
        }

        // No events: once per effective interval per scope that holds work.
        {
            let mut state = self.state();
            state
                .last_check_ms
                .retain(|scope, _| loads.iter().any(|l| &l.scope == scope && l.work_units >= 1));
            for load in loads.iter().filter(|load| load.work_units >= 1) {
                let previous = *state.last_check_ms.entry(load.scope.clone()).or_insert(now);
                let (Some(due), Some(interval)) = (
                    policy.due_ms(previous, load.work_units),
                    policy.interval(load.work_units),
                ) else {
                    continue;
                };
                if now < due {
                    continue;
                }
                state.last_check_ms.insert(load.scope.clone(), now);
                if settings.mode.runs_wake_turns() && !outstanding.contains(&load.scope) {
                    records.push(NewWakeRecord {
                        scope: load.scope.clone(),
                        kind: WakeKind::WorkCheck,
                        subject: "work".to_owned(),
                        dedupe_key: format!("check:{now}"),
                        notice: format!(
                            "Check on {} active work units. Report progress, failures and decisions that need attention.",
                            load.work_units
                        ),
                        needs_model: true,
                    });
                }
                let quiet_for = interval.max(STALL_QUIET_FLOOR);
                for attempt in running.iter().filter(|a| a.scope == load.scope) {
                    let activity = self.activity(attempt);
                    if let Some(stall @ Stall::NoEvents { .. }) =
                        stalled(&activity, now, quiet_for, Duration::MAX)
                    {
                        found.push((
                            attempt.scope.clone(),
                            attempt.task_id.clone(),
                            attempt.attempt_id.clone(),
                            stall,
                        ));
                    }
                }
            }
        }

        for (scope, task_id, attempt_id, stall) in found {
            let facts = self
                .store
                .task_wake_facts(&task_id)
                .map_err(|_| "cannot read task facts".to_owned())?;
            let candidate = stall_candidate(&task_id, &attempt_id, stall, facts.as_ref());
            Self::expand(candidate, &scope, &mut records);
        }
        if records.is_empty() {
            return Ok(0);
        }
        self.record(settings, &records, now)
    }

    fn activity(&self, attempt: &orchestrator_store::RunningAttempt) -> Activity {
        Activity {
            task_id: attempt.task_id.clone(),
            attempt_id: attempt.attempt_id.clone(),
            session_id: attempt.session_id.clone(),
            started_ms: attempt.started_ms,
            // No entry (a restart) counts the start as the last sign of life.
            last_event_ms: self
                .pulse
                .last_event_ms(&attempt.session_id)
                .unwrap_or(attempt.started_ms),
        }
    }

    /// A reminder for each proposal that has waited too long.
    fn proposal_scan(&self, settings: &OrchestratorSettings, now: u64) -> Result<usize, String> {
        let stale = self
            .store
            .stale_pending_proposals(now.saturating_sub(PROPOSAL_WAIT_MS))
            .map_err(|_| "cannot read proposals".to_owned())?;
        if stale.is_empty() {
            return Ok(0);
        }
        let records: Vec<NewWakeRecord> = stale
            .into_iter()
            .map(|proposal| NewWakeRecord {
                scope: proposal.scope,
                kind: WakeKind::ProposalWaiting,
                subject: "proposal".to_owned(),
                dedupe_key: format!("proposal:{}", proposal.id),
                notice: proposal_notice(&proposal.kind),
                needs_model: false,
            })
            .collect();
        self.record(settings, &records, now)
    }

    /// Start the batches whose window is due. The settings are read again for each
    /// batch, just before the claim, so a mode set to Off or a lowered cap after the top of the
    /// pass is honored.
    fn start_ready(&self, now: u64, report: &mut TickReport) -> Result<(), String> {
        let due = self.state().debounce.due(now);
        if due.is_empty() {
            return Ok(());
        }
        // The machine load gate: a wake turn starts an agent process, so it waits like a task
        // worker. Nothing is claimed, no ledger row is written and no cap is spent. The notices
        // already stand, and the windows stay due for the first tick after the gate opens.
        if matches!(self.load.start_gate(Instant::now()), StartGate::Closed(_)) {
            return Ok(());
        }
        let pending: HashMap<String, WakeRecord> = self
            .store
            .pending_wake_records()
            .map_err(|_| "cannot read wake records".to_owned())?
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect();
        for scope in due {
            let Some(target) = target_of(&scope) else {
                self.state().debounce.forget(&scope);
                continue;
            };
            // One wake turn per scope, and a wake waits while a user turn runs.
            match self.store.running_conversation_turn(&target) {
                Ok(None) => {}
                Ok(Some(_)) | Err(_) => continue,
            }
            let ids = self.state().debounce.take(&scope);
            // One report per turn keeps the existing relay prompt bound. Other reports stay
            // pending rather than being marked handled without their text reaching the model.
            let mut has_report = false;
            let mut deferred = Vec::new();
            let records: Vec<&WakeRecord> = ids
                .iter()
                .filter_map(|id| pending.get(id))
                .filter(|record| {
                    if record.kind == WakeKind::ScoutReportReady {
                        if has_report {
                            deferred.push(record.id.clone());
                            return false;
                        }
                        has_report = true;
                    }
                    true
                })
                .collect();
            self.offer_again(&scope, &deferred, now);
            let Some(first) = records.first() else {
                continue;
            };
            let ids: Vec<String> = records.iter().map(|record| record.id.clone()).collect();
            // From here to the claim, an error puts the records back in a window.
            let settings = self.or_offer(self.settings(), &scope, &ids, now)?;
            if !settings.mode.runs_wake_turns() {
                report.skipped +=
                    self.or_offer(self.skip(&ids, SkipReason::ModeOff), &scope, &ids, now)?;
                continue;
            }
            let usage = self.or_offer(
                self.store
                    .wake_usage(&scope, now)
                    .map_err(|_| "cannot read wake usage".to_owned()),
                &scope,
                &ids,
                now,
            )?;
            if let Admission::Paused { reason, .. } = admit(&settings.wake_limits, &usage) {
                report.skipped +=
                    self.or_offer(self.skip(&ids, skip_reason(reason)), &scope, &ids, now)?;
                continue;
            }
            let relay = self.or_offer(
                records
                    .iter()
                    .find(|record| record.kind == WakeKind::ScoutReportReady)
                    .map(|record| {
                        let report = self
                            .store
                            .task_report(&record.subject)
                            .map_err(|_| "cannot read task report".to_owned())?
                            .ok_or_else(|| "wake report is missing".to_owned())?;
                        Ok(super::report::relay_block(
                            &report.task_id,
                            &report.body,
                            report.truncated,
                        ))
                    })
                    .transpose(),
                &scope,
                &ids,
                now,
            )?;
            // The turn count in the ledger joins the seed, so a batch that a user message
            // cancelled and that is requeued unchanged still gets a new turn id. A replayed
            // claim sees the same count, so a restart never makes a second turn for a record.
            let batch_id = stable_id(
                "wake-batch",
                &format!("{}:{}:{}", first.id, first.attempts, usage.day_count),
            );
            let turn_id = stable_id("wake-turn", &batch_id);
            let claimed = self.or_offer(
                self.store
                    .claim_wake_batch(
                        &scope,
                        &ids,
                        &batch_id,
                        &turn_id,
                        settings.mode.notifies(),
                        now,
                    )
                    .map_err(|_| "cannot claim wake batch".to_owned()),
                &scope,
                &ids,
                now,
            )?;
            if claimed.is_empty() {
                self.offer_again(&scope, &ids, now);
                continue;
            }
            let brief = WakeBrief {
                events: claimed.iter().map(|record| record.notice.clone()).collect(),
            };
            match self
                .conversations
                .start_wake(&target, &turn_id, &brief, relay.as_deref())
            {
                Ok(_) => report.started += 1,
                Err(ConversationError::TurnRunning { .. }) => {
                    // A user turn slipped in: the records go back, no attempt is spent.
                    self.settle(
                        &batch_id,
                        Settle::Requeue {
                            count_attempt: false,
                        },
                    )?;
                    self.offer_again(&scope, &ids, now);
                }
                Err(_) => {
                    self.settle(&batch_id, Settle::Failed)?;
                    self.diagnostics.warn(LOG, "wake_turn_not_started", &[]);
                }
            }
        }
        Ok(())
    }

    /// Passes `result` through, and on an error offers the records to the debounce again so
    /// they are not stranded until a restart.
    fn or_offer<T>(
        &self,
        result: Result<T, String>,
        scope: &str,
        ids: &[String],
        now: u64,
    ) -> Result<T, String> {
        if result.is_err() {
            self.offer_again(scope, ids, now);
        }
        result
    }

    fn skip(&self, ids: &[String], reason: SkipReason) -> Result<usize, String> {
        self.store
            .skip_wake_records(ids, reason)
            .map_err(|_| "cannot skip wake records".to_owned())
    }

    fn offer_again(&self, scope: &str, ids: &[String], now: u64) {
        let mut state = self.state();
        for id in ids {
            state.debounce.offer(scope, id, now);
        }
    }

    /// Step 6: hourly, forget ledger rows older than two days.
    fn prune(&self, now: u64) {
        {
            let mut state = self.state();
            if now.saturating_sub(state.last_prune_ms) < HOUR_MS {
                return;
            }
            state.last_prune_ms = now;
        }
        if self
            .store
            .prune_wake_ledger(now.saturating_sub(LEDGER_KEEP_MS))
            .is_err()
        {
            self.diagnostics.warn(LOG, "wake_prune_failed", &[]);
        }
    }
}

/// The records of one running batch.
struct Batch {
    batch_id: String,
    turn_id: String,
    records: Vec<WakeRecord>,
}

fn batches(running: &[WakeRecord]) -> Vec<Batch> {
    let mut out: Vec<Batch> = Vec::new();
    for record in running {
        let (Some(batch_id), Some(turn_id)) = (&record.batch_id, &record.turn_id) else {
            continue;
        };
        match out.iter_mut().find(|batch| &batch.batch_id == batch_id) {
            Some(batch) => batch.records.push(record.clone()),
            None => out.push(Batch {
                batch_id: batch_id.clone(),
                turn_id: turn_id.clone(),
                records: vec![record.clone()],
            }),
        }
    }
    out
}

#[cfg(test)]
#[path = "wake_tests.rs"]
mod tests;
