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
//! - Both answer `{load, gate: {state, hold, meters}, feed}`. A new limit applies
//!   from the next sample, with no restart. No path and no id appears in the result.

use std::time::Instant;

use orchestrator_core::load_gate::{
    LoadLimits, LoadMeter, LoadStatus, Reading, Unknown, MIN_LIMIT, OFF,
};
use orchestrator_store::{LoadPatch, OrchestratorSettings, SettingsPatch};

use super::load::LoadReading;
use super::*;

const LOG: &str = "settings";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SetParams {
    idempotency_key: String,
    load: LoadParams,
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

    fn patch(&self) -> SettingsPatch {
        SettingsPatch {
            load: LoadPatch {
                cpu: self.cpu_percent,
                memory: self.memory_percent,
                disk: self.disk_percent,
            },
        }
    }
}

impl DaemonApi {
    pub(super) fn settings_get(&self, params: Value) -> Result<Value, RpcError> {
        require_empty_params(&params)?;
        Ok(self.settings_body())
    }

    pub(super) fn settings_set(&self, params: Value) -> Result<Value, RpcError> {
        let request: SetParams = parse_params(params)?;
        request.load.validate()?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "settings.set",
            &key,
            &request,
            || {
                let saved = self
                    .store
                    .update_orchestrator_settings(&request.load.patch());
                self.adopt(saved, &request.load)
            },
        )
    }

    /// Applies what the store saved. A failed write changes no limit.
    pub(super) fn adopt(
        &self,
        saved: Result<OrchestratorSettings, StoreError>,
        load: &LoadParams,
    ) -> Result<Value, RpcError> {
        let saved = saved.map_err(map_store_error)?;
        self.config.load.set_limits(saved.load);
        self.log_change(load);
        Ok(self.settings_body())
    }

    /// One line: the names of the fields that changed and their new numbers.
    fn log_change(&self, load: &LoadParams) {
        let fields: Vec<(&str, String)> = load
            .named()
            .into_iter()
            .filter_map(|(name, value)| value.map(|value| (name, value.to_string())))
            .collect();
        let fields: Vec<(&str, &str)> = fields
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        self.config
            .diagnostics
            .info(LOG, "settings_changed", &fields);
    }

    /// The limits and the live gate, as `settings.get` and `settings.set` answer.
    fn settings_body(&self) -> Value {
        let LoadReading { limits, status } = self.config.load.read(Instant::now());
        json!({
            "load": limits_wire(limits),
            "gate": gate_wire(&status),
            "feed": if self.config.load_feed { "sampler" } else { "disabled_by_environment" },
        })
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
