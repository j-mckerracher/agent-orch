//! The preference and learning methods through the public `Client` and
//! `Server` API (DPES-1108 phase 6). Each case uses a call-counting handler, so a test can
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
            ServerConfig::new(dir, "memory-test")
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

use crate::session::method_capability;
use crate::USER_ONLY_METHODS;

const WRITES: [&str; 3] = ["memory.set", "memory.remove", "memory.restore"];

#[test]
fn the_memory_methods_route_to_standing_approvals_and_only_the_writes_are_user_only() {
    for method in [
        "memory.list",
        "memory.set",
        "memory.remove",
        "memory.restore",
    ] {
        assert_eq!(
            method_capability(method),
            Some(Capability::StandingApprovals),
            "{method}"
        );
    }
    for method in WRITES {
        assert!(USER_ONLY_METHODS.contains(&method), "{method}");
    }
    assert!(!USER_ONLY_METHODS.contains(&"memory.list"));
    // The notes methods are gone: one source of truth.
    assert_eq!(method_capability("notes.list"), None);
    assert_eq!(method_capability("notes.set"), None);
}

#[test]
fn a_token_only_session_cannot_write_memory_but_lists_it() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = Arc::new(ApproverSecret::generate().unwrap());
    let daemon = serve(dir.path(), &recorder, &approver);

    let mut token_only = connect(
        dir.path(),
        &[Capability::Health, Capability::StandingApprovals],
        None,
    );
    let list = json!({"kind": "preference", "scope": "global"});
    assert!(token_only.memory_list(list).is_ok());
    let entry = json!({"kind": "preference", "scope": "global", "key": "k", "text": "x",
        "idempotency_key": "key-1"});
    assert!(matches!(
        token_only.memory_set(entry.clone()),
        Err(IpcError::Forbidden)
    ));
    assert!(matches!(
        token_only.memory_remove(entry),
        Err(IpcError::Forbidden)
    ));
    assert!(matches!(
        token_only.memory_restore(json!({"archive_id": 1, "idempotency_key": "key-2"})),
        Err(IpcError::Forbidden)
    ));
    // The handler saw the list and nothing else: the writes stopped at the gate.
    assert_eq!(recorder.calls(), ["memory.list"]);
    drop(token_only);
    stop(dir.path(), daemon);
}

#[test]
fn an_approver_session_reaches_the_handler_for_every_memory_method() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = Arc::new(ApproverSecret::generate().unwrap());
    let daemon = serve(dir.path(), &recorder, &approver);

    let mut user = connect(
        dir.path(),
        &[Capability::Health, Capability::StandingApprovals],
        Some(&approver),
    );
    let entry = json!({"kind": "preference", "scope": "global", "key": "k", "text": "x",
        "idempotency_key": "key-1"});
    assert!(user.memory_set(entry.clone()).is_ok());
    assert!(user.memory_remove(entry).is_ok());
    assert!(user
        .memory_restore(json!({"archive_id": 1, "idempotency_key": "key-2"}))
        .is_ok());
    assert!(user
        .memory_list(json!({"kind": "preference", "scope": "global"}))
        .is_ok());
    assert_eq!(
        recorder.calls(),
        [
            "memory.set",
            "memory.remove",
            "memory.restore",
            "memory.list"
        ]
    );
    drop(user);
    stop(dir.path(), daemon);
}
