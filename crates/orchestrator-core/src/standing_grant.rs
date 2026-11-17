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

/// Kinds a grant may name. Fail closed: a kind that is on no list is not grantable, and the
/// walk test fails until a person classifies it. `decision.hold` is left out by least
/// authority until its parameters are reviewed; `workstream.create` is left out because it
/// names no project or workstream to bind a grant to.
pub const GRANTABLE_KINDS: [ActionKind; 6] = [
    ActionKind::WorkstreamUpdate,
    ActionKind::ObjectiveAdd,
    ActionKind::ObjectiveRevise,
    ActionKind::TaskPropose,
    ActionKind::TaskRetry,
    ActionKind::WorkstreamLinkTask,
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

impl GrantTerms {
    /// Checks the terms. The first failure wins, in this order: kinds (empty, too many,
    /// duplicate, then each kind), scope, expiry, uses, budget.
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
        if !(1..=128).contains(&self.scope.id().chars().count()) {
            return Err(GrantRefusal::ScopeInvalid);
        }
        if !(MIN_EXPIRY_SECS..=MAX_EXPIRY_SECS).contains(&self.expires_in_secs) {
            return Err(GrantRefusal::ExpiryOutOfRange);
        }
        if !(1..=MAX_USES_CEILING).contains(&self.max_uses) {
            return Err(GrantRefusal::UsesOutOfRange);
        }
        if !(1..=MAX_TOKEN_BUDGET).contains(&self.token_budget) {
            return Err(GrantRefusal::BudgetOutOfRange);
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
    /// when a row names it.
    pub fn covers(&self, kind: ActionKind) -> bool {
        check_kind(kind).is_ok() && self.kinds.contains(&kind)
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
        }
    }

    #[test]
    fn every_action_kind_is_on_exactly_one_grant_list() {
        // The kinds a person reviewed and left off the allowlist without making them never
        // grantable. Adding a kind fails this test until it is placed on one list.
        let excluded = [ActionKind::WorkstreamCreate, ActionKind::DecisionHold];
        for kind in ActionKind::ALL {
            let never = NEVER_GRANTABLE.contains(&kind.as_str());
            let allowed = GRANTABLE_KINDS.contains(&kind);
            let left_out = excluded.contains(&kind);
            let lists = [never, allowed, left_out].into_iter().filter(|on| *on).count();
            assert_eq!(lists, 1, "{kind:?} must be on exactly one list");
            assert_eq!(kind.is_grantable(), !never, "{kind:?}");
            assert_eq!(check_kind(kind).is_ok(), allowed, "{kind:?}");
        }
    }

    #[test]
    fn the_four_never_grantable_kinds_are_refused_by_grant_validation() {
        for name in ["project.remove", "delivery.merge", "task.cancel", "task.revise"] {
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
        assert_eq!(case(|t| t.expires_in_secs = 59), Err(GrantRefusal::ExpiryOutOfRange));
        assert_eq!(case(|t| t.expires_in_secs = 60), Ok(()));
        assert_eq!(
            case(|t| t.expires_in_secs = MAX_EXPIRY_SECS + 1),
            Err(GrantRefusal::ExpiryOutOfRange)
        );
        assert_eq!(case(|t| t.expires_in_secs = MAX_EXPIRY_SECS), Ok(()));
        assert_eq!(case(|t| t.max_uses = 0), Err(GrantRefusal::UsesOutOfRange));
        assert_eq!(case(|t| t.max_uses = 201), Err(GrantRefusal::UsesOutOfRange));
        assert_eq!(case(|t| t.max_uses = 200), Ok(()));
        assert_eq!(case(|t| t.token_budget = 0), Err(GrantRefusal::BudgetOutOfRange));
        assert_eq!(
            case(|t| t.token_budget = MAX_TOKEN_BUDGET + 1),
            Err(GrantRefusal::BudgetOutOfRange)
        );
        assert_eq!(case(|t| t.kinds = GRANTABLE_KINDS.to_vec()), Ok(()));
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
