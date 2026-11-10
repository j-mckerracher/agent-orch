//! Provider-reported token and cost usage (board item M7-06).
//!
//! Plan: /Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/docs/technical-implementation-plan.md.
//!
//! Each adapter lifts the usage its provider reports into one [`UsageReport`].
//! Nothing is estimated here and no price table exists in this crate. A field
//! the provider did not report is `None`; `None` means unknown and is never
//! stored or shown as zero.
//!
//! # Normalized meaning
//!
//! - `input_tokens` is fresh input only: it excludes cache reads and writes.
//!   Codex reports OpenAI semantics (`input_tokens` includes
//!   `cached_input_tokens`), so the cached part is subtracted. Claude Code and
//!   Pi already report fresh input.
//! - `output_tokens` includes reasoning tokens. `reasoning_tokens` is
//!   informational and must not be added to it.
//! - `cost` is the provider's or CLI's own USD figure in integer nano-USD. A
//!   reported cost of zero is treated as unknown, because local or unpriced
//!   models report 0 for "no price".
//!
//! Adapter coverage (spike result): Codex reports tokens only. Claude Code
//! reports tokens and a CLI-computed USD cost on its terminal `result` event.
//! Pi reports tokens and a runtime-computed USD cost on each assistant
//! `message_end`.

use serde_json::{Map, Value};

const NANOS_PER_USD: f64 = 1_000_000_000.0;
/// Reject absurd values (one billion USD) rather than saturate silently.
const MAX_COST_USD: f64 = 1_000_000_000.0;

/// A cost exactly as a provider reported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportedCost {
    /// Nano-units of `currency` (one billionth), an integer so events stay `Eq`.
    pub nanos: u64,
    /// ISO-4217 style code, uppercase.
    pub currency: String,
}

impl ReportedCost {
    /// Convert a positive, finite USD amount. Zero, negative, non-finite, and
    /// absurdly large values are unknown, not a cost.
    pub fn from_usd(amount: f64) -> Option<Self> {
        if !amount.is_finite() || amount <= 0.0 || amount > MAX_COST_USD {
            return None;
        }
        let nanos = (amount * NANOS_PER_USD).round() as u64;
        (nanos > 0).then(|| Self {
            nanos,
            currency: "USD".to_owned(),
        })
    }
}

/// One usage observation. `receipt_key` is stable for the observation, so a
/// store can ignore a repeat and replace a revised value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageReport {
    pub receipt_key: String,
    pub model: Option<String>,
    pub input_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub cost: Option<ReportedCost>,
    /// Provider timestamp in Unix milliseconds, when the record carries one.
    pub occurred_at_ms: Option<u64>,
}

impl UsageReport {
    fn has_any_tokens(&self) -> bool {
        [
            self.input_tokens,
            self.cache_read_tokens,
            self.cache_write_tokens,
            self.output_tokens,
        ]
        .iter()
        .any(Option::is_some)
    }

    /// A report with no token or cost figure says nothing and is dropped.
    fn keep(self) -> Option<Self> {
        (self.has_any_tokens() || self.cost.is_some()).then_some(self)
    }
}

fn count(object: &Map<String, Value>, key: &str) -> Option<u64> {
    object.get(key).and_then(Value::as_u64)
}

fn nonempty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty() && text.len() <= 200)
        .map(str::to_owned)
}

/// Codex `turn.completed`. `turn` is the 1-based turn ordinal in the run.
pub fn codex_turn_usage(record: &Value, turn: u64) -> Option<UsageReport> {
    let usage = record.get("usage")?.as_object()?;
    let cached = count(usage, "cached_input_tokens");
    let input = match (count(usage, "input_tokens"), cached) {
        (Some(total), Some(cached)) => total.checked_sub(cached),
        (total, None) => total,
        (None, Some(_)) => None,
    };
    UsageReport {
        receipt_key: format!("turn.completed:{turn}"),
        model: None,
        input_tokens: input,
        cache_read_tokens: cached,
        cache_write_tokens: count(usage, "cache_write_input_tokens"),
        output_tokens: count(usage, "output_tokens"),
        reasoning_tokens: count(usage, "reasoning_output_tokens"),
        cost: None,
        occurred_at_ms: None,
    }
    .keep()
}

/// Claude Code stream-json terminal `result` event.
pub fn claude_result_usage(record: &Value) -> Option<UsageReport> {
    let usage = record.get("usage").and_then(Value::as_object);
    let model = record
        .get("modelUsage")
        .and_then(Value::as_object)
        .filter(|models| models.len() == 1)
        .and_then(|models| models.keys().next().cloned());
    UsageReport {
        receipt_key: "result".to_owned(),
        model,
        input_tokens: usage.and_then(|u| count(u, "input_tokens")),
        cache_read_tokens: usage.and_then(|u| count(u, "cache_read_input_tokens")),
        cache_write_tokens: usage.and_then(|u| count(u, "cache_creation_input_tokens")),
        output_tokens: usage.and_then(|u| count(u, "output_tokens")),
        reasoning_tokens: usage
            .and_then(|u| u.get("output_tokens_details"))
            .and_then(Value::as_object)
            .and_then(|details| count(details, "thinking_tokens")),
        cost: record
            .get("total_cost_usd")
            .and_then(Value::as_f64)
            .and_then(ReportedCost::from_usd),
        occurred_at_ms: None,
    }
    .keep()
}

/// Pi `message_end` for an assistant message. Pi can finish several assistant
/// messages in one turn; each has its own `responseId`. A message whose token
/// counts are all zero reports nothing.
pub fn pi_message_end_usage(record: &Map<String, Value>) -> Option<UsageReport> {
    let message = record.get("message")?.as_object()?;
    if message.get("role")?.as_str()? != "assistant" {
        return None;
    }
    let usage = message.get("usage")?.as_object()?;
    let all_zero = ["input", "output", "cacheRead", "cacheWrite"]
        .iter()
        .all(|key| count(usage, key).unwrap_or(0) == 0);
    if all_zero {
        return None;
    }
    let tokens = |key: &str| count(usage, key);
    let timestamp = message.get("timestamp").and_then(Value::as_u64);
    let receipt_key = match nonempty_string(message.get("responseId")) {
        Some(id) => format!("response:{id}"),
        None => format!("message:{}", timestamp?),
    };
    UsageReport {
        receipt_key,
        model: nonempty_string(message.get("model")),
        input_tokens: tokens("input"),
        cache_read_tokens: tokens("cacheRead"),
        cache_write_tokens: tokens("cacheWrite"),
        output_tokens: tokens("output"),
        reasoning_tokens: None,
        cost: usage
            .get("cost")
            .and_then(Value::as_object)
            .and_then(|cost| cost.get("total"))
            .and_then(Value::as_f64)
            .and_then(ReportedCost::from_usd),
        occurred_at_ms: timestamp,
    }
    .keep()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn codex_recorded_turn_subtracts_cached_input() {
        let record: Value = serde_json::from_str(
            r#"{"type":"turn.completed","usage":{"input_tokens":96936,"cached_input_tokens":72192,"cache_write_input_tokens":0,"output_tokens":471,"reasoning_output_tokens":203}}"#,
        )
        .unwrap();
        let report = codex_turn_usage(&record, 1).unwrap();
        assert_eq!(report.receipt_key, "turn.completed:1");
        assert_eq!(report.input_tokens, Some(24_744));
        assert_eq!(report.cache_read_tokens, Some(72_192));
        assert_eq!(report.cache_write_tokens, Some(0));
        assert_eq!(report.output_tokens, Some(471));
        assert_eq!(report.reasoning_tokens, Some(203));
        assert_eq!(report.cost, None);
    }

    #[test]
    fn codex_inconsistent_or_absent_usage_is_unknown() {
        let inverted = json!({"usage":{"input_tokens":5,"cached_input_tokens":9}});
        let report = codex_turn_usage(&inverted, 1).unwrap();
        assert_eq!(report.input_tokens, None);
        assert_eq!(report.cache_read_tokens, Some(9));
        assert_eq!(codex_turn_usage(&json!({"type":"turn.completed"}), 1), None);
        assert_eq!(codex_turn_usage(&json!({"usage":{}}), 1), None);
    }

    #[test]
    fn claude_recorded_result_reports_tokens_and_cost() {
        let record = json!({
            "type":"result","total_cost_usd":0.0086388,
            "usage":{"input_tokens":2,"cache_creation_input_tokens":1360,
                "cache_read_input_tokens":2254,"output_tokens":69,
                "output_tokens_details":{"thinking_tokens":0}},
            "modelUsage":{"claude-opus-5-5":{"inputTokens":2}}
        });
        let report = claude_result_usage(&record).unwrap();
        assert_eq!(report.input_tokens, Some(2));
        assert_eq!(report.cache_write_tokens, Some(1360));
        assert_eq!(report.cache_read_tokens, Some(2254));
        assert_eq!(report.output_tokens, Some(69));
        assert_eq!(report.reasoning_tokens, Some(0));
        assert_eq!(report.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(
            report.cost,
            Some(ReportedCost {
                nanos: 8_638_800,
                currency: "USD".into()
            })
        );
    }

    #[test]
    fn claude_without_cost_or_usage_is_unknown() {
        let report = claude_result_usage(&json!({"usage":{"output_tokens":3}})).unwrap();
        assert_eq!(report.cost, None);
        assert_eq!(report.input_tokens, None);
        assert_eq!(
            claude_result_usage(&json!({"total_cost_usd":0.0,"usage":{}})),
            None
        );
    }

    #[test]
    fn pi_assistant_message_end_reports_and_zero_cost_is_unknown() {
        let record = json!({"type":"message_end","message":{
            "role":"assistant","responseId":"resp_1","timestamp":1_762_900_000_123u64,
            "model":"m1","usage":{"input":10,"output":20,"cacheRead":30,"cacheWrite":0,
                "totalTokens":60,"cost":{"total":0.00125}}}});
        let report = pi_message_end_usage(record.as_object().unwrap()).unwrap();
        assert_eq!(report.receipt_key, "response:resp_1");
        assert_eq!(report.cache_write_tokens, Some(0));
        assert_eq!(report.cost.as_ref().map(|c| c.nanos), Some(1_250_000));
        assert_eq!(report.occurred_at_ms, Some(1_762_900_000_123));

        let free = json!({"message":{"role":"assistant","responseId":"r","usage":{
            "input":1,"output":1,"cost":{"total":0}}}});
        assert_eq!(
            pi_message_end_usage(free.as_object().unwrap())
                .unwrap()
                .cost,
            None
        );
    }

    #[test]
    fn pi_user_messages_and_empty_usage_report_nothing() {
        let user = json!({"message":{"role":"user","usage":{"input":5}}});
        assert_eq!(pi_message_end_usage(user.as_object().unwrap()), None);
        let empty = json!({"message":{"role":"assistant","responseId":"r","usage":{
            "input":0,"output":0,"cacheRead":0,"cacheWrite":0,"cost":{"total":0}}}});
        assert_eq!(pi_message_end_usage(empty.as_object().unwrap()), None);
        let keyless = json!({"message":{"role":"assistant","usage":{"input":1}}});
        assert_eq!(pi_message_end_usage(keyless.as_object().unwrap()), None);
    }

    #[test]
    fn cost_conversion_rejects_non_prices() {
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, 2e9, 1e-12] {
            assert_eq!(ReportedCost::from_usd(bad), None, "{bad}");
        }
    }
}
