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
use orchestrator_engine::reconcile::reconcile_before_scheduling;
use orchestrator_git::{create_task_worktree, Repository};
use orchestrator_runtime::reconcile::{capture_exact_process_identity, capture_process_identity};
use orchestrator_store::{
    CancellationOwner, CancellationStatus, SqliteStore, Store, UsageReceipt, UsageSource,
};

const CODEX_AGENT: &str = "codex";
const CLAUDE_AGENT: &str = "claude-code";
const BLOCKED_RETRY: Duration = Duration::from_secs(5);

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
        self.inner.stop.store(true, Ordering::SeqCst);
        self.inner.request_pass();
        if let Some(handle) = lock(&self.scheduler).take() {
            let _ = handle.join();
        }
    }
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
        let mut gate_open = false;
        let mut last_started_project = None;
        while !self.stop.load(Ordering::SeqCst) {
            *lock(&self.dirty) = false;
            if !gate_open {
                terminate_orphaned_agents(&self.store);
                match reconcile_before_scheduling(&self.store) {
                    Ok(()) => {
                        gate_open = true;
                        *lock(&self.blocked) = None;
                    }
                    Err(reason) => {
                        let mut blocked = lock(&self.blocked);
                        if blocked.as_deref() != Some(reason.as_str()) {
                            eprintln!("orchestrator-daemon: scheduling blocked: {reason}");
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
        let (tasks, workspaces, projects) = match (
            self.store.list(),
            self.store.list_workspaces(),
            self.store.list_projects(),
        ) {
            (Ok(tasks), Ok(workspaces), Ok(projects)) => (tasks, workspaces, projects),
            (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
                eprintln!("orchestrator-daemon: cannot read scheduler state: {error}");
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
                eprintln!("orchestrator-daemon: scheduler refused its snapshot: {error}");
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
        };
        if let Err(error) = self.prepare(task, number, job) {
            eprintln!("orchestrator-daemon: cannot launch task {task_id}: {error}");
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
}

fn supervise(inner: &Inner, job: Job) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!(
                "orchestrator-daemon: task {}: no async runtime: {error}",
                job.task_id
            );
            fail_before_agent_exit(&inner.store, &job);
            return;
        }
    };
    if let Err(error) = runtime.block_on(run_agent(inner, &job)) {
        eprintln!("orchestrator-daemon: task {}: {error}", job.task_id);
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
            return Err(format!("launch failed: {error}"));
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
        reap(&mut *handle).await;
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
            eprintln!(
                "orchestrator-daemon: task {}: owner identity unavailable: {error}",
                job.task_id
            );
            owner
        }
    };
    if let Err(error) = store.register_cancellation_owner(&owner) {
        reap(&mut *handle).await;
        fail_after_agent_exit(store, job);
        return Err(format!("cannot register cancellation owner: {error}"));
    }

    let monitored = monitor_agent(handle, store, &owner).await;
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
async fn reap(handle: &mut dyn AgentHandle) {
    let _ = handle.cancel().await;
    let _ = handle.wait().await;
}

struct MonitorOutcome {
    exit: AgentExit,
    cancellation_requested: bool,
    cancellation_succeeded: bool,
    cancellation_error: Option<String>,
    supervision_error: Option<String>,
}

/// Persists usage the agent reported so far. A usage failure is logged and
/// never fails or delays the run; the receipt key makes a repeat harmless.
fn record_usage(
    handle: &mut dyn AgentHandle,
    store: &SqliteStore,
    owner: &CancellationOwner,
    started_ms: u64,
) {
    for report in handle.take_usage() {
        let now = unix_now_ms();
        let receipt = UsageReceipt {
            task_id: owner.task_id.clone(),
            session_id: owner.session_id.clone(),
            source: UsageSource::ProviderReported,
            receipt_key: report.receipt_key,
            window_start_ms: started_ms,
            window_end_ms: report
                .occurred_at_ms
                .unwrap_or(now)
                .clamp(started_ms, now.max(started_ms)),
            reported_at_ms: now,
            input_tokens: report.input_tokens,
            cache_read_tokens: report.cache_read_tokens,
            cache_write_tokens: report.cache_write_tokens,
            output_tokens: report.output_tokens,
            reasoning_tokens: report.reasoning_tokens,
            cost: report.cost.map(|cost| (cost.nanos, cost.currency)),
            model: report.model,
        };
        if let Err(error) = store.record_usage_receipt(&receipt) {
            eprintln!(
                "orchestrator-daemon: cannot record usage for session {}: {error}",
                owner.session_id
            );
        }
    }
}

fn unix_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Drives the agent to exit. A cancellation request is consumed at most once,
/// and `cancellation_succeeded` is set only after the handle's cancel returned
/// Ok. Neither depends on any client connection.
async fn monitor_agent(
    mut handle: Box<dyn AgentHandle>,
    store: &SqliteStore,
    owner: &CancellationOwner,
) -> Result<MonitorOutcome, String> {
    let mut requested = false;
    let mut attempted = false;
    let mut succeeded = false;
    let mut cancel_error = None;
    let mut supervision_error = None;
    let started_ms = unix_now_ms();

    loop {
        if !attempted {
            match store.cancellation_request(owner) {
                Ok(Some(CancellationStatus::Pending)) => {
                    requested = true;
                    attempted = true;
                    match handle.cancel().await {
                        Ok(()) => {
                            let exit = handle.wait().await;
                            // Keep what the agent reported before it was stopped.
                            record_usage(&mut *handle, store, owner, started_ms);
                            let exit = exit.map_err(|error| {
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
                        Err(error) => cancel_error = Some(error.to_string()),
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
                Ok(None) | Err(_) => {}
            }
        }
        let event = tokio::select! {
            result = handle.next_event() => Some(result),
            _ = tokio::time::sleep(Duration::from_millis(100)) => None,
        };
        match event {
            None => continue,
            Some(Ok(Some(AgentEvent::SessionExited { .. }))) => {
                record_usage(&mut *handle, store, owner, started_ms);
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
            Some(Ok(Some(_))) => record_usage(&mut *handle, store, owner, started_ms),
            Some(Ok(None)) => {
                record_usage(&mut *handle, store, owner, started_ms);
                supervision_error =
                    Some("agent event stream ended without a process exit event".to_string());
                break;
            }
            Some(Err(error)) => {
                record_usage(&mut *handle, store, owner, started_ms);
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
    let exit = handle.wait().await;
    record_usage(&mut *handle, store, owner, started_ms);
    let exit = exit.map_err(|error| {
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

/// Maps a reaped agent to exactly one terminal record set. Success requires a
/// completed turn and exit code zero; every other outcome fails the attempt.
fn finish(
    store: &SqliteStore,
    job: &Job,
    owner: &CancellationOwner,
    monitored: MonitorOutcome,
) -> Result<(), String> {
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
            return store
                .confirm_cancellation(owner)
                .map_err(|error| format!("cannot confirm cancellation: {error}"));
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
                close(resolution)?;
                return Err(format!("cannot record candidate commit: {error}"));
            }
            store
                .update_attempt_state(&job.task_id, &job.attempt_id, AttemptState::CandidateReady)
                .map_err(|error| format!("cannot mark attempt candidate-ready: {error}"))?;
            store
                .transition(&job.task_id, TaskEvent::CandidateReady)
                .map_err(|error| format!("cannot record candidate: {error}"))?;
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
    for error in results.into_iter().filter_map(Result::err) {
        eprintln!(
            "orchestrator-daemon: task {}: cannot fail {error}",
            job.task_id
        );
    }
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
fn terminate_orphaned_agents(store: &SqliteStore) {
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
            eprintln!(
                "orchestrator-daemon: terminated orphaned agent of task {} session {}",
                task.contract.id, session.id
            );
        }
    }
}

#[cfg(not(unix))]
fn terminate_orphaned_agents(_store: &SqliteStore) {}

#[cfg(all(test, unix))]
#[path = "executor_tests.rs"]
mod tests;
