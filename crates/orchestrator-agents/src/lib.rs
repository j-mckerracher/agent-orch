//! Capability-aware adapters for Codex, Claude Code, and Pi subprocess
//! protocols. This crate defines the adapter contract in `contract` and
//! implements it on top of orchestrator-runtime's process supervisor.
//!
//! The adapter owns provider-specific protocol translation; the runtime
//! supervisor owns process handles and reconciles process exits. Nothing in
//! this crate interprets a session or process exit as task success, and it
//! holds no task policy or authorization.

pub mod codex_adapter;
pub mod codex_parser;
pub mod contract;
pub mod pi_rpc_adapter;

pub use codex_adapter::CodexAdapter;
pub use codex_parser::{Parser, ProtocolViolation};
pub use contract::{
    AgentAdapter, AgentCapabilities, AgentError, AgentEvent, AgentExit, AgentHandle, AgentKind,
    AgentLaunchRequest, Result, TurnOutcome,
};
pub use pi_rpc_adapter::PiRpcAdapter;
