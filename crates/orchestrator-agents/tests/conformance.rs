//! Shared M4-04 adapter conformance scenarios. Each adapter gets the same
//! scenario names and its own fake-child temp directory per test.
//!
//! Updated: 2026-11-08T10:28:19Z.

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
);
