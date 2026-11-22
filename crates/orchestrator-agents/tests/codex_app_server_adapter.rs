//! Tests for the Codex app-server adapter (DPES-1183). Each acceptance criterion
//! in the spec maps to one test.
//!
//! The fake child is a Rust program compiled with `rustc`, as in
//! `pi_rpc_adapter.rs`. It answers `initialize`, `thread/start`, `thread/resume`
//! and `turn/start` from the fixture files, then emits the frame files named in
//! the run's `scenario.txt`. It logs every stdin line to `stdin.log`. Tests read
//! that log only after `wait()`, which closes stdin and waits for the child to
//! exit, so the log is complete. Fixture provenance is in
//! `tests/fixtures/codex_app_server/README.md`.

use orchestrator_agents::{
    AgentError, AgentEvent, AgentExit, AgentHandle, AgentLaunchRequest, ApprovalDecision,
    CodexAppServerAdapter, CodexAppServerHandle, InteractiveAgentHandle, TurnOutcome,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use uuid::Uuid;

const MODEL: &str = "gpt-6-luna";
const THREAD: &str = "01a1ffc6-ddd8-78c3-bb7a-f654fa3eaf1c";
const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/codex_app_server"
);

const FAKE_SOURCE: &str = r##"
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

fn str_field(line: &str, name: &str) -> Option<String> {
    let marker = format!("\"{name}\":\"");
    let start = line.find(&marker)? + marker.len();
    let end = line[start..].find('"')? + start;
    Some(line[start..end].to_owned())
}

// The adapter writes `id` as the first key of every request, and request ids are numbers.
fn id_field(line: &str) -> String {
    let start = line.find("\"id\":").expect("request id") + "\"id\":".len();
    let rest = &line[start..];
    let end = rest.find([',', '}']).expect("request id end");
    rest[..end].to_owned()
}

fn frame(path: &Path) -> String {
    std::fs::read_to_string(path)
        .expect("frame file")
        .replace(&['\n', '\r'][..], "")
}

fn reply(out: &mut impl Write, request: &str, body: &str) {
    writeln!(out, "{{\"id\":{},\"result\":{body}}}", id_field(request)).expect("reply");
    out.flush().expect("flush reply");
}

fn main() {
    let dir = std::env::current_dir().expect("working dir");
    let scenario = std::fs::read_to_string(dir.join("scenario.txt")).unwrap_or_default();
    let mut bodies = PathBuf::new();
    let mut emits = Vec::new();
    let mut silent = false;
    for line in scenario.lines() {
        if let Some(path) = line.strip_prefix("bodies=") {
            bodies = PathBuf::from(path);
        } else if let Some(path) = line.strip_prefix("emit=") {
            emits.push(PathBuf::from(path));
        } else if line == "silent" {
            silent = true;
        }
    }
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("stdin.log"))
        .expect("stdin log");
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in std::io::stdin().lock().lines() {
        let line = line.expect("stdin line");
        writeln!(log, "{line}").expect("log line");
        log.flush().expect("flush log");
        if silent {
            std::process::exit(0);
        }
        let Some(method) = str_field(&line, "method") else {
            continue;
        };
        match method.as_str() {
            "initialize" => reply(&mut out, &line, &frame(&bodies.join("initialize_result.json"))),
            "thread/start" | "thread/resume" => {
                reply(&mut out, &line, &frame(&bodies.join("thread_start_result.json")))
            }
            "turn/start" => {
                reply(&mut out, &line, r#"{"turn":{"id":"turn-1","status":"inProgress"}}"#);
                for path in &emits {
                    writeln!(out, "{}", frame(path)).expect("emit");
                }
                out.flush().expect("flush emit");
            }
            _ => {}
        }
    }
}
"##;

static FAKE_DIR: LazyLock<tempfile::TempDir> = LazyLock::new(|| {
    let dir = tempfile::tempdir().expect("temporary fake child directory");
    let source = dir.path().join("fake_codex_app_server.rs");
    std::fs::write(&source, FAKE_SOURCE).expect("write fake child source");
    let status = std::process::Command::new("rustc")
        .args(["--edition", "2021", "-O"])
        .arg(&source)
        .arg("-o")
        .arg(fake_executable_path(dir.path()))
        .status()
        .expect("compile fake child");
    assert!(status.success(), "fake child compilation failed: {status}");
    dir
});

fn fake_executable_path(directory: &Path) -> PathBuf {
    let name = if cfg!(windows) {
        "fake_codex_app_server.exe"
    } else {
        "fake_codex_app_server"
    };
    directory.join(name)
}

fn fake_executable() -> PathBuf {
    fake_executable_path(FAKE_DIR.path())
}

fn fixture(name: &str) -> PathBuf {
    Path::new(FIXTURES).join(name)
}

fn session_started() -> AgentEvent {
    AgentEvent::SessionStarted {
        session_id: Uuid::parse_str(THREAD).expect("thread uuid"),
    }
}

fn plan_frame(turn_id: &str, text: &str) -> Value {
    json!({
        "method": "item/completed",
        "params": {
            "item": {"id": "item-plan-x", "text": text, "type": "plan"},
            "threadId": THREAD,
            "turnId": turn_id,
        },
    })
}

/// One test run: a working directory holding the scenario and the stdin log.
struct Run {
    dir: tempfile::TempDir,
}

impl Run {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("run directory"),
        }
    }

    /// The fake emits these frame files after its `turn/start` reply.
    fn script(&self, emits: &[PathBuf]) {
        let mut scenario = format!("bodies={FIXTURES}\n");
        for path in emits {
            scenario.push_str(&format!("emit={}\n", path.display()));
        }
        std::fs::write(self.dir.path().join("scenario.txt"), scenario).expect("scenario file");
    }

    /// Write a frame file into the run directory and return its path.
    fn frame(&self, name: &str, frame: Value) -> PathBuf {
        let path = self.dir.path().join(name);
        std::fs::write(&path, frame.to_string()).expect("frame file");
        path
    }

    fn cwd(&self) -> String {
        self.dir.path().to_string_lossy().into_owned()
    }

    fn request(&self) -> AgentLaunchRequest {
        AgentLaunchRequest {
            program: fake_executable(),
            args: Vec::new(),
            working_dir: self.dir.path().to_owned(),
            git_guard: false,
        }
    }

    async fn spawn(&self, resume: Option<&str>) -> CodexAppServerHandle {
        CodexAppServerAdapter::new()
            .spawn_interactive(self.request(), MODEL, resume.map(str::to_owned))
            .await
            .expect("spawn app-server")
    }

    /// Stdin lines the fake received, in order. Read only after `wait`.
    fn stdin_lines(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("stdin.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

async fn next(handle: &mut CodexAppServerHandle) -> AgentEvent {
    handle
        .next_event()
        .await
        .expect("next event")
        .expect("event stream ended early")
}

/// Spawn a session, consume `SessionStarted`, and send one turn.
async fn started_turn(run: &Run) -> CodexAppServerHandle {
    let mut handle = run.spawn(None).await;
    assert_eq!(next(&mut handle).await, session_started());
    handle.send_turn("do the task").await.expect("send turn");
    handle
}

/// The first stdin frame whose method is `method`.
fn request_frame(lines: &[String], method: &str) -> Value {
    lines
        .iter()
        .map(|line| serde_json::from_str::<Value>(line).expect("stdin line is JSON"))
        .find(|frame| frame["method"] == method)
        .unwrap_or_else(|| panic!("no {method} request in {lines:?}"))
}

#[test]
fn capabilities_match_the_spec() {
    let capabilities = CodexAppServerAdapter::new().capabilities();
    assert!(capabilities.streaming);
    assert!(capabilities.interactive_input);
    assert!(capabilities.structured_output);
    assert!(capabilities.cancellation);
    assert!(capabilities.session_resumption);
    assert!(capabilities.approvals);
    assert!(capabilities.model_selection);
}

#[tokio::test]
async fn ac1_handshake_requests_experimental_api_before_initialized() {
    let run = Run::new();
    run.script(&[]);
    let mut handle = run.spawn(None).await;
    assert_eq!(next(&mut handle).await, session_started());
    handle.wait().await.expect("wait");

    let lines = run.stdin_lines();
    assert_eq!(
        lines[0],
        r#"{"id":1,"method":"initialize","params":{"capabilities":{"experimentalApi":true},"clientInfo":{"name":"orchestrator-agents","version":"0.0.0"}}}"#
    );
    assert_eq!(lines[1], r#"{"method":"initialized"}"#);
    assert_eq!(
        request_frame(&lines, "thread/start")["params"],
        json!({
            "approvalPolicy": "on-request",
            "cwd": run.cwd(),
            "model": MODEL,
            "sandbox": "read-only",
        })
    );
}

#[tokio::test]
async fn ac2_resume_sends_thread_resume_and_no_thread_start() {
    let run = Run::new();
    run.script(&[]);
    let mut handle = run.spawn(Some(THREAD)).await;
    assert_eq!(next(&mut handle).await, session_started());
    handle.wait().await.expect("wait");

    let lines = run.stdin_lines();
    assert_eq!(
        request_frame(&lines, "thread/resume")["params"],
        json!({"cwd": run.cwd(), "threadId": THREAD})
    );
    assert!(lines.iter().all(|line| !line.contains("\"thread/start\"")));
}

#[tokio::test]
async fn ac3_approvals_answer_accept_or_decline_on_the_request_id() {
    let run = Run::new();
    run.script(&[
        fixture("approval_command_request.json"),
        fixture("approval_filechange_request.json"),
    ]);
    let mut handle = started_turn(&run).await;

    let AgentEvent::ApprovalRequested {
        approval_id: command,
        description,
    } = next(&mut handle).await
    else {
        panic!("expected a command approval");
    };
    assert_eq!(
        description,
        "cargo test -p orchestrator-agents (run the test suite)"
    );
    handle
        .respond_approval(command, ApprovalDecision::Approve)
        .await
        .expect("approve command");

    let AgentEvent::ApprovalRequested {
        approval_id: change,
        ..
    } = next(&mut handle).await
    else {
        panic!("expected a file-change approval");
    };
    handle
        .respond_approval(
            change,
            ApprovalDecision::Deny {
                reason: "outside scope".to_owned(),
            },
        )
        .await
        .expect("deny file change");

    handle.wait().await.expect("wait");
    let lines = run.stdin_lines();
    assert!(lines.contains(&r#"{"id":7,"result":{"decision":"accept"}}"#.to_owned()));
    assert!(lines.contains(&r#"{"id":8,"result":{"decision":"decline"}}"#.to_owned()));
}

#[tokio::test]
async fn ac4_turn_completed_maps_status_and_ignores_other_turns() {
    // A failed record for another turn produces no event. The completed record for the current turn is Success.
    let run = Run::new();
    let other_turn = run.frame(
        "other_turn_failed.json",
        json!({
            "method": "turn/completed",
            "params": {
                "threadId": THREAD,
                "turn": {"id": "turn-2", "status": "failed", "error": null, "items": []},
            },
        }),
    );
    run.script(&[other_turn, fixture("turn_completed_ok.json")]);
    let mut handle = started_turn(&run).await;
    assert_eq!(
        next(&mut handle).await,
        AgentEvent::TurnFinished {
            outcome: TurnOutcome::Success
        }
    );
    handle.wait().await.expect("wait");

    // Any status other than completed is Failure.
    let run = Run::new();
    let interrupted = run.frame(
        "interrupted.json",
        json!({
            "method": "turn/completed",
            "params": {
                "threadId": THREAD,
                "turn": {"id": "turn-1", "status": "interrupted", "error": null, "items": []},
            },
        }),
    );
    run.script(&[interrupted]);
    let mut handle = started_turn(&run).await;
    assert_eq!(
        next(&mut handle).await,
        AgentEvent::TurnFinished {
            outcome: TurnOutcome::Failure
        }
    );
    handle.wait().await.expect("wait");
}

#[tokio::test]
async fn ac5_usage_limit_error_is_a_protocol_error_then_a_failed_turn() {
    let run = Run::new();
    run.script(&[
        fixture("error_usage_limit.json"),
        fixture("turn_completed_failed.json"),
    ]);
    let mut handle = started_turn(&run).await;
    assert_eq!(
        next(&mut handle).await,
        AgentEvent::ProtocolError {
            message: "You have reached your usage limit.".to_owned()
        }
    );
    assert_eq!(
        next(&mut handle).await,
        AgentEvent::TurnFinished {
            outcome: TurnOutcome::Failure
        }
    );
    assert!(matches!(
        handle.wait().await.expect("wait"),
        AgentExit::Failed { .. }
    ));
}

#[tokio::test]
async fn ac6_plan_is_proposed_only_for_the_current_turn_and_must_not_be_empty() {
    let run = Run::new();
    let other_turn = run.frame(
        "plan_other_turn.json",
        plan_frame("turn-2", "A plan for another turn."),
    );
    let empty = run.frame("plan_empty.json", plan_frame("turn-1", ""));
    run.script(&[other_turn, empty, fixture("item_completed_plan.json")]);
    let mut handle = started_turn(&run).await;

    // The other-turn plan produces nothing, so the next event is the empty-plan error.
    assert_eq!(
        next(&mut handle).await,
        AgentEvent::ProtocolError {
            message: "app-server sent an empty plan".to_owned()
        }
    );
    match next(&mut handle).await {
        AgentEvent::PlanProposed { plan, .. } => {
            assert_eq!(plan, "Plan: add the adapter, then the tests.")
        }
        other => panic!("expected a proposed plan, got {other:?}"),
    }
    handle.wait().await.expect("wait");
}

#[tokio::test]
async fn plan_approval_is_answerable_once_and_writes_no_frame() {
    // Q16 default: Codex has no request behind a plan, so the answer writes nothing.
    let run = Run::new();
    run.script(&[fixture("item_completed_plan.json")]);
    let mut handle = started_turn(&run).await;
    let AgentEvent::PlanProposed { approval_id, .. } = next(&mut handle).await else {
        panic!("expected a proposed plan");
    };
    handle
        .respond_approval(approval_id, ApprovalDecision::Approve)
        .await
        .expect("first answer");
    let second = handle
        .respond_approval(approval_id, ApprovalDecision::Approve)
        .await;
    assert!(matches!(second, Err(AgentError::Protocol(_))));
    handle.wait().await.expect("wait");
    assert!(run
        .stdin_lines()
        .iter()
        .all(|line| !line.contains("\"result\"")));
}

#[tokio::test]
async fn ac7_send_turn_while_a_turn_runs_is_refused_without_writing() {
    let run = Run::new();
    run.script(&[]);
    let mut handle = run.spawn(None).await;
    assert_eq!(next(&mut handle).await, session_started());
    // Q13 default: a non-plan turn sends collaborationMode mode "default".
    handle.set_plan_mode(false);
    handle.send_turn("first").await.expect("first turn");
    let second = handle.send_turn("second").await;
    assert!(matches!(second, Err(AgentError::Protocol(ref m)) if m == "turn still running"));
    handle.wait().await.expect("wait");

    let turns: Vec<Value> = run
        .stdin_lines()
        .iter()
        .map(|line| serde_json::from_str::<Value>(line).expect("stdin line is JSON"))
        .filter(|frame| frame["method"] == "turn/start")
        .collect();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0]["params"]["collaborationMode"]["mode"], "default");
}

#[tokio::test]
async fn ac8_unknown_approval_id_is_refused_without_writing() {
    let run = Run::new();
    run.script(&[]);
    let mut handle = run.spawn(None).await;
    assert_eq!(next(&mut handle).await, session_started());
    let result = handle
        .respond_approval(Uuid::new_v4(), ApprovalDecision::Approve)
        .await;
    assert!(matches!(result, Err(AgentError::Protocol(_))));
    handle.wait().await.expect("wait");
    assert!(run
        .stdin_lines()
        .iter()
        .all(|line| !line.contains("\"result\"")));
}

#[tokio::test]
async fn ac9_plan_mode_turn_carries_plan_collaboration_mode_and_the_model() {
    let run = Run::new();
    run.script(&[]);
    let mut handle = run.spawn(None).await;
    assert_eq!(next(&mut handle).await, session_started());
    handle.set_plan_mode(true);
    handle.send_turn("plan it").await.expect("send turn");
    handle.wait().await.expect("wait");
    assert_eq!(
        request_frame(&run.stdin_lines(), "turn/start")["params"]["collaborationMode"],
        json!({"mode": "plan", "settings": {"model": MODEL}})
    );
}

#[tokio::test]
async fn handshake_fails_when_the_child_exits_before_answering() {
    let run = Run::new();
    std::fs::write(run.dir.path().join("scenario.txt"), "silent\n").expect("scenario file");
    let result = CodexAppServerAdapter::new()
        .spawn_interactive(run.request(), MODEL, None)
        .await;
    assert!(matches!(result, Err(AgentError::Protocol(_))));
}

/// Needs a local `codex` 0.171.0 on PATH. It runs the handshake and `thread/start`
/// against the real binary and checks the thread id is a UUID. No model turn is sent.
///
/// It does not cover resume. Codex 0.171.0 writes a thread's rollout only after a
/// turn, so `thread/resume` on a turnless thread fails with "no rollout found".
/// Resume needs a live turn, which the workspace spend cap blocks (DPES-1183 Q17).
#[tokio::test]
#[ignore = "needs a local codex 0.171.0 on PATH"]
async fn live_handshake_and_thread_start_return_a_uuid_thread() {
    let dir = tempfile::tempdir().expect("live directory");
    let mut handle = CodexAppServerAdapter::new()
        .spawn_interactive(
            AgentLaunchRequest {
                program: PathBuf::from("codex"),
                args: Vec::new(),
                working_dir: dir.path().to_owned(),
                git_guard: false,
            },
            MODEL,
            None,
        )
        .await
        .expect("start thread");
    assert!(matches!(
        next(&mut handle).await,
        AgentEvent::SessionStarted { .. }
    ));
    handle.cancel().await.expect("cancel");
}
