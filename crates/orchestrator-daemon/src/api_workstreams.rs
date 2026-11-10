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
use orchestrator_store::WorkstreamStoreError;

use super::*;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GateParams {
    description: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CreateParams {
    title: String,
    #[serde(default)]
    summary: String,
    idempotency_key: String,
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
struct UpdateParams {
    workstream_id: String,
    expected_revision: i64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    state: Option<WorkstreamState>,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AddObjectiveParams {
    workstream_id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    gates: Vec<GateParams>,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReviseObjectiveParams {
    objective_id: String,
    expected_revision: i64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    add_gates: Vec<GateParams>,
    idempotency_key: String,
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
struct LinkTaskParams {
    workstream_id: String,
    task_id: String,
    #[serde(default)]
    objective_id: Option<String>,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct UnlinkTaskParams {
    workstream_id: String,
    task_id: String,
    idempotency_key: String,
}

impl DaemonApi {
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
                let id = idempotency::stable_id("workstream", &request.idempotency_key);
                let workstream = Workstream::new(id, &request.title, &request.summary)
                    .map_err(|error| map_workstream_error(error.into()))?;
                let stored = self
                    .store
                    .create_workstream(&workstream, LOCAL_USER)
                    .map_err(map_workstream_error)?;
                to_json(&stored)
            },
        )
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
        let tasks = self
            .store
            .list_workstream_tasks(&request.workstream_id)
            .map_err(map_workstream_error)?;
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
        if request.title.is_none() && request.summary.is_none() && request.state.is_none() {
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
                let updated = self
                    .store
                    .update_workstream(
                        &request.workstream_id,
                        request.expected_revision,
                        request.title.as_deref(),
                        request.summary.as_deref(),
                        request.state,
                        LOCAL_USER,
                    )
                    .map_err(map_workstream_error)?;
                to_json(&updated)
            },
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
                let id = idempotency::stable_id("objective", &request.idempotency_key);
                let objective = Objective::new(
                    id,
                    &request.workstream_id,
                    &request.title,
                    &request.description,
                    new_gates(0, &request.gates),
                )
                .map_err(|error| map_workstream_error(error.into()))?;
                let stored = self
                    .store
                    .add_objective(&objective, LOCAL_USER)
                    .map_err(map_workstream_error)?;
                to_json(&stored)
            },
        )
    }

    pub(super) fn workstreams_revise_objective(&self, params: Value) -> Result<Value, RpcError> {
        let request: ReviseObjectiveParams = parse_params(params)?;
        if request.title.is_none() && request.description.is_none() && request.add_gates.is_empty()
        {
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
                let current = self
                    .store
                    .get_objective(&request.objective_id)
                    .map_err(map_workstream_error)?;
                let gates = new_gates(current.gates.len(), &request.add_gates);
                let revised = self
                    .store
                    .revise_objective(
                        &request.objective_id,
                        request.expected_revision,
                        request.title.as_deref(),
                        request.description.as_deref(),
                        &gates,
                        LOCAL_USER,
                    )
                    .map_err(map_workstream_error)?;
                to_json(&revised)
            },
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
                let link = self
                    .store
                    .link_task(
                        &request.workstream_id,
                        &request.task_id,
                        request.objective_id.as_deref(),
                    )
                    .map_err(map_workstream_error)?;
                to_json(&link)
            },
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

fn map_workstream_error(error: WorkstreamStoreError) -> RpcError {
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
