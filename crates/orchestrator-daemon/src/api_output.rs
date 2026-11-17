//! `sessions.output_tail`: a bounded, read-only view of a session's recent
//! output (DPES-848).
//!
//! Contract:
//! - Method `sessions.output_tail`; capability `sessions` (the same as
//!   `sessions.get` and `sessions.events`); read-only; served only on the
//!   owner-authenticated channel. It never writes, signals or otherwise
//!   affects the agent.
//! - Params: `{task_id, session_id, limit?, after_sequence?}`. `limit` defaults
//!   to 40 and values above 100 are clamped to 100; 0 or a negative `limit` is
//!   `InvalidParams`. `after_sequence` (default 0, negative is `InvalidParams`)
//!   returns only lines newer than that sequence.
//! - Result: `{task_id, session_id, available, ended, limit, lines,
//!   truncated_lines, last_sequence}`. Each line is `{sequence, stream,
//!   text, line_truncated}` (`stream` is `stdout` or `stderr`; `sequence` is
//!   monotonic per session; `line_truncated` means the line was cut to its byte
//!   cap). `truncated_lines` means newer-than-cursor lines exist that are not
//!   returned (dropped from the ring or beyond `limit`). Pass `last_sequence`
//!   back as `after_sequence` to see only what is new.
//! - A session the store does not know is `NotFound`. A known session with no
//!   retained output (not started yet, evicted after ending, or daemon
//!   restarted) returns `available: false` and no lines, with `ended` taken
//!   from the session state.
//! - Output is NOT redacted: it may contain secrets. Retained output lives
//!   only in memory; see `output_ring.rs` for the bounds.

use orchestrator_core::task::SessionState;
use serde::Deserialize;

use super::*;
use crate::executor::output_ring::{DEFAULT_TAIL_LINES, MAX_TAIL_LINES};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputTailParams {
    task_id: String,
    session_id: String,
    limit: Option<i64>,
    after_sequence: Option<i64>,
}

impl DaemonApi {
    pub(super) fn session_output_tail(&self, params: Value) -> Result<Value, RpcError> {
        let params: OutputTailParams = parse_params(params)?;
        let limit = match params.limit {
            None => DEFAULT_TAIL_LINES,
            Some(limit) if limit < 1 => return Err(RpcError::InvalidParams),
            Some(limit) => usize::try_from(limit).map_or(MAX_TAIL_LINES, |l| l.min(MAX_TAIL_LINES)),
        };
        let after = match params.after_sequence {
            None => 0,
            Some(after) => u64::try_from(after).map_err(|_| RpcError::InvalidParams)?,
        };
        let task = self.store.get(&params.task_id).map_err(map_store_error)?;
        let session = task
            .sessions
            .iter()
            .find(|session| session.id == params.session_id)
            .ok_or(RpcError::NotFound)?;
        let tail = self
            .executor
            .as_ref()
            .and_then(|executor| executor.output().tail(&params.session_id, limit, after));
        let session_over = matches!(session.state, SessionState::Exited | SessionState::Aborted);
        let (available, ended, lines, truncated_lines, last_sequence) = match tail {
            Some(tail) => (
                true,
                tail.ended || session_over,
                tail.lines
                    .into_iter()
                    .map(|line| {
                        json!({
                            "sequence": line.sequence,
                            "stream": line.stream.as_str(),
                            "text": line.text,
                            "line_truncated": line.line_truncated,
                        })
                    })
                    .collect::<Vec<_>>(),
                tail.truncated_lines,
                tail.last_sequence,
            ),
            None => (false, session_over, Vec::new(), false, 0),
        };
        Ok(json!({
            "task_id": params.task_id,
            "session_id": params.session_id,
            "available": available,
            "ended": ended,
            "limit": limit,
            "lines": lines,
            "truncated_lines": truncated_lines,
            "last_sequence": last_sequence,
        }))
    }
}
