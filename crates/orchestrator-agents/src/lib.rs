//! Capability-aware adapters for Codex, Claude Code, and Pi subprocess
//! protocols. Implements the adapter contract (plan section 5.4) on top of
//! orchestrator-runtime's process supervisor.
//!
//! The adapter owns provider-specific protocol translation; the runtime
//! supervisor owns process handles and reconciles process exits. Nothing in
//! this crate interprets a session or process exit as task success, and it
//! holds no task policy or authorization.

pub mod codex_adapter;
pub mod codex_parser;
pub mod contract;

pub use codex_adapter::CodexAdapter;
pub use codex_parser::{Parser, ProtocolViolation};
pub use contract::{
    AgentAdapter, AgentCapabilities, AgentEvent, AgentError, AgentExit, AgentHandle,
    AgentKind, AgentLaunchRequest, Result, TurnOutcome,
};
