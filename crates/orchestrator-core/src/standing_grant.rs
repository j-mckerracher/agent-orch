//! Standing grants: the pure rules (DPES-1105 phase 5a).
//!
//! A standing grant lets the user pre-approve a narrow set of orchestrator actions for one
//! project or one workstream, for a short time, a limited number of uses and a token budget.
//! It is the only authority besides a per-action approval, so everything here fails closed:
//! a kind that no person classified is refused, and a limit outside its range is refused.
//!
//! This module holds no I/O and reads no clock: callers pass `now_ms`. The store keeps the
//! rows and the counters; the daemon matches proposals. Design:
//! `docs/design/orchestrator-actions.md`.

use crate::orchestrator_action::{ActionKind, NEVER_GRANTABLE};

/// Shortest grant life, in seconds.
pub const MIN_EXPIRY_SECS: u64 = 60;
/// Life of a grant that names none, in seconds (8 hours).
pub const DEFAULT_EXPIRY_SECS: u64 = 8 * 3600;
/// Longest grant life, in seconds (7 days). Migration 0047 enforces the same limit.
pub const MAX_EXPIRY_SECS: u64 = 7 * 24 * 3600;
/// Uses of a grant that names no limit.
pub const DEFAULT_MAX_USES: u32 = 20;
/// Most uses one grant may allow.
pub const MAX_USES_CEILING: u32 = 200;
/// Largest token budget, in tokens.
pub const MAX_TOKEN_BUDGET: u64 = 1_000_000_000;
/// Most kinds one grant may name.
pub const MAX_KINDS: usize = 16;
/// Uses of a merge grant that names no limit.
pub const MERGE_DEFAULT_MAX_USES: u32 = 5;
/// Most uses a merge grant may allow.
pub const MERGE_MAX_USES_CEILING: u32 = 20;
/// Longest life of a merge grant, in seconds (24 hours). Migration 0052 enforces the same limit.
pub const MERGE_MAX_EXPIRY_SECS: u64 = 24 * 3600;
/// Longest head-branch prefix a merge grant may name, in characters.
pub const MAX_BRANCH_PREFIX_CHARS: usize = 64;

/// Kinds a grant may name. Fail closed: a kind that is on no list is not grantable, and the
/// walk test fails until a person classifies it. `decision.hold` is left out by least
/// authority until its parameters are reviewed. `decision.ask`, `task.message`, `memory.set`
/// (DPES-1108 phase 6), `coordinator.route` and `coordinator.report` (phase 7) are left out too,
/// until a person classifies them for grants: the authority model (phase 5) already runs their
/// autonomous cases on its own inside the daemon's bounds, and a `memory.set` writes text that
/// later prompts follow. `workstream.create` is left out because it
/// names no project or workstream to bind a grant to. `delivery.merge` is listed, but a grant
/// may name it only as the one kind, on a project, with the unattended-merge confirmation:
/// [`GrantTerms::validate`] holds those rules.
pub const GRANTABLE_KINDS: [ActionKind; 7] = [
    ActionKind::WorkstreamUpdate,
    ActionKind::ObjectiveAdd,
    ActionKind::ObjectiveRevise,
    ActionKind::TaskPropose,
    ActionKind::TaskRetry,
    ActionKind::WorkstreamLinkTask,
    ActionKind::DeliveryMerge,
];

/// What a grant is bound to. The string is the stored id: a project id or a workstream id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GrantScope {
    Project(String),
    Workstream(String),
}

impl GrantScope {
    /// Stored `scope_kind`.
    pub const fn kind_str(&self) -> &'static str {
        match self {
            Self::Project(_) => "project",
            Self::Workstream(_) => "workstream",
        }
    }

    /// Stored `scope_id`.
    pub fn id(&self) -> &str {
        match self {
            Self::Project(id) | Self::Workstream(id) => id,
        }
    }

    /// Reads back a stored scope kind and id.
    pub fn from_parts(kind: &str, id: &str) -> Option<Self> {
        match kind {
            "project" => Some(Self::Project(id.to_owned())),
            "workstream" => Some(Self::Workstream(id.to_owned())),
            _ => None,
        }
    }
}

/// What `grants.create` asks for, after the wire layer resolved the scope id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrantTerms {
    pub scope: GrantScope,
    pub kinds: Vec<ActionKind>,
    pub expires_in_secs: u64,
    pub max_uses: u32,
    pub token_budget: u64,
    /// The user's explicit consent to merge with no per-action click. Required for a merge
    /// grant; meaningless for any other.
    pub confirm_unattended_merge: bool,
    /// Head-branch prefix a merge grant covers. `None` covers every branch of the project.
    pub branch_prefix: Option<String>,
}

/// Why validation refused. Stable codes for the wire `reason` and for tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantRefusal {
    NoKinds,
    TooManyKinds,
    DuplicateKind,
    /// The kind is on [`NEVER_GRANTABLE`].
    KindNeverGrantable,
    /// The kind is not on [`GRANTABLE_KINDS`] and not never-grantable either.
    KindNotCovered,
    /// The name is not an action kind.
    KindUnknown,
    ScopeInvalid,
    ExpiryOutOfRange,
    UsesOutOfRange,
    BudgetOutOfRange,
    /// A grant that names `delivery.merge` names no other kind.
    MergeKindNotAlone,
    /// A merge grant is scoped to one project, never a workstream.
    MergeKindNeedsProject,
    /// A merge grant needs `confirm_unattended_merge`.
    MergeNeedsConfirmation,
    /// The branch prefix is malformed, absent on a merge grant, or the grant is not a merge
    /// grant.
    BranchPrefixInvalid,
}

impl GrantRefusal {
    /// Stable code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::NoKinds => "no_kinds",
            Self::TooManyKinds => "too_many_kinds",
            Self::DuplicateKind => "duplicate_kind",
            Self::KindNeverGrantable => "kind_never_grantable",
            Self::KindNotCovered => "kind_not_covered",
            Self::KindUnknown => "kind_unknown",
            Self::ScopeInvalid => "scope_invalid",
            Self::ExpiryOutOfRange => "expiry_out_of_range",
            Self::UsesOutOfRange => "uses_out_of_range",
            Self::BudgetOutOfRange => "budget_out_of_range",
            Self::MergeKindNotAlone => "merge_kind_not_alone",
            Self::MergeKindNeedsProject => "merge_kind_needs_project",
            Self::MergeNeedsConfirmation => "merge_needs_confirmation",
            Self::BranchPrefixInvalid => "branch_prefix_invalid",
        }
    }
}

/// Checks one wire kind name for a grant. A name on [`NEVER_GRANTABLE`] is refused even when
/// no [`ActionKind`] has it yet, so a kind a later phase adds is already covered.
pub fn check_kind_name(name: &str) -> Result<ActionKind, GrantRefusal> {
    if NEVER_GRANTABLE.contains(&name) {
        return Err(GrantRefusal::KindNeverGrantable);
    }
    let kind = ActionKind::parse(name).ok_or(GrantRefusal::KindUnknown)?;
    check_kind(kind)?;
    Ok(kind)
}

/// Checks one kind for a grant: never-grantable first, then the allowlist.
pub fn check_kind(kind: ActionKind) -> Result<(), GrantRefusal> {
    if !kind.is_grantable() {
        return Err(GrantRefusal::KindNeverGrantable);
    }
    if !GRANTABLE_KINDS.contains(&kind) {
        return Err(GrantRefusal::KindNotCovered);
    }
    Ok(())
}

/// Whether `prefix` may name the head branches of a merge grant: 1 to 64 characters of
/// letters, digits, `/`, `-`, `_` and `.`, with no `..`.
pub fn valid_branch_prefix(prefix: &str) -> bool {
    (1..=MAX_BRANCH_PREFIX_CHARS).contains(&prefix.chars().count())
        && prefix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'))
        && !prefix.contains("..")
}

impl GrantTerms {
    /// Whether these terms are a merge grant: they name `delivery.merge`.
    pub fn is_merge(&self) -> bool {
        self.kinds.contains(&ActionKind::DeliveryMerge)
    }

    /// Checks the terms. The first failure wins, in this order: kinds (empty, too many,
    /// duplicate, then each kind), the merge rules (alone, project scope, confirmation),
    /// scope, expiry, uses, budget, branch prefix. A merge grant has a shorter life (24
    /// hours) and fewer uses (20) than any other.
    pub fn validate(&self) -> Result<(), GrantRefusal> {
        if self.kinds.is_empty() {
            return Err(GrantRefusal::NoKinds);
        }
        if self.kinds.len() > MAX_KINDS {
            return Err(GrantRefusal::TooManyKinds);
        }
        for (index, kind) in self.kinds.iter().enumerate() {
            if self.kinds[..index].contains(kind) {
                return Err(GrantRefusal::DuplicateKind);
            }
        }
        for kind in &self.kinds {
            check_kind(*kind)?;
        }
        let merge = self.is_merge();
        if merge {
            if self.kinds.len() != 1 {
                return Err(GrantRefusal::MergeKindNotAlone);
            }
            if !matches!(self.scope, GrantScope::Project(_)) {
                return Err(GrantRefusal::MergeKindNeedsProject);
            }
            if !self.confirm_unattended_merge {
                return Err(GrantRefusal::MergeNeedsConfirmation);
            }
        }
        if !(1..=128).contains(&self.scope.id().chars().count()) {
            return Err(GrantRefusal::ScopeInvalid);
        }
        let (max_expiry, max_uses) = if merge {
            (MERGE_MAX_EXPIRY_SECS, MERGE_MAX_USES_CEILING)
        } else {
            (MAX_EXPIRY_SECS, MAX_USES_CEILING)
        };
        if !(MIN_EXPIRY_SECS..=max_expiry).contains(&self.expires_in_secs) {
            return Err(GrantRefusal::ExpiryOutOfRange);
        }
        if !(1..=max_uses).contains(&self.max_uses) {
            return Err(GrantRefusal::UsesOutOfRange);
        }
        if !(1..=MAX_TOKEN_BUDGET).contains(&self.token_budget) {
            return Err(GrantRefusal::BudgetOutOfRange);
        }
        // A merge grant names the head branches it covers: no prefix would cover every branch.
        if merge && self.branch_prefix.is_none() {
            return Err(GrantRefusal::BranchPrefixInvalid);
        }
        if let Some(prefix) = &self.branch_prefix {
            if !merge || !valid_branch_prefix(prefix) {
                return Err(GrantRefusal::BranchPrefixInvalid);
            }
        }
        Ok(())
    }

    /// When a grant created at `now_ms` with these terms expires.
    pub fn expires_at_ms(&self, now_ms: u64) -> u64 {
        now_ms.saturating_add(self.expires_in_secs.saturating_mul(1000))
    }
}

/// Where a grant stands. Computed on read, never stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantStatus {
    Active,
    Revoked,
    Expired,
    UsesExhausted,
    BudgetExhausted,
}

impl GrantStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Revoked => "revoked",
            Self::Expired => "expired",
            Self::UsesExhausted => "exhausted",
            Self::BudgetExhausted => "budget_exhausted",
        }
    }
}

/// A stored grant. Its terms never change; only `uses_used` and `revoked_at_ms` do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandingGrant {
    pub id: String,
    pub scope: GrantScope,
    pub kinds: Vec<ActionKind>,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
    pub max_uses: u32,
    pub uses_used: u32,
    pub token_budget: u64,
    pub revoked_at_ms: Option<u64>,
    /// Creation sequence of the newest proposal event when the grant was made. A proposal
    /// matches only if its own creation sequence is above it (no retroactive consent).
    pub proposal_floor: i64,
    /// Set only on a merge grant: the user confirmed merging with no per-action click.
    pub confirm_unattended_merge: bool,
    /// Head-branch prefix a merge grant covers. `None` covers every branch.
    pub branch_prefix: Option<String>,
}

impl StandingGrant {
    /// Status at `now_ms` given the tokens reported so far, in this order: revoked,
    /// expired (`now_ms >= expires_at_ms`), uses used up, budget spent.
    pub fn status(&self, now_ms: u64, tokens_spent: u64) -> GrantStatus {
        if self.revoked_at_ms.is_some() {
            GrantStatus::Revoked
        } else if now_ms >= self.expires_at_ms {
            GrantStatus::Expired
        } else if self.uses_used >= self.max_uses {
            GrantStatus::UsesExhausted
        } else if tokens_spent >= self.token_budget {
            GrantStatus::BudgetExhausted
        } else {
            GrantStatus::Active
        }
    }

    /// Whether the grant may be used at `now_ms`, by the terms the grant row holds: not
    /// revoked, not expired, uses left. The token budget needs a read of receipts, so the
    /// store checks it in [`Self::status`].
    pub fn is_active(&self, now_ms: u64) -> bool {
        self.status(now_ms, 0) == GrantStatus::Active
    }

    /// Whether the grant names `kind`. A kind that is never grantable is never covered, even
    /// when a row names it. `delivery.merge` is covered only by a grant that names it alone,
    /// on a project, with the confirmation, whatever else a row says.
    pub fn covers(&self, kind: ActionKind) -> bool {
        if check_kind(kind).is_err() || !self.kinds.contains(&kind) {
            return false;
        }
        kind != ActionKind::DeliveryMerge
            || (self.kinds.len() == 1
                && self.confirm_unattended_merge
                && matches!(self.scope, GrantScope::Project(_)))
    }

    /// Whether a merge grant covers a pull request whose head branch is `head_branch`. A grant
    /// with no prefix covers no branch: a merge grant is never created without one.
    pub fn covers_branch(&self, head_branch: &str) -> bool {
        self.branch_prefix
            .as_deref()
            .is_some_and(|prefix| head_branch.starts_with(prefix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms() -> GrantTerms {
        GrantTerms {
            scope: GrantScope::Project("p-1".to_owned()),
            kinds: vec![ActionKind::TaskPropose],
            expires_in_secs: DEFAULT_EXPIRY_SECS,
            max_uses: DEFAULT_MAX_USES,
            token_budget: 1000,
            confirm_unattended_merge: false,
            branch_prefix: None,
        }
    }

    fn merge_terms() -> GrantTerms {
        GrantTerms {
            kinds: vec![ActionKind::DeliveryMerge],
            max_uses: MERGE_DEFAULT_MAX_USES,
            confirm_unattended_merge: true,
            branch_prefix: Some("moa/".to_owned()),
            ..terms()
        }
    }

    fn grant() -> StandingGrant {
        StandingGrant {
            id: "grant-1".to_owned(),
            scope: GrantScope::Project("p-1".to_owned()),
            kinds: vec![ActionKind::TaskPropose],
            created_at_ms: 1_000,
            expires_at_ms: 2_000,
            max_uses: 2,
            uses_used: 0,
            token_budget: 100,
            revoked_at_ms: None,
            proposal_floor: 0,
            confirm_unattended_merge: false,
            branch_prefix: None,
        }
    }

    #[test]
    fn every_action_kind_is_on_exactly_one_grant_list() {
        // The kinds a person reviewed and left off the allowlist without making them never
        // grantable. Adding a kind fails this test until it is placed on one list.
        let excluded = [
            ActionKind::WorkstreamCreate,
            ActionKind::DecisionHold,
            ActionKind::DecisionAsk,
            ActionKind::TaskMessage,
            ActionKind::MemorySet,
            ActionKind::CoordinatorRoute,
            ActionKind::CoordinatorReport,
        ];
        for kind in ActionKind::ALL {
            let never = NEVER_GRANTABLE.contains(&kind.as_str());
            let allowed = GRANTABLE_KINDS.contains(&kind);
            let left_out = excluded.contains(&kind);
            let lists = [never, allowed, left_out]
                .into_iter()
                .filter(|on| *on)
                .count();
            assert_eq!(lists, 1, "{kind:?} must be on exactly one list");
            assert_eq!(kind.is_grantable(), !never, "{kind:?}");
            assert_eq!(check_kind(kind).is_ok(), allowed, "{kind:?}");
        }
    }

    #[test]
    fn the_three_never_grantable_kinds_are_refused_by_grant_validation() {
        for name in ["project.remove", "task.cancel", "task.revise"] {
            assert_eq!(
                check_kind_name(name),
                Err(GrantRefusal::KindNeverGrantable),
                "{name}"
            );
            if let Some(kind) = ActionKind::parse(name) {
                let mut refused = terms();
                refused.kinds = vec![ActionKind::TaskPropose, kind];
                assert_eq!(
                    refused.validate(),
                    Err(GrantRefusal::KindNeverGrantable),
                    "{name}"
                );
                let mut stored = grant();
                stored.kinds.push(kind);
                assert!(!stored.covers(kind), "{name}");
            }
        }
    }

    #[test]
    fn a_grant_for_a_never_grantable_kind_is_refused() {
        let mut refused = terms();
        refused.kinds = vec![ActionKind::TaskCancel];
        assert_eq!(refused.validate(), Err(GrantRefusal::KindNeverGrantable));
        assert_eq!(
            refused.validate().unwrap_err().code(),
            "kind_never_grantable"
        );
    }

    #[test]
    fn validate_refuses_each_code() {
        assert_eq!(terms().validate(), Ok(()));
        let case = |change: fn(&mut GrantTerms)| {
            let mut changed = terms();
            change(&mut changed);
            changed.validate()
        };
        assert_eq!(case(|t| t.kinds.clear()), Err(GrantRefusal::NoKinds));
        assert_eq!(
            case(|t| t.kinds = vec![ActionKind::TaskPropose; 2]),
            Err(GrantRefusal::DuplicateKind)
        );
        assert_eq!(
            case(|t| t.kinds = vec![ActionKind::WorkstreamCreate]),
            Err(GrantRefusal::KindNotCovered)
        );
        assert_eq!(
            case(|t| t.kinds = vec![ActionKind::DecisionHold]),
            Err(GrantRefusal::KindNotCovered)
        );
        assert_eq!(
            check_kind_name("grant.create"),
            Err(GrantRefusal::KindUnknown)
        );
        assert_eq!(
            case(|t| t.scope = GrantScope::Workstream(String::new())),
            Err(GrantRefusal::ScopeInvalid)
        );
        assert_eq!(
            case(|t| t.expires_in_secs = 59),
            Err(GrantRefusal::ExpiryOutOfRange)
        );
        assert_eq!(case(|t| t.expires_in_secs = 60), Ok(()));
        assert_eq!(
            case(|t| t.expires_in_secs = MAX_EXPIRY_SECS + 1),
            Err(GrantRefusal::ExpiryOutOfRange)
        );
        assert_eq!(case(|t| t.expires_in_secs = MAX_EXPIRY_SECS), Ok(()));
        assert_eq!(case(|t| t.max_uses = 0), Err(GrantRefusal::UsesOutOfRange));
        assert_eq!(
            case(|t| t.max_uses = 201),
            Err(GrantRefusal::UsesOutOfRange)
        );
        assert_eq!(case(|t| t.max_uses = 200), Ok(()));
        assert_eq!(
            case(|t| t.token_budget = 0),
            Err(GrantRefusal::BudgetOutOfRange)
        );
        assert_eq!(
            case(|t| t.token_budget = MAX_TOKEN_BUDGET + 1),
            Err(GrantRefusal::BudgetOutOfRange)
        );
        let ordinary: Vec<ActionKind> = GRANTABLE_KINDS
            .into_iter()
            .filter(|kind| *kind != ActionKind::DeliveryMerge)
            .collect();
        let mut with_ordinary = terms();
        with_ordinary.kinds = ordinary;
        assert_eq!(with_ordinary.validate(), Ok(()));
    }

    #[test]
    fn merge_grant_validation_applies_each_rule() {
        let case = |change: fn(&mut GrantTerms)| {
            let mut changed = merge_terms();
            change(&mut changed);
            changed.validate()
        };
        assert_eq!(merge_terms().validate(), Ok(()));
        assert_eq!(
            case(|t| t.kinds = vec![ActionKind::DeliveryMerge, ActionKind::TaskPropose]),
            Err(GrantRefusal::MergeKindNotAlone)
        );
        assert_eq!(
            case(|t| t.kinds = vec![ActionKind::TaskPropose, ActionKind::DeliveryMerge]),
            Err(GrantRefusal::MergeKindNotAlone)
        );
        assert_eq!(
            case(|t| t.scope = GrantScope::Workstream("w-1".to_owned())),
            Err(GrantRefusal::MergeKindNeedsProject)
        );
        assert_eq!(
            case(|t| t.confirm_unattended_merge = false),
            Err(GrantRefusal::MergeNeedsConfirmation)
        );
        assert_eq!(
            GrantRefusal::MergeNeedsConfirmation.code(),
            "merge_needs_confirmation"
        );
        assert_eq!(case(|t| t.max_uses = 20), Ok(()));
        assert_eq!(case(|t| t.max_uses = 21), Err(GrantRefusal::UsesOutOfRange));
        assert_eq!(case(|t| t.expires_in_secs = 24 * 3600), Ok(()));
        assert_eq!(
            case(|t| t.expires_in_secs = 24 * 3600 + 1),
            Err(GrantRefusal::ExpiryOutOfRange)
        );
        assert_eq!(
            case(|t| t.expires_in_secs = 59),
            Err(GrantRefusal::ExpiryOutOfRange)
        );
        assert_eq!(MERGE_DEFAULT_MAX_USES, 5);
    }

    #[test]
    fn mergegrant_core_refuses_a_merge_grant_without_a_branch_prefix() {
        let mut bare = merge_terms();
        bare.branch_prefix = None;
        assert_eq!(bare.validate(), Err(GrantRefusal::BranchPrefixInvalid));
        bare.branch_prefix = Some(String::new());
        assert_eq!(bare.validate(), Err(GrantRefusal::BranchPrefixInvalid));
    }

    #[test]
    fn a_branch_prefix_is_for_a_merge_grant_and_must_be_plain() {
        let mut with_prefix = merge_terms();
        with_prefix.branch_prefix = Some("moa/".to_owned());
        assert_eq!(with_prefix.validate(), Ok(()));
        let too_long = "a".repeat(65);
        for bad in ["", "a b", "moa/../x", "moa/\u{7f}", too_long.as_str()] {
            with_prefix.branch_prefix = Some(bad.to_owned());
            assert_eq!(
                with_prefix.validate(),
                Err(GrantRefusal::BranchPrefixInvalid),
                "{bad:?}"
            );
        }
        let mut ordinary = terms();
        ordinary.branch_prefix = Some("moa/".to_owned());
        assert_eq!(ordinary.validate(), Err(GrantRefusal::BranchPrefixInvalid));
    }

    #[test]
    fn a_stored_merge_grant_covers_merge_only_when_alone_confirmed_and_on_a_project() {
        let mut stored = grant();
        stored.kinds = vec![ActionKind::DeliveryMerge];
        assert!(!stored.covers(ActionKind::DeliveryMerge), "no confirmation");
        stored.confirm_unattended_merge = true;
        assert!(stored.covers(ActionKind::DeliveryMerge));
        stored.kinds.push(ActionKind::TaskPropose);
        assert!(!stored.covers(ActionKind::DeliveryMerge), "not alone");
        stored.kinds = vec![ActionKind::DeliveryMerge];
        stored.scope = GrantScope::Workstream("w-1".to_owned());
        assert!(!stored.covers(ActionKind::DeliveryMerge), "not a project");
    }

    #[test]
    fn a_branch_prefix_covers_only_the_branches_that_start_with_it() {
        let mut stored = grant();
        assert!(
            !stored.covers_branch("anything"),
            "no prefix covers no branch"
        );
        stored.branch_prefix = Some("moa/".to_owned());
        assert!(stored.covers_branch("moa/add-tests"));
        assert!(!stored.covers_branch("feature/moa/add-tests"));
        assert!(!stored.covers_branch("moa"));
    }

    #[test]
    fn defaults_are_eight_hours_and_twenty_uses() {
        assert_eq!(DEFAULT_EXPIRY_SECS, 8 * 3600);
        assert_eq!(MAX_EXPIRY_SECS, 7 * 24 * 3600);
        assert_eq!(DEFAULT_MAX_USES, 20);
        assert_eq!(MAX_USES_CEILING, 200);
        assert_eq!(terms().expires_at_ms(5_000), 5_000 + 8 * 3600 * 1000);
    }

    #[test]
    fn status_follows_the_documented_order_with_an_injected_clock() {
        let mut stored = grant();
        assert!(stored.is_active(1_000));
        assert!(stored.is_active(1_999));
        assert_eq!(stored.status(2_000, 0), GrantStatus::Expired);
        assert!(!stored.is_active(2_000));
        stored.uses_used = 2;
        assert_eq!(stored.status(1_500, 0), GrantStatus::UsesExhausted);
        stored.uses_used = 1;
        assert_eq!(stored.status(1_500, 99), GrantStatus::Active);
        assert_eq!(stored.status(1_500, 100), GrantStatus::BudgetExhausted);
        stored.revoked_at_ms = Some(1_200);
        assert_eq!(stored.status(1_500, 0), GrantStatus::Revoked);
        assert_eq!(stored.status(9_999, 999), GrantStatus::Revoked);
        assert!(!stored.is_active(1_500));
    }

    #[test]
    fn covers_only_the_named_kinds() {
        let stored = grant();
        assert!(stored.covers(ActionKind::TaskPropose));
        assert!(!stored.covers(ActionKind::TaskRetry));
    }

    #[test]
    fn no_grant_kind_exists_that_could_change_a_grant() {
        for kind in ActionKind::ALL {
            assert!(!kind.as_str().starts_with("grant"), "{kind:?}");
        }
    }
}
