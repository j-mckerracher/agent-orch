//! `host.metrics`: cached host CPU, memory, disk and GPU meters.
//!
//! Contract (details in `docs/host-metrics.md`):
//! - Method: `host.metrics`; capability `host`; read-only; authenticated like
//!   every other method.
//! - Params: none (`null` or `{}`); anything else is `InvalidParams`.
//! - Result: the `orchestrator_host_metrics::Snapshot` object
//!   `{schema_version, sample_interval_ms, meters: [{meter, state, kind, unit,
//!   scope, value, total, observed_at, sample_interval_ms, reason}]}` with one
//!   or more meters for each of `cpu`, `memory`, `disk` and `gpu`. `state` is
//!   `ok`, `unavailable` or `unsupported`; `value` is null unless `ok`.
//! - Never blocks: the handler copies a cache that a background thread fills.
//!   The sampler starts with the executor, or on first use when there is none.

use std::path::PathBuf;
use std::sync::OnceLock;

use orchestrator_host_metrics::{HostMetrics, SysinfoProbe, DEFAULT_INTERVAL};

use super::*;

pub(super) struct HostService {
    data_dir: PathBuf,
    metrics: OnceLock<HostMetrics>,
}

impl HostService {
    pub(super) fn new(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_owned(),
            metrics: OnceLock::new(),
        }
    }

    pub(super) fn start(&self) {
        self.metrics();
    }

    fn metrics(&self) -> &HostMetrics {
        self.metrics.get_or_init(|| {
            HostMetrics::start(SysinfoProbe::new(self.data_dir.clone()), DEFAULT_INTERVAL)
        })
    }
}

impl DaemonApi {
    pub(super) fn host_metrics(&self, params: Value) -> Result<Value, RpcError> {
        require_empty_params(&params)?;
        serde_json::to_value(self.host.metrics().snapshot()).map_err(|_| RpcError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api() -> (tempfile::TempDir, DaemonApi) {
        let directory = tempfile::tempdir().unwrap();
        let api = DaemonApi::new(directory.path()).unwrap();
        (directory, api)
    }

    #[test]
    fn the_host_capability_is_advertised() {
        let (_directory, api) = api();
        assert!(api.capabilities().contains(&Capability::Host));
    }

    #[test]
    fn dispatch_returns_a_snapshot_with_every_resource_before_any_sample() {
        let (_directory, api) = api();
        let snapshot = api.dispatch("host.metrics", Value::Null).unwrap();
        assert_eq!(snapshot["schema_version"], 1);
        let meters = snapshot["meters"].as_array().unwrap();
        for resource in ["cpu", "memory", "disk", "gpu"] {
            let meter = meters
                .iter()
                .find(|meter| meter["meter"] == resource)
                .unwrap_or_else(|| panic!("{resource} missing"));
            // Either not sampled yet or sampled: never a made-up number.
            if meter["state"] != "ok" {
                assert!(meter["value"].is_null(), "{meter}");
                assert!(meter["reason"].is_string(), "{meter}");
            }
        }
        assert!(api.dispatch("host.metrics", json!({})).is_ok());
    }

    #[test]
    fn parameters_are_rejected() {
        let (_directory, api) = api();
        for params in [json!({"detail": true}), json!([]), json!("cpu"), json!(1)] {
            assert_eq!(
                api.dispatch("host.metrics", params),
                Err(RpcError::InvalidParams)
            );
        }
    }
}
