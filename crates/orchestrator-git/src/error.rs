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

    #[error("diff candidate must be a full Git commit id")]
    InvalidDiffCandidate,

    #[error("diff path is outside the task worktree")]
    DiffPathOutsideWorktree,

    #[error("diff path is invalid")]
    InvalidDiffPath,

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

/// Summarize the whole of git's stderr on one line for `git worktree add`,
/// whose first line is a progress message and whose `fatal:` cause comes
/// last. Non-empty lines are joined by `; `; control characters (terminal
/// escapes) become `?`. Over 1024 characters, the tail is kept behind a
/// visible `...` because the cause is last. No secrets are redacted: this
/// is git's own output for local paths and refs, shown as received.
pub(super) fn stderr_detail(stderr: &[u8]) -> String {
    const MAX_CHARS: usize = 1024;
    const MARK: &str = "...";
    let text = String::from_utf8_lossy(stderr);
    let joined = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            line.chars()
                .map(|c| if c.is_control() { '?' } else { c })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("; ");
    let len = joined.chars().count();
    if len <= MAX_CHARS {
        return joined;
    }
    let start = joined
        .char_indices()
        .nth(len - (MAX_CHARS - MARK.len()))
        .map_or(joined.len(), |(index, _)| index);
    format!("{MARK}{}", &joined[start..])
}

impl Error {
    pub(crate) fn not_a_repository(path: PathBuf, detail: String) -> Self {
        Self::NotARepository { path, detail }
    }
}
