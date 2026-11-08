use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, Read, Write};
use thiserror::Error;

pub(crate) const VERSION: u16 = 1;
const HEADER_LEN: usize = 6;
const MAX_BODY: usize = 1024 * 1024;

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
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub(crate) enum RpcId {
    String(String),
    Number(serde_json::Number),
}

#[derive(Deserialize, Serialize)]
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
pub(crate) struct RpcFailure {
    pub(crate) code: i32,
    pub(crate) message: String,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Handshake {
    pub(crate) token: Option<String>,
    pub(crate) versions: Vec<u16>,
    pub(crate) capabilities: Vec<String>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Welcome {
    pub(crate) version: u16,
    pub(crate) capabilities: Vec<Capability>,
}

pub(crate) fn read<T: for<'de> Deserialize<'de>>(stream: &mut impl Read) -> Result<T, FrameError> {
    let mut header = [0u8; HEADER_LEN];
    read_part(stream, &mut header, "header")?;
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
