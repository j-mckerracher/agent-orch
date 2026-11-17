//! Action proposal resolution and execution (DPES-1105).
//!
//! `actions.resolve` is the only way a proposal leaves `pending`. Approval runs
//! the change the proposal names, a workstream write or a task action, under its
//! own idempotency key, with the idempotency gate held, so a clear or another
//! workstream write cannot land between approve and execute. Startup finishes any
//! approved proposal that a stopped process left behind.

use std::sync::MutexGuard;

use orchestrator_core::orchestrator_action::{Action, ProposeTask, ReviseTask};
use orchestrator_core::permission::PermissionLevel;
use orchestrator_core::task::AuditEvent;
use orchestrator_core::task_envelope::{check_task, CHAT_TASK_MAX_REPAIRS, CHAT_TASK_MAX_RETRIES};
use orchestrator_core::workstream::{WorkstreamRuleError, WorkstreamState};
use orchestrator_store::{
    ActionOutcome, ActionProposal, ArchiveError, ConversationTarget, GrantOutcome, HoldSubject,
    LineageError, LineageRelation, ProposalState, ResolveError, TaskRecord, WorkstreamStoreError,
};

use super::projects_defaults::envelope_for;
use super::workstreams::{
    AddObjectiveParams, CreateParams, GateParams, LinkTaskParams, ReviseObjectiveParams,
    UpdateParams,
};
use super::*;

/// Event actor of every 1c transition: the approver the audit trail records.
const APPROVER: &str = "user";
/// Most proposals one `conversations.get` page lists, and the most bytes they take.
const PROPOSAL_PAGE: usize = 100;
const PROPOSAL_BYTES: usize = 256 * 1024;
/// Longest task title a proposal's display facts carry.
const TARGET_TITLE_BYTES: usize = 80;
/// Failure code for a proposal the daemon will not run as stored.
const INVALID: &str = "invalid";
/// Failure codes of a proposal a grant approved but that did not start: the grant was revoked,
/// or had expired, by then.
pub(super) const GRANT_REVOKED: &str = "grant_revoked";
pub(super) const GRANT_EXPIRED: &str = "grant_expired";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ResolveParams {
    proposal_id: String,
    decision: Decision,
    /// Lowercase hex digest of the proposal the caller saw. A mismatch refuses the call,
    /// so an approval binds the exact parameters shown, not a later edit of them.
    digest: String,
    idempotency_key: String,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Decision {
    Approve,
    Reject,
}

/// Grant ids whose read fails on purpose, so a test can drive the read-error path.
#[cfg(test)]
pub(super) static FORCED_GRANT_READ_ERRORS: std::sync::Mutex<std::collections::BTreeSet<String>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

/// Why one step of an execution failed: the typed workstream store error, or the
/// `RpcError` the matching task method would have answered with.
#[derive(Debug)]
pub(super) enum ActionError {
    Store(WorkstreamStoreError),
    Rpc(RpcError),
    /// A write failed that the next start can repeat. The proposal stays `approved`.
    Retry,
}

impl ActionError {
    /// The stable code a failed execution records.
    pub(super) fn code(&self) -> &'static str {
        match self {
            Self::Store(error) => error_code(error),
            Self::Rpc(error) => rpc_error_code(error),
            Self::Retry => "internal",
        }
    }
}

impl From<WorkstreamStoreError> for ActionError {
    fn from(error: WorkstreamStoreError) -> Self {
        Self::Store(error)
    }
}

impl From<RpcError> for ActionError {
    fn from(error: RpcError) -> Self {
        Self::Rpc(error)
    }
}

impl From<StoreError> for ActionError {
    fn from(error: StoreError) -> Self {
        Self::Rpc(map_store_error(error))
    }
}

/// The change an approved proposal stands for. Each variant holds the request the
/// matching public handler would take, keyed by the action.
enum Execution {
    Create(CreateParams),
    Update(UpdateParams),
    AddObjective(AddObjectiveParams),
    Revise(ReviseObjectiveParams),
    Propose(Box<ProposeExec>),
    Cancel(CancelParams),
    Retry(RetryParams),
    Link(LinkTaskParams),
    ReviseTask(Box<ReviseTaskExec>),
    RemoveProject(RemoveParams),
    HoldDecision(HoldParams),
}

/// A `task.propose`: the `tasks.submit` request it stands for, and the workstream
/// the new task joins, if any.
struct ProposeExec {
    submit: TaskSubmitParams,
    link: Option<LinkTarget>,
    /// The finished scout this task promotes to a task that ships. Only the lineage link
    /// carries it: the objective is the text the proposal wrote, never the scout's report.
    promote_from: Option<String>,
}

struct LinkTarget {
    workstream_id: String,
    objective_id: Option<String>,
}

/// A `task.revise`: the task to stop, the instruction added to its objective, and the
/// new task that carries both. The new task is a `task.propose` in every respect but its
/// source: the daemon builds its contract from the old one and the fixed chat envelope.
struct ReviseTaskExec {
    old_task_id: String,
    added_intent: String,
    task: ProposeExec,
}

/// A `project.remove`: the project to archive. The archive hides it and keeps every row.
#[derive(Serialize)]
struct RemoveParams {
    project_id: String,
}

/// A `decision.hold`. The key seeds the hold id, so a rerun finds the hold it made.
#[derive(Serialize)]
struct HoldParams {
    task_id: String,
    until: String,
    idempotency_key: String,
}

impl Execution {
    /// The method the execution key is claimed under.
    fn method(&self) -> &'static str {
        match self {
            Self::Create(_) => "workstreams.create",
            Self::Update(_) => "workstreams.update",
            Self::AddObjective(_) => "workstreams.add_objective",
            Self::Revise(_) => "workstreams.revise_objective",
            Self::Propose(_) => "tasks.submit",
            Self::Cancel(_) => "tasks.cancel",
            Self::Retry(_) => "tasks.retry",
            Self::Link(_) => "workstreams.link_task",
            Self::ReviseTask(_) => "tasks.revise",
            Self::RemoveProject(_) => "projects.remove",
            Self::HoldDecision(_) => "holds.set",
        }
    }

    fn params(&self) -> Result<Value, RpcError> {
        let value = match self {
            Self::Create(request) => serde_json::to_value(request),
            Self::Update(request) => serde_json::to_value(request),
            Self::AddObjective(request) => serde_json::to_value(request),
            Self::Revise(request) => serde_json::to_value(request),
            Self::Propose(exec) => serde_json::to_value(&exec.submit),
            Self::Cancel(request) => serde_json::to_value(request),
            Self::Retry(request) => serde_json::to_value(request),
            Self::Link(request) => serde_json::to_value(request),
            Self::ReviseTask(exec) => {
                let submit =
                    serde_json::to_value(&exec.task.submit).map_err(|_| RpcError::Internal)?;
                Ok(json!({
                    "task_id": exec.old_task_id,
                    "added_intent": exec.added_intent,
                    "submit": submit,
                }))
            }
            Self::RemoveProject(request) => serde_json::to_value(request),
            Self::HoldDecision(request) => serde_json::to_value(request),
        };
        value.map_err(|_| RpcError::Internal)
    }

    /// Makes the change and returns the id of the record it names as its result.
    fn run(&self, api: &DaemonApi, actor: &str) -> Result<String, ActionError> {
        match self {
            Self::Create(request) => api
                .create_inner(request, actor)
                .map(|workstream| workstream.id)
                .map_err(ActionError::from),
            Self::Update(request) => api
                .update_inner(request, actor)
                .map(|workstream| workstream.id)
                .map_err(ActionError::from),
            Self::AddObjective(request) => api
                .add_objective_inner(request, actor)
                .map(|objective| objective.id)
                .map_err(ActionError::from),
            Self::Revise(request) => api
                .revise_objective_inner(request, actor)
                .map(|objective| objective.id)
                .map_err(ActionError::from),
            Self::Propose(exec) => exec.run(api),
            Self::Cancel(request) => {
                api.cancel_task_inner(&request.task_id)?;
                Ok(request.task_id.clone())
            }
            Self::Retry(request) => {
                api.retry_task_inner(&request.task_id)?;
                Ok(request.task_id.clone())
            }
            Self::Link(request) => api
                .link_task_inner(request)
                .map(|link| link.task_id)
                .map_err(ActionError::from),
            Self::ReviseTask(exec) => exec.run(api),
            Self::RemoveProject(request) => {
                match api.store.archive_project(&request.project_id) {
                    Ok(_) => Ok(project_code(&request.project_id)),
                    Err(error) => Err(match ArchiveError::of(&error) {
                        // The project has live work. The proposal ends failed, never retried.
                        Some(_) => RpcError::Conflict.into(),
                        None => error.into(),
                    }),
                }
            }
            Self::HoldDecision(request) => api
                .set_hold_inner(
                    HoldSubject::Approval,
                    &request.task_id,
                    &request.until,
                    &hold_id(&request.idempotency_key),
                )
                .map(|hold| format!("hold:{}", hold.id))
                .map_err(ActionError::from),
        }
    }

    /// The id of a change already committed, found from the record it
    /// would have stored. Used only when a stopped process left its claim open.
    fn effect_exists(&self, api: &DaemonApi, actor: &str) -> Result<Option<String>, ActionError> {
        let found = match self {
            Self::Create(request) => api
                .store
                .get_workstream(&idempotency::stable_id(
                    "workstream",
                    &request.idempotency_key,
                ))
                .map(|workstream| workstream.id),
            Self::AddObjective(request) => api
                .store
                .get_objective(&idempotency::stable_id(
                    "objective",
                    &request.idempotency_key,
                ))
                .map(|objective| objective.id),
            Self::Update(request) => {
                return revised_by(
                    api,
                    "workstream",
                    &request.workstream_id,
                    request.expected_revision,
                    actor,
                )
                .map_err(ActionError::from)
            }
            Self::Revise(request) => {
                return revised_by(
                    api,
                    "objective",
                    &request.objective_id,
                    request.expected_revision,
                    actor,
                )
                .map_err(ActionError::from)
            }
            Self::Propose(exec) => return exec.effect_exists(api),
            Self::Cancel(request) => return cancel_effect(api, &request.task_id),
            Self::Retry(request) => return retry_effect(api, &request.task_id),
            Self::Link(request) => return link_effect(api, request),
            Self::ReviseTask(exec) => return exec.effect_exists(api),
            Self::RemoveProject(request) => {
                return match api.store.project_is_archived(&request.project_id) {
                    Ok(true) => Ok(Some(project_code(&request.project_id))),
                    Ok(false) | Err(StoreError::RecordNotFound { .. }) => Ok(None),
                    Err(error) => Err(error.into()),
                }
            }
            Self::HoldDecision(request) => {
                let id = hold_id(&request.idempotency_key);
                return Ok(api
                    .store
                    .decision_hold(&id)?
                    .map(|hold| format!("hold:{}", hold.id)));
            }
        };
        match found {
            Ok(id) => Ok(Some(id)),
            Err(WorkstreamStoreError::NotFound { .. }) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

impl ProposeExec {
    /// The task this proposal makes. It derives from the execution key, so a rerun
    /// after a stop finds the task the first run made.
    fn task_id(&self) -> String {
        idempotency::stable_id("task", &self.submit.idempotency_key)
    }

    /// Makes the task, admits it, links it and wakes the executor. Every step is safe
    /// to repeat, so a rerun after a stop only does what is still missing.
    fn run(&self, api: &DaemonApi) -> Result<String, ActionError> {
        let task_id = self.task_id();
        match find_task(api, &task_id)? {
            // A stopped run created the task and never admitted it.
            Some(task) if task.state == TaskState::Created => {
                api.store.transition(&task_id, TaskEvent::Admit)?;
            }
            Some(_) => {}
            None => {
                self.preflight(api)?;
                api.submit_task_inner(&self.submit, &self.submit.idempotency_key, None)?;
            }
        }
        // Written once, right after the task exists. A rerun finds the same link and
        // changes nothing.
        if let Some(source) = &self.promote_from {
            api.store
                .add_task_lineage(&task_id, source, LineageRelation::PromotedFrom, None)
                .map_err(lineage_error)?;
        }
        if let Some(link) = &self.link {
            api.link_task_inner(&LinkTaskParams {
                workstream_id: link.workstream_id.clone(),
                task_id: task_id.clone(),
                objective_id: link.objective_id.clone(),
                idempotency_key: self.submit.idempotency_key.clone(),
            })?;
        }
        api.wake_executor();
        Ok(task_id)
    }

    /// Whether the task exists, which means a stopped run got that far. The steps it
    /// did not reach run now, so the proposal ends with the task admitted and linked.
    fn effect_exists(&self, api: &DaemonApi) -> Result<Option<String>, ActionError> {
        match find_task(api, &self.task_id())? {
            Some(_) => self.run(api).map(Some),
            None => Ok(None),
        }
    }

    /// Refuses a link the store would refuse before the task exists, so no task is
    /// made that cannot be linked. Every workstream writer takes the gate the caller
    /// holds, so the answer cannot change before the link is made.
    fn preflight(&self, api: &DaemonApi) -> Result<(), ActionError> {
        let Some(link) = &self.link else {
            return Ok(());
        };
        if api.store.get_workstream(&link.workstream_id)?.state == WorkstreamState::Archived {
            return Err(WorkstreamStoreError::Rule(WorkstreamRuleError::WorkstreamArchived).into());
        }
        if let Some(objective_id) = &link.objective_id {
            let objective = api.store.get_objective(objective_id)?;
            if objective.workstream_id != link.workstream_id {
                return Err(WorkstreamStoreError::ObjectiveMismatch {
                    objective_id: objective_id.clone(),
                    workstream_id: link.workstream_id.clone(),
                }
                .into());
            }
        }
        Ok(())
    }
}

impl ReviseTaskExec {
    /// Admits the new task, then asks the old one to stop, then links the two. Every step
    /// is safe to repeat and the new task id derives from the execution key, so a rerun
    /// after a stop only does what is still missing and never makes a second task.
    fn run(&self, api: &DaemonApi) -> Result<String, ActionError> {
        if find_task(api, &self.task.task_id())?.is_none() {
            self.preflight(api)?;
        }
        let new_task_id = self.task.run(api)?;
        if let Err(error) = self.stop_old(api) {
            // The proposal fails, so the new task must not be left queued beside a running
            // old one. A cancel that fails too leaves the proposal approved to try again.
            return match api.cancel_task_inner(&new_task_id) {
                Ok(_) => Err(error),
                Err(_) => Err(ActionError::Retry),
            };
        }
        api.store
            .add_task_lineage(
                &new_task_id,
                &self.old_task_id,
                LineageRelation::RevisedFrom,
                Some(&self.added_intent),
            )
            .map_err(lineage_write_error)?;
        Ok(new_task_id)
    }

    /// Whether the new task exists, which means a stopped run got that far. The steps it
    /// did not reach run now.
    fn effect_exists(&self, api: &DaemonApi) -> Result<Option<String>, ActionError> {
        match find_task(api, &self.task.task_id())? {
            Some(_) => self.run(api).map(Some),
            None => Ok(None),
        }
    }

    /// Refuses a revise that could not stop the old task, before anything is made. Only a
    /// queued task, or a running one whose owner is alive, can be asked to stop. A finished
    /// task is not revised: `task.retry` and `task.propose` start new work.
    fn preflight(&self, api: &DaemonApi) -> Result<(), ActionError> {
        let old = find_task(api, &self.old_task_id)?.ok_or(RpcError::NotFound)?;
        match old.state {
            TaskState::Queued => {}
            TaskState::Running => {
                api.require_live_cancellation_owner(&self.old_task_id)?;
                // No owner at all passes the check above and leaves nothing to take the request.
                let owned = api
                    .store
                    .open_cancellation_owners()?
                    .iter()
                    .any(|owner| owner.task_id == self.old_task_id);
                if !owned {
                    return Err(RpcError::Conflict.into());
                }
            }
            _ => return Err(RpcError::Conflict.into()),
        }
        if api.store.project_is_archived(&old.contract.project_id)? {
            return Err(RpcError::Conflict.into());
        }
        Ok(())
    }

    /// Asks the old task to stop, through the path `task.cancel` uses. Already asked, or
    /// already finished while the new task was made, counts as done. The old worktree and
    /// branch are never touched from here.
    fn stop_old(&self, api: &DaemonApi) -> Result<(), ActionError> {
        if cancel_effect(api, &self.old_task_id)?.is_some() {
            return Ok(());
        }
        let old = find_task(api, &self.old_task_id)?.ok_or(RpcError::NotFound)?;
        if matches!(old.state, TaskState::Queued | TaskState::Running) {
            api.cancel_task_inner(&self.old_task_id)?;
        }
        Ok(())
    }
}

/// The refusal a promotion meets, as the sink and the approval check it: a `task.propose`
/// with `promote_from` whose source is not a finished scout of the same project with a
/// relayed report is a contract denial. Any other proposal, and a promotion that passes, is
/// not refused. An unreadable store refuses, so nothing passes on a guess.
pub(super) fn promotion_denial(
    store: &SqliteStore,
    propose: &ProposeTask,
) -> Option<orchestrator_core::task_envelope::Denial> {
    let source = propose.promote_from.as_deref()?;
    let scout_project = promotable_scout(store, source).ok().flatten();
    let same_project = scout_project
        .zip(super::projects_defaults::project_id_for_code(
            store,
            &propose.project,
        ))
        .is_some_and(|(scout, own)| scout == own);
    (!same_project || propose.task_kind != TaskKind::Implement)
        .then_some(orchestrator_core::task_envelope::Denial::Contract)
}

/// The project of `task_id` when it is a scout that succeeded and whose report the user has
/// had relayed: the one condition a promotion asks of its source. `None` for a task that is
/// gone, is not a scout, has not succeeded, or has a report nobody relayed.
fn promotable_scout(store: &SqliteStore, task_id: &str) -> Result<Option<String>, StoreError> {
    let task = match store.get(task_id) {
        Ok(task) => task,
        Err(StoreError::NotFound(_)) => return Ok(None),
        Err(error) => return Err(error),
    };
    if task.contract.kind != TaskKind::Investigate || task.state != TaskState::Succeeded {
        return Ok(None);
    }
    let relayed = store
        .task_report(task_id)?
        .is_some_and(|report| report.relayed_at.is_some());
    Ok(relayed.then_some(task.contract.project_id))
}

/// The id of the hold an execution under `key` makes.
fn hold_id(key: &str) -> String {
    idempotency::stable_id("hold", key)
}

/// A lineage refusal is a conflict, whatever its cause. Any other store failure is internal.
fn lineage_error(error: StoreError) -> ActionError {
    match LineageError::of(&error) {
        Some(_) => RpcError::Conflict.into(),
        None => error.into(),
    }
}

/// The objective of the task a `task.revise` makes: the old one, a fixed heading and the
/// added instruction.
pub(super) fn revised_objective(old: &str, added_intent: &str) -> String {
    format!("{old}\n\nAdded instruction from the user:\n{added_intent}")
}

/// A refused lineage link is a conflict. Any other failure to write the link of a revise
/// is retried: the old task is already stopped, and a rerun finds the new task by its id
/// and writes the same link.
pub(super) fn lineage_write_error(error: StoreError) -> ActionError {
    match LineageError::of(&error) {
        Some(_) => RpcError::Conflict.into(),
        None => ActionError::Retry,
    }
}

/// The task, or `None` when no such task exists.
fn find_task(api: &DaemonApi, task_id: &str) -> Result<Option<TaskRecord>, ActionError> {
    match api.store.get(task_id) {
        Ok(task) => Ok(Some(task)),
        Err(StoreError::NotFound(_)) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// A cancel's effect is in place when the task is cancelled, or when its owner has a
/// cancellation request open. The task itself stops later, when the owner confirms.
fn cancel_effect(api: &DaemonApi, task_id: &str) -> Result<Option<String>, ActionError> {
    let Some(task) = find_task(api, task_id)? else {
        return Ok(None);
    };
    let requested = match task.state {
        TaskState::Cancelled => true,
        TaskState::Running => matches!(
            api.store.cancellation_status_for_task(task_id)?,
            Some(CancellationStatus::Pending | CancellationStatus::Confirmed)
        ),
        _ => false,
    };
    Ok(requested.then(|| task_id.to_owned()))
}

/// A retry's effect is in place when the task left `Failed` or `VerificationFailed`
/// and its journal records a retry. Only a retry leaves either state, but a task that
/// never failed has no retry to find, so it is rerun and refused.
fn retry_effect(api: &DaemonApi, task_id: &str) -> Result<Option<String>, ActionError> {
    let Some(task) = find_task(api, task_id)? else {
        return Ok(None);
    };
    let retried = match task.state {
        TaskState::Failed | TaskState::VerificationFailed => false,
        _ => task.events.contains(&AuditEvent::RetryScheduled),
    };
    Ok(retried.then(|| task_id.to_owned()))
}

/// A link's effect is in place when the task already serves the requested target.
fn link_effect(api: &DaemonApi, request: &LinkTaskParams) -> Result<Option<String>, ActionError> {
    let link = api.store.workstream_task_link(&request.task_id)?;
    Ok(link
        .filter(|link| {
            link.workstream_id == request.workstream_id && link.objective_id == request.objective_id
        })
        .map(|link| link.task_id))
}

impl DaemonApi {
    /// The refusal a pending promotion meets on approve, against the scout as it is now.
    /// `None` for any other proposal. Local reads only.
    pub(super) fn recheck_promotion(
        &self,
        proposal: &ActionProposal,
    ) -> Result<Option<orchestrator_core::task_envelope::Denial>, RpcError> {
        Ok(
            match serde_json::from_str::<Action>(&proposal.params_json) {
                Ok(Action::TaskPropose(propose)) => promotion_denial(&self.store, &propose),
                _ => None,
            },
        )
    }

    /// `actions.resolve`. Any session that negotiated `orchestrator_actions` may
    /// call it; `resolve_as` decides whether the caller may approve.
    pub(super) fn actions_resolve(&self, params: Value) -> Result<Value, RpcError> {
        self.resolve_as(&self.approval_actor, params)
    }

    /// Approves or rejects one proposal and, when approved, runs it. The gate is
    /// held throughout, so no clear or workstream write lands between the two.
    pub(super) fn resolve_as(&self, actor: &Actor, params: Value) -> Result<Value, RpcError> {
        let request: ResolveParams = parse_params(params)?;
        if !self.approval_allowlist.allows(actor) {
            return Err(RpcError::Forbidden);
        }
        let held = self
            .idempotency_gate
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        idempotency::run_locked(
            &held,
            &self.store,
            "actions.resolve",
            &request.idempotency_key,
            &request,
            || {
                let current = self.proposal(&request.proposal_id)?;
                if current.digest != request.digest {
                    return Err(RpcError::ProposalInvalid);
                }
                // Defense in depth: only a Restricted turn may carry proposals (DPES-1105).
                let turn = self
                    .store
                    .conversation_turn(&current.turn_id)
                    .map_err(|error| internal("actions", &format!("{error:?}")))?;
                if turn.and_then(|turn| turn.permission_level) != Some(PermissionLevel::Restricted)
                {
                    return Err(RpcError::ProposalInvalid);
                }
                // Defense in depth and a changed setting: an approval of a pending task
                // proposal re-checks the envelope as it stands now (the project's current
                // defaults and `chat_pr_delivery`). A refusal moves the card to `failed` with
                // `action_denied` in one transaction and approves nothing, so lowering a
                // setting between propose and approve denies the card. A reject is never
                // re-checked.
                if matches!(request.decision, Decision::Approve) {
                    let denial = match self.recheck_task_denial(&current)? {
                        Some(denial) => Some(denial),
                        None => match self.recheck_promotion(&current)? {
                            Some(denial) => Some(denial),
                            // A merge card re-reads the stored pull request: a head that moved, a
                            // check that went red or a project that went local-only denies it.
                            None => self.recheck_merge(&current)?,
                        },
                    };
                    if let Some(denial) = denial {
                        let denied = self
                            .store
                            .deny_action_proposal(&request.proposal_id, denial.as_str(), APPROVER)
                            .map_err(map_resolve_error)?;
                        return Ok(json!({"proposal_id": denied.id, "denied": denial.as_str()}));
                    }
                }
                let decided = match request.decision {
                    Decision::Approve => self
                        .store
                        .approve_action_proposal(&request.proposal_id, APPROVER),
                    Decision::Reject => self
                        .store
                        .reject_action_proposal(&request.proposal_id, APPROVER),
                };
                let proposal = decided.map_err(map_resolve_error)?;
                Ok(json!({"proposal_id": proposal.id}))
            },
        )?;
        // A replayed key finds the proposal already approved, so this also finishes
        // an execution whose first attempt failed after the approval committed.
        let proposal = self.proposal(&request.proposal_id)?;
        if proposal.state == ProposalState::Approved {
            self.execute_locked(&held, &proposal)?;
        }
        let proposal = self.proposal(&request.proposal_id)?;
        Ok(json!({"proposal": self.proposal_wire(&proposal)}))
    }

    /// `actions.pending`: the pending proposals of every conversation, newest first, so
    /// the app can count and list them without opening each conversation. At most one
    /// page and one byte budget of them; `more` says whether any were left out.
    pub(super) fn actions_pending(&self, params: Value) -> Result<Value, RpcError> {
        require_empty_params(&params)?;
        // One row past the page tells whether a pending proposal is left out.
        let proposals = self
            .store
            .pending_action_proposals(PROPOSAL_PAGE + 1)
            .map_err(map_store_error)?;
        let mut more = proposals.len() > PROPOSAL_PAGE;
        let mut bytes = 0;
        let mut wire = Vec::new();
        for proposal in proposals.iter().take(PROPOSAL_PAGE) {
            let mut value = self.proposal_wire(proposal);
            if let Value::Object(fields) = &mut value {
                fields.insert("scope".to_owned(), scope_wire(proposal));
            }
            let size = value.to_string().len();
            if bytes + size > PROPOSAL_BYTES {
                more = true;
                break;
            }
            bytes += size;
            wire.push(value);
        }
        Ok(json!({"proposals": wire, "more": more}))
    }

    /// Finishes every approved proposal before the daemon serves a request. Runs
    /// once per start, with the data directory lock held by the caller. A proposal
    /// that fails stays `approved` and is retried by the next start; the failure is
    /// logged and never stops the daemon from starting.
    pub(super) fn reconcile_actions(&self) {
        let held = self
            .idempotency_gate
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let approved = match self.store.approved_action_proposals() {
            Ok(approved) => approved,
            Err(error) => {
                self.config.diagnostics.error(
                    "api",
                    "action_reconcile_failed",
                    &[("cause", &error.to_string())],
                );
                return;
            }
        };
        for proposal in approved {
            if let Err(error) = self.execute_locked(&held, &proposal) {
                self.config.diagnostics.error(
                    "api",
                    "action_reconcile_failed",
                    &[("cause", &format!("{error:?}"))],
                );
            }
        }
    }

    /// Proposals of one conversation for `conversations.get`. Every pending proposal
    /// is listed. Settled ones (executed, failed, rejected, superseded) are the
    /// newest that fit the count and byte limits, and `more` says whether any settled
    /// proposal was left out.
    pub(super) fn proposals_page(
        &self,
        target: &ConversationTarget,
    ) -> Result<(Vec<Value>, bool), RpcError> {
        let proposals = self
            .store
            .action_proposals(target, usize::MAX)
            .map_err(map_store_error)?;
        let mut more = false;
        let mut settled = 0;
        let mut bytes = 0;
        let mut wire = Vec::new();
        for proposal in &proposals {
            if proposal.state == ProposalState::Pending {
                wire.push(self.proposal_wire(proposal));
                continue;
            }
            // The count limit is known before the wire is built, which reads the store.
            if settled == PROPOSAL_PAGE {
                more = true;
                continue;
            }
            let value = self.proposal_wire(proposal);
            let size = value.to_string().len();
            if bytes + size > PROPOSAL_BYTES {
                more = true;
                continue;
            }
            settled += 1;
            bytes += size;
            wire.push(value);
        }
        Ok((wire, more))
    }

    fn proposal(&self, id: &str) -> Result<ActionProposal, RpcError> {
        self.store
            .action_proposal(id)
            .map_err(map_store_error)?
            .ok_or(RpcError::ProposalNotFound)
    }

    /// Runs one approved proposal once and records its outcome. Called with the
    /// gate held, so the proposal cannot change state underneath it.
    fn execute_locked(
        &self,
        held: &MutexGuard<'_, ()>,
        proposal: &ActionProposal,
    ) -> Result<(), RpcError> {
        self.execute_locked_at(held, proposal, crate::executor::unix_now_ms())
    }

    /// [`Self::execute_locked`] at the clock reading `now_ms`. The event actor is whoever
    /// approved the proposal: `user`, or `grant:<id>`. A proposal a grant approved first
    /// checks the grant: revoked or expired, it does not start and ends `failed`. The use
    /// was spent when the grant approved, and the token budget blocks new uses only, so
    /// neither stops a use that already exists.
    pub(super) fn execute_locked_at(
        &self,
        held: &MutexGuard<'_, ()>,
        proposal: &ActionProposal,
        now_ms: u64,
    ) -> Result<(), RpcError> {
        // A merge is not run here: it calls the host, which must never happen under the gate.
        // The proposal stays `approved` and the landing thread runs it (DPES-1105 phase 4c).
        if proposal.kind == orchestrator_core::orchestrator_action::ActionKind::DeliveryMerge {
            self.poke_landing();
            return Ok(());
        }
        let approver = self
            .store
            .proposal_approver(&proposal.id)
            .map_err(map_store_error)?
            .unwrap_or_else(|| APPROVER.to_owned());
        let grant_id = approver.strip_prefix("grant:");
        let stop = match grant_id {
            Some(id) => self.grant_stop_code(id, now_ms)?,
            None => None,
        };
        if let Some(code) = stop {
            if let Some(id) = grant_id {
                self.record_grant_outcome(id, proposal, GrantOutcome::Stopped { code }, now_ms)?;
            }
            let failed = ActionOutcome::Failed { error_code: code };
            self.store
                .record_action_outcome(&proposal.id, failed, &approver)
                .map_err(map_resolve_error)?;
            return Ok(());
        }
        let parent = match &proposal.depends_on {
            Some(parent) => self
                .store
                .action_proposal(parent)
                .map_err(map_store_error)?,
            None => None,
        };
        let key = idempotency::stable_id("action", &proposal.id);
        let actor = format!("action:{}", proposal.id);
        let stored = match self.checked_execution(proposal, parent.as_ref(), &key) {
            Ok(execution) => self.run_once(held, &execution, &key, &actor)?,
            Err(code) => json!({"error_code": code}),
        };
        let outcome = match (stored["result_ref"].as_str(), stored["error_code"].as_str()) {
            (Some(result_ref), _) => ActionOutcome::Executed { result_ref },
            (None, Some(error_code)) => ActionOutcome::Failed { error_code },
            (None, None) => return Err(RpcError::Internal),
        };
        // The grant's audit row first: a stop between the two writes leaves the proposal
        // approved, and the rerun finds the key's outcome and closes it.
        if let Some(id) = grant_id {
            let closed = match outcome {
                ActionOutcome::Executed { result_ref } => GrantOutcome::Executed {
                    task_id: matches!(proposal.kind.as_str(), "task.propose" | "task.retry")
                        .then_some(result_ref),
                },
                ActionOutcome::Failed { error_code } => GrantOutcome::Stopped { code: error_code },
            };
            self.record_grant_outcome(id, proposal, closed, now_ms)?;
        }
        self.store
            .record_action_outcome(&proposal.id, outcome, &approver)
            .map_err(map_resolve_error)?;
        Ok(())
    }

    /// Why a grant no longer lets an approved proposal start, or `None` while it does. A grant
    /// that cannot be read is an error, not a revoke: the proposal stays `approved` and the
    /// next reconcile pass checks again.
    pub(super) fn grant_stop_code(
        &self,
        grant_id: &str,
        now_ms: u64,
    ) -> Result<Option<&'static str>, RpcError> {
        #[cfg(test)]
        {
            let forced = FORCED_GRANT_READ_ERRORS
                .lock()
                .is_ok_and(|forced| forced.contains(grant_id));
            if forced {
                return Err(RpcError::Internal);
            }
        }
        match self.store.standing_grant(grant_id) {
            Ok(Some(grant)) if grant.revoked_at_ms.is_some() => Ok(Some(GRANT_REVOKED)),
            Ok(Some(grant)) if now_ms >= grant.expires_at_ms => Ok(Some(GRANT_EXPIRED)),
            Ok(Some(_)) => Ok(None),
            // A grant row that does not exist fails closed, as a revoked one.
            Ok(None) => Ok(Some(GRANT_REVOKED)),
            Err(error) => Err(internal("grants", &error)),
        }
    }

    pub(super) fn record_grant_outcome(
        &self,
        grant_id: &str,
        proposal: &ActionProposal,
        outcome: GrantOutcome<'_>,
        now_ms: u64,
    ) -> Result<(), RpcError> {
        self.store
            .record_grant_outcome(grant_id, &proposal.id, &proposal.digest, outcome, now_ms)
            .map_err(|error| internal("grants", &error))
    }

    /// Builds the execution, then refuses one that reaches outside the workstream
    /// the proposal's conversation is scoped to. The block was checked at parse
    /// time; the store state may have changed since, so it is checked again here.
    fn checked_execution(
        &self,
        proposal: &ActionProposal,
        parent: Option<&ActionProposal>,
        key: &str,
    ) -> Result<Execution, &'static str> {
        let execution = self.build_execution(proposal, parent, key)?;
        match self.within_scope(proposal, &execution) {
            Ok(true) => Ok(execution),
            Ok(false) => Err(INVALID),
            Err(error) => Err(error.code()),
        }
    }

    fn within_scope(
        &self,
        proposal: &ActionProposal,
        execution: &Execution,
    ) -> Result<bool, ActionError> {
        let Some(workstream) = proposal.scope_workstream() else {
            return Ok(true);
        };
        Ok(match execution {
            Execution::Create(_) => false,
            Execution::Update(request) => request.workstream_id == workstream,
            Execution::AddObjective(request) => request.workstream_id == workstream,
            Execution::Revise(request) => {
                self.store
                    .get_objective(&request.objective_id)?
                    .workstream_id
                    == workstream
            }
            // A task proposed in a workstream conversation always joins that workstream.
            Execution::Propose(exec) => exec
                .link
                .as_ref()
                .is_some_and(|link| link.workstream_id == workstream),
            Execution::Cancel(request) => self.task_in_workstream(&request.task_id, workstream)?,
            Execution::Retry(request) => self.task_in_workstream(&request.task_id, workstream)?,
            Execution::Link(request) => request.workstream_id == workstream,
            Execution::ReviseTask(exec) => {
                self.task_in_workstream(&exec.old_task_id, workstream)?
            }
            Execution::HoldDecision(request) => {
                self.task_in_workstream(&request.task_id, workstream)?
            }
            // Removing a project reaches outside any one workstream.
            Execution::RemoveProject(_) => false,
        })
    }

    /// Whether the task is linked to the workstream. An unlinked task is not.
    fn task_in_workstream(&self, task_id: &str, workstream: &str) -> Result<bool, ActionError> {
        let link = self.store.workstream_task_link(task_id)?;
        Ok(link.is_some_and(|link| link.workstream_id == workstream))
    }

    /// Runs the execution under its own key. A replay returns the stored outcome,
    /// so nothing runs twice. An open claim means a stopped process was mid-run;
    /// that only happens at startup, where the effect is checked before rerunning.
    fn run_once(
        &self,
        held: &MutexGuard<'_, ()>,
        execution: &Execution,
        key: &str,
        actor: &str,
    ) -> Result<Value, RpcError> {
        if let Execution::ReviseTask(exec) = execution {
            // Every step of a revise is safe to repeat, so it runs outside the idempotency
            // record, which would keep a retryable failure for good. The proposal outcome
            // is the record: only an approved proposal runs.
            return match exec.run(self) {
                Ok(result_ref) => Ok(json!({"result_ref": result_ref})),
                Err(ActionError::Retry) => Err(RpcError::Internal),
                Err(error) => Ok(json!({"error_code": error.code()})),
            };
        }
        let method = execution.method();
        let params = execution.params()?;
        match idempotency::run_locked(held, &self.store, method, key, &params, || {
            Ok(self.outcome(execution, actor))
        }) {
            Err(RpcError::IdempotencyInProgress) => {
                // A failed read proves nothing either way, and the write may already
                // have committed, so the claim stays open and the next start retries.
                let found = execution
                    .effect_exists(self, actor)
                    .map_err(|error| internal("actions", &format!("{error:?}")))?;
                let reply = match found {
                    Some(result_ref) => json!({"result_ref": result_ref}),
                    None => self.outcome(execution, actor),
                };
                idempotency::complete_interrupted_locked(
                    held,
                    &self.store,
                    method,
                    key,
                    &params,
                    Ok(reply),
                )
            }
            other => other,
        }
    }

    /// What one run stores under its key: the record it made, or the code it failed with.
    fn outcome(&self, execution: &Execution, actor: &str) -> Value {
        match execution.run(self, actor) {
            Ok(result_ref) => json!({"result_ref": result_ref}),
            Err(error) => json!({"error_code": error.code()}),
        }
    }

    /// Turns one proposal into the change it names. `parent` is the proposal an
    /// `objective.add` or `task.propose` by ref joins, which has executed by the time
    /// this runs.
    fn build_execution(
        &self,
        proposal: &ActionProposal,
        parent: Option<&ActionProposal>,
        key: &str,
    ) -> Result<Execution, &'static str> {
        let action: Action = serde_json::from_str(&proposal.params_json).map_err(|_| INVALID)?;
        let key = key.to_owned();
        Ok(match action {
            Action::WorkstreamCreate(create) => Execution::Create(CreateParams {
                title: create.title,
                summary: create.summary,
                idempotency_key: key,
            }),
            Action::WorkstreamUpdate(update) => {
                if update.title.is_none() && update.summary.is_none() && update.state.is_none() {
                    return Err(INVALID);
                }
                Execution::Update(UpdateParams {
                    workstream_id: update.workstream_id,
                    expected_revision: update.expected_revision,
                    title: update.title,
                    summary: update.summary,
                    state: update.state,
                    idempotency_key: key,
                })
            }
            Action::ObjectiveAdd(add) => {
                let workstream_id = match (add.workstream_id, parent) {
                    (Some(workstream_id), _) => workstream_id,
                    (None, Some(parent)) => parent.result_ref.clone().ok_or(INVALID)?,
                    (None, None) => return Err(INVALID),
                };
                Execution::AddObjective(AddObjectiveParams {
                    workstream_id,
                    title: add.title,
                    description: add.description,
                    gates: gate_params(add.gates),
                    idempotency_key: key,
                })
            }
            Action::ObjectiveRevise(revise) => {
                if revise.title.is_none()
                    && revise.description.is_none()
                    && revise.add_gates.is_empty()
                {
                    return Err(INVALID);
                }
                Execution::Revise(ReviseObjectiveParams {
                    objective_id: revise.objective_id,
                    expected_revision: revise.expected_revision,
                    title: revise.title,
                    description: revise.description,
                    add_gates: gate_params(revise.add_gates),
                    idempotency_key: key,
                })
            }
            Action::TaskPropose(propose) => {
                Execution::Propose(Box::new(self.propose_execution(propose, parent, key)?))
            }
            Action::TaskCancel(cancel) => Execution::Cancel(CancelParams {
                task_id: cancel.task_id,
                idempotency_key: key,
            }),
            Action::TaskRetry(retry) => Execution::Retry(RetryParams {
                task_id: retry.task_id,
                idempotency_key: key,
            }),
            Action::WorkstreamLinkTask(link) => Execution::Link(LinkTaskParams {
                workstream_id: link.workstream_id,
                task_id: link.task_id,
                objective_id: link.objective_id,
                idempotency_key: key,
            }),
            Action::TaskRevise(revise) => {
                Execution::ReviseTask(Box::new(self.revise_execution(revise, key)?))
            }
            // Never built here: a merge runs on the landing thread, outside the gate.
            Action::DeliveryMerge(_) => return Err(INVALID),
            Action::ProjectRemove(remove) => Execution::RemoveProject(RemoveParams {
                project_id: self.resolve_project(&remove.project)?,
            }),
            Action::DecisionHold(hold) => Execution::HoldDecision(HoldParams {
                task_id: hold.task_id,
                until: hold.until,
                idempotency_key: key,
            }),
        })
    }

    /// The new task a `task.revise` stands for. Agent, kind, delivery and dependencies
    /// copy from the old contract; autonomy and verification come from the fixed chat
    /// envelope and no permission is set, so the new task takes the daemon default
    /// (Restricted) and never the level a hand-made task ran at. The new objective is the
    /// old one, a blank line, a fixed heading and the added instruction. It joins the old
    /// task's workstream and objective. The old task may be finished by now, so its state
    /// is not read here: `ReviseTaskExec::preflight` reads it when nothing is made yet.
    fn revise_execution(
        &self,
        revise: ReviseTask,
        key: String,
    ) -> Result<ReviseTaskExec, &'static str> {
        let old = match self.store.get(&revise.task_id) {
            Ok(old) => old,
            Err(StoreError::NotFound(_)) => return Err("not_found"),
            Err(error) => {
                internal("actions", &error);
                return Err("internal");
            }
        };
        let envelope = envelope_for(
            &self.store,
            Some(&old.contract.project_id),
            old.contract.kind,
        )
        .map_err(|error| {
            internal("actions", &error);
            "internal"
        })?;
        if !envelope.delivery_grants(old.contract.delivery) {
            return Err("action_denied");
        }
        let objective = revised_objective(&old.contract.objective, &revise.added_intent);
        if objective.len() > orchestrator_core::contract::limits::MAX_OBJECTIVE_BYTES {
            return Err(INVALID);
        }
        let link = self
            .store
            .workstream_task_link(&revise.task_id)
            .map_err(|error| ActionError::from(error).code())?
            .map(|link| LinkTarget {
                workstream_id: link.workstream_id,
                objective_id: link.objective_id,
            });
        Ok(ReviseTaskExec {
            old_task_id: revise.task_id,
            added_intent: revise.added_intent,
            task: ProposeExec {
                submit: TaskSubmitParams {
                    idempotency_key: key,
                    contract: TaskSubmission {
                        project_id: old.contract.project_id,
                        parent_id: None,
                        objective,
                        kind: old.contract.kind,
                        agent: old.contract.agent,
                        dependencies: old.contract.dependencies,
                        verification: envelope.verification,
                        delivery: old.contract.delivery,
                        autonomy: envelope.autonomy,
                        max_retries: CHAT_TASK_MAX_RETRIES,
                        max_repairs: CHAT_TASK_MAX_REPAIRS,
                        model_selection: None,
                        permission: None,
                    },
                },
                link,
                promote_from: None,
            },
        })
    }

    /// The `tasks.submit` request a `task.propose` stands for. Autonomy and verification
    /// come from the project's envelope (the fixed chat envelope unless the project has a
    /// pull-request mode), attempt limits are constants, and no permission is set, so the
    /// task takes the daemon default (Restricted). A proposal can raise none of them.
    /// `parent` is the `workstream.create` a `workstream_ref` names.
    fn propose_execution(
        &self,
        propose: ProposeTask,
        parent: Option<&ActionProposal>,
        key: String,
    ) -> Result<ProposeExec, &'static str> {
        let project = self.resolve_project(&propose.project);
        let envelope = envelope_for(&self.store, project.as_deref().ok(), propose.task_kind)
            .map_err(|error| {
                internal("actions", &error);
                "internal"
            })?;
        // Defense in depth: the block was checked against this envelope when it was stored,
        // and again when it was approved. A block stored unchecked must still not run what
        // the envelope refuses.
        if check_task(&propose, &envelope).is_err() {
            return Err(INVALID);
        }
        let agent = canonical_agent_id(&propose.agent)
            .ok_or(INVALID)?
            .to_owned();
        let project_id = project?;
        // A promotion is checked again here, at execution: the scout and its report are
        // read as they are now.
        if let Some(source) = &propose.promote_from {
            if propose.task_kind != TaskKind::Implement {
                return Err(INVALID);
            }
            match promotable_scout(&self.store, source) {
                Ok(Some(scout_project)) if scout_project == project_id => {}
                Ok(_) => return Err(INVALID),
                Err(error) => {
                    internal("actions", &error);
                    return Err("internal");
                }
            }
        }
        let workstream_id = match (propose.workstream_id, propose.workstream_ref, parent) {
            (Some(workstream_id), None, _) => Some(workstream_id),
            (None, Some(_), Some(parent)) => Some(parent.result_ref.clone().ok_or(INVALID)?),
            (None, None, _) => None,
            _ => return Err(INVALID),
        };
        let link = match (workstream_id, propose.objective_id) {
            (Some(workstream_id), objective_id) => Some(LinkTarget {
                workstream_id,
                objective_id,
            }),
            (None, None) => None,
            (None, Some(_)) => return Err(INVALID),
        };
        Ok(ProposeExec {
            submit: TaskSubmitParams {
                idempotency_key: key,
                contract: TaskSubmission {
                    project_id,
                    parent_id: None,
                    objective: propose.objective,
                    kind: propose.task_kind,
                    agent,
                    dependencies: Vec::new(),
                    verification: envelope.verification,
                    delivery: propose.delivery,
                    autonomy: envelope.autonomy,
                    max_retries: CHAT_TASK_MAX_RETRIES,
                    max_repairs: CHAT_TASK_MAX_REPAIRS,
                    model_selection: None,
                    permission: None,
                },
            },
            link,
            promote_from: propose.promote_from,
        })
    }

    /// What a `project.remove` card shows: the project's folder name, how many of its tasks
    /// stay in history and how many are still queued, running or verifying (the proposal
    /// fails closed while that is above zero). Never the path.
    fn remove_target(&self, code: &str) -> Value {
        let Ok(project_id) = self.resolve_project(code) else {
            return Value::Null;
        };
        let Ok(tasks) = self.store.list() else {
            return Value::Null;
        };
        let own = tasks
            .iter()
            .filter(|task| task.contract.project_id == project_id);
        let kept = own.clone().count();
        let active = own
            .filter(|task| {
                matches!(
                    task.state,
                    TaskState::Queued | TaskState::Running | TaskState::Verifying
                )
            })
            .count();
        json!({
            "project_name": project_name(&project_id),
            "task_count": kept,
            "running_tasks": active,
        })
    }

    /// The registered project whose code is `token`. `Err("not_found")` for none,
    /// `Err("conflict")` for two, and `Err("internal")` when the projects cannot be read.
    fn resolve_project(&self, token: &str) -> Result<String, &'static str> {
        let projects = self.store.list_projects().map_err(|error| {
            internal("actions", &error);
            "internal"
        })?;
        let mut matches = projects
            .into_iter()
            .filter(|project| project_code(&project.id) == token);
        match (matches.next(), matches.next()) {
            (Some(project), None) => Ok(project.id),
            (Some(_), Some(_)) => Err("conflict"),
            (None, _) => Err("not_found"),
        }
    }

    /// A proposal as the wire shows it. `params` is the canonical action the digest binds.
    /// `target` holds what a card needs to name the project or task, read now.
    fn proposal_wire(&self, proposal: &ActionProposal) -> Value {
        json!({
            "id": proposal.id,
            "turn_id": proposal.turn_id,
            "index": proposal.idx,
            "kind": proposal.kind.as_str(),
            "params": serde_json::from_str::<Value>(&proposal.params_json).unwrap_or(Value::Null),
            "digest": proposal.digest,
            "depends_on": proposal.depends_on,
            "state": proposal.state.as_str(),
            "result_ref": proposal.result_ref,
            "error_code": proposal.error_code,
            "denial": proposal.denial,
            "grant_id": self.proposal_grant_id(proposal),
            "target": self.proposal_target(proposal),
            // Whether the turn that made the proposal was handed a scout report.
            "after_report": self.store.turn_read_report(&proposal.turn_id).unwrap_or(false),
            "created_at": proposal.created_at,
            "updated_at": proposal.updated_at,
        })
    }

    /// The grant that approved a proposal, for a card that shows it, or null. A proposal no
    /// one approved has none. An unreadable store shows none.
    fn proposal_grant_id(&self, proposal: &ActionProposal) -> Value {
        if matches!(
            proposal.state,
            ProposalState::Pending | ProposalState::Superseded
        ) {
            return Value::Null;
        }
        self.store
            .proposal_approver(&proposal.id)
            .ok()
            .flatten()
            .and_then(|actor| actor.strip_prefix("grant:").map(str::to_owned))
            .map_or(Value::Null, Value::String)
    }

    /// What the app needs to name a task proposal's subject, read now: the project's
    /// folder name and whether the machine is busy for `task.propose`, or the task's title and state for the kinds that
    /// name a task. Null for the other kinds, and for a task that no longer exists. Never
    /// a path, and no id beyond what the proposal's parameters already hold.
    fn proposal_target(&self, proposal: &ActionProposal) -> Value {
        let Ok(action) = serde_json::from_str::<Action>(&proposal.params_json) else {
            return Value::Null;
        };
        let task_id = match &action {
            Action::TaskPropose(propose) => {
                let name = self
                    .resolve_project(&propose.project)
                    .ok()
                    .and_then(|id| project_name(&id).map(str::to_owned));
                // A promotion shows the scout it comes from, by title.
                let promoted_from_title = propose.promote_from.as_deref().and_then(|source| {
                    self.store.get(source).ok().map(|task| {
                        cut_at_char(&task.contract.objective, TARGET_TITLE_BYTES).to_owned()
                    })
                });
                // Advisory: the gate as it is now, which can change before the user decides.
                let machine_busy = self
                    .config
                    .load
                    .status(std::time::Instant::now())
                    .hold
                    .is_some();
                let mut target = json!({"project_name": name, "machine_busy": machine_busy});
                if let Some(title) = promoted_from_title {
                    target["promoted_from_title"] = json!(title);
                }
                // Only a project in a pull-request mode can be below its default.
                if let Some(mode) = self.below_project_default(propose) {
                    target["below_project_default"] = json!(true);
                    target["project_mode"] = json!(mode.as_str());
                }
                return target;
            }
            Action::ProjectRemove(remove) => return self.remove_target(&remove.project),
            Action::DeliveryMerge(merge) => return self.merge_target(merge),
            Action::TaskCancel(cancel) => &cancel.task_id,
            Action::TaskRetry(retry) => &retry.task_id,
            Action::TaskRevise(revise) => &revise.task_id,
            Action::DecisionHold(hold) => &hold.task_id,
            Action::WorkstreamLinkTask(link) => &link.task_id,
            _ => return Value::Null,
        };
        match self.store.get(task_id) {
            Ok(task) => json!({
                "task_title": cut_at_char(&task.contract.objective, TARGET_TITLE_BYTES),
                "task_state": serde_json::to_value(task.state).unwrap_or(Value::Null),
            }),
            Err(_) => Value::Null,
        }
    }
}

/// The short code a snapshot shows after a project's folder name, and a `task.propose`
/// names its project by: `p-` and the first four bytes of the SHA-256 of the project id,
/// in lowercase hex, such as `p-3fa9c1d2`. The id is the repository path, which the code
/// never reveals.
pub(super) fn project_code(project_id: &str) -> String {
    let hash = Sha256::digest(project_id.as_bytes());
    let short: String = hash[..4].iter().map(|byte| format!("{byte:02x}")).collect();
    format!("p-{short}")
}

/// The last path component of a project id, which is its folder name.
pub(super) fn project_name(project_id: &str) -> Option<&str> {
    project_id.rsplit(['/', '\\']).find(|part| !part.is_empty())
}

/// `text` cut to at most `limit` bytes, at a character boundary.
pub(super) fn cut_at_char(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// The conversation a pending proposal belongs to, as `actions.pending` lists it.
fn scope_wire(proposal: &ActionProposal) -> Value {
    match proposal.scope_workstream() {
        Some(workstream_id) => json!({"kind": "workstream", "workstream_id": workstream_id}),
        None => json!({"kind": "global"}),
    }
}

fn gate_params(descriptions: Vec<String>) -> Vec<GateParams> {
    descriptions
        .into_iter()
        .map(|description| GateParams { description })
        .collect()
}

/// Whether the revision the action would have written is stored, written by the
/// action itself. Returns the entity id when it is.
fn revised_by(
    api: &DaemonApi,
    kind: &str,
    id: &str,
    expected_revision: i64,
    actor: &str,
) -> Result<Option<String>, WorkstreamStoreError> {
    let revisions = api.store.workstream_revisions(kind, id)?;
    let found = revisions
        .iter()
        .any(|revision| revision.revision == expected_revision + 1 && revision.actor == actor);
    Ok(found.then(|| id.to_owned()))
}

/// The stable code a failed execution records. Store failures are `internal`:
/// the proposal fails rather than staying approved forever (DPES-1105).
fn error_code(error: &WorkstreamStoreError) -> &'static str {
    match error {
        WorkstreamStoreError::Stale { .. } => "stale_revision",
        WorkstreamStoreError::NotFound { .. }
        | WorkstreamStoreError::Rule(WorkstreamRuleError::UnknownGate(_)) => "not_found",
        WorkstreamStoreError::AlreadyExists { .. }
        | WorkstreamStoreError::TaskAlreadyLinked { .. }
        | WorkstreamStoreError::TaskIsGateEvidence { .. }
        | WorkstreamStoreError::Rule(
            WorkstreamRuleError::IllegalWorkstreamTransition { .. }
            | WorkstreamRuleError::IllegalGateTransition { .. }
            | WorkstreamRuleError::WorkstreamArchived,
        ) => "conflict",
        WorkstreamStoreError::Store(_) => "internal",
        _ => INVALID,
    }
}

/// The stable code for an error a task method answered with. Anything unlisted is
/// `internal`.
fn rpc_error_code(error: &RpcError) -> &'static str {
    match error {
        RpcError::InvalidParams => INVALID,
        RpcError::NotFound | RpcError::ParentNotFound => "not_found",
        RpcError::Conflict => "conflict",
        RpcError::Forbidden => "forbidden",
        _ => "internal",
    }
}

pub(super) fn map_resolve_error(error: ResolveError) -> RpcError {
    match error {
        ResolveError::NotFound => RpcError::ProposalNotFound,
        ResolveError::NotPending(_) => RpcError::ProposalNotPending,
        ResolveError::ParentNotExecuted => RpcError::ProposalInvalid,
        ResolveError::Store(error) => internal("actions", &error),
    }
}
