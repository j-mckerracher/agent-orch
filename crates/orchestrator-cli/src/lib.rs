//! Standalone task CLI modules. Run orchestration stays in this process and
//! writes task-owned data only to the configured data directory and isolated
//! worktree. Updated: 2026-11-07T22:31:40-07:00.

pub mod cli;
pub mod workflow;
