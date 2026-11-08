//! Codex `exec` adapter: launches the Codex CLI as a supervised process and
//! translates its NDJSON stdout into the canonical agent event vocabulary.
//!
//! Task policy, authorization, and result interpretation live in the
//! engine, not here. The adapter reports protocol facts.

use async_trait::async_trait;
use orchestrator_runtime::{ChildSupervisor, LaunchRequest, SupervisionEvent, SupervisorError};

use crate::codex_parser::Parser;
use crate::contract::{
    AgentAdapter, AgentCapabilities, AgentEvent, AgentError, AgentExit, AgentHandle,
    AgentKind, AgentLaunchRequest, Result,
};
use crate::routing::RoutableAdapter;

/// The Codex adapter for `codex exec --json` one-shot execution.
#[derive(Debug, Clone, Default)]
pub struct CodexAdapter {
    /// Executable to launch. `None` means `codex`, resolved against PATH.
    program: Option<std::path::PathBuf>,
}

impl CodexAdapter {
    /// Launch `codex`, resolved against PATH.
    pub fn new() -> Self {
        Self { program: None }
    }

    /// Use an explicit executable path.
    pub fn with_program(mut self, program: impl Into<std::path::PathBuf>) -> Self {
        self.program = Some(program.into());
        self
    }

    fn program(&self) -> std::path::PathBuf {
        self.program
            .clone()
            .unwrap_or_else(|| std::path::PathBuf::from("codex"))
    }

    /// Locate the executable for detection: an explicit path is checked
    /// as-is; a bare name is resolved against PATH (`.exe` on Windows).
    fn locate(&self) -> Option<std::path::PathBuf> {
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
                if !program
                    .extension()
                    .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("exe"))
                {
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

impl RoutableAdapter for CodexAdapter {
    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            streaming: true,           // CodexHandle::next_event feeds stdout to Parser::feed.
            interactive_input: false,  // AgentHandle exposes no input method (contract.rs:116-126).
            structured_output: true, // Parser::feed parses JSONL records (codex_parser.rs:86-113).
            cancellation: true, // CodexHandle::cancel calls ChildSupervisor::cancel (codex_adapter.rs:158-161).
            session_resumption: false, // `codex exec --json` is one-shot (codex_adapter.rs:16).
            approvals: false, // Parser::translate does not map approval events (codex_parser.rs:122-214).
            model_selection: false, // No verified model-selection operation in this adapter.
        }
    }
}

#[async_trait]
impl AgentAdapter for CodexAdapter {
    fn kind(&self) -> AgentKind {
        AgentKind::Codex
    }

    async fn detect(&self) -> Result<AgentCapabilities> {
        if self.locate().is_none() {
            return Err(AgentError::NotFound(
                self.program().to_string_lossy().into_owned(),
            ));
        }
        Ok(self.capabilities())
    }

    async fn spawn(&self, request: AgentLaunchRequest) -> Result<Box<dyn AgentHandle>> {
        // `detect` is the capability gate; a spawn of a missing binary
        // fails at launch and is reported as a launch error, not silently.
        let sup = ChildSupervisor::launch(&LaunchRequest {
            program: request.program,
            args: request.args,
            working_dir: request.working_dir,
        })?;
        Ok(Box::new(CodexHandle {
            sup,
            parser: Parser::new(),
            cancelled: false,
            code: None,
            exited: false,
        }))
    }
}

struct CodexHandle {
    sup: ChildSupervisor,
    parser: Parser,
    cancelled: bool,
    /// Last observed process exit code, if any.
    code: Option<i32>,
    /// True once `SessionExited` has been emitted; `wait` may then
    /// return the outcome without draining again.
    exited: bool,
}

#[async_trait]
impl AgentHandle for CodexHandle {
    fn pid(&self) -> u32 {
        self.sup.pid()
    }

    async fn next_event(&mut self) -> Result<Option<AgentEvent>> {
        loop {
            let ev = match self.sup.next_event().await {
                Some(e) => e,
                None => return Ok(None),
            };
            match ev {
                SupervisionEvent::StdoutLine(line) => {
                    if let Some(res) = self.parser.feed(&line) {
                        return Ok(Some(map_res(res)));
                    }
                    // No canonical mapping; keep pulling.
                }
                // Stderr is diagnostic; it does not fail the protocol.
                SupervisionEvent::StderrLine(_) => {}
                SupervisionEvent::Exited { code, .. } => {
                    self.code = code;
                    self.exited = true;
                    return Ok(Some(AgentEvent::SessionExited {
                        exit_code: code,
                    }));
                }
            }
        }
    }

    async fn cancel(&mut self) -> Result<()> {
        self.cancelled = true;
        self.sup.cancel().await;
        Ok(())
    }

    async fn wait(&mut self) -> Result<AgentExit> {
        if self.exited {
            return Ok(self.outcome());
        }
        // Drain to the exit event; the parser captures protocol facts.
        while let Some(ev) = self.next_event().await? {
            if matches!(ev, AgentEvent::SessionExited { .. }) {
                return Ok(self.outcome());
            }
        }
        Err(SupervisorError::NoExit.into())
    }
}

impl CodexHandle {
    fn outcome(&self) -> AgentExit {
        let last = self.parser.last_message().map(str::to_string);
        if self.cancelled {
            AgentExit::Cancelled {
                code: self.code,
                last_message: last,
            }
        } else if self.parser.turn_finished() {
            AgentExit::Completed {
                code: self.code,
                last_message: last,
            }
        } else {
            let reason = self.parser.violation().map(|v| v.to_string());
            AgentExit::Failed {
                code: self.code,
                reason,
            }
        }
    }
}

fn map_res(res: std::result::Result<AgentEvent, crate::codex_parser::ProtocolViolation>) -> AgentEvent {
    match res {
        Ok(a) => a,
        Err(v) => AgentEvent::ProtocolError {
            message: v.to_string(),
        },
    }
}
