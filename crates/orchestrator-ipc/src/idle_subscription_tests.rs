//! A live event subscription counts as server activity; an idle daemon still
//! exits. Timeouts are seconds-scale and assertions are strict so a slow
//! Windows host cannot turn a timing gap into a pass or a spurious failure.

use crate::{Capability, Client, IpcEvent, RpcError, RpcHandler, Server, ServerConfig};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const IDLE_TIMEOUT: Duration = Duration::from_secs(2);
const STREAM_TIMEOUT: Duration = Duration::from_secs(1);
const READY_DEADLINE: Duration = Duration::from_secs(15);

/// Serves an empty journal and relies on the default `keep_alive` (false), so
/// only the subscription itself can keep the server running.
struct EmptyJournal;

impl RpcHandler for EmptyJournal {
    fn capabilities(&self) -> Vec<Capability> {
        vec![Capability::Events]
    }

    fn dispatch(&self, _: &str, _: Value) -> Result<Value, RpcError> {
        Err(RpcError::InvalidParams)
    }

    fn events_after(&self, _: i64, _: usize) -> Result<Vec<IpcEvent>, RpcError> {
        Ok(Vec::new())
    }

    fn latest_event_sequence(&self) -> Result<i64, RpcError> {
        Ok(0)
    }
}

fn serve(dir: &Path) -> JoinHandle<()> {
    let dir = dir.to_path_buf();
    thread::spawn(move || {
        Server::serve_with_handler(
            ServerConfig::new(dir, "idle-subscription")
                .with_idle_timeout(IDLE_TIMEOUT)
                .with_stream_timeout(STREAM_TIMEOUT),
            Arc::new(EmptyJournal),
        )
        .unwrap();
    })
}

fn connect(dir: &Path) -> Client {
    let deadline = Instant::now() + READY_DEADLINE;
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

fn wait_for_exit(daemon: &JoinHandle<()>, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if daemon.is_finished() {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    daemon.is_finished()
}

#[test]
fn live_subscription_keeps_an_otherwise_idle_server_running() {
    let dir = TempDir::new().unwrap();
    let daemon = serve(dir.path());
    let mut client = connect(dir.path());
    client.events_subscribe(None).unwrap();

    // Well past two idle periods with no request and no event.
    thread::sleep(IDLE_TIMEOUT * 3);
    assert!(
        !daemon.is_finished(),
        "an open subscription must keep the server alive"
    );
    assert!(dir.path().join("daemon.token").exists());

    // The server is still usable: a fresh connection authenticates and stops it.
    let mut other = Client::connect(dir.path()).unwrap();
    assert_eq!(other.health().unwrap().package_version, "idle-subscription");
    other.shutdown().unwrap();
    assert!(wait_for_exit(&daemon, READY_DEADLINE));
    daemon.join().unwrap();
}

#[test]
fn server_idles_out_after_its_last_subscriber_leaves() {
    let dir = TempDir::new().unwrap();
    let daemon = serve(dir.path());
    let mut client = connect(dir.path());
    client.events_subscribe(None).unwrap();
    thread::sleep(IDLE_TIMEOUT * 2);
    assert!(!daemon.is_finished());

    let left = Instant::now();
    drop(client);
    assert!(
        wait_for_exit(&daemon, READY_DEADLINE),
        "a daemon with no subscriber and no work must still idle out"
    );
    // The idle clock restarts when the subscriber leaves, so the daemon does
    // not exit at once just because the subscription outlived the timeout.
    assert!(
        left.elapsed() >= IDLE_TIMEOUT - Duration::from_millis(200),
        "exited {:?} after the subscriber left",
        left.elapsed()
    );
    daemon.join().unwrap();
    assert!(!dir.path().join("daemon.token").exists());
}

#[test]
fn unsubscribed_connection_does_not_keep_the_server_alive() {
    let dir = TempDir::new().unwrap();
    let daemon = serve(dir.path());
    let client = connect(dir.path());
    assert!(
        wait_for_exit(&daemon, READY_DEADLINE),
        "an authenticated connection without a subscription is not activity"
    );
    drop(client);
    daemon.join().unwrap();
}

/// Answers every call after `SLOW_CALL`, longer than the server's idle timeout.
struct SlowCall;

const SLOW_CALL: Duration = Duration::from_secs(3);

impl RpcHandler for SlowCall {
    fn capabilities(&self) -> Vec<Capability> {
        vec![Capability::Projects]
    }

    fn dispatch(&self, _: &str, _: Value) -> Result<Value, RpcError> {
        thread::sleep(SLOW_CALL);
        Ok(Value::Array(Vec::new()))
    }

    fn events_after(&self, _: i64, _: usize) -> Result<Vec<IpcEvent>, RpcError> {
        Ok(Vec::new())
    }

    fn latest_event_sequence(&self) -> Result<i64, RpcError> {
        Ok(0)
    }
}

#[test]
fn a_call_longer_than_the_idle_timeout_does_not_stop_the_server() {
    let dir = TempDir::new().unwrap();
    let data = dir.path().to_path_buf();
    let daemon = thread::spawn(move || {
        Server::serve_with_handler(
            ServerConfig::new(data, "slow-call")
                .with_idle_timeout(Duration::from_secs(1))
                .with_stream_timeout(STREAM_TIMEOUT),
            Arc::new(SlowCall),
        )
        .unwrap();
    });
    let deadline = Instant::now() + READY_DEADLINE;
    let mut client = loop {
        match Client::connect_with_timeout(
            dir.path(),
            &[Capability::Health, Capability::Projects],
            READY_DEADLINE,
        ) {
            Ok(client) => break client,
            Err(error) => {
                assert!(Instant::now() < deadline, "daemon not ready: {error}");
                thread::sleep(Duration::from_millis(20));
            }
        }
    };

    assert_eq!(client.projects_list().unwrap(), Value::Array(Vec::new()));
    assert_eq!(
        client.health().unwrap().package_version,
        "slow-call",
        "the server must still serve this connection after a slow call"
    );
    drop(client);
    assert!(
        wait_for_exit(&daemon, READY_DEADLINE),
        "with the call finished the server idles out as usual"
    );
    daemon.join().unwrap();
}
