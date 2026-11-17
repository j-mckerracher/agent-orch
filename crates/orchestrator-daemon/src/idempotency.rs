//! Persistent result mapping shared by every mutating RPC method.
//!
//! The database claim prevents duplicate execution across retries and restarts.
//! A process-wide mutex serializes claims and effects within the daemon.
//! Updated: 2026-11-08T12:21:37-07:00.

use std::sync::{Mutex, MutexGuard};

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

/// Key prefix reserved for execution claims made by approved action proposals
/// (DPES-1105). Clients may not use it, so they cannot pre-claim one.
pub(crate) const ACTION_KEY_PREFIX: &str = "action-";

pub(crate) fn run<P: Serialize>(
    store: &SqliteStore,
    gate: &Mutex<()>,
    method: &str,
    key: &str,
    params: &P,
    operation: impl FnOnce() -> Result<Value, RpcError>,
) -> Result<Value, RpcError> {
    if key.starts_with(ACTION_KEY_PREFIX) {
        return Err(RpcError::InvalidParams);
    }
    let held = gate.lock().unwrap_or_else(|error| error.into_inner());
    run_locked(&held, store, method, key, params, operation)
}

/// `run` for a caller that already holds the gate. The guard is the proof.
pub(crate) fn run_locked<P: Serialize>(
    _held: &MutexGuard<'_, ()>,
    store: &SqliteStore,
    method: &str,
    key: &str,
    params: &P,
    operation: impl FnOnce() -> Result<Value, RpcError>,
) -> Result<Value, RpcError> {
    if !valid_key(key) {
        return Err(RpcError::InvalidParams);
    }
    let fingerprint = fingerprint(method, params)?;
    match store
        .begin_idempotency(key, method, &fingerprint)
        .map_err(map_store_error)?
    {
        IdempotencyStart::Completed(result) => return decode_reply(result),
        IdempotencyStart::InProgress => {
            // 2026-11-08 -07:00. A crash after an effect can leave a claimed key
            // without a result; retry fails closed as `IdempotencyInProgress`.
            // Recovery requires reconciliation at startup.
            return Err(RpcError::IdempotencyInProgress);
        }
        IdempotencyStart::Started => {}
    }

    finish(store, method, key, &fingerprint, operation())
}

/// Startup only: stores `reply` for a key that a stopped process claimed and never completed.
pub(crate) fn complete_interrupted_locked<P: Serialize>(
    _held: &MutexGuard<'_, ()>,
    store: &SqliteStore,
    method: &str,
    key: &str,
    params: &P,
    reply: Result<Value, RpcError>,
) -> Result<Value, RpcError> {
    let fingerprint = fingerprint(method, params)?;
    finish(store, method, key, &fingerprint, reply)
}

fn finish(
    store: &SqliteStore,
    method: &str,
    key: &str,
    fingerprint: &str,
    reply: Result<Value, RpcError>,
) -> Result<Value, RpcError> {
    let reply = match reply {
        Ok(value) => CachedReply::Success(value),
        Err(error) => CachedReply::Failure(error),
    };
    let cached = serde_json::to_value(&reply).map_err(|_| RpcError::Internal)?;
    store
        .complete_idempotency(key, method, fingerprint, &cached)
        .map_err(map_store_error)?;
    match reply {
        CachedReply::Success(value) => Ok(value),
        CachedReply::Failure(error) => Err(error),
    }
}

pub(crate) fn stable_id(prefix: &str, key: &str) -> String {
    format!("{prefix}-{}", hex(&Sha256::digest(key.as_bytes())))
}

fn fingerprint<P: Serialize>(method: &str, params: &P) -> Result<String, RpcError> {
    let encoded = serde_json::to_vec(&(method, params)).map_err(|_| RpcError::Internal)?;
    Ok(hex(&Sha256::digest(encoded)))
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn decode_reply(value: Value) -> Result<Value, RpcError> {
    match serde_json::from_value::<CachedReply>(value).map_err(|_| RpcError::Internal)? {
        CachedReply::Success(result) => Ok(result),
        CachedReply::Failure(error) => Err(error),
    }
}

pub(crate) fn map_store_error(error: StoreError) -> RpcError {
    match error {
        StoreError::InvalidIdempotencyKey => RpcError::InvalidParams,
        StoreError::IdempotencyConflict => RpcError::IdempotencyConflict,
        _ => crate::api::internal("idempotency", &error),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn store() -> (TempDir, SqliteStore) {
        let dir = TempDir::new().unwrap();
        let store = SqliteStore::new(&dir.path().join("store.sqlite")).unwrap();
        (dir, store)
    }

    #[test]
    fn run_refuses_a_client_key_in_the_action_namespace() {
        let (_dir, store) = store();
        let gate = Mutex::new(());
        let result = run(
            &store,
            &gate,
            "workstreams.create",
            "action-abc",
            &json!({}),
            || panic!("the operation must not run"),
        );
        assert_eq!(result, Err(RpcError::InvalidParams));
    }

    #[test]
    fn run_locked_accepts_a_key_in_the_action_namespace() {
        let (_dir, store) = store();
        let gate = Mutex::new(());
        let held = gate.lock().unwrap();
        let result = run_locked(
            &held,
            &store,
            "workstreams.create",
            "action-abc",
            &json!({}),
            || Ok(json!({"ok": true})),
        );
        assert_eq!(result, Ok(json!({"ok": true})));
    }
}
