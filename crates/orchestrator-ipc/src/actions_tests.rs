//! The per-call context a session passes to `RpcHandler::dispatch_in`, and the
//! `orchestrator_actions` capability name (DPES-1105). No socket: each case
//! calls the trait directly.

use crate::client::map_remote;
use crate::{
    ApproverSecret, CallContext, Capability, Client, IpcError, IpcEvent, RpcError, RpcHandler,
    Server, ServerConfig, DEFAULT_IO_TIMEOUT,
};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Answers every method with its name, so a test can see what was called.
struct Echo;

impl RpcHandler for Echo {
    fn capabilities(&self) -> Vec<Capability> {
        vec![Capability::Health, Capability::OrchestratorActions]
    }

    fn dispatch(&self, method: &str, _params: Value) -> Result<Value, RpcError> {
        Ok(json!({"method": method}))
    }

    fn events_after(&self, _after_sequence: i64, _limit: usize) -> Result<Vec<IpcEvent>, RpcError> {
        Ok(Vec::new())
    }
}

#[test]
fn a_handler_that_ignores_the_context_runs_dispatch() {
    let negotiated = [Capability::OrchestratorActions];
    let with = Echo.dispatch_in(
        "conversations.send",
        Value::Null,
        &CallContext::new(&negotiated),
    );
    let without = Echo.dispatch_in("conversations.send", Value::Null, &CallContext::NONE);
    assert_eq!(with.unwrap(), json!({"method": "conversations.send"}));
    assert_eq!(without.unwrap(), json!({"method": "conversations.send"}));
}

#[test]
fn a_context_reports_only_the_capabilities_it_was_given() {
    let negotiated = [Capability::Health, Capability::OrchestratorActions];
    let context = CallContext::new(&negotiated);
    assert!(context.negotiated(Capability::OrchestratorActions));
    assert!(!context.negotiated(Capability::Workstreams));
    assert!(!CallContext::NONE.negotiated(Capability::OrchestratorActions));
    assert!(!CallContext::default().negotiated(Capability::Health));
}

#[test]
fn orchestrator_actions_is_named_and_round_trips() {
    assert!(Capability::ALL.contains(&Capability::OrchestratorActions));
    assert_eq!(
        Capability::OrchestratorActions.as_str(),
        "orchestrator_actions"
    );
    let encoded = serde_json::to_value(Capability::OrchestratorActions).unwrap();
    assert_eq!(encoded, json!("orchestrator_actions"));
    assert_eq!(
        serde_json::from_value::<Capability>(encoded).unwrap(),
        Capability::OrchestratorActions
    );
}

const DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn serve(dir: &Path, approver: &Arc<ApproverSecret>) -> JoinHandle<()> {
    let dir = dir.to_path_buf();
    // `ApproverSecret` is deliberately not `Clone`; the server gets its own copy.
    let approver = ApproverSecret::from_wire(&approver.to_wire()).unwrap();
    thread::spawn(move || {
        Server::serve_with_handler(
            ServerConfig::new(dir, "actions-test")
                .with_idle_timeout(Duration::from_secs(30))
                .with_stream_timeout(Duration::from_secs(5))
                .with_approver(approver),
            Arc::new(Echo),
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

#[test]
fn actions_resolve_needs_orchestrator_actions() {
    let dir = TempDir::new().unwrap();
    let approver = Arc::new(ApproverSecret::generate().unwrap());
    let daemon = serve(dir.path(), &approver);
    let mut without = connect(dir.path(), &[Capability::Health], None);
    assert!(matches!(
        without.actions_resolve("proposal-1", "approve", DIGEST, "key-1"),
        Err(IpcError::UnsupportedCapability)
    ));
    // `call` skips the client-side gate, so the server refuses the method.
    assert!(matches!(
        without.call("actions.resolve", json!({})),
        Err(IpcError::UnsupportedCapability)
    ));
    drop(without);

    // An approver, so a refusal here could only be the capability gate.
    let mut with = connect(
        dir.path(),
        &[Capability::Health, Capability::OrchestratorActions],
        Some(&approver),
    );
    let result = with
        .actions_resolve("proposal-1", "approve", DIGEST, "key-1")
        .unwrap();
    assert_eq!(result["method"], "actions.resolve");
    with.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn proposal_codes_round_trip_through_map_remote() {
    assert!(matches!(
        map_remote(RpcError::ProposalNotFound.code()),
        IpcError::ProposalNotFound
    ));
    assert!(matches!(
        map_remote(RpcError::ProposalNotPending.code()),
        IpcError::ProposalNotPending
    ));
    assert!(matches!(
        map_remote(RpcError::ProposalInvalid.code()),
        IpcError::ProposalInvalid
    ));
    assert!(matches!(
        map_remote(RpcError::ConversationBusy.code()),
        IpcError::ConversationBusy
    ));
    assert_eq!(
        serde_json::to_value(RpcError::ProposalNotPending).unwrap(),
        json!("proposal_not_pending")
    );
}
