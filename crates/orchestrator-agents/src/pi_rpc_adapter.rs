//! Pi JSONL RPC adapter and command handle.
//!
//! The handle owns the child's sole stdin writer and the supervisor. It sends
//! one flushed JSON command per line, correlates responses by ID, and queues
//! session events encountered while waiting for responses. Malformed JSON
//! lines surface as recoverable protocol-error events and fail their turn.
//! Unknown event types and stderr diagnostics remain ignored. Process exits
//! remain transport facts; they never establish task success.
//!
//! `detect_protocol` checks the CLI version and a read-only `get_state` RPC
//! command. Pi does not expose a separate wire-protocol version in its records.
//! Updated: 2026-11-08T11:24:12Z.

use async_trait::async_trait;
use orchestrator_runtime::{
    ChildSupervisor, LaunchRequest, StdinWriter, SupervisionEvent, SupervisorError,
};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::contract::{
    AgentAdapter, AgentCapabilities, AgentError, AgentEvent, AgentExit, AgentHandle, AgentKind,
    AgentLaunchRequest, Result, TurnOutcome,
};
use crate::routing::RoutableAdapter;

/// Pi's RPC protocol facts verified by [`PiRpcAdapter::detect_protocol`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiRpcProtocolInfo {
    /// Version string returned by the installed `pi --version` command.
    pub cli_version: String,
    /// Commands that returned a valid successful response during detection.
    /// This list is verified, not exhaustive.
    pub verified_commands: Vec<String>,
}

/// A correlated response to one Pi RPC command.
#[derive(Debug, Clone, PartialEq)]
pub struct PiRpcResponse {
    /// Request ID echoed by Pi.
    pub id: String,
    /// RPC command name.
    pub command: String,
    /// Whether Pi accepted and handled the command.
    pub success: bool,
    /// Command-specific response data, when present.
    pub data: Option<Value>,
    /// Command error text, when present.
    pub error: Option<String>,
}

/// Adapter for Pi's persistent JSONL RPC mode.
#[derive(Debug, Clone, Default)]
pub struct PiRpcAdapter {
    /// Executable to locate for capability detection. `None` resolves `pi` on PATH.
    program: Option<PathBuf>,
}

impl PiRpcAdapter {
    /// Create an adapter that resolves `pi` from PATH during detection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Use an explicit executable path for detection.
    pub fn with_program(mut self, program: impl Into<PathBuf>) -> Self {
        self.program = Some(program.into());
        self
    }

    fn program(&self) -> PathBuf {
        self.program.clone().unwrap_or_else(|| PathBuf::from("pi"))
    }

    fn locate(&self) -> Option<PathBuf> {
        let program = self.program();
        if program.is_file() {
            return Some(program);
        }
        if let Some(pathvar) = std::env::var_os("PATH") {
            for directory in std::env::split_paths(&pathvar) {
                let candidate = directory.join(&program);
                if candidate.is_file() {
                    return Some(candidate);
                }
                #[cfg(windows)]
                if !program.extension().is_some_and(|extension| {
                    extension.to_string_lossy().eq_ignore_ascii_case("exe")
                }) {
                    let candidate = directory.join(format!("{}.exe", program.to_string_lossy()));
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
        None
    }

    /// Verify the CLI version and `get_state` command without sending a prompt.
    ///
    /// The probe runs Pi with RPC mode, no session persistence, and offline
    /// startup. It closes stdin after the response and requires a clean exit.
    pub async fn detect_protocol(&self) -> Result<PiRpcProtocolInfo> {
        let program = self
            .locate()
            .ok_or_else(|| AgentError::NotFound(self.program().to_string_lossy().into_owned()))?;
        let working_dir = std::env::current_dir().map_err(|error| {
            AgentError::Protocol(format!("could not resolve current directory: {error}"))
        })?;
        let cli_version = read_cli_version(&program, &working_dir).await?;
        let args = [
            "--no-session",
            "--offline",
            "--no-extensions",
            "--no-mcp",
            "--no-skills",
            "--no-context-files",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let mut handle = self
            .spawn_rpc(AgentLaunchRequest {
                program,
                args,
                working_dir,
            })
            .await?;
        let request_id = handle
            .send_command(serde_json::json!({"type":"get_state"}))
            .await?;
        let response = handle.read_response(&request_id).await?;
        if response.command != "get_state"
            || !response.success
            || response.data.as_ref().and_then(Value::as_object).is_none()
        {
            return Err(AgentError::Protocol(format!(
                "Pi RPC get_state probe failed: {}",
                response.error.as_deref().unwrap_or("invalid response")
            )));
        }

        handle.stdin.close();
        let code = match handle.supervisor.wait().await? {
            SupervisionEvent::Exited { code, .. } => code,
            _ => return Err(SupervisorError::NoExit.into()),
        };
        handle.code = code;
        handle.exited = true;
        if code != Some(0) {
            return Err(AgentError::Protocol(format!(
                "Pi RPC probe exited with code {code:?}"
            )));
        }

        Ok(PiRpcProtocolInfo {
            cli_version,
            verified_commands: vec!["get_state".to_owned()],
        })
    }

    /// Start Pi in RPC mode and return its command-capable handle.
    ///
    /// Existing RPC mode arguments are preserved. If no mode is specified,
    /// `--mode rpc` is appended. Request text is never passed through a shell.
    pub async fn spawn_rpc(&self, mut request: AgentLaunchRequest) -> Result<PiRpcHandle> {
        ensure_rpc_mode(&mut request.args)?;
        let launch = LaunchRequest {
            program: request.program,
            args: request.args,
            working_dir: request.working_dir,
        };
        let (supervisor, stdin) = ChildSupervisor::launch_with_stdin(&launch)?;
        Ok(PiRpcHandle::new(supervisor, stdin))
    }
}

impl RoutableAdapter for PiRpcAdapter {
    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            streaming: true, // PiRpcHandle::next_event parses JSONL events (pi_rpc_adapter.rs:384-399, 443-495).
            interactive_input: true, // send_command writes prompt/follow_up RPCs and reads correlated replies (pi_rpc_adapter.rs:240-318).
            structured_output: true, // parse_record decodes JSON responses and events (pi_rpc_adapter.rs:443-495).
            cancellation: true, // PiRpcHandle::cancel closes stdin and cancels the supervisor (pi_rpc_adapter.rs:402-409).
            session_resumption: false, // AgentHandle exposes no resume operation (contract.rs:116-126).
            approvals: false, // event_from_record maps no approval event (pi_rpc_adapter.rs:466-495).
            model_selection: false, // Detection verifies only get_state; no model-selection RPC is implemented (pi_rpc_adapter.rs:129-159).
        }
    }
}

#[async_trait]
impl AgentAdapter for PiRpcAdapter {
    fn kind(&self) -> AgentKind {
        AgentKind::Pi
    }

    async fn detect(&self) -> Result<AgentCapabilities> {
        self.detect_protocol().await?;
        Ok(self.capabilities())
    }

    async fn spawn(&self, request: AgentLaunchRequest) -> Result<Box<dyn AgentHandle>> {
        Ok(Box::new(self.spawn_rpc(request).await?))
    }
}

/// One persistent Pi RPC process.
///
/// The handle exclusively owns the supervisor and stdin writer. Callers send
/// commands by ID and read their responses; events received during response
/// waits remain queued for [`AgentHandle::next_event`].
pub struct PiRpcHandle {
    supervisor: ChildSupervisor,
    stdin: StdinWriter,
    next_id: u64,
    in_flight: HashSet<String>,
    responses: HashMap<String, PiRpcResponse>,
    events: VecDeque<AgentEvent>,
    turn_outcome: Option<TurnOutcome>,
    protocol_error: Option<String>,
    cancelled: bool,
    exited: bool,
    code: Option<i32>,
}

impl PiRpcHandle {
    fn new(supervisor: ChildSupervisor, stdin: StdinWriter) -> Self {
        Self {
            supervisor,
            stdin,
            next_id: 1,
            in_flight: HashSet::new(),
            responses: HashMap::new(),
            events: VecDeque::new(),
            turn_outcome: None,
            protocol_error: None,
            cancelled: false,
            exited: false,
            code: None,
        }
    }

    /// Send a JSON command and return its explicit or generated request ID.
    ///
    /// Commands must be JSON objects with a nonempty string `type`. A supplied
    /// nonempty string `id` is preserved and must be unique among pending work.
    pub async fn send_command(&mut self, mut command: Value) -> Result<String> {
        let object = command.as_object_mut().ok_or_else(|| {
            AgentError::Protocol("Pi RPC command must be a JSON object".to_owned())
        })?;
        let command_type = object
            .get("type")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty() && *name != "response")
            .ok_or_else(|| {
                AgentError::Protocol("Pi RPC command requires a nonempty command type".to_owned())
            })?
            .to_owned();
        let supplied_id = object
            .get("id")
            .map(|value| value.as_str().map(str::to_owned));
        let id = match supplied_id {
            Some(Some(id)) if !id.is_empty() => id,
            Some(_) => {
                return Err(AgentError::Protocol(
                    "Pi RPC command ID must be a nonempty string".to_owned(),
                ));
            }
            None => {
                let id = format!("pi-{}", self.next_id);
                self.next_id = self.next_id.checked_add(1).ok_or_else(|| {
                    AgentError::Protocol("Pi RPC request ID space exhausted".to_owned())
                })?;
                object.insert("id".to_owned(), Value::String(id.clone()));
                id
            }
        };
        if self.in_flight.contains(&id) || self.responses.contains_key(&id) {
            return Err(AgentError::Protocol(format!(
                "Pi RPC request ID is already pending: {id}"
            )));
        }
        if matches!(command_type.as_str(), "prompt" | "follow_up") {
            self.turn_outcome = None;
            self.protocol_error = None;
        }
        let line = serde_json::to_string(&command).map_err(|error| {
            AgentError::Protocol(format!("could not encode Pi RPC command: {error}"))
        })?;
        self.in_flight.insert(id.clone());
        if let Err(error) = self.stdin.write_line(&line).await {
            self.in_flight.remove(&id);
            return Err(error.into());
        }
        Ok(id)
    }

    /// Read the response matching `id`, buffering other responses and events.
    ///
    /// A command-level failure is returned as a response with `success == false`.
    pub async fn read_response(&mut self, id: &str) -> Result<PiRpcResponse> {
        if let Some(response) = self.responses.remove(id) {
            return Ok(response);
        }
        if !self.in_flight.contains(id) {
            return Err(AgentError::Protocol(format!(
                "Pi RPC request ID is not pending: {id}"
            )));
        }
        loop {
            match self.next_record().await? {
                Some(RpcRecord::Response(response)) if response.id == id => {
                    self.in_flight.remove(id);
                    return Ok(response);
                }
                Some(RpcRecord::Response(response)) => self.store_response(response),
                Some(RpcRecord::Event(event)) => self.queue_event(event),
                Some(RpcRecord::ProtocolError(message)) => {
                    let event = self.record_protocol_error(message);
                    self.events.push_back(event);
                }
                Some(RpcRecord::Exited) => {
                    self.in_flight.remove(id);
                    return Err(AgentError::Protocol(format!(
                        "Pi exited before response for request {id}"
                    )));
                }
                None => return Err(SupervisorError::NoExit.into()),
            }
        }
    }

    fn store_response(&mut self, response: PiRpcResponse) {
        if self.in_flight.remove(&response.id) {
            self.responses.insert(response.id.clone(), response);
        }
    }

    fn queue_event(&mut self, event: AgentEvent) {
        let event = self.record_event(event);
        self.events.push_back(event);
    }

    async fn next_record(&mut self) -> Result<Option<RpcRecord>> {
        if self.exited {
            return Ok(None);
        }
        loop {
            match self.supervisor.next_event().await {
                Some(SupervisionEvent::StdoutLine(line)) => {
                    if let Some(record) = parse_record(&line) {
                        return Ok(Some(record));
                    }
                }
                Some(SupervisionEvent::StderrLine(_)) => {}
                Some(SupervisionEvent::Exited { code, .. }) => {
                    self.exited = true;
                    self.code = code;
                    return Ok(Some(RpcRecord::Exited));
                }
                None => return Ok(None),
            }
        }
    }

    fn outcome(&self) -> AgentExit {
        terminal_outcome(
            self.cancelled,
            self.code,
            self.turn_outcome,
            self.protocol_error.as_deref(),
        )
    }
}

fn terminal_outcome(
    cancelled: bool,
    code: Option<i32>,
    turn_outcome: Option<TurnOutcome>,
    protocol_error: Option<&str>,
) -> AgentExit {
    if cancelled {
        AgentExit::Cancelled {
            code,
            last_message: None,
        }
    } else if let Some(exit_code) = code.filter(|code| *code != 0) {
        let reason = if turn_outcome.is_none() && protocol_error.is_none() {
            "Pi RPC process exited before agent_settled".to_owned()
        } else {
            let mut reason = format!("Pi RPC exited with code {exit_code}");
            if let Some(protocol_error) = protocol_error {
                reason.push_str("; protocol violation: ");
                reason.push_str(protocol_error);
            }
            reason
        };
        AgentExit::Failed {
            code: Some(exit_code),
            reason: Some(reason),
        }
    } else if let Some(protocol_error) = protocol_error {
        AgentExit::Failed {
            code,
            reason: Some(format!("Pi RPC protocol violation: {protocol_error}")),
        }
    } else if turn_outcome == Some(TurnOutcome::Failure) {
        AgentExit::Failed {
            code,
            reason: Some("Pi RPC agent_settled reported a failed turn".to_owned()),
        }
    } else if turn_outcome.is_some() {
        AgentExit::Completed {
            code,
            last_message: None,
        }
    } else {
        AgentExit::Failed {
            code,
            reason: Some("Pi RPC process exited before agent_settled".to_owned()),
        }
    }
}

#[async_trait]
impl AgentHandle for PiRpcHandle {
    fn pid(&self) -> u32 {
        self.supervisor.pid()
    }

    async fn next_event(&mut self) -> Result<Option<AgentEvent>> {
        if let Some(event) = self.events.pop_front() {
            return Ok(Some(event));
        }
        loop {
            match self.next_record().await? {
                Some(RpcRecord::Response(response)) => self.store_response(response),
                Some(RpcRecord::Event(event)) => return Ok(Some(self.record_event(event))),
                Some(RpcRecord::ProtocolError(message)) => {
                    return Ok(Some(self.record_protocol_error(message)));
                }
                Some(RpcRecord::Exited) => {
                    return Ok(Some(AgentEvent::SessionExited {
                        exit_code: self.code,
                    }));
                }
                None => return Ok(None),
            }
        }
    }

    async fn cancel(&mut self) -> Result<()> {
        if self.cancelled || self.exited {
            return Ok(());
        }
        self.cancelled = true;
        self.stdin.close();
        self.supervisor.cancel().await;
        Ok(())
    }

    async fn wait(&mut self) -> Result<AgentExit> {
        if self.exited {
            return Ok(self.outcome());
        }
        self.stdin.close();
        while let Some(event) = self.next_event().await? {
            if matches!(event, AgentEvent::SessionExited { .. }) {
                return Ok(self.outcome());
            }
        }
        Err(SupervisorError::NoExit.into())
    }
}

impl PiRpcHandle {
    fn record_event(&mut self, event: AgentEvent) -> AgentEvent {
        if let AgentEvent::TurnFinished { outcome } = event {
            let outcome = if self.protocol_error.is_some() {
                TurnOutcome::Failure
            } else {
                outcome
            };
            self.turn_outcome = Some(outcome);
            AgentEvent::TurnFinished { outcome }
        } else {
            event
        }
    }

    fn record_protocol_error(&mut self, message: String) -> AgentEvent {
        if self.protocol_error.is_none() {
            self.protocol_error = Some(message.clone());
        }
        AgentEvent::ProtocolError { message }
    }
}

enum RpcRecord {
    Response(PiRpcResponse),
    Event(AgentEvent),
    ProtocolError(String),
    Exited,
}

fn parse_record(line: &str) -> Option<RpcRecord> {
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => {
            return Some(RpcRecord::ProtocolError(format!("malformed JSON: {error}")));
        }
    };
    let object = value.as_object()?;
    let record_type = object.get("type")?.as_str()?;
    if record_type == "response" {
        return parse_response(object).map(RpcRecord::Response);
    }
    event_from_record(record_type, object).map(RpcRecord::Event)
}

fn parse_response(object: &Map<String, Value>) -> Option<PiRpcResponse> {
    Some(PiRpcResponse {
        id: object.get("id")?.as_str()?.to_owned(),
        command: object.get("command")?.as_str()?.to_owned(),
        success: object.get("success")?.as_bool()?,
        data: object.get("data").cloned(),
        error: object
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

fn event_from_record(record_type: &str, object: &Map<String, Value>) -> Option<AgentEvent> {
    match record_type {
        "message_update" => {
            let update = object.get("assistantMessageEvent")?.as_object()?;
            if update.get("type")?.as_str()? == "text_delta" {
                Some(AgentEvent::MessageDelta {
                    text: update.get("delta")?.as_str()?.to_owned(),
                })
            } else {
                None
            }
        }
        "tool_execution_start" => Some(AgentEvent::ToolStarted {
            tool_id: object.get("toolCallId")?.as_str()?.to_owned(),
            name: object.get("toolName")?.as_str()?.to_owned(),
        }),
        "tool_execution_end" => Some(AgentEvent::ToolFinished {
            tool_id: object.get("toolCallId")?.as_str()?.to_owned(),
            success: !object.get("isError")?.as_bool()?,
        }),
        "agent_settled" => Some(AgentEvent::TurnFinished {
            outcome: if object.get("aborted")?.as_bool()? {
                TurnOutcome::Failure
            } else {
                TurnOutcome::Success
            },
        }),
        _ => None,
    }
}

fn ensure_rpc_mode(args: &mut Vec<String>) -> Result<()> {
    let mut found_mode = false;
    let options_end = args
        .iter()
        .position(|argument| argument == "--")
        .unwrap_or(args.len());
    let mut index = 0;
    while index < options_end {
        if args[index] == "--mode" {
            let mode = args
                .get(index + 1)
                .filter(|_| index + 1 < options_end)
                .ok_or_else(|| AgentError::Protocol("Pi --mode requires a value".to_owned()))?;
            if mode != "rpc" {
                return Err(AgentError::Protocol(
                    "Pi RPC adapter requires --mode rpc".to_owned(),
                ));
            }
            found_mode = true;
            index += 2;
        } else if let Some(mode) = args[index].strip_prefix("--mode=") {
            if mode != "rpc" {
                return Err(AgentError::Protocol(
                    "Pi RPC adapter requires --mode rpc".to_owned(),
                ));
            }
            found_mode = true;
            index += 1;
        } else {
            index += 1;
        }
    }
    if !found_mode {
        args.insert(0, "rpc".to_owned());
        args.insert(0, "--mode".to_owned());
    }
    Ok(())
}

async fn read_cli_version(program: &Path, working_dir: &Path) -> Result<String> {
    let mut supervisor = ChildSupervisor::launch(&LaunchRequest {
        program: program.to_path_buf(),
        args: vec!["--version".to_owned()],
        working_dir: working_dir.to_path_buf(),
    })?;
    let mut version = None;
    let code = loop {
        match supervisor.next_event().await {
            Some(SupervisionEvent::StdoutLine(line)) => {
                if version.is_none() && !line.trim().is_empty() {
                    version = Some(line.trim().to_owned());
                }
            }
            Some(SupervisionEvent::StderrLine(_)) => {}
            Some(SupervisionEvent::Exited { code, .. }) => break code,
            None => return Err(SupervisorError::NoExit.into()),
        }
    };
    if code != Some(0) {
        return Err(AgentError::Protocol(format!(
            "Pi --version exited with code {code:?}"
        )));
    }
    version.ok_or_else(|| AgentError::Protocol("Pi --version returned no version".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_json_is_recoverable_and_fails_settled_turn() {
        let Some(RpcRecord::ProtocolError(message)) = parse_record("not JSON") else {
            panic!("malformed JSON must produce a recoverable protocol-error record");
        };
        assert!(message.contains("malformed JSON"), "message: {message}");

        let exit = terminal_outcome(false, Some(0), Some(TurnOutcome::Success), Some(&message));
        match exit {
            AgentExit::Failed {
                code: Some(0),
                reason: Some(reason),
            } => {
                assert!(reason.contains("protocol violation"), "reason: {reason}");
                assert!(reason.contains("malformed JSON"), "reason: {reason}");
            }
            other => panic!("expected protocol failure after settlement, got {other:?}"),
        }
    }
    #[test]
    fn aborted_turn_with_clean_exit_is_not_completed() {
        let exit = terminal_outcome(false, Some(0), Some(TurnOutcome::Failure), None);
        assert!(
            matches!(
                exit,
                AgentExit::Failed {
                    code: Some(0),
                    reason: Some(_)
                }
            ),
            "exit: {exit:?}"
        );
    }

    #[test]
    fn nonzero_exit_after_settled_turn_fails() {
        let exit = terminal_outcome(false, Some(7), Some(TurnOutcome::Success), None);
        assert!(
            matches!(
                &exit,
                AgentExit::Failed {
                    code: Some(7),
                    reason: Some(reason)
                } if reason.contains("code 7")
            ),
            "exit: {exit:?}"
        );
    }
}
