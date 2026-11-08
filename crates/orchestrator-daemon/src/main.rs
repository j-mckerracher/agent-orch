//! On-demand daemon entry point for authenticated local task APIs. Updated: 2026-11-08T17:34:31+00:00.
//! It accepts `--data-dir`. Its default is `$HOME/.local/share/agent-orch` on
//! Unix and `%LOCALAPPDATA%/agent-orch` on Windows. If the platform variable is
//! unset, it uses `<current-dir>/.agent-orch`. Acquire the data lock before
//! creating IPC token or socket.
mod api;
mod idempotency;

use api::DaemonApi;
use orchestrator_ipc::{Server, ServerConfig};
use orchestrator_store::DataDirectoryLock;
use std::path::PathBuf;
use std::sync::Arc;

fn main() {
    if let Err(error) = run() {
        eprintln!("orchestrator-daemon: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let data_dir = data_dir()?;
    let _lock = acquire_lock(&data_dir)?;
    let handler = Arc::new(DaemonApi::new(&data_dir)?);
    Server::serve_with_handler(
        ServerConfig::new(data_dir, env!("CARGO_PKG_VERSION")),
        handler,
    )
    .map_err(|error| format!("daemon IPC failed: {error}"))
}

fn acquire_lock(data_dir: &std::path::Path) -> Result<DataDirectoryLock, String> {
    DataDirectoryLock::acquire(data_dir)
        .map_err(|error| format!("cannot acquire data-directory lock: {error}"))
}

fn data_dir() -> Result<PathBuf, String> {
    let mut args = std::env::args_os().skip(1);
    let mut configured = None;
    while let Some(arg) = args.next() {
        if arg == "--data-dir" {
            if configured.is_some() {
                return Err("--data-dir may be provided only once".into());
            }
            configured = Some(PathBuf::from(
                args.next().ok_or("--data-dir requires a path")?,
            ));
        } else {
            return Err("usage: orchestrator-daemon [--data-dir PATH]".into());
        }
    }
    if let Some(path) = configured {
        return Ok(path);
    }
    #[cfg(windows)]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return Ok(PathBuf::from(local).join("agent-orch"));
    }
    #[cfg(not(windows))]
    if let Some(home) = std::env::var_os("HOME") {
        return Ok(PathBuf::from(home).join(".local/share/agent-orch"));
    }
    Ok(std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".agent-orch"))
}

#[cfg(test)]
mod tests {
    use super::acquire_lock;
    use orchestrator_store::{DataDirectoryLock, DataDirectoryLockError};

    #[test]
    fn lock_conflict_reports_live_holder_pid() {
        let dir = tempfile::tempdir().unwrap();
        let _owner = DataDirectoryLock::acquire(dir.path()).unwrap();
        let error = match acquire_lock(dir.path()) {
            Err(error) => error,
            Ok(lock) => {
                drop(lock);
                panic!("second daemon unexpectedly acquired data lock");
            }
        };
        let holder_pid = std::process::id();
        assert!(error.contains(&holder_pid.to_string()));
        assert!(matches!(
            DataDirectoryLock::acquire(dir.path()),
            Err(DataDirectoryLockError::AlreadyHeld { holder_pid: pid }) if pid == holder_pid
        ));
    }
}
