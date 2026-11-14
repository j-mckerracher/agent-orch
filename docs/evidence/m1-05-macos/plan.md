# M1-05 verification plan

Updated: 2026-11-07T23:12:48-07:00.

## Feature workflow

1. `how` over the affected subsystem. Read the CLI dispatch, run and cancel lifecycle, cleanup guards, and existing black-box tests.
2. `architect` for parallel design exploration. Skipping stays as `architect skipped: <reason>`; do not fold the design decision silently into implementation. `architect skipped: test-only changes extend existing cases; no new production design is planned.`
3. Write the throughput checkpoint as four todo items. A dimension that genuinely does not apply (single file, no fan-out) keeps its item with `n/a: <reason>` rather than being dropped:
   - **Blocking first steps.** Baseline and source inspection precede any edit.
   - **Independent workstreams.** Tests and any source fix share CLI behavior, so implementation stays serial.
   - **Shared mutable state.** One worker owns this branch and evidence directory; no concurrent writers.
   - **Smallest safe decomposition.** One owner is required because tests, a possible CLI fix, runtime evidence, and verification are coupled.
4. Delegate code-writing through the active adapter's **Bounded session** protocol with canonical `implementer` role and a specific scope (file paths, named data shape and operations, tests it owns, explicit no-touch zones). You stay the lead: review every line and own integration. One writer for tightly coupled code; parallel writers only on structurally disjoint modules with isolated worktrees. `Skip: this delegated worker cannot start child tasks under the pstack-pi task contract.`
5. Verify on the matching surface. "Inconclusive" or wrong-surface is not a pass; flag it.
6. Rebase into small, ordered commits; stack follow-ups. `Skip: assignment forbids rebase and requires one commit on this branch.`
   Use the **sequence-verifiable-units** principle skill, building, verifying, and committing each small unit before the next.
7. If the design is contested, `interrogate` before shipping. `Skip: no contested design or production-code change is expected.`
8. Run **Opening a PR**. `Skip: assignment forbids push and requests a branch commit only.`

## Work sequence

1. Baseline completed with `cargo test --locked --workspace`: 119 tests passed across 19 suites.
2. Strengthen the existing dirty-primary run test. Capture `HEAD`, `git status --porcelain`, and bytes for a modified tracked file and an untracked file before and after `run`.
3. Strengthen cleanup rejection assertions. Compare the clean unlanded candidate and dirty worktree state before and after each rejected cleanup. Keep existing cancellation coverage because it already checks cancelled state, reaped child, and retained worktree.
4. Run the scenario suite, then run installed Codex once against a unique disposable repository under this evidence directory. Keep the primary repository dirty and save command output, exit status, JSONL, and before/after snapshots.
5. Run workspace tests, CLI clippy, and `git diff --check`. Commit only in this worktree with the required trailer. Confirm clean status.

## Boundaries

Only `crates/orchestrator-cli/**` and `docs/evidence/m1-05-macos/**` are writable. Do not access the named primary checkout, other worktrees, `/private/tmp`, or the board. Do not use force cleanup or bypass-mode flags. Windows remains unverified.
