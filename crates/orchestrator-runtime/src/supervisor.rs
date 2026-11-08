//! Deterministic supervision of one child process.
//!
//! Launch rules: argument arrays only (never a shell), explicit working
//! directory, stdin null, stdout and stderr captured as line streams.
//! Cancellation kills the child *and its descendants*:
//!
//! - Unix: the child is spawned with `process_group(0)`, making it the
//!   leader of its own process group, so cancellation signals the whole
//!   tree with one negative PID (SIGTERM, then SIGKILL after a grace
//!   period).
//! - Windows: the child starts suspended, is assigned to a Job Object
//!   created with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, then resumes. Its
//!   descendants inherit job membership; setup fails closed if the tree
//!   cannot be owned before the child runs.
//!
//! Event order: every delivered stdout/stderr line precedes the single
//! `Exited` event. The supervisor reports process facts (lines, exit
//! code, cancellation flag); it never interprets them — a nonzero or
//! signal-killed exit is reported, not translated into task success.

use std::path::PathBuf;
#[cfg(unix)]
use std::time::Duration;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::mpsc;

#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(windows)]
use windows_sys::Win32::Foundation::HANDLE;

/// One line of output or the child's exit. Each line captures at most 64 KiB
/// of raw bytes; oversized lines end with `...[truncated]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisionEvent {
    /// A line from the child's stdout.
    StdoutLine(String),
    /// A line from the child's stderr.
    StderrLine(String),
    /// The child exited, and both stream readers reached EOF or were
    /// stopped after the five-second drain deadline.
    /// `code` is the numeric exit status; `None` when the process was
    /// killed by a signal (Unix) or could not be inspected.
    /// `cancelled` is true if `cancel` was called on this supervisor.
    Exited { code: Option<i32>, cancelled: bool },
}

/// What to launch. Argument array only; the supervisor never invokes a
/// shell and never interpolates text into a command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchRequest {
    /// Executable to run (resolved against PATH, as `std` does; pass an
    /// absolute path to remove that ambiguity).
    pub program: PathBuf,
    /// Argument array passed to the executable.
    pub args: Vec<String>,
    /// Working directory; must exist.
    pub working_dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("launch failed for {program}: {source}")]
    Launch {
        program: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("launched process for {program} exited before its PID could be recorded")]
    ProcessIdUnavailable { program: PathBuf },
    #[error("could not establish process-tree ownership for {program}: {source}")]
    TreeOwnership {
        program: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("event stream ended without an exit event")]
    NoExit,
}

#[cfg(unix)]
const CANCEL_GRACE: Duration = Duration::from_millis(500);

const MAX_LINE_CAPTURE_BYTES: usize = 64 * 1024;
const LINE_TRUNCATION_MARKER: &str = "...[truncated]";

/// Windows Job Object handle. The raw handle is not `Send`; the job is
/// only ever touched from the task that owns the supervisor, so wrap it
/// as `Send` to keep `ChildSupervisor: Send` (required by the adapter
/// contract's `AgentHandle: Send`).
#[cfg(windows)]
struct JobHandle(Option<HANDLE>);

#[cfg(windows)]
unsafe impl Send for JobHandle {}

/// A supervised child process.
pub struct ChildSupervisor {
    rx: mpsc::Receiver<SupervisionEvent>,
    pid: u32,
    cancelled: bool,
    exited: bool,
    /// Windows: Job Object handle owning the whole descendant tree;
    /// closing it (on cancellation or drop) kills the tree.
    #[cfg(windows)]
    job: JobHandle,
}

impl ChildSupervisor {
    /// Launch and supervise `request`.
    pub fn launch(request: &LaunchRequest) -> Result<Self, SupervisorError> {
        let mut cmd = tokio::process::Command::new(&request.program);
        cmd.args(&request.args)
            .current_dir(&request.working_dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        #[cfg(unix)]
        // Own process group: PGID 0 means "use the child's own PID as the
        // group ID" (setpgid(0, 0)), so the whole descendant tree can be
        // signalled with one negative PID. This is the pattern verified by
        // the M0-02 feasibility probe.
        cmd.process_group(0);

        #[cfg(windows)]
        cmd.creation_flags(windows_impl::PROCESS_CREATION_FLAGS);

        let mut child = cmd.spawn().map_err(|source| SupervisorError::Launch {
            program: request.program.clone(),
            source,
        })?;
        let Some(pid) = child.id() else {
            let _ = child.start_kill();
            return Err(SupervisorError::ProcessIdUnavailable {
                program: request.program.clone(),
            });
        };

        #[cfg(windows)]
        let job = {
            let Some(process) = child.raw_handle() else {
                let _ = child.start_kill();
                return Err(SupervisorError::TreeOwnership {
                    program: request.program.clone(),
                    source: std::io::Error::other("launched process has no active handle"),
                });
            };
            match windows_impl::assign_suspended_to_job(pid, process) {
                Ok(job) => JobHandle(Some(job)),
                Err(source) => {
                    let _ = child.start_kill();
                    return Err(SupervisorError::TreeOwnership {
                        program: request.program.clone(),
                        source,
                    });
                }
            }
        };

        let (tx, rx) = mpsc::channel::<SupervisionEvent>(64);
        let stdout = child.stdout.take().expect("stdout piped at launch");
        let stderr = child.stderr.take().expect("stderr piped at launch");
        let mut stdout_task = spawn_line_reader(stdout, tx.clone(), SupervisionEvent::StdoutLine);
        let mut stderr_task = spawn_line_reader(stderr, tx.clone(), SupervisionEvent::StderrLine);

        // Reap the child. `child.wait()` would block until both pipes EOF,
        // which a detached descendant holding a pipe open could stall
        // indefinitely; instead the line readers run concurrently with a
        // `try_wait` poll, and the exit is reported as soon as the child
        // process itself is dead.
        tokio::spawn(async move {
            let mut status: Option<std::process::ExitStatus> = None;
            while status.is_none() {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                match child.try_wait() {
                    Ok(s) => status = s,
                    Err(_) => break,
                }
            }
            // Stop readers that a detached descendant keeps alive before
            // reporting exit, so they cannot send lines after `Exited`.
            let streams_drained = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                let _ = (&mut stdout_task).await;
                let _ = (&mut stderr_task).await;
            })
            .await
            .is_ok();
            if !streams_drained {
                stdout_task.abort();
                stderr_task.abort();
                let _ = stdout_task.await;
                let _ = stderr_task.await;
            }
            let code = status.and_then(|s| s.code());
            let _ = tx
                .send(SupervisionEvent::Exited {
                    code,
                    cancelled: false,
                })
                .await;
        });

        Ok(Self {
            rx,
            pid,
            cancelled: false,
            exited: false,
            #[cfg(windows)]
            job,
        })
    }

    /// The child's PID (the process-group leader on Unix).
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// The next stream line or exit event, or `None` once the stream is
    /// exhausted. Once `Exited` is returned, buffered and later lines are
    /// discarded.
    pub async fn next_event(&mut self) -> Option<SupervisionEvent> {
        if self.exited {
            return None;
        }
        let ev = self.rx.recv().await?;
        match ev {
            SupervisionEvent::Exited { code, .. } => {
                self.exited = true;
                self.rx.close();
                Some(SupervisionEvent::Exited {
                    code,
                    cancelled: self.cancelled,
                })
            }
            event => Some(event),
        }
    }

    /// Consume the stream until the child has exited and return its exit
    /// event. Lines delivered along the way are discarded; use
    /// `next_event` when they are needed. This method has no deadline; add
    /// one at the call site if the child may remain alive indefinitely.
    pub async fn wait(&mut self) -> Result<SupervisionEvent, SupervisorError> {
        while let Some(ev) = self.next_event().await {
            if let SupervisionEvent::Exited { code, .. } = ev {
                return Ok(SupervisionEvent::Exited {
                    code,
                    cancelled: self.cancelled,
                });
            }
        }
        Err(SupervisorError::NoExit)
    }

    /// Kill the child and its descendants. Idempotent. If the exit event has
    /// not been read yet, it arrives with `cancelled = true`.
    pub async fn cancel(&mut self) {
        if self.cancelled {
            return;
        }
        self.cancelled = true;
        #[cfg(windows)]
        match std::mem::take(&mut self.job.0) {
            Some(job) => windows_impl::terminate_job(job),
            None => {}
        }
        #[cfg(unix)]
        {
            let pid = self.pid;
            // The child is its own process-group leader, so the group ID
            // equals its PID.
            let grp = -(pid as i32);
            let _ = unsafe { libc::kill(grp, libc::SIGTERM) };
            tokio::time::sleep(CANCEL_GRACE).await;
            let _ = unsafe { libc::kill(grp, libc::SIGKILL) };
        }
    }
}

impl Drop for ChildSupervisor {
    fn drop(&mut self) {
        // Last-resort cleanup if the caller abandoned the supervisor with
        // a live tree. Same kill path as `cancel`; safe if the tree is
        // already gone (signalling a dead pid/job is a no-op).
        #[cfg(windows)]
        match std::mem::take(&mut self.job.0) {
            Some(job) => windows_impl::terminate_job(job),
            None => {}
        }
        #[cfg(unix)]
        {
            let pid = self.pid;
            let _ = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
        }
    }
}

/// Read one stream to EOF, forwarding each line on `tx`. Returns the
/// reader task so the reap task can await EOF before `Exited`.
fn spawn_line_reader<R: AsyncRead + Unpin + Send + 'static>(
    reader: R,
    tx: mpsc::Sender<SupervisionEvent>,
    map: fn(String) -> SupervisionEvent,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut reader = BufReader::new(reader);
        loop {
            match read_bounded_line(&mut reader).await {
                Ok(Some(line)) => {
                    if tx.send(map(line)).await.is_err() {
                        break;
                    }
                }
                Ok(None) | Err(_) => break,
            }
        }
    })
}

async fn read_bounded_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> std::io::Result<Option<String>> {
    let mut captured = Vec::new();
    let mut truncated = false;
    let mut terminated = false;

    loop {
        let (consumed, ended) = {
            let buffer = reader.fill_buf().await?;
            if buffer.is_empty() {
                if captured.is_empty() && !truncated {
                    return Ok(None);
                }
                break;
            }

            let content_len = buffer.iter().position(|byte| *byte == b'\n');
            let content_len = content_len.unwrap_or(buffer.len());
            let ended = content_len < buffer.len();
            let copy_len = content_len.min(MAX_LINE_CAPTURE_BYTES - captured.len());
            captured.extend_from_slice(&buffer[..copy_len]);
            truncated |= copy_len < content_len;
            (content_len + usize::from(ended), ended)
        };
        reader.consume(consumed);
        terminated = ended;
        if terminated {
            break;
        }
    }

    if terminated && !truncated && captured.last() == Some(&b'\r') {
        captured.pop();
    }
    let mut line = String::from_utf8_lossy(&captured).into_owned();
    if truncated {
        line.push_str(LINE_TRUNCATION_MARKER);
    }
    Ok(Some(line))
}

/// Unix: true if any process in the group led by `pid` is still alive.
#[cfg(unix)]
pub fn process_group_alive(pid: u32) -> bool {
    // kill(-pgid, 0) sends no signal; a dead group yields ESRCH.
    unsafe { libc::kill(-(pid as i32), 0) == 0 }
}

/// Liveness check for tests and recovery code.
pub fn is_process_running(pid: u32) -> bool {
    #[cfg(windows)]
    {
        windows_impl::pid_running(pid)
    }
    #[cfg(not(windows))]
    {
        // kill(pid, 0) sends no signal; success or EPERM means alive.
        let r = unsafe { libc::kill(pid as i32, 0) };
        r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
}

#[cfg(windows)]
mod windows_impl;
