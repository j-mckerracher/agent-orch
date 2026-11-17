//! The desktop-only approver credential (DPES-1105): a 32-byte secret that only
//! the desktop host holds. The host generates it and hands it to the daemon it
//! spawns over an inherited pipe; it is never written to disk. A session that
//! presents it in the handshake may call the user-only methods, and every other
//! session is refused them with `Forbidden`.
//!
//! This closes the protocol-level hole: the token and the socket are no longer
//! enough to approve. It is not a sandbox, because a process that can edit the
//! store or replace the daemon is not stopped by any credential.

use crate::auth::{decode, encode};
use crate::transport::random_token;
use crate::{IpcError, RpcError};
use std::fmt;
use std::io::Read;
use subtle::ConstantTimeEq;

/// 64 hex characters and one optional trailing newline.
const MAX_WIRE_LEN: usize = 66;

/// Not `Clone`, `Copy`, `Display` or `Serialize`, and `Debug` is redacted, so the
/// bytes leave this type only through `to_wire`.
pub struct ApproverSecret {
    bytes: [u8; 32],
}

impl fmt::Debug for ApproverSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ApproverSecret([REDACTED])")
    }
}

impl ApproverSecret {
    /// 32 bytes from the OS CSPRNG.
    pub fn generate() -> Result<Self, IpcError> {
        let bytes = random_token().map_err(|_| IpcError::Operation)?;
        Ok(Self { bytes })
    }

    /// 64 lowercase hex characters. For the daemon's stdin pipe and the
    /// handshake only.
    pub fn to_wire(&self) -> String {
        encode(&self.bytes)
    }

    /// Strict: exactly 64 lowercase hex characters, after removing at most one
    /// trailing newline. All zeros is refused: `generate` never produces it, so
    /// it can only be a placeholder or a host bug.
    pub fn from_wire(text: &str) -> Option<Self> {
        let bytes = decode(text.strip_suffix('\n').unwrap_or(text))?;
        if bytes == [0u8; 32] {
            return None;
        }
        Some(Self { bytes })
    }

    /// Reads at most one byte past the longest valid input, to EOF. Input over
    /// 66 bytes, input that is not UTF-8 and bad hex are all one fixed error, so
    /// nothing read is ever echoed.
    pub fn read_from(input: &mut impl Read) -> Result<Self, IpcError> {
        let mut buffer = Vec::with_capacity(MAX_WIRE_LEN + 1);
        input
            .take(MAX_WIRE_LEN as u64 + 1)
            .read_to_end(&mut buffer)
            .map_err(|_| IpcError::Operation)?;
        if buffer.len() > MAX_WIRE_LEN {
            return Err(IpcError::Operation);
        }
        let text = std::str::from_utf8(&buffer).map_err(|_| IpcError::Operation)?;
        Self::from_wire(text).ok_or(IpcError::Operation)
    }

    /// Constant-time on the decoded 32 bytes. A supplied value that is not
    /// exactly 64 lowercase hex characters is simply no match.
    pub(crate) fn matches(&self, supplied: &str) -> bool {
        let Some(supplied) = decode(supplied) else {
            return false;
        };
        bool::from(self.bytes.as_slice().ct_eq(supplied.as_slice()))
    }
}

/// Methods only the user may call. A session must have proved the approver
/// secret. Adding a method here, and routing it in `method_capability`, is the
/// whole change that protects it. Public so the daemon's wire test can walk the
/// same list. Pinned by a test, so a change to this list forces a review. One
/// name per line, so two changes that add names do not conflict.
///
/// The two `review.*` writes are listed because they act as the user: a request
/// supersedes the user's Approved approval for another candidate, and recorded
/// changes block every later approve of that approval.
#[rustfmt::skip]
pub const USER_ONLY_METHODS: &[&str] = &[
    "actions.resolve",
    "approvals.resolve",
    "review.request_approval",
    "review.request_changes",
    "settings.set",
    "monitoring.set",
];

/// The one gate. `Ok` when the method is open to every session or the session
/// proved the secret.
pub(crate) fn require_approver(method: &str, approver: bool) -> Result<(), RpcError> {
    if approver || !USER_ONLY_METHODS.contains(&method) {
        Ok(())
    } else {
        Err(RpcError::Forbidden)
    }
}
