//! Process-level proof of work-item intake (DPES-673) against the real
//! `orchestrator-daemon` binary: `intake.normalize` is capability-gated, pure,
//! and bounded.

#![cfg(unix)]

#[macro_use]
mod support;

use orchestrator_ipc::{Capability, Relay, RelayEvent};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;
use support::process::{connect, Daemon, Layout};
use support::seed;

const WAIT: Duration = Duration::from_secs(30);
const INVALID_PARAMS: i64 = -32602;
const UNSUPPORTED_CAPABILITY: i64 = -32004;

/// One session over the real transport, so errors keep their JSON-RPC code.
struct Rpc {
    relay: Relay,
    frames: Receiver<RelayEvent>,
    next: u64,
}

impl Rpc {
    fn open(layout: &Layout, capabilities: &[Capability]) -> Self {
        let (events, frames) = mpsc::channel();
        let (relay, _welcome) = Relay::open(&layout.data, capabilities, move |event| {
            let _ = events.send(event);
        })
        .unwrap();
        Self {
            relay,
            frames,
            next: 0,
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        self.relay
            .send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .unwrap();
        loop {
            match self
                .frames
                .recv_timeout(WAIT)
                .expect("response within 30 s")
            {
                RelayEvent::Message(message) if message["id"] == id => return message,
                RelayEvent::Message(_) => {}
                RelayEvent::Closed(reason) => panic!("relay closed: {reason:?}"),
            }
        }
    }

    fn result(&mut self, method: &str, params: Value) -> Value {
        let response = self.call(method, params);
        assert!(response.get("error").is_none(), "{method}: {response}");
        response["result"].clone()
    }
}

fn stop(layout: &Layout, rpc: Rpc, mut daemon: Daemon) {
    rpc.relay.close();
    connect(&layout.data).shutdown().unwrap();
    daemon.wait_for_exit();
}

fn data_files(data: &Path) -> BTreeSet<String> {
    fs::read_dir(data)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn normalize_returns_a_draft_and_writes_nothing() {
    let layout = Layout::new();
    let daemon = Daemon::start(&layout);
    let mut client = Rpc::open(&layout, &Capability::ALL);
    let files_before = data_files(&layout.data);
    let events_before = client.result("tasks.list", Value::Null);
    assert_eq!(events_before, json!([]));

    let draft = client.result(
        "intake.normalize",
        json!({"input": "DPES-673 Add intake\nPaste an issue.\n\nAcceptance criteria\n1. Source kept\n2) Text bounded"}),
    );
    assert_eq!(
        draft,
        json!({"work_item": {
            "source": {"kind": "jira", "reference": "DPES-673"},
            "title": "DPES-673 Add intake",
            "ask": "Paste an issue.",
            "acceptance_criteria": [
                {"text": "Source kept", "drafted": false, "confirmed": false},
                {"text": "Text bounded", "drafted": false, "confirmed": false},
            ],
        }})
    );
    let bare = client.result("intake.normalize", json!({"input": "acme/widgets#7"}));
    assert_eq!(bare["work_item"]["acceptance_criteria"][0]["drafted"], true);
    assert_eq!(
        bare["work_item"]["acceptance_criteria"][0]["confirmed"],
        false
    );

    assert_eq!(client.result("tasks.list", Value::Null), json!([]));
    assert_eq!(
        data_files(&layout.data),
        files_before,
        "no file was written"
    );
    stop(&layout, client, daemon);
}

#[test]
fn normalize_refuses_over_limit_input_over_the_wire() {
    let layout = Layout::new();
    let daemon = Daemon::start(&layout);
    let mut client = Rpc::open(&layout, &Capability::ALL);
    for params in [
        json!({"input": "a".repeat(20_001)}),
        json!({"input": ""}),
        json!({"input": "x", "unknown": 1}),
        json!({}),
    ] {
        let response = client.call("intake.normalize", params);
        assert_eq!(response["error"]["code"], INVALID_PARAMS, "{response}");
    }
    stop(&layout, client, daemon);
}

#[test]
fn normalize_requires_the_tasks_capability() {
    let layout = Layout::new();
    let daemon = Daemon::start(&layout);
    let mut without = Rpc::open(&layout, &[Capability::Health, Capability::Projects]);
    let response = without.call("intake.normalize", json!({"input": "DPES-673"}));
    assert_eq!(
        response["error"]["code"], UNSUPPORTED_CAPABILITY,
        "{response}"
    );
    assert!(response.get("result").is_none());
    stop(&layout, without, daemon);
}

/// A fake codex (a shell script) so an admitted task never reaches a real agent.
fn start_with_fake_agent(layout: &Layout) -> Daemon {
    let program = layout.data.parent().unwrap().join("fake-codex");
    if !program.exists() {
        fs::write(
            &program,
            "#!/bin/sh\n\
             printf '{\"type\":\"thread.started\",\"thread_id\":\"01a1b721-1f91-7393-ae1d-6aace8e4646a\"}\\n'\n\
             printf '{\"type\":\"turn.started\"}\\n'\n\
             printf '{\"type\":\"turn.completed\"}\\n'\n",
        )
        .unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    }
    Daemon::start_with_env(layout, &[("AGENT_ORCH_CODEX_PROGRAM", program.as_os_str())])
}

fn submit(client: &mut Rpc, project: &str, key: &str, work_item: Option<Value>) -> Value {
    let mut contract = json!({
        "project_id": project,
        "parent_id": null,
        "objective": format!("objective of {key}"),
        "kind": "Implement",
        "agent": "codex",
        "dependencies": [],
        "verification": "None",
        "delivery": "ReportOnly",
        "autonomy": "WorkspaceWrites",
        "max_retries": 0,
        "max_repairs": 0
    });
    if let Some(item) = work_item {
        contract["work_item"] = item;
    }
    client.call(
        "tasks.submit",
        json!({"idempotency_key": key, "contract": contract}),
    )
}

#[test]
fn a_confirmed_work_item_survives_a_daemon_restart_and_an_unconfirmed_one_is_refused() {
    let layout = Layout::new();
    seed::init_repo(&layout.repo);
    let daemon = start_with_fake_agent(&layout);
    let mut client = Rpc::open(&layout, &Capability::ALL);
    let project = client.result(
        "projects.register",
        json!({"path": layout.repo.display().to_string()}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // The draft a client shows, then the same draft after the user confirms it.
    let mut item = client.result(
        "intake.normalize",
        json!({"input": "acme/widgets#7 Add intake\nPaste an issue.\n1. Source kept\n2. Text bounded"}),
    )["work_item"]
        .clone();
    let refused = submit(&mut client, &project, "unconfirmed", Some(item.clone()));
    assert_eq!(refused["error"]["code"], INVALID_PARAMS, "{refused}");
    assert_eq!(client.result("tasks.list", Value::Null), json!([]));

    for criterion in item["acceptance_criteria"].as_array_mut().unwrap() {
        criterion["confirmed"] = json!(true);
    }
    let submitted = submit(&mut client, &project, "confirmed", Some(item.clone()));
    assert!(submitted.get("error").is_none(), "{submitted}");
    let id = submitted["result"]["contract"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(submitted["result"]["contract"]["work_item"], item);
    let plain = submit(&mut client, &project, "plain", None);
    assert!(plain.get("error").is_none(), "{plain}");
    assert!(plain["result"]["contract"].get("work_item").is_none());
    stop(&layout, client, daemon);

    let daemon = start_with_fake_agent(&layout);
    let mut client = Rpc::open(&layout, &Capability::ALL);
    let got = client.result("tasks.get", json!({"task_id": id}));
    assert_eq!(got["contract"]["work_item"], item);
    let listed = client.result("tasks.list", Value::Null);
    let listed = listed.as_array().unwrap();
    assert_eq!(listed.len(), 2);
    let with: Vec<_> = listed
        .iter()
        .filter(|task| task["contract"].get("work_item").is_some())
        .collect();
    assert_eq!(with.len(), 1);
    assert_eq!(with[0]["contract"]["work_item"], item);
    stop(&layout, client, daemon);
}
