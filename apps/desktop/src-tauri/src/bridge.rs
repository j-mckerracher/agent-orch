//! Forwards the webview's JSON-RPC messages to the daemon over the
//! authenticated local transport.
//!
//! Boundaries kept here: the webview never sees the token or the socket/pipe;
//! each connection belongs to one window and dies with it; the host never
//! sends `shutdown` or a second `handshake` on the webview's behalf; and a
//! connection ending is reported as a connection event, never inferred to be a
//! daemon or agent state.

use crate::launch;
use orchestrator_ipc::{Capability, Relay, RelayError, RelayEvent};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

const MAX_CONNECTIONS: usize = 8;
const LAUNCH_WAIT: Duration = Duration::from_secs(15);
const LAUNCH_RETRY: Duration = Duration::from_millis(200);
const FORBIDDEN_METHODS: [&str; 2] = ["handshake", "shutdown"];

#[derive(Debug, Serialize, PartialEq)]
pub struct HostError {
    pub code: &'static str,
    pub message: String,
}

impl HostError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn unavailable(message: impl Into<String>) -> Self {
        Self::new("daemon-unavailable", message)
    }
}

impl From<RelayError> for HostError {
    fn from(error: RelayError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    pub connection_id: u64,
    pub welcome: Value,
    /// True when this open had to start the daemon. Says nothing about work.
    pub launched_by_host: bool,
}

struct Connection {
    window: String,
    relay: Relay,
    dead: Arc<AtomicBool>,
}

struct Inner {
    data_dir: Option<PathBuf>,
    daemon_program: Option<PathBuf>,
    next_id: AtomicU64,
    connections: Mutex<HashMap<u64, Connection>>,
    launch_gate: Mutex<()>,
}

#[derive(Clone)]
pub struct Bridge {
    inner: Arc<Inner>,
}

type Sink = Arc<dyn Fn(RelayEvent) + Send + Sync>;
type Attempt<'a> = dyn Fn(&Sink) -> Result<(Relay, Value), RelayError> + 'a;

impl Bridge {
    pub fn from_environment() -> Self {
        Self::new(launch::data_dir(), launch::daemon_program())
    }

    pub fn new(data_dir: Option<PathBuf>, daemon_program: Option<PathBuf>) -> Self {
        Self {
            inner: Arc::new(Inner {
                data_dir,
                daemon_program,
                next_id: AtomicU64::new(1),
                connections: Mutex::new(HashMap::new()),
                launch_gate: Mutex::new(()),
            }),
        }
    }

    /// Opens an authenticated connection for `window`, starting the daemon
    /// first when nothing is serving. `emit` receives `{kind:"message"}` and a
    /// final `{kind:"closed"}` payload and returns false once the webview is
    /// gone.
    pub fn open(
        &self,
        window: &str,
        requested: &[Capability],
        emit: impl Fn(Value) -> bool + Send + Sync + 'static,
    ) -> Result<Opened, HostError> {
        let data_dir = self
            .inner
            .data_dir
            .as_deref()
            .ok_or_else(|| HostError::unavailable("the data directory cannot be determined"))?;
        {
            let mut connections = self.connections();
            connections.retain(|_, connection| !connection.dead.load(Ordering::Acquire));
            if connections.len() >= MAX_CONNECTIONS {
                return Err(HostError::new("io", "too many open daemon connections"));
            }
        }
        let dead = Arc::new(AtomicBool::new(false));
        let sink: Sink = {
            let dead = dead.clone();
            Arc::new(move |event| {
                let (payload, closed) = match event {
                    RelayEvent::Message(message) => {
                        (json!({"kind": "message", "message": message}), false)
                    }
                    RelayEvent::Closed(reason) => {
                        (json!({"kind": "closed", "reason": reason.code()}), true)
                    }
                };
                if !emit(payload) || closed {
                    dead.store(true, Ordering::Release);
                }
            })
        };
        let attempt = |sink: &Sink| {
            let sink = sink.clone();
            Relay::open(data_dir, requested, move |event| sink(event))
        };

        let (relay, welcome, launched_by_host) = match attempt(&sink) {
            Ok((relay, welcome)) => (relay, welcome, false),
            Err(RelayError::Unavailable) => self.launch_and_open(data_dir, &sink, &attempt)?,
            Err(error) => return Err(error.into()),
        };
        let connection_id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        self.connections().insert(
            connection_id,
            Connection {
                window: window.to_owned(),
                relay,
                dead,
            },
        );
        Ok(Opened {
            connection_id,
            welcome,
            launched_by_host,
        })
    }

    fn launch_and_open(
        &self,
        data_dir: &std::path::Path,
        sink: &Sink,
        attempt: &Attempt<'_>,
    ) -> Result<(Relay, Value, bool), HostError> {
        // One launch at a time: a concurrent opener finds the daemon the
        // first one started instead of racing it for the data-directory lock.
        let _gate = self
            .inner
            .launch_gate
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        match attempt(sink) {
            Ok((relay, welcome)) => return Ok((relay, welcome, false)),
            Err(RelayError::Unavailable) => {}
            Err(error) => return Err(error.into()),
        }
        let program = self.inner.daemon_program.as_deref().ok_or_else(|| {
            HostError::unavailable(
                "no daemon is running and the daemon executable was not found next to the app",
            )
        })?;
        let mut child = launch::spawn(program, data_dir).map_err(|error| {
            HostError::unavailable(format!("could not start the daemon ({:?})", error.kind()))
        })?;
        let deadline = Instant::now() + LAUNCH_WAIT;
        loop {
            match attempt(sink) {
                Ok((relay, welcome)) => {
                    thread::spawn(move || {
                        let _ = child.wait();
                    });
                    return Ok((relay, welcome, true));
                }
                Err(RelayError::Unavailable) => {}
                Err(error) => return Err(error.into()),
            }
            if let Ok(Some(status)) = child.try_wait() {
                return Err(HostError::unavailable(launch::explain_with_log(
                    format!(
                        "the daemon exited right after starting ({status}); see {}",
                        data_dir.join(launch::DAEMON_LOG).display()
                    ),
                    data_dir,
                )));
            }
            if Instant::now() >= deadline {
                thread::spawn(move || {
                    let _ = child.wait();
                });
                return Err(HostError::unavailable(launch::explain_with_log(
                    format!(
                        "the daemon did not become ready within {} s; see {}",
                        LAUNCH_WAIT.as_secs(),
                        data_dir.join(launch::DAEMON_LOG).display()
                    ),
                    data_dir,
                )));
            }
            thread::sleep(LAUNCH_RETRY);
        }
    }

    /// Queues one request from `window` on its connection.
    pub fn send(&self, window: &str, connection_id: u64, message: Value) -> Result<(), HostError> {
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .ok_or_else(|| HostError::new("protocol", "a request needs a string method"))?;
        if FORBIDDEN_METHODS.contains(&method) {
            return Err(HostError::new(
                "protocol",
                "that method is not available to the UI",
            ));
        }
        let connections = self.connections();
        let connection = connections
            .get(&connection_id)
            .filter(|connection| connection.window == window)
            .ok_or_else(|| HostError::new("closed", "unknown connection"))?;
        if connection.dead.load(Ordering::Acquire) {
            return Err(HostError::new("closed", "the connection has ended"));
        }
        connection.relay.send(message).map_err(HostError::from)
    }

    /// Closes one connection of `window`. Never touches the daemon.
    pub fn close(&self, window: &str, connection_id: u64) {
        let mut connections = self.connections();
        if connections
            .get(&connection_id)
            .is_some_and(|connection| connection.window == window)
        {
            connections.remove(&connection_id);
        }
    }

    /// Closes every connection of a destroyed window. Never touches the daemon.
    pub fn close_window(&self, window: &str) {
        self.connections()
            .retain(|_, connection| connection.window != window);
    }

    #[cfg(test)]
    fn open_connections(&self) -> usize {
        self.connections().len()
    }

    fn connections(&self) -> MutexGuard<'_, HashMap<u64, Connection>> {
        self.inner
            .connections
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
#[path = "bridge_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "bridge_real_tests.rs"]
mod real_tests;
