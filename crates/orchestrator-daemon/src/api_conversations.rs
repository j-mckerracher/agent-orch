//! Orchestrator conversation methods (M7-08), global and per workstream.
//!
//! Method names are PROPOSED and not yet frozen in the protocol document:
//! `conversations.send`, `conversations.get`, `conversations.cancel`. They
//! reuse `Capability::Coordinator`, so no new capability is advertised.
//!
//! A conversation carries no authority. Every result states `"authority":
//! "none"`; nothing a message or a reply contains can create a task, approval
//! or delivery, because these methods never call into those subsystems. The
//! author and the turn identity are chosen by the daemon, never read from the
//! wire.
//!
//! Replay is cursor polling: `conversations.get` with `after_sequence` returns
//! every entry after that sequence in order, and `epoch` detects a replaced
//! store. A push stream is not provided; the global event journal requires a
//! task id and is not extended here.

use orchestrator_store::{
    ConversationEntry, ConversationError, ConversationTarget, ConversationTurn,
    MAX_CONVERSATION_MESSAGE_BYTES,
};

use super::*;

const CONVERSATION_PAGE_SIZE: usize = 100;

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum TargetParam {
    Global {},
    Workstream { workstream_id: String },
}

impl TargetParam {
    fn to_store(&self) -> ConversationTarget {
        match self {
            Self::Global {} => ConversationTarget::Global,
            Self::Workstream { workstream_id } => {
                ConversationTarget::Workstream(workstream_id.clone())
            }
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ConversationSendParams {
    target: TargetParam,
    text: String,
    idempotency_key: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConversationGetParams {
    target: TargetParam,
    after_sequence: Option<i64>,
    epoch: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ConversationCancelParams {
    target: TargetParam,
    turn_id: String,
    idempotency_key: String,
}

impl DaemonApi {
    pub(super) fn conversations_send(&self, params: Value) -> Result<Value, RpcError> {
        let request: ConversationSendParams = parse_params(params)?;
        if request.text.trim().is_empty() || request.text.len() > MAX_CONVERSATION_MESSAGE_BYTES {
            return Err(RpcError::InvalidParams);
        }
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "conversations.send",
            &key,
            &request,
            || {
                let target = request.target.to_store();
                self.conversations.require_target(&target)?;
                let turn_id = idempotency::stable_id("conversation-turn", &request.idempotency_key);
                let begun = self
                    .conversations
                    .start(&target, &turn_id, &request.text)
                    .map_err(map_conversation_error)?;
                Ok(json!({
                    "authority": "none",
                    "target": request.target,
                    "entry": entry_json(&begun.user_entry),
                    "turn": turn_json(&begun.turn),
                    "epoch": self.store.store_epoch().map_err(map_store_error)?
                }))
            },
        )
    }

    pub(super) fn conversations_get(&self, params: Value) -> Result<Value, RpcError> {
        let request: ConversationGetParams = parse_params(params)?;
        let epoch = self.store.store_epoch().map_err(map_store_error)?;
        if request.epoch.as_ref().is_some_and(|given| *given != epoch) {
            return Err(RpcError::EpochMismatch);
        }
        let cursor = request.after_sequence.unwrap_or(0);
        if cursor < 0 {
            return Err(RpcError::InvalidParams);
        }
        let target = request.target.to_store();
        self.conversations.require_target(&target)?;
        let mut page = self
            .store
            .conversation_page(&target, cursor, CONVERSATION_PAGE_SIZE + 1)
            .map_err(map_conversation_error)?;
        if cursor > page.latest_sequence {
            return Err(RpcError::ReplayGap);
        }
        let more = page.entries.len() > CONVERSATION_PAGE_SIZE;
        page.entries.truncate(CONVERSATION_PAGE_SIZE);
        let next_after_sequence = page.entries.last().map_or(cursor, |entry| entry.sequence);
        Ok(json!({
            "authority": "none",
            "target": request.target,
            "entries": page.entries.iter().map(entry_json).collect::<Vec<_>>(),
            "more": more,
            "next_after_sequence": next_after_sequence,
            "latest_sequence": page.latest_sequence,
            "turn": page.latest_turn.as_ref().map(turn_json),
            "epoch": epoch
        }))
    }

    pub(super) fn conversations_cancel(&self, params: Value) -> Result<Value, RpcError> {
        let request: ConversationCancelParams = parse_params(params)?;
        if request.turn_id.is_empty() {
            return Err(RpcError::InvalidParams);
        }
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "conversations.cancel",
            &key,
            &request,
            || {
                let target = request.target.to_store();
                let (turn, applied) = self
                    .conversations
                    .cancel(&target, &request.turn_id)
                    .map_err(map_conversation_error)?;
                Ok(json!({
                    "authority": "none",
                    "target": request.target,
                    "turn": turn_json(&turn),
                    "cancel_applied": applied,
                    "epoch": self.store.store_epoch().map_err(map_store_error)?
                }))
            },
        )
    }
}

fn map_conversation_error(error: ConversationError) -> RpcError {
    match error {
        ConversationError::Store(error) => map_store_error(error),
        ConversationError::TurnRunning { .. } | ConversationError::TurnNotRunning(_) => {
            RpcError::Conflict
        }
        ConversationError::TurnNotFound => RpcError::NotFound,
        ConversationError::ReplyTooLarge | ConversationError::InvalidInput(_) => {
            RpcError::InvalidParams
        }
    }
}

fn entry_json(entry: &ConversationEntry) -> Value {
    json!({
        "sequence": entry.sequence,
        "id": entry.id,
        "turn_id": entry.turn_id,
        "kind": entry.kind,
        "author": entry.author,
        "body": entry.body,
        "error_code": entry.error_code,
        "created_at": entry.created_at
    })
}

fn turn_json(turn: &ConversationTurn) -> Value {
    json!({
        "id": turn.id,
        "state": turn.state.as_str(),
        "error_code": turn.error_code,
        "runtime": {"name": turn.runtime_name, "version": turn.runtime_version},
        "reply_bytes": turn.reply_bytes,
        "created_at": turn.created_at,
        "updated_at": turn.updated_at
    })
}

#[cfg(test)]
#[path = "api_conversations_tests.rs"]
mod tests;
