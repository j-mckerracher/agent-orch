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

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError, RwLock, RwLockReadGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::activity::{ActivityBoard, Coalescer, Phase, ToolEnd, ToolStart};
use crate::api::admission::{AdmissionConfig, AdmissionGate};
use orchestrator_agents::cli_compat::{
    CliHelp, CompatibilityCache, CompatibilityError, Executable,
};
use orchestrator_agents::workflow_agent::{self, WorkflowAgent};
use orchestrator_agents::{
    locate_program, pi_catalog, resume_refusal, task_session, AgentAdapter, AgentCatalog,
    AgentEvent, AgentExit, AgentHandle, AgentLaunchRequest, ClaudeCodeAdapter, CodexAdapter,
    CopilotAdapter, CopilotRun, CopilotSignIn, TaskSession, COPILOT_AGENT,
};
use orchestrator_core::contract::{ModelSelection, TaskKind};
use orchestrator_core::permission::PermissionLevel;
use orchestrator_core::records::Workspace;
use orchestrator_core::scheduler::{Scheduler, SchedulerLimits, SchedulerSnapshot, TaskSnapshot};
use orchestrator_core::task::{
    Attempt, AttemptState, Session, SessionState, TaskEvent, TaskState, WaitReason,
};
use orchestrator_core::worker_status::{
    self as status_rules, Settlement, StatusVerb, MAX_QUESTIONS_PER_WAIT,
};
use orchestrator_diagnostics::Diagnostics;
use orchestrator_engine::reconcile::reconcile_before_scheduling_with;
use orchestrator_engine::verify::{pending_verifications, verify_attempt, Verified};
use orchestrator_git::{
    create_task_worktree, is_safe_branch_name, release_task_worktree, BranchPrefix,
    Error as GitError, Released, Repository, Treehouse,
};
use orchestrator_runtime::reconcile::{capture_exact_process_identity, capture_process_identity};
use orchestrator_store::{
    CancellationOwner, CancellationStatus, SqliteStore, Store, StoreError, TaskRecord,
    UsageReceipt, UsageSource, WorkerQuestion,
};
use output_ring::{EndOnDrop, OutputStore, Stream};
use worker_status::StatusTail;

use crate::api::load::LoadHandle;

const CODEX_AGENT: &str = "codex";

/// How long shutdown waits for interrupted verification runs to unregister.
const VERIFICATION_SHUTDOWN_WAIT: Duration = Duration::from_secs(2);
const CLAUDE_AGENT: &str = "claude-code";
const BLOCKED_RETRY: Duration = Duration::from_secs(5);
const LOG: &str = "executor";
/// Under the data directory, one private directory per Copilot run.
pub(crate) const COPILOT_RUNS_DIR: &str = "copilot-runs";

/// Launch settings. `*_program` overrides the executable resolved from PATH.
#[derive(Clone)]
pub(crate) struct ExecutorConfig {
    pub(crate) worktrees_root: PathBuf,
    /// Data directory holding `config/branch-prefix`, read at every launch.
    pub(crate) data_dir: PathBuf,
    pub(crate) codex_program: Option<PathBuf>,
    pub(crate) claude_program: Option<PathBuf>,
    pub(crate) pi_program: Option<PathBuf>,
    pub(crate) copilot_program: Option<PathBuf>,
    /// The bundled treehouse program that leases each task's worktree.
    pub(crate) treehouse_program: PathBuf,
    /// Whether Copilot runs pass `--max-ai-credits`, a soft cap. Off unless the
    /// daemon starts with `AGENT_ORCH_COPILOT_CREDIT_CAP=1` (DPES-1105 O2).
    pub(crate) copilot_credit_cap: bool,
    /// Copilot sign-in state for this daemon session. Shared by the catalog,
    /// the explicit check, and every Copilot run.
    pub(crate) copilot_sign_in: Arc<CopilotSignIn>,
    pub(crate) compatibility: Arc<CompatibilityCache>,
    pub(crate) limits: SchedulerLimits,
    /// Host-resource thresholds checked before each start; all off by default.
    pub(crate) admission: AdmissionConfig,
    pub(crate) poll: Duration,
    #[cfg(test)]
    pub(crate) wait_for_exit_before_identity: bool,
    /// Upper bound for one `agents.capabilities` probe of an agent binary.
    pub(crate) probe_timeout: Duration,
    /// Where failures and recovery actions are reported (stderr in the daemon).
    pub(crate) diagnostics: Diagnostics,
    /// Upper bound for each CLI probe of a catalog read. Reads run in the
    /// background, so a slow CLI gets the launch bound and its catalog is cached.
    pub(crate) catalog_timeout: Duration,
    /// How long a caller waits for a catalog read before it is told the
    /// catalog is not known yet. Must stay well under the 2 s client I/O
    /// timeout. The read itself continues in the background and fills the
    /// cache for the retry.
    pub(crate) catalog_wait: Duration,
    /// The machine load gate the scheduler pass asks before it starts a task.
    pub(crate) load: Arc<LoadHandle>,
    /// Whether the host sampler feeds `load`. Off in unit tests, and off when the
    /// daemon starts with `AGENT_ORCH_LOAD_GATE=off`, so a spawned test daemon
    /// does not read the real machine's load.
    pub(crate) load_feed: bool,
    /// Finds the pstack workflow agent each attempt runs as. Called per launch, so a
    /// plugin installed after the daemon starts is picked up. Tests inject a fixture.
    pub(crate) locate_workflow_agent: fn() -> Option<WorkflowAgent>,
}

/// No client waits on a launch, so its catalog check can wait for a slow CLI.
const LAUNCH_CATALOG_TIMEOUT: Duration = Duration::from_secs(10);

/// Replaces control characters in text taken from a request so it cannot
/// forge or hide log lines.
pub(crate) fn log_safe(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

impl ExecutorConfig {
    pub(crate) fn locate_agent(&self, agent: &str) -> Option<PathBuf> {
        match agent {
            CODEX_AGENT => CodexAdapter::new()
                .with_program(self.codex_program.as_deref().unwrap_or(Path::new("codex")))
                .locate(),
            CLAUDE_AGENT => ClaudeCodeAdapter::new()
                .with_program(
                    self.claude_program
                        .as_deref()
                        .unwrap_or(Path::new("claude")),
                )
                .locate(),
            COPILOT_AGENT => CopilotAdapter::new()
                .with_program(
                    self.copilot_program
                        .as_deref()
                        .unwrap_or(Path::new("copilot")),
                )
                .locate(),
            _ => None,
        }
    }

    pub(crate) async fn help_for(
        &self,
        agent: &str,
        program: &Path,
        executable: &Executable,
    ) -> Option<Arc<CliHelp>> {
        self.compatibility
            .help(agent, program, executable, self.probe_timeout)
            .await
    }

    pub(crate) async fn check_launch(
        &self,
        agent: &str,
        program: &Path,
        executable: &Executable,
        args: &[String],
    ) -> Result<(), CompatibilityError> {
        self.help_for(agent, program, executable)
            .await
            .ok_or(CompatibilityError::Unconfirmed)?
            .check(agent, args)
    }

    /// The workflow agent the installed CLI for `agent` can take, and the flag it lacks when it
    /// cannot. A CLI whose help lacks one of these flags cannot run the agent, so the attempt
    /// launches plain. Codex takes the agent through `-c`, which every Codex CLI offers. Without
    /// help (the probe failed) the agent is kept, and the launch check refuses the CLI.
    pub(crate) async fn offered_workflow_agent(
        &self,
        agent: &str,
        program: &Path,
        executable: &Executable,
        workflow_agent: Option<WorkflowAgent>,
    ) -> (Option<WorkflowAgent>, Option<&'static str>) {
        let Some(workflow_agent) = workflow_agent else {
            return (None, None);
        };
        if !matches!(agent, CLAUDE_AGENT | COPILOT_AGENT) {
            return (Some(workflow_agent), None);
        }
        let Some(help) = self.help_for(agent, program, executable).await else {
            return (Some(workflow_agent), None);
        };
        match ["--plugin-dir", "--agent"]
            .into_iter()
            .find(|flag| !help.offers(flag))
        {
            Some(flag) => (None, Some(flag)),
            None => (Some(workflow_agent), None),
        }
    }

    /// Reads one agent's catalog from its installed CLI. An agent the daemon
    /// cannot launch with a selection is reported `unsupported`.
    pub(crate) async fn read_catalog(
        &self,
        agent: &str,
        working_dir: &Path,
        limit: Duration,
    ) -> AgentCatalog {
        match agent {
            CODEX_AGENT => {
                let mut adapter = CodexAdapter::new();
                if let Some(program) = &self.codex_program {
                    adapter = adapter.with_program(program);
                }
                adapter.catalog(working_dir, limit).await
            }
            CLAUDE_AGENT => {
                let mut adapter = ClaudeCodeAdapter::new();
                if let Some(program) = &self.claude_program {
                    adapter = adapter.with_program(program);
                }
                adapter.catalog(working_dir, limit).await
            }
            "pi" => pi_catalog(),
            COPILOT_AGENT => {
                let mut adapter = CopilotAdapter::new();
                if let Some(program) = &self.copilot_program {
                    adapter = adapter.with_program(program);
                }
                adapter
                    .catalog(limit, &self.copilot_sign_in.snapshot())
                    .await
            }
            other => AgentCatalog::unsupported_agent(
                other,
                "the daemon has no launcher for this agent, so selection is unsupported",
            ),
        }
    }
}

/// Whether the host sampler feeds the load gate. Read once, from the daemon's own
/// environment, which an agent process cannot set (DPES-1105 Q17).
pub(crate) fn load_feed_enabled(setting: Option<&str>, under_test: bool) -> bool {
    !under_test && setting != Some("off")
}

impl ExecutorConfig {
    pub(crate) fn for_data_dir(data_dir: &Path) -> Self {
        let diagnostics = Diagnostics::stderr();
        let load_setting = std::env::var("AGENT_ORCH_LOAD_GATE").ok();
        let load_feed = load_feed_enabled(load_setting.as_deref(), cfg!(test));
        if !cfg!(test) && !load_feed {
            diagnostics.warn(LOG, "load_gate_feed_disabled", &[("source", "environment")]);
        }
        Self {
            worktrees_root: data_dir.join("worktrees"),
            data_dir: data_dir.to_path_buf(),
            codex_program: std::env::var_os("AGENT_ORCH_CODEX_PROGRAM").map(PathBuf::from),
            claude_program: std::env::var_os("AGENT_ORCH_CLAUDE_PROGRAM").map(PathBuf::from),
            pi_program: std::env::var_os("AGENT_ORCH_PI_PROGRAM").map(PathBuf::from),
            copilot_program: std::env::var_os("AGENT_ORCH_COPILOT_PROGRAM").map(PathBuf::from),
            treehouse_program: treehouse_program(),
            copilot_credit_cap: std::env::var("AGENT_ORCH_COPILOT_CREDIT_CAP").as_deref()
                == Ok("1"),
            copilot_sign_in: Arc::default(),
            limits: SchedulerLimits::default(),
            admission: AdmissionConfig::from_env(),
            poll: Duration::from_millis(500),
            compatibility: Arc::default(),
            #[cfg(test)]
            wait_for_exit_before_identity: false,
            // Leaves headroom under the 2 s client I/O timeout.
            probe_timeout: Duration::from_millis(1500),
            diagnostics,
            catalog_timeout: LAUNCH_CATALOG_TIMEOUT,
            catalog_wait: Duration::from_millis(1100),
            load: Arc::default(),
            load_feed,
            locate_workflow_agent: workflow_agent::locate,
        }
    }
}

/// The bundled treehouse beside the daemon executable. Debug builds first honor
/// `AGENT_ORCH_TREEHOUSE_PROGRAM` and then PATH, as the agent programs do. Release
/// builds never run a path taken from the environment or PATH.
fn treehouse_program() -> PathBuf {
    let name = if cfg!(windows) {
        "treehouse.exe"
    } else {
        "treehouse"
    };
    if cfg!(debug_assertions) {
        if let Some(path) =
            std::env::var_os("AGENT_ORCH_TREEHOUSE_PROGRAM").filter(|v| !v.is_empty())
        {
            return PathBuf::from(path);
        }
        if let Some(found) = locate_program(Path::new(name)) {
            return found;
        }
    }
    // An empty path never spawns, so a missing sidecar fails the lease, never falls back to PATH.
    std::env::current_exe().map_or_else(|_| PathBuf::new(), |exe| exe.with_file_name(name))
}

/// The treehouse binary the tests lease worktrees with, from `AGENT_ORCH_TREEHOUSE_PROGRAM`.
#[cfg(test)]
pub(crate) fn test_treehouse_program() -> PathBuf {
    std::env::var_os("AGENT_ORCH_TREEHOUSE_PROGRAM")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            panic!(
                "AGENT_ORCH_TREEHOUSE_PROGRAM is not set; fetch treehouse with \
                 `node apps/desktop/scripts/prepare-treehouse-sidecar.mjs --dest target/treehouse` \
                 and point the variable at the binary"
            )
        })
}

/// Admission gate for an idle shutdown: `false` while the daemon serves, `true` once an accepted
/// shutdown closed it. Work holds a read guard while it runs, and the shutdown decision takes the
/// write lock, so work either began before the decision (and vetoes it) or never begins.
pub(crate) type StopGate = Arc<RwLock<bool>>;

/// A read guard while the daemon still serves; `None` once the gate is closed.
pub(crate) fn serving(gate: &RwLock<bool>) -> Option<RwLockReadGuard<'_, bool>> {
    let guard = gate.read().unwrap_or_else(PoisonError::into_inner);
    (!*guard).then_some(guard)
}

pub(crate) struct Executor {
    inner: Arc<Inner>,
    scheduler: Mutex<Option<JoinHandle<()>>>,
}

struct Inner {
    store: Arc<SqliteStore>,
    config: ExecutorConfig,
    /// Leases every task worktree from the bundled pool.
    treehouse: Treehouse,
    /// True from a wake request until the next scheduling pass begins.
    dirty: Mutex<bool>,
    wake: Condvar,
    stop: AtomicBool,
    /// Why scheduling is blocked by the restart gate, if it is.
    blocked: Mutex<Option<String>>,
    /// Project that received the last start; mirrors the loop's fairness
    /// cursor so read models order the queue as the next pass would.
    cursor: Mutex<Option<String>>,
    /// What running sessions are doing, updated by `monitor_agent`. Shared with the chat turns
    /// and the wake scheduler's stall check.
    board: Arc<ActivityBoard>,
    admission: AdmissionGate,
    output: Arc<OutputStore>,
    /// Cancel flags of the verification runs in flight, keyed by task id.
    verifying: Mutex<BTreeMap<String, Arc<AtomicBool>>>,
    /// Supervisor and resumed-verification threads that have started and not ended.
    workers: AtomicUsize,
    stop_gate: StopGate,
}

/// Counts one supervisor or verification thread in `Inner::workers` from before it is spawned
/// until it ends, so an idle shutdown never sees a gap between the hand-off and the thread.
struct WorkerSlot(Arc<Inner>);

impl WorkerSlot {
    fn new(inner: &Arc<Inner>) -> Self {
        inner.workers.fetch_add(1, Ordering::AcqRel);
        Self(inner.clone())
    }
}

impl Drop for WorkerSlot {
    fn drop(&mut self) {
        self.0.workers.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Executor {
    /// [`Self::start_gated`] with a gate nothing closes.
    #[cfg(test)]
    pub(crate) fn start(store: Arc<SqliteStore>, config: ExecutorConfig) -> Result<Self, String> {
        Self::start_gated(store, config, StopGate::default(), Arc::default())
    }

    pub(crate) fn start_gated(
        store: Arc<SqliteStore>,
        config: ExecutorConfig,
        stop_gate: StopGate,
        board: Arc<ActivityBoard>,
    ) -> Result<Self, String> {
        let admission = AdmissionGate::new(config.admission.clone(), config.data_dir.clone());
        let treehouse = Treehouse::new(&config.treehouse_program, &config.data_dir)
            .map_err(|error| format!("cannot prepare treehouse: {error}"))?;
        let inner = Arc::new(Inner {
            store,
            config,
            treehouse,
            admission,
            dirty: Mutex::new(true),
            wake: Condvar::new(),
            stop: AtomicBool::new(false),
            blocked: Mutex::new(None),
            cursor: Mutex::new(None),
            board,
            output: Arc::default(),
            verifying: Mutex::default(),
            workers: AtomicUsize::new(0),
            stop_gate,
        });
        // A weak reference: the gate outlives the executor and must not keep it alive.
        let weak = Arc::downgrade(&inner);
        inner.config.load.on_open(move || {
            if let Some(inner) = weak.upgrade() {
                inner.request_pass();
            }
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

    /// The registry the wake scheduler's stall check reads.
    pub(crate) fn board(&self) -> Arc<ActivityBoard> {
        self.inner.board.clone()
    }

    /// Asks the scheduler to run a pass now (task admitted, retried, finished).
    pub(crate) fn wake(&self) {
        self.inner.request_pass();
    }

    /// Returns the treehouse slots of finished tasks to the pool. An in-place workspace
    /// holds no slot and is skipped. A dirty slot stays leased for inspection. Every
    /// outcome is logged; none reaches the caller.
    pub(crate) fn release_workspaces(&self, workspaces: &[Workspace]) {
        let inner = &*self.inner;
        let diagnostics = &inner.config.diagnostics;
        for workspace in workspaces
            .iter()
            .filter(|workspace| !workspace.is_in_place())
        {
            let (task_id, workspace_id) = (workspace.task_id.as_str(), workspace.id.as_str());
            let released = release_task_worktree(
                &inner.treehouse,
                &inner.config.worktrees_root,
                Path::new(&workspace.path),
                &workspace.id,
            );
            match released {
                Ok(Released::Returned) => {}
                Ok(Released::Dirty) => diagnostics.warn(
                    LOG,
                    "worktree_kept_leased",
                    &[("task_id", task_id), ("workspace_id", workspace_id)],
                ),
                Err(error) => diagnostics.error(
                    LOG,
                    "worktree_release_failed",
                    &[
                        ("task_id", task_id),
                        ("workspace_id", workspace_id),
                        ("error", &error.to_string()),
                    ],
                ),
            }
        }
    }

    /// Interrupts the verification run of `task_id`, if one is in flight. The
    /// run records nothing; the caller settles the cancellation.
    pub(crate) fn cancel_verification(&self, task_id: &str) {
        if let Some(flag) = lock(&self.inner.verifying).get(task_id) {
            flag.store(true, Ordering::SeqCst);
        }
    }

    /// True while a requested pass has not started, so the daemon cannot idle
    /// out between an admission and its launch.
    pub(crate) fn has_pending_pass(&self) -> bool {
        *lock(&self.inner.dirty)
    }

    /// True while a supervisor or verification thread is running.
    pub(crate) fn has_live_workers(&self) -> bool {
        self.inner.workers.load(Ordering::Acquire) != 0
    }

    /// The next scheduling pass's fairness cursor and whether the restart gate
    /// currently blocks every launch. Read-only; for the queue snapshot.
    pub(crate) fn dispatch_state(&self) -> (Option<String>, bool) {
        (
            lock(&self.inner.cursor).clone(),
            lock(&self.inner.blocked).is_some(),
        )
    }

    /// `admission` object for `host.metrics`: thresholds, last decision and
    /// deferred task ids.
    pub(crate) fn admission_json(&self) -> serde_json::Value {
        self.inner.admission.to_json()
    }

    /// Recent output of a session, read-only; see `output_ring.rs`.
    pub(crate) fn output(&self) -> &OutputStore {
        &self.inner.output
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
        let diag = &self.inner.config.diagnostics;
        match running_task_ids(&self.inner.store, diag, "shutdown") {
            Some(running) if running.is_empty() => {}
            running => {
                // An unreadable store is reported as unknown, never as no agents.
                let task_ids = running.map_or_else(|| "unknown".to_owned(), |ids| ids.join(","));
                diag.warn(
                    LOG,
                    "shutdown_with_running_agents",
                    &[
                        ("task_ids", &task_ids),
                        (
                            "recovery",
                            "agents are reconciled and their worktrees kept on the next daemon start",
                        ),
                    ],
                );
            }
        }
        self.inner.stop.store(true, Ordering::SeqCst);
        self.inner.request_pass();
        self.inner.interrupt_verifications();
        if let Some(handle) = lock(&self.scheduler).take() {
            let _ = handle.join();
        }
    }
}

/// Ids of tasks recorded as Running, or `None` after logging why the store
/// could not be listed. Callers must not treat `None` as "nothing running".
fn running_task_ids(
    store: &SqliteStore,
    diagnostics: &Diagnostics,
    context: &str,
) -> Option<Vec<String>> {
    running_ids_from(store.list(), diagnostics, context)
}

fn running_ids_from(
    listing: Result<Vec<TaskRecord>, StoreError>,
    diagnostics: &Diagnostics,
    context: &str,
) -> Option<Vec<String>> {
    match listing {
        Ok(tasks) => Some(
            tasks
                .into_iter()
                .filter(|task| task.state == TaskState::Running)
                .map(|task| task.contract.id)
                .collect(),
        ),
        Err(error) => {
            diagnostics.error(
                LOG,
                "running_tasks_unreadable",
                &[
                    ("context", context),
                    ("error", &error.to_string()),
                    (
                        "effect",
                        "running agents cannot be listed; their state is reported as unknown",
                    ),
                ],
            );
            None
        }
    }
}

/// Ids in `before` that are no longer in `after`, or `None` when either
/// listing failed.
fn interrupted_between(before: Option<&[String]>, after: Option<&[String]>) -> Option<Vec<String>> {
    let (before, after) = (before?, after?);
    Some(
        before
            .iter()
            .filter(|id| !after.contains(id))
            .cloned()
            .collect(),
    )
}

fn join_or_unknown(ids: Option<&[String]>) -> String {
    ids.map_or_else(|| "unknown".to_owned(), |ids| ids.join(","))
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// A verification run registered in `Inner::verifying`. Dropping it removes the
/// entry, so a run that panics cannot leave one behind.
struct VerificationSlot<'a> {
    inner: &'a Inner,
    task_id: String,
    cancel: Arc<AtomicBool>,
}

impl Drop for VerificationSlot<'_> {
    fn drop(&mut self) {
        lock(&self.inner.verifying).remove(&self.task_id);
    }
}

impl Inner {
    fn request_pass(&self) {
        *lock(&self.dirty) = true;
        self.wake.notify_all();
    }

    /// Registers a verification run for `task_id`, or returns `None` when one is
    /// already in flight. The entry exists before the run reads any state.
    fn claim_verification(&self, task_id: &str) -> Option<VerificationSlot<'_>> {
        let mut verifying = lock(&self.verifying);
        if verifying.contains_key(task_id) {
            return None;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        verifying.insert(task_id.to_owned(), cancel.clone());
        Some(VerificationSlot {
            inner: self,
            task_id: task_id.to_owned(),
            cancel,
        })
    }

    /// Starts verification for every candidate a stopped daemon left `Verifying`.
    /// Runs once, when the restart gate first opens: no supervisor of this
    /// process exists yet, so nothing else settles those attempts.
    fn resume_verifications(self: &Arc<Self>) {
        let pending = match pending_verifications(&self.store) {
            Ok(pending) => pending,
            Err(error) => {
                self.config.diagnostics.error(
                    LOG,
                    "verification_resume_unreadable",
                    &[("error", &error)],
                );
                return;
            }
        };
        for (task_id, attempt_id) in pending {
            let slot = WorkerSlot::new(self);
            let name = format!("verify-{task_id}");
            let spawned = thread::Builder::new()
                .name(name.clone())
                .spawn(move || slot.0.verify_candidate(&task_id, &attempt_id));
            if let Err(error) = spawned {
                self.config.diagnostics.error(
                    LOG,
                    "verification_resume_failed",
                    &[
                        ("thread", &name),
                        ("error", &error.to_string()),
                        (
                            "recovery",
                            "task stays Verifying; the next daemon start resumes verification",
                        ),
                    ],
                );
            }
        }
    }

    /// Runs and settles the verification of one candidate attempt. Blocks for
    /// as long as the checks run, so it is never called on the async runtime.
    fn verify_candidate(&self, task_id: &str, attempt_id: &str) {
        let outcome = {
            let Some(slot) = self.claim_verification(task_id) else {
                return;
            };
            verify_attempt(&self.store, task_id, attempt_id, &slot.cancel)
        };
        let diag = &self.config.diagnostics;
        match outcome {
            Ok(Verified::Passed { run_id }) => diag.info(
                LOG,
                "verification_passed",
                &[
                    ("task_id", task_id),
                    ("attempt_id", attempt_id),
                    ("run_id", &run_id),
                ],
            ),
            Ok(Verified::Failed { run_id, reason }) => diag.info(
                LOG,
                "verification_failed",
                &[
                    ("task_id", task_id),
                    ("attempt_id", attempt_id),
                    ("run_id", &run_id),
                    ("reason", &reason),
                ],
            ),
            Ok(Verified::Superseded) => diag.info(
                LOG,
                "verification_superseded",
                &[("task_id", task_id), ("attempt_id", attempt_id)],
            ),
            Ok(Verified::Interrupted) => diag.info(
                LOG,
                "verification_interrupted",
                &[("task_id", task_id), ("attempt_id", attempt_id)],
            ),
            Err(error) => diag.error(
                LOG,
                "verification_error",
                &[
                    ("task_id", task_id),
                    ("attempt_id", attempt_id),
                    ("error", &error),
                    (
                        "recovery",
                        "task stays Verifying; the next daemon start resumes verification",
                    ),
                ],
            ),
        }
        self.request_pass();
    }

    /// Sets the cancel flag of every verification run in flight, then waits up
    /// to [`VERIFICATION_SHUTDOWN_WAIT`] for them to unregister.
    fn interrupt_verifications(&self) {
        for flag in lock(&self.verifying).values() {
            flag.store(true, Ordering::SeqCst);
        }
        let deadline = Instant::now() + VERIFICATION_SHUTDOWN_WAIT;
        while !lock(&self.verifying).is_empty() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn run(self: Arc<Self>) {
        let diag = &self.config.diagnostics;
        let mut gate_open = false;
        let mut last_started_project = None;
        while !self.stop.load(Ordering::SeqCst) {
            *lock(&self.dirty) = false;
            // Held for the whole pass but not for the wait below.
            let pass_guard = serving(&self.stop_gate);
            if pass_guard.is_some() && !gate_open {
                terminate_orphaned_agents(&self.store, diag);
                let running_before = running_task_ids(&self.store, diag, "before_recovery_gate");
                match reconcile_before_scheduling_with(&self.store, diag) {
                    Ok(()) => {
                        gate_open = true;
                        *lock(&self.blocked) = None;
                        let running_after =
                            running_task_ids(&self.store, diag, "after_recovery_gate");
                        let interrupted = interrupted_between(
                            running_before.as_deref(),
                            running_after.as_deref(),
                        );
                        let interrupted_ids = join_or_unknown(interrupted.as_deref());
                        let still_running = join_or_unknown(running_after.as_deref());
                        let fields = [
                            ("interrupted_task_ids", interrupted_ids.as_str()),
                            ("still_running_task_ids", still_running.as_str()),
                        ];
                        if interrupted.is_some() && running_after.is_some() {
                            diag.info(LOG, "recovery_gate_open", &fields);
                        } else {
                            diag.warn(LOG, "recovery_gate_open", &fields);
                        }
                        self.resume_verifications();
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
            if pass_guard.is_some() && gate_open {
                self.schedule_pass(&mut last_started_project);
                *lock(&self.cursor) = last_started_project.clone();
            }
            let wait = if gate_open {
                self.config.poll
            } else {
                BLOCKED_RETRY
            };
            drop(pass_guard);
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
        // A task with a question open for the user does not start (DPES-1108 phase 3), and a
        // waiting task resumes only with a reply to deliver (phase 4).
        let open_questions = match self
            .store
            .tasks_with_open_questions()
            .and_then(|open| Ok((open, self.store.tasks_ready_to_resume()?)))
        {
            Ok(open_questions) => open_questions,
            Err(error) => {
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
        // A closed load gate leaves every task Queued; running tasks are not touched.
        let (open_questions, resumable) = open_questions;
        let scheduler = scheduler
            .with_start_gate(self.config.load.start_gate(Instant::now()))
            .with_open_questions(&open_questions)
            .with_resumable(&resumable);
        let plan = scheduler.schedule(Some(&authorization));
        let mut running = snapshots
            .iter()
            .filter(|snapshot| snapshot.state == TaskState::Running)
            .count();
        let mut deferred = Vec::new();
        let mut last_decision = None;
        let mut launched_project = None;
        for task_id in &plan.started {
            let Some(task) = tasks.iter().find(|task| &task.contract.id == task_id) else {
                continue;
            };
            let decision = self.admission.check(running);
            if !decision.admitted {
                deferred.push(task_id.clone());
                last_decision = Some(decision);
                continue;
            }
            last_decision = Some(decision);
            running += 1;
            launched_project = Some(task.contract.project_id.clone());
            if task.state == TaskState::Waiting {
                self.resume(task);
            } else {
                self.launch(task);
            }
        }
        if deferred.is_empty() {
            *last_started_project = plan.last_started_project;
        } else if launched_project.is_some() {
            *last_started_project = launched_project;
        }
        let changed = self
            .admission
            .record(last_decision.as_ref(), deferred.clone());
        if let (true, Some(decision)) = (changed, &last_decision) {
            if !decision.admitted {
                diag.warn(
                    LOG,
                    "start_deferred",
                    &[
                        ("reason_code", decision.reason_code.as_str()),
                        ("deferred_task_ids", &deferred.join(",")),
                    ],
                );
            }
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
            model_selection: task.contract.model_selection.clone(),
            permission: task.contract.permission,
            worktree: PathBuf::new(),
            in_place: false,
            status_dir: worker_status::status_dir(&self.config.data_dir, &task_id, number),
            session: JobSession::New(uuid::Uuid::new_v4().hyphenated().to_string()),
            diag: self.config.diagnostics.clone(),
            output: self.output.clone(),
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
        // The lease holder of this attempt's worktree, and the workspace record that names it.
        let workspace_id = format!("{task_id}-workspace-{number}");
        // The home directory is a project whose tasks run in the directory itself, unless the
        // directory has since become a Git repository: then it gets a worktree like any other.
        let mut in_place = store
            .get_project(&task.contract.project_id)
            .is_ok_and(|project| project.is_in_place());
        if in_place && Repository::new(Path::new(&task.contract.project_id)).is_ok() {
            store
                .clear_in_place(&task.contract.project_id)
                .map_err(|error| format!("cannot clear in-place flag: {error}"))?;
            in_place = false;
        }
        job.in_place = in_place;
        let checkout = if in_place {
            in_place_checkout(&task.contract.project_id)
        } else {
            Repository::new(Path::new(&task.contract.project_id)).and_then(|repository| {
                std::fs::create_dir_all(&self.config.worktrees_root).map_err(GitError::Io)?;
                let root =
                    std::fs::canonicalize(&self.config.worktrees_root).map_err(GitError::Io)?;
                // A project's own base branch and prefix win over HEAD and the machine-wide
                // prefix (DPES-1105). A task already running keeps the worktree it has.
                let defaults = store
                    .project_defaults(&task.contract.project_id)
                    .map_err(|error| GitError::Io(std::io::Error::other(error.to_string())))?;
                let prefix = match defaults
                    .as_ref()
                    .and_then(|row| row.branch_prefix.as_deref())
                {
                    Some(value) => BranchPrefix::parse_for_project(value)?,
                    None => BranchPrefix::read(&self.config.data_dir)?,
                };
                // The stored value passed a looser database check, so it is re-checked here, and
                // named under `refs/heads/` so a tag of the same name cannot decide the base.
                let base_ref = match defaults.as_ref().and_then(|row| row.base_branch.as_deref()) {
                    Some(branch) if is_safe_branch_name(branch) => format!("refs/heads/{branch}"),
                    Some(branch) => {
                        return Err(GitError::InvalidBaseRef {
                            refspec: branch.to_owned(),
                            detail: "not a safe branch name".to_owned(),
                        });
                    }
                    None => "HEAD".to_owned(),
                };
                let worktree = create_task_worktree(
                    &repository,
                    &task_id,
                    &base_ref,
                    &root,
                    &prefix,
                    &self.treehouse,
                    &workspace_id,
                )?;
                Ok(Checkout {
                    path: worktree.path().to_path_buf(),
                    branch: worktree.branch().to_string(),
                    base_commit: worktree.base_sha().to_string(),
                })
            })
        };
        store
            .add_attempt(
                &task_id,
                &Attempt {
                    id: job.attempt_id.clone(),
                    task_id: task_id.clone(),
                    attempt_number: number,
                    state: AttemptState::Pending,
                    failure_reason: None,
                    summary: None,
                    changed_files: None,
                },
            )
            .map_err(|error| format!("cannot record attempt: {error}"))?;
        // The status file is the worker's only channel back (DPES-1108 phase 4).
        let prepared = checkout
            .map_err(|error| {
                let reason = if in_place {
                    HOME_DIRECTORY_MISSING
                } else {
                    worktree_failure_reason(&error)
                };
                (reason, format!("cannot create worktree: {error}"))
            })
            .and_then(|checkout| {
                worker_status::prepare(&job.status_dir)
                    .map(|file| (checkout, file))
                    .map_err(|error| {
                        (
                            STATUS_FILE_FAILED,
                            format!("cannot prepare the status file: {error}"),
                        )
                    })
            });
        let (checkout, status_file) = match prepared {
            Ok(prepared) => prepared,
            Err((reason, detail)) => {
                // No process exists yet: take the attempt through Running so
                // it can end Failed, and fail the task so it can be retried.
                store
                    .transition(&task_id, TaskEvent::ExecutionStarted)
                    .map_err(|e| format!("cannot start task: {e}"))?;
                store
                    .update_attempt_state(&task_id, &job.attempt_id, AttemptState::Running)
                    .map_err(|e| format!("cannot start attempt: {e}"))?;
                store
                    .record_attempt_failure(&task_id, &job.attempt_id, reason)
                    .map_err(|e| format!("cannot fail attempt: {e}"))?;
                store
                    .transition(&task_id, TaskEvent::ExecutionFailed)
                    .map_err(|e| format!("cannot fail task: {e}"))?;
                return Err(detail);
            }
        };
        job.worktree = checkout.path.clone();
        // The agent launches with the brief, never the bare objective (DPES-1108).
        job.objective = orchestrator_core::brief::render(
            &task.contract,
            (!in_place).then_some(checkout.path.as_path()),
            &status_file,
        );
        let path = checkout.path.display().to_string();
        let workspace = if in_place {
            Workspace::in_place(workspace_id, task_id.clone(), path)
        } else {
            Workspace {
                id: workspace_id,
                task_id: task_id.clone(),
                path,
                branch: checkout.branch,
                base_commit: checkout.base_commit,
            }
        };
        store
            .add_workspace(&workspace)
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
        self.start_supervisor(job)
    }

    /// Hands a recorded session to its own supervisor thread.
    fn start_supervisor(self: &Arc<Self>, job: Job) -> Result<(), String> {
        let slot = WorkerSlot::new(self);
        let supervisor_job = job.clone();
        let spawned = thread::Builder::new()
            .name(format!("agent-{}", job.task_id))
            .spawn(move || {
                supervise(&slot.0, supervisor_job);
                slot.0.request_pass();
            });
        if let Err(error) = spawned {
            fail_before_agent_exit(
                &self.store,
                &job,
                Some(
                    "The daemon could not start the agent supervisor. Check the daemon log for details.",
                ),
            );
            return Err(format!("cannot start supervisor thread: {error}"));
        }
        Ok(())
    }

    /// Resumes a waiting task the scheduler planned: the same agent session, in the same
    /// worktree, at the same permission level, with its replies as the next prompt
    /// (DPES-1108 phase 4). A task that cannot resume so fails with a reason the user reads.
    fn resume(self: &Arc<Self>, task: &TaskRecord) {
        let task_id = &task.contract.id;
        if let Err(error) = self.prepare_resume(task) {
            self.config.diagnostics.error(
                LOG,
                "resume_failed",
                &[("task_id", task_id), ("error", &error)],
            );
        }
    }

    fn prepare_resume(self: &Arc<Self>, task: &TaskRecord) -> Result<(), String> {
        let store = &*self.store;
        let contract = &task.contract;
        let task_id = contract.id.clone();
        let fail = |why: &str| {
            let reason = format!(
                "MCAO could not resume the waiting agent: {why}. It did not resume it any other \
                 way. Retry the task; the answers stay in its decisions."
            );
            store
                .fail_waiting_task(&task_id, &reason)
                .map_err(|error| format!("cannot fail the waiting task: {error}"))
        };
        let target = store
            .resume_target(&task_id)
            .map_err(|error| format!("cannot read the waiting attempt: {error}"))?
            .ok_or("the task has no waiting attempt")?;
        if let Some(refusal) = resume_refusal(&contract.agent, contract.permission) {
            return fail(refusal);
        }
        let Some(agent_session_id) = target.agent_session_id else {
            return fail("its CLI session id is not known");
        };
        let replies = store
            .pending_replies(&task_id)
            .map_err(|error| format!("cannot read the replies: {error}"))?;
        let rendered: Vec<_> = replies.iter().map(reply_for_worker).collect();
        let Some(prompt) = orchestrator_core::brief::render_resume(&rendered) else {
            return fail("the replies are too long to send in one turn");
        };
        // A missing worktree is permanent, so it fails the task instead of being planned again
        // on every pass. Only a store read that failed is retried.
        let Some(workspace) = store
            .list_workspaces()
            .map_err(|error| format!("cannot read the worktree: {error}"))?
            .into_iter()
            .rev()
            .find(|workspace| workspace.task_id == task_id)
        else {
            return fail("its worktree is no longer recorded");
        };
        let number = target.attempt_number;
        let job = Job {
            task_id: task_id.clone(),
            attempt_id: target.attempt_id.clone(),
            session_id: format!("{task_id}-session-{number}-{}", target.sessions + 1),
            agent: contract.agent.clone(),
            objective: prompt,
            model_selection: contract.model_selection.clone(),
            permission: contract.permission,
            worktree: PathBuf::from(&workspace.path),
            in_place: workspace.is_in_place(),
            status_dir: worker_status::status_dir(&self.config.data_dir, &task_id, number),
            session: JobSession::Resume(agent_session_id.clone()),
            diag: self.config.diagnostics.clone(),
            output: self.output.clone(),
        };
        let reply_ids: Vec<i64> = replies.iter().map(|reply| reply.id).collect();
        store
            .begin_resume(
                &task_id,
                &Session {
                    id: job.session_id.clone(),
                    attempt_id: job.attempt_id.clone(),
                    adapter: job.agent.clone(),
                    state: SessionState::Spawning,
                },
                &agent_session_id,
                &reply_ids,
            )
            .map_err(|error| format!("cannot start the resume: {error}"))?;
        job.diag.info(
            LOG,
            "worker_resumed",
            &job.event_fields(&[("replies", &reply_ids.len().to_string())]),
        );
        self.start_supervisor(job)
    }
}

/// A stored reply as the worker reads it.
fn reply_for_worker(reply: &orchestrator_store::PendingReply) -> orchestrator_core::brief::Reply {
    use orchestrator_core::brief::{Reply, ReplyKind};
    Reply {
        kind: match reply.kind {
            orchestrator_store::ReplyKind::Answer => ReplyKind::Answer,
            orchestrator_store::ReplyKind::Reconcile => ReplyKind::Reconcile,
            orchestrator_store::ReplyKind::Message => ReplyKind::Message,
            orchestrator_store::ReplyKind::UserMessage => ReplyKind::UserMessage,
        },
        question: reply.question.clone(),
        text: reply.text.clone(),
    }
}

/// Where a task runs: a fresh worktree, or (in place) the project directory itself.
struct Checkout {
    path: PathBuf,
    branch: String,
    base_commit: String,
}

/// The directory of an in-place project. No branch, no base commit, nothing created.
fn in_place_checkout(root: &str) -> Result<Checkout, GitError> {
    let path = PathBuf::from(root);
    if !path.is_dir() {
        return Err(GitError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "the project directory does not exist",
        )));
    }
    Ok(Checkout {
        path,
        branch: String::new(),
        base_commit: String::new(),
    })
}

/// The agent CLI session a run starts or continues. A new id is the daemon's choice for Claude
/// and Copilot; Codex ignores it and reports its own.
#[derive(Clone)]
enum JobSession {
    New(String),
    Resume(String),
}

impl JobSession {
    fn as_task_session(&self) -> TaskSession<'_> {
        match self {
            Self::New(id) => TaskSession::New(id),
            Self::Resume(id) => TaskSession::Resume(id),
        }
    }
}

#[derive(Clone)]
struct Job {
    task_id: String,
    attempt_id: String,
    session_id: String,
    agent: String,
    objective: String,
    model_selection: Option<ModelSelection>,
    permission: PermissionLevel,
    worktree: PathBuf,
    /// The task runs in its project's directory, not a worktree: nothing to commit.
    in_place: bool,
    /// The attempt's status directory, the one path outside the worktree the agent may write.
    status_dir: PathBuf,
    session: JobSession,
    diag: Diagnostics,
    output: Arc<OutputStore>,
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
    git_checked(
        worktree,
        &["status", "--porcelain", "--untracked-files=all"],
    )
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

/// Attempt failure reason when the directory of an in-place project is gone.
const HOME_DIRECTORY_MISSING: &str =
    "The project directory no longer exists. Check the home directory in Settings, then retry.";
/// Attempt failure reason when the attempt's status file cannot be made.
const STATUS_FILE_FAILED: &str =
    "The daemon could not prepare the task's status file. Check the daemon log for details, then retry.";
/// Attempt failure reason when the bundled treehouse cannot be run.
pub(crate) const WORKTREE_MANAGER_MISSING: &str =
    "The worktree manager is missing or could not be started. Reinstall the app, then retry.";
/// Attempt failure reason when the project's base branch no longer resolves.
pub(crate) const BASE_BRANCH_MISSING: &str = "The project's base branch no longer exists. Set a base branch that exists in the project defaults, or clear it, then retry.";

/// User-facing worktree failure reasons must never copy paths or raw Git
/// output from the daemon log into task records.
fn worktree_failure_reason(error: &GitError) -> &'static str {
    match error {
        GitError::InvalidBaseRef { refspec, .. } if refspec == "HEAD" => {
            "The repository has no commit at HEAD. Create an initial commit in the registered repository, then retry."
        }
        // Only a project's base branch reaches here: the reason never names it.
        GitError::InvalidBaseRef { .. } => BASE_BRANCH_MISSING,
        GitError::WorktreePoolFull => {
            "Every task worktree is in use. Archive finished tasks to free one, then retry."
        }
        GitError::TreehouseUnavailable { .. } => WORKTREE_MANAGER_MISSING,
        GitError::TreehouseTimeout { .. } => {
            "The worktree manager did not finish in time. Retry the task."
        }
        GitError::CreationFailed { source, .. } => worktree_failure_reason(source),
        _ => {
            "The daemon could not prepare a task worktree. Check the daemon log for details, then retry."
        }
    }
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
            fail_before_agent_exit(
                &inner.store,
                job,
                Some(
                    "The daemon could not start the agent runtime. Check the daemon log for details.",
                ),
            );
            return;
        }
    };
    match runtime.block_on(run_agent(inner, job)) {
        // The cancellation owner is closed by now, so verification never races
        // the agent's own settlement.
        Ok(()) => inner.verify_candidate(&job.task_id, &job.attempt_id),
        Err(error) => {
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
}

/// A new session id of the same length class, for the submit-time checks.
pub(crate) const SESSION_PLACEHOLDER: &str = "00000000-0000-4000-8000-000000000000";

/// The status directory the submit-time checks measure, of the same length class as a run's.
pub(crate) fn placeholder_status_dir(config: &ExecutorConfig) -> PathBuf {
    worker_status::status_dir(&config.data_dir, "task-0000000000000000", 1)
}

/// The channel the submit-time checks measure: a new session and the placeholder directory,
/// as far as the installed CLI offers them.
pub(crate) fn submit_channel(status_dir: &Path, support: ChannelSupport) -> RunChannel<'_> {
    RunChannel {
        status_dir: support.status_dir.then_some(status_dir),
        session: support
            .session
            .then_some(TaskSession::New(SESSION_PLACEHOLDER)),
        in_place: false,
    }
}

/// Which status-channel flags the installed CLI offers (DPES-1108 phase 4). A CLI without
/// them still runs, as one without the workflow agent's flags does: without `--add-dir` its
/// worker cannot write the status file outside a full-access sandbox, and without the session
/// flags a wait fails closed instead of waiting. Codex names its own session, and `exec
/// resume` is a subcommand that `exec --help` cannot show, so Codex always counts as offering
/// it; a resume on a Codex without it fails to start and fails the task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChannelSupport {
    pub(crate) status_dir: bool,
    pub(crate) session: bool,
}

impl ChannelSupport {
    /// What the length checks measure: everything a run may add.
    pub(crate) const ALL: Self = Self {
        status_dir: true,
        session: true,
    };

    fn from_help(agent: &str, help: &CliHelp) -> Self {
        Self {
            status_dir: help.offers("--add-dir"),
            session: agent == CODEX_AGENT
                || (help.offers("--session-id") && help.offers("--resume")),
        }
    }
}

impl ExecutorConfig {
    /// The channel flags the CLI at `program` offers. Without help (the probe failed) every
    /// flag is kept, and the launch check refuses the CLI.
    pub(crate) async fn offered_channel(
        &self,
        agent: &str,
        program: &Path,
        executable: &Executable,
    ) -> ChannelSupport {
        match self.help_for(agent, program, executable).await {
            Some(help) => ChannelSupport::from_help(agent, &help),
            None => ChannelSupport::ALL,
        }
    }
}

/// What a task run passes beyond its prompt: its status directory and its session, when the
/// CLI offers them, and whether it runs in place (Codex then needs `--skip-git-repo-check`).
#[derive(Clone, Copy)]
pub(crate) struct RunChannel<'a> {
    pub(crate) status_dir: Option<&'a Path>,
    pub(crate) session: Option<TaskSession<'a>>,
    pub(crate) in_place: bool,
}

/// The program and argument vector the daemon launches for `agent`, or `None`
/// when the agent has no daemon launcher, or cannot resume `channel`'s session at
/// `permission`. Shared by the launch and by the submit-time command-line check so the two
/// cannot disagree; the selected model, effort, workflow agent, status directory and session
/// are part of the vector, so the check measures them too. The channel flags come after the
/// level's own, so a CLI that lacks a level flag is named for that flag first.
pub(crate) fn agent_command(
    config: &ExecutorConfig,
    agent: &str,
    objective: &str,
    selection: Option<&ModelSelection>,
    permission: PermissionLevel,
    workflow_agent: Option<&WorkflowAgent>,
    channel: RunChannel<'_>,
) -> Option<(PathBuf, Vec<String>)> {
    let model = selection.and_then(|s| s.model.as_deref());
    let effort = selection.and_then(|s| s.effort.as_deref());
    match agent {
        CODEX_AGENT => {
            let program = config
                .codex_program
                .clone()
                .unwrap_or_else(|| PathBuf::from("codex"));
            if let Some(TaskSession::Resume(id)) = channel.session {
                let args = CodexAdapter::args_for_resume(
                    permission,
                    id,
                    objective,
                    model,
                    effort,
                    workflow_agent,
                    channel.status_dir,
                    channel.in_place,
                )?;
                return Some((program, args));
            }
            let mut args =
                CodexAdapter::args_for_task(permission, objective, model, effort, workflow_agent)?;
            if let Some(dir) = channel.status_dir {
                let separator = args.iter().rposition(|arg| arg == "--")?;
                args.splice(separator..separator, task_session::status_dir_args(dir));
            }
            // `codex exec` refuses a directory that is not a Git repository unless told otherwise.
            if channel.in_place {
                args.insert(1, "--skip-git-repo-check".to_owned());
            }
            Some((program, args))
        }
        CLAUDE_AGENT => {
            let mut args = ClaudeCodeAdapter::args_for_task(
                permission,
                objective,
                model,
                effort,
                workflow_agent,
            );
            if let Some(dir) = channel.status_dir {
                args.extend(task_session::status_dir_args(dir));
            }
            if let Some(session) = channel.session {
                args.extend(task_session::claude_session_args(session));
            }
            Some((
                config
                    .claude_program
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("claude")),
                args,
            ))
        }
        // Submit-time length check only. The launch builds the real vector in
        // `spawn_copilot`, where the run directory and worktree are known; these
        // placeholders are the same length class.
        COPILOT_AGENT => {
            let mut args = CopilotAdapter::task_args(
                objective,
                Path::new("<copilot-usage-file>"),
                Path::new("<task-worktree>"),
                permission,
                config.copilot_credit_cap,
                workflow_agent,
            );
            extend_copilot_channel(&mut args, permission, channel);
            Some((
                config
                    .copilot_program
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("copilot")),
                args,
            ))
        }
        _ => None,
    }
}

/// Copilot's status directory, write rule and session flag, in that order.
fn extend_copilot_channel(
    args: &mut Vec<String>,
    permission: PermissionLevel,
    channel: RunChannel<'_>,
) {
    if let Some(dir) = channel.status_dir {
        args.extend(task_session::copilot_status_args(dir, permission));
    }
    if let Some(session) = channel.session {
        args.push(task_session::copilot_session_arg(session));
    }
}

impl Job {
    /// The run's channel, limited to what the CLI offers.
    fn channel(&self, support: ChannelSupport) -> RunChannel<'_> {
        RunChannel {
            status_dir: support.status_dir.then_some(self.status_dir.as_path()),
            session: support.session.then(|| self.session.as_task_session()),
            in_place: self.in_place,
        }
    }
}

/// Says the installed CLI lacks a channel flag, so the run launches without it. A resume
/// never launches without its session flag: the caller refuses it first.
fn warn_unsupported_channel(job: &Job, support: ChannelSupport) {
    let missing = match (support.status_dir, support.session) {
        (true, true) => return,
        (false, true) => "--add-dir",
        (true, false) => "--session-id or --resume",
        (false, false) => "--add-dir, --session-id or --resume",
    };
    let reason = format!(
        "installed CLI does not offer {missing}; the worker cannot report its status or wait"
    );
    job.diag.warn(
        LOG,
        "status_channel_unsupported",
        &job.event_fields(&[("reason", &reason)]),
    );
}

/// The channel one launch uses, and the refusal of a resume the CLI cannot run.
async fn launch_channel(
    inner: &Inner,
    job: &Job,
    program: &Path,
    executable: &Executable,
) -> Result<ChannelSupport, LaunchFailure> {
    let support = inner
        .config
        .offered_channel(&job.agent, program, executable)
        .await;
    if matches!(job.session, JobSession::Resume(_)) && !support.session {
        return Err(CompatibilityError::MissingFlag("--resume".to_owned()).into());
    }
    warn_unsupported_channel(job, support);
    Ok(support)
}

/// Launches a Copilot task. The run directory lives in the handle, so it is
/// removed when the run ends. A model selection is refused, because Copilot
/// offers none.
async fn spawn_copilot(
    inner: &Inner,
    job: &Job,
    workflow_agent: Option<WorkflowAgent>,
) -> Result<(Box<dyn AgentHandle>, ChannelSupport), LaunchFailure> {
    if job.model_selection.is_some() {
        return Err(
            "model selection refused: GitHub Copilot offers no model or effort selection".into(),
        );
    }
    let mut adapter = CopilotAdapter::new();
    if let Some(program) = &inner.config.copilot_program {
        adapter = adapter.with_program(program);
    }
    let program = adapter
        .locate()
        .ok_or("the copilot executable was not found")?;
    let executable = Executable::read(&program).map_err(|_| CompatibilityError::Unconfirmed)?;
    let (workflow_agent, missing) = inner
        .config
        .offered_workflow_agent(&job.agent, &program, &executable, workflow_agent)
        .await;
    if let Some(flag) = missing {
        warn_unsupported_workflow_agent(job, flag);
    }
    let support = launch_channel(inner, job, &program, &executable).await?;
    let run = CopilotRun::create(&inner.config.data_dir.join(COPILOT_RUNS_DIR))
        .map_err(|error| format!("cannot create the Copilot run directory: {error}"))?;
    let mut args = CopilotAdapter::task_args(
        &job.objective,
        &run.usage_file(),
        &job.worktree,
        job.permission,
        inner.config.copilot_credit_cap,
        workflow_agent.as_ref(),
    );
    extend_copilot_channel(&mut args, job.permission, job.channel(support));
    inner
        .config
        .check_launch(&job.agent, &program, &executable, &args)
        .await?;
    let request = AgentLaunchRequest {
        program,
        args,
        working_dir: job.worktree.clone(),
    };
    adapter
        .spawn_run(request, Some(run))
        .await
        .map(|handle| (handle, support))
        .map_err(|error| error.to_string().into())
}

#[derive(Debug)]
enum LaunchFailure {
    Compatibility(CompatibilityError),
    Other(String),
}

impl From<CompatibilityError> for LaunchFailure {
    fn from(error: CompatibilityError) -> Self {
        Self::Compatibility(error)
    }
}

impl From<String> for LaunchFailure {
    fn from(reason: String) -> Self {
        Self::Other(reason)
    }
}

impl From<&str> for LaunchFailure {
    fn from(reason: &str) -> Self {
        Self::Other(reason.to_owned())
    }
}

/// The workflow agent for one launch. A missing agent only warns on the attempt;
/// the attempt still launches, without it.
fn launch_workflow_agent(inner: &Inner, job: &Job) -> Option<WorkflowAgent> {
    let agent = (inner.config.locate_workflow_agent)();
    if agent.is_none() {
        job.diag.warn(
            LOG,
            "workflow_agent_missing",
            &job.event_fields(&[(
                "reason",
                "pstack workflow agent not found; launching without it",
            )]),
        );
    }
    agent
}

/// Says the installed CLI cannot take the workflow agent, so the attempt launches plain.
fn warn_unsupported_workflow_agent(job: &Job, flag: &str) {
    let reason =
        format!("installed CLI does not offer {flag}; launching without the pstack workflow agent");
    job.diag.warn(
        LOG,
        "workflow_agent_unsupported",
        &job.event_fields(&[("reason", &reason)]),
    );
}

async fn spawn_agent(
    inner: &Inner,
    job: &Job,
    workflow_agent: Option<WorkflowAgent>,
) -> Result<(Box<dyn AgentHandle>, ChannelSupport), LaunchFailure> {
    if job.agent == COPILOT_AGENT {
        return spawn_copilot(inner, job, workflow_agent).await;
    }
    let selection = job.model_selection.as_ref();
    let program = inner
        .config
        .locate_agent(&job.agent)
        .ok_or("the agent executable was not found")?;
    let executable = Executable::read(&program).map_err(|_| CompatibilityError::Unconfirmed)?;
    let (workflow_agent, missing) = inner
        .config
        .offered_workflow_agent(&job.agent, &program, &executable, workflow_agent)
        .await;
    if let Some(flag) = missing {
        warn_unsupported_workflow_agent(job, flag);
    }
    let support = launch_channel(inner, job, &program, &executable).await?;
    let Some((_, args)) = agent_command(
        &inner.config,
        &job.agent,
        &job.objective,
        selection,
        job.permission,
        workflow_agent.as_ref(),
        job.channel(support),
    ) else {
        return Err(format!(
            "agent {:?} has no daemon launcher for this run at {} permission",
            job.agent,
            job.permission.as_str()
        )
        .into());
    };
    if let Some(selection) = selection {
        // The catalog may have changed since submission (signed out, CLI
        // replaced), so the selection is checked again against a fresh read.
        inner
            .config
            .read_catalog(&job.agent, &job.worktree, LAUNCH_CATALOG_TIMEOUT)
            .await
            .validate(selection.model.as_deref(), selection.effort.as_deref())
            .map_err(|error| {
                format!("model selection refused: {}", log_safe(&error.to_string()))
            })?;
    }
    let detect = match job.agent.as_str() {
        CODEX_AGENT => {
            let adapter = CodexAdapter::new().with_program(program.clone());
            let caps = adapter.detect().await.map_err(|error| error.to_string())?;
            if !(caps.streaming && caps.structured_output && caps.cancellation) {
                return Err("codex lacks streaming, structured output or cancellation".into());
            }
            AdapterChoice::Codex(adapter)
        }
        _ => {
            let adapter = ClaudeCodeAdapter::new().with_program(program.clone());
            adapter.detect().await.map_err(|error| error.to_string())?;
            AdapterChoice::Claude(adapter)
        }
    };
    inner
        .config
        .check_launch(&job.agent, &program, &executable, &args)
        .await?;
    let request = AgentLaunchRequest {
        program,
        args,
        working_dir: job.worktree.clone(),
    };
    let result = match detect {
        AdapterChoice::Codex(adapter) => adapter.spawn(request).await,
        AdapterChoice::Claude(adapter) => adapter.spawn(request).await,
    };
    result
        .map(|handle| (handle, support))
        .map_err(|error| error.to_string().into())
}

enum AdapterChoice {
    Codex(CodexAdapter),
    Claude(ClaudeCodeAdapter),
}

async fn run_agent(inner: &Inner, job: &Job) -> Result<(), String> {
    let store = &*inner.store;
    let (mut handle, support) = match spawn_agent(inner, job, launch_workflow_agent(inner, job))
        .await
    {
        Ok(spawned) => spawned,
        Err(error) => {
            let (reason, detail) = match error {
                LaunchFailure::Compatibility(error) => {
                    let detail = error.to_string();
                    (format!("{} task at {} permission did not start. {detail}", job.agent, job.permission.as_str()), detail)
                }
                LaunchFailure::Other(detail) => ("The daemon could not start the selected agent. Check that its CLI is installed and signed in, then retry; see the daemon log for details.".to_owned(), detail),
            };
            fail_before_agent_exit(store, job, Some(&reason));
            let (worktree, uncommitted) = preserved_work(job);
            job.diag.error(
                LOG,
                "agent_launch_failed",
                &job.event_fields(&[
                    ("agent", &job.agent),
                    ("error", &detail),
                    ("worktree", &worktree),
                    ("uncommitted_paths", &uncommitted),
                ]),
            );
            return Ok(());
        }
    };
    // The level and the argument mapping the run launched with. Recorded here,
    // after the spawn, so a launch that never started records nothing.
    inner.output.begin(&job.session_id);
    let _output_ended = EndOnDrop {
        store: &inner.output,
        session_id: &job.session_id,
    };
    let profile =
        orchestrator_agents::permission::task_profile(&job.agent, job.permission).unwrap_or("none");
    job.diag.info(
        LOG,
        "agent_launched",
        &job.event_fields(&[
            ("agent", &job.agent),
            ("permission_level", job.permission.as_str()),
            ("permission_profile", profile),
        ]),
    );
    #[cfg(test)]
    if inner.config.wait_for_exit_before_identity {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while !handle.try_exited().map_err(|error| error.to_string())? {
            if std::time::Instant::now() >= deadline {
                return Err("agent did not exit".into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    let identity = match capture_process_identity(handle.pid()) {
        Ok(identity) => store
            .record_session_identity(&job.task_id, &job.session_id, &identity)
            .map_err(|error| format!("cannot persist process identity: {error}")),
        Err(error) => match handle.try_exited() {
            Ok(true) => {
                job.diag
                    .warn(LOG, "identity_skipped_agent_exited", &job.event_fields(&[]));
                Ok(())
            }
            _ => Err(format!("process identity unavailable: {error}")),
        },
    };
    if let Err(message) = identity {
        reap(&mut *handle, job).await;
        fail_before_agent_exit(
            store,
            job,
            Some(
                "The daemon could not verify the agent process and stopped the run. Check the daemon log for details.",
            ),
        );
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
        fail_after_agent_exit(
            store,
            job,
            &format!("Supervision error: cannot register cancellation owner: {error}"),
            None,
        );
        return Err(format!("cannot register cancellation owner: {error}"));
    }

    // A new session reads the attempt's status file after the lines earlier sessions left.
    let after = store
        .last_status_line(&job.task_id, &job.attempt_id)
        .unwrap_or(0);
    // Claude and Copilot took the id the daemon chose, when the CLI offers the flag; Codex
    // reports its own when it starts. A resume continues the id it named.
    let known_session = match &job.session {
        JobSession::New(_) if job.agent == CODEX_AGENT || !support.session => None,
        JobSession::New(id) => {
            match store.record_agent_session_id(&job.task_id, &job.session_id, id) {
                Ok(()) => Some(id.clone()),
                Err(error) => {
                    job.diag.error(
                        LOG,
                        "agent_session_id_not_recorded",
                        &job.event_fields(&[("error", &error.to_string())]),
                    );
                    None
                }
            }
        }
        JobSession::Resume(id) => Some(id.clone()),
    };
    let mut tail = StatusTail::new(
        job.status_dir
            .join(orchestrator_core::worker_status::STATUS_FILE_NAME),
        after,
        known_session,
    );
    let board = Some(inner.board.as_ref());
    let monitored = monitor_agent_with(handle, store, &owner, job, board, Some(&mut tail)).await;
    inner.board.forget(&job.session_id);
    tail.drain(store, job);
    if let (COPILOT_AGENT, Ok(outcome)) = (job.agent.as_str(), &monitored) {
        inner.config.copilot_sign_in.record_run(&outcome.exit);
    }
    let current = store
        .get(&job.task_id)
        .map_err(|error| format!("cannot reload task after agent exit: {error}"))?;
    if current.state == TaskState::Cancelled {
        return Err("task was canceled before this owner confirmed it".into());
    }
    let monitored = monitored.map_err(|reason| {
        format!("supervision failed, task stays Running for restart reconciliation: {reason}")
    })?;
    finish(store, job, &owner, monitored, &tail)
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

/// The model a Codex run was asked for. Codex's stream names no model, so this
/// is the only model a Codex receipt can carry; it is the requested model, not
/// one the provider reported, and is `None` when the run used Codex's default.
/// Claude is left out because its own result names the models that ran.
fn requested_codex_model(job: &Job) -> Option<String> {
    if job.agent != CODEX_AGENT {
        return None;
    }
    job.model_selection.as_ref()?.model.clone()
}

/// Persists usage the agent reported so far. A usage failure is logged and
/// never fails or delays the run; the receipt key makes a repeat harmless.
/// `requested_model` fills in a model the report does not name.
fn record_usage(
    handle: &mut dyn AgentHandle,
    store: &SqliteStore,
    owner: &CancellationOwner,
    started_ms: u64,
    requested_model: Option<&str>,
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
            model: report.model.or_else(|| requested_model.map(str::to_owned)),
        };
        if let Err(error) = store.record_usage_receipt(&receipt) {
            eprintln!(
                "orchestrator-daemon: cannot record usage for session {}: {error}",
                owner.session_id
            );
        }
    }
}

pub(crate) fn unix_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Drives the agent to exit. A cancellation request is consumed at most once,
/// and `cancellation_succeeded` is set only after the handle's cancel returned
/// Ok. Neither depends on any client connection.
#[cfg(test)]
async fn monitor_agent(
    handle: Box<dyn AgentHandle>,
    store: &SqliteStore,
    owner: &CancellationOwner,
    job: &Job,
) -> Result<MonitorOutcome, String> {
    monitor_agent_with(handle, store, owner, job, None, None).await
}

/// `board`, when given, is touched at the start and updated on every agent event. `tail`, when given,
/// reads the worker's status file about once a second and learns the agent's session id.
async fn monitor_agent_with(
    mut handle: Box<dyn AgentHandle>,
    store: &SqliteStore,
    owner: &CancellationOwner,
    job: &Job,
    board: Option<&ActivityBoard>,
    mut tail: Option<&mut StatusTail>,
) -> Result<MonitorOutcome, String> {
    let requested_model = requested_codex_model(job);
    let requested_model = requested_model.as_deref();
    let mut requested = false;
    let mut attempted = false;
    let mut succeeded = false;
    let mut cancel_error = None;
    let mut supervision_error = None;
    let mut request_unreadable_logged = false;
    let started_ms = unix_now_ms();
    if let Some(board) = board {
        board.begin(&job.session_id);
    }
    let mut thinking = Coalescer::default();

    loop {
        if !attempted {
            match store.cancellation_request(owner) {
                Ok(Some(CancellationStatus::Pending)) => {
                    requested = true;
                    attempted = true;
                    job.diag
                        .info(LOG, "cancellation_requested", &job.event_fields(&[]));
                    flush_thinking(store, job, &mut thinking);
                    match handle.cancel().await {
                        Ok(()) => {
                            let exit = handle.wait().await;
                            // Keep what the agent reported before it was stopped.
                            record_usage(&mut *handle, store, owner, started_ms, requested_model);
                            let exit = exit.map_err(|error| {
                                format!("canceled but termination was not confirmed: {error}")
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
        if let Some(tail) = tail.as_deref_mut() {
            if let Some(Ok(Some(AgentEvent::SessionStarted { session_id }))) = &event {
                note_agent_session(store, job, tail, &session_id.to_string());
            }
            tail.poll(store, job);
        }
        if thinking.due() {
            flush_thinking(store, job, &mut thinking);
        }
        if !matches!(
            event,
            None | Some(Ok(Some(AgentEvent::ThinkingDelta { .. })))
        ) {
            flush_thinking(store, job, &mut thinking);
        }
        match event {
            None => continue,
            Some(Ok(Some(AgentEvent::SessionExited { .. }))) => {
                record_usage(&mut *handle, store, owner, started_ms, requested_model);
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
            Some(Ok(Some(event))) => {
                record_output(job, &event);
                record_activity(store, board, job, &event, &mut thinking);
                record_usage(&mut *handle, store, owner, started_ms, requested_model)
            }
            Some(Ok(None)) => {
                record_usage(&mut *handle, store, owner, started_ms, requested_model);
                supervision_error =
                    Some("agent event stream ended without a process exit event".to_string());
                break;
            }
            Some(Err(error)) => {
                record_usage(&mut *handle, store, owner, started_ms, requested_model);
                supervision_error = Some(format!("agent event stream failed: {error}"));
                break;
            }
        }
    }

    flush_thinking(store, job, &mut thinking);
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
    record_usage(&mut *handle, store, owner, started_ms, requested_model);
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

/// Writes the buffered thinking as one row. Best effort: lost thinking never fails the run.
fn flush_thinking(store: &SqliteStore, job: &Job, thinking: &mut Coalescer) {
    if let Some(text) = thinking.take() {
        if let Err(error) = store.append_session_thinking(&job.session_id, &text) {
            eprintln!(
                "orchestrator-daemon: cannot record thinking for session {}: {error}",
                job.session_id
            );
        }
    }
}

/// Updates the board for one agent event and stores what it adds to the session's activity:
/// tool rows at once, thinking through `thinking`. Everything is best effort.
fn record_activity(
    store: &SqliteStore,
    board: Option<&ActivityBoard>,
    job: &Job,
    event: &AgentEvent,
    thinking: &mut Coalescer,
) {
    let session = job.session_id.as_str();
    let on_board = |update: &dyn Fn(&ActivityBoard)| {
        if let Some(board) = board {
            update(board);
        }
    };
    match event {
        AgentEvent::ThinkingDelta { text } => {
            on_board(&|board| board.set_phase(session, Phase::Thinking));
            if let Some(joined) = thinking.push(text) {
                if let Err(error) = store.append_session_thinking(session, &joined) {
                    eprintln!(
                        "orchestrator-daemon: cannot record thinking for session {session}: {error}"
                    );
                }
            }
        }
        AgentEvent::MessageDelta { .. } => {
            on_board(&|board| board.set_phase(session, Phase::Replying))
        }
        AgentEvent::ToolStarted {
            tool_id,
            name,
            summary,
        } => match ToolStart::bounded(tool_id, name, summary) {
            Some(tool) => {
                if let Err(error) =
                    store.append_session_tool_started(session, &tool.id, &tool.name, &tool.summary)
                {
                    eprintln!(
                        "orchestrator-daemon: cannot record tool for session {session}: {error}"
                    );
                }
                on_board(&|board| board.tool_started(session, tool.clone()));
            }
            None => on_board(&|board| board.touch(session)),
        },
        AgentEvent::ToolFinished {
            tool_id,
            success,
            summary,
        } => match ToolEnd::bounded(tool_id, *success, summary) {
            Some(end) => {
                if let Err(error) =
                    store.append_session_tool_finished(session, &end.id, end.ok, &end.summary)
                {
                    eprintln!(
                        "orchestrator-daemon: cannot record tool result for session {session}: {error}"
                    );
                }
                on_board(&|board| board.tool_finished(session, &end.id));
            }
            None => on_board(&|board| board.touch(session)),
        },
        _ => on_board(&|board| board.touch(session)),
    }
}

/// Records the session id the agent reported. Codex names its own session this way. Claude was
/// given its id, so a different one is logged and the given id kept.
fn note_agent_session(store: &SqliteStore, job: &Job, tail: &mut StatusTail, reported: &str) {
    match &tail.agent_session_id {
        Some(known) if known == reported => {}
        Some(_) => job
            .diag
            .warn(LOG, "agent_session_id_mismatch", &job.event_fields(&[])),
        None => {
            if store
                .record_agent_session_id(&job.task_id, &job.session_id, reported)
                .is_ok()
            {
                tail.agent_session_id = Some(reported.to_owned());
            } else {
                job.diag
                    .error(LOG, "agent_session_id_not_recorded", &job.event_fields(&[]));
            }
        }
    }
}

/// Copies the text of an event the executor already read into the session's
/// bounded ring. Infallible and non-blocking for the run: it neither reads the
/// agent's pipes nor changes the event.
fn record_output(job: &Job, event: &AgentEvent) {
    match event {
        AgentEvent::MessageDelta { text } => job.output.push(&job.session_id, Stream::Stdout, text),
        AgentEvent::ProtocolError { message } => {
            job.output.push(&job.session_id, Stream::Stderr, message);
            job.output.push(&job.session_id, Stream::Stderr, "\n");
        }
        _ => {}
    }
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

/// Maps a reaped agent to exactly one record set. A completed turn with exit
/// code zero leaves a candidate for verification; every other outcome fails the
/// attempt.
fn finish(
    store: &SqliteStore,
    job: &Job,
    owner: &CancellationOwner,
    monitored: MonitorOutcome,
    tail: &StatusTail,
) -> Result<(), String> {
    let (outcome, code, reason) = describe_exit(&monitored.exit);
    let final_message = last_message_of(&monitored.exit);
    job.diag.info(
        LOG,
        "agent_exited",
        &job.event_fields(&[
            ("outcome", outcome),
            ("exit_code", &code),
            ("reason", &reason),
            (
                "supervision_error",
                monitored.supervision_error.as_deref().unwrap_or(""),
            ),
            (
                "cancellation_error",
                monitored.cancellation_error.as_deref().unwrap_or(""),
            ),
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
                    &job.event_fields(&[
                        ("worktree", &worktree),
                        ("uncommitted_paths", &uncommitted),
                    ]),
                );
            }
            return confirmed;
        }
        fail_after_agent_exit(
            store,
            job,
            &exit_reason(outcome, &code, &reason),
            final_message,
        );
        return close(if monitored.cancellation_requested {
            CancellationStatus::Failed
        } else {
            CancellationStatus::Rejected
        });
    }
    if let Some(supervision) = &monitored.supervision_error {
        fail_after_agent_exit(
            store,
            job,
            &format!("Supervision error: {supervision}"),
            final_message,
        );
        return close(resolution);
    }
    match monitored.exit {
        // A clean exit is settled by what the worker reported first: a wait parks the run and
        // a `failed` line fails it. A pending cancel wins over both. A `done` line is only a
        // claim, so the candidate path below decides as before (DPES-1108 phase 4).
        AgentExit::Completed { code: Some(0), .. }
            if !monitored.cancellation_requested
                && settled_by_report(store, job, tail, final_message) =>
        {
            close(resolution)
        }
        AgentExit::Completed {
            code: Some(0),
            ref last_message,
        } => {
            store
                .update_session_state(&job.task_id, &job.session_id, SessionState::Exited)
                .map_err(|error| format!("cannot mark session exited: {error}"))?;
            // An in-place task changed its project directory itself: nothing to commit.
            let committed = if job.in_place {
                Ok(None)
            } else {
                commit_candidate(&job.worktree, &job.task_id)
            };
            let changed = match committed {
                Ok(changed) => changed,
                Err(error) => {
                    record_summary(store, job, final_message, None);
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
                            (
                                "recovery",
                                "worktree kept with the agent's changes; retry the task",
                            ),
                        ]),
                    );
                    close(resolution)?;
                    return Err(format!("cannot record candidate commit: {error}"));
                }
            };
            // Only an investigation may end with nothing changed: its finding is the product. An
            // in-place task has no commit to check, so it is exempt too.
            let investigates = store
                .get(&job.task_id)
                .is_ok_and(|task| task.contract.kind == TaskKind::Investigate);
            if changed.is_none() && !job.in_place && !investigates {
                record_summary(store, job, final_message, Some(0));
                let reason = no_change_reason(final_message);
                fail_records(store, job, SessionState::Exited, Some(reason.as_str()));
                return close(resolution);
            }
            let changed_files = changed.map_or(0, |count| u32::try_from(count).unwrap_or(u32::MAX));
            record_summary(store, job, final_message, Some(changed_files));
            store
                .update_attempt_state(&job.task_id, &job.attempt_id, AttemptState::CandidateReady)
                .map_err(|error| format!("cannot mark attempt candidate-ready: {error}"))?;
            store
                .transition(&job.task_id, TaskEvent::CandidateReady)
                .map_err(|error| format!("cannot record candidate: {error}"))?;
            job.diag
                .info(LOG, "candidate_ready", &job.event_fields(&[]));
            capture_report(store, job, last_message.as_deref());
            close(resolution)
        }
        _ => {
            fail_after_agent_exit(
                store,
                job,
                &exit_reason(outcome, &code, &reason),
                final_message,
            );
            close(resolution)
        }
    }
}

/// Settles a clean exit from the worker's status lines when they decide it, and says whether
/// they did. A wait parks the run (or fails it when it could not resume at the same level); a
/// `failed` line fails it. Anything else is left to the candidate path.
fn settled_by_report(
    store: &SqliteStore,
    job: &Job,
    tail: &StatusTail,
    final_message: Option<&str>,
) -> bool {
    let lines: Vec<_> = tail.lines.iter().map(|(_, line)| line.clone()).collect();
    match status_rules::settle(&lines) {
        Settlement::Ordinary => false,
        Settlement::ReportedFailure(note) => {
            let reason = match note.map(|note| one_line(&bounded_text(&note, EXCERPT_BYTES))) {
                Some(note) if !note.is_empty() => {
                    format!(
                        "The agent reported that it failed: {}.",
                        note.trim_end_matches('.')
                    )
                }
                _ => "The agent reported that it failed.".to_owned(),
            };
            fail_after_agent_exit(store, job, &reason, final_message);
            true
        }
        Settlement::Wait(reason) => {
            park(store, job, tail, reason, final_message);
            true
        }
    }
}

/// Parks a run whose worker reported a wait: the session ends, the attempt and the task wait,
/// and up to [`MAX_QUESTIONS_PER_WAIT`] of its questions are raised for the user, all in one
/// store transaction. The worktree stays leased. A run that could not be resumed at the same
/// permission level, or whose session id is unknown, fails instead: it is never left waiting
/// for a resume that cannot happen.
fn park(
    store: &SqliteStore,
    job: &Job,
    tail: &StatusTail,
    reason: WaitReason,
    final_message: Option<&str>,
) {
    let waits_for = match reason {
        WaitReason::Decision => "a decision",
        WaitReason::Blocker => "help",
    };
    let refusal = resume_refusal(&job.agent, job.permission).or_else(|| {
        tail.agent_session_id
            .is_none()
            .then_some("its CLI session id is not known")
    });
    if let Some(why) = refusal {
        let reason = format!(
            "The agent stopped to wait for {waits_for}, but MCAO cannot resume it: {why}. It \
             was not resumed any other way. Retry the task with the missing answer added."
        );
        fail_after_agent_exit(store, job, &reason, final_message);
        return;
    }
    record_summary(store, job, final_message, None);
    let questions: Vec<WorkerQuestion> = tail
        .lines
        .iter()
        .filter(|(_, line)| line.verb == StatusVerb::NeedsDecision)
        .filter_map(|(number, line)| {
            Some(WorkerQuestion {
                id: status_rules::question_id(&job.attempt_id, *number),
                question: line.note.clone()?,
                key: line.key.clone(),
            })
        })
        .take(MAX_QUESTIONS_PER_WAIT)
        .collect();
    match store.enter_waiting(
        &job.task_id,
        &job.attempt_id,
        &job.session_id,
        reason,
        &questions,
    ) {
        Ok(()) => job.diag.info(
            LOG,
            "worker_waiting",
            &job.event_fields(&[
                ("waits_for", waits_for),
                ("questions", &questions.len().to_string()),
            ]),
        ),
        Err(error) => {
            job.diag.error(
                LOG,
                "wait_not_recorded",
                &job.event_fields(&[("error", &error.to_string())]),
            );
            fail_after_agent_exit(
                store,
                job,
                "The agent stopped to wait, but the daemon could not record the wait. Check the daemon log for details, then retry.",
                final_message,
            );
        }
    }
}

/// Keeps a finished scout's last message as its report (DPES-1105 phase 4a). Only an
/// `Investigate` task, or a task that ran in place (nothing to review, so its last message is
/// the record of what it did), leaves one, and only when the agent wrote a message. The text is
/// untrusted agent output: it is redacted and capped before it is stored, and nothing here
/// puts it in a log line. A failure to store it is logged without the text and never
/// changes no outcome: verification alone decides whether the task succeeds.
fn capture_report(store: &SqliteStore, job: &Job, last_message: Option<&str>) {
    record_report(
        store,
        &job.diag,
        &job.task_id,
        &job.attempt_id,
        last_message,
    );
}

/// The body of [`capture_report`], without the job, so a test can run it.
pub(crate) fn record_report(
    store: &SqliteStore,
    diag: &Diagnostics,
    task_id: &str,
    attempt_id: &str,
    last_message: Option<&str>,
) {
    let Some(text) = last_message.map(str::trim).filter(|text| !text.is_empty()) else {
        return;
    };
    let leaves_report = store.get(task_id).is_ok_and(|task| {
        task.contract.kind == TaskKind::Investigate
            || store
                .get_project(&task.contract.project_id)
                .is_ok_and(|project| project.is_in_place())
    });
    if !leaves_report {
        return;
    }
    let (body, truncated) = crate::api::report::report_body(text);
    if body.trim().is_empty() {
        return;
    }
    let captured_at = crate::api::report::utc_text(unix_now_ms());
    if store
        .capture_task_report(task_id, attempt_id, &body, truncated, &captured_at)
        .is_err()
    {
        diag.error(
            LOG,
            "report_record_failed",
            &[("task_id", task_id), ("attempt_id", attempt_id)],
        );
    }
}

fn fail_before_agent_exit(store: &SqliteStore, job: &Job, failure_reason: Option<&str>) {
    fail_records(store, job, SessionState::Aborted, failure_reason);
}

/// Records a run that ended without a candidate: why its attempt failed, and the agent's last
/// message as the attempt's summary when it wrote one.
fn fail_after_agent_exit(store: &SqliteStore, job: &Job, reason: &str, last_message: Option<&str>) {
    record_summary(store, job, last_message, None);
    let reason = one_line(&bounded_text(reason, FAILURE_REASON_BYTES));
    fail_records(store, job, SessionState::Exited, Some(reason.as_str()));
}

/// Why a run that produced no candidate ended, in words a person can read. Built from the exit
/// facts; the agent's own text reaches it only through the reason of a protocol failure.
fn exit_reason(outcome: &str, code: &str, reason: &str) -> String {
    match outcome {
        "cancelled" => "The agent was cancelled.".to_owned(),
        // The protocol's reason may already end with a full stop; one is enough.
        "failed" if !reason.is_empty() => {
            format!("The agent failed: {}.", reason.trim_end_matches('.'))
        }
        _ if code == "none" => "The agent exited without an exit code.".to_owned(),
        _ => format!("The agent exited with code {code}."),
    }
}

/// The final message of a run that ended with one. A run that failed has none.
fn last_message_of(exit: &AgentExit) -> Option<&str> {
    match exit {
        AgentExit::Completed { last_message, .. } | AgentExit::Cancelled { last_message, .. } => {
            last_message.as_deref()
        }
        AgentExit::Failed { .. } => None,
    }
}

/// The reason an implement task that changed no files fails with. The agent's last message, when
/// it wrote one, says what it concluded in place of a change.
fn no_change_reason(last_message: Option<&str>) -> String {
    const HEAD: &str = "The agent exited without changing any files.";
    let excerpt = last_message.map(|text| one_line(&bounded_text(text, EXCERPT_BYTES)));
    match excerpt.filter(|excerpt| !excerpt.is_empty()) {
        Some(excerpt) => format!("{HEAD} Last message: {excerpt}"),
        None => HEAD.to_owned(),
    }
}

/// Most bytes of an agent's last message kept as its attempt's summary.
const SUMMARY_BYTES: usize = 4 * 1024;
/// Most bytes of a stored failure reason.
const FAILURE_REASON_BYTES: usize = 512;
/// Most bytes of a last message quoted inside a failure reason.
const EXCERPT_BYTES: usize = 200;

/// Stores the agent's last message as the attempt's summary, with the changed-file count when it
/// is known. It is written before the attempt changes state, so the wake that reports the outcome
/// can read it. A failure to store is logged without the text and never changes the outcome.
fn record_summary(
    store: &SqliteStore,
    job: &Job,
    last_message: Option<&str>,
    changed_files: Option<u32>,
) {
    let summary = last_message
        .map(|text| bounded_text(text, SUMMARY_BYTES))
        .filter(|summary| !summary.trim().is_empty());
    if store
        .record_attempt_summary(
            &job.task_id,
            &job.attempt_id,
            summary.as_deref(),
            changed_files,
        )
        .is_err()
    {
        job.diag
            .error(LOG, "attempt_summary_not_recorded", &job.event_fields(&[]));
    }
}

/// `text` redacted the way wake text is, then cut to `max_bytes` on a character boundary. The
/// cut comes last: redaction can lengthen the text, and a cut first could leave half of a
/// credential unmasked.
fn bounded_text(text: &str, max_bytes: usize) -> String {
    let (body, _) = crate::api::report::report_body(text);
    let mut redacted = crate::api::wake_events::redact(&body);
    let mut end = max_bytes.min(redacted.len());
    while !redacted.is_char_boundary(end) {
        end -= 1;
    }
    redacted.truncate(end);
    redacted
}

/// `text` on one line, for a reason shown in a notice.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Ends the session, attempt and task of a launch that produced no candidate.
/// Each write is attempted independently so one refusal cannot hide the rest.
/// The worktree is left in place and named in the event, so nothing the agent
/// wrote is discarded.
fn fail_records(
    store: &SqliteStore,
    job: &Job,
    session: SessionState,
    failure_reason: Option<&str>,
) {
    let failed_attempt = match failure_reason {
        Some(reason) => store.record_attempt_failure(&job.task_id, &job.attempt_id, reason),
        None => store.update_attempt_state(&job.task_id, &job.attempt_id, AttemptState::Failed),
    };
    let results = [
        store
            .update_session_state(&job.task_id, &job.session_id, session)
            .map_err(|error| format!("session: {error}")),
        failed_attempt.map_err(|error| format!("attempt: {error}")),
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
            (
                "records_complete",
                if write_failed { "false" } else { "true" },
            ),
            ("worktree", &worktree),
            ("uncommitted_paths", &uncommitted),
            (
                "recovery",
                "worktree kept; retry the task or inspect the worktree",
            ),
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
/// Returns how many files the commit holds, or `None` when the agent changed nothing.
fn commit_candidate(worktree: &Path, task_id: &str) -> Result<Option<usize>, String> {
    let status = git_checked(
        worktree,
        &["status", "--porcelain", "--untracked-files=all"],
    )?;
    if status.stdout.is_empty() {
        return Ok(None);
    }
    git_checked(worktree, &["add", "--all", "--"])?;
    if git(worktree, &["diff", "--cached", "--quiet"])?
        .status
        .success()
    {
        return Ok(None);
    }
    // NUL-terminated names, so a file name that contains a newline still counts as one file.
    let staged = git_checked(worktree, &["diff", "--cached", "--name-only", "-z"])?;
    let files = staged
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .count();
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
    )?;
    Ok(Some(files))
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

/// Portable (also built on Windows) tests for the running-task listing.
#[cfg(test)]
mod running_listing_tests {
    use super::*;

    #[test]
    fn a_failed_listing_is_logged_and_never_reads_as_nothing_running() {
        let diag = Diagnostics::memory();
        let result = running_ids_from(Err(StoreError::WorkerStopped), &diag, "shutdown");
        assert_eq!(result, None);
        let lines = diag.lines().join("\n");
        assert!(lines.contains("\"level\":\"error\""), "{lines}");
        assert!(
            lines.contains("\"event\":\"running_tasks_unreadable\""),
            "{lines}"
        );
        assert!(lines.contains("\"context\":\"shutdown\""), "{lines}");
    }

    #[test]
    fn a_readable_empty_listing_is_quiet_and_empty() {
        let diag = Diagnostics::memory();
        assert_eq!(
            running_ids_from(Ok(Vec::new()), &diag, "x"),
            Some(Vec::new())
        );
        assert!(diag.lines().is_empty());
    }

    #[test]
    fn interrupted_tasks_are_unknown_when_either_listing_failed() {
        let one = vec!["a".to_owned(), "b".to_owned()];
        let two = vec!["b".to_owned()];
        assert_eq!(
            interrupted_between(Some(&one), Some(&two)),
            Some(vec!["a".to_owned()])
        );
        assert_eq!(interrupted_between(None, Some(&two)), None);
        assert_eq!(interrupted_between(Some(&one), None), None);
        assert_eq!(join_or_unknown(None), "unknown");
        assert_eq!(join_or_unknown(Some(&one)), "a,b");
        assert_eq!(join_or_unknown(Some(&[])), "");
    }
}

#[path = "output_ring.rs"]
pub(crate) mod output_ring;

#[path = "worker_status.rs"]
pub(crate) mod worker_status;

#[cfg(all(test, unix))]
#[path = "executor_tests.rs"]
mod tests;
