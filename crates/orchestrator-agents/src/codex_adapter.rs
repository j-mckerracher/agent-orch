//! Codex `exec` adapter: launches the Codex CLI as a supervised process and
//! translates its NDJSON stdout into the canonical agent event vocabulary.
//!
//! Task policy, authorization, and result interpretation live in the
//! engine, not here. The adapter reports protocol facts.
//! A nonzero process exit fails even when Codex completed the protocol turn.
//! Updated: 2026-11-08T11:21:26Z.

use async_trait::async_trait;
use orchestrator_runtime::{ChildSupervisor, LaunchRequest, SupervisionEvent, SupervisorError};

use crate::codex_parser::Parser;
use crate::contract::{
    AgentAdapter, AgentCapabilities, AgentError, AgentEvent, AgentExit, AgentHandle, AgentKind,
    AgentLaunchRequest, Result,
};
use crate::routing::RoutableAdapter;
use crate::usage::UsageReport;

/// Parent variables Codex needs beyond the baseline allowlist.
const PROVIDER_ENV: &[&str] = &[
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "OPENAI_ORG_ID",
    "OPENAI_PROJECT",
    "CODEX_HOME",
];

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
    pub(crate) fn locate(&self) -> Option<std::path::PathBuf> {
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
            model_selection: true, // `selection_args` passes a catalog-validated model and effort (catalog.rs).
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
            env_names: PROVIDER_ENV.iter().map(|name| (*name).to_owned()).collect(),
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

    fn take_usage(&mut self) -> Vec<UsageReport> {
        self.parser.take_usage()
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
                    return Ok(Some(AgentEvent::SessionExited { exit_code: code }));
                }
            }
        }
    }

    async fn cancel(&mut self) -> Result<()> {
        // A natural exit already observed is terminal; do not relabel it.
        if self.cancelled || self.exited {
            return Ok(());
        }
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
        terminal_outcome(
            self.cancelled,
            self.code,
            self.parser.turn_finished(),
            self.parser.violation().map(ToString::to_string),
            self.parser.last_message().map(str::to_owned),
        )
    }
}

fn terminal_outcome(
    cancelled: bool,
    code: Option<i32>,
    turn_finished: bool,
    violation: Option<String>,
    last_message: Option<String>,
) -> AgentExit {
    if cancelled {
        return AgentExit::Cancelled { code, last_message };
    }
    if let Some(exit_code) = code.filter(|code| *code != 0) {
        let mut reason = format!("agent exited with code {exit_code}");
        if let Some(violation) = violation.as_deref() {
            reason.push_str("; protocol violation: ");
            reason.push_str(violation);
        }
        if let Some(message) = last_message.as_deref() {
            reason.push_str("; last message: ");
            reason.push_str(message);
        }
        return AgentExit::Failed {
            code: Some(exit_code),
            reason: Some(reason),
        };
    }
    if let Some(reason) = violation {
        AgentExit::Failed {
            code,
            reason: Some(reason),
        }
    } else if turn_finished {
        AgentExit::Completed { code, last_message }
    } else {
        AgentExit::Failed { code, reason: None }
    }
}

fn map_res(
    res: std::result::Result<AgentEvent, crate::codex_parser::ProtocolViolation>,
) -> AgentEvent {
    match res {
        Ok(a) => a,
        Err(v) => AgentEvent::ProtocolError {
            message: v.to_string(),
        },
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonzero_exit_after_completed_turn_fails_and_keeps_last_message() {
        let exit = terminal_outcome(false, Some(7), true, None, Some("done".to_owned()));
        match exit {
            AgentExit::Failed {
                code: Some(7),
                reason: Some(reason),
            } => {
                assert!(reason.contains("code 7"), "reason: {reason}");
                assert!(reason.contains("last message: done"), "reason: {reason}");
            }
            other => panic!("expected a failed exit with code 7, got {other:?}"),
        }
    }
}
