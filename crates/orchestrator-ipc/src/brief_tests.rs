//! The return brief methods through the public `Client` and `Server` API (DPES-1105 phase 5b). Each case uses a call-counting handler, so a test can
//! tell a refusal at the gate from a call that reached the handler. No
//! platform-specific socket code, so these also compile and run on Windows.

use crate::{
    ApproverSecret, Capability, Client, IpcError, IpcEvent, RpcError, RpcHandler, Server,
    ServerConfig, DEFAULT_IO_TIMEOUT,
};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Answers with the method name and records every call that reaches it.
#[derive(Default)]
struct Recorder {
    calls: Mutex<Vec<String>>,
}

impl Recorder {
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl RpcHandler for Recorder {
    fn capabilities(&self) -> Vec<Capability> {
        vec![Capability::Health, Capability::StandingApprovals]
    }

    fn dispatch(&self, method: &str, _params: Value) -> Result<Value, RpcError> {
        self.calls.lock().unwrap().push(method.to_owned());
        Ok(json!({"method": method}))
    }

    fn events_after(&self, _after_sequence: i64, _limit: usize) -> Result<Vec<IpcEvent>, RpcError> {
        Ok(Vec::new())
    }
}

fn serve(dir: &Path, recorder: &Arc<Recorder>, approver: &Arc<ApproverSecret>) -> JoinHandle<()> {
    let dir = dir.to_path_buf();
    let recorder = recorder.clone();
    // `ApproverSecret` is deliberately not `Clone`; the server gets its own copy.
    let approver = ApproverSecret::from_wire(&approver.to_wire()).unwrap();
    thread::spawn(move || {
        Server::serve_with_handler(
            ServerConfig::new(dir, "brief-test")
                .with_idle_timeout(Duration::from_secs(30))
                .with_stream_timeout(Duration::from_secs(5))
                .with_approver(approver),
            recorder,
        )
        .unwrap();
    })
}

fn connect(
    dir: &Path,
    capabilities: &[Capability],
    approver: Option<&Arc<ApproverSecret>>,
) -> Client {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match Client::connect_with_approver(
            dir,
            capabilities,
            DEFAULT_IO_TIMEOUT,
            approver.cloned(),
        ) {
            Ok(client) => return client,
            Err(error) => {
                assert!(Instant::now() < deadline, "daemon not ready: {error}");
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

fn stop(dir: &Path, daemon: JoinHandle<()>) {
    connect(dir, &[Capability::Health], None)
        .shutdown()
        .unwrap();
    daemon.join().unwrap();
}

#[test]
fn the_brief_methods_route_to_the_standing_approvals_capability() {
    use crate::session::method_capability;
    for method in ["monitoring.brief", "monitoring.brief_ack"] {
        assert_eq!(
            method_capability(method),
            Some(Capability::StandingApprovals),
            "{method}"
        );
    }
    // The grants and the older monitoring methods keep their routes.
    assert_eq!(
        method_capability("grants.list"),
        Some(Capability::StandingApprovals)
    );
    assert_eq!(
        method_capability("monitoring.ack"),
        Some(Capability::Monitoring)
    );
}

#[test]
fn the_brief_methods_are_open_and_monitoring_set_stays_user_only() {
    use crate::approver::USER_ONLY_METHODS;
    assert!(!USER_ONLY_METHODS.contains(&"monitoring.brief"));
    assert!(!USER_ONLY_METHODS.contains(&"monitoring.brief_ack"));
    assert!(USER_ONLY_METHODS.contains(&"monitoring.set"));
}

#[test]
fn brief_methods_need_the_standing_approvals_capability() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = Arc::new(ApproverSecret::generate().unwrap());
    let daemon = serve(dir.path(), &recorder, &approver);
    // The monitoring capability alone is not enough.
    let mut client = connect(
        dir.path(),
        &[Capability::Health, Capability::Monitoring],
        None,
    );
    for method in ["monitoring.brief", "monitoring.brief_ack"] {
        assert!(
            matches!(
                client.call(method, json!({})),
                Err(IpcError::UnsupportedCapability)
            ),
            "{method} was not refused as an unnegotiated capability"
        );
    }
    assert!(recorder.calls().is_empty());
    drop(client);
    stop(dir.path(), daemon);
}

#[test]
fn a_token_only_session_reads_and_acknowledges_the_brief() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = Arc::new(ApproverSecret::generate().unwrap());
    let daemon = serve(dir.path(), &recorder, &approver);

    let mut token_only = connect(
        dir.path(),
        &[Capability::Health, Capability::StandingApprovals],
        None,
    );
    assert!(!token_only.is_approver());
    assert!(token_only.monitoring_brief().is_ok());
    assert!(token_only.monitoring_brief_ack("key-1").is_ok());
    assert_eq!(
        recorder.calls(),
        ["monitoring.brief", "monitoring.brief_ack"]
    );
    drop(token_only);
    stop(dir.path(), daemon);
}
