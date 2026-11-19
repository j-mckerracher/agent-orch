//! Preferences and learnings (DPES-1108 phase 6): `memory.list`, `memory.set`, `memory.remove`
//! and `memory.restore`.
//!
//! A document is the keyed entries of one (kind, scope): the shared preferences are
//! `preference` in `global`, a workstream's or a project's are `preference` in
//! `workstream:<id>` or `project:<id>`, and learnings are `learning` in any of those scopes.
//! Every turn of a conversation reads its documents, so the writes here are user-only (routed
//! user-only in the ipc crate; the transport refuses a session without the approver secret before
//! any parameter is read). An orchestrator writes only through an approved `memory.set`
//! proposal (`api_actions.rs`). The core crate refuses what an entry may not hold; a refusal is a
//! stable code in the log, never the text, and no reply holds entry text, because replies are
//! cached under their idempotency key and would outlive a remove.

use orchestrator_core::memory::{
    budget, check_entry, check_key, is_stale, MemoryKind, MemoryScope, OverBudget,
    LEARNING_STALE_AFTER_DAYS, MAX_KEY_BYTES, MAX_TEXT_BYTES,
};
use orchestrator_store::{
    MemoryEntry, MemoryRestoreOutcome, MemoryWriteOutcome, MemoryWriter, WorkstreamStoreError,
};

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    kind: String,
    scope: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SetParams {
    kind: String,
    scope: String,
    key: String,
    text: String,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RemoveParams {
    kind: String,
    scope: String,
    key: String,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RestoreParams {
    archive_id: i64,
    idempotency_key: String,
}

fn over_budget(over: OverBudget) -> (Value, Value) {
    (json!(over.bytes), json!(over.budget))
}

impl DaemonApi {
    /// `memory.list`: one document's entries, most recently confirmed first, with its size,
    /// its budget, the newest archived entries and the limits the editor must respect. Open to
    /// every session that negotiated `standing_approvals`.
    pub(super) fn memory_list(&self, params: Value) -> Result<Value, RpcError> {
        let request: ListParams = parse_params(params)?;
        let (kind, scope) = self.memory_document(&request.kind, &request.scope)?;
        let entries = self
            .store
            .memory_entries(kind, &scope)
            .map_err(map_store_error)?;
        let archived = self
            .store
            .memory_archive(kind, &scope)
            .map_err(map_store_error)?;
        let now = crate::executor::unix_now_ms();
        let bytes: usize = entries.iter().map(MemoryEntry::bytes).sum();
        let entries: Vec<Value> = entries
            .iter()
            .map(|entry| {
                json!({
                    "key": entry.key,
                    "text": entry.text,
                    "written_by": entry.written_by.as_str(),
                    "created_at_ms": entry.created_at_ms,
                    "confirmed_at_ms": entry.confirmed_at_ms,
                    "stale": is_stale(kind, entry.confirmed_at_ms, now),
                })
            })
            .collect();
        let archived: Vec<Value> = archived
            .iter()
            .map(|entry| {
                json!({
                    "id": entry.id,
                    "key": entry.key,
                    "text": entry.text,
                    "written_by": entry.written_by.as_str(),
                    "archived_at_ms": entry.archived_at_ms,
                    "reason": entry.reason,
                    "restored": entry.restored,
                })
            })
            .collect();
        let stale_after_days = match kind {
            MemoryKind::Preference => Value::Null,
            MemoryKind::Learning => json!(LEARNING_STALE_AFTER_DAYS),
        };
        Ok(json!({
            "kind": kind.as_str(),
            "scope": request.scope,
            "entries": entries,
            "bytes": bytes,
            "budget": budget(kind, &scope),
            "archived": archived,
            "limits": {
                "max_key_bytes": MAX_KEY_BYTES,
                "max_text_bytes": MAX_TEXT_BYTES,
                "stale_after_days": stale_after_days,
            },
        }))
    }

    /// `memory.set`: writes one entry in place. User-only. The same key updates the entry (the
    /// old text is archived), the same key and text only confirms it. A write that would grow
    /// the document past its budget writes nothing and answers `over_budget` with the size it
    /// would have had. A refused key or text is `invalid_params` and changes nothing.
    pub(super) fn memory_set(&self, params: Value) -> Result<Value, RpcError> {
        let request: SetParams = parse_params(params)?;
        if let Err(refusal) = check_entry(&request.key, &request.text) {
            self.config.diagnostics.warn(
                "api",
                "memory_entry_refused",
                &[("code", refusal.code())],
            );
            return Err(RpcError::InvalidParams);
        }
        let (kind, scope) = self.memory_document(&request.kind, &request.scope)?;
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "memory.set",
            &request.idempotency_key,
            &request,
            || {
                let outcome = self
                    .store
                    .set_memory_entry(
                        kind,
                        &scope,
                        &request.key,
                        &request.text,
                        MemoryWriter::User,
                        crate::executor::unix_now_ms(),
                    )
                    .map_err(map_store_error)?;
                let (bytes, budget) = match outcome {
                    MemoryWriteOutcome::OverBudget(over) => over_budget(over),
                    _ => {
                        let entries = self
                            .store
                            .memory_entries(kind, &scope)
                            .map_err(map_store_error)?;
                        let bytes: usize = entries.iter().map(MemoryEntry::bytes).sum();
                        (json!(bytes), json!(budget(kind, &scope)))
                    }
                };
                Ok(json!({
                    "kind": kind.as_str(),
                    "scope": request.scope,
                    "key": request.key,
                    "outcome": outcome.as_str(),
                    "bytes": bytes,
                    "budget": budget,
                }))
            },
        )
    }

    /// `memory.remove`: moves one entry to the archive. User-only. `removed` is false when the
    /// document has no such key.
    pub(super) fn memory_remove(&self, params: Value) -> Result<Value, RpcError> {
        let request: RemoveParams = parse_params(params)?;
        check_key(&request.key).map_err(|_| RpcError::InvalidParams)?;
        let (kind, scope) = self.memory_document(&request.kind, &request.scope)?;
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "memory.remove",
            &request.idempotency_key,
            &request,
            || {
                let removed = self
                    .store
                    .remove_memory_entry(
                        kind,
                        &scope,
                        &request.key,
                        MemoryWriter::User,
                        crate::executor::unix_now_ms(),
                    )
                    .map_err(map_store_error)?;
                Ok(json!({
                    "kind": kind.as_str(),
                    "scope": request.scope,
                    "key": request.key,
                    "removed": removed,
                }))
            },
        )
    }

    /// `memory.restore`: puts an archived entry back, as confirmed now. User-only. `conflict`
    /// when its key is in use again or the row was restored before; `not_found` for an unknown
    /// id; `over_budget` (nothing written) when it no longer fits.
    pub(super) fn memory_restore(&self, params: Value) -> Result<Value, RpcError> {
        let request: RestoreParams = parse_params(params)?;
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "memory.restore",
            &request.idempotency_key,
            &request,
            || {
                let (outcome, _) = self
                    .store
                    .restore_memory_entry(request.archive_id, crate::executor::unix_now_ms())
                    .map_err(map_store_error)?;
                match outcome {
                    MemoryRestoreOutcome::Restored => Ok(json!({
                        "archive_id": request.archive_id,
                        "outcome": "restored",
                    })),
                    MemoryRestoreOutcome::OverBudget(over) => {
                        let (bytes, budget) = over_budget(over);
                        Ok(json!({
                            "archive_id": request.archive_id,
                            "outcome": "over_budget",
                            "bytes": bytes,
                            "budget": budget,
                        }))
                    }
                    MemoryRestoreOutcome::KeyInUse | MemoryRestoreOutcome::AlreadyRestored => {
                        Err(RpcError::Conflict)
                    }
                    MemoryRestoreOutcome::NotFound => Err(RpcError::NotFound),
                }
            },
        )
    }

    /// The document `kind` and `scope` name. The scope is `global`, `workstream:` plus the id of
    /// a workstream that exists, or `project:` plus the id of a registered project.
    fn memory_document(
        &self,
        kind: &str,
        scope: &str,
    ) -> Result<(MemoryKind, MemoryScope), RpcError> {
        let kind = MemoryKind::parse(kind).ok_or(RpcError::InvalidParams)?;
        let scope = MemoryScope::parse(scope).ok_or(RpcError::InvalidParams)?;
        match &scope {
            MemoryScope::Global => {}
            MemoryScope::Workstream(id) => match self.store.get_workstream(id) {
                Ok(_) => {}
                Err(WorkstreamStoreError::NotFound { .. }) => return Err(RpcError::NotFound),
                Err(error) => return Err(internal("memory", &format!("{error:?}"))),
            },
            MemoryScope::Project(id) => match self.store.get_project(id) {
                Ok(_) => {}
                Err(StoreError::RecordNotFound { .. }) => return Err(RpcError::NotFound),
                Err(error) => return Err(internal("memory", &error)),
            },
        }
        Ok((kind, scope))
    }
}
