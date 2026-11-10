//! Seeds reviewable tasks into a daemon data directory.
//!
//! Public RPC cannot create a task with a recorded workspace and a passed
//! verification run, so end-to-end tests write them into the store of a daemon
//! that is not running yet and start the real daemon over the same data.
//! Shared by the `seed_review` example (desktop e2e) and the daemon process
//! tests, so both prove the review flow against identically shaped records.
//! Plain std and workspace crates only: it compiles and runs on Unix and Windows.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::{Project, VerificationRun, Workspace};
use orchestrator_core::task::TaskEvent;
use orchestrator_git::{create_task_worktree, Repository};
use orchestrator_store::{SqliteStore, Store};
use serde_json::{json, Value};

pub const PROJECT_ID: &str = "e2e-project";

/// Runs git with identity, signing and line-ending config pinned so a host's
/// global or system config (Git for Windows defaults to `core.autocrlf=true`)
/// cannot change what is committed.
pub fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(path)
        .args([
            "-c",
            "user.name=e2e",
            "-c",
            "user.email=e2e@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.autocrlf=false",
        ])
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Creates a repository on `main` with one commit.
pub fn init_repo(path: &Path) {
    std::fs::create_dir_all(path).expect("repo dir");
    git(path, &["init", "-q", "-b", "main"]);
    git(path, &["config", "core.autocrlf", "false"]);
    std::fs::write(path.join("README.md"), "fixture repository\n").expect("write");
    git(path, &["add", "README.md"]);
    git(path, &["commit", "-q", "-m", "init"]);
}

/// Seeds one task in `Succeeded` with a worktree holding one candidate commit
/// and a passed verification run for it. Succeeded is irreversible, so the
/// daemon's executor never picks the task up. Returns the seeded identifiers
/// as one JSON object.
pub fn seed_review(data_dir: &Path, repo: &Path, worktrees: &Path, task_id: &str) -> Value {
    let repository = Repository::new(repo).expect("repository");
    let base = repository.resolve_base_ref("HEAD").expect("base");
    let worktree = create_task_worktree(&repository, task_id, "main", worktrees).expect("worktree");
    let worktree_path: PathBuf = worktree.path().to_path_buf();
    std::fs::write(worktree_path.join("candidate.txt"), "candidate\n").expect("write");
    git(&worktree_path, &["add", "candidate.txt"]);
    git(&worktree_path, &["commit", "-m", "candidate"]);
    let candidate = git(&worktree_path, &["rev-parse", "HEAD"]);

    let store = SqliteStore::new(&data_dir.join("store.sqlite")).expect("store");
    if store.get_project(PROJECT_ID).is_err() {
        store
            .register_project(&Project {
                id: PROJECT_ID.into(),
                root_path: repo.to_string_lossy().into_owned(),
                settings: json!({}),
            })
            .expect("project");
    }
    store
        .create(&TaskContract {
            id: task_id.into(),
            project_id: PROJECT_ID.into(),
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
        .expect("task");
    for event in [
        TaskEvent::Admit,
        TaskEvent::ExecutionStarted,
        TaskEvent::CandidateReady,
        TaskEvent::VerificationOutcome { passed: true },
    ] {
        store.transition(task_id, event).expect("transition");
    }
    let workspace_id = format!("{task_id}-workspace");
    store
        .add_workspace(&Workspace {
            id: workspace_id.clone(),
            task_id: task_id.into(),
            path: worktree_path.to_string_lossy().into_owned(),
            branch: worktree.branch().into(),
            base_commit: base.clone(),
        })
        .expect("workspace");
    let run_id = format!("{task_id}-verification");
    store
        .add_verification_run(&VerificationRun {
            id: run_id.clone(),
            task_id: task_id.into(),
            commit: candidate.clone(),
            status: "passed".into(),
            log_path: None,
        })
        .expect("verification");

    json!({
        "project_id": PROJECT_ID,
        "task_id": task_id,
        "workspace_id": workspace_id,
        "verification_run_id": run_id,
        "base_commit": base,
        "candidate_commit": candidate,
        "worktree": worktree_path,
    })
}
