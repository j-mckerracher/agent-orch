//! Coordinator and review capability behavior through the public `Client` and
//! `Server` API. These tests use no platform-specific socket code, so they also
//! compile and run on Windows.

use crate::{Capability, Client, IpcError, IpcEvent, RpcError, RpcHandler, Server, ServerConfig};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tempfile::TempDir;

struct Recorder {
    capabilities: Vec<Capability>,
    calls: Mutex<Vec<(String, Value)>>,
}

impl Recorder {
    fn new(capabilities: Vec<Capability>) -> Arc<Self> {
        Arc::new(Self {
            capabilities,
            calls: Mutex::new(Vec::new()),
        })
    }
}

impl RpcHandler for Recorder {
    fn capabilities(&self) -> Vec<Capability> {
        self.capabilities.clone()
    }

    fn dispatch(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.calls
            .lock()
            .unwrap()
            .push((method.to_owned(), params.clone()));
        if method == "review.request_changes" {
            return Err(RpcError::Conflict);
        }
        Ok(json!({"method": method, "params": params}))
    }

    fn events_after(&self, _: i64, _: usize) -> Result<Vec<IpcEvent>, RpcError> {
        Ok(Vec::new())
    }

    fn keep_alive(&self) -> bool {
        true
    }
}

fn serve(dir: &Path, handler: &Arc<Recorder>) -> JoinHandle<()> {
    let dir = dir.to_path_buf();
    let handler = handler.clone();
    thread::spawn(move || {
        Server::serve_with_handler(
            ServerConfig::new(dir, "coordinator-review-test")
                .with_idle_timeout(Duration::from_secs(30))
                .with_stream_timeout(Duration::from_millis(50)),
            handler,
        )
        .unwrap();
    })
}

fn connect(dir: &Path, capabilities: &[Capability]) -> Client {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match Client::connect_with_capabilities(dir, capabilities) {
            Ok(client) => return client,
            Err(error) => {
                assert!(Instant::now() < deadline, "daemon not ready: {error}");
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

#[test]
fn capability_set_names_are_unique_and_round_trip() {
    assert_eq!(Capability::ALL.len(), 10);
    let names: BTreeSet<_> = Capability::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(names.len(), Capability::ALL.len());
    assert!(names.contains("coordinator") && names.contains("review"));
    for capability in Capability::ALL {
        let encoded = serde_json::to_value(capability).unwrap();
        assert_eq!(encoded, json!(capability.as_str()));
        assert_eq!(
            serde_json::from_value::<Capability>(encoded).unwrap(),
            capability
        );
    }
}

#[test]
fn client_wrappers_send_the_documented_methods_and_parameters() {
    let dir = TempDir::new().unwrap();
    let handler = Recorder::new(vec![Capability::Coordinator, Capability::Review]);
    let daemon = serve(dir.path(), &handler);
    let mut client = connect(dir.path(), &[Capability::Coordinator, Capability::Review]);

    client
        .coordinator_send_message("p1", "hello", "k1")
        .unwrap();
    client.coordinator_propose("p1", "{}", "k2").unwrap();
    client.coordinator_get("p1", Some(7)).unwrap();
    client
        .review_request_approval(json!({"task_id": "t1"}))
        .unwrap();
    client
        .review_evidence("t1", Some("a1"), Some("src/lib.rs"))
        .unwrap();
    assert!(matches!(
        client.review_request_changes("a1", &"a".repeat(40), "fix", "k3"),
        Err(IpcError::Conflict)
    ));

    let calls = handler.calls.lock().unwrap().clone();
    let methods: Vec<_> = calls.iter().map(|(method, _)| method.as_str()).collect();
    assert_eq!(
        methods,
        [
            "coordinator.send_message",
            "coordinator.propose",
            "coordinator.get",
            "review.request_approval",
            "review.evidence",
            "review.request_changes"
        ]
    );
    assert_eq!(
        calls[0].1,
        json!({"project_id": "p1", "text": "hello", "idempotency_key": "k1"})
    );
    assert_eq!(calls[2].1, json!({"project_id": "p1", "after_sequence": 7}));
    assert_eq!(
        calls[4].1,
        json!({"task_id": "t1", "approval_id": "a1", "path": "src/lib.rs"})
    );
    assert_eq!(calls[5].1["candidate_commit"], "a".repeat(40));

    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn methods_are_unavailable_without_the_negotiated_capability() {
    let dir = TempDir::new().unwrap();
    let handler = Recorder::new(vec![
        Capability::Health,
        Capability::Coordinator,
        Capability::Review,
    ]);
    let daemon = serve(dir.path(), &handler);
    let mut client = connect(dir.path(), &[Capability::Health, Capability::Coordinator]);

    assert!(client.coordinator_get("p1", None).is_ok());
    assert!(matches!(
        client.review_evidence("t1", None, None),
        Err(IpcError::UnsupportedCapability)
    ));
    assert!(matches!(
        client.review_request_approval(json!({"task_id": "t1"})),
        Err(IpcError::UnsupportedCapability)
    ));
    let calls = handler.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 1, "review calls never reached the handler");
    assert_eq!(calls[0].0, "coordinator.get");

    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn a_handler_without_review_never_exposes_review_methods() {
    let dir = TempDir::new().unwrap();
    let handler = Recorder::new(vec![Capability::Health, Capability::Coordinator]);
    let daemon = serve(dir.path(), &handler);
    let mut client = connect(dir.path(), &[Capability::Health, Capability::Coordinator]);
    assert_eq!(
        client.capabilities().unwrap(),
        [Capability::Health, Capability::Coordinator]
    );
    let mut other = Client::connect_with_capabilities(dir.path(), &[Capability::Review]).unwrap();
    assert!(matches!(
        other.review_evidence("t1", None, None),
        Err(IpcError::UnsupportedCapability)
    ));
    assert!(handler.calls.lock().unwrap().is_empty());
    client.shutdown().unwrap();
    daemon.join().unwrap();
}
