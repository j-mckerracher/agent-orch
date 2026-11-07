//! Behavior tests for the Codex adapter and the supervisor it rides on,
//! using a fake child executable (compiled with rustc at test time) in
//! place of the real Codex CLI.
//!
//! Each test runs its whole async body inside one runtime: the
//! supervisor's reader and reap tasks live on that runtime and are
//! aborted if it drops, so the body must stay inside a single
//! `block_on`.
//!
//! Covered: JSONL event parsing through a live process, stderr handling,
//! successful and nonzero exit, garbage-stdout protocol errors,
//! cancellation, and process-tree cleanup of descendants.

use orchestrator_agents::{
    AgentAdapter, AgentEvent, AgentExit, AgentHandle, AgentKind, AgentLaunchRequest,
    TurnOutcome,
};
use orchestrator_runtime::{is_process_running, ChildSupervisor, LaunchRequest, SupervisionEvent};
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;

const SID: &str = "01a1b721-1f91-7393-ae1d-6aace8e4646a";

const FAKE_SRC: &str = r##"
use std::io::Write;
use std::process::Command;
use std::time::Duration;

const SID: &str = "01a1b721-1f91-7393-ae1d-6aace8e4646a";

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    let out = std::io::stdout();
    let mut out = out.lock();
    match mode.as_str() {
        "happy" => {
            writeln!(out, r#"{{"type":"thread.started","thread_id":"{SID}"}}"#).unwrap();
            writeln!(out, r#"{{"type":"turn.started"}}"#).unwrap();
            writeln!(out, r#"{{"type":"item.completed","item":{{"id":"item_0","type":"agent_message","text":"PONG"}}}}"#).unwrap();
            writeln!(out, r#"{{"type":"turn.completed","usage":{{"input_tokens":1}}}}"#).unwrap();
            out.flush().unwrap();
        }
        "stderr" => {
            let mut err = std::io::stderr().lock();
            writeln!(err, "diagnostic line on stderr").unwrap();
            err.flush().unwrap();
            writeln!(out, r#"{{"type":"thread.started","thread_id":"{SID}"}}"#).unwrap();
            writeln!(out, r#"{{"type":"turn.started"}}"#).unwrap();
            writeln!(out, r#"{{"type":"item.completed","item":{{"id":"item_0","type":"agent_message","text":"PONG"}}}}"#).unwrap();
            writeln!(out, r#"{{"type":"turn.completed","usage":{{"input_tokens":1}}}}"#).unwrap();
            out.flush().unwrap();
        }
        "exit7" => {
            std::process::exit(7);
        }
        "garbage" => {
            // Not JSON at all: a protocol error, and the process still
            // exits 0, which is NOT success.
            writeln!(out, "this is not json").unwrap();
            out.flush().unwrap();
        }
        "hang" => {
            std::thread::sleep(Duration::from_secs(300));
        }
        "hang-spawn" => {
            // Spawn a long-lived descendant, announce its PID on stderr,
            // then block. Cancelling this process must kill the
            // descendant too.
            let exe = std::env::current_exe().unwrap();
            let child = Command::new(exe).arg("sleep").spawn().unwrap();
            let mut err = std::io::stderr().lock();
            writeln!(err, "descendant={}", child.id()).unwrap();
            err.flush().unwrap();
            std::thread::sleep(Duration::from_secs(300));
        }
        "sleep" => {
            std::thread::sleep(Duration::from_secs(300));
        }
        _ => {
            std::process::exit(64);
        }
    }
}
"##;

/// One leaked tempdir for the whole test binary (fake binary + workdir).
static WORKDIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let dir = tempfile::tempdir().expect("tempdir");
    // Leaked deliberately: the fake binary must stay alive for the whole
    // test process.
    let dir = dir.keep();
    let src = dir.join("fake.rs");
    std::fs::write(&src, FAKE_SRC).expect("write fake source");
    let exe = if cfg!(windows) {
        dir.join("fake.exe")
    } else {
        dir.join("fake")
    };
    let status = std::process::Command::new("rustc")
        .args(["--edition", "2021", "-O"])
        .arg(&src)
        .args(["-o"])
        .arg(&exe)
        .status()
        .expect("run rustc");
    assert!(status.success(), "rustc failed: {status}");
    dir
});

fn fake_exe() -> PathBuf {
    if cfg!(windows) {
        WORKDIR.join("fake.exe")
    } else {
        WORKDIR.join("fake")
    }
}

/// Run the whole test body on one current-thread runtime. The body must
/// contain every supervised-async call in this test; the supervisor's
/// reader and reap tasks live on this runtime and die with it.
fn with_runtime<F>(f: F) -> F::Output
where
    F: std::future::Future,
{
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(f)
}

async fn spawn_adapter(mode: &str) -> orchestrator_agents::Result<Box<dyn AgentHandle>> {
    let adapter = orchestrator_agents::CodexAdapter::new().with_program(fake_exe());
    adapter
        .spawn(AgentLaunchRequest {
            program: fake_exe(),
            args: vec![mode.to_string()],
            working_dir: WORKDIR.clone(),
        })
        .await
}

#[test]
fn happy_run_streams_events_and_completes() {
    with_runtime(async {
        let mut h = spawn_adapter("happy").await.expect("spawn");
        assert!(matches!(
            h.next_event().await,
            Ok(Some(AgentEvent::SessionStarted { session_id }))
                if session_id.to_string() == SID
        ));
        assert!(matches!(
            h.next_event().await,
            Ok(Some(AgentEvent::MessageDelta { text })) if text == "PONG"
        ));
        assert!(matches!(
            h.next_event().await,
            Ok(Some(AgentEvent::TurnFinished { outcome: TurnOutcome::Success }))
        ));
        assert!(matches!(
            h.next_event().await,
            Ok(Some(AgentEvent::SessionExited { exit_code: Some(0) }))
        ));
        assert!(matches!(h.next_event().await, Ok(None)));
        let exit = h.wait().await.expect("wait");
        match exit {
            AgentExit::Completed {
                code: Some(0),
                last_message: Some(m),
            } => assert_eq!(m, "PONG"),
            other => panic!("expected Completed(last_message), got {other:?}"),
        }
    });
}

#[test]
fn stderr_is_diagnostics_and_nonzero_exit_fails() {
    // Stderr is a diagnostic line, not protocol: the happy sequence still
    // completes and the exit is 0.
    with_runtime(async {
        let mut h = spawn_adapter("stderr").await.expect("spawn");
        let mut events = Vec::new();
        while let Ok(Some(ev)) = h.next_event().await {
            events.push(ev);
        }
        assert!(
            events.iter().any(|e| {
                matches!(e, AgentEvent::MessageDelta { text } if text == "PONG")
            }),
            "stdout events must be delivered: {events:?}"
        );
        let exit = h.wait().await.expect("wait");
        assert!(
            matches!(exit, AgentExit::Completed { code: Some(0), .. }),
            "exit: {exit:?}"
        );
    });

    // Nonzero exit without a completed turn is a failure, not success.
    with_runtime(async {
        let mut h = spawn_adapter("exit7").await.expect("spawn");
        let exit = h.wait().await.expect("wait");
        assert!(
            matches!(exit, AgentExit::Failed { code: Some(7), reason: None }),
            "exit: {exit:?}"
        );
    });
}

#[test]
fn garbage_stdout_is_a_protocol_error_not_success() {
    with_runtime(async {
        let mut h = spawn_adapter("garbage").await.expect("spawn");
        let saw_protocol_error = loop {
            match h.next_event().await {
                Ok(Some(AgentEvent::ProtocolError { .. })) => break true,
                Ok(Some(_)) | Ok(None) => break false,
                Err(e) => panic!("next_event: {e}"),
            }
        };
        assert!(saw_protocol_error, "non-JSON stdout must surface as ProtocolError");
        // The process still exited 0 — the outcome must still be a failure.
        let exit = h.wait().await.expect("wait");
        match exit {
            AgentExit::Failed {
                code: Some(0),
                reason: Some(r),
            } => assert!(r.contains("malformed JSON"), "reason: {r}"),
            other => panic!("expected Failed(code=0, protocol reason), got {other:?}"),
        }
    });
}

#[test]
fn cancel_reports_cancelled_exit() {
    with_runtime(async {
        let mut h = spawn_adapter("hang").await.expect("spawn");
        let pid = h.pid();
        h.cancel().await.expect("cancel");
        // Unix: SIGTERM leaves a grace window in which the child may still
        // be alive. Windows: job-object kill is synchronous, so it may
        // already be dead.
        #[cfg(unix)]
        assert!(is_process_running(pid), "child alive before reap");
        let exit = h.wait().await.expect("wait");
        assert!(
            matches!(exit, AgentExit::Cancelled { .. }),
            "expected Cancelled, got {exit:?}"
        );
        // Unix: SIGKILL exits carry no code. Windows: a terminated process
        // reports the code it last set (0 if never set), so any code is
        // possible; the cancelled flag is the contract.
        #[cfg(windows)]
        if let AgentExit::Cancelled { code, .. } = exit {
            let _ = code;
        }
        assert!(!is_process_running(pid), "child still running after wait");
    });
}

#[test]
fn supervisor_cancel_kills_descendants() {
    // The child spawns a long-lived descendant (its PID on stderr), then
    // blocks. Cancelling must kill the whole tree, not just the child.
    with_runtime(async {
        let mut sup = ChildSupervisor::launch(&LaunchRequest {
            program: fake_exe(),
            args: vec!["hang-spawn".into()],
            working_dir: WORKDIR.clone(),
        })
        .expect("launch");
        let root = sup.pid();
        // Read stderr until the descendant PID arrives, then cancel.
        let desc = loop {
            let ev = sup.next_event().await.expect("event stream");
            if let SupervisionEvent::StderrLine(line) = ev {
                if let Some(pid) = line
                    .strip_prefix("descendant=")
                    .and_then(|s| s.parse::<u32>().ok())
                {
                    break pid;
                }
            }
        };
        assert!(is_process_running(root), "child should be running");
        assert!(is_process_running(desc), "descendant should be running");

        sup.cancel().await;

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if !is_process_running(desc) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "descendant {desc} survived cancel"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let exit = sup.wait().await.expect("exit event");
        assert!(
            matches!(exit, SupervisionEvent::Exited { cancelled: true, .. }),
            "exit: {exit:?}"
        );
    });
}

#[test]
fn detect_reports_capabilities_for_found_binary() {
    let adapter = orchestrator_agents::CodexAdapter::new().with_program(fake_exe());
    let caps = with_runtime(adapter.detect()).expect("detect");
    assert_eq!(
        orchestrator_agents::CodexAdapter::new().kind(),
        AgentKind::Codex
    );
    assert!(caps.streaming && caps.structured_output && caps.cancellation);
    assert!(
        !caps.interactive_input
            && !caps.session_resumption
            && !caps.approvals
            && !caps.model_selection
    );
}

#[test]
fn detect_reports_not_found_for_missing_binary() {
    let missing = PathBuf::from(if cfg!(windows) {
        "definitely-missing-codex-x.exe"
    } else {
        "definitely-missing-codex-x"
    });
    let adapter = orchestrator_agents::CodexAdapter::new().with_program(missing.clone());
    let err = with_runtime(adapter.detect()).expect_err("must be NotFound");
    assert!(
        matches!(err, orchestrator_agents::AgentError::NotFound(p) if p == missing.to_string_lossy())
    );
}
