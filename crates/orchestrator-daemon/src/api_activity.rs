//! `sessions.activity`: what one session did and is doing, paged by cursor (DPES-1044).
//!
//! Contract:
//! - Method `sessions.activity`; capability `sessions`; read-only.
//! - Params: `{session_id, after?, limit?}`. `after` (default 0) returns only entries newer
//!   than that sequence; `limit` defaults to 100 and is clamped by the store.
//! - Result: `{entries: [{sequence, kind, body}], next_cursor, truncated, live}`. `kind` is
//!   `thinking`, `tool_started` or `tool_finished`, all in one sequence order. A tool body is
//!   a JSON object text: `{"id","name","summary"}` for a start, `{"id","ok","summary"}` for an
//!   end, `{"truncated":true}` for the cap marker. Tool text is redacted and bounded; thinking
//!   is NOT redacted. Pass `next_cursor` back as `after`. `truncated` means a cap was reached.
//! - `live` is `{phase, tool, phase_ms, idle_ms}` while the session runs, else `null`.
//! - A session with no activity returns an empty page.

use serde::Deserialize;

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActivityParams {
    session_id: String,
    after: Option<u64>,
    limit: Option<usize>,
}

impl DaemonApi {
    pub(super) fn session_activity(&self, params: Value) -> Result<Value, RpcError> {
        let params: ActivityParams = parse_params(params)?;
        // The snapshot is read first: a row stored after it is still returned by the next poll,
        // while the other order could show a finished tool as running.
        let live = self.conversations.board().snapshot(&params.session_id);
        let page = self
            .store
            .session_activity(
                &params.session_id,
                params.after.unwrap_or(0),
                params.limit.unwrap_or(100),
            )
            .map_err(map_store_error)?;
        Ok(json!({
            "entries": page
                .entries
                .iter()
                .map(|entry| json!({"sequence": entry.sequence, "kind": entry.kind, "body": entry.body}))
                .collect::<Vec<_>>(),
            "next_cursor": page.next_cursor,
            "truncated": page.truncated,
            "live": live,
        }))
    }
}
