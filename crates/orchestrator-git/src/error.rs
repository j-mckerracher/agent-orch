use std::path::PathBuf;

use thiserror::Error;

/// Failures of repository validation and task worktree creation.
///
/// This crate is creation-only: on failure the partially created branch
/// and worktree directory are left in place (reported by the error) for
/// inspection and recovery. Nothing in this crate deletes worktrees,
/// branches, or files.
#[derive(Debug, Error)]
pub enum Error {
    #[error("not a usable Git work tree at {path}: {detail}")]
    NotARepository { path: PathBuf, detail: String },

    #[error("base ref {refspec:?} does not resolve to a commit: {detail}")]
    InvalidBaseRef { refspec: String, detail: String },

    #[error("task id is empty after sanitizing; a branch and worktree name is required")]
    EmptyTaskId,

    #[error("no free branch/worktree name after {attempts} attempts starting at {base:?}")]
    NameExhausted { base: String, attempts: u32 },

    #[error("worktrees root {root:?} is inside the repository work tree {toplevel:?}; it must live outside every work tree")]
    WorktreesRootInsideRepository { root: PathBuf, toplevel: PathBuf },

    #[error("worktree creation failed at {path} on branch {branch}; the partially created state was left in place for inspection")]
    CreationFailed {
        path: PathBuf,
        branch: String,
        #[source]
        source: Box<Error>,
    },

    #[error("git {step} exited with {status}: {stderr}")]
    Git {
        step: &'static str,
        status: std::process::ExitStatus,
        stderr: String,
    },

    #[error("worktree HEAD mismatch at {path}: expected {expected}, found {found}")]
    HeadMismatch { path: PathBuf, expected: String, found: String },

    #[error("failed to run git: {0}")]
    Io(#[from] std::io::Error),
}

/// Collapse git's stderr to its first line; git's fatal lines are single-line.
pub(super) fn first_line(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

impl Error {
    pub(crate) fn not_a_repository(path: PathBuf, detail: String) -> Self {
        Self::NotARepository { path, detail }
    }
}
