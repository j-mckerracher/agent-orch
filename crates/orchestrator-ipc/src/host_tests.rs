//! `host.metrics` capability behavior through the public `Client` and
//! `Server` API. No platform-specific socket code, so this also runs on Windows.

use crate::{Capability, Client, IpcError, IpcEvent, RpcError, RpcHandler, Server, ServerConfig};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tempfile::TempDir;

struct Recorder {
    calls: Mutex<Vec<String>>,
}

impl RpcHandler for Recorder {
    fn capabilities(&self) -> Vec<Capability> {
        vec![Capability::Health, Capability::Host]
    }

    fn dispatch(&self, method: &str, _: Value) -> Result<Value, RpcError> {
        self.calls.lock().unwrap().push(method.to_owned());
        Ok(json!({"schema_version": 1}))
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
            ServerConfig::new(dir, "host-metrics-test")
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
fn the_host_capability_is_named_and_listed() {
    assert!(Capability::ALL.contains(&Capability::Host));
    assert_eq!(Capability::Host.as_str(), "host");
    assert_eq!(
        serde_json::from_value::<Capability>(json!("host")).unwrap(),
        Capability::Host
    );
}

#[test]
fn host_metrics_needs_the_host_capability() {
    let dir = TempDir::new().unwrap();
    let handler = Arc::new(Recorder {
        calls: Mutex::new(Vec::new()),
    });
    let daemon = serve(dir.path(), &handler);

    let mut without = connect(dir.path(), &[Capability::Health]);
    assert!(matches!(
        without.call("host.metrics", Value::Null),
        Err(IpcError::UnsupportedCapability)
    ));
    assert!(matches!(
        without.host_metrics(),
        Err(IpcError::UnsupportedCapability)
    ));
    assert!(handler.calls.lock().unwrap().is_empty());

    let mut with = connect(dir.path(), &[Capability::Health, Capability::Host]);
    assert_eq!(with.host_metrics().unwrap(), json!({"schema_version": 1}));
    assert_eq!(*handler.calls.lock().unwrap(), vec!["host.metrics"]);

    drop(without);
    with.shutdown().unwrap();
    daemon.join().unwrap();
}
