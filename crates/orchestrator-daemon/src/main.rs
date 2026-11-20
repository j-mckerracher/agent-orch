//! On-demand daemon entry point for authenticated local task APIs. Updated: 2026-11-08T17:34:31+00:00.
//! It accepts `--data-dir`. Its default is `$HOME/.local/share/agent-orch` on
//! Unix and `%LOCALAPPDATA%/agent-orch` on Windows. If the platform variable is
//! unset, it uses `<current-dir>/.agent-orch`. Acquire the data lock before
//! creating IPC token or socket.
//!
//! It also accepts `--approver-stdin`: the desktop host that started it writes
//! the approver secret to its stdin and closes the pipe. The daemon reads it
//! before it creates anything, so a missing or malformed secret ends the start.
//! Without the flag the daemon never reads stdin and no session can approve.
mod activity;
mod api;
mod executor;
mod idempotency;
// Nothing starts a turn server until the turn wiring lands, so it is dead code until then.
#[allow(dead_code)]
mod mcp_relay;
#[allow(dead_code)]
mod mcp_server;

use api::DaemonApi;
use executor::ExecutorConfig;
use orchestrator_diagnostics::{install_panic_hook, Diagnostics};
use orchestrator_ipc::{ApproverSecret, Server, ServerConfig};
use orchestrator_private_fs::DataDirectoryLock;
use std::ffi::OsString;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

const USAGE: &str = "usage: orchestrator-daemon [--data-dir PATH] [--approver-stdin]";
/// Fixed on purpose: whatever was on stdin must never come back in a log line.
const APPROVER_REJECTED: &str = "approver secret on stdin is missing or malformed";

fn main() {
    // The MCP relay runs before diagnostics, the data lock, the approver stdin and the IPC
    // server: it never reads or locks the data directory and never starts a daemon.
    if std::env::args_os()
        .skip(1)
        .any(|arg| arg == mcp_relay::RELAY_FLAG)
    {
        std::process::exit(mcp_relay::run(std::env::args_os().skip(1)));
    }
    let diagnostics = Diagnostics::stderr();
    install_panic_hook(diagnostics.clone(), "daemon");
    diagnostics.info(
        "daemon",
        "daemon_start",
        &[
            ("version", env!("CARGO_PKG_VERSION")),
            ("pid", &std::process::id().to_string()),
        ],
    );
    if let Err(error) = run() {
        diagnostics.error("daemon", "daemon_failed", &[("error", &error)]);
        std::process::exit(1);
    }
    diagnostics.info("daemon", "daemon_stop", &[("reason", "idle or shutdown")]);
}

fn run() -> Result<(), String> {
    let (configured, approver_stdin) = parse_args(std::env::args_os().skip(1))?;
    // Read before the lock, the token or the socket exist: a missing or malformed
    // secret ends the start with nothing left behind.
    let approver = if approver_stdin {
        let secret = read_approver(&mut std::io::stdin().lock())?;
        orchestrator_diagnostics::register_secret(&secret.to_wire());
        Some(secret)
    } else {
        None
    };
    let data_dir = configured.unwrap_or_else(default_data_dir);
    // The IPC server writes the token after this point; the redactor reads the
    // file lazily, so it masks the token by value as soon as it exists.
    orchestrator_diagnostics::watch_secret_file(data_dir.join("daemon.token"));
    let _lock = acquire_lock(&data_dir)?;
    let config = ExecutorConfig::for_data_dir(&data_dir);
    let handler = Arc::new(DaemonApi::with_executor(&data_dir, config)?);
    handler.start_grant_matcher();
    handler.start_landing();
    let mut server = ServerConfig::new(data_dir, env!("CARGO_PKG_VERSION"));
    if let Some(secret) = approver {
        server = server.with_approver(secret);
    }
    let served = Server::serve_with_handler(server, handler.clone())
        .map_err(|error| format!("daemon IPC failed: {error}"));
    handler.shutdown();
    served
}

fn acquire_lock(data_dir: &std::path::Path) -> Result<DataDirectoryLock, String> {
    DataDirectoryLock::acquire(data_dir)
        .map_err(|error| format!("cannot acquire data-directory lock: {error}"))
}

/// The data directory the command line named, if any, and whether the approver
/// secret arrives on stdin. Each flag may appear once.
fn parse_args(mut args: impl Iterator<Item = OsString>) -> Result<(Option<PathBuf>, bool), String> {
    let mut configured = None;
    let mut approver_stdin = false;
    while let Some(arg) = args.next() {
        if arg == "--data-dir" {
            if configured.is_some() {
                return Err("--data-dir may be provided only once".into());
            }
            configured = Some(PathBuf::from(
                args.next().ok_or("--data-dir requires a path")?,
            ));
        } else if arg == "--approver-stdin" {
            if approver_stdin {
                return Err("--approver-stdin may be provided only once".into());
            }
            approver_stdin = true;
        } else {
            return Err(USAGE.into());
        }
    }
    Ok((configured, approver_stdin))
}

/// Every failure is the same fixed text, so nothing read from stdin is echoed.
fn read_approver(input: &mut impl Read) -> Result<ApproverSecret, String> {
    ApproverSecret::read_from(input).map_err(|_| APPROVER_REJECTED.to_owned())
}

fn default_data_dir() -> PathBuf {
    #[cfg(windows)]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local).join("agent-orch");
    }
    #[cfg(not(windows))]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/share/agent-orch");
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".agent-orch")
}

#[cfg(test)]
mod tests {
    use super::{acquire_lock, parse_args, read_approver, APPROVER_REJECTED, USAGE};
    use orchestrator_private_fs::{DataDirectoryLock, DataDirectoryLockError};
    use std::ffi::OsString;
    use std::path::PathBuf;

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

    #[test]
    fn the_approver_flag_reads_a_secret_and_rejects_bad_input() {
        let valid = "0123456789abcdef".repeat(4);
        let secret = read_approver(&mut valid.as_bytes()).unwrap();
        assert_eq!(secret.to_wire(), valid);
        let with_newline = format!("{valid}\n");
        let secret = read_approver(&mut with_newline.as_bytes()).unwrap();
        assert_eq!(secret.to_wire(), valid);

        for bad in [
            String::new(),
            valid[..63].to_owned(),
            format!("{valid}0"),
            format!("{valid}\n\n"),
            "g".repeat(64),
            valid.to_uppercase(),
            "x".repeat(4096),
        ] {
            let error = read_approver(&mut bad.as_bytes()).expect_err("rejected");
            assert_eq!(error, APPROVER_REJECTED);
            assert!(
                bad.is_empty() || !error.contains(&bad),
                "the input was echoed"
            );
        }
    }

    fn parsed(args: &[&str]) -> Result<(Option<PathBuf>, bool), String> {
        parse_args(args.iter().map(|arg| OsString::from(*arg)))
    }

    #[test]
    fn the_arguments_accept_each_flag_once_and_refuse_anything_else() {
        assert_eq!(parsed(&[]), Ok((None, false)));
        assert_eq!(
            parsed(&["--data-dir", "/d"]),
            Ok((Some(PathBuf::from("/d")), false))
        );
        assert_eq!(parsed(&["--approver-stdin"]), Ok((None, true)));
        assert_eq!(
            parsed(&["--approver-stdin", "--data-dir", "/d"]),
            Ok((Some(PathBuf::from("/d")), true))
        );
        assert_eq!(
            parsed(&["--data-dir", "/d", "--data-dir", "/e"]),
            Err("--data-dir may be provided only once".to_owned())
        );
        assert_eq!(
            parsed(&["--approver-stdin", "--approver-stdin"]),
            Err("--approver-stdin may be provided only once".to_owned())
        );
        assert_eq!(
            parsed(&["--data-dir"]),
            Err("--data-dir requires a path".to_owned())
        );

        assert_eq!(
            USAGE,
            "usage: orchestrator-daemon [--data-dir PATH] [--approver-stdin]"
        );
        let refused: [&[&str]; 4] = [
            &["--approver"],
            &["--approver-stdin=1"],
            &["--secret", "x"],
            &["/d"],
        ];
        for args in refused {
            assert_eq!(parsed(args), Err(USAGE.to_owned()), "{args:?}");
        }
    }
}
