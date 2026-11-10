//! Task contracts and contract validation (plan section 5.2).
//!
//! A contract is the persisted unit of requested work. It is validated
//! *before* execution and is immutable once admitted: an agent or
//! coordinator may propose a different contract, but the proposal is a new
//! object that must itself pass [`validate`] and receive fresh authorization
//! (see [`crate::authorization`]). An LLM proposal can never mutate an
//! admitted contract in place, and it can never widen its own autonomy or
//! delivery policy: proposals arrive as [`TaskProposal`] checked against the
//! caller's [`Authorization`] envelope.

use crate::authorization::Authorization;

/// Kind of work a contract requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TaskKind {
    /// Writes code in the task worktree.
    Implement,
    /// Read-only research producing a report; no worktree writes are granted
    /// by the kind alone, but the autonomy policy still bounds what the
    /// agent process can touch.
    Investigate,
}

/// How a finished candidate is delivered.
///
/// `Ord` here is container ordering only (for the authorization envelope's
/// delivery set); it confers no rank or "more permissive" relationship
/// between policies.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub enum DeliveryPolicy {
    /// Produce a report artifact only. Never merges code.
    ReportOnly,
    /// Prepare a candidate diff and require explicit authorization before
    /// integration.
    LocalReview,
    /// Push an authorized branch and open a pull request. The PR is a
    /// proposal; creation of the PR is not a merge and never merges.
    PullRequest,
}

/// How much the agent may act without further approval.
///
/// # No total ordering
/// These policies are *not* totally ordered, and none is "more" than the
/// others in general: [`AutonomyPolicy::ApprovalRequired`] is not a stronger
/// form of workspace access — it *requires approval* for consequential
/// actions. Proposal validation compares autonomy by *equality* against the
/// caller's envelope (an agent may not change its own policy); the only
/// structural predicate the engine derives is
/// [`AutonomyPolicy::grants_workspace_writes`], used to decide what the
/// admitted policy permits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AutonomyPolicy {
    /// The agent may read; any write requires approval.
    ReadOnly,
    /// The agent may write inside its task-owned worktree only.
    WorkspaceWrites,
    /// Every consequential action (delivery, external calls) requires
    /// approval even within the worktree. This does *not* by itself grant
    /// workspace writes or any delivery.
    ApprovalRequired,
}

impl AutonomyPolicy {
    /// The only structural fact the authorization layer relies on: only
    /// `WorkspaceWrites` grants filesystem writes inside the worktree.
    /// `ReadOnly` and `ApprovalRequired` both deny them. This keeps
    /// `ApprovalRequired` from silently granting what `WorkspaceWrites`
    /// grants.
    pub fn grants_workspace_writes(self) -> bool {
        matches!(self, Self::WorkspaceWrites)
    }
}

/// How the engine independently verifies a finished candidate (plan section
/// 12). Verification commands are themselves executable code; they run under
/// a trust policy with resource limits that the store/runtime supplies, not
/// here.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum VerificationPolicy {
    /// No automated verification gate; the candidate is reported as-is. The
    /// engine still records that no gate ran, so an absence of verification
    /// is auditable rather than accidental.
    None,
    /// Run an explicit, argument-array command list (no shell interpolation)
    /// against the candidate worktree. A worker editing the project's own
    /// verification config cannot weaken this gate, because the commands
    /// here are the contract's, not the repository's.
    Commands {
        /// Executable plus argument arrays, one entry per gate.
        commands: Vec<Vec<String>>,
    },
}

impl VerificationPolicy {
    /// Whether this policy enforces at least one automated gate.
    pub fn has_gate(self) -> bool {
        matches!(self, Self::Commands { .. })
    }
}

/// The persisted, validated contract for one unit of work.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TaskContract {
    /// Stable task identifier assigned by the engine (not by the proposer).
    pub id: String,
    /// Project (registered repository) the task belongs to.
    pub project_id: String,
    /// Optional parent task; dependency edges must form a DAG (enforced at
    /// submission, not here).
    pub parent_id: Option<String>,
    /// What the work is. Free text; treated as untrusted input anywhere it
    /// is later rendered to an agent prompt.
    pub objective: String,
    pub kind: TaskKind,
    /// Which adapter runs the task. Resolved against detected capabilities
    /// before admission; recorded verbatim for audit.
    pub agent: String,
    /// Ids of tasks that must reach a terminal success before this one may
    /// start. Must be acyclic.
    pub dependencies: Vec<String>,
    /// How the engine independently verifies a finished candidate.
    pub verification: VerificationPolicy,
    pub delivery: DeliveryPolicy,
    pub autonomy: AutonomyPolicy,
    /// Bounded retries for execution failures (plan section 7).
    pub max_retries: u32,
    /// Bounded verification-repair attempts (plan section 5.3).
    pub max_repairs: u32,
}

/// Errors from [`validate`]. Each variant names the invariant that failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    /// Objective is blank or whitespace-only.
    EmptyObjective,
    /// Dependencies include the task itself (self-loop).
    SelfDependency,
    /// Dependencies contain a duplicate entry.
    DuplicateDependency,
    /// An agent was not resolved.
    EmptyAgent,
    /// A `Commands` verification gate carries no commands.
    EmptyVerificationCommands,
    /// A field exceeds its documented size bound; the payload names it.
    FieldTooLarge(&'static str),
    /// A text field contains a NUL byte, which no process argument, path, or
    /// identifier can carry; the payload names the field.
    ContainsNul(&'static str),
}

/// Size bounds for untrusted contract fields. The 1 MiB IPC frame is the
/// only other limit; these keep one contract from filling the store or the
/// UI and keep argument vectors within what operating systems accept.
pub mod limits {
    /// Objective text, in bytes.
    pub const MAX_OBJECTIVE_BYTES: usize = 64 * 1024;
    /// Agent name, in bytes.
    pub const MAX_AGENT_BYTES: usize = 64;
    /// Project and parent identifiers, in bytes. Project ids are repository
    /// paths.
    pub const MAX_ID_BYTES: usize = 4096;
    /// Dependency count.
    pub const MAX_DEPENDENCIES: usize = 256;
    /// Verification commands per contract.
    pub const MAX_VERIFICATION_COMMANDS: usize = 32;
    /// Arguments per verification command, program included.
    pub const MAX_COMMAND_ARGS: usize = 64;
    /// One verification argument, in bytes.
    pub const MAX_COMMAND_ARG_BYTES: usize = 4096;
}

impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContractError::EmptyObjective => write!(f, "objective must be non-empty"),
            ContractError::SelfDependency => write!(f, "task cannot depend on itself"),
            ContractError::DuplicateDependency => write!(f, "duplicate dependency"),
            ContractError::EmptyAgent => write!(f, "agent must be resolved"),
            ContractError::EmptyVerificationCommands => {
                write!(
                    f,
                    "Commands verification policy must carry at least one command"
                )
            }
            ContractError::FieldTooLarge(field) => write!(f, "{field} exceeds its size limit"),
            ContractError::ContainsNul(field) => write!(f, "{field} contains a NUL byte"),
        }
    }
}

impl std::error::Error for ContractError {}

/// Validate an incoming contract. Cycle detection across a *set* of
/// contracts belongs at submission (the scheduler owns the DAG); this
/// function validates one contract's invariants.
pub fn validate(contract: &TaskContract) -> Result<(), ContractError> {
    if contract.objective.trim().is_empty() {
        return Err(ContractError::EmptyObjective);
    }
    if contract.agent.trim().is_empty() {
        return Err(ContractError::EmptyAgent);
    }
    validate_bounds(contract)?;
    if contract.dependencies.iter().any(|d| d == &contract.id) {
        return Err(ContractError::SelfDependency);
    }
    let mut seen = std::collections::BTreeSet::new();
    for d in &contract.dependencies {
        if !seen.insert(d) {
            return Err(ContractError::DuplicateDependency);
        }
    }
    match &contract.verification {
        VerificationPolicy::Commands { commands }
            if commands.is_empty()
                || commands
                    .iter()
                    .any(|cmd| cmd.is_empty() || cmd.iter().any(|s| s.is_empty())) =>
        {
            return Err(ContractError::EmptyVerificationCommands);
        }
        _ => {}
    }
    Ok(())
}

fn check_text(field: &'static str, value: &str, max_bytes: usize) -> Result<(), ContractError> {
    if value.len() > max_bytes {
        return Err(ContractError::FieldTooLarge(field));
    }
    if value.contains('\0') {
        return Err(ContractError::ContainsNul(field));
    }
    Ok(())
}

fn validate_bounds(contract: &TaskContract) -> Result<(), ContractError> {
    use limits::*;
    check_text("objective", &contract.objective, MAX_OBJECTIVE_BYTES)?;
    check_text("agent", &contract.agent, MAX_AGENT_BYTES)?;
    check_text("project_id", &contract.project_id, MAX_ID_BYTES)?;
    if let Some(parent) = &contract.parent_id {
        check_text("parent_id", parent, MAX_ID_BYTES)?;
    }
    if contract.dependencies.len() > MAX_DEPENDENCIES {
        return Err(ContractError::FieldTooLarge("dependencies"));
    }
    for dependency in &contract.dependencies {
        check_text("dependency", dependency, MAX_ID_BYTES)?;
    }
    if let VerificationPolicy::Commands { commands } = &contract.verification {
        if commands.len() > MAX_VERIFICATION_COMMANDS {
            return Err(ContractError::FieldTooLarge("verification commands"));
        }
        for command in commands {
            if command.len() > MAX_COMMAND_ARGS {
                return Err(ContractError::FieldTooLarge("verification command"));
            }
            for argument in command {
                check_text("verification argument", argument, MAX_COMMAND_ARG_BYTES)?;
            }
        }
    }
    Ok(())
}

/// A proposed contract arriving from a coordinator or agent. The engine
/// rewrites the engine-owned fields (id) before validation; the proposer
/// never supplies its own id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskProposal {
    pub objective: String,
    pub kind: TaskKind,
    pub agent: String,
    pub dependencies: Vec<String>,
    pub verification: VerificationPolicy,
    pub delivery: DeliveryPolicy,
    pub autonomy: AutonomyPolicy,
    pub max_retries: u32,
    pub max_repairs: u32,
}

/// The result of validating a proposal against a caller's authorization
/// envelope, before the engine assigns an id and persists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Validation {
    /// The proposal is legal to admit as-is.
    Accept,
    /// The proposal requests authority the envelope does not grant. This is
    /// a policy violation, not a retryable error: the caller must be
    /// re-authorized by the user.
    Denied { reason: String },
    /// The proposal is structurally invalid (see [`ContractError`]).
    Invalid(ContractError),
}

/// Validate a proposal against `envelope`.
///
/// Three independent authorization checks, matching plan section 5.2 ("An LLM
/// must not be able to silently change its own autonomy or delivery policy")
/// and section 13 ("schema validation and immutable policy enforcement"):
///
/// 1. **Autonomy.** A proposal may only *request* work the envelope's
///    autonomy actually grants. In particular it may not request
///    `WorkspaceWrites` unless the envelope grants workspace writes.
///    `ApprovalRequired` neither implies nor prevents this: it is a separate
///    policy dimension, not a rank.
/// 2. **Delivery.** The proposed delivery must be one of the delivery
///    choices the envelope explicitly authorizes. `PullRequest` is never
///    implied by any autonomy or by `LocalReview`; it is granted only if the
///    envelope lists it.
///
/// 3. **Verification.** The proposal must retain the authorized gate.
///
/// Delivery and autonomy are orthogonal; neither implies the other.
pub fn validate_proposal(proposal: &TaskProposal, envelope: &Authorization) -> Validation {
    // Autonomy: the proposal must request exactly the autonomy the envelope
    // authorizes. This is equality, not a rank: there is no "stronger" or
    // "weaker" autonomy to compare against. An LLM proposing a different
    // autonomy than the one it was granted is changing its own policy, which
    // plan section 5.2 forbids.
    if proposal.autonomy != envelope.autonomy {
        return Validation::Denied {
            reason: format!(
                "autonomy {:?} differs from the envelope's {:?}; an agent may not change its own autonomy policy",
                proposal.autonomy, envelope.autonomy
            ),
        };
    }
    if proposal.verification != envelope.verification {
        return Validation::Denied {
            reason: "verification policy differs from the authorized gate".into(),
        };
    }
    // Delivery: the proposed delivery must be explicitly authorized.
    if !envelope.delivery_grants(proposal.delivery) {
        return Validation::Denied {
            reason: format!(
                "delivery {:?} is not among the envelope's authorized choices; re-authorization required",
                proposal.delivery
            ),
        };
    }
    let probe = TaskContract {
        id: String::new(),
        project_id: String::new(),
        parent_id: None,
        objective: proposal.objective.clone(),
        kind: proposal.kind,
        agent: proposal.agent.clone(),
        dependencies: proposal.dependencies.clone(),
        verification: proposal.verification.clone(),
        delivery: proposal.delivery,
        autonomy: proposal.autonomy,
        max_retries: proposal.max_retries,
        max_repairs: proposal.max_repairs,
    };
    match validate(&probe) {
        Ok(()) => Validation::Accept,
        Err(e) => Validation::Invalid(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract() -> TaskContract {
        TaskContract {
            id: "t1".into(),
            project_id: "p1".into(),
            parent_id: None,
            objective: "Add unit tests".into(),
            kind: TaskKind::Implement,
            agent: "codex".into(),
            dependencies: vec![],
            verification: VerificationPolicy::Commands {
                commands: vec![vec!["cargo".into(), "test".into()]],
            },
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 1,
            max_repairs: 1,
        }
    }

    fn proposal() -> TaskProposal {
        TaskProposal {
            objective: "Add unit tests".into(),
            kind: TaskKind::Implement,
            agent: "codex".into(),
            dependencies: vec![],
            verification: VerificationPolicy::None,
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 1,
            max_repairs: 1,
        }
    }

    fn env(
        autonomy: AutonomyPolicy,
        delivery: &std::collections::BTreeSet<DeliveryPolicy>,
    ) -> Authorization {
        Authorization {
            autonomy,
            delivery: delivery.clone(),
            verification: VerificationPolicy::None,
        }
    }

    #[test]
    fn valid_contract_passes() {
        assert_eq!(validate(&contract()), Ok(()));
    }

    #[test]
    fn empty_objective_rejected() {
        let mut c = contract();
        c.objective = "   \n ".into();
        assert_eq!(validate(&c), Err(ContractError::EmptyObjective));
    }

    #[test]
    fn self_dependency_rejected() {
        let mut c = contract();
        c.dependencies = vec!["t1".into()];
        assert_eq!(validate(&c), Err(ContractError::SelfDependency));
    }

    #[test]
    fn duplicate_dependency_rejected() {
        let mut c = contract();
        c.dependencies = vec!["a".into(), "a".into()];
        assert_eq!(validate(&c), Err(ContractError::DuplicateDependency));
    }

    #[test]
    fn commands_verification_with_blank_command_rejected() {
        let mut c = contract();
        c.verification = VerificationPolicy::Commands {
            commands: vec![vec!["".into(), "test".into()]],
        };
        assert_eq!(validate(&c), Err(ContractError::EmptyVerificationCommands));
        c.verification = VerificationPolicy::Commands { commands: vec![] };
        assert_eq!(validate(&c), Err(ContractError::EmptyVerificationCommands));
    }

    #[test]
    fn only_workspace_writes_grants_workspace_writes() {
        assert!(AutonomyPolicy::WorkspaceWrites.grants_workspace_writes());
        assert!(!AutonomyPolicy::ReadOnly.grants_workspace_writes());
        // ApprovalRequired must not silently grant what WorkspaceWrites
        // grants; it is a separate policy dimension.
        assert!(!AutonomyPolicy::ApprovalRequired.grants_workspace_writes());
    }

    #[test]
    fn approval_required_does_not_imply_workspace_writes() {
        // A proposal for ApprovalRequired is *not* a workspace-write request,
        // so a ReadOnly envelope does not block it on that ground. It must be
        // authorized as a distinct policy by the envelope's autonomy choice.
        let mut p = proposal();
        p.autonomy = AutonomyPolicy::ApprovalRequired;
        // Envelope that authorizes ApprovalRequired but no writes.
        let e = Authorization {
            autonomy: AutonomyPolicy::ApprovalRequired,
            delivery: std::collections::BTreeSet::from([DeliveryPolicy::LocalReview]),
            verification: VerificationPolicy::None,
        };
        assert_eq!(validate_proposal(&p, &e), Validation::Accept);
    }

    #[test]
    fn proposal_cannot_change_autonomy() {
        // An agent proposing a different autonomy than the envelope it was
        // granted is changing its own policy. Denied, with the reason
        // naming the dimension.
        let mut p = proposal();
        p.autonomy = AutonomyPolicy::WorkspaceWrites;
        let e = env(
            AutonomyPolicy::ReadOnly,
            &std::collections::BTreeSet::from([DeliveryPolicy::LocalReview]),
        );
        match validate_proposal(&p, &e) {
            Validation::Denied { reason } => assert!(reason.contains("autonomy"), "{reason}"),
            other => panic!("expected Denied, got {other:?}"),
        }
    }

    #[test]
    fn proposal_within_envelope_accepted() {
        let p = proposal();
        let e = env(
            AutonomyPolicy::WorkspaceWrites,
            &std::collections::BTreeSet::from([DeliveryPolicy::LocalReview]),
        );
        assert_eq!(validate_proposal(&p, &e), Validation::Accept);
    }

    #[test]
    fn proposal_cannot_remove_authorized_verification_gate() {
        let p = proposal();
        let mut e = env(
            AutonomyPolicy::WorkspaceWrites,
            &std::collections::BTreeSet::from([DeliveryPolicy::LocalReview]),
        );
        e.verification = VerificationPolicy::Commands {
            commands: vec![vec!["cargo".into(), "test".into()]],
        };
        assert!(matches!(
            validate_proposal(&p, &e),
            Validation::Denied { .. }
        ));
    }

    #[test]
    fn pull_request_delivery_denied_without_explicit_authorization() {
        // LocalReview being authorized must not imply PullRequest.
        let mut p = proposal();
        p.delivery = DeliveryPolicy::PullRequest;
        let e = env(
            AutonomyPolicy::WorkspaceWrites,
            &std::collections::BTreeSet::from([DeliveryPolicy::LocalReview]),
        );
        match validate_proposal(&p, &e) {
            Validation::Denied { reason } => {
                assert!(reason.contains("delivery"), "{reason}")
            }
            other => panic!("expected Denied, got {other:?}"),
        }
        // And it is accepted when the envelope explicitly authorizes it.
        let e2 = env(
            AutonomyPolicy::WorkspaceWrites,
            &std::collections::BTreeSet::from([
                DeliveryPolicy::LocalReview,
                DeliveryPolicy::PullRequest,
            ]),
        );
        assert_eq!(validate_proposal(&p, &e2), Validation::Accept);
    }

    #[test]
    fn delivery_and_autonomy_are_orthogonal() {
        // A delivery a caller is *not* authorized is denied even when its
        // autonomy is fully authorized, and vice versa.
        let mut p = proposal();
        p.delivery = DeliveryPolicy::ReportOnly;
        let e = env(
            AutonomyPolicy::WorkspaceWrites, // autonomy is fine
            &std::collections::BTreeSet::from([DeliveryPolicy::LocalReview]), // delivery is not
        );
        assert!(matches!(
            validate_proposal(&p, &e),
            Validation::Denied { .. }
        ));
    }

    #[test]
    fn malformed_proposal_is_invalid_not_denied() {
        // Authorization passes (envelope matches the proposal's policies),
        // then structural validation fails: Invalid, not Denied.
        let mut p = proposal();
        p.objective = " ".into();
        let e = env(
            AutonomyPolicy::WorkspaceWrites,
            &std::collections::BTreeSet::from([DeliveryPolicy::LocalReview]),
        );
        assert_eq!(
            validate_proposal(&p, &e),
            Validation::Invalid(ContractError::EmptyObjective)
        );
    }

    #[test]
    fn contract_at_each_limit_is_accepted() {
        use limits::*;
        let mut c = contract();
        c.objective = "o".repeat(MAX_OBJECTIVE_BYTES);
        c.agent = "a".repeat(MAX_AGENT_BYTES);
        c.project_id = "p".repeat(MAX_ID_BYTES);
        c.parent_id = Some("q".repeat(MAX_ID_BYTES));
        c.dependencies = (0..MAX_DEPENDENCIES).map(|n| format!("dep-{n}")).collect();
        c.verification = VerificationPolicy::Commands {
            commands: vec![
                vec!["x".repeat(MAX_COMMAND_ARG_BYTES); MAX_COMMAND_ARGS];
                MAX_VERIFICATION_COMMANDS
            ],
        };
        assert_eq!(validate(&c), Ok(()));
    }

    #[test]
    fn contract_fields_over_their_limit_are_rejected() {
        use limits::*;
        let cases: Vec<(&str, Box<dyn Fn(&mut TaskContract)>)> = vec![
            (
                "objective",
                Box::new(|c| c.objective = "o".repeat(MAX_OBJECTIVE_BYTES + 1)),
            ),
            (
                "agent",
                Box::new(|c| c.agent = "a".repeat(MAX_AGENT_BYTES + 1)),
            ),
            (
                "project_id",
                Box::new(|c| c.project_id = "p".repeat(MAX_ID_BYTES + 1)),
            ),
            (
                "parent_id",
                Box::new(|c| c.parent_id = Some("q".repeat(MAX_ID_BYTES + 1))),
            ),
            (
                "dependencies",
                Box::new(|c| {
                    c.dependencies = (0..=MAX_DEPENDENCIES).map(|n| format!("dep-{n}")).collect()
                }),
            ),
            (
                "dependency",
                Box::new(|c| c.dependencies = vec!["d".repeat(MAX_ID_BYTES + 1)]),
            ),
            (
                "verification commands",
                Box::new(|c| {
                    c.verification = VerificationPolicy::Commands {
                        commands: vec![vec!["cargo".into()]; MAX_VERIFICATION_COMMANDS + 1],
                    }
                }),
            ),
            (
                "verification command",
                Box::new(|c| {
                    c.verification = VerificationPolicy::Commands {
                        commands: vec![vec!["a".into(); MAX_COMMAND_ARGS + 1]],
                    }
                }),
            ),
            (
                "verification argument",
                Box::new(|c| {
                    c.verification = VerificationPolicy::Commands {
                        commands: vec![vec!["a".repeat(MAX_COMMAND_ARG_BYTES + 1)]],
                    }
                }),
            ),
        ];
        for (field, mutate) in cases {
            let mut c = contract();
            mutate(&mut c);
            assert_eq!(
                validate(&c),
                Err(ContractError::FieldTooLarge(field)),
                "{field}"
            );
        }
    }

    #[test]
    fn contract_text_with_a_nul_byte_is_rejected() {
        let cases: Vec<(&str, Box<dyn Fn(&mut TaskContract)>)> = vec![
            ("objective", Box::new(|c| c.objective = "fix\0it".into())),
            ("agent", Box::new(|c| c.agent = "co\0dex".into())),
            ("project_id", Box::new(|c| c.project_id = "/repo\0x".into())),
            ("parent_id", Box::new(|c| c.parent_id = Some("p\0".into()))),
            (
                "dependency",
                Box::new(|c| c.dependencies = vec!["d\0".into()]),
            ),
            (
                "verification argument",
                Box::new(|c| {
                    c.verification = VerificationPolicy::Commands {
                        commands: vec![vec!["cargo".into(), "te\0st".into()]],
                    }
                }),
            ),
        ];
        for (field, mutate) in cases {
            let mut c = contract();
            mutate(&mut c);
            assert_eq!(
                validate(&c),
                Err(ContractError::ContainsNul(field)),
                "{field}"
            );
        }
    }
}
