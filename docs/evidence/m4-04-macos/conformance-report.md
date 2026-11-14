# M4-04 macOS conformance evidence

Recorded: 2026-11-08T11:43:06Z.
Branch: `orch/m4-04-conformance`.
Base: `66ffe06b0e271bec8c72ae7a78d593b979858f45`.

Plan: verify the assigned branch and baseline, read adapter contracts and capability values, add one shared eight-scenario suite with isolated fake children, run one bounded probe per installed CLI, then run workspace tests and Clippy. This file records the plan and results.

## Initial scenario results before adapter fixes

All 24 test names use the same eight scenario suffixes for Codex, Claude Code, and Pi. Each test owns a unique `tempfile::TempDir`; its fake executable and files are removed when that test drops the directory.

| Scenario | Codex | Claude Code | Pi |
| --- | --- | --- | --- |
| Framing | Pass | Pass | Pass |
| Launch | Pass | Pass | Pass |
| Cancellation | Pass | Pass | Pass |
| Errors | Fail | Fail | Fail |
| Unexpected EOF | Pass | Pass | Pass |
| Unsupported features | Pass | Pass | Pass |
| Interruption | Pass | Pass | Pass |
| Approvals | N/A, `approvals=false` | N/A, `approvals=false` | N/A, `approvals=false` |

The errors tests require non-zero process exit to fail after protocol completion. Codex, Claude Code, and Pi all returned `AgentExit::Completed { code: Some(7), .. }`. The Pi test also sends malformed JSON followed by a valid `agent_settled` record. Pi ignored malformed JSON and returned `Completed { code: Some(0), .. }`. Codex and Claude Code surfaced `AgentEvent::ProtocolError` and returned `Failed` for the exit-zero protocol-error case.

Failing test names: `codex_errors`, `claude_code_errors`, and `pi_rpc_errors`. Adapter sources were unchanged in the original M4-04 run.

The false capability values are Codex and Claude Code `interactive_input`, `session_resumption`, `approvals`, and `model_selection`; Pi `session_resumption`, `approvals`, and `model_selection`. The routing tests require each false capability individually and compare the returned `RoutingError` gap. Approval-request handling is not applicable because all three report `approvals=false`; routing rejects `Capability::Approvals`. `AgentEvent` has no approval-granted variant.

## macOS CLI probes

Each probe ran once and made no model call. Raw stdout and stderr are stored beside this report. Empty stderr files are zero bytes.

| Adapter | Command | Exit | Raw output |
| --- | --- | --- | --- |
| Codex | `/opt/homebrew/bin/codex --version` | 0 | `codex-version.stdout.txt`, 18 bytes. Output: `codex-cli 0.167.0` |
| Claude Code | `/opt/homebrew/bin/claude --version` | 0 | `claude-version.stdout.txt`, 22 bytes. Output: `2.1.311 (Claude Code)` |
| Pi | `printf '%s\n' '{"id":"m4-04-state","type":"get_state"}'` \| `/Users/mckerracher.joshua/.bun/bin/pi --mode rpc --no-session --offline --no-extensions --no-mcp --no-skills --no-context-files` | 0 | `pi-rpc.stdout.jsonl`, 1052 bytes. `get_state` succeeded; no prompt was sent. |

The commands captured stdout and stderr in these files:

```sh
/opt/homebrew/bin/codex --version > docs/evidence/m4-04-macos/codex-version.stdout.txt 2> docs/evidence/m4-04-macos/codex-version.stderr.txt
/opt/homebrew/bin/claude --version > docs/evidence/m4-04-macos/claude-version.stdout.txt 2> docs/evidence/m4-04-macos/claude-version.stderr.txt
printf '%s\n' '{"id":"m4-04-state","type":"get_state"}' | /Users/mckerracher.joshua/.bun/bin/pi --mode rpc --no-session --offline --no-extensions --no-mcp --no-skills --no-context-files > docs/evidence/m4-04-macos/pi-rpc.stdout.jsonl 2> docs/evidence/m4-04-macos/pi-rpc.stderr.txt
```

## Original verification log before coordinator amendment

| Command | Exit | Result |
| --- | ---: | --- |
| `cargo test --locked --workspace` baseline | 0 | 249 passed, 1 ignored. |
| `cargo test --locked -p orchestrator-agents --test conformance` initial run | 101 | 17 passed, 7 failed. Five Pi cases used the wrong fake-child argument offset because Pi prepends `--mode rpc`; fixture parsing was corrected. |
| `cargo test --locked -p orchestrator-agents --test conformance` after fixture correction | 101 | 21 passed, 3 failed. The three errors cases found non-zero-exit outcome defects. |
| `cargo test --locked -p orchestrator-agents --test conformance` after combining both error assertions | 101 | 21 passed, 3 failed. Output identified both Pi error outcomes. |
| `cargo test --locked --workspace` final run | 101 | Fail-fast stopped at `orchestrator-agents` conformance tests. Those tests reported 21 passed and 3 failed. |
| `cargo test --locked --workspace --no-fail-fast` final run | 101 | 270 passed, 3 failed, 1 ignored. Every pre-existing test passed. |
| `cargo clippy --locked -p orchestrator-agents --all-targets --no-deps` first run | 0 | Clippy reported a large enum and unused imports. |
| `cargo clippy --locked -p orchestrator-agents --all-targets --no-deps` final run | 0 | Clean after boxing the Pi test handle and removing unused imports. |
| `cargo fmt --package orchestrator-agents -- --check` | 1 | Reported formatting changes in existing `src/codex_adapter.rs`, `src/contract.rs`, and `src/lib.rs`, plus the new harness. Only in-scope test files were formatted. |
| `rustfmt --edition 2021 crates/orchestrator-agents/tests/conformance.rs crates/orchestrator-agents/tests/conformance/harness.rs crates/orchestrator-agents/tests/conformance/fake_child.rs` | 0 | Formatted only the three new Rust test files. |
| `rustfmt --edition 2021 --check crates/orchestrator-agents/tests/conformance.rs crates/orchestrator-agents/tests/conformance/harness.rs crates/orchestrator-agents/tests/conformance/fake_child.rs` | 0 | All new Rust files pass formatting check. |
| `git diff --check` | 0 | No unstaged whitespace errors. |
| `git diff --cached --check` | 0 | No staged whitespace errors in the checked index. |

The original Pi setup failures were fixture errors. The remaining `Errors` failures exposed adapter behavior. The coordinator amendment and final results follow.

## Initial acceptance status before adapter fixes

1. Not met. The suite runs the same scenario names for all adapters. The `Errors` scenario fails for all three adapters instead of passing or being not applicable.
2. Met. Tests assert protocol events, typed launch or protocol errors, terminal outcomes, routing gaps, and process liveness. They do not assert elapsed time.
3. Met. Each test creates and owns its fake-child temp directory.
4. Met. Probe stdout, stderr, commands, and exit codes are recorded in this directory.
5. Not met. `cargo test --locked --workspace` exits 101 because of the three conformance failures. The `--no-fail-fast` run confirms all pre-existing tests pass.
6. Met. Final Clippy command exits 0.
7. Met for macOS only. Windows remains pending. Run `cargo test --locked --workspace` and `cargo clippy --locked -p orchestrator-agents --all-targets --no-deps` on Windows when available.

Initial report updated: 2026-11-08T10:59:19Z.

## Coordinator amendment results

Recorded: 2026-11-08T11:47:07Z.

Adapter changes were made under the coordinator's scope amendment. The permitted source files changed are `codex_adapter.rs`, `claude_code_adapter.rs`, and `pi_rpc_adapter.rs`. The amended Pi path also rejects nonzero exit after a settled turn because the shared conformance scenario requires that outcome for every adapter.

| Defect | Before | After |
| --- | --- | --- |
| Codex nonzero exit after protocol completion | `AgentExit::Completed { code: Some(7), last_message: Some("done") }` | `AgentExit::Failed`, reason includes `code 7` and `last message: done`. |
| Claude Code nonzero exit after protocol completion | `AgentExit::Completed { code: Some(7), last_message: Some("done") }` | `AgentExit::Failed`, reason includes `code 7` and `last message: done`. |
| Pi malformed JSON followed by `agent_settled` | Malformed line ignored; exit 0 returned `Completed`. No protocol event surfaced. | Malformed line produces `AgentEvent::ProtocolError`. Parsing continues to later records. `TurnFinished` becomes `Failure`; terminal `AgentExit::Failed.reason` names the protocol violation. |
| Pi nonzero exit after `agent_settled` | Exit 7 returned `Completed`. | Exit 7 returns `Failed` with reason naming code 7. Mid-turn EOF keeps its existing `Pi RPC process exited before agent_settled` reason. |

`AgentExit::Failed` has no `last_message` field. Codex and Claude Code preserve the last message in the failure reason, and their focused unit tests assert both code and message.

Focused tests:

- `codex_adapter::tests::nonzero_exit_after_completed_turn_fails_and_keeps_last_message`
- `claude_code_adapter::tests::nonzero_exit_after_completed_turn_fails_and_keeps_last_message`
- `pi_rpc_adapter::tests::malformed_json_is_recoverable_and_fails_settled_turn`
- `pi_rpc_adapter::tests::nonzero_exit_after_settled_turn_fails`

Two existing integration-test expectations changed because they encoded the defects. `tests/codex_adapter.rs` now expects a code-bearing failure reason for exit 7. `tests/claude_code_adapter.rs` now expects failure after a completed protocol turn with exit 7. The existing Pi recovery test still reads a valid response after malformed input.

| Command | Exit | Result |
| --- | ---: | --- |
| Before fix: `cargo test --locked -p orchestrator-agents --test conformance` | 101 | 21 passed, 3 failed. Failing tests were `codex_errors`, `claude_code_errors`, and `pi_rpc_errors`. |
| Codex focused unit test | 0 | 1 passed. |
| Claude Code focused unit test | 0 | 1 passed. |
| Pi focused unit test, first run | 101 | Compile failed on a nested mutable borrow in the event queue. |
| Pi focused unit test, corrected run | 0 | 1 passed. |
| Adapter integration targets, first run | 101 | Compile failed because two assertions borrowed values after partial moves. |
| Adapter integration targets, second run | 101 | 8 passed, 2 failed. Existing Pi EOF tests required their previous reason string. The Pi fix retained that reason when no turn settled. |
| `cargo test --locked -p orchestrator-agents --tests`, final run | 0 | 76 passed across 6 suites. |
| `cargo test --locked -p orchestrator-agents --test conformance`, final run | 0 | 24 passed. |
| `cargo test --locked --workspace`, final run | 0 | 277 passed, 1 ignored across 34 suites. |
| `cargo clippy --locked -p orchestrator-agents --all-targets --no-deps`, final run | 0 | Clean. |

The original M4-04 baseline and CLI probe evidence remain above. Windows was not run and remains pending for the user.

## Final scenario results after fixes

| Scenario | Codex | Claude Code | Pi |
| --- | --- | --- | --- |
| Framing | Pass | Pass | Pass |
| Launch | Pass | Pass | Pass |
| Cancellation | Pass | Pass | Pass |
| Errors | Pass | Pass | Pass |
| Unexpected EOF | Pass | Pass | Pass |
| Unsupported features | Pass | Pass | Pass |
| Interruption | Pass | Pass | Pass |
| Approvals | N/A, `approvals=false` | N/A, `approvals=false` | N/A, `approvals=false` |


## Coordinator amendment acceptance

Recorded: 2026-11-08T11:53:16Z.

1. Met. `cargo test --locked -p orchestrator-agents --test conformance` passes all 24 tests. Approval cases remain N/A for the documented `approvals=false` capability.
2. Not met literally. All existing adapter and routing test targets pass. Two expectations changed because they asserted the defects: Codex expected no exit reason, and Claude Code expected `Completed` after exit 7.
3. Met. `cargo test --locked --workspace` passes 277 tests; 1 remains ignored.
4. Met. Final Clippy command exits 0.
5. Met. This report records before and after behavior and command exits for both defects.
6. Met for this macOS attempt. Windows remains pending.

Windows remains pending. Run these commands on Windows when available:

```sh
cargo test --locked --workspace
cargo clippy --locked -p orchestrator-agents --all-targets --no-deps
```
## Amendment command log

| Command | Exit | Result |
| --- | ---: | --- |
| Before fix: `cargo test --locked -p orchestrator-agents --test conformance` | 101 | 21 passed, 3 failed. Reproduced `codex_errors`, `claude_code_errors`, and `pi_rpc_errors`. |
| `cargo test --locked -p orchestrator-agents --lib codex_adapter::tests::nonzero_exit_after_completed_turn_fails_and_keeps_last_message` | 0 | 1 passed. |
| `cargo test --locked -p orchestrator-agents --lib claude_code_adapter::tests::nonzero_exit_after_completed_turn_fails_and_keeps_last_message` | 0 | 1 passed. |
| `cargo test --locked -p orchestrator-agents --lib pi_rpc_adapter::tests::malformed_json_is_recoverable_and_fails_settled_turn` first run | 101 | Compile failed on nested mutable borrow while queueing the error event. |
| `cargo test --locked -p orchestrator-agents --lib pi_rpc_adapter::tests::malformed_json_is_recoverable_and_fails_settled_turn` corrected run | 0 | 1 passed. |
| `cargo test --locked -p orchestrator-agents --lib` | 0 | 13 passed. |
| `cargo test --locked -p orchestrator-agents --test conformance` after tightening the Pi event assertion | 101 | Compile failed because the harness lacked the `TurnOutcome` import. The import was added. |
| `cargo test --locked -p orchestrator-agents --test conformance` after Pi protocol-error fix and import correction | 101 | 23 passed, 1 failed. Pi still returned `Completed` for exit 7 after settlement. |
| `cargo test --locked -p orchestrator-agents --tests`, first run | 101 | Compile failed because assertions borrowed `exit` after moving its reason. |
| `cargo test --locked -p orchestrator-agents --tests` second run | 101 | 8 passed, 2 failed. Pi mid-turn EOF tests expected the previous reason string. The final mapping preserves that reason when no turn settled. |
| `cargo test --locked -p orchestrator-agents --tests`, final run | 0 | 76 passed across 6 suites. |
| `cargo test --locked -p orchestrator-agents --test conformance`, final run | 0 | 24 passed. |
| `cargo test --locked --workspace`, final run | 0 | 277 passed, 1 ignored across 34 suites. |
| `cargo clippy --locked -p orchestrator-agents --all-targets --no-deps`, first post-fix run | 0 | Reported `items_after_test_module` for the Codex unit-test module. The test module was moved after the adapter functions. |
| `cargo clippy --locked -p orchestrator-agents --all-targets --no-deps`, final run | 0 | Clean. |
| `rustfmt --edition 2021 --check crates/orchestrator-agents/src/codex_adapter.rs crates/orchestrator-agents/src/claude_code_adapter.rs crates/orchestrator-agents/src/pi_rpc_adapter.rs crates/orchestrator-agents/tests/conformance/harness.rs crates/orchestrator-agents/tests/codex_adapter.rs crates/orchestrator-agents/tests/claude_code_adapter.rs` | 0 | All changed Rust files pass formatting. |
| `git diff --check` | 0 | No whitespace errors in the working tree. |
| `git diff --cached --check` | 0 | No whitespace errors in the staged amendment. |

The original Codex and Claude Code defects were fixed under the amendment. Pi's nonzero-after-settlement outcome was also changed because the shared conformance Errors scenario requires failure for all adapters. No Windows command was run.

Updated: 2026-11-08T11:53:16Z.

## M4-04 reverification at base `b5db71a3` (macOS, fake children)

Recorded: 2026-11-09. Everything above, including the 24-test, 76-suite and 277-workspace counts, is the historical result at the earlier base and is not the current count. The numbers below are the current, separately measured results; Mac counts do not transfer to Windows.

Branch `task/m4-04-adapter-conformance-b5db71a3-20261109`, base `b5db71a3b395d49c0767d8019c502d9fadfd0aa3`. Raw stdout, stderr, exit codes and metadata are stored outside the checkout in `.agent-worktrees/m4-04-evidence-20261109/` (files 00 to 09).

### Measured RED then GREEN with an identical test tree

The `crates/orchestrator-agents/tests` tree is `18d669e5e5c1b4e6ba692f8ca786c620e1217fde` in both runs. Only `src/` changed between them.

| Run | Commit | Command | Exit | Result |
| --- | --- | --- | ---: | --- |
| RED | `843bc40c` | `cargo test --locked -p orchestrator-agents --test conformance` | 101 | 37 tests: 33 passed, 4 failed. |
| GREEN | `32563640` | `cargo test --locked -p orchestrator-agents --tests` | 0 | 92 passed: lib 16, claude 10, codex 10, conformance 37, pi 10, routing 9. |
| GREEN | `32563640` | `cargo clippy --locked -p orchestrator-agents --all-targets --no-deps` | 0 | Clean. |

The four RED failures were real contract failures:

- `claude_code_error_result`: `is_error` result plus exit 0 returned `Completed`.
- `pi_rpc_error_result`: `agent_settled` with `aborted:true` plus exit 0 returned `Completed`.
- `claude_code_cancel_after_exit` and `codex_cancel_after_exit`: a cancel after an observed natural exit relabelled `Completed` as `Cancelled`. Pi already guarded this.

Fixes: Claude finishes the turn only on a successful `result`; Pi maps a `Failure` turn outcome to `Failed`; Codex and Claude `cancel()` do nothing once the exit was observed or a cancel was already issued. Codex has no failed-turn path: `turn.failed` is an unknown-type violation (parser unit test).

The 37 conformance tests are the 24 historical tests plus 13 new ones. Codex adds `crlf_framing`, `cancel_after_exit`, `cancellation_tree`, `nonterminal_warning`, `item_outside_turn`. Claude Code and Pi each add `crlf_framing`, `cancel_after_exit`, `cancellation_tree`, `error_result`. The count is taken from the test output.

A later source-only change makes a failed Claude `result` carry the provider error text in `AgentExit::Failed.reason`. It has a unit assertion in `claude_code_adapter::tests::error_result_with_clean_exit_is_not_completed`. Its results are not in the table above and are recorded only once it has been built and run.

### Per-adapter contract status (macOS, fake children)

| Scenario | Codex | Claude Code | Pi |
| --- | --- | --- | --- |
| Framing (split, oversized, CRLF) | Pass | Pass | Pass |
| Launch (argv `literal arg;$(echo must-not-run)`) | Pass | Pass | Pass |
| Cancellation, including a descendant process | Pass | Pass | Pass |
| Cancel after natural exit | Pass | Pass | Pass |
| Errors (nonzero exit keeps code and last message; protocol error) | Pass | Pass | Pass |
| Failed turn plus exit 0 is not `Completed` | N/A, no such record | Pass | Pass |
| Codex 0.167 warning order and out-of-turn negatives | Pass | N/A | N/A |
| Unexpected EOF | Pass | Pass | Pass |
| Unsupported features and routing | Pass | Pass | Pass |
| Interruption | Pass | Pass | Pass |
| Approvals | No-grant and routing refusal | No-grant and routing refusal | No-grant and routing refusal |

### Limits

- Fake children only. Every scenario uses a fake NDJSON or RPC child compiled from `tests/conformance/fake_child.rs`, with no provider credentials. The only real-CLI evidence is the macOS version and handshake probes above.
- Windows is not covered by this report. The tests have no `cfg(unix)` gating on the scenarios, so the same test names must run natively at the final SHA. Open Windows points: `rustc` on `PATH`, argv quoting of the launch argument, Codex resolving only `.exe` and not `.cmd` shims.
- Interruption differs by platform and is not equivalent. Unix sends SIGINT. Windows uses `taskkill /T /F`, a forced external kill, so a Windows pass proves no graceful interrupt.
- CRLF proof is tolerance only. It shows CRLF-terminated records decode like LF records. It does not prove the runtime's trailing `\r` stripping or a read-boundary crossing, and the split-record fixtures flush each half with no delay.
- Approvals prove no grant and routing refusal only. `approvals=false` for all three adapters, no approval protocol is implemented, and an approval-like provider record is never surfaced as `ApprovalRequested`. This is not evidence of safe interactive approval handling.

Updated: 2026-11-09.
