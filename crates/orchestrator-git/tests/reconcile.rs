use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use orchestrator_git::reconcile::{
    reconcile_delivery_steps, reconcile_workspaces, DeliveryOutcome, DeliveryStep,
    RecordedWorkspace, WorkspaceClassification,
};
use orchestrator_git::{create_task_worktree, Repository};
use tempfile::TempDir;

struct Fixture {
    _directory: TempDir,
    repository_path: PathBuf,
    worktrees_path: PathBuf,
    repository: Repository,
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().expect("temporary Git fixture");
    let repository_path = directory.path().join("repository");
    fs::create_dir_all(&repository_path).expect("create repository");
    git(&repository_path, &["init", "-b", "main"]);
    git(
        &repository_path,
        &["config", "user.email", "test@example.invalid"],
    );
    git(&repository_path, &["config", "user.name", "Test"]);
    fs::write(repository_path.join("base.txt"), "base\n").expect("write initial file");
    git(&repository_path, &["add", "base.txt"]);
    git(&repository_path, &["commit", "-m", "initial"]);
    let repository = Repository::new(&repository_path).expect("validated repository");
    Fixture {
        worktrees_path: directory.path().join("worktrees"),
        _directory: directory,
        repository_path,
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
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn foreign_workspace_is_reported_and_left_untouched() {
    let fixture = fixture();
    let path = fixture.worktrees_path.join("foreign");
    fs::create_dir_all(&path).expect("create foreign directory");
    fs::write(path.join("keep.txt"), "keep\n").expect("write foreign data");
    let recorded = [RecordedWorkspace {
        id: "w-foreign".into(),
        path: path.clone(),
        branch: "orchestrator/foreign".into(),
        base_commit: git(&fixture.repository_path, &["rev-parse", "HEAD"]),
    }];

    let plan = reconcile_workspaces(&fixture.repository, &recorded).expect("reconcile workspace");

    assert_eq!(
        plan.workspaces[0].classification,
        WorkspaceClassification::Foreign
    );
    assert_eq!(plan.foreign_paths, [fs::canonicalize(&path).unwrap()]);
    assert!(
        path.join("keep.txt").exists(),
        "reconciliation must not remove foreign data"
    );
}

#[test]
fn unchanged_worktree_matches_recorded_branch_and_base() {
    let fixture = fixture();
    let worktree = create_task_worktree(
        &fixture.repository,
        "matching-worktree",
        "HEAD",
        &fixture.worktrees_path,
    )
    .expect("create real worktree");
    let recorded = [RecordedWorkspace {
        id: "w-matching".into(),
        path: worktree.path().to_path_buf(),
        branch: worktree.branch().to_string(),
        base_commit: worktree.base_sha().to_string(),
    }];

    let plan = reconcile_workspaces(&fixture.repository, &recorded).expect("reconcile workspace");

    assert_eq!(
        plan.workspaces[0].classification,
        WorkspaceClassification::PresentAndMatching
    );
}

#[test]
fn unrecorded_worktree_is_reported_as_unexpected_and_left_untouched() {
    let fixture = fixture();
    let worktree = create_task_worktree(
        &fixture.repository,
        "unexpected-worktree",
        "HEAD",
        &fixture.worktrees_path,
    )
    .expect("create real worktree");

    let plan = reconcile_workspaces(&fixture.repository, &[]).expect("reconcile worktrees");

    assert_eq!(
        plan.unexpected_paths,
        [fs::canonicalize(worktree.path()).expect("canonical worktree path")]
    );
    assert!(
        worktree.path().exists(),
        "reconciliation must not remove worktrees"
    );
}

#[test]
fn removed_real_worktree_is_classified_missing() {
    let fixture = fixture();
    let worktree = create_task_worktree(
        &fixture.repository,
        "missing-worktree",
        "HEAD",
        &fixture.worktrees_path,
    )
    .expect("create real worktree");
    let recorded = [RecordedWorkspace {
        id: "w-missing".into(),
        path: worktree.path().to_path_buf(),
        branch: worktree.branch().to_string(),
        base_commit: worktree.base_sha().to_string(),
    }];
    fs::remove_dir_all(worktree.path()).expect("simulate out-of-band directory removal");

    let plan = reconcile_workspaces(&fixture.repository, &recorded).expect("reconcile workspace");

    assert_eq!(
        plan.workspaces[0].classification,
        WorkspaceClassification::Missing
    );
}

#[test]
fn checked_out_branch_mismatch_is_classified_branch_moved() {
    let fixture = fixture();
    let worktree = create_task_worktree(
        &fixture.repository,
        "moved-branch",
        "HEAD",
        &fixture.worktrees_path,
    )
    .expect("create real worktree");
    git(worktree.path(), &["switch", "--detach"]);
    let recorded = [RecordedWorkspace {
        id: "w-moved".into(),
        path: worktree.path().to_path_buf(),
        branch: worktree.branch().to_string(),
        base_commit: worktree.base_sha().to_string(),
    }];

    let plan = reconcile_workspaces(&fixture.repository, &recorded).expect("reconcile workspace");

    assert_eq!(
        plan.workspaces[0].classification,
        WorkspaceClassification::BranchMoved
    );
}

#[test]
fn committed_worktree_branch_is_classified_branch_moved() {
    let fixture = fixture();
    let worktree = create_task_worktree(
        &fixture.repository,
        "committed-worktree",
        "HEAD",
        &fixture.worktrees_path,
    )
    .expect("create real worktree");
    let base_commit = worktree.base_sha().to_string();
    fs::write(worktree.path().join("committed.txt"), "committed\n").expect("write worktree file");
    git(worktree.path(), &["add", "committed.txt"]);
    git(worktree.path(), &["commit", "-m", "worker commit"]);
    let recorded = [RecordedWorkspace {
        id: "w-committed".into(),
        path: worktree.path().to_path_buf(),
        branch: worktree.branch().to_string(),
        base_commit,
    }];

    let plan = reconcile_workspaces(&fixture.repository, &recorded).expect("reconcile workspace");

    assert_eq!(
        plan.workspaces[0].classification,
        WorkspaceClassification::BranchMoved
    );
}

#[test]
fn partial_delivery_refs_are_confirmed_not_applied_or_ambiguous() {
    let fixture = fixture();
    let before = git(&fixture.repository_path, &["rev-parse", "HEAD"]);
    fs::write(fixture.repository_path.join("target.txt"), "target\n").expect("write target");
    git(&fixture.repository_path, &["add", "target.txt"]);
    git(&fixture.repository_path, &["commit", "-m", "target"]);
    let after = git(&fixture.repository_path, &["rev-parse", "HEAD"]);
    git(
        &fixture.repository_path,
        &["branch", "not-applied", &before],
    );
    git(&fixture.repository_path, &["branch", "confirmed", &after]);
    fs::write(fixture.repository_path.join("later.txt"), "later\n").expect("write later");
    git(&fixture.repository_path, &["add", "later.txt"]);
    git(&fixture.repository_path, &["commit", "-m", "later"]);
    let later = git(&fixture.repository_path, &["rev-parse", "HEAD"]);
    git(&fixture.repository_path, &["branch", "ambiguous", &later]);
    let steps = [
        DeliveryStep {
            id: "d".into(),
            reference: "refs/heads/not-created".into(),
            expected_before: None,
            expected_after: after.clone(),
        },
        DeliveryStep {
            id: "a".into(),
            reference: "refs/heads/not-applied".into(),
            expected_before: Some(before.clone()),
            expected_after: after.clone(),
        },
        DeliveryStep {
            id: "b".into(),
            reference: "refs/heads/confirmed".into(),
            expected_before: Some(before.clone()),
            expected_after: after.clone(),
        },
        DeliveryStep {
            id: "c".into(),
            reference: "refs/heads/ambiguous".into(),
            expected_before: Some(before.clone()),
            expected_after: after.clone(),
        },
    ];

    let plan = reconcile_delivery_steps(&fixture.repository, &steps).expect("reconcile refs");

    assert_eq!(plan[0].id, "c");
    assert_eq!(
        plan[0].outcome,
        DeliveryOutcome::Ambiguous {
            actual: Some(later)
        }
    );
    assert_eq!(plan[1].id, "b");
    assert_eq!(plan[1].outcome, DeliveryOutcome::Confirmed);
    assert_eq!(plan[2].id, "a");
    assert_eq!(plan[2].outcome, DeliveryOutcome::NotApplied);
    assert_eq!(plan[3].id, "d");
    assert_eq!(plan[3].outcome, DeliveryOutcome::NotApplied);
}
