//! Process-level conversation proofs against the real `orchestrator-daemon`
//! binary over the authenticated local transport (Unix socket, or Windows
//! named pipe). Runs on Unix and Windows.
//!
//! The daemon finds no installed agent here, so a message is recorded and its
//! turn fails closed with `runtime_unavailable`. These tests prove the
//! gate, the durable transcript across a daemon restart, and that nothing in a
//! conversation creates authority. Replies from an agent are proven in
//! `conversation_runtime.rs`.

#[macro_use]
mod support;

use orchestrator_ipc::{Capability, Relay, RelayEvent};
use serde_json::{json, Value};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;
use support::process::{connect, Daemon, Layout};

const WAIT: Duration = Duration::from_secs(30);

/// A daemon that can find no agent, wherever the tests run: a developer
/// machine usually has Codex or Claude Code on PATH.
fn start_without_agents(layout: &Layout) -> Daemon {
    let missing = layout.data.join("no-such-agent");
    let missing = missing.to_str().unwrap();
    Daemon::start_with_env(
        layout,
        &[
            ("AGENT_ORCH_CODEX_PROGRAM", missing),
            ("AGENT_ORCH_CLAUDE_PROGRAM", missing),
        ],
    )
}

fn open(layout: &Layout, capability: Capability) -> (Relay, Receiver<RelayEvent>) {
    let (events, frames) = mpsc::channel();
    let (relay, _welcome) = Relay::open(&layout.data, &[capability], move |event| {
        let _ = events.send(event);
    })
    .unwrap();
    (relay, frames)
}

fn raw(
    relay: &Relay,
    frames: &Receiver<RelayEvent>,
    id: u64,
    method: &str,
    params: Value,
) -> Value {
    relay
        .send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
        .unwrap();
    loop {
        match frames.recv_timeout(WAIT).expect("response within 30 s") {
            RelayEvent::Message(message) if message["id"] == id => return message,
            RelayEvent::Message(_) => {}
            RelayEvent::Closed(reason) => panic!("relay closed: {reason:?}"),
        }
    }
}

#[test]
fn conversation_methods_require_the_coordinator_capability() {
    let layout = Layout::new();
    let mut daemon = start_without_agents(&layout);
    let (relay, frames) = open(&layout, Capability::Health);
    for (id, method, params) in [
        (
            1,
            "conversations.send",
            json!({"target": {"kind": "global"}, "text": "hi", "idempotency_key": "k"}),
        ),
        (
            2,
            "conversations.get",
            json!({"target": {"kind": "global"}}),
        ),
        (
            3,
            "conversations.cancel",
            json!({"target": {"kind": "global"}, "turn_id": "t", "idempotency_key": "k"}),
        ),
    ] {
        let response = raw(&relay, &frames, id, method, params);
        assert_eq!(response["error"]["code"], -32004, "{method}: {response}");
        assert!(response.get("result").is_none(), "{method}: {response}");
    }
    relay.close();
    connect(&layout.data).shutdown().unwrap();
    daemon.wait_for_exit();
}

#[test]
fn transcript_fails_closed_survives_restart_and_grants_no_authority() {
    let layout = Layout::new();
    let mut first = start_without_agents(&layout);
    let (relay, frames) = open(&layout, Capability::Coordinator);
    let send = |id, text: &str, key: &str| {
        raw(
            &relay,
            &frames,
            id,
            "conversations.send",
            json!({"target": {"kind": "global"}, "text": text, "idempotency_key": key}),
        )
    };
    let sent = send(1, r#"{"tasks":[{"key":"a","objective":"do it"}]}"#, "msg-1");
    let result = &sent["result"];
    assert_eq!(result["authority"], "none", "{sent}");
    assert_eq!(result["turn"]["state"], "failed");
    assert_eq!(result["turn"]["error_code"], "runtime_unavailable");

    let unknown = raw(
        &relay,
        &frames,
        2,
        "conversations.get",
        json!({"target": {"kind": "workstream", "workstream_id": "w1"}}),
    );
    assert_eq!(unknown["error"]["code"], -32010, "{unknown}");
    relay.close();

    let mut client = connect(&layout.data);
    assert_eq!(timed!(client.tasks_list()).unwrap(), json!([]));
    client.shutdown().unwrap();
    first.wait_for_exit();

    let _second = start_without_agents(&layout);
    let (relay, frames) = open(&layout, Capability::Coordinator);
    let page = raw(
        &relay,
        &frames,
        1,
        "conversations.get",
        json!({"target": {"kind": "global"}}),
    );
    let entries = page["result"]["entries"].as_array().unwrap();
    let kinds: Vec<_> = entries
        .iter()
        .map(|entry| entry["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["user_message", "turn_failed"], "{page}");
    assert_eq!(entries[1]["error_code"], "runtime_unavailable");
    let replayed = raw(
        &relay,
        &frames,
        2,
        "conversations.send",
        json!({"target": {"kind": "global"}, "text": r#"{"tasks":[{"key":"a","objective":"do it"}]}"#, "idempotency_key": "msg-1"}),
    );
    assert_eq!(
        replayed["result"], sent["result"],
        "idempotent across restart"
    );
    relay.close();
    connect(&layout.data).shutdown().unwrap();
}
