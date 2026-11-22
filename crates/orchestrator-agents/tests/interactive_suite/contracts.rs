//! Behaviors every interactive adapter in the applicable set must honor (DPES-1180), stated through
//! the public `AgentHandle` and `InteractiveAgentHandle` traits. Each function takes a provider and
//! asserts literal events, outcomes and stdin frame counts.

use crate::providers::{Handshaking, Planning, Provider};
use orchestrator_agents::{
    AgentError, AgentEvent, AgentExit, AgentLaunchRequest, ApprovalDecision,
    InteractiveAgentHandle, Result, TurnOutcome,
};
use std::path::Path;
use tempfile::TempDir;
use uuid::Uuid;

/// The supervisor's read limit for one stdout line. Longer lines are truncated.
const STDOUT_LINE_LIMIT: usize = 16 * 1024 * 1024;
const MALFORMED_LINE: &str = "this is not json";
const TEXT_ALPHABET: &[char] = &[
    'a', 'Z', '0', ' ', '"', '\\', '{', '}', '[', ']', ':', ',', '\n', '\r', '\t', 'é', '你', '🚀',
];

pub struct Session {
    dir: TempDir,
    handle: Box<dyn InteractiveAgentHandle>,
}

impl Session {
    async fn next_event(&mut self) -> AgentEvent {
        self.handle
            .next_event()
            .await
            .expect("next event")
            .expect("event before stream end")
    }

    async fn finish(mut self) -> (AgentExit, Vec<String>) {
        let exit = self.handle.wait().await.expect("wait");
        let stdin = std::fs::read_to_string(self.dir.path().join("stdin.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect();
        (exit, stdin)
    }
}

fn launch_request<P: Provider>(provider: &P, dir: &Path) -> AgentLaunchRequest {
    AgentLaunchRequest {
        program: provider.program(),
        args: Vec::new(),
        working_dir: dir.to_path_buf(),
        git_guard: false,
    }
}

async fn open<P: Provider>(provider: &P, turns: &[Vec<String>]) -> Session {
    let dir = tempfile::tempdir().expect("session directory");
    provider.write_script(dir.path(), turns);
    let handle = provider
        .launch(launch_request(provider, dir.path()))
        .await
        .expect("spawn session");
    Session { dir, handle }
}

async fn open_started<P: Provider>(provider: &P, turns: &[Vec<String>]) -> Session {
    let mut session = open(provider, turns).await;
    assert_eq!(
        session.next_event().await,
        AgentEvent::SessionStarted {
            session_id: provider.session_id(),
        }
    );
    session
}

fn deny(reason: &str) -> ApprovalDecision {
    ApprovalDecision::Deny {
        reason: reason.to_owned(),
    }
}

fn assert_protocol_refusal(result: Result<()>) {
    assert!(
        matches!(result, Err(AgentError::Protocol(_))),
        "expected a protocol refusal"
    );
}

fn text_event(text: &str) -> AgentEvent {
    AgentEvent::MessageDelta {
        text: text.to_owned(),
    }
}

fn turn_finished(outcome: TurnOutcome) -> AgentEvent {
    AgentEvent::TurnFinished { outcome }
}

fn round_trip_texts() -> Vec<String> {
    let mut texts: Vec<String> = [
        "",
        " ",
        "{\"type\":\"result\",\"subtype\":\"success\"}",
        "line one\nline two\r\n",
        "quote \" and backslash \\",
        "tab\tcomma, colon: é 你好 🚀",
        "[\"array\"]",
    ]
    .iter()
    .map(|text| (*text).to_owned())
    .collect();
    texts.extend(seeded_texts(0x5eed, 40));
    texts
}

fn seeded_texts(seed: u64, count: usize) -> Vec<String> {
    let mut random = Xorshift(seed);
    let mut texts = Vec::with_capacity(count);
    for _ in 0..count {
        let length = random.below(24);
        let mut text = String::new();
        for _ in 0..length {
            text.push(TEXT_ALPHABET[random.below(TEXT_ALPHABET.len())]);
        }
        texts.push(text);
    }
    texts
}

fn seeded_junk(seed: u64, count: usize) -> Vec<String> {
    let mut random = Xorshift(seed);
    let mut lines = Vec::with_capacity(count);
    for _ in 0..count {
        let length = random.below(60);
        let mut line = String::from("!");
        for _ in 0..length {
            line.push((0x20 + random.below(95) as u8) as char);
        }
        lines.push(line);
    }
    lines
}

struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

pub async fn session_started_is_the_first_event<P: Provider>(provider: &P) {
    let mut session = open(provider, &[]).await;
    assert_eq!(
        session.next_event().await,
        AgentEvent::SessionStarted {
            session_id: provider.session_id(),
        }
    );
    session.finish().await;
}

pub async fn session_without_a_turn_is_failed_on_exit<P: Provider>(provider: &P) {
    let (exit, _) = open_started(provider, &[]).await.finish().await;
    assert!(matches!(exit, AgentExit::Failed { .. }), "got {exit:?}");
}

pub async fn send_turn_is_refused_after_the_session_exits<P: Provider>(provider: &P) {
    let mut session = open_started(provider, &[]).await;
    session.handle.wait().await.expect("wait");
    assert_protocol_refusal(session.handle.send_turn("too late").await);
}

pub async fn send_turn_is_refused_while_a_turn_runs<P: Provider>(provider: &P) {
    let mut session = open_started(provider, &[]).await;
    session
        .handle
        .send_turn("first turn")
        .await
        .expect("first turn");
    assert_protocol_refusal(session.handle.send_turn("second turn").await);
    let (_, stdin) = session.finish().await;
    assert_eq!(
        stdin
            .iter()
            .filter(|line| line.contains("first turn"))
            .count(),
        1
    );
    assert!(stdin.iter().all(|line| !line.contains("second turn")));
}

pub async fn successful_turn_completes_the_session<P: Provider>(provider: &P) {
    let turns = [vec![provider.text("hello"), provider.turn_succeeded(0)]];
    let mut session = open_started(provider, &turns).await;
    session.handle.send_turn("say hello").await.expect("send");
    assert_eq!(session.next_event().await, text_event("hello"));
    assert_eq!(
        session.next_event().await,
        turn_finished(TurnOutcome::Success)
    );
    let (exit, _) = session.finish().await;
    assert!(matches!(exit, AgentExit::Completed { .. }), "got {exit:?}");
}

pub async fn failed_turn_fails_the_session<P: Provider>(provider: &P) {
    let turns = [vec![provider.turn_failed(0)]];
    let mut session = open_started(provider, &turns).await;
    session.handle.send_turn("do the task").await.expect("send");
    assert_eq!(
        session.next_event().await,
        turn_finished(TurnOutcome::Failure)
    );
    let (exit, _) = session.finish().await;
    assert!(matches!(exit, AgentExit::Failed { .. }), "got {exit:?}");
}

pub async fn finished_turn_frees_the_session_for_the_next_turn<P: Provider>(provider: &P) {
    let turns = [
        vec![provider.text("one"), provider.turn_succeeded(0)],
        vec![provider.text("two"), provider.turn_succeeded(1)],
    ];
    let mut session = open_started(provider, &turns).await;
    session.handle.send_turn("first").await.expect("first turn");
    assert_eq!(session.next_event().await, text_event("one"));
    assert_eq!(
        session.next_event().await,
        turn_finished(TurnOutcome::Success)
    );
    session
        .handle
        .send_turn("second")
        .await
        .expect("second turn after the first finished");
    assert_eq!(session.next_event().await, text_event("two"));
    assert_eq!(
        session.next_event().await,
        turn_finished(TurnOutcome::Success)
    );
    let (exit, _) = session.finish().await;
    assert!(matches!(exit, AgentExit::Completed { .. }), "got {exit:?}");
}

pub async fn unknown_approval_id_is_refused_and_writes_nothing<P: Provider>(provider: &P) {
    let mut session = open_started(provider, &[]).await;
    assert_protocol_refusal(
        session
            .handle
            .respond_approval(Uuid::new_v4(), deny("never issued"))
            .await,
    );
    let (_, stdin) = session.finish().await;
    assert_eq!(stdin.len(), provider.handshake_lines());
}

pub async fn approval_answers_each_id_at_most_once<P: Provider>(provider: &P) {
    let turns = [vec![provider.approval_request()]];
    let mut session = open_started(provider, &turns).await;
    session
        .handle
        .send_turn("make a change")
        .await
        .expect("send");
    let approval_id = match session.next_event().await {
        AgentEvent::ApprovalRequested { approval_id, .. } => approval_id,
        other => panic!("expected ApprovalRequested, got {other:?}"),
    };
    session
        .handle
        .respond_approval(approval_id, deny("not now"))
        .await
        .expect("first answer");
    assert_protocol_refusal(
        session
            .handle
            .respond_approval(approval_id, deny("again"))
            .await,
    );
    let (_, stdin) = session.finish().await;
    assert_eq!(stdin.len(), provider.handshake_lines() + 2);
}

pub async fn plan_proposal_is_answerable_once<P: Planning>(provider: &P) {
    let plan_text = "1. Edit calc.py\n2. Run the tests";
    let turns = [vec![provider.plan(plan_text)]];
    let mut session = open_started(provider, &turns).await;
    session
        .handle
        .send_turn("plan the change")
        .await
        .expect("send");
    let approval_id = match session.next_event().await {
        AgentEvent::PlanProposed { approval_id, plan } => {
            assert_eq!(plan, plan_text);
            approval_id
        }
        other => panic!("expected PlanProposed, got {other:?}"),
    };
    session
        .handle
        .respond_approval(approval_id, deny("revise"))
        .await
        .expect("first answer");
    assert_protocol_refusal(
        session
            .handle
            .respond_approval(approval_id, deny("again"))
            .await,
    );
    session.finish().await;
}

pub async fn empty_plan_is_a_protocol_error<P: Planning>(provider: &P) {
    let turns = [vec![provider.plan("")]];
    let mut session = open_started(provider, &turns).await;
    session.handle.send_turn("plan").await.expect("send");
    assert!(matches!(
        session.next_event().await,
        AgentEvent::ProtocolError { .. }
    ));
    session.finish().await;
}

pub async fn cancel_ends_the_session_as_cancelled_and_is_idempotent<P: Provider>(provider: &P) {
    let mut session = open_started(provider, &[]).await;
    session.handle.cancel().await.expect("first cancel");
    session.handle.cancel().await.expect("second cancel");
    let (exit, _) = session.finish().await;
    assert!(matches!(exit, AgentExit::Cancelled { .. }), "got {exit:?}");
}

pub async fn cancel_after_a_natural_exit_keeps_the_completed_outcome<P: Provider>(provider: &P) {
    let turns = [vec![provider.text("hi"), provider.turn_succeeded(0)]];
    let mut session = open_started(provider, &turns).await;
    session.handle.send_turn("go").await.expect("send");
    assert_eq!(session.next_event().await, text_event("hi"));
    assert_eq!(
        session.next_event().await,
        turn_finished(TurnOutcome::Success)
    );
    let exit = session.handle.wait().await.expect("wait");
    assert!(matches!(exit, AgentExit::Completed { .. }), "got {exit:?}");
    session.handle.cancel().await.expect("cancel after exit");
    assert_eq!(session.handle.wait().await.expect("second wait"), exit);
}

pub async fn next_event_returns_none_once_the_session_has_exited<P: Provider>(provider: &P) {
    let mut session = open_started(provider, &[]).await;
    session.handle.wait().await.expect("wait");
    assert_eq!(session.handle.next_event().await.expect("next event"), None);
}

pub async fn malformed_frame_is_a_protocol_error_and_parsing_continues<P: Provider>(provider: &P) {
    let turns = [vec![
        MALFORMED_LINE.to_owned(),
        provider.text("after"),
        provider.turn_succeeded(0),
    ]];
    let mut session = open_started(provider, &turns).await;
    session.handle.send_turn("go").await.expect("send");
    assert!(matches!(
        session.next_event().await,
        AgentEvent::ProtocolError { .. }
    ));
    assert_eq!(session.next_event().await, text_event("after"));
    assert_eq!(
        session.next_event().await,
        turn_finished(TurnOutcome::Success)
    );
    session.finish().await;
}

pub async fn protocol_violation_fails_the_exit_even_after_a_completed_turn<P: Provider>(
    provider: &P,
) {
    let turns = [vec![
        MALFORMED_LINE.to_owned(),
        provider.text("after"),
        provider.turn_succeeded(0),
    ]];
    let mut session = open_started(provider, &turns).await;
    session.handle.send_turn("go").await.expect("send");
    assert!(matches!(
        session.next_event().await,
        AgentEvent::ProtocolError { .. }
    ));
    assert_eq!(session.next_event().await, text_event("after"));
    assert_eq!(
        session.next_event().await,
        turn_finished(TurnOutcome::Success)
    );
    let (exit, _) = session.finish().await;
    assert!(matches!(exit, AgentExit::Failed { .. }), "got {exit:?}");
}

pub async fn oversized_frame_is_a_protocol_error_and_parsing_continues<P: Provider>(provider: &P) {
    let turns = [vec![
        provider.text(&"a".repeat(STDOUT_LINE_LIMIT)),
        provider.text("after"),
        provider.turn_succeeded(0),
    ]];
    let mut session = open_started(provider, &turns).await;
    session.handle.send_turn("go").await.expect("send");
    assert!(matches!(
        session.next_event().await,
        AgentEvent::ProtocolError { .. }
    ));
    assert_eq!(session.next_event().await, text_event("after"));
    assert_eq!(
        session.next_event().await,
        turn_finished(TurnOutcome::Success)
    );
    session.finish().await;
}

pub async fn large_frame_below_the_line_limit_is_delivered_intact<P: Provider>(provider: &P) {
    let large = "b".repeat(2 * 1024 * 1024);
    let turns = [vec![provider.text(&large), provider.turn_succeeded(0)]];
    let mut session = open_started(provider, &turns).await;
    session.handle.send_turn("go").await.expect("send");
    let delivered = session.next_event().await;
    assert!(
        matches!(&delivered, AgentEvent::MessageDelta { text } if *text == large),
        "2 MiB text was not delivered intact"
    );
    assert_eq!(
        session.next_event().await,
        turn_finished(TurnOutcome::Success)
    );
    session.finish().await;
}

pub async fn unknown_notification_is_skipped_without_ending_the_session<P: Provider>(provider: &P) {
    let turns = [vec![
        provider.unknown_notification(),
        provider.text("after"),
        provider.turn_succeeded(0),
    ]];
    let mut session = open_started(provider, &turns).await;
    session.handle.send_turn("go").await.expect("send");
    assert_eq!(session.next_event().await, text_event("after"));
    assert_eq!(
        session.next_event().await,
        turn_finished(TurnOutcome::Success)
    );
    session.finish().await;
}

pub async fn text_deltas_round_trip_exactly<P: Provider>(provider: &P) {
    let texts = round_trip_texts();
    let mut turn: Vec<String> = texts.iter().map(|text| provider.text(text)).collect();
    turn.push(provider.turn_succeeded(0));
    let mut session = open_started(provider, &[turn]).await;
    session.handle.send_turn("echo").await.expect("send");
    for text in &texts {
        assert_eq!(session.next_event().await, text_event(text));
    }
    assert_eq!(
        session.next_event().await,
        turn_finished(TurnOutcome::Success)
    );
    session.finish().await;
}

pub async fn junk_lines_are_each_a_protocol_error_and_the_session_continues<P: Provider>(
    provider: &P,
) {
    let junk = seeded_junk(0xbad, 40);
    let mut turn = junk.clone();
    turn.push(provider.text("ok"));
    turn.push(provider.turn_succeeded(0));
    let mut session = open_started(provider, &[turn]).await;
    session.handle.send_turn("go").await.expect("send");
    for _ in &junk {
        assert!(matches!(
            session.next_event().await,
            AgentEvent::ProtocolError { .. }
        ));
    }
    assert_eq!(session.next_event().await, text_event("ok"));
    assert_eq!(
        session.next_event().await,
        turn_finished(TurnOutcome::Success)
    );
    session.finish().await;
}

pub async fn missing_program_is_a_launch_error<P: Provider>(provider: &P) {
    let dir = tempfile::tempdir().expect("directory");
    let request = AgentLaunchRequest {
        program: dir.path().join("no-such-agent"),
        args: Vec::new(),
        working_dir: dir.path().to_path_buf(),
        git_guard: false,
    };
    assert!(matches!(
        provider.launch(request).await,
        Err(AgentError::Launch(_))
    ));
}

pub async fn nonzero_exit_fails_the_session_after_a_completed_turn<P: Provider>(provider: &P) {
    let turns = [vec![provider.text("done"), provider.turn_succeeded(0)]];
    let mut session = open_started(provider, &turns).await;
    std::fs::write(session.dir.path().join("exit_code"), "3").expect("exit code file");
    session.handle.send_turn("go").await.expect("send");
    assert_eq!(session.next_event().await, text_event("done"));
    assert_eq!(
        session.next_event().await,
        turn_finished(TurnOutcome::Success)
    );
    let (exit, _) = session.finish().await;
    assert!(
        matches!(exit, AgentExit::Failed { code: Some(3), .. }),
        "got {exit:?}"
    );
}

pub async fn live_session_reports_a_pid_and_running_until_it_exits<P: Provider>(provider: &P) {
    let mut session = open_started(provider, &[]).await;
    assert!(session.handle.pid() > 0);
    assert!(!session.handle.try_exited().expect("probe before exit"));
    session.handle.wait().await.expect("wait");
    assert!(session.handle.try_exited().expect("probe after exit"));
}

pub async fn child_exiting_before_the_handshake_fails_spawn<P: Handshaking>(provider: &P) {
    let dir = tempfile::tempdir().expect("directory");
    provider.write_child_exiting_before_answer(dir.path());
    let result = provider.launch(launch_request(provider, dir.path())).await;
    assert!(
        matches!(result, Err(AgentError::Protocol(_) | AgentError::Launch(_))),
        "spawn succeeded against a child that exited before answering"
    );
}
