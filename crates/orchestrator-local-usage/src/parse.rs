//! Reads the usage counters from `codeburn report --format json -p today`.
//! Only the fields below are read. Everything else in the reader's JSON
//! (project names, session ids, prompts-derived activity, model names) is
//! ignored and never stored. Pinned to codeburn 0.9.28; see `codeburn-pin.json`.

use serde::Deserialize;

use crate::model::ReaderState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    currency: String,
    period: String,
    overview: Overview,
    #[serde(default)]
    unpriced_models: Vec<Unpriced>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Overview {
    cost: f64,
    calls: u64,
    sessions: u64,
    tokens: Tokens,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Tokens {
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
}

/// Only the counts. The model name is deliberately not read.
#[derive(Debug, Deserialize)]
struct Unpriced {
    tokens: u64,
}

/// One provider's figures for the window, as the parser keeps them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Reading {
    pub sessions: u64,
    pub calls: u64,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// Estimated cost in nano-USD. Only priced calls contribute.
    pub cost_nanos: u128,
    pub unpriced_models: u64,
    pub unpriced_tokens: u64,
}

/// Parses one provider's report. `expected_day` is the local `YYYY-MM-DD` of
/// the requested window; a report for another day is `WrongDay`.
pub(crate) fn parse_report(bytes: &[u8], expected_day: &str) -> Result<Reading, ReaderState> {
    let report: Report = serde_json::from_slice(bytes).map_err(|_| ReaderState::BadOutput)?;
    if report.currency != "USD" {
        return Err(ReaderState::BadOutput);
    }
    let day = period_day(&report.period).ok_or(ReaderState::BadOutput)?;
    if day != expected_day {
        return Err(ReaderState::WrongDay);
    }
    let cost = report.overview.cost;
    if !cost.is_finite() || cost < 0.0 {
        return Err(ReaderState::BadOutput);
    }
    let overview = report.overview;
    Ok(Reading {
        sessions: overview.sessions,
        calls: overview.calls,
        input: overview.tokens.input,
        output: overview.tokens.output,
        cache_read: overview.tokens.cache_read,
        cache_write: overview.tokens.cache_write,
        cost_nanos: nanos(cost),
        unpriced_models: report.unpriced_models.len() as u64,
        unpriced_tokens: report
            .unpriced_models
            .iter()
            .map(|model| model.tokens)
            .sum(),
    })
}

/// `"Today (2026-11-15)"` to `"2026-11-15"`.
fn period_day(period: &str) -> Option<&str> {
    let open = period.rfind('(')?;
    let close = period.rfind(')')?;
    let day = period.get(open + 1..close)?;
    let bytes = day.as_bytes();
    let shape_ok = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && day
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit());
    shape_ok.then_some(day)
}

/// Dollars to nano-USD, rounded to the nearest nano-dollar.
pub(crate) fn nanos(dollars: f64) -> u128 {
    (dollars * 1e9).round() as u128
}

/// Nano-USD as a decimal string with trailing zeros trimmed.
pub(crate) fn format_nanos(nanos: u128) -> String {
    let whole = nanos / 1_000_000_000;
    let fraction = nanos % 1_000_000_000;
    if fraction == 0 {
        return whole.to_string();
    }
    format!("{whole}.{}", format!("{fraction:09}").trim_end_matches('0'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORDED: &[u8] = include_bytes!("../tests/fixtures/report-claude-synthetic.json");

    #[test]
    fn a_recorded_reader_report_parses_to_its_counters() {
        // Synthetic: one priced Sonnet call and one unpriced call, dated today.
        let reading = parse_report(RECORDED, "2026-11-15").unwrap();
        assert_eq!(reading.calls, 2);
        assert_eq!(reading.sessions, 2);
        assert_eq!(reading.input, 1900);
        assert_eq!(reading.output, 370);
        assert_eq!(reading.cache_read, 5000);
        assert_eq!(reading.cache_write, 800);
        assert_eq!(format_nanos(reading.cost_nanos), "0.0126");
        assert_eq!(reading.unpriced_models, 1);
        assert_eq!(reading.unpriced_tokens, 770);
    }

    #[test]
    fn a_report_for_another_day_is_wrong_day_not_a_figure() {
        assert_eq!(
            parse_report(RECORDED, "2026-11-14"),
            Err(ReaderState::WrongDay)
        );
    }

    #[test]
    fn malformed_reports_are_bad_output() {
        assert_eq!(
            parse_report(b"not json", "2026-11-15"),
            Err(ReaderState::BadOutput)
        );
        let eur = RECORDED.to_vec();
        let eur = String::from_utf8(eur)
            .unwrap()
            .replace("\"USD\"", "\"EUR\"");
        assert_eq!(
            parse_report(eur.as_bytes(), "2026-11-15"),
            Err(ReaderState::BadOutput)
        );
        let missing = String::from_utf8(RECORDED.to_vec())
            .unwrap()
            .replace("\"cacheWrite\"", "\"other\"");
        assert_eq!(
            parse_report(missing.as_bytes(), "2026-11-15"),
            Err(ReaderState::BadOutput)
        );
        let negative = String::from_utf8(RECORDED.to_vec())
            .unwrap()
            .replace("\"calls\": 2", "\"calls\": -2");
        assert_eq!(
            parse_report(negative.as_bytes(), "2026-11-15"),
            Err(ReaderState::BadOutput)
        );
    }

    #[test]
    fn the_period_label_must_carry_a_date() {
        assert_eq!(period_day("Today (2026-11-15)"), Some("2026-11-15"));
        assert_eq!(period_day("Today"), None);
        assert_eq!(period_day("Today (yesterday)"), None);
    }

    #[test]
    fn nano_amounts_are_exact_decimals() {
        assert_eq!(format_nanos(0), "0");
        assert_eq!(format_nanos(12_600_000), "0.0126");
        assert_eq!(format_nanos(2_000_000_000), "2");
        assert_eq!(nanos(0.012599999999999998), 12_600_000);
    }
}
