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

mod error;
mod git;
mod repository;
pub mod verification;
mod worktree;
pub mod reconcile;
pub mod delivery;
pub mod delivery_reconcile;

pub use error::Error;
pub use repository::Repository;
pub use worktree::{create_task_worktree, TaskWorktree};
