//! Daemon launch for the desktop host. The daemon is an independent process:
//! the host starts it on demand, detached, and never stops it. Closing the
//! app, or the host exiting, must not end agent work.

use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

pub const DAEMON_LOG: &str = "daemon.log";
const DAEMON_TOKEN: &str = "daemon.token";
const DAEMON_LOG_ROTATED: &str = "daemon.log.1";
/// Largest `daemon.log` carried into a new launch; a bigger one becomes `daemon.log.1`.
pub const DAEMON_LOG_MAX_BYTES: u64 = 4 * 1024 * 1024;

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
/// `<data dir>/daemon.log`. The log is rotated once at launch when it has grown
/// past `DAEMON_LOG_MAX_BYTES`, so it keeps at most about two generations.
pub fn spawn(program: &Path, data_dir: &Path) -> std::io::Result<Child> {
    create_private_dir(data_dir)?;
    rotate_log(data_dir);
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

/// How much of the end of `daemon.log` a launch failure quotes.
const LOG_EXCERPT_BYTES: u64 = 2048;

/// A launch-failure message plus the redacted end of `daemon.log`, so the user
/// sees why the daemon stopped without opening the file. Never quotes the log
/// unredacted: it can hold paths and messages a credential was part of. The
/// message itself is redacted too, since it can embed an error or a path.
pub fn explain_with_log(message: String, data_dir: &Path) -> String {
    watch_token(data_dir);
    let explained =
        match orchestrator_diagnostics::tail_redacted(&data_dir.join(DAEMON_LOG), LOG_EXCERPT_BYTES)
        {
            Ok(tail) if !tail.is_empty() => {
                format!("{message}\nlast daemon output:\n{tail}")
            }
            _ => message,
        };
    orchestrator_diagnostics::redact(&explained)
}

/// Lets the redactor mask the daemon's IPC token by exact value, wherever it
/// shows up in text this host shows or logs.
fn watch_token(data_dir: &Path) {
    orchestrator_diagnostics::watch_secret_file(data_dir.join(DAEMON_TOKEN));
}

/// Best effort: a failed rotation only leaves the log growing as before.
fn rotate_log(data_dir: &Path) {
    let log = data_dir.join(DAEMON_LOG);
    let oversized = fs::metadata(&log).is_ok_and(|meta| meta.len() > DAEMON_LOG_MAX_BYTES);
    if oversized {
        let _ = fs::rename(&log, data_dir.join(DAEMON_LOG_ROTATED));
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn an_oversized_log_is_rotated_and_a_small_one_is_kept() {
        let dir = TempDir::new().unwrap();
        let log = dir.path().join(DAEMON_LOG);
        let rotated = dir.path().join(DAEMON_LOG_ROTATED);

        fs::write(&log, b"small").unwrap();
        rotate_log(dir.path());
        assert!(log.is_file() && !rotated.exists());

        fs::write(&log, vec![b'x'; DAEMON_LOG_MAX_BYTES as usize + 1]).unwrap();
        rotate_log(dir.path());
        assert!(!log.exists());
        assert_eq!(
            fs::metadata(&rotated).unwrap().len(),
            DAEMON_LOG_MAX_BYTES + 1
        );

        rotate_log(dir.path());
        assert!(!log.exists(), "a missing log is not an error");
    }

    #[test]
    fn a_launch_failure_quotes_only_the_redacted_end_of_the_log() {
        let dir = TempDir::new().unwrap();
        let message = || "the daemon exited right after starting".to_owned();
        assert_eq!(explain_with_log(message(), dir.path()), message());

        let mut text = format!("old line token={}\n", "ABCDEF0123456789");
        text.push_str(&"x".repeat(LOG_EXCERPT_BYTES as usize));
        text.push_str("\n{\"event\":\"daemon_failed\",\"error\":\"password=hunter2hunter2\"}\n");
        fs::write(dir.path().join(DAEMON_LOG), text).unwrap();

        let explained = explain_with_log(message(), dir.path());
        assert!(explained.starts_with(&message()));
        assert!(explained.contains("daemon_failed"), "{explained}");
        assert!(!explained.contains("hunter2hunter2"), "{explained}");
        assert!(!explained.contains("old line"), "only the tail is quoted");
        assert!(explained.len() < 3 * LOG_EXCERPT_BYTES as usize);
    }

    #[test]
    fn the_whole_message_is_redacted_and_the_daemon_token_is_masked_exactly() {
        let dir = TempDir::new().unwrap();
        let token = "launchtest-Vx3Qm9Zr5Kb2Wd7Yn1Hs";
        fs::write(dir.path().join(DAEMON_TOKEN), format!("{token}\n")).unwrap();
        // The token is glued to other text in the log and in the message.
        fs::write(
            dir.path().join(DAEMON_LOG),
            format!("{{\"event\":\"daemon_failed\",\"error\":\"bad tok{token}x\"}}\n"),
        )
        .unwrap();

        let explained = explain_with_log(
            format!("cannot start (Authorization: Bearer abcdefghijklmnop, id{token})"),
            dir.path(),
        );
        assert!(!explained.contains(token), "{explained}");
        assert!(!explained.contains("abcdefghijklmnop"), "{explained}");
        assert!(explained.contains("daemon_failed"), "{explained}");
    }
}
