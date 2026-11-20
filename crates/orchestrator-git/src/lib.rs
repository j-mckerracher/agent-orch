//! System-Git validation, task worktree creation, and delivery.
//! (docs/technical-implementation-plan.md section 8).
//!
//! Each implementation task runs in its own worktree from a resolved base
//! commit. Delivery never checks out, merges, or deletes a branch or worktree.
//! Report-only delivery leaves no lasting repository changes. Local-review
//! delivery creates only refs under `refs/heads/orchestrator/review/`.
//! Destination locks remove only lock files created by this process.
//! Caller callbacks revalidate approval and verification state and persist
//! delivery outcomes. A task worktree is source isolation, not a security
//! sandbox.
//!
//! Updated: 2026-11-08T09:43:13Z.

mod branch_prefix;
pub mod delivery;
pub mod delivery_reconcile;
pub mod env_policy;
mod error;
mod git;
mod git_path;
pub mod github;
pub mod github_fake;
mod process;
pub mod reconcile;
mod repository;
#[cfg(test)]
mod test_support;
mod treehouse;
pub mod verification;
mod worktree;

pub use branch_prefix::{
    is_safe_branch_name, BranchPrefix, BRANCH_PREFIX_FILE, DEFAULT_BRANCH_PREFIX,
    MAX_BRANCH_NAME_LEN, MAX_BRANCH_PREFIX_LEN,
};
pub use error::Error;
pub use git::{slot_git_command, AGENT_EMAIL, AGENT_NAME, TASK_GIT_GUARD_ENV};
pub use repository::Repository;
pub use treehouse::{Released, Treehouse};
pub use worktree::{
    create_task_worktree, release_task_worktree, slot_attached_to, slot_git_dirs, SlotDirs,
    TaskWorktree,
};
