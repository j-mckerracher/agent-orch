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
    CodexAdapter, PiRpcAdapter, RoutableAdapter, RoutingError,
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
    let protocol_event = protocol_events
        .iter()
        .any(|event| matches!(event, AgentEvent::ProtocolError { .. }));
    let protocol_event_expected = !matches!(adapter, Adapter::PiRpc);
    let error_contract_met =
        nonzero_failed && protocol_failed && (!protocol_event_expected || protocol_event);
    assert!(
        error_contract_met,
        "nonzero exit failed={nonzero_failed}, exit-0 protocol error failed={protocol_failed}, \
         protocol event expected={protocol_event_expected}, surfaced={protocol_event}; \
         outcomes: {nonzero_exit:?}, {protocol_exit:?}; events: {protocol_events:?}"
    );
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

async fn approvals(adapter: Adapter, _child: &FakeChild) {
    let routed = adapter.routable();
    let capabilities = routed.capabilities();
    assert!(
        !capabilities.approvals,
        "approval workflow requires a protocol fixture when approvals are supported"
    );
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
