//! The `--mcp-relay` mode of the real `orchestrator-daemon` binary (DPES-1105 phase 6b),
//! against a listener in this test process that plays the turn server. The relay must say the
//! hello first, copy lines both ways, hold no data directory and print nothing but one fixed
//! word. The listener side of the protocol is covered in `src/mcp_server_tests.rs`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const SECRET: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const WAIT: Duration = Duration::from_secs(20);
const FIXED_WORD: &str = "mcp_relay_failed";

/// Kills the relay when a test fails before it exits.
struct Relay {
    child: Child,
}

impl Drop for Relay {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn(args: &[&str], secret: Option<&str>) -> Relay {
    let mut command = Command::new(env!("CARGO_BIN_EXE_orchestrator-daemon"));
    command
        .args(args)
        .env_remove("MCAO_MCP_SECRET")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(secret) = secret {
        command.env("MCAO_MCP_SECRET", secret);
    }
    Relay {
        child: command.spawn().unwrap(),
    }
}

impl Relay {
    fn wait(&mut self) -> ExitStatus {
        let start = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(start.elapsed() < WAIT, "the relay did not exit");
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn rest_of(&mut self) -> (String, String) {
        let (mut out, mut err) = (String::new(), String::new());
        self.child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut out)
            .unwrap();
        self.child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut err)
            .unwrap();
        (out, err)
    }
}

fn closed_port() -> u16 {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.local_addr().unwrap().port()
}

#[test]
fn the_relay_says_the_hello_first_then_copies_lines_both_ways() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let (lines, received) = mpsc::channel();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut writer = stream.try_clone().unwrap();
        let mut reader = BufReader::new(stream);
        for _ in 0..2 {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            lines.send(line).unwrap();
        }
        writer
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n")
            .unwrap();
        // Closing the socket ends the relay.
    });
    let mut relay = spawn(&["--mcp-relay", "--port", &port], Some(SECRET));
    let mut stdin: ChildStdin = relay.child.stdin.take().unwrap();
    let stdout: ChildStdout = relay.child.stdout.take().unwrap();

    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n")
        .unwrap();
    stdin.flush().unwrap();

    let hello = received.recv_timeout(WAIT).unwrap();
    assert_eq!(hello, format!("{{\"mcao_mcp_hello\":\"{SECRET}\"}}\n"));
    let forwarded = received.recv_timeout(WAIT).unwrap();
    assert_eq!(
        forwarded,
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n"
    );
    let mut reply = String::new();
    BufReader::new(stdout).read_line(&mut reply).unwrap();
    assert_eq!(reply, "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n");
    server.join().unwrap();

    let status = relay.wait();
    assert_eq!(status.code(), Some(0));
    // The relay logs nothing: no daemon start line, no secret, no port.
    let mut err = String::new();
    relay
        .child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    assert_eq!(err, "");
}

#[test]
fn the_relay_exits_when_its_stdin_closes() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        // Hold the socket open until the relay hangs up.
        let mut rest = String::new();
        let _ = reader.read_line(&mut rest);
        rest
    });
    let mut relay = spawn(&["--mcp-relay", "--port", &port], Some(SECRET));
    drop(relay.child.stdin.take());

    assert_eq!(relay.wait().code(), Some(0));
    assert_eq!(server.join().unwrap(), "");
}

#[test]
fn the_relay_rejects_unknown_arguments_without_connecting() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let refused: [&[&str]; 4] = [
        &["--mcp-relay", "--port", &port, "--data-dir", "somewhere"],
        &["--mcp-relay", "--port", &port, "--approver-stdin"],
        &["--mcp-relay", "--port", &port, "extra"],
        &["--mcp-relay"],
    ];
    for args in refused {
        let mut relay = spawn(args, Some(SECRET));
        let status = relay.wait();
        let (out, err) = relay.rest_of();
        assert_eq!(status.code(), Some(1), "{args:?}");
        assert_eq!(out, "", "{args:?}");
        assert_eq!(err, format!("{FIXED_WORD}\n"), "{args:?}");
    }
    assert!(
        listener.accept().is_err(),
        "a relay with bad arguments connected"
    );
}

#[test]
fn the_relay_without_the_secret_or_without_a_listener_fails_with_the_fixed_word() {
    let port = closed_port().to_string();

    let mut no_secret = spawn(&["--mcp-relay", "--port", &port], None);
    assert_eq!(no_secret.wait().code(), Some(1));
    assert_eq!(
        no_secret.rest_of(),
        (String::new(), format!("{FIXED_WORD}\n"))
    );

    // Nobody listens: three attempts, then the exit. The word never names the port or secret.
    let mut no_listener = spawn(&["--mcp-relay", "--port", &port], Some(SECRET));
    assert_eq!(no_listener.wait().code(), Some(1));
    let (out, err) = no_listener.rest_of();
    assert_eq!(out, "");
    assert_eq!(err, format!("{FIXED_WORD}\n"));
    assert!(!err.contains(SECRET) && !err.contains(&port));
}
