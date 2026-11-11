//! Relay behavior through the public `Server` API. No platform-specific socket
//! code, so these also compile and run on Windows; timeouts are in seconds.

use crate::{
    Capability, Client, CloseReason, IpcEvent, Relay, RelayError, RelayEvent, RpcError, RpcHandler,
    Server, ServerConfig,
};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const WAIT: Duration = Duration::from_secs(10);

#[derive(Default)]
struct Journal {
    events: Mutex<Vec<IpcEvent>>,
}

impl Journal {
    fn push_range(&self, range: std::ops::RangeInclusive<i64>) {
        let mut events = self.events.lock().unwrap();
        for sequence in range {
            events.push(IpcEvent {
                sequence,
                task_id: "task-1".into(),
                session_id: None,
                kind: "task.transition".into(),
                payload: json!({"sequence": sequence}),
                created_at: "2026-11-09T00:00:00Z".into(),
            });
        }
    }
}

impl RpcHandler for Journal {
    fn capabilities(&self) -> Vec<Capability> {
        vec![Capability::Events]
    }

    fn dispatch(&self, _: &str, _: Value) -> Result<Value, RpcError> {
        Err(RpcError::InvalidParams)
    }

    fn events_after(&self, after: i64, limit: usize) -> Result<Vec<IpcEvent>, RpcError> {
        Ok(self
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event.sequence > after)
            .take(limit)
            .cloned()
            .collect())
    }

    fn latest_event_sequence(&self) -> Result<i64, RpcError> {
        Ok(self
            .events
            .lock()
            .unwrap()
            .last()
            .map_or(0, |event| event.sequence))
    }

    fn journal_epoch(&self) -> Result<Option<String>, RpcError> {
        Ok(Some("epoch-a".into()))
    }

    fn keep_alive(&self) -> bool {
        true
    }
}

fn serve(dir: &Path, journal: &Arc<Journal>) -> JoinHandle<()> {
    let dir = dir.to_path_buf();
    let journal = journal.clone();
    thread::spawn(move || {
        Server::serve_with_handler(
            ServerConfig::new(dir, "relay-test")
                .with_idle_timeout(Duration::from_secs(60))
                .with_stream_timeout(Duration::from_secs(1)),
            journal,
        )
        .unwrap();
    })
}

fn open(dir: &Path) -> (Relay, Value, Receiver<RelayEvent>) {
    let deadline = Instant::now() + WAIT;
    loop {
        let (sender, receiver) = mpsc::channel();
        match Relay::open(dir, &[Capability::Events], move |event| {
            let _ = sender.send(event);
        }) {
            Ok((relay, welcome)) => return (relay, welcome, receiver),
            Err(error) => {
                assert!(Instant::now() < deadline, "daemon not ready: {error}");
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

fn subscribe(relay: &Relay, after: Option<i64>, epoch: Option<&str>) {
    relay
        .send(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "events.subscribe",
            "params": {"after_sequence": after, "epoch": epoch},
        }))
        .unwrap();
}

fn next(receiver: &Receiver<RelayEvent>) -> RelayEvent {
    receiver
        .recv_timeout(WAIT)
        .expect("relay event within 10 s")
}

fn expect_subscribed(receiver: &Receiver<RelayEvent>) {
    match next(receiver) {
        RelayEvent::Message(message) => {
            assert_eq!(message["id"], 1);
            assert_eq!(message["result"]["epoch"], "epoch-a");
        }
        other => panic!("expected the subscribe response, got {other:?}"),
    }
}

fn expect_sequences(receiver: &Receiver<RelayEvent>, range: std::ops::RangeInclusive<i64>) {
    for expected in range {
        match next(receiver) {
            RelayEvent::Message(message) => {
                assert_eq!(message["method"], "events.event");
                assert_eq!(message["params"]["event"]["sequence"], expected);
            }
            other => panic!("expected event {expected}, got {other:?}"),
        }
    }
}

#[test]
fn closing_a_relay_is_not_a_daemon_shutdown_and_a_new_relay_resumes_the_stream() {
    let dir = TempDir::new().unwrap();
    let journal = Arc::new(Journal::default());
    journal.push_range(1..=3);
    let daemon = serve(dir.path(), &journal);

    let (relay, welcome, events) = open(dir.path());
    assert_eq!(welcome["version"], 1);
    assert_eq!(welcome["capabilities"], json!(["events"]));
    assert_eq!(welcome["journal_epoch"], "epoch-a");
    subscribe(&relay, None, None);
    expect_subscribed(&events);
    expect_sequences(&events, 1..=3);
    relay.close();
    assert_eq!(next(&events), RelayEvent::Closed(CloseReason::Local));

    // The daemon is still serving: a fresh client authenticates and the
    // journal moved on while nobody was connected.
    journal.push_range(4..=5);
    let (relay, _, events) = open(dir.path());
    subscribe(&relay, Some(3), Some("epoch-a"));
    expect_subscribed(&events);
    expect_sequences(&events, 4..=5);
    relay.close();

    let mut client = Client::connect_with_capabilities(dir.path(), &[Capability::Events]).unwrap();
    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn a_daemon_that_stops_ends_the_relay_with_a_remote_close() {
    let dir = TempDir::new().unwrap();
    let journal = Arc::new(Journal::default());
    let daemon = serve(dir.path(), &journal);
    let (relay, _, events) = open(dir.path());
    subscribe(&relay, None, None);
    expect_subscribed(&events);

    let mut client = Client::connect_with_capabilities(dir.path(), &[Capability::Events]).unwrap();
    client.shutdown().unwrap();
    daemon.join().unwrap();
    assert_eq!(next(&events), RelayEvent::Closed(CloseReason::Remote));
    drop(relay);
}

#[test]
fn no_daemon_is_unavailable_not_an_authentication_failure() {
    let dir = TempDir::new().unwrap();
    let error = Relay::open(dir.path(), &[Capability::Events], |_| {})
        .err()
        .expect("no daemon");
    assert_eq!(error, RelayError::Unavailable);
    assert_eq!(error.code(), "daemon-unavailable");
    let missing = dir.path().join("absent");
    let error = Relay::open(&missing, &[Capability::Events], |_| {})
        .err()
        .expect("no data dir");
    assert_eq!(error, RelayError::Unavailable);
}

#[test]
fn the_welcome_reports_only_the_capabilities_the_daemon_grants() {
    let dir = TempDir::new().unwrap();
    let journal = Arc::new(Journal::default());
    let daemon = serve(dir.path(), &journal);
    let (relay, _, _events) = open(dir.path());

    // The daemon grants the intersection; the host relays it unchanged and
    // leaves the "missing capability" decision to the UI.
    let (second, welcome) =
        Relay::open(dir.path(), &[Capability::Events, Capability::Tasks], |_| {}).unwrap();
    assert_eq!(welcome["capabilities"], json!(["events"]));
    second.close();

    relay.close();
    let mut client = Client::connect_with_capabilities(dir.path(), &[Capability::Events]).unwrap();
    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[cfg(unix)]
#[test]
fn a_wrong_token_is_an_authentication_failure() {
    let dir = TempDir::new().unwrap();
    let journal = Arc::new(Journal::default());
    let daemon = serve(dir.path(), &journal);
    let (relay, _, _events) = open(dir.path());
    relay.close();

    let token_path = dir.path().join("daemon.token");
    let original = std::fs::read_to_string(&token_path).unwrap();
    std::fs::write(&token_path, "0".repeat(64)).unwrap();
    let error = Relay::open(dir.path(), &[Capability::Events], |_| {})
        .err()
        .expect("token mismatch");
    assert_eq!(error, RelayError::Authentication);

    // The daemon rejected that connection but kept serving.
    std::fs::write(&token_path, original).unwrap();
    let (relay, _, _events) = open(dir.path());
    relay.close();
    let mut client = Client::connect_with_capabilities(dir.path(), &[Capability::Events]).unwrap();
    client.shutdown().unwrap();
    daemon.join().unwrap();
}
