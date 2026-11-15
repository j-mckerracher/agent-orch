//! Background sign-in checks (DPES-1105), driven through `CopilotSignIn::start`
//! with fake `copilot` programs only. The real binary is never run: every
//! program here answers `--version` and replays a fixed stream, so no test
//! spends a premium request.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use orchestrator_agents::{CopilotAdapter, CopilotSignIn, SignInRecord, SignInState};

/// A completed turn in the verified event layout.
const COMPLETED: &str = concat!(
    "{\"type\":\"session.start\",\"data\":{\"sessionId\":\"00000000-0000-4000-8000-000000000001\",\"copilotVersion\":\"1.0.109\",\"selectedModel\":\"claude-haiku-4.5\"}}\n",
    "{\"type\":\"assistant.message\",\"data\":{\"content\":\"OK\",\"model\":\"claude-haiku-4.5\",\"providerId\":\"copilot\",\"toolRequests\":[]}}\n",
    "{\"type\":\"result\",\"exitCode\":0,\"usage\":{\"premiumRequests\":0.33}}\n",
);

/// A run that the CLI reports as not signed in.
const NOT_SIGNED_IN: &str = "{\"type\":\"session.error\",\"data\":{\"errorType\":\"auth\",\"message\":\"Not logged in. Please run copilot login.\"}}\n{\"type\":\"result\",\"exitCode\":1}\n";
const NOT_SIGNED_IN_REASON: &str =
    "Not signed in to GitHub Copilot. Run `copilot login`, then retry.";

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

/// A fake `copilot`. `--version` prints the pinned version; any other call runs
/// `body`, which is shell.
fn fake_copilot(dir: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("copilot");
    let text = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  printf '%s\\n' 'GitHub Copilot CLI 1.0.109.'\n  exit 0\nfi\n{body}\n"
    );
    std::fs::write(&path, text).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn futures_block<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

/// Starts a background check of `program`, as the daemon does.
fn start(cache: &Arc<CopilotSignIn>, program: &Path, dir: &Path, force: bool) -> SignInRecord {
    let adapter = CopilotAdapter::new().with_program(program);
    let parent = dir.to_path_buf();
    cache.start(force, move || {
        futures_block(adapter.sign_in_check(&parent, Duration::from_secs(30)))
    })
}

/// Waits until the background check stores its outcome.
fn settle(cache: &CopilotSignIn) -> SignInRecord {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let record = cache.snapshot();
        if record.state != SignInState::Checking {
            return record;
        }
        assert!(
            Instant::now() < deadline,
            "the background check never settled"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// How many runs (not `--version` probes) the fake program has started.
fn runs_started(count: &Path) -> usize {
    std::fs::read_to_string(count)
        .map(|text| text.lines().count())
        .unwrap_or(0)
}

#[test]
fn a_slow_check_returns_at_once_and_reads_checking_until_it_ends() {
    let dir = tempfile::tempdir().unwrap();
    let stream = write(dir.path(), "stream.jsonl", COMPLETED);
    let program = fake_copilot(
        dir.path(),
        &format!("sleep 2\ncat '{}'\nexit 0", stream.display()),
    );
    let cache = Arc::new(CopilotSignIn::default());

    let started_at = Instant::now();
    let first = start(&cache, &program, dir.path(), false);
    assert!(
        started_at.elapsed() < Duration::from_secs(1),
        "start waited for the two-second run"
    );
    assert_eq!(first.state, SignInState::Checking);
    assert_eq!(cache.snapshot().state, SignInState::Checking);

    let settled = settle(&cache);
    assert_eq!(settled.state, SignInState::Ready);
    assert!(settled.checked_at_ms.is_some());
    assert!(
        started_at.elapsed() >= Duration::from_secs(2),
        "the record settled before the run ended"
    );
}

#[test]
fn a_second_start_while_checking_spawns_no_second_run_even_when_forced() {
    let dir = tempfile::tempdir().unwrap();
    let stream = write(dir.path(), "stream.jsonl", COMPLETED);
    let count = dir.path().join("runs.txt");
    let program = fake_copilot(
        dir.path(),
        &format!(
            "echo run >> '{}'\nsleep 1\ncat '{}'\nexit 0",
            count.display(),
            stream.display()
        ),
    );
    let cache = Arc::new(CopilotSignIn::default());

    let first = start(&cache, &program, dir.path(), false);
    let second = start(&cache, &program, dir.path(), false);
    let forced = start(&cache, &program, dir.path(), true);
    assert_eq!(first.state, SignInState::Checking);
    assert_eq!(second.state, SignInState::Checking);
    assert_eq!(forced.state, SignInState::Checking);

    assert_eq!(settle(&cache).state, SignInState::Ready);
    assert_eq!(runs_started(&count), 1, "exactly one paid run");
}

#[test]
fn a_check_finishes_after_its_caller_lets_go_of_it() {
    let dir = tempfile::tempdir().unwrap();
    let stream = write(dir.path(), "stream.jsonl", COMPLETED);
    let finished = dir.path().join("finished");
    let program = fake_copilot(
        dir.path(),
        &format!(
            "sleep 1\ncat '{}'\ntouch '{}'\nexit 0",
            stream.display(),
            finished.display()
        ),
    );
    let cache = Arc::new(CopilotSignIn::default());
    let record = start(&cache, &program, dir.path(), false);
    assert_eq!(record.state, SignInState::Checking);
    // The caller (the RPC that started the check) drops everything it holds.
    drop(record);

    let deadline = Instant::now() + Duration::from_secs(30);
    while !finished.exists() {
        assert!(
            Instant::now() < deadline,
            "the run was cancelled with its caller"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(settle(&cache).state, SignInState::Ready);
}

#[test]
fn a_settled_answer_is_served_from_memory_and_only_force_runs_again() {
    let dir = tempfile::tempdir().unwrap();
    let not_signed_in = write(dir.path(), "not-signed-in.jsonl", NOT_SIGNED_IN);
    let count = dir.path().join("runs.txt");
    let program = fake_copilot(
        dir.path(),
        &format!(
            "echo run >> '{}'\ncat '{}'\nexit 1",
            count.display(),
            not_signed_in.display()
        ),
    );
    let cache = Arc::new(CopilotSignIn::default());

    start(&cache, &program, dir.path(), false);
    let settled = settle(&cache);
    assert_eq!(settled.state, SignInState::NotSignedIn);
    assert_eq!(settled.reason.as_deref(), Some(NOT_SIGNED_IN_REASON));
    assert!(settled.checked_at_ms.is_some());

    let served = start(&cache, &program, dir.path(), false);
    assert_eq!(served.state, SignInState::NotSignedIn);
    assert_eq!(runs_started(&count), 1, "a settled answer costs no run");

    let forced = start(&cache, &program, dir.path(), true);
    assert_eq!(forced.state, SignInState::Checking);
    assert_eq!(settle(&cache).state, SignInState::NotSignedIn);
    assert_eq!(runs_started(&count), 2);
}

#[test]
fn the_stored_record_keeps_the_usage_the_check_reported() {
    let dir = tempfile::tempdir().unwrap();
    let stream = write(dir.path(), "stream.jsonl", COMPLETED);
    let usage = write(
        dir.path(),
        "usage-source.json",
        r#"{"totalPremiumRequestCost":0.33,"totalNanoAiu":1500000000}"#,
    );
    // The CLI writes its usage file to the path after `--usage-output-file`.
    let program = fake_copilot(
        dir.path(),
        &format!(
            "while [ $# -gt 0 ]; do\n  if [ \"$1\" = \"--usage-output-file\" ]; then cp '{}' \"$2\"; fi\n  shift\ndone\ncat '{}'\nexit 0",
            usage.display(),
            stream.display()
        ),
    );
    let cache = Arc::new(CopilotSignIn::default());
    start(&cache, &program, dir.path(), false);

    let settled = settle(&cache);
    assert_eq!(settled.state, SignInState::Ready);
    assert_eq!(settled.premium_requests, Some(0.33));
    assert_eq!(settled.ai_credits, Some(1.5));
}

#[test]
fn a_check_for_a_missing_program_settles_with_its_reason_and_runs_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-such-copilot");
    let cache = Arc::new(CopilotSignIn::default());

    start(&cache, &missing, dir.path(), false);
    let settled = settle(&cache);
    assert_eq!(settled.state, SignInState::Error);
    assert!(
        settled
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("not found")),
        "{settled:?}"
    );
}
