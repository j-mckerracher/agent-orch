//! CLI lifecycle state lives in `store.sqlite`; worktree metadata and logs
//! remain sidecars. Run holds the data lock through setup, then releases it for
//! the rest of the run so a separate cancel command can enqueue a request for
//! the run process that owns the live agent handle, and so a daemon that holds
//! the lock for its lifetime cannot starve the owner's finish. Finish writes
//! need no exclusive lock because owner confirmation and closure are single
//! generation-fenced store transactions only this owner may commit.
//! Existing JSON task records are not migrated. Stale lock recovery precedes
//! reconciliation. Before scheduling, a session whose process is proven gone
//! has its recorded workspace inspected first: Foreign or Unknown evidence, or
//! Partial/Ambiguous (or Confirmed-on-unsucceeded) delivery evidence, refuses
//! scheduling and leaves the records unfinished for re-inspection; Dirty,
//! Missing or BranchMoved keeps the worktree and takes the failure transition.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command as ProcessCommand, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use orchestrator_agents::{
    AgentAdapter, AgentEvent, AgentExit, AgentHandle, AgentLaunchRequest, CodexAdapter,
};
use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::identity::{
    plan_process_reconciliation, ProcessClassification, ProcessIdentity, ProcessObservation,
    ProcessPlatform,
};
use orchestrator_core::records::{Project, Workspace};
use orchestrator_core::task::{Attempt, AttemptState, Session, SessionState, TaskEvent, TaskState};
#[cfg(test)]
use orchestrator_engine::reconcile::delivery_blocks;
use orchestrator_engine::reconcile::{reconcile_before_scheduling, reconcile_cancellation_owners};
use orchestrator_git::{create_task_worktree, Repository};
#[cfg(test)]
use orchestrator_runtime::reconcile::DeliveryEvidence;
use orchestrator_runtime::reconcile::{
    capture_exact_process_identity, capture_process_identity, observe_owner_process,
    observe_process,
};
use orchestrator_store::{
    CancellationOwner, CancellationStatus, DataDirectoryLock, DataDirectoryLockError, SqliteStore,
    Store,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[cfg(windows)]
const NULL_DEVICE: &str = "NUL";
#[cfg(not(windows))]
const NULL_DEVICE: &str = "/dev/null";
const STORE_FILE_NAME: &str = "store.sqlite";
const LOCK_WAIT_TIMEOUT: Duration = Duration::from_secs(2);
const LOCK_RETRY_INTERVAL: Duration = Duration::from_millis(50);
#[cfg(all(unix, feature = "fault-injection"))]
#[link(name = "c")]
unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

#[cfg(feature = "fault-injection")]
/// Explicit test hooks. They require this feature and a matching environment
/// value; the feature-off implementations below perform no work.
/// Updated: 2026-11-08T10:33:52+00:00.
mod fault_injection {
    #[cfg(unix)]
    use super::kill;
    use super::{Path, ProcessCommand};
    use std::ffi::OsStr;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    const MODE: &str = "AGENT_ORCH_FAULT_INJECTION";
    const GATE: &str = "AGENT_ORCH_FAULT_GATE";

    fn enabled(value: &str) -> bool {
        std::env::var_os(MODE).as_deref() == Some(OsStr::new(value))
    }

    pub fn before_admit(data_dir: &Path, task_id: &str) -> Result<(), String> {
        if !enabled("fail-event-insert") {
            return Ok(());
        }
        let database = data_dir.join("store.sqlite");
        let sql = format!(
            "CREATE TRIGGER agent_orch_m2_05_reject_event BEFORE INSERT ON events \
             WHEN NEW.task_id = '{task_id}' BEGIN \
             SELECT RAISE(ABORT, 'M2-05 injected event write failure'); END;"
        );
        let output = ProcessCommand::new("sqlite3")
            .arg("-batch")
            .arg(database)
            .arg(sql)
            .output()
            .map_err(|error| format!("cannot start sqlite3 fault injector: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "cannot install SQLite fault trigger: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(())
    }

    pub fn before_identity_capture() -> std::io::Result<()> {
        if enabled("fail-identity-capture") {
            return Err(std::io::Error::other("injected identity capture failure"));
        }
        Ok(())
    }

    pub fn before_agent_launch() {
        if enabled("crash-before-agent-launch") {
            crash_self();
        }
    }

    /// Waits for the gate file, then ends the run owner. On Unix the owned
    /// agent group is SIGKILLed first. On Windows the run owner's kill-on-close
    /// Job Object ends the agent tree when this process dies, so no extra kill
    /// is issued: the crash itself is the portable fault.
    pub fn after_lock_release(agent_pid: u32) -> Result<(), String> {
        if !enabled("kill-agent-group-and-cli") {
            return Ok(());
        }
        let gate = std::env::var_os(GATE)
            .map(PathBuf::from)
            .ok_or_else(|| format!("{GATE} is required for this fault"))?;
        let deadline = Instant::now() + Duration::from_secs(60);
        while !gate.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if !gate.exists() {
            return Err(format!("fault gate {} did not appear", gate.display()));
        }
        #[cfg(unix)]
        {
            let group = i32::try_from(agent_pid)
                .map_err(|_| format!("agent PID {agent_pid} exceeds process-group range"))?;
            // SAFETY: the supervisor launches this agent as leader of its owned group.
            if unsafe { kill(-group, 9) } != 0 {
                return Err(format!(
                    "cannot SIGKILL owned agent group {group}: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
        #[cfg(not(unix))]
        let _ = agent_pid;
        crash_self();
    }

    fn crash_self() -> ! {
        #[cfg(unix)]
        {
            let process = i32::try_from(std::process::id()).expect("Unix PID fits i32");
            // SAFETY: this signal targets only the CLI process running this hook.
            unsafe {
                kill(process, 9);
            }
        }
        // Abrupt, non-unwinding exit that avoids abort()'s Windows error report.
        std::process::exit(137);
    }
}

#[cfg(not(feature = "fault-injection"))]
mod fault_injection {
    use super::Path;

    pub fn before_admit(_data_dir: &Path, _task_id: &str) -> Result<(), String> {
        Ok(())
    }

    pub fn before_identity_capture() -> std::io::Result<()> {
        Ok(())
    }

    pub fn before_agent_launch() {}

    pub fn after_lock_release(_agent_pid: u32) -> Result<(), String> {
        Ok(())
    }
}

/// CLI sidecar metadata stores canonical repository/worktree paths and log
/// locations. The active owner generation in SQLite, not the diagnostic PID,
/// authorizes cancellation.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskMetadata {
    repository: PathBuf,
    worktree: PathBuf,
    branch: String,
    base_ref: String,
    base_sha: String,
    agent_pid: Option<u32>,
    stdout_log: PathBuf,
    stderr_log: PathBuf,
    exit_code: Option<i32>,
    agent_outcome: Option<String>,
    candidate_commit: Option<String>,
    failure: Option<String>,
}

#[derive(Debug)]
struct Paths {
    task_dir: PathBuf,
    metadata: PathBuf,
    stdout_log: PathBuf,
    stderr_log: PathBuf,
    session_log: PathBuf,
}

#[derive(Debug, Clone, Copy)]
struct LifecycleIds<'a> {
    task: &'a str,
    attempt: &'a str,
    session: &'a str,
}

struct AgentMonitorOutcome {
    exit: AgentExit,
    cancellation_requested: bool,
    cancellation_succeeded: bool,
    cancellation_error: Option<String>,
    supervision_error: Option<String>,
}

struct CanonicalTarget {
    path: PathBuf,
    existing_ancestor: PathBuf,
}

pub fn data_dir(configured: Option<PathBuf>) -> Result<PathBuf, String> {
    let path = configured.unwrap_or_else(default_data_dir);
    canonical_target(&path).map(|target| target.path)
}

fn default_data_dir() -> PathBuf {
    #[cfg(windows)]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local).join("agent-orch");
    }
    #[cfg(not(windows))]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/share/agent-orch");
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".agent-orch")
}

pub async fn run_task(
    data_dir: &Path,
    repo_path: &Path,
    requested_id: Option<&str>,
    objective: &str,
    base_ref: &str,
    agent_program: &Path,
) -> Result<i32, String> {
    let repository = Repository::new(repo_path)
        .map_err(|error| format!("cannot use repository {}: {error}", repo_path.display()))?;
    let data_target = canonical_target(data_dir)?;
    ensure_outside_primary(&repository, &data_target)?;
    orchestrator_store::create_private_dir(&data_target.path).map_err(|error| {
        format!(
            "cannot create data directory {}: {error}",
            data_target.path.display()
        )
    })?;

    let worktrees_root = data_target.path.join("worktrees");
    let worktrees_target = canonical_target(&worktrees_root)?;
    ensure_outside_primary(&repository, &worktrees_target)?;

    let task_id = requested_id
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().simple().to_string());
    validate_task_id(&task_id)?;
    if objective.trim().is_empty() {
        return Err("task objective must not be empty".to_string());
    }

    let agent_path = resolve_program(agent_program)?;
    let adapter = CodexAdapter::new().with_program(agent_path.clone());
    let capabilities = adapter
        .detect()
        .await
        .map_err(|error| format!("cannot use Codex agent {}: {error}", agent_path.display()))?;
    if !capabilities.streaming || !capabilities.structured_output || !capabilities.cancellation {
        return Err(format!(
            "Codex agent {} lacks required streaming, structured-output, or cancellation capability",
            agent_path.display()
        ));
    }

    let lock = acquire_data_directory_lock(&data_target.path)?;
    let store = open_store(&data_target.path)?;
    ensure_project_registered(&store, &repository)?;
    reconcile_before_scheduling(&store)?;
    let contract = TaskContract {
        id: task_id.clone(),
        project_id: repository.path().display().to_string(),
        parent_id: None,
        objective: objective.to_string(),
        kind: TaskKind::Implement,
        agent: "codex".to_string(),
        dependencies: Vec::new(),
        verification: VerificationPolicy::None,
        delivery: DeliveryPolicy::LocalReview,
        autonomy: AutonomyPolicy::WorkspaceWrites,
        max_retries: 0,
        max_repairs: 0,
    };
    store
        .create(&contract)
        .map_err(|error| format!("cannot record task {task_id}: {error}"))?;
    fault_injection::before_admit(&data_target.path, &task_id)?;
    store
        .transition(&task_id, TaskEvent::Admit)
        .map_err(|error| format!("cannot admit task {task_id}: {error}"))?;

    let worktree =
        match create_task_worktree(&repository, &task_id, base_ref, &worktrees_target.path) {
            Ok(worktree) => worktree,
            Err(error) => {
                let _ = store.transition(&task_id, TaskEvent::CancelRequested);
                return Err(format!(
                    "cannot create worktree for task {task_id}: {error}"
                ));
            }
        };
    assert!(
        !worktree.path().starts_with(repository.path()),
        "orchestrator-git returned a worktree inside the primary checkout"
    );
    store
        .add_workspace(&Workspace {
            id: format!("{task_id}-workspace-1"),
            task_id: task_id.clone(),
            path: worktree.path().display().to_string(),
            branch: worktree.branch().to_string(),
            base_commit: worktree.base_sha().to_string(),
        })
        .map_err(|error| format!("cannot record worktree for task {task_id}: {error}"))?;
    let task_paths = paths(&data_target.path, &task_id);
    fs::create_dir_all(&task_paths.task_dir).map_err(|error| {
        let _ = store.transition(&task_id, TaskEvent::CancelRequested);
        format!(
            "cannot create task output directory {} for worktree {}: {error}",
            task_paths.task_dir.display(),
            worktree.path().display()
        )
    })?;

    let mut metadata = TaskMetadata {
        repository: repository.path().to_path_buf(),
        worktree: worktree.path().to_path_buf(),
        branch: worktree.branch().to_string(),
        base_ref: base_ref.to_string(),
        base_sha: worktree.base_sha().to_string(),
        agent_pid: None,
        stdout_log: task_paths.stdout_log.clone(),
        stderr_log: task_paths.stderr_log.clone(),
        exit_code: None,
        agent_outcome: None,
        candidate_commit: None,
        failure: None,
    };
    write_metadata(&task_paths.metadata, &metadata).map_err(|error| {
        let _ = store.transition(&task_id, TaskEvent::CancelRequested);
        format!(
            "cannot record worktree {} for task {task_id}: {error}",
            worktree.path().display()
        )
    })?;

    let attempt_id = format!("{task_id}-attempt-1");
    let session_id = format!("{task_id}-session-1");
    let ids = LifecycleIds {
        task: &task_id,
        attempt: &attempt_id,
        session: &session_id,
    };
    let attempt = Attempt {
        id: attempt_id.clone(),
        task_id: task_id.clone(),
        attempt_number: 1,
        state: AttemptState::Pending,
    };
    store.add_attempt(&task_id, &attempt).map_err(|error| {
        format!("cannot record attempt {attempt_id} for task {task_id}: {error}")
    })?;
    store
        .transition(&task_id, TaskEvent::ExecutionStarted)
        .map_err(|error| format!("cannot start task {task_id}: {error}"))?;
    store
        .update_attempt_state(&task_id, &attempt_id, AttemptState::Running)
        .map_err(|error| format!("cannot start attempt {attempt_id}: {error}"))?;
    store
        .add_session(
            &task_id,
            &Session {
                id: session_id.clone(),
                attempt_id: attempt_id.clone(),
                adapter: "codex".to_string(),
                state: SessionState::Spawning,
            },
        )
        .map_err(|error| format!("cannot record session {session_id}: {error}"))?;

    fault_injection::before_agent_launch();
    let wrapper = std::env::current_exe()
        .map_err(|error| format!("cannot locate CLI executable for Codex supervision: {error}"))?;
    let wrapper_args = capture_args(&agent_path, &task_paths, objective)?;
    let handle = match adapter
        .spawn(AgentLaunchRequest {
            program: wrapper,
            args: wrapper_args,
            working_dir: metadata.worktree.clone(),
        })
        .await
    {
        Ok(handle) => handle,
        Err(error) => {
            fail_before_agent_exit(&store, &mut metadata, &task_paths, &ids, &error.to_string())?;
            return Err(format!(
                "failed to launch Codex for task {task_id} in {}: {error}",
                metadata.worktree.display()
            ));
        }
    };
    let agent_pid = handle.pid();
    metadata.agent_pid = Some(agent_pid);
    if let Err(error) = write_metadata(&task_paths.metadata, &metadata) {
        let mut handle = handle;
        handle.cancel().await.map_err(|cancel_error| {
            format!("cannot persist agent PID {agent_pid} in {}; cancellation also failed: {cancel_error}", task_paths.metadata.display())
        })?;
        let _ = handle.wait().await;
        fail_before_agent_exit(&store, &mut metadata, &task_paths, &ids, &error)?;
        return Err(format!(
            "cannot persist agent PID {agent_pid} in {}: {error}",
            task_paths.metadata.display()
        ));
    }
    // A Running session without a recorded identity cannot be reconciled or
    // recovered later, so identity capture and persistence are mandatory.
    let identity_recorded = fault_injection::before_identity_capture()
        .and_then(|()| capture_process_identity(agent_pid))
        .map_err(|error| format!("process identity unavailable for session {session_id}: {error}"))
        .and_then(|identity| {
            store
                .record_session_identity(&task_id, &session_id, &identity)
                .map_err(|error| {
                    format!("cannot persist identity for session {session_id}: {error}")
                })
        });
    if let Err(message) = identity_recorded {
        let _ = log_session_message(&task_paths.session_log, &message);
        let mut handle = handle;
        handle.cancel().await.map_err(|cancel_error| {
            format!("{message}; cancellation also failed: {cancel_error}")
        })?;
        let _ = handle.wait().await;
        fail_before_agent_exit(&store, &mut metadata, &task_paths, &ids, &message)?;
        return Err(message);
    }
    store
        .update_session_state(&task_id, &session_id, SessionState::Running)
        .map_err(|error| format!("cannot mark session {session_id} running: {error}"))?;
    let cancellation_owner = CancellationOwner::new(
        task_id.clone(),
        attempt_id.clone(),
        session_id.clone(),
        Uuid::new_v4().simple().to_string(),
    )
    .map_err(|error| format!("cannot create cancellation owner for {task_id}: {error}"))?;
    let cancellation_owner = match capture_exact_process_identity(std::process::id()) {
        Ok(identity) => cancellation_owner.with_exact_process_identity(identity),
        Err(error) => {
            eprintln!(
                "process identity unavailable for cancellation owner of task {task_id}: {error}"
            );
            cancellation_owner
        }
    };
    if let Err(registration_error) = store.register_cancellation_owner(&cancellation_owner) {
        let mut handle = handle;
        let cancel_error = handle.cancel().await.err().map(|error| error.to_string());
        let exit = match handle.wait().await {
            Ok(exit) => exit,
            Err(wait_error) => {
                let reason = format!(
                    "cannot register cancellation owner: {registration_error}; \
                     cannot confirm agent termination: {wait_error}; \
                     cancel error: {}",
                    cancel_error.as_deref().unwrap_or("none")
                );
                metadata.agent_outcome = Some("supervision-failed".to_string());
                metadata.failure = Some(reason.clone());
                write_metadata(&task_paths.metadata, &metadata)?;
                return Err(format!(
                    "{reason}; task {task_id} remains Running and worktree {} is preserved",
                    metadata.worktree.display()
                ));
            }
        };
        let code = match exit {
            AgentExit::Cancelled { code, .. }
            | AgentExit::Completed { code, .. }
            | AgentExit::Failed { code, .. } => code,
        };
        let reason = format!(
            "cannot register cancellation owner: {registration_error}; \
             startup cleanup cancel error: {}",
            cancel_error.as_deref().unwrap_or("none")
        );
        fail_after_agent_exit(&store, &mut metadata, &task_paths, &ids, code, &reason)?;
        return Err(format!(
            "{reason}; agent process was reaped and task {task_id} marked Failed; worktree {} is preserved",
            metadata.worktree.display()
        ));
    }

    println!("task: {task_id}");
    println!("worktree: {}", metadata.worktree.display());
    println!("branch: {}", metadata.branch);
    println!("Codex JSONL: {}", metadata.stdout_log.display());
    println!("session log: {}", task_paths.session_log.display());

    drop(lock);
    fault_injection::after_lock_release(agent_pid)?;
    let monitored = monitor_agent(handle, &store, &cancellation_owner).await;
    let current = store
        .get(&task_id)
        .map_err(|error| format!("cannot reload task {task_id} after agent exit: {error}"))?;
    if current.state == TaskState::Cancelled {
        return Err(format!(
            "task {task_id} was marked Cancelled before this run owner confirmed cancellation; \
             preserving worktree {} for inspection",
            metadata.worktree.display()
        ));
    }
    let monitored = match monitored {
        Ok(outcome) => outcome,
        Err(reason) => {
            metadata.agent_outcome = Some("supervision-failed".to_string());
            metadata.failure = Some(reason.clone());
            metadata.exit_code = None;
            write_metadata(&task_paths.metadata, &metadata)?;
            return Err(format!(
                "Codex supervision failed for task {task_id}: {reason}; \
                 process exit/reaping was not confirmed, task remains Running, \
                 cancellation is not acknowledged, and worktree {} is preserved",
                metadata.worktree.display()
            ));
        }
    };

    if let AgentExit::Cancelled { code, .. } = &monitored.exit {
        if monitored.cancellation_requested && monitored.cancellation_succeeded {
            finish_cancelled(
                &store,
                &mut metadata,
                &task_paths,
                &ids,
                *code,
                &cancellation_owner,
            )?;
            println!(
                "task {task_id} cancellation confirmed by owner after process {agent_pid} was reaped; worktree preserved at {}",
                metadata.worktree.display()
            );
            Ok(130)
        } else {
            let reason = if monitored.cancellation_requested {
                format!(
                    "agent reported cancellation, but the owner did not receive a successful \
                     cancel acknowledgement{}",
                    monitored
                        .cancellation_error
                        .as_deref()
                        .map(|error| format!(": {error}"))
                        .unwrap_or_default()
                )
            } else {
                "agent reported cancellation without a pending CLI cancellation request".to_string()
            };
            fail_after_agent_exit(&store, &mut metadata, &task_paths, &ids, *code, &reason)?;
            store
                .finish_cancellation_owner(
                    &cancellation_owner,
                    if monitored.cancellation_requested {
                        CancellationStatus::Failed
                    } else {
                        CancellationStatus::Rejected
                    },
                )
                .map_err(|error| {
                    format!(
                        "task {task_id} was marked Failed after an unconfirmed cancellation, \
                         but the owner result could not be persisted: {error}"
                    )
                })?;
            Err(format!(
                "{reason}; task {task_id} was not marked Cancelled; worktree {} is preserved",
                metadata.worktree.display()
            ))
        }
    } else if let Some(reason) = monitored.supervision_error.as_deref() {
        let code = agent_exit_code(&monitored.exit);
        fail_after_agent_exit(&store, &mut metadata, &task_paths, &ids, code, reason)?;
        store
            .finish_cancellation_owner(
                &cancellation_owner,
                cancellation_owner_resolution(&monitored),
            )
            .map_err(|error| {
                format!(
                    "task {task_id} was marked Failed after a supervision error, \
                     but the owner result could not be persisted: {error}"
                )
            })?;
        Err(format!(
            "Codex supervision failed for task {task_id}: {reason}; process was reaped, \
             task marked Failed, and worktree {} is preserved",
            metadata.worktree.display()
        ))
    } else {
        let cancellation_resolution = cancellation_owner_resolution(&monitored);
        match monitored.exit {
            AgentExit::Completed { code: Some(0), .. } => {
                store
                    .update_session_state(&task_id, &session_id, SessionState::Exited)
                    .map_err(|error| format!("cannot mark session {session_id} exited: {error}"))?;
                let commit = match commit_candidate(&metadata, &task_id) {
                    Ok(commit) => commit,
                    Err(error) => {
                        metadata.agent_outcome = Some("completed".to_string());
                        metadata.exit_code = Some(0);
                        metadata.failure = Some(error.clone());
                        write_metadata(&task_paths.metadata, &metadata)?;
                        store
                            .update_attempt_state(&task_id, &attempt_id, AttemptState::Failed)
                            .map_err(|store_error| {
                                format!("cannot fail attempt {attempt_id}: {store_error}")
                            })?;
                        store
                            .transition(&task_id, TaskEvent::ExecutionFailed)
                            .map_err(|store_error| {
                                format!("cannot fail task {task_id}: {store_error}")
                            })?;
                        store
                            .finish_cancellation_owner(&cancellation_owner, cancellation_resolution)
                            .map_err(|store_error| {
                                format!(
                                    "cannot close cancellation owner for failed task {task_id}: \
                                 {store_error}"
                                )
                            })?;
                        return Err(format!(
                            "could not record candidate commit for task {task_id}: {error}"
                        ));
                    }
                };
                metadata.agent_outcome = Some("completed".to_string());
                metadata.exit_code = Some(0);
                metadata.candidate_commit = Some(commit.clone());
                write_metadata(&task_paths.metadata, &metadata)?;
                store
                    .update_attempt_state(&task_id, &attempt_id, AttemptState::CandidateReady)
                    .map_err(|error| {
                        format!("cannot mark attempt {attempt_id} candidate-ready: {error}")
                    })?;
                store
                    .transition(&task_id, TaskEvent::CandidateReady)
                    .map_err(|error| {
                        format!("cannot record candidate for task {task_id}: {error}")
                    })?;
                store
                .finish_cancellation_owner(&cancellation_owner, cancellation_resolution)
                .map_err(|error| {
                    format!(
                        "candidate was recorded, but cancellation owner could not be closed: {error}"
                    )
                })?;
                println!("candidate: {commit}");
                println!("status: Verifying (candidate preserved for review)");
                println!("exit code: 0");
                Ok(0)
            }
            AgentExit::Completed { code, .. } => {
                fail_after_agent_exit(
                    &store,
                    &mut metadata,
                    &task_paths,
                    &ids,
                    code,
                    "Codex completed the turn with a nonzero or unavailable exit code",
                )?;
                store
                .finish_cancellation_owner(&cancellation_owner, cancellation_resolution)
                .map_err(|error| {
                    format!(
                        "task {task_id} was marked Failed, but cancellation owner could not be closed: {error}"
                    )
                })?;
                Err(format!("Codex returned exit code {code:?} for task {task_id}; inspect worktree {} and logs {} / {}", metadata.worktree.display(), metadata.stdout_log.display(), metadata.stderr_log.display()))
            }
            AgentExit::Failed { code, reason } => {
                let detail =
                    reason.unwrap_or_else(|| "Codex exited without a completed turn".to_string());
                fail_after_agent_exit(&store, &mut metadata, &task_paths, &ids, code, &detail)?;
                store
                .finish_cancellation_owner(&cancellation_owner, cancellation_resolution)
                .map_err(|error| {
                    format!(
                        "task {task_id} was marked Failed, but cancellation owner could not be closed: {error}"
                    )
                })?;
                Err(format!(
                "Codex failed for task {task_id}: {detail}; inspect worktree {} and logs {} / {}",
                metadata.worktree.display(),
                metadata.stdout_log.display(),
                metadata.stderr_log.display()
            ))
            }
            AgentExit::Cancelled { .. } => unreachable!("cancelled exit handled above"),
        }
    }
}

fn finish_cancelled(
    store: &SqliteStore,
    metadata: &mut TaskMetadata,
    task_paths: &Paths,
    ids: &LifecycleIds<'_>,
    code: Option<i32>,
    owner: &CancellationOwner,
) -> Result<(), String> {
    metadata.agent_outcome = Some("cancelled".to_string());
    metadata.exit_code = code;
    write_metadata(&task_paths.metadata, metadata)?;
    store.confirm_cancellation(owner).map_err(|error| {
        format!(
            "cannot confirm owner cancellation for task {}: {error}",
            ids.task
        )
    })?;
    Ok(())
}

fn fail_before_agent_exit(
    store: &SqliteStore,
    metadata: &mut TaskMetadata,
    task_paths: &Paths,
    ids: &LifecycleIds<'_>,
    reason: &str,
) -> Result<(), String> {
    metadata.agent_outcome = Some("failed".to_string());
    metadata.failure = Some(reason.to_string());
    write_metadata(&task_paths.metadata, metadata)?;
    store
        .update_session_state(ids.task, ids.session, SessionState::Aborted)
        .map_err(|error| format!("cannot abort session {}: {error}", ids.session))?;
    store
        .update_attempt_state(ids.task, ids.attempt, AttemptState::Failed)
        .map_err(|error| format!("cannot fail attempt {}: {error}", ids.attempt))?;
    store
        .transition(ids.task, TaskEvent::ExecutionFailed)
        .map_err(|error| format!("cannot fail task {}: {error}", ids.task))?;
    Ok(())
}

fn fail_after_agent_exit(
    store: &SqliteStore,
    metadata: &mut TaskMetadata,
    task_paths: &Paths,
    ids: &LifecycleIds<'_>,
    code: Option<i32>,
    reason: &str,
) -> Result<(), String> {
    metadata.agent_outcome = Some("failed".to_string());
    metadata.exit_code = code;
    metadata.failure = Some(reason.to_string());
    write_metadata(&task_paths.metadata, metadata)?;
    store
        .update_session_state(ids.task, ids.session, SessionState::Exited)
        .map_err(|error| format!("cannot mark session {} exited: {error}", ids.session))?;
    store
        .update_attempt_state(ids.task, ids.attempt, AttemptState::Failed)
        .map_err(|error| format!("cannot fail attempt {}: {error}", ids.attempt))?;
    store
        .transition(ids.task, TaskEvent::ExecutionFailed)
        .map_err(|error| format!("cannot fail task {}: {error}", ids.task))?;
    Ok(())
}

fn agent_exit_code(exit: &AgentExit) -> Option<i32> {
    match exit {
        AgentExit::Cancelled { code, .. }
        | AgentExit::Completed { code, .. }
        | AgentExit::Failed { code, .. } => *code,
    }
}

fn cancellation_owner_resolution(outcome: &AgentMonitorOutcome) -> CancellationStatus {
    if outcome.cancellation_error.is_some() {
        CancellationStatus::Failed
    } else {
        CancellationStatus::Rejected
    }
}

async fn monitor_agent(
    mut handle: Box<dyn AgentHandle>,
    store: &SqliteStore,
    owner: &CancellationOwner,
) -> Result<AgentMonitorOutcome, String> {
    let mut cancellation_requested = false;
    let mut cancellation_attempted = false;
    let mut cancellation_succeeded = false;
    let mut cancellation_error = None;
    let mut last_poll_error = None;
    let mut supervision_error = None;

    loop {
        if !cancellation_attempted {
            match store.cancellation_request(owner) {
                Ok(Some(CancellationStatus::Pending)) => {
                    cancellation_requested = true;
                    cancellation_attempted = true;
                    match handle.cancel().await {
                        Ok(()) => {
                            cancellation_succeeded = true;
                            let exit = handle.wait().await.map_err(|error| {
                                format!(
                                    "owner sent cancellation but could not confirm process \
                                     termination/reaping: {error}"
                                )
                            })?;
                            return Ok(AgentMonitorOutcome {
                                exit,
                                cancellation_requested,
                                cancellation_succeeded,
                                cancellation_error,
                                supervision_error,
                            });
                        }
                        Err(error) => {
                            let error = error.to_string();
                            eprintln!(
                                "cancellation request for task {} could not be sent through \
                                 its agent handle: {error}",
                                owner.task_id
                            );
                            cancellation_error = Some(error);
                        }
                    }
                }
                Ok(Some(CancellationStatus::Confirmed)) => {
                    cancellation_attempted = true;
                    cancellation_requested = true;
                    cancellation_error = Some(
                        "request was confirmed before this owner could consume it".to_string(),
                    );
                }
                Ok(Some(CancellationStatus::Rejected | CancellationStatus::Failed)) => {
                    cancellation_attempted = true;
                }
                Ok(None) => last_poll_error = None,
                Err(error) => {
                    let error = error.to_string();
                    if last_poll_error.as_deref() != Some(error.as_str()) {
                        eprintln!(
                            "cannot poll cancellation request for task {}: {error}",
                            owner.task_id
                        );
                        last_poll_error = Some(error);
                    }
                }
            }
        }

        let event_result = tokio::select! {
            result = handle.next_event() => Some(result),
            _ = tokio::time::sleep(Duration::from_millis(100)) => None,
        };
        let Some(event_result) = event_result else {
            continue;
        };
        match event_result {
            Ok(Some(AgentEvent::SessionExited { .. })) => {
                let exit = handle
                    .wait()
                    .await
                    .map_err(|error| format!("agent wait failed after exit event: {error}"))?;
                return Ok(AgentMonitorOutcome {
                    exit,
                    cancellation_requested,
                    cancellation_succeeded,
                    cancellation_error,
                    supervision_error,
                });
            }
            Ok(Some(AgentEvent::SessionStarted { session_id })) => {
                println!("Codex session: {session_id}")
            }
            Ok(Some(AgentEvent::MessageDelta { text })) => println!("{text}"),
            Ok(Some(AgentEvent::ToolStarted { name, .. })) => println!("tool started: {name}"),
            Ok(Some(AgentEvent::ToolFinished { success, .. })) => {
                println!("tool finished: {success}")
            }
            Ok(Some(AgentEvent::ApprovalRequested { description, .. })) => {
                println!("approval requested: {description}")
            }
            Ok(Some(AgentEvent::TurnFinished { outcome })) => println!("turn: {outcome:?}"),
            Ok(Some(AgentEvent::ProtocolError { message })) => {
                eprintln!("Codex protocol error: {message}")
            }
            Ok(None) => {
                supervision_error =
                    Some("agent event stream ended without a process exit event".to_string());
                break;
            }
            Err(error) => {
                supervision_error = Some(format!("agent event stream failed: {error}"));
                break;
            }
        }
    }

    if !cancellation_requested {
        if matches!(
            store.cancellation_request(owner),
            Ok(Some(CancellationStatus::Pending))
        ) {
            cancellation_requested = true;
        }
    }
    match handle.cancel().await {
        Ok(()) => cancellation_succeeded = true,
        Err(error) if cancellation_error.is_none() => {
            cancellation_error = Some(error.to_string());
        }
        Err(_) => {}
    }
    let exit = handle.wait().await.map_err(|error| {
        format!(
            "{}; process termination/reaping was not confirmed: {error}",
            supervision_error
                .as_deref()
                .unwrap_or("agent event stream failed")
        )
    })?;
    Ok(AgentMonitorOutcome {
        exit,
        cancellation_requested,
        cancellation_succeeded,
        cancellation_error,
        supervision_error,
    })
}

pub fn list_tasks(data_dir: &Path) -> Result<(), String> {
    let store = open_store(data_dir)?;
    let tasks = store
        .list()
        .map_err(|error| format!("cannot list tasks in {}: {error}", data_dir.display()))?;
    if tasks.is_empty() {
        println!("no tasks");
        return Ok(());
    }
    for task in tasks {
        println!("{}\t{:?}", task.contract.id, task.state);
    }
    Ok(())
}

pub fn inspect_task(data_dir: &Path, task_id: &str) -> Result<(), String> {
    validate_task_id(task_id)?;
    let store = open_store(data_dir)?;
    let task = store
        .get(task_id)
        .map_err(|error| format!("cannot inspect task {task_id}: {error}"))?;
    println!("task: {}", task.contract.id);
    println!("status: {:?}", task.state);
    for owner in store
        .open_cancellation_owners()
        .map_err(|error| format!("cannot inspect cancellation owners for {task_id}: {error}"))?
        .into_iter()
        .filter(|owner| owner.task_id == task_id)
    {
        let owner_status = owner
            .exact_process_identity()
            .map(observe_owner_process)
            .unwrap_or(ProcessClassification::Unknown);
        println!(
            "cancellation owner: attempt {} session {} generation {} (exact owner process: {:?})",
            owner.attempt_id,
            owner.session_id,
            owner.generation(),
            owner_status
        );
        if owner_status == ProcessClassification::Unknown {
            println!(
                "  manual recovery: automatic identity is unavailable or inconclusive; after \
                 confirming the owner and managed process tree have exited and this attempt \
                 and session are terminal, run `agent-orch recover-owner {task_id} --attempt-id \
                 {} --session-id {} --generation {} --confirm-processes-exited --reason \
                 \"<reason>\"`",
                owner.attempt_id,
                owner.session_id,
                owner.generation()
            );
        }
    }
    match store
        .cancellation_status_for_task(task_id)
        .map_err(|error| format!("cannot inspect cancellation status for {task_id}: {error}"))?
    {
        Some(status) => println!("cancellation request: {status:?}"),
        None if task.state == TaskState::Running => {
            println!("cancellation request: none or owner not registered")
        }
        None if task.state == TaskState::Cancelled && !task.sessions.is_empty() => {
            println!("cancellation request: no run-owner confirmation recorded; needs review")
        }
        None => {}
    }
    println!("agent: {}", task.contract.agent);
    println!("repository: {}", task.contract.project_id);
    println!("objective: {}", task.contract.objective);
    if task.attempts.is_empty() {
        println!("attempts: none");
    } else {
        println!("attempts:");
        for attempt in &task.attempts {
            println!(
                "  {}\t{}\t{:?}",
                attempt.attempt_number, attempt.id, attempt.state
            );
        }
    }
    if task.sessions.is_empty() {
        println!("sessions: none");
    } else {
        println!("sessions:");
        for session in &task.sessions {
            println!("  {}\t{}\t{:?}", session.id, session.adapter, session.state);
            let identity = store
                .session_identity(task_id, &session.id)
                .map_err(|error| {
                    format!(
                        "cannot inspect identity for session {}: {error}",
                        session.id
                    )
                })?;
            println!(
                "    process identity: {}",
                if identity.is_some() {
                    "recorded"
                } else {
                    "unavailable"
                }
            );
        }
    }
    let metadata_path = paths(data_dir, task_id).metadata;
    if !metadata_path.is_file() {
        println!("worktree: not recorded");
        return Ok(());
    }
    let metadata = read_metadata(data_dir, task_id)?;
    println!("worktree: {}", metadata.worktree.display());
    println!("branch: {}", metadata.branch);
    println!("base: {} ({})", metadata.base_ref, metadata.base_sha);
    println!("agent PID: {}", display_option(metadata.agent_pid));
    println!("exit code: {}", display_option(metadata.exit_code));
    println!(
        "agent outcome: {}",
        metadata
            .agent_outcome
            .as_deref()
            .unwrap_or("running or not recorded")
    );
    println!(
        "candidate commit: {}",
        metadata
            .candidate_commit
            .as_deref()
            .unwrap_or("not recorded")
    );
    println!("Codex JSONL: {}", metadata.stdout_log.display());
    println!("Codex stderr: {}", metadata.stderr_log.display());
    println!(
        "session log: {}",
        paths(data_dir, task_id).session_log.display()
    );
    if let Some(failure) = metadata.failure {
        println!("failure: {failure}");
    }
    if !metadata.worktree.exists() {
        println!("worktree state: removed or missing");
    }
    Ok(())
}

/// Requests cancellation from the run process that owns the live agent handle.
/// Reconciliation fails the request if that exact owner process has exited.
pub fn cancel_task(data_dir: &Path, task_id: &str) -> Result<(), String> {
    validate_task_id(task_id)?;
    let _lock = acquire_data_directory_lock(data_dir)?;
    let store = open_store(data_dir)?;
    let mut owner_blockers = reconcile_cancellation_owners(&store)?;
    let task = store
        .get(task_id)
        .map_err(|error| format!("cannot cancel task {task_id}: {error}"))?;
    if task.state != TaskState::Running {
        return Err(format!(
            "task {task_id} is {:?}, not Running; no cancellation request was sent",
            task.state
        ));
    }
    let Some(session) = task
        .sessions
        .iter()
        .rev()
        .find(|session| !session.state.is_terminal())
    else {
        return Err(format!("task {task_id} has no unfinished session"));
    };
    let attempt = task
        .attempts
        .iter()
        .find(|attempt| attempt.id == session.attempt_id)
        .ok_or_else(|| {
            format!(
                "session {}/{} references missing attempt {}",
                task_id, session.id, session.attempt_id
            )
        })?;
    if attempt.state != AttemptState::Running {
        return Err(format!(
            "attempt {} is {:?}; refusing to request cancellation",
            attempt.id, attempt.state
        ));
    }
    let task_owner_prefix = format!("cancellation owner for task {task_id} ");
    let initial_recovery_blocker = owner_blockers
        .iter()
        .find(|blocker| blocker.starts_with(&task_owner_prefix))
        .cloned();
    let status = store
        .request_cancellation_for_task(task_id)
        .map_err(|error| {
            let recovery_hint = initial_recovery_blocker
                .as_deref()
                .map(|blocker| format!("; recovery blocker: {blocker}"))
                .unwrap_or_default();
            format!("cannot request cancellation for task {task_id}: {error}{recovery_hint}")
        })?;
    let status = if status == CancellationStatus::Pending {
        owner_blockers.extend(reconcile_cancellation_owners(&store)?);
        store
            .cancellation_status_for_task(task_id)
            .map_err(|error| format!("cannot recheck cancellation for task {task_id}: {error}"))?
            .unwrap_or(status)
    } else {
        status
    };
    match status {
        CancellationStatus::Pending => {
            let recovery_hint = owner_blockers
                .iter()
                .find(|blocker| blocker.starts_with(&task_owner_prefix))
                .map(|blocker| format!("; unresolved owner: {blocker}"))
                .unwrap_or_default();
            println!(
                "cancellation pending for task {task_id}; the run owner has not confirmed \
                 process termination, task remains Running, and worktree is preserved; \
                 use `agent-orch inspect {task_id}` to check status{recovery_hint}"
            );
            Ok(())
        }
        CancellationStatus::Confirmed => {
            println!("task {task_id} cancellation was already confirmed by its run owner");
            Ok(())
        }
        CancellationStatus::Rejected => Err(format!(
            "task {task_id} cancellation was not confirmed: the run owner reports that the \
             agent exited without cancellation; inspect task state and preserved worktree"
        )),
        CancellationStatus::Failed => Err(format!(
            "task {task_id} cancellation was not confirmed: the run owner could not cancel \
             and reap the agent; task was not marked Cancelled and worktree is preserved"
        )),
    }
}

pub fn recover_cancellation_owner(
    data_dir: &Path,
    task_id: &str,
    attempt_id: &str,
    session_id: &str,
    generation: &str,
    processes_confirmed_exited: bool,
    reason: &str,
) -> Result<(), String> {
    validate_task_id(task_id)?;
    validate_task_id(attempt_id)?;
    validate_task_id(session_id)?;
    if !processes_confirmed_exited {
        return Err(
            "manual recovery requires --confirm-processes-exited after verifying the owner and \
             managed process tree are stopped"
                .to_string(),
        );
    }

    let _lock = acquire_data_directory_lock(data_dir)?;
    let store = open_store(data_dir)?;
    let owner = store
        .open_cancellation_owners()
        .map_err(|error| format!("cannot list cancellation owners for recovery: {error}"))?
        .into_iter()
        .find(|owner| {
            owner.task_id == task_id
                && owner.attempt_id == attempt_id
                && owner.session_id == session_id
                && owner.generation() == generation
        })
        .ok_or_else(|| {
            format!(
                "no open cancellation owner matches task {task_id}, attempt {attempt_id}, \
                 session {session_id}, generation {generation}"
            )
        })?;

    if owner.exact_process_identity().map(observe_owner_process)
        == Some(ProcessClassification::Alive)
    {
        return Err(format!(
            "cancellation owner generation {generation} is still alive; refusing manual recovery"
        ));
    }
    let session_classification = session_process_classification(&store, task_id, session_id)?;
    // Fail closed: only a proven-gone session process may be released. Unknown
    // includes a live group whose lead exited. Windows records no process
    // group (no job-object tracking), so descendants of a gone lead cannot be
    // detected there; the operator's --confirm-processes-exited is the guard.
    if let Some(classification) = session_classification {
        if !matches!(
            classification,
            ProcessClassification::Exited | ProcessClassification::IdentityMismatch
        ) {
            return Err(format!(
                "session process for {task_id}/{session_id} is {classification:?}, not proven \
                 exited (a live process group with an exited lead is Unknown; a reused or \
                 inconclusive lead PID with a live recorded group is refused); refusing manual \
                 recovery"
            ));
        }
    }

    store
        .manually_resolve_cancellation_owner(
            &owner,
            processes_confirmed_exited,
            session_classification,
            reason,
        )
        .map_err(|error| {
            format!("cannot manually recover owner generation {generation}: {error}")
        })?;
    println!(
        "owner generation {generation} was manually released; a pending cancellation request \
         was marked Failed, task lifecycle was not changed, and a durable audit event was recorded"
    );
    Ok(())
}

#[cfg(unix)]
fn recorded_group_alive(group: u32) -> bool {
    orchestrator_runtime::supervisor::process_group_alive(group)
}

#[cfg(not(unix))]
fn recorded_group_alive(_group: u32) -> bool {
    false
}

/// Classifies a session's recorded process for `recover-owner`.
///
/// `Exited` and `IdentityMismatch` both mean the session is provably gone.
/// `IdentityMismatch` is kept only for PID reuse: the lead PID is alive with a
/// different start time, and the recorded group is provably dead. A live
/// recorded group always refuses, even when the lead is a member of it, because
/// a start-time drift (for example a wall-clock step) is indistinguishable from
/// PID reuse and the lead may still be the original session process.
/// A same-start-time lead that changed group, missing start times, an absent
/// lead with a live group, or any live recorded group stay `Unknown`, so they
/// are refused. Windows records no process group and has no job-object
/// tracking, so only the start-time check applies there.
/// Classifies the recorded session process (and its process group); `None`
/// when no identity was recorded.
fn session_process_classification(
    store: &SqliteStore,
    task_id: &str,
    session_id: &str,
) -> Result<Option<ProcessClassification>, String> {
    Ok(store
        .session_identity(task_id, session_id)
        .map_err(|error| format!("cannot inspect session identity for {session_id}: {error}"))?
        .map(|identity| {
            session_recovery_classification(
                identity,
                observe_process(identity),
                recorded_group_alive,
            )
        }))
}

/// Cleanup guard: only a recorded, provably gone session process (and group)
/// permits removal. Missing, Alive or Unknown classifications refuse.
fn require_session_gone(
    task_id: &str,
    session_id: &str,
    classification: Option<ProcessClassification>,
) -> Result<(), String> {
    match classification {
        Some(ProcessClassification::Exited | ProcessClassification::IdentityMismatch) => Ok(()),
        other => Err(format!(
            "refusing cleanup: session {session_id} process of task {task_id} is not provably \
             gone (recorded session identity classification: {other:?}); the worktree is \
             preserved. After the agent tree has exited, retry; `agent-orch inspect {task_id}` \
             shows the recorded agent PID"
        )),
    }
}

fn session_recovery_classification(
    identity: ProcessIdentity,
    observation: ProcessObservation,
    group_alive: impl Fn(u32) -> bool,
) -> ProcessClassification {
    let classification = plan_process_reconciliation(identity, observation).classification;
    if classification != ProcessClassification::IdentityMismatch {
        return classification;
    }
    let ProcessObservation::Alive(observed) = observation else {
        return ProcessClassification::Unknown;
    };
    let pid_reused = observed.pid == identity.pid
        && observed.platform == identity.platform
        && matches!(
            (identity.start_time_micros, observed.start_time_micros),
            (Some(recorded), Some(current)) if recorded != current
        );
    let group_gone = match identity.process_group {
        Some(group) => !group_alive(group),
        None => identity.platform == ProcessPlatform::Windows,
    };
    if pid_reused && group_gone {
        ProcessClassification::IdentityMismatch
    } else {
        ProcessClassification::Unknown
    }
}

pub fn show_diff(data_dir: &Path, task_id: &str) -> Result<(), String> {
    let metadata = validated_metadata(data_dir, task_id)?;
    if !metadata.worktree.exists() {
        return Err(format!(
            "task worktree {} is missing; inspect task {} metadata",
            metadata.worktree.display(),
            task_id
        ));
    }
    let tracked = git_output(
        &metadata.worktree,
        &[
            "diff",
            "--no-ext-diff",
            "--binary",
            &metadata.base_sha,
            "--",
        ],
    )?;
    let mut diff = tracked.stdout;
    let untracked = git_output(
        &metadata.worktree,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    for path in untracked
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = std::str::from_utf8(path).map_err(|_| {
            format!(
                "Git returned a non-UTF-8 untracked path for worktree {}",
                metadata.worktree.display()
            )
        })?;
        let output = git_output_allow_diff_exit(
            &metadata.worktree,
            &[
                "diff",
                "--no-index",
                "--no-ext-diff",
                "--binary",
                "--",
                NULL_DEVICE,
                path,
            ],
        )?;
        diff.extend_from_slice(&output.stdout);
    }
    if diff.is_empty() {
        println!("no candidate diff from base {}", metadata.base_sha);
    } else {
        io::stdout()
            .write_all(&diff)
            .map_err(|error| format!("cannot write candidate diff for task {task_id}: {error}"))?;
    }
    Ok(())
}

pub fn cleanup_task(data_dir: &Path, task_id: &str) -> Result<(), String> {
    validate_task_id(task_id)?;
    let _lock = acquire_data_directory_lock(data_dir)?;
    let store = open_store(data_dir)?;
    let task = store
        .get(task_id)
        .map_err(|error| format!("cannot clean up task {task_id}: {error}"))?;
    // The run owner finishes without the data lock and closes its cancellation
    // owner last, so an open owner that is not provably exited may still use
    // the worktree. Read-only: no owner is retired and no process is signalled.
    let owners: Vec<_> = store
        .open_cancellation_owners()
        .map_err(|error| format!("cannot list cancellation owners for cleanup: {error}"))?
        .into_iter()
        .filter(|owner| owner.task_id == task_id)
        .collect();
    // `run_task` holds the data lock through owner registration, so a live run
    // cannot interleave here. A non-terminal task with no owner is therefore an
    // inconsistent state left by a startup that crashed or failed before the
    // owner was published: preserve it for inspection/recovery, never clean up.
    if owners.is_empty()
        && matches!(
            task.state,
            TaskState::Created | TaskState::Queued | TaskState::Running
        )
    {
        return Err(format!(
            "refusing cleanup: task {task_id} is {:?}, not settled, and has no published run \
             owner, which indicates a run that crashed or failed during startup; the worktree \
             is preserved pending inspection. Use `agent-orch inspect {task_id}`",
            task.state
        ));
    }
    for owner in owners {
        let classification = owner.exact_process_identity().map(observe_owner_process);
        if classification != Some(ProcessClassification::Exited) {
            return Err(format!(
                "refusing cleanup: run owner for task {task_id} session {} generation {} is \
                 not provably exited (exact owner process: {classification:?}); the worktree \
                 is preserved. Wait for the run to finish or use `agent-orch cancel \
                 {task_id}`; if the owner is gone, `agent-orch inspect {task_id}` shows the \
                 ids for `agent-orch recover-owner`",
                owner.session_id,
                owner.generation()
            ));
        }
        // The owner process exiting does not stop the supervised agent tree, so
        // the recorded session process (and its group) must be provably gone.
        require_session_gone(
            task_id,
            &owner.session_id,
            session_process_classification(&store, task_id, &owner.session_id)?,
        )?;
    }
    let metadata = validated_metadata(data_dir, task_id)?;
    let repository = Repository::new(&metadata.repository).map_err(|error| {
        format!(
            "cannot validate repository {}: {error}",
            metadata.repository.display()
        )
    })?;
    if !metadata.worktree.exists() {
        println!("worktree {} is already absent", metadata.worktree.display());
        return Ok(());
    }
    if metadata.worktree == repository.path() || metadata.worktree.starts_with(repository.path()) {
        return Err(format!(
            "refusing to remove path {} inside primary checkout {}",
            metadata.worktree.display(),
            repository.path().display()
        ));
    }
    let status = git_output(
        &metadata.worktree,
        &["status", "--porcelain", "--untracked-files=all"],
    )?;
    if !status.stdout.is_empty() {
        return Err(format!("refusing cleanup: worktree {} on branch {} is dirty; inspect with `git -C {} status --short`", metadata.worktree.display(), metadata.branch, metadata.worktree.display()));
    }
    let head = git_text(&metadata.worktree, &["rev-parse", "HEAD"])?;
    let reachable = git_output_allow_not_found(
        repository.path(),
        &["merge-base", "--is-ancestor", &head, &metadata.base_ref],
    )?;
    if !reachable.status.success() {
        return Err(format!("refusing cleanup: commit {head} from branch {} is not reachable from base ref {:?}; merge or otherwise land it before cleanup", metadata.branch, metadata.base_ref));
    }
    git_output(
        repository.path(),
        &[
            "worktree",
            "remove",
            "--",
            metadata.worktree.to_str().ok_or_else(|| {
                format!(
                    "worktree path {} is not valid UTF-8 for Git",
                    metadata.worktree.display()
                )
            })?,
        ],
    )?;
    println!(
        "removed clean, landed worktree {}",
        metadata.worktree.display()
    );
    Ok(())
}

fn validated_metadata(data_dir: &Path, task_id: &str) -> Result<TaskMetadata, String> {
    validate_task_id(task_id)?;
    let metadata = read_metadata(data_dir, task_id)?;
    let repository = Repository::new(&metadata.repository).map_err(|error| {
        format!(
            "cannot validate repository {} for task {task_id}: {error}",
            metadata.repository.display()
        )
    })?;
    if repository.path() != metadata.repository {
        return Err(format!(
            "task {task_id} repository metadata {} is not canonical",
            metadata.repository.display()
        ));
    }
    let data_target = canonical_target(data_dir)?;
    ensure_outside_primary(&repository, &data_target)?;
    if metadata.worktree == repository.path() || metadata.worktree.starts_with(repository.path()) {
        return Err(format!(
            "task {task_id} metadata points inside primary checkout: {}",
            metadata.worktree.display()
        ));
    }
    if metadata.worktree.exists() {
        ensure_registered_worktree(&repository, &metadata.worktree)?;
    }
    Ok(metadata)
}

fn ensure_registered_worktree(repository: &Repository, worktree: &Path) -> Result<(), String> {
    let canonical = fs::canonicalize(worktree).map_err(|error| {
        format!(
            "cannot resolve task worktree {}: {error}",
            worktree.display()
        )
    })?;
    let listing = git_text(repository.path(), &["worktree", "list", "--porcelain"])?;
    let registered = listing
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .any(|path| {
            fs::canonicalize(path)
                .map(|candidate| candidate == canonical)
                .unwrap_or(false)
        });
    if !registered {
        return Err(format!(
            "path {} is not a registered worktree of repository {}; refusing Git operation",
            worktree.display(),
            repository.path().display()
        ));
    }
    Ok(())
}

fn ensure_outside_primary(repository: &Repository, target: &CanonicalTarget) -> Result<(), String> {
    if target.path.starts_with(repository.path()) {
        return Err(format!(
            "refusing to write under primary checkout: target {} is inside {}",
            target.path.display(),
            repository.path().display()
        ));
    }
    assert!(
        !target.path.starts_with(repository.path()),
        "CLI data and worktree paths must be outside the primary checkout"
    );
    repository
        .worktrees_root_is_outside(&target.existing_ancestor)
        .map_err(|error| {
            format!(
                "refusing path {} because it is not outside repository worktrees: {error}",
                target.path.display()
            )
        })
}

fn canonical_target(path: &Path) -> Result<CanonicalTarget, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("cannot resolve relative path {}: {error}", path.display()))?
            .join(path)
    };
    let absolute = normalize_components(&absolute);
    let mut ancestor = absolute.clone();
    let mut missing = Vec::<OsString>::new();
    while !ancestor.exists() {
        let name = ancestor.file_name().ok_or_else(|| {
            format!(
                "cannot find existing parent for path {}",
                absolute.display()
            )
        })?;
        missing.push(name.to_os_string());
        if !ancestor.pop() {
            return Err(format!(
                "cannot find existing parent for path {}",
                absolute.display()
            ));
        }
    }
    let existing_ancestor = fs::canonicalize(&ancestor).map_err(|error| {
        format!(
            "cannot resolve existing parent {} for {}: {error}",
            ancestor.display(),
            absolute.display()
        )
    })?;
    let mut target = existing_ancestor.clone();
    for component in missing.iter().rev() {
        target.push(component);
    }
    Ok(CanonicalTarget {
        path: target,
        existing_ancestor,
    })
}

fn normalize_components(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

fn paths(data_dir: &Path, task_id: &str) -> Paths {
    let task_dir = data_dir.join("cli").join(task_id);
    Paths {
        metadata: task_dir.join("metadata.json"),
        stdout_log: task_dir.join("codex.jsonl"),
        stderr_log: task_dir.join("codex.stderr"),
        session_log: task_dir.join("session.log"),
        task_dir,
    }
}

fn acquire_data_directory_lock(data_dir: &Path) -> Result<DataDirectoryLock, String> {
    let deadline = Instant::now() + LOCK_WAIT_TIMEOUT;
    loop {
        match DataDirectoryLock::acquire(data_dir) {
            Ok(lock) => return Ok(lock),
            Err(
                DataDirectoryLockError::AlreadyHeld { .. }
                | DataDirectoryLockError::TakeoverRace { .. },
            ) if Instant::now() < deadline => {
                thread::sleep(LOCK_RETRY_INTERVAL);
            }
            Err(DataDirectoryLockError::AlreadyHeld { holder_pid }) => {
                return Err(format!(
                    "data directory lock is busy; holder PID {holder_pid} did not release within {} seconds",
                    LOCK_WAIT_TIMEOUT.as_secs()
                ));
            }
            Err(DataDirectoryLockError::TakeoverRace { holder_pid }) => {
                return Err(format!(
                    "data directory lock takeover remained contended; holder PID {holder_pid}"
                ));
            }
            Err(error) => {
                return Err(format!("cannot acquire data-directory lock: {error}"));
            }
        }
    }
}

fn open_store(data_dir: &Path) -> Result<SqliteStore, String> {
    SqliteStore::new(&data_dir.join(STORE_FILE_NAME))
        .map_err(|error| format!("cannot open task store in {}: {error}", data_dir.display()))
}

fn ensure_project_registered(store: &SqliteStore, repository: &Repository) -> Result<(), String> {
    let project_id = repository.path().display().to_string();
    let projects = store
        .list_projects()
        .map_err(|error| format!("cannot list registered projects: {error}"))?;
    if let Some(project) = projects.iter().find(|project| project.id == project_id) {
        if project.root_path == project_id {
            return Ok(());
        }
        return Err(format!(
            "registered project {project_id} has unexpected root {}",
            project.root_path
        ));
    }
    store
        .register_project(&Project {
            id: project_id.clone(),
            root_path: project_id,
            settings: serde_json::Value::Object(Default::default()),
        })
        .map_err(|error| format!("cannot register project: {error}"))
}

fn read_metadata(data_dir: &Path, task_id: &str) -> Result<TaskMetadata, String> {
    let path = paths(data_dir, task_id).metadata;
    let bytes = fs::read(&path)
        .map_err(|error| format!("cannot read task metadata {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid task metadata {}: {error}", path.display()))
}

fn write_metadata(path: &Path, metadata: &TaskMetadata) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(metadata)
        .map_err(|error| format!("cannot serialize metadata for {}: {error}", path.display()))?;
    let mut temp = path.as_os_str().to_os_string();
    temp.push(format!(".{}.tmp", std::process::id()));
    let temp = PathBuf::from(temp);
    fs::write(&temp, bytes)
        .map_err(|error| format!("cannot write task metadata {}: {error}", temp.display()))?;
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        format!("cannot replace task metadata {}: {error}", path.display())
    })
}

fn log_session_message(path: &Path, message: &str) -> Result<(), String> {
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("cannot open session log {}: {error}", path.display()))?;
    writeln!(log, "{message}")
        .map_err(|error| format!("cannot write session log {}: {error}", path.display()))
}
fn validate_task_id(task_id: &str) -> Result<(), String> {
    let valid = !task_id.is_empty()
        && task_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    if valid {
        Ok(())
    } else {
        Err(format!(
            "invalid task ID {task_id:?}; use only ASCII letters, digits, '-' or '_'"
        ))
    }
}

fn resolve_program(program: &Path) -> Result<PathBuf, String> {
    if program.is_absolute() || program.components().count() > 1 {
        let path = if program.is_absolute() {
            program.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|error| {
                    format!("cannot resolve agent path {}: {error}", program.display())
                })?
                .join(program)
        };
        Ok(path)
    } else {
        Ok(program.to_path_buf())
    }
}

// Codex rejects `--sandbox` with `--approve-for-me`; approval mode already uses workspace-write.
fn capture_args(
    agent_program: &Path,
    task_paths: &Paths,
    objective: &str,
) -> Result<Vec<String>, String> {
    Ok(vec![
        "__codex_capture".to_string(),
        path_string(agent_program)?,
        path_string(&task_paths.stdout_log)?,
        path_string(&task_paths.stderr_log)?,
        "--".to_string(),
        "exec".to_string(),
        "--json".to_string(),
        "--approve-for-me".to_string(),
        "--".to_string(),
        objective.to_string(),
    ])
}

fn path_string(path: &Path) -> Result<String, String> {
    path.to_str().map(str::to_string).ok_or_else(|| {
        format!(
            "path is not valid UTF-8 and cannot be passed to Codex supervisor: {}",
            path.display()
        )
    })
}

fn commit_candidate(metadata: &TaskMetadata, task_id: &str) -> Result<String, String> {
    let status = git_output(
        &metadata.worktree,
        &["status", "--porcelain", "--untracked-files=all"],
    )?;
    if !status.stdout.is_empty() {
        git_output(&metadata.worktree, &["add", "--all", "--"])?;
        let staged =
            git_output_allow_not_found(&metadata.worktree, &["diff", "--cached", "--quiet"])?;
        if !staged.status.success() {
            git_output(
                &metadata.worktree,
                &[
                    "-c",
                    "user.name=Agent Orchestrator",
                    "-c",
                    "user.email=agent-orch@localhost",
                    "commit",
                    "-m",
                    &format!("agent-orch candidate {task_id}"),
                ],
            )?;
        }
    }
    git_text(&metadata.worktree, &["rev-parse", "HEAD"])
}

fn git_text(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let output = git_output(cwd, args)?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_output(cwd: &Path, args: &[&str]) -> Result<Output, String> {
    let output = git_raw(cwd, args)?;
    if !output.status.success() {
        return Err(git_failure(cwd, args, &output));
    }
    Ok(output)
}

fn git_output_allow_diff_exit(cwd: &Path, args: &[&str]) -> Result<Output, String> {
    let output = git_raw(cwd, args)?;
    if !output.status.success() && output.status.code() != Some(1) {
        return Err(git_failure(cwd, args, &output));
    }
    Ok(output)
}

fn git_output_allow_not_found(cwd: &Path, args: &[&str]) -> Result<Output, String> {
    let output = git_raw(cwd, args)?;
    if !output.status.success() && output.status.code() != Some(1) {
        return Err(git_failure(cwd, args, &output));
    }
    Ok(output)
}

fn git_raw(cwd: &Path, args: &[&str]) -> Result<Output, String> {
    ProcessCommand::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .map_err(|error| {
            format!(
                "cannot run `git -C {} {}`: {error}",
                cwd.display(),
                args.join(" ")
            )
        })
}

fn git_failure(cwd: &Path, args: &[&str], output: &Output) -> String {
    format!(
        "`git -C {} {}` failed with {}: {}",
        cwd.display(),
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

fn display_option<T: std::fmt::Display>(value: Option<T>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "not recorded".to_string())
}

pub fn codex_capture_wrapper(args: &[OsString]) -> i32 {
    if args.len() < 5 || args[3] != OsStr::new("--") {
        eprintln!("invalid internal Codex capture arguments");
        return 2;
    }
    let program = PathBuf::from(&args[0]);
    let stdout_path = PathBuf::from(&args[1]);
    let stderr_path = PathBuf::from(&args[2]);
    let child_args = &args[4..];
    let stdout_log = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stdout_path)
    {
        Ok(file) => file,
        Err(error) => {
            eprintln!(
                "cannot create Codex JSONL log {}: {error}",
                stdout_path.display()
            );
            return 1;
        }
    };
    let stderr_log = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stderr_path)
    {
        Ok(file) => file,
        Err(error) => {
            eprintln!(
                "cannot create Codex stderr log {}: {error}",
                stderr_path.display()
            );
            return 1;
        }
    };
    let mut child = match ProcessCommand::new(&program)
        .args(child_args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            eprintln!("cannot launch Codex command {}: {error}", program.display());
            return 127;
        }
    };
    let stdout_reader = child.stdout.take().expect("stdout piped");
    let stderr_reader = child.stderr.take().expect("stderr piped");
    let stdout_thread =
        thread::spawn(move || capture_stream(stdout_reader, io::stdout(), stdout_log));
    let stderr_thread =
        thread::spawn(move || capture_stream(stderr_reader, io::stderr(), stderr_log));
    let status = child.wait();
    let stdout_result = stdout_thread
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("stdout capture thread panicked")));
    let stderr_result = stderr_thread
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("stderr capture thread panicked")));
    if let Err(error) = stdout_result {
        eprintln!("cannot capture Codex stdout: {error}");
        return 1;
    }
    if let Err(error) = stderr_result {
        eprintln!("cannot capture Codex stderr: {error}");
        return 1;
    }
    match status {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!(
                "cannot wait for Codex command {}: {error}",
                program.display()
            );
            1
        }
    }
}

fn capture_stream<R: Read, W: Write>(mut input: R, mut output: W, mut log: File) -> io::Result<()> {
    let mut buffer = [0u8; 8192];
    let mut first_error = None;
    loop {
        let count = match input.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        if let Err(error) = output.write_all(&buffer[..count]) {
            first_error.get_or_insert(error);
        }
        if let Err(error) = log.write_all(&buffer[..count]) {
            first_error.get_or_insert(error);
        }
    }
    if let Err(error) = output.flush() {
        first_error.get_or_insert(error);
    }
    if let Err(error) = log.flush() {
        first_error.get_or_insert(error);
    }
    first_error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn register_test_owner(
        store: &SqliteStore,
        task_id: &str,
        process_identity: Option<ProcessIdentity>,
    ) -> CancellationOwner {
        register_test_owner_in(
            store,
            task_id,
            process_identity,
            format!("project-{task_id}"),
        )
    }

    fn register_test_owner_in(
        store: &SqliteStore,
        task_id: &str,
        process_identity: Option<ProcessIdentity>,
        project_id: String,
    ) -> CancellationOwner {
        let (attempt_id, session_id) = start_test_run_in(store, task_id, project_id);
        let mut owner = CancellationOwner::new(
            task_id.into(),
            attempt_id,
            session_id,
            "0123456789abcdef0123456789abcdef".into(),
        )
        .unwrap();
        if let Some(identity) = process_identity {
            owner = owner.with_process_identity(identity);
        }
        store.register_cancellation_owner(&owner).unwrap();
        owner
    }

    /// A Running task with a running attempt and session, before any owner is
    /// published. Returns the attempt and session ids.
    fn start_test_run(store: &SqliteStore, task_id: &str) -> (String, String) {
        start_test_run_in(store, task_id, format!("project-{task_id}"))
    }

    fn start_test_run_in(
        store: &SqliteStore,
        task_id: &str,
        project_id: String,
    ) -> (String, String) {
        store
            .register_project(&Project {
                id: project_id.clone(),
                root_path: "/repo".into(),
                settings: serde_json::json!({}),
            })
            .unwrap();
        store
            .create(&TaskContract {
                id: task_id.into(),
                project_id,
                parent_id: None,
                objective: "test cancellation recovery".into(),
                kind: TaskKind::Implement,
                agent: "codex".into(),
                dependencies: Vec::new(),
                verification: VerificationPolicy::Commands {
                    commands: vec![vec!["true".into()]],
                },
                delivery: DeliveryPolicy::LocalReview,
                autonomy: AutonomyPolicy::WorkspaceWrites,
                max_retries: 0,
                max_repairs: 0,
            })
            .unwrap();
        store.transition(task_id, TaskEvent::Admit).unwrap();

        let attempt_id = format!("{task_id}-attempt-1");
        let session_id = format!("{task_id}-session-1");
        store
            .add_attempt(
                task_id,
                &Attempt {
                    id: attempt_id.clone(),
                    task_id: task_id.into(),
                    attempt_number: 1,
                    state: AttemptState::Pending,
                },
            )
            .unwrap();
        store
            .transition(task_id, TaskEvent::ExecutionStarted)
            .unwrap();
        store
            .update_attempt_state(task_id, &attempt_id, AttemptState::Running)
            .unwrap();
        store
            .add_session(
                task_id,
                &Session {
                    id: session_id.clone(),
                    attempt_id: attempt_id.clone(),
                    adapter: "codex".into(),
                    state: SessionState::Spawning,
                },
            )
            .unwrap();
        store
            .update_session_state(task_id, &session_id, SessionState::Running)
            .unwrap();

        (attempt_id, session_id)
    }

    fn git_in(cwd: &Path, args: &[&str]) {
        git_text(cwd, args).unwrap();
    }

    /// A clean worktree at its base commit with a registered run owner: every
    /// Git-only check (clean, HEAD reachable from base) passes, so only the
    /// owner check decides whether cleanup may proceed.
    fn owned_clean_worktree(
        task_id: &str,
        owner: Option<Option<ProcessIdentity>>,
    ) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("repo");
        let data_dir = root.path().join("data");
        fs::create_dir_all(&repo).unwrap();
        git_in(&repo, &["init", "-q", "-b", "main"]);
        git_in(&repo, &["config", "user.email", "t@example.invalid"]);
        git_in(&repo, &["config", "user.name", "t"]);
        git_in(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
        let repository = Repository::new(&repo).unwrap();
        let worktree = data_dir.join("worktrees").join(task_id);
        fs::create_dir_all(worktree.parent().unwrap()).unwrap();
        let branch = format!("task/{task_id}");
        git_in(
            repository.path(),
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                &branch,
                worktree.to_str().unwrap(),
                "main",
            ],
        );
        let worktree = fs::canonicalize(&worktree).unwrap();
        let store = open_store(&data_dir).unwrap();
        match owner {
            Some(identity) => {
                register_test_owner(&store, task_id, identity);
            }
            None => {
                start_test_run(&store, task_id);
            }
        }
        let task_paths = paths(&data_dir, task_id);
        fs::create_dir_all(&task_paths.task_dir).unwrap();
        write_metadata(
            &task_paths.metadata,
            &TaskMetadata {
                repository: repository.path().to_path_buf(),
                worktree: worktree.clone(),
                branch,
                base_ref: "main".into(),
                base_sha: git_text(&repo, &["rev-parse", "main"]).unwrap(),
                agent_pid: None,
                stdout_log: task_paths.stdout_log,
                stderr_log: task_paths.stderr_log,
                exit_code: None,
                agent_outcome: None,
                candidate_commit: None,
                failure: None,
            },
        )
        .unwrap();
        (root, data_dir, worktree)
    }

    /// Crashed-startup state: Running with an attempt and session but no owner.
    #[test]
    fn cleanup_refuses_clean_worktree_of_unsettled_task_without_a_published_owner() {
        let (_root, data_dir, worktree) = owned_clean_worktree("task-starting", None);
        let store = open_store(&data_dir).unwrap();
        assert!(store.open_cancellation_owners().unwrap().is_empty());
        assert_eq!(
            store.get("task-starting").unwrap().state,
            TaskState::Running
        );
        let error = cleanup_task(&data_dir, "task-starting").unwrap_err();
        assert!(error.contains("not settled"), "{error}");
        assert!(
            worktree.exists(),
            "cleanup removed a starting run's worktree"
        );
    }

    #[test]
    fn cleanup_removes_clean_landed_worktree_of_settled_task_without_an_owner() {
        let (_root, data_dir, worktree) = owned_clean_worktree("task-settled", None);
        let store = open_store(&data_dir).unwrap();
        let task = store.get("task-settled").unwrap();
        let (attempt, session) = (&task.attempts[0].id, &task.sessions[0].id);
        store
            .update_session_state("task-settled", session, SessionState::Exited)
            .unwrap();
        store
            .update_attempt_state("task-settled", attempt, AttemptState::CandidateReady)
            .unwrap();
        store
            .transition("task-settled", TaskEvent::CandidateReady)
            .unwrap();
        cleanup_task(&data_dir, "task-settled").unwrap();
        assert!(!worktree.exists());
    }

    #[test]
    fn cleanup_refuses_clean_worktree_when_owner_identity_is_unprovable() {
        let (_root, data_dir, worktree) = owned_clean_worktree("task-unproven-clean", Some(None));
        let error = cleanup_task(&data_dir, "task-unproven-clean").unwrap_err();
        assert!(error.contains("not provably exited"), "{error}");
        assert!(error.contains("recover-owner"), "{error}");
        assert!(
            worktree.exists(),
            "cleanup removed an unprovable owner's worktree"
        );
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_refuses_clean_worktree_while_owner_is_alive() {
        let identity = capture_process_identity(std::process::id()).expect("own identity");
        let (_root, data_dir, worktree) =
            owned_clean_worktree("task-live-clean", Some(Some(identity)));
        let error = cleanup_task(&data_dir, "task-live-clean").unwrap_err();
        assert!(error.contains("Some(Alive)"), "{error}");
        assert!(worktree.exists(), "cleanup removed a live owner's worktree");
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_refuses_exited_owner_while_its_session_process_is_alive() {
        let live = capture_process_identity(std::process::id()).expect("own identity");
        let (_root, data_dir, worktree) = exited_owner_clean_worktree("task-tree-alive");
        let store = open_store(&data_dir).unwrap();
        // The owner is provably exited, but the recorded session process is this live one.
        let session = store.get("task-tree-alive").unwrap().sessions[0].id.clone();
        store
            .record_session_identity("task-tree-alive", &session, &live)
            .unwrap();
        let error = cleanup_task(&data_dir, "task-tree-alive").unwrap_err();
        assert!(error.contains("not provably gone"), "{error}");
        assert!(
            worktree.exists(),
            "cleanup removed a worktree with a live session process"
        );
    }

    /// Identity captured from a child this test spawned, killed and reaped: its PID is absent.
    #[cfg(unix)]
    fn reaped_child_identity() -> ProcessIdentity {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let identity = capture_process_identity(child.id()).expect("child identity");
        child.kill().unwrap();
        child.wait().unwrap();
        identity
    }

    /// Owner proven exited because its recorded process is gone; returns the fixture.
    #[cfg(unix)]
    fn exited_owner_clean_worktree(task_id: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
        owned_clean_worktree(task_id, Some(Some(reaped_child_identity())))
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_follows_existing_contract_for_exited_owner_and_dead_session() {
        let (_root, data_dir, worktree) = exited_owner_clean_worktree("task-dead-clean");
        let store = open_store(&data_dir).unwrap();
        let session = store.get("task-dead-clean").unwrap().sessions[0].id.clone();
        let mut dead = reaped_child_identity();
        dead.process_group = None; // the child shared this test's live group
        store
            .record_session_identity("task-dead-clean", &session, &dead)
            .unwrap();
        cleanup_task(&data_dir, "task-dead-clean").unwrap();
        assert!(!worktree.exists());
        // Cleanup never retires or rewrites the owner record.
        assert_eq!(store.open_cancellation_owners().unwrap().len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_refuses_exited_owner_without_a_recorded_session_identity() {
        let (_root, data_dir, worktree) = exited_owner_clean_worktree("task-no-session-id");
        let error = cleanup_task(&data_dir, "task-no-session-id").unwrap_err();
        assert!(error.contains("not provably gone"), "{error}");
        assert!(error.contains("None"), "{error}");
        assert!(worktree.exists());
    }

    #[test]
    fn session_gone_guard_refuses_missing_alive_and_unknown_classifications() {
        let gone = [
            ProcessClassification::Exited,
            ProcessClassification::IdentityMismatch,
        ];
        for classification in gone {
            require_session_gone("t", "s", Some(classification)).unwrap();
        }
        for classification in [
            None,
            Some(ProcessClassification::Alive),
            Some(ProcessClassification::Unknown),
        ] {
            let error = require_session_gone("t", "s", classification).unwrap_err();
            assert!(error.contains("not provably gone"), "{error}");
        }
    }

    #[test]
    fn canonical_target_resolves_symlinked_ancestors_and_missing_suffixes() {
        let temp = tempfile::tempdir().unwrap();
        let target = canonical_target(&temp.path().join("new/child")).unwrap();
        let canonical_root = fs::canonicalize(temp.path()).unwrap();
        assert_eq!(target.path, canonical_root.join("new/child"));
        assert_eq!(target.existing_ancestor, canonical_root);
    }

    #[test]
    fn task_ids_reject_path_syntax_before_store_access() {
        assert!(validate_task_id("task_01-A").is_ok());
        assert!(validate_task_id("../task").is_err());
        assert!(validate_task_id("").is_err());
    }

    #[test]
    fn capture_stream_copies_raw_bytes_to_both_outputs() {
        let mut output = Vec::new();
        let temp = tempfile::tempdir().unwrap();
        let log_path = temp.path().join("out.log");
        let log = File::create(&log_path).unwrap();
        capture_stream(&b"{\"type\":\"turn.completed\"}\n"[..], &mut output, log).unwrap();
        assert_eq!(output, b"{\"type\":\"turn.completed\"}\n");
        assert_eq!(fs::read(log_path).unwrap(), output);
    }

    #[cfg(unix)]
    #[test]
    fn cancellation_owner_reconciliation_ignores_live_inherited_group_parent() {
        use std::process::Command;

        let directory = tempfile::tempdir().unwrap();
        let store = open_store(directory.path()).unwrap();
        let mut child = Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .expect("spawn child in inherited process group");
        let identity = capture_process_identity(child.id()).expect("capture child identity");
        let owner = register_test_owner(&store, "task-live-group-parent", Some(identity));
        store.request_cancellation_for_task(&owner.task_id).unwrap();

        child.kill().expect("kill owner child");
        let status = child.wait().expect("reap owner child");
        assert!(!status.success());
        assert_eq!(
            observe_process(identity),
            orchestrator_core::identity::ProcessObservation::Unknown
        );

        assert!(reconcile_cancellation_owners(&store).unwrap().is_empty());
        assert!(store.open_cancellation_owners().unwrap().is_empty());
        assert_eq!(
            store.cancellation_request(&owner).unwrap(),
            Some(CancellationStatus::Failed)
        );
    }

    #[cfg(unix)]
    fn legacy_windows_owner_identity() -> ProcessIdentity {
        ProcessIdentity {
            pid: std::process::id(),
            process_group: None,
            start_time_micros: Some(1_700_000_000_123_456),
            platform: ProcessPlatform::Windows,
        }
    }

    #[cfg(unix)]
    #[test]
    fn recover_owner_refuses_live_session_group_after_lead_exits() {
        use std::os::unix::process::CommandExt;
        use std::process::Command;

        let directory = tempfile::tempdir().unwrap();
        let data_dir = directory.path();
        let store = open_store(data_dir).unwrap();
        let owner = register_test_owner(
            &store,
            "task-live-session-group",
            Some(legacy_windows_owner_identity()),
        );
        store.request_cancellation_for_task(&owner.task_id).unwrap();

        // The lead shell exits at once; its background child keeps the group alive.
        let mut lead = Command::new("/bin/sh")
            .args(["-c", "/bin/sleep 30 &"])
            .process_group(0)
            .spawn()
            .expect("spawn group leader");
        let identity = capture_process_identity(lead.id()).expect("capture leader identity");
        store
            .record_session_identity(&owner.task_id, &owner.session_id, &identity)
            .unwrap();
        lead.wait().expect("reap leader");
        let group = identity.process_group.expect("leader has a group");
        assert_eq!(
            observe_process(identity),
            orchestrator_core::identity::ProcessObservation::Unknown
        );

        store
            .update_session_state(&owner.task_id, &owner.session_id, SessionState::Exited)
            .unwrap();
        store
            .update_attempt_state(&owner.task_id, &owner.attempt_id, AttemptState::Failed)
            .unwrap();
        store
            .transition(&owner.task_id, TaskEvent::ExecutionFailed)
            .unwrap();

        let recover = || {
            recover_cancellation_owner(
                data_dir,
                &owner.task_id,
                &owner.attempt_id,
                &owner.session_id,
                owner.generation(),
                true,
                "operator verified the owner and managed process tree exited",
            )
        };
        let refused = recover();
        let released_while_alive = store.open_cancellation_owners().unwrap().is_empty();
        let release_events = || {
            store
                .stream_events_after(0, 100)
                .unwrap()
                .into_iter()
                .filter(|event| event.kind == "cancellation.owner_manually_released")
                .collect::<Vec<_>>()
        };
        let events_while_alive = release_events().len();
        // Always reap the group before asserting so a failure cannot leak it.
        Command::new("/bin/sh")
            .args(["-c", &format!("kill -KILL -- -{group}")])
            .status()
            .expect("kill process group");
        let deadline = Instant::now() + Duration::from_secs(5);
        while observe_process(identity) != orchestrator_core::identity::ProcessObservation::Exited
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(20));
        }

        assert!(refused.unwrap_err().contains("not proven exited"));
        assert!(!released_while_alive);
        assert_eq!(events_while_alive, 0);
        assert_eq!(
            store.cancellation_request(&owner).unwrap(),
            Some(CancellationStatus::Pending)
        );

        recover().unwrap();
        assert!(store.open_cancellation_owners().unwrap().is_empty());
        let events = release_events();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].payload["session_process_classification"],
            "Exited"
        );
    }

    #[cfg(unix)]
    #[test]
    fn recover_owner_releases_reused_lead_pid_with_dead_group_and_audits_classification() {
        use std::os::unix::process::CommandExt;
        use std::process::Command;

        let directory = tempfile::tempdir().unwrap();
        let data_dir = directory.path();
        let store = open_store(data_dir).unwrap();
        let owner = register_test_owner(
            &store,
            "task-reused-lead-pid",
            Some(legacy_windows_owner_identity()),
        );
        store.request_cancellation_for_task(&owner.task_id).unwrap();

        // A reaped group leader supplies a dead recorded group.
        let mut dead_leader = Command::new("/bin/sh")
            .args(["-c", "true"])
            .process_group(0)
            .spawn()
            .expect("spawn short-lived leader");
        let dead_group = dead_leader.id();
        dead_leader.wait().expect("reap leader");
        // An unrelated live process now owns the recorded lead PID.
        let mut reused = Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .expect("spawn unrelated process");
        let live = capture_process_identity(reused.id()).expect("capture unrelated identity");
        let recorded = ProcessIdentity {
            process_group: Some(dead_group),
            start_time_micros: Some(live.start_time_micros.expect("start time") + 1),
            ..live
        };
        store
            .record_session_identity(&owner.task_id, &owner.session_id, &recorded)
            .unwrap();
        let observed = observe_process(recorded);
        let classification = plan_process_reconciliation(recorded, observed).classification;
        store
            .update_session_state(&owner.task_id, &owner.session_id, SessionState::Exited)
            .unwrap();
        store
            .update_attempt_state(&owner.task_id, &owner.attempt_id, AttemptState::Failed)
            .unwrap();
        store
            .transition(&owner.task_id, TaskEvent::ExecutionFailed)
            .unwrap();

        let result = recover_cancellation_owner(
            data_dir,
            &owner.task_id,
            &owner.attempt_id,
            &owner.session_id,
            owner.generation(),
            true,
            "operator verified the owner and managed process tree exited",
        );
        let _ = reused.kill();
        let _ = reused.wait();

        assert_eq!(classification, ProcessClassification::IdentityMismatch);
        result.unwrap();
        assert!(store.open_cancellation_owners().unwrap().is_empty());
        let events: Vec<_> = store
            .stream_events_after(0, 100)
            .unwrap()
            .into_iter()
            .filter(|event| event.kind == "cancellation.owner_manually_released")
            .collect();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].payload["session_process_classification"],
            "IdentityMismatch"
        );
    }

    #[cfg(unix)]
    #[test]
    fn recover_owner_refuses_live_lead_in_own_group_with_drifted_start_time() {
        use std::os::unix::process::CommandExt;
        use std::process::Command;

        let directory = tempfile::tempdir().unwrap();
        let data_dir = directory.path();
        let store = open_store(data_dir).unwrap();
        let owner = register_test_owner(
            &store,
            "task-live-lead-drift",
            Some(legacy_windows_owner_identity()),
        );
        store.request_cancellation_for_task(&owner.task_id).unwrap();

        let mut lead = Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .expect("spawn live lead");
        let live = capture_process_identity(lead.id()).expect("capture lead identity");
        assert_eq!(live.process_group, Some(lead.id()));
        // Simulate a wall-clock step that shifts the derived start time by 1s.
        let recorded = ProcessIdentity {
            start_time_micros: Some(live.start_time_micros.expect("start time") + 1_000_000),
            ..live
        };
        store
            .record_session_identity(&owner.task_id, &owner.session_id, &recorded)
            .unwrap();
        let classification =
            plan_process_reconciliation(recorded, observe_process(recorded)).classification;
        store
            .update_session_state(&owner.task_id, &owner.session_id, SessionState::Exited)
            .unwrap();
        store
            .update_attempt_state(&owner.task_id, &owner.attempt_id, AttemptState::Failed)
            .unwrap();
        store
            .transition(&owner.task_id, TaskEvent::ExecutionFailed)
            .unwrap();

        let result = recover_cancellation_owner(
            data_dir,
            &owner.task_id,
            &owner.attempt_id,
            &owner.session_id,
            owner.generation(),
            true,
            "operator verified the owner and managed process tree exited",
        );
        let still_open = !store.open_cancellation_owners().unwrap().is_empty();
        let release_events = store
            .stream_events_after(0, 100)
            .unwrap()
            .into_iter()
            .filter(|event| event.kind == "cancellation.owner_manually_released")
            .count();
        // Reap before asserting so a failure cannot leak the process.
        let _ = lead.kill();
        let _ = lead.wait();

        assert_eq!(classification, ProcessClassification::IdentityMismatch);
        assert!(result.unwrap_err().contains("not proven exited"));
        assert!(still_open);
        assert_eq!(release_events, 0);
    }

    #[test]
    fn recovery_classification_refuses_reused_lead_with_live_foreign_group() {
        let recorded = ProcessIdentity {
            pid: 4000,
            process_group: Some(4000),
            start_time_micros: Some(10),
            platform: ProcessPlatform::Unix,
        };
        let reused = |group, start| {
            ProcessObservation::Alive(ProcessIdentity {
                process_group: Some(group),
                start_time_micros: Some(start),
                ..recorded
            })
        };
        let classify = |observation, group_alive: bool| {
            session_recovery_classification(recorded, observation, move |_| group_alive)
        };
        // Reused lead PID in another group, recorded group dead: provably gone.
        assert_eq!(
            classify(reused(9000, 99), false),
            ProcessClassification::IdentityMismatch
        );
        // Reused lead PID in another group, recorded group still alive: refused.
        assert_eq!(
            classify(reused(9000, 99), true),
            ProcessClassification::Unknown
        );
        // Drifted start time on a lead inside its own live recorded group: refused.
        assert_eq!(
            classify(reused(4000, 99), true),
            ProcessClassification::Unknown
        );
        // Same process that merely changed group is not PID reuse: refused.
        assert_eq!(
            classify(reused(9000, 10), false),
            ProcessClassification::Unknown
        );
        // Absent lead with a live group stays Unknown; with a dead group, Exited.
        assert_eq!(
            classify(ProcessObservation::Unknown, true),
            ProcessClassification::Unknown
        );
        assert_eq!(
            classify(ProcessObservation::Exited, false),
            ProcessClassification::Exited
        );
    }

    #[cfg(unix)]
    #[test]
    fn running_task_with_dead_identityless_owner_reaches_audited_manual_recovery() {
        use std::os::unix::process::CommandExt;
        use std::process::Command;

        let (directory, repo, store) = gate_repo();
        let data_dir = directory.path().join("data");
        let data_dir = data_dir.as_path();
        // A legacy/Windows owner without an exact creation time is Unknown.
        let owner = register_test_owner_in(
            &store,
            "task-running-dead-owner",
            Some(legacy_windows_owner_identity()),
            repo.display().to_string(),
        );
        // The worktree was removed after the crash: Missing, preserved, permitted.
        record_test_workspace(
            &store,
            &owner.task_id,
            &repo,
            &data_dir.join("worktrees").join(&owner.task_id),
        );
        let mut child = Command::new("/bin/sleep")
            .arg("0.05")
            .process_group(0)
            .spawn()
            .expect("spawn short-lived session process");
        let identity = capture_process_identity(child.id()).expect("capture session identity");
        store
            .record_session_identity(&owner.task_id, &owner.session_id, &identity)
            .unwrap();
        child.wait().expect("reap session process");
        assert_eq!(store.get(&owner.task_id).unwrap().state, TaskState::Running);

        // Scheduling blocks on the owner, but the dead session is reconciled first.
        let error = reconcile_before_scheduling(&store).unwrap_err();
        assert!(error.contains("recover-owner"), "{error}");
        let record = store.get(&owner.task_id).unwrap();
        assert_eq!(record.state, TaskState::Failed);
        assert_eq!(record.attempts[0].state, AttemptState::Failed);
        assert_eq!(record.sessions[0].state, SessionState::Exited);

        // The documented recover-owner command is now accepted.
        recover_cancellation_owner(
            data_dir,
            &owner.task_id,
            &owner.attempt_id,
            &owner.session_id,
            owner.generation(),
            true,
            "operator verified the owner and managed process tree exited",
        )
        .unwrap();
        assert!(store.open_cancellation_owners().unwrap().is_empty());
        let events = store.stream_events_after(0, 100).unwrap();
        let released = events
            .iter()
            .find(|event| event.kind == "cancellation.owner_manually_released")
            .expect("audited release");
        assert_eq!(released.payload["session_process_classification"], "Exited");
        reconcile_before_scheduling(&store).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn reconciliation_tolerates_live_owner_mid_finish_but_not_dead_owner() {
        let directory = tempfile::tempdir().unwrap();
        let store = open_store(directory.path()).unwrap();
        // This test process stands in for a live run owner.
        let identity = capture_process_identity(std::process::id()).expect("own identity");
        let owner = register_test_owner(&store, "task-finishing", Some(identity));
        assert_eq!(
            observe_owner_process(owner.exact_process_identity().unwrap()),
            ProcessClassification::Alive
        );

        // Window 1: the owner marked the session Exited; the task is still Running.
        store
            .update_session_state(&owner.task_id, &owner.session_id, SessionState::Exited)
            .unwrap();
        reconcile_before_scheduling(&store).expect("live owner is still finishing");

        // Window 2: the owner moved the task to Verifying; it is still alive.
        store
            .update_attempt_state(
                &owner.task_id,
                &owner.attempt_id,
                AttemptState::CandidateReady,
            )
            .unwrap();
        store
            .transition(&owner.task_id, TaskEvent::CandidateReady)
            .unwrap();
        assert!(reconcile_cancellation_owners(&store).unwrap().is_empty());
        reconcile_before_scheduling(&store).expect("live owner is still finishing");
        assert_eq!(store.open_cancellation_owners().unwrap().len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn reconciliation_stays_fail_closed_for_unprovable_owner_in_finish_window() {
        let directory = tempfile::tempdir().unwrap();
        let store = open_store(directory.path()).unwrap();
        let owner = register_test_owner(&store, "task-unproven", None);
        store
            .update_session_state(&owner.task_id, &owner.session_id, SessionState::Exited)
            .unwrap();
        let error = reconcile_before_scheduling(&store).unwrap_err();
        assert!(
            error.contains("Running without an unfinished session"),
            "{error}"
        );
    }

    /// A real repository (canonical path) plus a data directory and store; the
    /// task's project is that repository so the gate can open it.
    fn gate_fixture(task_id: &str) -> (tempfile::TempDir, PathBuf, PathBuf, SqliteStore) {
        let (root, repo, store) = gate_repo();
        start_test_run_in(&store, task_id, repo.display().to_string());
        let worktree = root.path().join("data").join("worktrees").join(task_id);
        (root, repo, worktree, store)
    }

    fn gate_repo() -> (tempfile::TempDir, PathBuf, SqliteStore) {
        let root = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(root.path()).unwrap();
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        git_in(&repo, &["init", "-q", "-b", "main"]);
        git_in(&repo, &["config", "user.email", "t@example.invalid"]);
        git_in(&repo, &["config", "user.name", "t"]);
        git_in(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
        // Same form production registers as `project_id` and records as the
        // delivery destination: Git's `--show-toplevel`, not `fs::canonicalize`
        // (`\\?\C:\..` on Windows). Identical on macOS.
        let repo = Repository::new(&repo).unwrap().path().to_path_buf();
        let store = open_store(&base.join("data")).unwrap();
        (root, repo, store)
    }

    fn record_test_workspace(store: &SqliteStore, task_id: &str, repo: &Path, path: &Path) {
        store
            .add_workspace(&Workspace {
                id: format!("{task_id}-workspace-1"),
                task_id: task_id.into(),
                path: path.display().to_string(),
                branch: format!("task/{task_id}"),
                base_commit: git_text(repo, &["rev-parse", "HEAD"]).unwrap(),
            })
            .unwrap();
    }

    /// Settles the fixture task as Failed with an exited session.
    fn settle_failed(store: &SqliteStore, task_id: &str) {
        let task = store.get(task_id).unwrap();
        store
            .update_session_state(task_id, &task.sessions[0].id, SessionState::Exited)
            .unwrap();
        store
            .update_attempt_state(task_id, &task.attempts[0].id, AttemptState::Failed)
            .unwrap();
        store
            .transition(task_id, TaskEvent::ExecutionFailed)
            .unwrap();
    }

    /// Persists one local-review delivery step for `task_id`.
    fn record_test_delivery(
        store: &SqliteStore,
        task_id: &str,
        repo: &Path,
        step_id: &str,
        candidate: &str,
        review_ref: &str,
    ) {
        use orchestrator_core::approval::{ApprovalAction, ApprovalActionKind, ParameterDigest};
        use orchestrator_core::authorization::ActionKind;
        use orchestrator_store::{DeliveryOutcome, DeliveryStepRecord};

        let action = ApprovalAction::new(
            ApprovalActionKind::Delivery(ActionKind::LocalReviewMerge),
            ParameterDigest::from_sha256([7; 32]),
            candidate,
            task_id,
        )
        .unwrap();
        // Production records `Repository::path()` (Git's `--show-toplevel`),
        // never a `fs::canonicalize` result, which is `\\?\C:\..` on Windows
        // and would not match the destination `classify_step` compares against.
        let destination = Repository::new(repo).unwrap().path().display().to_string();
        store
            .record_delivery_step(&DeliveryStepRecord {
                id: step_id,
                approval_id: "approval-1",
                verification_run_id: "run-1",
                action: &action,
                destination: &destination,
                target_ref: Some(review_ref),
                outcome: DeliveryOutcome::ReviewRefCreated,
                diff_summary: None,
            })
            .unwrap();
    }

    #[test]
    fn delivery_blocks_only_partial_ambiguous_or_confirmed_on_unsucceeded_task() {
        use DeliveryEvidence::*;
        for evidence in [Partial, Ambiguous] {
            assert!(delivery_blocks(TaskState::Failed, evidence));
            assert!(delivery_blocks(TaskState::Succeeded, evidence));
        }
        assert!(delivery_blocks(TaskState::Failed, Confirmed));
        assert!(delivery_blocks(TaskState::Running, Confirmed));
        assert!(!delivery_blocks(TaskState::Succeeded, Confirmed));
        for state in [
            TaskState::Failed,
            TaskState::Cancelled,
            TaskState::Succeeded,
        ] {
            assert!(!delivery_blocks(state, None));
            assert!(!delivery_blocks(state, NotApplied));
        }
    }

    #[test]
    fn settled_failed_task_with_unlanded_delivery_and_absent_workspace_does_not_block() {
        let (_root, repo, worktree, store) = gate_fixture("task-settled-ok");
        record_test_workspace(&store, "task-settled-ok", &repo, &worktree);
        settle_failed(&store, "task-settled-ok");
        // Local-review step whose review ref was never created: NotApplied.
        let head = git_text(&repo, &["rev-parse", "HEAD"]).unwrap();
        record_test_delivery(
            &store,
            "task-settled-ok",
            &repo,
            "step-1",
            &head,
            "refs/heads/orchestrator/review/task-settled-ok",
        );
        // The retained worktree is absent: a settled record never gates on it.
        reconcile_before_scheduling(&store).expect("settled record must not block");
        assert_eq!(
            store.get("task-settled-ok").unwrap().state,
            TaskState::Failed
        );
    }

    #[test]
    fn confirmed_delivery_on_a_failed_task_blocks_and_preserves_everything() {
        let (_root, repo, worktree, store) = gate_fixture("task-confirmed-failed");
        record_test_workspace(&store, "task-confirmed-failed", &repo, &worktree);
        settle_failed(&store, "task-confirmed-failed");
        let head = git_text(&repo, &["rev-parse", "HEAD"]).unwrap();
        let review_ref = "refs/heads/orchestrator/review/task-confirmed-failed";
        git_in(&repo, &["update-ref", review_ref, &head]);
        record_test_delivery(
            &store,
            "task-confirmed-failed",
            &repo,
            "step-1",
            &head,
            review_ref,
        );
        let error = reconcile_before_scheduling(&store).unwrap_err();
        assert!(
            error.contains("Confirmed delivery evidence while Failed"),
            "{error}"
        );
        assert_eq!(git_text(&repo, &["rev-parse", review_ref]).unwrap(), head);
        assert_eq!(store.all_delivery_steps().unwrap().len(), 1);
    }

    #[test]
    fn ambiguous_delivery_blocks_scheduling_and_is_not_rewritten() {
        let (_root, repo, worktree, store) = gate_fixture("task-ambiguous");
        record_test_workspace(&store, "task-ambiguous", &repo, &worktree);
        settle_failed(&store, "task-ambiguous");
        let candidate = git_text(&repo, &["rev-parse", "HEAD"]).unwrap();
        git_in(&repo, &["commit", "-q", "--allow-empty", "-m", "other"]);
        let other = git_text(&repo, &["rev-parse", "HEAD"]).unwrap();
        let review_ref = "refs/heads/orchestrator/review/task-ambiguous";
        // The ref exists but points at an object the journal does not explain.
        git_in(&repo, &["update-ref", review_ref, &other]);
        record_test_delivery(
            &store,
            "task-ambiguous",
            &repo,
            "step-1",
            &candidate,
            review_ref,
        );
        let error = reconcile_before_scheduling(&store).unwrap_err();
        assert!(error.contains("Ambiguous delivery evidence"), "{error}");
        assert_eq!(git_text(&repo, &["rev-parse", review_ref]).unwrap(), other);
    }

    /// Identity of a session process that has exited and been reaped.
    #[cfg(unix)]
    fn dead_session_identity() -> ProcessIdentity {
        use std::os::unix::process::CommandExt;
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("0.05")
            .process_group(0)
            .spawn()
            .unwrap();
        let identity = capture_process_identity(child.id()).expect("child identity");
        child.wait().unwrap();
        identity
    }

    #[cfg(unix)]
    #[test]
    fn interrupted_task_with_foreign_workspace_stays_refused_and_unmodified() {
        let (_root, repo, worktree, store) = gate_fixture("task-foreign");
        let session = store.get("task-foreign").unwrap().sessions[0].id.clone();
        store
            .record_session_identity("task-foreign", &session, &dead_session_identity())
            .unwrap();
        // A plain directory the user owns, not a registered worktree.
        fs::create_dir_all(&worktree).unwrap();
        fs::write(worktree.join("user.txt"), "keep me\n").unwrap();
        record_test_workspace(&store, "task-foreign", &repo, &worktree);
        for _ in 0..2 {
            let error = reconcile_before_scheduling(&store).unwrap_err();
            assert!(error.contains("Foreign workspace evidence"), "{error}");
            let task = store.get("task-foreign").unwrap();
            assert_eq!(task.state, TaskState::Running);
            assert_eq!(task.attempts[0].state, AttemptState::Running);
            assert_eq!(task.sessions[0].state, SessionState::Running);
            assert_eq!(fs::read(worktree.join("user.txt")).unwrap(), b"keep me\n");
        }
    }

    #[cfg(unix)]
    #[test]
    fn interrupted_task_without_a_recorded_workspace_is_unknown_and_refused() {
        let (_root, _repo, _worktree, store) = gate_fixture("task-no-workspace");
        let session = store.get("task-no-workspace").unwrap().sessions[0]
            .id
            .clone();
        store
            .record_session_identity("task-no-workspace", &session, &dead_session_identity())
            .unwrap();
        let error = reconcile_before_scheduling(&store).unwrap_err();
        assert!(error.contains("Unknown workspace evidence"), "{error}");
        let task = store.get("task-no-workspace").unwrap();
        assert_eq!(task.state, TaskState::Running);
        assert_eq!(task.sessions[0].state, SessionState::Running);
    }

    #[cfg(unix)]
    #[test]
    fn interrupted_task_with_dirty_registered_worktree_keeps_it_and_does_not_block() {
        let (_root, repo, worktree, store) = gate_fixture("task-dirty");
        let session = store.get("task-dirty").unwrap().sessions[0].id.clone();
        store
            .record_session_identity("task-dirty", &session, &dead_session_identity())
            .unwrap();
        git_in(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "task/task-dirty",
                worktree.to_str().unwrap(),
                "HEAD",
            ],
        );
        record_test_workspace(&store, "task-dirty", &repo, &worktree);
        fs::write(worktree.join("unsaved.txt"), "uncommitted\n").unwrap();
        // Gate policy: a known-owned dirty worktree of a proven-dead task keeps
        // its files and takes the existing failure resolution (never Succeeded).
        reconcile_before_scheduling(&store).expect("dirty known workspace must not block");
        let task = store.get("task-dirty").unwrap();
        assert_eq!(task.state, TaskState::Failed);
        assert_eq!(task.sessions[0].state, SessionState::Exited);
        assert_eq!(
            fs::read(worktree.join("unsaved.txt")).unwrap(),
            b"uncommitted\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn interrupted_task_with_confirmed_delivery_is_refused_before_any_state_write() {
        let (_root, repo, worktree, store) = gate_fixture("task-interrupted-confirmed");
        let session = store.get("task-interrupted-confirmed").unwrap().sessions[0]
            .id
            .clone();
        store
            .record_session_identity(
                "task-interrupted-confirmed",
                &session,
                &dead_session_identity(),
            )
            .unwrap();
        record_test_workspace(&store, "task-interrupted-confirmed", &repo, &worktree);
        let head = git_text(&repo, &["rev-parse", "HEAD"]).unwrap();
        let review_ref = "refs/heads/orchestrator/review/task-interrupted-confirmed";
        git_in(&repo, &["update-ref", review_ref, &head]);
        record_test_delivery(
            &store,
            "task-interrupted-confirmed",
            &repo,
            "step-1",
            &head,
            review_ref,
        );
        let error = reconcile_before_scheduling(&store).unwrap_err();
        assert!(
            error.contains("Confirmed delivery evidence while Running"),
            "{error}"
        );
        let task = store.get("task-interrupted-confirmed").unwrap();
        assert_eq!(task.state, TaskState::Running);
        assert_eq!(task.sessions[0].state, SessionState::Running);
    }

    /// Registers the repository and a Queued (admitted) task with no attempt yet.
    fn admitted_task(store: &SqliteStore, repo: &Path, task_id: &str) {
        let project_id = repo.display().to_string();
        // The second task of a test reuses the already registered project.
        let _ = store.register_project(&Project {
            id: project_id.clone(),
            root_path: project_id.clone(),
            settings: serde_json::json!({}),
        });
        store
            .create(&TaskContract {
                id: task_id.into(),
                project_id,
                parent_id: None,
                objective: "unowned attempt".into(),
                kind: TaskKind::Implement,
                agent: "codex".into(),
                dependencies: Vec::new(),
                verification: VerificationPolicy::None,
                delivery: DeliveryPolicy::LocalReview,
                autonomy: AutonomyPolicy::WorkspaceWrites,
                max_retries: 0,
                max_repairs: 0,
            })
            .unwrap();
        store.transition(task_id, TaskEvent::Admit).unwrap();
    }

    #[test]
    fn pending_attempt_without_a_session_blocks_but_a_planned_task_does_not() {
        let (_root, repo, store) = gate_repo();
        admitted_task(&store, &repo, "task-planned");
        reconcile_before_scheduling(&store).expect("a Queued task with no attempt is planned");

        admitted_task(&store, &repo, "task-unowned");
        store
            .add_attempt(
                "task-unowned",
                &Attempt {
                    id: "task-unowned-attempt-1".into(),
                    task_id: "task-unowned".into(),
                    attempt_number: 1,
                    state: AttemptState::Pending,
                },
            )
            .unwrap();
        let error = reconcile_before_scheduling(&store).unwrap_err();
        assert!(
            error.contains("task-unowned-attempt-1") && error.contains("no unfinished session"),
            "{error}"
        );
        assert!(!error.contains("task-planned"), "{error}");
        let task = store.get("task-unowned").unwrap();
        assert_eq!(task.state, TaskState::Queued);
        assert_eq!(task.attempts[0].state, AttemptState::Pending);
    }

    #[test]
    fn settled_candidate_ready_attempt_does_not_block_scheduling() {
        let (_root, repo, _worktree, store) = gate_fixture("task-candidate");
        let task = store.get("task-candidate").unwrap();
        store
            .update_session_state("task-candidate", &task.sessions[0].id, SessionState::Exited)
            .unwrap();
        store
            .update_attempt_state(
                "task-candidate",
                &task.attempts[0].id,
                AttemptState::CandidateReady,
            )
            .unwrap();
        store
            .transition("task-candidate", TaskEvent::CandidateReady)
            .unwrap();
        let _ = repo;
        reconcile_before_scheduling(&store).expect("a preserved candidate is settled");
    }

    #[test]
    fn unresolved_legacy_windows_owner_has_audited_manual_recovery_path() {
        let directory = tempfile::tempdir().unwrap();
        let data_dir = directory.path();
        let store = open_store(data_dir).unwrap();
        let legacy_identity = ProcessIdentity {
            pid: std::process::id(),
            process_group: None,
            start_time_micros: Some(1_700_000_000_123_456),
            platform: ProcessPlatform::Windows,
        };
        let owner = register_test_owner(&store, "task-legacy-windows-owner", Some(legacy_identity));
        store.request_cancellation_for_task(&owner.task_id).unwrap();

        let blockers = reconcile_cancellation_owners(&store).unwrap();
        assert_eq!(blockers.len(), 1);
        assert!(blockers[0].contains(owner.generation()));
        assert!(blockers[0].contains("recover-owner"));
        assert_eq!(store.open_cancellation_owners().unwrap().len(), 1);
        assert_eq!(
            store.cancellation_request(&owner).unwrap(),
            Some(CancellationStatus::Pending)
        );

        store
            .update_session_state(&owner.task_id, &owner.session_id, SessionState::Exited)
            .unwrap();
        store
            .update_attempt_state(&owner.task_id, &owner.attempt_id, AttemptState::Failed)
            .unwrap();
        store
            .transition(&owner.task_id, TaskEvent::ExecutionFailed)
            .unwrap();

        assert!(recover_cancellation_owner(
            data_dir,
            &owner.task_id,
            &owner.attempt_id,
            &owner.session_id,
            owner.generation(),
            false,
            "operator verified the process tree exited",
        )
        .is_err());
        assert_eq!(
            store.cancellation_request(&owner).unwrap(),
            Some(CancellationStatus::Pending)
        );

        recover_cancellation_owner(
            data_dir,
            &owner.task_id,
            &owner.attempt_id,
            &owner.session_id,
            owner.generation(),
            true,
            "operator verified the owner and managed process tree exited",
        )
        .unwrap();
        let record = store.get(&owner.task_id).unwrap();
        assert_eq!(record.state, TaskState::Failed);
        assert_eq!(record.attempts[0].state, AttemptState::Failed);
        assert_eq!(record.sessions[0].state, SessionState::Exited);
        assert!(!record
            .events
            .contains(&orchestrator_core::task::AuditEvent::TaskCancelled));
        assert_eq!(
            store.cancellation_request(&owner).unwrap(),
            Some(CancellationStatus::Failed)
        );
        assert!(store.open_cancellation_owners().unwrap().is_empty());
        assert!(store
            .stream_events_after(0, 100)
            .unwrap()
            .iter()
            .any(|event| event.kind == "cancellation.owner_manually_released"));
    }
}
