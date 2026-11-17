//! Landing: the one rule for "may this pull request merge" (DPES-1105, phase 4c).
//!
//! Pure data and one pure function. Nothing here reads a clock, a network or a file: the
//! caller passes in what it saw on the remote and the time. A merge goes ahead only when the
//! pull request is open, its head is the commit the approver bound, the project is in a mode
//! that sends work to GitHub and allows the method, the observation is fresh, GitHub says it
//! is mergeable, and every required check passed. Missing, unknown or contradictory data
//! refuses: no evidence is not green.

pub use crate::project_mode::MergeMethod;
use crate::project_mode::ProjectMode;

/// Longest age of an observation the merge worker accepts, read just before the remote call.
pub const FRESH_MAX_AGE_MS: i64 = 60_000;
/// Longest age of a stored observation a card or an approval accepts. The poller backs off
/// to at most 15 minutes, so an older row means the poller has a problem the card shows.
pub const STORED_MAX_AGE_MS: i64 = 15 * 60 * 1000;

/// State of a pull request on the remote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrState {
    Open,
    Merged,
    Closed,
}

impl PrState {
    /// Stable key: `open`, `merged` or `closed`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Merged => "merged",
            Self::Closed => "closed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Open, Self::Merged, Self::Closed]
            .into_iter()
            .find(|state| state.as_str() == value)
    }
}

/// Result of one required check. Anything the reader cannot place is `Unknown`, which never
/// counts as passed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckState {
    Passed,
    Failed,
    Pending,
    Unknown,
}

impl CheckState {
    /// Stable key: `passed`, `failed`, `pending` or `unknown`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Pending => "pending",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Passed, Self::Failed, Self::Pending, Self::Unknown]
            .into_iter()
            .find(|state| state.as_str() == value)
    }
}

/// What the daemon saw of one pull request, reduced to what the merge rule needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrObservation {
    pub state: PrState,
    /// 40 hex characters.
    pub head_sha: String,
    /// `None` means GitHub has not computed it yet.
    pub mergeable: Option<bool>,
    /// Required checks only. Pending and unknown ones are counted in the total alone.
    pub required_total: u32,
    pub required_passed: u32,
    pub required_failed: u32,
    pub required_unknown: u32,
    /// When the daemon read it, in milliseconds.
    pub observed_ms: i64,
}

impl PrObservation {
    /// Counts `required` into the check fields. A caller that could not learn which checks
    /// are required passes an empty slice, which the verdict refuses as `ChecksMissing`.
    pub fn from_required_checks(
        state: PrState,
        head_sha: &str,
        mergeable: Option<bool>,
        required: &[CheckState],
        observed_ms: i64,
    ) -> Self {
        let count = |wanted: CheckState| {
            u32::try_from(required.iter().filter(|check| **check == wanted).count())
                .unwrap_or(u32::MAX)
        };
        Self {
            state,
            head_sha: head_sha.to_owned(),
            mergeable,
            required_total: u32::try_from(required.len()).unwrap_or(u32::MAX),
            required_passed: count(CheckState::Passed),
            required_failed: count(CheckState::Failed),
            required_unknown: count(CheckState::Unknown),
            observed_ms,
        }
    }
}

/// Why a merge may not go ahead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeRefusal {
    NotOpen,
    HeadMoved,
    ChecksRed,
    ChecksPending,
    ChecksMissing,
    ChecksUnknown,
    NotMergeable,
    Stale,
    ProjectMode,
    MethodNotAllowed,
}

impl MergeRefusal {
    pub const ALL: [Self; 10] = [
        Self::NotOpen,
        Self::HeadMoved,
        Self::ChecksRed,
        Self::ChecksPending,
        Self::ChecksMissing,
        Self::ChecksUnknown,
        Self::NotMergeable,
        Self::Stale,
        Self::ProjectMode,
        Self::MethodNotAllowed,
    ];

    /// Stable key for a card, a denial and a ledger row (at most 64 characters).
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotOpen => "not_open",
            Self::HeadMoved => "head_moved",
            Self::ChecksRed => "checks_red",
            Self::ChecksPending => "checks_pending",
            Self::ChecksMissing => "checks_missing",
            Self::ChecksUnknown => "checks_unknown",
            Self::NotMergeable => "not_mergeable",
            Self::Stale => "stale",
            Self::ProjectMode => "project_mode",
            Self::MethodNotAllowed => "method_not_allowed",
        }
    }
}

/// The only rule for "may this merge go ahead".
///
/// `bound_head` is the commit the approver saw; `method` is the one requested and
/// `project_method` the one the project allows. `max_age_ms` is [`FRESH_MAX_AGE_MS`] for a
/// fresh read and [`STORED_MAX_AGE_MS`] for a stored row. The first failing rule wins, in
/// this order: not open, head moved, project mode, method, stale, mergeable, checks.
pub fn merge_verdict(
    bound_head: &str,
    seen: &PrObservation,
    mode: ProjectMode,
    method: MergeMethod,
    project_method: MergeMethod,
    now_ms: i64,
    max_age_ms: i64,
) -> Result<(), MergeRefusal> {
    if seen.state != PrState::Open {
        return Err(MergeRefusal::NotOpen);
    }
    if seen.head_sha != bound_head {
        return Err(MergeRefusal::HeadMoved);
    }
    if mode == ProjectMode::LocalOnly {
        return Err(MergeRefusal::ProjectMode);
    }
    if method != project_method {
        return Err(MergeRefusal::MethodNotAllowed);
    }
    // An observation from the future cannot be trusted either.
    if seen.observed_ms > now_ms || now_ms.saturating_sub(seen.observed_ms) > max_age_ms {
        return Err(MergeRefusal::Stale);
    }
    match seen.mergeable {
        Some(false) => return Err(MergeRefusal::NotMergeable),
        None => return Err(MergeRefusal::ChecksUnknown),
        Some(true) => {}
    }
    let counted = u64::from(seen.required_passed)
        + u64::from(seen.required_failed)
        + u64::from(seen.required_unknown);
    if counted > u64::from(seen.required_total) {
        return Err(MergeRefusal::ChecksUnknown);
    }
    if seen.required_total == 0 {
        return Err(MergeRefusal::ChecksMissing);
    }
    if seen.required_failed > 0 {
        return Err(MergeRefusal::ChecksRed);
    }
    if seen.required_unknown > 0 {
        return Err(MergeRefusal::ChecksUnknown);
    }
    if seen.required_passed < seen.required_total {
        return Err(MergeRefusal::ChecksPending);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "0123456789abcdef0123456789abcdef01234567";
    const OTHER_HEAD: &str = "fedcba9876543210fedcba9876543210fedcba98";
    const NOW: i64 = 1_794_901_798_000;

    fn green() -> PrObservation {
        PrObservation::from_required_checks(
            PrState::Open,
            HEAD,
            Some(true),
            &[CheckState::Passed, CheckState::Passed],
            NOW - 1_000,
        )
    }

    fn verdict(seen: &PrObservation) -> Result<(), MergeRefusal> {
        merge_verdict(
            HEAD,
            seen,
            ProjectMode::NoMistakes,
            MergeMethod::Squash,
            MergeMethod::Squash,
            NOW,
            FRESH_MAX_AGE_MS,
        )
    }

    #[test]
    fn landing_green_required_checks_on_the_bound_head_merge() {
        assert_eq!(verdict(&green()), Ok(()));
    }

    #[test]
    fn landing_direct_pr_mode_with_green_checks_merges() {
        let result = merge_verdict(
            HEAD,
            &green(),
            ProjectMode::DirectPr,
            MergeMethod::Squash,
            MergeMethod::Squash,
            NOW,
            FRESH_MAX_AGE_MS,
        );
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn landing_a_red_required_check_refuses() {
        let mut seen = green();
        seen.required_passed = 1;
        seen.required_failed = 1;
        assert_eq!(verdict(&seen), Err(MergeRefusal::ChecksRed));
    }

    #[test]
    fn landing_red_wins_over_pending() {
        let seen = PrObservation::from_required_checks(
            PrState::Open,
            HEAD,
            Some(true),
            &[CheckState::Failed, CheckState::Pending, CheckState::Passed],
            NOW,
        );
        assert_eq!(verdict(&seen), Err(MergeRefusal::ChecksRed));
    }

    #[test]
    fn landing_a_pending_required_check_refuses() {
        let seen = PrObservation::from_required_checks(
            PrState::Open,
            HEAD,
            Some(true),
            &[CheckState::Passed, CheckState::Pending],
            NOW,
        );
        assert_eq!(verdict(&seen), Err(MergeRefusal::ChecksPending));
    }

    #[test]
    fn landing_an_unknown_check_state_refuses() {
        let seen = PrObservation::from_required_checks(
            PrState::Open,
            HEAD,
            Some(true),
            &[CheckState::Passed, CheckState::Unknown],
            NOW,
        );
        assert_eq!(verdict(&seen), Err(MergeRefusal::ChecksUnknown));
    }

    #[test]
    fn landing_no_required_checks_refuses_as_missing() {
        let seen = PrObservation::from_required_checks(PrState::Open, HEAD, Some(true), &[], NOW);
        assert_eq!(verdict(&seen), Err(MergeRefusal::ChecksMissing));
    }

    #[test]
    fn landing_check_counts_that_exceed_the_total_refuse() {
        let mut seen = green();
        seen.required_passed = 3;
        assert_eq!(verdict(&seen), Err(MergeRefusal::ChecksUnknown));
        let mut seen = green();
        seen.required_total = 0;
        seen.required_passed = 1;
        assert_eq!(verdict(&seen), Err(MergeRefusal::ChecksUnknown));
    }

    #[test]
    fn landing_counts_cannot_overflow_into_a_pass() {
        let mut seen = green();
        seen.required_total = u32::MAX;
        seen.required_passed = u32::MAX;
        seen.required_failed = u32::MAX;
        assert_eq!(verdict(&seen), Err(MergeRefusal::ChecksUnknown));
    }

    #[test]
    fn landing_a_moved_head_refuses() {
        let mut seen = green();
        seen.head_sha = OTHER_HEAD.to_owned();
        assert_eq!(verdict(&seen), Err(MergeRefusal::HeadMoved));
    }

    #[test]
    fn landing_a_head_that_differs_only_in_case_refuses() {
        let mut seen = green();
        seen.head_sha = HEAD.to_uppercase();
        assert_eq!(verdict(&seen), Err(MergeRefusal::HeadMoved));
    }

    #[test]
    fn landing_a_closed_or_merged_pull_request_refuses() {
        for state in [PrState::Closed, PrState::Merged] {
            let mut seen = green();
            seen.state = state;
            assert_eq!(verdict(&seen), Err(MergeRefusal::NotOpen));
        }
    }

    #[test]
    fn landing_a_conflict_refuses() {
        let mut seen = green();
        seen.mergeable = Some(false);
        assert_eq!(verdict(&seen), Err(MergeRefusal::NotMergeable));
    }

    #[test]
    fn landing_mergeable_not_yet_computed_refuses() {
        let mut seen = green();
        seen.mergeable = None;
        assert_eq!(verdict(&seen), Err(MergeRefusal::ChecksUnknown));
    }

    #[test]
    fn landing_local_only_projects_never_merge() {
        let result = merge_verdict(
            HEAD,
            &green(),
            ProjectMode::LocalOnly,
            MergeMethod::Squash,
            MergeMethod::Squash,
            NOW,
            FRESH_MAX_AGE_MS,
        );
        assert_eq!(result, Err(MergeRefusal::ProjectMode));
    }

    #[test]
    fn landing_a_method_the_project_does_not_allow_refuses() {
        for (asked, allowed) in [
            (MergeMethod::Merge, MergeMethod::Squash),
            (MergeMethod::Rebase, MergeMethod::Squash),
            (MergeMethod::Squash, MergeMethod::Rebase),
        ] {
            let result = merge_verdict(
                HEAD,
                &green(),
                ProjectMode::NoMistakes,
                asked,
                allowed,
                NOW,
                FRESH_MAX_AGE_MS,
            );
            assert_eq!(result, Err(MergeRefusal::MethodNotAllowed));
        }
        let result = merge_verdict(
            HEAD,
            &green(),
            ProjectMode::NoMistakes,
            MergeMethod::Rebase,
            MergeMethod::Rebase,
            NOW,
            FRESH_MAX_AGE_MS,
        );
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn landing_an_old_observation_is_stale_at_the_limit_plus_one() {
        let mut seen = green();
        seen.observed_ms = NOW - FRESH_MAX_AGE_MS;
        assert_eq!(verdict(&seen), Ok(()));
        seen.observed_ms = NOW - FRESH_MAX_AGE_MS - 1;
        assert_eq!(verdict(&seen), Err(MergeRefusal::Stale));
    }

    #[test]
    fn landing_an_observation_from_the_future_is_stale() {
        let mut seen = green();
        seen.observed_ms = NOW + 1;
        assert_eq!(verdict(&seen), Err(MergeRefusal::Stale));
    }

    #[test]
    fn landing_a_stored_row_gets_the_longer_age_limit() {
        let mut seen = green();
        seen.observed_ms = NOW - 10 * 60 * 1000;
        assert_eq!(verdict(&seen), Err(MergeRefusal::Stale));
        let result = merge_verdict(
            HEAD,
            &seen,
            ProjectMode::NoMistakes,
            MergeMethod::Squash,
            MergeMethod::Squash,
            NOW,
            STORED_MAX_AGE_MS,
        );
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn landing_the_first_failing_rule_wins() {
        let mut seen = green();
        seen.state = PrState::Closed;
        seen.head_sha = OTHER_HEAD.to_owned();
        seen.required_failed = 1;
        assert_eq!(verdict(&seen), Err(MergeRefusal::NotOpen));
        seen.state = PrState::Open;
        assert_eq!(verdict(&seen), Err(MergeRefusal::HeadMoved));
    }

    #[test]
    fn landing_from_required_checks_counts_each_state() {
        let seen = PrObservation::from_required_checks(
            PrState::Open,
            HEAD,
            Some(true),
            &[
                CheckState::Passed,
                CheckState::Failed,
                CheckState::Pending,
                CheckState::Unknown,
            ],
            NOW,
        );
        assert_eq!(
            (
                seen.required_total,
                seen.required_passed,
                seen.required_failed,
                seen.required_unknown
            ),
            (4, 1, 1, 1)
        );
    }

    #[test]
    fn landing_refusal_codes_are_unique_short_snake_case() {
        let mut seen = std::collections::HashSet::new();
        for refusal in MergeRefusal::ALL {
            let code = refusal.code();
            assert!(seen.insert(code), "duplicate code {code}");
            assert!(code.len() <= 64);
            assert!(code.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
        }
        assert_eq!(seen.len(), MergeRefusal::ALL.len());
    }

    #[test]
    fn landing_state_keys_round_trip() {
        for state in [PrState::Open, PrState::Merged, PrState::Closed] {
            assert_eq!(PrState::parse(state.as_str()), Some(state));
        }
        for state in [
            CheckState::Passed,
            CheckState::Failed,
            CheckState::Pending,
            CheckState::Unknown,
        ] {
            assert_eq!(CheckState::parse(state.as_str()), Some(state));
        }
        assert_eq!(PrState::parse("OPEN"), None);
        assert_eq!(CheckState::parse(""), None);
    }
}
