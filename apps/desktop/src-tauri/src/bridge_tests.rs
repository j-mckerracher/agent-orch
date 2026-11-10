use super::*;
use orchestrator_ipc::{Client, IpcEvent, RpcError, RpcHandler, Server, ServerConfig};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use tempfile::TempDir;

const WAIT: Duration = Duration::from_secs(10);

struct Journal;

impl RpcHandler for Journal {
    fn capabilities(&self) -> Vec<Capability> {
        vec![Capability::Events]
    }

    fn dispatch(&self, _: &str, _: Value) -> Result<Value, RpcError> {
        Err(RpcError::InvalidParams)
    }

    fn events_after(&self, after: i64, _: usize) -> Result<Vec<IpcEvent>, RpcError> {
        Ok((after + 1..=3)
            .map(|sequence| IpcEvent {
                sequence,
                task_id: "task-1".into(),
                session_id: None,
                kind: "task.transition".into(),
                payload: json!({}),
                created_at: "2026-11-09T00:00:00Z".into(),
            })
            .collect())
    }

    fn latest_event_sequence(&self) -> Result<i64, RpcError> {
        Ok(3)
    }

    fn keep_alive(&self) -> bool {
        true
    }
}

fn serve(dir: &std::path::Path) -> JoinHandle<()> {
    let dir = dir.to_path_buf();
    thread::spawn(move || {
        Server::serve_with_handler(
            ServerConfig::new(dir, "bridge-test").with_stream_timeout(Duration::from_secs(1)),
            Arc::new(Journal),
        )
        .unwrap();
    })
}

fn collector() -> (
    impl Fn(Value) -> bool + Send + Sync + 'static,
    Receiver<Value>,
) {
    let (sender, receiver) = mpsc::channel();
    let sender = Mutex::new(sender);
    (
        move |payload| sender.lock().unwrap().send(payload).is_ok(),
        receiver,
    )
}

fn open(bridge: &Bridge, window: &str) -> (Opened, Receiver<Value>) {
    let deadline = Instant::now() + WAIT;
    loop {
        let (emit, receiver) = collector();
        match bridge.open(window, &[Capability::Events], emit) {
            Ok(opened) => return (opened, receiver),
            Err(error) => {
                assert!(Instant::now() < deadline, "daemon not ready: {error:?}");
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

fn subscribe() -> Value {
    json!({"jsonrpc": "2.0", "id": 7, "method": "events.subscribe", "params": {}})
}

fn next(receiver: &Receiver<Value>) -> Value {
    receiver.recv_timeout(WAIT).expect("payload within 10 s")
}

fn stop(dir: &std::path::Path, daemon: JoinHandle<()>) {
    Client::connect_with_capabilities(dir, &[Capability::Events])
        .unwrap()
        .shutdown()
        .unwrap();
    daemon.join().unwrap();
}

#[test]
fn forwards_frames_and_closing_a_window_leaves_the_daemon_running() {
    let dir = TempDir::new().unwrap();
    let daemon = serve(dir.path());
    let bridge = Bridge::new(Some(dir.path().to_path_buf()), None);

    let (opened, frames) = open(&bridge, "main");
    assert!(!opened.launched_by_host);
    assert_eq!(opened.welcome["version"], 1);
    bridge
        .send("main", opened.connection_id, subscribe())
        .unwrap();
    let response = next(&frames);
    assert_eq!(response["kind"], "message");
    assert_eq!(response["message"]["id"], 7);
    for expected in 1..=3 {
        let event = next(&frames);
        assert_eq!(event["message"]["params"]["event"]["sequence"], expected);
    }

    bridge.close_window("main");
    assert_eq!(bridge.open_connections(), 0);
    assert_eq!(next(&frames), json!({"kind": "closed", "reason": "local"}));

    // A new window reauthenticates and replays from its own cursor; the
    // daemon never noticed the first window go away.
    let (reopened, frames) = open(&bridge, "main");
    assert_ne!(reopened.connection_id, opened.connection_id);
    let mut resume = subscribe();
    resume["params"] = json!({"after_sequence": 2});
    bridge.send("main", reopened.connection_id, resume).unwrap();
    assert_eq!(next(&frames)["message"]["id"], 7);
    assert_eq!(next(&frames)["message"]["params"]["event"]["sequence"], 3);
    bridge.close("main", reopened.connection_id);
    stop(dir.path(), daemon);
}

#[test]
fn the_ui_cannot_shut_the_daemon_down_or_use_another_windows_connection() {
    let dir = TempDir::new().unwrap();
    let daemon = serve(dir.path());
    let bridge = Bridge::new(Some(dir.path().to_path_buf()), None);
    let (opened, _frames) = open(&bridge, "main");

    for method in ["shutdown", "handshake"] {
        let error = bridge
            .send(
                "main",
                opened.connection_id,
                json!({"jsonrpc": "2.0", "id": 1, "method": method}),
            )
            .unwrap_err();
        assert_eq!(error.code, "protocol");
    }
    assert_eq!(
        bridge
            .send("main", opened.connection_id, json!({"id": 1}))
            .unwrap_err()
            .code,
        "protocol"
    );
    assert_eq!(
        bridge
            .send("other", opened.connection_id, subscribe())
            .unwrap_err()
            .code,
        "closed"
    );
    bridge.close("other", opened.connection_id);
    assert_eq!(bridge.open_connections(), 1);
    assert_eq!(
        bridge.send("main", 999, subscribe()).unwrap_err().code,
        "closed"
    );

    bridge.close_window("main");
    stop(dir.path(), daemon);
}

#[test]
fn a_daemon_that_stops_is_reported_as_a_closed_connection_not_a_task_state() {
    let dir = TempDir::new().unwrap();
    let daemon = serve(dir.path());
    let bridge = Bridge::new(Some(dir.path().to_path_buf()), None);
    let (opened, frames) = open(&bridge, "main");
    bridge
        .send("main", opened.connection_id, subscribe())
        .unwrap();
    assert_eq!(next(&frames)["message"]["id"], 7);

    stop(dir.path(), daemon);
    loop {
        let payload = next(&frames);
        if payload["kind"] == "closed" {
            assert_eq!(payload["reason"], "remote");
            break;
        }
    }
    assert_eq!(
        bridge
            .send("main", opened.connection_id, subscribe())
            .unwrap_err()
            .code,
        "closed"
    );
}

#[test]
fn no_daemon_and_no_executable_is_daemon_unavailable() {
    let dir = TempDir::new().unwrap();
    let bridge = Bridge::new(Some(dir.path().to_path_buf()), None);
    let (emit, _frames) = collector();
    let error = bridge
        .open("main", &[Capability::Events], emit)
        .unwrap_err();
    assert_eq!(error.code, "daemon-unavailable");

    let bridge = Bridge::new(None, None);
    let (emit, _frames) = collector();
    assert_eq!(
        bridge
            .open("main", &[Capability::Events], emit)
            .unwrap_err()
            .code,
        "daemon-unavailable"
    );
}

#[test]
fn a_daemon_executable_that_cannot_start_is_daemon_unavailable() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("no-such-daemon");
    let bridge = Bridge::new(Some(dir.path().join("data")), Some(missing));
    let (emit, _frames) = collector();
    let error = bridge
        .open("main", &[Capability::Events], emit)
        .unwrap_err();
    assert_eq!(error.code, "daemon-unavailable");
    assert!(
        error.message.contains("could not start the daemon"),
        "{error:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_daemon_that_exits_at_once_is_reported_with_its_log_location() {
    let dir = TempDir::new().unwrap();
    let data = dir.path().join("data");
    let bridge = Bridge::new(Some(data.clone()), Some(PathBuf::from("/usr/bin/false")));
    let (emit, _frames) = collector();
    let error = bridge
        .open("main", &[Capability::Events], emit)
        .unwrap_err();
    assert_eq!(error.code, "daemon-unavailable");
    assert!(
        error.message.contains("exited right after starting"),
        "{error:?}"
    );
    assert!(error.message.contains(launch::DAEMON_LOG), "{error:?}");
    assert!(data.join(launch::DAEMON_LOG).is_file());
}

// The test binary is the "daemon": it rejects `--data-dir` and exits at once on
// every platform, after the log already holds a line with a credential.
#[test]
fn a_daemon_that_exits_at_once_shows_the_redacted_end_of_its_log() {
    let dir = TempDir::new().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(
        data.join(launch::DAEMON_LOG),
        "{\"event\":\"daemon_failed\",\"error\":\"token=SECRETVALUE0123456789\"}\n",
    )
    .unwrap();
    let bridge = Bridge::new(Some(data), Some(std::env::current_exe().unwrap()));
    let (emit, _frames) = collector();
    let error = bridge
        .open("main", &[Capability::Events], emit)
        .unwrap_err();
    assert_eq!(error.code, "daemon-unavailable");
    assert!(error.message.contains("daemon_failed"), "{error:?}");
    assert!(!error.message.contains("SECRETVALUE"), "{error:?}");
}
