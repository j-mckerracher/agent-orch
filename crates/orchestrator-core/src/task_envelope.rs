//! The envelope a task proposed from chat is checked against (DPES-1105).
//!
//! A `task.propose` action asks the app to create a task. The agent chooses the
//! objective, kind, agent and delivery. Autonomy and verification never come
//! from the agent: [`task_proposal`] takes both from the envelope, and the
//! proposal sets no permission, so chat can never raise any of them. The
//! envelope is least-authority; [`task_envelope_for`] widens it only from a
//! project's defaults and the `chat_pr_delivery` setting.
//! A refused proposal gets a [`Denial`] key and never reaches the user as an
//! approvable card.
//!
//! Design: `docs/design/orchestrator-actions.md`.

use crate::authorization::Authorization;
use crate::contract::{
    validate_proposal, AutonomyPolicy, DeliveryPolicy, TaskKind, TaskProposal, Validation,
    VerificationPolicy,
};
use crate::orchestrator_action::ProposeTask;
use crate::project_mode::ProjectDefaults;

/// `error_code` of a proposal the envelope refused.
pub const ACTION_DENIED: &str = "action_denied";
/// Retries for a chat-proposed task. Equals the desktop form's default.
pub const CHAT_TASK_MAX_RETRIES: u32 = 1;
/// Repair attempts for a chat-proposed task. Equals the desktop form's default.
pub const CHAT_TASK_MAX_REPAIRS: u32 = 1;

/// What a task proposed from chat may ask for: approval required, report-only
/// or local-review delivery, and no verification commands. Pull-request
/// delivery stays a by-hand choice in the Agents view.
pub fn chat_task_envelope() -> Authorization {
    Authorization {
        autonomy: AutonomyPolicy::ApprovalRequired,
        delivery: [DeliveryPolicy::ReportOnly, DeliveryPolicy::LocalReview]
            .into_iter()
            .collect(),
        verification: VerificationPolicy::None,
    }
}

/// The envelope for a task of `kind` in a project with `defaults` (`None` means no row).
/// `chat_pr_delivery` is the global setting that lets chat ask for a pull request; it
/// is off by default and only the user turns it on.
///
/// Least authority first: approval always required; an `Investigate` task (a scout)
/// is `{ReportOnly}` with no verification in every mode; an `Implement` task gets
/// `{ReportOnly, LocalReview}`, plus `PullRequest` only for a pull-request mode with
/// `chat_pr_delivery` on, and the project's verification. A pull-request mode with the
/// setting off changes the card, never what chat may ask.
pub fn task_envelope_for(
    defaults: Option<&ProjectDefaults>,
    chat_pr_delivery: bool,
    kind: TaskKind,
) -> Authorization {
    let local = chat_task_envelope();
    let Some(defaults) = defaults else {
        return match kind {
            TaskKind::Implement => local,
            TaskKind::Investigate => report_only(local),
        };
    };
    match kind {
        TaskKind::Investigate => report_only(local),
        TaskKind::Implement => {
            let mut envelope = Authorization {
                verification: defaults.verification(),
                ..local
            };
            if chat_pr_delivery && defaults.mode.delivery() == DeliveryPolicy::PullRequest {
                envelope.delivery.insert(DeliveryPolicy::PullRequest);
            }
            envelope
        }
    }
}

fn report_only(envelope: Authorization) -> Authorization {
    Authorization {
        delivery: [DeliveryPolicy::ReportOnly].into_iter().collect(),
        verification: VerificationPolicy::None,
        ..envelope
    }
}

/// The proposal [`validate_proposal`] checks. The agent chooses the objective,
/// kind, agent and delivery. Autonomy and verification come from `envelope`,
/// the attempt limits are fixed, and there are no dependencies.
pub fn task_proposal(action: &ProposeTask, envelope: &Authorization) -> TaskProposal {
    TaskProposal {
        objective: action.objective.clone(),
        kind: action.task_kind,
        agent: action.agent.clone(),
        dependencies: Vec::new(),
        verification: envelope.verification.clone(),
        delivery: action.delivery,
        autonomy: envelope.autonomy,
        max_retries: CHAT_TASK_MAX_RETRIES,
        max_repairs: CHAT_TASK_MAX_REPAIRS,
    }
}

/// Why the envelope refused a proposal. [`Denial::as_str`] is the stable key
/// stored in `action_proposals.denial`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Denial {
    Autonomy,
    Verification,
    Delivery,
    Contract,
}

impl Denial {
    /// Stable key, such as `delivery_not_authorized`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Autonomy => "autonomy_not_authorized",
            Self::Verification => "verification_not_authorized",
            Self::Delivery => "delivery_not_authorized",
            Self::Contract => "contract_invalid",
        }
    }
}

/// Runs [`validate_proposal`] on the proposal built from `action`.
pub fn check_task(action: &ProposeTask, envelope: &Authorization) -> Result<(), Denial> {
    refuse(&task_proposal(action, envelope), envelope)
}

/// Maps the verdict on `proposal` to a denial. A `Denied` verdict names no
/// field, so the key comes from comparing the fields in the order
/// `validate_proposal` checks them: autonomy, verification, then delivery.
/// An `Invalid` verdict is a contract denial.
fn refuse(proposal: &TaskProposal, envelope: &Authorization) -> Result<(), Denial> {
    match validate_proposal(proposal, envelope) {
        Validation::Accept => Ok(()),
        Validation::Invalid(_) => Err(Denial::Contract),
        Validation::Denied { .. } => Err(if proposal.autonomy != envelope.autonomy {
            Denial::Autonomy
        } else if proposal.verification != envelope.verification {
            Denial::Verification
        } else {
            Denial::Delivery
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_mode::ProjectMode;

    fn propose(delivery: DeliveryPolicy) -> ProposeTask {
        ProposeTask {
            project: "p-3fa9c1d2".to_owned(),
            objective: "Add unit tests for the retry module.".to_owned(),
            agent: "codex".to_owned(),
            task_kind: TaskKind::Implement,
            delivery,
            workstream_id: None,
            workstream_ref: None,
            objective_id: None,
            promote_from: None,
        }
    }

    fn commands() -> VerificationPolicy {
        VerificationPolicy::Commands {
            commands: vec![vec!["cargo".to_owned(), "test".to_owned()]],
        }
    }

    #[test]
    fn the_chat_envelope_grants_review_and_report_delivery_and_nothing_else() {
        let envelope = chat_task_envelope();
        assert_eq!(envelope.autonomy, AutonomyPolicy::ApprovalRequired);
        assert!(!envelope.autonomy.grants_workspace_writes());
        assert_eq!(envelope.verification, VerificationPolicy::None);
        assert_eq!(envelope.delivery.len(), 2);
        assert!(envelope.delivery_grants(DeliveryPolicy::ReportOnly));
        assert!(envelope.delivery_grants(DeliveryPolicy::LocalReview));
        assert!(!envelope.delivery_grants(DeliveryPolicy::PullRequest));
    }

    #[test]
    fn a_project_with_no_defaults_gets_the_chat_envelope_unchanged() {
        for setting in [false, true] {
            assert_eq!(
                task_envelope_for(None, setting, TaskKind::Implement),
                chat_task_envelope()
            );
            assert_eq!(
                task_envelope_for(Some(&ProjectDefaults::LOCAL), setting, TaskKind::Implement),
                chat_task_envelope()
            );
        }
    }

    fn defaults(mode: ProjectMode) -> ProjectDefaults {
        ProjectDefaults {
            mode,
            checks: vec![vec!["cargo".to_owned(), "test".to_owned()]],
            ..ProjectDefaults::LOCAL
        }
    }

    #[test]
    fn the_scout_envelope_is_report_only_in_every_mode() {
        let expected = Authorization {
            autonomy: AutonomyPolicy::ApprovalRequired,
            delivery: [DeliveryPolicy::ReportOnly].into_iter().collect(),
            verification: VerificationPolicy::None,
        };
        for setting in [false, true] {
            assert_eq!(
                task_envelope_for(None, setting, TaskKind::Investigate),
                expected
            );
            for mode in ProjectMode::ALL {
                assert_eq!(
                    task_envelope_for(Some(&defaults(mode)), setting, TaskKind::Investigate),
                    expected,
                    "{mode:?} {setting}"
                );
            }
        }
    }

    #[test]
    fn the_ship_envelope_follows_mode_and_the_pr_setting() {
        let pr_modes = [ProjectMode::DirectPr, ProjectMode::NoMistakes];
        for mode in ProjectMode::ALL {
            let project = defaults(mode);
            for setting in [false, true] {
                let envelope = task_envelope_for(Some(&project), setting, TaskKind::Implement);
                let pr = pr_modes.contains(&mode) && setting;
                assert_eq!(envelope.autonomy, AutonomyPolicy::ApprovalRequired);
                assert_eq!(envelope.verification, project.verification(), "{mode:?}");
                assert!(envelope.delivery_grants(DeliveryPolicy::ReportOnly));
                assert!(envelope.delivery_grants(DeliveryPolicy::LocalReview));
                assert_eq!(
                    envelope.delivery_grants(DeliveryPolicy::PullRequest),
                    pr,
                    "{mode:?} {setting}"
                );
                // The proposal check agrees with the envelope.
                assert_eq!(
                    check_task(&propose(DeliveryPolicy::PullRequest), &envelope),
                    if pr { Ok(()) } else { Err(Denial::Delivery) }
                );
                assert_eq!(
                    check_task(&propose(DeliveryPolicy::LocalReview), &envelope),
                    Ok(())
                );
            }
        }
        // Checks are carried to the proposal for a mode that runs them, never for local-only.
        assert_eq!(
            task_envelope_for(
                Some(&defaults(ProjectMode::LocalOnly)),
                true,
                TaskKind::Implement
            )
            .verification,
            VerificationPolicy::None
        );
        assert_ne!(
            task_envelope_for(
                Some(&defaults(ProjectMode::DirectPr)),
                false,
                TaskKind::Implement
            )
            .verification,
            VerificationPolicy::None
        );
    }

    #[test]
    fn a_pull_request_proposal_is_denied_with_the_delivery_key() {
        let envelope = chat_task_envelope();
        let denied = check_task(&propose(DeliveryPolicy::PullRequest), &envelope);
        assert_eq!(denied, Err(Denial::Delivery));
        assert_eq!(Denial::Delivery.as_str(), "delivery_not_authorized");
        for delivery in [DeliveryPolicy::ReportOnly, DeliveryPolicy::LocalReview] {
            assert_eq!(check_task(&propose(delivery), &envelope), Ok(()));
        }
    }

    #[test]
    fn a_mismatched_autonomy_or_verification_is_denied_with_its_own_key() {
        // The proposal is built from the chat envelope and checked against a wider one.
        let action = propose(DeliveryPolicy::LocalReview);
        let proposal = task_proposal(&action, &chat_task_envelope());

        let writes = Authorization {
            autonomy: AutonomyPolicy::WorkspaceWrites,
            ..chat_task_envelope()
        };
        assert_eq!(refuse(&proposal, &writes), Err(Denial::Autonomy));
        assert_eq!(Denial::Autonomy.as_str(), "autonomy_not_authorized");

        let gated = Authorization {
            verification: commands(),
            ..chat_task_envelope()
        };
        assert_eq!(refuse(&proposal, &gated), Err(Denial::Verification));
        assert_eq!(Denial::Verification.as_str(), "verification_not_authorized");

        // A proposal that asks for more than the chat envelope is denied the same way.
        let mut greedy = proposal.clone();
        greedy.autonomy = AutonomyPolicy::WorkspaceWrites;
        assert_eq!(
            refuse(&greedy, &chat_task_envelope()),
            Err(Denial::Autonomy)
        );
        let mut greedy = proposal;
        greedy.verification = commands();
        assert_eq!(
            refuse(&greedy, &chat_task_envelope()),
            Err(Denial::Verification)
        );
    }

    #[test]
    fn an_empty_objective_is_a_contract_denial() {
        let envelope = chat_task_envelope();
        for objective in ["", "   \n\t", "fix \u{202E}this", "fix\u{7}this"] {
            let action = ProposeTask {
                objective: objective.to_owned(),
                ..propose(DeliveryPolicy::LocalReview)
            };
            assert_eq!(
                check_task(&action, &envelope),
                Err(Denial::Contract),
                "{objective:?}"
            );
        }
        assert_eq!(Denial::Contract.as_str(), "contract_invalid");
    }

    #[test]
    fn the_denial_key_matches_the_first_check_validate_proposal_fails() {
        let envelope = chat_task_envelope();
        // Fails all four checks at once; each step fixes the one that fired.
        let mut proposal = TaskProposal {
            objective: "  ".to_owned(),
            kind: TaskKind::Implement,
            agent: "codex".to_owned(),
            dependencies: Vec::new(),
            verification: commands(),
            delivery: DeliveryPolicy::PullRequest,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: CHAT_TASK_MAX_RETRIES,
            max_repairs: CHAT_TASK_MAX_REPAIRS,
        };
        let verdict = |proposal: &TaskProposal| validate_proposal(proposal, &envelope);

        assert!(matches!(verdict(&proposal), Validation::Denied { .. }));
        assert_eq!(refuse(&proposal, &envelope), Err(Denial::Autonomy));
        proposal.autonomy = envelope.autonomy;

        assert!(matches!(verdict(&proposal), Validation::Denied { .. }));
        assert_eq!(refuse(&proposal, &envelope), Err(Denial::Verification));
        proposal.verification = envelope.verification.clone();

        assert!(matches!(verdict(&proposal), Validation::Denied { .. }));
        assert_eq!(refuse(&proposal, &envelope), Err(Denial::Delivery));
        proposal.delivery = DeliveryPolicy::LocalReview;

        assert!(matches!(verdict(&proposal), Validation::Invalid(_)));
        assert_eq!(refuse(&proposal, &envelope), Err(Denial::Contract));
        proposal.objective = "Add unit tests".to_owned();

        assert_eq!(verdict(&proposal), Validation::Accept);
        assert_eq!(refuse(&proposal, &envelope), Ok(()));
    }

    #[test]
    fn task_proposal_takes_autonomy_and_verification_from_the_envelope() {
        let action = ProposeTask {
            task_kind: TaskKind::Investigate,
            delivery: DeliveryPolicy::ReportOnly,
            workstream_id: Some("ws-1".to_owned()),
            ..propose(DeliveryPolicy::ReportOnly)
        };
        let envelopes = [
            chat_task_envelope(),
            Authorization {
                autonomy: AutonomyPolicy::WorkspaceWrites,
                verification: commands(),
                ..chat_task_envelope()
            },
        ];
        for envelope in envelopes {
            let proposal = task_proposal(&action, &envelope);
            assert_eq!(proposal.autonomy, envelope.autonomy);
            assert_eq!(proposal.verification, envelope.verification);
            assert_eq!(proposal.objective, action.objective);
            assert_eq!(proposal.agent, action.agent);
            assert_eq!(proposal.kind, TaskKind::Investigate);
            assert_eq!(proposal.delivery, DeliveryPolicy::ReportOnly);
            assert_eq!(proposal.max_retries, CHAT_TASK_MAX_RETRIES);
            assert_eq!(proposal.max_repairs, CHAT_TASK_MAX_REPAIRS);
            assert!(proposal.dependencies.is_empty());
        }
    }
}
