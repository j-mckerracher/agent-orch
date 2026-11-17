//! Host-resource admission for agent starts.
//!
//! The executor asks [`AdmissionGate::check`] before it launches each task the
//! core scheduler approved. The decision itself is the pure
//! `orchestrator_host_metrics::evaluate`; this module only supplies its inputs
//! (thresholds, a metrics snapshot, the running count) and remembers the last
//! outcome for `host.metrics`.
//!
//! Defaults preserve current behavior: every threshold is off unless set, and
//! with none set no metrics are read. A deferred task stays `Queued` and is
//! reconsidered on the next pass; running agents are never touched.
//!
//! Configuration (daemon environment, percentages 0 to 100, counts >= 0; an
//! unset or unparsable value leaves that limit off):
//! `AGENT_ORCH_ADMISSION_MAX_CPU_PERCENT`,
//! `AGENT_ORCH_ADMISSION_MAX_MEMORY_PERCENT`,
//! `AGENT_ORCH_ADMISSION_MAX_DISK_PERCENT`,
//! `AGENT_ORCH_ADMISSION_MAX_RUNNING_AGENTS`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use orchestrator_host_metrics::{
    evaluate, Decision, HostMetrics, Snapshot, SysinfoProbe, Thresholds, DEFAULT_INTERVAL,
};
use serde_json::{json, Value};

/// Supplies the host snapshot used for a decision.
pub(crate) type SnapshotSource = Arc<dyn Fn() -> Snapshot + Send + Sync>;

#[derive(Clone, Default)]
pub(crate) struct AdmissionConfig {
    pub(crate) thresholds: Thresholds,
    /// Replaces the real host sampler (tests).
    pub(crate) source: Option<SnapshotSource>,
}

impl AdmissionConfig {
    pub(crate) fn from_env() -> Self {
        let percent = |name: &str| {
            std::env::var(name)
                .ok()
                .and_then(|text| text.trim().parse::<f64>().ok())
                .filter(|value| value.is_finite() && (0.0..=100.0).contains(value))
        };
        Self {
            thresholds: Thresholds {
                max_cpu_percent: percent("AGENT_ORCH_ADMISSION_MAX_CPU_PERCENT"),
                max_memory_percent: percent("AGENT_ORCH_ADMISSION_MAX_MEMORY_PERCENT"),
                max_disk_percent: percent("AGENT_ORCH_ADMISSION_MAX_DISK_PERCENT"),
                max_running_agents: std::env::var("AGENT_ORCH_ADMISSION_MAX_RUNNING_AGENTS")
                    .ok()
                    .and_then(|text| text.trim().parse().ok()),
            },
            source: None,
        }
    }
}

#[derive(Default)]
struct Status {
    last: Option<Decision>,
    deferred_task_ids: Vec<String>,
}

pub(crate) struct AdmissionGate {
    config: AdmissionConfig,
    data_dir: PathBuf,
    metrics: OnceLock<HostMetrics>,
    status: Mutex<Status>,
}

impl AdmissionGate {
    pub(crate) fn new(config: AdmissionConfig, data_dir: PathBuf) -> Self {
        Self {
            config,
            data_dir,
            metrics: OnceLock::new(),
            status: Mutex::default(),
        }
    }

    /// Decides whether one more agent may start while `running_agents` run.
    /// Never blocks: the sampler answers from its cache.
    pub(crate) fn check(&self, running_agents: usize) -> Decision {
        let thresholds = &self.config.thresholds;
        let snapshot = thresholds
            .needs_metrics()
            .then(|| match &self.config.source {
                Some(source) => source(),
                None => self
                    .metrics
                    .get_or_init(|| {
                        HostMetrics::start(
                            SysinfoProbe::new(self.data_dir.clone()),
                            DEFAULT_INTERVAL,
                        )
                    })
                    .snapshot(),
            });
        evaluate(thresholds, snapshot.as_ref(), running_agents)
    }

    /// Records the outcome of a pass; returns true when the reason code
    /// changed (callers log only changes). A pass with no candidates keeps the last
    /// decision and clears the deferred list.
    pub(crate) fn record(&self, decision: Option<&Decision>, deferred: Vec<String>) -> bool {
        let mut status = self.status.lock().unwrap_or_else(|e| e.into_inner());
        let previous = status.last.as_ref().map(|d| d.reason_code);
        status.deferred_task_ids = deferred;
        match decision {
            Some(decision) => {
                status.last = Some(decision.clone());
                previous != Some(decision.reason_code)
            }
            None => false,
        }
    }

    /// The `admission` object of `host.metrics`: `enabled`, the thresholds,
    /// the last decision (`null` before any start was considered) and the
    /// task ids currently deferred.
    pub(crate) fn to_json(&self) -> Value {
        let status = self.status.lock().unwrap_or_else(|e| e.into_inner());
        json!({
            "enabled": self.config.thresholds.any(),
            "thresholds": self.config.thresholds,
            "decision": status.last,
            "deferred_task_ids": status.deferred_task_ids,
        })
    }
}
