//! Capability-aware adapters for Codex, Claude Code, Pi, and GitHub Copilot
//! subprocess protocols. This crate defines the adapter contract in `contract` and
//! implements it on top of orchestrator-runtime's process supervisor.
//!
//! The adapter owns provider-specific protocol translation; the runtime
//! supervisor owns process handles and reconciles process exits. Nothing in
//! this crate interprets a session or process exit as task success, and it
//! holds no task policy or authorization.

pub mod catalog;
pub mod claude_code_adapter;
pub mod cli_compat;
pub mod codex_adapter;
pub mod codex_parser;
pub mod contract;
pub mod copilot_adapter;
mod copilot_models;
pub mod copilot_parser;
pub mod copilot_sign_in;
pub mod copilot_usage;
mod locate;
pub mod mcp_wiring;
pub mod permission;
pub mod pi_rpc_adapter;
pub mod routing;
pub mod task_session;
mod tool_summary;
pub mod usage;
pub mod workflow_agent;

pub use catalog::{
    pi_catalog, provider_of, AgentCatalog, CatalogState, Facet, ModelOption, SelectionError,
    SelectionReason,
};
pub use claude_code_adapter::ClaudeCodeAdapter;
pub use codex_adapter::CodexAdapter;
pub use codex_parser::{Parser, ProtocolViolation};
pub use contract::{
    AgentAdapter, AgentCapabilities, AgentError, AgentEvent, AgentExit, AgentHandle, AgentKind,
    AgentLaunchRequest, Result, TurnOutcome,
};
pub use copilot_adapter::{
    CopilotAdapter, CopilotRun, COPILOT_AGENT, PINNED_VERSION as COPILOT_PINNED_VERSION,
};
pub use copilot_sign_in::{CopilotSignIn, SignInRecord, SignInState};
pub use locate::locate_program;
pub use mcp_wiring::{McpWiring, MCP_SERVER_NAME, MCP_TOOLS};
pub use pi_rpc_adapter::PiRpcAdapter;
pub use routing::{
    select_adapter, AdapterCapabilityGap, Capability, RoutableAdapter, RoutingError,
};
pub use task_session::{resume_refusal, TaskSession};
pub use usage::{ReportedCost, UsageReport};
