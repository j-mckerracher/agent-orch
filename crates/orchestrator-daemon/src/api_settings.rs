//! `settings.get` and `settings.set`: the machine load limits and the live state of
//! the load gate (DPES-1105 phase 2c).
//!
//! Contract (plan: `plan-2.md` 11.5):
//! - Capability `settings`. `settings.get` takes no parameters and is open to any
//!   session with the capability. `settings.set` is user-only: the transport refuses
//!   a session that did not prove the approver secret before this handler runs, so
//!   the handler has no check of its own.
//! - `settings.set` takes `{idempotency_key, load: {cpu_percent?, memory_percent?,
//!   disk_percent?}}`. Every present value is an integer from 50 to 100 (100 turns
//!   that meter off); an unknown field or an empty `load` is `InvalidParams`.
//! - Both answer `{load, gate: {state, hold, meters}, feed, supervision, wake_limits}`. A
//!   new limit applies from the next sample, with no restart. No path and no id appears in
//!   the result, except the home directory below, which is a path the user typed.
//! - Phase 3 (DPES-1105) adds two groups to the same methods. `supervision` is
//!   `{base_interval_secs, min_interval_secs, attempt_wall_minutes}` and `wake_limits` is
//!   `{hourly_cap, daily_cap, daily_token_budget}`; in `wake_limits` a JSON `null` means
//!   unlimited and an absent field means unchanged. A patch names at least one group and each
//!   present group names at least one field. `settings.set` never sets the monitoring mode:
//!   only `monitoring.set` does. The wake scheduler reads the settings on every pass, so a
//!   change applies at its next tick.
//! - Phase 4b (DPES-1105) adds `chat_pr_delivery`, a boolean in both answers and an optional
//!   field of `settings.set` (off by default). `settings.set` stays user-only. A `report_relay_auto`
//!   setting does not exist.
//! - Phase 6b (DPES-1105) adds `mcp: {enabled}`, always present in both answers (off by
//!   default) and an optional group of `settings.set`: the read-only task tools for the
//!   orchestrator chat. `enabled` is a JSON boolean; an `mcp` group with no field is
//!   `InvalidParams`. No new method: `settings.set` stays user-only, so a token-only session
//!   cannot turn it on.
//! - `home_directory` is a string or `null` in both answers (`null` while none is set) and an
//!   optional field of `settings.set`: the directory a task runs in when it is not clear which
//!   project it belongs to. A string is a path (`~` is the user's home) that must name an
//!   existing directory, otherwise `InvalidParams`; the answer holds the canonical path. A
//!   JSON `null` clears it and an absent field leaves it. Setting it registers the directory as
//!   an in-place project unless it is already a project (see `api_home_directory.rs`). The log
//!   line says the setting was set or cleared, never where.

use std::time::Instant;

use orchestrator_core::load_gate::{
    LoadLimits, LoadMeter, LoadStatus, Reading, Unknown, MIN_LIMIT, OFF,
};
use orchestrator_store::{
    LoadPatch, McpPatch, OrchestratorSettings, SettingsPatch, SupervisionPatch, WakeLimitsPatch,
};
use serde::Deserializer;

use super::load::LoadReading;
use super::*;

const LOG: &str = "settings";

/// Present-null versus absent: `Some(None)` is a JSON `null`, `None` is an absent field.
fn present_null<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SetParams {
    idempotency_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    load: Option<LoadParams>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    supervision: Option<SupervisionParams>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wake_limits: Option<WakeLimitsParams>,
    /// Lets chat propose a task that ends in a pull request (DPES-1105 phase 4b). Off by
    /// default. A loosening, so only this user-only method sets it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chat_pr_delivery: Option<bool>,
    /// The read-only MCP tools for the orchestrator chat (phase 6b). Off by default; a
    /// loosening, so only the user-only method sets it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mcp: Option<McpParams>,
    /// The directory a task runs in when it is not clear which project it belongs to (a path;
    /// `~` is the user's home). A JSON `null` clears it and an absent field leaves it.
    #[serde(
        default,
        deserialize_with = "present_null",
        skip_serializing_if = "Option::is_none"
    )]
    home_directory: Option<Option<String>>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct McpParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
}

impl McpParams {
    /// At least one field.
    fn validate(&self) -> Result<(), RpcError> {
        if self.enabled.is_none() {
            return Err(RpcError::InvalidParams);
        }
        Ok(())
    }

    fn patch(&self) -> McpPatch {
        McpPatch {
            enabled: self.enabled,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SupervisionParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    base_interval_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    min_interval_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attempt_wall_minutes: Option<u32>,
}

impl SupervisionParams {
    /// At least one field. Ranges are checked by the store on the merged row.
    fn validate(&self) -> Result<(), RpcError> {
        if self.base_interval_secs.is_none()
            && self.min_interval_secs.is_none()
            && self.attempt_wall_minutes.is_none()
        {
            return Err(RpcError::InvalidParams);
        }
        Ok(())
    }

    fn patch(&self) -> SupervisionPatch {
        SupervisionPatch {
            base_interval_secs: self.base_interval_secs,
            min_interval_secs: self.min_interval_secs,
            attempt_wall_minutes: self.attempt_wall_minutes,
        }
    }

    fn fields(&self) -> Vec<(&'static str, String)> {
        [
            ("base_interval_secs", self.base_interval_secs),
            ("min_interval_secs", self.min_interval_secs),
            ("attempt_wall_minutes", self.attempt_wall_minutes),
        ]
        .into_iter()
        .filter_map(|(name, value)| value.map(|value| (name, value.to_string())))
        .collect()
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WakeLimitsParams {
    #[serde(
        default,
        deserialize_with = "present_null",
        skip_serializing_if = "Option::is_none"
    )]
    hourly_cap: Option<Option<u32>>,
    #[serde(
        default,
        deserialize_with = "present_null",
        skip_serializing_if = "Option::is_none"
    )]
    daily_cap: Option<Option<u32>>,
    #[serde(
        default,
        deserialize_with = "present_null",
        skip_serializing_if = "Option::is_none"
    )]
    daily_token_budget: Option<Option<u64>>,
}

impl WakeLimitsParams {
    fn validate(&self) -> Result<(), RpcError> {
        if self.hourly_cap.is_none()
            && self.daily_cap.is_none()
            && self.daily_token_budget.is_none()
        {
            return Err(RpcError::InvalidParams);
        }
        Ok(())
    }

    fn patch(&self) -> WakeLimitsPatch {
        WakeLimitsPatch {
            hourly_cap: self.hourly_cap,
            daily_cap: self.daily_cap,
            daily_token_budget: self.daily_token_budget,
        }
    }

    fn fields(&self) -> Vec<(&'static str, String)> {
        let show = |value: Option<String>| value.unwrap_or_else(|| "unlimited".to_owned());
        let mut fields = Vec::new();
        if let Some(cap) = self.hourly_cap {
            fields.push(("hourly_cap", show(cap.map(|v| v.to_string()))));
        }
        if let Some(cap) = self.daily_cap {
            fields.push(("daily_cap", show(cap.map(|v| v.to_string()))));
        }
        if let Some(budget) = self.daily_token_budget {
            fields.push(("daily_token_budget", show(budget.map(|v| v.to_string()))));
        }
        fields
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LoadParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cpu_percent: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    memory_percent: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    disk_percent: Option<u8>,
}

impl LoadParams {
    fn named(&self) -> [(&'static str, Option<u8>); 3] {
        [
            ("cpu_percent", self.cpu_percent),
            ("memory_percent", self.memory_percent),
            ("disk_percent", self.disk_percent),
        ]
    }

    /// At least one limit, each from 50 to 100.
    fn validate(&self) -> Result<(), RpcError> {
        let values: Vec<u8> = self.named().into_iter().filter_map(|(_, v)| v).collect();
        if values.is_empty() || values.iter().any(|v| !(MIN_LIMIT..=OFF).contains(v)) {
            return Err(RpcError::InvalidParams);
        }
        Ok(())
    }

    fn patch(&self) -> LoadPatch {
        LoadPatch {
            cpu: self.cpu_percent,
            memory: self.memory_percent,
            disk: self.disk_percent,
        }
    }

    fn fields(&self) -> Vec<(&'static str, String)> {
        self.named()
            .into_iter()
            .filter_map(|(name, value)| value.map(|value| (name, value.to_string())))
            .collect()
    }
}

impl DaemonApi {
    pub(super) fn settings_get(&self, params: Value) -> Result<Value, RpcError> {
        require_empty_params(&params)?;
        self.settings_body()
    }

    pub(super) fn settings_set(&self, params: Value) -> Result<Value, RpcError> {
        let request: SetParams = parse_params(params)?;
        if request.load.is_none()
            && request.supervision.is_none()
            && request.wake_limits.is_none()
            && request.chat_pr_delivery.is_none()
            && request.mcp.is_none()
            && request.home_directory.is_none()
        {
            return Err(RpcError::InvalidParams);
        }
        if let Some(load) = &request.load {
            load.validate()?;
        }
        if let Some(supervision) = &request.supervision {
            supervision.validate()?;
        }
        if let Some(wake_limits) = &request.wake_limits {
            wake_limits.validate()?;
        }
        if let Some(mcp) = &request.mcp {
            mcp.validate()?;
        }
        // Resolved before the key is claimed, so a path that does not exist yet does not
        // poison the key for a retry.
        let home_directory = match &request.home_directory {
            None => None,
            Some(None) => Some(None),
            Some(Some(typed)) => {
                let resolved = home_directory::resolve(typed)?;
                let text = resolved.to_str().ok_or(RpcError::InvalidParams)?.to_owned();
                Some(Some(text))
            }
        };
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "settings.set",
            &key,
            &request,
            || {
                let patch = SettingsPatch {
                    load: request
                        .load
                        .as_ref()
                        .map(LoadParams::patch)
                        .unwrap_or_default(),
                    supervision: request
                        .supervision
                        .as_ref()
                        .map(SupervisionParams::patch)
                        .unwrap_or_default(),
                    wake_limits: request
                        .wake_limits
                        .as_ref()
                        .map(WakeLimitsParams::patch)
                        .unwrap_or_default(),
                    // `settings.set` never sets the mode.
                    mode: None,
                    chat_pr_delivery: request.chat_pr_delivery,
                    mcp: request
                        .mcp
                        .as_ref()
                        .map(McpParams::patch)
                        .unwrap_or_default(),
                    home_directory: home_directory.clone(),
                };
                let mut fields = Vec::new();
                fields.extend(request.load.iter().flat_map(LoadParams::fields));
                fields.extend(
                    request
                        .supervision
                        .iter()
                        .flat_map(SupervisionParams::fields),
                );
                fields.extend(
                    request
                        .wake_limits
                        .iter()
                        .flat_map(WakeLimitsParams::fields),
                );
                if let Some(value) = request.chat_pr_delivery {
                    fields.push(("chat_pr_delivery", value.to_string()));
                }
                if let Some(value) = request.mcp.as_ref().and_then(|mcp| mcp.enabled) {
                    fields.push(("mcp_enabled", value.to_string()));
                }
                // The log names the change, never the path.
                if let Some(directory) = &home_directory {
                    if let Some(root) = directory {
                        self.ensure_home_project(root)?;
                    }
                    let change = if directory.is_some() {
                        "set"
                    } else {
                        "cleared"
                    };
                    fields.push(("home_directory", change.to_owned()));
                }
                let saved = self.store.update_orchestrator_settings(&patch);
                self.finish(saved, request.load.is_some(), &fields)
            },
        )
    }

    /// Applies what the store saved. A failed write changes no limit.
    #[cfg(test)]
    pub(super) fn adopt(
        &self,
        saved: Result<OrchestratorSettings, StoreError>,
        load: &LoadParams,
    ) -> Result<Value, RpcError> {
        self.finish(saved, true, &load.fields())
    }

    /// Hands the saved load limits to the gate only when the request carried a `load` group,
    /// so a supervision-only change leaves the gate untouched.
    fn finish(
        &self,
        saved: Result<OrchestratorSettings, StoreError>,
        load_changed: bool,
        fields: &[(&'static str, String)],
    ) -> Result<Value, RpcError> {
        let saved = saved.map_err(settings_store_error)?;
        if load_changed {
            self.config.load.set_limits(saved.load);
        }
        self.log_change(fields);
        self.settings_body()
    }

    /// One line: the names of the fields that changed and their new numbers.
    fn log_change(&self, fields: &[(&'static str, String)]) {
        let fields: Vec<(&str, &str)> = fields
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        self.config
            .diagnostics
            .info(LOG, "settings_changed", &fields);
    }

    /// The limits, the live gate and the Phase 3 groups, as `settings.get` and `settings.set`
    /// answer.
    fn settings_body(&self) -> Result<Value, RpcError> {
        let settings = self
            .store
            .orchestrator_settings()
            .map_err(map_store_error)?;
        let LoadReading { limits, status } = self.config.load.read(Instant::now());
        Ok(json!({
            "load": limits_wire(limits),
            "gate": gate_wire(&status),
            "feed": if self.config.load_feed { "sampler" } else { "disabled_by_environment" },
            "supervision": {
                "base_interval_secs": settings.supervision.base_interval_secs,
                "min_interval_secs": settings.supervision.min_interval_secs,
                "attempt_wall_minutes": settings.supervision.attempt_wall_minutes,
            },
            "wake_limits": {
                "hourly_cap": settings.wake_limits.hourly,
                "daily_cap": settings.wake_limits.daily,
                "daily_token_budget": settings.wake_limits.daily_tokens,
            },
            "chat_pr_delivery": settings.chat_pr_delivery,
            "mcp": { "enabled": settings.mcp.enabled },
            "home_directory": settings.home_directory,
        }))
    }
}

/// A value the store refused as out of range (including a minimum above the merged base) is
/// the caller's mistake. Any other failure is ours.
fn settings_store_error(error: StoreError) -> RpcError {
    match error {
        StoreError::Io(io) if io.kind() == std::io::ErrorKind::InvalidInput => {
            RpcError::InvalidParams
        }
        other => map_store_error(other),
    }
}

fn limits_wire(limits: LoadLimits) -> Value {
    json!({
        "cpu_percent": limits.cpu,
        "memory_percent": limits.memory,
        "disk_percent": limits.disk,
    })
}

fn gate_wire(status: &LoadStatus) -> Value {
    let hold = status.hold.map_or(Value::Null, |hold| {
        json!({
            "meter": hold.meter.as_str(),
            "mean_percent": hold.mean_percent,
            "limit_percent": hold.limit_percent,
            "reopen_percent": hold.reopen_percent(),
        })
    });
    let meters: Vec<Value> = status
        .meters
        .iter()
        .map(|(meter, reading)| meter_wire(*meter, reading))
        .collect();
    json!({
        "state": if status.hold.is_some() { "closed" } else { "open" },
        "hold": hold,
        "meters": meters,
    })
}

fn meter_wire(meter: LoadMeter, reading: &Reading) -> Value {
    let name = meter.as_str();
    match reading {
        Reading::Off => json!({"meter": name, "state": "off"}),
        Reading::Unknown(Unknown::WarmingUp) => {
            json!({"meter": name, "state": "unknown", "reason": "warming_up"})
        }
        Reading::Unknown(Unknown::Unavailable) => {
            json!({"meter": name, "state": "unknown", "reason": "unavailable"})
        }
        Reading::Mean(_) => {
            json!({"meter": name, "state": "mean", "mean_percent": reading.mean_percent()})
        }
    }
}
