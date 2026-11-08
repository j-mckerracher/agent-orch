use crate::auth::{authenticate, AuthError, Secret};
use crate::protocol::{self, Capability, Handshake, RpcFailure, RpcId, RpcRequest, RpcResponse};
use crate::unix_transport::set_timeouts;
use crate::{Health, IpcError};
use serde_json::{json, Value};
use std::io;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(crate) struct State {
    pub(crate) secret: Arc<Secret>,
    pub(crate) health: Health,
    pub(crate) started: Instant,
    pub(crate) idle_timeout: Duration,
    pub(crate) stream_timeout: Duration,
    pub(crate) last_request: Mutex<Instant>,
    pub(crate) stopping: AtomicBool,
}

impl State {
    pub(crate) fn new(
        secret: Arc<Secret>,
        data_dir: PathBuf,
        version: String,
        idle_timeout: Duration,
        stream_timeout: Duration,
    ) -> Self {
        let started = Instant::now();
        Self {
            secret,
            health: Health {
                package_version: version,
                uptime_seconds: 0,
                data_directory: data_dir,
            },
            started,
            idle_timeout,
            stream_timeout,
            last_request: Mutex::new(started),
            stopping: AtomicBool::new(false),
        }
    }
}

pub(crate) fn serve(mut stream: UnixStream, state: Arc<State>) {
    if set_timeouts(&stream, state.stream_timeout).is_err() {
        return;
    }
    let request = match protocol::read::<RpcRequest>(&mut stream) {
        Ok(request) if request.jsonrpc == "2.0" && request.method == "handshake" => request,
        _ => return,
    };
    let handshake = match serde_json::from_value::<Handshake>(request.params) {
        Ok(handshake) => handshake,
        Err(_) => {
            let _ = write_error(&mut stream, request.id, -32600, "invalid handshake");
            return;
        }
    };
    let (version, capabilities) = match authenticate(&state.secret, &handshake) {
        Ok(selected) => selected,
        Err(error) => {
            let (code, message) = match error {
                AuthError::Failed => (-32001, "authentication failed"),
                AuthError::UnsupportedCapability => (-32002, "requested capability is unavailable"),
                AuthError::UnsupportedVersion => (-32003, "no common protocol version"),
                _ => (-32600, "invalid handshake"),
            };
            let _ = write_error(&mut stream, request.id, code, message);
            return;
        }
    };
    let welcome = json!({"version": version, "capabilities": &capabilities});
    if write_result(&mut stream, request.id, welcome).is_err() {
        return;
    }
    *state.last_request.lock().unwrap() = Instant::now();
    while !state.stopping.load(Ordering::Acquire) {
        let request = match protocol::read::<RpcRequest>(&mut stream) {
            Ok(request) if request.jsonrpc == "2.0" => request,
            _ => break,
        };
        *state.last_request.lock().unwrap() = Instant::now();
        match request.method.as_str() {
            "capabilities" => {
                let result = json!({"capabilities": if capabilities.contains(&Capability::Health) { vec!["health"] } else { vec![] }});
                if write_result(&mut stream, request.id, result).is_err() {
                    break;
                }
            }
            "health" if capabilities.contains(&Capability::Health) => {
                let health = json!({
                    "package_version": &state.health.package_version,
                    "uptime_seconds": state.started.elapsed().as_secs(),
                    "data_directory": &state.health.data_directory
                });
                if write_result(&mut stream, request.id, health).is_err() {
                    break;
                }
            }
            "shutdown" => {
                state.stopping.store(true, Ordering::Release);
                let _ = write_result(&mut stream, request.id, json!({"stopping": true}));
                break;
            }
            "health" => {
                if write_error(&mut stream, request.id, -32004, "capability not negotiated")
                    .is_err()
                {
                    break;
                }
            }
            _ => {
                if write_error(&mut stream, request.id, -32601, "method unavailable").is_err() {
                    break;
                }
            }
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
}

fn write_result(stream: &mut UnixStream, id: RpcId, result: Value) -> Result<(), IpcError> {
    protocol::write(
        stream,
        &RpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        },
    )?;
    Ok(())
}

fn write_error(
    stream: &mut UnixStream,
    id: RpcId,
    code: i32,
    message: &'static str,
) -> io::Result<()> {
    protocol::write(
        stream,
        &RpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(RpcFailure {
                code,
                message: message.into(),
            }),
        },
    )
    .map_err(|_| io::Error::other("IPC response failed"))
}
