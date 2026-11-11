//! Multi-task review tests compose public core, Git, and SQLite APIs.
//! They do not launch agent processes or contact a remote repository.
//! Each case owns a temporary repository and SQLite database. The restart
//! case closes its only store handle after every persisted workflow stage.
//! They are not platform-gated: the failing validation command is `git`
//! itself, so no shell or Unix-only API is needed. Native Windows results
//! are recorded separately per pinned commit.
//! Updated: 2026-11-09T07:09:00-07:00.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use orchestrator_core::approval::{
    ActionMismatch, Actor, Approval, ApprovalAction, ApprovalActionKind, ApprovalError,
    ApprovalEvent, ApproverAllowlist, ParameterDigest,
};
use orchestrator_core::authorization::ActionKind;
use orchestrator_core::decomposition::{validate_proposal, ValidatedPlan};
use orchestrator_core::records::{Project, VerificationRun};
use orchestrator_core::scheduler::{
    RejectionReason, SchedulePlan, Scheduler, SchedulerLimits, SchedulerSnapshot, TaskSnapshot,
};
use orchestrator_core::task::{TaskEvent, TaskState};
use orchestrator_git::delivery::{
    self, ApprovalCheck, CallbackError, DeliveryAction, DeliveryCallbacks, DeliveryError,
    DeliveryOutcome, DeliveryRequest, DeliveryStep, VerificationCheck,
};
use orchestrator_git::verification::{
    verify, CheckOutcome, CommandSpec, CommandStatus, ExecutionTrustPolicy, ValidationCommand,
    VerificationPolicy as GitVerificationPolicy, VerificationRequest, VerificationResult,
    VerificationStatus, WorkspaceRecord,
};
use orchestrator_git::{create_task_worktree, Repository, TaskWorktree};
use orchestrator_store::{
    DeliveryOutcome as StoreDeliveryOutcome, DeliveryStepRecord, SqliteStore, Store, TaskRecord,
};
use serde_json::{json, Value};
use tempfile::TempDir;

fn proposal_task(title: &str, key: &str, dependencies: &[&str]) -> Value {
    json!({
        "title": title,
        "contract": {
            "id": key,
            "project_id": "project",
            "parent_id": null,
            "objective": format!("Implement {title}"),
            "kind": "Implement",
            "agent": "test-agent",
            "dependencies": dependencies,
            "verification": "None",
            "delivery": "LocalReview",
            "autonomy": "WorkspaceWrites",
            "max_retries": 0,
            "max_repairs": 0
        }
    })
}

fn proposal(keys: [&str; 3]) -> String {
    json!({
        "objective": "Deliver reviewed widget",
        "tasks": [
            proposal_task("A", keys[0], &[]),
            proposal_task("B", keys[1], &[]),
            proposal_task("C", keys[2], &[keys[0], keys[1]])
        ]
    })
    .to_string()
}

fn validated_plan() -> ValidatedPlan {
    validate_proposal(&proposal(["proposal-a", "proposal-b", "proposal-c"]))
        .expect("valid three-task proposal")
}

fn plan_ids(plan: &ValidatedPlan) -> Vec<String> {
    plan.tasks()
        .iter()
        .map(|task| task.contract().id.clone())
        .collect()
}

fn schedule(plan: &ValidatedPlan, states: &[TaskState]) -> SchedulePlan {
    assert_eq!(plan.tasks().len(), states.len());
    let tasks: Vec<_> = plan
        .tasks()
        .iter()
        .zip(states)
        .map(|(task, state)| TaskSnapshot {
            contract: task.contract(),
            state: *state,
            workspace: None,
        })
        .collect();
    let authorization = |project_id: &str, _task_id: &str| project_id == "project";
    Scheduler::new(
        SchedulerSnapshot {
            tasks: &tasks,
            last_started_project: None,
        },
        SchedulerLimits::default(),
    )
    .expect("valid scheduler snapshot")
    .schedule(Some(&authorization))
}

#[test]
fn decomposition_replaces_proposal_ids_and_rewrites_dependencies() {
    let plan = validate_proposal(&proposal(["proposal-a", "proposal-b", "proposal-c"]))
        .expect("validate proposal");
    let ids = plan_ids(&plan);

    assert_eq!(ids.len(), 3);
    assert!(ids.iter().all(|id| id.starts_with("decomp-")));
    assert!(ids
        .iter()
        .all(|id| { !["proposal-a", "proposal-b", "proposal-c"].contains(&id.as_str()) }));
    assert_eq!(
        plan.tasks()[2].contract().dependencies,
        vec![ids[0].clone(), ids[1].clone()]
    );

    let renamed_local_keys = validate_proposal(&proposal(["other-a", "other-b", "other-c"]))
        .expect("validate equivalent proposal");
    assert_eq!(plan_ids(&renamed_local_keys), ids);
}

#[test]
fn scheduler_starts_dependencies_before_dependent_task() {
    let plan = validated_plan();
    let ids = plan_ids(&plan);
    let first = schedule(&plan, &[TaskState::Queued; 3]);

    assert_eq!(first.started, vec![ids[0].clone(), ids[1].clone()]);
    assert!(first.rejected.iter().any(|rejection| {
        rejection.task_id == ids[2]
            && matches!(
                rejection.reason,
                RejectionReason::DependencyNotSucceeded { .. }
            )
    }));

    let while_b_runs = schedule(
        &plan,
        &[TaskState::Succeeded, TaskState::Running, TaskState::Queued],
    );
    assert!(while_b_runs.started.is_empty());
    assert!(while_b_runs.rejected.iter().any(|rejection| {
        rejection.task_id == ids[2]
            && matches!(
                &rejection.reason,
                RejectionReason::DependencyNotSucceeded { dependency_id }
                    if dependency_id == &ids[1]
            )
    }));

    let after_b_succeeds = schedule(
        &plan,
        &[
            TaskState::Succeeded,
            TaskState::Succeeded,
            TaskState::Queued,
        ],
    );
    assert_eq!(after_b_succeeds.started, vec![ids[2].clone()]);
}
struct TestWorkspace {
    _root: TempDir,
    primary_path: PathBuf,
    worktrees_root: PathBuf,
    database_path: PathBuf,
    repository: Repository,
}

struct Candidate {
    worktree: TaskWorktree,
    sha: String,
}

fn workspace_fixture() -> TestWorkspace {
    let root = tempfile::tempdir().expect("create isolated workflow directory");
    let primary_path = root.path().join("primary");
    fs::create_dir_all(&primary_path).expect("create primary checkout");
    git(&primary_path, &["init", "--quiet", "-b", "main"]);
    git(&primary_path, &["config", "user.name", "Workflow Test"]);
    git(
        &primary_path,
        &["config", "user.email", "workflow@example.invalid"],
    );
    fs::write(primary_path.join("base.txt"), "base\n").expect("write base file");
    git(&primary_path, &["add", "base.txt"]);
    git(&primary_path, &["commit", "--quiet", "-m", "base"]);
    let repository = Repository::new(&primary_path).expect("validate primary repository");
    let worktrees_root = root.path().join("worktrees");
    fs::create_dir_all(&worktrees_root).expect("create task worktree root");

    TestWorkspace {
        database_path: root.path().join("store.sqlite"),
        _root: root,
        primary_path,
        worktrees_root,
        repository,
    }
}

fn git(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(directory)
        .env("GIT_CONFIG_GLOBAL", "/nonexistent-git-global")
        .env("GIT_CONFIG_SYSTEM", "/nonexistent-git-system")
        .args(args)
        .output()
        .expect("run Git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn commit_file(worktree: &Path, path: &str, contents: &str, message: &str) -> String {
    let file = worktree.join(path);
    fs::create_dir_all(file.parent().expect("file has parent")).expect("create candidate path");
    fs::write(file, contents).expect("write candidate file");
    git(worktree, &["add", "--", path]);
    git(worktree, &["commit", "--quiet", "-m", message]);
    git(worktree, &["rev-parse", "HEAD"])
}

fn candidate(workspace: &TestWorkspace, task_id: &str, path: &str, contents: &str) -> Candidate {
    let worktree = create_task_worktree(
        &workspace.repository,
        task_id,
        "main",
        &workspace.worktrees_root,
    )
    .expect("create task-owned worktree");
    let sha = commit_file(worktree.path(), path, contents, "candidate");
    Candidate { worktree, sha }
}

fn register_plan(store: &SqliteStore, plan: &ValidatedPlan, repository_path: &Path) {
    store
        .register_project(&Project {
            id: "project".into(),
            root_path: repository_path.display().to_string(),
            settings: json!({}),
        })
        .expect("register project");
    for task in plan.tasks() {
        store
            .create(task.contract())
            .expect("persist task contract");
    }
    for task in plan.tasks() {
        store
            .transition(&task.contract().id, TaskEvent::Admit)
            .expect("admit task");
    }
}

fn restart_store(store: SqliteStore, path: &Path) -> SqliteStore {
    drop(store);
    SqliteStore::new(path).expect("reopen workflow database")
}

fn workspace_record(task_id: &str, candidate: &Candidate) -> WorkspaceRecord {
    WorkspaceRecord {
        task_id: task_id.into(),
        path: candidate.worktree.path().display().to_string(),
        branch: candidate.worktree.branch().into(),
        base_commit: candidate.worktree.base_sha().into(),
    }
}

fn verify_candidate(
    repository: &Repository,
    task_id: &str,
    candidate: &Candidate,
    run_id: &str,
    policy: &GitVerificationPolicy,
) -> VerificationResult {
    let workspace = workspace_record(task_id, candidate);
    verify(
        repository,
        VerificationRequest {
            run_id,
            task_id,
            worktree_path: candidate.worktree.path(),
            workspace: &workspace,
            recorded_commit: &candidate.sha,
            policy,
        },
    )
}

fn persist_verification(store: &SqliteStore, result: &VerificationResult) -> VerificationRun {
    let run = VerificationRun {
        id: result.run_id.clone(),
        task_id: result.task_id.clone(),
        commit: result
            .candidate_commit
            .clone()
            .expect("verifier resolved candidate commit"),
        status: result.status.as_str().into(),
        log_path: None,
    };
    store
        .record_verification_result(
            &run,
            result.started_at_ms,
            result.finished_at_ms,
            &verification_details(result),
        )
        .expect("persist verifier evidence");
    run
}

fn verification_details(result: &VerificationResult) -> Value {
    let commands: Vec<_> = result
        .commands
        .iter()
        .map(|command| {
            json!({
                "program": &command.command.program,
                "args": &command.command.args,
                "digest": &command.digest,
                "trust": format!("{:?}", command.trust),
                "status": format!("{:?}", command.status),
                "exit_code": command.exit_code,
                "stdout": &command.stdout,
                "stderr": &command.stderr,
                "output_truncated": command.output_truncated,
                "duration_ms": command.duration_ms,
                "detail": &command.detail,
            })
        })
        .collect();
    json!({
        "run_id": result.run_id,
        "task_id": result.task_id,
        "candidate_commit": result.candidate_commit,
        "status": result.status.as_str(),
        "worktree": format!("{:?}", result.worktree),
        "candidate": format!("{:?}", result.candidate),
        "protected_paths": format!("{:?}", result.protected_paths),
        "validation": format!("{:?}", result.validation),
        "commands": commands,
        "worktree_unchanged": format!("{:?}", result.worktree_unchanged),
    })
}

fn schedule_records(records: &[TaskRecord]) -> SchedulePlan {
    let tasks: Vec<_> = records
        .iter()
        .map(|record| TaskSnapshot {
            contract: &record.contract,
            state: record.state,
            workspace: None,
        })
        .collect();
    let authorization = |project_id: &str, _task_id: &str| project_id == "project";
    Scheduler::new(
        SchedulerSnapshot {
            tasks: &tasks,
            last_started_project: None,
        },
        SchedulerLimits::default(),
    )
    .expect("valid persisted scheduler snapshot")
    .schedule(Some(&authorization))
}

fn start_task(store: &SqliteStore, task_id: &str) -> String {
    let attempt_id = format!("attempt-{task_id}");
    store
        .add_attempt(
            task_id,
            &orchestrator_core::task::Attempt {
                id: attempt_id.clone(),
                task_id: task_id.into(),
                attempt_number: 1,
                state: orchestrator_core::task::AttemptState::Pending,
            },
        )
        .expect("record scheduled attempt");
    store
        .update_attempt_state(
            task_id,
            &attempt_id,
            orchestrator_core::task::AttemptState::Running,
        )
        .expect("start attempt");
    store
        .transition(task_id, TaskEvent::ExecutionStarted)
        .expect("start task");
    attempt_id
}

fn complete_task_candidate(store: &SqliteStore, task_id: &str, attempt_id: &str) {
    store
        .update_attempt_state(
            task_id,
            attempt_id,
            orchestrator_core::task::AttemptState::CandidateReady,
        )
        .expect("record candidate on attempt");
    store
        .transition(task_id, TaskEvent::CandidateReady)
        .expect("move task to verification");
    store
        .update_attempt_state(
            task_id,
            attempt_id,
            orchestrator_core::task::AttemptState::Succeeded,
        )
        .expect("complete verified attempt");
    store
        .transition(task_id, TaskEvent::VerificationOutcome { passed: true })
        .expect("complete verified task");
}

fn action(task_id: &str, candidate_sha: &str, digest: [u8; 32]) -> ApprovalAction {
    ApprovalAction::new(
        ApprovalActionKind::Delivery(ActionKind::LocalReviewMerge),
        ParameterDigest::from_sha256(digest),
        candidate_sha,
        task_id,
    )
    .expect("valid local-review approval binding")
}

fn approve_candidate(
    store: &SqliteStore,
    task_id: &str,
    candidate_sha: &str,
    digest: [u8; 32],
) -> Approval {
    let approval_id = format!("approval-{task_id}");
    let allowlist = ApproverAllowlist::new("reviewer", std::iter::empty::<String>())
        .expect("valid approver allowlist");
    let proposed = action(task_id, candidate_sha, digest);
    store
        .request_approval(
            &approval_id,
            &proposed,
            &Actor::Agent("worker".into()),
            &allowlist,
        )
        .expect("worker requests approval");
    store
        .approve_approval(&approval_id, &Actor::User("reviewer".into()), &allowlist)
        .expect("user approves candidate");
    store
        .get_approval(&approval_id)
        .expect("load approved action")
}

fn callback_error(error: impl std::fmt::Display) -> CallbackError {
    Box::new(std::io::Error::other(error.to_string()))
}

fn check_approval(store: &SqliteStore, check: &ApprovalCheck<'_>) -> Result<(), CallbackError> {
    let action_kind = match check.action {
        DeliveryAction::LocalReview => ActionKind::LocalReviewMerge,
        DeliveryAction::ReportOnly => ActionKind::ReportPublish,
    };
    store
        .check_delivery_approval(
            check.approval_id,
            check.task_id,
            action_kind,
            ParameterDigest::from_sha256(*check.parameter_digest),
            check.candidate_sha,
        )
        .map_err(callback_error)
}

fn check_verification(
    store: &SqliteStore,
    check: &VerificationCheck<'_>,
) -> Result<(), CallbackError> {
    match store.delivery_verification_matches(check.task_id, check.run_id, check.candidate_sha) {
        Ok(true) => Ok(()),
        Ok(false) => Err(callback_error("verification did not pass for candidate")),
        Err(error) => Err(callback_error(error)),
    }
}

fn persist_delivery(store: &SqliteStore, step: &DeliveryStep) -> Result<(), CallbackError> {
    let action_kind = match step.action {
        DeliveryAction::LocalReview => ActionKind::LocalReviewMerge,
        DeliveryAction::ReportOnly => ActionKind::ReportPublish,
    };
    let action = ApprovalAction::new(
        ApprovalActionKind::Delivery(action_kind),
        ParameterDigest::from_sha256(step.parameter_digest),
        step.candidate_sha.clone(),
        step.task_id.clone(),
    )
    .map_err(callback_error)?;
    let outcome = match step.outcome {
        DeliveryOutcome::Reported => StoreDeliveryOutcome::Reported,
        DeliveryOutcome::ReviewRefCreated => StoreDeliveryOutcome::ReviewRefCreated,
        DeliveryOutcome::ReviewRefAlreadyMatches => StoreDeliveryOutcome::ReviewRefAlreadyMatches,
        DeliveryOutcome::Refused => StoreDeliveryOutcome::Refused {
            detail: step.detail.clone().unwrap_or_default(),
        },
    };
    let destination = step.destination.display().to_string();
    store
        .record_delivery_step(&DeliveryStepRecord {
            id: &step.id,
            approval_id: &step.approval_id,
            verification_run_id: &step.verification_run_id,
            action: &action,
            destination: &destination,
            target_ref: step.target_ref.as_deref(),
            outcome,
            diff_summary: step.diff_summary.as_deref(),
        })
        .map_err(callback_error)
}

fn with_store_callbacks<T>(
    store: &SqliteStore,
    run: impl FnOnce(&DeliveryCallbacks<'_>) -> T,
) -> T {
    let approval: &dyn for<'borrow, 'check> Fn(
        &'borrow ApprovalCheck<'check>,
    ) -> Result<(), CallbackError> = &|check: &ApprovalCheck<'_>| check_approval(store, check);
    let verification: &dyn for<'borrow, 'check> Fn(
        &'borrow VerificationCheck<'check>,
    ) -> Result<(), CallbackError> =
        &|check: &VerificationCheck<'_>| check_verification(store, check);
    let persist = |step: &DeliveryStep| persist_delivery(store, step);
    let callbacks = DeliveryCallbacks {
        check_approval: approval,
        check_verification: verification,
        persist_step: &persist,
    };
    run(&callbacks)
}

#[test]
fn verification_blocks_failed_commands_and_names_protected_paths() {
    let plan = validated_plan();
    let ids = plan_ids(&plan);
    let workspace = workspace_fixture();
    let store = SqliteStore::new(&workspace.database_path).expect("open test database");
    register_plan(&store, &plan, &workspace.primary_path);

    let failed_candidate = candidate(&workspace, &ids[0], "candidate-a.txt", "candidate\n");
    let command = CommandSpec {
        program: "git".into(),
        args: ["rev-parse", "--verify", "--quiet", "refs/heads/missing"]
            .map(Into::into)
            .into(),
    };
    let mut execution_trust = ExecutionTrustPolicy::default();
    execution_trust
        .trust_command(&workspace.repository, command.clone())
        .expect("trust exact validation command");
    let failed_policy = GitVerificationPolicy {
        validation_commands: vec![ValidationCommand {
            command,
            timeout: Duration::from_secs(5),
        }],
        execution_trust,
        ..GitVerificationPolicy::default()
    };
    let failed = verify_candidate(
        &workspace.repository,
        &ids[0],
        &failed_candidate,
        "verification-failed-command",
        &failed_policy,
    );
    assert_eq!(failed.status, VerificationStatus::Failed);
    assert_eq!(failed.commands[0].status, CommandStatus::Failed);
    assert_eq!(failed.commands[0].exit_code, Some(1));
    let failed_run = persist_verification(&store, &failed);
    assert!(!store
        .delivery_verification_matches(&ids[0], &failed_run.id, &failed_candidate.sha)
        .expect("recheck failed verification"));

    let review_ref = format!("refs/heads/orchestrator/review/{}", ids[0]);
    // Approve the exact candidate first: only failed verification may block delivery.
    let failed_approval = approve_candidate(&store, &ids[0], &failed_candidate.sha, [7; 32]);
    let refs_before_refusal = reference_values(&workspace.primary_path);
    let request = DeliveryRequest {
        step_id: "delivery-failed-validation",
        task_id: &ids[0],
        candidate_sha: &failed_candidate.sha,
        base_sha: failed_candidate.worktree.base_sha(),
        approval_id: &failed_approval.id,
        verification_run_id: &failed_run.id,
        parameter_digest: [7; 32],
        worktree_path: failed_candidate.worktree.path(),
    };
    let refusal = with_store_callbacks(&store, |callbacks| {
        delivery::local_review(&workspace.repository, request, &review_ref, callbacks)
    })
    .expect_err("failed validation must block delivery");
    assert!(matches!(refusal, DeliveryError::Verification(_)));
    assert_eq!(
        reference_values(&workspace.primary_path),
        refs_before_refusal
    );

    let protected_candidate = candidate(
        &workspace,
        &ids[1],
        "config/ci/verification.yml",
        "validation: weakened\n",
    );
    let protected_policy = GitVerificationPolicy {
        protected_path_globs: vec!["config/**/verification.yml".into()],
        ..GitVerificationPolicy::default()
    };
    let protected = verify_candidate(
        &workspace.repository,
        &ids[1],
        &protected_candidate,
        "verification-protected-path",
        &protected_policy,
    );
    assert_eq!(protected.status, VerificationStatus::Failed);
    assert!(matches!(
        &protected.protected_paths,
        CheckOutcome::Failed(detail) if detail.contains("config/ci/verification.yml")
    ));
    let protected_run = persist_verification(&store, &protected);
    assert!(!store
        .delivery_verification_matches(&ids[1], &protected_run.id, &protected_candidate.sha)
        .expect("recheck protected-path failure"));
}

fn reference_values(repository_path: &Path) -> BTreeMap<String, String> {
    git(
        repository_path,
        &["for-each-ref", "--format=%(refname) %(objectname)"],
    )
    .lines()
    .map(|line| {
        let (name, object) = line.split_once(' ').expect("Git ref line has object id");
        (name.to_owned(), object.to_owned())
    })
    .collect()
}

#[test]
fn delivery_requires_exact_approval_and_reapproval_after_candidate_change() {
    let plan = validated_plan();
    let ids = plan_ids(&plan);
    let workspace = workspace_fixture();
    let mut store = SqliteStore::new(&workspace.database_path).expect("open test database");
    register_plan(&store, &plan, &workspace.primary_path);
    let mut candidate = candidate(&workspace, &ids[0], "candidate-a.txt", "first\n");
    let default_policy = GitVerificationPolicy::default();
    let first_result = verify_candidate(
        &workspace.repository,
        &ids[0],
        &candidate,
        "verification-first-candidate",
        &default_policy,
    );
    assert_eq!(first_result.status, VerificationStatus::Passed);
    let first_run = persist_verification(&store, &first_result);

    let digest = [9; 32];
    let old_approval = approve_candidate(&store, &ids[0], &candidate.sha, digest);
    assert_eq!(
        store.check_delivery_approval(
            &old_approval.id,
            &ids[0],
            ActionKind::LocalReviewMerge,
            ParameterDigest::from_sha256(digest),
            &candidate.sha,
        ),
        Ok(())
    );
    assert_eq!(
        store.check_delivery_approval(
            &old_approval.id,
            &ids[0],
            ActionKind::ReportPublish,
            ParameterDigest::from_sha256(digest),
            &candidate.sha,
        ),
        Err(ApprovalError::ActionMismatch(ActionMismatch::Kind))
    );
    assert_eq!(
        store.check_delivery_approval(
            &old_approval.id,
            &ids[0],
            ActionKind::LocalReviewMerge,
            ParameterDigest::from_sha256([10; 32]),
            &candidate.sha,
        ),
        Err(ApprovalError::ActionMismatch(ActionMismatch::Parameters))
    );

    let allowlist = ApproverAllowlist::new("reviewer", std::iter::empty::<String>())
        .expect("valid approver allowlist");
    let agent_action = action(&ids[1], &candidate.sha, [11; 32]);
    store
        .request_approval(
            "approval-agent-cannot-decide",
            &agent_action,
            &Actor::Agent("worker".into()),
            &allowlist,
        )
        .expect("agent may request approval");
    assert_eq!(
        store.approve_approval(
            "approval-agent-cannot-decide",
            &Actor::Agent("worker".into()),
            &allowlist,
        ),
        Err(ApprovalError::UnauthorizedActor {
            actor_id: "worker".into(),
        })
    );
    assert_eq!(
        store
            .get_approval("approval-agent-cannot-decide")
            .expect("load pending approval")
            .state
            .kind(),
        orchestrator_core::approval::ApprovalStateKind::Requested
    );

    candidate.sha = commit_file(
        candidate.worktree.path(),
        "candidate-a.txt",
        "changed after approval\n",
        "changed candidate",
    );
    let changed_result = verify_candidate(
        &workspace.repository,
        &ids[0],
        &candidate,
        "verification-changed-candidate",
        &default_policy,
    );
    assert_eq!(changed_result.status, VerificationStatus::Passed);
    let changed_run = persist_verification(&store, &changed_result);
    // Reopen from disk: the old approval and stale verification must still be refused.
    store = restart_store(store, &workspace.database_path);
    assert!(!store
        .delivery_verification_matches(&ids[0], &first_run.id, &candidate.sha)
        .expect("recheck stale verification"));
    let changed_action = action(&ids[0], &candidate.sha, digest);
    assert_eq!(
        store.check_approval(&old_approval.id, &changed_action),
        Err(ApprovalError::ActionMismatch(
            ActionMismatch::CandidateCommit
        ))
    );

    let review_ref = format!("refs/heads/orchestrator/review/{}", ids[0]);
    let changed_request = DeliveryRequest {
        step_id: "delivery-before-reapproval",
        task_id: &ids[0],
        candidate_sha: &candidate.sha,
        base_sha: candidate.worktree.base_sha(),
        approval_id: &old_approval.id,
        verification_run_id: &changed_run.id,
        parameter_digest: digest,
        worktree_path: candidate.worktree.path(),
    };
    let refs_before_refusal = reference_values(&workspace.primary_path);
    let refusal = with_store_callbacks(&store, |callbacks| {
        delivery::local_review(
            &workspace.repository,
            changed_request,
            &review_ref,
            callbacks,
        )
    })
    .expect_err("changed candidate must fail old approval");
    assert!(matches!(refusal, DeliveryError::Approval(_)));
    assert_eq!(
        reference_values(&workspace.primary_path),
        refs_before_refusal
    );

    let replacement = action(&ids[0], &candidate.sha, digest);
    store
        .request_approval(
            "approval-reapproved",
            &replacement,
            &Actor::Agent("worker".into()),
            &allowlist,
        )
        .expect("request approval for changed candidate");
    store
        .approve_approval(
            "approval-reapproved",
            &Actor::User("reviewer".into()),
            &allowlist,
        )
        .expect("approve changed candidate");
    assert_eq!(
        store.get_approval(&old_approval.id).unwrap().state,
        orchestrator_core::approval::ApprovalState::Superseded {
            approved_by: Some(Actor::User("reviewer".into())),
        }
    );
    assert_eq!(
        store.check_delivery_approval(
            "approval-reapproved",
            &ids[0],
            ActionKind::LocalReviewMerge,
            ParameterDigest::from_sha256(digest),
            &candidate.sha,
        ),
        Ok(())
    );

    let delivery_request = DeliveryRequest {
        step_id: "delivery-after-reapproval",
        task_id: &ids[0],
        candidate_sha: &candidate.sha,
        base_sha: candidate.worktree.base_sha(),
        approval_id: "approval-reapproved",
        verification_run_id: &changed_run.id,
        parameter_digest: digest,
        worktree_path: candidate.worktree.path(),
    };
    let delivered = with_store_callbacks(&store, |callbacks| {
        delivery::local_review(
            &workspace.repository,
            delivery_request,
            &review_ref,
            callbacks,
        )
    })
    .expect("deliver after matching reapproval");
    assert_eq!(delivered.outcome, DeliveryOutcome::ReviewRefCreated);
    assert_eq!(
        git(&workspace.primary_path, &["rev-parse", &review_ref]),
        candidate.sha
    );
}

struct TaskEvidence {
    id: String,
    candidate_sha: String,
    verification: (VerificationRun, Value),
    approval: Approval,
    approval_events: Vec<ApprovalEvent>,
    delivery_steps: Vec<Value>,
    delivery_events: Vec<(String, Vec<Value>)>,
}

struct WorkflowEvidence {
    tasks: Vec<TaskEvidence>,
    refs_before_delivery: BTreeMap<String, String>,
    refs_after_refusal: BTreeMap<String, String>,
    refs_after_delivery: BTreeMap<String, String>,
    main_before: String,
    main_after: String,
    primary_head_before: String,
    primary_head_after: String,
    symbolic_head_before: String,
    symbolic_head_after: String,
    status_before: String,
    status_after: String,
}

fn run_restarting_workflow() -> WorkflowEvidence {
    let plan = validated_plan();
    let ids = plan_ids(&plan);
    let workspace = workspace_fixture();
    let mut store = SqliteStore::new(&workspace.database_path).expect("open workflow database");
    register_plan(&store, &plan, &workspace.primary_path);

    let admitted_records = store.list().expect("read admitted tasks");
    store = restart_store(store, &workspace.database_path);
    assert_eq!(
        store.list().expect("read tasks after plan restart"),
        admitted_records
    );

    let scheduled_before_restart =
        schedule_records(&store.list().expect("read scheduler snapshot"));
    store = restart_store(store, &workspace.database_path);
    let scheduled_after_restart = schedule_records(&store.list().expect("read reopened snapshot"));
    assert_eq!(scheduled_after_restart, scheduled_before_restart);
    assert_eq!(
        scheduled_after_restart.started,
        vec![ids[0].clone(), ids[1].clone()]
    );
    let running_attempts: Vec<_> = scheduled_after_restart
        .started
        .iter()
        .map(|task_id| (task_id.clone(), start_task(&store, task_id)))
        .collect();

    let running_records = store.list().expect("read started tasks");
    store = restart_store(store, &workspace.database_path);
    assert_eq!(
        store.list().expect("read tasks after start restart"),
        running_records
    );
    for task_id in [&ids[0], &ids[1]] {
        assert_eq!(
            store.get(task_id).expect("load running task").state,
            TaskState::Running
        );
    }

    let candidate_a = candidate(&workspace, &ids[0], "task-a.txt", "A\n");
    let candidate_b = candidate(&workspace, &ids[1], "task-b.txt", "B\n");
    let policy = GitVerificationPolicy::default();
    let result_a = verify_candidate(
        &workspace.repository,
        &ids[0],
        &candidate_a,
        "verification-a",
        &policy,
    );
    assert_eq!(result_a.status, VerificationStatus::Passed);
    let run_a = persist_verification(&store, &result_a);
    let attempt_a = &running_attempts
        .iter()
        .find(|(task_id, _)| task_id == &ids[0])
        .expect("A was scheduled")
        .1;
    complete_task_candidate(&store, &ids[0], attempt_a);

    let records_after_a = store.list().expect("read A completion state");
    let verification_a_before_restart = store
        .verification_results_for_task(&ids[0])
        .expect("read A verifier evidence");
    store = restart_store(store, &workspace.database_path);
    assert_eq!(
        store.list().expect("read reopened A state"),
        records_after_a
    );
    assert_eq!(
        store
            .verification_results_for_task(&ids[0])
            .expect("read reopened A verifier evidence"),
        verification_a_before_restart
    );

    let blocked_c_before_restart =
        schedule_records(&store.list().expect("read A-complete scheduler snapshot"));
    assert!(blocked_c_before_restart.started.is_empty());
    assert!(blocked_c_before_restart.rejected.iter().any(|rejection| {
        rejection.task_id == ids[2]
            && matches!(
                &rejection.reason,
                RejectionReason::DependencyNotSucceeded { dependency_id }
                    if dependency_id == &ids[1]
            )
    }));
    store = restart_store(store, &workspace.database_path);
    let blocked_c_after_restart =
        schedule_records(&store.list().expect("read reopened A-complete snapshot"));
    assert_eq!(blocked_c_after_restart, blocked_c_before_restart);

    let attempt_b = running_attempts
        .iter()
        .find(|(task_id, _)| task_id == &ids[1])
        .expect("B was scheduled")
        .1
        .clone();
    let result_b = verify_candidate(
        &workspace.repository,
        &ids[1],
        &candidate_b,
        "verification-b",
        &policy,
    );
    assert_eq!(result_b.status, VerificationStatus::Passed);
    let run_b = persist_verification(&store, &result_b);
    complete_task_candidate(&store, &ids[1], &attempt_b);

    let records_after_b = store.list().expect("read B completion state");
    let verification_b_before_restart = store
        .verification_results_for_task(&ids[1])
        .expect("read B verifier evidence");
    store = restart_store(store, &workspace.database_path);
    assert_eq!(
        store.list().expect("read reopened B state"),
        records_after_b
    );
    assert_eq!(
        store
            .verification_results_for_task(&ids[1])
            .expect("read reopened B verifier evidence"),
        verification_b_before_restart
    );
    assert_eq!(verification_b_before_restart[0].0, run_b);

    let scheduled_c_before_restart =
        schedule_records(&store.list().expect("read both-success scheduler snapshot"));
    assert_eq!(scheduled_c_before_restart.started, vec![ids[2].clone()]);
    store = restart_store(store, &workspace.database_path);
    let scheduled_c_after_restart =
        schedule_records(&store.list().expect("read reopened both-success snapshot"));
    assert_eq!(scheduled_c_after_restart, scheduled_c_before_restart);
    let attempt_c = start_task(&store, &ids[2]);
    let records_with_c_running = store.list().expect("read C start state");
    store = restart_store(store, &workspace.database_path);
    assert_eq!(
        store.list().expect("read reopened C start state"),
        records_with_c_running
    );

    let candidate_c = candidate(&workspace, &ids[2], "task-c.txt", "C\n");
    let result_c = verify_candidate(
        &workspace.repository,
        &ids[2],
        &candidate_c,
        "verification-c",
        &policy,
    );
    assert_eq!(result_c.status, VerificationStatus::Passed);
    let run_c = persist_verification(&store, &result_c);
    complete_task_candidate(&store, &ids[2], &attempt_c);

    let verified_records = store.list().expect("read verified task records");
    let verification_before_restart: BTreeMap<_, _> = ids
        .iter()
        .map(|task_id| {
            (
                task_id.clone(),
                store
                    .verification_results_for_task(task_id)
                    .expect("read persisted verifier evidence"),
            )
        })
        .collect();
    store = restart_store(store, &workspace.database_path);
    assert_eq!(
        store.list().expect("read tasks after verification restart"),
        verified_records
    );
    for (task_id, expected) in &verification_before_restart {
        assert_eq!(
            store
                .verification_results_for_task(task_id)
                .expect("read verifier evidence after restart"),
            *expected
        );
    }
    assert_eq!(verification_before_restart[&ids[0]][0].0, run_a);
    assert_eq!(verification_before_restart[&ids[1]][0].0, run_b);
    assert_eq!(verification_before_restart[&ids[2]][0].0, run_c);
    assert!(verified_records
        .iter()
        .all(|record| record.state == TaskState::Succeeded));

    let candidates = [candidate_a, candidate_b, candidate_c];
    let run_ids = ["verification-a", "verification-b", "verification-c"];
    let digests = [[1; 32], [2; 32], [3; 32]];
    let mut approvals_before_restart = Vec::new();
    for (index, task_id) in ids.iter().enumerate() {
        let approval = approve_candidate(&store, task_id, &candidates[index].sha, digests[index]);
        let events = store
            .approval_events(&approval.id)
            .expect("read approval events");
        approvals_before_restart.push((approval, events));
    }
    store = restart_store(store, &workspace.database_path);
    for (index, task_id) in ids.iter().enumerate() {
        let (expected_approval, expected_events) = &approvals_before_restart[index];
        assert_eq!(
            store
                .get_approval(&expected_approval.id)
                .expect("read approval after restart"),
            *expected_approval
        );
        assert_eq!(
            store
                .approval_events(&expected_approval.id)
                .expect("read approval events after restart"),
            *expected_events
        );
        assert_eq!(
            store.check_delivery_approval(
                &expected_approval.id,
                task_id,
                ActionKind::LocalReviewMerge,
                ParameterDigest::from_sha256(digests[index]),
                &candidates[index].sha,
            ),
            Ok(())
        );
    }

    // After reopening, the persisted approval still rejects another candidate and digest.
    let refs_before_mismatch = reference_values(&workspace.primary_path);
    let reopened_approval_id = &approvals_before_restart[0].0.id;
    assert_eq!(
        store.check_delivery_approval(
            reopened_approval_id,
            &ids[0],
            ActionKind::LocalReviewMerge,
            ParameterDigest::from_sha256(digests[0]),
            &candidates[1].sha,
        ),
        Err(ApprovalError::ActionMismatch(
            ActionMismatch::CandidateCommit
        ))
    );
    assert_eq!(
        store.check_delivery_approval(
            reopened_approval_id,
            &ids[0],
            ActionKind::LocalReviewMerge,
            ParameterDigest::from_sha256([9; 32]),
            &candidates[0].sha,
        ),
        Err(ApprovalError::ActionMismatch(ActionMismatch::Parameters))
    );
    assert_eq!(
        reference_values(&workspace.primary_path),
        refs_before_mismatch
    );

    let refs_before_delivery = reference_values(&workspace.primary_path);
    let main_before = git(&workspace.primary_path, &["rev-parse", "refs/heads/main"]);
    let primary_head_before = git(&workspace.primary_path, &["rev-parse", "HEAD"]);
    let symbolic_head_before = git(&workspace.primary_path, &["symbolic-ref", "HEAD"]);
    let status_before = git(
        &workspace.primary_path,
        &["status", "--porcelain", "--untracked-files=all"],
    );
    let unapproved_ref = format!("refs/heads/orchestrator/review/{}", ids[0]);
    let unapproved_request = DeliveryRequest {
        step_id: "delivery-without-approval",
        task_id: &ids[0],
        candidate_sha: &candidates[0].sha,
        base_sha: candidates[0].worktree.base_sha(),
        approval_id: "missing-approval",
        verification_run_id: run_ids[0],
        parameter_digest: digests[0],
        worktree_path: candidates[0].worktree.path(),
    };
    let unapproved = with_store_callbacks(&store, |callbacks| {
        delivery::local_review(
            &workspace.repository,
            unapproved_request,
            &unapproved_ref,
            callbacks,
        )
    })
    .expect_err("delivery without matching approval must refuse");
    assert!(matches!(unapproved, DeliveryError::Approval(_)));
    let refs_after_refusal = reference_values(&workspace.primary_path);
    assert_eq!(refs_after_refusal, refs_before_delivery);
    assert!(store.get_approval("missing-approval").is_err());

    for (index, candidate) in candidates.iter().enumerate() {
        let target_ref = format!("refs/heads/orchestrator/review/{}", ids[index]);
        let step_id = format!("delivery-review-{index}");
        let approval_id = format!("approval-{}", ids[index]);
        let request = DeliveryRequest {
            step_id: &step_id,
            task_id: &ids[index],
            candidate_sha: &candidate.sha,
            base_sha: candidate.worktree.base_sha(),
            approval_id: &approval_id,
            verification_run_id: run_ids[index],
            parameter_digest: digests[index],
            worktree_path: candidate.worktree.path(),
        };
        let delivered = with_store_callbacks(&store, |callbacks| {
            delivery::local_review(&workspace.repository, request, &target_ref, callbacks)
        })
        .expect("deliver matching candidate to local review");
        assert_eq!(delivered.outcome, DeliveryOutcome::ReviewRefCreated);
    }

    let delivery_before_restart: BTreeMap<_, _> = ids
        .iter()
        .map(|task_id| {
            let steps = store
                .delivery_steps_for_task(task_id)
                .expect("read delivery reports");
            let events = steps
                .iter()
                .map(|step| {
                    let step_id = step["id"].as_str().expect("delivery step id");
                    (
                        step_id.to_owned(),
                        store
                            .delivery_events(step_id)
                            .expect("read delivery audit event"),
                    )
                })
                .collect::<Vec<_>>();
            (task_id.clone(), (steps, events))
        })
        .collect();
    let delivered_records = store.list().expect("read final task records");
    store = restart_store(store, &workspace.database_path);
    assert_eq!(
        store
            .list()
            .expect("read task records after delivery restart"),
        delivered_records
    );
    for task_id in &ids {
        let (expected_steps, expected_events) = &delivery_before_restart[task_id];
        let reopened_steps = store
            .delivery_steps_for_task(task_id)
            .expect("read delivery reports after restart");
        assert_eq!(&reopened_steps, expected_steps);
        for (step_id, events) in expected_events {
            assert_eq!(
                store
                    .delivery_events(step_id)
                    .expect("read delivery event after restart"),
                *events
            );
        }
    }

    let refs_after_delivery = reference_values(&workspace.primary_path);
    let main_after = git(&workspace.primary_path, &["rev-parse", "refs/heads/main"]);
    let primary_head_after = git(&workspace.primary_path, &["rev-parse", "HEAD"]);
    let symbolic_head_after = git(&workspace.primary_path, &["symbolic-ref", "HEAD"]);
    let status_after = git(
        &workspace.primary_path,
        &["status", "--porcelain", "--untracked-files=all"],
    );
    let tasks = ids
        .iter()
        .enumerate()
        .map(|(index, task_id)| {
            let verification_rows = store
                .verification_results_for_task(task_id)
                .expect("read verification after final restart");
            assert_eq!(verification_rows.len(), 1);
            let verification = verification_rows
                .into_iter()
                .next()
                .expect("verification exists");
            assert_eq!(verification.0.status, "passed");
            assert_eq!(verification.0.commit, candidates[index].sha);

            let approval_id = format!("approval-{task_id}");
            let approval = store
                .get_approval(&approval_id)
                .expect("read approval after final restart");
            assert!(matches!(
                approval.state,
                orchestrator_core::approval::ApprovalState::Approved { .. }
            ));
            assert_eq!(
                approval.action.kind(),
                ApprovalActionKind::Delivery(ActionKind::LocalReviewMerge)
            );
            assert_eq!(
                approval.action.parameter_digest(),
                &ParameterDigest::from_sha256(digests[index])
            );
            assert_eq!(
                approval.action.candidate_commit().as_str(),
                candidates[index].sha
            );
            let approval_events = store
                .approval_events(&approval_id)
                .expect("read approval history after restart");
            assert_eq!(approval_events, approvals_before_restart[index].1);

            let delivery_steps = store
                .delivery_steps_for_task(task_id)
                .expect("read final delivery reports");
            let success = delivery_steps
                .iter()
                .find(|step| step["outcome"] == "review_ref_created")
                .expect("each task has a review report");
            assert_eq!(success["approval_id"].as_str(), Some(approval_id.as_str()));
            assert_eq!(
                success["verification_run_id"].as_str(),
                Some(run_ids[index])
            );
            assert_eq!(
                success["candidate_sha"].as_str(),
                Some(candidates[index].sha.as_str())
            );
            assert!(store
                .delivery_verification_matches(task_id, run_ids[index], &candidates[index].sha,)
                .expect("revalidate final verification"));
            assert_eq!(
                store.check_delivery_approval(
                    &approval_id,
                    task_id,
                    ActionKind::LocalReviewMerge,
                    ParameterDigest::from_sha256(digests[index]),
                    &candidates[index].sha,
                ),
                Ok(())
            );

            let (_, expected_events) = &delivery_before_restart[task_id];
            TaskEvidence {
                id: task_id.clone(),
                candidate_sha: candidates[index].sha.clone(),
                verification,
                approval,
                approval_events,
                delivery_steps,
                delivery_events: expected_events.clone(),
            }
        })
        .collect();
    drop(store);

    WorkflowEvidence {
        tasks,
        refs_before_delivery,
        refs_after_refusal,
        refs_after_delivery,
        main_before,
        main_after,
        primary_head_before,
        primary_head_after,
        symbolic_head_before,
        symbolic_head_after,
        status_before,
        status_after,
    }
}

#[test]
fn workflow_records_survive_reopen_after_each_stage() {
    let evidence = run_restarting_workflow();

    assert_eq!(evidence.tasks.len(), 3);
    for task in &evidence.tasks {
        assert_eq!(task.verification.0.task_id, task.id);
        assert_eq!(task.verification.0.commit, task.candidate_sha);
        assert_eq!(task.verification.0.status, "passed");
        assert_eq!(task.verification.1["details"]["status"], "passed");
        assert!(matches!(
            task.approval.state,
            orchestrator_core::approval::ApprovalState::Approved { .. }
        ));
        assert_eq!(task.approval.action.task_id(), task.id);
        assert_eq!(
            task.approval.action.candidate_commit().as_str(),
            task.candidate_sha
        );
        assert_eq!(task.approval_events.len(), 2);
        assert!(task
            .delivery_steps
            .iter()
            .any(|step| step["outcome"] == "review_ref_created"));
        assert_eq!(task.delivery_events.len(), task.delivery_steps.len());
    }
    let refused = evidence.tasks[0]
        .delivery_steps
        .iter()
        .find(|step| step["outcome"] == "refused")
        .expect("unapproved delivery refusal was retained");
    assert_eq!(refused["approval_id"], "missing-approval");
}

#[test]
fn local_review_writes_only_approved_review_refs_without_merging() {
    let evidence = run_restarting_workflow();

    assert_eq!(evidence.refs_after_refusal, evidence.refs_before_delivery);
    assert_eq!(evidence.main_after, evidence.main_before);
    assert_eq!(evidence.primary_head_after, evidence.primary_head_before);
    assert_eq!(evidence.symbolic_head_after, evidence.symbolic_head_before);
    assert_eq!(evidence.status_after, evidence.status_before);
    assert!(evidence
        .refs_before_delivery
        .iter()
        .all(|(name, object)| { evidence.refs_after_delivery.get(name) == Some(object) }));
    assert_eq!(
        evidence.refs_before_delivery.get("refs/heads/main"),
        Some(&evidence.main_before)
    );

    let expected_reviews: BTreeMap<_, _> = evidence
        .tasks
        .iter()
        .map(|task| {
            (
                format!("refs/heads/orchestrator/review/{}", task.id),
                task.candidate_sha.clone(),
            )
        })
        .collect();
    let written_refs: BTreeMap<_, _> = evidence
        .refs_after_delivery
        .iter()
        .filter(|(name, object)| evidence.refs_before_delivery.get(*name) != Some(*object))
        .map(|(name, object)| (name.clone(), object.clone()))
        .collect();
    assert!(written_refs
        .keys()
        .all(|name| name.starts_with("refs/heads/orchestrator/review/")));
    assert_eq!(written_refs, expected_reviews);

    let refused = evidence.tasks[0]
        .delivery_steps
        .iter()
        .find(|step| step["outcome"] == "refused")
        .expect("unapproved attempt was persisted as refused");
    assert_eq!(refused["approval_id"], "missing-approval");
    assert!(!evidence.refs_after_refusal.contains_key(&format!(
        "refs/heads/orchestrator/review/{}",
        evidence.tasks[0].id
    )));
    for task in &evidence.tasks {
        let approved_report = task
            .delivery_steps
            .iter()
            .find(|step| step["outcome"] == "review_ref_created")
            .expect("approved candidate has delivery report");
        assert_eq!(
            approved_report["approval_id"].as_str(),
            Some(task.approval.id.as_str())
        );
        assert_eq!(
            approved_report["candidate_sha"].as_str(),
            Some(task.candidate_sha.as_str())
        );
    }
}
