//! Integration tests for Claude Code argument construction and stream handling.
//!
//! Updated: 2026-11-07T22:12:57-07:00.

use orchestrator_agents::claude_code_adapter::{ProtocolViolation, StreamParser};
use orchestrator_agents::{
    AgentAdapter, AgentCapabilities, AgentEvent, AgentExit, AgentHandle, AgentKind,
    AgentLaunchRequest, RoutableAdapter, TurnOutcome,
};
use orchestrator_runtime::is_process_running;
use std::future::Future;
use std::path::PathBuf;
use std::sync::LazyLock;

const SESSION_ID: &str = "b32e7013-0825-494f-8410-195b0694c19d";
const VERIFIED_OUTPUT: &str =
    include_str!("../../../docs/evidence/m4-02-macos/20261107T215309-0700.stdout.jsonl");

const FAKE_SOURCE: &str = r##"
use std::time::Duration;

const SESSION_ID: &str = "01a1b721-1f91-7393-ae1d-6aace8e4646a";

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    match mode.as_str() {
        "mid-turn" => {
            println!(r#"{{"type":"system","subtype":"init","session_id":"{SESSION_ID}"}}"#);
            println!(r#"{{"type":"stream_event","event":{{"type":"content_block_delta","delta":{{"type":"text_delta","text":"partial"}}}}}}"#);
        }
        "complete-exit-7" => {
            println!(r#"{{"type":"system","subtype":"init","session_id":"{SESSION_ID}"}}"#);
            println!(r#"{{"type":"result","subtype":"success","is_error":false,"result":"done"}}"#);
            std::process::exit(7);
        }
        "malformed" => println!("not JSON"),
        "hang" => std::thread::sleep(Duration::from_secs(300)),
        _ => std::process::exit(64),
    }
}
"##;

static WORKDIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let dir = tempfile::tempdir().expect("temporary directory").keep();
    let source = dir.join("fake_claude.rs");
    std::fs::write(&source, FAKE_SOURCE).expect("write fake executable source");
    let executable = if cfg!(windows) {
        dir.join("fake_claude.exe")
    } else {
        dir.join("fake_claude")
    };
    let status = std::process::Command::new("rustc")
        .args(["--edition", "2021", "-O"])
        .arg(&source)
        .args(["-o"])
        .arg(&executable)
        .status()
        .expect("run rustc");
    assert!(status.success(), "rustc failed: {status}");
    dir
});

fn fake_executable() -> PathBuf {
    if cfg!(windows) {
        WORKDIR.join("fake_claude.exe")
    } else {
        WORKDIR.join("fake_claude")
    }
}

fn with_runtime<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future)
}

async fn spawn_fake(mode: &str) -> orchestrator_agents::Result<Box<dyn AgentHandle>> {
    let program = fake_executable();
    orchestrator_agents::ClaudeCodeAdapter::new()
        .with_program(program.clone())
        .spawn(AgentLaunchRequest {
            program,
            args: vec![mode.to_owned()],
            working_dir: WORKDIR.clone(),
        })
        .await
}

#[test]
fn arguments_keep_prompt_separate_and_restrict_tools() {
    let prompt = "Read this literally; don't run it";
    let args = orchestrator_agents::ClaudeCodeAdapter::args_for_prompt(prompt);

    assert_eq!(args[0], "-p");
    assert_eq!(args[1], prompt);
    assert!(args
        .windows(2)
        .any(|pair| pair == ["--permission-mode", "dontAsk"]));
    assert!(args
        .windows(2)
        .any(|pair| pair == ["--tools", "Read,Edit,Write"]));
    assert!(args
        .windows(2)
        .any(|pair| pair == ["--allowedTools", "Read,Edit,Write"]));
    assert!(!args.iter().any(|argument| matches!(
        argument.as_str(),
        "--dangerously-skip-permissions" | "--allow-dangerously-skip-permissions"
    )));
    assert!(!args
        .windows(2)
        .any(|pair| pair == ["--permission-mode", "bypassPermissions"]));
}

#[test]
fn parses_the_recorded_stream_json_output() {
    let mut parser = StreamParser::new();
    let mut events = Vec::new();
    for line in VERIFIED_OUTPUT.lines() {
        if let Some(event) = parser.feed(line) {
            events.push(event.expect("verified output line"));
        }
    }

    assert!(matches!(
        events.first(),
        Some(AgentEvent::SessionStarted { session_id }) if session_id.to_string() == SESSION_ID
    ));
    let mut streamed_message = String::new();
    for event in &events {
        if let AgentEvent::MessageDelta { text } = event {
            streamed_message.push_str(text);
        }
    }
    assert_eq!(
        streamed_message,
        "Hi! What do you want to work on? The working directory looks empty and isn't a git repo. Tell me what you have in mind, like writing a script, setting up a project, or digging into some files, and I'll start on it."
    );
    assert_eq!(parser.last_message(), Some(streamed_message.as_str()));
    assert!(matches!(
        events.last(),
        Some(AgentEvent::TurnFinished {
            outcome: TurnOutcome::Success
        })
    ));
    assert!(parser.turn_finished());
    assert_eq!(parser.violation(), None);
}

#[test]
fn unknown_events_are_ignored() {
    let mut parser = StreamParser::new();
    assert_eq!(parser.feed(r#"{"type":"future_event"}"#), None);
    assert_eq!(
        parser.feed(r#"{"type":"stream_event","event":{"type":"future_delta"}}"#),
        None
    );
    assert_eq!(parser.violation(), None);
}

#[test]
fn missing_fields_in_known_events_are_protocol_violations() {
    let mut parser = StreamParser::new();
    assert!(matches!(
        parser.feed(r#"{"type":"stream_event","event":{}}"#),
        Some(Err(ProtocolViolation::BadField("event.type")))
    ));
}

#[test]
fn malformed_lines_report_a_protocol_violation_without_panicking() {
    let mut parser = StreamParser::new();
    assert!(matches!(
        parser.feed("not JSON"),
        Some(Err(ProtocolViolation::MalformedJson(_)))
    ));
    assert!(parser.violation().is_some());
    assert_eq!(parser.feed(r#"{"type":"result"}"#), None);
}

#[test]
fn eof_mid_turn_is_not_reported_as_completion() {
    with_runtime(async {
        let mut handle = spawn_fake("mid-turn").await.expect("spawn");
        let mut events = Vec::new();
        while let Some(event) = handle.next_event().await.expect("next event") {
            events.push(event);
        }

        assert!(events.iter().any(|event| matches!(
            event,
            AgentEvent::MessageDelta { text } if text == "partial"
        )));
        assert!(matches!(
            events.last(),
            Some(AgentEvent::SessionExited { exit_code: Some(0) })
        ));
        assert!(matches!(
            handle.wait().await.expect("wait"),
            AgentExit::Failed {
                code: Some(0),
                reason: None
            }
        ));
    });
}

#[test]
fn nonzero_exit_after_completed_turn_fails_and_keeps_last_message() {
    with_runtime(async {
        let mut handle = spawn_fake("complete-exit-7").await.expect("spawn");
        let exit = handle.wait().await.expect("wait");
        match exit {
            AgentExit::Failed {
                code: Some(7),
                reason: Some(reason),
            } => {
                assert!(reason.contains("code 7"), "reason: {reason}");
                assert!(reason.contains("last message: done"), "reason: {reason}");
            }
            other => panic!("expected a failed exit with code 7, got {other:?}"),
        }
    });
}

#[test]
fn malformed_process_output_fails_even_when_process_exits_zero() {
    with_runtime(async {
        let mut handle = spawn_fake("malformed").await.expect("spawn");
        assert!(matches!(
            handle.next_event().await.expect("protocol event"),
            Some(AgentEvent::ProtocolError { .. })
        ));
        assert!(matches!(
            handle.wait().await.expect("wait"),
            AgentExit::Failed {
                code: Some(0),
                reason: Some(_)
            }
        ));
    });
}

#[test]
fn cancellation_is_reported_as_a_transport_outcome() {
    with_runtime(async {
        let mut handle = spawn_fake("hang").await.expect("spawn");
        let pid = handle.pid();
        handle.cancel().await.expect("cancel");
        assert!(matches!(
            handle.wait().await.expect("wait"),
            AgentExit::Cancelled { .. }
        ));
        assert!(!is_process_running(pid));
    });
}

#[test]
fn detection_reports_verified_capabilities() {
    let adapter = orchestrator_agents::ClaudeCodeAdapter::new().with_program(fake_executable());
    assert_eq!(adapter.kind(), AgentKind::ClaudeCode);
    assert_eq!(
        adapter.capabilities(),
        AgentCapabilities {
            streaming: true,
            interactive_input: false,
            structured_output: true,
            cancellation: true,
            session_resumption: false,
            approvals: false,
            model_selection: false,
        }
    );
    assert_eq!(
        with_runtime(adapter.detect()).expect("detect"),
        adapter.capabilities()
    );
}
