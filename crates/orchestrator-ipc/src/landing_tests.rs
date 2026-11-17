//! The `landing` capability and its two methods through the public `Client` and
//! `Server` API (DPES-1105). Each case uses a call-counting handler, so a test can
//! tell a refusal at the gate from a call that reached the handler. No
//! platform-specific socket code, so these also compile and run on Windows.

use crate::approver::USER_ONLY_METHODS;
use crate::session::method_capability;
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
        vec![Capability::Health, Capability::Landing]
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
            ServerConfig::new(dir, "landing-test")
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
fn landing_is_named_and_round_trips() {
    assert!(Capability::ALL.contains(&Capability::Landing));
    assert_eq!(Capability::Landing.as_str(), "landing");
    assert_eq!(Capability::from_name("landing"), Some(Capability::Landing));
    let encoded = serde_json::to_value(Capability::Landing).unwrap();
    assert_eq!(encoded, json!("landing"));
    assert_eq!(
        serde_json::from_value::<Capability>(encoded).unwrap(),
        Capability::Landing
    );
}

#[test]
fn pull_methods_route_to_landing_and_are_not_user_only() {
    for method in ["pulls.list", "pulls.refresh"] {
        assert_eq!(method_capability(method), Some(Capability::Landing));
        assert!(!USER_ONLY_METHODS.contains(&method));
    }
    // Merging is an action kind through `actions.resolve`, not a method of its own.
    assert_eq!(method_capability("pulls.merge"), None);
    assert_eq!(method_capability("delivery.merge"), None);
}

#[test]
fn pull_methods_need_the_landing_capability() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = Arc::new(ApproverSecret::generate().unwrap());
    let daemon = serve(dir.path(), &recorder, &approver);
    for proof in [None, Some(&approver)] {
        let mut client = connect(dir.path(), &[Capability::Health], proof);
        for method in ["pulls.list", "pulls.refresh"] {
            // `call` skips the client-side gate, so the server answers -32004.
            assert!(
                matches!(
                    client.call(method, json!({})),
                    Err(IpcError::UnsupportedCapability)
                ),
                "{method} was not refused as an unnegotiated capability"
            );
        }
    }
    assert!(recorder.calls().is_empty());
    stop(dir.path(), daemon);
}

#[test]
fn a_token_only_session_reaches_both_pull_methods() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = Arc::new(ApproverSecret::generate().unwrap());
    let daemon = serve(dir.path(), &recorder, &approver);

    let mut client = connect(dir.path(), &[Capability::Health, Capability::Landing], None);
    assert!(client.pulls_list(json!({"project_id": "p1"})).is_ok());
    assert!(client.pulls_refresh(json!({"task_id": "task-1"})).is_ok());
    assert_eq!(recorder.calls(), ["pulls.list", "pulls.refresh"]);
    drop(client);
    stop(dir.path(), daemon);
}

#[test]
fn the_client_refuses_a_pull_call_without_the_landing_capability() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = Arc::new(ApproverSecret::generate().unwrap());
    let daemon = serve(dir.path(), &recorder, &approver);

    let mut client = connect(dir.path(), &[Capability::Health], None);
    assert!(matches!(
        client.pulls_list(json!({})),
        Err(IpcError::UnsupportedCapability)
    ));
    assert!(recorder.calls().is_empty());
    drop(client);
    stop(dir.path(), daemon);
}
