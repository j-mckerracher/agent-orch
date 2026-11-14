//! `queue.snapshot` capability gating through the public `Client` and `Server`
//! API. No platform-specific socket code, so these also run on Windows.

use crate::{Capability, Client, IpcError, IpcEvent, RpcError, RpcHandler, Server, ServerConfig};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tempfile::TempDir;

struct Recorder {
    capabilities: Vec<Capability>,
    calls: Mutex<Vec<String>>,
}

impl RpcHandler for Recorder {
    fn capabilities(&self) -> Vec<Capability> {
        self.capabilities.clone()
    }

    fn dispatch(&self, method: &str, _: Value) -> Result<Value, RpcError> {
        self.calls.lock().unwrap().push(method.to_owned());
        Ok(json!({"method": method}))
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
            ServerConfig::new(dir, "queue-test")
                .with_idle_timeout(Duration::from_secs(30))
                .with_stream_timeout(Duration::from_secs(5)),
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
fn queue_capability_is_named_and_part_of_the_full_set() {
    assert_eq!(Capability::Queue.as_str(), "queue");
    assert!(Capability::ALL.contains(&Capability::Queue));
    assert_eq!(
        serde_json::from_value::<Capability>(json!("queue")).unwrap(),
        Capability::Queue
    );
}

#[test]
fn queue_snapshot_needs_the_negotiated_queue_capability() {
    let dir = TempDir::new().unwrap();
    let handler = Arc::new(Recorder {
        capabilities: vec![Capability::Health, Capability::Queue],
        calls: Mutex::new(Vec::new()),
    });
    let daemon = serve(dir.path(), &handler);
    let mut without = connect(dir.path(), &[Capability::Health]);
    let mut with = connect(dir.path(), &[Capability::Health, Capability::Queue]);

    // `call` skips the client-side gate, so this reaches the server's gate.
    assert!(matches!(
        without.call("queue.snapshot", json!({})),
        Err(IpcError::UnsupportedCapability)
    ));
    assert!(matches!(
        without.queue_snapshot(),
        Err(IpcError::UnsupportedCapability)
    ));
    assert!(handler.calls.lock().unwrap().is_empty());

    assert_eq!(with.queue_snapshot().unwrap()["method"], "queue.snapshot");
    assert_eq!(*handler.calls.lock().unwrap(), ["queue.snapshot"]);

    drop(without);
    with.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn a_handler_without_queue_never_exposes_it() {
    let dir = TempDir::new().unwrap();
    let handler = Arc::new(Recorder {
        capabilities: vec![Capability::Health],
        calls: Mutex::new(Vec::new()),
    });
    let daemon = serve(dir.path(), &handler);
    let mut client = connect(dir.path(), &[Capability::Health, Capability::Queue]);

    assert_eq!(client.capabilities().unwrap(), [Capability::Health]);
    assert!(matches!(
        client.queue_snapshot(),
        Err(IpcError::UnsupportedCapability)
    ));
    assert!(handler.calls.lock().unwrap().is_empty());

    client.shutdown().unwrap();
    daemon.join().unwrap();
}
