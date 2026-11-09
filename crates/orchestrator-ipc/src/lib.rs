//! Authenticated local JSON-RPC control plane between daemon and clients.
//!
//! Updated: 2026-11-08T17:19:10+00:00. Each frame is `u16` protocol version
//! (big-endian), `u32` JSON byte length (big-endian), then exactly that many
//! UTF-8 JSON bytes. Version 1 is current. Bodies over 1 MiB are rejected
//! before allocation. Typed frame errors distinguish oversized, truncated,
//! wrong-version, malformed-JSON, and I/O failures. Malformed-JSON errors carry
//! no parser detail or input text. JSON-RPC 2.0 `handshake` is the first
//! request on each stream. Request IDs accept strings and numbers. Its params
//! have a lowercase 64-character hex token, supported versions, and requested
//! capabilities. Server picks the highest common version and compares the
//! token in constant time. It negotiates only implemented, requested
//! capabilities; `capabilities` reports that set. `health` returns package
//! version, uptime seconds, and data directory. `shutdown` remains lifecycle
//! control even when no feature capability is negotiated. Unsupported
//! capabilities fail with a typed, payload-free error. Handlers may
//! name their journal with `journal_epoch`. The welcome carries it as an
//! optional `journal_epoch`, `events.subscribe` returns it as `epoch` and
//! accepts an `epoch` that must match (else typed `epoch_mismatch`, code
//! -32018). Both are additive, so protocol version 1 is unchanged. Tokens use fresh
//! 32-byte OS randomness. On Unix, `daemon.token` has mode 0600, and the data
//! directory is owner-owned and not group/other-writable. On Windows, the token
//! file is created with a protected DACL for the current process token SID before
//! secret bytes are written, then published by temp-file rename. Unix startup
//! removes stale token files only after owner and identity checks. Token bytes
//! and attacker-supplied capability text never appear in errors or debug output.
//!
//! Unix socket lives at `data_dir/ipc/daemon.sock`. Its directory has mode
//! 0700 and the socket has mode 0600. Listener cleanup unlinks only its
//! recorded device, inode, and owner identity. Startup reclaims only an
//! owner-matched socket whose connect probe returns `ConnectionRefused`, after
//! rechecking identity. Any other probe result fails closed. Orderly shutdown
//! removes owned token and socket. Acquire the daemon lock before creating
//! server resources. Idle timer resets on authenticated requests and events.
//! Active-work handlers can keep the daemon alive past the default 60 seconds.
//! The accept loop is nonblocking for idle checks; accepted streams are reset
//! to blocking before per-stream read and write timeouts are applied.
//!
//! Windows transport contract updated: 2026-11-08T16:55:20Z. Named-pipe names
//! derive only from canonical data-directory paths. Protected DACLs grant access
//! only to the current process token SID. Pipes reject remote clients and cap
//! instances at 16. Every accepted stream reaches the shared handshake and token
//! check before method dispatch. Other targets use the typed `Unsupported`
//! transport.
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

#[cfg(all(test, any(unix, windows)))]
#[path = "coordinator_review_tests.rs"]
mod coordinator_review_tests;
#[cfg(all(test, any(unix, windows)))]
#[path = "event_stream_tests.rs"]
mod event_stream_tests;
#[cfg(all(test, unix))]
#[path = "integration_tests.rs"]
mod integration_tests;
#[cfg(all(test, unix))]
#[path = "lifecycle_tests.rs"]
mod lifecycle_tests;

/// One path-free durable event returned by the authenticated daemon stream.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct IpcEvent {
    /// Monotonic database-wide sequence.
    pub sequence: i64,
    /// Owning task identifier.
    pub task_id: String,
    /// Session identifier for session-scoped events.
    pub session_id: Option<String>,
    /// Stable event type.
    pub kind: String,
    /// Event-specific, path-free payload.
    pub payload: serde_json::Value,
    /// Event creation time in UTC.
    pub created_at: String,
}

/// Sanitized failure returned by a method handler.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RpcError {
    InvalidParams,
    NotFound,
    Conflict,
    Forbidden,
    IdempotencyConflict,
    IdempotencyInProgress,
    SlowConsumer,
    ResultTooLarge,
    ReplayGap,
    EpochMismatch,
    Internal,
}

impl RpcError {
    pub(crate) const fn code(self) -> i32 {
        match self {
            Self::InvalidParams => -32602,
            Self::NotFound => -32010,
            Self::Conflict => -32011,
            Self::Forbidden => -32012,
            Self::IdempotencyConflict => -32013,
            Self::IdempotencyInProgress => -32014,
            Self::SlowConsumer => -32015,
            Self::ResultTooLarge => -32016,
            Self::ReplayGap => -32017,
            Self::EpochMismatch => -32018,
            Self::Internal => -32603,
        }
    }

    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::InvalidParams => "request parameters are invalid",
            Self::NotFound => "resource was not found",
            Self::Conflict => "operation conflicts with current state",
            Self::Forbidden => "operation is not authorized",
            Self::IdempotencyConflict => "idempotency key conflicts with prior request",
            Self::IdempotencyInProgress => "idempotent operation is still in progress",
            Self::SlowConsumer => "event subscriber fell behind",
            Self::ResultTooLarge => "result exceeds size limit",
            Self::ReplayGap => "event cursor is ahead of the durable journal",
            Self::EpochMismatch => "event cursor belongs to a different durable journal",
            Self::Internal => "internal operation failed",
        }
    }
}

/// Daemon-owned API boundary used by the transport after authentication.
pub trait RpcHandler: Send + Sync + 'static {
    /// Returns only capabilities implemented by this handler.
    fn capabilities(&self) -> Vec<Capability>;

    /// Runs a negotiated method and returns a JSON result.
    fn dispatch(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, RpcError>;

    /// Loads a bounded, ordered page of durable events after a cursor.
    fn events_after(&self, after_sequence: i64, limit: usize) -> Result<Vec<IpcEvent>, RpcError>;
    /// Returns the latest durable event cursor.
    fn latest_event_sequence(&self) -> Result<i64, RpcError> {
        Ok(i64::MAX)
    }
    /// Names the durable journal behind the cursors this handler issues. A
    /// handler that returns one lets clients detect a replaced journal; `None`
    /// means cursors cannot be tied to a journal.
    fn journal_epoch(&self) -> Result<Option<String>, RpcError> {
        Ok(None)
    }

    /// Keeps the daemon alive while durable work remains active.
    fn keep_alive(&self) -> bool {
        false
    }
}

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
    #[error("invalid RPC parameters")]
    InvalidParams,
    #[error("RPC resource not found")]
    NotFound,
    #[error("RPC operation conflicts with current state")]
    Conflict,
    #[error("RPC operation is not authorized")]
    Forbidden,
    #[error("RPC idempotency key conflicts with prior request")]
    IdempotencyConflict,
    #[error("RPC idempotent operation is still in progress")]
    IdempotencyInProgress,
    #[error("RPC event subscriber fell behind")]
    SlowConsumer,
    #[error("RPC result exceeds size limit")]
    ResultTooLarge,
    #[error("event cursor is ahead of the durable journal")]
    ReplayGap,
    #[error("event cursor belongs to a different durable journal")]
    EpochMismatch,
    #[error("event subscription is not active")]
    NoSubscription,
    #[error("local IPC operation failed")]
    Operation,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Health {
    pub package_version: String,
    pub uptime_seconds: u64,
    pub data_directory: std::path::PathBuf,
}
