//! Helpers the CLI integration tests share. Treehouse places a task's worktree in a
//! pool slot, so a test finds it through the workspace its run recorded instead of
//! assuming a path, and checks a cleanup by whether the slot was returned.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use orchestrator_store::SqliteStore;
use serde_json::Value;

/// The worktree the task's workspace records.
pub fn leased_worktree(data_dir: &Path, task_id: &str) -> PathBuf {
    let store = SqliteStore::new(&data_dir.join("store.sqlite")).expect("open task store");
    store
        .list_workspaces()
        .expect("list workspaces")
        .into_iter()
        .find(|workspace| workspace.task_id == task_id)
        .map(|workspace| PathBuf::from(workspace.path))
        .unwrap_or_else(|| panic!("task {task_id} has no recorded workspace"))
}

/// Whether treehouse lists the slot at `worktree` as available, so a later lease can reuse it.
/// Treehouse reads the pool of the repository it runs in, so `repo` is its working directory.
pub fn returned_to_pool(data_dir: &Path, repo: &Path, worktree: &Path) -> bool {
    let program = std::env::var_os("AGENT_ORCH_TREEHOUSE_PROGRAM")
        .expect("AGENT_ORCH_TREEHOUSE_PROGRAM must name the treehouse binary");
    let output = Command::new(program)
        .current_dir(repo)
        .env("HOME", data_dir.join("treehouse-home"))
        .env("USERPROFILE", data_dir.join("treehouse-home"))
        .env("TREEHOUSE_NO_UPDATE_CHECK", "1")
        .arg("--root")
        .arg(data_dir.join("worktrees"))
        .args(["status", "--json"])
        .output()
        .expect("run treehouse status");
    let slots: Value = serde_json::from_slice(&output.stdout).expect("treehouse status JSON");
    let path = worktree.to_string_lossy();
    slots
        .as_array()
        .into_iter()
        .flatten()
        .any(|slot| slot["path"] == path.as_ref() && slot["status"] == "available")
}
