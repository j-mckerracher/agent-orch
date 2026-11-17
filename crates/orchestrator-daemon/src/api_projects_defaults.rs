//! Per-project defaults served and enforced (DPES-1105 phase 4b): `projects.get_defaults`,
//! `projects.set_defaults`, and the envelope a task proposed from chat is checked against.
//!
//! - `projects.get_defaults` takes `{project_id}` and is open to any session with the
//!   `projects` capability. It answers `{defaults: {mode, base_branch, branch_prefix, checks,
//!   merge_method}}`; a project with no row reads as `local-only` with the rest empty.
//! - `projects.set_defaults` is user-only: the transport refuses a session that did not prove
//!   the approver secret before this handler runs. It takes the whole row plus an
//!   `idempotency_key`. A bad mode, check, prefix or base branch name is `invalid_params`; a
//!   base branch the repository does not have is `invalid_params` too; an unknown project is
//!   `not_found`; an archived project is `conflict`. Nothing is written on a refusal.
//! - A project with no row, or one in `local-only`, runs exactly the fixed chat envelope. A
//!   pull-request mode lets chat ask for `PullRequest` delivery only while the global
//!   `chat_pr_delivery` setting is on (off by default). Both are loosenings only the user sets.
//!
//! No reply, log line or card text holds a path, a branch name, a prefix or a check command.

use std::path::Path;

use orchestrator_core::authorization::Authorization;
use orchestrator_core::contract::TaskKind;
use orchestrator_core::orchestrator_action::{Action, ProposeTask};
use orchestrator_core::project_mode::{drops, MergeMethod, ProjectDefaults, ProjectMode};
use orchestrator_core::task_envelope::{
    chat_task_envelope, check_task, in_place_envelope, task_envelope_for, Denial,
};
use orchestrator_git::{is_safe_branch_name, BranchPrefix};
use orchestrator_store::{ProjectDefaultsRow, SqliteStore};

use super::*;

const LOG: &str = "projects";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GetParams {
    project_id: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SetParams {
    project_id: String,
    mode: String,
    base_branch: Option<String>,
    branch_prefix: Option<String>,
    checks: Vec<Vec<String>>,
    merge_method: String,
    idempotency_key: String,
}

/// The typed defaults of a stored row. The table refuses an unknown mode or method, so the
/// fallbacks only name the least authority.
pub(super) fn typed(row: &ProjectDefaultsRow) -> ProjectDefaults {
    ProjectDefaults {
        mode: ProjectMode::parse(&row.mode).unwrap_or(ProjectMode::LocalOnly),
        base_branch: row.base_branch.clone(),
        branch_prefix: row.branch_prefix.clone(),
        checks: row.checks.clone(),
        merge_method: MergeMethod::parse(&row.merge_method).unwrap_or_default(),
    }
}

/// The envelope a task of `kind` proposed from chat is checked against, for the project
/// `project_id` (`None` when it is unknown). No project, no row and `local-only` all give the
/// fixed chat envelope, so a project that never set a default behaves as before. A pull-request
/// mode goes through `task_envelope_for` with the current `chat_pr_delivery` setting. A project
/// that runs in place (the home directory) has no candidate to review or land, so only
/// `ReportOnly` is granted.
pub(super) fn envelope_for(
    store: &SqliteStore,
    project_id: Option<&str>,
    kind: TaskKind,
) -> Result<Authorization, StoreError> {
    let Some(project_id) = project_id else {
        return Ok(chat_task_envelope());
    };
    if store
        .get_project(project_id)
        .is_ok_and(|project| project.is_in_place())
    {
        return Ok(in_place_envelope());
    }
    let Some(row) = store.project_defaults(project_id)? else {
        return Ok(chat_task_envelope());
    };
    let defaults = typed(&row);
    if defaults.mode == ProjectMode::LocalOnly {
        return Ok(chat_task_envelope());
    }
    let setting = store.orchestrator_settings()?.chat_pr_delivery;
    Ok(task_envelope_for(Some(&defaults), setting, kind))
}

/// The registered project whose code is `code`, when exactly one matches.
pub(super) fn project_id_for_code(store: &SqliteStore, code: &str) -> Option<String> {
    let mut matches = store
        .list_projects()
        .ok()?
        .into_iter()
        .filter(|project| project_code(&project.id) == code);
    match (matches.next(), matches.next()) {
        (Some(project), None) => Some(project.id),
        _ => None,
    }
}

/// The envelope's refusal of a `task.propose`, as the reply sink checks it when it stores the
/// block. When the defaults cannot be read it checks against the fixed chat envelope, the
/// least authority.
pub(super) fn denial_of(store: &SqliteStore, propose: &ProposeTask) -> Option<Denial> {
    let project = project_id_for_code(store, &propose.project);
    let envelope = envelope_for(store, project.as_deref(), propose.task_kind)
        .unwrap_or_else(|_| chat_task_envelope());
    check_task(propose, &envelope).err()
}

/// The envelope's refusal of a `task.revise`, as the reply sink checks it when it stores the
/// block: the old task's delivery must still be granted for its own project. A task that is
/// gone is not refused here (execution fails it with `not_found`); an unreadable store
/// refuses, so nothing passes on a guess. The combined objective must fit its bound.
pub(super) fn revise_denial(
    store: &SqliteStore,
    revise: &orchestrator_core::orchestrator_action::ReviseTask,
) -> Option<Denial> {
    let old = match store.get(&revise.task_id) {
        Ok(old) => old,
        Err(StoreError::NotFound(_)) => return None,
        Err(_) => return Some(Denial::Delivery),
    };
    // An over-long objective is refused at the card, not after the user approves.
    let objective =
        super::actions::revised_objective(&old.contract.objective, &revise.added_intent);
    if objective.len() > orchestrator_core::contract::limits::MAX_OBJECTIVE_BYTES {
        return Some(Denial::Contract);
    }
    match envelope_for(store, Some(&old.contract.project_id), old.contract.kind) {
        Ok(envelope) => {
            (!envelope.delivery_grants(old.contract.delivery)).then_some(Denial::Delivery)
        }
        Err(_) => Some(Denial::Delivery),
    }
}

fn defaults_wire(row: &ProjectDefaultsRow) -> Value {
    json!({
        "mode": row.mode,
        "base_branch": row.base_branch,
        "branch_prefix": row.branch_prefix,
        "checks": row.checks,
        "merge_method": row.merge_method,
    })
}

/// A value the store refused is the caller's mistake; any other failure is ours.
fn defaults_store_error(error: StoreError) -> RpcError {
    match error {
        StoreError::Io(io) if io.kind() == std::io::ErrorKind::InvalidInput => {
            RpcError::InvalidParams
        }
        other => map_store_error(other),
    }
}

impl DaemonApi {
    /// `projects.get_defaults`. Reads only; a project with no row reads as the local defaults.
    pub(super) fn projects_get_defaults(&self, params: Value) -> Result<Value, RpcError> {
        let request: GetParams = parse_params(params)?;
        let project = self
            .store
            .get_project(&request.project_id)
            .map_err(map_store_error)?;
        let row = self
            .store
            .project_defaults_or_local(&project.id)
            .map_err(map_store_error)?;
        Ok(json!({"defaults": defaults_wire(&row)}))
    }

    /// `projects.set_defaults`: replaces one project's defaults. User-only (routed in the ipc
    /// crate). Idempotent by key.
    pub(super) fn projects_set_defaults(&self, params: Value) -> Result<Value, RpcError> {
        let request: SetParams = parse_params(params)?;
        let mode = ProjectMode::parse(&request.mode).ok_or(RpcError::InvalidParams)?;
        let merge_method =
            MergeMethod::parse(&request.merge_method).ok_or(RpcError::InvalidParams)?;
        let defaults = ProjectDefaults {
            mode,
            base_branch: request.base_branch.clone(),
            branch_prefix: request.branch_prefix.clone(),
            checks: request.checks.clone(),
            merge_method,
        };
        defaults.validate().map_err(|_| RpcError::InvalidParams)?;
        // The prefix is stored as `BranchPrefix::parse_for_project` reads it, and never as `orchestrator/`.
        let branch_prefix = match &request.branch_prefix {
            Some(value) => Some(
                BranchPrefix::parse_for_project(value)
                    .map_err(|_| RpcError::InvalidParams)?
                    .as_str()
                    .to_owned(),
            ),
            None => None,
        };
        if matches!(&request.base_branch, Some(branch) if !is_safe_branch_name(branch)) {
            return Err(RpcError::InvalidParams);
        }
        let key = request.idempotency_key.clone();
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "projects.set_defaults",
            &key,
            &request,
            || {
                let project = self
                    .store
                    .get_project(&request.project_id)
                    .map_err(map_store_error)?;
                if self
                    .store
                    .project_is_archived(&project.id)
                    .map_err(map_store_error)?
                {
                    return Err(RpcError::Conflict);
                }
                if let Some(branch) = &request.base_branch {
                    // A local branch only: a tag or a commit id is not a base branch.
                    let repository =
                        Repository::new(Path::new(&project.root_path)).map_err(map_git_error)?;
                    match repository.resolve_base_ref(&format!("refs/heads/{branch}")) {
                        Ok(_) => {}
                        Err(orchestrator_git::Error::InvalidBaseRef { .. }) => {
                            return Err(RpcError::InvalidParams)
                        }
                        Err(other) => return Err(map_git_error(other)),
                    }
                }
                let row = ProjectDefaultsRow {
                    project_id: project.id.clone(),
                    mode: mode.as_str().to_owned(),
                    base_branch: request.base_branch.clone(),
                    branch_prefix: branch_prefix.clone(),
                    checks: request.checks.clone(),
                    merge_method: merge_method.as_str().to_owned(),
                    updated_at: String::new(),
                };
                self.store
                    .set_project_defaults(&row)
                    .map_err(defaults_store_error)?;
                let saved = self
                    .store
                    .project_defaults_or_local(&project.id)
                    .map_err(map_store_error)?;
                self.config.diagnostics.info(
                    LOG,
                    "defaults_changed",
                    &[
                        ("project", &project_code(&project.id)),
                        ("mode", mode.as_str()),
                    ],
                );
                Ok(json!({"defaults": defaults_wire(&saved)}))
            },
        )
    }

    /// The refusal a pending `task.propose` or `task.revise` meets against the project's
    /// CURRENT defaults and `chat_pr_delivery`, for `resolve_as` to apply on approve. `None`
    /// for every other kind, and when the project or task is gone (execution fails then, with
    /// its own code). Local reads only. A store failure is an error, so nothing is approved
    /// on a guess.
    pub(super) fn recheck_task_denial(
        &self,
        proposal: &orchestrator_store::ActionProposal,
    ) -> Result<Option<Denial>, RpcError> {
        let Ok(action) = serde_json::from_str::<Action>(&proposal.params_json) else {
            return Ok(None);
        };
        match action {
            Action::TaskPropose(propose) => {
                let Some(project_id) = project_id_for_code(&self.store, &propose.project) else {
                    return Ok(None);
                };
                let envelope = envelope_for(&self.store, Some(&project_id), propose.task_kind)
                    .map_err(map_store_error)?;
                Ok(check_task(&propose, &envelope).err())
            }
            Action::TaskRevise(revise) => {
                let old = match self.store.get(&revise.task_id) {
                    Ok(old) => old,
                    Err(StoreError::NotFound(_)) => return Ok(None),
                    Err(error) => return Err(map_store_error(error)),
                };
                let envelope = envelope_for(
                    &self.store,
                    Some(&old.contract.project_id),
                    old.contract.kind,
                )
                .map_err(map_store_error)?;
                Ok((!envelope.delivery_grants(old.contract.delivery)).then_some(Denial::Delivery))
            }
            _ => Ok(None),
        }
    }

    /// The mode of the project when a `task.propose` asks for less delivery than the mode's
    /// default, for the card's "below the project default" line. `None` for a scout, for a
    /// project with no row or in `local-only`, and when nothing is below the default.
    pub(super) fn below_project_default(&self, propose: &ProposeTask) -> Option<ProjectMode> {
        if propose.task_kind != TaskKind::Implement {
            return None;
        }
        let project_id = project_id_for_code(&self.store, &propose.project)?;
        let row = self.store.project_defaults(&project_id).ok()??;
        let defaults = typed(&row);
        if defaults.mode == ProjectMode::LocalOnly {
            return None;
        }
        let envelope = envelope_for(&self.store, Some(&project_id), propose.task_kind).ok()?;
        let below = !drops(&defaults, propose.delivery, &envelope.verification).is_empty();
        below.then_some(defaults.mode)
    }
}
