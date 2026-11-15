//! Coordinator conversation methods.
//!
//! A proposal is validated by the M3-01 decomposition validator and stored with
//! its structured outcome. It carries no authority: it never creates a task,
//! an approval, or a policy change. The message author is fixed by the daemon
//! per method and is never read from the wire.
//!
//! DPES-1105: `coordinator.clear` deletes a log, and `coordinator.remove` deletes it
//! and hides the coordinator. `coordinator.restore` brings it back empty. The idempotency
//! record for a send keeps references only, so a cleared message leaves no text behind.

use orchestrator_core::decomposition::{self, DecompositionError};
use orchestrator_store::{CoordinatorChange, CoordinatorEntry};

use super::conversations::reference_str;
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
        let reference = idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "coordinator.send_message",
            &key,
            &request,
            || {
                self.require_active_coordinator(&request.project_id)?;
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
                Ok(entry_reference(&entry))
            },
        )?;
        self.coordinator_reply(&request.project_id, &reference)
    }

    pub(super) fn coordinator_propose(&self, params: Value) -> Result<Value, RpcError> {
        let request: ProposeParams = parse_params(params)?;
        if request.proposal.trim().is_empty() || request.proposal.len() > MAX_PROPOSAL_BYTES {
            return Err(RpcError::InvalidParams);
        }
        let key = request.idempotency_key.clone();
        let reference = idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "coordinator.propose",
            &key,
            &request,
            || {
                self.require_active_coordinator(&request.project_id)?;
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
                Ok(entry_reference(&entry))
            },
        )?;
        self.coordinator_reply(&request.project_id, &reference)
    }

    pub(super) fn coordinator_get(&self, params: Value) -> Result<Value, RpcError> {
        let request: CoordinatorGetParams = parse_params(params)?;
        let cursor = request.after_sequence.unwrap_or(0);
        if cursor < 0 {
            return Err(RpcError::InvalidParams);
        }
        self.require_active_coordinator(&request.project_id)?;
        let scope = self
            .store
            .coordinator_scope(&request.project_id)
            .map_err(map_store_error)?;
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
            "next_after_sequence": next_after_sequence,
            "clear_generation": scope.clear_generation
        }))
    }

    /// Permanently deletes a project coordinator's log (DPES-1105). The project and its
    /// tasks are untouched. Refused with NotFound while the coordinator is removed.
    pub(super) fn coordinator_clear(&self, params: Value) -> Result<Value, RpcError> {
        let request: CoordinatorHistoryParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "coordinator.clear",
            &key,
            &request,
            || {
                let outcome = self
                    .store
                    .clear_coordinator_history(&request.project_id)
                    .map_err(map_store_error)?;
                Ok(json!({
                    "project_id": request.project_id,
                    "cleared": outcome.cleared,
                    "entries_deleted": outcome.entries_deleted,
                    "clear_generation": outcome.clear_generation
                }))
            },
        )
    }

    /// Removes a project's coordinator from the conversation targets and deletes its log
    /// permanently (DPES-1105). Idempotent. The project and its tasks stay.
    pub(super) fn coordinator_remove(&self, params: Value) -> Result<Value, RpcError> {
        let request: CoordinatorHistoryParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "coordinator.remove",
            &key,
            &request,
            || {
                let change = self
                    .store
                    .remove_coordinator(&request.project_id)
                    .map_err(map_store_error)?;
                Ok(coordinator_change_json(&request.project_id, &change))
            },
        )
    }

    /// Restores a removed coordinator. It starts empty. Idempotent.
    pub(super) fn coordinator_restore(&self, params: Value) -> Result<Value, RpcError> {
        let request: CoordinatorHistoryParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "coordinator.restore",
            &key,
            &request,
            || {
                let change = self
                    .store
                    .restore_coordinator(&request.project_id)
                    .map_err(map_store_error)?;
                Ok(coordinator_change_json(&request.project_id, &change))
            },
        )
    }

    /// Builds a message or proposal response from its cached reference. The entry is
    /// read back from the store; a cleared entry is answered as a reference with
    /// `cleared: true`.
    fn coordinator_reply(&self, project_id: &str, reference: &Value) -> Result<Value, RpcError> {
        let entry_id = reference_str(reference, "entry_id")?;
        match self
            .store
            .coordinator_entry(entry_id)
            .map_err(map_store_error)?
        {
            Some(entry) => Ok(entry_json(&entry)),
            None => Ok(json!({
                "id": entry_id,
                "sequence": reference.get("entry_sequence").cloned().unwrap_or(Value::Null),
                "created_at": reference.get("entry_created_at").cloned().unwrap_or(Value::Null),
                "project_id": project_id,
                "cleared": true
            })),
        }
    }

    /// Fails with NotFound for an unknown project and for a removed coordinator.
    fn require_active_coordinator(&self, project_id: &str) -> Result<(), RpcError> {
        self.require_project(project_id)?;
        let scope = self
            .store
            .coordinator_scope(project_id)
            .map_err(map_store_error)?;
        if scope.removed {
            return Err(RpcError::NotFound);
        }
        Ok(())
    }

    fn require_project(&self, project_id: &str) -> Result<(), RpcError> {
        self.store
            .get_project(project_id)
            .map(|_| ())
            .map_err(map_store_error)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CoordinatorHistoryParams {
    project_id: String,
    idempotency_key: String,
}

fn coordinator_change_json(project_id: &str, change: &CoordinatorChange) -> Value {
    json!({
        "project_id": project_id,
        "changed": change.changed,
        "removed": change.removed,
        "entries_deleted": change.entries_deleted,
        "clear_generation": change.clear_generation
    })
}

/// The reference a fresh message or proposal caches: its id, sequence and creation time.
fn entry_reference(entry: &CoordinatorEntry) -> Value {
    json!({
        "entry_id": entry.id,
        "entry_sequence": entry.sequence,
        "entry_created_at": entry.created_at
    })
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
