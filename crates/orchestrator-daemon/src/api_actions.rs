//! Action proposal resolution and execution (DPES-1105).
//!
//! `actions.resolve` is the only way a proposal leaves `pending`. Approval runs
//! the change the proposal names, a workstream write or a task action, under its
//! own idempotency key, with the idempotency gate held, so a clear or another
//! workstream write cannot land between approve and execute. Startup finishes any
//! approved proposal that a stopped process left behind.

use std::sync::MutexGuard;

use orchestrator_core::orchestrator_action::{Action, ProposeTask};
use orchestrator_core::permission::PermissionLevel;
use orchestrator_core::task::AuditEvent;
use orchestrator_core::task_envelope::{
    chat_task_envelope, check_task, CHAT_TASK_MAX_REPAIRS, CHAT_TASK_MAX_RETRIES,
};
use orchestrator_core::workstream::{WorkstreamRuleError, WorkstreamState};
use orchestrator_store::{
    ActionOutcome, ActionProposal, ConversationTarget, ProposalState, ResolveError, TaskRecord,
    WorkstreamStoreError,
};

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

/// Why one step of an execution failed: the typed workstream store error, or the
/// `RpcError` the matching task method would have answered with.
#[derive(Debug)]
enum ActionError {
    Store(WorkstreamStoreError),
    Rpc(RpcError),
}

impl ActionError {
    /// The stable code a failed execution records.
    fn code(&self) -> &'static str {
        match self {
            Self::Store(error) => error_code(error),
            Self::Rpc(error) => rpc_error_code(error),
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
}

/// A `task.propose`: the `tasks.submit` request it stands for, and the workstream
/// the new task joins, if any.
struct ProposeExec {
    submit: TaskSubmitParams,
    link: Option<LinkTarget>,
}

struct LinkTarget {
    workstream_id: String,
    objective_id: Option<String>,
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
                if turn.and_then(|turn| turn.permission_level) != Some(PermissionLevel::Restricted) {
                    return Err(RpcError::ProposalInvalid);
                }
                // The envelope is not checked again here. Once it comes from settings
                // (phase 4b), a pending `task.propose` must be checked again against it at
                // this point, and moved to `failed` with `action_denied` in the same
                // transaction as the decision. The fixed chat envelope cannot change after
                // the block was stored, so there is nothing to re-check yet.
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
        let parent = match &proposal.depends_on {
            Some(parent) => self.store.action_proposal(parent).map_err(map_store_error)?,
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
        self.store
            .record_action_outcome(&proposal.id, outcome, APPROVER)
            .map_err(map_resolve_error)?;
        Ok(())
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
        })
    }

    /// The `tasks.submit` request a `task.propose` stands for. Autonomy and verification
    /// come from the fixed chat envelope, attempt limits are constants, and no permission
    /// is set, so the task takes the daemon default (Restricted). A proposal can raise none
    /// of them. `parent` is the `workstream.create` a `workstream_ref` names.
    fn propose_execution(
        &self,
        propose: ProposeTask,
        parent: Option<&ActionProposal>,
        key: String,
    ) -> Result<ProposeExec, &'static str> {
        let envelope = chat_task_envelope();
        // Defense in depth: the block was checked against this same envelope when it was
        // stored. A block stored unchecked must still not run what the envelope refuses.
        if check_task(&propose, &envelope).is_err() {
            return Err(INVALID);
        }
        let agent = canonical_agent_id(&propose.agent)
            .ok_or(INVALID)?
            .to_owned();
        let project_id = self.resolve_project(&propose.project)?;
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
            "target": self.proposal_target(proposal),
            "created_at": proposal.created_at,
            "updated_at": proposal.updated_at,
        })
    }

    /// What the app needs to name a task proposal's subject, read now: the project's
    /// folder name for `task.propose`, or the task's title and state for the kinds that
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
                return json!({"project_name": name});
            }
            Action::TaskCancel(cancel) => &cancel.task_id,
            Action::TaskRetry(retry) => &retry.task_id,
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
fn project_name(project_id: &str) -> Option<&str> {
    project_id.rsplit(['/', '\\']).find(|part| !part.is_empty())
}

/// `text` cut to at most `limit` bytes, at a character boundary.
fn cut_at_char(text: &str, limit: usize) -> &str {
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

fn map_resolve_error(error: ResolveError) -> RpcError {
    match error {
        ResolveError::NotFound => RpcError::ProposalNotFound,
        ResolveError::NotPending(_) => RpcError::ProposalNotPending,
        ResolveError::ParentNotExecuted => RpcError::ProposalInvalid,
        ResolveError::Store(error) => internal("actions", &error),
    }
}
