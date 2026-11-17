//! Pure admission decision: may another agent start on this host right now?
//!
//! [`evaluate`] compares a [`Snapshot`] and the number of running agents with
//! [`Thresholds`]. It does no I/O, so every rule is unit-testable.
//!
//! Rules:
//! - A threshold left `None` is off; with every threshold off the answer is
//!   always [`ReasonCode::Admitted`] (today's behavior).
//! - CPU, memory and disk thresholds are percentages (0 to 100). A start is
//!   deferred only when the reading is strictly above the threshold; a reading
//!   equal to it is admitted.
//! - The running-agents threshold is a count of agents that may run at once. A
//!   start is deferred when the count has already reached it, so `0` defers
//!   every start.
//! - A meter that is not `ok`, has no finite value, or (for memory and disk)
//!   has no positive total is unknown. Unknown never defers: the start is
//!   admitted with [`ReasonCode::AdmittedMetricUnknown`] and the meters are
//!   listed in [`Decision::unknown`].

use crate::model::{Meter, MeterId, MeterState, Snapshot};
use serde::{Deserialize, Serialize};

/// Limits above which a start is deferred. `None` turns a limit off.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Thresholds {
    pub max_cpu_percent: Option<f64>,
    pub max_memory_percent: Option<f64>,
    pub max_disk_percent: Option<f64>,
    pub max_running_agents: Option<u32>,
}

impl Thresholds {
    /// True when at least one limit is set.
    pub fn any(&self) -> bool {
        self.needs_metrics() || self.max_running_agents.is_some()
    }

    /// True when a host meter must be read to evaluate these limits.
    pub fn needs_metrics(&self) -> bool {
        self.max_cpu_percent.is_some()
            || self.max_memory_percent.is_some()
            || self.max_disk_percent.is_some()
    }
}

/// Stable machine-readable outcome. The `snake_case` names are API.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    Admitted,
    /// Admitted because a checked meter could not be read (fail open).
    AdmittedMetricUnknown,
    RunningAgentsAtLimit,
    CpuOverThreshold,
    MemoryOverThreshold,
    DiskOverThreshold,
}

impl ReasonCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admitted => "admitted",
            Self::AdmittedMetricUnknown => "admitted_metric_unknown",
            Self::RunningAgentsAtLimit => "running_agents_at_limit",
            Self::CpuOverThreshold => "cpu_over_threshold",
            Self::MemoryOverThreshold => "memory_over_threshold",
            Self::DiskOverThreshold => "disk_over_threshold",
        }
    }
}

/// One limit that was exceeded.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Breach {
    pub reason_code: ReasonCode,
    pub observed: f64,
    pub threshold: f64,
}

/// Result of one evaluation.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Decision {
    pub admitted: bool,
    /// The first breach (running agents, CPU, memory, disk) when deferred;
    /// otherwise `admitted` or `admitted_metric_unknown`.
    pub reason_code: ReasonCode,
    /// Every exceeded limit; empty when admitted.
    pub breaches: Vec<Breach>,
    /// Checked meters that could not be read.
    pub unknown: Vec<MeterId>,
}

/// Decides whether one more agent may start.
pub fn evaluate(
    thresholds: &Thresholds,
    snapshot: Option<&Snapshot>,
    running_agents: usize,
) -> Decision {
    let mut breaches = Vec::new();
    let mut unknown = Vec::new();

    if let Some(max) = thresholds.max_running_agents {
        if running_agents >= max as usize {
            breaches.push(Breach {
                reason_code: ReasonCode::RunningAgentsAtLimit,
                observed: running_agents as f64,
                threshold: f64::from(max),
            });
        }
    }
    let meters = [
        (
            MeterId::Cpu,
            thresholds.max_cpu_percent,
            ReasonCode::CpuOverThreshold,
        ),
        (
            MeterId::Memory,
            thresholds.max_memory_percent,
            ReasonCode::MemoryOverThreshold,
        ),
        (
            MeterId::Disk,
            thresholds.max_disk_percent,
            ReasonCode::DiskOverThreshold,
        ),
    ];
    for (id, limit, reason_code) in meters {
        let Some(limit) = limit else { continue };
        match snapshot.and_then(|snapshot| percent_used(snapshot, id)) {
            Some(observed) if observed > limit => breaches.push(Breach {
                reason_code,
                observed,
                threshold: limit,
            }),
            Some(_) => {}
            None => unknown.push(id),
        }
    }

    let reason_code = match breaches.first() {
        Some(breach) => breach.reason_code,
        None if unknown.is_empty() => ReasonCode::Admitted,
        None => ReasonCode::AdmittedMetricUnknown,
    };
    Decision {
        admitted: breaches.is_empty(),
        reason_code,
        breaches,
        unknown,
    }
}

/// Used share of a meter in percent, or `None` when it cannot be trusted.
fn percent_used(snapshot: &Snapshot, id: MeterId) -> Option<f64> {
    let meter: &Meter = snapshot.meters.iter().find(|meter| meter.meter == id)?;
    if meter.state != MeterState::Ok {
        return None;
    }
    let value = meter.value.filter(|value| value.is_finite())?;
    match id {
        MeterId::Cpu | MeterId::Gpu => Some(value),
        MeterId::Memory | MeterId::Disk => {
            let total = meter
                .total
                .filter(|total| total.is_finite() && *total > 0.0)?;
            Some(value / total * 100.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{shape, SCHEMA_VERSION};

    fn ok(id: MeterId, value: f64, total: Option<f64>) -> Meter {
        let (kind, unit) = shape(id);
        Meter::ok(id, kind, unit, "test", value, total)
    }

    fn snapshot(cpu: f64, memory_used: f64, disk_used: f64) -> Snapshot {
        Snapshot {
            schema_version: SCHEMA_VERSION,
            sample_interval_ms: 1000,
            meters: vec![
                ok(MeterId::Cpu, cpu, None),
                ok(MeterId::Memory, memory_used, Some(100.0)),
                ok(MeterId::Disk, disk_used, Some(200.0)),
            ],
        }
    }

    fn with(max_cpu: Option<f64>, max_mem: Option<f64>, max_disk: Option<f64>) -> Thresholds {
        Thresholds {
            max_cpu_percent: max_cpu,
            max_memory_percent: max_mem,
            max_disk_percent: max_disk,
            max_running_agents: None,
        }
    }

    #[test]
    fn unset_thresholds_always_admit_even_without_metrics() {
        let decision = evaluate(&Thresholds::default(), None, 99);
        assert!(decision.admitted);
        assert_eq!(decision.reason_code, ReasonCode::Admitted);
        assert!(decision.unknown.is_empty() && decision.breaches.is_empty());
        assert!(!Thresholds::default().any());
    }

    #[test]
    fn cpu_under_at_and_over() {
        let limit = with(Some(80.0), None, None);
        assert!(evaluate(&limit, Some(&snapshot(79.9, 0.0, 0.0)), 0).admitted);
        assert!(evaluate(&limit, Some(&snapshot(80.0, 0.0, 0.0)), 0).admitted);
        let over = evaluate(&limit, Some(&snapshot(80.1, 0.0, 0.0)), 0);
        assert!(!over.admitted);
        assert_eq!(over.reason_code, ReasonCode::CpuOverThreshold);
        assert_eq!(over.breaches[0].threshold, 80.0);
    }

    #[test]
    fn memory_under_at_and_over() {
        let limit = with(None, Some(90.0), None);
        assert!(evaluate(&limit, Some(&snapshot(0.0, 89.0, 0.0)), 0).admitted);
        assert!(evaluate(&limit, Some(&snapshot(0.0, 90.0, 0.0)), 0).admitted);
        let over = evaluate(&limit, Some(&snapshot(0.0, 91.0, 0.0)), 0);
        assert_eq!(over.reason_code, ReasonCode::MemoryOverThreshold);
        assert_eq!(over.breaches[0].observed, 91.0);
    }

    #[test]
    fn disk_under_at_and_over() {
        let limit = with(None, None, Some(50.0));
        assert!(evaluate(&limit, Some(&snapshot(0.0, 0.0, 99.0)), 0).admitted);
        assert!(evaluate(&limit, Some(&snapshot(0.0, 0.0, 100.0)), 0).admitted);
        let over = evaluate(&limit, Some(&snapshot(0.0, 0.0, 101.0)), 0);
        assert_eq!(over.reason_code, ReasonCode::DiskOverThreshold);
    }

    #[test]
    fn running_agents_under_at_and_over() {
        let limit = Thresholds {
            max_running_agents: Some(2),
            ..Thresholds::default()
        };
        assert!(evaluate(&limit, None, 1).admitted);
        for running in [2, 3] {
            let decision = evaluate(&limit, None, running);
            assert!(!decision.admitted);
            assert_eq!(decision.reason_code, ReasonCode::RunningAgentsAtLimit);
        }
        let zero = Thresholds {
            max_running_agents: Some(0),
            ..Thresholds::default()
        };
        assert!(!evaluate(&zero, None, 0).admitted);
    }

    #[test]
    fn a_missing_or_unreadable_meter_fails_open_with_an_unknown_reason() {
        let limit = with(Some(10.0), Some(10.0), Some(10.0));
        let none = evaluate(&limit, None, 0);
        assert!(none.admitted);
        assert_eq!(none.reason_code, ReasonCode::AdmittedMetricUnknown);
        assert_eq!(none.unknown, [MeterId::Cpu, MeterId::Memory, MeterId::Disk]);

        let mut partial = snapshot(5.0, 5.0, 5.0);
        let (kind, unit) = shape(MeterId::Cpu);
        partial.meters[0] = Meter::missing(
            MeterId::Cpu,
            MeterState::Unavailable,
            kind,
            unit,
            None,
            "warming up",
        );
        partial.meters.remove(2);
        let decision = evaluate(&limit, Some(&partial), 0);
        assert!(decision.admitted);
        assert_eq!(decision.unknown, [MeterId::Cpu, MeterId::Disk]);
    }

    #[test]
    fn a_known_breach_wins_over_an_unknown_meter() {
        let limit = with(Some(10.0), Some(10.0), None);
        let mut partial = snapshot(0.0, 50.0, 0.0);
        partial.meters.remove(0);
        let decision = evaluate(&limit, Some(&partial), 0);
        assert!(!decision.admitted);
        assert_eq!(decision.reason_code, ReasonCode::MemoryOverThreshold);
        assert_eq!(decision.unknown, [MeterId::Cpu]);
    }

    #[test]
    fn a_zero_total_or_non_finite_value_is_unknown_not_a_number() {
        let limit = with(Some(10.0), Some(10.0), None);
        let mut broken = snapshot(f64::NAN, 50.0, 0.0);
        broken.meters[1] = ok(MeterId::Memory, 50.0, Some(0.0));
        let decision = evaluate(&limit, Some(&broken), 0);
        assert!(decision.admitted);
        assert_eq!(decision.unknown, [MeterId::Cpu, MeterId::Memory]);
    }

    #[test]
    fn the_first_breach_is_reported_in_a_fixed_order() {
        let limit = Thresholds {
            max_cpu_percent: Some(1.0),
            max_memory_percent: Some(1.0),
            max_disk_percent: Some(1.0),
            max_running_agents: Some(1),
        };
        let decision = evaluate(&limit, Some(&snapshot(50.0, 50.0, 50.0)), 1);
        assert_eq!(decision.reason_code, ReasonCode::RunningAgentsAtLimit);
        assert_eq!(decision.breaches.len(), 4);
        assert_eq!(
            serde_json::to_value(decision.reason_code).unwrap(),
            "running_agents_at_limit"
        );
    }
}
