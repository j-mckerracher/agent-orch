# M2-05 macOS verification

Updated: 2026-11-08T11:40:19+00:00.
Base: `66ffe06b0e271bec8c72ae7a78d593b979858f45`.

The stale-lock takeover change in `crates/orchestrator-store/src/data_lock.rs` was made under the coordinator's scope amendment. New lock records store PID, process start identity, and token. The CLI takes over verified stale locks before reconciliation. Legacy `owner=` records remain readable.

## Commands and results

| Command | Exit | Result |
|---|---:|---|
| `cargo test --locked --workspace` before edits | 0 | 249 passed, 1 ignored, 33 suites |
| `cargo test --locked --workspace` after changes | 0 | 253 passed, 1 ignored, 34 suites, no warnings |
| `cargo test --locked -p orchestrator-store --test data_lock` | 0 | 7 passed, 1 ignored |
| `cargo test --locked -p orchestrator-cli --features fault-injection` | 0 | 28 passed, 5 suites |
| `cargo test --locked -p orchestrator-cli --features fault-injection --test recovery -- --nocapture` | 0 | 5 passed, 1 suite |
| `cargo test --locked -p orchestrator-cli --test recovery two_parallel_tasks_preserve_disjoint_worktrees_for_five_runs -- --nocapture` | 0 | 1 passed; five repeat outcomes recorded below |
| `cargo clippy --locked -p orchestrator-cli --all-targets --no-deps` | 0 | passed |
| `cargo clippy --locked -p orchestrator-cli --all-targets --no-deps --features fault-injection` | 0 | passed |
| `cargo clippy --locked -p orchestrator-store --all-targets --no-deps` | 0 | passed |
| `rustfmt --edition 2021 --check crates/orchestrator-cli/src/workflow.rs crates/orchestrator-cli/tests/recovery.rs crates/orchestrator-store/src/data_lock.rs crates/orchestrator-store/tests/data_lock.rs` | 0 | passed |
| `git diff --check` | 0 | passed |
| `cargo fmt --all -- --check` | 1 | Reported formatting diffs in touched and unchanged files. Ran rustfmt only on touched Rust files and left unrelated workspace formatting unchanged. |

## Concurrent repeat outcomes

All runs used fresh temp repositories and data directories. Each run completed both tasks in distinct worktrees. Both task records reached `Verifying`, and each disjoint file remained present.

1. Run 1 of 5. Both tasks `Verifying`; disjoint files preserved.
2. Run 2 of 5. Both tasks `Verifying`; disjoint files preserved.
3. Run 3 of 5. Both tasks `Verifying`; disjoint files preserved.
4. Run 4 of 5. Both tasks `Verifying`; disjoint files preserved.
5. Run 5 of 5. Both tasks `Verifying`; disjoint files preserved.

The injected child-process failure test recorded `MarkInterrupted`, then persisted task state `Failed`, attempt state `Failed`, and session state `Exited`. The other task completed. The crashed task's uncommitted file checksum and bytes matched before and after reconciliation.

The prelaunch crash test killed CLI after persisting the attempt and spawning session, while CLI held the data lock. The next run reclaimed the dead owner's lock, entered reconciliation, and reported the orphan session's missing process identity before creating another task or attempt.

The runtime fault test made SQLite abort the journal insert inside the transition transaction. Task state and journal remained unchanged. After clearing the trigger, `Admit` produced one state transition and one event. The audit test checked every task event against the global journal and asserted contiguous sequence numbers.

Windows and power-loss durability were not tested. Feature-gated event injection uses the system `sqlite3` executable. No Cargo dependencies were added.
