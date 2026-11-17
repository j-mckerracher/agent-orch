//! The per-call context a session passes to `RpcHandler::dispatch_in`, and the
//! `orchestrator_actions` capability name (DPES-1105). No socket: each case
//! calls the trait directly.

use crate::{CallContext, Capability, IpcEvent, RpcError, RpcHandler};
use serde_json::{json, Value};

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
