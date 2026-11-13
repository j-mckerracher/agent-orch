# M7-08 verification report

| Criterion | Implementation | Tests | Result / gap |
|---|---|---|---|
| AC1 runtime, version pin, ownership | `docs/m7-08-conversation-service.md`; `api_conversation_agent.rs` | `versions_parse_from_both_cli_formats_and_refuse_junk`; `pins_accept_known_agents_with_a_plain_version_only` | PASS; M7-25 `AgentRuntime` and daemon-owned SQLite recovery are documented. |
| AC2 ordering, correlation, idempotence, restart | `api_conversations.rs`; `sqlite_conversations.rs` | `replies_are_ordered_correlated_and_carry_history_and_workstream_context`; `replies_survive_a_restart_replay_by_sequence_and_stay_idempotent` | PASS; replies are cursor-polled chunks, not push events. |
| AC3 no task, approval, or delivery authority | `api_conversation_runtime.rs`; `api_conversations.rs` | `conversation_text_and_replies_create_no_task_approval_or_delivery` | PASS; malicious message/reply text is inert for Global and workstream targets; no delivery rows. |
| AC4 sequence replay | `api_conversations.rs`; `sqlite_conversations.rs` | `replies_survive_a_restart_replay_by_sequence_and_stay_idempotent` | PASS; exact native reconnect remains deferred. |

**Gates:** examples build passed. Default-parallel daemon test run: 176 passed, 6 process tests failed with version-probe timeouts, 1 ignored. Full serial daemon suite passed (176 unit; conversation runtime 13 passed/1 ignored; all other targets passed). `orchestrator-agents`: 127 passed. Daemon clippy with `-D warnings`: passed. CI disabled by owner; local gates only.

**Native/live gaps:** real Codex/WebView and Windows runs were not attempted per fast-delivery instructions; see `M7-08-native-checklist.md`. The ignored live Codex test is not a pass. No product behavior change; only a focused authority regression test and required evidence docs.