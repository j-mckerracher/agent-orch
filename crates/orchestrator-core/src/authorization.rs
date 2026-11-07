//! Authorization invariants (plan section 13, "Required controls").
//!
//! Two things live here: the *envelope* a caller (coordinator, user, or the
//! engine acting on a user request) is authorized to grant — which autonomy
//! policies and which delivery choices — and *approval integrity*: an
//! approval binds the exact action, its destination and parameters, the
//! candidate commit, and the approving actor, and any change after approval
//! requires a new approval.
//!
//! The engine is the only actor that *applies* an authorization; everything
//! else *proposes* against it.

use crate::contract::{AutonomyPolicy, DeliveryPolicy, VerificationPolicy};
use std::collections::BTreeSet;

/// The bounds a caller is authorized to grant. A proposal may stay within the
/// envelope; it may never exceed it.
///
/// This is a value, not a grant: holding an `Authorization` does not let the
/// holder act. The engine checks proposals against it at submission and
/// records the check in the audit journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authorization {
    /// The autonomy a proposed contract may request. A proposal requesting
    /// workspace writes is denied unless this autonomy grants them; `ApprovalRequired`
    /// is a separate policy dimension, not a rank above the others.
    pub autonomy: AutonomyPolicy,
    /// The delivery choices a proposed contract may select. A proposal for a
    /// delivery not in this set is denied. `LocalReview` never implies
    /// `PullRequest`; each choice is authorized independently.
    pub delivery: BTreeSet<DeliveryPolicy>,
    /// The verification gate authorized for this proposal.
    pub verification: VerificationPolicy,
}

impl Authorization {
    /// Whether the envelope explicitly authorizes `d`.
    pub fn delivery_grants(&self, d: DeliveryPolicy) -> bool {
        self.delivery.contains(&d)
    }
}

/// The kind of gated operation an approval authorizes, in the same vocabulary
/// the engine uses for delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    /// Integrate a verified candidate into a local branch (local-review
    /// delivery).
    LocalReviewMerge,
    /// Push a branch and open a pull request (PR delivery). Creating the PR
    /// is not a merge.
    PullRequestCreate,
    /// Publish a report artifact (report-only delivery).
    ReportPublish,
}

/// Where a gated operation applies. Typed so a destination change is a
/// visible, comparable difference — not a free string an agent could
/// reformat to slip past an equality check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// Integrate into a named local branch of the primary checkout.
    LocalBranch { branch: String },
    /// Open a PR against a named remote and target branch.
    PullRequest {
        remote: String,
        target_branch: String,
    },
    /// Write a report artifact to a path.
    ReportArtifact { path: String },
}

/// The exact gated action an approval was granted for. *Every* field must
/// match at application time; a change to the kind, the destination, any
/// parameter, the candidate commit, or the task voids the match. This is the
/// invariant that stops a worker from approving one diff for one destination
/// and delivering another (plan section 13, "Approval integrity").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub kind: ActionKind,
    /// Destination and operation parameters, fully bound.
    pub destination: Destination,
    /// Every remaining operation parameter must be represented here and bound
    /// to the approval. Delivery implementations must not accept unbound inputs.
    pub parameters: std::collections::BTreeMap<String, String>,
    /// The exact candidate commit the approval was reviewed against.
    pub candidate_commit: String,
    /// The task the approval applies to. Approvals are not transferable
    /// between tasks.
    pub task_id: String,
}

impl Action {
    /// Field-by-field comparison so a failure names which dimension drifted.
    fn matches(&self, other: &Action) -> Result<(), ActionMismatch> {
        if self.kind != other.kind {
            return Err(ActionMismatch::Kind);
        }
        if self.destination != other.destination {
            return Err(ActionMismatch::Destination);
        }
        if self.parameters != other.parameters {
            return Err(ActionMismatch::Parameters);
        }
        if self.candidate_commit != other.candidate_commit {
            return Err(ActionMismatch::CandidateCommit);
        }
        if self.task_id != other.task_id {
            return Err(ActionMismatch::Task);
        }
        Ok(())
    }
}

/// Which dimension of a bound action failed to match on application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionMismatch {
    /// A different operation kind was requested than approved.
    Kind,
    /// A different destination (or parameter) was requested than approved.
    Destination,
    /// An operation parameter changed after approval.
    Parameters,
    /// The candidate changed after the approval was granted.
    CandidateCommit,
    /// The approval is being applied to a different task.
    Task,
}

/// A granted approval. Terminal once resolved; a resolved approval is
/// evidence, not a reusable token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approval {
    pub id: String,
    pub action: Action,
    /// The approving actor. For v1 this is the human user; the engine records
    /// the identity but never the raw token (plan section 13, "Environment
    /// handling").
    pub granted_by: String,
    pub state: ApprovalState,
}

/// Lifecycle of an approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalState {
    /// Granted and not yet consumed.
    Granted,
    /// Consumed exactly once, for the bound action. Terminal.
    Consumed,
    /// The bound action or candidate changed after granting; the approval is
    /// void and must be re-granted. Terminal.
    Voided,
    /// The task was cancelled before the approval was consumed. Terminal.
    Expired,
}

impl ApprovalState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            ApprovalState::Consumed | ApprovalState::Voided | ApprovalState::Expired
        )
    }
}

/// The result of attempting to apply an approval to a concrete action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyError {
    /// The approval is not in the `Granted` state.
    NotGranted,
    /// Some dimension of the action does not match the approval. The approval
    /// is not consumed; it remains `Granted` for the exact bound action, or
    /// should be `Voided` if the candidate is permanently changed.
    Mismatch(ActionMismatch),
}

/// Check whether `approval` may be applied to `action`.
///
/// This is a pure check: it does not consume the approval. The caller (the
/// delivery step) transitions the approval to `Consumed` in the same
/// transaction it records the delivery, so a crash between check and consume
/// cannot produce a double-delivery.
pub fn apply(approval: &Approval, action: &Action) -> Result<(), ApplyError> {
    if approval.state != ApprovalState::Granted {
        return Err(ApplyError::NotGranted);
    }
    approval
        .action
        .matches(action)
        .map_err(ApplyError::Mismatch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action() -> Action {
        Action {
            kind: ActionKind::LocalReviewMerge,
            destination: Destination::LocalBranch {
                branch: "main".into(),
            },
            parameters: std::collections::BTreeMap::new(),
            candidate_commit: "abc123".into(),
            task_id: "t1".into(),
        }
    }

    fn approval() -> Approval {
        Approval {
            id: "ap1".into(),
            action: action(),
            granted_by: "user".into(),
            state: ApprovalState::Granted,
        }
    }

    #[test]
    fn matching_granted_approval_applies() {
        assert_eq!(apply(&approval(), &action()), Ok(()));
    }

    #[test]
    fn changed_candidate_commit_denied() {
        let mut a = action();
        a.candidate_commit = "def456".into();
        assert_eq!(
            apply(&approval(), &a),
            Err(ApplyError::Mismatch(ActionMismatch::CandidateCommit))
        );
    }

    #[test]
    fn changed_destination_denied() {
        let mut a = action();
        a.destination = Destination::LocalBranch {
            branch: "release".into(),
        };
        assert_eq!(
            apply(&approval(), &a),
            Err(ApplyError::Mismatch(ActionMismatch::Destination))
        );
    }

    #[test]
    fn changed_destination_parameters_denied() {
        // Same destination *kind*, different parameter: still a different
        // action.
        let mut a = action();
        a.kind = ActionKind::PullRequestCreate;
        a.destination = Destination::PullRequest {
            remote: "origin".into(),
            target_branch: "main".into(),
        };
        assert_eq!(
            apply(&approval(), &a),
            Err(ApplyError::Mismatch(ActionMismatch::Kind))
        );
    }

    #[test]
    fn changed_operation_parameter_denied() {
        let mut a = action();
        a.parameters.insert("source_branch".into(), "other".into());
        assert_eq!(
            apply(&approval(), &a),
            Err(ApplyError::Mismatch(ActionMismatch::Parameters))
        );
    }

    #[test]
    fn changed_kind_denied() {
        let mut a = action();
        a.kind = ActionKind::ReportPublish;
        a.destination = Destination::ReportArtifact {
            path: "r.md".into(),
        };
        assert_eq!(
            apply(&approval(), &a),
            Err(ApplyError::Mismatch(ActionMismatch::Kind))
        );
    }

    #[test]
    fn approval_is_not_transferable_between_tasks() {
        let mut a = action();
        a.task_id = "t2".into();
        assert_eq!(
            apply(&approval(), &a),
            Err(ApplyError::Mismatch(ActionMismatch::Task))
        );
    }

    #[test]
    fn consumed_approval_cannot_be_reused() {
        let mut ap = approval();
        ap.state = ApprovalState::Consumed;
        assert_eq!(apply(&ap, &action()), Err(ApplyError::NotGranted));
    }

    #[test]
    fn voided_approval_cannot_be_reused() {
        let mut ap = approval();
        ap.state = ApprovalState::Voided;
        assert_eq!(apply(&ap, &action()), Err(ApplyError::NotGranted));
    }

    #[test]
    fn expired_approval_cannot_be_reused() {
        let mut ap = approval();
        ap.state = ApprovalState::Expired;
        assert_eq!(apply(&ap, &action()), Err(ApplyError::NotGranted));
    }

    #[test]
    fn delivery_envelope_grants_are_explicit() {
        let mut e = Authorization {
            autonomy: AutonomyPolicy::WorkspaceWrites,
            delivery: BTreeSet::new(),
            verification: VerificationPolicy::None,
        };
        assert!(!e.delivery_grants(DeliveryPolicy::PullRequest));
        e.delivery.insert(DeliveryPolicy::LocalReview);
        assert!(e.delivery_grants(DeliveryPolicy::LocalReview));
        // Authorizing LocalReview does not grant PullRequest.
        assert!(!e.delivery_grants(DeliveryPolicy::PullRequest));
    }
}
