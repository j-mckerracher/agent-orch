//! Review evidence, approval requests, and change requests.
//!
//! Evidence is pinned to the approval's candidate commit, never to the live
//! work tree. A review that no longer matches the candidate, verification,
//! workspace base, or approved parameters reports `stale` with reasons. Results
//! omit repository, worktree, log, and artifact paths.

use orchestrator_core::approval::{ApprovalState, CandidateCommit};
use orchestrator_store::ReviewError;

use super::*;

const MAX_SNAPSHOT_FILES: usize = 1000;
const MAX_VERIFICATION_DETAIL_BYTES: usize = 64 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RequestApprovalParams {
    task_id: String,
    #[serde(default)]
    workspace_id: Option<String>,
    verification_run_id: String,
    action: String,
    #[serde(default)]
    target_ref: Option<String>,
    idempotency_key: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceParams {
    task_id: String,
    #[serde(default)]
    approval_id: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RequestChangesParams {
    approval_id: String,
    candidate_commit: String,
    feedback: String,
    idempotency_key: String,
}

impl DaemonApi {
    /// Request a delivery approval whose readable parameters are stored beside
    /// its digest. The candidate is the passed verification run's commit and
    /// must still be the worktree HEAD.
    pub(super) fn review_request_approval(&self, params: Value) -> Result<Value, RpcError> {
        let request: RequestApprovalParams = parse_params(params)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "review.request_approval",
            &key,
            &request,
            || self.perform_request_approval(&request),
        )
    }

    fn perform_request_approval(&self, request: &RequestApprovalParams) -> Result<Value, RpcError> {
        let task = self.store.get(&request.task_id).map_err(map_store_error)?;
        let (action, action_kind) = match (task.contract.delivery, request.action.as_str()) {
            (DeliveryPolicy::ReportOnly, "report_only") => {
                (DeliveryAction::ReportOnly, ActionKind::ReportPublish)
            }
            (DeliveryPolicy::LocalReview, "local_review") => {
                (DeliveryAction::LocalReview, ActionKind::LocalReviewMerge)
            }
            _ => return Err(RpcError::Forbidden),
        };
        let target_ref = match (action, request.target_ref.as_deref()) {
            (DeliveryAction::ReportOnly, None) => None,
            (DeliveryAction::LocalReview, Some(target_ref)) if !target_ref.is_empty() => {
                Some(target_ref)
            }
            _ => return Err(RpcError::InvalidParams),
        };
        let workspace =
            self.selected_workspace(&request.task_id, request.workspace_id.as_deref())?;
        let project = self
            .store
            .get_project(&task.contract.project_id)
            .map_err(map_store_error)?;
        let (primary, worktree) = task_repositories(&project, &workspace)?;
        let verification = self
            .store
            .verification_runs_for_task(&request.task_id)
            .map_err(map_store_error)?
            .into_iter()
            .find(|run| run.id == request.verification_run_id)
            .ok_or(RpcError::NotFound)?;
        if verification.status != "passed" {
            return Err(RpcError::Conflict);
        }
        let head = worktree.resolve_base_ref("HEAD").map_err(map_git_error)?;
        if head != verification.commit {
            return Err(RpcError::Conflict);
        }
        let destination = primary.path().to_string_lossy().into_owned();
        let digest = delivery_digest(action, &destination, &workspace.base_commit, target_ref)?;
        let approval_action = ApprovalAction::new(
            ApprovalActionKind::Delivery(action_kind),
            ParameterDigest::from_sha256(digest),
            &verification.commit,
            &request.task_id,
        )
        .map_err(map_approval_error)?;

        let existing = self
            .store
            .approval_records_for_task(&request.task_id)
            .map_err(map_approval_error)?
            .into_iter()
            .find(|approval| {
                approval.action == approval_action
                    && matches!(
                        approval.state,
                        ApprovalState::Requested | ApprovalState::Approved { .. }
                    )
            });
        let (approval, existing) = match existing {
            Some(approval) => (approval, true),
            None => {
                let id = idempotency::stable_id("review-approval", &request.idempotency_key);
                let parameters = json!({
                    "action": action.as_str(),
                    "destination": destination,
                    "base_sha": workspace.base_commit,
                    "target_ref": target_ref,
                });
                let approval = self
                    .store
                    .request_approval_with_parameters(
                        &id,
                        &approval_action,
                        &self.approval_actor,
                        &self.approval_allowlist,
                        &parameters,
                    )
                    .map_err(map_approval_error)?;
                (approval, false)
            }
        };
        Ok(json!({
            "approval": approval_json(&approval),
            "action": {
                "action": action.as_str(),
                "destination": {"project_id": project.id},
                "base_sha": workspace.base_commit,
                "target_ref": target_ref,
                "candidate_commit": verification.commit,
            },
            "digest": hex(&digest),
            "existing": existing,
        }))
    }

    /// Record reviewer feedback for one approval candidate.
    pub(super) fn review_request_changes(&self, params: Value) -> Result<Value, RpcError> {
        let request: RequestChangesParams = parse_params(params)?;
        CandidateCommit::try_from(request.candidate_commit.clone())
            .map_err(|_| RpcError::InvalidParams)?;
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "review.request_changes",
            &key,
            &request,
            || {
                let id = idempotency::stable_id("change-request", &request.idempotency_key);
                let change = self
                    .store
                    .request_changes(
                        &id,
                        &request.approval_id,
                        &request.candidate_commit,
                        &request.feedback,
                        &self.approval_actor,
                        &self.approval_allowlist,
                    )
                    .map_err(map_review_error)?;
                Ok(json!({
                    "change_request": change_json(&change),
                    "review_state": "changes_requested",
                    "approval_state": "requested",
                }))
            },
        )
    }

    pub(super) fn review_evidence(&self, params: Value) -> Result<Value, RpcError> {
        let request: EvidenceParams = parse_params(params)?;
        let path = request
            .path
            .as_deref()
            .map(validate_snapshot_path)
            .transpose()?;
        let task = self.store.get(&request.task_id).map_err(map_store_error)?;
        let project = self
            .store
            .get_project(&task.contract.project_id)
            .map_err(map_store_error)?;
        let mut workspaces = self
            .store
            .workspaces_for_task(&request.task_id)
            .map_err(map_store_error)?;
        if workspaces.len() > 1 {
            return Err(RpcError::Conflict);
        }
        let workspace = workspaces.pop();

        let mut approvals = self
            .store
            .approval_records_for_task(&request.task_id)
            .map_err(map_approval_error)?;
        let approval = match request.approval_id.as_deref() {
            Some(id) => Some(
                approvals
                    .into_iter()
                    .find(|approval| approval.id == id)
                    .ok_or(RpcError::NotFound)?,
            ),
            None => {
                approvals.retain(|approval| {
                    matches!(
                        approval.state,
                        ApprovalState::Requested | ApprovalState::Approved { .. }
                    )
                });
                match approvals.len() {
                    0 => None,
                    1 => approvals.pop(),
                    _ => return Err(RpcError::Conflict),
                }
            }
        };
        let stored = match &approval {
            Some(approval) => self
                .store
                .approval_parameters(&approval.id)
                .map_err(map_store_error)?,
            None => None,
        };
        let change_request = match &approval {
            Some(approval) => self
                .store
                .change_request_for_approval(&approval.id)
                .map_err(map_store_error)?,
            None => None,
        };

        let repositories = match &workspace {
            Some(workspace) => match task_repositories(&project, workspace) {
                Ok(pair) => Some(pair),
                Err(RpcError::Forbidden) => return Err(RpcError::Forbidden),
                Err(_) => None,
            },
            None => None,
        };
        let head = repositories
            .as_ref()
            .and_then(|(_, worktree)| worktree.resolve_base_ref("HEAD").ok());
        let (candidate, candidate_source) = match (&approval, &head) {
            (Some(approval), _) => (
                Some(approval.action.candidate_commit().as_str().to_owned()),
                "approval",
            ),
            (None, Some(head)) => (Some(head.clone()), "workspace_head"),
            (None, None) => (None, "none"),
        };

        let snapshot = match (&workspace, &repositories, &candidate) {
            (Some(workspace), Some((_, worktree)), Some(candidate)) => Some(snapshot_json(
                worktree,
                &workspace.base_commit,
                candidate,
                path.as_deref(),
            )?),
            _ => None,
        };

        let runs = self
            .store
            .verification_results_for_task(&request.task_id)
            .map_err(map_store_error)?;
        let verification: Vec<Value> = runs
            .iter()
            .map(|(run, envelope)| verification_json(run, envelope, candidate.as_deref()))
            .collect();
        let candidate_runs: Vec<_> = runs
            .iter()
            .filter(|(run, _)| Some(&run.commit) == candidate.as_ref())
            .collect();
        let artifacts: Vec<Value> = self
            .store
            .artifacts_for_task(&request.task_id)
            .map_err(map_store_error)?
            .into_iter()
            .map(|artifact| {
                json!({"id": artifact.id, "kind": artifact.kind, "checksum": artifact.checksum})
            })
            .collect();

        let current_destination = repositories
            .as_ref()
            .map(|(primary, _)| primary.path().to_string_lossy().into_owned());
        let mut reasons: Vec<&'static str> = Vec::new();
        let relationship = match &approval {
            None => {
                reasons.push("no_approval");
                "unavailable"
            }
            Some(approval) => {
                if !matches!(
                    approval.state,
                    ApprovalState::Requested | ApprovalState::Approved { .. }
                ) {
                    reasons.push("approval_inactive");
                }
                match (&workspace, &head) {
                    (None, _) | (_, None) => reasons.push("workspace_unavailable"),
                    (Some(_), Some(head)) => {
                        if Some(head) != candidate.as_ref() {
                            reasons.push("candidate_not_head");
                        }
                    }
                }
                if let Some(snapshot) = &snapshot {
                    if snapshot["diff_state"] == "unavailable" {
                        reasons.push("candidate_unavailable");
                    }
                }
                if candidate_runs.is_empty() {
                    reasons.push("verification_commit_mismatch");
                } else if !candidate_runs.iter().any(|(run, _)| run.status == "passed") {
                    reasons.push("verification_not_passed");
                }
                digest_relationship(
                    approval,
                    stored.as_ref(),
                    current_destination.as_deref(),
                    workspace
                        .as_ref()
                        .map(|workspace| workspace.base_commit.as_str()),
                    &mut reasons,
                )
            }
        };
        let state = if approval.is_none() {
            "missing"
        } else if reasons.is_empty() {
            "current"
        } else {
            "stale"
        };
        let review_state = match (&approval, &change_request) {
            (None, _) => "not_requested",
            (Some(approval), Some(_)) if approval.state == ApprovalState::Requested => {
                "changes_requested"
            }
            (Some(approval), _) => match approval.state {
                ApprovalState::Requested => "awaiting_decision",
                ApprovalState::Approved { .. } => "approved",
                ApprovalState::Rejected => "rejected",
                ApprovalState::Superseded { .. } => "superseded",
            },
        };
        let decision_ready =
            state == "current" && review_state == "awaiting_decision" && change_request.is_none();
        let action = stored
            .as_ref()
            .zip(approval.as_ref())
            .map(|(stored, approval)| {
                json!({
                    "action": stored["action"],
                    "destination": {"project_id": project.id},
                    "base_sha": stored["base_sha"],
                    "target_ref": stored["target_ref"],
                    "candidate_commit": approval.action.candidate_commit().as_str(),
                })
            });

        Ok(json!({
            "task_id": request.task_id,
            "project_id": project.id,
            "state": state,
            "reasons": reasons,
            "review_state": review_state,
            "decision_ready": decision_ready,
            "approval": approval.as_ref().map(approval_json),
            "action": action,
            "digest_relationship": relationship,
            "candidate": {
                "commit": candidate,
                "source": candidate_source,
                "workspace_head": head,
            },
            "workspace": workspace.as_ref().map(|workspace| json!({
                "id": workspace.id,
                "branch": workspace.branch,
                "base_commit": workspace.base_commit,
            })),
            "verification": verification,
            "artifacts": artifacts,
            "snapshot": snapshot,
            "change_request": change_request.as_ref().map(change_json),
        }))
    }
}

/// Compare the approval digest with its readable parameters and the current
/// destination and base. Adds a reason for each difference.
fn digest_relationship(
    approval: &Approval,
    stored: Option<&Value>,
    current_destination: Option<&str>,
    current_base: Option<&str>,
    reasons: &mut Vec<&'static str>,
) -> &'static str {
    let Some(stored) = stored else {
        return "unavailable";
    };
    let (Some(action), Some(destination), Some(base_sha)) = (
        stored["action"].as_str(),
        stored["destination"].as_str(),
        stored["base_sha"].as_str(),
    ) else {
        reasons.push("digest_mismatch");
        return "mismatch";
    };
    let (delivery_action, kind) = match action {
        "report_only" => (DeliveryAction::ReportOnly, ActionKind::ReportPublish),
        "local_review" => (DeliveryAction::LocalReview, ActionKind::LocalReviewMerge),
        _ => {
            reasons.push("digest_mismatch");
            return "mismatch";
        }
    };
    let recomputed = delivery_digest(
        delivery_action,
        destination,
        base_sha,
        stored["target_ref"].as_str(),
    )
    .map(ParameterDigest::from_sha256);
    let mut matches = true;
    if recomputed.as_ref() != Ok(approval.action.parameter_digest())
        || approval.action.kind() != ApprovalActionKind::Delivery(kind)
    {
        reasons.push("digest_mismatch");
        matches = false;
    }
    if current_destination.is_some_and(|current| current != destination) {
        reasons.push("destination_changed");
        matches = false;
    }
    if current_base.is_some_and(|current| current != base_sha) {
        reasons.push("workspace_base_changed");
        matches = false;
    }
    if matches {
        "matches"
    } else {
        "mismatch"
    }
}

fn snapshot_json(
    worktree: &Repository,
    base: &str,
    candidate: &str,
    path: Option<&str>,
) -> Result<Value, RpcError> {
    let unavailable = || {
        json!({
            "base_commit": base,
            "candidate_commit": candidate,
            "files": [],
            "files_truncated": false,
            "path": path,
            "diff": null,
            "diff_state": "unavailable",
            "diff_bytes": 0,
        })
    };
    let (mut files, diff) = match worktree.snapshot_between(base, candidate, path.map(Path::new)) {
        Ok(snapshot) => snapshot,
        Err(
            orchestrator_git::Error::InvalidDiffBase
            | orchestrator_git::Error::InvalidDiffCandidate
            | orchestrator_git::Error::InvalidBaseRef { .. },
        ) => return Ok(unavailable()),
        Err(error) => return Err(map_git_error(error)),
    };
    let files_truncated = files.len() > MAX_SNAPSHOT_FILES;
    files.truncate(MAX_SNAPSHOT_FILES);
    let diff_bytes = diff.len();
    let (diff, diff_state) = if diff_bytes > MAX_DIFF_BYTES {
        (Value::Null, "too_large")
    } else {
        (Value::String(diff), "complete")
    };
    Ok(json!({
        "base_commit": base,
        "candidate_commit": candidate,
        "files": files,
        "files_truncated": files_truncated,
        "path": path,
        "diff": diff,
        "diff_state": diff_state,
        "diff_bytes": diff_bytes,
    }))
}

fn verification_json(
    run: &orchestrator_core::records::VerificationRun,
    envelope: &Value,
    candidate: Option<&str>,
) -> Value {
    let details = &envelope["details"];
    let oversized = details.to_string().len() > MAX_VERIFICATION_DETAIL_BYTES;
    json!({
        "id": run.id,
        "status": run.status,
        "commit": run.commit,
        "started_at_ms": envelope["started_at_ms"],
        "finished_at_ms": envelope["finished_at_ms"],
        "matches_candidate": Some(run.commit.as_str()) == candidate,
        "details": if oversized { Value::Null } else { details.clone() },
        "details_truncated": oversized,
    })
}

fn approval_json(approval: &Approval) -> Value {
    let state = match approval.state {
        ApprovalState::Requested => "requested",
        ApprovalState::Approved { .. } => "approved",
        ApprovalState::Rejected => "rejected",
        ApprovalState::Superseded { .. } => "superseded",
    };
    let kind = match approval.action.kind() {
        ApprovalActionKind::Delivery(ActionKind::ReportPublish) => "report_publish",
        ApprovalActionKind::Delivery(ActionKind::LocalReviewMerge) => "local_review_merge",
        ApprovalActionKind::Delivery(_) => "delivery",
        ApprovalActionKind::PolicyChange => "policy_change",
    };
    json!({
        "id": approval.id,
        "state": state,
        "kind": kind,
        "task_id": approval.action.task_id(),
        "candidate_commit": approval.action.candidate_commit().as_str(),
    })
}

fn change_json(change: &orchestrator_store::ChangeRequest) -> Value {
    json!({
        "id": change.id,
        "task_id": change.task_id,
        "approval_id": change.approval_id,
        "candidate_commit": change.candidate_commit,
        "feedback": change.feedback,
        "requested_by": change.requested_by,
        "created_at": change.created_at,
    })
}

/// Accept only a relative, option-free path. Snapshot paths name committed
/// objects, so the worktree filesystem is never consulted.
fn validate_snapshot_path(path: &str) -> Result<String, RpcError> {
    let components = Path::new(path);
    if path.is_empty()
        || path.starts_with('-')
        || components.is_absolute()
        || components.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(RpcError::InvalidParams);
    }
    Ok(path.to_owned())
}

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(super) fn map_review_error(error: ReviewError) -> RpcError {
    match error {
        ReviewError::Approval(error) => map_approval_error(error),
        ReviewError::Store(error) => map_store_error(error),
        ReviewError::ChangesRequested { .. }
        | ReviewError::AlreadyRequested { .. }
        | ReviewError::CandidateMismatch => RpcError::Conflict,
        ReviewError::InvalidFeedback => RpcError::InvalidParams,
    }
}
