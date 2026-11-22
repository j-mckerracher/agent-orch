//! Claude Code stream-json adapter tests (DPES-1181 acceptance criteria AC1-AC9 and the empty-plan
//! boundary). Each test runs a scripted child built from `fixtures/claude_stream/`, so no real
//! `claude` runs. The child records its argv and every stdin line, and the assertions read those files.

use orchestrator_agents::{
    AgentEvent, AgentExit, AgentHandle, AgentLaunchRequest, ApprovalDecision,
    ClaudeCodeStreamAdapter, ClaudeCodeStreamHandle, InteractiveAgentHandle, TurnOutcome,
};
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;
use uuid::Uuid;

const FAKE_CHILD: &str = include_str!("fixtures/claude_stream/fake_stream_child.rs");
const INIT: &str = include_str!("fixtures/claude_stream/init.json");
const EXIT_PLAN_MODE: &str = include_str!("fixtures/claude_stream/exit_plan_mode_request.json");
const BASH_REQUEST: &str = include_str!("fixtures/claude_stream/bash_request.json");
const RESULT_SUCCESS: &str = include_str!("fixtures/claude_stream/result_success.json");
const RESULT_ERROR: &str = include_str!("fixtures/claude_stream/result_error.json");
const PLAN_REQUEST_ID: &str = "83a4e245-7f2a-4c1e-9b3d-5e6f7a8b9c0d";
const SESSION_ID: &str = "0c9f1d2e-3a4b-4c5d-8e6f-7a8b9c0d1e2f";
const DENY_REASON: &str = "Plan phase: read-only. Stop after the plan.";

/// Compiles the scripted child into a fresh temp directory and returns the directory with its
/// launch request. The directory is the child's working directory.
fn child(script: &str) -> (tempfile::TempDir, AgentLaunchRequest) {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("fake_stream_child.rs");
    let program = dir.path().join(if cfg!(windows) {
        "fake_stream_child.exe"
    } else {
        "fake_stream_child"
    });
    std::fs::write(&source, FAKE_CHILD).expect("write fake child source");
    std::fs::write(dir.path().join("script.jsonl"), script).expect("write script");
    let status = Command::new("rustc")
        .args(["--edition=2021", "-O"])
        .arg(&source)
        .arg("-o")
        .arg(&program)
        .status()
        .expect("compile fake child");
    assert!(status.success(), "fake child compilation failed: {status}");
    let request = AgentLaunchRequest {
        program,
        args: Vec::new(),
        working_dir: dir.path().to_path_buf(),
        git_guard: false,
    };
    (dir, request)
}

/// Every stdin line the child received, in order. Read only after `wait`, which closes stdin
/// and waits for the child to exit, so the log is complete.
fn stdin_lines(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("stdin.log"))
        .expect("child stdin log")
        .lines()
        .map(str::to_owned)
        .collect()
}

async fn next(handle: &mut ClaudeCodeStreamHandle) -> AgentEvent {
    handle
        .next_event()
        .await
        .expect("next event")
        .expect("event before stream end")
}

async fn session(script: &str) -> (tempfile::TempDir, ClaudeCodeStreamHandle) {
    let (dir, request) = child(script);
    let handle = ClaudeCodeStreamAdapter
        .spawn_interactive(request, None)
        .await
        .expect("spawn interactive session");
    (dir, handle)
}

fn deny(reason: &str) -> ApprovalDecision {
    ApprovalDecision::Deny {
        reason: reason.to_owned(),
    }
}

#[tokio::test]
async fn init_frame_yields_session_started() {
    let (_dir, mut handle) = session(INIT).await;
    let AgentEvent::SessionStarted { session_id } = next(&mut handle).await else {
        panic!("expected SessionStarted");
    };
    assert_eq!(session_id, Uuid::parse_str(SESSION_ID).unwrap());
}

#[tokio::test]
async fn exit_plan_mode_proposes_plan_and_denies_once_on_the_wire() {
    let (dir, mut handle) = session(&format!("{INIT}{EXIT_PLAN_MODE}")).await;
    next(&mut handle).await;
    let AgentEvent::PlanProposed { approval_id, plan } = next(&mut handle).await else {
        panic!("expected PlanProposed");
    };
    assert_eq!(plan, "# Fix add()\n\n1. Edit calc.py");

    // Approve is refused before any state change, so the id is still answerable with Deny.
    assert!(handle
        .respond_approval(approval_id, ApprovalDecision::Approve)
        .await
        .is_err());
    handle
        .respond_approval(approval_id, deny(DENY_REASON))
        .await
        .expect("deny accepted");
    assert!(
        handle
            .respond_approval(approval_id, deny(DENY_REASON))
            .await
            .is_err(),
        "an approval id is answerable once"
    );
    handle.wait().await.expect("wait");

    let lines = stdin_lines(dir.path());
    assert_eq!(lines.len(), 2, "init then one deny: {lines:?}");
    assert_eq!(
        lines[1],
        format!(
            r#"{{"type":"control_response","response":{{"subtype":"success","request_id":"{PLAN_REQUEST_ID}","response":{{"behavior":"deny","message":"{DENY_REASON}"}}}}}}"#
        )
    );
}

#[tokio::test]
async fn deny_reason_is_json_escaped() {
    let (dir, mut handle) = session(&format!("{INIT}{EXIT_PLAN_MODE}")).await;
    next(&mut handle).await;
    let AgentEvent::PlanProposed { approval_id, .. } = next(&mut handle).await else {
        panic!("expected PlanProposed");
    };
    handle
        .respond_approval(approval_id, deny("Say \"stop\"\nnow"))
        .await
        .expect("deny accepted");
    handle.wait().await.expect("wait");

    assert_eq!(
        stdin_lines(dir.path())[1],
        format!(
            r#"{{"type":"control_response","response":{{"subtype":"success","request_id":"{PLAN_REQUEST_ID}","response":{{"behavior":"deny","message":"Say \"stop\"\nnow"}}}}}}"#
        )
    );
}

#[tokio::test]
async fn bash_request_surfaces_as_approval_with_tool_prefix() {
    let (_dir, mut handle) = session(&format!("{INIT}{BASH_REQUEST}")).await;
    next(&mut handle).await;
    let AgentEvent::ApprovalRequested { description, .. } = next(&mut handle).await else {
        panic!("expected ApprovalRequested");
    };
    assert!(
        description.starts_with("Bash: "),
        "description: {description}"
    );
    handle.wait().await.expect("wait");
}

#[tokio::test]
async fn result_frames_map_to_turn_outcome() {
    let (_dir, mut handle) = session(RESULT_SUCCESS).await;
    assert_eq!(
        next(&mut handle).await,
        AgentEvent::TurnFinished {
            outcome: TurnOutcome::Success
        }
    );
    handle.wait().await.expect("wait");

    let (_dir, mut handle) = session(RESULT_ERROR).await;
    assert_eq!(
        next(&mut handle).await,
        AgentEvent::TurnFinished {
            outcome: TurnOutcome::Failure
        }
    );
    handle.wait().await.expect("wait");
}

#[tokio::test]
async fn second_turn_is_refused_while_one_is_running() {
    let (dir, mut handle) = session(INIT).await;
    next(&mut handle).await;
    handle.send_turn("first").await.expect("first turn");
    assert!(matches!(
        handle.send_turn("second").await,
        Err(orchestrator_agents::AgentError::Protocol(message)) if message == "turn still running"
    ));
    handle.wait().await.expect("wait");

    let lines = stdin_lines(dir.path());
    assert_eq!(lines.len(), 2, "init then one user frame: {lines:?}");
    let frame: Value = serde_json::from_str(&lines[1]).expect("user frame JSON");
    assert_eq!(
        frame,
        json!({
            "type": "user",
            "message": {"role": "user", "content": "first"},
            "parent_tool_use_id": null,
            "session_id": "",
        })
    );
}

#[tokio::test]
async fn non_json_line_is_a_protocol_error_and_parsing_continues() {
    // The middle line is valid JSON with no `is_error` field, so it is a schema violation.
    // The next valid result must still parse.
    let script = format!(
        "not json\n{}{RESULT_SUCCESS}",
        r#"{"type":"result","subtype":"success"}"#.to_owned() + "\n"
    );
    let (_dir, mut handle) = session(&script).await;
    assert!(matches!(
        next(&mut handle).await,
        AgentEvent::ProtocolError { .. }
    ));
    assert!(matches!(
        next(&mut handle).await,
        AgentEvent::ProtocolError { .. }
    ));
    assert_eq!(
        next(&mut handle).await,
        AgentEvent::TurnFinished {
            outcome: TurnOutcome::Success
        }
    );
    handle.wait().await.expect("wait");
}

#[tokio::test]
async fn malformed_result_ends_the_turn_so_the_next_turn_can_start() {
    // `result` without `is_error` is a schema violation. The turn still ended, so a follow-up
    // turn must be accepted, not refused as "turn still running".
    let script = format!("{INIT}{}\n", r#"{"type":"result","subtype":"success"}"#);
    let (dir, mut handle) = session(&script).await;
    next(&mut handle).await;
    handle.send_turn("first").await.expect("first turn");
    assert!(matches!(
        next(&mut handle).await,
        AgentEvent::ProtocolError { .. }
    ));
    handle
        .send_turn("second")
        .await
        .expect("second turn after a malformed result");
    handle.wait().await.expect("wait");
    assert_eq!(stdin_lines(dir.path()).len(), 3, "init and two user frames");
}

#[tokio::test]
async fn argv_is_fixed_and_resume_is_appended_last() {
    let session_id = Uuid::parse_str(SESSION_ID).unwrap();
    let base = [
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--permission-prompt-tool",
        "stdio",
        "--permission-mode",
        "plan",
        "--setting-sources=",
    ];

    for (resume, expected) in [
        (None, base.to_vec()),
        (
            Some(session_id),
            [&base[..], &["--resume", SESSION_ID]].concat(),
        ),
    ] {
        let (dir, mut request) = child(INIT);
        // Caller args are ignored: the adapter owns argv.
        request.args = vec!["-p".to_owned(), "ignored".to_owned()];
        let mut handle = ClaudeCodeStreamAdapter
            .spawn_interactive(request, resume)
            .await
            .expect("spawn");
        next(&mut handle).await;
        handle.wait().await.expect("wait");

        let argv = std::fs::read_to_string(dir.path().join("argv.txt")).expect("argv log");
        assert_eq!(argv.lines().collect::<Vec<_>>(), expected);
        assert!(!argv.lines().any(|arg| arg == "-p"));
    }
}

#[tokio::test]
async fn unknown_approval_id_is_refused_and_writes_nothing() {
    let (dir, mut handle) = session(&format!("{INIT}{EXIT_PLAN_MODE}")).await;
    next(&mut handle).await;
    next(&mut handle).await;
    assert!(matches!(
        handle
            .respond_approval(Uuid::new_v4(), deny(DENY_REASON))
            .await,
        Err(orchestrator_agents::AgentError::Protocol(_))
    ));
    handle.wait().await.expect("wait");
    assert_eq!(stdin_lines(dir.path()).len(), 1, "only the init frame");
}

#[tokio::test]
async fn empty_plan_is_a_protocol_error_and_writes_nothing() {
    let empty_plan = r#"{"type":"control_request","request_id":"r-empty","request":{"subtype":"can_use_tool","tool_name":"ExitPlanMode","input":{"plan":""}}}"#;
    let (dir, mut handle) = session(&format!("{INIT}{empty_plan}\n")).await;
    next(&mut handle).await;
    assert!(matches!(
        next(&mut handle).await,
        AgentEvent::ProtocolError { .. }
    ));
    let AgentExit::Failed { .. } = handle.wait().await.expect("wait") else {
        panic!("a protocol violation fails the session");
    };
    assert_eq!(stdin_lines(dir.path()).len(), 1, "only the init frame");
}
