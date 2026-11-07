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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
}

impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContractError::EmptyObjective => write!(f, "objective must be non-empty"),
            ContractError::SelfDependency => write!(f, "task cannot depend on itself"),
            ContractError::DuplicateDependency => write!(f, "duplicate dependency"),
            ContractError::EmptyAgent => write!(f, "agent must be resolved"),
            ContractError::EmptyVerificationCommands => {
                write!(f, "Commands verification policy must carry at least one command")
            }
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
            if commands
                .iter()
                .any(|cmd| cmd.is_empty() || cmd.iter().any(|s| s.is_empty())) =>
        {
            return Err(ContractError::EmptyVerificationCommands);
        }
        _ => {}
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
/// Two independent authorization checks, matching plan section 5.2 ("An LLM
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

    fn env(autonomy: AutonomyPolicy, delivery: &std::collections::BTreeSet<DeliveryPolicy>) -> Authorization {
        Authorization {
            autonomy,
            delivery: delivery.clone(),
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
        // An empty gate list is allowed (equivalent to None); a *blank*
        // command entry is not.
        assert_eq!(validate(&c), Ok(()));
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
}
