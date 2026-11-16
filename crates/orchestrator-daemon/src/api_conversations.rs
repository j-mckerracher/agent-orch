//! Orchestrator conversation methods (M7-08), global and per workstream.
//!
//! Method names are PROPOSED and not yet frozen in the protocol document:
//! `conversations.send`, `conversations.get`, `conversations.cancel`. They
//! reuse `Capability::Coordinator`. `conversations.clear` (DPES-1105) needs
//! `Capability::ConversationHistory`, which older daemons do not advertise.
//!
//! A send's idempotency record keeps references (entry id, sequence, turn id),
//! never the message text. Each send response is rebuilt from the store, so a
//! message that a clear deleted replays as `{"cleared": true}` instead of failing.
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

use orchestrator_core::contract::ModelSelection;
use orchestrator_core::permission::PermissionLevel;
use orchestrator_store::{
    ConversationEntry, ConversationError, ConversationTarget, ConversationTurn,
    MAX_CONVERSATION_MESSAGE_BYTES,
};

use super::conversation_runtime::{Choice, RuntimeIdentity};
use super::*;

const CONVERSATION_PAGE_SIZE: usize = 100;
const MAX_CHOICE_AGENT_BYTES: usize = 64;

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

/// The agent, model and effort the picker chose for one message (DPES-1105).
/// Omitted, so an older desktop's send is byte-identical to today's request
/// and its idempotency fingerprint does not change.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SelectionParam {
    agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    effort: Option<String>,
}

impl SelectionParam {
    /// A malformed choice is a bad request. A choice the catalog refuses is not:
    /// it is recorded as a failed turn (see `start_turn`).
    fn to_choice(&self) -> Result<Choice, RpcError> {
        if self.agent.is_empty() || self.agent.len() > MAX_CHOICE_AGENT_BYTES {
            return Err(RpcError::InvalidParams);
        }
        if self.model.is_some() || self.effort.is_some() {
            ModelSelection {
                model: self.model.clone(),
                effort: self.effort.clone(),
            }
            .validate()
            .map_err(|_| RpcError::InvalidParams)?;
        }
        Ok(Choice {
            agent: self.agent.clone(),
            model: self.model.clone(),
            effort: self.effort.clone(),
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ConversationSendParams {
    target: TargetParam,
    text: String,
    idempotency_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selection: Option<SelectionParam>,
    /// The permission level for this message (DPES-1105). Omitted means
    /// Restricted, and the request serializes as it did before, so its
    /// idempotency fingerprint is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    permission: Option<PermissionLevel>,
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

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ConversationClearParams {
    target: TargetParam,
    idempotency_key: String,
}

impl DaemonApi {
    pub(super) fn conversations_send(&self, params: Value) -> Result<Value, RpcError> {
        let request: ConversationSendParams = parse_params(params)?;
        if request.text.trim().is_empty() || request.text.len() > MAX_CONVERSATION_MESSAGE_BYTES {
            return Err(RpcError::InvalidParams);
        }
        let choice = request
            .selection
            .as_ref()
            .map(SelectionParam::to_choice)
            .transpose()?;
        let level = request.permission.unwrap_or_default();
        let key = request.idempotency_key.clone();
        self.conversations.prepare(choice.as_ref())?;
        // The idempotency record keeps references only, never the message text
        // (DPES-1105). Every reply, the first one included, is read back from the store.
        let reference = idempotency::run(
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
                    .start(&target, &turn_id, &request.text, choice.as_ref(), level)
                    .map_err(map_conversation_error)?;
                Ok(json!({
                    "entry_id": begun.user_entry.id,
                    "entry_sequence": begun.user_entry.sequence,
                    "entry_created_at": begun.user_entry.created_at,
                    "turn_id": begun.turn.id,
                    "epoch": self.store.store_epoch().map_err(map_store_error)?
                }))
            },
        )?;
        self.send_reply(&request.target, &reference)
    }

    /// Builds a send response from its cached references. The turn is read as it is
    /// now, and the epoch is the one recorded when the message was sent. A message
    /// that a history clear deleted comes back as its id and sequence with
    /// `cleared: true`, and its turn is `null`.
    fn send_reply(&self, target: &TargetParam, reference: &Value) -> Result<Value, RpcError> {
        let entry_id = reference_str(reference, "entry_id")?;
        let turn_id = reference_str(reference, "turn_id")?;
        let entry = match self
            .store
            .conversation_entry(entry_id)
            .map_err(map_store_error)?
        {
            Some(entry) => entry_json(&entry),
            None => cleared_reference_json(reference)?,
        };
        let turn = self
            .store
            .conversation_turn(turn_id)
            .map_err(map_conversation_error)?;
        Ok(json!({
            "authority": "none",
            "target": target,
            "entry": entry,
            "turn": turn.as_ref().map(|turn| self.turn_wire(turn)),
            "epoch": reference.get("epoch").cloned().unwrap_or(Value::Null)
        }))
    }

    /// Permanently deletes one conversation's history (DPES-1105). Refused with
    /// `conversation_busy` while a turn runs; the refusal is cached for the key, so a
    /// retry after the turn ends needs a new key.
    pub(super) fn conversations_clear(&self, params: Value) -> Result<Value, RpcError> {
        let request: ConversationClearParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "conversations.clear",
            &key,
            &request,
            || {
                let target = request.target.to_store();
                self.conversations.require_target(&target)?;
                let outcome = self
                    .store
                    .clear_conversation_history(&target)
                    .map_err(map_clear_error)?;
                Ok(json!({
                    "authority": "none",
                    "target": request.target,
                    "cleared": outcome.cleared,
                    "entries_deleted": outcome.entries_deleted,
                    "turns_deleted": outcome.turns_deleted,
                    "clear_generation": outcome.clear_generation,
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
        let entries = page
            .entries
            .iter()
            .map(|entry| entry_wire(&self.store, entry))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({
            "authority": "none",
            "target": request.target,
            "entries": entries,
            "more": more,
            "next_after_sequence": next_after_sequence,
            "latest_sequence": page.latest_sequence,
            "clear_generation": page.clear_generation,
            "turn": page.latest_turn.as_ref().map(|turn| self.turn_wire(turn)),
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
                    "turn": self.turn_wire(&turn),
                    "cancel_applied": applied,
                    "epoch": self.store.store_epoch().map_err(map_store_error)?
                }))
            },
        )
    }
}

fn map_clear_error(error: ConversationError) -> RpcError {
    match error {
        ConversationError::TurnRunning { .. } => RpcError::ConversationBusy,
        other => map_conversation_error(other),
    }
}

/// The reference a cleared message is answered with: its id, sequence and creation
/// time, and `cleared: true`. The body is never kept, so it is absent.
pub(super) fn cleared_reference_json(reference: &Value) -> Result<Value, RpcError> {
    Ok(json!({
        "id": reference_str(reference, "entry_id")?,
        "sequence": reference.get("entry_sequence").cloned().unwrap_or(Value::Null),
        "created_at": reference.get("entry_created_at").cloned().unwrap_or(Value::Null),
        "cleared": true
    }))
}

/// Reads a reference the daemon wrote itself. A missing field is an internal fault.
pub(super) fn reference_str<'a>(reference: &'a Value, field: &str) -> Result<&'a str, RpcError> {
    reference
        .get(field)
        .and_then(Value::as_str)
        .ok_or(RpcError::Internal)
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

/// An entry as `conversations.get` returns it. A turn refused for its version
/// also names the agent and version it refused (DPES-1106), so the desktop can
/// say which one to pin. Both are read from the turn the daemon recorded.
fn entry_wire(store: &SqliteStore, entry: &ConversationEntry) -> Result<Value, RpcError> {
    let mut value = entry_json(entry);
    if entry.error_code.as_deref() == Some("version_mismatch") {
        let turn = store
            .conversation_turn(&entry.turn_id)
            .map_err(map_conversation_error)?;
        if let Some(turn) = turn {
            value["runtime"] = json!({"name": turn.runtime_name, "version": turn.runtime_version});
        }
    }
    Ok(value)
}

impl DaemonApi {
    /// A turn as the wire shows it. `verified` says whether the version the turn
    /// recorded is one the daemon has verified; the desktop warns once per version
    /// when it is not (DPES-1106). The version itself is always recorded.
    fn turn_wire(&self, turn: &ConversationTurn) -> Value {
        let identity = RuntimeIdentity {
            name: turn.runtime_name.clone(),
            version: turn.runtime_version.clone(),
        };
        let mut value = turn_json(turn);
        value["verified"] = json!(self.conversations.verified(&identity));
        value
    }
}

fn turn_json(turn: &ConversationTurn) -> Value {
    json!({
        "id": turn.id,
        "state": turn.state.as_str(),
        "error_code": turn.error_code,
        "runtime": {"name": turn.runtime_name, "version": turn.runtime_version},
        "reply_bytes": turn.reply_bytes,
        // The level the turn ran with. Null for a turn from before permission
        // levels, and for a turn the agent refused before it ran.
        "permission_level": turn.permission_level.map(PermissionLevel::as_str),
        "permission_profile": turn.permission_profile,
        "created_at": turn.created_at,
        "updated_at": turn.updated_at
    })
}

#[cfg(test)]
#[path = "api_conversations_tests.rs"]
mod tests;
