//! Adapter case model and process fixtures for the shared conformance matrix.
//! Each case owns its fake executable and unique temporary directory.
//! Updated: 2026-11-08T10:55:07Z.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
};

use orchestrator_agents::{
    select_adapter, AdapterCapabilityGap, AgentAdapter, AgentCapabilities, AgentError, AgentEvent,
    AgentExit, AgentHandle, AgentKind, AgentLaunchRequest, Capability, ClaudeCodeAdapter,
    CodexAdapter, PiRpcAdapter, RoutableAdapter, RoutingError, TurnOutcome,
};
use orchestrator_runtime::is_process_running;
use tempfile::TempDir;

#[derive(Clone, Copy)]
pub(super) enum Adapter {
    Codex,
    ClaudeCode,
    PiRpc,
}

#[derive(Clone, Copy)]
pub(super) enum Scenario {
    Framing,
    Launch,
    Cancellation,
    Errors,
    UnexpectedEof,
    UnsupportedFeatures,
    Interruption,
    Approvals,
    CrlfFraming,
    /// Codex 0.167 order: `thread.started`, nonterminal error item, `turn.started`.
    NonterminalWarning,
    /// Codex items outside a turn (before `turn.started` or after
    /// `turn.completed`) are protocol violations, not warnings.
    ItemOutsideTurn,
    /// A provider-reported failed turn is surfaced as `TurnOutcome::Failure`
    /// and must not be reported as a completed run.
    ErrorResult,
    /// Cancelling (twice) after a natural exit must not relabel the outcome.
    CancelAfterExit,
    /// Cancellation must also remove the child's descendant process.
    CancellationTree,
}

impl Adapter {
    fn name(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude",
            Self::PiRpc => "pi",
        }
    }

    fn kind(self) -> AgentKind {
        match self {
            Self::Codex => AgentKind::Codex,
            Self::ClaudeCode => AgentKind::ClaudeCode,
            Self::PiRpc => AgentKind::Pi,
        }
    }

    fn routable(self) -> Box<dyn RoutableAdapter> {
        match self {
            Self::Codex => Box::new(CodexAdapter::new()),
            Self::ClaudeCode => Box::new(ClaudeCodeAdapter::new()),
            Self::PiRpc => Box::new(PiRpcAdapter::new()),
        }
    }
}

impl Scenario {
    fn name(self) -> &'static str {
        match self {
            Self::Framing => "framing",
            Self::Launch => "launch",
            Self::Cancellation => "cancellation",
            Self::Errors => "errors",
            Self::UnexpectedEof => "unexpected_eof",
            Self::UnsupportedFeatures => "unsupported_features",
            Self::Interruption => "interruption",
            Self::Approvals => "approvals",
            Self::CrlfFraming => "framing",
            Self::NonterminalWarning | Self::ItemOutsideTurn | Self::ErrorResult => "errors",
            Self::CancelAfterExit => "completed",
            Self::CancellationTree => "cancellation",
        }
    }
}

pub(super) fn run(adapter: Adapter, scenario: Scenario) {
    let child = FakeChild::new();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");
    runtime.block_on(run_case(adapter, scenario, &child));
}

async fn run_case(adapter: Adapter, scenario: Scenario, child: &FakeChild) {
    match scenario {
        Scenario::Framing => framing(adapter, child).await,
        Scenario::Launch => launch(adapter, child).await,
        Scenario::Cancellation => cancellation(adapter, child).await,
        Scenario::Errors => errors(adapter, child).await,
        Scenario::UnexpectedEof => unexpected_eof(adapter, child).await,
        Scenario::UnsupportedFeatures => unsupported_features(adapter),
        Scenario::Interruption => interruption(adapter, child).await,
        Scenario::Approvals => approvals(adapter, child).await,
        Scenario::CrlfFraming => crlf_framing(adapter, child).await,
        Scenario::NonterminalWarning => nonterminal_warning(adapter, child).await,
        Scenario::ItemOutsideTurn => item_outside_turn(adapter, child).await,
        Scenario::ErrorResult => error_result(adapter, child).await,
        Scenario::CancelAfterExit => cancel_after_exit(adapter, child).await,
        Scenario::CancellationTree => cancellation_tree(adapter, child).await,
    }
}

struct FakeChild {
    directory: TempDir,
    program: PathBuf,
}

impl FakeChild {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("unique fake-child directory");
        let source = directory.path().join("fake_child.rs");
        let program = directory.path().join(if cfg!(windows) {
            "fake_child.exe"
        } else {
            "fake_child"
        });
        std::fs::write(&source, include_str!("fake_child.rs")).expect("write fake child source");
        let status = Command::new("rustc")
            .args(["--edition=2021", "-O"])
            .arg(&source)
            .arg("-o")
            .arg(&program)
            .status()
            .expect("compile fake child");
        assert!(status.success(), "fake child compilation failed: {status}");
        Self { directory, program }
    }

    fn directory(&self) -> &Path {
        self.directory.path()
    }
}

fn launch_request(
    adapter: Adapter,
    child: &FakeChild,
    scenario: Scenario,
    variant: &str,
    argument: Option<&str>,
) -> AgentLaunchRequest {
    let mut args = vec![
        adapter.name().to_owned(),
        scenario.name().to_owned(),
        variant.to_owned(),
    ];
    if let Some(argument) = argument {
        args.push(argument.to_owned());
    }
    AgentLaunchRequest {
        program: child.program.clone(),
        args,
        working_dir: child.directory().to_path_buf(),
    }
}
enum Handle {
    Shared(Box<dyn AgentHandle>),
    Pi(Box<orchestrator_agents::pi_rpc_adapter::PiRpcHandle>),
}

impl Handle {
    fn pid(&self) -> u32 {
        match self {
            Self::Shared(handle) => handle.pid(),
            Self::Pi(handle) => handle.pid(),
        }
    }

    async fn next_event(&mut self) -> orchestrator_agents::Result<Option<AgentEvent>> {
        match self {
            Self::Shared(handle) => handle.next_event().await,
            Self::Pi(handle) => handle.next_event().await,
        }
    }

    async fn cancel(&mut self) -> orchestrator_agents::Result<()> {
        match self {
            Self::Shared(handle) => handle.cancel().await,
            Self::Pi(handle) => handle.cancel().await,
        }
    }

    async fn wait(&mut self) -> orchestrator_agents::Result<AgentExit> {
        match self {
            Self::Shared(handle) => handle.wait().await,
            Self::Pi(handle) => handle.wait().await,
        }
    }

    async fn send_prompt(&mut self) -> orchestrator_agents::Result<()> {
        if let Self::Pi(handle) = self {
            handle
                .send_command(serde_json::json!({"type":"prompt","message":"M4-04"}))
                .await?;
        }
        Ok(())
    }

    async fn send_command(
        &mut self,
        command: serde_json::Value,
    ) -> orchestrator_agents::Result<String> {
        match self {
            Self::Pi(handle) => handle.send_command(command).await,
            Self::Shared(_) => panic!("RPC commands are only available for Pi"),
        }
    }

    async fn read_response(
        &mut self,
        id: &str,
    ) -> orchestrator_agents::Result<orchestrator_agents::pi_rpc_adapter::PiRpcResponse> {
        match self {
            Self::Pi(handle) => handle.read_response(id).await,
            Self::Shared(_) => panic!("RPC responses are only available for Pi"),
        }
    }
}

async fn spawn_handle(
    adapter: Adapter,
    child: &FakeChild,
    scenario: Scenario,
    variant: &str,
    argument: Option<&str>,
) -> orchestrator_agents::Result<Handle> {
    let request = launch_request(adapter, child, scenario, variant, argument);
    match adapter {
        Adapter::Codex => Ok(Handle::Shared(CodexAdapter::new().spawn(request).await?)),
        Adapter::ClaudeCode => Ok(Handle::Shared(
            ClaudeCodeAdapter::new().spawn(request).await?,
        )),
        Adapter::PiRpc => Ok(Handle::Pi(Box::new(
            PiRpcAdapter::new().spawn_rpc(request).await?,
        ))),
    }
}

async fn spawn_missing(
    adapter: Adapter,
    request: AgentLaunchRequest,
) -> orchestrator_agents::Result<Box<dyn AgentHandle>> {
    match adapter {
        Adapter::Codex => CodexAdapter::new().spawn(request).await,
        Adapter::ClaudeCode => ClaudeCodeAdapter::new().spawn(request).await,
        Adapter::PiRpc => PiRpcAdapter::new().spawn(request).await,
    }
}

async fn collect_events(handle: &mut Handle) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    while let Some(event) = handle.next_event().await.expect("next adapter event") {
        events.push(event);
    }
    events
}

async fn framing(adapter: Adapter, child: &FakeChild) {
    if matches!(adapter, Adapter::PiRpc) {
        let mut handle = spawn_handle(adapter, child, Scenario::Framing, "partial", None)
            .await
            .expect("spawn Pi framing child");
        let id = handle
            .send_command(serde_json::json!({"type":"get_state"}))
            .await
            .expect("send state request");
        let response = handle
            .read_response(&id)
            .await
            .expect("decode split response");
        assert_eq!(response.data.expect("response data")["marker"], "complete");
        assert!(matches!(
            handle.wait().await.expect("reap framing child"),
            AgentExit::Completed { code: Some(0), .. }
        ));

        let mut oversized = spawn_handle(adapter, child, Scenario::Framing, "oversized", None)
            .await
            .expect("spawn oversized Pi child");
        let id = oversized
            .send_command(serde_json::json!({"type":"get_state"}))
            .await
            .expect("send oversized state request");
        let error = oversized
            .read_response(&id)
            .await
            .expect_err("over-limit response must be rejected");
        assert!(matches!(error, AgentError::Protocol(_)), "error: {error}");
        assert!(matches!(
            oversized.wait().await.expect("reap oversized child"),
            AgentExit::Failed { code: Some(0), .. }
        ));
    } else {
        let mut handle = spawn_handle(adapter, child, Scenario::Framing, "partial", None)
            .await
            .expect("spawn fragmented-record child");
        let events = collect_events(&mut handle).await;
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AgentEvent::SessionStarted { .. })),
            "fragmented record was not decoded: {events:?}"
        );
        assert!(matches!(
            handle.wait().await.expect("reap framing child"),
            AgentExit::Completed { code: Some(0), .. }
        ));

        let mut oversized = spawn_handle(adapter, child, Scenario::Framing, "oversized", None)
            .await
            .expect("spawn oversized child");
        let events = collect_events(&mut oversized).await;
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AgentEvent::ProtocolError { .. })),
            "oversized record must produce a typed protocol error: {events:?}"
        );
        assert!(matches!(
            oversized.wait().await.expect("reap oversized child"),
            AgentExit::Failed { code: Some(0), .. }
        ));
    }
}

async fn launch(adapter: Adapter, child: &FakeChild) {
    const ARGUMENT: &str = "literal arg;$(echo must-not-run)";
    let mut handle = spawn_handle(adapter, child, Scenario::Launch, "normal", Some(ARGUMENT))
        .await
        .expect("spawn with argv");
    handle.send_prompt().await.expect("send Pi prompt");
    let exit = handle.wait().await.expect("reap argv child");
    assert!(
        matches!(exit, AgentExit::Completed { code: Some(0), .. }),
        "argument vector was not preserved: {exit:?}"
    );

    let request = AgentLaunchRequest {
        program: child.directory().join("missing-executable"),
        args: vec!["verbatim argument".to_owned()],
        working_dir: child.directory().to_path_buf(),
    };
    let error = match spawn_missing(adapter, request).await {
        Err(error) => error,
        Ok(_) => panic!("missing executable unexpectedly launched"),
    };
    assert!(matches!(error, AgentError::Launch(_)), "error: {error}");
}

async fn cancellation(adapter: Adapter, child: &FakeChild) {
    let mut handle = spawn_handle(adapter, child, Scenario::Cancellation, "normal", None)
        .await
        .expect("spawn cancellable child");
    handle.send_prompt().await.expect("send Pi prompt");
    let pid = handle.pid();
    let event = handle
        .next_event()
        .await
        .expect("startup event")
        .expect("child event before cancellation");
    assert!(
        matches!(
            event,
            AgentEvent::SessionStarted { .. } | AgentEvent::MessageDelta { .. }
        ),
        "unexpected startup event: {event:?}"
    );
    handle.cancel().await.expect("cancel child");
    assert!(
        matches!(
            handle.wait().await.expect("reap cancelled child"),
            AgentExit::Cancelled { .. }
        ),
        "cancel must remain the terminal outcome"
    );
    assert!(!is_process_running(pid), "child remains alive after reap");
}
async fn errors(adapter: Adapter, child: &FakeChild) {
    let mut nonzero = spawn_handle(adapter, child, Scenario::Errors, "nonzero", None)
        .await
        .expect("spawn nonzero child");
    nonzero.send_prompt().await.expect("send Pi prompt");
    let nonzero_exit = nonzero.wait().await.expect("reap nonzero child");

    let mut protocol = spawn_handle(adapter, child, Scenario::Errors, "protocol", None)
        .await
        .expect("spawn protocol-error child");
    protocol.send_prompt().await.expect("send Pi prompt");
    let protocol_events = collect_events(&mut protocol).await;
    let protocol_exit = protocol.wait().await.expect("reap protocol-error child");

    let nonzero_failed = matches!(&nonzero_exit, AgentExit::Failed { code: Some(7), .. });
    let protocol_failed = matches!(&protocol_exit, AgentExit::Failed { code: Some(0), .. });
    let protocol_error_surfaced = protocol_events
        .iter()
        .any(|event| matches!(event, AgentEvent::ProtocolError { .. }));
    let pi_turn_failed = !matches!(adapter, Adapter::PiRpc)
        || protocol_events.iter().any(|event| {
            matches!(
                event,
                AgentEvent::TurnFinished {
                    outcome: TurnOutcome::Failure
                }
            )
        });
    let pi_protocol_reason = !matches!(adapter, Adapter::PiRpc)
        || matches!(
            &protocol_exit,
            AgentExit::Failed {
                reason: Some(reason),
                ..
            } if reason.contains("protocol violation")
        );
    let error_contract_met = nonzero_failed
        && protocol_failed
        && protocol_error_surfaced
        && pi_turn_failed
        && pi_protocol_reason;
    assert!(
        error_contract_met,
        "nonzero exit failed={nonzero_failed}, exit-0 protocol error failed={protocol_failed}, \
         protocol error surfaced={protocol_error_surfaced}, Pi turn failed={pi_turn_failed}, \
         Pi protocol reason={pi_protocol_reason}; outcomes: {nonzero_exit:?}, \
         {protocol_exit:?}; events: {protocol_events:?}"
    );
    let AgentExit::Failed {
        reason: Some(reason),
        ..
    } = &nonzero_exit
    else {
        panic!("nonzero exit must carry a reason: {nonzero_exit:?}");
    };
    assert!(
        reason.contains("code 7"),
        "failure reason must preserve the exit code: {reason}"
    );
    if !matches!(adapter, Adapter::PiRpc) {
        assert!(
            reason.contains("last message: done"),
            "failure reason must preserve the completed message: {reason}"
        );
    }
}

/// Spawns one fake-child variant and returns every event plus the terminal exit.
async fn run_variant(
    adapter: Adapter,
    child: &FakeChild,
    scenario: Scenario,
    variant: &str,
) -> (Vec<AgentEvent>, AgentExit) {
    let mut handle = spawn_handle(adapter, child, scenario, variant, None)
        .await
        .expect("spawn variant child");
    handle.send_prompt().await.expect("send Pi prompt");
    let events = collect_events(&mut handle).await;
    let exit = handle.wait().await.expect("reap variant child");
    (events, exit)
}

fn has_protocol_error(events: &[AgentEvent]) -> bool {
    events
        .iter()
        .any(|event| matches!(event, AgentEvent::ProtocolError { .. }))
}

fn has_turn_outcome(events: &[AgentEvent], expected: TurnOutcome) -> bool {
    events
        .iter()
        .any(|event| matches!(event, AgentEvent::TurnFinished { outcome } if *outcome == expected))
}

/// A producer that terminates records with CRLF, as Windows tools commonly do,
/// must decode exactly like the LF-only stream.
async fn crlf_framing(adapter: Adapter, child: &FakeChild) {
    if matches!(adapter, Adapter::PiRpc) {
        let mut handle = spawn_handle(adapter, child, Scenario::CrlfFraming, "crlf", None)
            .await
            .expect("spawn Pi CRLF child");
        let id = handle
            .send_command(serde_json::json!({"type":"get_state"}))
            .await
            .expect("send state request");
        let response = handle
            .read_response(&id)
            .await
            .expect("decode CRLF response");
        assert_eq!(response.data.expect("response data")["marker"], "complete");
        assert!(matches!(
            handle.wait().await.expect("reap CRLF child"),
            AgentExit::Completed { code: Some(0), .. }
        ));
        return;
    }
    let (events, exit) = run_variant(adapter, child, Scenario::CrlfFraming, "crlf").await;
    assert!(
        matches!(events.first(), Some(AgentEvent::SessionStarted { .. })),
        "CRLF session record was not decoded first: {events:?}"
    );
    assert!(
        !has_protocol_error(&events),
        "CRLF is not a violation: {events:?}"
    );
    assert!(
        has_turn_outcome(&events, TurnOutcome::Success),
        "{events:?}"
    );
    assert!(
        matches!(exit, AgentExit::Completed { code: Some(0), .. }),
        "CRLF stream must complete: {exit:?}"
    );
}

/// Codex 0.167.0 emits a deprecation error item between `thread.started` and
/// `turn.started`; the command and message that follow must still complete.
async fn nonterminal_warning(adapter: Adapter, child: &FakeChild) {
    assert!(matches!(adapter, Adapter::Codex), "Codex-only ordering");
    let (events, exit) = run_variant(adapter, child, Scenario::NonterminalWarning, "warning").await;
    assert!(
        matches!(events.first(), Some(AgentEvent::SessionStarted { .. })),
        "events: {events:?}"
    );
    assert!(
        !has_protocol_error(&events),
        "warning is nonterminal: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::MessageDelta { text } if text == "done")),
        "message after the warning and command was lost: {events:?}"
    );
    assert!(
        has_turn_outcome(&events, TurnOutcome::Success),
        "{events:?}"
    );
    assert!(
        matches!(
            &exit,
            AgentExit::Completed { code: Some(0), last_message: Some(message) } if message == "done"
        ),
        "warning before the turn must not fail the run: {exit:?}"
    );
}

/// The warning exemption is only for the pre-turn error item; any other item
/// outside a turn remains a protocol violation, and an error item after the
/// turn completed is not swallowed.
async fn item_outside_turn(adapter: Adapter, child: &FakeChild) {
    assert!(matches!(adapter, Adapter::Codex), "Codex-only ordering");
    for variant in ["preturn_message", "postturn_error"] {
        let (events, exit) = run_variant(adapter, child, Scenario::ItemOutsideTurn, variant).await;
        assert!(has_protocol_error(&events), "{variant} events: {events:?}");
        assert!(
            matches!(exit, AgentExit::Failed { code: Some(0), .. }),
            "{variant} must fail the run: {exit:?}"
        );
    }
}

async fn error_result(adapter: Adapter, child: &FakeChild) {
    let variant = match adapter {
        Adapter::ClaudeCode => "result_error",
        Adapter::PiRpc => "aborted",
        Adapter::Codex => panic!("Codex has no failed-turn result record"),
    };
    let (events, exit) = run_variant(adapter, child, Scenario::ErrorResult, variant).await;
    assert!(!has_protocol_error(&events), "events: {events:?}");
    assert!(
        has_turn_outcome(&events, TurnOutcome::Failure),
        "provider failure must not be reported as success: {events:?}"
    );
    assert!(
        !has_turn_outcome(&events, TurnOutcome::Success),
        "{events:?}"
    );
    assert!(
        matches!(exit, AgentExit::Failed { code: Some(0), .. }),
        "a failed turn must not be reported as a completed run: {exit:?}"
    );
}

async fn cancel_after_exit(adapter: Adapter, child: &FakeChild) {
    let mut handle = spawn_handle(adapter, child, Scenario::CancelAfterExit, "normal", None)
        .await
        .expect("spawn naturally completing child");
    handle.send_prompt().await.expect("send Pi prompt");
    let pid = handle.pid();
    let first = handle.wait().await.expect("reap completed child");
    assert!(
        matches!(first, AgentExit::Completed { code: Some(0), .. }),
        "child must complete before late cancellation: {first:?}"
    );
    assert!(!is_process_running(pid), "completed child remains alive");
    handle.cancel().await.expect("late cancel");
    handle.cancel().await.expect("repeated late cancel");
    let second = handle.wait().await.expect("reap after late cancel");
    assert!(
        matches!(second, AgentExit::Completed { code: Some(0), .. }),
        "cancel after natural exit must not relabel the outcome: {second:?}"
    );
    assert!(
        !is_process_running(pid),
        "child restarted after late cancel"
    );
}

async fn cancellation_tree(adapter: Adapter, child: &FakeChild) {
    let mut handle = spawn_handle(adapter, child, Scenario::CancellationTree, "tree", None)
        .await
        .expect("spawn child with descendant");
    handle.send_prompt().await.expect("send Pi prompt");
    let pid = handle.pid();
    handle
        .next_event()
        .await
        .expect("startup event")
        .expect("child event before cancellation");
    let descendant: u32 = std::fs::read_to_string(child.directory().join("descendant.pid"))
        .expect("descendant pid recorded before the first event")
        .trim()
        .parse()
        .expect("descendant pid");
    assert_ne!(descendant, pid);
    assert!(is_process_running(descendant), "descendant must be running");
    handle.cancel().await.expect("cancel child");
    assert!(matches!(
        handle.wait().await.expect("reap cancelled child"),
        AgentExit::Cancelled { .. }
    ));
    assert!(!is_process_running(pid), "child remains alive after reap");
    // Bounded condition wait: process-tree teardown may finish just after reap.
    for _ in 0..200 {
        if !is_process_running(descendant) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("descendant {descendant} survived cancellation of its parent");
}

async fn unexpected_eof(adapter: Adapter, child: &FakeChild) {
    let mut handle = spawn_handle(adapter, child, Scenario::UnexpectedEof, "normal", None)
        .await
        .expect("spawn mid-turn EOF child");
    handle.send_prompt().await.expect("send Pi prompt");
    let exit = handle.wait().await.expect("reap mid-turn EOF child");
    assert!(
        matches!(exit, AgentExit::Failed { .. }),
        "EOF before turn completion must fail: {exit:?}"
    );
}

fn unsupported_features(adapter: Adapter) {
    let routed = adapter.routable();
    assert_eq!(routed.kind(), adapter.kind());
    let capabilities = routed.capabilities();
    for capability in [
        Capability::Streaming,
        Capability::InteractiveInput,
        Capability::StructuredOutput,
        Capability::Cancellation,
        Capability::SessionResumption,
        Capability::Approvals,
        Capability::ModelSelection,
    ] {
        if capability_supported(capability, &capabilities) {
            continue;
        }
        let required = BTreeSet::from([capability]);
        let error = match select_adapter(&[routed.as_ref()], &required) {
            Err(error) => error,
            Ok(_) => panic!("{:?} silently accepted {capability:?}", routed.kind()),
        };
        assert_eq!(
            error,
            RoutingError::NoMatchingAdapter {
                required,
                gaps: vec![AdapterCapabilityGap {
                    adapter: routed.kind(),
                    missing: BTreeSet::from([capability]),
                }],
            }
        );
    }
}

fn capability_supported(capability: Capability, values: &AgentCapabilities) -> bool {
    match capability {
        Capability::Streaming => values.streaming,
        Capability::InteractiveInput => values.interactive_input,
        Capability::StructuredOutput => values.structured_output,
        Capability::Cancellation => values.cancellation,
        Capability::SessionResumption => values.session_resumption,
        Capability::Approvals => values.approvals,
        Capability::ModelSelection => values.model_selection,
    }
}

async fn interruption(adapter: Adapter, child: &FakeChild) {
    let mut handle = spawn_handle(adapter, child, Scenario::Interruption, "normal", None)
        .await
        .expect("spawn interruptible child");
    handle.send_prompt().await.expect("send Pi prompt");
    let pid = handle.pid();
    let event = handle
        .next_event()
        .await
        .expect("startup event")
        .expect("child event before interruption");
    assert!(
        matches!(
            event,
            AgentEvent::SessionStarted { .. } | AgentEvent::MessageDelta { .. }
        ),
        "unexpected startup event: {event:?}"
    );
    interrupt(pid);
    let exit = handle.wait().await.expect("reap interrupted child");
    assert!(
        matches!(exit, AgentExit::Failed { .. }),
        "interrupted turn must be recorded as failed: {exit:?}"
    );
    assert!(!is_process_running(pid), "interrupted child remains alive");
}

/// Interruption mechanism differs by platform and is NOT equivalent:
/// - Unix: `kill -INT` delivers SIGINT to the child (an interrupt request).
/// - Windows: `taskkill /T /F` is an external forced termination of the tree.
///   A Windows pass proves only that an externally killed child is reported as
///   failed and reaped; it proves no graceful-interrupt behavior.
fn interrupt(pid: u32) {
    #[cfg(unix)]
    let status = Command::new("kill")
        .arg("-INT")
        .arg(pid.to_string())
        .status()
        .expect("send SIGINT");
    #[cfg(windows)]
    let status = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("interrupt child");
    assert!(status.success(), "interruption command failed: {status}");
}

async fn approvals(adapter: Adapter, child: &FakeChild) {
    let routed = adapter.routable();
    let capabilities = routed.capabilities();
    assert!(
        !capabilities.approvals,
        "approval workflow requires a protocol fixture when approvals are supported"
    );
    // No approval protocol is implemented, so a provider approval-style
    // request must never surface as (or be auto-granted through) an
    // `ApprovalRequested` event. Codex rejects the unknown record and fails
    // closed; Claude Code and Pi ignore unknown records and the run completes.
    let (events, exit) = run_variant(adapter, child, Scenario::Approvals, "request").await;
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::ApprovalRequested { .. })),
        "unsupported approval request was surfaced as supported: {events:?}"
    );
    if matches!(adapter, Adapter::Codex) {
        assert!(has_protocol_error(&events), "events: {events:?}");
        assert!(matches!(exit, AgentExit::Failed { .. }), "exit: {exit:?}");
    } else {
        assert!(
            matches!(exit, AgentExit::Completed { code: Some(0), .. }),
            "exit: {exit:?}"
        );
    }
    let required = BTreeSet::from([Capability::Approvals]);
    let error = match select_adapter(&[routed.as_ref()], &required) {
        Err(error) => error,
        Ok(_) => panic!(
            "{:?} silently accepted approval requirements",
            routed.kind()
        ),
    };
    assert_eq!(
        error,
        RoutingError::NoMatchingAdapter {
            required,
            gaps: vec![AdapterCapabilityGap {
                adapter: routed.kind(),
                missing: BTreeSet::from([Capability::Approvals]),
            }],
        }
    );
}
