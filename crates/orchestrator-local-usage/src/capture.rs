//! Runs the reader: argv only (never a shell), an environment built from an
//! allowlist, stdout capped in size, a timeout and cancellation, and a kill
//! that reaches the whole tree. Unix kills the process group. Windows puts the
//! child in a kill-on-close Job Object, so closing it ends any descendants.
//! The window between spawn and job assignment is short but not zero; a reader
//! that forks during that window would survive. codeburn does not fork.

use std::ffi::OsString;
use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(20);
/// How long to wait for the output readers after the child is gone.
const READER_GRACE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    pub timeout: Duration,
    pub max_stdout: u64,
}

#[derive(Debug)]
pub(crate) struct Output {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RunError {
    Spawn,
    Timeout,
    Cancelled,
    TooLarge,
    Wait,
}

/// Runs `program args...` with exactly `env` as its environment.
pub(crate) fn run(
    program: &Path,
    args: &[OsString],
    env: &[(OsString, OsString)],
    limits: Limits,
    cancelled: &dyn Fn() -> bool,
) -> Result<Output, RunError> {
    let mut command = Command::new(program);
    command
        .args(args)
        .env_clear()
        .envs(
            env.iter()
                .map(|(key, value)| (key.as_os_str(), value.as_os_str())),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }

    let mut child = command.spawn().map_err(|_| RunError::Spawn)?;
    let group = Group::adopt(&child);
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        abort(&group, &mut child);
        return Err(RunError::Spawn);
    };

    let too_large = Arc::new(AtomicBool::new(false));
    let (out_tx, out_rx) = mpsc::channel();
    let flag = Arc::clone(&too_large);
    let max = limits.max_stdout;
    thread::spawn(move || {
        let _ = out_tx.send(read_capped(stdout, max, &flag));
    });
    let (err_tx, err_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut stderr = stderr;
        let _ = err_tx.send(io::copy(&mut stderr, &mut io::sink()));
    });

    let deadline = Instant::now() + limits.timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(_) => break Err(RunError::Wait),
        }
        if too_large.load(Ordering::Acquire) {
            break Err(RunError::TooLarge);
        }
        if cancelled() {
            break Err(RunError::Cancelled);
        }
        if Instant::now() >= deadline {
            break Err(RunError::Timeout);
        }
        thread::sleep(POLL);
    };
    let status = match status {
        Ok(status) => status,
        Err(error) => {
            abort(&group, &mut child);
            return Err(error);
        }
    };

    // Anything the reader left behind (a background child holding the pipes) dies now.
    group.kill();
    let stdout = out_rx
        .recv_timeout(READER_GRACE)
        .map_err(|_| RunError::Wait)?;
    let _ = err_rx.recv_timeout(READER_GRACE);
    if too_large.load(Ordering::Acquire) || stdout.len() as u64 > limits.max_stdout {
        return Err(RunError::TooLarge);
    }
    Ok(Output {
        code: status.code(),
        stdout,
    })
}

fn abort(group: &Group, child: &mut Child) {
    group.kill();
    let _ = child.kill();
    let _ = child.wait();
}

/// Reads up to `max + 1` bytes into memory. Past the cap it keeps reading and
/// discarding, so the child never blocks on a full pipe, and raises `too_large`.
fn read_capped(mut reader: impl Read, max: u64, too_large: &AtomicBool) -> Vec<u8> {
    let mut kept = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                let room = (max as usize + 1).saturating_sub(kept.len());
                kept.extend_from_slice(&chunk[..read.min(room)]);
                if kept.len() as u64 > max {
                    too_large.store(true, Ordering::Release);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    kept
}

#[cfg(unix)]
struct Group {
    pid: i32,
}

#[cfg(unix)]
impl Group {
    fn adopt(child: &Child) -> Self {
        Self {
            pid: child.id() as i32,
        }
    }

    /// SIGKILL to every member of the group. Harmless when the group is empty.
    fn kill(&self) {
        // SAFETY: a negative pid only signals the process group this crate
        // created with `process_group(0)`; no memory is touched.
        let _ = unsafe { libc::kill(-self.pid, libc::SIGKILL) };
    }
}

#[cfg(windows)]
struct Group {
    job: Option<windows_sys::Win32::Foundation::HANDLE>,
}

#[cfg(windows)]
impl Group {
    fn adopt(child: &Child) -> Self {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        // SAFETY: plain Win32 calls on a handle we create; every failure path
        // closes what it opened. A missing job falls back to killing the child.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Self { job: None };
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let configured = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            let assigned = configured != 0
                && AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) != 0;
            if !assigned {
                CloseHandle(job);
                return Self { job: None };
            }
            Self { job: Some(job) }
        }
    }

    fn kill(&self) {
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;
        if let Some(job) = self.job {
            // SAFETY: the handle is owned by this value and still open.
            unsafe { TerminateJobObject(job, 1) };
        }
    }
}

#[cfg(windows)]
impl Drop for Group {
    fn drop(&mut self) {
        if let Some(job) = self.job.take() {
            // SAFETY: closing the owned handle; kill-on-close ends any survivors.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reader_stops_at_the_cap_and_flags_it() {
        let flag = AtomicBool::new(false);
        let kept = read_capped(&b"0123456789"[..], 4, &flag);
        assert!(flag.load(Ordering::Acquire));
        assert_eq!(
            kept.len(),
            5,
            "keeps max + 1 so the caller can see it overflowed"
        );
    }

    #[test]
    fn output_under_the_cap_is_kept_whole_and_unflagged() {
        let flag = AtomicBool::new(false);
        let kept = read_capped(&b"abc"[..], 4, &flag);
        assert!(!flag.load(Ordering::Acquire));
        assert_eq!(kept, b"abc");
    }
}
