//! `usage.localTools*`: AI usage from the coding tools on this machine
//! (DPES-1105), estimated by the pinned codeburn reader. See
//! `crates/orchestrator-local-usage` for the collector and its rules.
//!
//! * `usage.localTools` reads the cached snapshot; it never runs the reader.
//! * `usage.localTools.setEnabled` is the opt-in (default off, persisted in the
//!   data directory). Turning it off stops every read.
//! * `usage.localTools.refresh` asks the background collector to read now,
//!   rate limited. It returns at once.
//!
//! Params (all three): `timezone` (IANA name or label, 1..=64 bytes),
//! `utc_offset_seconds` (-43200..=50400), and the local day
//! `window_start_unix_ms` / `window_end_unix_ms` (span at most 26 hours).
//! `setEnabled` also takes `enabled` (boolean). Capability: `agents`.

use orchestrator_local_usage::{CodeburnSource, LocalUsageService, Timing, Window};

use super::*;

const MAX_WINDOW_MS: i64 = 26 * 3_600_000;
const MIN_OFFSET_SECONDS: i64 = -12 * 3600;
const MAX_OFFSET_SECONDS: i64 = 14 * 3600;
const MAX_TIMEZONE_BYTES: usize = 64;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowParams {
    timezone: String,
    utc_offset_seconds: i64,
    window_start_unix_ms: i64,
    window_end_unix_ms: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetEnabledParams {
    timezone: String,
    utc_offset_seconds: i64,
    window_start_unix_ms: i64,
    window_end_unix_ms: i64,
    enabled: bool,
}

impl WindowParams {
    fn window(self) -> Result<Window, RpcError> {
        let timezone = self.timezone.trim();
        if timezone.is_empty()
            || self.timezone.len() > MAX_TIMEZONE_BYTES
            || timezone.chars().any(char::is_control)
            || !(MIN_OFFSET_SECONDS..=MAX_OFFSET_SECONDS).contains(&self.utc_offset_seconds)
            || self.window_start_unix_ms < 0
            || self.window_end_unix_ms <= self.window_start_unix_ms
            || self.window_end_unix_ms - self.window_start_unix_ms > MAX_WINDOW_MS
        {
            return Err(RpcError::InvalidParams);
        }
        Ok(Window {
            start_unix_ms: self.window_start_unix_ms,
            end_unix_ms: self.window_end_unix_ms,
            timezone: timezone.to_owned(),
            utc_offset_seconds: self.utc_offset_seconds,
        })
    }
}

impl SetEnabledParams {
    fn split(self) -> Result<(bool, Window), RpcError> {
        let enabled = self.enabled;
        let window = WindowParams {
            timezone: self.timezone,
            utc_offset_seconds: self.utc_offset_seconds,
            window_start_unix_ms: self.window_start_unix_ms,
            window_end_unix_ms: self.window_end_unix_ms,
        }
        .window()?;
        Ok((enabled, window))
    }
}

fn snapshot_json(snapshot: orchestrator_local_usage::Snapshot) -> Result<Value, RpcError> {
    serde_json::to_value(snapshot).map_err(|_| RpcError::Internal)
}

impl DaemonApi {
    pub(super) fn local_usage_read(&self, params: Value) -> Result<Value, RpcError> {
        let window = parse_params::<WindowParams>(params)?.window()?;
        snapshot_json(self.local_usage.read(window))
    }

    pub(super) fn local_usage_set_enabled(&self, params: Value) -> Result<Value, RpcError> {
        let (enabled, window) = parse_params::<SetEnabledParams>(params)?.split()?;
        let snapshot = self
            .local_usage
            .set_enabled(enabled, window)
            .map_err(|_| RpcError::Internal)?;
        snapshot_json(snapshot)
    }

    pub(super) fn local_usage_refresh(&self, params: Value) -> Result<Value, RpcError> {
        let window = parse_params::<WindowParams>(params)?.window()?;
        snapshot_json(self.local_usage.refresh(window))
    }
}

/// The collector for a daemon data directory. The reader is looked up on the
/// first read, not at start, so a daemon without the feature pays nothing.
pub(super) fn start_local_usage(data_dir: &Path) -> LocalUsageService {
    LocalUsageService::new(
        data_dir,
        Box::new(CodeburnSource::default()),
        Timing::PRODUCTION,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window_params(start: i64, end: i64) -> Value {
        json!({
            "timezone": "America/Chicago",
            "utc_offset_seconds": -21600,
            "window_start_unix_ms": start,
            "window_end_unix_ms": end,
        })
    }

    #[test]
    fn the_read_method_reports_off_by_default_without_reading() {
        let directory = tempfile::tempdir().unwrap();
        let api = DaemonApi::new(directory.path()).unwrap();
        let params = window_params(1_763_182_800_000, 1_763_269_200_000);
        let snapshot = api.dispatch("usage.localTools", params).unwrap();
        assert_eq!(snapshot["schema_version"], 1);
        assert_eq!(snapshot["enabled"], false, "opt-in is off by default");
        assert_eq!(snapshot["reader"]["state"], "off");
        assert!(snapshot["reader"]["development_build"].is_boolean());
        assert_eq!(snapshot["providers"].as_array().unwrap().len(), 3);
        assert_eq!(snapshot["providers"][0]["state"], "not_run");
        assert!(snapshot["providers"][0]["estimated_costs"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn window_params_are_validated_before_anything_runs() {
        let directory = tempfile::tempdir().unwrap();
        let api = DaemonApi::new(directory.path()).unwrap();
        let too_long = window_params(0, 27 * 3_600_000);
        assert!(matches!(
            api.dispatch("usage.localTools", too_long),
            Err(RpcError::InvalidParams)
        ));
        let inverted = window_params(10, 10);
        assert!(matches!(
            api.dispatch("usage.localTools", inverted),
            Err(RpcError::InvalidParams)
        ));
        let mut bad_offset = window_params(0, 1000);
        bad_offset["utc_offset_seconds"] = json!(99_999);
        assert!(matches!(
            api.dispatch("usage.localTools", bad_offset),
            Err(RpcError::InvalidParams)
        ));
    }

    #[test]
    fn the_opt_in_is_persisted_and_turning_it_off_clears_the_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let api = DaemonApi::new(directory.path()).unwrap();
        let window = window_params(1_763_182_800_000, 1_763_269_200_000);
        let mut on = window.clone();
        on["enabled"] = json!(true);
        let snapshot = api.dispatch("usage.localTools.setEnabled", on).unwrap();
        assert_eq!(snapshot["enabled"], true);
        assert_eq!(snapshot["reader"]["state"], "not_started");
        assert!(directory.path().join("usage-local-tools.json").is_file());

        let mut off = window.clone();
        off["enabled"] = json!(false);
        let snapshot = api.dispatch("usage.localTools.setEnabled", off).unwrap();
        assert_eq!(snapshot["enabled"], false);
        assert_eq!(snapshot["reader"]["state"], "off");
        assert!(snapshot["observed_at_unix_ms"].is_null());
    }
}
