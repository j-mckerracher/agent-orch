//! Authenticated project, task, session, approval, workspace, and delivery API.
//!
//! Wire parameters are parsed once at the RPC boundary. Results omit repository
//! and worktree paths; durable mutations use the shared idempotency executor.
//! Updated: 2026-11-08T12:21:37-07:00.

use std::path::{Component, Path};
use std::sync::{Arc, Mutex, TryLockError};
use std::time::{Duration, Instant};

use orchestrator_agents::{
    AgentAdapter, ClaudeCodeAdapter, CodexAdapter, CopilotAdapter, PiRpcAdapter, SignInRecord,
    COPILOT_AGENT,
};
use orchestrator_core::approval::{
    Actor, Approval, ApprovalAction, ApprovalActionKind, ApproverAllowlist, ParameterDigest,
};
use orchestrator_core::authorization::ActionKind;
use orchestrator_core::contract::{
    self, AutonomyPolicy, DeliveryPolicy, ModelSelection, TaskContract, TaskKind,
    VerificationPolicy,
};
use orchestrator_core::identity::ProcessClassification;
use orchestrator_core::records::{Project, Workspace};
use orchestrator_core::scheduler::{Scheduler, SchedulerLimits, SchedulerSnapshot, TaskSnapshot};
use orchestrator_core::task::{AttemptState, TaskEvent, TaskState};
use orchestrator_git::delivery::{
    self, ApprovalCheck, CallbackError, DeliveryAction, DeliveryCallbacks, DeliveryOutcome,
    DeliveryRequest, DeliveryStep, VerificationCheck,
};
use orchestrator_git::github::GitHub;
use orchestrator_git::Repository;
use orchestrator_ipc::{CallContext, Capability, IpcEvent, RpcError, RpcHandler};
use orchestrator_runtime::reconcile::observe_owner_process;
use orchestrator_store::{
    ArchiveError, CancellationStatus, DeliveryOutcome as StoredDeliveryOutcome, DeliveryStepRecord,
    MergeState, MessageAuthor, SqliteStore, Store, StoreError, WaitError,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use orchestrator_core::permission::{PermissionLevel, TASK_DEFAULT_PERMISSION};

use crate::executor::{
    agent_command, placeholder_status_dir, serving, submit_channel, ChannelSupport, Executor,
    ExecutorConfig, StopGate, COPILOT_RUNS_DIR,
};
use crate::idempotency;
use orchestrator_diagnostics::Level;

const MAX_DIFF_BYTES: usize = 512 * 1024;
const EVENT_PAGE_SIZE: usize = 256;
const LOCAL_USER: &str = "local-user";
/// Bound for the explicit Copilot sign-in check, which is one real model call.
const COPILOT_SIGN_IN_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CopilotSignInParams {
    #[serde(default)]
    force: bool,
}

pub(crate) struct DaemonApi {
    store: Arc<SqliteStore>,
    /// Launch settings, also used to detect agents for `agents.capabilities`.
    config: ExecutorConfig,
    /// Absent for API-only use (tests); `main` always starts one.
    executor: Option<Executor>,
    idempotency_gate: Mutex<()>,
    approval_actor: Actor,
    approval_allowlist: ApproverAllowlist,
    host: host::HostService,
    /// AI usage from local coding tools (DPES-1105); opt-in, read in the background.
    local_usage: orchestrator_local_usage::LocalUsageService,
    /// M7-08 conversation service (global and per-workstream).
    conversations: conversation_runtime::ConversationService,
    /// Short-lived reuse of definitive agent catalog reads (see `api_models`).
    catalog_cache: Arc<models::CatalogCache>,
    /// The wake scheduler (DPES-1105 phase 3). Absent without an executor, as in tests that
    /// build their own with a manual clock.
    wake: Option<Arc<wake::WakeService>>,
    /// The standing-grant matcher thread (DPES-1105 phase 5). Started by `main` once the API is
    /// in an `Arc`; stopped by `shutdown`.
    grant_matcher: Mutex<Option<grant_matcher::GrantMatcherHandle>>,
    grant_try_state: Mutex<grant_matcher::TryState>,
    /// The only door to a remote forge (DPES-1105 phase 4c). `GhCli` in the daemon; every test
    /// passes a `FakeGitHub`.
    github: Arc<dyn GitHub>,
    /// The pull request poller and merge worker: its thread, its signal and its in-memory
    /// schedule. Started by `main`; stopped by `shutdown`.
    landing: landing::LandingState,
    /// Closed by an accepted idle shutdown. Requests, the scheduler and the background passes hold
    /// a read guard while they work, so none can start after it closes.
    stop_gate: StopGate,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TaskSubmitParams {
    idempotency_key: String,
    contract: TaskSubmission,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TaskSubmission {
    project_id: String,
    #[serde(default)]
    parent_id: Option<String>,
    objective: String,
    kind: TaskKind,
    /// Optional when a work class or the saved dispatch defaults set the agent.
    #[serde(default)]
    agent: String,
    dependencies: Vec<String>,
    verification: VerificationPolicy,
    delivery: DeliveryPolicy,
    autonomy: AutonomyPolicy,
    max_retries: u32,
    max_repairs: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model_selection: Option<ModelSelection>,
    /// The permission level (DPES-1105). Absent means [`TASK_DEFAULT_PERMISSION`], and an
    /// absent field is omitted from the request, so its idempotency key is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    permission: Option<PermissionLevel>,
    /// A work class (DPES-677) whose settings fill in the agent, model and effort the
    /// task leaves unset. Omitted when absent, so its idempotency key is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    work_class: Option<String>,
    /// The confirmed intake record (DPES-673). Refused unless it has criteria and every
    /// one is confirmed. Omitted when absent, so its idempotency key is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    work_item: Option<orchestrator_core::work_item::WorkItem>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisterProjectParams {
    path: String,
}

/// An archived project (`project.remove`) is left out of `projects.list` unless the
/// caller asks for it.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectsListParams {
    #[serde(default)]
    include_archived: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskIdParams {
    task_id: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CancelParams {
    task_id: String,
    idempotency_key: String,
}

/// A task and an idempotency key: `tasks.discard`, `tasks.force_start`.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TaskKeyParams {
    task_id: String,
    idempotency_key: String,
}

/// A task, the user's words for it and an idempotency key: `tasks.message`.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct UserMessageParams {
    task_id: String,
    text: String,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ArchiveParams {
    task_ids: Vec<String>,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RetryParams {
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
    epoch: Option<String>,
}

#[derive(Deserialize, Serialize)]
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

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DeliveryParams {
    task_id: String,
    #[serde(default)]
    workspace_id: Option<String>,
    approval_id: String,
    verification_run_id: String,
    action: String,
    #[serde(default)]
    target_ref: Option<String>,
    idempotency_key: String,
}

impl DaemonApi {
    /// Opens the store without launching anything.
    #[cfg(test)]
    pub(crate) fn new(data_dir: &Path) -> Result<Self, String> {
        Self::with_github(
            data_dir,
            Arc::new(orchestrator_git::github_fake::FakeGitHub::new()),
        )
    }

    /// [`Self::new`] with the GitHub double a test scripts. No test builds a real `GhCli`.
    #[cfg(test)]
    pub(crate) fn with_github(data_dir: &Path, github: Arc<dyn GitHub>) -> Result<Self, String> {
        Self::with_config(data_dir, ExecutorConfig::for_data_dir(data_dir), github)
    }

    fn with_config(
        data_dir: &Path,
        config: ExecutorConfig,
        github: Arc<dyn GitHub>,
    ) -> Result<Self, String> {
        let store = Arc::new(
            SqliteStore::new(&data_dir.join("store.sqlite"))
                .map_err(|_| "cannot open daemon store".to_owned())?,
        );
        home_directory::clear_stale_in_place_flags(&store)
            .map_err(|_| "cannot clear stale in-place flags".to_owned())?;
        // The saved limits apply before anything can start a task.
        let settings = store
            .orchestrator_settings()
            .map_err(|_| "cannot read orchestrator settings".to_owned())?;
        config.load.set_limits(settings.load);
        let approval_actor = Actor::User(LOCAL_USER.to_owned());
        let approval_allowlist =
            ApproverAllowlist::new(LOCAL_USER, std::iter::empty::<String>())
                .map_err(|_| "cannot initialize local approval policy".to_owned())?;
        let catalog_cache: Arc<models::CatalogCache> = Arc::default();
        let runtime = conversation_agent::AgentRuntime::new(
            config.clone(),
            catalog_cache.clone(),
            conversation_agent::ConversationConfig::for_data_dir(data_dir),
            store.clone(),
        );
        let pinned = !runtime.pins().is_empty();
        let conversations = conversation_runtime::ConversationService::new(
            store.clone(),
            Arc::new(runtime),
            Arc::new(conversation_agent::StoreWorkstreams(store.clone())),
            pinned,
            config.load.clone(),
        );
        conversations.recover()?;
        let feed = config.load_feed.then(|| config.load.clone());
        let host = host::HostService::new(data_dir, feed);
        let api = Self {
            store,
            config,
            executor: None,
            idempotency_gate: Mutex::new(()),
            approval_actor,
            approval_allowlist,
            host,
            local_usage: usage_local::start_local_usage(data_dir),
            conversations,
            catalog_cache,
            wake: None,
            grant_matcher: Mutex::new(None),
            grant_try_state: Mutex::new(grant_matcher::TryState::default()),
            github,
            landing: landing::LandingState::default(),
            stop_gate: StopGate::default(),
        };
        api.reconcile_actions();
        Ok(api)
    }

    /// Opens the store and starts the executor, which reconciles the store
    /// with the OS before it launches anything.
    pub(crate) fn with_executor(data_dir: &Path, config: ExecutorConfig) -> Result<Self, String> {
        // Only the daemon that holds the data-directory lock may do this, and
        // only before it starts any turn of its own.
        conversation_orphans::reap(
            &conversation_agent::ConversationConfig::for_data_dir(data_dir).agents,
        );
        let github: Arc<dyn GitHub> = Arc::new(orchestrator_git::github::GhCli::from_env());
        let mut api = Self::with_config(data_dir, config.clone(), github)?;
        let executor = Executor::start_gated(
            api.store.clone(),
            config,
            api.stop_gate.clone(),
            api.conversations.board(),
        )?;
        // Recovery settles the batches a crash left running before the loop starts, so a
        // restart never makes a second turn for one record.
        let wake = Arc::new(
            wake::WakeService::new(
                api.store.clone(),
                api.conversations.clone(),
                executor.board(),
                Arc::new(wake::SystemClock),
                api.config.load.clone(),
                api.config.diagnostics.clone(),
            )
            .with_stop_gate(api.stop_gate.clone()),
        );
        // A transient store error is logged inside and does not stop startup.
        let _ = wake.recover();
        wake.start();
        api.executor = Some(executor);
        api.wake = Some(wake);
        api.host.start();
        Ok(api)
    }

    pub(crate) fn shutdown(&self) {
        self.stop_landing();
        self.stop_grant_matcher();
        self.conversations.shutdown();
        self.local_usage.shutdown();
        if let Some(wake) = &self.wake {
            wake.stop();
        }
        if let Some(executor) = &self.executor {
            executor.shutdown();
        }
    }

    fn wake_executor(&self) {
        if let Some(executor) = &self.executor {
            executor.wake();
        }
    }

    /// Runs one request and writes the causes of any unexpected (`Internal`)
    /// failures it hit to the injected diagnostics sink.
    pub(crate) fn report_internal<T>(&self, call: impl FnOnce() -> T) -> T {
        INTERNAL_CAUSES.with(|causes| causes.borrow_mut().clear());
        let output = call();
        let causes = INTERNAL_CAUSES.with(|causes| std::mem::take(&mut *causes.borrow_mut()));
        for (source, cause) in &causes {
            self.config.diagnostics.error(
                "api",
                "internal_error",
                &[("source", source), ("cause", cause)],
            );
        }
        output
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
            Capability::Agents,
            Capability::Coordinator,
            Capability::Review,
            Capability::Host,
            Capability::Queue,
            Capability::Workstreams,
            Capability::ConversationHistory,
            Capability::ConversationSelection,
            Capability::ProjectConversations,
            Capability::PermissionLevels,
            Capability::OrchestratorActions,
            Capability::Settings,
            Capability::Monitoring,
            Capability::Holds,
            Capability::StandingApprovals,
            Capability::Landing,
        ]
    }

    fn dispatch(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.dispatch_in(method, params, &CallContext::NONE)
    }

    fn dispatch_in(
        &self,
        method: &str,
        params: Value,
        context: &CallContext<'_>,
    ) -> Result<Value, RpcError> {
        let Some(_serving) = serving(&self.stop_gate) else {
            return Err(RpcError::Conflict);
        };
        let result = self.report_internal(|| match method {
            "projects.list" => self.projects_list(params),
            "projects.register" => self.register_project(params),
            "projects.get_defaults" => self.projects_get_defaults(params),
            "projects.set_defaults" => self.projects_set_defaults(params),
            "intake.normalize" => self.intake_normalize(params),
            "tasks.submit" => self.submit_task(params),
            "tasks.list" => self.list_tasks(params),
            "tasks.get" => self.get_task(params),
            "tasks.cancel" => self.cancel_task(params),
            "tasks.archive" => self.archive_tasks(params),
            "tasks.discard" => self.discard_task(params),
            "tasks.message" => self.message_task(params),
            "tasks.retry" => self.retry_task(params),
            "tasks.force_start" => self.force_start_task(params),
            "agents.capabilities" => self.agent_capabilities(params),
            "agents.catalog" => self.agents_catalog(params),
            "agents.validate_selection" => self.agents_validate_selection(params),
            "agents.check_copilot_sign_in" => self.check_copilot_sign_in(params),
            "sessions.list" => self.list_sessions(params),
            "sessions.get" => self.get_session(params),
            "sessions.events" => self.session_events(params),
            "sessions.output_tail" => self.session_output_tail(params),
            "sessions.thinking" => self.session_thinking(params),
            "sessions.activity" => self.session_activity(params),
            "approvals.list" => self.list_approvals(params),
            "approvals.resolve" => self.resolve_approval(params),
            "workspaces.diff" => self.workspace_diff(params),
            "delivery.request" => self.request_delivery(params),
            "coordinator.send_message" => self.coordinator_send_message(params),
            "coordinator.propose" => self.coordinator_propose(params),
            "coordinator.get" => self.coordinator_get(params),
            "coordinator.clear" => self.coordinator_clear(params),
            "coordinator.remove" => self.coordinator_remove(params),
            "coordinator.restore" => self.coordinator_restore(params),
            "coordinator.set_scope" => self.coordinator_set_scope(params),
            "conversations.send" => {
                self.conversations_send(params, context.negotiated(Capability::OrchestratorActions))
            }
            "conversations.get" => {
                self.conversations_get(params, context.negotiated(Capability::OrchestratorActions))
            }
            "conversations.cancel" => self.conversations_cancel(params),
            "conversations.clear" => self.conversations_clear(params),
            "review.request_approval" => self.review_request_approval(params),
            "review.evidence" => self.review_evidence(params),
            "review.request_changes" => self.review_request_changes(params),
            "host.metrics" => self.host_metrics(params),
            "settings.get" => self.settings_get(params),
            "settings.set" => self.settings_set(params),
            "monitoring.get" => self.monitoring_get(params),
            "monitoring.set" => self.monitoring_set(params),
            "monitoring.ack" => self.monitoring_ack(params),
            "monitoring.brief" => self.monitoring_brief(params),
            "monitoring.brief_ack" => self.monitoring_brief_ack(params),
            "queue.snapshot" => self.queue_snapshot(params),
            "usage.summary" => self.usage_summary(params),
            "usage.localTools" => self.local_usage_read(params),
            "usage.localTools.setEnabled" => self.local_usage_set_enabled(params),
            "usage.localTools.refresh" => self.local_usage_refresh(params),
            "settings.workClasses.get" => self.work_classes_get(params),
            "settings.workClasses.set" => self.work_classes_set(params),
            "workstreams.create" => self.workstreams_create(params),
            "workstreams.list" => self.workstreams_list(params),
            "workstreams.get" => self.workstreams_get(params),
            "workstreams.update" => self.workstreams_update(params),
            "workstreams.prioritize" => self.workstreams_prioritize(params),
            "workstreams.add_objective" => self.workstreams_add_objective(params),
            "workstreams.revise_objective" => self.workstreams_revise_objective(params),
            "workstreams.set_gate" => self.workstreams_set_gate(params),
            "workstreams.link_task" => self.workstreams_link_task(params),
            "workstreams.unlink_task" => self.workstreams_unlink_task(params),
            "workstreams.add_task" => self.workstreams_add_task(params),
            "workstreams.split_task" => self.workstreams_split_task(params),
            "workstreams.combine_tasks" => self.workstreams_combine_tasks(params),
            "workstreams.remove_task" => self.workstreams_remove_task(params),
            "workstreams.reassign_task" => self.workstreams_reassign_task(params),
            "workstreams.set_task_policy" => self.workstreams_set_task_policy(params),
            "actions.resolve" => self.actions_resolve(params),
            "actions.pending" => self.actions_pending(params),
            "holds.set" => self.holds_set(params),
            "holds.release" => self.holds_release(params),
            "holds.list" => self.holds_list(params),
            "decisions.list" => self.decisions_list(params),
            "decisions.answer" => self.decisions_answer(params),
            "decisions.defer" => self.decisions_defer(params),
            "decisions.reconcile" => self.decisions_reconcile(params),
            "grants.create" => self.grants_create(params),
            "grants.revoke" => self.grants_revoke(params),
            "grants.list" => self.grants_list(params),
            "memory.list" => self.memory_list(params),
            "memory.set" => self.memory_set(params),
            "memory.remove" => self.memory_remove(params),
            "memory.restore" => self.memory_restore(params),
            "reports.list" => self.reports_list(params),
            "reports.get" => self.reports_get(params),
            "reports.relay" => {
                self.reports_relay(params, context.negotiated(Capability::OrchestratorActions))
            }
            "pulls.list" => self.pulls_list(params),
            "pulls.refresh" => self.pulls_refresh(params),
            _ => Err(RpcError::InvalidParams),
        });
        // Only the method and the error class are reported, never parameters.
        // Caller mistakes (invalid params, unknown ids) stay quiet.
        if let Err(error) = &result {
            let level = match error {
                RpcError::InvalidParams | RpcError::NotFound | RpcError::ParentNotFound => None,
                RpcError::Internal => Some(Level::Error),
                _ => Some(Level::Warn),
            };
            if let Some(level) = level {
                self.config.diagnostics.emit(
                    level,
                    "api",
                    "rpc_failed",
                    &[("method", method), ("error", &format!("{error:?}"))],
                );
            }
        }
        result
    }

    fn events_after(&self, after_sequence: i64, limit: usize) -> Result<Vec<IpcEvent>, RpcError> {
        self.report_internal(|| {
            self.store
                .stream_events_after(after_sequence, limit)
                .map(|events| events.into_iter().map(stream_event).collect())
                .map_err(map_store_error)
        })
    }

    fn latest_event_sequence(&self) -> Result<i64, RpcError> {
        self.report_internal(|| self.store.latest_event_sequence().map_err(map_store_error))
    }

    fn journal_epoch(&self) -> Result<Option<String>, RpcError> {
        self.report_internal(|| self.store.store_epoch().map(Some).map_err(map_store_error))
    }

    /// Closes the admission gate when nothing runs or can start. `try_write` fails while any
    /// request or acting background pass holds a read guard, which counts as busy; a poisoned
    /// gate is recovered as `serving` does.
    fn try_begin_idle_shutdown(&self) -> bool {
        let mut closed = match self.stop_gate.try_write() {
            Ok(closed) => closed,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => return false,
        };
        if *closed {
            return true;
        }
        let Ok(tasks) = self.store.list() else {
            return false;
        };
        let busy = tasks
            .iter()
            .any(|task| matches!(task.state, TaskState::Running | TaskState::Verifying))
            || self.conversations.has_active_turns()
            || self
                .executor
                .as_ref()
                .is_some_and(|executor| executor.has_pending_pass() || executor.has_live_workers());
        if busy {
            return false;
        }
        *closed = true;
        true
    }

    fn keep_alive(&self) -> bool {
        if self
            .executor
            .as_ref()
            .is_some_and(|executor| executor.has_pending_pass())
        {
            return true;
        }
        self.store.list().map_or(true, |tasks| {
            tasks
                .iter()
                .any(|task| matches!(task.state, TaskState::Running | TaskState::Verifying))
        })
    }
}

impl DaemonApi {
    fn projects_list(&self, params: Value) -> Result<Value, RpcError> {
        let request: ProjectsListParams = if params.is_null() {
            ProjectsListParams::default()
        } else {
            parse_params(params)?
        };
        let archived = if request.include_archived {
            Vec::new()
        } else {
            self.store.archived_project_ids().map_err(map_store_error)?
        };
        let projects = self.store.list_projects().map_err(map_store_error)?;
        // `coordinator_active` is true only after the project's coordinator was added
        // with `coordinator.restore`: registration does not create one. The project
        // itself is listed either way.
        let without_coordinator = self
            .store
            .projects_without_coordinator()
            .map_err(map_store_error)?;
        // `mode` appears only for a project that has a defaults row, `archived` only (as true)
        // for an archived one, and `in_place` only (as true) for one whose tasks run in its
        // directory, so a project that never set a default lists as before.
        let modes: std::collections::HashMap<String, String> = self
            .store
            .list_project_defaults()
            .map_err(map_store_error)?
            .into_iter()
            .map(|row| (row.project_id, row.mode))
            .collect();
        let archived_all = self.store.archived_project_ids().map_err(map_store_error)?;
        Ok(Value::Array(
            projects
                .into_iter()
                .filter(|project| !archived.contains(&project.id))
                .map(|project| {
                    let coordinator_active = !without_coordinator.contains(&project.id);
                    let mut row =
                        json!({"id": project.id, "coordinator_active": coordinator_active});
                    if let Some(mode) = modes.get(&project.id) {
                        row["mode"] = json!(mode);
                    }
                    if archived_all.contains(&project.id) {
                        row["archived"] = json!(true);
                    }
                    if project.is_in_place() {
                        row["in_place"] = json!(true);
                    }
                    row
                })
                .collect(),
        ))
    }

    /// Registers a Git work tree. The id is the canonical work-tree root, the
    /// same identity the CLI uses, so both front ends share one project row.
    /// Re-registering the same root returns the existing project.
    fn register_project(&self, params: Value) -> Result<Value, RpcError> {
        let params: RegisterProjectParams = parse_params(params)?;
        let requested = Path::new(&params.path);
        if !requested.is_absolute() {
            return Err(RpcError::InvalidParams);
        }
        let repository = Repository::new(requested).map_err(|error| match error {
            orchestrator_git::Error::NotARepository { .. } => RpcError::InvalidParams,
            other => map_git_error(other),
        })?;
        let root = repository.path().display().to_string();
        let _guard = self
            .idempotency_gate
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        match self.store.get_project(&root) {
            // Registering a removed project again is the undo of `project.remove`.
            Ok(project) if project.root_path == root => {
                self.store.restore_project(&root).map_err(map_store_error)?;
            }
            Ok(_) => return Err(RpcError::Conflict),
            Err(StoreError::RecordNotFound { .. }) => self
                .store
                .register_project(&Project {
                    id: root.clone(),
                    root_path: root.clone(),
                    settings: json!({}),
                })
                .map_err(map_store_error)?,
            Err(error) => return Err(map_store_error(error)),
        }
        Ok(json!({"id": root}))
    }

    fn submit_task(&self, params: Value) -> Result<Value, RpcError> {
        let mut request: TaskSubmitParams = parse_params(params)?;
        if !request.contract.agent.trim().is_empty() {
            request.contract.agent = canonical_agent_id(&request.contract.agent)
                .ok_or(RpcError::InvalidParams)?
                .to_owned();
        }
        let key = request.idempotency_key.clone();
        if let Some(reply) = idempotency::replay(&self.store, "tasks.submit", &key, &request)? {
            return Ok(reply);
        }
        // An unconfirmed work item is invalid regardless of CLI availability.
        if let Some(item) = &request.contract.work_item {
            contract::validate_work_item(item).map_err(|_| RpcError::InvalidParams)?;
        }
        let (resolved, selection_catalog) = self.resolve_submission(&request.contract)?;
        // The fingerprint is the request as sent; the write sees what it resolved to.
        let resolved = TaskSubmitParams {
            idempotency_key: key.clone(),
            contract: resolved,
        };
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "tasks.submit",
            &key,
            &request,
            || self.submit_task_inner(&resolved, &key, selection_catalog.as_ref()),
        )
    }

    /// Fills in work-class and saved dispatch defaults, and reads the catalog the
    /// selection is checked against. The catalog read can take as long as a CLI probe, so a
    /// caller does this before the idempotency gate is taken: the gate serializes every
    /// mutating request and must not wait on a subprocess.
    fn resolve_submission(
        &self,
        submission: &TaskSubmission,
    ) -> Result<(TaskSubmission, Option<orchestrator_agents::AgentCatalog>), RpcError> {
        // Both metadata reads must fit under the 2 s IPC deadline.
        let deadline = Instant::now() + Duration::from_millis(1700);
        let resolved = self.resolve_dispatch_defaults(submission)?;
        let selection_catalog = self
            .selection_catalog(&resolved.agent, resolved.model_selection.as_ref())
            .inspect_err(|_| {
                if let Some(class) = &resolved.work_class {
                    eprintln!(
                        "orchestrator-daemon: work class {} could not be checked against the catalog",
                        crate::executor::log_safe(class)
                    );
                }
            })?;
        // A store-only API has no task launcher and does not probe host executables.
        if self.executor.is_some() {
            self.check_submission_cli(
                &resolved,
                deadline.saturating_duration_since(Instant::now()),
            )?;
        }
        Ok((resolved, selection_catalog))
    }

    fn check_submission_cli(
        &self,
        submission: &TaskSubmission,
        budget: Duration,
    ) -> Result<(), RpcError> {
        use orchestrator_agents::cli_compat::{CompatibilityError, Executable};
        let permission = submission.permission.unwrap_or(TASK_DEFAULT_PERMISSION);
        // An agent without a daemon launcher has no CLI to check, so it is accepted before its
        // executable is looked up or read.
        let status_dir = placeholder_status_dir(&self.config);
        if agent_command(
            &self.config,
            &submission.agent,
            "objective",
            submission.model_selection.as_ref(),
            permission,
            None,
            submit_channel(&status_dir, ChannelSupport::ALL),
        )
        .is_none()
        {
            return Ok(());
        }
        // Keep accepting work while a CLI is not installed. The executor
        // checks again before any launch, including after an update or retry.
        let Some(program) = self.config.locate_agent(&submission.agent) else {
            return Ok(());
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| RpcError::Internal)?;
        let result = match Executable::read(&program) {
            Ok(executable) => runtime.block_on(async {
                tokio::time::timeout(budget, async {
                    // Measures the argv the launch will use, so a CLI without the agent flags is
                    // checked plain, as the launch will run it.
                    let (workflow_agent, _) = self
                        .config
                        .offered_workflow_agent(
                            &submission.agent,
                            &program,
                            &executable,
                            (self.config.locate_workflow_agent)(),
                        )
                        .await;
                    // The channel flags the CLI lacks are left out, as the launch leaves them.
                    let support = self
                        .config
                        .offered_channel(&submission.agent, &program, &executable)
                        .await;
                    let Some((_, args)) = agent_command(
                        &self.config,
                        &submission.agent,
                        "objective",
                        submission.model_selection.as_ref(),
                        permission,
                        workflow_agent.as_ref(),
                        submit_channel(&status_dir, support),
                    ) else {
                        return Ok(());
                    };
                    self.config
                        .check_launch(&submission.agent, &program, &executable, &args)
                        .await
                })
                .await
                .unwrap_or(Err(CompatibilityError::Unconfirmed))
            }),
            Err(_) => Err(CompatibilityError::Unconfirmed),
        };
        result.map_err(|error| {
            self.config.diagnostics.warn(
                "api",
                "task_submit_cli_compatibility_refused",
                &[
                    ("agent", &submission.agent),
                    ("permission", permission.as_str()),
                    ("reason", &error.to_string()),
                ],
            );
            RpcError::Conflict
        })
    }

    /// Explicit choices and named work classes win. A different explicit model does not
    /// inherit an effort configured for another model.
    fn resolve_dispatch_defaults(
        &self,
        submission: &TaskSubmission,
    ) -> Result<TaskSubmission, RpcError> {
        let work_classes::Resolved {
            agent,
            selection,
            class,
        } = work_classes::resolve(
            &self.config.data_dir,
            submission.work_class.as_deref(),
            &submission.agent,
            submission.model_selection.as_ref(),
        )?;
        let defaults = self
            .store
            .orchestrator_settings()
            .map_err(map_store_error)?
            .dispatch_defaults;
        let agent = if agent.trim().is_empty() {
            defaults.default_agent.as_deref().unwrap_or("")
        } else {
            &agent
        };
        let agent = canonical_agent_id(agent)
            .ok_or(RpcError::InvalidParams)?
            .to_owned();
        let selection = match (selection, defaults.providers.get(&agent)) {
            (None, saved) => saved.cloned(),
            (Some(mut chosen), Some(saved))
                if chosen.model.is_some() || chosen.effort.is_some() =>
            {
                if chosen.model.is_none() {
                    chosen.model = saved.model.clone();
                }
                if chosen.effort.is_none() && chosen.model == saved.model {
                    chosen.effort = saved.effort.clone();
                }
                Some(chosen)
            }
            (chosen, _) => chosen,
        };
        let mut resolved = submission.clone();
        resolved.agent = agent;
        resolved.model_selection = selection;
        resolved.work_class = class;
        Ok(resolved)
    }

    /// The submit write. `key` seeds the task id, and the
    /// caller holds the idempotency gate. `selection_catalog` is the catalog read the caller
    /// made before taking the gate; it is `None` when the request names no model selection.
    fn submit_task_inner(
        &self,
        request: &TaskSubmitParams,
        key: &str,
        selection_catalog: Option<&orchestrator_agents::AgentCatalog>,
    ) -> Result<Value, RpcError> {
        let contract = self.checked_contract(request, key, selection_catalog)?;
        self.admit_contract(&contract)
    }

    /// Builds the contract a submit request stands for and runs every check a submit makes,
    /// writing nothing. The caller holds the idempotency gate.
    fn checked_contract(
        &self,
        request: &TaskSubmitParams,
        key: &str,
        selection_catalog: Option<&orchestrator_agents::AgentCatalog>,
    ) -> Result<TaskContract, RpcError> {
        let contract = self.checked_contract_without_catalog(request, key)?;
        if let Some(selection) = &contract.model_selection {
            let catalog = selection_catalog.ok_or(RpcError::Internal)?;
            self.check_resolved_selection(
                request.contract.work_class.as_deref(),
                catalog,
                selection,
            )?;
        }
        Ok(contract)
    }

    fn checked_contract_without_catalog(
        &self,
        request: &TaskSubmitParams,
        key: &str,
    ) -> Result<TaskContract, RpcError> {
        // The executor always launches a writing agent, so a policy
        // that denies workspace writes cannot be honored. Refuse it
        // rather than run an agent with more access than requested.
        if matches!(request.contract.autonomy, AutonomyPolicy::ReadOnly) {
            return Err(RpcError::Forbidden);
        }
        let permission = request
            .contract
            .permission
            .unwrap_or(TASK_DEFAULT_PERMISSION);
        // A level the agent has no task mapping for is refused, so no task is
        // created for it. This includes the task default for an agent with no
        // mapping at any level, such as Pi. Restricted always takes the existing
        // path, so an explicit Restricted task for such an agent still fails at launch.
        if permission != PermissionLevel::Restricted
            && orchestrator_agents::permission::task_profile(&request.contract.agent, permission)
                .is_none()
        {
            self.config.diagnostics.warn(
                "api",
                orchestrator_core::permission::PERMISSION_LEVEL_UNSUPPORTED,
                &[
                    ("agent", &request.contract.agent),
                    ("permission", permission.as_str()),
                ],
            );
            return Err(RpcError::InvalidParams);
        }
        let contract = TaskContract {
            id: idempotency::stable_id("task", key),
            project_id: request.contract.project_id.clone(),
            parent_id: request.contract.parent_id.clone(),
            objective: request.contract.objective.clone(),
            kind: request.contract.kind,
            agent: request.contract.agent.clone(),
            dependencies: request.contract.dependencies.clone(),
            verification: request.contract.verification.clone(),
            delivery: request.contract.delivery,
            autonomy: request.contract.autonomy,
            max_retries: request.contract.max_retries,
            max_repairs: request.contract.max_repairs,
            model_selection: request.contract.model_selection.clone(),
            permission,
            work_item: request.contract.work_item.clone(),
        };
        let project = self
            .store
            .get_project(&contract.project_id)
            .map_err(map_store_error)?;
        // A task in the home directory leaves nothing to review or land.
        if project.is_in_place() && contract.delivery != DeliveryPolicy::ReportOnly {
            return Err(RpcError::InvalidParams);
        }
        // Nor is there a candidate there for a verification gate to run against.
        if project.is_in_place() && contract.verification.clone().has_gate() {
            return Err(RpcError::InvalidParams);
        }
        // A removed project takes no new task until it is registered again.
        if self
            .store
            .project_is_archived(&contract.project_id)
            .map_err(map_store_error)?
        {
            return Err(RpcError::Conflict);
        }
        contract::validate(&contract).map_err(|_| RpcError::InvalidParams)?;
        if let Some(parent_id) = &contract.parent_id {
            match self.store.get(parent_id) {
                Ok(_) => {}
                Err(StoreError::NotFound(_) | StoreError::InvalidTaskId(_)) => {
                    return Err(RpcError::ParentNotFound);
                }
                Err(error) => return Err(map_store_error(error)),
            }
        }
        self.check_agent_command_line(&contract)?;
        self.validate_scheduler(&contract)?;
        Ok(contract)
    }

    /// Creates and admits a contract `checked_contract` built, then wakes the executor.
    fn admit_contract(&self, contract: &TaskContract) -> Result<Value, RpcError> {
        self.store.create(contract).map_err(map_store_error)?;
        self.store
            .transition(&contract.id, TaskEvent::Admit)
            .map_err(map_store_error)?;
        self.wake_executor();
        let record = self.store.get(&contract.id).map_err(map_store_error)?;
        crate::task_view::task_view(&record).map_err(|_| RpcError::Internal)
    }

    /// Queues a new attempt for a `Failed` or `VerificationFailed` task. The
    /// failed attempt stays on record; the executor appends the next attempt
    /// when it launches the task. The contract bounds how many attempts a task
    /// may have: `max_retries` for a failed run, `max_repairs` after failed
    /// verification. Exhausting the bound is a `Conflict`.
    fn retry_task(&self, params: Value) -> Result<Value, RpcError> {
        let request: RetryParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "tasks.retry",
            &key,
            &request,
            || self.retry_task_inner(&request.task_id),
        )
    }

    /// The retry write, shared with action execution. The caller holds the idempotency gate.
    pub(super) fn retry_task_inner(&self, task_id: &str) -> Result<Value, RpcError> {
        let task = self.store.get(task_id).map_err(map_store_error)?;
        // A removed project takes no new work, as `tasks.submit` refuses.
        if self
            .store
            .project_is_archived(&task.contract.project_id)
            .map_err(map_store_error)?
        {
            return Err(RpcError::Conflict);
        }
        let bound = match task.state {
            TaskState::Failed => task.contract.max_retries,
            TaskState::VerificationFailed => task.contract.max_repairs,
            _ => return Err(RpcError::Conflict),
        };
        // A follow-up to a finished attempt is neither a retry nor a repair.
        let follow_ups = self
            .store
            .follow_up_attempts(task_id)
            .map_err(map_store_error)? as usize;
        if u32::try_from(task.attempts.len() - follow_ups).map_or(true, |attempts| attempts > bound)
        {
            return Err(RpcError::Conflict);
        }
        self.store
            .transition(task_id, TaskEvent::RetryRequested)
            .map_err(map_store_error)?;
        self.wake_executor();
        let record = self.store.get(task_id).map_err(map_store_error)?;
        crate::task_view::task_view(&record).map_err(|_| RpcError::Internal)
    }

    /// `tasks.force_start` (user-only): the user's word that a queued task starts now, past the
    /// worker caps, the load gate and the admission check. The restart gate, dependencies,
    /// open questions, project registration and worktree ownership still hold it back. A task
    /// that is not queued, or that a pass would not start even when forced, is `Conflict`; an
    /// unknown one is `NotFound`. The task leaves `Queued` when the executor launches it.
    fn force_start_task(&self, params: Value) -> Result<Value, RpcError> {
        let request: TaskKeyParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "tasks.force_start",
            &key,
            &request,
            || {
                let task = self.store.get(&request.task_id).map_err(map_store_error)?;
                let executor = self.executor.as_ref().ok_or(RpcError::Conflict)?;
                if task.state != TaskState::Queued
                    || executor.dispatch_state().1
                    || !self.can_start_forced(&request.task_id)?
                {
                    return Err(RpcError::Conflict);
                }
                executor.force_start(&request.task_id);
                Ok(json!({"task_id": request.task_id, "forced": true}))
            },
        )
    }

    /// Reports each known agent family: whether its executable is installed
    /// and what it supports, plus whether the daemon can launch it. Results
    /// carry no executable paths.
    fn agent_capabilities(&self, params: Value) -> Result<Value, RpcError> {
        require_empty_params(&params)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| RpcError::Internal)?;
        let mut codex = CodexAdapter::new();
        if let Some(program) = &self.config.codex_program {
            codex = codex.with_program(program);
        }
        let mut claude = ClaudeCodeAdapter::new();
        if let Some(program) = &self.config.claude_program {
            claude = claude.with_program(program);
        }
        // Probes can run real agent binaries (Pi's RPC probe), so they run
        // together and each is bounded. Dropping an unfinished probe drops its
        // `ChildSupervisor`, which SIGKILLs the child's process group by PID
        // (Job Object termination on Windows), so a wedged binary neither
        // blocks the reply nor outlives it.
        let mut pi = PiRpcAdapter::new();
        if let Some(program) = &self.config.pi_program {
            pi = pi.with_program(program);
        }
        let mut copilot = CopilotAdapter::new();
        if let Some(program) = &self.config.copilot_program {
            copilot = copilot.with_program(program);
        }
        let limit = self.config.probe_timeout;
        let (codex, claude, pi, copilot, codex_chat, claude_chat, copilot_chat) =
            runtime.block_on(async {
                tokio::join!(
                    probe(limit, codex.detect()),
                    probe(limit, claude.detect()),
                    probe(limit, pi.detect()),
                    probe(limit, copilot.detect()),
                    conversation_agent::check_chat_provider(&self.config, "codex"),
                    conversation_agent::check_chat_provider(&self.config, "claude-code"),
                    conversation_agent::check_chat_provider(&self.config, COPILOT_AGENT),
                )
            });
        let sign_in = self.config.copilot_sign_in.snapshot();
        Ok(json!([
            with_chat_compatibility(agent_entry("codex", true, codex), codex_chat),
            with_chat_compatibility(agent_entry("claude-code", true, claude), claude_chat),
            // The daemon has no stdin RPC launcher for Pi yet.
            agent_entry("pi", false, pi),
            with_chat_compatibility(copilot_entry(copilot, &sign_in), copilot_chat),
        ]))
    }

    /// Runs the one explicit Copilot sign-in check. The check spends one call on
    /// `claude-haiku-4.5`, and its answer is kept for the session unless `force`
    /// asks for a new call. A check whose executable is missing spends nothing.
    fn check_copilot_sign_in(&self, params: Value) -> Result<Value, RpcError> {
        let force = match params {
            Value::Null => false,
            params => {
                serde_json::from_value::<CopilotSignInParams>(params)
                    .map_err(|_| RpcError::InvalidParams)?
                    .force
            }
        };
        // Starts the check on a background thread and returns at once. The
        // thread is not tied to this request, so a client that disconnects or
        // times out cancels nothing. The answer is read with `agents.capabilities`.
        let mut probe = CopilotAdapter::new();
        if let Some(program) = &self.config.copilot_program {
            probe = probe.with_program(program);
        }
        let record = if probe.locate().is_none() {
            // A missing executable is known now, so nothing is started for it.
            SignInRecord::refused("the copilot executable was not found".to_owned())
        } else {
            let program = self.config.copilot_program.clone();
            let runs = self.config.data_dir.join(COPILOT_RUNS_DIR);
            let credit_cap = self.config.copilot_credit_cap;
            self.config.copilot_sign_in.start(force, move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        return SignInRecord::failed(format!(
                            "could not start a runtime for the check: {error}"
                        ));
                    }
                };
                let mut copilot = CopilotAdapter::new();
                if let Some(program) = program {
                    copilot = copilot.with_program(program);
                }
                runtime.block_on(copilot.sign_in_check(&runs, COPILOT_SIGN_IN_TIMEOUT, credit_cap))
            })
        };
        serde_json::to_value(record).map_err(|_| RpcError::Internal)
    }

    /// Refuses an objective the operating system could not pass to the agent
    /// CLI. `RpcError` carries no detail, so the actionable message goes to the
    /// diagnostics log and the caller sees `InvalidParams`.
    fn check_agent_command_line(&self, contract: &TaskContract) -> Result<(), RpcError> {
        let workflow_agent = (self.config.locate_workflow_agent)();
        let status_dir = placeholder_status_dir(&self.config);
        let Some((program, args)) = agent_command(
            &self.config,
            &contract.agent,
            &contract.objective,
            contract.model_selection.as_ref(),
            contract.permission,
            workflow_agent.as_ref(),
            submit_channel(&status_dir, ChannelSupport::ALL),
        ) else {
            return Ok(());
        };
        orchestrator_runtime::check_command_line(&program, &args).map_err(|error| {
            self.config.diagnostics.warn(
                "api",
                "task_submit_command_line_too_long",
                &[("agent", &contract.agent), ("error", &error.to_string())],
            );
            RpcError::InvalidParams
        })
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
        // Archived tasks are left out, as archived projects are left out of `projects.list`.
        let archived = self.store.archived_task_ids().map_err(map_store_error)?;
        let tasks: Vec<_> = self
            .store
            .list()
            .map_err(map_store_error)?
            .into_iter()
            .filter(|task| !archived.contains(&task.contract.id))
            .map(|task| crate::task_view::task_view(&task))
            .collect::<Result<_, _>>()
            .map_err(|_| RpcError::Internal)?;
        Ok(Value::Array(tasks))
    }

    fn get_task(&self, params: Value) -> Result<Value, RpcError> {
        let params: TaskIdParams = parse_params(params)?;
        let record = self.store.get(&params.task_id).map_err(map_store_error)?;
        crate::task_view::task_view(&record).map_err(|_| RpcError::Internal)
    }
}

/// The daemon launches only canonical identifiers. Case-insensitive display
/// names are accepted at the boundary and persisted as the launcher's ID.
fn canonical_agent_id(agent: &str) -> Option<&'static str> {
    match agent.trim().to_ascii_lowercase().as_str() {
        "codex" => Some("codex"),
        "claude-code" | "claude code" => Some("claude-code"),
        "github-copilot" | "github copilot" => Some("github-copilot"),
        "pi" => Some("pi"),
        _ => None,
    }
}

impl DaemonApi {
    /// Archives finished tasks, all or none. An unknown id is `NotFound`; a task that is not
    /// finished is `Conflict`, and nothing is written. Replies with the ids this call archived.
    fn archive_tasks(&self, params: Value) -> Result<Value, RpcError> {
        let request: ArchiveParams = parse_params(params)?;
        if request.task_ids.is_empty() {
            return Err(RpcError::InvalidParams);
        }
        let key = request.idempotency_key.clone();
        // Only the tasks this call archived. A replayed reply archives nothing again.
        let mut archived_now = Vec::new();
        let reply = idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "tasks.archive",
            &key,
            &request,
            || {
                let archived = self
                    .store
                    .archive_tasks(&request.task_ids)
                    .map_err(|error| match ArchiveError::of(&error) {
                        Some(_) => RpcError::Conflict,
                        None => map_store_error(error),
                    })?;
                archived_now.clone_from(&archived);
                Ok(json!({"archived": archived}))
            },
        )?;
        self.release_task_slots(&archived_now);
        Ok(reply)
    }

    /// `tasks.discard` (DPES-1108 phase 5, user-only): the user's word that a finished task's
    /// committed work is not wanted. It only records that word, which lets the task be archived
    /// and its slot returned; the task's branch stays in the repository. A task that is not
    /// finished is `Conflict`, an unknown one `NotFound`. Replies with whether this call
    /// recorded it.
    fn discard_task(&self, params: Value) -> Result<Value, RpcError> {
        let request: TaskKeyParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "tasks.discard",
            &key,
            &request,
            || {
                let discarded = self
                    .store
                    .discard_task_candidate(&request.task_id)
                    .map_err(|error| match ArchiveError::of(&error) {
                        Some(_) => RpcError::Conflict,
                        None => map_store_error(error),
                    })?;
                Ok(json!({"task_id": request.task_id, "discarded": discarded}))
            },
        )
    }

    /// `tasks.message` (DPES-1106, user-only): the user's own words to a waiting task. They
    /// queue as the user's, not the orchestrator's, and the scheduler resumes the worker with
    /// them. An unknown task is `NotFound`. A task that does not wait, a task with an open
    /// question, or a full queue of the user's messages, is `Conflict`. Text the reply rules
    /// refuse is `InvalidParams`.
    fn message_task(&self, params: Value) -> Result<Value, RpcError> {
        let request: UserMessageParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "tasks.message",
            &key,
            &request,
            || {
                let task = self.store.get(&request.task_id).map_err(map_store_error)?;
                let finished = matches!(
                    task.state,
                    TaskState::Succeeded
                        | TaskState::Failed
                        | TaskState::VerificationFailed
                        | TaskState::Cancelled
                );
                let mut slot_returned = false;
                if finished {
                    // Refused here, not at the resume: a resume that cannot run fails the task.
                    if orchestrator_agents::resume_refusal(
                        &task.contract.agent,
                        task.contract.permission,
                    )
                    .is_some()
                    {
                        return Err(RpcError::Conflict);
                    }
                    slot_returned = self.slot_returned(&request.task_id)?;
                    if slot_returned {
                        let session = self
                            .store
                            .continuable_session(&request.task_id)
                            .map_err(map_store_error)?;
                        let workspace = self
                            .store
                            .list_workspaces()
                            .map_err(map_store_error)?
                            .into_iter()
                            .rev()
                            .find(|workspace| workspace.task_id == request.task_id);
                        // A saved conversation that cannot follow the task to a new slot is
                        // refused now, so the finished task stays as it is.
                        let followable = match (session, workspace) {
                            (Some(session), Some(workspace)) => {
                                workspace.is_in_place()
                                    || crate::executor::session_transcript::has_transcript(
                                        &task.contract.agent,
                                        Path::new(&workspace.path),
                                        &session,
                                    )
                            }
                            _ => false,
                        };
                        if !followable {
                            return Err(RpcError::Conflict);
                        }
                    }
                }
                self.store
                    .queue_message(
                        &request.task_id,
                        &request.idempotency_key,
                        &request.text,
                        MessageAuthor::User,
                        slot_returned,
                    )
                    .map_err(|error| match WaitError::of(&error) {
                        Some(
                            WaitError::NotWaiting
                            | WaitError::QueueFull
                            | WaitError::QuestionOpen
                            | WaitError::NoSession,
                        ) => RpcError::Conflict,
                        Some(WaitError::BadLine) => RpcError::InvalidParams,
                        _ => map_store_error(error),
                    })?;
                self.wake_resumes();
                Ok(json!({"task_id": request.task_id, "queued": true}))
            },
        )
    }

    /// Returns the treehouse slots of finished tasks to the pool. Called after the
    /// write, outside the gate, since each return runs treehouse. A failure is logged
    /// and never fails the RPC that finished the task. A task whose committed work is not
    /// landed or discarded keeps its slot (DPES-1108 phase 5): archive refuses such a task
    /// first, so this is the second guard.
    fn release_task_slots(&self, task_ids: &[String]) {
        let Some(executor) = &self.executor else {
            return;
        };
        for task_id in task_ids {
            match self.store.has_unlanded_candidate(task_id) {
                Ok(false) => {}
                Ok(true) => {
                    self.config.diagnostics.warn(
                        "api",
                        "worktree_kept_unlanded",
                        &[("task_id", task_id)],
                    );
                    continue;
                }
                Err(error) => {
                    self.config.diagnostics.error(
                        "api",
                        "unlanded_check_failed",
                        &[("task_id", task_id), ("error", &error.to_string())],
                    );
                    continue;
                }
            }
            match self.store.workspaces_for_task(task_id) {
                Ok(workspaces) => executor.release_workspaces(&workspaces),
                Err(error) => self.config.diagnostics.error(
                    "api",
                    "workspaces_unread_for_release",
                    &[("task_id", task_id), ("error", &error.to_string())],
                ),
            }
        }
    }

    fn cancel_task(&self, params: Value) -> Result<Value, RpcError> {
        let request: CancelParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "tasks.cancel",
            &key,
            &request,
            || self.cancel_task_inner(&request.task_id),
        )
    }

    /// The cancel write, shared with action execution. The caller holds the idempotency gate.
    /// A running task only has its cancellation requested; the owner confirms later. A
    /// verifying task is cancelled at once, and its verification run is interrupted. A waiting
    /// task is cancelled at once; its worktree stays as any cancelled task's does.
    pub(super) fn cancel_task_inner(&self, task_id: &str) -> Result<Value, RpcError> {
        self.cancel_task_record(task_id)?;
        let record = self.store.get(task_id).map_err(map_store_error)?;
        crate::task_view::task_view(&record).map_err(|_| RpcError::Internal)
    }

    fn cancel_task_record(&self, task_id: &str) -> Result<(), RpcError> {
        let task = self.store.get(task_id).map_err(map_store_error)?;
        match task.state {
            TaskState::Running => {
                self.require_live_cancellation_owner(task_id)?;
                return match self
                    .store
                    .request_cancellation_for_task(task_id)
                    .map_err(map_store_error)?
                {
                    CancellationStatus::Pending | CancellationStatus::Confirmed => Ok(()),
                    CancellationStatus::Rejected | CancellationStatus::Failed => {
                        Err(RpcError::Conflict)
                    }
                };
            }
            TaskState::Created | TaskState::Queued => {
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
            TaskState::Verifying => {
                // The store cancels the attempt and records the cancel in one transaction.
                self.store
                    .cancel_verifying_task(task_id)
                    .map_err(map_store_error)?;
                if let Some(executor) = &self.executor {
                    executor.cancel_verification(task_id);
                }
                return Ok(());
            }
            // A waiting worker has no process, so the cancel needs no owner (DPES-1108 phase 4).
            TaskState::Waiting => {
                return self
                    .store
                    .cancel_waiting_task(task_id)
                    .map_err(map_store_error);
            }
            _ => return Err(RpcError::Conflict),
        };
        self.store
            .transition(task_id, TaskEvent::CancelRequested)
            .map_err(map_store_error)?;
        Ok(())
    }

    /// Refuses to queue a cancellation that no live run owner can consume.
    ///
    /// The daemon holds the data-directory lock for its whole lifetime (see
    /// `main`), so no CLI command that takes the lock can reconcile owners
    /// concurrently. A live run owner takes no lock while it finishes, and
    /// confirms or closes its own record in one generation-fenced store
    /// transaction, so it can race this method safely. An owner
    /// whose exact recorded process has exited is retired for that exact
    /// generation, which marks its request `Failed`. An owner that is not
    /// provably alive (missing or inconclusive identity) is left untouched.
    /// Both return `Conflict` before any request is created. `RpcError` carries
    /// no detail, so recovery guidance goes to the diagnostics log.
    fn require_live_cancellation_owner(&self, task_id: &str) -> Result<(), RpcError> {
        let owners = self
            .store
            .open_cancellation_owners()
            .map_err(map_store_error)?;
        for owner in owners.into_iter().filter(|owner| owner.task_id == task_id) {
            let classification = owner.exact_process_identity().map(observe_owner_process);
            if classification == Some(ProcessClassification::Alive) {
                continue;
            }
            if classification == Some(ProcessClassification::Exited) {
                self.store
                    .retire_stale_cancellation_owner(&owner)
                    .map_err(map_store_error)?;
                self.config.diagnostics.warn(
                    "api",
                    "cancellation_owner_exited",
                    &[
                        ("task_id", task_id),
                        ("generation", owner.generation()),
                        (
                            "recovery",
                            &format!(
                                "request marked Failed and the task remains Running. Stop the \
                                 daemon and use `agent-orch inspect {task_id}` to review the \
                                 preserved state"
                            ),
                        ),
                    ],
                );
            } else {
                self.config.diagnostics.warn(
                    "api",
                    "cancellation_owner_not_provably_alive",
                    &[
                        ("task_id", task_id),
                        ("generation", owner.generation()),
                        ("classification", &format!("{classification:?}")),
                        (
                            "recovery",
                            &format!(
                                "no cancellation was requested. Stop the daemon, confirm the \
                                 owner and agent processes have exited, then run `agent-orch \
                                 recover-owner {task_id} --attempt-id {} --session-id {} \
                                 --generation {} --confirm-processes-exited --reason \"<reason>\"`. \
                                 This Conflict is cached for the idempotency key, so retry \
                                 cancellation with a new key after recovery",
                                owner.attempt_id,
                                owner.session_id,
                                owner.generation()
                            ),
                        ),
                    ],
                );
            }
            return Err(RpcError::Conflict);
        }
        Ok(())
    }

    fn list_sessions(&self, params: Value) -> Result<Value, RpcError> {
        let params: SessionsListParams = parse_params(params)?;
        let tasks = match params.task_id {
            Some(task_id) => vec![self.store.get(&task_id).map_err(map_store_error)?],
            None => self.store.list().map_err(map_store_error)?,
        };
        let sessions = tasks.into_iter().flat_map(|task| {
            let task_id = task.contract.id.clone();
            let effective = models::effective_configuration(&task.contract);
            task.sessions.into_iter().map(move |session| {
                json!({
                    "task_id": &task_id,
                    "session": session,
                    "effective_configuration": &effective,
                })
            })
        });
        Ok(Value::Array(sessions.collect()))
    }

    fn get_session(&self, params: Value) -> Result<Value, RpcError> {
        let params: SessionParams = parse_params(params)?;
        let task = self.store.get(&params.task_id).map_err(map_store_error)?;
        let effective = models::effective_configuration(&task.contract);
        let session = task
            .sessions
            .into_iter()
            .find(|session| session.id == params.session_id)
            .ok_or(RpcError::NotFound)?;
        Ok(json!({
            "task_id": params.task_id,
            "session": session,
            "effective_configuration": effective,
        }))
    }

    fn session_events(&self, params: Value) -> Result<Value, RpcError> {
        let params: SessionEventsParams = parse_params(params)?;
        let epoch = self.store.store_epoch().map_err(map_store_error)?;
        if params.epoch.as_ref().is_some_and(|given| *given != epoch) {
            return Err(RpcError::EpochMismatch);
        }
        let cursor = params.after_sequence.unwrap_or(0);
        let latest = self
            .store
            .latest_event_sequence()
            .map_err(map_store_error)?;
        if cursor < 0 {
            return Err(RpcError::InvalidParams);
        }
        if cursor > latest {
            return Err(RpcError::ReplayGap);
        }
        let mut events = self
            .store
            .session_events_after(
                &params.task_id,
                &params.session_id,
                cursor,
                EVENT_PAGE_SIZE + 1,
            )
            .map_err(map_store_error)?;
        let more = events.len() > EVENT_PAGE_SIZE;
        events.truncate(EVENT_PAGE_SIZE);
        let next_after_sequence = events.last().map_or(cursor, |event| event.sequence);
        Ok(json!({
            "task_id": params.task_id,
            "session_id": params.session_id,
            "events": events.into_iter().map(stream_event).collect::<Vec<_>>(),
            "more": more,
            "next_after_sequence": next_after_sequence,
            "epoch": epoch
        }))
    }

    fn list_approvals(&self, params: Value) -> Result<Value, RpcError> {
        require_empty_params(&params)?;
        serde_json::to_value(self.store.pending_approvals().map_err(map_approval_error)?)
            .map_err(|_| RpcError::Internal)
    }

    fn resolve_approval(&self, params: Value) -> Result<Value, RpcError> {
        let request: ResolveApprovalParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "approvals.resolve",
            &key,
            &request,
            || {
                let result: Approval = match request.decision.as_str() {
                    "approve" => self
                        .store
                        .approve_reviewed_approval(
                            &request.approval_id,
                            &self.approval_actor,
                            &self.approval_allowlist,
                        )
                        .map_err(map_review_error)?,
                    "reject" => self
                        .store
                        .reject_approval(
                            &request.approval_id,
                            &self.approval_actor,
                            &self.approval_allowlist,
                        )
                        .map_err(map_approval_error)?,
                    _ => return Err(RpcError::InvalidParams),
                };
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
        let scoped_path =
            validate_workspace_diff_path(Path::new(&workspace.path), params.path.as_deref())?;
        let project = self
            .store
            .get_project(&task.contract.project_id)
            .map_err(map_store_error)?;
        let (_primary, worktree) = task_repositories(&project, &workspace)?;
        let diff = worktree
            .diff_from(
                &workspace.base_commit,
                scoped_path.as_deref().map(Path::new),
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

    /// Whether the task's treehouse slot has been returned to the pool (the task is archived or
    /// its pull request is merged). The recorded path may then be leased to another task, so it
    /// is not read.
    pub(super) fn slot_returned(&self, task_id: &str) -> Result<bool, RpcError> {
        let archived = self.store.archived_task_ids().map_err(map_store_error)?;
        let merged = self
            .store
            .latest_pull_request_merge(task_id)
            .map_err(map_store_error)?
            .is_some_and(|merge| merge.state == MergeState::Merged);
        Ok(merged || archived.contains(task_id))
    }

    fn selected_workspace(
        &self,
        task_id: &str,
        requested_id: Option<&str>,
    ) -> Result<Workspace, RpcError> {
        if self.slot_returned(task_id)? {
            return Err(RpcError::NotFound);
        }
        // A task that ran in place has no checkout of its own to diff, review or land.
        let workspaces: Vec<Workspace> = self
            .store
            .workspaces_for_task(task_id)
            .map_err(map_store_error)?
            .into_iter()
            .filter(|workspace| !workspace.is_in_place())
            .collect();
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

fn validate_workspace_diff_path(
    worktree_root: &Path,
    path: Option<&str>,
) -> Result<Option<String>, RpcError> {
    let Some(path) = path else {
        return Ok(None);
    };
    let path_components = Path::new(path);
    if path.is_empty()
        || path.starts_with('-')
        || path_components.is_absolute()
        || path_components.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(RpcError::InvalidParams);
    }
    let canonical_root = std::fs::canonicalize(worktree_root).map_err(|_| RpcError::NotFound)?;
    let canonical_path = std::fs::canonicalize(canonical_root.join(path_components))
        .map_err(|_| RpcError::InvalidParams)?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(RpcError::InvalidParams);
    }
    Ok(Some(path.to_owned()))
}

impl DaemonApi {
    fn request_delivery(&self, params: Value) -> Result<Value, RpcError> {
        let request: DeliveryParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "delivery.request",
            &key,
            &request,
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

/// Bounds one capability probe; an unfinished probe is dropped, which kills
/// the child it supervises, and is reported as not installed.
async fn probe<T>(
    limit: std::time::Duration,
    detect: impl std::future::Future<Output = orchestrator_agents::Result<T>>,
) -> orchestrator_agents::Result<T> {
    match tokio::time::timeout(limit, detect).await {
        Ok(result) => result,
        Err(_) => Err(orchestrator_agents::AgentError::Protocol(
            "capability probe timed out".into(),
        )),
    }
}

fn with_chat_compatibility(
    mut entry: Value,
    result: Option<
        Vec<(
            PermissionLevel,
            Result<(), orchestrator_agents::cli_compat::CompatibilityError>,
        )>,
    >,
) -> Value {
    if let Some(levels) = result {
        use orchestrator_agents::cli_compat::CompatibilityError;
        entry["installed"] = json!(true);
        let failures: serde_json::Map<String, Value> = levels
            .into_iter()
            .filter_map(|(level, answer)| {
                let error = answer.err()?;
                Some((
                    level.as_str().to_owned(),
                    json!({
                        "retryable": error == CompatibilityError::Unconfirmed,
                        "reason": error.to_string(),
                    }),
                ))
            })
            .collect();
        if !failures.is_empty() {
            entry["chat_compatibility"] = json!(failures);
        }
    }
    entry
}

/// Copilot additionally carries the cached sign-in record.
fn copilot_entry(
    detected: orchestrator_agents::Result<orchestrator_agents::AgentCapabilities>,
    sign_in: &SignInRecord,
) -> Value {
    let mut entry = agent_entry(COPILOT_AGENT, detected.is_ok(), detected);
    // The whole record: state, reason, time, and the counts a check reported.
    entry["sign_in"] = serde_json::to_value(sign_in).unwrap_or(Value::Null);
    entry
}

fn agent_entry(
    agent: &str,
    launchable: bool,
    detected: orchestrator_agents::Result<orchestrator_agents::AgentCapabilities>,
) -> Value {
    match detected {
        Ok(caps) => json!({
            "agent": agent,
            "installed": true,
            "launchable": launchable,
            "capabilities": {
                "streaming": caps.streaming,
                "interactive_input": caps.interactive_input,
                "structured_output": caps.structured_output,
                "cancellation": caps.cancellation,
                "session_resumption": caps.session_resumption,
                "approvals": caps.approvals,
                "model_selection": caps.model_selection,
            },
        }),
        Err(_) => json!({
            "agent": agent,
            "installed": false,
            "launchable": launchable,
            "capabilities": Value::Null,
        }),
    }
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
        StoreError::AlreadyExists(_)
        | StoreError::IllegalTransition(_)
        | StoreError::NoActiveCancellationOwner { .. }
        | StoreError::CancellationConflict { .. } => RpcError::Conflict,
        other => internal("store", &other),
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
        ApprovalError::Persistence(cause) => internal("approval", &cause),
    }
}

fn map_git_error(error: orchestrator_git::Error) -> RpcError {
    match error {
        orchestrator_git::Error::DiffPathOutsideWorktree => RpcError::Forbidden,
        orchestrator_git::Error::InvalidDiffPath | orchestrator_git::Error::InvalidDiffBase => {
            RpcError::InvalidParams
        }
        orchestrator_git::Error::InvalidBaseRef { .. } => RpcError::InvalidParams,
        other => internal("git", &other),
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
        other => internal("delivery", &other),
    }
}

/// An unexpected failure reaches the client as a bare `Internal`, so its cause
/// is queued here and written by [`DaemonApi::report_internal`] to the injected
/// diagnostics sink (redacted) or it would be lost.
pub(crate) fn internal(source: &str, cause: &dyn std::fmt::Display) -> RpcError {
    INTERNAL_CAUSES.with(|causes| {
        causes
            .borrow_mut()
            .push((source.to_owned(), cause.to_string()));
    });
    RpcError::Internal
}

thread_local! {
    // Requests are handled synchronously on one thread, so the causes queued
    // by the error mappers belong to the request being reported.
    static INTERNAL_CAUSES: std::cell::RefCell<Vec<(String, String)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

#[path = "api_actions.rs"]
mod actions;
use actions::project_code;
#[path = "api_activity.rs"]
mod activity_rpc;
#[path = "admission.rs"]
pub(crate) mod admission;
#[path = "api_authority.rs"]
mod authority;
#[path = "api_brief.rs"]
mod brief;
#[path = "api_conversation_agent.rs"]
pub(crate) mod conversation_agent;
#[path = "api_conversation_orphans.rs"]
mod conversation_orphans;
#[path = "api_conversation_runtime.rs"]
mod conversation_runtime;
#[path = "api_conversation_snapshot.rs"]
mod conversation_snapshot;
#[path = "api_conversations.rs"]
mod conversations;
#[path = "api_coordinator.rs"]
mod coordinator;
#[path = "api_decisions.rs"]
mod decisions;
#[path = "grant_matcher.rs"]
mod grant_matcher;
#[path = "api_grants.rs"]
mod grants;
#[path = "api_holds.rs"]
mod holds;
#[path = "api_home_directory.rs"]
mod home_directory;
#[path = "api_host.rs"]
mod host;
#[path = "api_intake.rs"]
mod intake;
#[path = "landing.rs"]
mod landing;
#[path = "api_landing.rs"]
mod landing_api;
#[path = "api_load.rs"]
pub(crate) mod load;
#[path = "api_memory.rs"]
mod memory;
#[path = "api_models.rs"]
mod models;
#[path = "api_monitoring.rs"]
mod monitoring;
#[path = "api_output.rs"]
mod output;
#[path = "api_projects_defaults.rs"]
mod projects_defaults;
#[path = "api_queue.rs"]
mod queue;
#[path = "report.rs"]
pub(crate) mod report;
#[path = "api_reports.rs"]
mod reports;
#[path = "api_review.rs"]
mod review;
#[path = "api_thinking.rs"]
mod thinking;
use review::map_review_error;
#[path = "api_settings.rs"]
mod settings;
#[path = "api_usage.rs"]
mod usage;
#[path = "api_usage_local.rs"]
mod usage_local;
#[path = "wake.rs"]
pub(crate) mod wake;
#[path = "wake_events.rs"]
pub(crate) mod wake_events;

#[cfg(test)]
#[path = "api_usage_tests.rs"]
mod usage_tests;

#[cfg(test)]
#[path = "api_queue_tests.rs"]
mod queue_tests;

#[cfg(test)]
#[path = "api_settings_tests.rs"]
mod settings_tests;
#[path = "api_work_classes.rs"]
mod work_classes;
#[path = "api_workstream_plan.rs"]
mod workstream_plan;
#[path = "api_workstreams.rs"]
mod workstreams;

#[cfg(test)]
#[path = "api_review_tests.rs"]
mod review_tests;

#[cfg(test)]
#[path = "api_actions_phase4_tests.rs"]
mod actions_phase4_tests;
#[cfg(test)]
#[path = "api_actions_task_tests.rs"]
mod actions_task_tests;
#[cfg(test)]
#[path = "api_actions_tests.rs"]
mod actions_tests;
#[cfg(test)]
#[path = "api_authority_tests.rs"]
mod authority_tests;
#[cfg(test)]
#[path = "api_coordinator_tests.rs"]
mod coordinator_tests;
#[cfg(test)]
#[path = "api_decisions_tests.rs"]
mod decisions_tests;
#[cfg(test)]
#[path = "grant_matcher_tests.rs"]
mod grant_matcher_tests;
#[cfg(test)]
#[path = "api_grants_tests.rs"]
mod grants_tests;
#[cfg(test)]
#[path = "api_home_directory_tests.rs"]
mod home_directory_tests;
#[cfg(test)]
#[path = "api_landing_merge_tests.rs"]
mod landing_merge_tests;
#[cfg(test)]
#[path = "api_landing_tests.rs"]
mod landing_tests;
#[cfg(test)]
#[path = "api_memory_tests.rs"]
mod memory_tests;
#[cfg(test)]
#[path = "api_projects_defaults_tests.rs"]
mod projects_defaults_tests;
#[cfg(test)]
#[path = "api_reports_tests.rs"]
mod reports_tests;
#[cfg(test)]
#[path = "api_routing_tests.rs"]
mod routing_tests;
#[cfg(test)]
#[path = "api_workstream_plan_tests.rs"]
mod workstream_plan_tests;
#[cfg(test)]
#[path = "api_workstreams_tests.rs"]
mod workstreams_tests;

#[cfg(test)]
#[path = "api_archive_tests.rs"]
mod archive_tests;

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;
