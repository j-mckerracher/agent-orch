//! `sessions.thinking`: the agent's recorded thinking for one session, paged by cursor.
//!
//! Contract:
//! - Method `sessions.thinking`; capability `sessions`; read-only.
//! - Params: `{session_id, after?, limit?}`. `after` (default 0) returns only chunks newer
//!   than that sequence; `limit` defaults to 100 and is clamped by the store.
//! - Result: `{chunks: [{sequence, body}], next_cursor, truncated}`. Pass `next_cursor`
//!   back as `after`. `truncated` means the session's size cap was reached.
//! - A session with no thinking returns an empty page. Thinking is NOT redacted.

use serde::Deserialize;

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThinkingParams {
    session_id: String,
    after: Option<u64>,
    limit: Option<usize>,
}

impl DaemonApi {
    pub(super) fn session_thinking(&self, params: Value) -> Result<Value, RpcError> {
        let params: ThinkingParams = parse_params(params)?;
        let page = self
            .store
            .session_thinking(
                &params.session_id,
                params.after.unwrap_or(0),
                params.limit.unwrap_or(100),
            )
            .map_err(map_store_error)?;
        Ok(json!({
            "chunks": page
                .chunks
                .iter()
                .map(|chunk| json!({"sequence": chunk.sequence, "body": chunk.body}))
                .collect::<Vec<_>>(),
            "next_cursor": page.next_cursor,
            "truncated": page.truncated,
        }))
    }
}
