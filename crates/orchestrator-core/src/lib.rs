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

pub mod approval;
pub mod authorization;
pub mod brief;
pub mod contract;
pub mod decomposition;
pub mod identity;
pub mod interval;
pub mod landing;
pub mod load_gate;
pub mod notes;
pub mod orchestrator_action;
pub mod permission;
pub mod project_mode;
pub mod question;
pub mod records;
pub mod retry;
pub mod return_brief;
pub mod scheduler;
pub mod standing_grant;
pub mod supervision;
pub mod task;
pub mod task_envelope;
pub mod work_item;
pub mod worker_status;
pub mod workstream;

pub use identity::ProcessIdentity;
pub use project_mode::{DefaultsError, MergeMethod, ProjectDefaults, ProjectMode};
pub use records::{Artifact, Project, VerificationRun, Workspace};

pub use contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, TaskProposal, Validation,
    VerificationPolicy,
};
