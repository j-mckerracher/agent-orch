//! Process-level proof of persisted work classes (DPES-677) against the real
//! `orchestrator-daemon` binary and a fake codex CLI (a shell script, so Unix only).

#![cfg(unix)]

#[macro_use]
mod support;

use orchestrator_ipc::{Capability, Relay, RelayEvent};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;
use support::process::{connect, Daemon, Layout};
use support::seed;

const WAIT: Duration = Duration::from_secs(30);
const INVALID_PARAMS: i64 = -32602;
const UNSUPPORTED_CAPABILITY: i64 = -32004;

const MODELS: &str = r#"{"models":[
    {"slug":"gpt-6-luna","display_name":"Luna","visibility":"list",
     "supported_reasoning_levels":[{"effort":"low","description":"x"},{"effort":"medium","description":"y"}]},
    {"slug":"gpt-6-sol","display_name":"Sol","visibility":"list",
     "supported_reasoning_levels":[{"effort":"high","description":"x"}]}
]}"#;

struct Fixture {
    layout: Layout,
    program: PathBuf,
    models: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let layout = Layout::new();
        seed::init_repo(&layout.repo);
        let root = layout.data.parent().unwrap().to_owned();
        let models = root.join("models.json");
        fs::write(&models, MODELS).unwrap();
        let program = root.join("fake-codex");
        fs::write(
            &program,
            format!(
                "#!/bin/sh\ncase \"$1\" in\n\
                 login) echo 'Logged in using ChatGPT'; exit 0;;\n\
                 debug) cat '{}'; exit 0;;\n\
                 esac\n\
                 printf '{{\"type\":\"thread.started\",\"thread_id\":\"01a1b721-1f91-7393-ae1d-6aace8e4646a\"}}\\n'\n\
                 printf '{{\"type\":\"turn.started\"}}\\n'\n\
                 printf '{{\"type\":\"turn.completed\"}}\\n'\n",
                models.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            layout,
            program,
            models,
        }
    }

    fn start(&self) -> Daemon {
        Daemon::start_with_env(
            &self.layout,
            &[("AGENT_ORCH_CODEX_PROGRAM", self.program.as_os_str())],
        )
    }

    fn stop(&self, rpc: Rpc, mut daemon: Daemon) {
        rpc.relay.close();
        connect(&self.layout.data).shutdown().unwrap();
        daemon.wait_for_exit();
    }

    fn client(&self) -> Rpc {
        Rpc::open(&self.layout, &Capability::ALL)
    }

    fn project(&self, client: &mut Rpc) -> String {
        client.result(
            "projects.register",
            json!({"path": self.layout.repo.display().to_string()}),
        )["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }
}

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

    fn code(&mut self, method: &str, params: Value) -> i64 {
        let response = self.call(method, params);
        response["error"]["code"]
            .as_i64()
            .unwrap_or_else(|| panic!("{method} should fail: {response}"))
    }
}

fn submit(client: &mut Rpc, project: &str, key: &str, extra: Value) -> Value {
    let mut contract = json!({
        "project_id": project,
        "parent_id": null,
        "objective": format!("objective of {key}"),
        "kind": "Implement",
        "dependencies": [],
        "verification": "None",
        "delivery": "ReportOnly",
        "autonomy": "WorkspaceWrites",
        "max_retries": 0,
        "max_repairs": 0
    });
    for (field, value) in extra.as_object().unwrap() {
        contract[field] = value.clone();
    }
    client.call(
        "tasks.submit",
        json!({"idempotency_key": key, "contract": contract}),
    )
}

fn accepted(response: Value) -> Value {
    assert!(response.get("error").is_none(), "{response}");
    response["result"].clone()
}

fn refused(response: Value) {
    assert_eq!(response["error"]["code"], INVALID_PARAMS, "{response}");
}

fn set(client: &mut Rpc, classes: Value) -> Value {
    client.result("settings.workClasses.set", json!({"classes": classes}))
}

fn get(client: &mut Rpc) -> Value {
    client.result("settings.workClasses.get", Value::Null)
}

#[test]
fn work_classes_round_trip_across_a_restart_in_an_owner_only_file() {
    let fixture = Fixture::new();
    let daemon = fixture.start();
    let mut client = fixture.client();
    let empty = get(&mut client);
    assert_eq!(empty["state"], "off");
    assert_eq!(empty["classes"], json!([]));

    let saved = set(
        &mut client,
        json!([
            {"name": "review", "agent": "codex", "model": "gpt-6-sol", "effort": "high"},
            {"name": "debugging", "model": "gpt-6-luna"},
        ]),
    );
    assert_eq!(saved["saved"], true, "{saved}");
    let file = fixture.layout.data.join("work-classes.json");
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let names: Vec<_> = fs::read_dir(&fixture.layout.data)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("work-classes"))
        .collect();
    assert_eq!(names, ["work-classes.json"], "no temporary file is left");
    fixture.stop(client, daemon);

    let daemon = fixture.start();
    let mut client = fixture.client();
    let read = get(&mut client);
    assert_eq!(read["state"], "ok");
    assert_eq!(
        read["classes"],
        json!([
            {"name": "debugging", "agent": null, "model": "gpt-6-luna", "effort": null},
            {"name": "review", "agent": "codex", "model": "gpt-6-sol", "effort": "high"},
        ])
    );
    fixture.stop(client, daemon);
}

#[test]
fn set_refuses_an_invalid_class_naming_it_and_writes_nothing() {
    let fixture = Fixture::new();
    let daemon = fixture.start();
    let mut client = fixture.client();
    assert_eq!(
        set(&mut client, json!([{"name": "keep", "agent": "codex"}]))["saved"],
        true
    );
    let file = fixture.layout.data.join("work-classes.json");
    let before = fs::read(&file).unwrap();

    for (classes, class, field) in [
        (
            json!([{"name": "ok"}, {"name": "bad", "agent": "no-such-agent"}]),
            json!("bad"),
            "agent",
        ),
        (
            json!([{"name": "ok"}, {"name": "slow", "agent": "codex", "model": "gpt-9"}]),
            json!("slow"),
            "model",
        ),
        (
            json!([{"name": "ok"}, {"name": "deep", "agent": "codex", "model": "gpt-6-luna", "effort": "high"}]),
            json!("deep"),
            "effort",
        ),
        (
            json!([{"name": "ok"}, {"name": "x", "model": "-bad"}]),
            json!("x"),
            "model",
        ),
        (json!([{"name": "Not A Slug"}]), Value::Null, "name"),
        (json!([{"name": "a"}, {"name": "a"}]), json!("a"), "name"),
    ] {
        let result = set(&mut client, classes);
        assert_eq!(result["saved"], false, "{result}");
        assert_eq!(result["error"]["code"], "invalid_work_class");
        assert_eq!(result["error"]["class"], class, "{result}");
        assert_eq!(result["error"]["field"], field, "{result}");
        assert_eq!(fs::read(&file).unwrap(), before, "nothing was written");
    }

    let too_many: Vec<Value> = (0..17).map(|n| json!({"name": format!("c{n}")})).collect();
    let result = set(&mut client, Value::Array(too_many));
    assert_eq!(result["error"]["reason"], "too_many_classes");
    assert_eq!(fs::read(&file).unwrap(), before);
    assert_eq!(
        client.code(
            "settings.workClasses.set",
            json!({"classes": [], "extra": 1})
        ),
        INVALID_PARAMS
    );
    assert_eq!(fs::read(&file).unwrap(), before);
    fixture.stop(client, daemon);
}

#[test]
fn submit_applies_class_values_field_wise_and_refuses_what_no_longer_validates() {
    let fixture = Fixture::new();
    let daemon = fixture.start();
    let mut client = fixture.client();
    let project = fixture.project(&mut client);
    assert_eq!(
        set(
            &mut client,
            json!([
                {"name": "review", "agent": "codex", "model": "gpt-6-sol", "effort": "high"},
                {"name": "quick", "model": "gpt-6-luna", "effort": "low"},
            ])
        )["saved"],
        true
    );

    let applied = accepted(submit(
        &mut client,
        &project,
        "class-only",
        json!({"work_class": "review"}),
    ));
    assert_eq!(applied["contract"]["agent"], "codex");
    assert_eq!(
        applied["contract"]["model_selection"],
        json!({"model": "gpt-6-sol", "effort": "high"})
    );

    let overridden = accepted(submit(
        &mut client,
        &project,
        "override",
        json!({"work_class": "quick", "agent": "codex",
               "model_selection": {"model": "gpt-6-sol", "effort": "high"}}),
    ));
    assert_eq!(
        overridden["contract"]["model_selection"],
        json!({"model": "gpt-6-sol", "effort": "high"})
    );

    let mixed = accepted(submit(
        &mut client,
        &project,
        "field-wise",
        json!({"work_class": "quick", "agent": "codex",
               "model_selection": {"effort": "medium"}}),
    ));
    assert_eq!(
        mixed["contract"]["model_selection"],
        json!({"model": "gpt-6-luna", "effort": "medium"})
    );

    refused(submit(
        &mut client,
        &project,
        "unknown",
        json!({"work_class": "nope", "agent": "codex"}),
    ));
    refused(submit(
        &mut client,
        &project,
        "no-agent",
        json!({"work_class": "quick"}),
    ));

    // The class's model disappears from the live catalog: the next submit re-validates.
    fs::write(
        &fixture.models,
        r#"{"models":[{"slug":"gpt-6-luna","visibility":"list","supported_reasoning_levels":[{"effort":"low","description":"x"}]}]}"#,
    )
    .unwrap();
    fixture.stop(client, daemon);
    let daemon = fixture.start();
    let mut client = fixture.client();
    refused(submit(
        &mut client,
        &project,
        "stale",
        json!({"work_class": "review"}),
    ));
    let tasks = client.result("tasks.list", Value::Null);
    assert_eq!(
        tasks.as_array().unwrap().len(),
        3,
        "the refused submit created no task"
    );
    fixture.stop(client, daemon);
}

#[test]
fn without_a_work_class_submission_is_unchanged_even_when_the_file_is_broken() {
    let fixture = Fixture::new();
    let daemon = fixture.start();
    let mut client = fixture.client();
    let project = fixture.project(&mut client);
    let plain = accepted(submit(
        &mut client,
        &project,
        "plain",
        json!({"agent": "codex"}),
    ));
    assert_eq!(plain["contract"]["agent"], "codex");
    assert!(plain["contract"].get("model_selection").is_none());
    refused(submit(&mut client, &project, "blank", json!({})));

    fs::write(fixture.layout.data.join("work-classes.json"), "{ not json").unwrap();
    let broken = get(&mut client);
    assert_eq!(broken["state"], "invalid_configuration");
    assert_eq!(broken["classes"], json!([]));
    let still = accepted(submit(
        &mut client,
        &project,
        "still-plain",
        json!({"agent": "codex", "model_selection": {"model": "gpt-6-luna"}}),
    ));
    assert_eq!(still["contract"]["model_selection"]["model"], "gpt-6-luna");
    refused(submit(
        &mut client,
        &project,
        "named",
        json!({"agent": "codex", "work_class": "x"}),
    ));
    // A valid set repairs the file.
    assert_eq!(set(&mut client, json!([]))["saved"], true);
    assert_eq!(get(&mut client)["state"], "ok");
    fixture.stop(client, daemon);
}

#[test]
fn the_work_class_methods_require_the_agents_capability() {
    let fixture = Fixture::new();
    let daemon = fixture.start();
    let mut without = Rpc::open(&fixture.layout, &[Capability::Health]);
    for (method, params) in [
        ("settings.workClasses.get", Value::Null),
        ("settings.workClasses.set", json!({"classes": []})),
    ] {
        let response = without.call(method, params);
        assert_eq!(
            response["error"]["code"], UNSUPPORTED_CAPABILITY,
            "{method}: {response}"
        );
        assert!(response.get("result").is_none());
    }
    assert!(!fixture.layout.data.join("work-classes.json").exists());
    fixture.stop(without, daemon);
}
