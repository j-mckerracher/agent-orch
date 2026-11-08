# M4-04 macOS conformance evidence

Recorded: 2026-11-08T10:59:19Z.
Branch: `orch/m4-04-conformance`.
Base: `66ffe06b0e271bec8c72ae7a78d593b979858f45`.

Plan: verify the assigned branch and baseline, read adapter contracts and capability values, add one shared eight-scenario suite with isolated fake children, run one bounded probe per installed CLI, then run workspace tests and Clippy. This file records the plan and results.

## Scenario results

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

Failing test names: `codex_errors`, `claude_code_errors`, and `pi_rpc_errors`. Adapter sources were not changed, as required.

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

## Verification log

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

The first targeted run's Pi failures were fixture errors, not adapter findings. The final workspace run confirms the three remaining failures are adapter behavior. The required workspace test command exits non-zero because of those failures.

## Acceptance status

1. Not met. The suite runs the same scenario names for all adapters. The `Errors` scenario fails for all three adapters instead of passing or being not applicable.
2. Met. Tests assert protocol events, typed launch or protocol errors, terminal outcomes, routing gaps, and process liveness. They do not assert elapsed time.
3. Met. Each test creates and owns its fake-child temp directory.
4. Met. Probe stdout, stderr, commands, and exit codes are recorded in this directory.
5. Not met. `cargo test --locked --workspace` exits 101 because of the three conformance failures. The `--no-fail-fast` run confirms all pre-existing tests pass.
6. Met. Final Clippy command exits 0.
7. Met for macOS only. Windows remains pending. Run `cargo test --locked --workspace` and `cargo clippy --locked -p orchestrator-agents --all-targets --no-deps` on Windows when available.

Updated: 2026-11-08T10:59:19Z.
