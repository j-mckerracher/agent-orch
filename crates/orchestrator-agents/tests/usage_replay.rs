//! Replays recorded adapter streams through the real parsers and asserts the
//! usage each provider reports. The inputs are committed evidence captures,
//! not synthetic fixtures: M1-05 (Codex, macOS) and M4-02 (Claude Code, macOS).
//! Pi has no recorded live usage stream; its parser is covered by the fake RPC
//! child in `pi_rpc_adapter.rs`.

use orchestrator_agents::claude_code_adapter::StreamParser;
use orchestrator_agents::{AgentEvent, Parser, ReportedCost, TurnOutcome};

const CODEX_STREAM: &str = include_str!(
    "../../../docs/evidence/m1-05-macos/codex-run-5c27yt83/data/cli/m105-codex-isolation/codex.jsonl"
);
const CLAUDE_STREAM: &str =
    include_str!("../../../docs/evidence/m4-02-macos/20261107T215309-0700.stdout.jsonl");

#[test]
fn codex_recorded_stream_reports_tokens_and_no_cost() {
    let mut parser = Parser::new();
    let mut finished = false;
    for line in CODEX_STREAM.lines() {
        if let Some(Ok(AgentEvent::TurnFinished { outcome })) = parser.feed(line) {
            assert_eq!(outcome, TurnOutcome::Success);
            finished = true;
        }
    }
    assert!(finished && parser.violation().is_none());
    let reports = parser.take_usage();
    assert_eq!(reports.len(), 1);
    let report = &reports[0];
    assert_eq!(report.receipt_key, "turn.completed:1");
    assert_eq!(report.input_tokens, Some(72_641 - 48_128));
    assert_eq!(report.cache_read_tokens, Some(48_128));
    assert_eq!(report.output_tokens, Some(419));
    assert_eq!(report.reasoning_tokens, Some(141));
    assert_eq!(report.cost, None, "Codex reports no cost");
    assert!(parser.take_usage().is_empty());
}

#[test]
fn claude_recorded_stream_reports_tokens_and_cost() {
    let mut parser = StreamParser::new();
    for line in CLAUDE_STREAM.lines() {
        parser.feed(line);
    }
    assert!(parser.turn_finished() && parser.violation().is_none());
    let reports = parser.take_usage();
    assert_eq!(reports.len(), 1);
    let report = &reports[0];
    assert_eq!(report.receipt_key, "result");
    assert_eq!(report.input_tokens, Some(2));
    assert_eq!(report.cache_write_tokens, Some(1_360));
    assert_eq!(report.cache_read_tokens, Some(2_254));
    assert_eq!(report.output_tokens, Some(69));
    assert_eq!(report.model.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(
        report.cost,
        Some(ReportedCost {
            nanos: 8_638_800,
            currency: "USD".into()
        })
    );
}

#[test]
fn stream_without_a_usage_record_reports_nothing() {
    let mut parser = Parser::new();
    for line in CODEX_STREAM
        .lines()
        .filter(|l| !l.contains("turn.completed"))
    {
        parser.feed(line);
    }
    assert!(parser.take_usage().is_empty());
}
