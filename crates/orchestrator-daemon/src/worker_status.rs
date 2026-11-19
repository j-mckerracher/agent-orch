//! The worker status file of one attempt, and the daemon's tail of it (DPES-1108 phase 4).
//!
//! The file lives in the data directory, outside the worktree: `task-status/<task>/<attempt
//! number>/status`. The daemon creates the directory owner-only and the file empty before the
//! agent starts, names it in the brief, and grants the agent that one directory. It reads the
//! file about once a second while the run lives and again at exit, through the pure rules of
//! `orchestrator_core::worker_status`. Every accepted line is redacted before it is stored;
//! the text of a refused line is never logged. The file is opened without following a link at
//! its last component and only a regular file is read, at most
//! `worker_status::MAX_FILE_BYTES` of it.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use orchestrator_core::question::MAX_QUESTION_BYTES;
use orchestrator_core::worker_status::{read_lines, StatusLine, MAX_FILE_BYTES, STATUS_FILE_NAME};
use orchestrator_store::SqliteStore;

use super::{bounded_text, Job, LOG};

/// Under the data directory, one directory per task and attempt.
pub(crate) const STATUS_DIR: &str = "task-status";
/// How often a live run's file is read.
const READ_EVERY: Duration = Duration::from_secs(1);

/// The status directory of attempt `number` of `task_id`.
pub(crate) fn status_dir(data_dir: &Path, task_id: &str, number: u32) -> PathBuf {
    data_dir
        .join(STATUS_DIR)
        .join(task_id)
        .join(number.to_string())
}

/// Creates the owner-only status directory and its empty file. An existing file is kept: a
/// resume of the same attempt appends to it.
pub(crate) fn prepare(dir: &Path) -> std::io::Result<PathBuf> {
    orchestrator_store::create_private_dir(dir)?;
    let file = dir.join(STATUS_FILE_NAME);
    orchestrator_store::create_private_file(&file)?;
    Ok(file)
}

/// Reads at most [`MAX_FILE_BYTES`] of a regular file, refusing a link at the last component.
fn read_regular(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, libc::O_NOFOLLOW);
    #[cfg(not(unix))]
    if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(std::io::Error::other("the status file is a link"));
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other(
            "the status file is not a regular file",
        ));
    }
    let mut content = Vec::new();
    file.take(MAX_FILE_BYTES).read_to_end(&mut content)?;
    Ok(content)
}

/// The tail of one session's run. `lines` holds what this session reported, for settling it
/// at exit; earlier sessions of the attempt were read from the line after the stored last one.
pub(super) struct StatusTail {
    path: PathBuf,
    after: u32,
    read_at: Option<Instant>,
    unreadable_logged: bool,
    pub(super) lines: Vec<(u32, StatusLine)>,
    /// The agent CLI's own session id, once known: chosen at launch for Claude and Copilot,
    /// reported by Codex's `thread.started`.
    pub(super) agent_session_id: Option<String>,
}

impl StatusTail {
    pub(super) fn new(path: PathBuf, after: u32, agent_session_id: Option<String>) -> Self {
        Self {
            path,
            after,
            read_at: None,
            unreadable_logged: false,
            lines: Vec::new(),
            agent_session_id,
        }
    }

    /// Reads new lines when a second has passed since the last read.
    pub(super) fn poll(&mut self, store: &SqliteStore, job: &Job) {
        if self.read_at.is_some_and(|at| at.elapsed() < READ_EVERY) {
            return;
        }
        self.read_at = Some(Instant::now());
        self.read(store, job, false);
    }

    /// Reads everything left once the run has exited, a final line without a line break
    /// included, within the per-attempt limit.
    pub(super) fn drain(&mut self, store: &SqliteStore, job: &Job) {
        loop {
            let before = self.after;
            self.read(store, job, true);
            if self.after == before {
                return;
            }
        }
    }

    fn read(&mut self, store: &SqliteStore, job: &Job, last: bool) {
        let content = match read_regular(&self.path) {
            Ok(content) => content,
            Err(error) => {
                if !self.unreadable_logged {
                    self.unreadable_logged = true;
                    job.diag.warn(
                        LOG,
                        "status_file_unreadable",
                        &job.event_fields(&[("error", &error.to_string())]),
                    );
                }
                return;
            }
        };
        let read = read_lines(&content, self.after, last);
        self.after = read.through;
        let mut accepted = Vec::new();
        for (number, parsed) in read.lines {
            match parsed {
                Ok(line) => accepted.push((number, redacted(line))),
                Err(error) => job.diag.warn(
                    LOG,
                    "status_line_refused",
                    &job.event_fields(&[("line", &number.to_string()), ("reason", error.code())]),
                ),
            }
        }
        if accepted.is_empty() {
            return;
        }
        if store
            .record_status_events(&job.task_id, &job.attempt_id, &job.session_id, &accepted)
            .is_err()
        {
            job.diag
                .error(LOG, "status_events_not_recorded", &job.event_fields(&[]));
        }
        self.lines.extend(accepted);
    }
}

/// A line with its note redacted the way other worker text is, and cut to the question limit.
/// A note that redacts to nothing visible is dropped, except a question, which keeps the
/// marker the redaction left.
fn redacted(mut line: StatusLine) -> StatusLine {
    line.note = line
        .note
        .map(|note| bounded_text(&note, MAX_QUESTION_BYTES))
        .filter(|note| !note.trim().is_empty());
    line
}
