//! SQLite persistence for report-only and local-review delivery.
//!
//! Each outcome row, its audit event, and its path-free global event commit
//! together. Approval checks use `check_approval`; persisted verifier evidence
//! must match the exact task and candidate.
//!
//! Updated: 2026-11-08T16:59:05+00:00.

use std::io;

use orchestrator_core::approval::{
    ApprovalAction, ApprovalActionKind, ApprovalError, ParameterDigest,
};
use orchestrator_core::authorization::ActionKind;
use serde_json::{json, Value};
use sqlx::Row;

use crate::sqlite_events::append_durable_event;
use crate::{SqliteStore, StoreError};

/// One final delivery result persisted with its audit event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryOutcome {
    /// Report-only evidence was persisted.
    Reported,
    /// A new local-review ref was created.
    ReviewRefCreated,
    /// The review ref already pointed to this candidate.
    ReviewRefAlreadyMatches,
    /// Delivery was refused before completion.
    Refused { detail: String },
}

/// Typed input for one append-only delivery step.
pub struct DeliveryStepRecord<'a> {
    /// Caller-assigned step id.
    pub id: &'a str,
    /// Approval record checked for this action.
    pub approval_id: &'a str,
    /// Persisted verification run checked for this candidate.
    pub verification_run_id: &'a str,
    /// Exact action, task, parameter digest, and candidate binding.
    pub action: &'a ApprovalAction,
    /// Canonical destination repository path.
    pub destination: &'a str,
    /// Local-review ref, absent for report-only delivery.
    pub target_ref: Option<&'a str>,
    /// Final delivery outcome.
    pub outcome: DeliveryOutcome,
    /// Git diff summary, when available.
    pub diff_summary: Option<&'a str>,
}

impl SqliteStore {
    /// Checks the exact delivery binding with the existing M3-02 state machine.
    ///
    /// The caller must include destination, review ref, and every other action
    /// parameter in `parameter_digest`; this method never reimplements matching.
    pub fn check_delivery_approval(
        &self,
        approval_id: &str,
        task_id: &str,
        action_kind: ActionKind,
        parameter_digest: ParameterDigest,
        candidate_sha: &str,
    ) -> Result<(), ApprovalError> {
        let proposed = ApprovalAction::new(
            ApprovalActionKind::Delivery(action_kind),
            parameter_digest,
            candidate_sha,
            task_id,
        )?;
        self.check_approval(approval_id, &proposed)
    }

    /// Persists one immutable delivery outcome and its matching audit event.
    ///
    /// The typed record preserves missing approval and verification ids so
    /// refusals remain auditable. The outcome row and event commit together.
    pub fn record_delivery_step(&self, step: &DeliveryStepRecord<'_>) -> Result<(), StoreError> {
        let (delivery_mode, _) = delivery_action(step.action.kind())?;
        let (outcome, detail) = match (&step.outcome, delivery_mode) {
            (DeliveryOutcome::Reported, "report_only") => ("reported", None),
            (DeliveryOutcome::ReviewRefCreated, "local_review") => ("review_ref_created", None),
            (DeliveryOutcome::ReviewRefAlreadyMatches, "local_review") => {
                ("review_ref_already_matches", None)
            }
            (DeliveryOutcome::Refused { detail }, _) => ("refused", Some(detail.clone())),
            _ => {
                return Err(StoreError::Io(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "delivery outcome does not match its action kind",
                )))
            }
        };
        let parameter_digest = hex_digest(step.action.parameter_digest().as_bytes());
        let task_id = step.action.task_id().to_owned();
        let candidate_sha = step.action.candidate_commit().as_str().to_owned();
        let action = serde_json::to_string(step.action)?;
        let id = step.id.to_owned();
        let approval_id = step.approval_id.to_owned();
        let verification_run_id = step.verification_run_id.to_owned();
        let destination = step.destination.to_owned();
        let target_ref = step.target_ref.map(str::to_owned);
        let diff_summary = step.diff_summary.map(str::to_owned);
        let audit_payload = serde_json::to_string(&json!({
            "outcome": outcome,
            "detail": detail,
        }))?;
        let stream_payload = json!({"step_id": &id, "outcome": outcome});
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            sqlx::query(
                "INSERT INTO delivery_steps (id, task_id, action_kind, action, parameter_digest, candidate_sha, approval_id, verification_run_id, destination, target_ref, outcome, detail, diff_summary) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&task_id)
            .bind(delivery_mode)
            .bind(action)
            .bind(parameter_digest)
            .bind(candidate_sha)
            .bind(&approval_id)
            .bind(&verification_run_id)
            .bind(destination)
            .bind(target_ref)
            .bind(outcome)
            .bind(detail)
            .bind(diff_summary)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                "INSERT INTO delivery_step_events (delivery_step_id, task_id, kind, payload) VALUES (?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&task_id)
            .bind(outcome)
            .bind(audit_payload)
            .execute(&mut *transaction)
            .await?;
            let stream_kind = format!("delivery.{outcome}");
            append_durable_event(
                &mut transaction,
                &task_id,
                None,
                &stream_kind,
                &stream_payload,
            )
            .await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    /// Returns whether stored verification evidence passes for this exact candidate.
    pub fn delivery_verification_matches(
        &self,
        task_id: &str,
        run_id: &str,
        candidate_sha: &str,
    ) -> Result<bool, StoreError> {
        let runs = self.verification_results_for_task(task_id)?;
        Ok(runs.into_iter().any(|(run, _)| {
            run.id == run_id
                && run.task_id == task_id
                && run.commit == candidate_sha
                && run.status == "passed"
        }))
    }
    /// Loads delivery steps in creation order as JSON records.
    pub fn delivery_steps_for_task(&self, task_id: &str) -> Result<Vec<Value>, StoreError> {
        let task_id = task_id.to_owned();
        self.call(move |pool| async move {
            let rows = sqlx::query(
                "SELECT id, task_id, action_kind, action, parameter_digest, candidate_sha, approval_id, verification_run_id, destination, target_ref, outcome, detail, diff_summary FROM delivery_steps WHERE task_id = ? ORDER BY created_at, id",
            )
            .bind(&task_id)
            .fetch_all(&pool)
            .await?;
            rows.into_iter()
                .map(|row| {
                    let id: String = row.try_get("id")?;
                    let stored_task_id: String = row.try_get("task_id")?;
                    let stored_action_kind: String = row.try_get("action_kind")?;
                    let action_json: String = row.try_get("action")?;
                    let action: ApprovalAction = serde_json::from_str(&action_json)?;
                    let (delivery_mode, action_name) = delivery_action(action.kind())?;
                    let parameter_digest: String = row.try_get("parameter_digest")?;
                    let candidate_sha: String = row.try_get("candidate_sha")?;
                    if stored_task_id != task_id
                        || stored_action_kind != delivery_mode
                        || parameter_digest != hex_digest(action.parameter_digest().as_bytes())
                        || candidate_sha != action.candidate_commit().as_str()
                    {
                        return Err(invalid_delivery_record(
                            "stored delivery action columns do not match action payload",
                        ));
                    }
                    let approval_id: String = row.try_get("approval_id")?;
                    let verification_run_id: String = row.try_get("verification_run_id")?;
                    let destination: String = row.try_get("destination")?;
                    let target_ref: Option<String> = row.try_get("target_ref")?;
                    let outcome: String = row.try_get("outcome")?;
                    let detail: Option<String> = row.try_get("detail")?;
                    let diff_summary: Option<String> = row.try_get("diff_summary")?;
                    Ok(json!({
                        "id": id,
                        "task_id": stored_task_id,
                        "delivery_mode": delivery_mode,
                        "action_kind": action_name,
                        "action": action,
                        "parameter_digest": parameter_digest,
                        "candidate_sha": candidate_sha,
                        "approval_id": approval_id,
                        "verification_run_id": verification_run_id,
                        "destination": destination,
                        "target_ref": target_ref,
                        "outcome": outcome,
                        "detail": detail,
                        "diff_summary": diff_summary,
                    }))
                })
                .collect()
        })
    }

    /// Loads every delivery step in stable creation order.
    pub fn all_delivery_steps(&self) -> Result<Vec<Value>, StoreError> {
        self.call(move |pool| async move {
            let rows = sqlx::query(
                "SELECT id, task_id, action_kind, action, parameter_digest, candidate_sha, approval_id, verification_run_id, destination, target_ref, outcome, detail, diff_summary FROM delivery_steps ORDER BY created_at, id",
            )
            .fetch_all(&pool)
            .await?;
            rows.into_iter()
                .map(|row| {
                    let id: String = row.try_get("id")?;
                    let stored_task_id: String = row.try_get("task_id")?;
                    let stored_action_kind: String = row.try_get("action_kind")?;
                    let action_json: String = row.try_get("action")?;
                    let action: ApprovalAction = serde_json::from_str(&action_json)?;
                    let (delivery_mode, action_name) = delivery_action(action.kind())?;
                    let parameter_digest: String = row.try_get("parameter_digest")?;
                    let candidate_sha: String = row.try_get("candidate_sha")?;
                    if stored_task_id != action.task_id()
                        || stored_action_kind != delivery_mode
                        || parameter_digest != hex_digest(action.parameter_digest().as_bytes())
                        || candidate_sha != action.candidate_commit().as_str()
                    {
                        return Err(invalid_delivery_record(
                            "stored delivery action columns do not match action payload",
                        ));
                    }
                    let approval_id: String = row.try_get("approval_id")?;
                    let verification_run_id: String = row.try_get("verification_run_id")?;
                    let destination: String = row.try_get("destination")?;
                    let target_ref: Option<String> = row.try_get("target_ref")?;
                    let outcome: String = row.try_get("outcome")?;
                    let detail: Option<String> = row.try_get("detail")?;
                    let diff_summary: Option<String> = row.try_get("diff_summary")?;
                    Ok(json!({
                        "id": id,
                        "task_id": stored_task_id,
                        "delivery_mode": delivery_mode,
                        "action_kind": action_name,
                        "action": action,
                        "parameter_digest": parameter_digest,
                        "candidate_sha": candidate_sha,
                        "approval_id": approval_id,
                        "verification_run_id": verification_run_id,
                        "destination": destination,
                        "target_ref": target_ref,
                        "outcome": outcome,
                        "detail": detail,
                        "diff_summary": diff_summary,
                    }))
                })
                .collect()
        })
    }

    /// Loads the single append-only audit event associated with a delivery step.
    pub fn delivery_events(&self, step_id: &str) -> Result<Vec<Value>, StoreError> {
        let step_id = step_id.to_owned();
        self.call(move |pool| async move {
            let exists = sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS (SELECT 1 FROM delivery_steps WHERE id = ?)",
            )
            .bind(&step_id)
            .fetch_one(&pool)
            .await?;
            if exists == 0 {
                return Err(StoreError::RecordNotFound {
                    entity: "delivery step",
                    id: step_id,
                });
            }
            let rows = sqlx::query(
                "SELECT kind, payload FROM delivery_step_events WHERE delivery_step_id = ? ORDER BY seq",
            )
            .bind(&step_id)
            .fetch_all(&pool)
            .await?;
            rows.into_iter()
                .map(|row| {
                    let kind: String = row.try_get("kind")?;
                    let payload: String = row.try_get("payload")?;
                    let payload: Value = serde_json::from_str(&payload)?;
                    if payload["outcome"] != kind {
                        return Err(invalid_delivery_record(
                            "delivery event kind does not match payload",
                        ));
                    }
                    Ok(json!({ "kind": kind, "payload": payload }))
                })
                .collect()
        })
    }
}

fn delivery_action(kind: ApprovalActionKind) -> Result<(&'static str, &'static str), StoreError> {
    match kind {
        ApprovalActionKind::Delivery(ActionKind::ReportPublish) => {
            Ok(("report_only", "report_publish"))
        }
        ApprovalActionKind::Delivery(ActionKind::LocalReviewMerge) => {
            Ok(("local_review", "local_review_merge"))
        }
        _ => Err(StoreError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "delivery step requires report-publish or local-review action",
        ))),
    }
}

fn hex_digest(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(64);
    for byte in bytes {
        value.push(HEX[(byte >> 4) as usize] as char);
        value.push(HEX[(byte & 0x0f) as usize] as char);
    }
    value
}

fn invalid_delivery_record(detail: &str) -> StoreError {
    StoreError::Io(io::Error::new(io::ErrorKind::InvalidData, detail))
}
