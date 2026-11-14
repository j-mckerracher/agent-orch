//! Shared M4-04 adapter conformance scenarios. Each adapter gets the same
//! scenario names and its own fake-child temp directory per test.
//!
//! Per adapter: 8 base scenarios + crlf_framing, cancel_after_exit and
//! cancellation_tree; Codex adds nonterminal_warning and item_outside_turn;
//! Claude Code and Pi add error_result. Fake NDJSON/RPC children only.
//!
//! Updated: 2026-11-09.

#[path = "conformance/harness.rs"]
mod harness;

macro_rules! adapter_suite {
    ($adapter:expr, $($test:ident => $scenario:ident),+ $(,)?) => {
        $(
            #[test]
            fn $test() {
                harness::run($adapter, harness::Scenario::$scenario);
            }
        )+
    };
}

adapter_suite!(
    harness::Adapter::Codex,
    codex_framing => Framing,
    codex_launch => Launch,
    codex_cancellation => Cancellation,
    codex_errors => Errors,
    codex_unexpected_eof => UnexpectedEof,
    codex_unsupported_features => UnsupportedFeatures,
    codex_interruption => Interruption,
    codex_approvals => Approvals,
    codex_crlf_framing => CrlfFraming,
    codex_cancel_after_exit => CancelAfterExit,
    codex_cancellation_tree => CancellationTree,
    codex_nonterminal_warning => NonterminalWarning,
    codex_item_outside_turn => ItemOutsideTurn,
);

adapter_suite!(
    harness::Adapter::ClaudeCode,
    claude_code_framing => Framing,
    claude_code_launch => Launch,
    claude_code_cancellation => Cancellation,
    claude_code_errors => Errors,
    claude_code_unexpected_eof => UnexpectedEof,
    claude_code_unsupported_features => UnsupportedFeatures,
    claude_code_interruption => Interruption,
    claude_code_approvals => Approvals,
    claude_code_crlf_framing => CrlfFraming,
    claude_code_cancel_after_exit => CancelAfterExit,
    claude_code_cancellation_tree => CancellationTree,
    claude_code_error_result => ErrorResult,
);

adapter_suite!(
    harness::Adapter::PiRpc,
    pi_rpc_framing => Framing,
    pi_rpc_launch => Launch,
    pi_rpc_cancellation => Cancellation,
    pi_rpc_errors => Errors,
    pi_rpc_unexpected_eof => UnexpectedEof,
    pi_rpc_unsupported_features => UnsupportedFeatures,
    pi_rpc_interruption => Interruption,
    pi_rpc_approvals => Approvals,
    pi_rpc_crlf_framing => CrlfFraming,
    pi_rpc_cancel_after_exit => CancelAfterExit,
    pi_rpc_cancellation_tree => CancellationTree,
    pi_rpc_error_result => ErrorResult,
);
