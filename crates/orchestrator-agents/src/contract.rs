//! Capability-aware adapter contract (plan section 5.4) and the canonical
//! agent event vocabulary (plan section 5.5).
//!
//! The adapter owns provider-specific protocol translation; the runtime
//! supervisor owns process handles and reconciles process exits. Nothing in
//! this module interprets a session or process exit as task success — that
//! is the engine's decision.

use async_trait::async_trait;
use orchestrator_runtime::SupervisorError;
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

/// Convenience result alias.
pub type Result<T, E = AgentError> = std::result::Result<T, E>;

/// The agent provider family an adapter speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentKind {
    Codex,
    ClaudeCode,
    Pi,
}

/// A launch request. The caller owns the argument array; the adapter
/// launches it verbatim through the supervisor and never interpolates
/// request text into a shell.
#[derive(Debug, Clone)]
pub struct AgentLaunchRequest {
    /// Executable path, or a bare name resolved against PATH.
    pub program: PathBuf,
    /// Argument array.
    pub args: Vec<String>,
    /// Explicit working directory for the child.
    pub working_dir: PathBuf,
}

/// Actual feature support of a detected adapter. Unsupported capabilities
/// are reported as `false`, never represented as successful no-ops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCapabilities {
    /// Events are streamed as they are produced (NDJSON on stdout).
    pub streaming: bool,
    /// The agent accepts interactive input after launch.
    pub interactive_input: bool,
    /// The agent emits machine-readable structured output.
    pub structured_output: bool,
    /// The running process can be cancelled with process-tree cleanup.
    pub cancellation: bool,
    /// A session can be resumed after a crash or restart.
    pub session_resumption: bool,
    /// The agent can request human approval mid-session.
    pub approvals: bool,
    /// The model can be selected at launch.
    pub model_selection: bool,
}

/// Canonical agent events (plan section 5.5). Provider output is translated
/// into this vocabulary; provider-specific detail stays in the raw stream,
/// which callers may keep for debugging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    SessionStarted { session_id: Uuid },
    MessageDelta { text: String },
    ToolStarted { tool_id: String, name: String },
    ToolFinished { tool_id: String, success: bool },
    ApprovalRequested { approval_id: Uuid, description: String },
    TurnFinished { outcome: TurnOutcome },
    SessionExited { exit_code: Option<i32> },
    ProtocolError { message: String },
}

/// Outcome of a completed turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnOutcome {
    Success,
    Failure,
}

/// Terminal agent outcome. `Completed` means the protocol reached a
/// completed turn; it says nothing about task success, which is decided by
/// the engine from verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentExit {
    /// The protocol observed a completed turn; the process then exited.
    Completed {
        code: Option<i32>,
        last_message: Option<String>,
    },
    /// The process exited without a completed turn, or the protocol was
    /// violated (`reason` carries the first violation, if any).
    Failed { code: Option<i32>, reason: Option<String> },
    /// Cancellation was requested before the process exited.
    Cancelled { code: Option<i32>, last_message: Option<String> },
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("agent binary not found: {0}")]
    NotFound(String),
    #[error("failed to launch agent: {0}")]
    Launch(#[from] SupervisorError),
    #[error("protocol error: {0}")]
    Protocol(String),
}

#[async_trait]
pub trait AgentAdapter: Send + Sync {
    fn kind(&self) -> AgentKind;
    async fn detect(&self) -> Result<AgentCapabilities>;
    async fn spawn(&self, request: AgentLaunchRequest) -> Result<Box<dyn AgentHandle>>;
}

#[async_trait]
pub trait AgentHandle: Send {
    /// The child's PID (the process-tree root).
    fn pid(&self) -> u32;
    /// The next canonical event, or `None` once the process has exited and
    /// the stream is exhausted (after the `SessionExited` event).
    async fn next_event(&mut self) -> Result<Option<AgentEvent>>;
    /// Kill the child and its descendants. Idempotent.
    async fn cancel(&mut self) -> Result<()>;
    /// Consume the stream until the process has exited and return the
    /// terminal outcome.
    async fn wait(&mut self) -> Result<AgentExit>;
}
