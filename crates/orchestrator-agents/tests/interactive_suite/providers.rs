//! Wire formats behind the shared interactive-session contract. Each provider writes its scripted
//! child's input, builds the frames that child prints, and launches the real adapter against it.

use async_trait::async_trait;
use orchestrator_agents::{
    AgentLaunchRequest, ClaudeCodeStreamAdapter, CodexAppServerAdapter, CopilotAcpAdapter,
    InteractiveAgentHandle, Result,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;
use tempfile::TempDir;
use uuid::Uuid;

#[async_trait]
pub trait Provider: Sync {
    fn program(&self) -> PathBuf;
    fn session_id(&self) -> Uuid;
    /// Stdin lines the adapter writes during spawn, before any user turn.
    fn handshake_lines(&self) -> usize;
    /// Writes the child's script; `turns[n]` is printed after the handshake for the `n`th turn.
    fn write_script(&self, dir: &Path, turns: &[Vec<String>]);
    async fn launch(&self, request: AgentLaunchRequest) -> Result<Box<dyn InteractiveAgentHandle>>;
    fn text(&self, text: &str) -> String;
    fn turn_succeeded(&self, turn: usize) -> String;
    fn turn_failed(&self, turn: usize) -> String;
    fn approval_request(&self) -> String;
    fn unknown_notification(&self) -> String;
}

/// Providers whose wire protocol carries a plan proposal.
pub trait Planning: Provider {
    fn plan(&self, plan: &str) -> String;
}

/// Providers whose child can be made to exit before answering the handshake.
pub trait Handshaking: Provider {
    fn write_child_exiting_before_answer(&self, dir: &Path);
}

fn boxed<H: InteractiveAgentHandle + 'static>(handle: H) -> Box<dyn InteractiveAgentHandle> {
    Box::new(handle)
}

fn compact(json_text: &str) -> String {
    serde_json::from_str::<Value>(json_text)
        .expect("fixture is JSON")
        .to_string()
}

fn compile_fake(name: &str, source: &str) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("fake child directory");
    let source_path = dir.path().join(format!("{name}.rs"));
    std::fs::write(&source_path, source).expect("write fake child source");
    let program = dir.path().join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    });
    let status = Command::new("rustc")
        .args(["--edition", "2021", "-O"])
        .arg(&source_path)
        .arg("-o")
        .arg(&program)
        .status()
        .expect("compile fake child");
    assert!(status.success(), "fake child compilation failed: {status}");
    (dir, program)
}

const CLAUDE_INIT: &str = include_str!("../fixtures/claude_stream/init.json");
const CLAUDE_RESULT_SUCCESS: &str = include_str!("../fixtures/claude_stream/result_success.json");
const CLAUDE_RESULT_ERROR: &str = include_str!("../fixtures/claude_stream/result_error.json");
const CLAUDE_BASH_REQUEST: &str = include_str!("../fixtures/claude_stream/bash_request.json");
const CLAUDE_SESSION_ID: &str = "0c9f1d2e-3a4b-4c5d-8e6f-7a8b9c0d1e2f";
const CLAUDE_PLAN_REQUEST_ID: &str = "83a4e245-7f2a-4c1e-9b3d-5e6f7a8b9c0d";
static CLAUDE_FAKE: LazyLock<(TempDir, PathBuf)> = LazyLock::new(|| {
    compile_fake(
        "fake_claude_stream",
        include_str!("../fixtures/claude_stream/fake_stream_child.rs"),
    )
});

/// Claude Code stream-json (DPES-1181). Its child prints the init frame, then each turn's frames.
pub struct ClaudeCode;

#[async_trait]
impl Provider for ClaudeCode {
    fn program(&self) -> PathBuf {
        CLAUDE_FAKE.1.clone()
    }

    fn session_id(&self) -> Uuid {
        Uuid::parse_str(CLAUDE_SESSION_ID).expect("session uuid")
    }

    fn handshake_lines(&self) -> usize {
        1
    }

    fn write_script(&self, dir: &Path, turns: &[Vec<String>]) {
        let mut lines = vec![compact(CLAUDE_INIT)];
        lines.extend(turns.iter().flatten().cloned());
        std::fs::write(dir.join("script.jsonl"), lines.join("\n")).expect("script.jsonl");
    }

    async fn launch(&self, request: AgentLaunchRequest) -> Result<Box<dyn InteractiveAgentHandle>> {
        ClaudeCodeStreamAdapter
            .spawn_interactive(request, None)
            .await
            .map(boxed)
    }

    fn text(&self, text: &str) -> String {
        json!({
            "type": "stream_event",
            "event": {
                "type": "content_block_delta",
                "delta": {"type": "text_delta", "text": text},
            },
        })
        .to_string()
    }

    fn turn_succeeded(&self, _turn: usize) -> String {
        compact(CLAUDE_RESULT_SUCCESS)
    }

    fn turn_failed(&self, _turn: usize) -> String {
        compact(CLAUDE_RESULT_ERROR)
    }

    fn approval_request(&self) -> String {
        compact(CLAUDE_BASH_REQUEST)
    }

    fn unknown_notification(&self) -> String {
        json!({"type": "future_kind", "payload": 1}).to_string()
    }
}

impl Planning for ClaudeCode {
    fn plan(&self, plan: &str) -> String {
        json!({
            "type": "control_request",
            "request_id": CLAUDE_PLAN_REQUEST_ID,
            "request": {
                "subtype": "can_use_tool",
                "tool_name": "ExitPlanMode",
                "input": {"plan": plan},
            },
        })
        .to_string()
    }
}

const COPILOT_INITIALIZE: &str = include_str!("../fixtures/copilot_acp/initialize_result.json");
const COPILOT_SESSION_NEW: &str = include_str!("../fixtures/copilot_acp/session_new_result.json");
const COPILOT_PERMISSION: &str =
    include_str!("../fixtures/copilot_acp/permission_request_edit.json");
const COPILOT_SESSION_ID: &str = "01a1b721-1f91-7393-ae1d-6aace8e4646a";
const COPILOT_SET_CONFIG_ACK: &str = r#"{"jsonrpc":"2.0","id":3,"result":{}}"#;
/// Requests 1 to 3 are initialize, session/new and set_config_option, so the first prompt is 4.
const COPILOT_FIRST_PROMPT_ID: u64 = 4;
static COPILOT_FAKE: LazyLock<(TempDir, PathBuf)> = LazyLock::new(|| {
    compile_fake(
        "fake_copilot_acp",
        include_str!("../fixtures/copilot_acp/fake_copilot.rs"),
    )
});

/// Copilot ACP (DPES-1182). Its child prints the whole script at startup; the adapter reads it in
/// order, so each turn's response id follows the prompt ids the adapter issues.
pub struct CopilotAcp;

#[async_trait]
impl Provider for CopilotAcp {
    fn program(&self) -> PathBuf {
        COPILOT_FAKE.1.clone()
    }

    fn session_id(&self) -> Uuid {
        Uuid::parse_str(COPILOT_SESSION_ID).expect("session uuid")
    }

    fn handshake_lines(&self) -> usize {
        3
    }

    fn write_script(&self, dir: &Path, turns: &[Vec<String>]) {
        let mut lines = vec![
            compact(COPILOT_INITIALIZE),
            compact(COPILOT_SESSION_NEW),
            COPILOT_SET_CONFIG_ACK.to_owned(),
        ];
        lines.extend(turns.iter().flatten().cloned());
        std::fs::write(dir.join("script.txt"), lines.join("\n")).expect("script.txt");
    }

    async fn launch(&self, request: AgentLaunchRequest) -> Result<Box<dyn InteractiveAgentHandle>> {
        CopilotAcpAdapter
            .spawn_interactive(request, "gpt-5", None)
            .await
            .map(boxed)
    }

    fn text(&self, text: &str) -> String {
        json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {
                "sessionId": COPILOT_SESSION_ID,
                "update": {
                    "sessionUpdate": "agent_message_chunk",
                    "content": {"type": "text", "text": text},
                },
            },
        })
        .to_string()
    }

    fn turn_succeeded(&self, turn: usize) -> String {
        json!({
            "jsonrpc": "2.0",
            "id": COPILOT_FIRST_PROMPT_ID + turn as u64,
            "result": {"stopReason": "end_turn"},
        })
        .to_string()
    }

    fn turn_failed(&self, turn: usize) -> String {
        json!({
            "jsonrpc": "2.0",
            "id": COPILOT_FIRST_PROMPT_ID + turn as u64,
            "error": {"code": -32603, "message": "model unavailable"},
        })
        .to_string()
    }

    fn approval_request(&self) -> String {
        compact(COPILOT_PERMISSION)
    }

    fn unknown_notification(&self) -> String {
        json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {
                "sessionId": COPILOT_SESSION_ID,
                "update": {"sessionUpdate": "future_update", "payload": 1},
            },
        })
        .to_string()
    }
}

impl Handshaking for CopilotAcp {
    fn write_child_exiting_before_answer(&self, dir: &Path) {
        std::fs::write(dir.join("script.txt"), "").expect("empty script.txt");
    }
}

const CODEX_FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/codex_app_server"
);
const CODEX_MODEL: &str = "gpt-6-luna";
const CODEX_THREAD_ID: &str = "01a1ffc6-ddd8-78c3-bb7a-f654fa3eaf1c";
/// The fake answers every `turn/start` with this id, so every turn's notifications name it.
const CODEX_TURN_ID: &str = "turn-1";
const CODEX_APPROVAL_REQUEST: &str =
    include_str!("../fixtures/codex_app_server/approval_command_request.json");
static CODEX_FAKE: LazyLock<(TempDir, PathBuf)> = LazyLock::new(|| {
    compile_fake(
        "fake_codex_app_server",
        include_str!("../fixtures/codex_app_server/fake_codex_app_server.rs"),
    )
});

/// Codex app-server (DPES-1183). Its fake answers requests itself and prints each turn's frames
/// after that turn's `turn/start` reply, so `turns[n]` is emitted for the `n`th `send_turn`.
pub struct CodexAppServer;

fn codex_turn_completed(status: &str, error: Value) -> String {
    json!({
        "method": "turn/completed",
        "params": {
            "threadId": CODEX_THREAD_ID,
            "turn": {"id": CODEX_TURN_ID, "status": status, "error": error, "items": []},
        },
    })
    .to_string()
}

#[async_trait]
impl Provider for CodexAppServer {
    fn program(&self) -> PathBuf {
        CODEX_FAKE.1.clone()
    }

    fn session_id(&self) -> Uuid {
        Uuid::parse_str(CODEX_THREAD_ID).expect("thread uuid")
    }

    fn handshake_lines(&self) -> usize {
        3
    }

    fn write_script(&self, dir: &Path, turns: &[Vec<String>]) {
        let mut scenario = format!("bodies={CODEX_FIXTURES}\n");
        for (turn, frames) in turns.iter().enumerate() {
            if turn > 0 {
                scenario.push_str("turn\n");
            }
            for (index, frame) in frames.iter().enumerate() {
                let path = dir.join(format!("turn-{turn}-frame-{index}.json"));
                std::fs::write(&path, frame).expect("frame file");
                scenario.push_str(&format!("emit={}\n", path.display()));
            }
        }
        std::fs::write(dir.join("scenario.txt"), scenario).expect("scenario.txt");
    }

    async fn launch(&self, request: AgentLaunchRequest) -> Result<Box<dyn InteractiveAgentHandle>> {
        CodexAppServerAdapter::new()
            .spawn_interactive(request, CODEX_MODEL, None)
            .await
            .map(boxed)
    }

    fn text(&self, text: &str) -> String {
        json!({
            "method": "item/agentMessage/delta",
            "params": {
                "delta": text,
                "itemId": "item-msg-1",
                "threadId": CODEX_THREAD_ID,
                "turnId": CODEX_TURN_ID,
            },
        })
        .to_string()
    }

    fn turn_succeeded(&self, _turn: usize) -> String {
        codex_turn_completed("completed", Value::Null)
    }

    fn turn_failed(&self, _turn: usize) -> String {
        codex_turn_completed("failed", json!({"codexErrorInfo": "usageLimitExceeded"}))
    }

    fn approval_request(&self) -> String {
        compact(CODEX_APPROVAL_REQUEST)
    }

    fn unknown_notification(&self) -> String {
        json!({"method": "item/futureKind", "params": {}}).to_string()
    }
}

impl Planning for CodexAppServer {
    fn plan(&self, plan: &str) -> String {
        json!({
            "method": "item/completed",
            "params": {
                "item": {"id": "item-plan-1", "text": plan, "type": "plan"},
                "threadId": CODEX_THREAD_ID,
                "turnId": CODEX_TURN_ID,
            },
        })
        .to_string()
    }
}

impl Handshaking for CodexAppServer {
    fn write_child_exiting_before_answer(&self, dir: &Path) {
        std::fs::write(dir.join("scenario.txt"), "silent\n").expect("scenario.txt");
    }
}
