//! Action proposal resolution and execution (DPES-1105).
//!
//! `actions.resolve` is the only way a proposal leaves `pending`. Approval runs
//! the workstream change the proposal names under its own idempotency key, with
//! the idempotency gate held, so a clear or another workstream write cannot land
//! between approve and execute. Startup finishes any approved proposal that a
//! stopped process left behind.

use std::sync::MutexGuard;

use orchestrator_core::orchestrator_action::Action;
use orchestrator_core::permission::PermissionLevel;
use orchestrator_core::workstream::WorkstreamRuleError;
use orchestrator_store::{
    ActionOutcome, ActionProposal, ConversationTarget, ProposalState, ResolveError,
    WorkstreamStoreError,
};

use super::workstreams::{
    AddObjectiveParams, CreateParams, GateParams, ReviseObjectiveParams, UpdateParams,
};
use super::*;

/// Event actor of every 1c transition: the approver the audit trail records.
const APPROVER: &str = "user";
/// Most proposals one `conversations.get` page lists, and the most bytes they take.
const PROPOSAL_PAGE: usize = 100;
const PROPOSAL_BYTES: usize = 256 * 1024;
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

/// The workstream change an approved proposal stands for. Each variant holds the
/// request the matching `workstreams.*` handler would take, keyed by the action.
enum Execution {
    Create(CreateParams),
    Update(UpdateParams),
    AddObjective(AddObjectiveParams),
    Revise(ReviseObjectiveParams),
}

impl Execution {
    /// The method the execution key is claimed under.
    fn method(&self) -> &'static str {
        match self {
            Self::Create(_) => "workstreams.create",
            Self::Update(_) => "workstreams.update",
            Self::AddObjective(_) => "workstreams.add_objective",
            Self::Revise(_) => "workstreams.revise_objective",
        }
    }

    fn params(&self) -> Result<Value, RpcError> {
        let value = match self {
            Self::Create(request) => serde_json::to_value(request),
            Self::Update(request) => serde_json::to_value(request),
            Self::AddObjective(request) => serde_json::to_value(request),
            Self::Revise(request) => serde_json::to_value(request),
        };
        value.map_err(|_| RpcError::Internal)
    }

    /// Makes the workstream change and returns the id of the record it stored.
    fn run(&self, api: &DaemonApi, actor: &str) -> Result<String, WorkstreamStoreError> {
        match self {
            Self::Create(request) => api
                .create_inner(request, actor)
                .map(|workstream| workstream.id),
            Self::Update(request) => api
                .update_inner(request, actor)
                .map(|workstream| workstream.id),
            Self::AddObjective(request) => api
                .add_objective_inner(request, actor)
                .map(|objective| objective.id),
            Self::Revise(request) => api
                .revise_objective_inner(request, actor)
                .map(|objective| objective.id),
        }
    }

    /// The id of the change if it already committed, found from the record it
    /// would have stored. Used only when a stopped process left its claim open.
    fn effect_exists(
        &self,
        api: &DaemonApi,
        actor: &str,
    ) -> Result<Option<String>, WorkstreamStoreError> {
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
            }
            Self::Revise(request) => {
                return revised_by(
                    api,
                    "objective",
                    &request.objective_id,
                    request.expected_revision,
                    actor,
                )
            }
        };
        match found {
            Ok(id) => Ok(Some(id)),
            Err(WorkstreamStoreError::NotFound { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }
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
        Ok(json!({"proposal": proposal_wire(&proposal)}))
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
            let value = proposal_wire(proposal);
            if proposal.state == ProposalState::Pending {
                wire.push(value);
                continue;
            }
            let size = value.to_string().len();
            if settled == PROPOSAL_PAGE || bytes + size > PROPOSAL_BYTES {
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
        let execution = build_execution(proposal, parent, key)?;
        match self.within_scope(proposal, &execution) {
            Ok(true) => Ok(execution),
            Ok(false) => Err(INVALID),
            Err(error) => Err(error_code(&error)),
        }
    }

    fn within_scope(
        &self,
        proposal: &ActionProposal,
        execution: &Execution,
    ) -> Result<bool, WorkstreamStoreError> {
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
        })
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
                    .map_err(|error| internal("actions", &error))?;
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
            Err(error) => json!({"error_code": error_code(&error)}),
        }
    }
}

/// Turns one proposal into the workstream change it names. `parent` is the proposal
/// an `objective.add` by ref joins, which has executed by the time this runs.
fn build_execution(
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
    })
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

fn map_resolve_error(error: ResolveError) -> RpcError {
    match error {
        ResolveError::NotFound => RpcError::ProposalNotFound,
        ResolveError::NotPending(_) => RpcError::ProposalNotPending,
        ResolveError::ParentNotExecuted => RpcError::ProposalInvalid,
        ResolveError::Store(error) => internal("actions", &error),
    }
}

/// A proposal as the wire shows it. `params` is the canonical action the digest binds.
fn proposal_wire(proposal: &ActionProposal) -> Value {
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
        "created_at": proposal.created_at,
        "updated_at": proposal.updated_at,
    })
}
