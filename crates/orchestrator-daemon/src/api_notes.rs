//! The user's orchestrator notes (DPES-1105 phase 5b): `notes.list` and `notes.set`.
//!
//! A note is one short line of user text that the conversation snapshot lists as data. It is
//! persistent text the next prompt carries, so only a user session may write one (`notes.set`
//! is routed user-only in the ipc crate, and the transport refuses a session without the
//! approver secret before any parameter is read). The orchestrator proposes nothing here:
//! no action kind reaches a note. The core crate refuses what a note may not hold; a refusal
//! is a stable code in the log, never the text.

use orchestrator_core::notes::{check_notes, MAX_NOTES_PER_SCOPE, MAX_NOTE_BYTES};
use orchestrator_store::WorkstreamStoreError;

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    scope: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SetParams {
    scope: String,
    notes: Vec<String>,
    idempotency_key: String,
}

impl DaemonApi {
    /// `notes.list`: one scope's notes in order, with the limits the editor must respect.
    /// `scope` is `global` or `workstream:` plus a workstream id. Open to every session that
    /// negotiated `standing_approvals`.
    pub(super) fn notes_list(&self, params: Value) -> Result<Value, RpcError> {
        let request: ListParams = parse_params(params)?;
        self.notes_wire(&request.scope)
    }

    /// `notes.set`: replaces the whole list of one scope in one transaction. User-only. A
    /// workstream scope must name an existing workstream. A refused list changes nothing. The
    /// reply is `{scope, count}` and holds no note body: the reply is cached under the
    /// idempotency key, and a body kept there would outlive a clear. `notes.list` reads bodies.
    pub(super) fn notes_set(&self, params: Value) -> Result<Value, RpcError> {
        let request: SetParams = parse_params(params)?;
        if let Err(refusal) = check_notes(&request.notes) {
            self.config
                .diagnostics
                .warn("api", "note_refused", &[("code", refusal.code())]);
            return Err(RpcError::InvalidParams);
        }
        self.check_notes_scope(&request.scope)?;
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "notes.set",
            &request.idempotency_key,
            &request,
            || {
                self.store
                    .set_orchestrator_notes(&request.scope, &request.notes)
                    .map_err(map_store_error)?;
                Ok(json!({"scope": request.scope, "count": request.notes.len()}))
            },
        )
    }

    /// `global`, or `workstream:` plus the id of a workstream that exists.
    fn check_notes_scope(&self, scope: &str) -> Result<(), RpcError> {
        if scope == "global" {
            return Ok(());
        }
        let id = scope
            .strip_prefix("workstream:")
            .filter(|id| !id.is_empty())
            .ok_or(RpcError::InvalidParams)?;
        match self.store.get_workstream(id) {
            Ok(_) => Ok(()),
            Err(WorkstreamStoreError::NotFound { .. }) => Err(RpcError::NotFound),
            Err(error) => Err(internal("notes", &format!("{error:?}"))),
        }
    }

    fn notes_wire(&self, scope: &str) -> Result<Value, RpcError> {
        self.check_notes_scope(scope)?;
        let notes = self
            .store
            .orchestrator_notes(scope)
            .map_err(map_store_error)?;
        let notes: Vec<Value> = notes
            .iter()
            .enumerate()
            .map(|(position, body)| json!({"position": position, "body": body}))
            .collect();
        Ok(json!({
            "scope": scope,
            "notes": notes,
            "limits": {"max_notes": MAX_NOTES_PER_SCOPE, "max_bytes": MAX_NOTE_BYTES},
        }))
    }
}
