//! Daemon-owned task execution.
//!
//! One scheduler thread owns every launch decision. It first reconciles the
//! store with the OS (the same gate the CLI runs, from `orchestrator-engine`),
//! then repeatedly asks the core [`Scheduler`] which `Queued` tasks may start
//! and launches them. Each launched task gets its own supervisor thread that
//! owns the agent handle until the process is reaped, so a client disconnect
//! never affects running work: no request or connection state reaches these
//! threads.
//!
//! Every state change goes through a store operation, and each of those
//! operations appends its durable journal event in the same transaction, so
//! the event stream a client resumes from already contains every transition
//! this module performs.
//!
//! Crash and restart: the daemon holds the data-directory lock for its whole
//! life, so at startup any agent whose owning daemon is gone is an orphan that
//! nobody can monitor or cancel. On Unix an orphan is terminated only after its
//! exact recorded identity (pid, start time, process group) is observed alive,
//! and then the shared reconcile gate records the interrupted attempt. On
//! Windows the Job Object that supervises each agent kills the tree when the
//! daemon process dies, so no orphan survives and the gate alone suffices.
//!
//! Known limits (see the M5-02b report): a panicking supervisor thread leaves
//! its task `Running` until the next daemon start reconciles it, and agent
//! output is consumed but not persisted beyond the lifecycle events.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use orchestrator_agents::{
    AgentAdapter, AgentEvent, AgentExit, AgentHandle, AgentLaunchRequest, ClaudeCodeAdapter,
    CodexAdapter,
};
use orchestrator_core::records::Workspace;
use orchestrator_core::scheduler::{Scheduler, SchedulerLimits, SchedulerSnapshot, TaskSnapshot};
use orchestrator_core::task::{Attempt, AttemptState, Session, SessionState, TaskEvent, TaskState};
use orchestrator_diagnostics::Diagnostics;
use orchestrator_engine::reconcile::reconcile_before_scheduling;
use orchestrator_git::{create_task_worktree, Repository};
use orchestrator_runtime::reconcile::{capture_exact_process_identity, capture_process_identity};
use orchestrator_store::{CancellationOwner, CancellationStatus, SqliteStore, Store};

const CODEX_AGENT: &str = "codex";
const CLAUDE_AGENT: &str = "claude-code";
const BLOCKED_RETRY: Duration = Duration::from_secs(5);
const LOG: &str = "executor";

/// Launch settings. `*_program` overrides the executable resolved from PATH.
#[derive(Clone)]
pub(crate) struct ExecutorConfig {
    pub(crate) worktrees_root: PathBuf,
    pub(crate) codex_program: Option<PathBuf>,
    pub(crate) claude_program: Option<PathBuf>,
    pub(crate) pi_program: Option<PathBuf>,
    pub(crate) limits: SchedulerLimits,
    pub(crate) poll: Duration,
    /// Upper bound for one `agents.capabilities` probe of an agent binary.
    pub(crate) probe_timeout: Duration,
    /// Where failures and recovery actions are reported (stderr in the daemon).
    pub(crate) diagnostics: Diagnostics,
}

impl ExecutorConfig {
    pub(crate) fn for_data_dir(data_dir: &Path) -> Self {
        Self {
            worktrees_root: data_dir.join("worktrees"),
            codex_program: std::env::var_os("AGENT_ORCH_CODEX_PROGRAM").map(PathBuf::from),
            claude_program: std::env::var_os("AGENT_ORCH_CLAUDE_PROGRAM").map(PathBuf::from),
            pi_program: std::env::var_os("AGENT_ORCH_PI_PROGRAM").map(PathBuf::from),
            limits: SchedulerLimits::default(),
            poll: Duration::from_millis(500),
            // Leaves headroom under the 2 s client I/O timeout.
            probe_timeout: Duration::from_millis(1200),
            diagnostics: Diagnostics::stderr(),
        }
    }
}

pub(crate) struct Executor {
    inner: Arc<Inner>,
    scheduler: Mutex<Option<JoinHandle<()>>>,
}

struct Inner {
    store: Arc<SqliteStore>,
    config: ExecutorConfig,
    /// True from a wake request until the next scheduling pass begins.
    dirty: Mutex<bool>,
    wake: Condvar,
    stop: AtomicBool,
    /// Why scheduling is blocked by the restart gate, if it is.
    blocked: Mutex<Option<String>>,
}

impl Executor {
    pub(crate) fn start(store: Arc<SqliteStore>, config: ExecutorConfig) -> Result<Self, String> {
        let inner = Arc::new(Inner {
            store,
            config,
            dirty: Mutex::new(true),
            wake: Condvar::new(),
            stop: AtomicBool::new(false),
            blocked: Mutex::new(None),
        });
        let worker = inner.clone();
        let handle = thread::Builder::new()
            .name("executor-scheduler".into())
            .spawn(move || worker.run())
            .map_err(|error| format!("cannot start executor thread: {error}"))?;
        Ok(Self {
            inner,
            scheduler: Mutex::new(Some(handle)),
        })
    }

    /// Asks the scheduler to run a pass now (task admitted, retried, finished).
    pub(crate) fn wake(&self) {
        self.inner.request_pass();
    }

    /// True while a requested pass has not started, so the daemon cannot idle
    /// out between an admission and its launch.
    pub(crate) fn has_pending_pass(&self) -> bool {
        *lock(&self.inner.dirty)
    }

    /// The restart-gate refusal currently blocking all launches, if any.
    /// Only the Unix-only executor tests read it.
    #[cfg(all(test, unix))]
    pub(crate) fn blocked_reason(&self) -> Option<String> {
        lock(&self.inner.blocked).clone()
    }

    /// Stops the scheduler thread. Running agents keep their own threads and
    /// are reconciled by the next daemon start if this process exits first.
    pub(crate) fn shutdown(&self) {
        let running = running_task_ids(&self.inner.store);
        if !running.is_empty() {
            self.inner.config.diagnostics.warn(
                LOG,
                "shutdown_with_running_agents",
                &[
                    ("task_ids", &running.join(",")),
                    (
                        "recovery",
                        "agents are reconciled and their worktrees kept on the next daemon start",
                    ),
                ],
            );
        }
        self.inner.stop.store(true, Ordering::SeqCst);
        self.inner.request_pass();
        if let Some(handle) = lock(&self.scheduler).take() {
            let _ = handle.join();
        }
    }
}

fn running_task_ids(store: &SqliteStore) -> Vec<String> {
    store
        .list()
        .map(|tasks| {
            tasks
                .into_iter()
                .filter(|task| task.state == TaskState::Running)
                .map(|task| task.contract.id)
                .collect()
        })
        .unwrap_or_default()
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

impl Inner {
    fn request_pass(&self) {
        *lock(&self.dirty) = true;
        self.wake.notify_all();
    }

    fn run(self: Arc<Self>) {
        let diag = &self.config.diagnostics;
        let mut gate_open = false;
        let mut last_started_project = None;
        while !self.stop.load(Ordering::SeqCst) {
            *lock(&self.dirty) = false;
            if !gate_open {
                terminate_orphaned_agents(&self.store, diag);
                let running_before = running_task_ids(&self.store);
                match reconcile_before_scheduling(&self.store) {
                    Ok(()) => {
                        gate_open = true;
                        *lock(&self.blocked) = None;
                        let running_after = running_task_ids(&self.store);
                        let interrupted: Vec<_> = running_before
                            .iter()
                            .filter(|id| !running_after.contains(id))
                            .cloned()
                            .collect();
                        diag.info(
                            LOG,
                            "recovery_gate_open",
                            &[
                                ("interrupted_task_ids", &interrupted.join(",")),
                                ("still_running_task_ids", &running_after.join(",")),
                            ],
                        );
                    }
                    Err(reason) => {
                        let mut blocked = lock(&self.blocked);
                        if blocked.as_deref() != Some(reason.as_str()) {
                            diag.error(LOG, "recovery_gate_blocked", &[("reason", &reason)]);
                        }
                        *blocked = Some(reason);
                    }
                }
            }
            if gate_open {
                self.schedule_pass(&mut last_started_project);
            }
            let wait = if gate_open {
                self.config.poll
            } else {
                BLOCKED_RETRY
            };
            let dirty = lock(&self.dirty);
            if !*dirty && !self.stop.load(Ordering::SeqCst) {
                drop(
                    self.wake
                        .wait_timeout(dirty, wait)
                        .unwrap_or_else(|error| error.into_inner()),
                );
            }
        }
    }

    fn schedule_pass(self: &Arc<Self>, last_started_project: &mut Option<String>) {
        let diag = &self.config.diagnostics;
        let (tasks, workspaces, projects) = match (
            self.store.list(),
            self.store.list_workspaces(),
            self.store.list_projects(),
        ) {
            (Ok(tasks), Ok(workspaces), Ok(projects)) => (tasks, workspaces, projects),
            (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
                diag.error(
                    LOG,
                    "scheduler_state_unreadable",
                    &[("error", &error.to_string())],
                );
                return;
            }
        };
        let registered: BTreeSet<&str> =
            projects.iter().map(|project| project.id.as_str()).collect();
        let snapshots: Vec<_> = tasks
            .iter()
            .map(|task| TaskSnapshot {
                contract: &task.contract,
                state: task.state,
                // A retried task owns one workspace per attempt; the newest
                // is the one that can conflict with other running work.
                workspace: workspaces
                    .iter()
                    .rev()
                    .find(|workspace| workspace.task_id == task.contract.id),
            })
            .collect();
        let scheduler = match Scheduler::new(
            SchedulerSnapshot {
                tasks: &snapshots,
                last_started_project: last_started_project.as_deref(),
            },
            self.config.limits,
        ) {
            Ok(scheduler) => scheduler,
            Err(error) => {
                diag.error(
                    LOG,
                    "scheduler_snapshot_refused",
                    &[("error", &error.to_string())],
                );
                return;
            }
        };
        // Authorization: the authenticated IPC session already authorized the
        // submission; a task may start only while its project is registered.
        let authorization = |project_id: &str, _task_id: &str| registered.contains(project_id);
        let plan = scheduler.schedule(Some(&authorization));
        *last_started_project = plan.last_started_project;
        for task_id in &plan.started {
            let Some(task) = tasks.iter().find(|task| &task.contract.id == task_id) else {
                continue;
            };
            self.launch(task);
        }
    }

    /// Records the attempt, worktree and session for a scheduler-approved task,
    /// then hands the agent to its own supervisor thread.
    fn launch(self: &Arc<Self>, task: &orchestrator_store::TaskRecord) {
        let task_id = task.contract.id.clone();
        let number = task.attempts.len() as u32 + 1;
        let job = Job {
            task_id: task_id.clone(),
            attempt_id: format!("{task_id}-attempt-{number}"),
            session_id: format!("{task_id}-session-{number}"),
            agent: task.contract.agent.clone(),
            objective: task.contract.objective.clone(),
            worktree: PathBuf::new(),
            diag: self.config.diagnostics.clone(),
        };
        if let Err(error) = self.prepare(task, number, job) {
            self.config.diagnostics.error(
                LOG,
                "launch_failed",
                &[("task_id", &task_id), ("error", &error)],
            );
        }
    }

    fn prepare(
        self: &Arc<Self>,
        task: &orchestrator_store::TaskRecord,
        number: u32,
        mut job: Job,
    ) -> Result<(), String> {
        let store = &*self.store;
        let task_id = job.task_id.clone();
        let worktree = Repository::new(Path::new(&task.contract.project_id))
            .map_err(|error| error.to_string())
            .and_then(|repository| {
                std::fs::create_dir_all(&self.config.worktrees_root)
                    .map_err(|error| error.to_string())?;
                let root = std::fs::canonicalize(&self.config.worktrees_root)
                    .map_err(|error| error.to_string())?;
                create_task_worktree(&repository, &task_id, "HEAD", &root)
                    .map_err(|error| error.to_string())
            });
        store
            .add_attempt(
                &task_id,
                &Attempt {
                    id: job.attempt_id.clone(),
                    task_id: task_id.clone(),
                    attempt_number: number,
                    state: AttemptState::Pending,
                },
            )
            .map_err(|error| format!("cannot record attempt: {error}"))?;
        let worktree = match worktree {
            Ok(worktree) => worktree,
            Err(error) => {
                // No process exists yet: take the attempt through Running so
                // it can end Failed, and fail the task so it can be retried.
                store
                    .transition(&task_id, TaskEvent::ExecutionStarted)
                    .map_err(|e| format!("cannot start task: {e}"))?;
                store
                    .update_attempt_state(&task_id, &job.attempt_id, AttemptState::Running)
                    .map_err(|e| format!("cannot start attempt: {e}"))?;
                store
                    .update_attempt_state(&task_id, &job.attempt_id, AttemptState::Failed)
                    .map_err(|e| format!("cannot fail attempt: {e}"))?;
                store
                    .transition(&task_id, TaskEvent::ExecutionFailed)
                    .map_err(|e| format!("cannot fail task: {e}"))?;
                return Err(format!("cannot create worktree: {error}"));
            }
        };
        job.worktree = worktree.path().to_path_buf();
        store
            .add_workspace(&Workspace {
                id: format!("{task_id}-workspace-{number}"),
                task_id: task_id.clone(),
                path: worktree.path().display().to_string(),
                branch: worktree.branch().to_string(),
                base_commit: worktree.base_sha().to_string(),
            })
            .map_err(|error| format!("cannot record worktree: {error}"))?;
        store
            .transition(&task_id, TaskEvent::ExecutionStarted)
            .map_err(|error| format!("cannot start task: {error}"))?;
        store
            .update_attempt_state(&task_id, &job.attempt_id, AttemptState::Running)
            .map_err(|error| format!("cannot start attempt: {error}"))?;
        store
            .add_session(
                &task_id,
                &Session {
                    id: job.session_id.clone(),
                    attempt_id: job.attempt_id.clone(),
                    adapter: job.agent.clone(),
                    state: SessionState::Spawning,
                },
            )
            .map_err(|error| format!("cannot record session: {error}"))?;

        let inner = self.clone();
        let supervisor_job = job.clone();
        let spawned = thread::Builder::new()
            .name(format!("agent-{task_id}"))
            .spawn(move || {
                supervise(&inner, supervisor_job);
                inner.request_pass();
            });
        if let Err(error) = spawned {
            fail_before_agent_exit(store, &job);
            return Err(format!("cannot start supervisor thread: {error}"));
        }
        Ok(())
    }
}

#[derive(Clone)]
struct Job {
    task_id: String,
    attempt_id: String,
    session_id: String,
    agent: String,
    objective: String,
    worktree: PathBuf,
    diag: Diagnostics,
}

impl Job {
    fn event_fields<'a>(&'a self, extra: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
        let mut fields = vec![
            ("task_id", self.task_id.as_str()),
            ("attempt_id", self.attempt_id.as_str()),
        ];
        fields.extend_from_slice(extra);
        fields
    }
}

/// Paths `git status` reports as changed or untracked in a worktree, or `None`
/// when it cannot be inspected. Used only to say how much work a failed or
/// interrupted run left behind; the worktree itself is never removed.
fn uncommitted_paths(worktree: &Path) -> Option<usize> {
    if worktree.as_os_str().is_empty() {
        return None;
    }
    git_checked(worktree, &["status", "--porcelain", "--untracked-files=all"])
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).lines().count())
}

/// The preserved-work facts every failure event carries.
fn preserved_work(job: &Job) -> (String, String) {
    let worktree = job.worktree.display().to_string();
    let uncommitted = uncommitted_paths(&job.worktree)
        .map_or_else(|| "unknown".to_owned(), |count| count.to_string());
    (worktree, uncommitted)
}

fn supervise(inner: &Inner, job: Job) {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        supervise_inner(inner, &job)
    }));
    if outcome.is_err() {
        // The panic hook has already recorded the message and location.
        let (worktree, uncommitted) = preserved_work(&job);
        job.diag.error(
            LOG,
            "supervisor_panicked",
            &job.event_fields(&[
                ("task_state", "running"),
                ("worktree", &worktree),
                ("uncommitted_paths", &uncommitted),
                (
                    "recovery",
                    "task stays Running until the next daemon start reconciles it; worktree kept",
                ),
            ]),
        );
    }
}

fn supervise_inner(inner: &Inner, job: &Job) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            job.diag.error(
                LOG,
                "async_runtime_unavailable",
                &job.event_fields(&[("error", &error.to_string())]),
            );
            fail_before_agent_exit(&inner.store, job);
            return;
        }
    };
    if let Err(error) = runtime.block_on(run_agent(inner, job)) {
        let (worktree, uncommitted) = preserved_work(job);
        job.diag.error(
            LOG,
            "supervision_error",
            &job.event_fields(&[
                ("error", &error),
                ("worktree", &worktree),
                ("uncommitted_paths", &uncommitted),
            ]),
        );
    }
}

async fn spawn_agent(inner: &Inner, job: &Job) -> Result<Box<dyn AgentHandle>, String> {
    let (program, args, detect) = match job.agent.as_str() {
        CODEX_AGENT => {
            let program = inner
                .config
                .codex_program
                .clone()
                .unwrap_or_else(|| PathBuf::from("codex"));
            let args = ["exec", "--json", "--approve-for-me", "--"]
                .into_iter()
                .map(str::to_owned)
                .chain([job.objective.clone()])
                .collect::<Vec<_>>();
            let adapter = CodexAdapter::new().with_program(program.clone());
            let caps = adapter.detect().await.map_err(|error| error.to_string())?;
            if !(caps.streaming && caps.structured_output && caps.cancellation) {
                return Err("codex lacks streaming, structured output or cancellation".into());
            }
            (program, args, AdapterChoice::Codex(adapter))
        }
        CLAUDE_AGENT => {
            let program = inner
                .config
                .claude_program
                .clone()
                .unwrap_or_else(|| PathBuf::from("claude"));
            let adapter = ClaudeCodeAdapter::new().with_program(program.clone());
            adapter.detect().await.map_err(|error| error.to_string())?;
            (
                program,
                ClaudeCodeAdapter::args_for_prompt(&job.objective),
                AdapterChoice::Claude(adapter),
            )
        }
        other => return Err(format!("agent {other:?} has no daemon launcher")),
    };
    let request = AgentLaunchRequest {
        program,
        args,
        working_dir: job.worktree.clone(),
    };
    let result = match detect {
        AdapterChoice::Codex(adapter) => adapter.spawn(request).await,
        AdapterChoice::Claude(adapter) => adapter.spawn(request).await,
    };
    result.map_err(|error| error.to_string())
}

enum AdapterChoice {
    Codex(CodexAdapter),
    Claude(ClaudeCodeAdapter),
}

async fn run_agent(inner: &Inner, job: &Job) -> Result<(), String> {
    let store = &*inner.store;
    let mut handle = match spawn_agent(inner, job).await {
        Ok(handle) => handle,
        Err(error) => {
            fail_before_agent_exit(store, job);
            let (worktree, uncommitted) = preserved_work(job);
            job.diag.error(
                LOG,
                "agent_launch_failed",
                &job.event_fields(&[
                    ("agent", &job.agent),
                    ("error", &error),
                    ("worktree", &worktree),
                    ("uncommitted_paths", &uncommitted),
                ]),
            );
            return Ok(());
        }
    };
    // A Running session without a recorded identity cannot be reconciled
    // later, so identity capture and persistence are mandatory.
    let identity = capture_process_identity(handle.pid())
        .map_err(|error| format!("process identity unavailable: {error}"))
        .and_then(|identity| {
            store
                .record_session_identity(&job.task_id, &job.session_id, &identity)
                .map_err(|error| format!("cannot persist process identity: {error}"))
        });
    if let Err(message) = identity {
        reap(&mut *handle, job).await;
        fail_before_agent_exit(store, job);
        return Err(message);
    }
    store
        .update_session_state(&job.task_id, &job.session_id, SessionState::Running)
        .map_err(|error| format!("cannot mark session running: {error}"))?;
    let owner = CancellationOwner::new(
        job.task_id.clone(),
        job.attempt_id.clone(),
        job.session_id.clone(),
        uuid::Uuid::new_v4().simple().to_string(),
    )
    .map_err(|error| format!("cannot create cancellation owner: {error}"))?;
    let owner = match capture_exact_process_identity(std::process::id()) {
        Ok(identity) => owner.with_exact_process_identity(identity),
        Err(error) => {
            job.diag.warn(
                LOG,
                "owner_identity_unavailable",
                &job.event_fields(&[("error", &error.to_string())]),
            );
            owner
        }
    };
    if let Err(error) = store.register_cancellation_owner(&owner) {
        reap(&mut *handle, job).await;
        fail_after_agent_exit(store, job);
        return Err(format!("cannot register cancellation owner: {error}"));
    }

    let monitored = monitor_agent(handle, store, &owner, job).await;
    let current = store
        .get(&job.task_id)
        .map_err(|error| format!("cannot reload task after agent exit: {error}"))?;
    if current.state == TaskState::Cancelled {
        return Err("task was cancelled before this owner confirmed it".into());
    }
    let monitored = monitored.map_err(|reason| {
        format!("supervision failed, task stays Running for restart reconciliation: {reason}")
    })?;
    finish(store, job, &owner, monitored)
}

/// Cancels and reaps a handle that cannot be supervised further.
async fn reap(handle: &mut dyn AgentHandle, job: &Job) {
    if let Err(error) = handle.cancel().await {
        job.diag.error(
            LOG,
            "reap_cancel_failed",
            &job.event_fields(&[("error", &error.to_string())]),
        );
    }
    if let Err(error) = handle.wait().await {
        job.diag.error(
            LOG,
            "reap_wait_failed",
            &job.event_fields(&[("error", &error.to_string())]),
        );
    }
}

struct MonitorOutcome {
    exit: AgentExit,
    cancellation_requested: bool,
    cancellation_succeeded: bool,
    cancellation_error: Option<String>,
    supervision_error: Option<String>,
}

/// Drives the agent to exit. A cancellation request is consumed at most once,
/// and `cancellation_succeeded` is set only after the handle's cancel returned
/// Ok. Neither depends on any client connection.
async fn monitor_agent(
    mut handle: Box<dyn AgentHandle>,
    store: &SqliteStore,
    owner: &CancellationOwner,
    job: &Job,
) -> Result<MonitorOutcome, String> {
    let mut requested = false;
    let mut attempted = false;
    let mut succeeded = false;
    let mut cancel_error = None;
    let mut supervision_error = None;
    let mut request_unreadable_logged = false;

    loop {
        if !attempted {
            match store.cancellation_request(owner) {
                Ok(Some(CancellationStatus::Pending)) => {
                    requested = true;
                    attempted = true;
                    job.diag
                        .info(LOG, "cancellation_requested", &job.event_fields(&[]));
                    match handle.cancel().await {
                        Ok(()) => {
                            let exit = handle.wait().await.map_err(|error| {
                                format!("cancelled but termination was not confirmed: {error}")
                            })?;
                            return Ok(MonitorOutcome {
                                exit,
                                cancellation_requested: true,
                                cancellation_succeeded: true,
                                cancellation_error: None,
                                supervision_error: None,
                            });
                        }
                        Err(error) => {
                            job.diag.error(
                                LOG,
                                "cancellation_failed",
                                &job.event_fields(&[("error", &error.to_string())]),
                            );
                            cancel_error = Some(error.to_string());
                        }
                    }
                }
                Ok(Some(CancellationStatus::Confirmed)) => {
                    attempted = true;
                    requested = true;
                    cancel_error =
                        Some("request was confirmed before this owner could consume it".into());
                }
                Ok(Some(CancellationStatus::Rejected | CancellationStatus::Failed)) => {
                    attempted = true;
                }
                Ok(None) => {}
                Err(error) => {
                    if !request_unreadable_logged {
                        request_unreadable_logged = true;
                        job.diag.error(
                            LOG,
                            "cancellation_request_unreadable",
                            &job.event_fields(&[("error", &error.to_string())]),
                        );
                    }
                }
            }
        }
        let event = tokio::select! {
            result = handle.next_event() => Some(result),
            _ = tokio::time::sleep(Duration::from_millis(100)) => None,
        };
        match event {
            None => continue,
            Some(Ok(Some(AgentEvent::SessionExited { .. }))) => {
                let exit = handle
                    .wait()
                    .await
                    .map_err(|error| format!("wait failed after exit event: {error}"))?;
                return Ok(MonitorOutcome {
                    exit,
                    cancellation_requested: requested,
                    cancellation_succeeded: succeeded,
                    cancellation_error: cancel_error,
                    supervision_error,
                });
            }
            Some(Ok(Some(_))) => {}
            Some(Ok(None)) => {
                supervision_error =
                    Some("agent event stream ended without a process exit event".to_string());
                break;
            }
            Some(Err(error)) => {
                supervision_error = Some(format!("agent event stream failed: {error}"));
                break;
            }
        }
    }

    if !requested
        && matches!(
            store.cancellation_request(owner),
            Ok(Some(CancellationStatus::Pending))
        )
    {
        requested = true;
    }
    match handle.cancel().await {
        Ok(()) => succeeded = true,
        Err(error) if cancel_error.is_none() => cancel_error = Some(error.to_string()),
        Err(_) => {}
    }
    let exit = handle.wait().await.map_err(|error| {
        format!(
            "{}; termination was not confirmed: {error}",
            supervision_error
                .as_deref()
                .unwrap_or("agent stream failed")
        )
    })?;
    Ok(MonitorOutcome {
        exit,
        cancellation_requested: requested,
        cancellation_succeeded: succeeded,
        cancellation_error: cancel_error,
        supervision_error,
    })
}

fn describe_exit(exit: &AgentExit) -> (&'static str, String, String) {
    let show = |code: &Option<i32>| code.map_or_else(|| "none".to_owned(), |c| c.to_string());
    match exit {
        AgentExit::Completed { code, .. } => ("completed", show(code), String::new()),
        AgentExit::Failed { code, reason } => {
            ("failed", show(code), reason.clone().unwrap_or_default())
        }
        AgentExit::Cancelled { code, .. } => ("cancelled", show(code), String::new()),
    }
}

/// Maps a reaped agent to exactly one terminal record set. Success requires a
/// completed turn and exit code zero; every other outcome fails the attempt.
fn finish(
    store: &SqliteStore,
    job: &Job,
    owner: &CancellationOwner,
    monitored: MonitorOutcome,
) -> Result<(), String> {
    let (outcome, code, reason) = describe_exit(&monitored.exit);
    job.diag.info(
        LOG,
        "agent_exited",
        &job.event_fields(&[
            ("outcome", outcome),
            ("exit_code", &code),
            ("reason", &reason),
            ("supervision_error", monitored.supervision_error.as_deref().unwrap_or("")),
            ("cancellation_error", monitored.cancellation_error.as_deref().unwrap_or("")),
        ]),
    );
    let resolution = if monitored.cancellation_error.is_some() {
        CancellationStatus::Failed
    } else {
        CancellationStatus::Rejected
    };
    let close = |status: CancellationStatus| {
        store
            .finish_cancellation_owner(owner, status)
            .map_err(|error| format!("cannot close cancellation owner: {error}"))
    };
    if let AgentExit::Cancelled { .. } = monitored.exit {
        if monitored.cancellation_requested && monitored.cancellation_succeeded {
            let confirmed = store
                .confirm_cancellation(owner)
                .map_err(|error| format!("cannot confirm cancellation: {error}"));
            if confirmed.is_ok() {
                let (worktree, uncommitted) = preserved_work(job);
                job.diag.info(
                    LOG,
                    "cancellation_confirmed",
                    &job.event_fields(&[("worktree", &worktree), ("uncommitted_paths", &uncommitted)]),
                );
            }
            return confirmed;
        }
        fail_after_agent_exit(store, job);
        return close(if monitored.cancellation_requested {
            CancellationStatus::Failed
        } else {
            CancellationStatus::Rejected
        });
    }
    if monitored.supervision_error.is_some() {
        fail_after_agent_exit(store, job);
        return close(resolution);
    }
    match monitored.exit {
        AgentExit::Completed { code: Some(0), .. } => {
            store
                .update_session_state(&job.task_id, &job.session_id, SessionState::Exited)
                .map_err(|error| format!("cannot mark session exited: {error}"))?;
            if let Err(error) = commit_candidate(&job.worktree, &job.task_id) {
                store
                    .update_attempt_state(&job.task_id, &job.attempt_id, AttemptState::Failed)
                    .map_err(|e| format!("cannot fail attempt: {e}"))?;
                store
                    .transition(&job.task_id, TaskEvent::ExecutionFailed)
                    .map_err(|e| format!("cannot fail task: {e}"))?;
                let (worktree, uncommitted) = preserved_work(job);
                job.diag.error(
                    LOG,
                    "candidate_commit_failed",
                    &job.event_fields(&[
                        ("error", &error),
                        ("worktree", &worktree),
                        ("uncommitted_paths", &uncommitted),
                        ("recovery", "worktree kept with the agent's changes; retry the task"),
                    ]),
                );
                close(resolution)?;
                return Err(format!("cannot record candidate commit: {error}"));
            }
            store
                .update_attempt_state(&job.task_id, &job.attempt_id, AttemptState::CandidateReady)
                .map_err(|error| format!("cannot mark attempt candidate-ready: {error}"))?;
            store
                .transition(&job.task_id, TaskEvent::CandidateReady)
                .map_err(|error| format!("cannot record candidate: {error}"))?;
            job.diag.info(LOG, "candidate_ready", &job.event_fields(&[]));
            close(resolution)
        }
        _ => {
            fail_after_agent_exit(store, job);
            close(resolution)
        }
    }
}

fn fail_before_agent_exit(store: &SqliteStore, job: &Job) {
    fail_records(store, job, SessionState::Aborted);
}

fn fail_after_agent_exit(store: &SqliteStore, job: &Job) {
    fail_records(store, job, SessionState::Exited);
}

/// Ends the session, attempt and task of a launch that produced no candidate.
/// Each write is attempted independently so one refusal cannot hide the rest.
/// The worktree is left in place and named in the event, so nothing the agent
/// wrote is discarded.
fn fail_records(store: &SqliteStore, job: &Job, session: SessionState) {
    let results = [
        store
            .update_session_state(&job.task_id, &job.session_id, session)
            .map_err(|error| format!("session: {error}")),
        store
            .update_attempt_state(&job.task_id, &job.attempt_id, AttemptState::Failed)
            .map_err(|error| format!("attempt: {error}")),
        store
            .transition(&job.task_id, TaskEvent::ExecutionFailed)
            .map(|_| ())
            .map_err(|error| format!("task: {error}")),
    ];
    let mut write_failed = false;
    for error in results.into_iter().filter_map(Result::err) {
        write_failed = true;
        job.diag.error(
            LOG,
            "failure_record_not_written",
            &job.event_fields(&[("error", &error)]),
        );
    }
    let (worktree, uncommitted) = preserved_work(job);
    job.diag.error(
        LOG,
        "task_attempt_failed",
        &job.event_fields(&[
            ("records_complete", if write_failed { "false" } else { "true" }),
            ("worktree", &worktree),
            ("uncommitted_paths", &uncommitted),
            ("recovery", "worktree kept; retry the task or inspect the worktree"),
        ]),
    );
}

fn git(cwd: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    Command::new("git")
        .args(["-c", "core.longpaths=true"])
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("cannot run git {}: {error}", args.join(" ")))
}

fn git_checked(cwd: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    let output = git(cwd, args)?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// Commits whatever the agent left in the worktree so the candidate is a
/// stable commit on the task branch. Never touches the primary checkout.
fn commit_candidate(worktree: &Path, task_id: &str) -> Result<(), String> {
    let status = git_checked(
        worktree,
        &["status", "--porcelain", "--untracked-files=all"],
    )?;
    if status.stdout.is_empty() {
        return Ok(());
    }
    git_checked(worktree, &["add", "--all", "--"])?;
    if git(worktree, &["diff", "--cached", "--quiet"])?
        .status
        .success()
    {
        return Ok(());
    }
    let message = format!("agent-orch candidate {task_id}");
    git_checked(
        worktree,
        &[
            "-c",
            "user.name=Agent Orchestrator",
            "-c",
            "user.email=agent-orch@localhost",
            "commit",
            "-m",
            &message,
        ],
    )
    .map(|_| ())
}

/// Terminates agents whose owner daemon is gone, using only an exact identity
/// match, then lets the reconcile gate record the interruption. Unix only: on
/// Windows the supervising Job Object has already killed the tree.
#[cfg(unix)]
fn terminate_orphaned_agents(store: &SqliteStore, diag: &Diagnostics) {
    use orchestrator_core::identity::{ProcessClassification, ProcessObservation};
    use orchestrator_runtime::reconcile::{observe_owner_process, observe_process};
    use orchestrator_runtime::supervisor::process_group_alive;

    let (Ok(tasks), Ok(owners)) = (store.list(), store.open_cancellation_owners()) else {
        return;
    };
    let owned: BTreeSet<String> = owners
        .into_iter()
        .filter(|owner| {
            owner.exact_process_identity().map(observe_owner_process)
                == Some(ProcessClassification::Alive)
        })
        .map(|owner| owner.task_id)
        .collect();
    for task in tasks
        .iter()
        .filter(|task| task.state == TaskState::Running && !owned.contains(&task.contract.id))
    {
        for session in task
            .sessions
            .iter()
            .filter(|session| !session.state.is_terminal())
        {
            let Ok(Some(identity)) = store.session_identity(&task.contract.id, &session.id) else {
                continue;
            };
            let ProcessObservation::Alive(observed) = observe_process(identity) else {
                continue;
            };
            // Signal only the exact recorded process group leader.
            if observed != identity || identity.process_group != Some(identity.pid) {
                continue;
            }
            let group = -(identity.pid as i32);
            for (signal, patience) in [(libc::SIGTERM, 30), (libc::SIGKILL, 60)] {
                unsafe { libc::kill(group, signal) };
                for _ in 0..patience {
                    if !process_group_alive(identity.pid) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                if !process_group_alive(identity.pid) {
                    break;
                }
            }
            diag.warn(
                LOG,
                "orphan_terminated",
                &[
                    ("task_id", &task.contract.id),
                    ("session_id", &session.id),
                    ("pid", &identity.pid.to_string()),
                    (
                        "recovery",
                        "agent of a gone daemon stopped; reconcile records the interruption and keeps the worktree",
                    ),
                ],
            );
        }
    }
}

#[cfg(not(unix))]
fn terminate_orphaned_agents(_store: &SqliteStore, _diag: &Diagnostics) {}

#[cfg(all(test, unix))]
#[path = "executor_tests.rs"]
mod tests;
