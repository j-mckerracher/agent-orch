//! Independent verification of task-owned Git candidates.
//!
//! The verifier resolves candidates from Git and ignores worker reports.
//! It writes no verifier artifacts into the candidate worktree, but trusted
//! commands run there, inherit the coordinator environment, and may have
//! filesystem side effects. This is not a sandbox.
//! Command trust is default-deny. Its digest covers exact argv, not executable
//! bytes or inherited environment; protected globs must cover referenced code.
//!
//! Updated: 2026-11-07T23:52:00-07:00.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::repository::Repository;
use crate::worktree::sanitize_task_id;
use crate::Error;

const MAX_COMMAND_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const MAX_CAPTURED_OUTPUT: usize = 1024 * 1024;

/// Recorded task workspace identity used to reject foreign worktrees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRecord {
    /// Owning task identifier.
    pub task_id: String,
    /// Recorded worktree path.
    pub path: String,
    /// Recorded task branch.
    pub branch: String,
    /// Commit from which this worktree was created.
    pub base_commit: String,
}

/// Exact executable and argument vector for one validation command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    /// Executable name or path.
    pub program: String,
    /// Arguments passed without shell parsing.
    pub args: Vec<String>,
}

impl CommandSpec {
    /// Returns repository-format Git object digest of length-prefixed argv.
    ///
    /// The digest identifies the command specification, not executable bytes
    /// or files referenced by arguments. Callers must protect those files.
    pub fn digest(&self, repository: &Repository) -> Result<String, Error> {
        let mut child = Command::new("git")
            .current_dir(repository.path())
            .args(["hash-object", "--stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let Some(mut stdin) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Io(io::Error::other(
                "git hash-object stdin was not piped",
            )));
        };
        stdin.write_all(b"orchestrator-validation-command-v1\0")?;
        stdin.write_all(&(self.args.len() as u64).to_be_bytes())?;
        for part in std::iter::once(&self.program).chain(self.args.iter()) {
            stdin.write_all(&(part.len() as u64).to_be_bytes())?;
            stdin.write_all(part.as_bytes())?;
        }
        drop(stdin);
        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Err(Error::Git {
                step: "hash validation command",
                status: output.status,
                stderr: crate::error::first_line(&output.stderr),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

/// A configured validation command and its maximum runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationCommand {
    /// Program and arguments run directly without a shell.
    pub command: CommandSpec,
    /// Per-command timeout. Values over one hour are refused.
    pub timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrustedCommand {
    command: CommandSpec,
    digest: String,
}

/// Explicit allowlist for executable validation commands.
///
/// The default allowlist is empty. A command runs only when both its exact
/// argv and the repository-format digest match an entry added by the caller.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionTrustPolicy {
    trusted_commands: Vec<TrustedCommand>,
}

impl ExecutionTrustPolicy {
    /// Explicitly trusts one exact command specification and its digest.
    pub fn trust_command(
        &mut self,
        repository: &Repository,
        command: CommandSpec,
    ) -> Result<(), Error> {
        let digest = command.digest(repository)?;
        self.trusted_commands
            .push(TrustedCommand { command, digest });
        Ok(())
    }
}

/// Caller-owned policy for protected paths and validation commands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerificationPolicy {
    /// Repository-relative globs: `*` excludes `/`, `**` includes it, and `?`
    /// matches one non-`/` Unicode character. Character classes are literal.
    pub protected_path_globs: Vec<String>,
    /// Commands to run after ownership, commit, and protected-path checks pass.
    pub validation_commands: Vec<ValidationCommand>,
    /// Caller-authorized commands. Defaults to deny all commands.
    pub execution_trust: ExecutionTrustPolicy,
}

/// Independent verification inputs from the task and attempt records.
#[derive(Debug)]
pub struct VerificationRequest<'a> {
    /// Caller-assigned identifier for this append-only run.
    pub run_id: &'a str,
    /// Task that owns `workspace`.
    pub task_id: &'a str,
    /// Worktree path supplied by the coordinator.
    pub worktree_path: &'a Path,
    /// Workspace identity read from the task's durable record.
    pub workspace: &'a WorkspaceRecord,
    /// Candidate commit recorded by the attempt.
    pub recorded_commit: &'a str,
    /// Policy supplied by the caller, not loaded from the worktree.
    pub policy: &'a VerificationPolicy,
}

/// Overall outcome of one independent verification run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationStatus {
    /// Every applicable check passed.
    Passed,
    /// A protected-path or validation check failed.
    Failed,
    /// The worktree or candidate does not belong to this recorded attempt.
    Rejected,
}

impl VerificationStatus {
    /// Stable database status value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Rejected => "rejected",
        }
    }
}

/// Result of one named verifier check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    /// The check passed.
    Passed,
    /// The check failed and includes evidence.
    Failed(String),
    /// The candidate was rejected and includes the reason.
    Rejected(String),
    /// A prerequisite failed, so the check was not run.
    Skipped(String),
}

/// Trust decision made for one configured command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustDecision {
    /// Exact command and digest matched caller authorization.
    Trusted,
    /// No exact command-and-digest authorization matched.
    Denied,
    /// An earlier check prevented trust evaluation.
    NotEvaluated,
}

/// Execution outcome of one validation command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandStatus {
    /// Process exited with code zero.
    Passed,
    /// Process exited nonzero or output capture failed.
    Failed,
    /// Process was killed after exceeding its timeout.
    TimedOut,
    /// Command was not authorized by the caller.
    Untrusted,
    /// Configured timeout exceeded the one-hour verifier limit.
    InvalidTimeout,
    /// An earlier verifier check prevented execution.
    Skipped,
    /// Operating system could not start the process.
    SpawnFailed,
}

/// Captured trust decision and output for one validation command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    /// Exact executable and arguments considered by the verifier.
    pub command: CommandSpec,
    /// Git object digest of the exact command specification, when available.
    pub digest: Option<String>,
    /// Caller-policy decision.
    pub trust: TrustDecision,
    /// Process outcome.
    pub status: CommandStatus,
    /// Exit code, or `None` when process was killed or could not start.
    pub exit_code: Option<i32>,
    /// Captured stdout, limited to one MiB.
    pub stdout: String,
    /// Captured stderr, limited to one MiB.
    pub stderr: String,
    /// Whether either output stream exceeded the capture limit.
    pub output_truncated: bool,
    /// Process wall time in milliseconds.
    pub duration_ms: u64,
    /// Trust, timeout, spawn, wait, or output-capture detail.
    pub detail: Option<String>,
}

/// Complete retained evidence produced by one verification run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationResult {
    /// Caller-assigned run identifier.
    pub run_id: String,
    /// Task identifier checked.
    pub task_id: String,
    /// Commit resolved from Git, absent when ownership or Git resolution failed.
    pub candidate_commit: Option<String>,
    /// Overall result derived from all check outcomes.
    pub status: VerificationStatus,
    /// Worktree path, branch, task, and repository ownership result.
    pub worktree: CheckOutcome,
    /// Resolved candidate versus attempt-recorded commit result.
    pub candidate: CheckOutcome,
    /// Protected paths changed between recorded base and candidate.
    pub protected_paths: CheckOutcome,
    /// Validation command summary.
    pub validation: CheckOutcome,
    /// Per-command trust, output, exit code, and timeout result.
    pub commands: Vec<CommandResult>,
    /// Final check that branch, commit, and worktree remained unchanged.
    pub worktree_unchanged: CheckOutcome,
    /// Run start time as Unix milliseconds.
    pub started_at_ms: i64,
    /// Run completion time as Unix milliseconds.
    pub finished_at_ms: i64,
}

/// Verifies Git ownership, the recorded candidate, protected paths, and trusted commands.
///
/// Git failures become retained check outcomes rather than worker claims.
/// Commands run sequentially in the candidate worktree only after earlier
/// checks pass. The verifier captures output in memory and writes no report.
pub fn verify(repository: &Repository, request: VerificationRequest<'_>) -> VerificationResult {
    let started_at_ms = unix_millis();
    let worktree = match check_worktree(repository, &request) {
        Ok(()) => CheckOutcome::Passed,
        Err(detail) => CheckOutcome::Rejected(detail),
    };
    let owned = matches!(worktree, CheckOutcome::Passed);
    let (candidate_commit, candidate) = if owned {
        check_candidate(repository, &request)
    } else {
        (
            None,
            CheckOutcome::Skipped("worktree ownership was rejected".into()),
        )
    };
    let candidate_matches = matches!(candidate, CheckOutcome::Passed);
    let protected_paths = if candidate_matches {
        match changed_protected_paths(
            repository,
            &request.workspace.base_commit,
            candidate_commit.as_deref().unwrap(),
            &request.policy.protected_path_globs,
        ) {
            Ok(paths) if paths.is_empty() => CheckOutcome::Passed,
            Ok(paths) => {
                CheckOutcome::Failed(format!("protected paths changed: {}", paths.join(", ")))
            }
            Err(detail) => CheckOutcome::Failed(detail),
        }
    } else {
        CheckOutcome::Skipped("candidate commit was rejected".into())
    };
    let protected_paths_pass = matches!(protected_paths, CheckOutcome::Passed);
    let prerequisites_pass = owned && candidate_matches && protected_paths_pass;
    let mut commands = Vec::with_capacity(request.policy.validation_commands.len());
    if prerequisites_pass {
        for configured in &request.policy.validation_commands {
            commands.push(evaluate_command(
                repository,
                request.worktree_path,
                configured,
                &request.policy.execution_trust,
            ));
        }
    } else {
        let reason = "ownership, candidate, or protected-path check failed";
        commands.extend(
            request
                .policy
                .validation_commands
                .iter()
                .map(|configured| skipped_command(configured, reason)),
        );
    }
    let validation = if !prerequisites_pass {
        CheckOutcome::Skipped("validation prerequisites failed".into())
    } else if commands
        .iter()
        .all(|command| command.status == CommandStatus::Passed)
    {
        CheckOutcome::Passed
    } else {
        CheckOutcome::Failed("one or more validation commands failed or were not trusted".into())
    };
    let worktree_unchanged = if owned && candidate_matches {
        check_candidate_unchanged(
            repository,
            request.worktree_path,
            &request.workspace.branch,
            candidate_commit.as_deref().unwrap(),
        )
    } else {
        CheckOutcome::Skipped("no accepted candidate was available".into())
    };
    let status = overall_status(
        &worktree,
        &candidate,
        &protected_paths,
        &validation,
        &worktree_unchanged,
    );
    let finished_at_ms = unix_millis().max(started_at_ms);
    VerificationResult {
        run_id: request.run_id.to_owned(),
        task_id: request.task_id.to_owned(),
        candidate_commit,
        status,
        worktree,
        candidate,
        protected_paths,
        validation,
        commands,
        worktree_unchanged,
        started_at_ms,
        finished_at_ms,
    }
}

fn check_worktree(
    repository: &Repository,
    request: &VerificationRequest<'_>,
) -> Result<(), String> {
    let workspace = request.workspace;
    if workspace.task_id != request.task_id {
        return Err(format!(
            "recorded workspace belongs to task {}, not {}",
            workspace.task_id, request.task_id
        ));
    }
    if workspace.path.contains(['\n', '\r']) {
        return Err("recorded worktree path contains a line break".into());
    }
    let recorded_path = fs::canonicalize(&workspace.path)
        .map_err(|error| format!("recorded worktree path is unavailable: {error}"))?;
    let supplied_path = fs::canonicalize(request.worktree_path)
        .map_err(|error| format!("supplied worktree path is unavailable: {error}"))?;
    if recorded_path != supplied_path {
        return Err(format!(
            "supplied worktree {} differs from recorded workspace {}",
            supplied_path.display(),
            recorded_path.display()
        ));
    }
    let slug = sanitize_task_id(request.task_id).map_err(|error| error.to_string())?;
    let suffix = task_branch_suffix(request.task_id, &workspace.branch)?;
    let expected_path_name = format!("{slug}{suffix}");
    let path_name = recorded_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "worktree path has no UTF-8 final component".to_owned())?;
    if path_name != expected_path_name {
        return Err(format!(
            "worktree path component {path_name:?} does not match task {expected_path_name:?}"
        ));
    }
    if !registered_worktree(repository, &recorded_path, &workspace.branch)
        .map_err(|error| error.to_string())?
    {
        return Err("path and branch are not registered as this repository's worktree".into());
    }
    let current_branch = crate::git::git(
        "read verification branch",
        &recorded_path,
        ["branch", "--show-current"],
    )
    .map_err(|error| error.to_string())?;
    if current_branch != workspace.branch {
        return Err(format!(
            "worktree branch {current_branch:?} differs from recorded branch {:?}",
            workspace.branch
        ));
    }
    let worktree_common_dir = crate::git::git(
        "read worktree common directory",
        &recorded_path,
        ["rev-parse", "--git-common-dir"],
    )
    .map_err(|error| error.to_string())?;
    let worktree_common_dir = PathBuf::from(worktree_common_dir);
    let worktree_common_dir = if worktree_common_dir.is_absolute() {
        worktree_common_dir
    } else {
        recorded_path.join(worktree_common_dir)
    };
    let worktree_common_dir = fs::canonicalize(&worktree_common_dir)
        .map_err(|error| format!("worktree Git directory is unavailable: {error}"))?;
    let repository_common_dir = fs::canonicalize(repository.git_common_dir())
        .map_err(|error| format!("repository Git directory is unavailable: {error}"))?;
    if worktree_common_dir != repository_common_dir {
        return Err("worktree is attached to a different Git repository".into());
    }
    Ok(())
}

fn task_branch_suffix(task_id: &str, branch: &str) -> Result<String, String> {
    let slug = sanitize_task_id(task_id).map_err(|error| error.to_string())?;
    let prefix = format!("orchestrator/{slug}");
    let suffix = branch
        .strip_prefix(&prefix)
        .ok_or_else(|| format!("branch {branch:?} is not owned by task {task_id:?}"))?;
    if suffix.is_empty() {
        return Ok(String::new());
    }
    let number = suffix
        .strip_prefix('-')
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|number| *number > 0)
        .ok_or_else(|| format!("branch {branch:?} is not a task worktree branch"))?;
    Ok(format!("-{number}"))
}

fn registered_worktree(
    repository: &Repository,
    expected_path: &Path,
    expected_branch: &str,
) -> Result<bool, Error> {
    let output = run_git(
        repository.path(),
        &["worktree", "list", "--porcelain"],
        "list verification worktrees",
    )?;
    let listing = String::from_utf8_lossy(&output.stdout);
    let mut path: Option<PathBuf> = None;
    let mut branch: Option<String> = None;
    for line in listing.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if let (Some(path), Some(branch)) = (path.take(), branch.take()) {
                let canonical_path = fs::canonicalize(&path).unwrap_or(path);
                if canonical_path == expected_path
                    && branch == format!("refs/heads/{expected_branch}")
                {
                    return Ok(true);
                }
            } else {
                path = None;
                branch = None;
            }
        } else if let Some(value) = line.strip_prefix("worktree ") {
            path = Some(PathBuf::from(value));
        } else if let Some(value) = line.strip_prefix("branch ") {
            branch = Some(value.to_owned());
        }
    }
    Ok(false)
}

fn check_candidate(
    repository: &Repository,
    request: &VerificationRequest<'_>,
) -> (Option<String>, CheckOutcome) {
    let candidate = match crate::git::git(
        "resolve verification candidate",
        request.worktree_path,
        ["rev-parse", "--verify", "--quiet", "HEAD^{commit}"],
    ) {
        Ok(candidate) => candidate,
        Err(error) => return (None, CheckOutcome::Rejected(error.to_string())),
    };
    if !is_full_object_id(request.recorded_commit, candidate.len()) {
        return (
            Some(candidate),
            CheckOutcome::Rejected("attempt record must contain a full commit object id".into()),
        );
    }
    let expected = match repository.resolve_base_ref(request.recorded_commit) {
        Ok(expected) => expected,
        Err(error) => return (Some(candidate), CheckOutcome::Rejected(error.to_string())),
    };
    if candidate != expected {
        return (
            Some(candidate.clone()),
            CheckOutcome::Rejected(format!(
                "worktree HEAD {candidate} differs from attempt commit {expected}"
            )),
        );
    }
    match crate::git::git(
        "check verification worktree cleanliness",
        request.worktree_path,
        ["status", "--porcelain=v1", "--untracked-files=all"],
    ) {
        Ok(status) if status.is_empty() => (Some(candidate), CheckOutcome::Passed),
        Ok(status) => (
            Some(candidate),
            CheckOutcome::Rejected(format!(
                "worktree has uncommitted changes: {}",
                status.replace('\n', "; ")
            )),
        ),
        Err(error) => (Some(candidate), CheckOutcome::Rejected(error.to_string())),
    }
}

fn is_full_object_id(value: &str, expected_length: usize) -> bool {
    value.len() == expected_length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn changed_protected_paths(
    repository: &Repository,
    base_commit: &str,
    candidate_commit: &str,
    protected_globs: &[String],
) -> Result<Vec<String>, String> {
    if !is_full_object_id(base_commit, candidate_commit.len()) {
        return Err("workspace base must contain a full commit object id".into());
    }
    let base = repository
        .resolve_base_ref(base_commit)
        .map_err(|error| error.to_string())?;
    let output = run_git(
        repository.path(),
        &[
            "diff",
            "--no-ext-diff",
            "--no-renames",
            "--name-only",
            "-z",
            &base,
            candidate_commit,
            "--",
        ],
        "diff verification candidate",
    )
    .map_err(|error| error.to_string())?;
    let mut protected = Vec::new();
    for raw_path in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = String::from_utf8_lossy(raw_path).into_owned();
        let invalid_utf8 = std::str::from_utf8(raw_path).is_err();
        if (invalid_utf8 && !protected_globs.is_empty())
            || protected_globs
                .iter()
                .any(|pattern| glob_matches(pattern, &path))
        {
            protected.push(path);
        }
    }
    protected.sort();
    protected.dedup();
    Ok(protected)
}

fn glob_matches(pattern: &str, path: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let path: Vec<char> = path.chars().collect();
    let mut previous = vec![false; path.len() + 1];
    previous[0] = true;
    let mut index = 0;
    while index < pattern.len() {
        let mut current = vec![false; path.len() + 1];
        if pattern[index] == '*' {
            let globstar = pattern.get(index + 1) == Some(&'*');
            index += if globstar { 2 } else { 1 };
            current[0] = previous[0];
            for path_index in 1..=path.len() {
                current[path_index] = previous[path_index]
                    || (current[path_index - 1] && (globstar || path[path_index - 1] != '/'));
            }
        } else {
            for path_index in 1..=path.len() {
                current[path_index] = previous[path_index - 1]
                    && (pattern[index] == '?' && path[path_index - 1] != '/'
                        || pattern[index] == path[path_index - 1]);
            }
            index += 1;
        }
        previous = current;
    }
    previous[path.len()]
}

fn evaluate_command(
    repository: &Repository,
    worktree_path: &Path,
    configured: &ValidationCommand,
    trust_policy: &ExecutionTrustPolicy,
) -> CommandResult {
    let digest = match configured.command.digest(repository) {
        Ok(digest) => digest,
        Err(error) => {
            return denied_command(
                configured,
                None,
                format!("could not compute command digest: {error}"),
            )
        }
    };
    let trusted = trust_policy
        .trusted_commands
        .iter()
        .any(|entry| entry.command == configured.command && entry.digest == digest);
    if !trusted {
        return denied_command(
            configured,
            Some(digest),
            "no exact command-and-digest trust entry matched".into(),
        );
    }
    if configured.timeout > MAX_COMMAND_TIMEOUT {
        return CommandResult {
            command: configured.command.clone(),
            digest: Some(digest),
            trust: TrustDecision::Trusted,
            status: CommandStatus::InvalidTimeout,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            output_truncated: false,
            duration_ms: 0,
            detail: Some("timeout exceeds the one-hour verifier limit".into()),
        };
    }
    let mut result = run_command(worktree_path, configured);
    result.digest = Some(digest);
    result.trust = TrustDecision::Trusted;
    result
}

fn denied_command(
    configured: &ValidationCommand,
    digest: Option<String>,
    detail: String,
) -> CommandResult {
    CommandResult {
        command: configured.command.clone(),
        digest,
        trust: TrustDecision::Denied,
        status: CommandStatus::Untrusted,
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
        output_truncated: false,
        duration_ms: 0,
        detail: Some(detail),
    }
}

fn skipped_command(configured: &ValidationCommand, reason: &str) -> CommandResult {
    CommandResult {
        command: configured.command.clone(),
        digest: None,
        trust: TrustDecision::NotEvaluated,
        status: CommandStatus::Skipped,
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
        output_truncated: false,
        duration_ms: 0,
        detail: Some(reason.to_owned()),
    }
}

fn run_command(worktree_path: &Path, configured: &ValidationCommand) -> CommandResult {
    let started = Instant::now();
    let mut command = Command::new(&configured.command.program);
    command
        .args(&configured.command.args)
        .current_dir(worktree_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return CommandResult {
                command: configured.command.clone(),
                digest: None,
                trust: TrustDecision::NotEvaluated,
                status: CommandStatus::SpawnFailed,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                output_truncated: false,
                duration_ms: elapsed_millis(started),
                detail: Some(error.to_string()),
            }
        }
    };
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let stdout_reader = thread::spawn(move || capture_output(stdout));
    let stderr_reader = thread::spawn(move || capture_output(stderr));
    let mut timed_out = false;
    let mut wait_error = None;
    let exit_status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() >= configured.timeout => {
                timed_out = true;
                kill_process(&mut child);
                break child.wait().ok();
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                wait_error = Some(error.to_string());
                kill_process(&mut child);
                break child.wait().ok();
            }
        }
    };
    let (stdout, stdout_truncated, stdout_error) = joined_output(stdout_reader);
    let (stderr, stderr_truncated, stderr_error) = joined_output(stderr_reader);
    let detail = wait_error.or(stdout_error).or(stderr_error);
    let status = if timed_out {
        CommandStatus::TimedOut
    } else if detail.is_some() {
        CommandStatus::Failed
    } else if exit_status.and_then(|status| status.code()) == Some(0) {
        CommandStatus::Passed
    } else {
        CommandStatus::Failed
    };
    CommandResult {
        command: configured.command.clone(),
        digest: None,
        trust: TrustDecision::NotEvaluated,
        status,
        exit_code: exit_status.and_then(|status| status.code()),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        output_truncated: stdout_truncated || stderr_truncated,
        duration_ms: elapsed_millis(started),
        detail: detail.or_else(|| {
            (status == CommandStatus::Failed && exit_status.is_none())
                .then(|| "process exited without a status".into())
        }),
    }
}

fn capture_output(mut stream: impl Read) -> io::Result<(Vec<u8>, bool)> {
    let mut output = Vec::with_capacity(8192);
    let mut buffer = [0; 8192];
    let mut truncated = false;
    loop {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            return Ok((output, truncated));
        }
        let available = MAX_CAPTURED_OUTPUT.saturating_sub(output.len());
        let captured = count.min(available);
        output.extend_from_slice(&buffer[..captured]);
        truncated |= captured < count;
    }
}

fn joined_output(
    reader: thread::JoinHandle<io::Result<(Vec<u8>, bool)>>,
) -> (Vec<u8>, bool, Option<String>) {
    match reader.join() {
        Ok(Ok((output, truncated))) => (output, truncated, None),
        Ok(Err(error)) => (Vec::new(), false, Some(error.to_string())),
        Err(_) => (
            Vec::new(),
            false,
            Some("output reader thread panicked".into()),
        ),
    }
}

fn kill_process(child: &mut Child) {
    #[cfg(unix)]
    {
        let process_group = format!("-{}", child.id());
        let _ = Command::new("kill")
            .args(["-KILL", "--", &process_group])
            .status();
    }
    let _ = child.kill();
}

fn check_candidate_unchanged(
    repository: &Repository,
    worktree_path: &Path,
    expected_branch: &str,
    expected_commit: &str,
) -> CheckOutcome {
    let branch = match crate::git::git(
        "recheck verification branch",
        worktree_path,
        ["branch", "--show-current"],
    ) {
        Ok(branch) => branch,
        Err(error) => return CheckOutcome::Rejected(error.to_string()),
    };
    if branch != expected_branch {
        return CheckOutcome::Rejected(format!(
            "branch moved from {expected_branch:?} to {branch:?} during verification"
        ));
    }
    let commit = match crate::git::git(
        "recheck verification candidate",
        worktree_path,
        ["rev-parse", "--verify", "--quiet", "HEAD^{commit}"],
    ) {
        Ok(commit) => commit,
        Err(error) => return CheckOutcome::Rejected(error.to_string()),
    };
    if commit != expected_commit {
        return CheckOutcome::Rejected(format!(
            "branch moved from candidate {expected_commit} to {commit} during verification"
        ));
    }
    let status = match crate::git::git(
        "recheck verification worktree cleanliness",
        worktree_path,
        ["status", "--porcelain=v1", "--untracked-files=all"],
    ) {
        Ok(status) => status,
        Err(error) => return CheckOutcome::Rejected(error.to_string()),
    };
    if !status.is_empty() {
        return CheckOutcome::Rejected(format!(
            "worktree changed during verification: {}",
            status.replace('\n', "; ")
        ));
    }
    let common_dir = match crate::git::git(
        "recheck worktree common directory",
        worktree_path,
        ["rev-parse", "--git-common-dir"],
    ) {
        Ok(common_dir) => PathBuf::from(common_dir),
        Err(error) => return CheckOutcome::Rejected(error.to_string()),
    };
    let common_dir = if common_dir.is_absolute() {
        common_dir
    } else {
        worktree_path.join(common_dir)
    };
    let common_dir = match fs::canonicalize(common_dir) {
        Ok(common_dir) => common_dir,
        Err(error) => return CheckOutcome::Rejected(error.to_string()),
    };
    let repository_common_dir = match fs::canonicalize(repository.git_common_dir()) {
        Ok(common_dir) => common_dir,
        Err(error) => return CheckOutcome::Rejected(error.to_string()),
    };
    if common_dir != repository_common_dir {
        return CheckOutcome::Rejected("worktree changed Git repositories".into());
    }
    CheckOutcome::Passed
}

fn run_git(directory: &Path, args: &[&str], step: &'static str) -> Result<Output, Error> {
    let output = Command::new("git")
        .current_dir(directory)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(Error::Git {
            step,
            status: output.status,
            stderr: crate::error::first_line(&output.stderr),
        });
    }
    Ok(output)
}

fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn overall_status(
    worktree: &CheckOutcome,
    candidate: &CheckOutcome,
    protected_paths: &CheckOutcome,
    validation: &CheckOutcome,
    worktree_unchanged: &CheckOutcome,
) -> VerificationStatus {
    if [
        worktree,
        candidate,
        protected_paths,
        validation,
        worktree_unchanged,
    ]
    .iter()
    .any(|check| matches!(check, CheckOutcome::Rejected(_)))
    {
        VerificationStatus::Rejected
    } else if [
        worktree,
        candidate,
        protected_paths,
        validation,
        worktree_unchanged,
    ]
    .iter()
    .any(|check| matches!(check, CheckOutcome::Failed(_) | CheckOutcome::Skipped(_)))
    {
        VerificationStatus::Failed
    } else {
        VerificationStatus::Passed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::Repository;
    use crate::worktree::{create_task_worktree, TaskWorktree};
    use std::fs;
    use std::path::Path;
    use std::process::Command;
    use tempfile::TempDir;

    fn git(directory: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(directory)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn setup_worktree(temp: &TempDir, task_id: &str) -> (Repository, TaskWorktree) {
        let repository_path = temp.path().join("repository");
        fs::create_dir(&repository_path).unwrap();
        git(&repository_path, &["init"]);
        git(
            &repository_path,
            &["config", "user.name", "Verification Test"],
        );
        git(
            &repository_path,
            &["config", "user.email", "verification@example.test"],
        );
        fs::write(repository_path.join("README.md"), "base\n").unwrap();
        git(&repository_path, &["add", "README.md"]);
        git(&repository_path, &["commit", "-m", "base"]);
        let repository = Repository::new(&repository_path).unwrap();
        let worktrees_root = temp.path().join("worktrees");
        fs::create_dir(&worktrees_root).unwrap();
        let worktree = create_task_worktree(&repository, task_id, "HEAD", &worktrees_root).unwrap();
        (repository, worktree)
    }

    fn workspace_record(task_id: &str, worktree: &TaskWorktree) -> WorkspaceRecord {
        WorkspaceRecord {
            task_id: task_id.into(),
            path: worktree.path().to_str().unwrap().into(),
            branch: worktree.branch().into(),
            base_commit: worktree.base_sha().into(),
        }
    }

    fn commit_file(worktree_path: &Path, path: &str, contents: &str) -> String {
        let file = worktree_path.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, contents).unwrap();
        git(worktree_path, &["add", path]);
        git(worktree_path, &["commit", "-m", "candidate"]);
        git(worktree_path, &["rev-parse", "HEAD"])
    }

    fn verify_owned(
        repository: &Repository,
        task_id: &str,
        worktree: &TaskWorktree,
        recorded_commit: &str,
        policy: &VerificationPolicy,
    ) -> VerificationResult {
        let workspace = workspace_record(task_id, worktree);
        verify(
            repository,
            VerificationRequest {
                run_id: "verification-test",
                task_id,
                worktree_path: worktree.path(),
                workspace: &workspace,
                recorded_commit,
                policy,
            },
        )
    }

    #[test]
    fn rejects_worktree_owned_by_another_task() {
        let temp = TempDir::new().unwrap();
        let (repository, worktree) = setup_worktree(&temp, "task-a");
        let recorded_commit = git(worktree.path(), &["rev-parse", "HEAD"]);
        let workspace = WorkspaceRecord {
            task_id: "task-a".into(),
            path: worktree.path().to_string_lossy().into_owned(),
            branch: worktree.branch().into(),
            base_commit: worktree.base_sha().into(),
        };
        let policy = VerificationPolicy::default();

        let result = verify(
            &repository,
            VerificationRequest {
                run_id: "verification-1",
                task_id: "task-b",
                worktree_path: worktree.path(),
                workspace: &workspace,
                recorded_commit: &recorded_commit,
                policy: &policy,
            },
        );

        assert_eq!(result.status, VerificationStatus::Rejected);
        assert!(matches!(result.worktree, CheckOutcome::Rejected(_)));
        assert_eq!(result.candidate_commit, None);
    }

    #[test]
    fn rejects_branch_not_derived_from_task_id() {
        let temp = TempDir::new().unwrap();
        let (repository, worktree) = setup_worktree(&temp, "task-a");
        let recorded_commit = git(worktree.path(), &["rev-parse", "HEAD"]);
        let workspace = WorkspaceRecord {
            task_id: "task-a".into(),
            path: worktree.path().to_string_lossy().into_owned(),
            branch: "orchestrator/task-b".into(),
            base_commit: worktree.base_sha().into(),
        };
        let policy = VerificationPolicy::default();

        let result = verify(
            &repository,
            VerificationRequest {
                run_id: "verification-branch-mismatch",
                task_id: "task-a",
                worktree_path: worktree.path(),
                workspace: &workspace,
                recorded_commit: &recorded_commit,
                policy: &policy,
            },
        );

        assert_eq!(result.status, VerificationStatus::Rejected);
        assert!(matches!(result.worktree, CheckOutcome::Rejected(_)));
        assert_eq!(result.candidate_commit, None);
    }

    #[test]
    fn rejects_candidate_after_branch_moves() {
        let temp = TempDir::new().unwrap();
        let (repository, worktree) = setup_worktree(&temp, "task-a");
        let recorded_commit = git(worktree.path(), &["rev-parse", "HEAD"]);
        let current_commit = commit_file(worktree.path(), "candidate.txt", "candidate\n");

        let result = verify_owned(
            &repository,
            "task-a",
            &worktree,
            &recorded_commit,
            &VerificationPolicy::default(),
        );

        assert_eq!(result.status, VerificationStatus::Rejected);
        assert_eq!(
            result.candidate_commit.as_deref(),
            Some(current_commit.as_str())
        );
        assert!(matches!(
            &result.candidate,
            CheckOutcome::Rejected(detail) if detail.contains("differs from attempt commit")
        ));
        assert!(result.commands.is_empty());
    }

    #[test]
    fn rejects_mutable_branch_reference_as_attempt_commit() {
        let temp = TempDir::new().unwrap();
        let (repository, worktree) = setup_worktree(&temp, "task-a");
        let recorded_branch = worktree.branch().to_owned();

        let result = verify_owned(
            &repository,
            "task-a",
            &worktree,
            &recorded_branch,
            &VerificationPolicy::default(),
        );

        assert_eq!(result.status, VerificationStatus::Rejected);
        assert!(matches!(
            &result.candidate,
            CheckOutcome::Rejected(detail) if detail.contains("full commit object id")
        ));
    }

    #[test]
    fn fails_and_names_changed_protected_path() {
        let temp = TempDir::new().unwrap();
        let (repository, worktree) = setup_worktree(&temp, "task-a");
        let candidate = commit_file(
            worktree.path(),
            "config/ci/verification.yml",
            "weakened: true\n",
        );
        let policy = VerificationPolicy {
            protected_path_globs: vec!["config/**/verification.yml".into()],
            ..VerificationPolicy::default()
        };

        let result = verify_owned(&repository, "task-a", &worktree, &candidate, &policy);

        assert_eq!(result.status, VerificationStatus::Failed);
        assert!(matches!(
            &result.protected_paths,
            CheckOutcome::Failed(detail) if detail.contains("config/ci/verification.yml")
        ));
    }

    #[test]
    fn question_mark_matches_one_unicode_path_character() {
        let temp = TempDir::new().unwrap();
        let (repository, worktree) = setup_worktree(&temp, "task-a");
        let candidate = commit_file(worktree.path(), "config/ci/é.yml", "protected\n");
        let policy = VerificationPolicy {
            protected_path_globs: vec!["config/ci/?.yml".into()],
            ..VerificationPolicy::default()
        };

        let result = verify_owned(&repository, "task-a", &worktree, &candidate, &policy);

        assert!(matches!(
            &result.protected_paths,
            CheckOutcome::Failed(detail) if detail.contains("config/ci/é.yml")
        ));
    }

    #[test]
    fn default_trust_policy_does_not_run_untrusted_command() {
        let temp = TempDir::new().unwrap();
        let (repository, worktree) = setup_worktree(&temp, "task-a");
        let candidate = git(worktree.path(), &["rev-parse", "HEAD"]);
        let marker = temp.path().join("must-not-exist");
        let command = CommandSpec {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), format!("touch '{}'", marker.display())],
        };
        let policy = VerificationPolicy {
            validation_commands: vec![ValidationCommand {
                command,
                timeout: Duration::from_secs(2),
            }],
            ..VerificationPolicy::default()
        };

        let result = verify_owned(&repository, "task-a", &worktree, &candidate, &policy);

        assert!(!marker.exists());
        assert_eq!(result.status, VerificationStatus::Failed);
        assert_eq!(result.commands[0].trust, TrustDecision::Denied);
        assert_eq!(result.commands[0].status, CommandStatus::Untrusted);
        assert_eq!(result.commands[0].exit_code, None);
    }

    #[test]
    fn trusted_command_does_not_authorize_different_argv() {
        let temp = TempDir::new().unwrap();
        let (repository, worktree) = setup_worktree(&temp, "task-a");
        let candidate = git(worktree.path(), &["rev-parse", "HEAD"]);
        let marker = temp.path().join("untrusted-command-marker");
        let trusted = CommandSpec {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "printf authorized".into()],
        };
        let untrusted = CommandSpec {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), format!("touch '{}'", marker.display())],
        };
        let mut execution_trust = ExecutionTrustPolicy::default();
        execution_trust.trust_command(&repository, trusted).unwrap();
        let policy = VerificationPolicy {
            validation_commands: vec![ValidationCommand {
                command: untrusted,
                timeout: Duration::from_secs(2),
            }],
            execution_trust,
            ..VerificationPolicy::default()
        };

        let result = verify_owned(&repository, "task-a", &worktree, &candidate, &policy);

        assert!(!marker.exists());
        assert_eq!(result.commands[0].trust, TrustDecision::Denied);
        assert_eq!(result.commands[0].status, CommandStatus::Untrusted);
    }

    #[test]
    fn trusted_command_runs_and_records_output_and_exit_code() {
        let temp = TempDir::new().unwrap();
        let (repository, worktree) = setup_worktree(&temp, "task-a");
        let candidate = git(worktree.path(), &["rev-parse", "HEAD"]);
        let command = CommandSpec {
            program: "git".into(),
            args: vec!["--version".into()],
        };
        let mut execution_trust = ExecutionTrustPolicy::default();
        execution_trust
            .trust_command(&repository, command.clone())
            .unwrap();
        let policy = VerificationPolicy {
            validation_commands: vec![ValidationCommand {
                command,
                timeout: Duration::from_secs(2),
            }],
            execution_trust,
            ..VerificationPolicy::default()
        };

        let result = verify_owned(&repository, "task-a", &worktree, &candidate, &policy);

        assert_eq!(result.status, VerificationStatus::Passed);
        assert_eq!(result.commands[0].trust, TrustDecision::Trusted);
        assert_eq!(result.commands[0].status, CommandStatus::Passed);
        assert_eq!(result.commands[0].exit_code, Some(0));
        assert!(result.commands[0].stdout.starts_with("git version"));
    }

    #[test]
    fn trusted_nonzero_exit_is_recorded_as_failed() {
        let temp = TempDir::new().unwrap();
        let (repository, worktree) = setup_worktree(&temp, "task-a");
        let candidate = git(worktree.path(), &["rev-parse", "HEAD"]);
        let command = CommandSpec {
            program: "git".into(),
            args: ["rev-parse", "--verify", "--quiet", "refs/heads/missing"]
                .map(Into::into)
                .into(),
        };
        let mut execution_trust = ExecutionTrustPolicy::default();
        execution_trust
            .trust_command(&repository, command.clone())
            .unwrap();
        let policy = VerificationPolicy {
            validation_commands: vec![ValidationCommand {
                command,
                timeout: Duration::from_secs(2),
            }],
            execution_trust,
            ..VerificationPolicy::default()
        };

        let result = verify_owned(&repository, "task-a", &worktree, &candidate, &policy);

        assert_eq!(result.status, VerificationStatus::Failed);
        assert_eq!(result.commands[0].status, CommandStatus::Failed);
        assert_eq!(result.commands[0].exit_code, Some(1));
    }

    #[test]
    fn timed_out_command_is_killed_and_recorded() {
        let temp = TempDir::new().unwrap();
        let (repository, worktree) = setup_worktree(&temp, "task-a");
        let candidate = git(worktree.path(), &["rev-parse", "HEAD"]);
        let command = if cfg!(windows) {
            CommandSpec {
                program: "ping".into(),
                args: ["-n", "6", "127.0.0.1"].map(Into::into).into(),
            }
        } else {
            CommandSpec {
                program: "/bin/sleep".into(),
                args: vec!["5".into()],
            }
        };
        let mut execution_trust = ExecutionTrustPolicy::default();
        execution_trust
            .trust_command(&repository, command.clone())
            .unwrap();
        let policy = VerificationPolicy {
            validation_commands: vec![ValidationCommand {
                command,
                timeout: Duration::from_millis(100),
            }],
            execution_trust,
            ..VerificationPolicy::default()
        };

        let result = verify_owned(&repository, "task-a", &worktree, &candidate, &policy);

        assert_eq!(result.status, VerificationStatus::Failed);
        assert_eq!(result.commands[0].status, CommandStatus::TimedOut);
        assert_eq!(result.commands[0].exit_code, None);
        assert!(result.commands[0].duration_ms < 2_000);
    }
}
