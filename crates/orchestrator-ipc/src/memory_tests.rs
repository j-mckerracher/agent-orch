//! The notes methods through the public `Client` and
//! `Server` API (DPES-1105 phase 5b). Each case uses a call-counting handler, so a test can
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
            ServerConfig::new(dir, "notes-test")
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

#[test]
fn the_notes_methods_route_to_standing_approvals_and_only_the_write_is_user_only() {
    for method in ["notes.list", "notes.set"] {
        assert_eq!(
            method_capability(method),
            Some(Capability::StandingApprovals),
            "{method}"
        );
    }
    assert!(USER_ONLY_METHODS.contains(&"notes.set"));
    assert!(!USER_ONLY_METHODS.contains(&"notes.list"));
}

#[test]
fn a_token_only_session_cannot_set_notes_but_lists_them() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = Arc::new(ApproverSecret::generate().unwrap());
    let daemon = serve(dir.path(), &recorder, &approver);

    let mut token_only = connect(
        dir.path(),
        &[Capability::Health, Capability::StandingApprovals],
        None,
    );
    assert!(token_only.notes_list(json!({"scope": "global"})).is_ok());
    assert!(matches!(
        token_only.notes_set(json!({"scope": "global", "notes": [], "idempotency_key": "key-1"})),
        Err(IpcError::Forbidden)
    ));
    // The handler saw the list and nothing else: the write stopped at the gate.
    assert_eq!(recorder.calls(), ["notes.list"]);
    drop(token_only);
    stop(dir.path(), daemon);
}

#[test]
fn an_approver_session_reaches_the_handler_for_both_notes_methods() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = Arc::new(ApproverSecret::generate().unwrap());
    let daemon = serve(dir.path(), &recorder, &approver);

    let mut user = connect(
        dir.path(),
        &[Capability::Health, Capability::StandingApprovals],
        Some(&approver),
    );
    assert!(user
        .notes_set(json!({"scope": "global", "notes": [], "idempotency_key": "key-1"}))
        .is_ok());
    assert!(user.notes_list(json!({"scope": "global"})).is_ok());
    assert_eq!(recorder.calls(), ["notes.set", "notes.list"]);
    drop(user);
    stop(dir.path(), daemon);
}
