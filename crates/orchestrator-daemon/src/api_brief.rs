//! `monitoring.brief` and `monitoring.brief_ack` (DPES-1105 phase 5b).
//!
//! Contract (plan: `plan-5.md` 7.2 and 7.4):
//! - Capability `standing_approvals`. Both methods are open to any session with it. The brief is
//!   read-only. The ack only clears the closed window, deletes no data and grants no authority,
//!   so it is not user-only (the `monitoring.ack` precedent).
//! - While the mode is `away` the window is live: `[away_since_ms, now]` with `open: true`.
//!   Otherwise a closed window that the user has not acknowledged is shown, else `null`.
//! - The brief holds counts, ids, short titles, state words and project codes. Every title goes
//!   through the same redaction the wake notices use. No report body, task output, failure
//!   reason or objective beyond its first 80-byte line is read. The brief is shown to the user
//!   and is never put into a snapshot or a prompt.

use orchestrator_core::return_brief::{cut_title, Brief, BriefItem};
use orchestrator_store::AwayWindows;

use super::actions::project_code;
use super::wake_events::redact;
use super::*;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AckParams {
    idempotency_key: String,
}

impl DaemonApi {
    /// `monitoring.brief`: `{"authority": "none", "brief": Brief | null}`.
    pub(super) fn monitoring_brief(&self, params: Value) -> Result<Value, RpcError> {
        require_empty_params(&params)?;
        let windows = self.store.away_windows().map_err(map_store_error)?;
        let now = self.now_ms();
        let brief = match window_of(&windows, now) {
            Some((from, to, open)) => Some(self.build_brief(from, to, open, now)?),
            None => None,
        };
        Ok(json!({"authority": "none", "brief": brief}))
    }

    /// `monitoring.brief_ack`: clears the closed window. Idempotent. The live window of a user
    /// who is still away stays.
    pub(super) fn monitoring_brief_ack(&self, params: Value) -> Result<Value, RpcError> {
        let request: AckParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "monitoring.brief_ack",
            &key,
            &request,
            || {
                self.store.ack_brief().map_err(map_store_error)?;
                Ok(json!({"authority": "none", "brief_pending": false}))
            },
        )
    }

    fn build_brief(&self, from: u64, to: u64, open: bool, now: u64) -> Result<Brief, RpcError> {
        let mut brief = self
            .store
            .return_brief(from, to, open, now)
            .map_err(map_store_error)?;
        for item in brief.waiting.iter_mut().chain(brief.attention.iter_mut()) {
            safe_item(item);
        }
        for grant in &mut brief.grants {
            if grant.scope_kind == "workstream" {
                // The store holds the workstream id; the brief shows its redacted title.
                let title = self
                    .store
                    .get_workstream(&grant.scope_label)
                    .map(|workstream| workstream.title)
                    .unwrap_or_default();
                grant.scope_label = cut_title(&redact(&title));
            }
        }
        Ok(brief.capped())
    }
}

/// The window to report and whether it is still open: the live one while away, else the
/// closed one nobody acknowledged.
pub(super) fn window_of(windows: &AwayWindows, now: u64) -> Option<(u64, u64, bool)> {
    if let Some(since) = windows.away_since_ms {
        return Some((since, now.max(since), true));
    }
    match (windows.brief_from_ms, windows.brief_to_ms) {
        (Some(from), Some(to)) => Some((from, to, false)),
        _ => None,
    }
}

/// Redacts the title and turns the raw project id into its code. The store hands over both raw.
fn safe_item(item: &mut BriefItem) {
    item.title = cut_title(&redact(&item.title));
    item.project = item.project.take().map(|id| project_code(&id));
}
