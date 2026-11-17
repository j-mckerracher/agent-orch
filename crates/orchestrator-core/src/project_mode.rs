//! Per-project defaults: the mode that decides what a task proposed from chat may ask for
//! (DPES-1105, phase 4b).
//!
//! Pure data and rules. A project with no defaults is [`ProjectDefaults::LOCAL`], which
//! behaves as before this module. A mode that sends work off the machine is a loosening:
//! only the user sets it, and chat still needs the separate `chat_pr_delivery` setting
//! to propose a pull request (see `task_envelope_for`).
//!
//! `base_branch` and `branch_prefix` are validated by the caller with `orchestrator-git`
//! (`is_safe_branch_name`, `BranchPrefix::parse`); this crate holds no Git rules.

use crate::contract::{DeliveryPolicy, VerificationPolicy};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Most check commands a project may hold.
pub const MAX_CHECKS: usize = 8;
/// Most arguments in one check command.
pub const MAX_CHECK_ARGS: usize = 16;
/// Longest single check argument, in bytes.
pub const MAX_CHECK_ARG_LEN: usize = 256;

/// How far a project's tasks aim to leave the machine. Ordered by that reach.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectMode {
    /// Review ref only. The default.
    LocalOnly,
    /// Pull request delivery with the project's checks, if any.
    DirectPr,
    /// Pull request delivery with required checks.
    NoMistakes,
}

impl ProjectMode {
    pub const ALL: [Self; 3] = [Self::LocalOnly, Self::DirectPr, Self::NoMistakes];

    /// Stable key: `local-only`, `direct-pr` or `no-mistakes`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalOnly => "local-only",
            Self::DirectPr => "direct-pr",
            Self::NoMistakes => "no-mistakes",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.as_str() == value)
    }

    /// The delivery a task of this mode asks for.
    pub const fn delivery(self) -> DeliveryPolicy {
        match self {
            Self::LocalOnly => DeliveryPolicy::LocalReview,
            Self::DirectPr | Self::NoMistakes => DeliveryPolicy::PullRequest,
        }
    }

    /// Whether the mode runs the project's checks before delivery.
    pub const fn requires_checks(self) -> bool {
        !matches!(self, Self::LocalOnly)
    }
}

/// How a pull request is merged. Only `Squash` is the default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MergeMethod {
    Merge,
    #[default]
    Squash,
    Rebase,
}

impl MergeMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Squash => "squash",
            Self::Rebase => "rebase",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Merge, Self::Squash, Self::Rebase]
            .into_iter()
            .find(|method| method.as_str() == value)
    }
}

/// Why a [`ProjectDefaults`] value is refused. Names no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultsError {
    TooManyChecks,
    EmptyCheck,
    TooManyArgs,
    EmptyArg,
    ArgTooLong,
    NulInArg,
    /// `no-mistakes` with no checks.
    ChecksRequired,
}

impl DefaultsError {
    /// Stable key for a reply.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TooManyChecks => "too_many_checks",
            Self::EmptyCheck => "empty_check",
            Self::TooManyArgs => "too_many_args",
            Self::EmptyArg => "empty_arg",
            Self::ArgTooLong => "arg_too_long",
            Self::NulInArg => "nul_in_arg",
            Self::ChecksRequired => "checks_required",
        }
    }
}

/// The defaults of one project. `Debug` prints the mode and counts only: a check
/// argument or branch name may look like a path or token.
#[derive(Clone, PartialEq, Eq)]
pub struct ProjectDefaults {
    pub mode: ProjectMode,
    /// `None` means HEAD of the primary checkout.
    pub base_branch: Option<String>,
    /// `None` means the machine-wide prefix.
    pub branch_prefix: Option<String>,
    /// Argument arrays, never a shell string.
    pub checks: Vec<Vec<String>>,
    pub merge_method: MergeMethod,
}

impl fmt::Debug for ProjectDefaults {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProjectDefaults")
            .field("mode", &self.mode)
            .field("has_base_branch", &self.base_branch.is_some())
            .field("has_branch_prefix", &self.branch_prefix.is_some())
            .field("checks", &self.checks.len())
            .field("merge_method", &self.merge_method)
            .finish()
    }
}

impl ProjectDefaults {
    /// What a project with no row has: the behavior before per-project defaults.
    pub const LOCAL: ProjectDefaults = ProjectDefaults {
        mode: ProjectMode::LocalOnly,
        base_branch: None,
        branch_prefix: None,
        checks: Vec::new(),
        merge_method: MergeMethod::Squash,
    };

    /// The verification a task of this project asks for: the checks when the mode
    /// runs them and some are set, otherwise none.
    pub fn verification(&self) -> VerificationPolicy {
        if self.mode.requires_checks() && !self.checks.is_empty() {
            VerificationPolicy::Commands {
                commands: self.checks.clone(),
            }
        } else {
            VerificationPolicy::None
        }
    }

    /// Bounds on the checks, and checks present for `no-mistakes`.
    pub fn validate(&self) -> Result<(), DefaultsError> {
        if self.checks.len() > MAX_CHECKS {
            return Err(DefaultsError::TooManyChecks);
        }
        for command in &self.checks {
            if command.is_empty() {
                return Err(DefaultsError::EmptyCheck);
            }
            if command.len() > MAX_CHECK_ARGS {
                return Err(DefaultsError::TooManyArgs);
            }
            for arg in command {
                if arg.is_empty() {
                    return Err(DefaultsError::EmptyArg);
                }
                if arg.len() > MAX_CHECK_ARG_LEN {
                    return Err(DefaultsError::ArgTooLong);
                }
                if arg.contains('\0') {
                    return Err(DefaultsError::NulInArg);
                }
            }
        }
        if self.mode == ProjectMode::NoMistakes && self.checks.is_empty() {
            return Err(DefaultsError::ChecksRequired);
        }
        Ok(())
    }
}

impl Default for ProjectDefaults {
    fn default() -> Self {
        Self::LOCAL
    }
}

/// What a proposal gives up against the project default. Data for the card, never text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drop {
    /// The proposal's delivery is lower than the mode's.
    Delivery {
        default: DeliveryPolicy,
        proposed: DeliveryPolicy,
    },
    /// The project has checks to run and the proposal runs none.
    Checks,
}

/// What a proposal drops below `defaults`. Empty means nothing. Delivery order is
/// `ReportOnly < LocalReview < PullRequest`, for the card only, never for authority.
/// The caller skips a scout: it is a different shape, not a drop.
pub fn drops(
    defaults: &ProjectDefaults,
    delivery: DeliveryPolicy,
    verification: &VerificationPolicy,
) -> Vec<Drop> {
    let mut found = Vec::new();
    let default = defaults.mode.delivery();
    if delivery < default {
        found.push(Drop::Delivery {
            default,
            proposed: delivery,
        });
    }
    if defaults.verification() != VerificationPolicy::None
        && *verification == VerificationPolicy::None
    {
        found.push(Drop::Checks);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checks() -> Vec<Vec<String>> {
        vec![vec!["cargo".to_owned(), "test".to_owned()]]
    }

    fn with(mode: ProjectMode, checks: Vec<Vec<String>>) -> ProjectDefaults {
        ProjectDefaults {
            mode,
            checks,
            ..ProjectDefaults::LOCAL
        }
    }

    #[test]
    fn modes_round_trip_and_map_to_delivery_and_checks() {
        let table = [
            (ProjectMode::LocalOnly, "local-only", DeliveryPolicy::LocalReview, false),
            (ProjectMode::DirectPr, "direct-pr", DeliveryPolicy::PullRequest, true),
            (ProjectMode::NoMistakes, "no-mistakes", DeliveryPolicy::PullRequest, true),
        ];
        for (mode, key, delivery, requires) in table {
            assert_eq!(mode.as_str(), key);
            assert_eq!(ProjectMode::parse(key), Some(mode));
            assert_eq!(mode.delivery(), delivery);
            assert_eq!(mode.requires_checks(), requires);
            assert_eq!(serde_json::to_string(&mode).unwrap(), format!("\"{key}\""));
        }
        assert!(ProjectMode::LocalOnly < ProjectMode::DirectPr);
        assert!(ProjectMode::DirectPr < ProjectMode::NoMistakes);
        assert_eq!(ProjectMode::parse("Direct-PR"), None);
        assert_eq!(ProjectMode::parse(""), None);
    }

    #[test]
    fn merge_methods_round_trip_and_default_to_squash() {
        assert_eq!(MergeMethod::default(), MergeMethod::Squash);
        for method in [MergeMethod::Merge, MergeMethod::Squash, MergeMethod::Rebase] {
            assert_eq!(MergeMethod::parse(method.as_str()), Some(method));
        }
        assert_eq!(MergeMethod::parse("fast-forward"), None);
    }

    #[test]
    fn the_local_defaults_are_local_only_with_nothing_set() {
        let local = ProjectDefaults::LOCAL;
        assert_eq!(local.mode, ProjectMode::LocalOnly);
        assert_eq!(local.base_branch, None);
        assert_eq!(local.branch_prefix, None);
        assert!(local.checks.is_empty());
        assert_eq!(local.merge_method, MergeMethod::Squash);
        assert_eq!(local.verification(), VerificationPolicy::None);
        assert_eq!(local.validate(), Ok(()));
        assert_eq!(ProjectDefaults::default(), local);
    }

    #[test]
    fn verification_follows_the_mode_and_the_checks() {
        let commands = VerificationPolicy::Commands { commands: checks() };
        assert_eq!(with(ProjectMode::LocalOnly, checks()).verification(), VerificationPolicy::None);
        assert_eq!(with(ProjectMode::DirectPr, Vec::new()).verification(), VerificationPolicy::None);
        assert_eq!(with(ProjectMode::DirectPr, checks()).verification(), commands);
        assert_eq!(with(ProjectMode::NoMistakes, checks()).verification(), commands);
    }

    #[test]
    fn validate_refuses_each_unbounded_check_and_no_mistakes_without_checks() {
        let arg = |n: usize| "a".repeat(n);
        type Case = (Vec<Vec<String>>, Result<(), DefaultsError>);
        let table: [Case; 8] = [
            (vec![vec![arg(1)]; MAX_CHECKS], Ok(())),
            (vec![vec![arg(1)]; MAX_CHECKS + 1], Err(DefaultsError::TooManyChecks)),
            (vec![Vec::new()], Err(DefaultsError::EmptyCheck)),
            (vec![vec![arg(1); MAX_CHECK_ARGS + 1]], Err(DefaultsError::TooManyArgs)),
            (vec![vec![String::new()]], Err(DefaultsError::EmptyArg)),
            (vec![vec![arg(MAX_CHECK_ARG_LEN + 1)]], Err(DefaultsError::ArgTooLong)),
            (vec![vec!["a\0b".to_owned()]], Err(DefaultsError::NulInArg)),
            (vec![vec![arg(MAX_CHECK_ARG_LEN); MAX_CHECK_ARGS]], Ok(())),
        ];
        for (checks, expected) in table {
            assert_eq!(with(ProjectMode::DirectPr, checks).validate(), expected);
        }
        assert_eq!(
            with(ProjectMode::NoMistakes, Vec::new()).validate(),
            Err(DefaultsError::ChecksRequired)
        );
        assert_eq!(with(ProjectMode::NoMistakes, checks()).validate(), Ok(()));
        assert_eq!(with(ProjectMode::DirectPr, Vec::new()).validate(), Ok(()));
        assert_eq!(DefaultsError::ChecksRequired.as_str(), "checks_required");
    }

    #[test]
    fn debug_prints_names_and_counts_never_values() {
        let defaults = ProjectDefaults {
            base_branch: Some("secret-base".to_owned()),
            branch_prefix: Some("secret-prefix/".to_owned()),
            checks: vec![vec!["/home/me/token".to_owned()]],
            ..ProjectDefaults::LOCAL
        };
        let text = format!("{defaults:?}");
        for leak in ["secret", "token", "/home"] {
            assert!(!text.contains(leak), "{text}");
        }
        assert!(text.contains("LocalOnly"));
    }

    #[test]
    fn drops_lists_every_way_a_proposal_falls_below_the_default() {
        let commands = VerificationPolicy::Commands { commands: checks() };
        let none = VerificationPolicy::None;
        let local = DeliveryPolicy::LocalReview;
        let pr = DeliveryPolicy::PullRequest;
        let report = DeliveryPolicy::ReportOnly;
        let delivery = |default, proposed| Drop::Delivery { default, proposed };

        // A project with no defaults: nothing is below the default.
        for proposed in [report, local, pr] {
            let expected = if proposed < local { vec![delivery(local, proposed)] } else { vec![] };
            assert_eq!(drops(&ProjectDefaults::LOCAL, proposed, &none), expected);
        }
        let direct = with(ProjectMode::DirectPr, checks());
        assert_eq!(drops(&direct, pr, &commands), vec![]);
        assert_eq!(drops(&direct, local, &commands), vec![delivery(pr, local)]);
        assert_eq!(drops(&direct, local, &none), vec![delivery(pr, local), Drop::Checks]);
        assert_eq!(drops(&direct, pr, &none), vec![Drop::Checks]);
        assert_eq!(drops(&direct, report, &commands), vec![delivery(pr, report)]);
        // No checks set, so none to drop.
        let bare = with(ProjectMode::DirectPr, Vec::new());
        assert_eq!(drops(&bare, pr, &none), vec![]);
        assert_eq!(drops(&bare, local, &none), vec![delivery(pr, local)]);
        // A local-only project with checks set does not run them, so none to drop.
        assert_eq!(drops(&with(ProjectMode::LocalOnly, checks()), local, &none), vec![]);
    }
}
