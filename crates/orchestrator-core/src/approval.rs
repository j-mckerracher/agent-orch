//! Approval integrity for gated task actions.
//!
//! Callers supply a 32-byte SHA-256 digest of canonical action parameters. This
//! module stores and compares that digest, never the parameter text. Candidate
//! commits must be full lowercase Git SHA-1 or SHA-256 identifiers. The caller
//! supplies actor identity from a trusted authentication boundary; the
//! allowlist rejects agents and identities it does not recognize.
//!
//! Updated: 2026-11-08T00:04:58-07:00.

use crate::authorization::ActionKind;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

/// Action type bound to an approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "kind", rename_all = "snake_case")]
pub enum ApprovalActionKind {
    /// A gated delivery action from the authorization model.
    Delivery(ActionKind),
    /// A proposed change to task or coordinator policy.
    PolicyChange,
}

/// SHA-256 digest of canonical action-parameter bytes.
///
/// The caller owns parameter canonicalization and hashing. The approval store
/// only persists and compares these fixed-width digest bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ParameterDigest([u8; 32]);

impl ParameterDigest {
    /// Wrap a SHA-256 digest computed from canonical parameter bytes.
    pub const fn from_sha256(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Return the fixed-width digest bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// A full, canonical Git commit identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CandidateCommit(String);

impl CandidateCommit {
    /// Return the lowercase commit identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for CandidateCommit {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let valid_length = matches!(value.len(), 40 | 64);
        let valid_hex = value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if valid_length && valid_hex {
            Ok(Self(value))
        } else {
            Err("candidate commit must be a full lowercase Git SHA-1 or SHA-256".into())
        }
    }
}

impl From<CandidateCommit> for String {
    fn from(value: CandidateCommit) -> Self {
        value.0
    }
}

/// Immutable action dimensions covered by an approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalAction {
    kind: ApprovalActionKind,
    parameter_digest: ParameterDigest,
    candidate_commit: CandidateCommit,
    task_id: String,
}

impl ApprovalAction {
    /// Build a binding from an action kind, digest, full commit SHA, and task id.
    pub fn new(
        kind: ApprovalActionKind,
        parameter_digest: ParameterDigest,
        candidate_commit: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Result<Self, ApprovalError> {
        let candidate_commit = CandidateCommit::try_from(candidate_commit.into())
            .map_err(|_| ApprovalError::InvalidCandidateCommit)?;
        let task_id = task_id.into();
        if task_id.is_empty() {
            return Err(ApprovalError::InvalidTaskId);
        }
        Ok(Self {
            kind,
            parameter_digest,
            candidate_commit,
            task_id,
        })
    }

    /// Return the bound action kind.
    pub const fn kind(&self) -> ApprovalActionKind {
        self.kind
    }

    /// Return the bound parameter digest.
    pub const fn parameter_digest(&self) -> &ParameterDigest {
        &self.parameter_digest
    }

    /// Return the bound candidate commit.
    pub fn candidate_commit(&self) -> &CandidateCommit {
        &self.candidate_commit
    }

    /// Return the task id covered by this action.
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    fn mismatch(&self, proposed: &Self) -> Option<ActionMismatch> {
        if self.kind != proposed.kind {
            Some(ActionMismatch::Kind)
        } else if self.parameter_digest != proposed.parameter_digest {
            Some(ActionMismatch::Parameters)
        } else if self.candidate_commit != proposed.candidate_commit {
            Some(ActionMismatch::CandidateCommit)
        } else if self.task_id != proposed.task_id {
            Some(ActionMismatch::Task)
        } else {
            None
        }
    }
}

/// Actor identity and role supplied by the trusted caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Actor {
    /// The configured user identity.
    User(String),
    /// An agent identity. Agents may request ordinary approvals only.
    Agent(String),
    /// A non-agent identity granted approval authority by the caller.
    Approved(String),
}

impl Actor {
    /// Return this actor's identity string.
    pub fn id(&self) -> &str {
        match self {
            Actor::User(id) | Actor::Agent(id) | Actor::Approved(id) => id,
        }
    }

    fn is_agent(&self) -> bool {
        matches!(self, Actor::Agent(_))
    }
}

/// Explicit allowlist of non-agent identities that may decide approvals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApproverAllowlist {
    user: String,
    approved: BTreeSet<String>,
}

impl ApproverAllowlist {
    /// Build an allowlist from one user and any approved non-agent identities.
    pub fn new<I, S>(user: impl Into<String>, approved: I) -> Result<Self, ApprovalError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let user = user.into();
        let approved: BTreeSet<String> = approved.into_iter().map(Into::into).collect();
        if user.is_empty() || approved.contains("") {
            return Err(ApprovalError::InvalidActor);
        }
        Ok(Self { user, approved })
    }

    /// Whether the supplied identity is an allowlisted non-agent actor.
    pub fn allows(&self, actor: &Actor) -> bool {
        match actor {
            Actor::User(id) => id == &self.user,
            Actor::Approved(id) => self.approved.contains(id),
            Actor::Agent(_) => false,
        }
    }
}

/// Persisted lifecycle state of an approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ApprovalState {
    /// Awaiting a decision.
    Requested,
    /// Approved by an allowlisted non-agent identity.
    Approved { by: Actor },
    /// Rejected by an allowlisted non-agent identity.
    Rejected,
    /// Replaced by a newer action. The prior approver remains recorded.
    Superseded { approved_by: Option<Actor> },
}

impl ApprovalState {
    /// Return the state name without decision details.
    pub const fn kind(&self) -> ApprovalStateKind {
        match self {
            ApprovalState::Requested => ApprovalStateKind::Requested,
            ApprovalState::Approved { .. } => ApprovalStateKind::Approved,
            ApprovalState::Rejected => ApprovalStateKind::Rejected,
            ApprovalState::Superseded { .. } => ApprovalStateKind::Superseded,
        }
    }
}

/// State names used in typed transition errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStateKind {
    /// Awaiting a decision.
    Requested,
    /// Approved.
    Approved,
    /// Rejected.
    Rejected,
    /// Replaced by a newer action.
    Superseded,
}

/// Event names used in typed transition errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalEventKind {
    /// Create a pending approval.
    Requested,
    /// Approve a pending action.
    Approved,
    /// Reject a pending action.
    Rejected,
    /// Replace a pending or approved action.
    Superseded,
}

/// One requested or decided approval transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ApprovalEvent {
    /// Create a pending approval.
    Requested { by: Actor },
    /// Approve a pending action.
    Approved { by: Actor },
    /// Reject a pending action.
    Rejected { by: Actor },
    /// Supersede a pending or approved action.
    Superseded { by: Actor },
}

impl ApprovalEvent {
    /// Return the stable event kind stored beside the JSON payload.
    pub const fn kind(&self) -> &'static str {
        match self {
            ApprovalEvent::Requested { .. } => "requested",
            ApprovalEvent::Approved { .. } => "approved",
            ApprovalEvent::Rejected { .. } => "rejected",
            ApprovalEvent::Superseded { .. } => "superseded",
        }
    }

    fn event_kind(&self) -> ApprovalEventKind {
        match self {
            ApprovalEvent::Requested { .. } => ApprovalEventKind::Requested,
            ApprovalEvent::Approved { .. } => ApprovalEventKind::Approved,
            ApprovalEvent::Rejected { .. } => ApprovalEventKind::Rejected,
            ApprovalEvent::Superseded { .. } => ApprovalEventKind::Superseded,
        }
    }

    fn actor(&self) -> &Actor {
        match self {
            ApprovalEvent::Requested { by }
            | ApprovalEvent::Approved { by }
            | ApprovalEvent::Rejected { by }
            | ApprovalEvent::Superseded { by } => by,
        }
    }
}

/// One approval and its exact action binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approval {
    /// Stable approval identifier.
    pub id: String,
    /// Action dimensions that must match at check time.
    pub action: ApprovalAction,
    /// Current lifecycle state.
    pub state: ApprovalState,
}

/// Dimension that differs between an approved action and a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionMismatch {
    /// The action kind changed.
    Kind,
    /// The canonical parameter digest changed.
    Parameters,
    /// The candidate commit changed.
    CandidateCommit,
    /// The task id changed.
    Task,
}

/// Errors from approval transitions, actor checks, and persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalError {
    /// The approval id is empty.
    InvalidApprovalId,
    /// The action task id is empty.
    InvalidTaskId,
    /// The candidate commit is not a full lowercase SHA-1 or SHA-256.
    InvalidCandidateCommit,
    /// The actor identity is empty.
    InvalidActor,
    /// The actor is not an allowlisted non-agent identity.
    UnauthorizedActor { actor_id: String },
    /// An agent proposed a policy change.
    UnauthorizedPolicyChange { actor_id: String },
    /// An agent attempted to approve or reject an action.
    AgentCannotDecide { actor_id: String },
    /// No legal edge exists from the current state for this event.
    IllegalTransition {
        from: Option<ApprovalStateKind>,
        event: ApprovalEventKind,
    },
    /// The approval is not in the approved state.
    NotApproved {
        approval_id: String,
        state: ApprovalStateKind,
    },
    /// One bound action dimension changed.
    ActionMismatch(ActionMismatch),
    /// The approval id has no persisted row.
    NotFound { approval_id: String },
    /// SQLite or serialization failed.
    Persistence(String),
}

impl fmt::Display for ApprovalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApprovalError::InvalidApprovalId => f.write_str("approval id must not be empty"),
            ApprovalError::InvalidTaskId => f.write_str("approval task id must not be empty"),
            ApprovalError::InvalidCandidateCommit => {
                f.write_str("candidate commit must be a full lowercase Git SHA-1 or SHA-256")
            }
            ApprovalError::InvalidActor => f.write_str("actor identity must not be empty"),
            ApprovalError::UnauthorizedActor { actor_id } => {
                write!(f, "actor {actor_id:?} is not an allowlisted non-agent")
            }
            ApprovalError::UnauthorizedPolicyChange { actor_id } => {
                write!(f, "actor {actor_id:?} cannot propose a policy change")
            }
            ApprovalError::AgentCannotDecide { actor_id } => {
                write!(f, "agent {actor_id:?} cannot approve or reject an action")
            }
            ApprovalError::IllegalTransition { from, event } => {
                write!(f, "illegal approval transition from {from:?} for {event:?}")
            }
            ApprovalError::NotApproved { approval_id, state } => {
                write!(f, "approval {approval_id:?} is {state:?}, not approved")
            }
            ApprovalError::ActionMismatch(mismatch) => {
                write!(f, "approved action differs in {mismatch:?}")
            }
            ApprovalError::NotFound { approval_id } => {
                write!(f, "approval {approval_id:?} not found")
            }
            ApprovalError::Persistence(detail) => {
                write!(f, "approval persistence failed: {detail}")
            }
        }
    }
}

impl std::error::Error for ApprovalError {}

/// Apply one legal approval lifecycle edge.
///
/// Request events may originate from agents. Decision events cannot. The
/// persistence layer validates every decision actor against its allowlist.
pub fn transition(
    current: Option<&ApprovalState>,
    event: &ApprovalEvent,
) -> Result<ApprovalState, ApprovalError> {
    let actor = event.actor();
    if actor.id().is_empty() {
        return Err(ApprovalError::InvalidActor);
    }
    if matches!(
        event,
        ApprovalEvent::Approved { .. } | ApprovalEvent::Rejected { .. }
    ) && actor.is_agent()
    {
        return Err(ApprovalError::AgentCannotDecide {
            actor_id: actor.id().to_owned(),
        });
    }

    match (current, event) {
        (None, ApprovalEvent::Requested { .. }) => Ok(ApprovalState::Requested),
        (Some(ApprovalState::Requested), ApprovalEvent::Approved { by }) => {
            Ok(ApprovalState::Approved { by: by.clone() })
        }
        (Some(ApprovalState::Requested), ApprovalEvent::Rejected { .. }) => {
            Ok(ApprovalState::Rejected)
        }
        (Some(ApprovalState::Requested), ApprovalEvent::Superseded { .. }) => {
            Ok(ApprovalState::Superseded { approved_by: None })
        }
        (Some(ApprovalState::Approved { by: approved_by }), ApprovalEvent::Superseded { .. }) => {
            Ok(ApprovalState::Superseded {
                approved_by: Some(approved_by.clone()),
            })
        }
        _ => Err(ApprovalError::IllegalTransition {
            from: current.map(ApprovalState::kind),
            event: event.event_kind(),
        }),
    }
}

/// Require an approved record whose action binding exactly matches a proposal.
pub fn check(approval: &Approval, proposed: &ApprovalAction) -> Result<(), ApprovalError> {
    if !matches!(approval.state, ApprovalState::Approved { .. }) {
        return Err(ApprovalError::NotApproved {
            approval_id: approval.id.clone(),
            state: approval.state.kind(),
        });
    }
    if let Some(mismatch) = approval.action.mismatch(proposed) {
        return Err(ApprovalError::ActionMismatch(mismatch));
    }
    Ok(())
}
