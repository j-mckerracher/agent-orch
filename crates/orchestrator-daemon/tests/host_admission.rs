//! Process-level proofs that `host.metrics` exposes the start-admission state
//! of the real `orchestrator-daemon` binary. Runs on Unix and Windows.

#[macro_use]
mod support;

use support::process::{connect, Daemon, Layout};

#[test]
fn admission_is_off_by_default() {
    let layout = Layout::new();
    let _daemon = Daemon::start(&layout);
    let mut client = connect(&layout.data);

    let result = timed!(client.host_metrics()).unwrap();
    let admission = &result["admission"];
    assert_eq!(admission["enabled"], false, "{admission}");
    assert!(admission["decision"].is_null(), "{admission}");
    assert_eq!(admission["deferred_task_ids"], serde_json::json!([]));
}

#[test]
fn configured_thresholds_are_reported() {
    let layout = Layout::new();
    let _daemon = Daemon::start_with_env(
        &layout,
        &[
            ("AGENT_ORCH_ADMISSION_MAX_MEMORY_PERCENT", "85"),
            ("AGENT_ORCH_ADMISSION_MAX_RUNNING_AGENTS", "3"),
        ],
    );
    let mut client = connect(&layout.data);

    let result = timed!(client.host_metrics()).unwrap();
    let admission = &result["admission"];
    assert_eq!(admission["enabled"], true, "{admission}");
    assert_eq!(admission["thresholds"]["max_memory_percent"], 85.0);
    assert_eq!(admission["thresholds"]["max_running_agents"], 3);
    assert!(admission["thresholds"]["max_cpu_percent"].is_null());
}
