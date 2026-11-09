//! Seeds one reviewable task into a daemon data directory for end-to-end tests.
//!
//! Public RPC cannot create a task with a recorded workspace and a passed
//! verification run, so the desktop e2e suite runs this against a stopped
//! daemon's `store.sqlite`, then starts the real daemon over the same data.
//!
//! Usage: `seed_review <data-dir> <repo> <worktrees-dir> [task-id]`
//! Prints one JSON object describing the seeded records on stdout.

use std::path::{Path, PathBuf};
use std::process::Command;

use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::{Project, VerificationRun, Workspace};
use orchestrator_core::task::TaskEvent;
use orchestrator_git::{create_task_worktree, Repository};
use orchestrator_store::{SqliteStore, Store};
use serde_json::json;

const PROJECT_ID: &str = "e2e-project";

fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(path)
        .args([
            "-c",
            "user.name=e2e",
            "-c",
            "user.email=e2e@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .expect("git runs");
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data_dir, repo, worktrees, rest @ ..] = args.as_slice() else {
        eprintln!("usage: seed_review <data-dir> <repo> <worktrees-dir> [task-id]");
        std::process::exit(2);
    };
    let task_id = rest.first().map_or("e2e-task", String::as_str);
    let repo_path = PathBuf::from(repo);

    let repository = Repository::new(&repo_path).expect("repository");
    let base = repository.resolve_base_ref("HEAD").expect("base");
    let worktree = create_task_worktree(&repository, task_id, "main", Path::new(worktrees))
        .expect("worktree");
    let worktree_path = worktree.path().to_path_buf();
    std::fs::write(worktree_path.join("candidate.txt"), "candidate\n").expect("write");
    git(&worktree_path, &["add", "candidate.txt"]);
    git(&worktree_path, &["commit", "-m", "candidate"]);
    let candidate = git(&worktree_path, &["rev-parse", "HEAD"]);

    let store = SqliteStore::new(&Path::new(data_dir).join("store.sqlite")).expect("store");
    store
        .register_project(&Project {
            id: PROJECT_ID.into(),
            root_path: repo_path.to_string_lossy().into_owned(),
            settings: json!({}),
        })
        .expect("project");
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
    // Succeeded is irreversible, so the daemon's executor never picks the task up.
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

    println!(
        "{}",
        json!({
            "project_id": PROJECT_ID,
            "task_id": task_id,
            "workspace_id": workspace_id,
            "verification_run_id": run_id,
            "base_commit": base,
            "candidate_commit": candidate,
            "worktree": worktree_path,
        })
    );
}
