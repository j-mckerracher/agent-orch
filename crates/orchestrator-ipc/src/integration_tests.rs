use crate::{Capability, Client, IpcError, IpcEvent, RpcError, RpcHandler, Server, ServerConfig};
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn start(dir: &Path, idle: Duration) -> JoinHandle<()> {
    let dir = dir.to_path_buf();
    thread::spawn(move || {
        Server::serve(
            ServerConfig::new(dir, "test-version")
                .with_idle_timeout(idle)
                .with_stream_timeout(Duration::from_millis(250)),
        )
        .unwrap();
    })
}

#[derive(Default)]
struct EventHandler {
    events: std::sync::Mutex<Vec<IpcEvent>>,
}

impl EventHandler {
    fn push(&self, sequence: i64) {
        self.events.lock().unwrap().push(IpcEvent {
            sequence,
            task_id: "task-1".into(),
            session_id: None,
            kind: "task.transition".into(),
            payload: json!({"sequence": sequence}),
            created_at: "2026-11-08T00:00:00Z".into(),
        });
    }
}

impl RpcHandler for EventHandler {
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

    fn keep_alive(&self) -> bool {
        true
    }
}

fn start_with_handler(dir: &Path, handler: std::sync::Arc<dyn RpcHandler>) -> JoinHandle<()> {
    let dir = dir.to_path_buf();
    thread::spawn(move || {
        Server::serve_with_handler(
            ServerConfig::new(dir, "event-test")
                .with_idle_timeout(Duration::from_secs(10))
                .with_stream_timeout(Duration::from_millis(50)),
            handler,
        )
        .unwrap();
    })
}

fn ready(dir: &Path) {
    let deadline = Instant::now() + Duration::from_secs(3);
    let token_path = dir.join("daemon.token");
    let socket_path = dir.join("ipc/daemon.sock");
    while !socket_path.exists() {
        assert!(
            Instant::now() < deadline,
            "daemon did not create IPC socket"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        fs::symlink_metadata(&token_path).is_ok_and(|metadata| metadata.file_type().is_file()),
        "socket appeared before authentication token"
    );
    loop {
        let token_ready = fs::symlink_metadata(&token_path).is_ok_and(|metadata| {
            metadata.file_type().is_file() && metadata.permissions().mode() & 0o777 == 0o600
        });
        let directory_ready = fs::metadata(dir.join("ipc"))
            .is_ok_and(|metadata| metadata.permissions().mode() & 0o777 == 0o700);
        let socket_ready = fs::symlink_metadata(&socket_path).is_ok_and(|metadata| {
            metadata.file_type().is_socket() && metadata.permissions().mode() & 0o777 == 0o600
        });
        if token_ready && directory_ready && socket_ready {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "daemon IPC permissions timed out"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn raw_request(dir: &Path, token: Option<&str>, capabilities: Vec<&str>) -> String {
    let mut stream = UnixStream::connect(dir.join("ipc/daemon.sock")).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut params = json!({"versions":[1], "capabilities":capabilities});
    if let Some(token) = token {
        params["token"] = json!(token);
    }
    let request = json!({"jsonrpc":"2.0", "id":9, "method":"handshake", "params":params});
    crate::protocol::write(&mut stream, &request).unwrap();
    let response: Value = crate::protocol::read(&mut stream).unwrap();
    let mut trailing = [0u8; 1];
    assert_eq!(stream.read(&mut trailing).unwrap(), 0);
    response.to_string()
}

fn authenticated_stream(dir: &Path, capabilities: &[&str]) -> UnixStream {
    let token = fs::read_to_string(dir.join("daemon.token")).unwrap();
    let mut stream = UnixStream::connect(dir.join("ipc/daemon.sock")).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    crate::protocol::write(
        &mut stream,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "handshake",
            "params": {
                "token": token,
                "versions": [1],
                "capabilities": capabilities
            }
        }),
    )
    .unwrap();
    let welcome: Value = crate::protocol::read(&mut stream).unwrap();
    assert!(welcome.get("result").is_some());
    stream
}

#[test]
fn authenticated_health_and_shutdown_remove_owned_files() {
    let dir = TempDir::new().unwrap();
    let daemon = start(dir.path(), Duration::from_secs(10));
    ready(dir.path());
    assert_eq!(
        fs::metadata(dir.path().join("ipc"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(dir.path().join("ipc/daemon.sock"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(dir.path().join("daemon.token"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let token = fs::read_to_string(dir.path().join("daemon.token")).unwrap();
    assert_eq!(token.len(), 64);
    assert!(token
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    let mut client = Client::connect(dir.path()).unwrap();
    assert_eq!(
        client.capabilities().unwrap(),
        vec![crate::Capability::Health]
    );
    let health = client.health().unwrap();
    assert_eq!(health.package_version, "test-version");
    assert!(health.uptime_seconds < 10);
    assert_eq!(health.data_directory.as_path(), dir.path());
    client.shutdown().unwrap();
    daemon.join().unwrap();
    assert!(!dir.path().join("daemon.token").exists());
    assert!(!dir.path().join("ipc/daemon.sock").exists());
}

#[test]
fn wrong_missing_and_unsupported_handshakes_are_sanitized() {
    let dir = TempDir::new().unwrap();
    let daemon = start(dir.path(), Duration::from_secs(10));
    ready(dir.path());
    let token = fs::read_to_string(dir.path().join("daemon.token")).unwrap();
    let mut wrong_token = token.clone();
    wrong_token.replace_range(..1, if token.starts_with('0') { "1" } else { "0" });
    let wrong = raw_request(dir.path(), Some(&wrong_token), vec!["health"]);
    assert!(wrong.contains("authentication failed"));
    assert!(!wrong.contains(&token));
    assert!(!wrong.contains(&wrong_token));
    let missing = raw_request(dir.path(), None, vec!["health"]);
    assert!(missing.contains("authentication failed"));
    let unknown = raw_request(
        dir.path(),
        Some(&token),
        vec!["attacker-controlled-capability"],
    );
    assert!(unknown.contains("requested capability is unavailable"));
    assert!(!unknown.contains("attacker-controlled-capability"));
    fs::write(dir.path().join("daemon.token"), &wrong_token).unwrap();
    let error = Client::connect(dir.path()).err().unwrap();
    assert!(!error.to_string().contains(&token));
    assert!(!format!("{error:?}").contains(&token));
    fs::write(dir.path().join("daemon.token"), &token).unwrap();
    let debug = format!("{:?}", crate::FrameError::MalformedJson);
    assert!(!debug.contains(&token));
    let mut client = Client::connect(dir.path()).unwrap();
    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn stalled_and_garbage_connections_do_not_block_health() {
    let dir = TempDir::new().unwrap();
    let daemon = start(dir.path(), Duration::from_secs(10));
    ready(dir.path());
    let mut stalled = UnixStream::connect(dir.path().join("ipc/daemon.sock")).unwrap();
    stalled.write_all(&[0]).unwrap();
    let mut garbage = UnixStream::connect(dir.path().join("ipc/daemon.sock")).unwrap();
    garbage.write_all(&[0, 2, 0, 0, 0, 0]).unwrap();
    let mut client = Client::connect(dir.path()).unwrap();
    assert_eq!(client.health().unwrap().package_version, "test-version");
    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn subscription_replays_then_streams_and_reconnects_from_observed_cursor() {
    let dir = TempDir::new().unwrap();
    let handler = std::sync::Arc::new(EventHandler::default());
    handler.push(1);
    handler.push(2);
    let daemon = start_with_handler(dir.path(), handler.clone());
    ready(dir.path());

    let mut client = Client::connect(dir.path()).unwrap();
    let subscription = client.events_subscribe(None).unwrap();
    assert!(!subscription.is_empty());
    assert_eq!(client.next_event().unwrap().sequence, 1);
    handler.push(3);
    assert_eq!(client.next_event().unwrap().sequence, 2);
    assert_eq!(client.next_event().unwrap().sequence, 3);
    assert_eq!(client.last_event_sequence(), Some(3));
    drop(client);

    handler.push(4);
    handler.push(5);
    let mut reconnected = Client::connect(dir.path()).unwrap();
    reconnected.events_subscribe(Some(3)).unwrap();
    assert_eq!(reconnected.next_event().unwrap().sequence, 4);
    assert_eq!(reconnected.next_event().unwrap().sequence, 5);
    assert_eq!(reconnected.last_event_sequence(), Some(5));
    reconnected.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn slow_event_subscriber_receives_typed_lag_error() {
    let dir = TempDir::new().unwrap();
    let handler = std::sync::Arc::new(EventHandler::default());
    for sequence in 1..=257 {
        handler.push(sequence);
    }
    let daemon = start_with_handler(dir.path(), handler);
    ready(dir.path());
    let mut client = Client::connect(dir.path()).unwrap();
    client.events_subscribe(None).unwrap();
    assert!(matches!(client.next_event(), Err(IpcError::SlowConsumer)));
    assert!(matches!(client.next_event(), Err(IpcError::NoSubscription)));
    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn methods_require_handshake_and_negotiated_capability() {
    let dir = TempDir::new().unwrap();
    let handler = std::sync::Arc::new(EventHandler::default());
    let daemon = start_with_handler(dir.path(), handler);
    ready(dir.path());

    let mut unopened = UnixStream::connect(dir.path().join("ipc/daemon.sock")).unwrap();
    unopened
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    crate::protocol::write(
        &mut unopened,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "events.subscribe", "params": {}}),
    )
    .unwrap();
    assert!(crate::protocol::read::<Value>(&mut unopened).is_err());
    drop(unopened);

    let token = fs::read_to_string(dir.path().join("daemon.token")).unwrap();
    let mut stream = UnixStream::connect(dir.path().join("ipc/daemon.sock")).unwrap();
    crate::protocol::write(
        &mut stream,
        &json!({
            "jsonrpc": "2.0", "id": 1, "method": "handshake",
            "params": {"token": token, "versions": [1], "capabilities": ["health"]}
        }),
    )
    .unwrap();
    let _: Value = crate::protocol::read(&mut stream).unwrap();
    crate::protocol::write(
        &mut stream,
        &json!({"jsonrpc": "2.0", "id": 2, "method": "events.subscribe", "params": {}}),
    )
    .unwrap();
    let response: Value = crate::protocol::read(&mut stream).unwrap();
    assert_eq!(response["error"]["code"], -32004);

    let mut client = Client::connect(dir.path()).unwrap();
    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn handshake_rejects_unknown_fields() {
    let dir = TempDir::new().unwrap();
    let daemon = start(dir.path(), Duration::from_secs(10));
    ready(dir.path());
    let token = fs::read_to_string(dir.path().join("daemon.token")).unwrap();
    let mut stream = UnixStream::connect(dir.path().join("ipc/daemon.sock")).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    crate::protocol::write(
        &mut stream,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "handshake",
            "params": {
                "token": token,
                "versions": [1],
                "capabilities": ["health"],
                "unexpected": true
            }
        }),
    )
    .unwrap();
    let response: Value = crate::protocol::read(&mut stream).unwrap();
    assert_eq!(response["error"]["code"], -32600);
    let mut client = Client::connect(dir.path()).unwrap();
    client.shutdown().unwrap();
    daemon.join().unwrap();
}

#[test]
fn requests_reject_unknown_fields_and_control_params() {
    let dir = TempDir::new().unwrap();
    let daemon = start(dir.path(), Duration::from_secs(10));
    ready(dir.path());
    let mut stream = authenticated_stream(dir.path(), &["health"]);
    for (id, method) in [(2, "capabilities"), (3, "health"), (4, "shutdown")] {
        crate::protocol::write(
            &mut stream,
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": {"unexpected": true}
            }),
        )
        .unwrap();
        let response: Value = crate::protocol::read(&mut stream).unwrap();
        assert_eq!(response["error"]["code"], -32602, "{method}");
    }
    drop(stream);

    let mut stream = authenticated_stream(dir.path(), &["health"]);
    crate::protocol::write(
        &mut stream,
        &json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "health",
            "params": null,
            "unexpected": true
        }),
    )
    .unwrap();
    assert!(crate::protocol::read::<Value>(&mut stream).is_err());
    drop(stream);

    let mut client = Client::connect(dir.path()).unwrap();
    assert_eq!(client.health().unwrap().package_version, "test-version");
    client.shutdown().unwrap();
    daemon.join().unwrap();
}
