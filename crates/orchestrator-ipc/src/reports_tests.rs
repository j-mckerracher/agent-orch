//! The scout report methods (`reports.list`, `reports.get`, `reports.relay`) through the public `Client` and
//! `Server` API (DPES-1105 phase 4a). Each case uses a call-counting handler, so a test can
//! tell a refusal at the gate from a call that reached the handler. No
//! platform-specific socket code, so these also compile and run on Windows.

use crate::{
    ApproverSecret, Capability, Client, IpcError, IpcEvent, RpcError, RpcHandler, Server,
    ServerConfig, DEFAULT_IO_TIMEOUT, USER_ONLY_METHODS,
};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const METHODS: [&str; 3] = ["reports.list", "reports.get", "reports.relay"];

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
        vec![Capability::Health, Capability::Coordinator]
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
            ServerConfig::new(dir, "reports-test")
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
fn report_methods_need_the_coordinator_capability() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = Arc::new(ApproverSecret::generate().unwrap());
    let daemon = serve(dir.path(), &recorder, &approver);
    for proof in [None, Some(&approver)] {
        let mut client = connect(dir.path(), &[Capability::Health], proof);
        for method in METHODS {
            // `call` skips the client-side gate, so the server answers -32004.
            assert!(
                matches!(
                    client.call(method, json!({})),
                    Err(IpcError::UnsupportedCapability)
                ),
                "{method} was not refused as an unnegotiated capability"
            );
        }
        assert!(matches!(
            client.reports_list(json!({})),
            Err(IpcError::UnsupportedCapability)
        ));
    }
    assert!(recorder.calls().is_empty());
    stop(dir.path(), daemon);
}

#[test]
fn a_token_only_session_lists_reports_but_cannot_read_or_relay_one() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = Arc::new(ApproverSecret::generate().unwrap());
    let daemon = serve(dir.path(), &recorder, &approver);

    let mut client = connect(dir.path(), &[Capability::Health, Capability::Coordinator], None);
    assert!(!client.is_approver());
    assert!(client.reports_list(json!({})).is_ok());
    assert!(matches!(
        client.reports_get(json!({"task_id": "task-1"})),
        Err(IpcError::Forbidden)
    ));
    assert!(matches!(
        client.reports_relay(json!({"task_id": "task-1", "idempotency_key": "key-1"})),
        Err(IpcError::Forbidden)
    ));
    assert_eq!(recorder.calls(), ["reports.list"]);
    drop(client);
    stop(dir.path(), daemon);
}

#[test]
fn an_approver_session_reaches_every_report_method() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = Arc::new(ApproverSecret::generate().unwrap());
    let daemon = serve(dir.path(), &recorder, &approver);

    let mut client = connect(
        dir.path(),
        &[Capability::Health, Capability::Coordinator],
        Some(&approver),
    );
    assert!(client.is_approver());
    assert!(client.reports_list(json!({})).is_ok());
    assert!(client.reports_get(json!({"task_id": "task-1"})).is_ok());
    assert!(client
        .reports_relay(json!({"task_id": "task-1", "idempotency_key": "key-1"}))
        .is_ok());
    assert_eq!(recorder.calls(), METHODS);
    drop(client);
    stop(dir.path(), daemon);
}

#[test]
fn only_reading_and_relaying_a_report_are_user_only() {
    for method in METHODS {
        assert_eq!(
            USER_ONLY_METHODS.contains(&method),
            method != "reports.list",
            "{method} has the wrong user-only status"
        );
    }
}
