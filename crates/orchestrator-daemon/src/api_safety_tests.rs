use super::super::*;
use orchestrator_core::contract::{AutonomyPolicy, DeliveryPolicy, TaskKind, VerificationPolicy};
use orchestrator_core::records::{Project, VerificationRun, Workspace};
use orchestrator_core::task::TaskEvent;
use orchestrator_git::{create_task_worktree, Repository};
use orchestrator_store::Store;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

struct Fixture {
    _directory: TempDir,
    api: DaemonApi,
    repository_path: PathBuf,
    worktree_path: PathBuf,
}

fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(path)
        .env("GIT_CONFIG_GLOBAL", "/nonexistent-git-global")
        .env("GIT_CONFIG_SYSTEM", "/nonexistent-git-system")
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let repository_path = directory.path().join("repo");
    fs::create_dir_all(&repository_path).unwrap();
    git(&repository_path, &["init", "-b", "main"]);
    git(
        &repository_path,
        &["config", "user.email", "test@example.invalid"],
    );
    git(
        &repository_path,
        &["config", "user.name", "Daemon API Test"],
    );
    fs::write(repository_path.join("base.txt"), "base\n").unwrap();
    git(&repository_path, &["add", "base.txt"]);
    git(&repository_path, &["commit", "-m", "base"]);
    let repository = Repository::new(&repository_path).unwrap();
    let base_sha = repository.resolve_base_ref("HEAD").unwrap();
    let worktree = create_task_worktree(
        &repository,
        "task-1",
        "main",
        &directory.path().join("worktrees"),
    )
    .unwrap();
    let worktree_path = worktree.path().to_path_buf();
    fs::write(worktree_path.join("candidate.txt"), "candidate\n").unwrap();
    git(&worktree_path, &["add", "candidate.txt"]);
    git(&worktree_path, &["commit", "-m", "candidate"]);
    let candidate_sha = Repository::new(&worktree_path)
        .unwrap()
        .resolve_base_ref("HEAD")
        .unwrap();

    let api = DaemonApi::new(&directory.path().join("data")).unwrap();
    api.store
        .register_project(&Project {
            id: "project-1".into(),
            root_path: repository_path.to_string_lossy().into_owned(),
            settings: json!({}),
        })
        .unwrap();
    api.store
        .create(&TaskContract {
            id: "task-1".into(),
            project_id: "project-1".into(),
            parent_id: None,
            objective: "Review one candidate".into(),
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
    api.store.transition("task-1", TaskEvent::Admit).unwrap();
    api.store
        .add_workspace(&Workspace {
            id: "workspace-1".into(),
            task_id: "task-1".into(),
            path: worktree_path.to_string_lossy().into_owned(),
            branch: worktree.branch().into(),
            base_commit: base_sha.clone(),
        })
        .unwrap();
    api.store
        .add_verification_run(&VerificationRun {
            id: "verification-1".into(),
            task_id: "task-1".into(),
            commit: candidate_sha.clone(),
            status: "passed".into(),
            log_path: None,
        })
        .unwrap();
    Fixture {
        _directory: directory,
        api,
        repository_path,
        worktree_path,
    }
}

#[test]
fn workspace_diff_accepts_candidate_path_inside_task_worktree() {
    let fixture = fixture();
    let diff = fixture
        .api
        .dispatch(
            "workspaces.diff",
            json!({
                "task_id": "task-1",
                "workspace_id": "workspace-1",
                "path": "candidate.txt"
            }),
        )
        .unwrap();
    assert!(diff["diff"].as_str().unwrap().contains("+candidate"));
}

fn invalid_path_fixture() -> (TempDir, DaemonApi, PathBuf, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let worktree = directory.path().join("not-a-repository-worktree");
    let outside = directory.path().join("outside");
    fs::create_dir_all(&worktree).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let api = DaemonApi::new(&directory.path().join("data")).unwrap();
    api.store
        .register_project(&Project {
            id: "project-1".into(),
            root_path: directory
                .path()
                .join("not-a-repository-project")
                .to_string_lossy()
                .into_owned(),
            settings: json!({}),
        })
        .unwrap();
    api.store
        .create(&TaskContract {
            id: "task-1".into(),
            project_id: "project-1".into(),
            parent_id: None,
            objective: "Reject unsafe diff paths".into(),
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
    api.store.transition("task-1", TaskEvent::Admit).unwrap();
    api.store
        .add_workspace(&Workspace {
            id: "workspace-1".into(),
            task_id: "task-1".into(),
            path: worktree.to_string_lossy().into_owned(),
            branch: "not-a-branch".into(),
            base_commit: "a".repeat(40),
        })
        .unwrap();
    (directory, api, worktree, outside)
}

fn diff_error(api: &DaemonApi, path: &str) -> Result<Value, RpcError> {
    api.dispatch(
        "workspaces.diff",
        json!({"task_id": "task-1", "workspace_id": "workspace-1", "path": path}),
    )
}

#[test]
fn workspace_diff_rejects_empty_path_before_git() {
    let (_directory, api, _worktree, _outside) = invalid_path_fixture();
    assert_eq!(diff_error(&api, ""), Err(RpcError::InvalidParams));
}

#[test]
fn workspace_diff_rejects_absolute_path_before_git() {
    let (_directory, api, _worktree, outside) = invalid_path_fixture();
    assert_eq!(
        diff_error(&api, outside.join("file.txt").to_str().unwrap()),
        Err(RpcError::InvalidParams)
    );
}

#[test]
fn workspace_diff_rejects_parent_component_before_git() {
    let (_directory, api, _worktree, _outside) = invalid_path_fixture();
    assert_eq!(
        diff_error(&api, "nested/../outside.txt"),
        Err(RpcError::InvalidParams)
    );
}

#[test]
fn workspace_diff_rejects_option_prefix_before_git() {
    let (_directory, api, _worktree, _outside) = invalid_path_fixture();
    assert_eq!(diff_error(&api, "-option"), Err(RpcError::InvalidParams));
}

#[cfg(unix)]
#[test]
fn workspace_diff_rejects_symlink_escape_before_git() {
    let (_directory, api, worktree, outside) = invalid_path_fixture();
    std::os::unix::fs::symlink(outside, worktree.join("escape")).unwrap();
    assert_eq!(diff_error(&api, "escape"), Err(RpcError::InvalidParams));
}

#[test]
fn delivery_refuses_unapproved_local_review_without_changing_refs() {
    let fixture = fixture();
    let before = git(&fixture.repository_path, &["show-ref"]);
    let result = fixture.api.dispatch(
        "delivery.request",
        json!({
            "task_id": "task-1",
            "workspace_id": "workspace-1",
            "approval_id": "missing-approval",
            "verification_run_id": "verification-1",
            "action": "local_review",
            "target_ref": "refs/heads/orchestrator/review/task-1",
            "idempotency_key": "delivery-key-1"
        }),
    );
    assert_eq!(result, Err(RpcError::Forbidden));
    assert_eq!(git(&fixture.repository_path, &["show-ref"]), before);
    let steps = fixture.api.store.delivery_steps_for_task("task-1").unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["outcome"], "refused");
    assert!(!format!("{result:?}").contains(&fixture.worktree_path.to_string_lossy().to_string()));
}
