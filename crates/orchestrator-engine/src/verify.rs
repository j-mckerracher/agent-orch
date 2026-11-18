//! Independent verification of a candidate awaiting review.
//!
//! [`verify_attempt`] is the only way a `Verifying` task leaves that state. It
//! runs the contract's checks against the candidate branch and settles the
//! attempt in one store transaction, which discards the result if the task was
//! cancelled while the checks ran. Every outcome that can be determined is
//! recorded, including a candidate the verifier cannot even open, so a task
//! never stays `Verifying` because of verifier setup.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use orchestrator_core::contract::VerificationPolicy;
use orchestrator_core::records::{Project, VerificationRun, Workspace};
use orchestrator_core::task::{AttemptState, TaskState};
use orchestrator_diagnostics::{redact, truncate};
use orchestrator_git::verification::{
    self, CheckOutcome, CommandSpec, ExecutionTrustPolicy, ValidationCommand,
    VerificationPolicy as GitVerificationPolicy, VerificationRequest, VerificationResult,
    VerificationStatus, WorkspaceRecord,
};
use orchestrator_git::Repository;
use orchestrator_store::{Settlement, SqliteStore, Store, TaskRecord};
use serde_json::{json, Value};

/// Wall-clock limit for each contract command.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Bytes of evidence one run may retain. The daemon drops run details above
/// 64 KiB, so this leaves room for the envelope and JSON escaping.
const DETAIL_BUDGET: usize = 48 * 1024;

/// Result of one [`verify_attempt`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verified {
    /// Every check passed; the attempt succeeded and the task left `Verifying`.
    Passed { run_id: String },
    /// A check failed or the candidate was rejected; the attempt failed with `reason`.
    Failed { run_id: String, reason: String },
    /// The task or attempt was no longer `Verifying` and `CandidateReady`, for
    /// example because it was cancelled. Nothing was recorded.
    Superseded,
    /// `cancel` was set while the checks ran. Nothing was recorded.
    Interrupted,
}

/// Verifies one `CandidateReady` attempt and settles it.
///
/// Returns [`Verified::Superseded`] when the task or attempt is not
/// `Verifying` and `CandidateReady`, either at entry or at settlement. Returns
/// [`Verified::Interrupted`] when `cancel` is set after the checks run. Store
/// read or write failures return `Err` and leave the task unchanged.
pub fn verify_attempt(
    store: &SqliteStore,
    task_id: &str,
    attempt_id: &str,
    cancel: &AtomicBool,
) -> Result<Verified, String> {
    let task = store
        .get(task_id)
        .map_err(|error| format!("cannot load task {task_id}: {error}"))?;
    if !is_candidate_ready(&task, attempt_id) {
        return Ok(Verified::Superseded);
    }
    let project = store
        .get_project(&task.contract.project_id)
        .map_err(|error| format!("cannot load project {}: {error}", task.contract.project_id))?;
    // A retried task owns one workspace per attempt; the newest one is the candidate.
    let workspace = store
        .workspaces_for_task(task_id)
        .map_err(|error| format!("cannot list workspaces for task {task_id}: {error}"))?
        .into_iter()
        .last();
    let started_at_ms = unix_millis();
    let run_id = format!("{attempt_id}-verification-{started_at_ms}");
    let evidence = if project.is_in_place() {
        in_place_evidence(&task.contract.verification, started_at_ms)
    } else {
        match verify_candidate(&task, &project, workspace, &run_id, started_at_ms, cancel) {
            Some(evidence) => evidence,
            None => return Ok(Verified::Interrupted),
        }
    };
    settle(store, task_id, attempt_id, run_id, evidence)
}

/// Lists `(task_id, attempt_id)` for every task in `Verifying` whose attempt is
/// `CandidateReady`. Restart uses this to resume verification that a stopped
/// daemon never finished.
pub fn pending_verifications(store: &SqliteStore) -> Result<Vec<(String, String)>, String> {
    let tasks = store
        .list()
        .map_err(|error| format!("cannot list tasks for verification: {error}"))?;
    Ok(tasks
        .into_iter()
        .filter(|task| task.state == TaskState::Verifying)
        .filter_map(|task| {
            let attempt_id = task
                .attempts
                .iter()
                .find(|attempt| attempt.state == AttemptState::CandidateReady)?
                .id
                .clone();
            Some((task.contract.id, attempt_id))
        })
        .collect())
}

/// Evidence for one run, ready to be settled.
struct Evidence {
    commit: String,
    status: String,
    started_at_ms: i64,
    finished_at_ms: i64,
    details: Value,
    failure_reason: Option<String>,
}

impl Evidence {
    fn from_result(result: &VerificationResult, gate: &str) -> Self {
        let failure_reason =
            (result.status != VerificationStatus::Passed).then(|| failed_check_reason(result));
        Self {
            commit: result.candidate_commit.clone().unwrap_or_default(),
            status: result.status.as_str().to_owned(),
            started_at_ms: result.started_at_ms,
            finished_at_ms: result.finished_at_ms,
            details: details(result, gate),
            failure_reason,
        }
    }
}

/// An in-place project has no candidate commit to check, so the run is
/// synthetic. A contract gate cannot run there and fails the run.
fn in_place_evidence(verification: &VerificationPolicy, started_at_ms: i64) -> Evidence {
    let (status, failure_reason) = if verification.clone().has_gate() {
        (
            VerificationStatus::Rejected,
            Some("verification commands cannot run in an in-place project"),
        )
    } else {
        (VerificationStatus::Passed, None)
    };
    Evidence {
        commit: String::new(),
        status: status.as_str().to_owned(),
        started_at_ms,
        finished_at_ms: started_at_ms,
        details: json!({"gate": gate_name(verification), "reason": "in_place"}),
        failure_reason: failure_reason.map(str::to_owned),
    }
}

/// Runs the Git verifier on the candidate. `None` means `cancel` was set and
/// the run must record nothing, so restart can resume it.
fn verify_candidate(
    task: &TaskRecord,
    project: &Project,
    workspace: Option<Workspace>,
    run_id: &str,
    started_at_ms: i64,
    cancel: &AtomicBool,
) -> Option<Evidence> {
    let verification = &task.contract.verification;
    let evidence = match prepare(task, project, workspace) {
        Ok(setup) => {
            let result = verification::verify(
                &setup.repository,
                VerificationRequest {
                    run_id,
                    task_id: &task.contract.id,
                    worktree_path: Path::new(&setup.workspace.path),
                    workspace: &setup.workspace,
                    recorded_commit: &setup.recorded_commit,
                    policy: &setup.policy,
                    cancel: Some(cancel),
                },
            );
            Evidence::from_result(&result, gate_name(verification))
        }
        Err(reason) => rejected_evidence(verification, started_at_ms, reason),
    };
    (!cancel.load(Ordering::Acquire)).then_some(evidence)
}

/// Everything [`verify`](verification::verify) needs, read from the store and Git.
struct Setup {
    repository: Repository,
    workspace: WorkspaceRecord,
    recorded_commit: String,
    policy: GitVerificationPolicy,
}

fn prepare(
    task: &TaskRecord,
    project: &Project,
    workspace: Option<Workspace>,
) -> Result<Setup, String> {
    let workspace = workspace.ok_or("task has no recorded workspace")?;
    let repository = Repository::new(Path::new(&project.root_path))
        .map_err(|error| format!("project repository is unusable: {error}"))?;
    let recorded_commit = repository
        .resolve_base_ref(&format!("refs/heads/{}", workspace.branch))
        .map_err(|error| {
            format!(
                "candidate branch {} does not resolve: {error}",
                workspace.branch
            )
        })?;
    let policy = validation_policy(&task.contract.verification, &repository)?;
    Ok(Setup {
        repository,
        workspace: WorkspaceRecord {
            task_id: workspace.task_id,
            path: workspace.path,
            branch: workspace.branch,
            base_commit: workspace.base_commit,
        },
        recorded_commit,
        policy,
    })
}

/// Trusts each contract command exactly as admitted. The contract, not the
/// repository, is the authority for what runs.
fn validation_policy(
    verification: &VerificationPolicy,
    repository: &Repository,
) -> Result<GitVerificationPolicy, String> {
    let commands: &[Vec<String>] = match verification {
        VerificationPolicy::None => &[],
        VerificationPolicy::Commands { commands } => commands,
    };
    let mut execution_trust = ExecutionTrustPolicy::default();
    let mut validation_commands = Vec::with_capacity(commands.len());
    for argv in commands {
        let Some((program, args)) = argv.split_first() else {
            return Err("verification command is empty".into());
        };
        let command = CommandSpec {
            program: program.clone(),
            args: args.to_vec(),
        };
        execution_trust
            .trust_command(repository, command.clone())
            .map_err(|error| format!("cannot trust verification command {program}: {error}"))?;
        validation_commands.push(ValidationCommand {
            command,
            timeout: COMMAND_TIMEOUT,
        });
    }
    Ok(GitVerificationPolicy {
        validation_commands,
        execution_trust,
        ..GitVerificationPolicy::default()
    })
}

/// A run that could not start the checks. The reason is safe to show.
fn rejected_evidence(
    verification: &VerificationPolicy,
    started_at_ms: i64,
    reason: String,
) -> Evidence {
    let reason = redact(&reason);
    Evidence {
        commit: String::new(),
        status: VerificationStatus::Rejected.as_str().to_owned(),
        started_at_ms,
        finished_at_ms: started_at_ms,
        details: json!({"gate": gate_name(verification), "reason": reason}),
        failure_reason: Some(reason),
    }
}

/// Appends the run, then settles the attempt and task in one store transaction.
fn settle(
    store: &SqliteStore,
    task_id: &str,
    attempt_id: &str,
    run_id: String,
    evidence: Evidence,
) -> Result<Verified, String> {
    let run = VerificationRun {
        id: run_id.clone(),
        task_id: task_id.to_owned(),
        commit: evidence.commit,
        status: evidence.status,
        log_path: None,
    };
    let settlement = store
        .settle_verification(
            task_id,
            attempt_id,
            &run,
            evidence.started_at_ms,
            evidence.finished_at_ms,
            &evidence.details,
            evidence.failure_reason.as_deref(),
        )
        .map_err(|error| format!("cannot record verification for task {task_id}: {error}"))?;
    Ok(match settlement {
        Settlement::Settled { passed: true } => Verified::Passed { run_id },
        Settlement::Settled { passed: false } => Verified::Failed {
            run_id,
            reason: evidence
                .failure_reason
                .unwrap_or_else(|| "verification failed".to_owned()),
        },
        Settlement::Superseded => Verified::Superseded,
    })
}

fn is_candidate_ready(task: &TaskRecord, attempt_id: &str) -> bool {
    task.state == TaskState::Verifying
        && task.attempts.iter().any(|attempt| {
            attempt.id == attempt_id && attempt.state == AttemptState::CandidateReady
        })
}

fn gate_name(verification: &VerificationPolicy) -> &'static str {
    if verification.clone().has_gate() {
        "commands"
    } else {
        "none"
    }
}

/// The detail of the first check that failed or was rejected, redacted.
fn failed_check_reason(result: &VerificationResult) -> String {
    [
        &result.worktree,
        &result.candidate,
        &result.protected_paths,
        &result.validation,
        &result.worktree_unchanged,
    ]
    .into_iter()
    .find_map(|check| match check {
        CheckOutcome::Failed(detail) | CheckOutcome::Rejected(detail) => Some(redact(detail)),
        CheckOutcome::Passed | CheckOutcome::Skipped(_) => None,
    })
    .unwrap_or_else(|| "verification failed".to_owned())
}

/// Run evidence: every check outcome, and each command's trust and output.
/// Output is redacted and shares the budget across commands.
fn details(result: &VerificationResult, gate: &str) -> Value {
    // Each command gets an equal share. A share covers two streams, and JSON
    // escapes newlines, so each stream is cut to a quarter of the share.
    let stream_chars = DETAIL_BUDGET / (4 * result.commands.len().max(1));
    let commands: Vec<Value> = result
        .commands
        .iter()
        .map(|command| {
            json!({
                "program": command.command.program,
                "args": command.command.args,
                "digest": command.digest,
                "trust": format!("{:?}", command.trust),
                "status": format!("{:?}", command.status),
                "exit_code": command.exit_code,
                "stdout": truncate(&redact(&command.stdout), stream_chars),
                "stderr": truncate(&redact(&command.stderr), stream_chars),
                "output_truncated": command.output_truncated,
                "duration_ms": command.duration_ms,
                "detail": command.detail.as_deref().map(redact),
            })
        })
        .collect();
    json!({
        "gate": gate,
        "run_id": result.run_id,
        "task_id": result.task_id,
        "candidate_commit": result.candidate_commit,
        "status": result.status.as_str(),
        "worktree": redact(&format!("{:?}", result.worktree)),
        "candidate": redact(&format!("{:?}", result.candidate)),
        "protected_paths": redact(&format!("{:?}", result.protected_paths)),
        "validation": redact(&format!("{:?}", result.validation)),
        "commands": commands,
        "worktree_unchanged": redact(&format!("{:?}", result.worktree_unchanged)),
    })
}

fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
