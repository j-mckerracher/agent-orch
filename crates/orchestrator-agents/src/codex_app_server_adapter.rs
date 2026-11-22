//! Codex `app-server` interactive adapter (DPES-1183).
//!
//! One long-lived `codex app-server --listen stdio://` process speaks
//! newline-delimited JSON-RPC. The caller starts or resumes a thread, sends
//! turns, receives plan proposals, and answers command and file-change
//! approvals. Plan mode is gated behind the `experimentalApi` capability
//! that the handshake requests.
//!
//! Spec decisions taken here, pending owner confirmation (DPES-1183 Q11, Q13,
//! Q14, Q15, Q16): see the doc comment on each item.

use crate::contract::{
    AgentCapabilities, AgentError, AgentEvent, AgentExit, AgentHandle, AgentLaunchRequest,
    ApprovalDecision, InteractiveAgentHandle, Result, TurnOutcome,
};
use async_trait::async_trait;
use orchestrator_runtime::{
    ChildSupervisor, LaunchRequest, StdinWriter, SupervisionEvent, SupervisorError,
};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

/// Parent variables Codex needs beyond the baseline allowlist. Mirrors
/// `codex_adapter::PROVIDER_ENV`, which is private to that module.
const PROVIDER_ENV: &[&str] = &[
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "OPENAI_ORG_ID",
    "OPENAI_PROJECT",
    "CODEX_HOME",
];

/// Launches `codex app-server` for interactive, plan-capable sessions.
#[derive(Debug, Clone, Copy, Default)]
pub struct CodexAppServerAdapter;

impl CodexAppServerAdapter {
    pub fn new() -> Self {
        Self
    }

    /// Capabilities of an app-server session.
    pub fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            streaming: true,
            interactive_input: true,
            structured_output: true,
            cancellation: true,
            session_resumption: true,
            approvals: true,
            model_selection: true,
        }
    }

    /// Start `codex app-server`, complete the handshake, then start or resume a thread.
    ///
    /// `request.program` is the executable. `request.args` is ignored because
    /// the protocol fixes argv to `app-server --listen stdio://`. The first
    /// event is `SessionStarted`.
    pub async fn spawn_interactive(
        &self,
        request: AgentLaunchRequest,
        model: &str,
        resume: Option<String>,
    ) -> Result<CodexAppServerHandle> {
        let cwd = request.working_dir.to_string_lossy().into_owned();
        let (supervisor, stdin) = ChildSupervisor::launch_with_stdin(&LaunchRequest {
            git_guard: request.git_guard,
            program: request.program,
            args: vec![
                "app-server".to_owned(),
                "--listen".to_owned(),
                "stdio://".to_owned(),
            ],
            working_dir: request.working_dir,
            env_names: PROVIDER_ENV.iter().map(|name| (*name).to_owned()).collect(),
        })?;
        let mut handle = CodexAppServerHandle::new(supervisor, stdin, model.to_owned());

        handle
            .rpc(
                "initialize",
                json!({
                    "capabilities": {"experimentalApi": true},
                    "clientInfo": {"name": "orchestrator-agents", "version": "0.0.0"},
                }),
            )
            .await?;
        handle.write(json!({"method": "initialized"})).await?;

        let thread = match resume {
            Some(thread_id) => {
                handle
                    .rpc("thread/resume", json!({"cwd": cwd, "threadId": thread_id}))
                    .await?
            }
            // Q11 default: the plan phase runs read-only, with on-request approvals.
            None => {
                handle
                    .rpc(
                        "thread/start",
                        json!({
                            "approvalPolicy": "on-request",
                            "cwd": cwd,
                            "model": model,
                            "sandbox": "read-only",
                        }),
                    )
                    .await?
            }
        };
        let thread_id = thread["thread"]["id"]
            .as_str()
            .ok_or_else(|| AgentError::Protocol("thread response has no thread.id".to_owned()))?;
        handle.thread_id = thread_id.to_owned();
        let first = match Uuid::parse_str(thread_id) {
            Ok(session_id) => AgentEvent::SessionStarted { session_id },
            Err(_) => handle.record_protocol_error(format!("thread id is not a UUID: {thread_id}")),
        };
        handle.events.push_back(first);
        Ok(handle)
    }
}

/// One live `codex app-server` process bound to one thread.
pub struct CodexAppServerHandle {
    supervisor: ChildSupervisor,
    stdin: StdinWriter,
    next_id: u64,
    model: String,
    thread_id: String,
    current_turn_id: Option<String>,
    plan_mode: bool,
    /// Approval ids issued to the caller and not yet answered.
    pending: HashMap<Uuid, PendingAnswer>,
    /// Stdout records read while waiting for a response. They are translated
    /// in `next_event`, so a request that races a response is never lost.
    inbox: VecDeque<Inbound>,
    /// Events that have no wire record (`SessionStarted`).
    events: VecDeque<AgentEvent>,
    turn_outcome: Option<TurnOutcome>,
    protocol_error: Option<String>,
    cancelled: bool,
    exited: bool,
    exit_code: Option<i32>,
}

enum Inbound {
    Line(String),
    Exited(Option<i32>),
}

enum PendingAnswer {
    /// Answered with a JSON-RPC response to this server request id.
    Request(Value),
    /// A proposed plan. Codex sends no request for it, so answering writes nothing
    /// (Q16 default). The next `send_turn` carries the caller's decision.
    Plan,
}

impl CodexAppServerHandle {
    fn new(supervisor: ChildSupervisor, stdin: StdinWriter, model: String) -> Self {
        Self {
            supervisor,
            stdin,
            next_id: 1,
            model,
            thread_id: String::new(),
            current_turn_id: None,
            // The handle starts in the plan phase, matching the read-only thread/start.
            plan_mode: true,
            pending: HashMap::new(),
            inbox: VecDeque::new(),
            events: VecDeque::new(),
            turn_outcome: None,
            protocol_error: None,
            cancelled: false,
            exited: false,
            exit_code: None,
        }
    }

    /// Choose the collaboration mode for the next `send_turn`. Plan mode is the default.
    pub fn set_plan_mode(&mut self, plan_mode: bool) {
        self.plan_mode = plan_mode;
    }

    /// Send a request and wait for its response. Other records are queued.
    async fn rpc(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.write(json!({"id": id, "method": method, "params": params}))
            .await?;
        loop {
            match self.read_supervisor().await {
                Some(Inbound::Line(line)) => {
                    if let Some(answer) = answer_to(&line, id) {
                        return answer.map_err(|message| {
                            AgentError::Protocol(format!("{method} failed: {message}"))
                        });
                    }
                    self.inbox.push_back(Inbound::Line(line));
                }
                Some(Inbound::Exited(code)) => {
                    self.inbox.push_back(Inbound::Exited(code));
                    return Err(AgentError::Protocol(format!(
                        "app-server exited before answering {method}"
                    )));
                }
                None => {
                    return Err(AgentError::Protocol(format!(
                        "app-server closed its output before answering {method}"
                    )));
                }
            }
        }
    }

    async fn write(&mut self, frame: Value) -> Result<()> {
        self.stdin
            .write_line(&frame.to_string())
            .await
            .map_err(|error| {
                AgentError::Protocol(format!("failed to write to app-server: {error}"))
            })
    }

    /// The next stdout record or exit from the supervisor. Stderr is skipped.
    async fn read_supervisor(&mut self) -> Option<Inbound> {
        loop {
            match self.supervisor.next_event().await? {
                SupervisionEvent::StdoutLine(line) => return Some(Inbound::Line(line)),
                SupervisionEvent::StderrLine(_) => {}
                SupervisionEvent::Exited { code, .. } => return Some(Inbound::Exited(code)),
            }
        }
    }

    fn is_current_turn(&self, turn_id: Option<&str>) -> bool {
        turn_id.is_some() && turn_id == self.current_turn_id.as_deref()
    }

    /// Translate one stdout record into a canonical event. `None` means the record is ignored.
    fn translate(&mut self, line: &str) -> Option<AgentEvent> {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            return Some(
                self.record_protocol_error(format!("app-server wrote a non-JSON line: {line}")),
            );
        };
        // A response with no pending request is not an event.
        let method = record.get("method").and_then(Value::as_str)?;
        let params = &record["params"];
        if let Some(rpc_id) = record.get("id") {
            return Some(self.server_request(method, rpc_id.clone(), params));
        }
        match method {
            "item/agentMessage/delta" => match params["delta"].as_str() {
                Some(text) => Some(AgentEvent::MessageDelta {
                    text: text.to_owned(),
                }),
                None => Some(
                    self.record_protocol_error("item/agentMessage/delta has no delta".to_owned()),
                ),
            },
            "item/completed"
                if params["item"]["type"] == "plan"
                    && self.is_current_turn(params["turnId"].as_str()) =>
            {
                let text = params["item"]["text"].as_str().unwrap_or("");
                if text.is_empty() {
                    return Some(
                        self.record_protocol_error("app-server sent an empty plan".to_owned()),
                    );
                }
                let approval_id = Uuid::new_v4();
                self.pending.insert(approval_id, PendingAnswer::Plan);
                Some(AgentEvent::PlanProposed {
                    approval_id,
                    plan: text.to_owned(),
                })
            }
            // Only `completed` is Success. The spec maps any other status to Failure.
            "turn/completed" if self.is_current_turn(params["turn"]["id"].as_str()) => {
                self.current_turn_id = None;
                let outcome = if params["turn"]["status"] == "completed" {
                    TurnOutcome::Success
                } else {
                    TurnOutcome::Failure
                };
                self.turn_outcome = Some(outcome);
                Some(AgentEvent::TurnFinished { outcome })
            }
            "error" => {
                let message = params["error"]["message"]
                    .as_str()
                    .unwrap_or("app-server error")
                    .to_owned();
                Some(self.record_protocol_error(message))
            }
            _ => None,
        }
    }

    /// Only the two approval requests are answered. Any other server request is
    /// reported as a protocol error and never answered (Q15 default), so the
    /// server may stall the turn until cancel.
    fn server_request(&mut self, method: &str, rpc_id: Value, params: &Value) -> AgentEvent {
        let description = match method {
            "item/commandExecution/requestApproval" => command_description(params),
            "item/fileChange/requestApproval" => file_change_description(params),
            _ => {
                return self
                    .record_protocol_error(format!("unsupported app-server request: {method}"))
            }
        };
        let approval_id = Uuid::new_v4();
        self.pending
            .insert(approval_id, PendingAnswer::Request(rpc_id));
        AgentEvent::ApprovalRequested {
            approval_id,
            description,
        }
    }

    fn record_protocol_error(&mut self, message: String) -> AgentEvent {
        if self.protocol_error.is_none() {
            self.protocol_error = Some(message.clone());
        }
        AgentEvent::ProtocolError { message }
    }

    /// Terminal outcome. A turn's own status decides `Completed` or `Failed`.
    /// A protocol error or a nonzero exit fails the run, as in the Pi adapter.
    fn outcome(&self) -> AgentExit {
        let code = self.exit_code;
        if self.cancelled {
            return AgentExit::Cancelled {
                code,
                last_message: None,
            };
        }
        let reason = if let Some(code) = code.filter(|code| *code != 0) {
            format!("app-server exited with code {code}")
        } else if let Some(message) = &self.protocol_error {
            format!("app-server protocol violation: {message}")
        } else if self.turn_outcome == Some(TurnOutcome::Failure) {
            "app-server reported a failed turn".to_owned()
        } else if self.turn_outcome == Some(TurnOutcome::Success) {
            return AgentExit::Completed {
                code,
                last_message: None,
            };
        } else {
            "app-server exited before a turn completed".to_owned()
        };
        AgentExit::Failed {
            code,
            reason: Some(reason),
        }
    }
}

/// The result of the response to request `id`, or `None` for any other record.
fn answer_to(line: &str, id: u64) -> Option<std::result::Result<Value, String>> {
    let record: Value = serde_json::from_str(line).ok()?;
    if record.get("method").is_some() || record.get("id") != Some(&json!(id)) {
        return None;
    }
    Some(match record.get("error") {
        Some(error) => Err(error["message"]
            .as_str()
            .unwrap_or("unknown error")
            .to_owned()),
        None => Ok(record.get("result").cloned().unwrap_or(Value::Null)),
    })
}

/// Q14 default: the command, with the agent's reason when it gives one.
fn command_description(params: &Value) -> String {
    let command = params["command"].as_str().unwrap_or("command execution");
    match params["reason"].as_str() {
        Some(reason) => format!("{command} ({reason})"),
        None => command.to_owned(),
    }
}

/// Q14 default: the agent's reason, or a generic label.
fn file_change_description(params: &Value) -> String {
    params["reason"]
        .as_str()
        .unwrap_or("file changes")
        .to_owned()
}

#[async_trait]
impl AgentHandle for CodexAppServerHandle {
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
        loop {
            let inbound = match self.inbox.pop_front() {
                Some(inbound) => inbound,
                None if self.exited => return Ok(None),
                None => match self.read_supervisor().await {
                    Some(inbound) => inbound,
                    None => return Ok(None),
                },
            };
            match inbound {
                Inbound::Line(line) => {
                    if let Some(event) = self.translate(&line) {
                        return Ok(Some(event));
                    }
                }
                Inbound::Exited(code) => {
                    self.exited = true;
                    self.exit_code = code;
                    return Ok(Some(AgentEvent::SessionExited { exit_code: code }));
                }
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
        if !self.exited {
            self.stdin.close();
            while let Some(event) = self.next_event().await? {
                if matches!(event, AgentEvent::SessionExited { .. }) {
                    break;
                }
            }
        }
        if !self.exited {
            return Err(SupervisorError::NoExit.into());
        }
        Ok(self.outcome())
    }
}

#[async_trait]
impl InteractiveAgentHandle for CodexAppServerHandle {
    async fn send_turn(&mut self, prompt: &str) -> Result<()> {
        if self.exited {
            return Err(AgentError::Protocol("app-server has exited".to_owned()));
        }
        if self.current_turn_id.is_some() {
            return Err(AgentError::Protocol("turn still running".to_owned()));
        }
        // Q13 default: a non-plan turn sends mode "default" explicitly. The mode
        // persists on the thread (spec 5.12), so omitting it would leave a
        // resumed plan thread in plan mode.
        let mode = if self.plan_mode { "plan" } else { "default" };
        let model = self.model.clone();
        let response = self
            .rpc(
                "turn/start",
                json!({
                    "collaborationMode": {"mode": mode, "settings": {"model": model}},
                    "input": [{"text": prompt, "text_elements": [], "type": "text"}],
                    "model": model,
                    // Q11 default: every turn is read-only. No write policy is sent until the owner decides.
                    "sandboxPolicy": {"networkAccess": false, "type": "readOnly"},
                    "threadId": self.thread_id,
                }),
            )
            .await?;
        let turn_id = response["turn"]["id"]
            .as_str()
            .ok_or_else(|| AgentError::Protocol("turn/start response has no turn.id".to_owned()))?;
        self.current_turn_id = Some(turn_id.to_owned());
        Ok(())
    }

    async fn respond_approval(
        &mut self,
        approval_id: Uuid,
        decision: ApprovalDecision,
    ) -> Result<()> {
        match self.pending.remove(&approval_id) {
            None => Err(AgentError::Protocol(format!(
                "unknown or already answered approval id {approval_id}"
            ))),
            Some(PendingAnswer::Plan) => Ok(()),
            Some(PendingAnswer::Request(rpc_id)) => {
                let decision = match decision {
                    ApprovalDecision::Approve => "accept",
                    // Codex's approval response has no reason field, so `reason` is not sent.
                    ApprovalDecision::Deny { .. } => "decline",
                };
                self.write(json!({"id": rpc_id, "result": {"decision": decision}}))
                    .await
            }
        }
    }
}
