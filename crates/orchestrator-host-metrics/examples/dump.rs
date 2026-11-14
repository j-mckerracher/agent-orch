//! Prints one live `host.metrics` snapshot as JSON, for comparing against
//! Activity Monitor or Task Manager: `cargo run -p orchestrator-host-metrics
//! --example dump -- <data-dir>`. Takes a few samples first, because CPU
//! needs two.

use std::time::Duration;

use orchestrator_host_metrics::{HostMetrics, SysinfoProbe, MIN_INTERVAL};

fn main() {
    let data_dir = std::env::args_os()
        .nth(1)
        .unwrap_or_else(|| std::env::temp_dir().into_os_string());
    let metrics = HostMetrics::start(SysinfoProbe::new(data_dir), MIN_INTERVAL);
    std::thread::sleep(Duration::from_secs(4));
    let snapshot = metrics.snapshot();
    println!("{}", serde_json::to_string_pretty(&snapshot).unwrap());
}
