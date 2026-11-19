//! `monitoring.get`, `monitoring.set` and `monitoring.ack` (DPES-1105 phase 3c). The return
//! brief methods are in `api_brief.rs`.
//!
//! Contract (plan: `plan-3.md` 6.6):
//! - Capability `monitoring`. `monitoring.get` and `monitoring.ack` are open to any session
//!   with the capability: the first reads, the second only marks wake records seen and changes
//!   no decision. `monitoring.set` is user-only: the transport refuses a session that did not
//!   prove the approver secret before this handler runs, so the handler has no check of its own.
//! - `monitoring.set` takes `{idempotency_key, mode}` and sets only the mode: `on`, `quiet`,
//!   `off` or `away`. An unknown word is `InvalidParams`. `away` adds no authority: it is quiet
//!   plus a return brief window, and every cap, the token budget, the debounce and the load gate
//!   apply to it as to quiet. The caps and intervals go through `settings.set`.
//! - Every result states `"authority": "none"`. No path and no id of a task appears in any
//!   result beyond the ids the wake records already carry.

use std::collections::HashSet;

use orchestrator_core::supervision::{admit, Admission, MonitoringMode};
use orchestrator_store::{ConversationTarget, WakeOutcome, WakeRecord, WakeState, WAKE_PAGE_SIZE};

use super::conversations::TargetParam;
use super::wake::{scope_of, target_of};
use super::*;

const LOG: &str = "monitoring";
const GLOBAL_SCOPE: &str = "conversation:global";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SetModeParams {
    idempotency_key: String,
    mode: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AckParams {
    target: TargetParam,
    idempotency_key: String,
}

impl DaemonApi {
    pub(super) fn monitoring_get(&self, params: Value) -> Result<Value, RpcError> {
        require_empty_params(&params)?;
        self.monitoring_body()
    }

    pub(super) fn monitoring_set(&self, params: Value) -> Result<Value, RpcError> {
        let request: SetModeParams = parse_params(params)?;
        let mode = MonitoringMode::parse(&request.mode).ok_or(RpcError::InvalidParams)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "monitoring.set",
            &key,
            &request,
            || {
                let before = self
                    .store
                    .set_monitoring_mode(mode, self.now_ms())
                    .map_err(map_store_error)?;
                if before != mode {
                    if let Some(wake) = &self.wake {
                        wake.mode_changed(mode);
                    }
                    self.config.diagnostics.info(
                        LOG,
                        "monitoring_mode_changed",
                        &[("mode", mode.as_str())],
                    );
                }
                self.monitoring_body()
            },
        )
    }

    pub(super) fn monitoring_ack(&self, params: Value) -> Result<Value, RpcError> {
        let request: AckParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "monitoring.ack",
            &key,
            &request,
            || {
                let target = request.target.to_store();
                self.conversations.require_target(&target)?;
                let acked = self
                    .store
                    .ack_wake_records(&scope_of(&target))
                    .map_err(map_store_error)?;
                Ok(json!({"authority": "none", "acked": acked}))
            },
        )
    }

    /// The newest wake records of a target, oldest first, and whether older ones exist.
    pub(super) fn wake_records(
        &self,
        target: &ConversationTarget,
    ) -> Result<(Vec<WakeRecord>, bool), RpcError> {
        self.store
            .wake_records_page(&scope_of(target), WAKE_PAGE_SIZE)
            .map_err(map_store_error)
    }

    pub(super) fn now_ms(&self) -> u64 {
        self.wake
            .as_ref()
            .map_or_else(crate::executor::unix_now_ms, |wake| wake.now_ms())
    }

    /// The mode, the paused targets and the unseen counts. Paused is computed on read from the
    /// ledger, so nothing is stored for it and it clears itself when the window frees.
    fn monitoring_body(&self) -> Result<Value, RpcError> {
        let settings = self
            .store
            .orchestrator_settings()
            .map_err(map_store_error)?;
        let now = self.now_ms();
        let mut scopes = vec![GLOBAL_SCOPE.to_owned()];
        let workstreams = self
            .store
            .list_workstreams(None)
            .map_err(|_| RpcError::Internal)?;
        scopes.extend(
            workstreams
                .into_iter()
                .map(|workstream| scope_of(&ConversationTarget::Workstream(workstream.id))),
        );
        let mut paused = Vec::new();
        for scope in &scopes {
            let usage = self.store.wake_usage(scope, now).map_err(map_store_error)?;
            if let Admission::Paused { reason, until_ms } = admit(&settings.wake_limits, &usage) {
                paused.push(json!({
                    "target": target_wire(scope),
                    "reason": reason.as_str(),
                    "until": until_ms.map(rfc3339),
                }));
            }
        }
        let unseen: Vec<Value> = self
            .store
            .unseen_wake_counts()
            .map_err(map_store_error)?
            .into_iter()
            // A coordinator's records (DPES-1108 phase 7) are shown in its own conversation; the
            // monitoring targets name Global and workstreams only, so they are not counted here
            // rather than read as Global's.
            .filter(|(scope, _)| !matches!(target_of(scope), Some(ConversationTarget::Project(_))))
            .map(|(scope, count)| json!({"target": target_wire(&scope), "count": count}))
            .collect();
        let away = self.store.away_windows().map_err(map_store_error)?;
        Ok(json!({
            "authority": "none",
            "mode": settings.mode.as_str(),
            "away_since_ms": away.away_since_ms,
            "brief_pending": away.brief_to_ms.is_some(),
            "paused": paused,
            "unseen": unseen,
            "epoch": self.store.store_epoch().map_err(map_store_error)?,
        }))
    }
}

/// The wire target of a scope, the shape `conversations.get` takes. An unknown scope reads as
/// Global.
fn target_wire(scope: &str) -> Value {
    match target_of(scope) {
        Some(ConversationTarget::Workstream(id)) => {
            json!({"kind": "workstream", "workstream_id": id})
        }
        _ => json!({"kind": "global"}),
    }
}

/// A wake record as `conversations.get` carries it. `notify` is true when the record counts
/// toward the badge, the same rule as `unseen_wake_counts`.
pub(super) fn wake_record_wire(record: &WakeRecord) -> Value {
    // An outcome that a quiet reply did not answer always notifies: its notice is the only word
    // the user gets about it.
    let notify = matches!(
        record.state,
        WakeState::Noticed | WakeState::Skipped | WakeState::Failed
    ) || (record.state == WakeState::Handled
        && record.outcome == Some(WakeOutcome::Unanswered))
        || (record.state == WakeState::Handled
            && record.outcome == Some(WakeOutcome::Replied)
            && record.notify_reply);
    json!({
        "id": record.id,
        "event": record.kind.as_str(),
        "subject": record.subject,
        "notice": record.notice,
        "state": record.state.as_str(),
        "outcome": record.outcome.map(WakeOutcome::as_str),
        "skip_reason": record.skip_reason.map(|reason| reason.as_str()),
        "turn_id": record.turn_id,
        "anchor_sequence": record.anchor_sequence,
        "notify": notify,
        "seen": record.seen,
        "created_at": record.created_at,
    })
}

/// The turns whose whole reply was the quiet token. An unanswered record came from a quiet
/// reply too, so its turn's reply entries stay out of the wire as well.
pub(super) fn quiet_turn_ids(records: &[WakeRecord]) -> HashSet<&str> {
    records
        .iter()
        .filter(|record| {
            record.state == WakeState::Handled
                && matches!(
                    record.outcome,
                    Some(WakeOutcome::Quiet | WakeOutcome::Unanswered)
                )
        })
        .filter_map(|record| record.turn_id.as_deref())
        .collect()
}

/// Unix milliseconds as RFC 3339 UTC, to the second.
pub(super) fn rfc3339(ms: u64) -> String {
    let secs = ms / 1000;
    let rest = secs % 86_400;
    // Civil date from the day count (Howard Hinnant's algorithm).
    let days = i64::try_from(secs / 86_400).unwrap_or(0) + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}
