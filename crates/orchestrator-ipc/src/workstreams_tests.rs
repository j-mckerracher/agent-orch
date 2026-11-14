//! Workstream capability behavior through the public `Client` and `Server`
//! API. These tests use no platform-specific socket code, so they also compile
//! and run on Windows.

use crate::{Capability, Client, IpcError, IpcEvent, RpcError, RpcHandler, Server, ServerConfig};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const WORKSTREAM_METHODS: [&str; 9] = [
    "workstreams.create",
    "workstreams.list",
    "workstreams.get",
    "workstreams.update",
    "workstreams.add_objective",
    "workstreams.revise_objective",
    "workstreams.set_gate",
    "workstreams.link_task",
    "workstreams.unlink_task",
];

struct Recorder {
    capabilities: Vec<Capability>,
    calls: Mutex<Vec<(String, Value)>>,
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
        Ok(json!({"method": method, "params": params}))
    }

    fn events_after(&self, _: i64, _: usize) -> Result<Vec<IpcEvent>, RpcError> {
        Ok(Vec::new())
    }

    fn keep_alive(&self) -> bool {
        true
    }
}

fn serve(dir: &Path, capabilities: Vec<Capability>) -> (Arc<Recorder>, JoinHandle<()>) {
    let handler = Arc::new(Recorder {
        capabilities,
        calls: Mutex::new(Vec::new()),
    });
    let dir = dir.to_path_buf();
    let served = handler.clone();
    let thread = thread::spawn(move || {
        Server::serve_with_handler(
            ServerConfig::new(dir, "workstreams-test")
                .with_idle_timeout(Duration::from_secs(30))
                .with_stream_timeout(Duration::from_secs(5)),
            served,
        )
        .unwrap();
    });
    (handler, thread)
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
fn workstreams_capability_is_named_and_round_trips() {
    assert!(Capability::ALL.contains(&Capability::Workstreams));
    assert_eq!(Capability::Workstreams.as_str(), "workstreams");
    let encoded = serde_json::to_value(Capability::Workstreams).unwrap();
    assert_eq!(encoded, json!("workstreams"));
    assert_eq!(
        serde_json::from_value::<Capability>(encoded).unwrap(),
        Capability::Workstreams
    );
}

#[test]
fn wrapper_sends_workstream_methods_with_the_capability() {
    let dir = TempDir::new().unwrap();
    let (handler, daemon) = serve(
        dir.path(),
        vec![Capability::Health, Capability::Workstreams],
    );
    let mut client = connect(dir.path(), &[Capability::Workstreams]);

    let params = json!({"workstream_id": "w1"});
    let reply = client
        .workstreams_call("workstreams.get", params.clone())
        .unwrap();
    assert_eq!(reply["method"], "workstreams.get");
    assert_eq!(reply["params"], params);
    assert!(matches!(
        client.workstreams_call("tasks.list", json!({})),
        Err(IpcError::Operation)
    ));
    assert_eq!(handler.calls.lock().unwrap().len(), 1);

    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn every_workstream_method_is_gated_by_the_negotiated_capability() {
    let dir = TempDir::new().unwrap();
    let (handler, daemon) = serve(
        dir.path(),
        vec![
            Capability::Health,
            Capability::Projects,
            Capability::Workstreams,
        ],
    );
    let mut without = connect(dir.path(), &[Capability::Health, Capability::Projects]);
    for method in WORKSTREAM_METHODS {
        // `Client::call` skips the client-side gate, so the server decides.
        let result = without.call(method, json!({}));
        assert!(
            matches!(result, Err(IpcError::UnsupportedCapability)),
            "{method} must require the workstreams capability, got {result:?}"
        );
        assert!(matches!(
            without.workstreams_call(method, json!({})),
            Err(IpcError::UnsupportedCapability)
        ));
    }
    assert!(handler.calls.lock().unwrap().is_empty());

    let mut with = connect(dir.path(), &[Capability::Health, Capability::Workstreams]);
    for method in WORKSTREAM_METHODS {
        assert_eq!(with.call(method, json!({})).unwrap()["method"], method);
    }
    assert_eq!(
        handler.calls.lock().unwrap().len(),
        WORKSTREAM_METHODS.len()
    );

    drop(without);
    with.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn a_handler_without_workstreams_never_negotiates_it() {
    let dir = TempDir::new().unwrap();
    let (handler, daemon) = serve(dir.path(), vec![Capability::Health]);
    let mut client = connect(dir.path(), &[Capability::Health, Capability::Workstreams]);
    assert_eq!(client.capabilities().unwrap(), [Capability::Health]);
    assert!(matches!(
        client.workstreams_call("workstreams.list", json!({})),
        Err(IpcError::UnsupportedCapability)
    ));
    assert!(handler.calls.lock().unwrap().is_empty());
    client.shutdown().unwrap();
    daemon.join().unwrap();
}
