//! Standing grants (DPES-1105 phase 5a): `grants.create`, `grants.revoke` and `grants.list`.
//!
//! A grant is the one authority besides a per-action approval, so only a user session may
//! make or revoke one (both are routed user-only in the ipc crate, and the transport refuses
//! a session without the approver secret before any parameter is read). Terms are validated
//! by the core crate, and the store keeps them fixed. A grant names one project, by its
//! `p-xxxxxxxx` code, or one workstream, never every project. Nothing here, on the wire or in
//! the store, holds a path: the code is a hash of the project id.

use orchestrator_core::orchestrator_action::ActionKind;
use orchestrator_core::orchestrator_action::NEVER_GRANTABLE;
use orchestrator_core::standing_grant::{
    check_kind_name, GrantRefusal, GrantScope, GrantTerms, StandingGrant, DEFAULT_EXPIRY_SECS,
    DEFAULT_MAX_USES, GRANTABLE_KINDS, MAX_EXPIRY_SECS, MAX_TOKEN_BUDGET, MAX_USES_CEILING,
    MERGE_DEFAULT_MAX_USES, MERGE_MAX_EXPIRY_SECS, MERGE_MAX_USES_CEILING, MIN_EXPIRY_SECS,
};
use orchestrator_store::{GrantError, WorkstreamStoreError};

use super::actions::{cut_at_char, project_name};
use super::projects_defaults::project_id_for_code;
use super::*;

/// Longest workstream title a grant's display label carries.
const LABEL_BYTES: usize = 80;
/// Most grants one `grants.list` call returns.
const LIST_LIMIT_MAX: usize = 200;
const LIST_LIMIT_DEFAULT: usize = 50;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ScopeParams {
    kind: String,
    id: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CreateParams {
    scope: ScopeParams,
    kinds: Vec<String>,
    #[serde(default)]
    expires_in_secs: Option<u64>,
    #[serde(default)]
    max_uses: Option<u32>,
    token_budget: u64,
    /// Required `true` for a merge grant: the user's consent to merging with no per-action
    /// click. Absent means `false`.
    #[serde(default)]
    confirm_unattended_merge: bool,
    /// Head-branch prefix a merge grant covers. Merge grants only.
    #[serde(default)]
    branch_prefix: Option<String>,
    idempotency_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RevokeParams {
    grant_id: String,
    idempotency_key: String,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    #[serde(default)]
    include_ended: bool,
    #[serde(default)]
    limit: Option<usize>,
}

impl DaemonApi {
    /// `grants.create`: creates one grant. User-only (routed in the ipc crate). The grant id
    /// derives from the idempotency key, so a replay returns the same grant.
    pub(super) fn grants_create(&self, params: Value) -> Result<Value, RpcError> {
        let request: CreateParams = parse_params(params)?;
        let terms = self.grant_terms(&request)?;
        let id = idempotency::stable_id("grant", &request.idempotency_key);
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "grants.create",
            &request.idempotency_key,
            &request,
            || {
                let grant = self
                    .store
                    .create_standing_grant(&id, &terms, self.grants_now_ms())
                    .map_err(|error| self.map_grant_error(error))?;
                Ok(json!({"grant": self.grant_wire(&grant, self.grants_now_ms())?}))
            },
        )
    }

    /// `grants.revoke`: ends a grant now and fails every proposal it approved that has not
    /// started. User-only. A task the grant already started keeps running: a revoke stops new
    /// work, and cancelling a task is a separate approval. Takes the idempotency gate, as an
    /// approval does, so a revoke waits for a running execution and nothing starts after it.
    pub(super) fn grants_revoke(&self, params: Value) -> Result<Value, RpcError> {
        let request: RevokeParams = parse_params(params)?;
        idempotency::run(
            &self.store,
            &self.idempotency_gate,
            "grants.revoke",
            &request.idempotency_key,
            &request,
            || {
                let now = self.grants_now_ms();
                let (grant, stopped) = self
                    .store
                    .revoke_standing_grant_and_stop(&request.grant_id, now)
                    .map_err(|error| self.map_grant_error(error))?;
                Ok(json!({"grant": self.grant_wire(&grant, now)?, "stopped": stopped}))
            },
        )
    }

    /// `grants.list`: newest first, with the limits the create form must respect. Open to
    /// every session that negotiated `standing_grants`.
    pub(super) fn grants_list(&self, params: Value) -> Result<Value, RpcError> {
        let request: ListParams = if params.is_null() {
            ListParams::default()
        } else {
            parse_params(params)?
        };
        let limit = request
            .limit
            .unwrap_or(LIST_LIMIT_DEFAULT)
            .clamp(1, LIST_LIMIT_MAX);
        let now = self.grants_now_ms();
        // One row past the limit tells whether more exist.
        let mut grants = self
            .store
            .standing_grants(request.include_ended, now, limit + 1)
            .map_err(map_store_error)?;
        let more = grants.len() > limit;
        grants.truncate(limit);
        let wire = grants
            .iter()
            .map(|grant| self.grant_wire(grant, now))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({"grants": wire, "more": more, "policy": grant_policy()}))
    }

    /// The daemon clock in Unix milliseconds.
    fn grants_now_ms(&self) -> u64 {
        crate::executor::unix_now_ms()
    }

    /// The validated terms a create request stands for. A scope that names nothing the daemon
    /// knows is `GrantNotFound`; terms the core refuses are `GrantInvalid`, and only the
    /// stable refusal code is logged.
    fn grant_terms(&self, request: &CreateParams) -> Result<GrantTerms, RpcError> {
        let kinds = request
            .kinds
            .iter()
            .map(|name| check_kind_name(name))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|refusal| self.grant_refused(refusal))?;
        let scope = match request.scope.kind.as_str() {
            "project" => {
                project_id_for_code(&self.store, &request.scope.id)
                    .ok_or(RpcError::GrantNotFound)?;
                GrantScope::Project(request.scope.id.clone())
            }
            "workstream" => match self.store.get_workstream(&request.scope.id) {
                Ok(_) => GrantScope::Workstream(request.scope.id.clone()),
                Err(WorkstreamStoreError::NotFound { .. }) => return Err(RpcError::GrantNotFound),
                Err(error) => return Err(internal("grants", &format!("{error:?}"))),
            },
            _ => return Err(self.grant_refused(GrantRefusal::ScopeInvalid)),
        };
        // A merge grant has fewer uses by default than any other.
        let default_uses = if kinds.contains(&ActionKind::DeliveryMerge) {
            MERGE_DEFAULT_MAX_USES
        } else {
            DEFAULT_MAX_USES
        };
        let terms = GrantTerms {
            scope,
            kinds,
            expires_in_secs: request.expires_in_secs.unwrap_or(DEFAULT_EXPIRY_SECS),
            max_uses: request.max_uses.unwrap_or(default_uses),
            token_budget: request.token_budget,
            confirm_unattended_merge: request.confirm_unattended_merge,
            branch_prefix: request.branch_prefix.clone(),
        };
        terms
            .validate()
            .map_err(|refusal| self.grant_refused(refusal))?;
        Ok(terms)
    }

    fn grant_refused(&self, refusal: GrantRefusal) -> RpcError {
        self.config
            .diagnostics
            .warn("api", "grant_refused", &[("code", refusal.code())]);
        RpcError::GrantInvalid
    }

    fn map_grant_error(&self, error: StoreError) -> RpcError {
        match GrantError::of(&error) {
            Some(GrantError::NotFound) => RpcError::GrantNotFound,
            Some(GrantError::Invalid(refusal)) => self.grant_refused(refusal),
            Some(GrantError::IdInUse) => RpcError::Conflict,
            Some(GrantError::BadInput) => RpcError::InvalidParams,
            Some(_) | None => map_store_error(error),
        }
    }

    /// A grant as the wire shows it. `label` is a folder name or a cut title, never a path.
    fn grant_wire(&self, grant: &StandingGrant, now_ms: u64) -> Result<Value, RpcError> {
        let spent = self
            .store
            .grant_tokens_spent(&grant.id)
            .map_err(map_store_error)?;
        let label = match &grant.scope {
            GrantScope::Project(code) => project_id_for_code(&self.store, code)
                .and_then(|id| project_name(&id).map(str::to_owned)),
            GrantScope::Workstream(id) => self
                .store
                .get_workstream(id)
                .ok()
                .map(|workstream| cut_at_char(&workstream.title, LABEL_BYTES).to_owned()),
        };
        Ok(json!({
            "id": grant.id,
            "scope": {"kind": grant.scope.kind_str(), "id": grant.scope.id(), "label": label},
            "kinds": grant.kinds.iter().map(|kind| kind.as_str()).collect::<Vec<_>>(),
            "branch_prefix": grant.branch_prefix,
            "confirm_unattended_merge": grant.confirm_unattended_merge,
            "created_at_ms": grant.created_at_ms,
            "expires_at_ms": grant.expires_at_ms,
            "max_uses": grant.max_uses,
            "uses_used": grant.uses_used,
            "token_budget": grant.token_budget,
            "tokens_spent": spent,
            "status": grant.status(now_ms, spent).as_str(),
            "revoked_at_ms": grant.revoked_at_ms,
        }))
    }
}

/// The limits a create form offers, so the desktop never keeps its own copy of the list.
fn grant_policy() -> Value {
    json!({
        // The merge kind is not in this list: it has its own form, and no ordinary grant may name it.
        "grantable_kinds": GRANTABLE_KINDS
            .iter()
            .filter(|kind| **kind != ActionKind::DeliveryMerge)
            .map(|kind| kind.as_str())
            .collect::<Vec<_>>(),
        "never_grantable_kinds": NEVER_GRANTABLE,
        "default_expires_in_secs": DEFAULT_EXPIRY_SECS,
        "max_expires_in_secs": MAX_EXPIRY_SECS,
        "min_expires_in_secs": MIN_EXPIRY_SECS,
        "default_max_uses": DEFAULT_MAX_USES,
        "max_max_uses": MAX_USES_CEILING,
        "max_token_budget": MAX_TOKEN_BUDGET,
        "merge_kind_available": true,
        "merge_default_max_uses": MERGE_DEFAULT_MAX_USES,
        "merge_max_max_uses": MERGE_MAX_USES_CEILING,
        "merge_max_expires_in_secs": MERGE_MAX_EXPIRY_SECS,
    })
}
