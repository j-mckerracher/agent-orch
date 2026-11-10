//! Capability-aware adapters for Codex, Claude Code, and Pi subprocess
//! protocols. This crate defines the adapter contract in `contract` and
//! implements it on top of orchestrator-runtime's process supervisor.
//!
//! The adapter owns provider-specific protocol translation; the runtime
//! supervisor owns process handles and reconciles process exits. Nothing in
//! this crate interprets a session or process exit as task success, and it
//! holds no task policy or authorization.

pub mod catalog;
pub mod claude_code_adapter;
pub mod codex_adapter;
pub mod codex_parser;
pub mod contract;
pub mod pi_rpc_adapter;
pub mod routing;

pub use catalog::{
    pi_catalog, provider_of, AgentCatalog, CatalogState, Facet, ModelOption, SelectionError,
    SelectionReason,
};
pub use codex_adapter::CodexAdapter;
pub use claude_code_adapter::ClaudeCodeAdapter;
pub use codex_parser::{Parser, ProtocolViolation};
pub use contract::{
    AgentAdapter, AgentCapabilities, AgentError, AgentEvent, AgentExit, AgentHandle, AgentKind,
    AgentLaunchRequest, Result, TurnOutcome,
};
pub use pi_rpc_adapter::PiRpcAdapter;
pub use routing::{
    select_adapter, AdapterCapabilityGap, Capability, RoutableAdapter, RoutingError,
};
