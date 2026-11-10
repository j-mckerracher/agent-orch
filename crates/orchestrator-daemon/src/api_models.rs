//! Agent catalog, selection validation and effective configuration (M7-09).
//!
//! Plan: /Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/docs/technical-implementation-plan.md
//!
//! The catalog is read from the installed agent CLIs, never from a built-in
//! list. A selection is checked against it when a task is submitted and again
//! when the agent is launched. RPC errors are sanitized, so the precise reason
//! for a refused selection is available from `agents.validate_selection`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use orchestrator_agents::{provider_of, AgentCatalog, CatalogState, SelectionError};
use orchestrator_core::contract::ModelSelection;

use super::*;

/// Agents listed by `agents.catalog`, in display order.
const CATALOG_AGENTS: [&str; 3] = ["codex", "claude-code", "pi"];
/// How long a definitive catalog read is reused. Launch always reads afresh.
const CATALOG_TTL: Duration = Duration::from_secs(30);

pub(crate) type CatalogCache = Mutex<HashMap<String, (Instant, AgentCatalog)>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateSelectionParams {
    agent: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    effort: Option<String>,
}

impl DaemonApi {
    /// Lists what each known agent offers for selection.
    pub(super) fn agents_catalog(&self, params: Value) -> Result<Value, RpcError> {
        require_empty_params(&params)?;
        let catalogs = self.catalogs(&CATALOG_AGENTS)?;
        serde_json::to_value(catalogs).map_err(|_| RpcError::Internal)
    }

    /// Reports whether a selection would be accepted, and if not, why.
    pub(super) fn agents_validate_selection(&self, params: Value) -> Result<Value, RpcError> {
        let params: ValidateSelectionParams = parse_params(params)?;
        let selection = ModelSelection {
            model: params.model,
            effort: params.effort,
        };
        selection.validate().map_err(|_| RpcError::InvalidParams)?;
        let catalog = self.catalog(&params.agent)?;
        Ok(match check(&catalog, &selection) {
            Ok(()) => json!({"valid": true}),
            Err(error) => json!({
                "valid": false,
                "reason": error.reason,
                "detail": error.detail,
            }),
        })
    }

    /// Submit-time gate. A request that can never be satisfied is
    /// `InvalidParams`; one that fails because the adapter's state could not
    /// be established (not installed, signed out, unreadable) is `Conflict`.
    pub(super) fn check_selection(
        &self,
        agent: &str,
        selection: &ModelSelection,
    ) -> Result<(), RpcError> {
        let catalog = self.catalog(agent)?;
        check(&catalog, selection).map_err(|error| {
            eprintln!("orchestrator-daemon: model selection for agent {agent} refused: {error}");
            if error.reason.is_invalid_request() {
                RpcError::InvalidParams
            } else {
                RpcError::Conflict
            }
        })
    }

    fn catalog(&self, agent: &str) -> Result<AgentCatalog, RpcError> {
        self.catalogs(&[agent])?
            .into_iter()
            .next()
            .ok_or(RpcError::Internal)
    }

    /// Reads the catalogs together, each on its own thread, so the slowest
    /// CLI bounds the call rather than the sum.
    fn catalogs(&self, agents: &[&str]) -> Result<Vec<AgentCatalog>, RpcError> {
        std::thread::scope(|scope| {
            let reads: Vec<_> = agents
                .iter()
                .map(|agent| scope.spawn(move || self.read_catalog_cached(agent)))
                .collect();
            reads
                .into_iter()
                .map(|read| read.join().map_err(|_| RpcError::Internal)?)
                .collect()
        })
    }

    fn read_catalog_cached(&self, agent: &str) -> Result<AgentCatalog, RpcError> {
        if let Some(catalog) = self.cached_catalog(agent) {
            return Ok(catalog);
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| RpcError::Internal)?;
        let working_dir = self
            .config
            .worktrees_root
            .parent()
            .map_or_else(|| PathBuf::from("."), PathBuf::from);
        let catalog = runtime.block_on(self.config.read_catalog(
            agent,
            &working_dir,
            self.config.catalog_timeout,
        ));
        // An unknown state is a failed read, not a fact worth reusing.
        if catalog.state != CatalogState::Unknown {
            self.catalog_cache
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(agent.to_owned(), (Instant::now(), catalog.clone()));
        }
        Ok(catalog)
    }

    fn cached_catalog(&self, agent: &str) -> Option<AgentCatalog> {
        let cache = self
            .catalog_cache
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        cache
            .get(agent)
            .filter(|(read_at, _)| read_at.elapsed() < CATALOG_TTL)
            .map(|(_, catalog)| catalog.clone())
    }
}

fn check(catalog: &AgentCatalog, selection: &ModelSelection) -> Result<(), SelectionError> {
    catalog.validate(selection.model.as_deref(), selection.effort.as_deref())
}

/// The configuration a task's agent runs with, derived only from the
/// immutable contract. A value the task did not select is reported as `null`
/// from the `adapter_default` source: the adapter chooses it at run time and
/// the engine does not know it.
pub(super) fn effective_configuration(contract: &TaskContract) -> Value {
    let selected = contract.model_selection.as_ref();
    let field = |value: Option<&String>| match value {
        Some(value) => json!({"value": value, "source": "task_selection"}),
        None => json!({"value": null, "source": "adapter_default"}),
    };
    json!({
        "agent": contract.agent,
        "provider": provider_of(&contract.agent),
        "model": field(selected.and_then(|s| s.model.as_ref())),
        "effort": field(selected.and_then(|s| s.effort.as_ref())),
    })
}
