use crate::auth::{authenticate, AuthError, Secret};
use crate::protocol::{self, Capability, Handshake, RpcFailure, RpcId, RpcRequest, RpcResponse};
use crate::transport::IpcStream;
use crate::{Health, IpcError, RpcError, RpcHandler};
use serde_json::{json, Value};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    pub(crate) handler: Option<Arc<dyn RpcHandler>>,
    pub(crate) capabilities: Vec<Capability>,
}

impl State {
    pub(crate) fn new(
        secret: Arc<Secret>,
        data_dir: PathBuf,
        version: String,
        idle_timeout: Duration,
        stream_timeout: Duration,
        handler: Option<Arc<dyn RpcHandler>>,
    ) -> Self {
        let started = Instant::now();
        let mut capabilities = vec![Capability::Health];
        if let Some(handler) = &handler {
            for capability in handler.capabilities() {
                if !capabilities.contains(&capability) {
                    capabilities.push(capability);
                }
            }
        }
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
            handler,
            capabilities,
        }
    }
}

pub(crate) fn serve(mut stream: Box<dyn IpcStream>, state: Arc<State>) {
    if stream.set_timeouts(state.stream_timeout).is_err() {
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
    let (version, capabilities) =
        match authenticate(&state.secret, &handshake, &state.capabilities) {
            Ok(selected) => selected,
            Err(error) => {
                let (code, message) = match error {
                    AuthError::Failed => (-32001, "authentication failed"),
                    AuthError::UnsupportedCapability => {
                        (-32002, "requested capability is unavailable")
                    }
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
    *state
        .last_request
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Instant::now();

    let mut subscription = None;
    while !state.stopping.load(Ordering::Acquire) {
        let request = match protocol::read_optional::<RpcRequest>(&mut stream) {
            Ok(Some(request)) if request.jsonrpc == "2.0" => Some(request),
            Ok(Some(_)) | Err(_) => break,
            Ok(None) if subscription.is_none() => break,
            Ok(None) => None,
        };
        if let Some(request) = request {
            *state
                .last_request
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = Instant::now();
            if !handle_request(&mut stream, &state, &capabilities, request, &mut subscription) {
                break;
            }
        }
        if let Some(subscription) = &mut subscription {
            if !send_pending_events(&mut stream, &state, subscription) {
                break;
            }
        }
    }
}

static NEXT_SUBSCRIPTION_ID: AtomicU64 = AtomicU64::new(0);
const MAX_BUFFERED_EVENTS: usize = 256;

struct Subscription {
    id: String,
    cursor: i64,
    ended: bool,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SubscribeParams {
    #[serde(default)]
    after_sequence: Option<i64>,
}

fn handle_request(
    stream: &mut Box<dyn IpcStream>,
    state: &State,
    capabilities: &[Capability],
    request: RpcRequest,
    subscription: &mut Option<Subscription>,
) -> bool {
    let method = request.method.as_str();
    if method == "capabilities" {
        let names: Vec<_> = capabilities.iter().map(|capability| capability.as_str()).collect();
        return write_result(stream, request.id, json!({"capabilities": names})).is_ok();
    }
    if method == "health" {
        if !capabilities.contains(&Capability::Health) {
            return write_error(stream, request.id, -32004, "capability not negotiated").is_ok();
        }
        let health = json!({
            "package_version": &state.health.package_version,
            "uptime_seconds": state.started.elapsed().as_secs(),
            "data_directory": &state.health.data_directory
        });
        return write_result(stream, request.id, health).is_ok();
    }
    if method == "shutdown" {
        state.stopping.store(true, Ordering::Release);
        let _ = write_result(stream, request.id, json!({"stopping": true}));
        return false;
    }
    if method == "events.subscribe" {
        if !capabilities.contains(&Capability::Events) {
            return write_error(stream, request.id, -32004, "capability not negotiated").is_ok();
        }
        if subscription.is_some() {
            return write_rpc_error(stream, request.id, RpcError::Conflict);
        }
        let params = match serde_json::from_value::<SubscribeParams>(request.params) {
            Ok(params) => params,
            Err(_) => return write_rpc_error(stream, request.id, RpcError::InvalidParams),
        };
        let cursor = params.after_sequence.unwrap_or(0);
        if cursor < 0 {
            return write_rpc_error(stream, request.id, RpcError::InvalidParams);
        }
        let Some(handler) = &state.handler else {
            return write_error(stream, request.id, -32601, "method unavailable").is_ok();
        };
        match handler.latest_event_sequence() {
            Ok(latest) if cursor <= latest => {}
            Ok(_) => return write_rpc_error(stream, request.id, RpcError::InvalidParams),
            Err(error) => return write_rpc_error(stream, request.id, error),
        }
        let id = format!(
            "subscription-{}",
            NEXT_SUBSCRIPTION_ID.fetch_add(1, Ordering::Relaxed)
        );
        if write_result(
            stream,
            request.id,
            json!({"subscription_id": id, "after_sequence": cursor}),
        )
        .is_err()
        {
            return false;
        }
        *subscription = Some(Subscription {
            id,
            cursor,
            ended: false,
        });
        return true;
    }

    let Some(capability) = method_capability(method) else {
        return write_error(stream, request.id, -32601, "method unavailable").is_ok();
    };
    if !capabilities.contains(&capability) {
        return write_error(stream, request.id, -32004, "capability not negotiated").is_ok();
    }
    let Some(handler) = &state.handler else {
        return write_error(stream, request.id, -32601, "method unavailable").is_ok();
    };
    match handler.dispatch(method, request.params) {
        Ok(result) => write_result(stream, request.id, result).is_ok(),
        Err(error) => write_rpc_error(stream, request.id, error),
    }
}

fn method_capability(method: &str) -> Option<Capability> {
    match method {
        "projects.list" => Some(Capability::Projects),
        "tasks.submit" | "tasks.list" | "tasks.get" | "tasks.cancel" => {
            Some(Capability::Tasks)
        }
        "sessions.list" | "sessions.get" | "sessions.events" => Some(Capability::Sessions),
        "approvals.list" | "approvals.resolve" => Some(Capability::Approvals),
        "workspaces.diff" => Some(Capability::Workspaces),
        "delivery.request" => Some(Capability::Delivery),
        _ => None,
    }
}

fn send_pending_events(
    stream: &mut Box<dyn IpcStream>,
    state: &State,
    subscription: &mut Subscription,
) -> bool {
    if subscription.ended {
        return true;
    }
    let Some(handler) = &state.handler else {
        return false;
    };
    let events = match handler.events_after(subscription.cursor, MAX_BUFFERED_EVENTS + 1) {
        Ok(events) => events,
        Err(error) => return write_event_error(stream, subscription, error),
    };
    if events.len() > MAX_BUFFERED_EVENTS {
        return write_event_error(stream, subscription, RpcError::SlowConsumer);
    }
    for event in events {
        if subscription.cursor.checked_add(1) != Some(event.sequence) {
            return write_event_error(stream, subscription, RpcError::Internal);
        }
        let params = json!({"subscription_id": subscription.id, "event": event});
        if protocol::write(
            stream,
            &protocol::RpcNotification {
                jsonrpc: "2.0".into(),
                method: "events.event".into(),
                params,
            },
        )
        .is_err()
        {
            return false;
        }
        subscription.cursor = event.sequence;
        *state
            .last_request
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Instant::now();
    }
    true
}

fn write_event_error(
    stream: &mut Box<dyn IpcStream>,
    subscription: &mut Subscription,
    error: RpcError,
) -> bool {
    let result = protocol::write(
        stream,
        &protocol::RpcNotification {
            jsonrpc: "2.0".into(),
            method: "events.error".into(),
            params: json!({
                "subscription_id": subscription.id,
                "error": {"code": error.code(), "message": error.message()}
            }),
        },
    )
    .is_ok();
    subscription.ended = true;
    result
}

fn write_rpc_error(stream: &mut Box<dyn IpcStream>, id: RpcId, error: RpcError) -> bool {
    write_error(stream, id, error.code(), error.message()).is_ok()
}


fn write_result(stream: &mut Box<dyn IpcStream>, id: RpcId, result: Value) -> Result<(), IpcError> {
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
    stream: &mut Box<dyn IpcStream>,
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
