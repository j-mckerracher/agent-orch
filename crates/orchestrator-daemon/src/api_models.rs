//! Agent catalog, selection validation and effective configuration (M7-09).
//!
//! Plan: /Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/docs/technical-implementation-plan.md
//!
//! The catalog is read from the installed agent CLIs, never from a built-in
//! list. A selection is checked against it when a task is submitted and again
//! when the agent is launched. RPC errors are sanitized, so the precise reason
//! for a refused selection is available from `agents.validate_selection`.
//!
//! Reads never run under a lock another request needs. Each agent has at most
//! one read in flight (single-flight); callers wait for it only up to
//! `ExecutorConfig::catalog_wait` and otherwise get an `unknown` catalog with
//! a retry hint while the read finishes in the background. Only a definitive
//! result (no unknown facet) is reused, so one slow read is never remembered.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Condvar};
use std::time::{Duration, Instant};

use orchestrator_agents::{provider_of, AgentCatalog, SelectionError};
use orchestrator_core::contract::ModelSelection;

use super::*;
use crate::executor::log_safe;

/// Agents listed by `agents.catalog`, in display order.
const CATALOG_AGENTS: [&str; 3] = ["codex", "claude-code", "pi"];
/// How long a definitive catalog read is reused. Launch always reads afresh.
const CATALOG_TTL: Duration = Duration::from_secs(30);
const READ_PENDING: &str = "the catalog is still being read; retry shortly";

/// Definitive reads plus the reads in flight.
#[derive(Default)]
pub(crate) struct CatalogCache {
    /// Lock order: `flights`, then `entries`.
    entries: Mutex<HashMap<String, (Instant, AgentCatalog)>>,
    flights: Mutex<HashMap<String, Arc<Flight>>>,
}

/// One read in progress, shared by every caller that wants the same agent.
#[derive(Default)]
struct Flight {
    result: Mutex<Option<AgentCatalog>>,
    done: Condvar,
}

enum Source {
    Cached(AgentCatalog),
    Reading(Arc<Flight>),
}

impl Flight {
    fn wait_until(&self, deadline: Instant) -> Option<AgentCatalog> {
        let mut result = self
            .result
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        loop {
            if let Some(catalog) = result.as_ref() {
                return Some(catalog.clone());
            }
            let remaining = deadline.checked_duration_since(Instant::now())?;
            result = self
                .done
                .wait_timeout(result, remaining)
                .unwrap_or_else(|error| error.into_inner())
                .0;
        }
    }
}

impl CatalogCache {
    /// The cached catalog, or the read to wait for (started here if none is
    /// running).
    fn source(
        self: &Arc<Self>,
        agent: &str,
        config: &ExecutorConfig,
        working_dir: PathBuf,
    ) -> Source {
        let mut flights = self
            .flights
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(catalog) = self.cached(agent) {
            return Source::Cached(catalog);
        }
        if let Some(flight) = flights.get(agent) {
            return Source::Reading(flight.clone());
        }
        let flight = Arc::new(Flight::default());
        flights.insert(agent.to_owned(), flight.clone());
        let started = std::thread::Builder::new()
            .name("catalog-read".into())
            .spawn({
                let (cache, config, flight, agent) = (
                    self.clone(),
                    config.clone(),
                    flight.clone(),
                    agent.to_owned(),
                );
                move || {
                    let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        read_blocking(&config, &agent, &working_dir)
                    }));
                    let catalog = read.unwrap_or_else(|_| {
                        AgentCatalog::unknown(&agent, "the catalog read failed unexpectedly")
                    });
                    cache.finish(&agent, &flight, catalog);
                }
            });
        if started.is_err() {
            flights.remove(agent);
            *flight
                .result
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = Some(AgentCatalog::unknown(
                agent,
                "the catalog read could not be started",
            ));
        }
        Source::Reading(flight)
    }

    fn cached(&self, agent: &str) -> Option<AgentCatalog> {
        let entries = self
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        entries
            .get(agent)
            .filter(|(read_at, _)| read_at.elapsed() < CATALOG_TTL)
            .map(|(_, catalog)| catalog.clone())
    }

    fn finish(&self, agent: &str, flight: &Flight, catalog: AgentCatalog) {
        {
            let mut flights = self
                .flights
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            // A failed or partial read is not a fact worth reusing: it would
            // refuse selections for a whole TTL after one slow probe.
            if catalog.is_definitive() {
                let mut entries = self
                    .entries
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                entries.retain(|_, (read_at, _)| read_at.elapsed() < CATALOG_TTL);
                entries.insert(agent.to_owned(), (Instant::now(), catalog.clone()));
            }
            flights.remove(agent);
        }
        *flight
            .result
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(catalog);
        flight.done.notify_all();
    }
}

fn read_blocking(
    config: &ExecutorConfig,
    agent: &str,
    working_dir: &std::path::Path,
) -> AgentCatalog {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return AgentCatalog::unknown(agent, "the catalog read could not be started");
    };
    runtime.block_on(config.read_catalog(agent, working_dir, config.catalog_timeout))
}

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
        serde_json::to_value(self.catalogs(&CATALOG_AGENTS)).map_err(|_| RpcError::Internal)
    }

    /// Reports whether a selection would be accepted, and if not, why.
    pub(super) fn agents_validate_selection(&self, params: Value) -> Result<Value, RpcError> {
        let params: ValidateSelectionParams = parse_params(params)?;
        let selection = ModelSelection {
            model: params.model,
            effort: params.effort,
        };
        selection.validate().map_err(|_| RpcError::InvalidParams)?;
        let catalog = self.catalog(&params.agent);
        Ok(match check(&catalog, &selection) {
            Ok(()) => json!({"valid": true}),
            Err(error) => json!({
                "valid": false,
                "reason": error.reason,
                "detail": error.detail,
            }),
        })
    }

    /// Reads, before the idempotency gate is taken, the catalog a submitted
    /// selection is checked against. `None` when the submission carries no
    /// selection (or a malformed one, which the contract check refuses). A
    /// catalog that is not fully known is `Conflict`, refused here so that
    /// the transient failure is not stored under the request's idempotency
    /// key: retry with the same key.
    pub(super) fn selection_catalog(
        &self,
        agent: &str,
        selection: Option<&ModelSelection>,
    ) -> Result<Option<AgentCatalog>, RpcError> {
        let Some(selection) = selection else {
            return Ok(None);
        };
        if agent.is_empty() || selection.validate().is_err() {
            return Ok(None);
        }
        let catalog = self.catalog(agent);
        if !catalog.is_definitive() {
            eprintln!(
                "orchestrator-daemon: model selection for agent {} deferred: {}",
                log_safe(agent),
                log_safe(&catalog.detail)
            );
            return Err(RpcError::Conflict);
        }
        Ok(Some(catalog))
    }

    /// Submit-time gate. A request that can never be satisfied is
    /// `InvalidParams`; one that fails because the adapter's state could not
    /// be established (not installed, signed out, unreadable) is `Conflict`.
    pub(super) fn check_selection(
        &self,
        catalog: &AgentCatalog,
        selection: &ModelSelection,
    ) -> Result<(), RpcError> {
        check(catalog, selection).map_err(|error| {
            eprintln!(
                "orchestrator-daemon: model selection for agent {} refused: {}",
                log_safe(&catalog.agent),
                log_safe(&error.to_string())
            );
            if error.reason.is_invalid_request() {
                RpcError::InvalidParams
            } else {
                RpcError::Conflict
            }
        })
    }

    fn catalog(&self, agent: &str) -> AgentCatalog {
        self.catalogs(&[agent]).remove(0)
    }

    /// Reads the catalogs together, so the slowest CLI bounds the call rather
    /// than the sum, and never longer than `catalog_wait` in total. An agent
    /// whose read is not finished by then is reported `unknown`.
    fn catalogs(&self, agents: &[&str]) -> Vec<AgentCatalog> {
        self.catalog_cache.read(&self.config, agents)
    }
}

impl CatalogCache {
    pub(crate) fn read(
        self: &Arc<Self>,
        config: &ExecutorConfig,
        agents: &[&str],
    ) -> Vec<AgentCatalog> {
        let deadline = Instant::now() + config.catalog_wait;
        let working_dir = config
            .worktrees_root
            .parent()
            .map_or_else(|| PathBuf::from("."), PathBuf::from);
        let sources: Vec<_> = agents
            .iter()
            .map(|agent| self.source(agent, config, working_dir.clone()))
            .collect();
        sources
            .into_iter()
            .zip(agents)
            .map(|(source, agent)| match source {
                Source::Cached(catalog) => catalog,
                Source::Reading(flight) => flight
                    .wait_until(deadline)
                    .unwrap_or_else(|| AgentCatalog::unknown(agent, READ_PENDING)),
            })
            .collect()
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
