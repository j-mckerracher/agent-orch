//! The `--mcp-relay` mode (DPES-1105 phase 6b): `orchestrator-daemon --mcp-relay --port N`.
//!
//! An agent CLI starts this as its stdio MCP server. It copies stdin lines to the turn's
//! loopback socket and socket lines to stdout, and holds no logic, no store and no data
//! directory: `main` runs it before anything else, so a relay never locks the data directory
//! and never starts a daemon. The per-turn secret arrives in `MCAO_MCP_SECRET` and is removed
//! from the environment once read.

use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub(crate) const RELAY_FLAG: &str = "--mcp-relay";
pub(crate) const PORT_FLAG: &str = "--port";
/// Not an `AGENT_ORCH_` name, which the agent environment policy never forwards.
pub(crate) const SECRET_ENV: &str = "MCAO_MCP_SECRET";
const HELLO_KEY: &str = "mcao_mcp_hello";
const CONNECT_ATTEMPTS: u32 = 3;
const CONNECT_PAUSE: Duration = Duration::from_millis(100);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// Fixed on purpose: nothing about the port, the secret or the cause reaches stderr.
const FAILED: &str = "mcp_relay_failed";

/// The port, when the arguments are exactly `--mcp-relay` and `--port N` (N from 1 to 65535),
/// each once, in either order. Anything else is `None`.
fn parse_args(mut args: impl Iterator<Item = OsString>) -> Option<u16> {
    let (mut relay, mut port) = (false, None);
    while let Some(arg) = args.next() {
        if arg == RELAY_FLAG && !relay {
            relay = true;
        } else if arg == PORT_FLAG && port.is_none() {
            let value: u16 = args.next()?.to_str()?.parse().ok()?;
            if value == 0 {
                return None;
            }
            port = Some(value);
        } else {
            return None;
        }
    }
    port.filter(|_| relay)
}

/// Runs the relay for the process arguments after the program name and returns the exit code:
/// 0 when either side closes, 1 when the arguments, the secret, the connect or the hello fail.
pub(crate) fn run(args: impl Iterator<Item = OsString>) -> i32 {
    let secret = std::env::var(SECRET_ENV).ok();
    std::env::remove_var(SECRET_ENV);
    let (Some(port), Some(secret)) = (parse_args(args), secret) else {
        return failed();
    };
    match relay(port, &secret, io::stdin(), io::stdout()) {
        Some(()) => 0,
        None => failed(),
    }
}

fn failed() -> i32 {
    eprintln!("{FAILED}");
    1
}

fn connect(port: u16) -> Option<TcpStream> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    for attempt in 0..CONNECT_ATTEMPTS {
        if attempt > 0 {
            thread::sleep(CONNECT_PAUSE);
        }
        if let Ok(stream) = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            return Some(stream);
        }
    }
    None
}

/// Connects, sends the hello line, then copies both ways until either side closes. `None`
/// when the connect or the hello fails.
fn relay(
    port: u16,
    secret: &str,
    input: impl Read + Send + 'static,
    output: impl Write + Send + 'static,
) -> Option<()> {
    let stream = connect(port)?;
    let mut writer = stream.try_clone().ok()?;
    let mut hello = serde_json::Map::new();
    hello.insert(HELLO_KEY.to_owned(), secret.into());
    let hello = serde_json::Value::Object(hello).to_string();
    writer
        .write_all(hello.as_bytes())
        .and_then(|()| writer.write_all(b"\n"))
        .and_then(|()| writer.flush())
        .ok()?;
    let (done, closed) = mpsc::channel();
    let up_done = done.clone();
    thread::spawn(move || {
        pump(input, writer);
        let _ = up_done.send(());
    });
    thread::spawn(move || {
        pump(stream, output);
        let _ = done.send(());
    });
    let _ = closed.recv();
    Some(())
}

/// Copies until either side ends, flushing every chunk so a line is never held back.
fn pump(mut from: impl Read, mut to: impl Write) {
    let mut buffer = [0u8; 8192];
    loop {
        match from.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(read) => {
                if to
                    .write_all(&buffer[..read])
                    .and_then(|()| to.flush())
                    .is_err()
                {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Option<u16> {
        parse_args(args.iter().map(OsString::from))
    }

    #[test]
    fn the_relay_accepts_only_its_two_flags_each_once() {
        assert_eq!(parse(&["--mcp-relay", "--port", "4242"]), Some(4242));
        assert_eq!(parse(&["--port", "4242", "--mcp-relay"]), Some(4242));
        let refused: [&[&str]; 10] = [
            &[],
            &["--mcp-relay"],
            &["--port", "4242"],
            &["--mcp-relay", "--port"],
            &["--mcp-relay", "--port", "0"],
            &["--mcp-relay", "--port", "65536"],
            &["--mcp-relay", "--port", "4242", "--port", "4243"],
            &["--mcp-relay", "--mcp-relay", "--port", "4242"],
            &["--mcp-relay", "--port", "4242", "--data-dir", "x"],
            &["--mcp-relay", "--port", "4242", "extra"],
        ];
        for args in refused {
            assert_eq!(parse(args), None, "{args:?}");
        }
    }

    #[test]
    fn the_relay_removes_the_secret_from_the_environment_and_needs_it_to_run() {
        std::env::set_var(SECRET_ENV, "0".repeat(64));
        // Bad arguments end the run right after the read, before any connection.
        assert_eq!(run(["--unknown"].iter().map(OsString::from)), 1);
        assert!(std::env::var_os(SECRET_ENV).is_none());
        // Good arguments but no secret: fails before any connection too.
        let args = ["--mcp-relay", "--port", "4242"];
        assert_eq!(run(args.iter().map(OsString::from)), 1);
    }
}
