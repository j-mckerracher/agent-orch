# M2-06 gate wiring: workspace and delivery evidence (macOS)

Plan: [docs/technical-implementation-plan.md](../technical-implementation-plan.md) section 9 (lock, process identity, inspect worktrees and Git, reconcile delivery, classify, then resume). This supersedes the "pending ruling" note in `m2-06-reverify-693-macos.md`, which is preserved unchanged, as is `m2-06-macos/m2-06-cli-wiring.md`.

Candidate source commit: `ce01372e`, parent `c0f83c1f` (chain `693dd9d8` -> `28280fce` -> `e6541011` -> `aee53581` -> `c0f83c1f` -> `ce01372e`); a later docs-only commit may sit above it and changes no source, branch `orch/m2-06-reverify-copilot-20261109`. Source change: `crates/orchestrator-cli/src/workflow.rs` and `crates/orchestrator-cli/tests/cli.rs` only. Library crates are untouched. Host: macOS arm64, rustc 1.95.0.

## Gate policy (coordinator ruling, implemented read-only)

| Evidence | Population | Result |
|---|---|---|
| Foreign or Unknown workspace (not a registered worktree, no recorded workspace row, repository not inspectable) | session whose process is proven gone, this pass | refuse; session, attempt and task untouched, re-inspected next run |
| Dirty, Missing, BranchMoved workspace | same | worktree kept; advisory on stderr; existing Failed transition. Not completion, not cleanup authority, Dirty is never relabelled Matching |
| Partial or Ambiguous delivery; Confirmed delivery on a non-Succeeded task | any task with persisted delivery steps | refuse |
| Consistent Confirmed on Succeeded, NotApplied, no steps | any task | no block |
| Dirty, gone or moved retained worktree | already-settled terminal task | no block |
| Live owner | any | existing behaviour, not blocked |
| Pending/Running attempt with no unfinished session of its own | any task not live-owned | refuse (Ambiguous); a task with no attempt (planned) and a settled CandidateReady attempt do not block |

Delivery steps come from `store.all_delivery_steps()`; workspaces from `store.list_workspaces()`; the repository is the task's `project_id`. Inputs are adapted with `reconcile_workspaces`, `check_worktree_safe_to_delete`, `reconcile_delivery_steps` and classified with `workspace_evidence`, `delivery_evidence` and `classify_interrupted_task`. Nothing writes Git or delivery state.

## Owner results on ce01372e

Artifacts (per attempt, outside the checkout, with before/after HEAD and status, command, times, complete stdout/stderr, producer exit) are under `.agent-worktrees/m2-06-evidence/` in the coordinator root:

| Artifact | Command | Exit | Counts |
|---|---|---|---|
| `20261109T102032Z-66783-owner-cli-full-ce01372e` | `cargo test --locked -p orchestrator-cli` | 0 | lib 32, main 0, cleanup_safety 4, cli 20, multi_task_review 6, portable_acceptance 2, recovery 1; 65 passed, 0 failed |
| `20261109T102056Z-70643-owner-fault-ce01372e` | `... --features fault-injection --test cleanup_safety --test recovery` | 0 | cleanup_safety 5, recovery 7 |
| `20261109T102108Z-72393-owner-clippy-ce01372e` | `cargo clippy --locked -p orchestrator-core -p orchestrator-store -p orchestrator-runtime -p orchestrator-git -p orchestrator-cli --all-targets --no-deps` | 0 | 4 baseline warnings (1 `collapsible_if` in `workflow.rs`, 3 unused-code in `tests/recovery.rs`), none new |

Every run touched all `.rs`/`Cargo.toml` mtimes first and shows `Compiling orchestrator-{core,git,runtime,store,agents,cli}` in stderr, because a shared `CARGO_TARGET_DIR` can otherwise reuse another worktree's newer artifacts (found by QA). Earlier unsaved/WIP runs and `c0f83c1f` runs (cli 63, same fault and clippy results) are preserved as artifacts, not evidence for this SHA.

New inline tests (10, 8 gate + 2 attempt audit; the last two are `pending_attempt_without_a_session_blocks_but_a_planned_task_does_not` and `settled_candidate_ready_attempt_does_not_block_scheduling`): `delivery_blocks_only_partial_ambiguous_or_confirmed_on_unsucceeded_task`, `settled_failed_task_with_unlanded_delivery_and_absent_workspace_does_not_block`, `confirmed_delivery_on_a_failed_task_blocks_and_preserves_everything`, `ambiguous_delivery_blocks_scheduling_and_is_not_rewritten`, `interrupted_task_with_foreign_workspace_stays_refused_and_unmodified` (two consecutive runs), `interrupted_task_without_a_recorded_workspace_is_unknown_and_refused`, `interrupted_task_with_dirty_registered_worktree_keeps_it_and_does_not_block`, `interrupted_task_with_confirmed_delivery_is_refused_before_any_state_write`. Fixtures that seed a dead session (`cli.rs::seed_running_session`, the inline audited-recovery test) now record a Missing-path workspace row; no assertion was weakened.

`cargo test --locked --workspace` is reserved for the final integrated candidate. QA's black-box `tests/engine_reconciliation_gate.rs` (RED on 693dd9d8, GREEN on the candidate) is recorded by QA with its own artifacts.

## Windows gap

The new inline gate tests that need a dead process are `#[cfg(unix)]`; `cli.rs`, `recovery.rs` and `multi_task_review.rs` are `#![cfg(unix)]`. Native evidence `37bb201f` proves only exact 693dd9d8. This candidate changes source, so a fresh native Windows run on its exact SHA is required; nothing is inferred from macOS.
