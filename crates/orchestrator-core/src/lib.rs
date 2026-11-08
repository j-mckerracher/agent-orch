//! Domain contracts for the agent orchestrator: the task contract, the
//! centrally validated task and session state machines, the session/attempt
//! separation, the authorization invariants, and the canonical agent event
//! vocabulary.
//!
//! This crate is the root of the workspace dependency graph (see the root
//! Cargo.toml header): downstream crates depend on it, never the reverse. It
//! has no I/O dependencies: no tokio, no Tauri, no SQLite, no Git
//! implementation, no agent provider. Everything here is pure.
//!
//! Authority rule: the UI, the coordinator, and agents all *propose* state
//! changes. Only the central [`task::transition`] decides what a task state
//! change means; [`task::Attempt`] and [`task::Session`] are value types whose
//! terminal states are checked, not mutated by agents. See `docs/security.md`
//! for the threat model these contracts implement.

pub mod authorization;
pub mod approval;
pub mod contract;
pub mod records;
pub mod scheduler;
pub mod task;

pub use records::{Artifact, Project, VerificationRun, Workspace};

pub use contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, TaskProposal, Validation,
    VerificationPolicy,
};
