//! Read-only reconciliation of persisted delivery steps against review refs.
//!
//! The caller supplies a store snapshot for one destination repository. The
//! plan preserves each record and observed ref value for review. This module
//! never writes Git refs or store records.
//!
//! ponytail: callers own storage access, so this crate stays storage-agnostic.
//!
//! Updated: 2026-11-08T03:41:55-07:00.

use std::path::PathBuf;

use crate::delivery::DeliveryAction;
use crate::git::git;
use crate::{Error, Repository};

const REVIEW_REF_PREFIX: &str = "refs/heads/orchestrator/review/";

/// Git-state result for one persisted delivery step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryClassification {
    /// The recorded review ref points to the recorded candidate commit.
    Confirmed,
    /// No review ref was requested or exists for a local-review attempt.
    NotApplied,
    /// The record is incomplete or the ref points to an unexplained object.
    Ambiguous,
}

/// Store snapshot fields required to reconcile one delivery step.
///
/// Optional fields preserve incomplete or unknown persisted records for review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedDeliveryStep {
    /// Append-only delivery step id.
    pub id: String,
    /// Owning task id.
    pub task_id: Option<String>,
    /// Delivery action decoded from the persisted mode.
    pub action: Option<DeliveryAction>,
    /// Candidate commit recorded by the delivery step.
    pub candidate_sha: Option<String>,
    /// Destination recorded by the delivery step.
    pub destination: Option<PathBuf>,
    /// Local-review ref recorded by the delivery step.
    pub target_ref: Option<String>,
}

/// Reconciliation result with original record and observed ref value retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryStepDecision {
    /// Persisted record supplied to reconciliation.
    pub record: RecordedDeliveryStep,
    /// Classification against the current Git ref.
    pub classification: DeliveryClassification,
    /// Object id currently resolved from the recorded ref, if it exists.
    pub observed_ref: Option<String>,
}

/// Stable, read-only plan for the supplied persisted delivery steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryReconciliationPlan {
    /// Decisions sorted by step id and record fields.
    pub steps: Vec<DeliveryStepDecision>,
}

impl DeliveryReconciliationPlan {
    /// Returns true when a human must review at least one delivery step.
    pub fn has_ambiguous_steps(&self) -> bool {
        self.steps
            .iter()
            .any(|step| step.classification == DeliveryClassification::Ambiguous)
    }
}

/// Compares persisted delivery records with refs in `repository` without writes.
///
/// Records for another destination, missing fields, and invalid review refs are
/// ambiguous. Local-review records with no matching ref are not applied.
pub fn reconcile_delivery_steps(
    repository: &Repository,
    recorded: &[RecordedDeliveryStep],
) -> Result<DeliveryReconciliationPlan, Error> {
    let mut steps = recorded
        .iter()
        .map(|record| classify_step(repository, record))
        .collect::<Result<Vec<_>, _>>()?;
    steps.sort_by(|left, right| {
        left.record
            .id
            .cmp(&right.record.id)
            .then_with(|| left.record.task_id.cmp(&right.record.task_id))
            .then_with(|| left.record.destination.cmp(&right.record.destination))
            .then_with(|| action_order(left.record.action).cmp(&action_order(right.record.action)))
            .then_with(|| left.record.candidate_sha.cmp(&right.record.candidate_sha))
            .then_with(|| left.record.target_ref.cmp(&right.record.target_ref))
    });
    Ok(DeliveryReconciliationPlan { steps })
}

fn classify_step(
    repository: &Repository,
    record: &RecordedDeliveryStep,
) -> Result<DeliveryStepDecision, Error> {
    let destination_matches = record.destination.as_deref() == Some(repository.path());
    let valid_ref = record
        .target_ref
        .as_deref()
        .map(|target_ref| is_valid_review_ref(repository, target_ref))
        .transpose()?
        .unwrap_or(false);
    let observed_ref = match (destination_matches, record.target_ref.as_deref(), valid_ref) {
        (true, Some(target_ref), true) => read_ref(repository, target_ref)?,
        _ => None,
    };
    let action_matches_ref = match (record.action, record.target_ref.as_ref()) {
        (Some(DeliveryAction::ReportOnly), None) => true,
        (Some(DeliveryAction::LocalReview), Some(_)) => valid_ref,
        _ => false,
    };
    let complete = !record.id.is_empty()
        && record
            .task_id
            .as_deref()
            .is_some_and(|task_id| !task_id.is_empty())
        && destination_matches
        && record
            .candidate_sha
            .as_deref()
            .is_some_and(is_full_object_id)
        && action_matches_ref;
    let classification = if !complete {
        DeliveryClassification::Ambiguous
    } else {
        match record.action {
            Some(DeliveryAction::ReportOnly) => DeliveryClassification::NotApplied,
            Some(DeliveryAction::LocalReview) => match observed_ref.as_deref() {
                None => DeliveryClassification::NotApplied,
                Some(observed) if record.candidate_sha.as_deref() == Some(observed) => {
                    DeliveryClassification::Confirmed
                }
                Some(_) => DeliveryClassification::Ambiguous,
            },
            None => DeliveryClassification::Ambiguous,
        }
    };
    Ok(DeliveryStepDecision {
        record: record.clone(),
        classification,
        observed_ref,
    })
}

fn action_order(action: Option<DeliveryAction>) -> u8 {
    match action {
        None => 0,
        Some(DeliveryAction::ReportOnly) => 1,
        Some(DeliveryAction::LocalReview) => 2,
    }
}

fn is_valid_review_ref(repository: &Repository, target_ref: &str) -> Result<bool, Error> {
    if !target_ref.starts_with(REVIEW_REF_PREFIX) || target_ref.len() == REVIEW_REF_PREFIX.len() {
        return Ok(false);
    }
    match git(
        "validate delivery review ref",
        repository.path(),
        ["check-ref-format", target_ref],
    ) {
        Ok(_) => Ok(true),
        Err(Error::Git { status, .. }) if status.code() == Some(1) => Ok(false),
        Err(error) => Err(error),
    }
}

fn read_ref(repository: &Repository, target_ref: &str) -> Result<Option<String>, Error> {
    match git(
        "read delivery review ref",
        repository.path(),
        [
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            target_ref,
        ],
    ) {
        Ok(object_id) => Ok(Some(object_id)),
        Err(Error::Git { status, .. }) if status.code() == Some(1) => Ok(None),
        Err(error) => Err(error),
    }
}

fn is_full_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
