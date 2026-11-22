//! GitHub Copilot ACP interactive adapter.
//!
//! Runs `copilot --acp` as one long-lived process speaking NDJSON JSON-RPC 2.0
//! over stdio (Agent Client Protocol, protocolVersion 1). The handle owns the
//! child's sole stdin writer and correlates responses by request id. Permission
//! requests surface as approvals. Process exits remain transport facts; they
//! never establish task success.
//!
//! Wire frames were recorded from Copilot CLI 1.0.114 (DPES-1182 spike).
//! Updated: 2026-11-21.

use async_trait::async_trait;
use orchestrator_runtime::{
    ChildSupervisor, LaunchRequest, StdinWriter, SupervisionEvent, SupervisorError,
};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

use crate::contract::{
    AgentCapabilities, AgentError, AgentEvent, AgentExit, AgentHandle, AgentLaunchRequest,
    ApprovalDecision, InteractiveAgentHandle, Result, TurnOutcome,
};

/// Starts Copilot in ACP mode. Stateless; each spawn owns its own child.
#[derive(Debug, Default, Clone, Copy)]
pub struct CopilotAcpAdapter;

impl CopilotAcpAdapter {
    /// Capabilities of `copilot --acp`.
    ///
    /// Inherent rather than `RoutableAdapter`: `AgentAdapter::spawn` cannot carry
    /// a model or a resume id, so routing wiring waits for that.
    pub fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            streaming: true,          // next_event decodes session/update notifications.
            interactive_input: true,  // send_turn writes session/prompt on the live session.
            structured_output: true,  // every frame is a JSON-RPC 2.0 object with typed fields.
            cancellation: true,       // cancel closes stdin and kills the process tree.
            session_resumption: true, // resume sends session/load when loadSession is advertised.
            approvals: true,          // session/request_permission surfaces as ApprovalRequested.
            model_selection: true,    // spawn_interactive pins the model before any prompt.
        }
    }

    /// Start `copilot --acp`, initialize, open or load a session, and pin `model`.
    ///
    /// The request argument array gets `--acp` added when absent. The prompt never
    /// travels in argv.
    pub async fn spawn_interactive(
        &self,
        mut request: AgentLaunchRequest,
        model: &str,
        resume: Option<String>,
    ) -> Result<CopilotAcpHandle> {
        if !request.args.iter().any(|argument| argument == "--acp") {
            request.args.insert(0, "--acp".to_owned());
        }
        let cwd = request.working_dir.to_string_lossy().into_owned();
        let launch = LaunchRequest {
            git_guard: request.git_guard,
            program: request.program,
            args: request.args,
            working_dir: request.working_dir,
            // The shared baseline only: Copilot takes no provider variable.
            env_names: Vec::new(),
        };
        let (supervisor, stdin) = ChildSupervisor::launch_with_stdin(&launch)?;
        let mut handle = CopilotAcpHandle::new(supervisor, stdin);

        let init = handle
            .request(
                "initialize",
                json!({
                    "protocolVersion": 1,
                    "clientCapabilities": {
                        "fs": {"readTextFile": false, "writeTextFile": false},
                        "terminal": false,
                    },
                    "clientInfo": {"name": "orchestrator-agents", "version": "0.0.0"},
                }),
            )
            .await?;
        let (session_id, result) = match resume {
            Some(session_id) => {
                let load_session = init
                    .pointer("/agentCapabilities/loadSession")
                    .and_then(Value::as_bool)
                    == Some(true);
                if !load_session {
                    return Err(AgentError::Unavailable(
                        "Copilot ACP did not advertise loadSession".to_owned(),
                    ));
                }
                let params = json!({"sessionId": session_id, "cwd": cwd, "mcpServers": []});
                let result = handle.request("session/load", params).await?;
                (session_id, result)
            }
            None => {
                let result = handle
                    .request("session/new", json!({"cwd": cwd, "mcpServers": []}))
                    .await?;
                let session_id = result["sessionId"]
                    .as_str()
                    .ok_or_else(|| {
                        AgentError::Protocol("session/new result has no sessionId".to_owned())
                    })?
                    .to_owned();
                (session_id, result)
            }
        };
        let uuid = Uuid::parse_str(&session_id).map_err(|error| {
            AgentError::Protocol(format!("Copilot session id is not a UUID: {error}"))
        })?;

        handle.session_id = session_id.clone();
        handle.mode_ids = result
            .pointer("/modes/availableModes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|mode| mode["id"].as_str().map(str::to_owned))
            .collect();
        handle
            .request(
                "session/set_config_option",
                json!({"sessionId": session_id, "configId": "model", "value": model}),
            )
            .await?;
        // Queued events from the handshake (for example history replay on load) follow.
        handle
            .events
            .push_front(AgentEvent::SessionStarted { session_id: uuid });
        Ok(handle)
    }
}

/// A live `copilot --acp` session. Owns the child's sole stdin writer.
pub struct CopilotAcpHandle {
    supervisor: ChildSupervisor,
    stdin: StdinWriter,
    next_id: i64,
    session_id: String,
    mode_ids: Vec<String>,
    /// Request id of the in-flight `session/prompt`, if a turn is running.
    prompt_id: Option<i64>,
    /// Issued approval id to the JSON-RPC id of the server's permission request.
    pending_permissions: HashMap<Uuid, i64>,
    events: VecDeque<AgentEvent>,
    turn_outcome: Option<TurnOutcome>,
    cancelled: bool,
    exited: bool,
    code: Option<i32>,
}

impl CopilotAcpHandle {
    fn new(supervisor: ChildSupervisor, stdin: StdinWriter) -> Self {
        Self {
            supervisor,
            stdin,
            next_id: 1,
            session_id: String::new(),
            mode_ids: Vec::new(),
            prompt_id: None,
            pending_permissions: HashMap::new(),
            events: VecDeque::new(),
            turn_outcome: None,
            cancelled: false,
            exited: false,
            code: None,
        }
    }

    /// Switch to the plan session mode. The runtime enforces plan mode.
    pub async fn set_plan_mode(&mut self) -> Result<()> {
        self.set_mode("#plan").await
    }

    /// Switch back to agent mode, for the execute turn after a plan.
    pub async fn set_agent_mode(&mut self) -> Result<()> {
        self.set_mode("#agent").await
    }

    async fn set_mode(&mut self, suffix: &str) -> Result<()> {
        if self.prompt_id.is_some() {
            return Err(AgentError::Protocol(
                "cannot change mode while a turn is running".to_owned(),
            ));
        }
        let mode_id = self
            .mode_ids
            .iter()
            .find(|id| id.ends_with(suffix))
            .cloned()
            .ok_or_else(|| {
                AgentError::Protocol(format!(
                    "Copilot advertised no session mode ending {suffix}"
                ))
            })?;
        self.request(
            "session/set_mode",
            json!({"sessionId": self.session_id, "modeId": mode_id}),
        )
        .await?;
        Ok(())
    }

    fn next_request_id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    async fn send(&mut self, frame: Value) -> Result<()> {
        self.stdin.write_line(&frame.to_string()).await?;
        Ok(())
    }

    /// Send a request and wait for its response. Notifications that arrive first are queued.
    async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_request_id();
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await?;
        loop {
            let message = self.read_message(method).await?;
            if message.get("method").is_none() && message.get("id") == Some(&json!(id)) {
                return result_of(message);
            }
            if let Some(event) = self.translate(message) {
                self.events.push_back(event);
            }
        }
    }

    /// Next stdout frame during a request. Malformed JSON here is a protocol error.
    async fn read_message(&mut self, awaiting: &str) -> Result<Value> {
        loop {
            match self.supervisor.next_event().await {
                Some(SupervisionEvent::StdoutLine(line)) => {
                    return serde_json::from_str(&line).map_err(|error| {
                        AgentError::Protocol(format!("malformed JSON from copilot --acp: {error}"))
                    });
                }
                Some(SupervisionEvent::StderrLine(_)) => {}
                Some(SupervisionEvent::Exited { code, .. }) => {
                    self.exited = true;
                    self.code = code;
                    self.events
                        .push_back(AgentEvent::SessionExited { exit_code: code });
                    return Err(AgentError::Protocol(format!(
                        "copilot --acp exited before answering {awaiting}"
                    )));
                }
                None => return Err(SupervisorError::NoExit.into()),
            }
        }
    }

    /// Turn one inbound frame into a canonical event, if it maps to one.
    fn translate(&mut self, message: Value) -> Option<AgentEvent> {
        match message.get("method").and_then(Value::as_str) {
            Some("session/request_permission") => Some(self.approval_request(&message)),
            Some("session/update") => session_update_event(&message["params"]["update"]),
            // ponytail: other server requests are ignored. fs and terminal are advertised false.
            Some(_) => None,
            None => self.prompt_finished(&message),
        }
    }

    fn approval_request(&mut self, message: &Value) -> AgentEvent {
        let Some(jsonrpc_id) = message.get("id").and_then(Value::as_i64) else {
            return AgentEvent::ProtocolError {
                message: "session/request_permission without an integer id".to_owned(),
            };
        };
        let tool_call = &message["params"]["toolCall"];
        let approval_id = Uuid::new_v4();
        self.pending_permissions.insert(approval_id, jsonrpc_id);
        AgentEvent::ApprovalRequested {
            approval_id,
            description: format!(
                "{}: {}",
                text(&tool_call["kind"]),
                text(&tool_call["title"])
            ),
        }
    }

    fn prompt_finished(&mut self, message: &Value) -> Option<AgentEvent> {
        let prompt_id = self.prompt_id?;
        if message.get("id").and_then(Value::as_i64) != Some(prompt_id) {
            return None;
        }
        self.prompt_id = None;
        // A JSON-RPC error has no result, so it fails the turn instead of hanging the caller.
        let end_turn = message
            .pointer("/result/stopReason")
            .and_then(Value::as_str)
            == Some("end_turn");
        let outcome = if end_turn {
            TurnOutcome::Success
        } else {
            TurnOutcome::Failure
        };
        self.turn_outcome = Some(outcome);
        Some(AgentEvent::TurnFinished { outcome })
    }

    fn outcome(&self) -> AgentExit {
        if self.cancelled {
            return AgentExit::Cancelled {
                code: self.code,
                last_message: None,
            };
        }
        match (self.code, self.turn_outcome) {
            (Some(code), _) if code != 0 => AgentExit::Failed {
                code: Some(code),
                reason: Some(format!("copilot --acp exited with code {code}")),
            },
            (_, Some(TurnOutcome::Failure)) => AgentExit::Failed {
                code: self.code,
                reason: Some("Copilot ACP turn ended with a failure".to_owned()),
            },
            (_, Some(TurnOutcome::Success)) => AgentExit::Completed {
                code: self.code,
                last_message: None,
            },
            _ => AgentExit::Failed {
                code: self.code,
                reason: Some("copilot --acp exited before a turn finished".to_owned()),
            },
        }
    }
}

#[async_trait]
impl AgentHandle for CopilotAcpHandle {
    fn pid(&self) -> u32 {
        self.supervisor.pid()
    }

    fn try_exited(&self) -> std::io::Result<bool> {
        self.supervisor.try_exited()
    }

    async fn next_event(&mut self) -> Result<Option<AgentEvent>> {
        if let Some(event) = self.events.pop_front() {
            return Ok(Some(event));
        }
        if self.exited {
            return Ok(None);
        }
        loop {
            match self.supervisor.next_event().await {
                Some(SupervisionEvent::StdoutLine(line)) => {
                    let event = match serde_json::from_str::<Value>(&line) {
                        Ok(message) => self.translate(message),
                        Err(error) => Some(AgentEvent::ProtocolError {
                            message: format!("malformed JSON: {error}"),
                        }),
                    };
                    if let Some(event) = event {
                        return Ok(Some(event));
                    }
                }
                Some(SupervisionEvent::StderrLine(_)) => {}
                Some(SupervisionEvent::Exited { code, .. }) => {
                    self.exited = true;
                    self.code = code;
                    return Ok(Some(AgentEvent::SessionExited { exit_code: code }));
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

#[async_trait]
impl InteractiveAgentHandle for CopilotAcpHandle {
    async fn send_turn(&mut self, prompt: &str) -> Result<()> {
        // try_exited catches a child that died before its exit event was read.
        if self.exited || self.supervisor.try_exited().unwrap_or(false) {
            return Err(AgentError::Protocol(
                "copilot --acp session already exited".to_owned(),
            ));
        }
        if self.prompt_id.is_some() {
            return Err(AgentError::Protocol("a turn is already running".to_owned()));
        }
        let id = self.next_request_id();
        let frame = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "session/prompt",
            "params": {
                "sessionId": self.session_id,
                "prompt": [{"type": "text", "text": prompt}],
            },
        });
        self.send(frame).await?;
        self.prompt_id = Some(id);
        Ok(())
    }

    async fn respond_approval(
        &mut self,
        approval_id: Uuid,
        decision: ApprovalDecision,
    ) -> Result<()> {
        let jsonrpc_id = self
            .pending_permissions
            .remove(&approval_id)
            .ok_or_else(|| {
                AgentError::Protocol(format!("approval id {approval_id} is not pending"))
            })?;
        // ponytail: ACP has no denial-reason field, so Deny { reason } sends reject_once only.
        let option_id = match decision {
            ApprovalDecision::Approve => "allow_once",
            ApprovalDecision::Deny { .. } => "reject_once",
        };
        self.send(json!({
            "jsonrpc": "2.0",
            "id": jsonrpc_id,
            "result": {"outcome": {"outcome": "selected", "optionId": option_id}},
        }))
        .await
    }
}

fn result_of(message: Value) -> Result<Value> {
    match message.get("error") {
        Some(error) => Err(AgentError::Protocol(format!(
            "Copilot ACP request failed: {error}"
        ))),
        None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
    }
}

fn session_update_event(update: &Value) -> Option<AgentEvent> {
    match update["sessionUpdate"].as_str()? {
        "agent_message_chunk" => Some(AgentEvent::MessageDelta {
            text: text(&update["content"]["text"]),
        }),
        "agent_thought_chunk" => Some(AgentEvent::ThinkingDelta {
            text: text(&update["content"]["text"]),
        }),
        "tool_call" => Some(AgentEvent::ToolStarted {
            tool_id: text(&update["toolCallId"]),
            name: text(&update["kind"]),
            summary: text(&update["title"]),
        }),
        "tool_call_update" => {
            let success = match update["status"].as_str()? {
                "completed" => true,
                "failed" => false,
                _ => return None,
            };
            Some(AgentEvent::ToolFinished {
                tool_id: text(&update["toolCallId"]),
                success,
                summary: first_text(&update["content"]),
            })
        }
        // Unknown updates (plan, usage, commands, config) are ignored by design.
        _ => None,
    }
}

/// The first `content[].content.text` block, or empty when there is none.
fn first_text(content: &Value) -> String {
    content
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|item| item.pointer("/content/text").and_then(Value::as_str))
        .unwrap_or_default()
        .to_owned()
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}
