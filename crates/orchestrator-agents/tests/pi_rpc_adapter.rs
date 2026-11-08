use orchestrator_agents::pi_rpc_adapter::PiRpcAdapter;
use orchestrator_agents::{
    AgentAdapter, AgentCapabilities, AgentEvent, AgentExit, AgentHandle, AgentKind,
    AgentLaunchRequest, RoutableAdapter, TurnOutcome,
};
use std::future::Future;
use std::path::PathBuf;
use std::sync::LazyLock;

const FAKE_SOURCE: &str = r##"
use std::io::{BufRead, Write};

fn field(line: &str, name: &str) -> String {
    let marker = format!("\"{name}\":\"");
    let start = line.find(&marker).expect("field") + marker.len();
    let end = line[start..].find('"').expect("string end") + start;
    line[start..end].to_owned()
}

fn response(out: &mut impl Write, line: &str, data: &str) {
    let id = field(line, "id");
    let command = field(line, "type");
    writeln!(
        out,
        r#"{{"type":"response","id":"{id}","command":"{command}","success":true,"data":{data}}}"#
    )
    .unwrap();
    out.flush().unwrap();
}

fn event(out: &mut impl Write, line: &str) {
    writeln!(out, "{line}").unwrap();
    out.flush().unwrap();
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|argument| argument == "--version") {
        println!("1.5.0");
        return;
    }
    let mode = args
        .iter()
        .map(String::as_str)
        .find(|argument| {
            matches!(
                *argument,
                "echo" | "reverse" | "interleaved" | "malformed" | "unknown" | "stderr" | "fail"
                    | "eof" | "second-turn-eof"
            )
        })
        .unwrap_or("probe");
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let stdout = std::io::stdout();
    let mut stdout = std::io::BufWriter::new(stdout.lock());
    match mode {
        "echo" | "probe" => {
            for line in lines {
                let line = line.expect("stdin line");
                let id = field(&line, "id");
                response(&mut stdout, &line, &format!(r#"{{"echo":"{id}"}}"#));
            }
        }
        "reverse" => {
            let first = lines.next().expect("first command").expect("first line");
            let second = lines.next().expect("second command").expect("second line");
            let second_id = field(&second, "id");
            response(&mut stdout, &second, &format!(r#"{{"echo":"{second_id}"}}"#));
            let first_id = field(&first, "id");
            response(&mut stdout, &first, &format!(r#"{{"echo":"{first_id}"}}"#));
        }
        "interleaved" => {
            let first = lines.next().expect("first command").expect("first line");
            let second = lines.next().expect("second command").expect("second line");
            event(
                &mut stdout,
                r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"before"}}"#,
            );
            response(
                &mut stdout,
                &second,
                r#"{"marker":"second"}"#,
            );
            event(
                &mut stdout,
                r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"between"}}"#,
            );
            response(
                &mut stdout,
                &first,
                r#"{"marker":"first"}"#,
            );
            event(
                &mut stdout,
                r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"after"}}"#,
            );
            event(&mut stdout, r#"{"type":"agent_settled","aborted":false}"#);
        }
        "malformed" => {
            let line = lines.next().expect("command").expect("line");
            writeln!(stdout, "not JSON").unwrap();
            stdout.flush().unwrap();
            response(&mut stdout, &line, r#"{"ok":true}"#);
        }
        "unknown" => {
            let line = lines.next().expect("command").expect("line");
            event(&mut stdout, r#"{"type":"future_event","payload":1}"#);
            response(&mut stdout, &line, r#"{"ok":true}"#);
        }
        "stderr" => {
            let line = lines.next().expect("command").expect("line");
            writeln!(std::io::stderr(), "child diagnostic").unwrap();
            response(&mut stdout, &line, r#"{"ok":true}"#);
        }
        "fail" => {
            let line = lines.next().expect("command").expect("line");
            let id = field(&line, "id");
            let command = field(&line, "type");
            writeln!(
                stdout,
                r#"{{"type":"response","id":"{id}","command":"{command}","success":false,"error":"rejected"}}"#
            )
            .unwrap();
            stdout.flush().unwrap();
        }
        "second-turn-eof" => {
            let first = lines.next().expect("first command").expect("first line");
            response(&mut stdout, &first, r#"{"accepted":true}"#);
            event(&mut stdout, r#"{"type":"agent_settled","aborted":false}"#);
            let _second = lines.next().expect("second command").expect("second line");
            for line in lines {
                line.expect("stdin line");
            }
            std::process::exit(7);
        }
        "eof" => {
            let _line = lines.next().expect("command").expect("line");
            event(&mut stdout, r#"{"type":"agent_start"}"#);
            for line in lines {
                line.expect("stdin line");
            }
            std::process::exit(7);
        }
        _ => std::process::exit(64),
    }
}
"##;

static WORKDIR: LazyLock<tempfile::TempDir> = LazyLock::new(|| {
    let dir = tempfile::tempdir().expect("temporary fake child directory");
    let source = dir.path().join("fake_pi.rs");
    std::fs::write(&source, FAKE_SOURCE).expect("write fake child source");
    let executable = fake_executable_path(dir.path());
    let status = std::process::Command::new("rustc")
        .args(["--edition", "2021", "-O"])
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .status()
        .expect("compile fake child");
    assert!(status.success(), "fake child compilation failed: {status}");
    dir
});

fn fake_executable_path(directory: &std::path::Path) -> PathBuf {
    if cfg!(windows) {
        directory.join("fake_pi.exe")
    } else {
        directory.join("fake_pi")
    }
}

fn fake_executable() -> PathBuf {
    fake_executable_path(WORKDIR.path())
}

fn with_runtime<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future)
}

async fn spawn_fake(mode: &str) -> orchestrator_agents::pi_rpc_adapter::PiRpcHandle {
    PiRpcAdapter::new()
        .spawn_rpc(AgentLaunchRequest {
            program: fake_executable(),
            args: vec![mode.to_string()],
            working_dir: WORKDIR.path().to_path_buf(),
        })
        .await
        .expect("spawn fake Pi RPC child")
}

#[test]
fn matches_response_to_request_id() {
    with_runtime(async {
        let mut handle = spawn_fake("echo").await;
        let id = handle
            .send_command(serde_json::json!({"id":"req-1","type":"get_state"}))
            .await
            .expect("send command");
        let response = handle
            .read_response(&id)
            .await
            .expect("read correlated response");

        assert_eq!(response.id, "req-1");
        assert_eq!(response.command, "get_state");
        assert!(response.success);
        assert_eq!(response.data.expect("response data")["echo"], "req-1");
        handle.cancel().await.expect("cancel fake child");
    });
}
#[test]
fn correlates_out_of_order_responses() {
    with_runtime(async {
        let mut handle = spawn_fake("reverse").await;
        let first = handle
            .send_command(serde_json::json!({"id":"first","type":"get_state"}))
            .await
            .expect("send first command");
        let second = handle
            .send_command(serde_json::json!({"id":"second","type":"get_state"}))
            .await
            .expect("send second command");

        let first_response = handle
            .read_response(&first)
            .await
            .expect("read first response");
        let second_response = handle
            .read_response(&second)
            .await
            .expect("read buffered second response");
        assert_eq!(first_response.data.expect("first data")["echo"], "first");
        assert_eq!(second_response.data.expect("second data")["echo"], "second");
        handle.cancel().await.expect("cancel fake child");
    });
}

#[test]
fn buffers_events_before_between_and_after_responses() {
    with_runtime(async {
        let mut handle = spawn_fake("interleaved").await;
        let first = handle
            .send_command(serde_json::json!({"id":"first","type":"get_state"}))
            .await
            .expect("send first command");
        let second = handle
            .send_command(serde_json::json!({"id":"second","type":"get_state"}))
            .await
            .expect("send second command");

        let first_response = handle
            .read_response(&first)
            .await
            .expect("read first response");
        let second_response = handle
            .read_response(&second)
            .await
            .expect("read buffered second response");
        assert_eq!(first_response.data.expect("first data")["marker"], "first");
        assert_eq!(
            second_response.data.expect("second data")["marker"],
            "second"
        );

        for expected in ["before", "between", "after"] {
            assert_eq!(
                handle.next_event().await.expect("next event"),
                Some(AgentEvent::MessageDelta {
                    text: expected.to_owned()
                })
            );
        }
        assert_eq!(
            handle.next_event().await.expect("settled event"),
            Some(AgentEvent::TurnFinished {
                outcome: TurnOutcome::Success
            })
        );
        assert_eq!(
            handle.wait().await.expect("wait for child"),
            AgentExit::Completed {
                code: Some(0),
                last_message: None
            }
        );
    });
}

#[test]
fn ignores_malformed_json_lines() {
    with_runtime(async {
        let mut handle = spawn_fake("malformed").await;
        let id = handle
            .send_command(serde_json::json!({"type":"get_state"}))
            .await
            .expect("send command");
        assert!(
            handle
                .read_response(&id)
                .await
                .expect("response after malformed line")
                .success
        );
        handle.cancel().await.expect("cancel fake child");
    });
}

#[test]
fn ignores_unknown_event_types() {
    with_runtime(async {
        let mut handle = spawn_fake("unknown").await;
        let id = handle
            .send_command(serde_json::json!({"type":"get_state"}))
            .await
            .expect("send command");
        assert!(
            handle
                .read_response(&id)
                .await
                .expect("response after unknown event")
                .success
        );
        handle.cancel().await.expect("cancel fake child");
    });
}

#[test]
fn keeps_stderr_separate_from_rpc_records() {
    with_runtime(async {
        let mut handle = spawn_fake("stderr").await;
        let id = handle
            .send_command(serde_json::json!({"type":"get_state"}))
            .await
            .expect("send command");
        assert!(
            handle
                .read_response(&id)
                .await
                .expect("response despite stderr diagnostic")
                .success
        );
        handle.cancel().await.expect("cancel fake child");
    });
}

#[test]
fn reports_mid_turn_eof_as_transport_exit() {
    with_runtime(async {
        let mut handle = spawn_fake("eof").await;
        handle
            .send_command(serde_json::json!({"type":"prompt","message":"test"}))
            .await
            .expect("send prompt");
        assert_eq!(
            handle.wait().await.expect("wait for mid-turn exit"),
            AgentExit::Failed {
                code: Some(7),
                reason: Some("Pi RPC process exited before agent_settled".to_owned())
            }
        );
    });
}
#[test]
fn does_not_reuse_previous_completion_after_new_prompt() {
    with_runtime(async {
        let mut handle = spawn_fake("second-turn-eof").await;
        let first_id = handle
            .send_command(serde_json::json!({"type":"prompt","message":"first"}))
            .await
            .expect("send first prompt");
        assert!(
            handle
                .read_response(&first_id)
                .await
                .expect("first prompt response")
                .success
        );
        assert_eq!(
            handle.next_event().await.expect("first turn event"),
            Some(AgentEvent::TurnFinished {
                outcome: TurnOutcome::Success
            })
        );

        handle
            .send_command(serde_json::json!({"type":"prompt","message":"second"}))
            .await
            .expect("send second prompt");
        assert_eq!(
            handle.wait().await.expect("wait for second-turn exit"),
            AgentExit::Failed {
                code: Some(7),
                reason: Some("Pi RPC process exited before agent_settled".to_owned())
            }
        );
    });
}

#[test]
fn preserves_failing_command_response() {
    with_runtime(async {
        let mut handle = spawn_fake("fail").await;
        let id = handle
            .send_command(serde_json::json!({"type":"get_state"}))
            .await
            .expect("send command");
        let response = handle
            .read_response(&id)
            .await
            .expect("read failed command response");
        assert!(!response.success);
        assert_eq!(response.error.as_deref(), Some("rejected"));
        handle.cancel().await.expect("cancel fake child");
    });
}

#[test]
fn detects_only_the_rpc_command_it_verified() {
    with_runtime(async {
        let adapter = PiRpcAdapter::new().with_program(fake_executable());
        assert_eq!(adapter.kind(), AgentKind::Pi);
        let info = adapter
            .detect_protocol()
            .await
            .expect("detect RPC protocol");
        assert_eq!(info.cli_version, "1.5.0");
        assert_eq!(info.verified_commands, ["get_state"]);

        let capabilities = adapter.capabilities();
        assert_eq!(
            capabilities,
            AgentCapabilities {
                streaming: true,
                interactive_input: true,
                structured_output: true,
                cancellation: true,
                session_resumption: false,
                approvals: false,
                model_selection: false,
            }
        );
        assert_eq!(
            adapter.detect().await.expect("detect capabilities"),
            capabilities
        );
    });
}
