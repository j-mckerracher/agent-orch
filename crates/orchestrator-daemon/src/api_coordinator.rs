//! Coordinator conversation methods.
//!
//! A proposal is validated by the M3-01 decomposition validator and stored with
//! its structured outcome. It carries no authority: it never creates a task,
//! an approval, or a policy change. The message author is fixed by the daemon
//! per method and is never read from the wire.

use orchestrator_core::decomposition::{self, DecompositionError};
use orchestrator_store::CoordinatorEntry;

use super::*;

const MAX_MESSAGE_BYTES: usize = 8 * 1024;
const MAX_PROPOSAL_BYTES: usize = 64 * 1024;
const COORDINATOR_PAGE_SIZE: usize = 100;
const MAX_ERROR_CHARS: usize = 512;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SendMessageParams {
    project_id: String,
    text: String,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProposeParams {
    project_id: String,
    proposal: String,
    idempotency_key: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CoordinatorGetParams {
    project_id: String,
    after_sequence: Option<i64>,
}

impl DaemonApi {
    pub(super) fn coordinator_send_message(&self, params: Value) -> Result<Value, RpcError> {
        let request: SendMessageParams = parse_params(params)?;
        if request.text.trim().is_empty() || request.text.len() > MAX_MESSAGE_BYTES {
            return Err(RpcError::InvalidParams);
        }
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "coordinator.send_message",
            &key,
            &request,
            || {
                self.require_project(&request.project_id)?;
                let id = idempotency::stable_id("coordinator-message", &request.idempotency_key);
                let entry = self
                    .store
                    .append_coordinator_entry(
                        &id,
                        &request.project_id,
                        "message",
                        "user",
                        &request.text,
                        None,
                    )
                    .map_err(map_store_error)?;
                Ok(entry_json(&entry))
            },
        )
    }

    pub(super) fn coordinator_propose(&self, params: Value) -> Result<Value, RpcError> {
        let request: ProposeParams = parse_params(params)?;
        if request.proposal.trim().is_empty() || request.proposal.len() > MAX_PROPOSAL_BYTES {
            return Err(RpcError::InvalidParams);
        }
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "coordinator.propose",
            &key,
            &request,
            || {
                self.require_project(&request.project_id)?;
                let validation = validation_outcome(&request.project_id, &request.proposal);
                let id = idempotency::stable_id("coordinator-proposal", &request.idempotency_key);
                let entry = self
                    .store
                    .append_coordinator_entry(
                        &id,
                        &request.project_id,
                        "proposal",
                        "coordinator",
                        &request.proposal,
                        Some(&validation),
                    )
                    .map_err(map_store_error)?;
                Ok(entry_json(&entry))
            },
        )
    }

    pub(super) fn coordinator_get(&self, params: Value) -> Result<Value, RpcError> {
        let request: CoordinatorGetParams = parse_params(params)?;
        let cursor = request.after_sequence.unwrap_or(0);
        if cursor < 0 {
            return Err(RpcError::InvalidParams);
        }
        self.require_project(&request.project_id)?;
        let mut entries = self
            .store
            .coordinator_entries_after(&request.project_id, cursor, COORDINATOR_PAGE_SIZE + 1)
            .map_err(map_store_error)?;
        let more = entries.len() > COORDINATOR_PAGE_SIZE;
        entries.truncate(COORDINATOR_PAGE_SIZE);
        let next_after_sequence = entries.last().map_or(cursor, |entry| entry.sequence);
        Ok(json!({
            "project_id": request.project_id,
            "entries": entries.iter().map(entry_json).collect::<Vec<_>>(),
            "more": more,
            "next_after_sequence": next_after_sequence
        }))
    }

    fn require_project(&self, project_id: &str) -> Result<(), RpcError> {
        self.store
            .get_project(project_id)
            .map(|_| ())
            .map_err(map_store_error)
    }
}

fn entry_json(entry: &CoordinatorEntry) -> Value {
    json!({
        "sequence": entry.sequence,
        "id": entry.id,
        "project_id": entry.project_id,
        "kind": entry.kind,
        "author": entry.author,
        "body": entry.body,
        "validation": entry.validation,
        "created_at": entry.created_at
    })
}

/// Validate a proposal and describe the outcome as a stable wire shape.
///
/// `authority` is always `none`: the engine alone creates tasks and approvals.
fn validation_outcome(project_id: &str, proposal: &str) -> Value {
    match decomposition::validate_proposal(proposal) {
        Ok(plan) => {
            let foreign: Vec<Value> = plan
                .tasks()
                .iter()
                .filter(|task| task.contract().project_id != project_id)
                .map(|task| {
                    json!({
                        "code": "project_mismatch",
                        "task_key": task.contract().id,
                        "message": "task project does not match the conversation project"
                    })
                })
                .collect();
            if !foreign.is_empty() {
                return invalid(foreign);
            }
            json!({
                "status": "valid",
                "authority": "none",
                "objective": plan.objective(),
                "tasks": plan.tasks().iter().map(|task| json!({
                    "id": task.contract().id,
                    "title": task.title(),
                    "dependencies": task.contract().dependencies,
                    "contract": task.contract(),
                })).collect::<Vec<_>>()
            })
        }
        Err(error) => invalid(decomposition_errors(&error)),
    }
}

fn invalid(errors: Vec<Value>) -> Value {
    json!({"status": "invalid", "authority": "none", "errors": errors})
}

fn decomposition_errors(error: &DecompositionError) -> Vec<Value> {
    let one = |code: &str, task_key: Option<&str>, message: String| json!({"code": code, "task_key": task_key.map(bounded), "message": bounded(&message)});
    match error {
        DecompositionError::Json(source) => {
            vec![one("invalid_json", None, source.to_string())]
        }
        DecompositionError::EmptyTasks => vec![one("empty_tasks", None, error.to_string())],
        DecompositionError::DuplicateKey(key) => {
            vec![one("duplicate_key", Some(key), error.to_string())]
        }
        DecompositionError::UnknownDependency { task_key, .. } => {
            vec![one("unknown_dependency", Some(task_key), error.to_string())]
        }
        DecompositionError::InvalidContracts { failures } => failures
            .iter()
            .map(|failure| {
                one(
                    "invalid_contract",
                    Some(&failure.key),
                    failure.error.to_string(),
                )
            })
            .collect(),
        DecompositionError::DependencyCycle { .. } => {
            vec![one("dependency_cycle", None, error.to_string())]
        }
        DecompositionError::Scheduler(_) => vec![one("scheduler", None, error.to_string())],
    }
}

fn bounded(text: &str) -> String {
    text.chars().take(MAX_ERROR_CHARS).collect()
}
