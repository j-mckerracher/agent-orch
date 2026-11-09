# M2-06 reverification at accepted P5 source 693dd9d8

Plan: [docs/technical-implementation-plan.md](../technical-implementation-plan.md) section 9 (persistence and recovery). The earlier evidence in `docs/evidence/m2-06-macos/m2-06-cli-wiring.md` is preserved unchanged.

Tested HEAD: `28280fce1f897179aceb50d3c92afa7796daeb40` on branch `orch/m2-06-reverify-copilot-20261109`, parent `693dd9d8e1726f90b8b1c7a0a0c4c298834ae9bb`. `git diff 693dd9d8 HEAD -- crates Cargo.toml Cargo.lock` is empty: the engine source under test is exactly 693dd9d8. The only commit above it changes `README.md`. Worktree clean for every run below. Host: macOS arm64, rustc 1.95.0. Windows was not run here.

## Criterion to code map (`crates/orchestrator-cli/src/workflow.rs`)

- **SqliteStore, not FileStore:** `open_store` serves `run`, `tasks`, `inspect`, `cancel`, `recover-owner` and `cleanup`. No FileStore use remains.
- **Data-directory lock for mutating commands:** `run_task`, `cancel_task`, `recover_cancellation_owner` and `cleanup_task` call `acquire_data_directory_lock`. `tasks`, `inspect` and `diff` are read-only and do not. Per the M2-06 decision the lock covers short mutation sections, not the agent's lifetime.
- **Process identity at launch:** `capture_process_identity` then `record_session_identity` before the session is marked Running. Failure cancels the agent and fails the task (`identity_capture_failure_fails_run_without_leaving_running_session`).
- **Reconciliation before scheduling:** `reconcile_before_scheduling` runs in `run_task` after the lock and store open and before the task record is created (`prelaunch_crash_reconciles_before_any_second_attempt`, `sigkill_reconciles_dead_task_while_parallel_task_completes`).
- **Refusal while unresolved or ambiguous:** `scheduling_may_resume` over `ReconciliationResolution`. Blocking inputs are a session with no identity, an alive, mismatched or unknown session in a contradictory task state, an unconfirmed Cancelled task, a Running task with no unfinished session, and an open cancellation owner with an unresolved identity. A task whose cancellation owner is observed Alive is skipped, so a normal concurrent run is not blocked.
- **JSON records not migrated:** stated in the module doc and, since this change, in `README.md` ("CLI task store and reconciliation").

Workspace and delivery evidence (`classify_interrupted_task`, `workspace_evidence`, `delivery_evidence`) are not inputs to the CLI gate at 693dd9d8. Whether they must be is pending a parent ruling; this document does not decide it.

## Results (all exit codes are cargo's own, taken from the producer)

| Command | Exit | Counts |
|---|---|---|
| `cargo test --locked -p orchestrator-cli --lib` | 0 | 22 passed |
| `cargo test --locked -p orchestrator-cli` | 0 | lib 22, main 0, cleanup_safety 4, cli 20, multi_task_review 6, portable_acceptance 2, recovery 1; 55 passed, 0 failed, 0 ignored |
| `cargo test --locked -p orchestrator-cli --features fault-injection --test cleanup_safety --test recovery` | 0 | cleanup_safety 5, recovery 7; 12 passed, 0 failed |
| `cargo clippy --locked -p orchestrator-core -p orchestrator-store -p orchestrator-runtime -p orchestrator-git -p orchestrator-cli --all-targets --no-deps` | 0 | 4 warnings, the known baseline: 1 `collapsible_if` at `workflow.rs:928`, 3 unused-code in `tests/recovery.rs`; none new |

`cargo test --locked --workspace` was not run in this pass (disk budget); it is reserved for the final integrated candidate.

## Windows gap

`tests/cli.rs`, `tests/recovery.rs` and `tests/multi_task_review.rs` are `#![cfg(unix)]`; they compile to nothing on Windows. Native evidence `37bb201f8c0ff8a0330c90b9885e0feb16715100` proves only exact source 693dd9d8. This candidate changes only `README.md` relative to it, but the new SHA has no native run, and no Windows result is inferred from these macOS runs.
