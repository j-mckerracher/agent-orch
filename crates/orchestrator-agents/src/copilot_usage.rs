//! Usage the Copilot CLI writes to its `--usage-output-file` (DPES-1105).
//!
//! Copilot reports premium requests and AI credits, never a USD amount, so
//! these figures keep their own units and never enter
//! [`crate::usage::ReportedCost`]. A figure the file does not carry is `None`,
//! which means not reported. It is never zero, and an unreadable file is not
//! usage at all.
//!
//! Shape verified against Copilot CLI 1.0.109 (`tests/fixtures/copilot/usage.json`).

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

/// `totalNanoAiu` counts nano-AI-credits: one AI credit is 1e9 of them.
const NANO_AIU_PER_CREDIT: f64 = 1_000_000_000.0;

/// Token counts for one model the session used.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CopilotModelUsage {
    pub model_id: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}

/// Usage for one Copilot session.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CopilotUsage {
    /// `totalPremiumRequestCost`: premium requests the session was charged.
    pub premium_requests: Option<f64>,
    /// `totalNanoAiu` in AI credits. The division keeps the nine decimal
    /// places the file gives.
    pub ai_credits: Option<f64>,
    /// Per-model tokens from `providerModelMetrics`.
    pub models: Vec<CopilotModelUsage>,
}

/// Parses usage JSON. `None` when the text is not usage JSON, or when it
/// carries none of the figures this module reads.
pub fn parse_usage(text: &str) -> Option<CopilotUsage> {
    let root: Value = serde_json::from_str(text).ok()?;
    let object = root.as_object()?;
    let premium_requests = object
        .get("totalPremiumRequestCost")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0);
    let ai_credits = object
        .get("totalNanoAiu")
        .and_then(Value::as_u64)
        .map(|nano| nano as f64 / NANO_AIU_PER_CREDIT);
    let models: Vec<CopilotModelUsage> = object
        .get("providerModelMetrics")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(model_usage)
        .collect();
    if premium_requests.is_none() && ai_credits.is_none() && models.is_empty() {
        return None;
    }
    Some(CopilotUsage {
        premium_requests,
        ai_credits,
        models,
    })
}

fn model_usage(entry: &Value) -> Option<CopilotModelUsage> {
    let model_id = entry
        .get("modelId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())?
        .to_owned();
    let usage = entry.get("metrics")?.get("usage")?.as_object()?;
    let count = |key: &str| usage.get(key).and_then(Value::as_u64);
    Some(CopilotModelUsage {
        model_id,
        input_tokens: count("inputTokens"),
        output_tokens: count("outputTokens"),
        cache_read_tokens: count("cacheReadTokens"),
        cache_write_tokens: count("cacheWriteTokens"),
    })
}

/// Reads and parses the usage file. A missing or unreadable file is `None`.
pub fn read_usage(path: &Path) -> Option<CopilotUsage> {
    parse_usage(&std::fs::read_to_string(path).ok()?)
}
