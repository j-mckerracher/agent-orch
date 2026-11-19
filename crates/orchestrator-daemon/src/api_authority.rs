//! Autonomous actions (DPES-1108 phase 5, plan 5.5).
//!
//! A completed turn's block is stored as cards, exactly as before. Right after, the daemon's
//! autonomy pass looks at that turn's own new cards once, in block order, and runs each one the
//! authority model lets the orchestrator run on its own: it approves the card with the actor
//! `agent` and executes it through the same path a user approval takes. Everything else stays a
//! card for the user. The pass looks at a turn once: a card it leaves is never picked up later,
//! so turning the switch back on does not run old cards, and a stop before the pass leaves the
//! cards pending, which is the safe direction.
//!
//! The bounds (the guard for "never widens"):
//! - the user's `autonomous_actions` setting is on;
//! - the kind is [`ActionAuthority::Autonomous`] and [`autonomy_check`] passes: a turn without
//!   user words (a wake, or a turn that read an agent's report) may only retry, message or ask
//!   about existing work, no pull-request task, at most `MAX_AUTONOMOUS_PER_TURN` per turn;
//! - the turn completed at a level that may carry actions;
//! - a task's project is registered and not removed, and a named task exists in such a project;
//!   work is routed only to such a project whose coordinator is active, and one routed request
//!   starts at most `MAX_AUTONOMOUS_TASKS_PER_REQUEST` tasks on its own;
//! - the approve-time re-checks a user approval makes (envelope, promotion) pass;
//! - a task the orchestrator starts cites its user turn as its origin, or, for a coordinator's
//!   task, the open routed request it serves; written before the task is admitted
//!   (`ProposeExec::run`), and the store refuses any other origin.
//!
//! Capacity caps, the load gate, the wake budget and attempt limits are not touched: a task the
//! orchestrator starts is queued like any other, and a wake turn runs only inside the budget.
//!
//! [`ActionAuthority::Autonomous`]: orchestrator_core::orchestrator_action::ActionAuthority

use orchestrator_core::orchestrator_action::{
    autonomy_check, Action, MAX_AUTONOMOUS_TASKS_PER_REQUEST,
};
use orchestrator_store::{
    ActionOutcome, ActionProposal, ProposalState, TaskOverride, TurnState, TurnTrigger,
};

use super::actions::map_resolve_error;
use super::conversation_runtime::level_allows_actions;
use super::projects_defaults::{envelope_for, project_id_for_code, typed};
use super::*;

/// The approver an autonomous run records: the orchestrator agent itself. Its proposal names the
/// turn, so the audit trail ties every such approval to the turn that issued it.
pub(super) const AUTONOMOUS_ACTOR: &str = "agent";
/// Failure code of an autonomous approval the restart path finds the switch turned off for.
const AUTONOMY_OFF: &str = "autonomy_off";

const LOG: &str = "autonomy";

impl DaemonApi {
    /// Runs the autonomous actions of every turn queued since the last pass. Called by the matcher
    /// thread before its grant pass.
    pub(super) fn autonomy_tick(&self, now_ms: u64) {
        for turn in self.conversations.take_completed_turns() {
            self.run_autonomous(&turn.turn_id, &turn.proposals, now_ms);
        }
    }

    /// Runs, for one completed turn, each of `proposals` the bounds allow, in block order, and
    /// returns how many it approved. A card it does not run stays pending for the user.
    pub(super) fn run_autonomous(&self, turn_id: &str, proposals: &[String], now_ms: u64) -> usize {
        match self.store.orchestrator_settings() {
            Ok(settings) if settings.autonomous_actions => {}
            Ok(_) => return 0,
            Err(_) => {
                self.config
                    .diagnostics
                    .warn(LOG, "autonomy_settings_unread", &[]);
                return 0;
            }
        }
        let from_user = match self.turn_authority(turn_id) {
            Ok(Some(from_user)) => from_user,
            Ok(None) => return 0,
            Err(()) => {
                self.config
                    .diagnostics
                    .warn(LOG, "autonomy_turn_unread", &[]);
                return 0;
            }
        };
        let mut ran = 0;
        for id in proposals {
            let proposal = match self.store.action_proposal(id) {
                Ok(Some(proposal)) => proposal,
                Ok(None) => continue,
                Err(_) => {
                    self.config
                        .diagnostics
                        .warn(LOG, "autonomy_proposal_unread", &[]);
                    continue;
                }
            };
            // A card an earlier turn made, that this block only matched, is not this turn's.
            if proposal.turn_id != turn_id
                || proposal.state != ProposalState::Pending
                || proposal.denial.is_some()
            {
                continue;
            }
            let Ok(action) = serde_json::from_str::<Action>(&proposal.params_json) else {
                continue;
            };
            if autonomy_check(&action, from_user, ran).is_err() {
                continue;
            }
            match self.inside_bounds(&action) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(()) => {
                    self.config
                        .diagnostics
                        .warn(LOG, "autonomy_bounds_unread", &[]);
                    continue;
                }
            }
            let held = self
                .idempotency_gate
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            // The re-checks a user approval makes. A refusal leaves the card to the user, whose
            // approval meets the same check.
            let denied = match self.recheck_task_denial(&proposal) {
                Ok(None) => self.recheck_promotion(&proposal),
                other => other,
            };
            if !matches!(denied, Ok(None)) {
                continue;
            }
            // A user who decided first, or a parent that did not run, refuses the approval.
            let Ok(approved) = self
                .store
                .approve_action_proposal(&proposal.id, AUTONOMOUS_ACTOR)
            else {
                continue;
            };
            ran += 1;
            // A failure leaves the proposal approved; the next start finishes it.
            if self.execute_locked_at(&held, &approved, now_ms).is_err() {
                self.config
                    .diagnostics
                    .warn(LOG, "autonomy_execute_failed", &[]);
            }
        }
        ran
    }

    /// Whether the turn may run actions on its own, and if so whether it has user words to cite
    /// (`Some(true)`: a user message opened it and it read no agent report). `None` for a turn
    /// that is gone, did not complete, or ran at a level that carries no actions.
    pub(super) fn turn_authority(&self, turn_id: &str) -> Result<Option<bool>, ()> {
        let Some(turn) = self.store.conversation_turn(turn_id).map_err(|_| ())? else {
            return Ok(None);
        };
        if turn.state != TurnState::Completed
            || !turn.permission_level.is_some_and(level_allows_actions)
        {
            return Ok(None);
        }
        let read_report = self.store.turn_read_report(turn_id).map_err(|_| ())?;
        Ok(Some(turn.trigger == TurnTrigger::User && !read_report))
    }

    /// The project bound: a task's project is registered and not removed, and a task the action
    /// names exists in such a project. Work is routed on its own only to such a project whose
    /// coordinator is active (DPES-1108 integration); the parser already lets only the global
    /// conversation route. The executions refuse most of these too; checking first keeps a
    /// refused action a card the user can read instead of a failed run.
    fn inside_bounds(&self, action: &Action) -> Result<bool, ()> {
        let project = match action {
            Action::TaskPropose(propose) => {
                // A routed request starts at most so many tasks on its own; the store holds the
                // same rule when the origin is written.
                if let Some(request) = &propose.request {
                    let started = self
                        .store
                        .routed_request_origin_count(request)
                        .map_err(|_| ())?;
                    if started >= MAX_AUTONOMOUS_TASKS_PER_REQUEST {
                        return Ok(false);
                    }
                }
                match project_id_for_code(&self.store, &propose.project) {
                    Some(project) => project,
                    None => return Ok(false),
                }
            }
            Action::CoordinatorRoute(route) => {
                let Some(project) = project_id_for_code(&self.store, &route.project) else {
                    return Ok(false);
                };
                if self
                    .store
                    .coordinator_scope(&project)
                    .map_err(|_| ())?
                    .removed
                {
                    return Ok(false);
                }
                project
            }
            Action::WorkstreamLinkTask(link) => match self.task_project(&link.task_id)? {
                Some(project) => project,
                None => return Ok(false),
            },
            other => match other.target_task() {
                Some(task_id) => match self.task_project(task_id)? {
                    Some(project) => project,
                    None => return Ok(false),
                },
                None => return Ok(true),
            },
        };
        match self.store.project_is_archived(&project) {
            Ok(archived) => Ok(!archived),
            Err(StoreError::RecordNotFound { .. }) => Ok(false),
            Err(_) => Err(()),
        }
    }

    fn task_project(&self, task_id: &str) -> Result<Option<String>, ()> {
        match self.store.get(task_id) {
            Ok(task) => Ok(Some(task.contract.project_id)),
            Err(StoreError::NotFound(_)) => Ok(None),
            Err(_) => Err(()),
        }
    }

    /// The restart path's check of a proposal the orchestrator approved: the switch is still on,
    /// the turn still has authority and the action still passes [`autonomy_check`]. The turn
    /// cap is not counted again here. `Some(code)` fails the proposal with that code.
    pub(super) fn autonomous_refusal(
        &self,
        proposal: &ActionProposal,
    ) -> Result<Option<&'static str>, RpcError> {
        let settings = self
            .store
            .orchestrator_settings()
            .map_err(map_store_error)?;
        if !settings.autonomous_actions {
            return Ok(Some(AUTONOMY_OFF));
        }
        let Ok(action) = serde_json::from_str::<Action>(&proposal.params_json) else {
            return Ok(Some("invalid"));
        };
        let from_user = match self.turn_authority(&proposal.turn_id) {
            Ok(Some(from_user)) => from_user,
            Ok(None) => return Ok(Some("user_word_required")),
            Err(()) => return Err(RpcError::Internal),
        };
        Ok(autonomy_check(&action, from_user, 0)
            .err()
            .map(|refusal| refusal.code()))
    }

    /// The message of the user turn `turn_id`, verbatim, when a user message opened it and it read
    /// no agent report. `None` otherwise, or once a history clear deleted the message.
    pub(super) fn user_words(&self, turn_id: &str) -> Result<Option<String>, ()> {
        if self.turn_authority(turn_id)? != Some(true) {
            return Ok(None);
        }
        Ok(self
            .store
            .conversation_entry(&format!("{turn_id}:user"))
            .map_err(|_| ())?
            .map(|entry| entry.body)
            .filter(|body| !body.is_empty()))
    }

    /// The one-task override a task proposed from the user turn `turn_id` carries: its delivery
    /// when that departs from the project's default, with the user's `words`. `None` when it
    /// does not depart, or the default cannot be read. Nothing reads it for another task.
    pub(super) fn delivery_override(
        &self,
        contract: &TaskSubmission,
        words: &str,
        turn_id: &str,
    ) -> Option<TaskOverride> {
        let default = self.project_default_delivery(&contract.project_id, contract.kind)?;
        (contract.delivery != default).then(|| TaskOverride {
            task_id: String::new(),
            field: "delivery".to_owned(),
            project_default: delivery_name(default).to_owned(),
            chosen: delivery_name(contract.delivery).to_owned(),
            user_words: words.to_owned(),
            turn_id: turn_id.to_owned(),
            created_at: String::new(),
        })
    }

    /// The delivery a chat task of `kind` gets in the project when nobody asks for another: the
    /// mode's delivery when chat may ask for it, else a local review, else a report. A scout
    /// always reports, and a project that runs in place can only report.
    fn project_default_delivery(&self, project_id: &str, kind: TaskKind) -> Option<DeliveryPolicy> {
        let envelope = envelope_for(&self.store, Some(project_id), kind).ok()?;
        let mode = self
            .store
            .project_defaults(project_id)
            .ok()?
            .map(|row| typed(&row).mode.delivery());
        let preferred = match (kind, mode) {
            (TaskKind::Investigate, _) => DeliveryPolicy::ReportOnly,
            (TaskKind::Implement, Some(delivery)) => delivery,
            (TaskKind::Implement, None) => DeliveryPolicy::LocalReview,
        };
        Some(
            [
                preferred,
                DeliveryPolicy::LocalReview,
                DeliveryPolicy::ReportOnly,
            ]
            .into_iter()
            .find(|delivery| envelope.delivery_grants(*delivery))
            .unwrap_or(DeliveryPolicy::ReportOnly),
        )
    }

    /// Records a refused autonomous approval as failed, with the orchestrator as the actor.
    pub(super) fn fail_autonomous(
        &self,
        proposal: &ActionProposal,
        code: &str,
    ) -> Result<(), RpcError> {
        self.store
            .record_action_outcome(
                &proposal.id,
                ActionOutcome::Failed { error_code: code },
                AUTONOMOUS_ACTOR,
            )
            .map(|_| ())
            .map_err(map_resolve_error)
    }
}

/// The wire name of a delivery, as a proposal spells it.
fn delivery_name(delivery: DeliveryPolicy) -> &'static str {
    match delivery {
        DeliveryPolicy::ReportOnly => "ReportOnly",
        DeliveryPolicy::LocalReview => "LocalReview",
        DeliveryPolicy::PullRequest => "PullRequest",
    }
}
