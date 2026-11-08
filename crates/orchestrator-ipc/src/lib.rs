//! Authenticated local JSON-RPC control plane between daemon and clients.
//!
//! Updated: 2026-11-08T14:48:33+00:00. Each frame is `u16` protocol version
//! (big-endian), `u32` JSON byte length (big-endian), then exactly that many
//! UTF-8 JSON bytes. Version 1 is current. Bodies over 1 MiB are rejected
//! before allocation. Typed frame errors distinguish oversized, truncated,
//! wrong-version, malformed-JSON, and I/O failures. Malformed-JSON errors carry
//! no parser detail or input text. JSON-RPC 2.0 `handshake` is the first
//! request on each stream. Request IDs accept strings and numbers. Its params
//! have a lowercase 64-character hex token, supported versions, and requested
//! capabilities. Server picks the highest common version and compares the
//! token in constant time. It advertises only `health`;
//! `capabilities` discovers the negotiated set. `health` returns package
//! version, uptime seconds, and data directory. `shutdown` is lifecycle control
//! and remains callable when no feature capability is negotiated. Unsupported
//! capabilities fail with a typed, payload-free error. Tokens use fresh
//! 32-byte OS randomness. On Unix, `daemon.token` has mode 0600, and the data
//! directory is owner-owned and not group/other-writable. On Windows, the
//! token file inherits the data-directory ACL. Startup removes only an owned,
//! regular stale token file after locking and rechecking identity. Token bytes
//! and attacker-supplied capability text never appear in errors or debug output.
//!
//! Unix socket lives at `data_dir/ipc/daemon.sock`. Its directory has mode
//! 0700 and the socket has mode 0600. Listener cleanup unlinks only its
//! recorded device, inode, and owner identity. Startup reclaims only an
//! owner-matched socket whose connect probe returns `ConnectionRefused`, after
//! rechecking identity. Any other probe result fails closed. Orderly shutdown
//! removes owned token and socket. Acquire the daemon lock before creating
//! server resources. Idle timer starts at daemon startup and resets on each
//! authenticated request. Default is 60 seconds. The accept loop is nonblocking
//! for idle checks; it resets accepted streams to blocking before applying
//! per-stream read and write timeouts.
//!
//! Windows clients use a named pipe derived from the data directory. The pipe
//! rejects remote clients and uses the process default DACL. The token file
//! inherits the data-directory ACL, and token authentication gates every
//! connection. Named pipes are implemented. Other targets return typed
//! `Unsupported`.
//!

#[cfg(any(unix, windows))]
mod auth;
mod protocol;
#[cfg(any(unix, windows))]
mod session;
#[cfg(any(unix, windows))]
mod transport;
#[cfg(unix)]
mod unix_transport;
#[cfg(windows)]
mod windows_transport;

pub use protocol::{Capability, FrameError};
#[cfg(any(unix, windows))]
mod client;
#[cfg(any(unix, windows))]
mod server;
#[cfg(not(any(unix, windows)))]
mod unsupported;
#[cfg(any(unix, windows))]
pub use client::Client;
#[cfg(any(unix, windows))]
pub use server::{Server, ServerConfig};
#[cfg(not(any(unix, windows)))]
pub use unsupported::{Client, Server, ServerConfig};

#[cfg(all(test, unix))]
#[path = "integration_tests.rs"]
mod integration_tests;
#[cfg(all(test, unix))]
#[path = "lifecycle_tests.rs"]
mod lifecycle_tests;

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error("local IPC authentication failed")]
    Authentication,
    #[error("requested capability is unavailable")]
    UnsupportedCapability,
    #[error("no common IPC protocol version")]
    UnsupportedVersion,
    #[error("local IPC transport is unsupported on this platform")]
    Unsupported,
    #[error("local IPC listener already exists")]
    AlreadyRunning,
    #[error("local IPC path has unsafe or ambiguous ownership")]
    UnsafePath,
    #[error("local IPC operation failed")]
    Operation,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Health {
    pub package_version: String,
    pub uptime_seconds: u64,
    pub data_directory: std::path::PathBuf,
}
