//! Domain contracts for the agent orchestrator: the task contract, the task
//! state machine, and the agent adapter vocabulary.
//!
//! This crate is the root of the workspace dependency graph (see the root
//! Cargo.toml header): downstream crates depend on it, never the reverse. It
//! has no I/O dependencies: no tokio, no Tauri, no SQLite, no Git
//! implementation, no agent provider.
//!
//! Placeholder: boundary only; the domain modules are not sketched here.
