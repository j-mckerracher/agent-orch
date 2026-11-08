//! Single-task orchestration and local Git operations. The store owns task
//! lifecycle records; this module owns CLI-only worktree metadata, logs, and
//! cancellation requests. The primary checkout is validated as read-only and
//! all agent writes run in a separately registered worktree. Updated: 2026-11-07T22:31:40-07:00.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command as ProcessCommand, Output, Stdio};
use std::thread;
use std::time::Duration;

use orchestrator_agents::{
    AgentAdapter, AgentEvent, AgentExit, AgentHandle, AgentLaunchRequest, CodexAdapter,
};
use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::task::{Attempt, AttemptState, Session, SessionState, TaskEvent, TaskState};
use orchestrator_git::{create_task_worktree, Repository};
use orchestrator_runtime::is_process_running;
use orchestrator_store::{FileStore, Store};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[cfg(windows)]
const NULL_DEVICE: &str = "NUL";
#[cfg(not(windows))]
const NULL_DEVICE: &str = "/dev/null";

/// CLI-owned metadata complements the store's lifecycle record with canonical
/// repository/worktree paths, logs, and process ownership. The persisted PID
/// is diagnostic only; cancellation is delivered through a request file to
/// the live `AgentHandle`, which alone signals its owned process group.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskMetadata {
    repository: PathBuf,
    worktree: PathBuf,
    branch: String,
    base_ref: String,
    base_sha: String,
    runner_pid: u32,
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
    cancel_request: PathBuf,
}

#[derive(Debug, Clone, Copy)]
struct LifecycleIds<'a> {
    task: &'a str,
    attempt: &'a str,
    session: &'a str,
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
    fs::create_dir_all(&data_target.path).map_err(|error| {
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

    let store = FileStore::new(&data_target.path).map_err(|error| {
        format!(
            "cannot open task store in {}: {error}",
            data_target.path.display()
        )
    })?;
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
        runner_pid: std::process::id(),
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

    if task_paths.cancel_request.exists() {
        finish_cancelled(
            &store,
            &mut metadata,
            &task_paths,
            &ids,
            SessionState::Aborted,
            None,
        )?;
        println!(
            "task {task_id} cancelled before agent launch; worktree preserved at {}",
            metadata.worktree.display()
        );
        return Ok(130);
    }

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
    store
        .update_session_state(&task_id, &session_id, SessionState::Running)
        .map_err(|error| format!("cannot mark session {session_id} running: {error}"))?;

    println!("task: {task_id}");
    println!("worktree: {}", metadata.worktree.display());
    println!("branch: {}", metadata.branch);
    println!("Codex JSONL: {}", metadata.stdout_log.display());

    let monitored = monitor_agent(handle, &task_paths.cancel_request).await;
    let exit = match monitored {
        Ok(exit) => exit,
        Err(reason) => {
            metadata.agent_outcome = Some("supervision-failed".to_string());
            metadata.failure = Some(reason.clone());
            metadata.exit_code = None;
            write_metadata(&task_paths.metadata, &metadata)?;
            store
                .update_session_state(&task_id, &session_id, SessionState::Aborted)
                .map_err(|error| format!("cannot mark session {session_id} aborted: {error}"))?;
            store
                .update_attempt_state(&task_id, &attempt_id, AttemptState::Failed)
                .map_err(|error| format!("cannot fail attempt {attempt_id}: {error}"))?;
            store
                .transition(&task_id, TaskEvent::ExecutionFailed)
                .map_err(|error| format!("cannot fail task {task_id}: {error}"))?;
            return Err(format!(
                "Codex supervision failed for task {task_id}: {reason}"
            ));
        }
    };

    match exit {
        AgentExit::Cancelled { code, .. } => {
            finish_cancelled(
                &store,
                &mut metadata,
                &task_paths,
                &ids,
                SessionState::Exited,
                code,
            )?;
            println!(
                "task {task_id} cancelled; process {agent_pid} reaped; worktree preserved at {}",
                metadata.worktree.display()
            );
            Ok(130)
        }
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
                .map_err(|error| format!("cannot record candidate for task {task_id}: {error}"))?;
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
            Err(format!("Codex returned exit code {code:?} for task {task_id}; inspect worktree {} and logs {} / {}", metadata.worktree.display(), metadata.stdout_log.display(), metadata.stderr_log.display()))
        }
        AgentExit::Failed { code, reason } => {
            let detail =
                reason.unwrap_or_else(|| "Codex exited without a completed turn".to_string());
            fail_after_agent_exit(&store, &mut metadata, &task_paths, &ids, code, &detail)?;
            Err(format!(
                "Codex failed for task {task_id}: {detail}; inspect worktree {} and logs {} / {}",
                metadata.worktree.display(),
                metadata.stdout_log.display(),
                metadata.stderr_log.display()
            ))
        }
    }
}

fn finish_cancelled(
    store: &FileStore,
    metadata: &mut TaskMetadata,
    task_paths: &Paths,
    ids: &LifecycleIds<'_>,
    session_state: SessionState,
    code: Option<i32>,
) -> Result<(), String> {
    metadata.agent_outcome = Some("cancelled".to_string());
    metadata.exit_code = code;
    write_metadata(&task_paths.metadata, metadata)?;
    store
        .update_session_state(ids.task, ids.session, session_state)
        .map_err(|error| {
            format!(
                "cannot mark session {} {:?} after cancellation: {error}",
                ids.session, session_state
            )
        })?;
    store
        .update_attempt_state(ids.task, ids.attempt, AttemptState::Cancelled)
        .map_err(|error| format!("cannot cancel attempt {}: {error}", ids.attempt))?;
    store
        .transition(ids.task, TaskEvent::CancelRequested)
        .map_err(|error| format!("cannot cancel task {}: {error}", ids.task))?;
    Ok(())
}

fn fail_before_agent_exit(
    store: &FileStore,
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
    store: &FileStore,
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

async fn monitor_agent(
    mut handle: Box<dyn AgentHandle>,
    cancel_request: &Path,
) -> Result<AgentExit, String> {
    let mut poll = tokio::time::interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            event = handle.next_event() => {
                match event.map_err(|error| format!("agent event stream failed: {error}"))? {
                    Some(AgentEvent::SessionExited { .. }) => {
                        return handle.wait().await.map_err(|error| format!("agent wait failed: {error}"));
                    }
                    Some(AgentEvent::SessionStarted { session_id }) => println!("Codex session: {session_id}"),
                    Some(AgentEvent::MessageDelta { text }) => println!("{text}"),
                    Some(AgentEvent::ToolStarted { name, .. }) => println!("tool started: {name}"),
                    Some(AgentEvent::ToolFinished { success, .. }) => println!("tool finished: {success}"),
                    Some(AgentEvent::ApprovalRequested { description, .. }) => println!("approval requested: {description}"),
                    Some(AgentEvent::TurnFinished { outcome }) => println!("turn: {outcome:?}"),
                    Some(AgentEvent::ProtocolError { message }) => eprintln!("Codex protocol error: {message}"),
                    None => return Err("agent event stream ended without a process exit event".to_string()),
                }
            }
            _ = poll.tick() => {
                let requested = cancel_request.try_exists().map_err(|error| {
                    format!("cannot check cancellation request {}: {error}", cancel_request.display())
                })?;
                if requested {
                    handle.cancel().await.map_err(|error| format!("agent cancellation failed: {error}"))?;
                    return handle.wait().await.map_err(|error| format!("agent reap after cancellation failed: {error}"));
                }
            }
        }
    }
}

pub fn list_tasks(data_dir: &Path) -> Result<(), String> {
    if !data_dir.join("tasks").exists() {
        println!("no tasks");
        return Ok(());
    }
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
    let record_path = data_dir.join("tasks").join(format!("{task_id}.json"));
    if !record_path.is_file() {
        return Err(format!(
            "task record {} does not exist",
            record_path.display()
        ));
    }
    let store = open_store(data_dir)?;
    let task = store
        .get(task_id)
        .map_err(|error| format!("cannot inspect task {task_id}: {error}"))?;
    println!("task: {}", task.contract.id);
    println!("status: {:?}", task.state);
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
    if let Some(failure) = metadata.failure {
        println!("failure: {failure}");
    }
    if !metadata.worktree.exists() {
        println!("worktree state: removed or missing");
    }
    Ok(())
}

pub fn cancel_task(data_dir: &Path, task_id: &str) -> Result<(), String> {
    validate_task_id(task_id)?;
    let record_path = data_dir.join("tasks").join(format!("{task_id}.json"));
    if !record_path.is_file() {
        return Err(format!(
            "task record {} does not exist",
            record_path.display()
        ));
    }
    let store = open_store(data_dir)?;
    let task = store
        .get(task_id)
        .map_err(|error| format!("cannot cancel task {task_id}: {error}"))?;
    if task.state != TaskState::Running {
        return Err(format!(
            "task {task_id} is {:?}, not Running; no agent process group will be signalled",
            task.state
        ));
    }
    let metadata = read_metadata(data_dir, task_id)?;
    let repository = Repository::new(&metadata.repository).map_err(|error| {
        format!(
            "cannot validate repository {} for task {task_id}: {error}",
            metadata.repository.display()
        )
    })?;
    let data_target = canonical_target(data_dir)?;
    ensure_outside_primary(&repository, &data_target)?;
    let task_paths = paths(&data_target.path, task_id);
    let Some(agent_pid) = metadata.agent_pid else {
        return Err(format!(
            "task {task_id} has no recorded agent process; inspect {}",
            task_paths.metadata.display()
        ));
    };
    if !is_process_running(agent_pid) {
        return Err(format!("agent process {agent_pid} for task {task_id} is no longer running; preserve worktree {} and inspect task state", metadata.worktree.display()));
    }
    if !is_process_running(metadata.runner_pid) {
        return Err(format!("task runner process {} is no longer active; cannot deliver cancellation request for task {task_id}", metadata.runner_pid));
    }
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&task_paths.cancel_request)
    {
        Ok(mut file) => {
            writeln!(file, "cancel task {task_id}").map_err(|error| {
                format!(
                    "cannot write cancellation request {}: {error}",
                    task_paths.cancel_request.display()
                )
            })?;
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Err(format!("cancellation already requested for task {task_id}; runner will stop its owned process group"));
        }
        Err(error) => {
            return Err(format!(
                "cannot create cancellation request {}: {error}",
                task_paths.cancel_request.display()
            ))
        }
    }
    println!("cancellation requested for task {task_id}; active runner will stop its owned agent process group");
    Ok(())
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
        cancel_request: task_dir.join("cancel.request"),
        task_dir,
    }
}

fn open_store(data_dir: &Path) -> Result<FileStore, String> {
    FileStore::new(data_dir)
        .map_err(|error| format!("cannot open task store in {}: {error}", data_dir.display()))
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
        "--sandbox".to_string(),
        "workspace-write".to_string(),
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
}
