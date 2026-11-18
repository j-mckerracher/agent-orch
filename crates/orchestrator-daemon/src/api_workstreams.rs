//! Workstream and objective methods (M7-07, working names `workstreams.*`).
//!
//! The method names and shapes are proposed, not frozen: they are the backend
//! for the post-release workstream entity of UI gap decision D-4. All methods
//! sit behind the `workstreams` capability and the authenticated session.
//! Mutations carry an idempotency key, and a write presents the
//! `expected_revision` it read; a stale revision is rejected as `Conflict`
//! with nothing written (the wire error carries no payload, so a client
//! re-reads with `workstreams.get`).
//!
//! Linking a task never changes the task or its contract; `tasks.submit` is
//! unchanged. `suspended` is advisory: the scheduler does not consult it.

use orchestrator_core::workstream::{
    AcceptanceGate, GateState, Objective, Workstream, WorkstreamRuleError, WorkstreamState,
};
use orchestrator_store::{WorkstreamStoreError, WorkstreamTask};

use super::*;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GateParams {
    pub(super) description: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreateParams {
    pub(super) title: String,
    #[serde(default)]
    pub(super) summary: String,
    pub(super) idempotency_key: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    #[serde(default)]
    state: Option<WorkstreamState>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GetParams {
    workstream_id: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PrioritizeParams {
    workstream_id: String,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UpdateParams {
    pub(super) workstream_id: String,
    pub(super) expected_revision: i64,
    #[serde(default)]
    pub(super) title: Option<String>,
    #[serde(default)]
    pub(super) summary: Option<String>,
    #[serde(default)]
    pub(super) state: Option<WorkstreamState>,
    pub(super) idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AddObjectiveParams {
    pub(super) workstream_id: String,
    pub(super) title: String,
    #[serde(default)]
    pub(super) description: String,
    #[serde(default)]
    pub(super) gates: Vec<GateParams>,
    pub(super) idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReviseObjectiveParams {
    pub(super) objective_id: String,
    pub(super) expected_revision: i64,
    #[serde(default)]
    pub(super) title: Option<String>,
    #[serde(default)]
    pub(super) description: Option<String>,
    #[serde(default)]
    pub(super) add_gates: Vec<GateParams>,
    pub(super) idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SetGateParams {
    objective_id: String,
    expected_revision: i64,
    gate_id: String,
    state: GateState,
    #[serde(default)]
    evidence_task_id: Option<String>,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LinkTaskParams {
    pub(super) workstream_id: String,
    pub(super) task_id: String,
    #[serde(default)]
    pub(super) objective_id: Option<String>,
    pub(super) idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct UnlinkTaskParams {
    workstream_id: String,
    task_id: String,
    idempotency_key: String,
}

impl UpdateParams {
    /// Refused before the idempotency claim, so an empty patch is never cached.
    pub(super) fn is_empty_patch(&self) -> bool {
        self.title.is_none() && self.summary.is_none() && self.state.is_none()
    }
}

impl ReviseObjectiveParams {
    /// Refused before the idempotency claim, so an empty patch is never cached.
    pub(super) fn is_empty_patch(&self) -> bool {
        self.title.is_none() && self.description.is_none() && self.add_gates.is_empty()
    }
}

/// Seeds the stable id of the workstream that holds the tasks the global conversation
/// proposes without naming a workstream, so every run finds the one it made.
const GLOBAL_TASKS_KEY: &str = "default:global-tasks";
const GLOBAL_TASKS_TITLE: &str = "Global tasks";
const GLOBAL_TASKS_SUMMARY: &str =
    "Tasks the global orchestrator proposed without naming a workstream.";

impl DaemonApi {
    /// The default workstream for tasks proposed in the global conversation. It is made on
    /// first use and found by its stable id afterwards, so a rename keeps it. The caller
    /// holds the workstream gate.
    pub(super) fn global_tasks_workstream(&self) -> Result<Workstream, WorkstreamStoreError> {
        let id = idempotency::stable_id("workstream", GLOBAL_TASKS_KEY);
        match self.store.get_workstream(&id) {
            Err(WorkstreamStoreError::NotFound { .. }) => self.create_inner(
                &CreateParams {
                    title: GLOBAL_TASKS_TITLE.to_owned(),
                    summary: GLOBAL_TASKS_SUMMARY.to_owned(),
                    idempotency_key: GLOBAL_TASKS_KEY.to_owned(),
                },
                "daemon",
            ),
            found => found,
        }
    }

    pub(super) fn workstreams_create(&self, params: Value) -> Result<Value, RpcError> {
        let request: CreateParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "workstreams.create",
            &key,
            &request,
            || {
                self.create_inner(&request, LOCAL_USER)
                    .map_err(map_workstream_error)
                    .and_then(|stored| to_json(&stored))
            },
        )
    }

    /// The create write, shared with action execution. Returns the typed store error.
    pub(super) fn create_inner(
        &self,
        request: &CreateParams,
        actor: &str,
    ) -> Result<Workstream, WorkstreamStoreError> {
        let id = idempotency::stable_id("workstream", &request.idempotency_key);
        let workstream = Workstream::new(id, &request.title, &request.summary)?;
        self.store.create_workstream(&workstream, actor)
    }

    pub(super) fn workstreams_list(&self, params: Value) -> Result<Value, RpcError> {
        let request: ListParams = parse_params(params)?;
        let workstreams = self
            .store
            .list_workstreams(request.state)
            .map_err(map_workstream_error)?;
        to_json(&workstreams)
    }

    pub(super) fn workstreams_get(&self, params: Value) -> Result<Value, RpcError> {
        let request: GetParams = parse_params(params)?;
        let workstream = self
            .store
            .get_workstream(&request.workstream_id)
            .map_err(map_workstream_error)?;
        let objectives = self
            .store
            .list_objectives(&request.workstream_id)
            .map_err(map_workstream_error)?;
        // Links to archived tasks are left out of the view. The store query keeps them, so the
        // gates and the scheduler still see every link.
        let archived = self.store.archived_task_ids().map_err(map_store_error)?;
        let tasks: Vec<_> = self
            .store
            .list_workstream_tasks(&request.workstream_id)
            .map_err(map_workstream_error)?
            .into_iter()
            .filter(|task| !archived.contains(&task.task_id))
            .collect();
        let mut project_ids: Vec<&str> =
            tasks.iter().map(|task| task.project_id.as_str()).collect();
        project_ids.sort_unstable();
        project_ids.dedup();
        Ok(json!({
            "workstream": to_json(&workstream)?,
            "objectives": to_json(&objectives)?,
            "tasks": to_json(&tasks)?,
            "project_ids": project_ids,
        }))
    }

    pub(super) fn workstreams_update(&self, params: Value) -> Result<Value, RpcError> {
        let request: UpdateParams = parse_params(params)?;
        if request.is_empty_patch() {
            return Err(RpcError::InvalidParams);
        }
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "workstreams.update",
            &key,
            &request,
            || {
                self.update_inner(&request, LOCAL_USER)
                    .map_err(map_workstream_error)
                    .and_then(|updated| to_json(&updated))
            },
        )
    }

    /// Moves a workstream to the top of the priority order. Advisory, like
    /// `suspended`: the scheduler does not consult it.
    pub(super) fn workstreams_prioritize(&self, params: Value) -> Result<Value, RpcError> {
        let request: PrioritizeParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "workstreams.prioritize",
            &key,
            &request,
            || {
                self.store
                    .prioritize_workstream(&request.workstream_id)
                    .map_err(map_workstream_error)
                    .and_then(|workstream| to_json(&workstream))
            },
        )
    }

    /// The update write, shared with action execution. Returns the typed store error.
    pub(super) fn update_inner(
        &self,
        request: &UpdateParams,
        actor: &str,
    ) -> Result<Workstream, WorkstreamStoreError> {
        self.store.update_workstream(
            &request.workstream_id,
            request.expected_revision,
            request.title.as_deref(),
            request.summary.as_deref(),
            request.state,
            actor,
        )
    }

    pub(super) fn workstreams_add_objective(&self, params: Value) -> Result<Value, RpcError> {
        let request: AddObjectiveParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "workstreams.add_objective",
            &key,
            &request,
            || {
                self.add_objective_inner(&request, LOCAL_USER)
                    .map_err(map_workstream_error)
                    .and_then(|stored| to_json(&stored))
            },
        )
    }

    /// The add-objective write, shared with action execution. Returns the typed store error.
    pub(super) fn add_objective_inner(
        &self,
        request: &AddObjectiveParams,
        actor: &str,
    ) -> Result<Objective, WorkstreamStoreError> {
        let id = idempotency::stable_id("objective", &request.idempotency_key);
        let objective = Objective::new(
            id,
            &request.workstream_id,
            &request.title,
            &request.description,
            new_gates(0, &request.gates),
        )?;
        self.store.add_objective(&objective, actor)
    }

    pub(super) fn workstreams_revise_objective(&self, params: Value) -> Result<Value, RpcError> {
        let request: ReviseObjectiveParams = parse_params(params)?;
        if request.is_empty_patch() {
            return Err(RpcError::InvalidParams);
        }
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "workstreams.revise_objective",
            &key,
            &request,
            || {
                self.revise_objective_inner(&request, LOCAL_USER)
                    .map_err(map_workstream_error)
                    .and_then(|revised| to_json(&revised))
            },
        )
    }

    /// The revise write, shared with action execution. Returns the typed store error.
    pub(super) fn revise_objective_inner(
        &self,
        request: &ReviseObjectiveParams,
        actor: &str,
    ) -> Result<Objective, WorkstreamStoreError> {
        let current = self.store.get_objective(&request.objective_id)?;
        let gates = new_gates(current.gates.len(), &request.add_gates);
        self.store.revise_objective(
            &request.objective_id,
            request.expected_revision,
            request.title.as_deref(),
            request.description.as_deref(),
            &gates,
            actor,
        )
    }

    pub(super) fn workstreams_set_gate(&self, params: Value) -> Result<Value, RpcError> {
        let request: SetGateParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "workstreams.set_gate",
            &key,
            &request,
            || {
                let updated = self
                    .store
                    .set_gate_state(
                        &request.objective_id,
                        request.expected_revision,
                        &request.gate_id,
                        request.state,
                        request.evidence_task_id.as_deref(),
                        LOCAL_USER,
                    )
                    .map_err(map_workstream_error)?;
                to_json(&updated)
            },
        )
    }

    pub(super) fn workstreams_link_task(&self, params: Value) -> Result<Value, RpcError> {
        let request: LinkTaskParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "workstreams.link_task",
            &key,
            &request,
            || {
                self.link_task_inner(&request)
                    .map_err(map_workstream_error)
                    .and_then(|link| to_json(&link))
            },
        )
    }

    /// The link write, shared with action execution. Returns the typed store error.
    pub(super) fn link_task_inner(
        &self,
        request: &LinkTaskParams,
    ) -> Result<WorkstreamTask, WorkstreamStoreError> {
        self.store.link_task(
            &request.workstream_id,
            &request.task_id,
            request.objective_id.as_deref(),
        )
    }

    pub(super) fn workstreams_unlink_task(&self, params: Value) -> Result<Value, RpcError> {
        let request: UnlinkTaskParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "workstreams.unlink_task",
            &key,
            &request,
            || {
                let removed = self
                    .store
                    .unlink_task(&request.workstream_id, &request.task_id)
                    .map_err(map_workstream_error)?;
                Ok(json!({"removed": removed}))
            },
        )
    }
}

/// Builds pending gates with ids that continue after `existing` gates. Gates
/// are never removed, so `gate-N` stays unique.
fn new_gates(existing: usize, gates: &[GateParams]) -> Vec<AcceptanceGate> {
    gates
        .iter()
        .enumerate()
        .map(|(offset, gate)| AcceptanceGate {
            id: format!("gate-{}", existing + offset + 1),
            description: gate.description.clone(),
            state: GateState::Pending,
            evidence_task_id: None,
        })
        .collect()
}

fn to_json<T: Serialize>(value: &T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|_| RpcError::Internal)
}

pub(super) fn map_workstream_error(error: WorkstreamStoreError) -> RpcError {
    match error {
        WorkstreamStoreError::Rule(rule) => match rule {
            WorkstreamRuleError::IllegalWorkstreamTransition { .. }
            | WorkstreamRuleError::IllegalGateTransition { .. }
            | WorkstreamRuleError::WorkstreamArchived => RpcError::Conflict,
            WorkstreamRuleError::UnknownGate(_) => RpcError::NotFound,
            _ => RpcError::InvalidParams,
        },
        WorkstreamStoreError::Stale { .. }
        | WorkstreamStoreError::AlreadyExists { .. }
        | WorkstreamStoreError::TaskAlreadyLinked { .. }
        | WorkstreamStoreError::TaskIsGateEvidence { .. } => RpcError::Conflict,
        WorkstreamStoreError::NotFound { .. } => RpcError::NotFound,
        WorkstreamStoreError::ObjectiveMismatch { .. }
        | WorkstreamStoreError::EvidenceNotLinked { .. } => RpcError::InvalidParams,
        WorkstreamStoreError::Store(error) => map_store_error(error),
    }
}
