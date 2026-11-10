//! Deterministic supervision of one child process.
//!
//! Launch rules: argument arrays only (never a shell), explicit working
//! directory, stdin null by default, stdout and stderr captured as line
//! streams.
//! Stdin ownership contract updated 2026-11-07T22:32:53-07:00. `launch_with_stdin`
//! returns the supervisor and an owned writer. The writer owns `ChildStdin`;
//! the reaper shares only an atomic exit flag with it. Writes hold no lock
//! across awaits, so cancellation remains independent. Closing the writer
//! drops `ChildStdin` and signals EOF. Writes after close or observed exit
//! return `SupervisorError`.
//! Existing `LaunchRequest` struct literals stay valid, so piped mode uses a
//! separate constructor.
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
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
#[cfg(unix)]
use std::time::Duration;

use crate::reconcile::{capture_process_identity, ProcessIdentity};
use orchestrator_git::env_policy::EnvPolicy;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

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
    /// Extra parent environment variable names to forward, for example a
    /// provider's API key. The child receives only the baseline allowlist,
    /// these names, and the user's `AGENT_ORCH_ENV_PASSTHROUGH` list; see
    /// `orchestrator_git::env_policy`. Names only, never values.
    pub env_names: Vec<String>,
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
    #[error("child stdin is closed")]
    StdinClosed,
    #[error("child process has exited")]
    ChildExited,
    #[error("failed to write to child stdin: {source}")]
    StdinWrite {
        #[source]
        source: std::io::Error,
    },
    #[error("piped stdin was unavailable after launch")]
    StdinUnavailable,
}

/// The sole asynchronous writer for one supervised child's stdin.
pub struct StdinWriter {
    stdin: Option<tokio::process::ChildStdin>,
    child_exited: Arc<AtomicBool>,
}

impl StdinWriter {
    /// Write bytes and flush them before returning.
    pub async fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), SupervisorError> {
        self.write_parts(bytes, None).await
    }

    /// Write a UTF-8 line with a trailing newline and flush it.
    pub async fn write_line(&mut self, line: &str) -> Result<(), SupervisorError> {
        self.write_parts(line.as_bytes(), Some(b"\n")).await
    }

    /// Close stdin and signal EOF. Repeated calls have no effect.
    pub fn close(&mut self) {
        drop(self.stdin.take());
    }

    async fn write_parts(
        &mut self,
        bytes: &[u8],
        suffix: Option<&[u8]>,
    ) -> Result<(), SupervisorError> {
        let child_exited = &self.child_exited;
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(SupervisorError::StdinClosed);
        };
        if child_exited.load(Ordering::Acquire) {
            return Err(SupervisorError::ChildExited);
        }
        if let Err(source) = stdin.write_all(bytes).await {
            return Err(map_stdin_write_error(child_exited, source));
        }
        if let Some(suffix) = suffix {
            if let Err(source) = stdin.write_all(suffix).await {
                return Err(map_stdin_write_error(child_exited, source));
            }
        }
        if let Err(source) = stdin.flush().await {
            return Err(map_stdin_write_error(child_exited, source));
        }
        if child_exited.load(Ordering::Acquire) {
            return Err(SupervisorError::ChildExited);
        }
        Ok(())
    }
}

fn map_stdin_write_error(child_exited: &AtomicBool, source: std::io::Error) -> SupervisorError {
    if child_exited.load(Ordering::Acquire) {
        SupervisorError::ChildExited
    } else {
        SupervisorError::StdinWrite { source }
    }
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
    identity: Option<ProcessIdentity>,
    cancelled: bool,
    exited: bool,
    /// Windows: Job Object handle owning the whole descendant tree;
    /// closing it (on cancellation or drop) kills the tree.
    #[cfg(windows)]
    job: JobHandle,
}

impl ChildSupervisor {
    /// Launch and supervise `request` with null stdin.
    pub fn launch(request: &LaunchRequest) -> Result<Self, SupervisorError> {
        Self::launch_inner(request, std::process::Stdio::null()).map(|(supervisor, _)| supervisor)
    }

    /// Launch and supervise `request` with piped stdin.
    ///
    /// Returns the supervisor and the sole writer for child stdin. Move the
    /// writer to a separate task to write while this supervisor handles
    /// events or cancellation.
    pub fn launch_with_stdin(
        request: &LaunchRequest,
    ) -> Result<(Self, StdinWriter), SupervisorError> {
        let (supervisor, stdin) = Self::launch_inner(request, std::process::Stdio::piped())?;
        match stdin {
            Some(stdin) => Ok((supervisor, stdin)),
            None => Err(SupervisorError::StdinUnavailable),
        }
    }

    fn launch_inner(
        request: &LaunchRequest,
        stdin: std::process::Stdio,
    ) -> Result<(Self, Option<StdinWriter>), SupervisorError> {
        let mut cmd = tokio::process::Command::new(&request.program);
        cmd.args(&request.args)
            .current_dir(&request.working_dir)
            .stdin(stdin)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .env_clear()
            .envs(EnvPolicy::for_launch(&request.env_names).from_process_env());

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
        let identity = capture_process_identity(pid).ok();

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
        let (stdin_writer, child_exited) = match child.stdin.take() {
            Some(stdin) => {
                let child_exited = Arc::new(AtomicBool::new(false));
                (
                    Some(StdinWriter {
                        stdin: Some(stdin),
                        child_exited: Arc::clone(&child_exited),
                    }),
                    Some(child_exited),
                )
            }
            None => (None, None),
        };
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
            if let Some(child_exited) = child_exited {
                child_exited.store(true, Ordering::Release);
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

        Ok((
            Self {
                rx,
                pid,
                identity,
                cancelled: false,
                exited: false,
                #[cfg(windows)]
                job,
            },
            stdin_writer,
        ))
    }

    /// The child's PID (the process-group leader on Unix).
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Returns launch identity when platform metadata capture succeeds.
    /// Windows identity capture is deferred; this accessor never re-queries PID.
    ///
    /// Updated: 2026-11-08T08:17:57+00:00.
    pub fn identity(&self) -> Option<ProcessIdentity> {
        self.identity
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
        while let Ok(Some(line)) = read_bounded_line(&mut reader).await {
            if tx.send(map(line)).await.is_err() {
                break;
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

/// Unix: false only when the group led by `pid` provably no longer exists.
///
/// `kill(-pgid, 0)` sends no signal. Only ESRCH proves the group is gone. EPERM
/// means a live group this user cannot signal, and any other error is
/// inconclusive, so both count as alive: callers use `false` to release or
/// mark a session exited, so uncertainty must fail closed.
#[cfg(unix)]
pub fn process_group_alive(pid: u32) -> bool {
    let probe_ok = unsafe { libc::kill(-(pid as i32), 0) } == 0;
    group_probe_means_alive(probe_ok, std::io::Error::last_os_error().raw_os_error())
}

#[cfg(unix)]
fn group_probe_means_alive(probe_ok: bool, errno: Option<i32>) -> bool {
    probe_ok || errno != Some(libc::ESRCH)
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn request(program: &str) -> LaunchRequest {
        LaunchRequest {
            program: PathBuf::from(program),
            args: Vec::new(),
            working_dir: std::env::current_dir().expect("current directory"),
            env_names: Vec::new(),
        }
    }

    #[test]
    fn group_probe_only_esrch_means_dead() {
        assert!(group_probe_means_alive(true, None));
        assert!(group_probe_means_alive(false, Some(libc::EPERM)));
        assert!(group_probe_means_alive(false, Some(libc::EINVAL)));
        assert!(group_probe_means_alive(false, None));
        assert!(!group_probe_means_alive(false, Some(libc::ESRCH)));
    }

    #[test]
    fn unsignalable_live_group_is_alive() {
        // A live group we cannot signal (typically root-owned) probes as EPERM.
        // Skip when this host exposes none, for example when running as root.
        let group = (2..4096u32).find(|group| {
            let rc = unsafe { libc::kill(-(*group as i32), 0) };
            rc != 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
        });
        let Some(group) = group else {
            eprintln!("skipped: no unsignalable process group on this host");
            return;
        };
        assert!(process_group_alive(group));
    }

    #[tokio::test]
    async fn launch_keeps_stdin_null_by_default() {
        let mut supervisor = ChildSupervisor::launch(&request("/bin/cat")).expect("launch");
        let exit = tokio::time::timeout(std::time::Duration::from_secs(5), supervisor.wait())
            .await
            .expect("default child exit")
            .expect("exit");
        assert!(matches!(
            exit,
            SupervisionEvent::Exited {
                code: Some(0),
                cancelled: false
            }
        ));
    }

    #[tokio::test]
    async fn piped_stdin_round_trips_line_and_close_signals_eof() {
        let (mut supervisor, mut stdin) =
            ChildSupervisor::launch_with_stdin(&request("/bin/cat")).expect("launch");
        stdin.write_line("hello").await.expect("write line");
        let echoed_line =
            tokio::time::timeout(std::time::Duration::from_secs(5), supervisor.next_event())
                .await
                .expect("echo before close");
        assert!(matches!(
            echoed_line,
            Some(SupervisionEvent::StdoutLine(line)) if line == "hello"
        ));
        stdin.close();
        stdin.close();
        assert!(matches!(
            stdin.write_bytes(b"late").await,
            Err(SupervisorError::StdinClosed)
        ));
        let exit = tokio::time::timeout(std::time::Duration::from_secs(5), supervisor.wait())
            .await
            .expect("child exit after EOF")
            .expect("exit");
        assert!(matches!(
            exit,
            SupervisionEvent::Exited {
                code: Some(0),
                cancelled: false
            }
        ));
    }

    #[tokio::test]
    async fn write_after_exit_returns_typed_error_without_overriding_status() {
        let mut request = request("/usr/bin/awk");
        request.args.push("{ exit 1 }".into());
        let (mut supervisor, mut stdin) =
            ChildSupervisor::launch_with_stdin(&request).expect("launch");
        stdin.write_line("hello").await.expect("write line");
        assert!(matches!(
            supervisor.wait().await.expect("exit"),
            SupervisionEvent::Exited {
                code: Some(1),
                cancelled: false
            }
        ));
        assert!(matches!(
            stdin.write_bytes(b"late").await,
            Err(SupervisorError::ChildExited)
        ));
    }

    #[tokio::test]
    async fn cancel_reaps_child_while_stdin_write_is_pending() {
        let mut request = request("/bin/sleep");
        request.args.push("60".into());
        let (mut supervisor, mut stdin) =
            ChildSupervisor::launch_with_stdin(&request).expect("launch");
        let pid = supervisor.pid();
        let payload = vec![b'x'; 8 * 1024 * 1024];
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let write_task = tokio::spawn(async move {
            started_tx.send(()).expect("signal writer start");
            stdin.write_bytes(&payload).await
        });
        started_rx.await.expect("writer start");

        supervisor.cancel().await;

        let write_result = tokio::time::timeout(std::time::Duration::from_secs(5), write_task)
            .await
            .expect("stdin write stopped after cancellation")
            .expect("writer task");
        assert!(matches!(
            write_result,
            Err(SupervisorError::ChildExited | SupervisorError::StdinWrite { .. })
        ));
        let exit = tokio::time::timeout(std::time::Duration::from_secs(5), supervisor.wait())
            .await
            .expect("exit event")
            .expect("exit");
        assert!(matches!(
            exit,
            SupervisionEvent::Exited {
                cancelled: true,
                ..
            }
        ));
        assert!(!is_process_running(pid));
    }
}

#[cfg(windows)]
mod windows_impl;
