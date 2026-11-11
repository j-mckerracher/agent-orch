//! Workstream and objective domain model (board item M7-07).
//!
//! A [`Workstream`] is a cross-project initiative. It owns [`Objective`]s, and
//! each objective carries [`AcceptanceGate`]s. Tasks are *linked* to a
//! workstream (and optionally to one objective) by the store; the task
//! contract, `parent_id`, and `project_id` are unchanged, so existing task
//! submit contracts keep their semantics. A workstream's project membership is
//! derived from the projects of its linked tasks, never stored on the
//! workstream. See
//! `/Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/docs/technical-implementation-plan.md`
//! and the UI gap analysis decision D-4 for the project-versus-workstream
//! rationale.
//!
//! # Contracts
//!
//! * Every record carries a `revision` that starts at [`INITIAL_REVISION`] and
//!   increases by exactly one per accepted change. Writers present the
//!   revision they read (`expected_revision`); a mismatch is a stale-revision
//!   rejection and nothing is written.
//! * [`WorkstreamState`] and [`GateState`] changes go through the central
//!   [`workstream_transition`] and [`gate_transition`] tables. Same-state
//!   changes are illegal so a retry never silently bumps a revision.
//! * Gates are append-only: a gate's description can not be edited and a gate
//!   can not be removed, so acceptance criteria can not be loosened after the
//!   fact. Only a gate's state and evidence task change.
//! * `Suspended` is advisory data for the UI. The scheduler does not consult
//!   it.
//!
//! Updated: 2026-11-10T19:30:00+00:00.

use std::collections::HashSet;

/// First revision of every workstream and objective.
pub const INITIAL_REVISION: i64 = 1;
/// Maximum characters in a workstream or objective title.
pub const MAX_TITLE_CHARS: usize = 200;
/// Maximum characters in a summary, description, or gate description.
pub const MAX_TEXT_CHARS: usize = 4000;
/// Maximum acceptance gates on one objective.
pub const MAX_GATES: usize = 50;
/// Maximum characters in an identifier.
pub const MAX_ID_CHARS: usize = 128;

/// Lifecycle of a workstream.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum WorkstreamState {
    Active,
    Suspended,
    Archived,
}

impl WorkstreamState {
    /// Stable database tag.
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Archived => "archived",
        }
    }

    /// Parses a stored tag.
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "active" => Some(Self::Active),
            "suspended" => Some(Self::Suspended),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}

/// Verification state of one acceptance gate.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum GateState {
    Pending,
    Verified,
    Failed,
}

impl GateState {
    /// Stable database tag.
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Verified => "verified",
            Self::Failed => "failed",
        }
    }

    /// Parses a stored tag.
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "pending" => Some(Self::Pending),
            "verified" => Some(Self::Verified),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// One acceptance criterion of an objective.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceGate {
    /// Identifier unique within the objective.
    pub id: String,
    /// What must hold. Immutable after creation.
    pub description: String,
    pub state: GateState,
    /// Linked task whose result evidences the gate, when known.
    #[serde(default)]
    pub evidence_task_id: Option<String>,
}

/// A cross-project initiative.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Workstream {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub state: WorkstreamState,
    pub revision: i64,
}

/// An outcome a workstream is working toward, with its acceptance gates.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Objective {
    pub id: String,
    pub workstream_id: String,
    pub title: String,
    pub description: String,
    pub gates: Vec<AcceptanceGate>,
    pub revision: i64,
}

/// Violated workstream or objective rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkstreamRuleError {
    EmptyField(&'static str),
    FieldTooLong {
        field: &'static str,
        max: usize,
    },
    InvalidId(&'static str),
    TooManyGates {
        max: usize,
    },
    DuplicateGateId(String),
    UnknownGate(String),
    /// Evidence is only meaningful for a gate that is not pending.
    EvidenceOnPendingGate,
    IllegalWorkstreamTransition {
        from: WorkstreamState,
        to: WorkstreamState,
    },
    IllegalGateTransition {
        from: GateState,
        to: GateState,
    },
    /// Archived workstreams accept no further changes.
    WorkstreamArchived,
}

impl std::fmt::Display for WorkstreamRuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} must not be blank"),
            Self::FieldTooLong { field, max } => {
                write!(f, "{field} must be at most {max} characters")
            }
            Self::InvalidId(field) => write!(f, "{field} is not a valid identifier"),
            Self::TooManyGates { max } => write!(f, "an objective holds at most {max} gates"),
            Self::DuplicateGateId(id) => write!(f, "duplicate gate id {id}"),
            Self::UnknownGate(id) => write!(f, "unknown gate id {id}"),
            Self::EvidenceOnPendingGate => {
                write!(f, "a pending gate cannot carry an evidence task")
            }
            Self::IllegalWorkstreamTransition { from, to } => write!(
                f,
                "workstream cannot move from {} to {}",
                from.tag(),
                to.tag()
            ),
            Self::IllegalGateTransition { from, to } => {
                write!(f, "gate cannot move from {} to {}", from.tag(), to.tag())
            }
            Self::WorkstreamArchived => write!(f, "workstream is archived"),
        }
    }
}

impl std::error::Error for WorkstreamRuleError {}

/// Central workstream state transition table.
///
/// Active and Suspended swap freely; either may archive; Archived is terminal.
pub fn workstream_transition(
    from: WorkstreamState,
    to: WorkstreamState,
) -> Result<WorkstreamState, WorkstreamRuleError> {
    use WorkstreamState::{Active, Archived, Suspended};
    match (from, to) {
        (Active, Suspended) | (Suspended, Active) | (Active, Archived) | (Suspended, Archived) => {
            Ok(to)
        }
        _ => Err(WorkstreamRuleError::IllegalWorkstreamTransition { from, to }),
    }
}

/// Central gate state transition table.
///
/// Pending resolves to Verified or Failed; Failed may be retried (Pending) or
/// later verified; Verified may be reopened (Pending) but never marked Failed
/// directly.
pub fn gate_transition(from: GateState, to: GateState) -> Result<GateState, WorkstreamRuleError> {
    use GateState::{Failed, Pending, Verified};
    match (from, to) {
        (Pending, Verified)
        | (Pending, Failed)
        | (Failed, Pending)
        | (Failed, Verified)
        | (Verified, Pending) => Ok(to),
        _ => Err(WorkstreamRuleError::IllegalGateTransition { from, to }),
    }
}

/// Validates an identifier: non-empty, bounded, printable ASCII without
/// whitespace.
pub fn validate_id(field: &'static str, value: &str) -> Result<(), WorkstreamRuleError> {
    if value.is_empty() {
        return Err(WorkstreamRuleError::EmptyField(field));
    }
    if value.chars().count() > MAX_ID_CHARS || !value.chars().all(|c| c.is_ascii_graphic()) {
        return Err(WorkstreamRuleError::InvalidId(field));
    }
    Ok(())
}

/// Validates a title: non-blank and bounded.
pub fn validate_title(field: &'static str, value: &str) -> Result<(), WorkstreamRuleError> {
    if value.trim().is_empty() {
        return Err(WorkstreamRuleError::EmptyField(field));
    }
    if value.chars().count() > MAX_TITLE_CHARS {
        return Err(WorkstreamRuleError::FieldTooLong {
            field,
            max: MAX_TITLE_CHARS,
        });
    }
    Ok(())
}

/// Validates free text. Blank is rejected when `required`.
pub fn validate_text(
    field: &'static str,
    value: &str,
    required: bool,
) -> Result<(), WorkstreamRuleError> {
    if required && value.trim().is_empty() {
        return Err(WorkstreamRuleError::EmptyField(field));
    }
    if value.chars().count() > MAX_TEXT_CHARS {
        return Err(WorkstreamRuleError::FieldTooLong {
            field,
            max: MAX_TEXT_CHARS,
        });
    }
    Ok(())
}

impl Workstream {
    /// Builds a new active workstream at the initial revision.
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        summary: impl Into<String>,
    ) -> Result<Self, WorkstreamRuleError> {
        let workstream = Self {
            id: id.into(),
            title: title.into(),
            summary: summary.into(),
            state: WorkstreamState::Active,
            revision: INITIAL_REVISION,
        };
        workstream.validate()?;
        Ok(workstream)
    }

    /// Checks every field rule.
    pub fn validate(&self) -> Result<(), WorkstreamRuleError> {
        validate_id("workstream id", &self.id)?;
        validate_title("title", &self.title)?;
        validate_text("summary", &self.summary, false)
    }

    /// Applies an update and returns the next revision. The caller has already
    /// matched `expected_revision` against the stored value.
    pub fn apply_update(
        &self,
        title: Option<&str>,
        summary: Option<&str>,
        state: Option<WorkstreamState>,
    ) -> Result<Self, WorkstreamRuleError> {
        if self.state == WorkstreamState::Archived {
            return Err(WorkstreamRuleError::WorkstreamArchived);
        }
        let mut next = self.clone();
        if let Some(title) = title {
            next.title = title.to_owned();
        }
        if let Some(summary) = summary {
            next.summary = summary.to_owned();
        }
        if let Some(state) = state {
            next.state = workstream_transition(self.state, state)?;
        }
        next.validate()?;
        next.revision = self.revision + 1;
        Ok(next)
    }
}

impl Objective {
    /// Builds a new objective at the initial revision. Gates start pending.
    pub fn new(
        id: impl Into<String>,
        workstream_id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        gates: Vec<AcceptanceGate>,
    ) -> Result<Self, WorkstreamRuleError> {
        let objective = Self {
            id: id.into(),
            workstream_id: workstream_id.into(),
            title: title.into(),
            description: description.into(),
            gates,
            revision: INITIAL_REVISION,
        };
        objective.validate()?;
        Ok(objective)
    }

    /// Checks every field rule.
    pub fn validate(&self) -> Result<(), WorkstreamRuleError> {
        validate_id("objective id", &self.id)?;
        validate_id("workstream id", &self.workstream_id)?;
        validate_title("title", &self.title)?;
        validate_text("description", &self.description, false)?;
        if self.gates.len() > MAX_GATES {
            return Err(WorkstreamRuleError::TooManyGates { max: MAX_GATES });
        }
        let mut seen = HashSet::new();
        for gate in &self.gates {
            validate_id("gate id", &gate.id)?;
            validate_text("gate description", &gate.description, true)?;
            if !seen.insert(gate.id.as_str()) {
                return Err(WorkstreamRuleError::DuplicateGateId(gate.id.clone()));
            }
            if let Some(task_id) = &gate.evidence_task_id {
                validate_id("evidence task id", task_id)?;
                if gate.state == GateState::Pending {
                    return Err(WorkstreamRuleError::EvidenceOnPendingGate);
                }
            }
        }
        Ok(())
    }

    /// Applies a revision (retitle, redescribe, append gates) and returns the
    /// next revision. Existing gates are never edited or removed here.
    pub fn apply_revision(
        &self,
        title: Option<&str>,
        description: Option<&str>,
        new_gates: &[AcceptanceGate],
    ) -> Result<Self, WorkstreamRuleError> {
        let mut next = self.clone();
        if let Some(title) = title {
            next.title = title.to_owned();
        }
        if let Some(description) = description {
            next.description = description.to_owned();
        }
        next.gates.extend(new_gates.iter().cloned());
        next.validate()?;
        next.revision = self.revision + 1;
        Ok(next)
    }

    /// Moves one gate through [`gate_transition`] and returns the next
    /// revision. Evidence is replaced by `evidence_task_id` on every call, so
    /// a reopened gate drops stale evidence.
    pub fn apply_gate_state(
        &self,
        gate_id: &str,
        state: GateState,
        evidence_task_id: Option<&str>,
    ) -> Result<Self, WorkstreamRuleError> {
        let mut next = self.clone();
        let gate = next
            .gates
            .iter_mut()
            .find(|gate| gate.id == gate_id)
            .ok_or_else(|| WorkstreamRuleError::UnknownGate(gate_id.to_owned()))?;
        gate.state = gate_transition(gate.state, state)?;
        gate.evidence_task_id = evidence_task_id.map(str::to_owned);
        next.validate()?;
        next.revision = self.revision + 1;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate(id: &str) -> AcceptanceGate {
        AcceptanceGate {
            id: id.into(),
            description: format!("gate {id}"),
            state: GateState::Pending,
            evidence_task_id: None,
        }
    }

    fn objective() -> Objective {
        Objective::new("o1", "w1", "Ship it", "", vec![gate("g1")]).unwrap()
    }

    #[test]
    fn new_workstream_is_active_at_initial_revision() {
        let w = Workstream::new("w1", "Title", "").unwrap();
        assert_eq!(w.state, WorkstreamState::Active);
        assert_eq!(w.revision, INITIAL_REVISION);
    }

    #[test]
    fn blank_and_oversized_fields_are_rejected() {
        assert_eq!(
            Workstream::new("w1", "  ", ""),
            Err(WorkstreamRuleError::EmptyField("title"))
        );
        assert!(matches!(
            Workstream::new("w1", "x".repeat(MAX_TITLE_CHARS + 1), ""),
            Err(WorkstreamRuleError::FieldTooLong { field: "title", .. })
        ));
        assert!(matches!(
            Workstream::new("w1", "t", "x".repeat(MAX_TEXT_CHARS + 1)),
            Err(WorkstreamRuleError::FieldTooLong {
                field: "summary",
                ..
            })
        ));
        assert_eq!(
            Workstream::new("", "t", ""),
            Err(WorkstreamRuleError::EmptyField("workstream id"))
        );
        assert_eq!(
            Workstream::new("has space", "t", ""),
            Err(WorkstreamRuleError::InvalidId("workstream id"))
        );
    }

    #[test]
    fn update_bumps_revision_by_one_and_applies_fields() {
        let w = Workstream::new("w1", "Title", "").unwrap();
        let next = w
            .apply_update(Some("New"), Some("Sum"), Some(WorkstreamState::Suspended))
            .unwrap();
        assert_eq!(next.revision, 2);
        assert_eq!(next.title, "New");
        assert_eq!(next.summary, "Sum");
        assert_eq!(next.state, WorkstreamState::Suspended);
    }

    #[test]
    fn workstream_transition_table_is_exhaustive() {
        use WorkstreamState::{Active, Archived, Suspended};
        let all = [Active, Suspended, Archived];
        let legal = [
            (Active, Suspended),
            (Suspended, Active),
            (Active, Archived),
            (Suspended, Archived),
        ];
        for from in all {
            for to in all {
                assert_eq!(
                    workstream_transition(from, to).is_ok(),
                    legal.contains(&(from, to)),
                    "{from:?} -> {to:?}"
                );
            }
        }
    }

    #[test]
    fn archived_workstream_rejects_every_update() {
        let mut w = Workstream::new("w1", "Title", "").unwrap();
        w.state = WorkstreamState::Archived;
        assert_eq!(
            w.apply_update(Some("x"), None, None),
            Err(WorkstreamRuleError::WorkstreamArchived)
        );
    }

    #[test]
    fn failed_update_leaves_original_unchanged() {
        let w = Workstream::new("w1", "Title", "").unwrap();
        assert!(w.apply_update(Some(" "), None, None).is_err());
        assert_eq!(w.revision, INITIAL_REVISION);
    }

    #[test]
    fn gate_transition_table_is_exhaustive() {
        use GateState::{Failed, Pending, Verified};
        let all = [Pending, Verified, Failed];
        let legal = [
            (Pending, Verified),
            (Pending, Failed),
            (Failed, Pending),
            (Failed, Verified),
            (Verified, Pending),
        ];
        for from in all {
            for to in all {
                assert_eq!(
                    gate_transition(from, to).is_ok(),
                    legal.contains(&(from, to)),
                    "{from:?} -> {to:?}"
                );
            }
        }
    }

    #[test]
    fn objective_rejects_duplicate_gate_ids_and_overflow() {
        let dup = Objective::new("o1", "w1", "t", "", vec![gate("g"), gate("g")]);
        assert_eq!(dup, Err(WorkstreamRuleError::DuplicateGateId("g".into())));
        let many = (0..=MAX_GATES).map(|i| gate(&format!("g{i}"))).collect();
        assert_eq!(
            Objective::new("o1", "w1", "t", "", many),
            Err(WorkstreamRuleError::TooManyGates { max: MAX_GATES })
        );
    }

    #[test]
    fn revision_appends_gates_and_cannot_duplicate_existing() {
        let o = objective();
        let next = o
            .apply_revision(Some("Renamed"), None, &[gate("g2")])
            .unwrap();
        assert_eq!(next.revision, 2);
        assert_eq!(next.gates.len(), 2);
        assert_eq!(next.gates[0], o.gates[0]);
        assert_eq!(
            o.apply_revision(None, None, &[gate("g1")]),
            Err(WorkstreamRuleError::DuplicateGateId("g1".into()))
        );
    }

    #[test]
    fn gate_state_change_records_and_clears_evidence() {
        let o = objective();
        let verified = o
            .apply_gate_state("g1", GateState::Verified, Some("task-1"))
            .unwrap();
        assert_eq!(verified.revision, 2);
        assert_eq!(
            verified.gates[0].evidence_task_id.as_deref(),
            Some("task-1")
        );
        let reopened = verified
            .apply_gate_state("g1", GateState::Pending, None)
            .unwrap();
        assert_eq!(reopened.gates[0].evidence_task_id, None);
    }

    #[test]
    fn gate_state_rejects_unknown_same_state_and_pending_evidence() {
        let o = objective();
        assert_eq!(
            o.apply_gate_state("nope", GateState::Verified, None),
            Err(WorkstreamRuleError::UnknownGate("nope".into()))
        );
        assert!(matches!(
            o.apply_gate_state("g1", GateState::Pending, None),
            Err(WorkstreamRuleError::IllegalGateTransition { .. })
        ));
        let verified = o.apply_gate_state("g1", GateState::Verified, None).unwrap();
        assert_eq!(
            verified.apply_gate_state("g1", GateState::Pending, Some("t")),
            Err(WorkstreamRuleError::EvidenceOnPendingGate)
        );
        assert!(matches!(
            verified.apply_gate_state("g1", GateState::Failed, None),
            Err(WorkstreamRuleError::IllegalGateTransition { .. })
        ));
    }

    #[test]
    fn tags_round_trip() {
        for s in [
            WorkstreamState::Active,
            WorkstreamState::Suspended,
            WorkstreamState::Archived,
        ] {
            assert_eq!(WorkstreamState::from_tag(s.tag()), Some(s));
        }
        for s in [GateState::Pending, GateState::Verified, GateState::Failed] {
            assert_eq!(GateState::from_tag(s.tag()), Some(s));
        }
        assert_eq!(WorkstreamState::from_tag("x"), None);
        assert_eq!(GateState::from_tag("x"), None);
    }

    #[test]
    fn gate_json_rejects_unknown_fields() {
        let bad = r#"{"id":"g","description":"d","state":"pending","extra":1}"#;
        assert!(serde_json::from_str::<AcceptanceGate>(bad).is_err());
        let ok = r#"{"id":"g","description":"d","state":"pending"}"#;
        assert_eq!(
            serde_json::from_str::<AcceptanceGate>(ok)
                .unwrap()
                .evidence_task_id,
            None
        );
    }
}
