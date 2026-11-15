//! The conversation-history capability (DPES-1105) through the public `Client` and
//! `Server` API. An older daemon does not know the name, so a desktop that asks for
//! it is refused and degrades; these tests pin the name and the gate.

use crate::{Capability, Client, IpcError, RpcError, RpcHandler, Server, ServerConfig};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const HISTORY_METHODS: [&str; 4] = [
    "conversations.clear",
    "coordinator.clear",
    "coordinator.remove",
    "coordinator.restore",
];

struct Recorder {
    capabilities: Vec<Capability>,
    calls: Mutex<Vec<String>>,
}

impl RpcHandler for Recorder {
    fn capabilities(&self) -> Vec<Capability> {
        self.capabilities.clone()
    }

    fn dispatch(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.calls.lock().unwrap().push(method.to_owned());
        Ok(json!({"method": method, "params": params}))
    }

    fn events_after(&self, _: i64, _: usize) -> Result<Vec<crate::IpcEvent>, RpcError> {
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
            ServerConfig::new(dir, "history-test")
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
fn the_history_capability_is_named_and_round_trips() {
    assert!(Capability::ALL.contains(&Capability::ConversationHistory));
    assert_eq!(
        Capability::ConversationHistory.as_str(),
        "conversation_history"
    );
    assert_eq!(
        Capability::from_name("conversation_history"),
        Some(Capability::ConversationHistory)
    );
    let encoded = serde_json::to_value(Capability::ConversationHistory).unwrap();
    assert_eq!(encoded, json!("conversation_history"));
}

#[test]
fn every_history_method_is_gated_by_the_negotiated_capability() {
    let dir = TempDir::new().unwrap();
    let (handler, daemon) = serve(
        dir.path(),
        vec![
            Capability::Health,
            Capability::Coordinator,
            Capability::ConversationHistory,
        ],
    );
    let mut without = connect(dir.path(), &[Capability::Health, Capability::Coordinator]);
    for method in HISTORY_METHODS {
        // `Client::call` skips the client-side gate, so the server decides.
        let result = without.call(method, json!({}));
        assert!(
            matches!(result, Err(IpcError::UnsupportedCapability)),
            "{method} must require conversation_history, got {result:?}"
        );
    }
    assert!(handler.calls.lock().unwrap().is_empty());

    let mut with = connect(
        dir.path(),
        &[Capability::Health, Capability::ConversationHistory],
    );
    for method in HISTORY_METHODS {
        assert_eq!(with.call(method, json!({})).unwrap()["method"], method);
    }
    assert_eq!(handler.calls.lock().unwrap().len(), HISTORY_METHODS.len());

    drop(without);
    with.shutdown().unwrap();
    daemon.join().unwrap();
}
