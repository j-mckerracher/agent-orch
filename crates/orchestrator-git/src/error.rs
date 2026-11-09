use std::path::PathBuf;

use thiserror::Error;

/// Failures of repository validation, task worktree creation, and scoped diffs.
///
/// Worktree creation is conservative: partially created Git state remains for
/// inspection. Diff operations accept only paths scoped to the validated
/// repository and never invoke external diff or text-conversion tools.
///
/// Updated: 2026-11-08T17:01:05+00:00.
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

    #[error("worktree creation failed at {path} on branch {branch}; the partially created state was left in place for inspection: {source}")]
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
    HeadMismatch {
        path: PathBuf,
        expected: String,
        found: String,
    },
    #[error("diff base must be a full Git object id")]
    InvalidDiffBase,

    #[error("diff path is outside the task worktree")]
    DiffPathOutsideWorktree,

    #[error("diff path is invalid")]
    InvalidDiffPath,

    #[error("failed to run git: {0}")]
    Io(#[from] std::io::Error),
}

/// Summarize git's stderr on one line: every non-empty line, in order, joined
/// by `; ` and capped at `MAX_STDERR_CHARS`. Git often prints a progress line
/// (for example `Preparing worktree ...`) before the `fatal:` line that holds
/// the cause, so keeping only the first line hides the cause.
pub(super) fn stderr_detail(stderr: &[u8]) -> String {
    const MAX_STDERR_CHARS: usize = 1024;
    let text = String::from_utf8_lossy(stderr);
    let joined = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    match joined.char_indices().nth(MAX_STDERR_CHARS) {
        Some((end, _)) => format!("{}...", &joined[..end]),
        None => joined,
    }
}

impl Error {
    pub(crate) fn not_a_repository(path: PathBuf, detail: String) -> Self {
        Self::NotARepository { path, detail }
    }
}
