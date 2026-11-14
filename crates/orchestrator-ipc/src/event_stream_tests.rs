//! Reconnect and replay behavior through the public `Client` and `Server` API.
//! These tests use no platform-specific socket code, so they also compile and
//! run on Windows.

use crate::{Capability, Client, IpcError, IpcEvent, RpcError, RpcHandler, Server, ServerConfig};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tempfile::TempDir;

#[derive(Default)]
struct Journal {
    events: Mutex<Vec<IpcEvent>>,
    epoch: Mutex<Option<String>>,
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

    fn reset(&self) {
        self.events.lock().unwrap().clear();
    }

    /// Models a database file that was swapped for another one.
    fn replace(&self, epoch: &str) {
        self.reset();
        *self.epoch.lock().unwrap() = Some(epoch.to_owned());
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
        Ok(self.epoch.lock().unwrap().clone())
    }

    fn keep_alive(&self) -> bool {
        true
    }
}

/// Also the polling period of a subscribed connection, so it bounds how long a
/// live event waits. An unsubscribed connection is closed after this much
/// silence, which must outlast connect-to-first-call gaps on a loaded Windows
/// host (a 50 ms limit made every test here fail with a 150 ms gap).
const STREAM_TIMEOUT: Duration = Duration::from_secs(1);

fn serve(dir: &Path, journal: &Arc<Journal>) -> JoinHandle<()> {
    let dir = dir.to_path_buf();
    let journal = journal.clone();
    thread::spawn(move || {
        Server::serve_with_handler(
            ServerConfig::new(dir, "reconnect-test")
                .with_idle_timeout(Duration::from_secs(30))
                .with_stream_timeout(STREAM_TIMEOUT),
            journal,
        )
        .unwrap();
    })
}

fn connect(dir: &Path) -> Client {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match Client::connect_with_capabilities(dir, &[Capability::Events]) {
            Ok(client) => return client,
            Err(error) => {
                assert!(Instant::now() < deadline, "daemon not ready: {error}");
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

fn read_through(client: &mut Client, last: i64) {
    let start = client.last_event_sequence().unwrap_or(0);
    for expected in start + 1..=last {
        assert_eq!(client.next_event().unwrap().sequence, expected);
    }
    assert_eq!(client.last_event_sequence(), Some(last));
}

#[test]
fn replay_larger_than_the_live_buffer_is_paged_across_reconnect() {
    let dir = TempDir::new().unwrap();
    let journal = Arc::new(Journal::default());
    journal.push_range(1..=700);
    let daemon = serve(dir.path(), &journal);

    let mut client = connect(dir.path());
    client.events_subscribe(None).unwrap();
    read_through(&mut client, 300);
    client.reconnect().unwrap();
    read_through(&mut client, 700);
    journal.push_range(701..=701);
    read_through(&mut client, 701);
    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn cursor_ahead_of_the_journal_is_a_typed_replay_gap() {
    let dir = TempDir::new().unwrap();
    let journal = Arc::new(Journal::default());
    journal.push_range(1..=5);
    let daemon = serve(dir.path(), &journal);

    let mut client = connect(dir.path());
    assert!(matches!(
        client.events_subscribe(Some(9)),
        Err(IpcError::ReplayGap)
    ));
    assert!(matches!(
        client.events_subscribe(Some(-1)),
        Err(IpcError::InvalidParams)
    ));
    client.events_subscribe(None).unwrap();
    read_through(&mut client, 5);

    journal.reset();
    journal.push_range(1..=2);
    assert!(matches!(client.reconnect(), Err(IpcError::ReplayGap)));
    client.events_subscribe(None).unwrap();
    read_through(&mut client, 2);
    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn failed_resume_keeps_the_cursor_until_a_resume_succeeds() {
    let dir = TempDir::new().unwrap();
    let journal = Arc::new(Journal::default());
    journal.push_range(1..=5);
    let daemon = serve(dir.path(), &journal);

    let mut client = connect(dir.path());
    client.events_subscribe(None).unwrap();
    read_through(&mut client, 5);

    journal.reset();
    journal.push_range(1..=2);
    for _ in 0..2 {
        assert!(matches!(client.reconnect(), Err(IpcError::ReplayGap)));
        assert_eq!(client.last_event_sequence(), Some(5));
        assert!(matches!(client.next_event(), Err(IpcError::NoSubscription)));
    }

    journal.push_range(3..=6);
    client.reconnect().unwrap();
    read_through(&mut client, 6);
    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn replaced_journal_with_a_longer_history_is_an_epoch_mismatch_not_a_silent_resume() {
    let dir = TempDir::new().unwrap();
    let journal = Arc::new(Journal::default());
    journal.replace("epoch-a");
    journal.push_range(1..=5);
    let daemon = serve(dir.path(), &journal);

    let mut client = connect(dir.path());
    assert_eq!(client.journal_epoch(), Some("epoch-a"));
    client.events_subscribe(None).unwrap();
    assert_eq!(client.cursor_epoch(), Some("epoch-a"));
    read_through(&mut client, 5);

    // The replacement is longer than the cursor, so only the epoch can tell
    // that events 6..=9 belong to a different journal.
    journal.replace("epoch-b");
    journal.push_range(1..=9);
    for _ in 0..2 {
        assert!(matches!(client.reconnect(), Err(IpcError::EpochMismatch)));
        assert_eq!(client.last_event_sequence(), Some(5));
        assert_eq!(client.cursor_epoch(), Some("epoch-a"));
        assert_eq!(client.journal_epoch(), Some("epoch-b"));
        assert!(matches!(client.next_event(), Err(IpcError::NoSubscription)));
    }

    client.events_subscribe(None).unwrap();
    assert_eq!(client.cursor_epoch(), Some("epoch-b"));
    read_through(&mut client, 9);
    client.reconnect().unwrap();
    journal.push_range(10..=10);
    read_through(&mut client, 10);
    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn resume_in_the_same_epoch_still_works_and_a_wrong_epoch_is_refused_at_subscribe() {
    let dir = TempDir::new().unwrap();
    let journal = Arc::new(Journal::default());
    journal.replace("epoch-a");
    journal.push_range(1..=3);
    let daemon = serve(dir.path(), &journal);

    let mut client = connect(dir.path());
    client.events_subscribe(None).unwrap();
    read_through(&mut client, 3);
    journal.push_range(4..=5);
    client.reconnect().unwrap();
    read_through(&mut client, 5);

    let mut other = connect(dir.path());
    assert!(matches!(
        other.events_subscribe_in_epoch(Some(2), Some("epoch-z")),
        Err(IpcError::EpochMismatch)
    ));
    assert_eq!(other.last_event_sequence(), None);
    other
        .events_subscribe_in_epoch(Some(2), Some("epoch-a"))
        .unwrap();
    read_through(&mut other, 5);
    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn call_during_a_large_replay_keeps_events_and_the_connection_in_sync() {
    let dir = TempDir::new().unwrap();
    let journal = Arc::new(Journal::default());
    journal.push_range(1..=700);
    let daemon = serve(dir.path(), &journal);

    let mut client = connect(dir.path());
    client.events_subscribe(None).unwrap();
    assert_eq!(client.capabilities().unwrap(), vec![Capability::Events]);
    assert_eq!(client.capabilities().unwrap(), vec![Capability::Events]);
    read_through(&mut client, 700);
    assert_eq!(client.capabilities().unwrap(), vec![Capability::Events]);
    journal.push_range(701..=701);
    read_through(&mut client, 701);
    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn live_lag_is_typed_and_reconnect_catches_up_in_order() {
    let dir = TempDir::new().unwrap();
    let journal = Arc::new(Journal::default());
    let daemon = serve(dir.path(), &journal);

    let mut client = connect(dir.path());
    client.events_subscribe(None).unwrap();
    journal.push_range(1..=300);
    assert!(matches!(client.next_event(), Err(IpcError::SlowConsumer)));
    assert!(matches!(client.next_event(), Err(IpcError::NoSubscription)));
    client.reconnect().unwrap();
    read_through(&mut client, 300);
    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn reconnect_follows_a_daemon_restart_with_a_new_token() {
    let dir = TempDir::new().unwrap();
    let journal = Arc::new(Journal::default());
    journal.push_range(1..=2);
    let daemon = serve(dir.path(), &journal);

    let mut client = connect(dir.path());
    client.events_subscribe(None).unwrap();
    read_through(&mut client, 2);
    connect(dir.path()).shutdown().unwrap();
    daemon.join().unwrap();
    assert!(client.next_event().is_err());
    assert!(client.reconnect().is_err());

    journal.push_range(3..=4);
    let daemon = serve(dir.path(), &journal);
    let deadline = Instant::now() + Duration::from_secs(10);
    while client.reconnect().is_err() {
        assert!(Instant::now() < deadline, "reconnect did not succeed");
        thread::sleep(Duration::from_millis(20));
    }
    read_through(&mut client, 4);
    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn reconnect_still_requires_the_current_token() {
    let dir = TempDir::new().unwrap();
    let journal = Arc::new(Journal::default());
    journal.push_range(1..=1);
    let daemon = serve(dir.path(), &journal);

    let mut client = connect(dir.path());
    client.events_subscribe(None).unwrap();
    read_through(&mut client, 1);
    let token_path = dir.path().join("daemon.token");
    let genuine = std::fs::read_to_string(&token_path).unwrap();
    std::fs::write(&token_path, "0".repeat(64)).unwrap();
    assert!(matches!(client.reconnect(), Err(IpcError::Authentication)));
    std::fs::write(&token_path, genuine).unwrap();
    client.reconnect().unwrap();
    assert!(matches!(
        client.projects_register("/unused"),
        Err(IpcError::UnsupportedCapability)
    ));
    client.shutdown().unwrap();
    daemon.join().unwrap();
}
