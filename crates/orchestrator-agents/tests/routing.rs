//! Routing tests cover deterministic preference order and fail-closed selection.
//!
//! Unsupported requirements are rejected before spawn because `AgentHandle`
//! exposes no operations for input, resumption, approvals, or model selection.
//!
//! Updated: 2026-11-08T09:01:12+00:00.

use std::collections::BTreeSet;

use async_trait::async_trait;
use orchestrator_agents::{
    select_adapter, AdapterCapabilityGap, AgentAdapter, AgentCapabilities, AgentHandle, AgentKind,
    AgentLaunchRequest, Capability, Result, RoutableAdapter, RoutingError,
};
use orchestrator_agents::{ClaudeCodeAdapter, CodexAdapter, PiRpcAdapter};

struct FixtureAdapter {
    kind: AgentKind,
    capabilities: AgentCapabilities,
}

impl FixtureAdapter {
    fn new(kind: AgentKind, capabilities: AgentCapabilities) -> Self {
        Self { kind, capabilities }
    }
}

#[async_trait]
impl AgentAdapter for FixtureAdapter {
    fn kind(&self) -> AgentKind {
        self.kind
    }

    async fn detect(&self) -> Result<AgentCapabilities> {
        panic!("selector must not probe adapters")
    }

    async fn spawn(&self, _request: AgentLaunchRequest) -> Result<Box<dyn AgentHandle>> {
        panic!("selector must not spawn adapters")
    }
}

impl RoutableAdapter for FixtureAdapter {
    fn capabilities(&self) -> AgentCapabilities {
        self.capabilities.clone()
    }
}

fn all_capabilities() -> AgentCapabilities {
    AgentCapabilities {
        streaming: true,
        interactive_input: true,
        structured_output: true,
        cancellation: true,
        session_resumption: true,
        approvals: true,
        model_selection: true,
    }
}

#[test]
fn selects_first_adapter_that_supports_every_requirement() {
    let first = FixtureAdapter::new(
        AgentKind::Codex,
        AgentCapabilities {
            interactive_input: false,
            ..all_capabilities()
        },
    );
    let second = FixtureAdapter::new(AgentKind::ClaudeCode, all_capabilities());
    let third = FixtureAdapter::new(AgentKind::Pi, all_capabilities());
    let preferences: [&dyn RoutableAdapter; 3] = [&first, &second, &third];
    let required = BTreeSet::from([Capability::InteractiveInput, Capability::SessionResumption]);

    assert_eq!(
        select_adapter(&preferences, &required)
            .expect("second adapter covers both requirements")
            .kind(),
        AgentKind::ClaudeCode
    );
}

#[test]
fn no_match_reports_missing_capabilities_for_each_candidate() {
    let first = FixtureAdapter::new(
        AgentKind::Codex,
        AgentCapabilities {
            session_resumption: false,
            ..all_capabilities()
        },
    );
    let second = FixtureAdapter::new(
        AgentKind::ClaudeCode,
        AgentCapabilities {
            interactive_input: false,
            ..all_capabilities()
        },
    );
    let preferences: [&dyn RoutableAdapter; 2] = [&first, &second];
    let required = BTreeSet::from([Capability::InteractiveInput, Capability::SessionResumption]);

    let error = select_adapter(&preferences, &required)
        .err()
        .expect("no candidate covers both requirements");
    assert_eq!(
        error,
        RoutingError::NoMatchingAdapter {
            required: required.clone(),
            gaps: vec![
                AdapterCapabilityGap {
                    adapter: AgentKind::Codex,
                    missing: BTreeSet::from([Capability::SessionResumption]),
                },
                AdapterCapabilityGap {
                    adapter: AgentKind::ClaudeCode,
                    missing: BTreeSet::from([Capability::InteractiveInput]),
                },
            ],
        }
    );
    assert!(error.to_string().contains("InteractiveInput"));
    assert!(error.to_string().contains("SessionResumption"));
}

#[test]
fn empty_requirements_select_first_preference() {
    let first = FixtureAdapter::new(
        AgentKind::Codex,
        AgentCapabilities {
            streaming: false,
            ..all_capabilities()
        },
    );
    let second = FixtureAdapter::new(AgentKind::Pi, all_capabilities());
    let preferences: [&dyn RoutableAdapter; 2] = [&first, &second];

    assert_eq!(
        select_adapter(&preferences, &BTreeSet::new())
            .expect("empty set is covered by first adapter")
            .kind(),
        AgentKind::Codex
    );
}

#[test]
fn preference_order_decides_between_equally_capable_adapters() {
    let first = FixtureAdapter::new(AgentKind::Codex, all_capabilities());
    let second = FixtureAdapter::new(AgentKind::Pi, all_capabilities());
    let required = BTreeSet::from([Capability::Streaming]);

    // Preference order is policy, so permuting candidates may change selection.
    assert_eq!(
        select_adapter(&[&first, &second], &required)
            .expect("first preference matches")
            .kind(),
        AgentKind::Codex
    );
    assert_eq!(
        select_adapter(&[&second, &first], &required)
            .expect("new first preference matches")
            .kind(),
        AgentKind::Pi
    );
}

#[test]
fn identical_inputs_always_select_same_adapter() {
    let first = FixtureAdapter::new(AgentKind::Codex, all_capabilities());
    let second = FixtureAdapter::new(AgentKind::Pi, all_capabilities());
    let preferences: [&dyn RoutableAdapter; 2] = [&first, &second];
    let required = BTreeSet::from([Capability::Streaming, Capability::Cancellation]);

    for _ in 0..8 {
        assert_eq!(
            select_adapter(&preferences, &required)
                .expect("first preference matches")
                .kind(),
            AgentKind::Codex
        );
    }
}

#[test]
fn empty_preference_list_fails_closed() {
    let preferences: [&dyn RoutableAdapter; 0] = [];
    let required = BTreeSet::from([Capability::Streaming]);

    let error = select_adapter(&preferences, &required)
        .err()
        .expect("empty preference list must not select an adapter");
    assert_eq!(
        error,
        RoutingError::NoMatchingAdapter {
            required,
            gaps: Vec::new(),
        }
    );
}

fn assert_unsupported_requirements(adapter: &dyn RoutableAdapter, capabilities: &[Capability]) {
    for capability in capabilities {
        let required = BTreeSet::from([*capability]);
        let error = select_adapter(&[adapter], &required)
            .err()
            .expect("unsupported requirement must not select adapter");
        assert_eq!(
            error,
            RoutingError::NoMatchingAdapter {
                required,
                gaps: vec![AdapterCapabilityGap {
                    adapter: adapter.kind(),
                    missing: BTreeSet::from([*capability]),
                }],
            }
        );
    }
}

#[test]
fn codex_unavailable_capabilities_are_rejected() {
    let adapter = CodexAdapter::new();
    assert_unsupported_requirements(
        &adapter,
        &[
            Capability::InteractiveInput,
            Capability::SessionResumption,
            Capability::Approvals,
        ],
    );
}

#[test]
fn codex_routes_model_selection_and_claude_code_does_not() {
    let required = BTreeSet::from([Capability::ModelSelection]);
    let codex = CodexAdapter::new();
    let selected = select_adapter(&[&codex as &dyn RoutableAdapter], &required)
        .expect("codex validates models against its catalog");
    assert_eq!(selected.kind(), AgentKind::Codex);
}

#[test]
fn claude_code_unavailable_capabilities_are_rejected() {
    let adapter = ClaudeCodeAdapter::new();
    assert_unsupported_requirements(
        &adapter,
        &[
            Capability::InteractiveInput,
            Capability::SessionResumption,
            Capability::Approvals,
            Capability::ModelSelection,
        ],
    );
}

#[test]
fn pi_unavailable_capabilities_are_rejected() {
    let adapter = PiRpcAdapter::new();
    assert_unsupported_requirements(
        &adapter,
        &[
            Capability::SessionResumption,
            Capability::Approvals,
            Capability::ModelSelection,
        ],
    );
}
