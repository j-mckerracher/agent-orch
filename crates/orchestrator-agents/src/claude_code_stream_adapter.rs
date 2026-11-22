//! Claude Code stream-json adapter for interactive, plan-gated sessions.
//!
//! One long-lived `claude` process reads user turns as stream-json on stdin and
//! reports tool approvals and plans as `can_use_tool` control requests on stdout.
//! The one-shot `ClaudeCodeAdapter` is unchanged. Argv is fixed here, so
//! `AgentLaunchRequest::args` is ignored. Only Deny answers approvals; Approve is
//! refused until its allow frame is verified on the raw wire.
//!
//! A protocol violation does not end the session. `StreamParser` latches on its
//! first violation, so it is reset after each one, and the first violation is
//! kept as the failure reason in the terminal `AgentExit`.

use async_trait::async_trait;
use orchestrator_runtime::{
    ChildSupervisor, LaunchRequest, StdinWriter, SupervisionEvent, SupervisorError,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

use crate::claude_code_adapter::StreamParser;
use crate::contract::{
    AgentAdapter, AgentCapabilities, AgentError, AgentEvent, AgentExit, AgentHandle, AgentKind,
    AgentLaunchRequest, ApprovalDecision, InteractiveAgentHandle, Result,
};
use crate::locate_program;
use crate::routing::RoutableAdapter;
use crate::tool_summary::input_summary;

/// Argv for one interactive session, verified on CLI 2.1.311 (DPES-1181 spec 5.1).
/// `-p` is absent on purpose: the verified run omitted it, and adding it changes behavior.
const ARGV: &[&str] = &[
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
    "--permission-prompt-tool",
    "stdio",
    "--permission-mode",
    "plan",
    // An empty value stops the user's hooks from firing (spike saw 16 SessionStart hooks).
    "--setting-sources=",
];

/// First stdin frame, required before the CLI answers user turns (spec 5.2).
const INIT_FRAME: &str =
    r#"{"type":"control_request","request_id":"init-1","request":{"subtype":"initialize"}}"#;

/// Mirrors `claude_code_adapter::PROVIDER_ENV`, which is private. Keep both lists in sync.
const PROVIDER_ENV: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "CLAUDE_CONFIG_DIR",
];

/// Claude Code adapter for interactive sessions.
///
/// `spawn` returns the handle as a plain `AgentHandle`. Call `spawn_interactive`
/// to reach `send_turn` and `respond_approval`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClaudeCodeStreamAdapter;

impl ClaudeCodeStreamAdapter {
    /// Start one session. `resume` appends `--resume <id>` after the fixed argv.
    pub async fn spawn_interactive(
        &self,
        request: AgentLaunchRequest,
        resume: Option<Uuid>,
    ) -> Result<ClaudeCodeStreamHandle> {
        let mut args: Vec<String> = ARGV.iter().map(|arg| (*arg).to_owned()).collect();
        if let Some(id) = resume {
            args.push("--resume".to_owned());
            args.push(id.to_string());
        }
        let launch = LaunchRequest {
            git_guard: request.git_guard,
            program: request.program,
            args,
            working_dir: request.working_dir,
            env_names: PROVIDER_ENV.iter().map(|name| (*name).to_owned()).collect(),
        };
        let (supervisor, mut stdin) = ChildSupervisor::launch_with_stdin(&launch)?;
        stdin.write_line(INIT_FRAME).await?;
        Ok(ClaudeCodeStreamHandle {
            supervisor,
            stdin,
            parser: StreamParser::new(),
            pending: HashMap::new(),
            turn_open: false,
            violation: None,
            cancelled: false,
            exited: false,
            code: None,
        })
    }
}

impl RoutableAdapter for ClaudeCodeStreamAdapter {
    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            streaming: true,          // `next_event` reads live stdout lines.
            interactive_input: true, // `send_turn` writes user frames on stdin (DPES-1181 spec 5.3).
            structured_output: true, // Lines are stream-json, decoded by `StreamParser`.
            cancellation: true,      // `cancel` closes stdin, then cancels the supervisor.
            session_resumption: true, // `--resume <id>` (spec 5.1). Combined with ARGV unverified (Q5).
            approvals: true, // `can_use_tool` surfaces as events. Deny verified, Approve refused (Q3).
            model_selection: false, // ARGV passes no `--model`.
        }
    }
}

#[async_trait]
impl AgentAdapter for ClaudeCodeStreamAdapter {
    fn kind(&self) -> AgentKind {
        AgentKind::ClaudeCode
    }

    /// Checks that `claude` resolves on PATH, like `ClaudeCodeAdapter::detect` with no override.
    async fn detect(&self) -> Result<AgentCapabilities> {
        if locate_program(Path::new("claude")).is_none() {
            return Err(AgentError::NotFound("claude".to_owned()));
        }
        Ok(self.capabilities())
    }

    /// Ignores `request.args`, because argv is fixed (see module docs).
    async fn spawn(&self, request: AgentLaunchRequest) -> Result<Box<dyn AgentHandle>> {
        Ok(Box::new(self.spawn_interactive(request, None).await?))
    }
}

/// A live session. The handle owns the child's only stdin writer and its supervisor.
pub struct ClaudeCodeStreamHandle {
    supervisor: ChildSupervisor,
    stdin: StdinWriter,
    parser: StreamParser,
    /// Approval ids issued to the caller, mapped to the CLI's `request_id`.
    pending: HashMap<Uuid, String>,
    turn_open: bool,
    /// First protocol violation. It becomes the failure reason in `AgentExit`.
    violation: Option<String>,
    cancelled: bool,
    exited: bool,
    code: Option<i32>,
}

impl ClaudeCodeStreamHandle {
    /// Decodes one stdout line into at most one event. Blank lines produce nothing.
    fn decode(&mut self, line: &str) -> Option<AgentEvent> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) => return Some(self.protocol_error(format!("malformed JSON: {error}"))),
        };
        if value["type"].as_str() == Some("control_request") {
            return Some(self.control_request(&value));
        }
        match self.parser.feed(line)? {
            Ok(event) => {
                if matches!(event, AgentEvent::TurnFinished { .. }) {
                    self.turn_open = false;
                }
                Some(event)
            }
            Err(violation) => {
                // A malformed `result` still ends the turn. Without this the session would
                // refuse every later turn until cancel. Other malformed lines keep the turn open.
                if value["type"].as_str() == Some("result") {
                    self.turn_open = false;
                }
                // ponytail: a violation resets the parser, so `last_message` is lost for this session.
                self.parser = StreamParser::new();
                Some(self.protocol_error(violation.to_string()))
            }
        }
    }

    /// Turns a `can_use_tool` request into an approval event and records its CLI id.
    /// The CLI id is recorded only for events the caller can answer.
    fn control_request(&mut self, value: &Value) -> AgentEvent {
        let Some(cli_request_id) = value["request_id"].as_str() else {
            return self.protocol_error("control_request without request_id".to_owned());
        };
        if value["request"]["subtype"].as_str() != Some("can_use_tool") {
            return self.protocol_error("unknown control request".to_owned());
        }
        let tool_name = value["request"]["tool_name"].as_str().unwrap_or_default();
        let input = &value["request"]["input"];
        let approval_id = Uuid::new_v4();
        let event = if tool_name == "ExitPlanMode" {
            match input["plan"].as_str() {
                Some(plan) if !plan.is_empty() => AgentEvent::PlanProposed {
                    approval_id,
                    plan: plan.to_owned(),
                },
                _ => return self.protocol_error("ExitPlanMode without plan".to_owned()),
            }
        } else {
            AgentEvent::ApprovalRequested {
                approval_id,
                description: format!("{tool_name}: {}", input_summary(input)),
            }
        };
        self.pending.insert(approval_id, cli_request_id.to_owned());
        event
    }

    fn protocol_error(&mut self, message: String) -> AgentEvent {
        self.violation.get_or_insert_with(|| message.clone());
        AgentEvent::ProtocolError { message }
    }

    fn outcome(&self) -> AgentExit {
        let last_message = self.parser.last_message().map(str::to_owned);
        if self.cancelled {
            return AgentExit::Cancelled {
                code: self.code,
                last_message,
            };
        }
        if let Some(code) = self.code.filter(|code| *code != 0) {
            return AgentExit::Failed {
                code: Some(code),
                reason: Some(format!("agent exited with code {code}")),
            };
        }
        if let Some(violation) = &self.violation {
            return AgentExit::Failed {
                code: self.code,
                reason: Some(format!("protocol violation: {violation}")),
            };
        }
        if self.parser.turn_finished() {
            return AgentExit::Completed {
                code: self.code,
                last_message,
            };
        }
        AgentExit::Failed {
            code: self.code,
            reason: self.parser.turn_failure().map(str::to_owned),
        }
    }
}

/// JSON string literal with escaping. `Value` display cannot fail.
fn json_str(text: &str) -> String {
    Value::String(text.to_owned()).to_string()
}

#[async_trait]
impl AgentHandle for ClaudeCodeStreamHandle {
    fn pid(&self) -> u32 {
        self.supervisor.pid()
    }

    fn try_exited(&self) -> std::io::Result<bool> {
        self.supervisor.try_exited()
    }

    async fn next_event(&mut self) -> Result<Option<AgentEvent>> {
        loop {
            if let Some(event) = self.parser.take_pending() {
                return Ok(Some(event));
            }
            match self.supervisor.next_event().await {
                Some(SupervisionEvent::StdoutLine(line)) => {
                    if let Some(event) = self.decode(&line) {
                        return Ok(Some(event));
                    }
                }
                Some(SupervisionEvent::StderrLine(_)) => {}
                Some(SupervisionEvent::Exited { code, .. }) => {
                    self.code = code;
                    self.exited = true;
                    return Ok(Some(AgentEvent::SessionExited { exit_code: code }));
                }
                None => return Ok(None),
            }
        }
    }

    async fn cancel(&mut self) -> Result<()> {
        // A natural exit already observed terminal; do not relabel it.
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
impl InteractiveAgentHandle for ClaudeCodeStreamHandle {
    async fn send_turn(&mut self, prompt: &str) -> Result<()> {
        if self.exited || self.supervisor.try_exited().unwrap_or(true) {
            return Err(AgentError::Protocol("session ended".to_owned()));
        }
        if self.turn_open {
            return Err(AgentError::Protocol("turn still running".to_owned()));
        }
        let frame = json!({
            "type": "user",
            "message": {"role": "user", "content": prompt},
            "parent_tool_use_id": null,
            "session_id": "",
        });
        self.stdin.write_line(&frame.to_string()).await?;
        self.turn_open = true;
        Ok(())
    }

    async fn respond_approval(
        &mut self,
        approval_id: Uuid,
        decision: ApprovalDecision,
    ) -> Result<()> {
        let Some(cli_request_id) = self.pending.get(&approval_id).cloned() else {
            return Err(AgentError::Protocol("unknown approval id".to_owned()));
        };
        // Refused before any state change, so the id stays answerable with Deny.
        let ApprovalDecision::Deny { reason } = decision else {
            return Err(AgentError::Protocol("approve not yet specified".to_owned()));
        };
        // Key order is fixed by format!, not by serde_json, because the wire check depends on it.
        let frame = format!(
            r#"{{"type":"control_response","response":{{"subtype":"success","request_id":{request_id},"response":{{"behavior":"deny","message":{message}}}}}}}"#,
            request_id = json_str(&cli_request_id),
            message = json_str(&reason),
        );
        self.stdin.write_line(&frame).await?;
        self.pending.remove(&approval_id);
        Ok(())
    }
}
