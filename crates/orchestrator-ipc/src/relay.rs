//! Raw-frame relay for hosts that forward JSON-RPC messages for another
//! process, such as the desktop shell forwarding for its webview.
//!
//! The relay reads the token, connects, performs the authenticated handshake,
//! and then moves whole JSON frames in both directions without interpreting
//! them. The token never leaves this module. Closing or dropping a relay only
//! closes the client stream: it never sends `shutdown`, so a client going away
//! is not a daemon stop.
//!
//! One worker thread owns the stream. It alternates between draining queued
//! outgoing messages and polling for incoming bytes with a short timeout.
//! Incoming bytes are reassembled across polls, so a timeout in the middle of
//! a frame cannot corrupt framing.

use crate::client::read_token;
use crate::protocol::{
    self, Handshake, RpcId, RpcRequest, RpcResponse, HEADER_LEN, MAX_BODY, VERSION,
};
use crate::transport::{connect_fail_fast as connect_local, IpcStream, TransportError};
use crate::Capability;
use serde_json::Value;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Seconds-scale: a first pipe or socket use on a loaded Windows host (virus
/// scan, cold start) can take longer than two seconds. A wedged daemon delays
/// a close by at most `WRITE_TIMEOUT`.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_TIMEOUT: Duration = Duration::from_millis(25);

/// Why a relay could not open, or could not carry a message.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RelayError {
    /// No daemon is serving this data directory (no token, or nothing listening).
    #[error("daemon is not running")]
    Unavailable,
    #[error("daemon authentication failed")]
    Authentication,
    #[error("no common protocol version")]
    UnsupportedVersion,
    #[error("requested capability is unavailable")]
    UnsupportedCapability,
    #[error("daemon sent an invalid or unexpected frame")]
    Protocol,
    #[error("local transport failed")]
    Io,
    /// The relay is already closed.
    #[error("relay is closed")]
    Closed,
}

impl RelayError {
    /// Stable machine-readable code for hosts that relay the error.
    pub fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "daemon-unavailable",
            Self::Authentication => "auth",
            Self::UnsupportedVersion => "version",
            Self::UnsupportedCapability => "capability-unavailable",
            Self::Protocol => "protocol",
            Self::Io => "io",
            Self::Closed => "closed",
        }
    }
}

/// Why an open relay ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseReason {
    /// The daemon closed the stream.
    Remote,
    /// The daemon sent an oversized, wrong-version or malformed frame.
    Protocol,
    /// The transport failed.
    Io,
    /// This side closed the relay.
    Local,
}

impl CloseReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::Remote => "remote",
            Self::Protocol => "protocol",
            Self::Io => "io",
            Self::Local => "local",
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum RelayEvent {
    /// One JSON-RPC response or notification, unmodified.
    Message(Value),
    /// Always the last event of a relay.
    Closed(CloseReason),
}

enum Command {
    Send(Value),
    Close,
}

pub struct Relay {
    commands: Sender<Command>,
    worker: Option<JoinHandle<()>>,
}

impl Relay {
    /// Connects, authenticates and starts relaying. Returns the relay and the
    /// handshake `result` (version, capabilities, optional `journal_epoch`).
    /// `on_event` runs on the relay's worker thread; it must not block for long.
    pub fn open(
        data_dir: impl AsRef<Path>,
        requested: &[Capability],
        on_event: impl FnMut(RelayEvent) + Send + 'static,
    ) -> Result<(Self, Value), RelayError> {
        let data_dir = data_dir.as_ref();
        if matches!(
            fs::symlink_metadata(data_dir.join("daemon.token")),
            Err(ref error) if error.kind() == io::ErrorKind::NotFound
        ) {
            return Err(RelayError::Unavailable);
        }
        let token = read_token(data_dir).map_err(|_| RelayError::Authentication)?;
        let mut stream = connect_local(data_dir).map_err(connect_error)?;
        stream
            .set_timeouts(HANDSHAKE_TIMEOUT)
            .map_err(|_| RelayError::Io)?;
        let request = RpcRequest {
            jsonrpc: "2.0".into(),
            id: RpcId::Number(0u64.into()),
            method: "handshake".into(),
            params: serde_json::to_value(Handshake {
                token: Some(token),
                versions: vec![VERSION],
                capabilities: requested
                    .iter()
                    .map(|capability| capability.as_str().to_owned())
                    .collect(),
            })
            .map_err(|_| RelayError::Protocol)?,
        };
        protocol::write(&mut stream, &request).map_err(|_| RelayError::Io)?;
        let response = protocol::read::<RpcResponse>(&mut stream).map_err(|error| match error {
            protocol::FrameError::Io(_) | protocol::FrameError::Truncated { .. } => RelayError::Io,
            _ => RelayError::Protocol,
        })?;
        if response.jsonrpc != "2.0" || response.id != RpcId::Number(0u64.into()) {
            return Err(RelayError::Protocol);
        }
        if let Some(failure) = response.error {
            return Err(match failure.code {
                -32001 => RelayError::Authentication,
                -32002 | -32004 => RelayError::UnsupportedCapability,
                -32003 => RelayError::UnsupportedVersion,
                _ => RelayError::Protocol,
            });
        }
        let welcome = response.result.ok_or(RelayError::Protocol)?;
        let version_ok = welcome.get("version").and_then(Value::as_u64) == Some(u64::from(VERSION));
        let granted: Option<Vec<Capability>> = welcome
            .get("capabilities")
            .and_then(|value| serde_json::from_value(value.clone()).ok());
        match granted {
            Some(granted) if version_ok && granted.iter().all(|c| requested.contains(c)) => {}
            _ => return Err(RelayError::UnsupportedVersion),
        }

        let (commands, queue) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("orchestrator-ipc-relay".into())
            .spawn(move || run(stream, queue, on_event))
            .map_err(|_| RelayError::Io)?;
        Ok((
            Self {
                commands,
                worker: Some(worker),
            },
            welcome,
        ))
    }

    /// Queues one JSON-RPC message. Delivery failures surface as `Closed`
    /// events, not here.
    pub fn send(&self, message: Value) -> Result<(), RelayError> {
        self.commands
            .send(Command::Send(message))
            .map_err(|_| RelayError::Closed)
    }

    /// Closes the stream and waits for the worker to end.
    pub fn close(mut self) {
        let _ = self.commands.send(Command::Close);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Close);
    }
}

fn connect_error(error: TransportError) -> RelayError {
    match error {
        TransportError::Io(
            io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused | io::ErrorKind::TimedOut,
        ) => RelayError::Unavailable,
        _ => RelayError::Io,
    }
}

fn run(
    mut stream: Box<dyn IpcStream>,
    queue: Receiver<Command>,
    mut on_event: impl FnMut(RelayEvent),
) {
    let mut frames = Reassembler::default();
    let reason = 'serve: loop {
        loop {
            match queue.try_recv() {
                Ok(Command::Send(message)) => {
                    if stream.set_timeouts(WRITE_TIMEOUT).is_err()
                        || protocol::write(&mut stream, &message).is_err()
                    {
                        break 'serve CloseReason::Io;
                    }
                }
                Ok(Command::Close) | Err(TryRecvError::Disconnected) => {
                    break 'serve CloseReason::Local;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        if stream.set_timeouts(POLL_TIMEOUT).is_err() {
            break CloseReason::Io;
        }
        match frames.poll(&mut stream) {
            Poll::Frame(message) => on_event(RelayEvent::Message(message)),
            Poll::Idle => {}
            Poll::Closed(reason) => break reason,
        }
    };
    drop(stream);
    on_event(RelayEvent::Closed(reason));
}

enum Poll {
    Frame(Value),
    Idle,
    Closed(CloseReason),
}

#[derive(Default)]
struct Reassembler {
    buffer: Vec<u8>,
}

impl Reassembler {
    fn poll(&mut self, stream: &mut Box<dyn IpcStream>) -> Poll {
        loop {
            match self.take_frame() {
                Ok(Some(message)) => return Poll::Frame(message),
                Ok(None) => {}
                Err(reason) => return Poll::Closed(reason),
            }
            let mut chunk = [0u8; 8192];
            match io::Read::read(stream, &mut chunk) {
                Ok(0) => return Poll::Closed(CloseReason::Remote),
                Ok(read) => self.buffer.extend_from_slice(&chunk[..read]),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                    ) =>
                {
                    return Poll::Idle;
                }
                Err(_) => return Poll::Closed(CloseReason::Io),
            }
        }
    }

    fn take_frame(&mut self) -> Result<Option<Value>, CloseReason> {
        if self.buffer.len() < HEADER_LEN {
            return Ok(None);
        }
        let version = u16::from_be_bytes([self.buffer[0], self.buffer[1]]);
        let length = u32::from_be_bytes([
            self.buffer[2],
            self.buffer[3],
            self.buffer[4],
            self.buffer[5],
        ]) as usize;
        if version != VERSION || length > MAX_BODY {
            return Err(CloseReason::Protocol);
        }
        let end = HEADER_LEN + length;
        if self.buffer.len() < end {
            return Ok(None);
        }
        let message = serde_json::from_slice(&self.buffer[HEADER_LEN..end])
            .map_err(|_| CloseReason::Protocol)?;
        self.buffer.drain(..end);
        Ok(Some(message))
    }
}

#[cfg(test)]
#[path = "relay_tests.rs"]
mod tests;
