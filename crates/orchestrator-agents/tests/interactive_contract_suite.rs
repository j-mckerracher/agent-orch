//! Interactive-session contract (DPES-1180) run against each adapter's scripted child: Claude Code
//! stream-json (DPES-1181), Copilot ACP (DPES-1182) and Codex app-server (DPES-1183). Each test
//! checks one behavior. The list under each provider is the set of contracts that adapter's wire
//! protocol can express; wire formats live in `interactive_suite/providers.rs`.

#[path = "interactive_suite/contracts.rs"]
mod contracts;
#[path = "interactive_suite/providers.rs"]
mod providers;

macro_rules! contract_suite {
    ($module:ident, $provider:path, [$($contract:ident),* $(,)?]) => {
        mod $module {
            use super::{contracts, providers};

            $(
                #[tokio::test]
                async fn $contract() {
                    contracts::$contract(&$provider).await;
                }
            )*
        }
    };
}

contract_suite!(
    claude_code,
    providers::ClaudeCode,
    [
        session_started_is_the_first_event,
        session_without_a_turn_is_failed_on_exit,
        send_turn_is_refused_after_the_session_exits,
        send_turn_is_refused_while_a_turn_runs,
        successful_turn_completes_the_session,
        failed_turn_fails_the_session,
        finished_turn_frees_the_session_for_the_next_turn,
        unknown_approval_id_is_refused_and_writes_nothing,
        approval_answers_each_id_at_most_once,
        plan_proposal_is_answerable_once,
        empty_plan_is_a_protocol_error,
        cancel_ends_the_session_as_cancelled_and_is_idempotent,
        cancel_after_a_natural_exit_keeps_the_completed_outcome,
        next_event_returns_none_once_the_session_has_exited,
        malformed_frame_is_a_protocol_error_and_parsing_continues,
        protocol_violation_fails_the_exit_even_after_a_completed_turn,
        oversized_frame_is_a_protocol_error_and_parsing_continues,
        large_frame_below_the_line_limit_is_delivered_intact,
        unknown_notification_is_skipped_without_ending_the_session,
        text_deltas_round_trip_exactly,
        junk_lines_are_each_a_protocol_error_and_the_session_continues,
        missing_program_is_a_launch_error,
        nonzero_exit_fails_the_session_after_a_completed_turn,
        live_session_reports_a_pid_and_running_until_it_exits,
    ]
);

contract_suite!(
    copilot_acp,
    providers::CopilotAcp,
    [
        session_started_is_the_first_event,
        session_without_a_turn_is_failed_on_exit,
        send_turn_is_refused_after_the_session_exits,
        send_turn_is_refused_while_a_turn_runs,
        successful_turn_completes_the_session,
        failed_turn_fails_the_session,
        finished_turn_frees_the_session_for_the_next_turn,
        unknown_approval_id_is_refused_and_writes_nothing,
        approval_answers_each_id_at_most_once,
        cancel_ends_the_session_as_cancelled_and_is_idempotent,
        cancel_after_a_natural_exit_keeps_the_completed_outcome,
        next_event_returns_none_once_the_session_has_exited,
        malformed_frame_is_a_protocol_error_and_parsing_continues,
        protocol_violation_fails_the_exit_even_after_a_completed_turn,
        oversized_frame_is_a_protocol_error_and_parsing_continues,
        large_frame_below_the_line_limit_is_delivered_intact,
        unknown_notification_is_skipped_without_ending_the_session,
        text_deltas_round_trip_exactly,
        junk_lines_are_each_a_protocol_error_and_the_session_continues,
        missing_program_is_a_launch_error,
        nonzero_exit_fails_the_session_after_a_completed_turn,
        live_session_reports_a_pid_and_running_until_it_exits,
        child_exiting_before_the_handshake_fails_spawn,
    ]
);

contract_suite!(
    codex_app_server,
    providers::CodexAppServer,
    [
        session_started_is_the_first_event,
        session_without_a_turn_is_failed_on_exit,
        send_turn_is_refused_after_the_session_exits,
        send_turn_is_refused_while_a_turn_runs,
        successful_turn_completes_the_session,
        failed_turn_fails_the_session,
        finished_turn_frees_the_session_for_the_next_turn,
        unknown_approval_id_is_refused_and_writes_nothing,
        approval_answers_each_id_at_most_once,
        plan_proposal_is_answerable_once,
        empty_plan_is_a_protocol_error,
        cancel_ends_the_session_as_cancelled_and_is_idempotent,
        cancel_after_a_natural_exit_keeps_the_completed_outcome,
        next_event_returns_none_once_the_session_has_exited,
        malformed_frame_is_a_protocol_error_and_parsing_continues,
        protocol_violation_fails_the_exit_even_after_a_completed_turn,
        oversized_frame_is_a_protocol_error_and_parsing_continues,
        large_frame_below_the_line_limit_is_delivered_intact,
        unknown_notification_is_skipped_without_ending_the_session,
        text_deltas_round_trip_exactly,
        junk_lines_are_each_a_protocol_error_and_the_session_continues,
        missing_program_is_a_launch_error,
        nonzero_exit_fails_the_session_after_a_completed_turn,
        live_session_reports_a_pid_and_running_until_it_exits,
        child_exiting_before_the_handshake_fails_spawn,
    ]
);
