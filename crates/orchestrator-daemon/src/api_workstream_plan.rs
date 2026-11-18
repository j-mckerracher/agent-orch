//! Plan editing for a workstream's tasks (wireframe OX-W9, working names `workstreams.*`):
//! add, split, combine, remove, reassign and change the execution policy of linked tasks,
//! each in one request under the `workstreams` capability.
//!
//! A task contract never changes once admitted, so split, combine, reassign and a policy
//! change replace tasks. Each makes new tasks from the old contracts, cancels the old tasks
//! that have not started, links every new task to the old task's workstream target, and
//! records `revised_from` lineage from each new task to the task it came from (a combined
//! task links to the first task it replaces; the note names every one). A replaced task stays
//! linked to the workstream as history and cannot be replaced again.
//!
//! Every check runs before the first write. The checks and writes run under the idempotency
//! gate that every workstream and task writer takes, so a refused request writes nothing.
//! The executor takes no gate: if it starts an old task between the checks and the cancel,
//! the cancel asks the running task to stop, as `tasks.cancel` does.

use orchestrator_core::workstream::{WorkstreamRuleError, WorkstreamState};
use orchestrator_store::{
    LineageError, LineageRelation, TaskRecord, WorkstreamStoreError, WorkstreamTask,
};

use super::workstreams::{map_workstream_error, LinkTaskParams};
use super::*;

/// The most tasks one split makes, and the most tasks one combine takes.
const MAX_PLAN_TASKS: usize = 8;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AddTaskParams {
    workstream_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    objective_id: Option<String>,
    contract: TaskSubmission,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SplitTaskParams {
    workstream_id: String,
    task_id: String,
    objectives: Vec<String>,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CombineTasksParams {
    workstream_id: String,
    task_ids: Vec<String>,
    objective: String,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RemoveTaskParams {
    workstream_id: String,
    task_id: String,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReassignTaskParams {
    workstream_id: String,
    task_id: String,
    agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model_selection: Option<ModelSelection>,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SetTaskPolicyParams {
    workstream_id: String,
    task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    permission: Option<PermissionLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    autonomy: Option<AutonomyPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    delivery: Option<DeliveryPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_retries: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_repairs: Option<u32>,
    idempotency_key: String,
}

impl SetTaskPolicyParams {
    fn is_empty_patch(&self) -> bool {
        self.permission.is_none()
            && self.autonomy.is_none()
            && self.delivery.is_none()
            && self.max_retries.is_none()
            && self.max_repairs.is_none()
    }

    /// Whether the patch sets any value the contract does not already have.
    fn changes(&self, old: &TaskContract) -> bool {
        self.permission.is_some_and(|value| value != old.permission)
            || self.autonomy.is_some_and(|value| value != old.autonomy)
            || self.delivery.is_some_and(|value| value != old.delivery)
            || self
                .max_retries
                .is_some_and(|value| value != old.max_retries)
            || self
                .max_repairs
                .is_some_and(|value| value != old.max_repairs)
    }

    /// The old contract with this patch applied, as a new submission.
    fn apply(&self, old: &TaskContract) -> TaskSubmission {
        let mut next = resubmission(old);
        next.permission = Some(self.permission.unwrap_or(old.permission));
        next.autonomy = self.autonomy.unwrap_or(old.autonomy);
        next.delivery = self.delivery.unwrap_or(old.delivery);
        next.max_retries = self.max_retries.unwrap_or(old.max_retries);
        next.max_repairs = self.max_repairs.unwrap_or(old.max_repairs);
        next
    }
}

/// One task a replace makes: what it is submitted as, the task it links back to in the
/// lineage, and the note on that link.
struct NewTask {
    submission: TaskSubmission,
    source: String,
    note: String,
}

impl DaemonApi {
    pub(super) fn workstreams_add_task(&self, params: Value) -> Result<Value, RpcError> {
        let request: AddTaskParams = parse_params(params)?;
        if let Some(reply) = idempotency::replay(
            &self.store,
            "workstreams.add_task",
            &request.idempotency_key,
            &request,
        )? {
            return Ok(reply);
        }
        let mut submission = request.contract.clone();
        if !submission.agent.trim().is_empty() {
            submission.agent = canonical_agent_id(&submission.agent)
                .ok_or(RpcError::InvalidParams)?
                .to_owned();
        }
        let (resolved, catalog) = self.resolve_submission(&submission)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "workstreams.add_task",
            &key,
            &request,
            || {
                self.check_link_target(&request.workstream_id, request.objective_id.as_deref())?;
                let submit = TaskSubmitParams {
                    idempotency_key: key.clone(),
                    contract: resolved.clone(),
                };
                let seed = task_seed("workstreams.add_task", &key, 0);
                let contract = self.checked_contract(&submit, &seed, catalog.as_ref())?;
                self.admit_contract(&contract)?;
                self.link_task_inner(&LinkTaskParams {
                    workstream_id: request.workstream_id.clone(),
                    task_id: contract.id.clone(),
                    objective_id: request.objective_id.clone(),
                    idempotency_key: key.clone(),
                })
                .map_err(map_workstream_error)?;
                Ok(json!({"created": [contract.id], "replaced": []}))
            },
        )
    }

    pub(super) fn workstreams_split_task(&self, params: Value) -> Result<Value, RpcError> {
        let request: SplitTaskParams = parse_params(params)?;
        if !(2..=MAX_PLAN_TASKS).contains(&request.objectives.len())
            || request.objectives.iter().any(|text| text.trim().is_empty())
        {
            return Err(RpcError::InvalidParams);
        }
        let catalog = self.source_catalog(&request.task_id)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "workstreams.split_task",
            &key,
            &request,
            || {
                let sources = self.replaceable(&request.workstream_id, &[&request.task_id])?;
                let (old, link) = &sources[0];
                let count = request.objectives.len();
                let tasks = request
                    .objectives
                    .iter()
                    .enumerate()
                    .map(|(index, objective)| {
                        let mut submission = resubmission(&old.contract);
                        submission.objective = objective.clone();
                        submission.work_item = None;
                        NewTask {
                            submission,
                            source: old.contract.id.clone(),
                            note: format!("split into {count} tasks: part {}", index + 1),
                        }
                    })
                    .collect();
                self.replace(
                    "workstreams.split_task",
                    &key,
                    &sources,
                    link.objective_id.clone(),
                    tasks,
                    catalog.as_ref(),
                )
            },
        )
    }

    pub(super) fn workstreams_combine_tasks(&self, params: Value) -> Result<Value, RpcError> {
        let request: CombineTasksParams = parse_params(params)?;
        let mut distinct = request.task_ids.clone();
        distinct.sort_unstable();
        distinct.dedup();
        if !(2..=MAX_PLAN_TASKS).contains(&request.task_ids.len())
            || distinct.len() != request.task_ids.len()
            || request.objective.trim().is_empty()
        {
            return Err(RpcError::InvalidParams);
        }
        let catalog = self.source_catalog(&request.task_ids[0])?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "workstreams.combine_tasks",
            &key,
            &request,
            || {
                let ids: Vec<&str> = request.task_ids.iter().map(String::as_str).collect();
                let sources = self.replaceable(&request.workstream_id, &ids)?;
                let first = &sources[0].0.contract;
                let compatible = sources.iter().all(|(task, _)| {
                    task.contract.project_id == first.project_id && task.contract.kind == first.kind
                });
                if !compatible {
                    return Err(RpcError::InvalidParams);
                }
                let mut submission = resubmission(first);
                submission.objective = request.objective.clone();
                submission.work_item = None;
                submission.dependencies = combined_dependencies(&sources, &request.task_ids);
                let objective_id = sources[0].1.objective_id.clone();
                let shared = sources
                    .iter()
                    .all(|(_, link)| link.objective_id == objective_id);
                let task = NewTask {
                    submission,
                    source: first.id.clone(),
                    note: format!("combined from {}", request.task_ids.join(", ")),
                };
                self.replace(
                    "workstreams.combine_tasks",
                    &key,
                    &sources,
                    objective_id.filter(|_| shared),
                    vec![task],
                    catalog.as_ref(),
                )
            },
        )
    }

    pub(super) fn workstreams_remove_task(&self, params: Value) -> Result<Value, RpcError> {
        let request: RemoveTaskParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "workstreams.remove_task",
            &key,
            &request,
            || {
                let (task, _) = self.linked_task(&request.workstream_id, &request.task_id)?;
                self.refuse_gate_evidence(&request.workstream_id, &[&request.task_id])?;
                let cancel = match task.state {
                    TaskState::Created | TaskState::Queued | TaskState::Verifying => true,
                    TaskState::Succeeded
                    | TaskState::Failed
                    | TaskState::VerificationFailed
                    | TaskState::Cancelled => false,
                    TaskState::Running => return Err(RpcError::Conflict),
                };
                if cancel {
                    self.refuse_live_sessions(&task)?;
                    self.refuse_unfinished_dependents(&[&request.task_id])?;
                    self.cancel_task_record(&request.task_id)?;
                }
                let removed = self
                    .store
                    .unlink_task(&request.workstream_id, &request.task_id)
                    .map_err(map_workstream_error)?;
                Ok(json!({
                    "task_id": request.task_id,
                    "cancelled": cancel,
                    "removed": removed,
                }))
            },
        )
    }

    pub(super) fn workstreams_reassign_task(&self, params: Value) -> Result<Value, RpcError> {
        let mut request: ReassignTaskParams = parse_params(params)?;
        request.agent = canonical_agent_id(&request.agent)
            .ok_or(RpcError::InvalidParams)?
            .to_owned();
        if let Some(reply) = idempotency::replay(
            &self.store,
            "workstreams.reassign_task",
            &request.idempotency_key,
            &request,
        )? {
            return Ok(reply);
        }
        let mut submission = resubmission(
            &self
                .store
                .get(&request.task_id)
                .map_err(map_store_error)?
                .contract,
        );
        submission.agent = request.agent.clone();
        submission.model_selection = request.model_selection.clone();
        let (resolved, catalog) = self.resolve_submission(&submission)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "workstreams.reassign_task",
            &key,
            &request,
            || {
                let sources = self.replaceable(&request.workstream_id, &[&request.task_id])?;
                let (old, link) = &sources[0];
                if old.contract.agent == request.agent
                    && old.contract.model_selection == resolved.model_selection
                {
                    return Err(RpcError::InvalidParams);
                }
                let mut submission = resubmission(&old.contract);
                submission.agent = request.agent.clone();
                submission.model_selection = resolved.model_selection.clone();
                let task = NewTask {
                    submission,
                    source: old.contract.id.clone(),
                    note: format!(
                        "reassigned from {} to {}",
                        old.contract.agent, request.agent
                    ),
                };
                self.replace(
                    "workstreams.reassign_task",
                    &key,
                    &sources,
                    link.objective_id.clone(),
                    vec![task],
                    catalog.as_ref(),
                )
            },
        )
    }

    pub(super) fn workstreams_set_task_policy(&self, params: Value) -> Result<Value, RpcError> {
        let request: SetTaskPolicyParams = parse_params(params)?;
        if request.is_empty_patch() {
            return Err(RpcError::InvalidParams);
        }
        let catalog = self.source_catalog(&request.task_id)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "workstreams.set_task_policy",
            &key,
            &request,
            || {
                let sources = self.replaceable(&request.workstream_id, &[&request.task_id])?;
                let (old, link) = &sources[0];
                if !request.changes(&old.contract) {
                    return Err(RpcError::InvalidParams);
                }
                let task = NewTask {
                    submission: request.apply(&old.contract),
                    source: old.contract.id.clone(),
                    note: "execution policy changed".to_owned(),
                };
                self.replace(
                    "workstreams.set_task_policy",
                    &key,
                    &sources,
                    link.objective_id.clone(),
                    vec![task],
                    catalog.as_ref(),
                )
            },
        )
    }

    /// Checks every new contract, then cancels the sources that have not started, then admits
    /// and links each new task and writes its lineage. The caller holds the gate and has
    /// checked the sources with [`Self::replaceable`].
    fn replace(
        &self,
        method: &str,
        key: &str,
        sources: &[(TaskRecord, WorkstreamTask)],
        objective_id: Option<String>,
        tasks: Vec<NewTask>,
        catalog: Option<&orchestrator_agents::AgentCatalog>,
    ) -> Result<Value, RpcError> {
        let workstream_id = sources[0].1.workstream_id.clone();
        let mut checked = Vec::with_capacity(tasks.len());
        for (index, task) in tasks.into_iter().enumerate() {
            let submit = TaskSubmitParams {
                idempotency_key: key.to_owned(),
                contract: task.submission,
            };
            let seed = task_seed(method, key, index);
            let contract = self.checked_contract(&submit, &seed, catalog)?;
            checked.push((contract, task.source, task.note));
        }
        for (old, _) in sources {
            if matches!(old.state, TaskState::Created | TaskState::Queued) {
                self.cancel_task_record(&old.contract.id)?;
            }
        }
        let mut created = Vec::with_capacity(checked.len());
        for (contract, source, note) in checked {
            self.admit_contract(&contract)?;
            self.link_task_inner(&LinkTaskParams {
                workstream_id: workstream_id.clone(),
                task_id: contract.id.clone(),
                objective_id: objective_id.clone(),
                idempotency_key: key.to_owned(),
            })
            .map_err(map_workstream_error)?;
            self.store
                .add_task_lineage(
                    &contract.id,
                    &source,
                    LineageRelation::RevisedFrom,
                    Some(&note),
                )
                .map_err(|error| match LineageError::of(&error) {
                    Some(_) => RpcError::Conflict,
                    None => map_store_error(error),
                })?;
            created.push(contract.id);
        }
        let replaced: Vec<&str> = sources
            .iter()
            .map(|(task, _)| task.contract.id.as_str())
            .collect();
        Ok(json!({"created": created, "replaced": replaced}))
    }

    /// The tasks a replace may take, with their links, in request order. Each must be linked
    /// to this workstream, not started or failed, not already replaced, not cited as gate
    /// evidence, and not needed by an unfinished task outside the set.
    fn replaceable(
        &self,
        workstream_id: &str,
        task_ids: &[&str],
    ) -> Result<Vec<(TaskRecord, WorkstreamTask)>, RpcError> {
        let mut sources = Vec::with_capacity(task_ids.len());
        for task_id in task_ids {
            let (task, link) = self.linked_task(workstream_id, task_id)?;
            match task.state {
                TaskState::Created | TaskState::Queued => self.refuse_live_sessions(&task)?,
                TaskState::Failed | TaskState::VerificationFailed => {}
                _ => return Err(RpcError::Conflict),
            }
            let replaced = self
                .store
                .task_lineage_children(task_id)
                .map_err(map_store_error)?;
            if !replaced.is_empty() {
                return Err(RpcError::Conflict);
            }
            sources.push((task, link));
        }
        self.refuse_gate_evidence(workstream_id, task_ids)?;
        self.refuse_unfinished_dependents(task_ids)?;
        Ok(sources)
    }

    /// The task and its link, when the task is linked to this workstream and the workstream
    /// takes changes. A task linked elsewhere, or to nothing, is not found here.
    fn linked_task(
        &self,
        workstream_id: &str,
        task_id: &str,
    ) -> Result<(TaskRecord, WorkstreamTask), RpcError> {
        let workstream = self
            .store
            .get_workstream(workstream_id)
            .map_err(map_workstream_error)?;
        if workstream.state == WorkstreamState::Archived {
            return Err(RpcError::Conflict);
        }
        let link = self
            .store
            .workstream_task_link(task_id)
            .map_err(map_workstream_error)?
            .filter(|link| link.workstream_id == workstream_id)
            .ok_or(RpcError::NotFound)?;
        let task = self.store.get(task_id).map_err(map_store_error)?;
        Ok((task, link))
    }

    /// Refuses a task that a gate of this workstream cites as evidence: the gate would cite
    /// a task the plan no longer runs.
    fn refuse_gate_evidence(&self, workstream_id: &str, task_ids: &[&str]) -> Result<(), RpcError> {
        let objectives = self
            .store
            .list_objectives(workstream_id)
            .map_err(map_workstream_error)?;
        let cited = objectives
            .iter()
            .flat_map(|objective| &objective.gates)
            .any(|gate| {
                gate.evidence_task_id
                    .as_deref()
                    .is_some_and(|id| task_ids.contains(&id))
            });
        if cited {
            return Err(RpcError::Conflict);
        }
        Ok(())
    }

    /// Refuses when an unfinished task outside `task_ids` depends on one of them: cancelling
    /// the task would leave that task waiting on work that never comes.
    fn refuse_unfinished_dependents(&self, task_ids: &[&str]) -> Result<(), RpcError> {
        let tasks = self.store.list().map_err(map_store_error)?;
        let blocked = tasks.iter().any(|task| {
            !task.state.is_terminal()
                && !task_ids.contains(&task.contract.id.as_str())
                && task
                    .contract
                    .dependencies
                    .iter()
                    .any(|dependency| task_ids.contains(&dependency.as_str()))
        });
        if blocked {
            return Err(RpcError::Conflict);
        }
        Ok(())
    }

    /// The same refusal `tasks.cancel` makes for a task that has not started, made before
    /// anything is written.
    fn refuse_live_sessions(&self, task: &TaskRecord) -> Result<(), RpcError> {
        if task
            .sessions
            .iter()
            .any(|session| !session.state.is_terminal())
        {
            return Err(RpcError::Conflict);
        }
        Ok(())
    }

    /// Refuses a link the store would refuse, before any task is made for it.
    fn check_link_target(
        &self,
        workstream_id: &str,
        objective_id: Option<&str>,
    ) -> Result<(), RpcError> {
        let workstream = self
            .store
            .get_workstream(workstream_id)
            .map_err(map_workstream_error)?;
        if workstream.state == WorkstreamState::Archived {
            return Err(map_workstream_error(WorkstreamStoreError::Rule(
                WorkstreamRuleError::WorkstreamArchived,
            )));
        }
        if let Some(objective_id) = objective_id {
            let objective = self
                .store
                .get_objective(objective_id)
                .map_err(map_workstream_error)?;
            if objective.workstream_id != workstream_id {
                return Err(RpcError::InvalidParams);
            }
        }
        Ok(())
    }

    /// The catalog a copy of `task_id` is checked against, read before the gate is taken
    /// (see `submit_task`). The contract never changes, so the read stays true under the gate.
    fn source_catalog(
        &self,
        task_id: &str,
    ) -> Result<Option<orchestrator_agents::AgentCatalog>, RpcError> {
        let task = self.store.get(task_id).map_err(map_store_error)?;
        self.selection_catalog(&task.contract.agent, task.contract.model_selection.as_ref())
    }
}

/// The submission that makes a copy of `contract`.
fn resubmission(contract: &TaskContract) -> TaskSubmission {
    TaskSubmission {
        project_id: contract.project_id.clone(),
        parent_id: contract.parent_id.clone(),
        objective: contract.objective.clone(),
        kind: contract.kind,
        agent: contract.agent.clone(),
        dependencies: contract.dependencies.clone(),
        verification: contract.verification.clone(),
        delivery: contract.delivery,
        autonomy: contract.autonomy,
        max_retries: contract.max_retries,
        max_repairs: contract.max_repairs,
        model_selection: contract.model_selection.clone(),
        permission: Some(contract.permission),
        work_class: None,
        work_item: contract.work_item.clone(),
    }
}

/// Every dependency of the combined tasks, in first-seen order, without the combined tasks
/// themselves.
fn combined_dependencies(
    sources: &[(TaskRecord, WorkstreamTask)],
    combined: &[String],
) -> Vec<String> {
    let mut dependencies: Vec<String> = Vec::new();
    for dependency in sources
        .iter()
        .flat_map(|(task, _)| &task.contract.dependencies)
    {
        if !combined.contains(dependency) && !dependencies.contains(dependency) {
            dependencies.push(dependency.clone());
        }
    }
    dependencies
}

/// Seeds the id of the `index`th task one request makes. A rerun with the same key makes the
/// same ids, and two methods never share one.
fn task_seed(method: &str, key: &str, index: usize) -> String {
    format!("{method}:{key}:{index}")
}
