use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, Read, Write};
use thiserror::Error;

pub(crate) const VERSION: u16 = 1;
pub(crate) const HEADER_LEN: usize = 6;
pub(crate) const MAX_BODY: usize = 1024 * 1024;

#[derive(Debug, Error)]
pub enum FrameError {
    #[error("frame body length {declared} exceeds maximum {maximum}")]
    Oversized { declared: u32, maximum: usize },
    #[error("truncated frame {part}")]
    Truncated { part: &'static str },
    #[error("unsupported frame version {received}; supported version is {supported}")]
    WrongVersion { received: u16, supported: u16 },
    #[error("malformed JSON frame")]
    MalformedJson,
    #[error("frame I/O failed: {0}")]
    Io(io::ErrorKind),
}

impl From<io::Error> for FrameError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Health,
    Projects,
    Tasks,
    Sessions,
    Events,
    Approvals,
    Workspaces,
    Delivery,
    Agents,
    Coordinator,
    Review,
    Host,
    Queue,
    Workstreams,
    /// Clearing conversation history and removing or restoring project coordinators (DPES-1105).
    ConversationHistory,
    /// A model and effort chosen for one `conversations.send` (DPES-1105).
    ConversationSelection,
    /// A permission level chosen for one `conversations.send` (DPES-1105). Not
    /// advertised by an older daemon, which runs every message read-only.
    PermissionLevels,
    /// A completed reply may end in one `mcao-actions` block that becomes
    /// pending proposals (DPES-1105). Not advertised by an older daemon, which
    /// never parses a reply.
    OrchestratorActions,
    /// `settings.get` and the user-only `settings.set`: the machine load limits and the
    /// live state of the load gate (DPES-1105).
    Settings,
    /// `monitoring.get` and `monitoring.ack` (open) and the user-only `monitoring.set`: the
    /// wake-up mode and the paused and unseen state of each scope (DPES-1105).
    Monitoring,
    /// `holds.list` (open) and the user-only `holds.set` and `holds.release` (DPES-1105).
    Holds,
}

impl Capability {
    pub const ALL: [Self; 21] = [
        Self::Health,
        Self::Projects,
        Self::Tasks,
        Self::Sessions,
        Self::Events,
        Self::Approvals,
        Self::Workspaces,
        Self::Delivery,
        Self::Agents,
        Self::Coordinator,
        Self::Review,
        Self::Host,
        Self::Queue,
        Self::Workstreams,
        Self::ConversationHistory,
        Self::ConversationSelection,
        Self::PermissionLevels,
        Self::OrchestratorActions,
        Self::Settings,
        Self::Monitoring,
        Self::Holds,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Health => "health",
            Self::Projects => "projects",
            Self::Tasks => "tasks",
            Self::Sessions => "sessions",
            Self::Events => "events",
            Self::Approvals => "approvals",
            Self::Workspaces => "workspaces",
            Self::Delivery => "delivery",
            Self::Agents => "agents",
            Self::Coordinator => "coordinator",
            Self::Review => "review",
            Self::Host => "host",
            Self::Queue => "queue",
            Self::Workstreams => "workstreams",
            Self::ConversationHistory => "conversation_history",
            Self::ConversationSelection => "conversation_selection",
            Self::PermissionLevels => "permission_levels",
            Self::OrchestratorActions => "orchestrator_actions",
            Self::Settings => "settings",
            Self::Monitoring => "monitoring",
            Self::Holds => "holds",
        }
    }

    pub(crate) fn from_name(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|capability| capability.as_str() == value)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub(crate) enum RpcId {
    String(String),
    Number(serde_json::Number),
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RpcRequest {
    pub(crate) jsonrpc: String,
    pub(crate) id: RpcId,
    pub(crate) method: String,
    #[serde(default)]
    pub(crate) params: Value,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct RpcResponse {
    pub(crate) jsonrpc: String,
    pub(crate) id: RpcId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<RpcFailure>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct RpcNotification {
    pub(crate) jsonrpc: String,
    pub(crate) method: String,
    pub(crate) params: Value,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum IncomingMessage {
    Response(RpcResponse),
    Notification(RpcNotification),
}

#[derive(Deserialize, Serialize)]
pub(crate) struct RpcFailure {
    pub(crate) code: i32,
    pub(crate) message: String,
}

/// No `Debug`: `token` and `approver` are secrets.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Handshake {
    pub(crate) token: Option<String>,
    pub(crate) versions: Vec<u16>,
    pub(crate) capabilities: Vec<String>,
    /// Proof of the approver secret. Left out when absent, so a daemon that
    /// predates it (and refuses unknown fields) still accepts the handshake.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) approver: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Welcome {
    pub(crate) version: u16,
    pub(crate) capabilities: Vec<Capability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) journal_epoch: Option<String>,
    /// True when this session proved the approver secret. A daemon that
    /// predates it omits the field, which reads as false.
    #[serde(default)]
    pub(crate) approver: bool,
}

pub(crate) fn read<T: for<'de> Deserialize<'de>>(stream: &mut impl Read) -> Result<T, FrameError> {
    let mut header = [0u8; HEADER_LEN];
    read_part(stream, &mut header, "header")?;
    read_body(stream, &header)
}

pub(crate) fn read_optional<T: for<'de> Deserialize<'de>>(
    stream: &mut impl Read,
) -> Result<Option<T>, FrameError> {
    let mut first = [0u8; 1];
    loop {
        match stream.read(&mut first) {
            Ok(0) => return Err(FrameError::Truncated { part: "header" }),
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                return Ok(None)
            }
            Err(error) => return Err(FrameError::Io(error.kind())),
        }
    }
    let mut header = [0u8; HEADER_LEN];
    header[0] = first[0];
    read_part(stream, &mut header[1..], "header")?;
    read_body(stream, &header).map(Some)
}

fn read_body<T: for<'de> Deserialize<'de>>(
    stream: &mut impl Read,
    header: &[u8; HEADER_LEN],
) -> Result<T, FrameError> {
    let version = u16::from_be_bytes([header[0], header[1]]);
    if version != VERSION {
        return Err(FrameError::WrongVersion {
            received: version,
            supported: VERSION,
        });
    }
    let length = u32::from_be_bytes([header[2], header[3], header[4], header[5]]);
    if length as usize > MAX_BODY {
        return Err(FrameError::Oversized {
            declared: length,
            maximum: MAX_BODY,
        });
    }
    let mut body = vec![0; length as usize];
    read_part(stream, &mut body, "body")?;
    serde_json::from_slice(&body).map_err(|_| FrameError::MalformedJson)
}

fn read_part(
    stream: &mut impl Read,
    buffer: &mut [u8],
    part: &'static str,
) -> Result<(), FrameError> {
    match stream.read_exact(buffer) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
            Err(FrameError::Truncated { part })
        }
        Err(error) => Err(FrameError::Io(error.kind())),
    }
}

pub(crate) fn write<T: Serialize>(stream: &mut impl Write, value: &T) -> Result<(), FrameError> {
    let body = serde_json::to_vec(value).map_err(|_| FrameError::MalformedJson)?;
    if body.len() > MAX_BODY {
        return Err(FrameError::Oversized {
            declared: body.len().min(u32::MAX as usize) as u32,
            maximum: MAX_BODY,
        });
    }
    stream.write_all(&VERSION.to_be_bytes())?;
    stream.write_all(&(body.len() as u32).to_be_bytes())?;
    stream.write_all(&body)?;
    Ok(())
}
#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
