//! The `standing_approvals` capability and its three methods through the public `Client` and
//! `Server` API (DPES-1105). Each case uses a call-counting handler, so a test can
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
            ServerConfig::new(dir, "grants-test")
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
fn standing_approvals_is_named_and_round_trips() {
    assert!(Capability::ALL.contains(&Capability::StandingApprovals));
    assert_eq!(Capability::ALL.len(), 25);
    assert_eq!(Capability::StandingApprovals.as_str(), "standing_approvals");
    assert_eq!(
        Capability::from_name("standing_approvals"),
        Some(Capability::StandingApprovals)
    );
    let encoded = serde_json::to_value(Capability::StandingApprovals).unwrap();
    assert_eq!(encoded, json!("standing_approvals"));
    assert_eq!(
        serde_json::from_value::<Capability>(encoded).unwrap(),
        Capability::StandingApprovals
    );
}

#[test]
fn grant_methods_need_the_standing_approvals_capability() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = Arc::new(ApproverSecret::generate().unwrap());
    let daemon = serve(dir.path(), &recorder, &approver);
    for proof in [None, Some(&approver)] {
        let mut client = connect(dir.path(), &[Capability::Health], proof);
        for method in ["grants.create", "grants.revoke", "grants.list"] {
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
fn a_token_only_session_cannot_create_or_revoke_a_grant_but_lists_them() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = Arc::new(ApproverSecret::generate().unwrap());
    let daemon = serve(dir.path(), &recorder, &approver);

    let mut token_only = connect(
        dir.path(),
        &[Capability::Health, Capability::StandingApprovals],
        None,
    );
    assert!(token_only.grants_list(json!({})).is_ok());
    assert!(matches!(
        token_only.grants_create(json!({"idempotency_key": "key-1"})),
        Err(IpcError::Forbidden)
    ));
    assert!(matches!(
        token_only.grants_revoke(json!({"grant_id": "grant-1", "idempotency_key": "key-2"})),
        Err(IpcError::Forbidden)
    ));
    // The handler saw the list and nothing else: the two writes stopped at the gate.
    assert_eq!(recorder.calls(), ["grants.list"]);
    drop(token_only);
    stop(dir.path(), daemon);
}

#[test]
fn an_approver_session_reaches_the_handler_for_every_grant_method() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = Arc::new(ApproverSecret::generate().unwrap());
    let daemon = serve(dir.path(), &recorder, &approver);

    let mut user = connect(
        dir.path(),
        &[Capability::Health, Capability::StandingApprovals],
        Some(&approver),
    );
    assert!(user.is_approver());
    assert!(user
        .grants_create(json!({"idempotency_key": "key-1"}))
        .is_ok());
    assert!(user
        .grants_revoke(json!({"grant_id": "grant-1", "idempotency_key": "key-2"}))
        .is_ok());
    assert!(user.grants_list(json!({})).is_ok());
    assert_eq!(
        recorder.calls(),
        ["grants.create", "grants.revoke", "grants.list"]
    );
    drop(user);
    stop(dir.path(), daemon);
}

#[test]
fn the_grant_error_codes_map_to_their_own_ipc_errors() {
    assert_eq!(RpcError::GrantNotFound.code(), -32024);
    assert_eq!(RpcError::GrantInvalid.code(), -32025);
    assert!(matches!(
        crate::client::map_remote(RpcError::GrantNotFound.code()),
        IpcError::GrantNotFound
    ));
    assert!(matches!(
        crate::client::map_remote(RpcError::GrantInvalid.code()),
        IpcError::GrantInvalid
    ));
}
