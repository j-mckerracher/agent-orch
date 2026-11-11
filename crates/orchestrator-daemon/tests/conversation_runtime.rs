//! Process-level proofs of the agent-backed conversation runtime (M7-25)
//! against the real `orchestrator-daemon` binary, with the deterministic
//! `fake_agent` example standing in for the Codex CLI. Runs on Unix and
//! Windows; only the process-liveness check is Unix-only.
//!
//! The fake is launched through the same adapter, supervisor and catalog code
//! as a real agent, so these tests cover selection, pinning, the bounded turn
//! thread, persistence and replay. `live_codex_answers_a_global_message` is the
//! opt-in test against a real, signed-in CLI.

#[macro_use]
mod support;

use orchestrator_ipc::{Capability, Relay, RelayEvent};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};
use support::process::{connect, Daemon, Layout};

const WAIT: Duration = Duration::from_secs(30);
const CONFLICT: i64 = -32011;
const PIN: &str = "codex@0.0.0-fake";

/// The example binary built next to the test binary. `cargo test` builds it;
/// `cargo test --test conversation_runtime` alone does not.
fn fake_agent() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("examples");
    path.push(format!("fake_agent{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.exists(),
        "missing {}; run `cargo build -p orchestrator-daemon --examples`",
        path.display()
    );
    path
}

fn absent(layout: &Layout) -> PathBuf {
    layout.data.join("no-such-agent")
}

/// Environment that selects the fake as the Codex agent and hides Claude Code.
fn fake_env(layout: &Layout, extra: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut env = vec![
        ("AGENT_ORCH_CODEX_PROGRAM".to_owned(), text(&fake_agent())),
        (
            "AGENT_ORCH_CLAUDE_PROGRAM".to_owned(),
            text(&absent(layout)),
        ),
        ("AGENT_ORCH_CONVERSATION_PINS".to_owned(), PIN.to_owned()),
    ];
    for (name, value) in extra {
        env.retain(|(existing, _)| existing != name);
        env.push(((*name).to_owned(), (*value).to_owned()));
    }
    env
}

fn text(path: &Path) -> String {
    path.to_str().expect("utf-8 path").to_owned()
}

fn start(layout: &Layout, extra: &[(&str, &str)]) -> Daemon {
    let env = fake_env(layout, extra);
    let pairs: Vec<(&str, &str)> = env
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    Daemon::start_with_env(layout, &pairs)
}

struct Chat {
    relay: Relay,
    frames: Receiver<RelayEvent>,
    next_id: u64,
}

impl Chat {
    fn open(layout: &Layout) -> Self {
        let (events, frames) = mpsc::channel();
        let capabilities = [
            Capability::Coordinator,
            Capability::Tasks,
            Capability::Approvals,
            Capability::Workstreams,
        ];
        let (relay, _welcome) = Relay::open(&layout.data, &capabilities, move |event| {
            let _ = events.send(event);
        })
        .unwrap();
        Self {
            relay,
            frames,
            next_id: 0,
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
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

    /// Sends a message and returns the `result`. A conflict means the runtime
    /// was still being selected (the first catalog read can outlast one call),
    /// so the same idempotency key is retried; nothing was recorded.
    fn send(&mut self, target: &Value, message: &str, key: &str) -> Value {
        for _ in 0..50 {
            let response = self.call(
                "conversations.send",
                json!({"target": target, "text": message, "idempotency_key": key}),
            );
            if response["error"]["code"] == CONFLICT {
                thread::sleep(Duration::from_millis(100));
                continue;
            }
            assert!(response.get("error").is_none(), "send failed: {response}");
            return response["result"].clone();
        }
        panic!("runtime selection kept reporting a conflict");
    }

    fn get(&mut self, target: &Value, after: i64) -> Value {
        let response = self.call(
            "conversations.get",
            json!({"target": target, "after_sequence": after}),
        );
        assert!(response.get("error").is_none(), "get failed: {response}");
        response["result"].clone()
    }

    /// The transcript once the newest turn has stopped running.
    fn settled(&mut self, target: &Value) -> Value {
        let deadline = Instant::now() + WAIT;
        loop {
            let page = self.get(target, 0);
            if page["turn"]["state"] != "running" {
                return page;
            }
            assert!(Instant::now() < deadline, "turn still running: {page}");
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn assert_no_authority(&mut self) {
        let tasks = self.call("tasks.list", Value::Null);
        assert_eq!(tasks["result"], json!([]), "{tasks}");
        let approvals = self.call("approvals.list", Value::Null);
        assert_eq!(approvals["result"], json!([]), "{approvals}");
    }
}

fn global() -> Value {
    json!({"kind": "global"})
}

fn kinds(page: &Value) -> Vec<String> {
    page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["kind"].as_str().unwrap().to_owned())
        .collect()
}

/// The reply of the newest turn on the page.
fn reply(page: &Value) -> String {
    let mut text = String::new();
    for entry in page["entries"].as_array().unwrap() {
        match entry["kind"].as_str().unwrap() {
            "user_message" => text.clear(),
            "reply_chunk" => text.push_str(entry["body"].as_str().unwrap()),
            _ => {}
        }
    }
    text
}

fn stop(layout: &Layout, mut daemon: Daemon, chat: Chat) {
    chat.relay.close();
    connect(&layout.data).shutdown().unwrap();
    daemon.wait_for_exit();
}

#[test]
fn replies_are_ordered_correlated_and_carry_history_and_workstream_context() {
    let layout = Layout::new();
    let daemon = start(&layout, &[]);
    let mut chat = Chat::open(&layout);
    let first = chat.send(&global(), "hello there", "m-1");
    assert_eq!(first["authority"], "none");
    assert_eq!(first["turn"]["runtime"]["name"], "codex");
    assert_eq!(first["turn"]["runtime"]["version"], "0.0.0-fake");
    let page = chat.settled(&global());
    assert_eq!(page["turn"]["state"], "completed", "{page}");
    assert_eq!(reply(&page), "fake reply to: hello there");
    let kinds_first = kinds(&page);
    assert_eq!(kinds_first.first().unwrap(), "user_message");
    assert_eq!(kinds_first.last().unwrap(), "turn_completed");
    let turn_id = first["turn"]["id"].as_str().unwrap();
    assert!(page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["turn_id"] == turn_id));

    chat.send(&global(), "[fake:echo]", "m-2");
    let page = chat.settled(&global());
    assert_eq!(page["turn"]["state"], "completed", "{page}");
    let sequences: Vec<i64> = page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["sequence"].as_i64().unwrap())
        .collect();
    assert!(
        sequences.windows(2).all(|pair| pair[0] < pair[1]),
        "{sequences:?}"
    );
    let echoed = reply(&page);
    assert!(
        echoed.contains("Scope: the global orchestrator."),
        "{echoed}"
    );
    assert!(
        echoed.contains("Operator: hello there\nAssistant: fake reply to: hello there"),
        "history reaches the next turn: {echoed}"
    );

    let created = chat.call(
        "workstreams.create",
        json!({"title": "Auth rewrite", "summary": "Replace session cookies", "idempotency_key": "w-1"}),
    );
    let workstream = json!({"kind": "workstream", "workstream_id": created["result"]["id"]});
    chat.send(&workstream, "[fake:echo]", "w-m-1");
    let page = chat.settled(&workstream);
    assert_eq!(page["turn"]["state"], "completed", "{page}");
    let echoed = reply(&page);
    assert!(echoed.contains("Scope: one workstream."), "{echoed}");
    assert!(
        echoed.contains("Workstream context: Auth rewrite"),
        "{echoed}"
    );
    assert!(
        !echoed.contains("hello there"),
        "scopes do not share history"
    );

    chat.assert_no_authority();
    stop(&layout, daemon, chat);
}

#[test]
fn replies_survive_a_restart_replay_by_sequence_and_stay_idempotent() {
    let layout = Layout::new();
    let first = start(&layout, &[]);
    let mut chat = Chat::open(&layout);
    let sent = chat.send(&global(), "remember me", "m-1");
    let before = chat.settled(&global());
    assert_eq!(before["turn"]["state"], "completed", "{before}");
    stop(&layout, first, chat);

    let second = start(&layout, &[]);
    let mut chat = Chat::open(&layout);
    let after = chat.get(&global(), 0);
    assert_eq!(
        after["entries"], before["entries"],
        "same transcript after restart"
    );
    assert_eq!(reply(&after), "fake reply to: remember me");

    let cursor = after["entries"][0]["sequence"].as_i64().unwrap();
    let tail = chat.get(&global(), cursor);
    assert_eq!(
        tail["entries"].as_array().unwrap().len(),
        after["entries"].as_array().unwrap().len() - 1
    );
    assert_eq!(tail["entries"][0]["sequence"].as_i64().unwrap(), cursor + 1);
    let end = chat.get(&global(), after["latest_sequence"].as_i64().unwrap());
    assert_eq!(end["entries"], json!([]));

    let replayed = chat.send(&global(), "remember me", "m-1");
    assert_eq!(replayed, sent, "idempotent across restart");
    assert_eq!(chat.get(&global(), 0)["entries"], before["entries"]);
    stop(&layout, second, chat);
}

#[test]
fn a_turn_the_daemon_lost_is_failed_on_restart_not_left_running() {
    let layout = Layout::new();
    let mut first = start(&layout, &[("AGENT_ORCH_CONVERSATION_TIMEOUT_MS", "600000")]);
    let mut chat = Chat::open(&layout);
    chat.send(&global(), "[fake:hang]", "m-1");
    assert_eq!(chat.get(&global(), 0)["turn"]["state"], "running");
    // Kill, not shutdown: the daemon gets no chance to clean up.
    drop(chat);
    first.kill();

    let second = start(&layout, &[]);
    let mut chat = Chat::open(&layout);
    let page = chat.get(&global(), 0);
    assert_eq!(page["turn"]["state"], "failed", "{page}");
    assert_eq!(page["turn"]["error_code"], "interrupted_by_restart");
    stop(&layout, second, chat);
}

#[test]
fn cancel_stops_a_streaming_reply_and_later_output_is_discarded() {
    let layout = Layout::new();
    let daemon = start(&layout, &[]);
    let mut chat = Chat::open(&layout);
    let sent = chat.send(&global(), "[fake:slow]", "m-1");
    let turn_id = sent["turn"]["id"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + WAIT;
    while !kinds(&chat.get(&global(), 0)).contains(&"reply_chunk".to_owned()) {
        assert!(Instant::now() < deadline, "no reply chunk arrived");
        thread::sleep(Duration::from_millis(20));
    }
    let cancelled = chat.call(
        "conversations.cancel",
        json!({"target": global(), "turn_id": turn_id, "idempotency_key": "c-1"}),
    );
    assert_eq!(
        cancelled["result"]["turn"]["state"], "cancelled",
        "{cancelled}"
    );
    let frozen = chat.get(&global(), 0);
    thread::sleep(Duration::from_millis(1500));
    assert_eq!(chat.get(&global(), 0)["entries"], frozen["entries"]);
    assert_eq!(kinds(&frozen).last().unwrap(), "turn_cancelled");
    stop(&layout, daemon, chat);
}

#[test]
fn a_hung_agent_is_killed_at_the_deadline() {
    let layout = Layout::new();
    let pid_file = layout.data.join("agent.pid");
    let daemon = start(
        &layout,
        &[
            ("AGENT_ORCH_CONVERSATION_TIMEOUT_MS", "4000"),
            ("AGENT_ORCH_FAKE_PID_FILE", pid_file.to_str().unwrap()),
        ],
    );
    let mut chat = Chat::open(&layout);
    chat.send(&global(), "[fake:hang]", "m-1");
    let page = chat.settled(&global());
    assert_eq!(page["turn"]["state"], "failed", "{page}");
    assert_eq!(page["turn"]["error_code"], "runtime_timeout");
    #[cfg(unix)]
    {
        let pid = std::fs::read_to_string(&pid_file).expect("the agent started");
        let deadline = Instant::now() + Duration::from_secs(10);
        while process_alive(pid.trim()) {
            assert!(
                Instant::now() < deadline,
                "agent {pid} survived the deadline"
            );
            thread::sleep(Duration::from_millis(50));
        }
    }
    stop(&layout, daemon, chat);
}

#[cfg(unix)]
fn process_alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[test]
fn agent_failures_and_oversized_replies_fail_the_turn_with_stable_codes() {
    let layout = Layout::new();
    let daemon = start(&layout, &[]);
    let mut chat = Chat::open(&layout);
    chat.send(&global(), "[fake:fail]", "m-1");
    let page = chat.settled(&global());
    assert_eq!(page["turn"]["error_code"], "runtime_failed", "{page}");

    chat.send(&global(), "[fake:flood]", "m-2");
    let page = chat.settled(&global());
    assert_eq!(page["turn"]["state"], "failed", "{page}");
    assert_eq!(page["turn"]["error_code"], "reply_too_large");
    assert!(page["turn"]["reply_bytes"].as_i64().unwrap() <= 256 * 1024);

    chat.send(&global(), "still alive?", "m-3");
    let page = chat.settled(&global());
    assert_eq!(page["turn"]["state"], "completed", "{page}");
    chat.assert_no_authority();
    stop(&layout, daemon, chat);
}

#[test]
fn control_characters_never_reach_the_transcript() {
    let layout = Layout::new();
    let daemon = start(&layout, &[]);
    let mut chat = Chat::open(&layout);
    chat.send(&global(), "[fake:control]", "m-1");
    let page = chat.settled(&global());
    assert_eq!(page["turn"]["state"], "completed", "{page}");
    assert_eq!(reply(&page), "a[31mredb\ncd");

    // A message that carries control characters is cleaned before the agent
    // sees it; the stored operator message is untouched.
    chat.send(&global(), "up\u{1b}[2Jset", "m-2");
    let page = chat.settled(&global());
    assert_eq!(
        reply(&page).lines().last().unwrap(),
        "fake reply to: up[2Jset"
    );
    stop(&layout, daemon, chat);
}

#[test]
fn the_agent_runs_read_only_in_an_empty_scratch_directory() {
    let layout = Layout::new();
    let log = layout.data.join("agent.log");
    let daemon = start(&layout, &[("AGENT_ORCH_FAKE_LOG", log.to_str().unwrap())]);
    let mut chat = Chat::open(&layout);
    chat.send(&global(), "hi", "m-1");
    assert_eq!(chat.settled(&global())["turn"]["state"], "completed");
    let launched = std::fs::read_to_string(&log).unwrap();
    assert!(launched.contains("conversation-scratch"), "{launched}");
    assert!(launched.contains("\"read-only\""), "{launched}");
    assert!(launched.contains("\"--ephemeral\""), "{launched}");
    stop(&layout, daemon, chat);
}

#[test]
fn a_version_that_is_not_pinned_never_receives_the_message() {
    let layout = Layout::new();
    let log = layout.data.join("agent.log");
    let daemon = start(
        &layout,
        &[
            ("AGENT_ORCH_FAKE_VERSION", "9.9.9"),
            ("AGENT_ORCH_FAKE_LOG", log.to_str().unwrap()),
        ],
    );
    let mut chat = Chat::open(&layout);
    let sent = chat.send(&global(), "hi", "m-1");
    assert_eq!(sent["turn"]["state"], "failed");
    assert_eq!(sent["turn"]["error_code"], "version_mismatch");
    assert_eq!(sent["turn"]["runtime"]["version"], "9.9.9");
    assert!(!log.exists(), "the agent was launched");
    stop(&layout, daemon, chat);
}

#[test]
fn a_configured_model_the_catalog_does_not_offer_fails_closed() {
    let layout = Layout::new();
    let log = layout.data.join("agent.log");
    let daemon = start(
        &layout,
        &[
            ("AGENT_ORCH_CONVERSATION_MODEL", "no-such-model"),
            ("AGENT_ORCH_FAKE_LOG", log.to_str().unwrap()),
        ],
    );
    let mut chat = Chat::open(&layout);
    let sent = chat.send(&global(), "hi", "m-1");
    assert_eq!(sent["turn"]["error_code"], "runtime_unavailable", "{sent}");
    assert!(!log.exists(), "the agent was launched");

    stop(&layout, daemon, chat);
    let layout = Layout::new();
    let daemon = start(
        &layout,
        &[
            ("AGENT_ORCH_CONVERSATION_MODEL", "fake-model"),
            ("AGENT_ORCH_CONVERSATION_EFFORT", "high"),
            ("AGENT_ORCH_FAKE_LOG", log.to_str().unwrap()),
        ],
    );
    let mut chat = Chat::open(&layout);
    chat.send(&global(), "hi", "m-2");
    assert_eq!(chat.settled(&global())["turn"]["state"], "completed");
    let launched = std::fs::read_to_string(&log).unwrap();
    assert!(launched.contains("fake-model"), "{launched}");
    stop(&layout, daemon, chat);
}

/// Opt-in: talks to the real Codex CLI on PATH, which must be signed in, and
/// spends a few tokens. Run with
/// `AGENT_ORCH_LIVE_CONVERSATION=1 cargo test -p orchestrator-daemon --test conversation_runtime -- --ignored live`.
/// When the installed version is not pinned, set `AGENT_ORCH_CONVERSATION_PINS`.
#[test]
#[ignore = "needs a signed-in codex CLI; set AGENT_ORCH_LIVE_CONVERSATION=1"]
fn live_codex_answers_a_global_message() {
    if std::env::var_os("AGENT_ORCH_LIVE_CONVERSATION").is_none() {
        eprintln!("skipped: AGENT_ORCH_LIVE_CONVERSATION is not set");
        return;
    }
    let layout = Layout::new();
    let daemon = Daemon::start_with_env(
        &layout,
        &[
            ("AGENT_ORCH_CONVERSATION_AGENT", "codex"),
            ("AGENT_ORCH_CONVERSATION_TIMEOUT_MS", "120000"),
        ],
    );
    let mut chat = Chat::open(&layout);
    let sent = chat.send(&global(), "Reply with exactly: pong", "live-1");
    let page = chat.settled(&global());
    assert_eq!(
        page["turn"]["state"], "completed",
        "turn {sent}; if this is version_mismatch, set AGENT_ORCH_CONVERSATION_PINS: {page}"
    );
    assert!(reply(&page).to_lowercase().contains("pong"), "{page}");
    chat.assert_no_authority();
    stop(&layout, daemon, chat);
}
