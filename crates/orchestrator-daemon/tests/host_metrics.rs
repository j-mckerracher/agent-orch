//! Process-level `host.metrics` proofs against the real `orchestrator-daemon`
//! binary over the authenticated local transport. Runs on Unix and Windows.
//!
//! Values are host-dependent, so the tests check the contract, not numbers:
//! every resource is present, an `ok` meter carries a finite value, and any
//! other meter carries no value and a reason (never a zero).

#[macro_use]
mod support;

use orchestrator_ipc::{Capability, Client, IpcError};
use serde_json::Value;
use std::time::{Duration, Instant};
use support::process::{connect, Daemon, Layout, CALL_TIMEOUT};

const WAIT: Duration = Duration::from_secs(30);

fn check_meter(meter: &Value) {
    let state = meter["state"].as_str().unwrap();
    assert!(
        ["ok", "unavailable", "unsupported"].contains(&state),
        "{meter}"
    );
    assert!(["utilization", "capacity"].contains(&meter["kind"].as_str().unwrap()));
    assert!(["percent", "bytes"].contains(&meter["unit"].as_str().unwrap()));
    for field in ["scope", "value", "total", "observed_at", "reason"] {
        assert!(meter.get(field).is_some(), "{field} missing: {meter}");
    }
    if state == "ok" {
        let value = meter["value"].as_f64().unwrap();
        assert!(value.is_finite() && value >= 0.0, "{meter}");
        assert!(meter["observed_at"].is_string(), "{meter}");
        assert!(meter["reason"].is_null(), "{meter}");
    } else {
        assert!(meter["value"].is_null(), "{meter}");
        assert!(
            !meter["reason"].as_str().unwrap_or("").is_empty(),
            "{meter}"
        );
    }
}

#[test]
fn host_metrics_serves_every_resource_through_the_real_daemon() {
    let layout = Layout::new();
    let _daemon = Daemon::start(&layout);
    let mut client = connect(&layout.data);

    let deadline = Instant::now() + WAIT;
    let snapshot = loop {
        let started = Instant::now();
        let snapshot = timed!(client.host_metrics()).unwrap();
        assert!(
            started.elapsed() < CALL_TIMEOUT,
            "host.metrics must answer from the cache"
        );
        let memory_ready = snapshot["meters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|meter| meter["meter"] == "memory" && meter["state"] == "ok");
        if memory_ready || Instant::now() > deadline {
            break snapshot;
        }
        std::thread::sleep(Duration::from_millis(100));
    };

    assert_eq!(snapshot["schema_version"], 1);
    assert!(snapshot["sample_interval_ms"].as_u64().unwrap() >= 500);
    let meters = snapshot["meters"].as_array().unwrap();
    for resource in ["cpu", "memory", "disk", "gpu"] {
        assert!(
            meters.iter().any(|meter| meter["meter"] == resource),
            "{resource} missing: {snapshot}"
        );
    }
    meters.iter().for_each(check_meter);
    let memory = meters.iter().find(|m| m["meter"] == "memory").unwrap();
    assert_eq!(memory["state"], "ok", "no memory sample within {WAIT:?}");
    assert_eq!(memory["kind"], "capacity");

    // Other RPCs keep working next to the metrics call.
    assert!(timed!(client.projects_list()).is_ok());
}

#[test]
fn host_metrics_needs_the_host_capability() {
    let layout = Layout::new();
    let _daemon = Daemon::start(&layout);
    let mut client = Client::connect_with_timeout(
        &layout.data,
        &[Capability::Health, Capability::Projects],
        CALL_TIMEOUT,
    )
    .unwrap();
    assert!(matches!(
        client.host_metrics(),
        Err(IpcError::UnsupportedCapability)
    ));
    assert!(client.projects_list().is_ok());
}
