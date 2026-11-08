//! Persistent result mapping shared by every mutating RPC method.
//!
//! The database claim prevents duplicate execution across retries and restarts.
//! A process-wide mutex serializes claims and effects within the daemon.
//! Updated: 2026-11-08T17:21:16+00:00.

use std::sync::Mutex;

use orchestrator_ipc::RpcError;
use orchestrator_store::{IdempotencyStart, SqliteStore, StoreError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Deserialize, Serialize)]
#[serde(tag = "status", content = "body", rename_all = "snake_case")]
enum CachedReply {
    Success(Value),
    Failure(RpcError),
}

pub(crate) fn run(
    store: &SqliteStore,
    gate: &Mutex<()>,
    method: &str,
    key: &str,
    params: &Value,
    operation: impl FnOnce() -> Result<Value, RpcError>,
) -> Result<Value, RpcError> {
    if !valid_key(key) {
        return Err(RpcError::InvalidParams);
    }
    let _guard = gate.lock().unwrap_or_else(|error| error.into_inner());
    let fingerprint = fingerprint(method, params)?;
    match store
        .begin_idempotency(key, method, &fingerprint)
        .map_err(map_store_error)?
    {
        IdempotencyStart::Completed(result) => return decode_reply(result),
        IdempotencyStart::InProgress => return Err(RpcError::IdempotencyInProgress),
        IdempotencyStart::Started => {}
    }

    let reply = match operation() {
        Ok(value) => CachedReply::Success(value),
        Err(error) => CachedReply::Failure(error),
    };
    let cached = serde_json::to_value(&reply).map_err(|_| RpcError::Internal)?;
    store
        .complete_idempotency(key, method, &fingerprint, &cached)
        .map_err(map_store_error)?;
    match reply {
        CachedReply::Success(value) => Ok(value),
        CachedReply::Failure(error) => Err(error),
    }
}

pub(crate) fn stable_id(prefix: &str, key: &str) -> String {
    format!("{prefix}-{}", hex(&Sha256::digest(key.as_bytes())))
}

fn fingerprint(method: &str, params: &Value) -> Result<String, RpcError> {
    let encoded = serde_json::to_vec(params).map_err(|_| RpcError::Internal)?;
    let mut bytes = Vec::with_capacity(method.len() + 1 + encoded.len());
    bytes.extend_from_slice(method.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(&encoded);
    Ok(hex(&Sha256::digest(bytes)))
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
        })
}

fn decode_reply(value: Value) -> Result<Value, RpcError> {
    match serde_json::from_value::<CachedReply>(value).map_err(|_| RpcError::Internal)? {
        CachedReply::Success(result) => Ok(result),
        CachedReply::Failure(error) => Err(error),
    }
}

fn map_store_error(error: StoreError) -> RpcError {
    match error {
        StoreError::InvalidIdempotencyKey => RpcError::InvalidParams,
        StoreError::IdempotencyConflict => RpcError::IdempotencyConflict,
        _ => RpcError::Internal,
    }
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(HEX[(byte >> 4) as usize] as char);
        value.push(HEX[(byte & 15) as usize] as char);
    }
    value
}
