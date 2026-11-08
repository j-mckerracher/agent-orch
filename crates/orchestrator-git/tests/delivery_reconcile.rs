//! Real-Git coverage for read-only delivery-step reconciliation.
//!
//! Updated: 2026-11-08T03:41:55-07:00.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use orchestrator_git::delivery::DeliveryAction;
use orchestrator_git::delivery_reconcile::{
    reconcile_delivery_steps, DeliveryClassification, RecordedDeliveryStep,
};
use orchestrator_git::Repository;
use tempfile::TempDir;

const REVIEW_REF: &str = "refs/heads/orchestrator/review/task-1";

struct Fixture {
    _directory: TempDir,
    path: PathBuf,
    repository: Repository,
    base_sha: String,
    candidate_sha: String,
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().expect("temporary Git directory");
    let path = directory.path().join("repository");
    fs::create_dir_all(&path).expect("create repository directory");
    git(&path, &["init", "-b", "main"]);
    git(&path, &["config", "user.email", "test@example.invalid"]);
    git(&path, &["config", "user.name", "Test"]);
    fs::write(path.join("base.txt"), "base\n").expect("write base file");
    git(&path, &["add", "base.txt"]);
    git(&path, &["commit", "-m", "base"]);
    let base_sha = git(&path, &["rev-parse", "HEAD"]);
    fs::write(path.join("candidate.txt"), "candidate\n").expect("write candidate file");
    git(&path, &["add", "candidate.txt"]);
    git(&path, &["commit", "-m", "candidate"]);
    let candidate_sha = git(&path, &["rev-parse", "HEAD"]);
    let repository = Repository::new(&path).expect("validated repository");

    Fixture {
        _directory: directory,
        path,
        repository,
        base_sha,
        candidate_sha,
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

fn recorded_step(fixture: &Fixture) -> RecordedDeliveryStep {
    RecordedDeliveryStep {
        id: "delivery-1".into(),
        task_id: Some("task-1".into()),
        action: Some(DeliveryAction::LocalReview),
        candidate_sha: Some(fixture.candidate_sha.clone()),
        destination: Some(fixture.repository.path().to_path_buf()),
        target_ref: Some(REVIEW_REF.into()),
    }
}

#[test]
fn review_ref_at_candidate_is_confirmed_and_does_not_block_gate() {
    let fixture = fixture();
    git(
        &fixture.path,
        &["update-ref", REVIEW_REF, fixture.candidate_sha.as_str()],
    );

    let plan = reconcile_delivery_steps(&fixture.repository, &[recorded_step(&fixture)])
        .expect("reconcile delivery step");

    assert_eq!(
        plan.steps[0].classification,
        DeliveryClassification::Confirmed
    );
    assert_eq!(
        plan.steps[0].observed_ref.as_deref(),
        Some(fixture.candidate_sha.as_str())
    );
    assert!(!plan.has_ambiguous_steps());
}

#[test]
fn missing_review_ref_is_not_applied() {
    let fixture = fixture();

    let plan = reconcile_delivery_steps(&fixture.repository, &[recorded_step(&fixture)])
        .expect("reconcile delivery step");

    assert_eq!(
        plan.steps[0].classification,
        DeliveryClassification::NotApplied
    );
    assert_eq!(plan.steps[0].observed_ref, None);
}

#[test]
fn report_only_step_has_no_review_ref_to_apply() {
    let fixture = fixture();
    let mut step = recorded_step(&fixture);
    step.action = Some(DeliveryAction::ReportOnly);
    step.target_ref = None;

    let plan = reconcile_delivery_steps(&fixture.repository, &[step])
        .expect("reconcile report-only delivery step");

    assert_eq!(
        plan.steps[0].classification,
        DeliveryClassification::NotApplied
    );
    assert!(!plan.has_ambiguous_steps());
}

#[test]
fn review_ref_at_an_unrecorded_commit_is_ambiguous_and_blocks_gate() {
    let fixture = fixture();
    git(
        &fixture.path,
        &["update-ref", REVIEW_REF, fixture.base_sha.as_str()],
    );

    let plan = reconcile_delivery_steps(&fixture.repository, &[recorded_step(&fixture)])
        .expect("reconcile delivery step");

    assert_eq!(
        plan.steps[0].classification,
        DeliveryClassification::Ambiguous
    );
    assert_eq!(
        plan.steps[0].observed_ref.as_deref(),
        Some(fixture.base_sha.as_str())
    );
    assert!(plan.has_ambiguous_steps());
}

#[test]
fn incomplete_record_is_ambiguous_and_keeps_observed_ref() {
    let fixture = fixture();
    git(
        &fixture.path,
        &["update-ref", REVIEW_REF, fixture.candidate_sha.as_str()],
    );
    let mut step = recorded_step(&fixture);
    step.candidate_sha = None;

    let plan = reconcile_delivery_steps(&fixture.repository, &[step])
        .expect("reconcile incomplete delivery step");

    assert_eq!(
        plan.steps[0].classification,
        DeliveryClassification::Ambiguous
    );
    assert_eq!(
        plan.steps[0].observed_ref.as_deref(),
        Some(fixture.candidate_sha.as_str())
    );
    assert_eq!(plan.steps[0].record.id, "delivery-1");
}

#[test]
fn reconciliation_does_not_change_git_or_supplied_record_snapshot() {
    let fixture = fixture();
    git(
        &fixture.path,
        &["update-ref", REVIEW_REF, fixture.candidate_sha.as_str()],
    );
    fs::write(fixture.path.join("untracked.txt"), "preserve\n")
        .expect("write untracked worktree data");
    let records = vec![recorded_step(&fixture)];
    let records_before = records.clone();
    let refs_before = git(
        &fixture.path,
        &["for-each-ref", "--format=%(refname) %(objectname)"],
    );
    let worktree_before = working_tree_snapshot(&fixture.path);

    reconcile_delivery_steps(&fixture.repository, &records).expect("reconcile delivery step");

    assert_eq!(
        git(
            &fixture.path,
            &["for-each-ref", "--format=%(refname) %(objectname)"],
        ),
        refs_before
    );
    assert_eq!(working_tree_snapshot(&fixture.path), worktree_before);
    assert_eq!(records, records_before);
}

#[test]
fn plan_order_does_not_depend_on_input_order() {
    let fixture = fixture();
    let mut first = recorded_step(&fixture);
    first.id = "delivery-a".into();
    first.target_ref = Some("refs/heads/orchestrator/review/a".into());
    let mut second = recorded_step(&fixture);
    second.id = "delivery-b".into();
    second.target_ref = Some("refs/heads/orchestrator/review/b".into());

    let forward = reconcile_delivery_steps(&fixture.repository, &[first.clone(), second.clone()])
        .expect("reconcile ordered records");
    let reversed = reconcile_delivery_steps(&fixture.repository, &[second, first])
        .expect("reconcile reversed records");

    assert_eq!(forward, reversed);
    assert_eq!(forward.steps[0].record.id, "delivery-a");
}

#[derive(Debug, PartialEq, Eq)]
struct WorkingTreeSnapshot {
    status: String,
    unstaged_diff: String,
    staged_diff: String,
    files: Vec<(String, Vec<u8>)>,
}

fn working_tree_snapshot(path: &Path) -> WorkingTreeSnapshot {
    let tracked_and_untracked = git(
        path,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
    );
    let files = tracked_and_untracked
        .split('\0')
        .filter(|relative| !relative.is_empty())
        .map(|relative| {
            (
                relative.to_owned(),
                fs::read(path.join(relative)).expect("read working tree file"),
            )
        })
        .collect();

    WorkingTreeSnapshot {
        status: git(path, &["status", "--porcelain", "--untracked-files=all"]),
        unstaged_diff: git(path, &["diff", "--binary"]),
        staged_diff: git(path, &["diff", "--cached", "--binary"]),
        files,
    }
}
