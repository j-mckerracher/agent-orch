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
//! - Windows: the child is assigned to a Job Object created with
//!   `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; closing the handle kills the
//!   whole tree (a Toolhelp-snapshot kill of the target and its
//!   descendants is the fallback when no job is attached).
//!
//! Event order: every stdout/stderr line is delivered before the single
//! `Exited` event. The supervisor reports process facts (lines, exit
//! code, cancellation flag); it never interprets them — a nonzero or
//! signal-killed exit is reported, not translated into task success.

use std::path::PathBuf;
#[cfg(unix)]
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::mpsc;

#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(windows)]
use windows_sys::Win32::Foundation::HANDLE;

/// One line of output or the child's exit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisionEvent {
    /// A line from the child's stdout.
    StdoutLine(String),
    /// A line from the child's stderr.
    StderrLine(String),
    /// The child process has exited and both stream readers have drained.
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
    #[error("event stream ended without an exit event")]
    NoExit,
}

#[cfg(unix)]
const CANCEL_GRACE: Duration = Duration::from_millis(500);

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
            .stderr(std::process::Stdio::piped());

        #[cfg(unix)]
        // Own process group: PGID 0 means "use the child's own PID as the
        // group ID" (setpgid(0, 0)), so the whole descendant tree can be
        // signalled with one negative PID. This is the pattern verified by
        // the M0-02 feasibility probe.
        cmd.process_group(0);

        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

        let mut child = cmd
            .spawn()
            .map_err(|source| SupervisorError::Launch {
                program: request.program.clone(),
                source,
            })?;
        let pid = child
            .id()
            .expect("a just-spawned child always has a PID on this platform");

        let (tx, rx) = mpsc::channel::<SupervisionEvent>(64);
        let stdout = child.stdout.take().expect("stdout piped at launch");
        let stderr = child.stderr.take().expect("stderr piped at launch");
        let stdout_task = spawn_line_reader(stdout, tx.clone(), SupervisionEvent::StdoutLine);
        let stderr_task = spawn_line_reader(stderr, tx.clone(), SupervisionEvent::StderrLine);

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
            // Report the exit only after both streams have reached EOF, so
            // every captured line precedes `Exited`. A detached descendant
            // holding a pipe open would stall the readers; the bound
            // releases the exit event in that case (the line-reader tasks
            // keep running until their pipes close).
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                async {
                    let _ = stdout_task.await;
                    let _ = stderr_task.await;
                },
            )
            .await;
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
            #[cfg(windows)]
            job: JobHandle(windows_impl::assign_to_job(pid)),
        })
    }

    /// The child's PID (the process-group leader on Unix).
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// The next stream line or exit event, or `None` once the stream is
    /// exhausted (after the exit event).
    pub async fn next_event(&mut self) -> Option<SupervisionEvent> {
        let ev = self.rx.recv().await?;
        if matches!(ev, SupervisionEvent::Exited { .. }) && self.cancelled {
            return Some(SupervisionEvent::Exited {
                code: ev_code(&ev),
                cancelled: true,
            });
        }
        Some(ev)
    }

    /// Consume the stream until the child has exited and return its exit
    /// event. Lines delivered along the way are discarded; use
    /// `next_event` when they are needed. A bounded deadline is a caller
    /// concern; this method itself cannot hang while the tree is alive.
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

    /// Kill the child and its descendants. Idempotent. The exit event
    /// still arrives afterwards, with `cancelled = true`.
    pub async fn cancel(&mut self) {
        if self.cancelled {
            return;
        }
        self.cancelled = true;
        let pid = self.pid;
        #[cfg(windows)]
        match std::mem::take(&mut self.job.0) {
            Some(job) => windows_impl::kill_tree_via_job(pid, job),
            None => windows_impl::kill_tree(pid),
        }
        #[cfg(unix)]
        {
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
        let pid = self.pid;
        #[cfg(windows)]
        match std::mem::take(&mut self.job.0) {
            Some(job) => windows_impl::kill_tree_via_job(pid, job),
            None => windows_impl::kill_tree(pid),
        }
        #[cfg(unix)]
        {
            let _ = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
        }
    }
}

fn ev_code(ev: &SupervisionEvent) -> Option<i32> {
    match ev {
        SupervisionEvent::Exited { code, .. } => *code,
        _ => None,
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
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx.send(map(line)).await.is_err() {
                break;
            }
        }
    })
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
        r == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
}

#[cfg(windows)]
mod windows_impl;
