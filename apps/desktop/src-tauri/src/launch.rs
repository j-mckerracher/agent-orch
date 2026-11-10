//! Daemon launch for the desktop host. The daemon is an independent process:
//! the host starts it on demand, detached, and never stops it. Closing the
//! app, or the host exiting, must not end agent work.

use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

pub const DAEMON_LOG: &str = "daemon.log";

/// Resolves the data directory the same way the daemon does, plus an
/// `AGENT_ORCH_DATA_DIR` override. The host always passes the resolved path to
/// the daemon it starts, so both sides agree.
pub fn data_dir() -> Option<PathBuf> {
    if let Some(configured) = std::env::var_os("AGENT_ORCH_DATA_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(configured));
    }
    #[cfg(windows)]
    let base =
        std::env::var_os("LOCALAPPDATA").map(|local| PathBuf::from(local).join("agent-orch"));
    #[cfg(not(windows))]
    let base =
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share/agent-orch"));
    base
}

/// Finds the daemon executable: the bundled sidecar next to the app
/// executable. Debug builds also honor `AGENT_ORCH_DAEMON_BIN`; release builds
/// never run a path taken from the environment.
pub fn daemon_program() -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        if let Some(path) = std::env::var_os("AGENT_ORCH_DAEMON_BIN").filter(|v| !v.is_empty()) {
            return Some(PathBuf::from(path));
        }
    }
    let name = if cfg!(windows) {
        "orchestrator-daemon.exe"
    } else {
        "orchestrator-daemon"
    };
    let candidate = std::env::current_exe().ok()?.parent()?.join(name);
    candidate.is_file().then_some(candidate)
}

/// Starts the daemon detached from this process, with its output appended to
/// `<data dir>/daemon.log`.
pub fn spawn(program: &Path, data_dir: &Path) -> std::io::Result<Child> {
    create_private_dir(data_dir)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(data_dir.join(DAEMON_LOG))?;
    let mut command = Command::new(program);
    command
        .arg("--data-dir")
        .arg(data_dir)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    detach(&mut command);
    command.spawn()
}

fn create_private_dir(path: &Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path)
}

#[cfg(unix)]
fn detach(command: &mut Command) {
    std::os::unix::process::CommandExt::process_group(command, 0);
}

#[cfg(windows)]
fn detach(command: &mut Command) {
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    std::os::windows::process::CommandExt::creation_flags(
        command,
        DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP,
    );
}
