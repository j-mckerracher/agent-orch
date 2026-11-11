//! Spawns the real `orchestrator-daemon` binary for process-level tests and
//! reaches it through the same authenticated local transport every client
//! uses (Unix socket or Windows named pipe). Nothing here depends on how the
//! transport is named on disk, so it runs unchanged on Unix and Windows.
//!
//! Short paths: Windows has a 260-character path limit, so every test root is
//! `<temp>/ao*` with one-letter children.
//!
//! Failure diagnostics: the daemon's stdout and stderr go to `daemon.log`;
//! its tail, and the elapsed time of every call made through `timed!`, are
//! printed when the test panics.

#![allow(dead_code)]

use orchestrator_ipc::{Capability, Client};
use std::cell::RefCell;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Waits for a daemon to open its transport; the first use of a pipe on a
/// loaded Windows host can be slow.
const READY: Duration = Duration::from_secs(30);
const STOP: Duration = Duration::from_secs(15);
/// Read/write timeout of test clients. Review calls run several Git processes
/// in the daemon, which takes seconds on a slow Windows host; the production
/// default (`DEFAULT_IO_TIMEOUT`) is sized for fast calls.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);
const LOG_TAIL_LINES: usize = 60;

thread_local! {
    static TIMINGS: RefCell<Vec<(String, Duration, bool)>> = const { RefCell::new(Vec::new()) };
}

/// Runs one client call and records its label, elapsed time and whether it
/// returned `Ok`, for the failure report.
pub fn timed<T, E>(label: &str, call: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    let started = Instant::now();
    let result = call();
    let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
    TIMINGS.with(|timings| {
        timings
            .borrow_mut()
            .push((label, started.elapsed(), result.is_ok()))
    });
    result
}

fn print_timings() {
    TIMINGS.with(|timings| {
        eprintln!("--- per-call timing (client I/O timeout {CALL_TIMEOUT:?}) ---");
        for (label, elapsed, ok) in timings.borrow().iter() {
            eprintln!(
                "{:>9.1} ms  {}  {label}",
                elapsed.as_secs_f64() * 1000.0,
                if *ok { "ok " } else { "ERR" }
            );
        }
        eprintln!("--- end per-call timing ---");
    });
}

fn log_tail(log: &Path) -> String {
    let text = fs::read_to_string(log).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(LOG_TAIL_LINES)..].join("\n")
}

fn connect_all(data: &Path) -> Result<Client, orchestrator_ipc::IpcError> {
    Client::connect_with_timeout(data, &Capability::ALL, CALL_TIMEOUT)
}

pub struct Layout {
    _root: TempDir,
    pub data: PathBuf,
    pub repo: PathBuf,
    pub worktrees: PathBuf,
    pub log: PathBuf,
}

impl Layout {
    pub fn new() -> Self {
        let root = tempfile::Builder::new().prefix("ao").tempdir().unwrap();
        let data = root.path().join("d");
        fs::create_dir(&data).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&data, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self {
            data,
            repo: root.path().join("r"),
            worktrees: root.path().join("w"),
            log: root.path().join("daemon.log"),
            _root: root,
        }
    }
}

pub struct Daemon {
    child: Child,
    log: PathBuf,
}

impl Daemon {
    /// Starts the daemon over `layout.data` and returns once a client can
    /// authenticate. Panics, with the daemon log, if it exits or is not ready
    /// within `READY`.
    pub fn start(layout: &Layout) -> Self {
        Self::start_with_env(layout, &[])
    }

    /// Like [`Self::start`], with extra environment variables for the daemon.
    pub fn start_with_env(layout: &Layout, env: &[(&str, &str)]) -> Self {
        TIMINGS.with(|timings| timings.borrow_mut().clear());
        let log = File::options()
            .create(true)
            .append(true)
            .open(&layout.log)
            .unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_orchestrator-daemon"))
            .arg("--data-dir")
            .arg(&layout.data)
            .envs(env.iter().copied())
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        let mut daemon = Self {
            child,
            log: layout.log.clone(),
        };
        daemon.wait_ready(&layout.data);
        daemon
    }

    fn wait_ready(&mut self, data: &Path) {
        let deadline = Instant::now() + READY;
        loop {
            match connect_all(data) {
                Ok(_) => return,
                Err(error) => {
                    assert!(
                        self.child.try_wait().unwrap().is_none(),
                        "daemon exited before it was ready ({error:?})"
                    );
                    assert!(
                        Instant::now() < deadline,
                        "daemon not ready within {READY:?}: {error:?}"
                    );
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }

    /// Ends the daemon without giving it a chance to clean up.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    pub fn wait_for_exit(&mut self) {
        let deadline = Instant::now() + STOP;
        while self.child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "daemon did not stop");
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            self.child.try_wait().unwrap().unwrap().success(),
            "daemon exited unsuccessfully"
        );
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if thread::panicking() {
            print_timings();
            eprintln!(
                "--- daemon log tail ({}) ---\n{}\n--- end daemon log ---",
                self.log.display(),
                log_tail(&self.log)
            );
        }
    }
}

/// Connects with every capability and `CALL_TIMEOUT`, retrying briefly.
pub fn connect(data: &Path) -> Client {
    let deadline = Instant::now() + READY;
    loop {
        match connect_all(data) {
            Ok(client) => return client,
            Err(error) => {
                assert!(Instant::now() < deadline, "cannot connect: {error:?}");
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
}
