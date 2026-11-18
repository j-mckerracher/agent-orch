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
//!
//! Approver: a `Layout` owns the approver secret. The daemon is started with
//! `--approver-stdin` and the secret written to its stdin, which is then
//! closed, exactly as the desktop host starts it. `connect` presents nothing, so
//! it is the session an agent holding only the token would have;
//! `connect_as_approver` is the desktop's.

#![allow(dead_code)]

use orchestrator_ipc::{ApproverSecret, Capability, Client};
use std::cell::RefCell;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
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

/// The last `LOG_TAIL_LINES` lines of the daemon log, redacted.
fn log_tail(log: &Path) -> String {
    let text = orchestrator_diagnostics::tail_redacted(log, 64 * 1024).unwrap_or_default();
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
    /// The daemon's home: it holds no pstack plugin, so the daemon runs without the workflow
    /// agent on every machine, as CI does. Never created.
    pub home: PathBuf,
    approver: Arc<ApproverSecret>,
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
            home: root.path().join("no-home"),
            approver: Arc::new(ApproverSecret::generate().unwrap()),
            _root: root,
        }
    }

    /// The secret every `Daemon::start*` for this layout is given on stdin.
    pub fn approver(&self) -> Arc<ApproverSecret> {
        self.approver.clone()
    }
}

pub struct Daemon {
    child: Child,
    log: PathBuf,
    /// Held open for a daemon started without `--approver-stdin`: it must not
    /// read stdin, so an input that never ends is no reason for it to wait.
    _stdin: Option<ChildStdin>,
}

impl Daemon {
    /// Starts the daemon over `layout.data` and returns once a client can
    /// authenticate. Panics, with the daemon log, if it exits or is not ready
    /// within `READY`.
    pub fn start(layout: &Layout) -> Self {
        Self::start_with_env(layout, &[] as &[(&str, &str)])
    }

    /// Like [`Daemon::start`], with extra environment for the daemon process.
    pub fn start_with_env<V: AsRef<std::ffi::OsStr> + Copy>(
        layout: &Layout,
        env: &[(&str, V)],
    ) -> Self {
        Self::spawn(layout, env, Some(&*layout.approver))
    }

    /// Starts the daemon with `secret` instead of the layout's own, as a host that
    /// started it again would: a restart brings another secret.
    pub fn start_with_secret(layout: &Layout, secret: &ApproverSecret) -> Self {
        Self::spawn(layout, &[] as &[(&str, &str)], Some(secret))
    }

    /// Starts the daemon without `--approver-stdin`, as the CLI or a person at a
    /// terminal does: no session can be an approver.
    pub fn start_without_approver(layout: &Layout) -> Self {
        Self::spawn(layout, &[] as &[(&str, &str)], None)
    }

    fn spawn<V: AsRef<std::ffi::OsStr> + Copy>(
        layout: &Layout,
        env: &[(&str, V)],
        approver: Option<&ApproverSecret>,
    ) -> Self {
        TIMINGS.with(|timings| timings.borrow_mut().clear());
        let log = File::options()
            .create(true)
            .append(true)
            .open(&layout.log)
            .unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_orchestrator-daemon"));
        command.arg("--data-dir").arg(&layout.data);
        if approver.is_some() {
            command.arg("--approver-stdin");
        }
        let mut child = command
            // The real machine load must not decide whether a test task starts (DPES-1105 part 2c).
            .env("AGENT_ORCH_LOAD_GATE", "off")
            .env("HOME", &layout.home)
            .env("USERPROFILE", &layout.home)
            .envs(env.iter().copied())
            .stdin(Stdio::piped())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take();
        if let Some(secret) = approver {
            let pipe = stdin.take().unwrap();
            if let Err(error) = write_secret(pipe, secret) {
                kill_tree(&mut child);
                panic!("cannot give the daemon its approver secret: {error}");
            }
        }
        let mut daemon = Self {
            child,
            log: layout.log.clone(),
            _stdin: stdin,
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
        kill_tree(&mut self.child);
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

/// Writes the secret and closes the pipe: the end of input the daemon reads to.
fn write_secret(mut pipe: ChildStdin, secret: &ApproverSecret) -> std::io::Result<()> {
    pipe.write_all(secret.to_wire().as_bytes())
}

/// Kills the daemon and, on Windows, every process it started. The tree must
/// be walked before the daemon dies: once it is gone its children no longer
/// name it as their parent, and a failed test would leave an agent behind.
fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    if matches!(child.try_wait(), Ok(None)) {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

impl Drop for Daemon {
    fn drop(&mut self) {
        kill_tree(&mut self.child);
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

/// Connects with every capability and `CALL_TIMEOUT` as the approver, the session
/// the desktop host opens, retrying briefly.
pub fn connect_as_approver(layout: &Layout) -> Client {
    let deadline = Instant::now() + READY;
    loop {
        match Client::connect_with_approver(
            &layout.data,
            &Capability::ALL,
            CALL_TIMEOUT,
            Some(layout.approver()),
        ) {
            Ok(client) => {
                assert!(
                    client.is_approver(),
                    "the daemon did not accept the approver"
                );
                return client;
            }
            Err(error) => {
                assert!(Instant::now() < deadline, "cannot connect: {error:?}");
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
}
