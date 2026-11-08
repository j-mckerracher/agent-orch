//! Claude Code print-mode adapter using the verified `stream-json` protocol.
//!
//! The caller owns the argument array. Use `ClaudeCodeAdapter::args_for_prompt`
//! to build the fixed, restricted invocation. `spawn` passes it verbatim to the
//! runtime supervisor, so callers must not append flags that weaken its policy.
//! The adapter translates stdout only. Stderr stays diagnostic, and process
//! exit codes remain transport facts rather than task results. Permission
//! prompts are disabled, and only `Read`, `Edit`, and `Write` are exposed by
//! the helper. `--restricted` confines file tools to the caller's working
//! directory. Each handle owns one parser, and mutable handle calls consume
//! lines sequentially without shared parser state.
//!
//! Detection checks only that the executable exists. Capabilities describe
//! this fixed invocation, not runtime model or plugin discovery.
//!
//! Updated: 2026-11-07T22:12:57-07:00.

use async_trait::async_trait;
use orchestrator_runtime::{ChildSupervisor, LaunchRequest, SupervisionEvent, SupervisorError};
use serde_json::Value;
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

use crate::contract::{
    AgentAdapter, AgentCapabilities, AgentError, AgentEvent, AgentExit, AgentHandle, AgentKind,
    AgentLaunchRequest, Result, TurnOutcome,
};

const ALLOWED_TOOLS: &str = "Read,Edit,Write";

/// Claude Code's one-shot adapter for `claude -p --output-format stream-json`.
#[derive(Debug, Clone, Default)]
pub struct ClaudeCodeAdapter {
    /// Executable to launch. `None` means `claude`, resolved against PATH.
    program: Option<PathBuf>,
}

impl ClaudeCodeAdapter {
    /// Launch `claude`, resolved against PATH.
    pub fn new() -> Self {
        Self { program: None }
    }

    /// Use an explicit executable path.
    pub fn with_program(mut self, program: impl Into<PathBuf>) -> Self {
        self.program = Some(program.into());
        self
    }

    /// Build the verified print-mode arguments for one prompt.
    ///
    /// The returned vector keeps the prompt as one argument and never invokes
    /// a shell. `dontAsk` permits only pre-approved tools. Both allowlists
    /// contain only `Read`, `Edit`, and `Write`; `--restricted` confines file
    /// tools to the eventual working directory and rejects permission bypass.
    /// Callers own this vector and must preserve these restrictions.
    pub fn args_for_prompt(prompt: &str) -> Vec<String> {
        vec![
            "-p".into(),
            prompt.into(),
            "--verbose".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--include-partial-messages".into(),
            "--permission-mode".into(),
            "dontAsk".into(),
            "--permission-prompts".into(),
            "none".into(),
            "--restricted".into(),
            "--strict-mcp-config".into(),
            "--tools".into(),
            ALLOWED_TOOLS.into(),
            "--allowedTools".into(),
            ALLOWED_TOOLS.into(),
        ]
    }

    fn program(&self) -> PathBuf {
        self.program
            .clone()
            .unwrap_or_else(|| PathBuf::from("claude"))
    }

    fn locate(&self) -> Option<PathBuf> {
        let program = self.program();
        if program.is_file() {
            return Some(program);
        }
        if let Some(pathvar) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&pathvar) {
                let joined = dir.join(&program);
                if joined.is_file() {
                    return Some(joined);
                }
                #[cfg(windows)]
                if !program.extension().is_some_and(|extension| {
                    extension.to_string_lossy().eq_ignore_ascii_case("exe")
                }) {
                    let exe = dir.join(format!("{}.exe", program.to_string_lossy()));
                    if exe.is_file() {
                        return Some(exe);
                    }
                }
            }
        }
        None
    }
}

#[async_trait]
impl AgentAdapter for ClaudeCodeAdapter {
    fn kind(&self) -> AgentKind {
        AgentKind::ClaudeCode
    }

    async fn detect(&self) -> Result<AgentCapabilities> {
        if self.locate().is_none() {
            return Err(AgentError::NotFound(
                self.program().to_string_lossy().into_owned(),
            ));
        }
        Ok(AgentCapabilities {
            streaming: true,
            interactive_input: false,
            structured_output: true,
            cancellation: true,
            session_resumption: false,
            approvals: false,
            model_selection: false,
        })
    }

    async fn spawn(&self, request: AgentLaunchRequest) -> Result<Box<dyn AgentHandle>> {
        let supervisor = ChildSupervisor::launch(&LaunchRequest {
            program: request.program,
            args: request.args,
            working_dir: request.working_dir,
        })?;
        Ok(Box::new(ClaudeCodeHandle {
            supervisor,
            parser: StreamParser::new(),
            cancelled: false,
            code: None,
            exited: false,
        }))
    }
}

struct ClaudeCodeHandle {
    supervisor: ChildSupervisor,
    parser: StreamParser,
    cancelled: bool,
    code: Option<i32>,
    exited: bool,
}

#[async_trait]
impl AgentHandle for ClaudeCodeHandle {
    fn pid(&self) -> u32 {
        self.supervisor.pid()
    }

    async fn next_event(&mut self) -> Result<Option<AgentEvent>> {
        loop {
            match self.supervisor.next_event().await {
                Some(SupervisionEvent::StdoutLine(line)) => {
                    if let Some(result) = self.parser.feed(&line) {
                        return Ok(Some(map_protocol_result(result)));
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
        self.cancelled = true;
        self.supervisor.cancel().await;
        Ok(())
    }

    async fn wait(&mut self) -> Result<AgentExit> {
        if self.exited {
            return Ok(self.outcome());
        }
        while let Some(event) = self.next_event().await? {
            if matches!(event, AgentEvent::SessionExited { .. }) {
                return Ok(self.outcome());
            }
        }
        Err(SupervisorError::NoExit.into())
    }
}

impl ClaudeCodeHandle {
    fn outcome(&self) -> AgentExit {
        let last_message = self.parser.last_message().map(str::to_owned);
        if self.cancelled {
            AgentExit::Cancelled {
                code: self.code,
                last_message,
            }
        } else if self.parser.violation().is_some() {
            AgentExit::Failed {
                code: self.code,
                reason: self.parser.violation().map(ToString::to_string),
            }
        } else if self.parser.turn_finished() {
            AgentExit::Completed {
                code: self.code,
                last_message,
            }
        } else {
            AgentExit::Failed {
                code: self.code,
                reason: None,
            }
        }
    }
}

fn map_protocol_result(result: std::result::Result<AgentEvent, ProtocolViolation>) -> AgentEvent {
    match result {
        Ok(event) => event,
        Err(violation) => AgentEvent::ProtocolError {
            message: violation.to_string(),
        },
    }
}

/// A malformed Claude Code event or a known event with a missing required field.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProtocolViolation {
    #[error("malformed JSON: {0}")]
    MalformedJson(String),
    #[error("missing or invalid required field `{0}`")]
    BadField(&'static str),
}

/// Incremental translator for the verified Claude Code `stream-json` lines.
///
/// Unknown event types are ignored. Malformed JSON and incomplete known event
/// shapes produce one violation. The parser never treats process EOF as a
/// completed turn.
#[derive(Debug, Clone, Default)]
pub struct StreamParser {
    last_message: Option<String>,
    turn_finished: bool,
    violation: Option<ProtocolViolation>,
}

impl StreamParser {
    /// Create a parser for one supervised Claude Code process.
    pub fn new() -> Self {
        Self::default()
    }

    /// True after a terminal `result` event.
    pub fn turn_finished(&self) -> bool {
        self.turn_finished
    }

    /// The latest complete message observed in deltas or the result event.
    pub fn last_message(&self) -> Option<&str> {
        self.last_message.as_deref()
    }

    /// The first protocol violation, if one occurred.
    pub fn violation(&self) -> Option<&ProtocolViolation> {
        self.violation.as_ref()
    }

    /// Translate one stdout line. Unknown events and blank lines have no
    /// canonical event. Malformed lines return a protocol violation instead
    /// of panicking.
    pub fn feed(&mut self, line: &str) -> Option<Result<AgentEvent, ProtocolViolation>> {
        if self.violation.is_some() {
            return None;
        }
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let value = match serde_json::from_str::<Value>(line) {
            Ok(value) => value,
            Err(error) => {
                return self.fault(ProtocolViolation::MalformedJson(error.to_string()));
            }
        };
        match self.translate(&value) {
            Ok(Some(event)) => Some(Ok(event)),
            Ok(None) => None,
            Err(violation) => self.fault(violation),
        }
    }

    fn fault(
        &mut self,
        violation: ProtocolViolation,
    ) -> Option<Result<AgentEvent, ProtocolViolation>> {
        self.violation = Some(violation.clone());
        Some(Err(violation))
    }

    fn translate(&mut self, value: &Value) -> Result<Option<AgentEvent>, ProtocolViolation> {
        let object = value
            .as_object()
            .ok_or(ProtocolViolation::BadField("type"))?;
        let event_type = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or(ProtocolViolation::BadField("type"))?;
        match event_type {
            "system" => self.translate_system(object),
            "stream_event" => self.translate_stream_event(object),
            "result" => self.translate_result(object),
            _ => Ok(None),
        }
    }

    fn translate_system(
        &self,
        object: &serde_json::Map<String, Value>,
    ) -> Result<Option<AgentEvent>, ProtocolViolation> {
        if object.get("subtype").and_then(Value::as_str) != Some("init") {
            return Ok(None);
        }
        let session_id = object
            .get("session_id")
            .and_then(Value::as_str)
            .and_then(|id| Uuid::parse_str(id).ok())
            .ok_or(ProtocolViolation::BadField("session_id"))?;
        Ok(Some(AgentEvent::SessionStarted { session_id }))
    }

    fn translate_stream_event(
        &mut self,
        object: &serde_json::Map<String, Value>,
    ) -> Result<Option<AgentEvent>, ProtocolViolation> {
        let event = object
            .get("event")
            .and_then(Value::as_object)
            .ok_or(ProtocolViolation::BadField("event"))?;
        let event_type = event
            .get("type")
            .and_then(Value::as_str)
            .ok_or(ProtocolViolation::BadField("event.type"))?;
        if event_type != "content_block_delta" {
            return Ok(None);
        }
        let delta = event
            .get("delta")
            .and_then(Value::as_object)
            .ok_or(ProtocolViolation::BadField("delta"))?;
        let delta_type = delta
            .get("type")
            .and_then(Value::as_str)
            .ok_or(ProtocolViolation::BadField("delta.type"))?;
        if delta_type != "text_delta" {
            return Ok(None);
        }
        let text = delta
            .get("text")
            .and_then(Value::as_str)
            .ok_or(ProtocolViolation::BadField("delta.text"))?;
        self.last_message
            .get_or_insert_with(String::new)
            .push_str(text);
        Ok(Some(AgentEvent::MessageDelta {
            text: text.to_owned(),
        }))
    }

    fn translate_result(
        &mut self,
        object: &serde_json::Map<String, Value>,
    ) -> Result<Option<AgentEvent>, ProtocolViolation> {
        let subtype = object
            .get("subtype")
            .and_then(Value::as_str)
            .ok_or(ProtocolViolation::BadField("subtype"))?;
        let is_error = object
            .get("is_error")
            .and_then(Value::as_bool)
            .ok_or(ProtocolViolation::BadField("is_error"))?;
        if let Some(result) = object.get("result").and_then(Value::as_str) {
            self.last_message = Some(result.to_owned());
        }
        self.turn_finished = true;
        Ok(Some(AgentEvent::TurnFinished {
            outcome: if subtype == "success" && !is_error {
                TurnOutcome::Success
            } else {
                TurnOutcome::Failure
            },
        }))
    }
}
