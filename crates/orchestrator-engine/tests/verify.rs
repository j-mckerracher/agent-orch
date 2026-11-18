use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicBool;

use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::{Project, Workspace};
use orchestrator_core::task::{Attempt, AttemptState, TaskEvent, TaskState};
use orchestrator_engine::verify::{pending_verifications, verify_attempt, Verified};
use orchestrator_git::{create_task_worktree, BranchPrefix, Repository};
use orchestrator_store::{SqliteStore, Store};
use serde_json::{json, Value};
use tempfile::TempDir;

const TASK: &str = "task-1";
const ATTEMPT: &str = "a1";

/// A task whose candidate is committed in its own worktree.
struct Candidate {
    store: SqliteStore,
    sha: String,
    _root: TempDir,
}

fn candidate(verification: VerificationPolicy) -> Candidate {
    let root = tempfile::tempdir().expect("create isolated directory");
    let primary = root.path().join("primary");
    fs::create_dir_all(&primary).expect("create primary checkout");
    git(&primary, &["init", "--quiet", "-b", "main"]);
    git(&primary, &["config", "user.name", "Engine Test"]);
    git(
        &primary,
        &["config", "user.email", "engine@example.invalid"],
    );
    commit_file(&primary, "base.txt", "base\n", "base");
    let repository = Repository::new(&primary).expect("validate primary repository");
    let worktrees_root = root.path().join("worktrees");
    fs::create_dir_all(&worktrees_root).expect("create worktree root");
    let store = SqliteStore::new(&root.path().join("store.sqlite")).expect("open store");
    create_task(&store, &project(&primary, json!({})), verification);
    let worktree = create_task_worktree(
        &repository,
        TASK,
        "main",
        &worktrees_root,
        &BranchPrefix::default(),
    )
    .expect("create task worktree");
    let sha = commit_file(worktree.path(), "candidate.txt", "candidate\n", "candidate");
    store
        .add_workspace(&Workspace {
            id: "workspace-1".into(),
            task_id: TASK.into(),
            path: worktree.path().display().to_string(),
            branch: worktree.branch().into(),
            base_commit: worktree.base_sha().into(),
        })
        .expect("record workspace");
    Candidate {
        store,
        sha,
        _root: root,
    }
}

/// A task whose project runs in place, so no worktree or commit exists.
fn in_place(verification: VerificationPolicy) -> (TempDir, SqliteStore) {
    let root = tempfile::tempdir().expect("create isolated directory");
    let store = SqliteStore::new(&root.path().join("store.sqlite")).expect("open store");
    create_task(
        &store,
        &project(root.path(), json!({"in_place": true})),
        verification,
    );
    (root, store)
}

fn project(root_path: &Path, settings: Value) -> Project {
    Project {
        id: "project-1".into(),
        root_path: root_path.display().to_string(),
        settings,
    }
}

/// Registers the project, creates the task, and drives it to `Verifying` with
/// a `CandidateReady` attempt.
fn create_task(store: &SqliteStore, project: &Project, verification: VerificationPolicy) {
    store.register_project(project).expect("register project");
    store
        .create(&TaskContract {
            id: TASK.into(),
            project_id: project.id.clone(),
            parent_id: None,
            objective: "verify candidate".into(),
            kind: TaskKind::Implement,
            agent: "test-agent".into(),
            dependencies: Vec::new(),
            verification,
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 1,
            max_repairs: 1,
            model_selection: None,
            permission: Default::default(),
            work_item: None,
        })
        .expect("create task");
    store
        .transition(TASK, TaskEvent::Admit)
        .expect("admit task");
    store
        .add_attempt(
            TASK,
            &Attempt {
                id: ATTEMPT.into(),
                task_id: TASK.into(),
                attempt_number: 1,
                state: AttemptState::Pending,
                failure_reason: None,
            },
        )
        .expect("add attempt");
    store
        .transition(TASK, TaskEvent::ExecutionStarted)
        .expect("start execution");
    store
        .update_attempt_state(TASK, ATTEMPT, AttemptState::Running)
        .expect("run attempt");
    store
        .transition(TASK, TaskEvent::CandidateReady)
        .expect("record candidate");
    store
        .update_attempt_state(TASK, ATTEMPT, AttemptState::CandidateReady)
        .expect("record candidate attempt");
}

/// A contract gate running one Git command, which runs on every platform.
fn git_gate(args: &[&str]) -> VerificationPolicy {
    let mut command = vec!["git".to_owned()];
    command.extend(args.iter().map(|arg| (*arg).to_owned()));
    VerificationPolicy::Commands {
        commands: vec![command],
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
    fs::write(worktree.join(path), contents).expect("write candidate file");
    git(worktree, &["add", "--", path]);
    git(worktree, &["commit", "--quiet", "-m", message]);
    git(worktree, &["rev-parse", "HEAD"])
}

#[test]
fn no_gate_candidate_succeeds_with_gate_none_evidence() {
    let candidate = candidate(VerificationPolicy::None);
    let cancel = AtomicBool::new(false);

    let verified = verify_attempt(&candidate.store, TASK, ATTEMPT, &cancel).expect("verify");

    assert!(matches!(verified, Verified::Passed { .. }), "{verified:?}");
    assert_eq!(
        candidate.store.get(TASK).unwrap().state,
        TaskState::Succeeded
    );
    let runs = candidate.store.verification_results_for_task(TASK).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].0.commit, candidate.sha);
    assert_eq!(runs[0].0.status, "passed");
    assert_eq!(runs[0].1["details"]["gate"], "none");
}

#[test]
fn passing_command_succeeds_the_attempt() {
    let candidate = candidate(git_gate(&["rev-parse", "--verify", "--quiet", "HEAD"]));
    let cancel = AtomicBool::new(false);

    let verified = verify_attempt(&candidate.store, TASK, ATTEMPT, &cancel).expect("verify");

    assert!(matches!(verified, Verified::Passed { .. }), "{verified:?}");
    assert_eq!(
        candidate.store.get(TASK).unwrap().state,
        TaskState::Succeeded
    );
    let runs = candidate.store.verification_results_for_task(TASK).unwrap();
    assert_eq!(runs[0].0.status, "passed");
    assert_eq!(runs[0].1["details"]["gate"], "commands");
    assert_eq!(runs[0].1["details"]["commands"][0]["status"], "Passed");
}

#[test]
fn failing_command_fails_the_attempt_with_its_reason() {
    let candidate = candidate(git_gate(&[
        "rev-parse",
        "--verify",
        "--quiet",
        "refs/heads/missing",
    ]));
    let cancel = AtomicBool::new(false);

    let verified = verify_attempt(&candidate.store, TASK, ATTEMPT, &cancel).expect("verify");

    let Verified::Failed { run_id, reason } = verified else {
        panic!("expected failure, got {verified:?}");
    };
    assert!(run_id.starts_with("a1-verification-"), "{run_id}");
    assert_eq!(
        reason,
        "one or more validation commands failed or were not trusted"
    );
    assert_eq!(
        candidate.store.get(TASK).unwrap().state,
        TaskState::VerificationFailed
    );
    let attempts = candidate.store.attempts_for_task(TASK).unwrap();
    assert_eq!(attempts[0].state, AttemptState::Failed);
    assert_eq!(attempts[0].failure_reason.as_deref(), Some(reason.as_str()));
    let runs = candidate.store.verification_results_for_task(TASK).unwrap();
    assert_eq!(runs[0].0.status, "failed");
}

#[test]
fn cancel_flag_records_nothing_and_reports_interrupted() {
    let candidate = candidate(git_gate(&["rev-parse", "--verify", "--quiet", "HEAD"]));
    let cancel = AtomicBool::new(true);

    let verified = verify_attempt(&candidate.store, TASK, ATTEMPT, &cancel).expect("verify");

    assert_eq!(verified, Verified::Interrupted);
    assert_eq!(
        candidate.store.get(TASK).unwrap().state,
        TaskState::Verifying
    );
    let attempts = candidate.store.attempts_for_task(TASK).unwrap();
    assert_eq!(attempts[0].state, AttemptState::CandidateReady);
    assert!(candidate
        .store
        .verification_results_for_task(TASK)
        .unwrap()
        .is_empty());
}

#[test]
fn cancel_flag_records_nothing_even_when_the_candidate_is_unusable() {
    let root = tempfile::tempdir().expect("create isolated directory");
    let store = SqliteStore::new(&root.path().join("store.sqlite")).expect("open store");
    create_task(
        &store,
        &project(root.path(), json!({})),
        VerificationPolicy::None,
    );
    let cancel = AtomicBool::new(true);

    let verified = verify_attempt(&store, TASK, ATTEMPT, &cancel).expect("verify");

    assert_eq!(verified, Verified::Interrupted);
    assert_eq!(store.get(TASK).unwrap().state, TaskState::Verifying);
    assert!(store
        .verification_results_for_task(TASK)
        .unwrap()
        .is_empty());
}

#[test]
fn unusable_candidate_fails_the_attempt_so_it_can_be_retried() {
    let root = tempfile::tempdir().expect("create isolated directory");
    let store = SqliteStore::new(&root.path().join("store.sqlite")).expect("open store");
    create_task(
        &store,
        &project(root.path(), json!({})),
        VerificationPolicy::None,
    );
    let cancel = AtomicBool::new(false);

    let verified = verify_attempt(&store, TASK, ATTEMPT, &cancel).expect("verify");

    let Verified::Failed { run_id, reason } = verified else {
        panic!("expected failure, got {verified:?}");
    };
    assert!(run_id.starts_with("a1-verification-"), "{run_id}");
    assert_eq!(reason, "task has no recorded workspace");
    assert_eq!(
        store.get(TASK).unwrap().state,
        TaskState::VerificationFailed
    );
    let runs = store.verification_results_for_task(TASK).unwrap();
    assert_eq!(runs[0].0.status, "rejected");
    assert_eq!(
        runs[0].1["details"]["reason"],
        "task has no recorded workspace"
    );
}

#[test]
fn in_place_project_records_a_synthetic_run() {
    let (_root, store) = in_place(VerificationPolicy::None);
    let cancel = AtomicBool::new(false);

    let verified = verify_attempt(&store, TASK, ATTEMPT, &cancel).expect("verify");

    assert!(matches!(verified, Verified::Passed { .. }), "{verified:?}");
    assert_eq!(store.get(TASK).unwrap().state, TaskState::Succeeded);
    let runs = store.verification_results_for_task(TASK).unwrap();
    assert_eq!(runs[0].0.commit, "");
    assert_eq!(runs[0].0.status, "passed");
    assert_eq!(runs[0].1["details"]["gate"], "none");
    assert_eq!(runs[0].1["details"]["reason"], "in_place");
}

#[test]
fn in_place_project_with_a_gate_fails_the_run() {
    let (_root, store) = in_place(git_gate(&["rev-parse", "--verify", "--quiet", "HEAD"]));
    let cancel = AtomicBool::new(false);

    let verified = verify_attempt(&store, TASK, ATTEMPT, &cancel).expect("verify");

    assert!(
        matches!(&verified, Verified::Failed { reason, .. }
            if reason == "verification commands cannot run in an in-place project"),
        "{verified:?}"
    );
    assert_eq!(
        store.get(TASK).unwrap().state,
        TaskState::VerificationFailed
    );
    let runs = store.verification_results_for_task(TASK).unwrap();
    assert_eq!(runs[0].0.status, "rejected");
}

#[test]
fn cancelled_candidate_is_superseded_and_writes_nothing() {
    let candidate = candidate(VerificationPolicy::None);
    candidate
        .store
        .cancel_verifying_task(TASK)
        .expect("cancel verifying task");
    let cancel = AtomicBool::new(false);

    let verified = verify_attempt(&candidate.store, TASK, ATTEMPT, &cancel).expect("verify");

    assert_eq!(verified, Verified::Superseded);
    assert_eq!(
        candidate.store.get(TASK).unwrap().state,
        TaskState::Cancelled
    );
    assert!(candidate
        .store
        .verification_results_for_task(TASK)
        .unwrap()
        .is_empty());
}

#[test]
fn pending_verifications_lists_only_candidates_awaiting_verification() {
    let (_root, store) = in_place(VerificationPolicy::None);
    let cancel = AtomicBool::new(false);
    assert_eq!(
        pending_verifications(&store).expect("list pending"),
        vec![(TASK.to_owned(), ATTEMPT.to_owned())]
    );

    verify_attempt(&store, TASK, ATTEMPT, &cancel).expect("verify");

    assert!(pending_verifications(&store)
        .expect("list pending")
        .is_empty());
}
