//! Capability-based adapter selection without host detection or process launch.
//!
//! `RoutableAdapter::capabilities` reports static adapter facts. Preference
//! order belongs to the caller; selection returns the first complete match.
//!
//! Updated: 2026-11-08T08:55:02+00:00.

use std::collections::BTreeSet;

use thiserror::Error;

use crate::contract::{AgentAdapter, AgentCapabilities, AgentKind};

/// One capability that a caller can require from an adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    Streaming,
    InteractiveInput,
    StructuredOutput,
    Cancellation,
    SessionResumption,
    Approvals,
    ModelSelection,
}

impl Capability {
    fn is_supported_by(self, capabilities: &AgentCapabilities) -> bool {
        match self {
            Self::Streaming => capabilities.streaming,
            Self::InteractiveInput => capabilities.interactive_input,
            Self::StructuredOutput => capabilities.structured_output,
            Self::Cancellation => capabilities.cancellation,
            Self::SessionResumption => capabilities.session_resumption,
            Self::Approvals => capabilities.approvals,
            Self::ModelSelection => capabilities.model_selection,
        }
    }
}

/// An adapter's synchronous, non-probing capability report.
///
/// Implementations must not inspect the host, run `detect`, or launch a
/// process. Call `AgentAdapter::detect` separately when host verification is
/// needed before spawning.
pub trait RoutableAdapter: AgentAdapter {
    fn capabilities(&self) -> AgentCapabilities;
}

/// Required capabilities absent from one preferred adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterCapabilityGap {
    pub adapter: AgentKind,
    pub missing: BTreeSet<Capability>,
}

/// Failure to find a preferred adapter that covers every requirement.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RoutingError {
    #[error(
        "no preferred adapter supports required capabilities {required:?}; missing by adapter: {gaps:?}"
    )]
    NoMatchingAdapter {
        required: BTreeSet<Capability>,
        gaps: Vec<AdapterCapabilityGap>,
    },
}

/// Select the first preferred adapter that supports every required capability.
///
/// This function reads only each adapter's static `capabilities()` report. It
/// never probes the host or starts an agent. If no adapter matches, the error
/// lists each candidate's missing capabilities in preference order.
pub fn select_adapter<'a>(
    preferences: &[&'a dyn RoutableAdapter],
    required: &BTreeSet<Capability>,
) -> Result<&'a dyn RoutableAdapter, RoutingError> {
    let mut gaps = Vec::new();
    for adapter in preferences {
        let capabilities = adapter.capabilities();
        if required
            .iter()
            .all(|capability| capability.is_supported_by(&capabilities))
        {
            return Ok(*adapter);
        }

        let missing = required
            .iter()
            .copied()
            .filter(|capability| !capability.is_supported_by(&capabilities))
            .collect();
        gaps.push(AdapterCapabilityGap {
            adapter: adapter.kind(),
            missing,
        });
    }

    Err(RoutingError::NoMatchingAdapter {
        required: required.clone(),
        gaps,
    })
}
