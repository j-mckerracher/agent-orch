//! The standing-grant matcher (DPES-1105 phase 5a).
//!
//! A thread polls every two seconds. It finds pending proposals that a standing grant covers
//! and approves each one with the grant as the actor, then runs it exactly as a user approval
//! would. A grant is never an override: it never approves a proposal from a turn that was not
//! `Restricted`, never covers a kind on the never-grantable list, never covers a
//! `workstream.update` that sets the state, never approves a proposal that existed before it
//! was created, and never skips the approve-time re-check against the project's current
//! defaults. A denial moves the card to `failed` exactly as it does for a user approval.
//!
//! The grant scope relates to what an action writes, not to the conversation it came from, so
//! each proposal is resolved to the project codes and workstream ids it touches (`touched`).
//! Every entity the action writes must lie inside the grant.
//!
//! The clock is a parameter of [`DaemonApi::grant_matcher_tick`]: the thread passes wall time
//! and a test passes any reading and calls it directly. A store error in a tick is logged by
//! event name only (never a path, id or value), and the tick goes on with the next step.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Condvar, MutexGuard, Weak};
use std::thread::{self, JoinHandle};

use orchestrator_core::orchestrator_action::Action;
use orchestrator_core::standing_grant::{check_kind, GrantScope, StandingGrant};
use orchestrator_store::{
    ActionProposal, GrantClaimError, ProposalState, ResolveError, WorkstreamStoreError,
};

use super::projects_defaults::project_id_for_code;
use super::*;

const LOG: &str = "grants";
/// How often the loop thread runs a pass.
const TICK: Duration = Duration::from_secs(2);
/// Open grants read, and proposals tried, per pass.
const GRANT_PAGE: usize = 200;
const TRY_PAGE: usize = 50;
/// Consecutive failed tries after which a proposal is marked tried and left to the user.
const MAX_TRY_FAILURES: u8 = 3;

/// Per-daemon try bookkeeping, in memory only: failures are cleared on success and gone on restart.
#[derive(Default)]
pub(super) struct TryState {
    /// Consecutive failed tries per proposal id.
    failures: BTreeMap<String, u8>,
    /// Proposal ids whose try fails on purpose, so a test can drive the failure path.
    #[cfg(test)]
    pub(super) forced_errors: BTreeSet<String>,
}

impl TryState {
    fn forced_error(&self, _id: &str) -> bool {
        #[cfg(test)]
        {
            self.forced_errors.contains(_id)
        }
        #[cfg(not(test))]
        {
            false
        }
    }

    /// Counts one failed try of `id`. True on the failure that reaches the limit (and resets).
    fn failed_for_good(&mut self, id: &str) -> bool {
        let count = self.failures.entry(id.to_owned()).or_insert(0);
        *count += 1;
        if *count >= MAX_TRY_FAILURES {
            self.failures.remove(id);
            return true;
        }
        false
    }
}

/// What one pass did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct MatchReport {
    /// Grant-approved proposals found approved and run again (a stopped run, or a restart).
    pub reconciled: usize,
    /// New proposals looked at.
    pub tried: usize,
    /// New proposals a grant approved.
    pub approved: usize,
}

/// The matcher thread, kept by the API so `shutdown` can stop it.
pub(super) struct GrantMatcherHandle {
    stop: Arc<(Mutex<Signal>, Condvar)>,
    thread: JoinHandle<()>,
}

/// What wakes the matcher thread before its tick: a stop, or a completed turn.
#[derive(Default)]
struct Signal {
    stopped: bool,
    poked: bool,
}

/// What an action writes: project codes and workstream ids.
#[derive(Default)]
struct Touched {
    projects: BTreeSet<String>,
    workstreams: BTreeSet<String>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

impl DaemonApi {
    /// Spawns the thread "grant-matcher": a pass, then two seconds of sleep, until poked or
    /// stopped. Each pass first runs the autonomous actions of the turns that completed since the
    /// last one (DPES-1108 phase 5), then the grant pass, so a grant never spends a use on a card
    /// the orchestrator may run itself. A completed turn pokes the thread, so its actions run at
    /// once. The thread holds only a weak reference, so a dropped API ends it. Called once by
    /// `main` after the API is in an `Arc`; the API never starts it for itself, so a test that
    /// builds one never has a matcher running behind it.
    pub(crate) fn start_grant_matcher(self: &Arc<Self>) {
        let weak: Weak<Self> = Arc::downgrade(self);
        let stop = Arc::new((Mutex::new(Signal::default()), Condvar::new()));
        let poke = stop.clone();
        self.conversations.on_turn_completed(Box::new(move || {
            let (flag, wake) = &*poke;
            lock(flag).poked = true;
            wake.notify_all();
        }));
        let signal = stop.clone();
        let spawned = thread::Builder::new()
            .name("grant-matcher".into())
            .spawn(move || loop {
                let Some(api) = weak.upgrade() else {
                    return;
                };
                let now_ms = crate::executor::unix_now_ms();
                let passed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    api.autonomy_tick(now_ms);
                    api.grant_matcher_tick(now_ms)
                }));
                if passed.is_err() {
                    api.config
                        .diagnostics
                        .error(LOG, "grant_tick_panicked", &[]);
                }
                drop(api);
                let (flag, wake) = &*signal;
                let (mut state, _) = wake
                    .wait_timeout_while(lock(flag), TICK, |state| !state.stopped && !state.poked)
                    .unwrap_or_else(|error| error.into_inner());
                if state.stopped {
                    return;
                }
                state.poked = false;
            });
        match spawned {
            Ok(thread) => *lock(&self.grant_matcher) = Some(GrantMatcherHandle { stop, thread }),
            Err(_) => self
                .config
                .diagnostics
                .error(LOG, "grant_thread_not_started", &[]),
        }
    }

    /// Stops the matcher thread and waits for it. Nothing happens when none runs.
    pub(super) fn stop_grant_matcher(&self) {
        let Some(handle) = lock(&self.grant_matcher).take() else {
            return;
        };
        let (flag, wake) = &*handle.stop;
        lock(flag).stopped = true;
        wake.notify_all();
        let _ = handle.thread.join();
    }

    /// One pass at `now_ms`.
    ///
    /// 1. Runs again every proposal a grant approved and that is still approved: a crash or a
    ///    store error between the claim and the end of the execution leaves one. The run
    ///    checks the grant first, so a revoked or expired grant fails it instead.
    /// 2. With no open grant, stops: nothing else is read or written.
    /// 3. Tries each new pending proposal once, oldest first.
    pub(super) fn grant_matcher_tick(&self, now_ms: u64) -> MatchReport {
        let mut report = MatchReport::default();
        self.reconcile_grant_approvals(now_ms, &mut report);
        let grants = match self.store.standing_grants(false, now_ms, GRANT_PAGE) {
            Ok(grants) => grants,
            Err(_) => {
                self.config.diagnostics.warn(LOG, "grant_list_failed", &[]);
                return report;
            }
        };
        let Some(floor) = grants.iter().map(|grant| grant.proposal_floor).min() else {
            return report;
        };
        let pending = match self.store.untried_pending_proposals(floor, TRY_PAGE) {
            Ok(pending) => pending,
            Err(_) => {
                self.config
                    .diagnostics
                    .warn(LOG, "grant_pending_failed", &[]);
                return report;
            }
        };
        for (proposal, sequence) in pending {
            // A proposal whose try failed on the store stays untried and is tried again, up to
            // `MAX_TRY_FAILURES` times in a row; then it is marked tried so it cannot starve
            // newer ones, and falls back to the user's card.
            let Ok(approved) = self.try_match(&proposal, sequence, &grants, now_ms) else {
                self.config.diagnostics.warn(LOG, "grant_try_failed", &[]);
                if lock(&self.grant_try_state).failed_for_good(&proposal.id) {
                    self.config.diagnostics.warn(LOG, "grant_try_given_up", &[]);
                    if self.store.mark_grant_tried(&proposal.id).is_err() {
                        self.config.diagnostics.warn(LOG, "grant_mark_failed", &[]);
                    }
                }
                continue;
            };
            lock(&self.grant_try_state).failures.remove(&proposal.id);
            report.tried += 1;
            report.approved += usize::from(approved);
            if self.store.mark_grant_tried(&proposal.id).is_err() {
                self.config.diagnostics.warn(LOG, "grant_mark_failed", &[]);
            }
        }
        report
    }

    fn reconcile_grant_approvals(&self, now_ms: u64, report: &mut MatchReport) {
        let approved = match self.store.approved_by_grant() {
            Ok(approved) => approved,
            Err(_) => {
                self.config
                    .diagnostics
                    .warn(LOG, "grant_reconcile_failed", &[]);
                return;
            }
        };
        for (proposal, _grant_id) in approved {
            let held = lock(&self.idempotency_gate);
            // Read again under the gate: a run that finished since the list was read is done.
            let current = match self.store.action_proposal(&proposal.id) {
                Ok(Some(current)) if current.state == ProposalState::Approved => current,
                Ok(_) => continue,
                Err(_) => {
                    self.config
                        .diagnostics
                        .warn(LOG, "grant_reconcile_failed", &[]);
                    continue;
                }
            };
            match self.execute_locked_at(&held, &current, now_ms) {
                Ok(()) => report.reconciled += 1,
                Err(_) => self
                    .config
                    .diagnostics
                    .warn(LOG, "grant_execute_failed", &[]),
            }
        }
    }

    /// Tries to approve one pending proposal under a grant and run it. `Ok(true)` when a grant
    /// approved it. `Err` only for a store failure, so the proposal is tried again.
    fn try_match(
        &self,
        proposal: &ActionProposal,
        creation_sequence: i64,
        grants: &[StandingGrant],
        now_ms: u64,
    ) -> Result<bool, ()> {
        if lock(&self.grant_try_state).forced_error(&proposal.id) {
            return Err(());
        }
        if proposal.state != ProposalState::Pending
            || proposal.denial.is_some()
            || proposal.depends_on.is_some()
        {
            return Ok(false);
        }
        // Only a Restricted turn carries a proposal a grant may approve.
        let turn = self
            .store
            .conversation_turn(&proposal.turn_id)
            .map_err(|_| ())?;
        if turn.and_then(|turn| turn.permission_level) != Some(PermissionLevel::Restricted) {
            return Ok(false);
        }
        if check_kind(proposal.kind).is_err() {
            return Ok(false);
        }
        let Ok(action) = serde_json::from_str::<Action>(&proposal.params_json) else {
            return Ok(false);
        };
        if !delivery_stays_in_the_envelope(&action) {
            return Ok(false);
        }
        let Some(touched) = self.touched(&action)? else {
            return Ok(false);
        };
        let mut candidates = Vec::new();
        for grant in grants {
            if grant.proposal_floor < creation_sequence
                && grant.is_active(now_ms)
                && grant.covers(proposal.kind)
                && self.in_scope(grant, &touched)?
            {
                candidates.push(grant);
            }
        }
        // Workstream grants first, then the soonest expiry, then the lowest id.
        candidates.sort_by_key(|grant| {
            (
                matches!(grant.scope, GrantScope::Project(_)),
                grant.expires_at_ms,
                grant.id.clone(),
            )
        });
        let Some(first) = candidates.first() else {
            return Ok(false);
        };

        // Claim and run under one hold of the gate, as a user approval does.
        let held = lock(&self.idempotency_gate);
        // The re-check an approval makes, before any use is spent. A refusal ends the card as it
        // would for a user: `failed` with `action_denied`, and no grant approved it.
        let denial = match self.recheck_task_denial(proposal).map_err(|_| ())? {
            Some(denial) => Some(denial),
            None => self.recheck_promotion(proposal).map_err(|_| ())?,
        };
        if let Some(denial) = denial {
            let actor = format!("grant:{}", first.id);
            return match self
                .store
                .deny_action_proposal(&proposal.id, denial.as_str(), &actor)
            {
                Ok(_) | Err(ResolveError::NotPending(_) | ResolveError::NotFound) => Ok(false),
                Err(_) => Err(()),
            };
        }
        for grant in candidates {
            match self.store.approve_action_proposal_by_grant(
                &proposal.id,
                &proposal.digest,
                &grant.id,
                now_ms,
            ) {
                Ok(approved) => {
                    if self.execute_locked_at(&held, &approved, now_ms).is_err() {
                        // Left approved: the next pass runs it again.
                        self.config
                            .diagnostics
                            .warn(LOG, "grant_execute_failed", &[]);
                    }
                    return Ok(true);
                }
                // This grant cannot take the use. The next one may.
                Err(GrantClaimError::Grant(_)) => continue,
                Err(GrantClaimError::Proposal(ResolveError::Store(_))) => return Err(()),
                // A user decided first, or the proposal changed: nothing to approve.
                Err(_) => return Ok(false),
            }
        }
        Ok(false)
    }

    /// What `action` writes, as project codes and workstream ids. `None` for an action no grant
    /// can cover or that names something that is not there. The match has no wildcard, so a
    /// new kind forces a decision here.
    fn touched(&self, action: &Action) -> Result<Option<Touched>, ()> {
        let mut found = Touched::default();
        match action {
            Action::WorkstreamUpdate(update) => {
                // Pausing or archiving a workstream is never covered.
                if update.state.is_some() {
                    return Ok(None);
                }
                found.workstreams.insert(update.workstream_id.clone());
            }
            Action::ObjectiveAdd(add) => {
                // An objective added to a workstream the same block creates has no scope yet.
                let (None, Some(workstream_id)) = (&add.workstream_ref, &add.workstream_id) else {
                    return Ok(None);
                };
                found.workstreams.insert(workstream_id.clone());
            }
            Action::ObjectiveRevise(revise) => match self.store.get_objective(&revise.objective_id)
            {
                Ok(objective) => {
                    found.workstreams.insert(objective.workstream_id);
                }
                Err(WorkstreamStoreError::NotFound { .. }) => return Ok(None),
                Err(_) => return Err(()),
            },
            Action::TaskPropose(propose) => {
                if propose.workstream_ref.is_some()
                    || project_id_for_code(&self.store, &propose.project).is_none()
                {
                    return Ok(None);
                }
                found.projects.insert(propose.project.clone());
                if let Some(workstream_id) = &propose.workstream_id {
                    found.workstreams.insert(workstream_id.clone());
                }
            }
            Action::TaskRetry(retry) => {
                let Some(code) = self.task_project_code(&retry.task_id)? else {
                    return Ok(None);
                };
                found.projects.insert(code);
            }
            Action::WorkstreamLinkTask(link) => {
                let Some(code) = self.task_project_code(&link.task_id)? else {
                    return Ok(None);
                };
                found.projects.insert(code);
                found.workstreams.insert(link.workstream_id.clone());
            }
            // A merge is covered only by a merge grant, which names one project. The host reads
            // that decide the rest (head, checks, branch, repository) run at execution.
            Action::DeliveryMerge(merge) => {
                let Some(code) = self.task_project_code(&merge.task_id)? else {
                    return Ok(None);
                };
                found.projects.insert(code);
            }
            Action::WorkstreamCreate(_)
            | Action::TaskCancel(_)
            | Action::TaskRevise(_)
            | Action::ProjectRemove(_)
            | Action::DecisionHold(_)
            | Action::DecisionAsk(_)
            | Action::TaskMessage(_)
            | Action::MemorySet(_) => return Ok(None),
        }
        Ok(Some(found))
    }

    fn task_project_code(&self, task_id: &str) -> Result<Option<String>, ()> {
        match self.store.get(task_id) {
            Ok(task) => Ok(Some(project_code(&task.contract.project_id))),
            Err(StoreError::NotFound(_)) => Ok(None),
            Err(_) => Err(()),
        }
    }

    /// Whether every entity the action writes lies inside the grant. A project grant covers
    /// actions that write only its project (and no workstream). A workstream grant covers
    /// actions that write only its workstream, on projects that are already member projects of
    /// it: a workstream grant never starts work on a project the user did not put there.
    fn in_scope(&self, grant: &StandingGrant, touched: &Touched) -> Result<bool, ()> {
        match &grant.scope {
            GrantScope::Project(code) => Ok(touched.workstreams.is_empty()
                && !touched.projects.is_empty()
                && touched.projects.iter().all(|project| project == code)),
            GrantScope::Workstream(id) => {
                if !touched
                    .workstreams
                    .iter()
                    .all(|workstream| workstream == id)
                {
                    return Ok(false);
                }
                if touched.projects.is_empty() {
                    return Ok(true);
                }
                let members: BTreeSet<String> = self
                    .store
                    .list_workstream_tasks(id)
                    .map_err(|_| ())?
                    .iter()
                    .map(|task| project_code(&task.project_id))
                    .collect();
                Ok(touched
                    .projects
                    .iter()
                    .all(|project| members.contains(project)))
            }
        }
    }
}

/// A grant covers a task only when its delivery is report-only or a local review: a pull request
/// widens what the task may do beyond the chat envelope.
fn delivery_stays_in_the_envelope(action: &Action) -> bool {
    match action {
        Action::TaskPropose(propose) => matches!(
            propose.delivery,
            DeliveryPolicy::ReportOnly | DeliveryPolicy::LocalReview
        ),
        _ => true,
    }
}
