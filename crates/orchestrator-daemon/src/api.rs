//! Authenticated project, task, session, approval, workspace, and delivery API.
//!
//! Wire parameters are parsed once at the RPC boundary. Results omit repository
//! and worktree paths; durable mutations use the shared idempotency executor.
//! Updated: 2026-11-08T17:33:37+00:00.

use std::path::Path;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use orchestrator_core::approval::{
    Actor, Approval, ApprovalAction, ApprovalActionKind, ApproverAllowlist, ParameterDigest,
};
use orchestrator_core::authorization::ActionKind;
use orchestrator_core::contract::{
    self, AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::identity::{
    classify_identity, ProcessClassification, ProcessIdentity, ProcessObservation,
};
use orchestrator_core::records::{Project, Workspace};
use orchestrator_core::scheduler::{Scheduler, SchedulerLimits, SchedulerSnapshot, TaskSnapshot};
use orchestrator_core::task::{AttemptState, SessionState, TaskEvent, TaskState};
use orchestrator_git::delivery::{
    self, ApprovalCheck, CallbackError, DeliveryAction, DeliveryCallbacks, DeliveryOutcome,
    DeliveryRequest, DeliveryStep, VerificationCheck,
};
use orchestrator_git::Repository;
use orchestrator_ipc::{Capability, IpcEvent, RpcError, RpcHandler};
use orchestrator_runtime::reconcile::observe_process;
use orchestrator_store::{
    DeliveryOutcome as StoredDeliveryOutcome, DeliveryStepRecord, SqliteStore, Store, StoreError,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::idempotency;

const MAX_DIFF_BYTES: usize = 512 * 1024;
const EVENT_PAGE_SIZE: usize = 256;
const LOCAL_USER: &str = "local-user";

pub(crate) struct DaemonApi {
    store: SqliteStore,
    idempotency_gate: Mutex<()>,
    approval_actor: Actor,
    approval_allowlist: ApproverAllowlist,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskSubmitParams {
    idempotency_key: String,
    contract: TaskSubmission,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskSubmission {
    project_id: String,
    parent_id: Option<String>,
    objective: String,
    kind: TaskKind,
    agent: String,
    dependencies: Vec<String>,
    verification: VerificationPolicy,
    delivery: DeliveryPolicy,
    autonomy: AutonomyPolicy,
    max_retries: u32,
    max_repairs: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskIdParams {
    task_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelParams {
    task_id: String,
    idempotency_key: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionsListParams {
    task_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionParams {
    task_id: String,
    session_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionEventsParams {
    task_id: String,
    session_id: String,
    after_sequence: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveApprovalParams {
    approval_id: String,
    decision: String,
    idempotency_key: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DiffParams {
    task_id: String,
    workspace_id: Option<String>,
    path: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeliveryParams {
    task_id: String,
    workspace_id: Option<String>,
    approval_id: String,
    verification_run_id: String,
    action: String,
    target_ref: Option<String>,
    idempotency_key: String,
}

impl DaemonApi {
    pub(crate) fn new(data_dir: &Path) -> Result<Self, String> {
        let store = SqliteStore::new(&data_dir.join("store.sqlite"))
            .map_err(|_| "cannot open daemon store".to_owned())?;
        let approval_actor = Actor::User(LOCAL_USER.to_owned());
        let approval_allowlist =
            ApproverAllowlist::new(LOCAL_USER, std::iter::empty::<String>())
                .map_err(|_| "cannot initialize local approval policy".to_owned())?;
        Ok(Self {
            store,
            idempotency_gate: Mutex::new(()),
            approval_actor,
            approval_allowlist,
        })
    }
}

impl RpcHandler for DaemonApi {
    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::Projects,
            Capability::Tasks,
            Capability::Sessions,
            Capability::Events,
            Capability::Approvals,
            Capability::Workspaces,
            Capability::Delivery,
        ]
    }

    fn dispatch(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "projects.list" => self.projects_list(params),
            "tasks.submit" => self.submit_task(params),
            "tasks.list" => self.list_tasks(params),
            "tasks.get" => self.get_task(params),
            "tasks.cancel" => self.cancel_task(params),
            "sessions.list" => self.list_sessions(params),
            "sessions.get" => self.get_session(params),
            "sessions.events" => self.session_events(params),
            "approvals.list" => self.list_approvals(params),
            "approvals.resolve" => self.resolve_approval(params),
            "workspaces.diff" => self.workspace_diff(params),
            "delivery.request" => self.request_delivery(params),
            _ => Err(RpcError::InvalidParams),
        }
    }

    fn events_after(&self, after_sequence: i64, limit: usize) -> Result<Vec<IpcEvent>, RpcError> {
        self.store
            .stream_events_after(after_sequence, limit)
            .map(|events| events.into_iter().map(stream_event).collect())
            .map_err(map_store_error)
    }

    fn latest_event_sequence(&self) -> Result<i64, RpcError> {
        self.store.latest_event_sequence().map_err(map_store_error)
    }

    fn keep_alive(&self) -> bool {
        self.store.list().map_or(true, |tasks| {
            tasks
                .iter()
                .any(|task| matches!(task.state, TaskState::Running | TaskState::Verifying))
        })
    }
}

impl DaemonApi {
    fn projects_list(&self, params: Value) -> Result<Value, RpcError> {
        require_empty_params(&params)?;
        let projects = self.store.list_projects().map_err(map_store_error)?;
        Ok(Value::Array(
            projects
                .into_iter()
                .map(|project| json!({"id": project.id}))
                .collect(),
        ))
    }

    fn submit_task(&self, params: Value) -> Result<Value, RpcError> {
        let request: TaskSubmitParams = parse_params(params.clone())?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "tasks.submit",
            &key,
            &params,
            || {
                let contract = TaskContract {
                    id: idempotency::stable_id("task", &key),
                    project_id: request.contract.project_id,
                    parent_id: request.contract.parent_id,
                    objective: request.contract.objective,
                    kind: request.contract.kind,
                    agent: request.contract.agent,
                    dependencies: request.contract.dependencies,
                    verification: request.contract.verification,
                    delivery: request.contract.delivery,
                    autonomy: request.contract.autonomy,
                    max_retries: request.contract.max_retries,
                    max_repairs: request.contract.max_repairs,
                };
                self.store
                    .get_project(&contract.project_id)
                    .map_err(map_store_error)?;
                contract::validate(&contract).map_err(|_| RpcError::InvalidParams)?;
                self.validate_scheduler(&contract)?;
                self.store.create(&contract).map_err(map_store_error)?;
                self.store
                    .transition(&contract.id, TaskEvent::Admit)
                    .map_err(map_store_error)?;
                serde_json::to_value(self.store.get(&contract.id).map_err(map_store_error)?)
                    .map_err(|_| RpcError::Internal)
            },
        )
    }

    fn validate_scheduler(&self, candidate: &TaskContract) -> Result<(), RpcError> {
        let existing = self.store.list().map_err(map_store_error)?;
        let workspaces = self.store.list_workspaces().map_err(map_store_error)?;
        let mut contracts: Vec<_> = existing.iter().map(|task| task.contract.clone()).collect();
        contracts.push(candidate.clone());
        let snapshots: Vec<_> = contracts
            .iter()
            .map(|contract| TaskSnapshot {
                contract,
                state: existing
                    .iter()
                    .find(|task| task.contract.id == contract.id)
                    .map_or(TaskState::Created, |task| task.state),
                workspace: workspaces
                    .iter()
                    .find(|workspace| workspace.task_id == contract.id),
            })
            .collect();
        Scheduler::new(
            SchedulerSnapshot {
                tasks: &snapshots,
                last_started_project: None,
            },
            SchedulerLimits::default(),
        )
        .map_err(|_| RpcError::Conflict)?;
        Ok(())
    }

    fn list_tasks(&self, params: Value) -> Result<Value, RpcError> {
        require_empty_params(&params)?;
        serde_json::to_value(self.store.list().map_err(map_store_error)?)
            .map_err(|_| RpcError::Internal)
    }

    fn get_task(&self, params: Value) -> Result<Value, RpcError> {
        let params: TaskIdParams = parse_params(params)?;
        serde_json::to_value(self.store.get(&params.task_id).map_err(map_store_error)?)
            .map_err(|_| RpcError::Internal)
    }
}

impl DaemonApi {
    fn cancel_task(&self, params: Value) -> Result<Value, RpcError> {
        let request: CancelParams = parse_params(params.clone())?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "tasks.cancel",
            &key,
            &params,
            || {
                self.cancel_task_record(&request.task_id)?;
                serde_json::to_value(self.store.get(&request.task_id).map_err(map_store_error)?)
                    .map_err(|_| RpcError::Internal)
            },
        )
    }

    fn cancel_task_record(&self, task_id: &str) -> Result<(), RpcError> {
        let task = self.store.get(task_id).map_err(map_store_error)?;
        match task.state {
            TaskState::Running => self.cancel_running_task(&task)?,
            TaskState::Created | TaskState::Queued | TaskState::Verifying => {
                if task
                    .sessions
                    .iter()
                    .any(|session| !session.state.is_terminal())
                {
                    return Err(RpcError::Conflict);
                }
                for attempt in task
                    .attempts
                    .iter()
                    .filter(|attempt| !attempt.state.is_terminal())
                {
                    self.store
                        .update_attempt_state(task_id, &attempt.id, AttemptState::Cancelled)
                        .map_err(map_store_error)?;
                }
            }
            _ => return Err(RpcError::Conflict),
        }
        self.store
            .transition(task_id, TaskEvent::CancelRequested)
            .map_err(map_store_error)?;
        Ok(())
    }

    fn cancel_running_task(&self, task: &orchestrator_store::TaskRecord) -> Result<(), RpcError> {
        let mut sessions = task
            .sessions
            .iter()
            .filter(|session| !session.state.is_terminal());
        let session = sessions.next().ok_or(RpcError::Forbidden)?;
        if sessions.next().is_some() {
            return Err(RpcError::Conflict);
        }
        let attempt = task
            .attempts
            .iter()
            .find(|attempt| attempt.id == session.attempt_id)
            .ok_or(RpcError::Conflict)?;
        if attempt.state != AttemptState::Running {
            return Err(RpcError::Conflict);
        }
        let identity = self
            .store
            .session_identity(&task.contract.id, &session.id)
            .map_err(map_store_error)?
            .ok_or(RpcError::Forbidden)?;
        terminate_process_group(identity)?;
        self.store
            .update_session_state(&task.contract.id, &session.id, SessionState::Aborted)
            .map_err(map_store_error)?;
        self.store
            .update_attempt_state(&task.contract.id, &attempt.id, AttemptState::Cancelled)
            .map_err(map_store_error)?;
        Ok(())
    }

    fn list_sessions(&self, params: Value) -> Result<Value, RpcError> {
        let params: SessionsListParams = parse_params(params)?;
        let tasks = match params.task_id {
            Some(task_id) => vec![self.store.get(&task_id).map_err(map_store_error)?],
            None => self.store.list().map_err(map_store_error)?,
        };
        let sessions = tasks.into_iter().flat_map(|task| {
            let task_id = task.contract.id;
            task.sessions
                .into_iter()
                .map(move |session| json!({"task_id": &task_id, "session": session}))
        });
        Ok(Value::Array(sessions.collect()))
    }

    fn get_session(&self, params: Value) -> Result<Value, RpcError> {
        let params: SessionParams = parse_params(params)?;
        let task = self.store.get(&params.task_id).map_err(map_store_error)?;
        let session = task
            .sessions
            .into_iter()
            .find(|session| session.id == params.session_id)
            .ok_or(RpcError::NotFound)?;
        Ok(json!({"task_id": params.task_id, "session": session}))
    }

    fn session_events(&self, params: Value) -> Result<Value, RpcError> {
        let params: SessionEventsParams = parse_params(params)?;
        let cursor = params.after_sequence.unwrap_or(0);
        let latest = self
            .store
            .latest_event_sequence()
            .map_err(map_store_error)?;
        if cursor < 0 || cursor > latest {
            return Err(RpcError::InvalidParams);
        }
        let events = self
            .store
            .session_events_after(
                &params.task_id,
                &params.session_id,
                cursor,
                EVENT_PAGE_SIZE + 1,
            )
            .map_err(map_store_error)?;
        if events.len() > EVENT_PAGE_SIZE {
            return Err(RpcError::SlowConsumer);
        }
        Ok(json!({
            "task_id": params.task_id,
            "session_id": params.session_id,
            "events": events.into_iter().map(stream_event).collect::<Vec<_>>()
        }))
    }

    fn list_approvals(&self, params: Value) -> Result<Value, RpcError> {
        require_empty_params(&params)?;
        serde_json::to_value(self.store.pending_approvals().map_err(map_approval_error)?)
            .map_err(|_| RpcError::Internal)
    }

    fn resolve_approval(&self, params: Value) -> Result<Value, RpcError> {
        let request: ResolveApprovalParams = parse_params(params.clone())?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "approvals.resolve",
            &key,
            &params,
            || {
                let result: Approval = match request.decision.as_str() {
                    "approve" => self.store.approve_approval(
                        &request.approval_id,
                        &self.approval_actor,
                        &self.approval_allowlist,
                    ),
                    "reject" => self.store.reject_approval(
                        &request.approval_id,
                        &self.approval_actor,
                        &self.approval_allowlist,
                    ),
                    _ => return Err(RpcError::InvalidParams),
                }
                .map_err(map_approval_error)?;
                serde_json::to_value(result).map_err(|_| RpcError::Internal)
            },
        )
    }
}

impl DaemonApi {
    fn workspace_diff(&self, params: Value) -> Result<Value, RpcError> {
        let params: DiffParams = parse_params(params)?;
        let task = self.store.get(&params.task_id).map_err(map_store_error)?;
        let workspace = self.selected_workspace(&params.task_id, params.workspace_id.as_deref())?;
        let project = self
            .store
            .get_project(&task.contract.project_id)
            .map_err(map_store_error)?;
        let (_primary, worktree) = task_repositories(&project, &workspace)?;
        let diff = worktree
            .diff_from(
                &workspace.base_commit,
                params.path.as_deref().map(Path::new),
            )
            .map_err(map_git_error)?;
        if diff.len() > MAX_DIFF_BYTES {
            return Err(RpcError::ResultTooLarge);
        }
        Ok(json!({
            "task_id": params.task_id,
            "workspace_id": workspace.id,
            "diff": diff
        }))
    }

    fn selected_workspace(
        &self,
        task_id: &str,
        requested_id: Option<&str>,
    ) -> Result<Workspace, RpcError> {
        let workspaces = self
            .store
            .workspaces_for_task(task_id)
            .map_err(map_store_error)?;
        match requested_id {
            Some(id) => workspaces
                .into_iter()
                .find(|workspace| workspace.id == id)
                .ok_or(RpcError::NotFound),
            None => match workspaces.len() {
                0 => Err(RpcError::NotFound),
                1 => Ok(workspaces.into_iter().next().expect("one workspace")),
                _ => Err(RpcError::Conflict),
            },
        }
    }
}

fn task_repositories(
    project: &Project,
    workspace: &Workspace,
) -> Result<(Repository, Repository), RpcError> {
    let primary = Repository::new(Path::new(&project.root_path)).map_err(map_git_error)?;
    let worktree = Repository::new(Path::new(&workspace.path)).map_err(map_git_error)?;
    let primary_common =
        std::fs::canonicalize(primary.git_common_dir()).map_err(|_| RpcError::Internal)?;
    let worktree_common =
        std::fs::canonicalize(worktree.git_common_dir()).map_err(|_| RpcError::Internal)?;
    if primary.path() == worktree.path() || primary_common != worktree_common {
        return Err(RpcError::Forbidden);
    }
    Ok((primary, worktree))
}

impl DaemonApi {
    fn request_delivery(&self, params: Value) -> Result<Value, RpcError> {
        let request: DeliveryParams = parse_params(params.clone())?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "delivery.request",
            &key,
            &params,
            || self.perform_delivery(&request),
        )
    }

    fn perform_delivery(&self, request: &DeliveryParams) -> Result<Value, RpcError> {
        let task = self.store.get(&request.task_id).map_err(map_store_error)?;
        let action = match (task.contract.delivery, request.action.as_str()) {
            (DeliveryPolicy::ReportOnly, "report_only") => DeliveryAction::ReportOnly,
            (DeliveryPolicy::LocalReview, "local_review") => DeliveryAction::LocalReview,
            _ => return Err(RpcError::Forbidden),
        };
        let target_ref = match (action, request.target_ref.as_deref()) {
            (DeliveryAction::ReportOnly, None) => None,
            (DeliveryAction::LocalReview, Some(target_ref)) if !target_ref.is_empty() => {
                Some(target_ref)
            }
            _ => return Err(RpcError::InvalidParams),
        };
        let workspace =
            self.selected_workspace(&request.task_id, request.workspace_id.as_deref())?;
        let project = self
            .store
            .get_project(&task.contract.project_id)
            .map_err(map_store_error)?;
        let (primary, _) = task_repositories(&project, &workspace)?;
        let verification = self
            .store
            .verification_runs_for_task(&request.task_id)
            .map_err(map_store_error)?
            .into_iter()
            .find(|run| run.id == request.verification_run_id)
            .ok_or(RpcError::NotFound)?;
        let action_kind = match action {
            DeliveryAction::ReportOnly => ActionKind::ReportPublish,
            DeliveryAction::LocalReview => ActionKind::LocalReviewMerge,
        };
        let destination = primary.path().to_string_lossy().into_owned();
        let digest = delivery_digest(action, &destination, &workspace.base_commit, target_ref)?;
        let approval_action = ApprovalAction::new(
            ApprovalActionKind::Delivery(action_kind),
            ParameterDigest::from_sha256(digest),
            &verification.commit,
            &request.task_id,
        )
        .map_err(map_approval_error)?;
        let step_id = idempotency::stable_id("delivery", &request.idempotency_key);
        let delivery_request = DeliveryRequest {
            step_id: &step_id,
            task_id: &request.task_id,
            candidate_sha: &verification.commit,
            base_sha: &workspace.base_commit,
            approval_id: &request.approval_id,
            verification_run_id: &request.verification_run_id,
            parameter_digest: digest,
            worktree_path: Path::new(&workspace.path),
        };
        let step = self.execute_delivery(
            &primary,
            delivery_request,
            action,
            target_ref,
            &approval_action,
            &destination,
        )?;
        if step.outcome == DeliveryOutcome::Refused {
            return Err(RpcError::Forbidden);
        }
        Ok(json!({
            "task_id": request.task_id,
            "step_id": step.id,
            "outcome": step.outcome.as_str()
        }))
    }
}

impl DaemonApi {
    fn execute_delivery(
        &self,
        repository: &Repository,
        request: DeliveryRequest<'_>,
        action: DeliveryAction,
        target_ref: Option<&str>,
        approval_action: &ApprovalAction,
        destination: &str,
    ) -> Result<DeliveryStep, RpcError> {
        let check_approval = |check: &ApprovalCheck<'_>| -> Result<(), CallbackError> {
            let action_kind = match check.action {
                DeliveryAction::ReportOnly => ActionKind::ReportPublish,
                DeliveryAction::LocalReview => ActionKind::LocalReviewMerge,
            };
            self.store
                .check_delivery_approval(
                    check.approval_id,
                    check.task_id,
                    action_kind,
                    ParameterDigest::from_sha256(*check.parameter_digest),
                    check.candidate_sha,
                )
                .map_err(|_| callback_failure())
        };
        let check_verification = |check: &VerificationCheck<'_>| -> Result<(), CallbackError> {
            let verified = self
                .store
                .delivery_verification_matches(check.task_id, check.run_id, check.candidate_sha)
                .map_err(|_| callback_failure())?;
            if verified {
                Ok(())
            } else {
                Err(callback_failure())
            }
        };
        let persist_step = |step: &DeliveryStep| -> Result<(), CallbackError> {
            let outcome = match step.outcome {
                DeliveryOutcome::Reported => StoredDeliveryOutcome::Reported,
                DeliveryOutcome::ReviewRefCreated => StoredDeliveryOutcome::ReviewRefCreated,
                DeliveryOutcome::ReviewRefAlreadyMatches => {
                    StoredDeliveryOutcome::ReviewRefAlreadyMatches
                }
                DeliveryOutcome::Refused => StoredDeliveryOutcome::Refused {
                    detail: step
                        .detail
                        .clone()
                        .unwrap_or_else(|| "delivery refused".to_owned()),
                },
            };
            self.store
                .record_delivery_step(&DeliveryStepRecord {
                    id: &step.id,
                    approval_id: &step.approval_id,
                    verification_run_id: &step.verification_run_id,
                    action: approval_action,
                    destination,
                    target_ref: step.target_ref.as_deref(),
                    outcome,
                    diff_summary: step.diff_summary.as_deref(),
                })
                .map_err(|_| callback_failure())
        };
        let callbacks = DeliveryCallbacks {
            check_approval: &check_approval,
            check_verification: &check_verification,
            persist_step: &persist_step,
        };
        match action {
            DeliveryAction::ReportOnly => delivery::report_only(repository, request, &callbacks),
            DeliveryAction::LocalReview => delivery::local_review(
                repository,
                request,
                target_ref.ok_or(RpcError::InvalidParams)?,
                &callbacks,
            ),
        }
        .map_err(map_delivery_error)
    }
}

fn parse_params<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RpcError> {
    serde_json::from_value(params).map_err(|_| RpcError::InvalidParams)
}

fn require_empty_params(params: &Value) -> Result<(), RpcError> {
    if params.is_null() || params.as_object().is_some_and(|object| object.is_empty()) {
        Ok(())
    } else {
        Err(RpcError::InvalidParams)
    }
}

fn stream_event(event: orchestrator_store::StreamEvent) -> IpcEvent {
    IpcEvent {
        sequence: event.sequence,
        task_id: event.task_id,
        session_id: event.session_id,
        kind: event.kind,
        payload: event.payload,
        created_at: event.created_at,
    }
}

fn delivery_digest(
    action: DeliveryAction,
    destination: &str,
    base_sha: &str,
    target_ref: Option<&str>,
) -> Result<[u8; 32], RpcError> {
    let parameters = json!({
        "action": action.as_str(),
        "destination": destination,
        "base_sha": base_sha,
        "target_ref": target_ref,
    });
    let encoded = serde_json::to_vec(&parameters).map_err(|_| RpcError::Internal)?;
    Ok(Sha256::digest(encoded).into())
}

fn callback_failure() -> CallbackError {
    Box::new(std::io::Error::other("delivery policy check failed"))
}

fn map_store_error(error: StoreError) -> RpcError {
    match error {
        StoreError::NotFound(_)
        | StoreError::RecordNotFound { .. }
        | StoreError::NoAttempt { .. }
        | StoreError::NoSession { .. } => RpcError::NotFound,
        StoreError::InvalidContract(_)
        | StoreError::InvalidTaskId(_)
        | StoreError::InvalidEventCursor
        | StoreError::InvalidEventLimit
        | StoreError::InvalidIdempotencyKey => RpcError::InvalidParams,
        StoreError::AlreadyExists(_) | StoreError::IllegalTransition(_) => RpcError::Conflict,
        _ => RpcError::Internal,
    }
}

fn map_approval_error(error: orchestrator_core::approval::ApprovalError) -> RpcError {
    use orchestrator_core::approval::ApprovalError;
    match error {
        ApprovalError::InvalidApprovalId
        | ApprovalError::InvalidTaskId
        | ApprovalError::InvalidCandidateCommit
        | ApprovalError::InvalidActor => RpcError::InvalidParams,
        ApprovalError::NotFound { .. } => RpcError::NotFound,
        ApprovalError::UnauthorizedActor { .. }
        | ApprovalError::UnauthorizedPolicyChange { .. }
        | ApprovalError::AgentCannotDecide { .. }
        | ApprovalError::NotApproved { .. }
        | ApprovalError::ActionMismatch(_) => RpcError::Forbidden,
        ApprovalError::IllegalTransition { .. } => RpcError::Conflict,
        ApprovalError::Persistence(_) => RpcError::Internal,
    }
}

fn map_git_error(error: orchestrator_git::Error) -> RpcError {
    match error {
        orchestrator_git::Error::DiffPathOutsideWorktree => RpcError::Forbidden,
        orchestrator_git::Error::InvalidDiffPath | orchestrator_git::Error::InvalidDiffBase => {
            RpcError::InvalidParams
        }
        orchestrator_git::Error::InvalidBaseRef { .. } => RpcError::InvalidParams,
        _ => RpcError::Internal,
    }
}

fn map_delivery_error(error: orchestrator_git::delivery::DeliveryError) -> RpcError {
    use orchestrator_git::delivery::DeliveryError;
    match error {
        DeliveryError::Approval(_) | DeliveryError::Verification(_) => RpcError::Forbidden,
        DeliveryError::InvalidRequest
        | DeliveryError::InvalidCandidateSha(_)
        | DeliveryError::InvalidBaseSha(_)
        | DeliveryError::InvalidBaseRef(_)
        | DeliveryError::InvalidReviewRef(_) => RpcError::InvalidParams,
        DeliveryError::ReviewRefConflict { .. } => RpcError::Conflict,
        DeliveryError::DifferentRepository { .. } | DeliveryError::PrimaryCheckoutCandidate => {
            RpcError::Forbidden
        }
        _ => RpcError::Internal,
    }
}

#[cfg(unix)]
fn terminate_process_group(identity: ProcessIdentity) -> Result<(), RpcError> {
    if identity.process_group != Some(identity.pid) || !process_is_alive(identity)? {
        return Err(RpcError::Forbidden);
    }
    let group = i32::try_from(identity.pid).map_err(|_| RpcError::Forbidden)?;
    // SAFETY: persisted PID, group, and start identity were rechecked immediately before signal.
    // Updated: 2026-11-08T17:33:37+00:00.
    if unsafe { libc::kill(-group, libc::SIGTERM) } != 0 && process_is_alive(identity)? {
        return Err(RpcError::Forbidden);
    }
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline && process_is_alive(identity)? {
        thread::sleep(Duration::from_millis(20));
    }
    if process_is_alive(identity)? {
        // SAFETY: process identity was rechecked after the grace period.
        if unsafe { libc::kill(-group, libc::SIGKILL) } != 0 && process_is_alive(identity)? {
            return Err(RpcError::Forbidden);
        }
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline && process_is_alive(identity)? {
            thread::sleep(Duration::from_millis(20));
        }
    }
    if process_is_alive(identity)? {
        Err(RpcError::Forbidden)
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn process_is_alive(identity: ProcessIdentity) -> Result<bool, RpcError> {
    match observe_process(identity) {
        ProcessObservation::Exited => Ok(false),
        ProcessObservation::Alive(observed)
            if classify_identity(identity, observed) == ProcessClassification::Alive =>
        {
            Ok(true)
        }
        _ => Err(RpcError::Forbidden),
    }
}

#[cfg(not(unix))]
fn terminate_process_group(_identity: ProcessIdentity) -> Result<(), RpcError> {
    Err(RpcError::Forbidden)
}
#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;
