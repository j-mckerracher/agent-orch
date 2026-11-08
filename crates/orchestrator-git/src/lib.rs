//! System-git-based repository validation and task worktree creation
//! (docs/technical-implementation-plan.md section 8).
//!
//! Every implementation task runs in its own worktree created from a
//! resolved base commit. The primary checkout is never modified: no branch
//! switch, no hard reset, no deletion of uncommitted work. This crate is
//! creation-only: on failure the partially created branch and worktree are
//! left in place for inspection and recovery, reported by
//! [`Error::CreationFailed`]. Nothing in this crate deletes worktrees,
//! branches, or files. A worktree is source isolation, not a security
//! sandbox.
//!
//! Diff capture and delivery are not implemented yet; their modules arrive
//! with the delivery milestone.

mod error;
mod git;
mod repository;
pub mod verification;
mod worktree;
pub mod reconcile;

pub use error::Error;
pub use repository::Repository;
pub use worktree::{create_task_worktree, TaskWorktree};
