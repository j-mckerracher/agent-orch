//! Copilot ACP adapter tests. Each test runs a fake `copilot --acp` child that
//! prints the lines of `script.txt` from its working directory, records every
//! stdin line to `stdin.log`, and exits when stdin closes.

use orchestrator_agents::{
    AgentError, AgentEvent, AgentExit, AgentHandle, AgentLaunchRequest, ApprovalDecision,
    CopilotAcpAdapter, CopilotAcpHandle, InteractiveAgentHandle, TurnOutcome,
};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::LazyLock;
use uuid::Uuid;

const FAKE_SOURCE: &str = include_str!("fixtures/copilot_acp/fake_copilot.rs");

static FAKE: LazyLock<tempfile::TempDir> = LazyLock::new(|| {
    let dir = tempfile::tempdir().expect("fake child directory");
    let source = dir.path().join("fake_copilot.rs");
    std::fs::write(&source, FAKE_SOURCE).expect("write fake child source");
    let status = std::process::Command::new("rustc")
        .args(["--edition", "2021", "-O"])
        .arg(&source)
        .arg("-o")
        .arg(fake_path(dir.path()))
        .status()
        .expect("compile fake child");
    assert!(status.success(), "fake child compilation failed: {status}");
    dir
});

fn fake_path(directory: &std::path::Path) -> PathBuf {
    if cfg!(windows) {
        directory.join("fake_copilot.exe")
    } else {
        directory.join("fake_copilot")
    }
}

const INITIALIZE: &str = include_str!("fixtures/copilot_acp/initialize_result.json");
const SESSION_NEW: &str = include_str!("fixtures/copilot_acp/session_new_result.json");
const SESSION_LOAD_REQUEST: &str = include_str!("fixtures/copilot_acp/session_load_request.json");
const PERMISSION_EDIT: &str = include_str!("fixtures/copilot_acp/permission_request_edit.json");
const MESSAGE_CHUNK: &str = include_str!("fixtures/copilot_acp/update_message_chunk.json");
const TOOL_CALL: &str = include_str!("fixtures/copilot_acp/update_tool_call.json");
const TOOL_CALL_FAILED: &str = include_str!("fixtures/copilot_acp/update_tool_call_failed.json");
const PROMPT_END_TURN: &str = include_str!("fixtures/copilot_acp/prompt_result_end_turn.json");

const SESSION: &str = "01a1b721-1f91-7393-ae1d-6aace8e4646a";
const PLAN_MODE: &str = "https://agentclientprotocol.com/protocol/session-modes#plan";
// The spec records no set_config_option or set_mode result, so these are minimal acks.
const SET_CONFIG_ACK: &str = r#"{"jsonrpc":"2.0","id":3,"result":{}}"#;
const SET_MODE_ACK: &str = r#"{"jsonrpc":"2.0","id":4,"result":{}}"#;
const SESSION_NEW_WITHOUT_PLAN: &str = r#"{"jsonrpc":"2.0","id":2,"result":{"sessionId":"01a1b721-1f91-7393-ae1d-6aace8e4646a","modes":{"availableModes":[{"id":"https://agentclientprotocol.com/protocol/session-modes#agent"}]}}}"#;
const INITIALIZE_WITHOUT_LOAD: &str =
    r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{}}}"#;
const THOUGHT_CHUNK: &str = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"01a1b721-1f91-7393-ae1d-6aace8e4646a","update":{"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"Considering the edit."}}}}"#;
const UNKNOWN_UPDATE: &str = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"01a1b721-1f91-7393-ae1d-6aace8e4646a","update":{"sessionUpdate":"future_update","payload":1}}}"#;

/// Writes `script` as the fake child's stdout, one line per entry.
fn workdir(script: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("working directory");
    let lines: Vec<&str> = script.iter().map(|line| line.trim()).collect();
    std::fs::write(dir.path().join("script.txt"), lines.join("\n")).expect("script");
    dir
}

fn launch(dir: &tempfile::TempDir) -> AgentLaunchRequest {
    AgentLaunchRequest {
        program: fake_path(FAKE.path()),
        args: Vec::new(),
        working_dir: dir.path().to_path_buf(),
        git_guard: false,
    }
}

async fn spawn(
    dir: &tempfile::TempDir,
    resume: Option<String>,
) -> Result<CopilotAcpHandle, AgentError> {
    CopilotAcpAdapter
        .spawn_interactive(launch(dir), "gpt-5", resume)
        .await
}

/// Ends the child by closing stdin, then returns every frame it was sent.
async fn finish(mut handle: CopilotAcpHandle, dir: &tempfile::TempDir) -> Vec<Value> {
    handle.wait().await.expect("wait");
    std::fs::read_to_string(dir.path().join("stdin.log"))
        .expect("stdin.log")
        .lines()
        .map(|line| serde_json::from_str(line).expect("frame is JSON"))
        .collect()
}

fn frame(text: &str) -> Value {
    serde_json::from_str(text).expect("fixture is JSON")
}

async fn started(handle: &mut CopilotAcpHandle) {
    let event = handle.next_event().await.expect("event");
    assert!(
        matches!(event, Some(AgentEvent::SessionStarted { .. })),
        "first event: {event:?}"
    );
}

async fn expect_event(handle: &mut CopilotAcpHandle) -> AgentEvent {
    handle
        .next_event()
        .await
        .expect("event")
        .expect("stream still open")
}

#[tokio::test]
async fn ac1_spawn_emits_one_session_started_first() {
    let dir = workdir(&[INITIALIZE, SESSION_NEW, SET_CONFIG_ACK]);
    let mut handle = spawn(&dir, None).await.expect("spawn");
    assert_eq!(
        expect_event(&mut handle).await,
        AgentEvent::SessionStarted {
            session_id: Uuid::parse_str(SESSION).expect("uuid"),
        }
    );
    handle.cancel().await.expect("cancel");
    // Nothing but the exit is left, so no second SessionStarted was queued.
    assert!(matches!(
        expect_event(&mut handle).await,
        AgentEvent::SessionExited { .. }
    ));
}

#[tokio::test]
async fn ac2_set_plan_mode_writes_mode_frame_and_refuses_when_absent() {
    let dir = workdir(&[INITIALIZE, SESSION_NEW, SET_CONFIG_ACK, SET_MODE_ACK]);
    let mut handle = spawn(&dir, None).await.expect("spawn");
    handle.set_plan_mode().await.expect("plan mode");
    let frames = finish(handle, &dir).await;
    assert_eq!(
        frames[3],
        json!({"jsonrpc": "2.0", "id": 4, "method": "session/set_mode",
            "params": {"sessionId": SESSION, "modeId": PLAN_MODE}})
    );

    let dir = workdir(&[INITIALIZE, SESSION_NEW_WITHOUT_PLAN, SET_CONFIG_ACK]);
    let mut handle = spawn(&dir, None).await.expect("spawn");
    assert!(matches!(
        handle.set_plan_mode().await,
        Err(AgentError::Protocol(_))
    ));
    let frames = finish(handle, &dir).await;
    assert_eq!(frames.len(), 3, "a refused mode switch writes nothing");
}

#[tokio::test]
async fn ac3_edit_permission_is_approval_and_allow_once_is_written() {
    let dir = workdir(&[INITIALIZE, SESSION_NEW, SET_CONFIG_ACK, PERMISSION_EDIT]);
    let mut handle = spawn(&dir, None).await.expect("spawn");
    started(&mut handle).await;
    let AgentEvent::ApprovalRequested {
        approval_id,
        description,
    } = expect_event(&mut handle).await
    else {
        panic!("expected ApprovalRequested");
    };
    assert!(
        description.starts_with("edit: "),
        "description: {description}"
    );
    handle
        .respond_approval(approval_id, ApprovalDecision::Approve)
        .await
        .expect("respond");
    let frames = finish(handle, &dir).await;
    assert_eq!(
        frames[3],
        json!({"jsonrpc": "2.0", "id": 0,
            "result": {"outcome": {"outcome": "selected", "optionId": "allow_once"}}})
    );
}

#[tokio::test]
async fn ac4_deny_is_reject_once_and_allow_always_is_never_written() {
    let dir = workdir(&[INITIALIZE, SESSION_NEW, SET_CONFIG_ACK, PERMISSION_EDIT]);
    let mut handle = spawn(&dir, None).await.expect("spawn");
    started(&mut handle).await;
    let AgentEvent::ApprovalRequested { approval_id, .. } = expect_event(&mut handle).await else {
        panic!("expected ApprovalRequested");
    };
    handle
        .respond_approval(
            approval_id,
            ApprovalDecision::Deny {
                reason: "needs review".to_owned(),
            },
        )
        .await
        .expect("respond");
    let frames = finish(handle, &dir).await;
    assert_eq!(
        frames[3],
        json!({"jsonrpc": "2.0", "id": 0,
            "result": {"outcome": {"outcome": "selected", "optionId": "reject_once"}}})
    );
    assert!(frames
        .iter()
        .all(|frame| !frame.to_string().contains("allow_always")));
}

#[tokio::test]
async fn ac5_stream_updates_map_to_canonical_events() {
    let dir = workdir(&[
        INITIALIZE,
        SESSION_NEW,
        SET_CONFIG_ACK,
        MESSAGE_CHUNK,
        THOUGHT_CHUNK,
        TOOL_CALL,
        TOOL_CALL_FAILED,
    ]);
    let mut handle = spawn(&dir, None).await.expect("spawn");
    started(&mut handle).await;
    assert_eq!(
        expect_event(&mut handle).await,
        AgentEvent::MessageDelta {
            text: "Planning the change.".to_owned(),
        }
    );
    assert_eq!(
        expect_event(&mut handle).await,
        AgentEvent::ThinkingDelta {
            text: "Considering the edit.".to_owned(),
        }
    );
    assert_eq!(
        expect_event(&mut handle).await,
        AgentEvent::ToolStarted {
            tool_id: "toolu_01".to_owned(),
            name: "read".to_owned(),
            summary: "Read calc.py".to_owned(),
        }
    );
    assert_eq!(
        expect_event(&mut handle).await,
        AgentEvent::ToolFinished {
            tool_id: "toolu_01".to_owned(),
            success: false,
            summary: "Plan mode does not permit changes outside session folder.".to_owned(),
        }
    );
    finish(handle, &dir).await;
}

/// Plan mode, then one prompt whose response is `prompt_result`. The prompt is request 5.
async fn turn_outcome_for(prompt_result: &str) -> AgentEvent {
    let dir = workdir(&[
        INITIALIZE,
        SESSION_NEW,
        SET_CONFIG_ACK,
        SET_MODE_ACK,
        prompt_result,
    ]);
    let mut handle = spawn(&dir, None).await.expect("spawn");
    started(&mut handle).await;
    handle.set_plan_mode().await.expect("plan mode");
    handle
        .send_turn("Add a docstring to calc.py")
        .await
        .expect("send_turn");
    let event = expect_event(&mut handle).await;
    finish(handle, &dir).await;
    event
}

#[tokio::test]
async fn ac6_stop_reason_decides_turn_outcome() {
    assert_eq!(
        turn_outcome_for(PROMPT_END_TURN).await,
        AgentEvent::TurnFinished {
            outcome: TurnOutcome::Success,
        }
    );
    assert_eq!(
        turn_outcome_for(r#"{"jsonrpc":"2.0","id":5,"result":{"stopReason":"refusal"}}"#).await,
        AgentEvent::TurnFinished {
            outcome: TurnOutcome::Failure,
        }
    );
}

#[tokio::test]
async fn ac7_resume_loads_session_when_advertised_and_refuses_without_it() {
    let dir = workdir(&[INITIALIZE, SESSION_NEW, SET_CONFIG_ACK]);
    let handle = spawn(&dir, Some(SESSION.to_owned())).await.expect("resume");
    let frames = finish(handle, &dir).await;
    let mut expected = frame(SESSION_LOAD_REQUEST);
    expected["params"]["cwd"] = json!(dir.path().to_string_lossy().into_owned());
    assert_eq!(frames[1], expected);
    assert!(frames.iter().all(|frame| frame["method"] != "session/new"));

    let dir = workdir(&[INITIALIZE_WITHOUT_LOAD]);
    let result = spawn(&dir, Some(SESSION.to_owned())).await;
    assert!(matches!(result, Err(AgentError::Unavailable(_))));
}

#[tokio::test]
async fn ac8_unknown_approval_id_is_refused_and_writes_nothing() {
    let dir = workdir(&[INITIALIZE, SESSION_NEW, SET_CONFIG_ACK]);
    let mut handle = spawn(&dir, None).await.expect("spawn");
    let result = handle
        .respond_approval(Uuid::new_v4(), ApprovalDecision::Approve)
        .await;
    assert!(matches!(result, Err(AgentError::Protocol(_))));
    let frames = finish(handle, &dir).await;
    assert_eq!(frames.len(), 3, "only the three handshake frames");
}

#[tokio::test]
async fn ac9_unknown_session_update_is_skipped() {
    let dir = workdir(&[
        INITIALIZE,
        SESSION_NEW,
        SET_CONFIG_ACK,
        UNKNOWN_UPDATE,
        MESSAGE_CHUNK,
    ]);
    let mut handle = spawn(&dir, None).await.expect("spawn");
    started(&mut handle).await;
    assert_eq!(
        expect_event(&mut handle).await,
        AgentEvent::MessageDelta {
            text: "Planning the change.".to_owned(),
        }
    );
    finish(handle, &dir).await;
}

#[tokio::test]
async fn prompt_error_response_finishes_turn_as_failure() {
    let dir = workdir(&[
        INITIALIZE,
        SESSION_NEW,
        SET_CONFIG_ACK,
        r#"{"jsonrpc":"2.0","id":4,"error":{"code":-32603,"message":"model unavailable"}}"#,
    ]);
    let mut handle = spawn(&dir, None).await.expect("spawn");
    started(&mut handle).await;
    handle.send_turn("hi").await.expect("send_turn");
    assert_eq!(
        expect_event(&mut handle).await,
        AgentEvent::TurnFinished {
            outcome: TurnOutcome::Failure,
        }
    );
    finish(handle, &dir).await;
}

#[tokio::test]
async fn malformed_initialize_response_fails_spawn_with_a_protocol_error() {
    let dir = workdir(&["not json"]);
    assert!(matches!(
        spawn(&dir, None).await,
        Err(AgentError::Protocol(_))
    ));
}

#[tokio::test]
async fn session_new_result_without_a_session_id_fails_spawn() {
    let dir = workdir(&[
        INITIALIZE,
        r#"{"jsonrpc":"2.0","id":2,"result":{"modes":{"availableModes":[]}}}"#,
    ]);
    assert!(matches!(
        spawn(&dir, None).await,
        Err(AgentError::Protocol(_))
    ));
}

#[tokio::test]
async fn permission_request_without_an_integer_id_fails_the_exit_after_a_completed_turn() {
    let dir = workdir(&[
        INITIALIZE,
        SESSION_NEW,
        SET_CONFIG_ACK,
        r#"{"jsonrpc":"2.0","method":"session/request_permission","params":{"toolCall":{"toolCallId":"toolu_02","kind":"edit","title":"Edit file"}}}"#,
        r#"{"jsonrpc":"2.0","id":4,"result":{"stopReason":"end_turn"}}"#,
    ]);
    let mut handle = spawn(&dir, None).await.expect("spawn");
    started(&mut handle).await;
    handle.send_turn("edit").await.expect("send_turn");
    assert!(matches!(
        expect_event(&mut handle).await,
        AgentEvent::ProtocolError { .. }
    ));
    assert_eq!(
        expect_event(&mut handle).await,
        AgentEvent::TurnFinished {
            outcome: TurnOutcome::Success,
        }
    );
    assert!(matches!(handle.wait().await, Ok(AgentExit::Failed { .. })));
}
