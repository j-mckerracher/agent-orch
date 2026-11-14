//! Execution logic shared by the CLI and the daemon.
//!
//! Dependency direction: this crate composes core, git, runtime and store and
//! is consumed by the two executors (`orchestrator-cli`, `orchestrator-daemon`).
//! It holds only logic both executors must run identically, so a restart can
//! never be reconciled differently depending on which front end launched the
//! work.

pub mod reconcile;
