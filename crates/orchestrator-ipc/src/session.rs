use crate::auth::{authenticate, AuthError, Secret};
use crate::protocol::{self, Capability, Handshake, RpcFailure, RpcId, RpcRequest, RpcResponse};
use crate::transport::IpcStream;
use crate::{Health, IpcError, IpcEvent, RpcError, RpcHandler};
use serde_json::{json, Value};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub(crate) struct State {
    pub(crate) secret: Arc<Secret>,
    pub(crate) health: Health,
    pub(crate) started: Instant,
    pub(crate) idle_timeout: Duration,
    pub(crate) stream_timeout: Duration,
    pub(crate) last_request: Mutex<Instant>,
    /// Authenticated connections whose event subscription is still delivering.
    /// While any exists the server is in use and must not idle out.
    pub(crate) live_subscriptions: AtomicUsize,
    /// Handler calls that have started and not finished. A call that outlasts
    /// the idle timeout must not let the server exit under it.
    pub(crate) in_flight: AtomicUsize,
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
            live_subscriptions: AtomicUsize::new(0),
            in_flight: AtomicUsize::new(0),
            stopping: AtomicBool::new(false),
            handler,
            capabilities,
        }
    }

    pub(crate) fn touch(&self) {
        *self
            .last_request
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Instant::now();
    }
}

/// Counts one live event subscription as server activity. Dropping it, when
/// the subscription ends or its connection closes for any reason, restarts the
/// idle clock so a daemon whose last UI just left still gets the full idle
/// period instead of exiting at once.
struct LiveSubscription(Arc<State>);

impl LiveSubscription {
    fn new(state: &Arc<State>) -> Self {
        state.live_subscriptions.fetch_add(1, Ordering::AcqRel);
        state.touch();
        Self(state.clone())
    }
}

impl Drop for LiveSubscription {
    fn drop(&mut self) {
        self.0.live_subscriptions.fetch_sub(1, Ordering::AcqRel);
        self.0.touch();
    }
}

/// Counts one running handler call as server activity and restarts the idle
/// clock when it ends, so the idle period is measured from the response.
struct InFlight<'a>(&'a State);

impl<'a> InFlight<'a> {
    fn new(state: &'a State) -> Self {
        state.in_flight.fetch_add(1, Ordering::AcqRel);
        Self(state)
    }
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, Ordering::AcqRel);
        self.0.touch();
    }
}

/// `AGENT_ORCH_TRACE_REQUESTS` set to anything writes one line to stderr (the
/// daemon log) when a handler call starts and when it ends. It records the
/// method, request id, elapsed time and outcome; never parameters or results.
fn trace_requests() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("AGENT_ORCH_TRACE_REQUESTS").is_some())
}

fn trace(line: std::fmt::Arguments<'_>) {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    eprintln!("[{millis}] {line}");
}

fn sync_live(
    live: &mut Option<LiveSubscription>,
    subscription: &Option<Subscription>,
    state: &Arc<State>,
) {
    let active = subscription.as_ref().is_some_and(|s| !s.ended);
    match (active, live.is_some()) {
        (true, false) => *live = Some(LiveSubscription::new(state)),
        (false, true) => *live = None,
        _ => {}
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
    let (version, capabilities) = match authenticate(&state.secret, &handshake, &state.capabilities)
    {
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
    let journal_epoch = match state
        .handler
        .as_ref()
        .map(|handler| handler.journal_epoch())
    {
        Some(Ok(epoch)) => epoch,
        Some(Err(error)) => {
            let _ = write_rpc_error(&mut stream, request.id, error);
            return;
        }
        None => None,
    };
    let mut welcome = json!({"version": version, "capabilities": &capabilities});
    if let Some(epoch) = journal_epoch {
        welcome["journal_epoch"] = json!(epoch);
    }
    if write_result(&mut stream, request.id, welcome).is_err() {
        return;
    }
    *state
        .last_request
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Instant::now();

    let mut subscription = None;
    let mut live = None;
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
            if !handle_request(
                &mut stream,
                &state,
                &capabilities,
                request,
                &mut subscription,
            ) {
                break;
            }
        }
        if let Some(subscription) = &mut subscription {
            if !send_pending_events(&mut stream, &state, subscription) {
                break;
            }
        }
        sync_live(&mut live, &subscription, &state);
    }
}

static NEXT_SUBSCRIPTION_ID: AtomicU64 = AtomicU64::new(0);
const MAX_BUFFERED_EVENTS: usize = 256;

struct Subscription {
    id: String,
    cursor: i64,
    /// Latest durable sequence when the subscription began. Events up to here
    /// are replayed in pages; only live backlog beyond it can lag.
    replay_until: i64,
    ended: bool,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SubscribeParams {
    #[serde(default)]
    after_sequence: Option<i64>,
    /// Journal epoch the cursor was issued under. A different journal fails
    /// with `EpochMismatch` instead of replaying unrelated events.
    #[serde(default)]
    epoch: Option<String>,
}

fn handle_request(
    stream: &mut Box<dyn IpcStream>,
    state: &State,
    capabilities: &[Capability],
    request: RpcRequest,
    subscription: &mut Option<Subscription>,
) -> bool {
    let method = request.method.as_str();
    if matches!(method, "capabilities" | "health" | "shutdown")
        && !empty_control_params(&request.params)
    {
        return write_rpc_error(stream, request.id, RpcError::InvalidParams);
    }
    if method == "capabilities" {
        let names: Vec<_> = capabilities
            .iter()
            .map(|capability| capability.as_str())
            .collect();
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
        let epoch = match handler.journal_epoch() {
            Ok(epoch) => epoch,
            Err(error) => return write_rpc_error(stream, request.id, error),
        };
        if params.epoch.is_some() && params.epoch != epoch {
            return write_rpc_error(stream, request.id, RpcError::EpochMismatch);
        }
        let replay_until = match handler.latest_event_sequence() {
            Ok(latest) if cursor <= latest => latest,
            Ok(_) => return write_rpc_error(stream, request.id, RpcError::ReplayGap),
            Err(error) => return write_rpc_error(stream, request.id, error),
        };
        let id = format!(
            "subscription-{}",
            NEXT_SUBSCRIPTION_ID.fetch_add(1, Ordering::Relaxed)
        );
        let mut result = json!({"subscription_id": id, "after_sequence": cursor});
        if let Some(epoch) = epoch {
            result["epoch"] = json!(epoch);
        }
        if write_result(stream, request.id, result).is_err() {
            return false;
        }
        *subscription = Some(Subscription {
            id,
            cursor,
            replay_until,
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
    let _in_flight = InFlight::new(state);
    let traced = trace_requests().then(|| (format!("{:?}", request.id), Instant::now()));
    if let Some((id, _)) = &traced {
        trace(format_args!("request start id={id} method={method}"));
    }
    let outcome = handler.dispatch(method, request.params);
    if let Some((id, started)) = &traced {
        let status = match &outcome {
            Ok(_) => "ok".to_owned(),
            Err(error) => format!("error {}", error.code()),
        };
        trace(format_args!(
            "request end id={id} method={method} elapsed_ms={} status={status}",
            started.elapsed().as_millis()
        ));
    }
    match outcome {
        Ok(result) => write_result(stream, request.id, result).is_ok(),
        Err(error) => write_rpc_error(stream, request.id, error),
    }
}

fn empty_control_params(params: &Value) -> bool {
    params.is_null() || params.as_object().is_some_and(|object| object.is_empty())
}

fn method_capability(method: &str) -> Option<Capability> {
    match method {
        "projects.list" | "projects.register" => Some(Capability::Projects),
        "tasks.submit" | "tasks.list" | "tasks.get" | "tasks.cancel" | "tasks.retry" => {
            Some(Capability::Tasks)
        }
        "agents.capabilities"
        | "agents.catalog"
        | "agents.validate_selection"
        | "usage.summary" => Some(Capability::Agents),
        "sessions.list" | "sessions.get" | "sessions.events" => Some(Capability::Sessions),
        "approvals.list" | "approvals.resolve" => Some(Capability::Approvals),
        "workspaces.diff" => Some(Capability::Workspaces),
        "delivery.request" => Some(Capability::Delivery),
        "coordinator.send_message" | "coordinator.propose" | "coordinator.get" => {
            Some(Capability::Coordinator)
        }
        "review.request_approval" | "review.evidence" | "review.request_changes" => {
            Some(Capability::Review)
        }
        "host.metrics" => Some(Capability::Host),
        "queue.snapshot" => Some(Capability::Queue),
        "workstreams.create"
        | "workstreams.list"
        | "workstreams.get"
        | "workstreams.update"
        | "workstreams.add_objective"
        | "workstreams.revise_objective"
        | "workstreams.set_gate"
        | "workstreams.link_task"
        | "workstreams.unlink_task" => Some(Capability::Workstreams),
        // M7-08 (proposed names): conversations share the coordinator capability.
        "conversations.send" | "conversations.get" | "conversations.cancel" => {
            Some(Capability::Coordinator)
        }
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
    while subscription.cursor < subscription.replay_until {
        if state.stopping.load(Ordering::Acquire) {
            return false;
        }
        let events = match handler.events_after(subscription.cursor, MAX_BUFFERED_EVENTS) {
            Ok(events) if !events.is_empty() => events,
            Ok(_) => return write_event_error(stream, subscription, RpcError::Internal),
            Err(error) => return write_event_error(stream, subscription, error),
        };
        if !send_events(stream, state, subscription, events) {
            return false;
        }
        if subscription.ended {
            return true;
        }
    }
    let events = match handler.events_after(subscription.cursor, MAX_BUFFERED_EVENTS + 1) {
        Ok(events) => events,
        Err(error) => return write_event_error(stream, subscription, error),
    };
    if events.len() > MAX_BUFFERED_EVENTS {
        return write_event_error(stream, subscription, RpcError::SlowConsumer);
    }
    send_events(stream, state, subscription, events)
}

/// Writes contiguous events and advances the cursor. A hole ends the
/// subscription with a typed error; a failed write returns `false`.
fn send_events(
    stream: &mut Box<dyn IpcStream>,
    state: &State,
    subscription: &mut Subscription,
    events: Vec<IpcEvent>,
) -> bool {
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
