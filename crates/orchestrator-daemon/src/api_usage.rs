//! `usage.summary`: token and cost totals for one local day.
//!
//! Contract (see docs/usage-ledger.md):
//!
//! * Params: `timezone` (IANA name or label echoed back, 1..=64 bytes),
//!   `utc_offset_seconds` (-43200..=50400, the client's current offset), and
//!   optionally `now_unix_ms` and a pair `window_start_unix_ms` /
//!   `window_end_unix_ms`. Without the pair the window is the local day that
//!   contains `now` (the daemon clock by default), `[local midnight, next
//!   local midnight)`. The pair lets a client pin a DST day of 23 or 25 hours;
//!   its span is limited to 26 hours.
//! * The daemon has no timezone database, so the offset is taken from the
//!   client and applied as fixed for the window.
//! * Result: `window`, `totals`, and `providers[]`. Each bucket reports session
//!   coverage, `token_status` and `cost_status` (`unknown` when nothing was
//!   reported, `partial` when only some sessions reported, `complete`), and the
//!   `reported` and `estimated` figures separately. A token figure that no
//!   receipt reported is `null`, never 0. Costs are a list of
//!   `{currency, amount}` with decimal-string amounts, never merged across
//!   currencies; an empty list means no cost was reported.

use std::time::{SystemTime, UNIX_EPOCH};

use orchestrator_store::{UsageBucket, UsageFigures};

use super::*;

const DAY_MS: i64 = 86_400_000;
const MAX_WINDOW_MS: i64 = 26 * 3_600_000;
const MIN_OFFSET_SECONDS: i64 = -12 * 3600;
const MAX_OFFSET_SECONDS: i64 = 14 * 3600;
const MAX_TIMEZONE_BYTES: usize = 64;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SummaryParams {
    timezone: String,
    utc_offset_seconds: i64,
    #[serde(default)]
    now_unix_ms: Option<i64>,
    #[serde(default)]
    window_start_unix_ms: Option<i64>,
    #[serde(default)]
    window_end_unix_ms: Option<i64>,
}

impl DaemonApi {
    pub(super) fn usage_summary(&self, params: Value) -> Result<Value, RpcError> {
        let request: SummaryParams = parse_params(params)?;
        let timezone = request.timezone.trim();
        if timezone.is_empty()
            || request.timezone.len() > MAX_TIMEZONE_BYTES
            || timezone.chars().any(char::is_control)
            || !(MIN_OFFSET_SECONDS..=MAX_OFFSET_SECONDS).contains(&request.utc_offset_seconds)
        {
            return Err(RpcError::InvalidParams);
        }
        let (start, end) = match (request.window_start_unix_ms, request.window_end_unix_ms) {
            (Some(start), Some(end)) => {
                if start < 0 || end <= start || end - start > MAX_WINDOW_MS {
                    return Err(RpcError::InvalidParams);
                }
                (start, end)
            }
            (None, None) => {
                let now = match request.now_unix_ms {
                    Some(now) if now >= 0 => now,
                    Some(_) => return Err(RpcError::InvalidParams),
                    None => system_now_ms(),
                };
                local_day_window(now, request.utc_offset_seconds)
            }
            _ => return Err(RpcError::InvalidParams),
        };
        let summary = self
            .store
            .usage_window_summary(start as u64, end as u64)
            .map_err(map_store_error)?;
        Ok(json!({
            "window": {
                "start_unix_ms": start,
                "end_unix_ms": end,
                "timezone": timezone,
                "utc_offset_seconds": request.utc_offset_seconds,
            },
            "totals": bucket_json(&summary.totals),
            "providers": summary
                .providers
                .iter()
                .map(|provider| {
                    let mut value = bucket_json(&provider.bucket);
                    value["provider"] = json!(provider.provider);
                    value
                })
                .collect::<Vec<_>>(),
        }))
    }
}

fn system_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

/// `[local midnight, next local midnight)` in UTC milliseconds for the local
/// day containing `now_ms` at a fixed offset.
fn local_day_window(now_ms: i64, offset_seconds: i64) -> (i64, i64) {
    let offset_ms = offset_seconds * 1000;
    let start = (now_ms + offset_ms).div_euclid(DAY_MS) * DAY_MS - offset_ms;
    (start, start + DAY_MS)
}

fn coverage_status(covered: u64, sessions: u64) -> &'static str {
    if covered == 0 {
        "unknown"
    } else if covered < sessions {
        "partial"
    } else {
        "complete"
    }
}

fn bucket_json(bucket: &UsageBucket) -> Value {
    json!({
        "sessions": bucket.sessions,
        "sessions_with_tokens": bucket.sessions_with_tokens,
        "sessions_with_cost": bucket.sessions_with_cost,
        "token_status": coverage_status(bucket.sessions_with_tokens, bucket.sessions),
        "cost_status": coverage_status(bucket.sessions_with_cost, bucket.sessions),
        "reported": figures_json(&bucket.reported),
        "estimated": figures_json(&bucket.estimated),
    })
}

fn figures_json(figures: &UsageFigures) -> Value {
    json!({
        "input_tokens": figures.input_tokens,
        "cache_read_tokens": figures.cache_read_tokens,
        "cache_write_tokens": figures.cache_write_tokens,
        "output_tokens": figures.output_tokens,
        "reasoning_tokens": figures.reasoning_tokens,
        "costs": figures
            .costs
            .iter()
            .map(|(currency, nanos)| json!({"currency": currency, "amount": format_nanos(*nanos)}))
            .collect::<Vec<_>>(),
    })
}

/// Nano-units as a decimal string with trailing zeros trimmed.
fn format_nanos(nanos: u128) -> String {
    let whole = nanos / 1_000_000_000;
    let fraction = nanos % 1_000_000_000;
    if fraction == 0 {
        return whole.to_string();
    }
    let fraction = format!("{fraction:09}");
    format!("{whole}.{}", fraction.trim_end_matches('0'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nanos_format_as_trimmed_decimals() {
        assert_eq!(format_nanos(0), "0");
        assert_eq!(format_nanos(8_638_800), "0.0086388");
        assert_eq!(format_nanos(2_000_000_000), "2");
        assert_eq!(format_nanos(1), "0.000000001");
    }

    #[test]
    fn local_midnight_boundaries_follow_the_offset() {
        // 2026-01-02T00:00:00Z
        let utc_midnight = 1_767_312_000_000;
        let la = -8 * 3600;
        // 2026-01-01T23:59:59.999 in UTC-8 is 07:59:59.999Z on Jan 2.
        let (start, end) = local_day_window(utc_midnight + 8 * 3_600_000 - 1, la);
        assert_eq!(start, utc_midnight - 16 * 3_600_000);
        assert_eq!(end, utc_midnight + 8 * 3_600_000);
        // One millisecond later is the next local day.
        let (next_start, _) = local_day_window(utc_midnight + 8 * 3_600_000, la);
        assert_eq!(next_start, end);
        let tokyo = 9 * 3600;
        let (start, end) = local_day_window(utc_midnight, tokyo);
        assert_eq!(start, utc_midnight - 9 * 3_600_000);
        assert_eq!(end - start, DAY_MS);
    }
}
