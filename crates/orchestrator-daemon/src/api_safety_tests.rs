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
    fs::write(worktree.join("absolute.txt"), "inside worktree").unwrap();
    fs::create_dir(worktree.join("nested")).unwrap();
    fs::write(worktree.join("inside.txt"), "inside worktree").unwrap();
    fs::write(worktree.join("-argument.txt"), "inside worktree").unwrap();
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
    let (_directory, api, worktree, _outside) = invalid_path_fixture();
    assert_eq!(
        diff_error(&api, worktree.join("absolute.txt").to_str().unwrap()),
        Err(RpcError::InvalidParams)
    );
}

#[test]
fn workspace_diff_rejects_parent_component_before_git() {
    let (_directory, api, _worktree, _outside) = invalid_path_fixture();
    assert_eq!(
        diff_error(&api, "nested/../inside.txt"),
        Err(RpcError::InvalidParams)
    );
}

#[test]
fn workspace_diff_rejects_option_prefix_before_git() {
    let (_directory, api, _worktree, _outside) = invalid_path_fixture();
    assert_eq!(
        diff_error(&api, "-argument.txt"),
        Err(RpcError::InvalidParams)
    );
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

#[test]
fn approved_local_review_delivery_creates_the_review_ref_once() {
    use orchestrator_core::approval::{Actor, ApprovalActionKind, ApproverAllowlist};

    let fixture = fixture();
    let api = &fixture.api;
    let workspace = api.store.workspaces_for_task("task-1").unwrap().remove(0);
    let verification = api
        .store
        .verification_runs_for_task("task-1")
        .unwrap()
        .remove(0);
    let target_ref = "refs/heads/orchestrator/review/task-1";
    let destination = Repository::new(&fixture.repository_path)
        .unwrap()
        .path()
        .to_string_lossy()
        .into_owned();
    let digest = delivery_digest(
        DeliveryAction::LocalReview,
        &destination,
        &workspace.base_commit,
        Some(target_ref),
    )
    .unwrap();
    let action = ApprovalAction::new(
        ApprovalActionKind::Delivery(ActionKind::LocalReviewMerge),
        ParameterDigest::from_sha256(digest),
        &verification.commit,
        "task-1",
    )
    .unwrap();
    let allowlist = ApproverAllowlist::new("local-user", Vec::<String>::new()).unwrap();
    api.store
        .request_approval(
            "approval-1",
            &action,
            &Actor::Agent("agent-1".into()),
            &allowlist,
        )
        .unwrap();
    api.dispatch(
        "approvals.resolve",
        json!({
            "approval_id": "approval-1",
            "decision": "approve",
            "idempotency_key": "approve-key-1"
        }),
    )
    .unwrap();

    let params = json!({
        "task_id": "task-1",
        "workspace_id": "workspace-1",
        "approval_id": "approval-1",
        "verification_run_id": "verification-1",
        "action": "local_review",
        "target_ref": target_ref,
        "idempotency_key": "delivery-key-2"
    });
    let delivered = api.dispatch("delivery.request", params.clone()).unwrap();
    assert_eq!(delivered["task_id"], "task-1");
    assert_eq!(delivered["outcome"], "review_ref_created");
    assert_eq!(
        git(&fixture.repository_path, &["rev-parse", target_ref]),
        verification.commit
    );
    assert_eq!(api.dispatch("delivery.request", params).unwrap(), delivered);
    assert_eq!(
        api.store.delivery_steps_for_task("task-1").unwrap().len(),
        1
    );
}

fn submit_with(
    api: &DaemonApi,
    key: &str,
    edit: impl FnOnce(&mut Value),
) -> Result<Value, RpcError> {
    let mut params = json!({
        "idempotency_key": key,
        "contract": {
            "project_id": "project-1",
            "parent_id": null,
            "objective": "Review one candidate",
            "kind": "Implement",
            "agent": "codex",
            "dependencies": [],
            "verification": "None",
            "delivery": "ReportOnly",
            "autonomy": "ApprovalRequired",
            "max_retries": 0,
            "max_repairs": 0
        }
    });
    edit(&mut params["contract"]);
    api.dispatch("tasks.submit", params)
}

#[test]
fn submit_refuses_read_only_autonomy_because_the_agent_would_write() {
    let fixture = fixture();
    let before = fixture.api.store.list().unwrap().len();
    assert_eq!(
        submit_with(&fixture.api, "read-only-key", |c| c["autonomy"] =
            json!("ReadOnly")),
        Err(RpcError::Forbidden)
    );
    assert_eq!(fixture.api.store.list().unwrap().len(), before);
    assert!(submit_with(&fixture.api, "approval-key", |_| {}).is_ok());
}

#[test]
fn submit_rejects_oversized_and_nul_contract_fields() {
    let fixture = fixture();
    let huge = "x".repeat(orchestrator_core::contract::limits::MAX_OBJECTIVE_BYTES + 1);
    assert_eq!(
        submit_with(&fixture.api, "huge-objective", |c| c["objective"] =
            json!(huge)),
        Err(RpcError::InvalidParams)
    );
    assert_eq!(
        submit_with(&fixture.api, "nul-objective", |c| c["objective"] =
            json!("a\u{0}b")),
        Err(RpcError::InvalidParams)
    );
    assert_eq!(
        submit_with(&fixture.api, "nul-command", |c| {
            c["verification"] = json!({"Commands": {"commands": [["cargo", "te\u{0}st"]]}})
        }),
        Err(RpcError::InvalidParams)
    );
    assert_eq!(fixture.api.store.list().unwrap().len(), 1);
}
