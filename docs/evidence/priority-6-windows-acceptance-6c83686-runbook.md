# Priority 6 (M2-06 engine reconciliation gate and M3-04 delivery) native Windows acceptance runbook (candidate 6c83686c3c3cedf07f8fab50f4764d88f643a2c6)

This runbook is adapted from the approved Priority 5 harness
`docs/evidence/priority-5-windows-acceptance-693dd9d-runbook.md` at docs commit
`d976dc40a636c3462e71fb61cd6da75a1c62c72c`, which derives from the M1-04 harness
`docs/evidence/m1-04-windows-acceptance-46497ac-runbook.md` at `ff8a2d3a5a7a8fa7d16cde7849ccb0bd6d9a303e`.
Those files are immutable git objects and are absent from the source checkout. The approved helper
blocks (`Protect-Text`, `Invoke-Bounded`, the survivor scan, the publication scan) are reused unchanged. No
new framework is introduced. The section "What changed from the approved harness" lists every
difference.

If the candidate changes, copy this runbook and replace the pinned SHAs, the branch names, the
counts, the test names, and the file name before anyone runs it. Do not run an old runbook
against a new candidate.

## Status of this run

**Native Windows acceptance of this candidate is unproven. No native Windows run of it exists,
and this document does not claim one.** The accepted Priority 5 native evidence
`37bb201f8c0ff8a0330c90b9885e0feb16715100` (14 of 14 gates, workspace 385 passed, 0 failed, 1 ignored)
proves **only** source `693dd9d8e1726f90b8b1c7a0a0c4c298834ae9bb`. It is not evidence for this
candidate and none of its results is reused.

This run records how the candidate behaves on native Windows. It does not close M2-06 or M3-04.
Both stay open until independent Mac, source, and native acceptance, and the coordinator decides
acceptance. Nothing here is proof from macOS, from a cross-compile, or from a review.

The items under test, in the plan's words:

- **M2-06.** Move the CLI engine from the JSON FileStore to SqliteStore for task execution, acquire the
  data-directory lock for mutating commands, persist session process identity at launch, run
  reconciliation before any new attempt is scheduled, and refuse to schedule while any record is
  unresolved or ambiguous. The existing JSON records are not migrated.
- **M3-04.** Require authorization before integration, serialize delivery to a destination, revalidate
  against current state and retain unsafe-to-delete worktrees.

Contract of the M2-06 reconciliation gate, as ruled by the coordinator (the criteria the new
`engine_reconciliation_gate` tests must check, not just an exit code or a count). The gate inspects
an interrupted record's workspace and delivery evidence **before** it writes a `Failed`
transition. It blocks `Foreign`, `Unknown`, and unresolved (`Partial` or `Ambiguous`) delivery
journals. It permits a known, owned `Dirty`, `Missing`, or `BranchMoved` retained failure and shows
diagnostics. Settled terminal records and live-owner records are exempt unless an unresolved
journal exists. Delivery journals are read-only across all tasks. M3-06 write and resume scope is
**not** included, and no M3-06 behavior is claimed.

What the coordinator reported about the exact source (not re-run here, not Windows evidence).

**Measured on macOS at exactly `6c83686c3c3cedf07f8fab50f4764d88f643a2c6`** (reported by the
coordinator and integrator, with a clean `HEAD`, tree, and status before and after every producer;
raw private logs with a 20-file hash manifest are held by the coordinator and are not published
here). Package-clean CLI, Git, and store runs exited 0. `cargo test --locked --workspace
--no-fail-fast` exited 0 with **509 passed, 0 failed, 1 ignored**. Per package:

- CLI 87 passed: lib 32, `cleanup_safety` 4, `cli` 20, `engine_reconciliation_gate` 22,
  `multi_task_review` 6, `portable_acceptance` 2, `recovery` 1.
- Git 74 passed: lib 33, `delivery` 21, `delivery_acceptance_contract` 6, `delivery_reconcile` 7,
  `reconcile` 6, `scoped_diff` 1.
- Store 89 passed and 1 ignored: lib 32, approvals 9, `attempt_history` 2, `cancellation_owner` 13,
  `data_lock` 7 (+1 ignored), `delivery` 11, `delivery_acceptance_contract` 3, `lock_identity` 4,
  `reconnectable_events` 2, `sqlite_reconcile` 5, `verification` 1.
- Fault-injection: `cargo test --locked -p orchestrator-cli --features fault-injection --test
  cleanup_safety --test recovery` exited 0, 5 and 7 passed.
- Scoped five-crate `clippy --all-targets --no-deps` exited 0 with the 4 known baseline warnings.

These Mac numbers include Unix-only tests and are **not** Windows expectations or evidence.

**Historical, superseded.** The two M2 notes in this source, `docs/evidence/m2-06-gate-wiring-macos.md`
and `docs/evidence/m2-06-reverify-693-macos.md`, describe earlier intermediate commits (`693dd9d8…`,
`ce01372e…`) and contain statements such as "not validated" and "reserved for the final integrated
candidate". They are historical, are superseded by the exact combined Mac results above, and are
neither a current failure nor a blocker. Their macOS counts do not describe this source.

**Calculated, not measured.** Every Windows count in this document is a static-source expectation
(see the derivation). **Unproven.** Native Windows behavior of this source. Coordinator status:
Priority 6 items M2-06 and M3-04 remain `in_progress` pending this exact source's native Windows
acceptance. Priority 5 was accepted at `693dd9d8…`, and the native evidence `37bb201f…` covers only
that commit. No Priority 7 work is part of this run.

## Candidate

- Repository: `https://github.com/j-mckerracher/agent-orch.git`
- Source branch (the coordinator publishes it after review): `orch/priority-6-windows-acceptance-6c83686`. If the dispatch
  message gives another name, replace `$SourceBranch` and its name check in step 1.
- Source commit: `6c83686c3c3cedf07f8fab50f4764d88f643a2c6`
- Accepted baseline (must be an ancestor): `693dd9d8e1726f90b8b1c7a0a0c4c298834ae9bb`
- Evidence branch (you create it): `evidence/priority-6-windows-acceptance-6c83686-<RunId>`
- Runbook: file `docs/evidence/priority-6-windows-acceptance-6c83686-runbook.md` on
  `docs/priority-6-windows-acceptance-6c83686`. The coordinator gives the actual branch, the full runbook commit, and the URL in
  the dispatch message. The runbook is not part of the source commit. The declared runbook commit
  is operator-declared and is not byte proof. The measured source `HEAD` decides the pin, and an
  independent publisher base is allowed. If the source branch tip is later than the source
  commit, test the pinned commit and report the tip.

Test only the exact source commit. Before any other command, `HEAD` must equal the full source
commit. If the branch does not contain the commit, stop and report the mismatch.

### Ancestry and what the candidate changes

`6c83686c3c3cedf07f8fab50f4764d88f643a2c6` (tree `34da44b1669378fb5045a1f149805a8be98ac87b`) is 13 commits
above the accepted baseline `693dd9d8e1726f90b8b1c7a0a0c4c298834ae9bb`, in order:

- 10 M2-06 commits ending at `1d66d648e08e335b45aeecade36acb51e2930454`: `28280fce`, `e6541011`,
  `aee53581` (docs and the earlier reverification notes), `c0f83c1f`, `ce01372e` (the engine
  gate source), `70d64bfb` (docs), `d80c58fd`, `bf676fc6` (engine tests), `b9ec2816` (the live-owner
  journal fix), and `1d66d648` (its tests).
- 3 M3-04 commits that were cherry-picked (patch-equivalent to the owner's test-only work at
  `37344247b3c671e0dcc676e153f662b0e05670db`): `f61a410b`, `74585b9d`, and `6c83686c`, the candidate.

The integrator reported that ancestry, patch-ID, trailer, blob, and union checks passed. The M3 test
files are blob-identical to `37344247…`, and the engine file blob `67bed0953a1c78bee70d7dd9c9e18cc6e4a73c33`
is unchanged. The integrator branch is `task/integ5-m2-06-m3-04-20261109T104112Z-36036`.

`693dd9d8…` is the only ancestry pin. The earlier pins `46497ac`, `a017134e`, and `c1be4bba`, and any
raw M2 or M3 item commit that was cherry-picked, are history and are not checked. The two gates
`range-count` and `range-files` are consistency gates, not patch-equivalence proof.

Step 1 checks that the commit count is 13 and that the files changed from the baseline are exactly
these 9. A different set is an `INCONCLUSIVE`-level finding to report, not an edit.

| File changed from the baseline | Change |
|---|---|
| `README.md` | M2-06 note: JSON task records are not migrated, CLI lock and reconciliation gate (docs only) |
| `crates/orchestrator-cli/src/workflow.rs` | M2-06 engine: SQLite store, data-directory lock, launch identity, reconciliation gate; 10 new inline tests (6 portable, 4 `cfg(unix)`) |
| `crates/orchestrator-cli/tests/cli.rs` | fixture adds a workspace row (`cfg(unix)` file, zero Windows tests) |
| `crates/orchestrator-cli/tests/engine_reconciliation_gate.rs` | new, 22 portable engine tests |
| `crates/orchestrator-git/tests/delivery.rs` | M3-04: 8 added tests (13 to 21) |
| `crates/orchestrator-git/tests/delivery_acceptance_contract.rs` | new M3-04, 6 tests |
| `crates/orchestrator-store/tests/delivery_acceptance_contract.rs` | new M3-04, 3 tests |
| `docs/evidence/m2-06-gate-wiring-macos.md` | historical M2 owner note (docs only) |
| `docs/evidence/m2-06-reverify-693-macos.md` | historical M2 reverification note (docs only) |

No Rust source other than `workflow.rs` changed; `Cargo.toml` and `Cargo.lock` are unchanged. M3-04
is test-only above `693dd9d8…`.

Primer and plan at the pinned commit:
[orchestration primer](https://github.com/j-mckerracher/agent-orch/blob/6c83686c3c3cedf07f8fab50f4764d88f643a2c6/orch-impl-m.md)
and
[implementation plan](https://github.com/j-mckerracher/agent-orch/blob/6c83686c3c3cedf07f8fab50f4764d88f643a2c6/docs/technical-implementation-plan.md).
Follow the primer's skill requirements. Report any unavailable guidance and any setting that you
cannot observe as unverified.

## Assignment

Run the checks below on native Windows. Publish the evidence on a separate branch. Do not edit the
candidate source or `board.json`. Do not request operator action beyond running this runbook. Do not
open a pull request or merge.

The user requires GitHub Copilot subagents configured for Claude Sonnet 5.5 with high reasoning,
including descendants. This replaces the worker model setting in the primer for this run. Do not
start an OMP worker. The Windows host may run the commands below itself even if its own subagent
runtime cannot select Sonnet 5.5 with high reasoning, because the checks are shell commands whose
result does not depend on the model. If the host cannot select or observe that setting, write
`agent runtime: <what you observed>; Sonnet 5.5/high unavailable or unverified`. Never state that a
Sonnet agent ran when you did not observe it.

**PowerShell 7 (`pwsh`) only.** Do not use Windows PowerShell 5.1. The commands below do not work in
it. The host needs Windows 11 or later, Git for Windows, and the MSVC Rust toolchain. `rustc` must be
on `PATH` for the tests that compile a fake agent. Use a short checkout path and `core.longpaths` as
the blocks do.

## Checks

Each check runs once. Each records its command, environment summary, exit code, duration, full
stdout and stderr, and every `running N tests` and `test result` line.

### How the Windows counts were derived (calculated, not measured)

Every count below is **calculated from the source at the pinned commit. None is measured on Windows
for this candidate.** Mac counts are not Windows counts, because many tests are `cfg(unix)`. Each
number maps to one argv, package, binary, selector, and platform. Do not compare numbers from
different selectors or platforms. In particular, `git-delivery-lib` (`--lib delivery`) is a
**filtered** selector, not a whole-lib total.

The Windows counts below were calculated by **static source reading**, not by a build:

1. M3-04 (frozen reviewed input `37344247b3c671e0dcc676e153f662b0e05670db`, test-only above
   `693dd9d8…`): `crates/orchestrator-git/tests/delivery.rs` gains 8 tests (13 to 21), and two new
   files `delivery_acceptance_contract.rs` (git: 6 tests, store: 3 tests) are added. None of the 17
   has `cfg`, `ignore`, `unix`, or `windows` gating, so all 17 are portable. Git package: 59 + 14 = 73
   (binaries 21 + 7 + 32 + 6 + 1 + 6).
2. M2-06 (frozen reviewed input `1d66d648e08e335b45aeecade36acb51e2930454`, engine file blob
   `67bed0953a1c78bee70d7dd9c9e18cc6e4a73c33`): the new file
   `crates/orchestrator-cli/tests/engine_reconciliation_gate.rs` has 22 `#[test]` functions and no
   file-level or test-level `cfg`, so 22 are expected on Windows. `workflow.rs` gains 10 inline tests:
   4 carry `#[cfg(unix)]` and 6 do not, so the lib gains 6 on Windows (the 4 Unix-only ones are not
   Windows evidence). `cli.rs` gains no test.
3. Method check: the same reader (a function that pairs each `#[test]` with the `#[cfg]` and
   `#[ignore]` attributes above it) was run on `crates/orchestrator-cli/src` at `693dd9d8…`
   and gives 22 tests, of which 11 are not `cfg(unix)`. That equals the Mac measured lib count 22
   and the native Windows measured `cli-lib` count 11 of the accepted evidence `37bb201f…`. At
   `1d66d648…` it gives 32 (the same as the macOS lib count 32 in the historical M2 owner note, and as
   the measured combined Mac `lib 32` at `6c83686c…`) and 17 for Windows. The reader gives the same
   22, 32, 17, and the same 22, 6, 3 engine and M3 test counts at `6c83686c…`, whose `src` and engine
   file equal the frozen inputs. This is a counting
   check only. **No Rust `cfg` expansion, `cargo check`, or build was run for this candidate**, so
   an attribute that the reader cannot see (for example a module-level `cfg`) would be missed. The
   reader found no `cfg` on the `tests` modules.
4. Calculated Windows totals above `693dd9d8…` (385 passed, 1 ignored): 385 + 17 (M3) + 6 (inline) +
   22 (engine) = **430 passed, 1 ignored (431 listed)**. The 22 engine tests, the 6 inline tests, and
   the 17 M3 tests are the only additions in the union of the two frozen inputs. This is arithmetic,
   not a measured result of any union build. The integrator reported exactly 9 changed paths above 693dd9d8… and no test file beyond the frozen M2 and M3 inputs; the measured Mac workspace total at this source is 509 passed, 0 failed, 1 ignored.

Evidence classes in this document, kept apart: (a) **measured** results come only from the coordinator's
Mac QA of the exact final union and are listed in the Status section once supplied, (b)
**historical** notes (for example the two `docs/evidence/m2-06-*.md` files that M2 added, which
describe earlier intermediate commits, an unvalidated fix at their time, and QA artifacts that are
not in this checkout) are not evidence for the final source, (c) the **calculated** Windows
expectations in this section, and (d) the **unproven** native Windows behavior, which only this
run can establish.

The counts of the 14 retained gates are re-derived from this source. They are **not** copied from the
Priority 5 runbook. For reference only, Priority 5 expected 385 workspace passed, 59 git-package
(13 + 7 + 32 + 6 + 1), 4 `git-delivery-lib`, 7 store `data_lock` integration (plus 1 ignored), 7
store `data_lock` lib, 3 `lock_identity_contract`, 14 `reconciliation_contract`, 28 `reconcile`, 1
`ipc-token-owner`, 11 `cli-lib`, 3 `cli-cleanup-units`, 1 `cli-session-gone-guard`, 3
`cli-cleanup-safety`, and 2 `portable_acceptance`, all at `693dd9d8…` only. Arithmetic projections such
as a Mac 477 from 460 are not observed proof.

A different measured Windows total is `INCONCLUSIVE` with a note, never an automatic pass or an edit of
this runbook. The coordinator then reconciles the number.

### Check table

| Check name | Command (after `cargo`) | Expected (calculated) |
|---|---|---|
| `workspace-tests` | `test --locked --workspace --no-fail-fast` | Exit 0. 430 passed, 0 failed, 1 ignored. Every named test above reports `ok`. Binaries with `running 0 tests` are normal. |
| `git-package` | `test --locked -p orchestrator-git` | 73 passed over six test binaries (21 + 7 + 32 + 6 + 1 + 6). |
| `git-delivery-lib` | `test --locked -p orchestrator-git --lib delivery` | One binary. 4 passed, 0 ignored. A **filtered** selector (the other lib tests are filtered out), not a lib total. |
| `store-data-lock-integration` | `test --locked -p orchestrator-store --test data_lock` | One binary. `running 8 tests`, 7 passed, 1 ignored (`lock_child_process_helper`). |
| `store-data-lock-lib` | `test --locked -p orchestrator-store --lib data_lock` | One binary. 7 passed, 0 ignored. |
| `store-lock-identity-contract` | `test --locked -p orchestrator-store --test lock_identity_contract -- --nocapture` | One binary. 3 passed. `--nocapture` makes the standalone stderr record the `directory_at_the_lock_path` diagnostic (`acquire=<outcome> directory_kept=<bool> marker_intact=<bool>`). On PASS it must show `directory_kept=true marker_intact=true` and `acquire=HolderUnknown` or `acquire=Io(..)`. `acquire=Ok(lock)` is a FAIL for the reviewer. The verdict code does not parse it. |
| `runtime-reconciliation-contract` | `test --locked -p orchestrator-runtime --test reconciliation_contract` | One binary. 14 passed. |
| `runtime-reconcile` | `test --locked -p orchestrator-runtime --test reconcile` | One binary. 28 passed. |
| `ipc-token-owner` | `test --locked -p orchestrator-ipc --lib startup_replaces_only_regular_stale_token` | One binary. 1 passed. |
| `cli-lib` | `test --locked -p orchestrator-cli --lib` | One binary. 17 passed, 0 ignored (Windows: the 4 Unix-only M2-06 inline gate tests are not built). |
| `cli-cleanup-units` | `test --locked -p orchestrator-cli --lib cleanup` | One binary. 3 passed. |
| `cli-session-gone-guard` | `test --locked -p orchestrator-cli --lib session_gone_guard_refuses_missing_alive_and_unknown_classifications` | One binary. 1 passed. Separate: `--lib cleanup` does not match it. |
| `cli-cleanup-safety` | `test --locked -p orchestrator-cli --test cleanup_safety` | One binary. 3 passed. Isolated temp directory and survivor scan. |
| `cli-acceptance` | `test --locked -p orchestrator-cli --test portable_acceptance` | One binary. 2 passed, 0 ignored. Isolated, survivor scan, 10-minute limit. |
| `git-delivery-acceptance-contract` | `test --locked -p orchestrator-git --test delivery_acceptance_contract` | One binary. 6 passed. New. |
| `store-delivery-acceptance-contract` | `test --locked -p orchestrator-store --test delivery_acceptance_contract` | One binary. 3 passed. New. |
| `cli-engine-reconciliation-gate` | `test --locked -p orchestrator-cli --test engine_reconciliation_gate` | One binary. 22 passed, 0 ignored, no Unix gating. New. Isolated, survivor scan, `rustc` on `PATH`, 10-minute limit. |

All 17 checks count in the overall result. A missing verdict is a `FAIL`. A filter that runs
zero tests is never a pass: any `Single` check that reports `running 0 tests`, or more than one
test binary, is `INCONCLUSIVE`.

The existing stderr of `store-lock-identity-contract` records a diagnostic such as
`Io(OS code 5 PermissionDenied) directory_kept=true marker_intact=true`. That is the `Io` outcome
the contract permits on Windows. Keep `-- --nocapture` as in Priority 5: an ordinary passing test capture
hides it and only this standalone gate records it. The verdict code does not parse stderr.
`acquire=Ok(lock)` is a FAIL for the reviewer.

### Required named tests

These must report `ok` (the harness checks each by name in its own check and again in
`workspace-tests`). If the candidate renamed a test, the check reports `INCONCLUSIVE` with a note.
Do not edit the runbook. Names for the retained gates come from the arrays in step 3.

- New M3-04 Git delivery (`delivery.rs`, 8 added tests, in `git-package` and `workspace-tests`), and
  the 6 `delivery_acceptance_contract` Git tests and 3 store `delivery_acceptance_contract` tests:
  the exact names are in the arrays `$GitDeliveryNewTests`, `$GitDeliveryAcceptanceTests`, and
  `$StoreDeliveryAcceptanceTests` in step 3. They check authorization before integration,
  serialized delivery to one destination, revalidation against current state, and retained
  unsafe-to-delete worktrees, in throwaway local repositories.
- New M2-06 engine gate, from `crates/orchestrator-cli/tests/engine_reconciliation_gate.rs`, which has
  no file-level `cfg(unix)` and must run on Windows. The 22 test names are in the array `$CliEngineReconciliationTests` in step 3 (blob
  `67bed0953a1c78bee70d7dd9c9e18cc6e4a73c33` at `1d66d648…`). They fall into these groups:
  launch identity persisted in SQLite with legacy JSON left unmigrated; the data-directory lock
  (mutating commands refused while it is held, readers unaffected, a live parallel run neither blocks
  scheduling nor holds it); unidentified or unfinished sessions and attempts refuse scheduling;
  planned, admitted, and settled candidate-ready tasks do not block; interrupted tasks with a known
  owned workspace resume scheduling and keep evidence, while foreign, uninspectable, or unrecorded
  workspaces refuse; partial, ambiguous, and confirmed-on-failed delivery journals refuse, for
  settled, interrupted, **and live-owned** tasks (3 live-owner refusals), while consistent journals
  do not block, and a live owner with a consistent not-applied journal does not block (1 live-owner
  positive).
  The check passes only when each of those names reports `ok` **and** the test body checks the
  contract above (blocking `Foreign`, `Unknown`, `Partial`, and `Ambiguous`, permitting a known owned
  `Dirty`, `Missing`, or `BranchMoved`, inspecting before the `Failed` write, and exempting settled
  and live-owner records). Exit 0 and a count alone are not acceptance. A reviewer reads
  `cli-engine-reconciliation-gate.stdout.txt` for each named test and the source for the criteria.
- Retained M1 and M2 names (unchanged intent): the `cleanup_safety` tests, the non-Unix cleanup unit
  tests, the session-gone guard, `run_produces_isolated_diff_and_cleanup_preserves_primary`,
  `cancel_terminates_agent_and_descendant`, the runtime `reconciliation_contract` and `reconcile`
  arrays, the store `lock_identity_contract` and `data_lock` tests, the Git `delivery` lib tests, and
  `startup_replaces_only_regular_stale_token`.

### Coverage gaps (not run on Windows, not claimed)

- Unix-only tests are not built on Windows and are not Windows evidence, for example the orphan-tree
  test, the fault-injection startup test, the Unix process-group survivor test, and the Unix symlink
  test. Their macOS acceptance is separate.
- `cli.rs`, `recovery.rs`, `multi_task_review.rs` in `orchestrator-cli` and `daemon.rs` in
  `orchestrator-daemon` are file-wide `cfg(unix)` and compile to zero tests on Windows. The
  Windows-native engine coverage is only the new `engine_reconciliation_gate`, `portable_acceptance`,
  and `cleanup_safety`. There is no Windows coverage of crash recovery (`recover-owner`),
  multi-task review, or the daemon.
- M3-06 write and resume scope is not in this candidate and is not claimed. The existing JSON records
  are not migrated by M2-06, and no migration is tested.
- No delivery to a real remote ran. The delivery tests use throwaway local repositories.
- The orphan-survival behavior on Windows rests on the job-object contract read from source and on
  `cancel_terminates_agent_and_descendant`. It is not a native claim.

## Safety rules

- Use a new clone in a unique directory under `$env:TEMP`.
- Do not change the user's primary checkout.
- Do not install tools or change credentials.
- Do not use the agent bus or copy repository code to another repository.
- Do not change source files, manifests, lockfiles, or `board.json`.
- Do not delete worktrees, directories, or failed attempts.
- Do not use `git reset`, `git clean`, `git push --force`, or `git merge` in your own checkouts. (The
  `cli-*` acceptance tests run `git merge --ff-only` inside a throwaway repository that they create
  themselves. That is expected.)
- Never stop a process by name or by a bare PID. The harness ends only the process tree it started,
  when that process times out. If a check leaves a process, report its PID, path, and command line. Do
  not stop it.
- Do not publish credentials, agent memory, session files, or unrelated files.
- Do not record remote URLs or the Windows profile name in any evidence file. The `Protect-Text`
  helper below replaces every URL, including scp-style `user@host:path` forms, with `<url-redacted>`
  and the directory name after a `Users` path segment with `<user>`. Never print `git remote -v`,
  `git config --list`, or a clone URL.

## 1. Prepare the checkout

Use native Windows with PowerShell 7 (`pwsh`) only. Open a new `pwsh` session. Run each block in that
session. Keep the session open for all steps.

### Use the published runbook only

1. The coordinator's dispatch message gives the full 40-character commit of this runbook file
   (`RunbookCommit`) and the branch it is on. Use only that commit. If the dispatch message does not
   give it, stop and ask. Do not run a copy, an older variant, or text pasted from chat.
2. Take every block from the file at that commit, for example with `git show
   <RunbookCommit>:docs/evidence/priority-6-windows-acceptance-6c83686-runbook.md` in a clone you do
   not otherwise use. Do not edit a block except as step 1 allows (`$SourceBranch` and its name
   check, and `$RunbookCommit`).
3. Set `$RunbookCommit` in the first block to that full commit. The block stops when it is empty. The
   value is recorded in `runbook-identity.txt` as **operator-declared**. It is not proof of the bytes
   that you ran; step 2 above is what keeps the bytes honest.
4. The starting revision of the test checkout is the source commit that the first block pins and
   checks, not the runbook commit.

```powershell
$env:GIT_TERMINAL_PROMPT = '0'
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if (-not $IsWindows -or $PSVersionTable.PSVersion.Major -lt 7) {
    throw 'Native Windows and PowerShell 7 or later are required.'
}

$Git = (Get-Command git.exe -CommandType Application | Select-Object -First 1).Source
$Cargo = (Get-Command cargo.exe -CommandType Application | Select-Object -First 1).Source
$Rustc = (Get-Command rustc.exe -CommandType Application | Select-Object -First 1).Source
$Repository = 'https://github.com/j-mckerracher/agent-orch.git'
$SourceBranch = 'orch/priority-6-windows-acceptance-6c83686'
$SourceSha = '6c83686c3c3cedf07f8fab50f4764d88f643a2c6'
# Accepted Priority 5 source. It proves only itself and must be an ancestor of the
# final source. Older pins (46497ac, a017134e) are history and are not checked.
# Raw M2 and M3 owner commits may not be ancestors if the integration cherry-picked
# them; never check one.
$BaseSha = '693dd9d8e1726f90b8b1c7a0a0c4c298834ae9bb'
$ExpectedRangeCount = 13
$ExpectedRangeFiles = @(
    'README.md',
    'crates/orchestrator-cli/src/workflow.rs',
    'crates/orchestrator-cli/tests/cli.rs',
    'crates/orchestrator-cli/tests/engine_reconciliation_gate.rs',
    'crates/orchestrator-git/tests/delivery.rs',
    'crates/orchestrator-git/tests/delivery_acceptance_contract.rs',
    'crates/orchestrator-store/tests/delivery_acceptance_contract.rs',
    'docs/evidence/m2-06-gate-wiring-macos.md',
    'docs/evidence/m2-06-reverify-693-macos.md'
)
# Required: the full 40-character commit of THIS runbook file, as the
# coordinator states it in the dispatch message. It is recorded as declared.
# It does not prove that the commands pasted here match that commit.
$RunbookCommit = ''
if ($RunbookCommit -cnotmatch '^[0-9a-f]{40}$') {
    throw 'Set RunbookCommit to the full lowercase 40-character commit from the dispatch message.'
}
foreach ($Sha in $SourceSha, $BaseSha) {
    if ($Sha -cnotmatch '^[0-9a-f]{40}$') {
        throw 'A pinned commit is not a full lowercase 40-character SHA.'
    }
}
if ($SourceBranch -cnotmatch '^orch/priority-6-[a-z0-9-]+$') {
    throw 'The source branch name is not valid.'
}
$Short = $SourceSha.Substring(0, 7)
$RunId = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ') + '-' +
    [Guid]::NewGuid().ToString('N')
$RunRoot = Join-Path $env:TEMP "agent-orch-p6-acc-$RunId"
$SourceDir = Join-Path $RunRoot 'source'
# Published, redacted evidence. Lives outside the measured checkout.
$EvidenceDir = Join-Path $RunRoot 'evidence'
# Raw, byte-exact stdout and stderr. Never published. Outside the checkout.
$RawDir = Join-Path $RunRoot 'raw'
$BuildDir = Join-Path $RunRoot 'cargo-target'
# Isolated checks run with TEMP/TMP below this directory. Their fake agents then
# live under a path that no other program uses, so survivors can be found by path.
$TestTmp = Join-Path $RunRoot 'test-tmp'

if (Test-Path -LiteralPath $RunRoot) {
    throw "Run directory already exists: $RunRoot"
}
New-Item -ItemType Directory -Path $RunRoot | Out-Null
New-Item -ItemType Directory -Path $EvidenceDir | Out-Null
New-Item -ItemType Directory -Path $RawDir | Out-Null
New-Item -ItemType Directory -Path $TestTmp | Out-Null
$env:CARGO_TARGET_DIR = $BuildDir
$env:CARGO_TERM_COLOR = 'never'
$env:RUST_BACKTRACE = '1'

# Redacts remote URLs (scheme form and scp form) and the profile directory name
# that follows a Users path segment. The segment may be separated by one
# backslash, a slash, or a doubled backslash (JSON). Only that one component is
# replaced, never a bare word. Every string is redacted BEFORE it is serialized.
function Protect-Text([string] $Text) {
    # The leading lookbehinds start a match only at the beginning of a word run.
    # Without them, a long run of word characters with no match is rescanned from
    # every start position (quadratic), which can hang a large captured output.
    # The anchor must not let a URL escape: the literal "://" fallback (linear)
    # redacts the rest of any URL whose scheme prefix is not a word-run start,
    # such as "1https://host/q" or "-https://host/q".
    $Text = [regex]::Replace($Text, '(?<![A-Za-z0-9+.-])[A-Za-z][A-Za-z0-9+.-]*://\S+', '<url-redacted>')
    $Text = [regex]::Replace($Text, '://\S+', '<url-redacted>')
    $Text = [regex]::Replace($Text, '(?<![\w.-])[\w.-]+@[\w.-]+:\S+', '<url-redacted>')
    return [regex]::Replace($Text,
        '(?i)(?<=[\\/]Users(?:\\\\|[\\/]))[^\\/"''<>|:*?\r\n]+', '<user>')
}

# What must not survive in any published file: a Users segment that is not
# followed by <user> (the atomic group stops the doubled-separator form
# \\Users\\<user> from backtracking into a false hit), and the account name,
# the profile directory name, and its 8.3 alias as whole words.
$ProfileLeaf = Split-Path -Leaf $env:USERPROFILE
$ProfileShort = (New-Object -ComObject Scripting.FileSystemObject).GetFolder($env:USERPROFILE).ShortName
$ProfileNames = @($env:USERNAME, $ProfileLeaf, $ProfileShort) |
    Where-Object { $_ } | Sort-Object -Unique
if ($ProfileNames.Count -eq 0) { throw 'Cannot determine the profile names.' }
$ProfileLeakPatterns = @('(?i)[\\/]Users(?>[\\/]+)(?!<user>)') +
    @($ProfileNames | ForEach-Object {
        '(?i)(?<![A-Za-z0-9_])' + [regex]::Escape($_) + '(?![A-Za-z0-9_])' })

# First-step self-check with this host's real TEMP and profile, in the long,
# slash, doubled-backslash (JSON), 8.3, and verbatim forms, plus URL and scp
# samples. Any surviving profile name, URL, or scp form stops the run before
# anything is built or published.
$FsoTemp = (New-Object -ComObject Scripting.FileSystemObject).GetFolder($env:TEMP).ShortPath
$Samples = @($env:TEMP, $env:USERPROFILE, $FsoTemp, ($env:TEMP -replace '\\', '/'),
    ($env:TEMP -replace '\\', '\\'), ('\\?\' + $env:TEMP),
    'https://account:token@example.test/org/repo.git', 'git@example.test:org/repo.git')
$Redacted = @($Samples | ForEach-Object { Protect-Text $_ })
[IO.File]::WriteAllLines((Join-Path $EvidenceDir 'protect-text-selfcheck.txt'),
    [string[]]$Redacted)
foreach ($Line in $Redacted) {
    if ($Line -match '://|(?<![\w.-])[\w.-]+@[\w.-]+:') {
        throw 'Protect-Text left a URL in a sample. Do not continue. Report BLOCKED.'
    }
    foreach ($Pattern in $ProfileLeakPatterns) {
        if ([regex]::IsMatch($Line, $Pattern)) {
            throw 'Protect-Text left a profile name in a sample. Do not continue. Report BLOCKED.'
        }
    }
}

# Runs one native command with a time limit. The raw bytes of stdout and stderr
# are copied to $RawDir by asynchronous stream copies, so a full pipe cannot
# block the child and no byte is changed. A redacted text copy goes to
# $EvidenceDir. The exit status is the status of this one process, never of a
# pipeline. On timeout only the process tree that this helper started is ended.
# Writes <Name>.stdout.txt, <Name>.stderr.txt, <Name>.exit.txt, <Name>.result.json.
function Invoke-Bounded {
    param(
        [string] $Exe, [string] $Name, [string[]] $Arguments,
        [string] $Directory = $RunRoot, [int] $TimeoutSeconds = 900,
        [bool] $Isolated = $false
    )
    $RawOut = Join-Path $RawDir "$Name.stdout.bin"
    $RawErr = Join-Path $RawDir "$Name.stderr.bin"
    $Result = [ordered]@{
        name = $Name
        command = Protect-Text ((Split-Path -Leaf $Exe) + ' ' + ($Arguments -join ' '))
        working_directory = Protect-Text $Directory
        source_commit = $SourceSha
        started_utc = [DateTime]::UtcNow.ToString('o')
        duration_seconds = $null
        timeout_seconds = $TimeoutSeconds
        environment = [ordered]@{
            temp = Protect-Text $env:TEMP
            tmp = Protect-Text $env:TMP
            cargo_target_dir = Protect-Text $env:CARGO_TARGET_DIR
            isolated_temp = $Isolated
            rustc_dir_on_path = ($env:PATH -like ((Split-Path -Parent $Rustc) + ';*'))
        }
        exit_code = $null
        timed_out = $false
        root_process_stopped = $true
        tree_kill_attempted = $false
        stdout_bytes = $null
        stdout_sha256 = $null
        stderr_bytes = $null
        stderr_sha256 = $null
        error = $null
    }
    $Clock = [Diagnostics.Stopwatch]::StartNew()
    $OutFile = $null; $ErrFile = $null
    try {
        $Info = [Diagnostics.ProcessStartInfo]::new($Exe)
        foreach ($Argument in $Arguments) { $Info.ArgumentList.Add($Argument) }
        $Info.WorkingDirectory = $Directory
        $Info.UseShellExecute = $false
        $Info.CreateNoWindow = $true
        $Info.RedirectStandardOutput = $true
        $Info.RedirectStandardError = $true
        $OutFile = [IO.File]::Create($RawOut)
        $ErrFile = [IO.File]::Create($RawErr)
        $Process = [Diagnostics.Process]::Start($Info)
        $Result.root_process_stopped = $false
        $OutCopy = $Process.StandardOutput.BaseStream.CopyToAsync($OutFile)
        $ErrCopy = $Process.StandardError.BaseStream.CopyToAsync($ErrFile)
        if (-not $Process.WaitForExit($TimeoutSeconds * 1000)) {
            $Result.timed_out = $true
            try {
                $Result.tree_kill_attempted = $true
                $Process.Kill($true)
            } catch {
                if (-not $Process.HasExited) { $Result.error = $_.Exception.Message }
            }
            if (-not $Process.WaitForExit(10000)) {
                $Result.error = 'The owned process did not stop.'
            }
        }
        if ($Process.HasExited) {
            $Result.root_process_stopped = $true
            $Result.exit_code = $Process.ExitCode
        }
        # A descendant that holds a pipe open keeps the copy running. Bound the wait.
        if (-not [Threading.Tasks.Task]::WaitAll(
                [Threading.Tasks.Task[]]@($OutCopy, $ErrCopy), 30000)) {
            $Result.error = 'Output streams did not close within 30 seconds; a descendant may still run.'
        }
    } catch {
        $Result.error = $_.Exception.Message
    } finally {
        foreach ($File in $OutFile, $ErrFile) {
            if ($File) { try { $File.Flush(); $File.Dispose() } catch { } }
        }
        $Clock.Stop()
        $Result.duration_seconds = [Math]::Round($Clock.Elapsed.TotalSeconds, 3)
    }

    $Text = @{ stdout = ''; stderr = '' }
    foreach ($Stream in 'stdout', 'stderr') {
        $Raw = if ($Stream -eq 'stdout') { $RawOut } else { $RawErr }
        if (Test-Path -LiteralPath $Raw) {
            $Bytes = [IO.File]::ReadAllBytes($Raw)
            $Result["${Stream}_bytes"] = $Bytes.Length
            $Result["${Stream}_sha256"] = [BitConverter]::ToString(
                [Security.Cryptography.SHA256]::HashData($Bytes)).Replace('-', '').ToLowerInvariant()
            $Text[$Stream] = Protect-Text ([Text.UTF8Encoding]::new($false).GetString($Bytes))
        }
        [IO.File]::WriteAllText((Join-Path $EvidenceDir "$Name.$Stream.txt"), $Text[$Stream],
            [Text.UTF8Encoding]::new($false))
    }
    if ($Result.error) { $Result.error = Protect-Text $Result.error }
    [IO.File]::WriteAllText((Join-Path $EvidenceDir "$Name.exit.txt"), "$($Result.exit_code)`n")
    [IO.File]::WriteAllText((Join-Path $EvidenceDir "$Name.result.json"),
        ($Result | ConvertTo-Json -Depth 4))
    $Result['Code'] = $Result.exit_code
    $Result['Out'] = @($Text.stdout -split "\r?\n" | Where-Object { $_ -ne '' })
    return [PSCustomObject]$Result
}

function Invoke-Git([string] $Name, [string[]] $Arguments) {
    Invoke-Bounded -Exe $Git -Name $Name -Arguments (@('-C', $SourceDir) + $Arguments)
}

function Assert-Head([string] $Where) {
    $Head = & $Git -C $SourceDir rev-parse HEAD
    if ($LASTEXITCODE -ne 0 -or $Head -cne $SourceSha) {
        throw "HEAD is not $SourceSha ($Where). Preserve $RunRoot and report it."
    }
}

$Run = Invoke-Bounded -Exe $Git -Name 'clone' -Arguments @(
    'clone', '--no-checkout', '--single-branch', '--branch', $SourceBranch,
    $Repository, $SourceDir)
if ($Run.Code -ne 0) { throw "Clone failed. Preserve $RunRoot and report it." }

$Run = Invoke-Git 'branch-tip' @('rev-parse', "refs/remotes/origin/$SourceBranch")
if ($Run.Code -ne 0) { throw 'Cannot read the branch tip.' }
$BranchTip = $Run.Out[0]
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'branch-tip.txt'), "$BranchTip`n")

$Run = Invoke-Git 'candidate-containment' @(
    'merge-base', '--is-ancestor', $SourceSha, "refs/remotes/origin/$SourceBranch")
if ($Run.Code -ne 0) { throw 'The branch does not contain the source commit.' }
$Run = Invoke-Git 'base-ancestry' @('merge-base', '--is-ancestor', $BaseSha, $SourceSha)
if ($Run.Code -ne 0) { throw "The source commit does not descend from $BaseSha." }
# Consistency gates only. They do not prove patch equivalence of any cherry-pick.
$Run = Invoke-Git 'range-count' @('rev-list', '--count', "$BaseSha..$SourceSha")
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or [int]$Run.Out[0] -ne $ExpectedRangeCount) {
    throw "The commit count above the baseline is not $ExpectedRangeCount."
}
$Run = Invoke-Git 'range-files' @('diff', '--name-only', $BaseSha, $SourceSha)
if ($Run.Code -ne 0) { throw 'Cannot list the changed files.' }
if ((@($Run.Out | Sort-Object) -join "`n") -cne (@($ExpectedRangeFiles | Sort-Object) -join "`n")) {
    throw 'The files changed above the baseline are not the expected set. Report it.'
}

$Run = Invoke-Git 'switch' @('switch', '--detach', $SourceSha)
if ($Run.Code -ne 0) { throw "Checkout failed. Preserve $RunRoot and report it." }
Assert-Head 'after checkout'
$Run = Invoke-Git 'source-head-before' @('rev-parse', 'HEAD')
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or $Run.Out[0] -cne $SourceSha) {
    throw 'Recorded HEAD before the checks is not the source commit.'
}
$Run = Invoke-Git 'source-status-before' @('status', '--porcelain=v1', '--untracked-files=all')
if ($Run.Code -ne 0) { throw 'Cannot read source status.' }
if ($Run.Out.Count -ne 0) { throw 'Source checkout is not clean.' }
```

If Git, Cargo, Rust, or PowerShell is missing, report the missing tool. Do not install it. Preserve
any output that exists. A setup failure gives the result `BLOCKED`.

## 2. Record the environment

Record the operating system, CPU, toolchain, and source commit. Do not record the complete
environment or any credentials.

```powershell
Assert-Head 'before environment'
cmd.exe /c ver > (Join-Path $EvidenceDir 'windows-version.txt')
Get-CimInstance Win32_OperatingSystem |
    Select-Object Caption, Version, BuildNumber, OSArchitecture |
    Format-List | Out-File -LiteralPath (Join-Path $EvidenceDir 'os.txt') -Encoding utf8
Get-CimInstance Win32_Processor |
    Select-Object Name, Manufacturer, NumberOfCores, NumberOfLogicalProcessors |
    Format-List | Out-File -LiteralPath (Join-Path $EvidenceDir 'cpu.txt') -Encoding utf8
[IO.File]::WriteAllText(
    (Join-Path $EvidenceDir 'powershell-version.txt'),
    $PSVersionTable.PSVersion.ToString()
)
[IO.File]::WriteAllLines(
    (Join-Path $EvidenceDir 'tool-paths.txt'),
    [string[]]@($Git, $Cargo, $Rustc | ForEach-Object { Protect-Text $_ })
)
$GitVersion = Invoke-Bounded -Exe $Git -Name 'git-version' -Arguments @('--version')
$RustcVersion = Invoke-Bounded -Exe $Rustc -Name 'rustc-version' -Arguments @('-Vv') -Directory $SourceDir
$CargoVersion = Invoke-Bounded -Exe $Cargo -Name 'cargo-version' -Arguments @('-V') -Directory $SourceDir
# Exit code 1 means the key is not set. That is not an error.
$Autocrlf = Invoke-Git 'git-autocrlf' @('config', '--show-origin', '--get-all', 'core.autocrlf')
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'source-sha.txt'), "$SourceSha`n")
# Declared by the operator. It does not prove that the pasted commands match
# the runbook file of that commit.
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'runbook-identity.txt'),
    "runbook file: docs/evidence/priority-6-windows-acceptance-6c83686-runbook.md`n" +
    "runbook commit (operator-declared, not byte proof): $RunbookCommit`n" +
    "source commit: $SourceSha`n")
if ($GitVersion.Code -ne 0 -or $RustcVersion.Code -ne 0 -or $CargoVersion.Code -ne 0) {
    throw "Toolchain check failed. Preserve $RunRoot and report it."
}
```

The files `git-version.stdout.txt`, `rustc-version.stdout.txt`, `cargo-version.stdout.txt`, and
`git-autocrlf.stdout.txt` hold the toolchain record. An empty `git-autocrlf.stdout.txt` with exit
code 1 means `core.autocrlf` is not set.

## 3. Run the checks

`Invoke-Bounded` records the stdout, stderr, exit code, duration, environment summary, and timeout of
each command. Each command has a limit (30 minutes by default). A timeout is a failure, even if the
process later exits with code zero. Run each command once. Do not retry a failed command in this
evidence directory.

```powershell
function Invoke-CapturedCargo {
    param([string] $Name, [string[]] $Arguments, [int] $TimeoutSeconds = 1800,
          [bool] $Isolated = $false)
    Assert-Head "before $Name"
    Invoke-Bounded -Exe $Cargo -Name $Name -Arguments $Arguments -Directory $SourceDir `
        -TimeoutSeconds $TimeoutSeconds -Isolated $Isolated
}

# Test names. Each is a real test of the pinned source for the Windows cfg set.
$GitTests = @(
    'timed_out_command_is_killed_and_recorded',
    'trusted_command_runs_and_records_output_and_exit_code',
    'trusted_nonzero_exit_is_recorded_as_failed',
    'dirty_worktree_is_unsafe_and_survives_safety_check',
    'report_only_preserves_refs_worktrees_and_repository_files',
    'local_review_creates_only_review_ref',
    'unlanded_worktree_is_unsafe_and_survives_safety_check',
    'git_add_failure_reports_git_stderr_cause',
    'name_collision_is_recognized_only_for_gits_own_conflict_lines',
    'long_stderr_keeps_the_final_cause_within_the_bound',
    'stderr_detail_neutralizes_terminal_control_characters',
    'verbatim_drive_and_unc_paths_become_plain_paths',
    'verbatim_paths_with_other_meaning_are_left_alone',
    'near_reserved_names_are_plain',
    'for_git_is_a_no_op_off_windows',
    'verbatim_worktrees_root_creates_a_worktree'
)
$GitDeliveryTests = @(
    'identity_failure_fails_acquisition_with_the_os_error_and_keeps_the_artifact',
    'missing_platform_identity_fails_acquisition_and_keeps_the_artifact',
    'verified_identity_lock_is_removed_on_drop',
    'identity_matches_hard_links_and_distinguishes_files'
)
$DataLockTests = @(
    'dead_pid_lock_is_taken_over_and_new_holder_is_recorded',
    'live_pid_with_different_start_identity_is_taken_over',
    'concurrent_takeovers_have_one_holder_and_one_named_conflict',
    'foreign_non_lock_file_is_never_removed',
    'live_holder_is_refused_with_pid_and_drop_releases_lock',
    'live_legacy_lock_is_refused_with_clear_error',
    'separate_process_cannot_acquire_a_live_lock'
)
$DataLockLibTests = @(
    'range_lock_does_not_block_other_handles_reading_the_record',
    'identity_matches_hard_links_and_distinguishes_files',
    'fresh_creation_identity_failure_is_an_io_error_and_keeps_the_artifact',
    'fresh_creation_without_platform_identity_fails_explicitly_and_keeps_the_artifact',
    'identity_lookup_failure_is_an_io_error_not_a_retry_and_preserves_the_lock',
    'platform_without_identity_still_retries_and_preserves_the_lock',
    'verified_creation_is_cleaned_up_on_drop'
)
$LockContractTests = @(
    'directory_at_the_lock_path_is_never_removed',
    'malformed_records_are_never_taken_over',
    'stale_record_without_a_token_is_reported_and_preserved'
)
$RuntimeContractTests = @(
    'forged_start_time_needs_review_and_blocks_the_gate',
    'live_process_is_left_running_and_does_not_block_the_gate',
    'git_state::ambiguous_delivery_step_is_never_summarised_as_landed',
    'git_state::clean_matching_worktree_is_recoverable_and_dirty_work_is_preserved',
    'git_state::committed_unlanded_work_is_never_treated_as_matching_or_discardable',
    'git_state::named_matrix_anchor_cases_hold',
    'git_state::only_git_confirmed_delivery_is_completed_and_unlanded_delivery_is_not',
    'git_state::partially_landed_delivery_requires_inspection_and_leaves_git_untouched',
    'git_state::process_uncertainty_outranks_every_workspace_and_delivery_state',
    'git_state::real_git_evidence_matrix_matches_the_approved_precedence',
    'git_state::removed_worktree_blocks_the_task_without_blocking_other_scheduling',
    'git_state::uninspected_or_unrecorded_workspace_requires_inspection',
    'git_state::unsafe_workspace_requires_inspection_even_when_delivery_is_confirmed',
    'git_state::unsafe_workspace_requires_inspection_even_when_task_succeeded'
)
$RuntimeReconcileTests = @(
    'changed_start_time_is_an_identity_mismatch',
    'forged_windows_creation_time_is_a_mismatch_for_every_100ns_remainder',
    'owner_observation_classifies_alive_and_reused_pid_by_creation_time',
    'owner_observation_requires_a_creation_time',
    'process_plan_order_is_deterministic',
    'scheduling_gate_blocks_unresolved_or_ambiguous_records',
    'windows_identity_compares_creation_time_at_100ns_precision',
    'windows_owner_observation_uses_creation_time_not_only_pid',
    'git_evidence::delivery_steps_summarise_conservatively',
    'git_evidence::dirty_or_primary_checkout_overrides_a_matching_workspace',
    'git_evidence::missing_workspace_needs_no_safety_inspection',
    'git_evidence::uninspected_or_unrecorded_workspace_fails_closed',
    'git_evidence::unlanded_head_on_a_matching_workspace_is_branch_moved',
    'git_evidence::workspace_classification_maps_one_to_one_when_clean',
    'task_classification::active_tasks_without_delivery_keep_recovery_semantics',
    'task_classification::candidate_commit_on_moved_branch_is_recoverable_for_verification',
    'task_classification::confirmed_delivery_completes_only_a_succeeded_task_with_a_clean_known_workspace',
    'task_classification::exit_without_recorded_success_or_confirmed_delivery_is_never_completed',
    'task_classification::exited_owner_with_matching_workspace_and_no_delivery_is_recoverable',
    'task_classification::live_owner_blocks_the_task_without_blocking_scheduling',
    'task_classification::missing_workspace_blocks_the_task',
    'task_classification::partial_or_ambiguous_delivery_requires_inspection',
    'task_classification::succeeded_with_a_recorded_unapplied_delivery_is_not_proven_complete',
    'task_classification::succeeded_without_delivery_steps_completes_only_with_a_matching_workspace',
    'task_classification::terminal_failures_without_delivery_stay_blocked',
    'task_classification::uncommitted_or_unlanded_work_is_preserved_for_inspection',
    'task_classification::unproven_owner_requires_inspection_and_blocks_scheduling',
    'task_classification::unsafe_workspace_outranks_every_completion_shortcut'
)
$IpcTokenTests = @('startup_replaces_only_regular_stale_token')
$CliCleanupUnitTests = @(
    'cleanup_refuses_clean_worktree_of_unsettled_task_without_a_published_owner',
    'cleanup_removes_clean_landed_worktree_of_settled_task_without_an_owner',
    'cleanup_refuses_clean_worktree_when_owner_identity_is_unprovable'
)
$CliSessionGoneTests = @('session_gone_guard_refuses_missing_alive_and_unknown_classifications')
$CliLibTests = $CliCleanupUnitTests + $CliSessionGoneTests + @(
    'parses_every_command_and_global_data_directory',
    'rejects_run_without_repository_or_objective',
    'canonical_target_resolves_symlinked_ancestors_and_missing_suffixes',
    'capture_stream_copies_raw_bytes_to_both_outputs',
    'recovery_classification_refuses_reused_lead_with_live_foreign_group',
    'task_ids_reject_path_syntax_before_store_access',
    'unresolved_legacy_windows_owner_has_audited_manual_recovery_path',
    # M2-06 inline tests with no cfg(unix) (static read of workflow.rs at 1d66d648).
    'delivery_blocks_only_partial_ambiguous_or_confirmed_on_unsucceeded_task',
    'settled_failed_task_with_unlanded_delivery_and_absent_workspace_does_not_block',
    'confirmed_delivery_on_a_failed_task_blocks_and_preserves_everything',
    'ambiguous_delivery_blocks_scheduling_and_is_not_rewritten',
    'pending_attempt_without_a_session_blocks_but_a_planned_task_does_not',
    'settled_candidate_ready_attempt_does_not_block_scheduling'
)
$CliCleanupSafetyTests = @(
    'cleanup_refuses_staged_deleted_and_unlanded_work_in_a_landed_worktree',
    'cleanup_refuses_clean_worktree_while_run_owner_is_live',
    'cleanup_follows_existing_contract_for_provably_exited_run_owner'
)
$CliAcceptanceTests = @(
    'run_produces_isolated_diff_and_cleanup_preserves_primary',
    'cancel_terminates_agent_and_descendant'
)
# P6 additions. Names come from the final source (cfg set for Windows), never from a
# candidate branch. The M3 names below were read from 37344247b3c671e0dcc676e153f662b0e05670db
# and must be re-verified against the final source before this runbook is published.
# New tests of crates/orchestrator-git/tests/delivery.rs (13 to 21 tests).
$GitDeliveryNewTests = @(
    'local_review_refuses_refs_outside_the_review_namespace_before_any_callback',
    'verification_is_revalidated_before_approval_and_before_any_write',
    'refused_delivery_persists_refusal_releases_lock_and_leaves_destination_reusable',
    'candidate_changed_by_approval_time_is_refused_without_writing_a_ref',
    'dirty_candidate_is_refused_before_verification_or_approval',
    'primary_checkout_and_foreign_repository_are_not_deliverable_candidates',
    'persistence_failure_is_reported_and_still_releases_the_destination_lock',
    'primary_checkout_is_retained_and_a_clean_landed_worktree_is_safe'
)
# crates/orchestrator-git/tests/delivery_acceptance_contract.rs (new, no cfg).
$GitDeliveryAcceptanceTests = @(
    'approval_and_verification_receive_exactly_the_requested_binding_for_each_action',
    'contender_is_refused_and_recorded_while_owner_lock_and_destination_stay_intact',
    'foreign_lock_without_an_owner_pid_blocks_delivery_and_is_preserved',
    'review_ref_created_by_another_writer_during_approval_is_revalidated_not_overwritten',
    'deletion_safety_follows_current_repository_state_and_never_deletes',
    'every_callback_runs_under_the_destination_lock_and_a_persistence_failure_keeps_the_written_ref'
)
# crates/orchestrator-store/tests/delivery_acceptance_contract.rs (new, no cfg).
$StoreDeliveryAcceptanceTests = @(
    'only_an_approved_current_exact_approval_authorizes_delivery',
    'outcome_that_does_not_match_a_delivery_action_persists_nothing',
    'delivery_step_is_append_only_and_a_reused_id_changes_nothing'
)
# crates/orchestrator-cli/tests/engine_reconciliation_gate.rs (new, no file-level cfg).
# Read from blob 67bed0953a1c78bee70d7dd9c9e18cc6e4a73c33 (M2 commit 1d66d648). 22 tests, no
# file-level or test-level cfg. Re-verify the blob id against the final source.
$CliEngineReconciliationTests = @(
    'run_persists_launch_identity_in_sqlite_and_leaves_legacy_json_unmigrated',
    'live_parallel_run_neither_blocks_scheduling_nor_holds_the_lock',
    'held_data_directory_lock_refuses_mutating_commands_and_leaves_readers_alone',
    'unidentified_running_session_refuses_scheduling_and_preserves_records',
    'running_task_without_an_unfinished_session_refuses_scheduling',
    'pending_attempt_without_any_session_refuses_scheduling',
    'planned_and_admitted_tasks_without_attempts_do_not_block_scheduling',
    'settled_candidate_ready_attempt_does_not_block_scheduling',
    'interrupted_task_with_known_owned_workspace_resumes_scheduling_and_keeps_evidence',
    'settled_failed_task_with_dirty_worktree_does_not_block_scheduling',
    'interrupted_foreign_workspace_refuses_scheduling_and_keeps_all_evidence',
    'interrupted_task_whose_project_cannot_be_inspected_refuses_scheduling',
    'interrupted_task_without_a_recorded_workspace_refuses_scheduling',
    'interrupted_task_with_partial_delivery_refuses_scheduling_and_keeps_evidence',
    'settled_task_with_partial_delivery_journal_refuses_scheduling',
    'settled_task_with_ambiguous_delivery_journal_refuses_scheduling',
    'confirmed_delivery_on_a_failed_task_is_contradictory_and_refuses_scheduling',
    'consistent_delivery_journals_do_not_block_scheduling',
    'live_owner_with_partial_delivery_journal_refuses_scheduling',
    'live_owner_with_ambiguous_delivery_journal_refuses_scheduling',
    'live_owner_with_confirmed_delivery_while_running_refuses_scheduling',
    'live_owner_with_consistent_not_applied_journal_does_not_block_scheduling'
)
# Every named test must also report ok inside the full workspace run.
$WorkspaceTests = $GitTests + $GitDeliveryTests + $DataLockTests + $DataLockLibTests +
    $LockContractTests + $RuntimeContractTests + $RuntimeReconcileTests + $IpcTokenTests +
    $CliLibTests + $CliCleanupSafetyTests + $CliAcceptanceTests +
    $GitDeliveryNewTests + $GitDeliveryAcceptanceTests + $StoreDeliveryAcceptanceTests +
    $CliEngineReconciliationTests

# Expect: tests that must report ok.
# Single: the run must have exactly one test binary, and that binary must run
#   at least one test. This is how a filter that matches nothing is caught.
# Passed, Ignored: exact counts. All are CALCULATED from the source for Windows,
#   not measured. A different measured count is INCONCLUSIVE, never edited here.
# Isolated: run with TEMP/TMP under $TestTmp and rustc on PATH.
# TimeoutSeconds: limit for the command (default 1800).
$Checks = @(
    @{ Name = 'workspace-tests'; Expect = $WorkspaceTests; Single = $false
       Passed = 430; Ignored = 1
       Arguments = @('test', '--locked', '--workspace', '--no-fail-fast') },
    @{ Name = 'git-package'; Expect = $GitTests + $GitDeliveryTests + $GitDeliveryNewTests + $GitDeliveryAcceptanceTests; Single = $false
       Passed = 73
       Arguments = @('test', '--locked', '-p', 'orchestrator-git') },
    @{ Name = 'git-delivery-lib'; Expect = $GitDeliveryTests; Single = $true
       Passed = 4; Ignored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-git', '--lib', 'delivery') },
    @{ Name = 'store-data-lock-integration'; Expect = $DataLockTests; Single = $true
       Passed = 7; Ignored = 1
       Arguments = @('test', '--locked', '-p', 'orchestrator-store',
           '--test', 'data_lock') },
    @{ Name = 'store-data-lock-lib'; Expect = $DataLockLibTests; Single = $true
       Passed = 7; Ignored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-store',
           '--lib', 'data_lock') },
    @{ Name = 'store-lock-identity-contract'; Expect = $LockContractTests; Single = $true
       Passed = 3; Ignored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-store',
           '--test', 'lock_identity_contract', '--', '--nocapture') },
    @{ Name = 'runtime-reconciliation-contract'; Expect = $RuntimeContractTests; Single = $true
       Passed = 14; Ignored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-runtime',
           '--test', 'reconciliation_contract') },
    @{ Name = 'runtime-reconcile'; Expect = $RuntimeReconcileTests; Single = $true
       Passed = 28; Ignored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-runtime',
           '--test', 'reconcile') },
    @{ Name = 'ipc-token-owner'; Expect = $IpcTokenTests; Single = $true
       Passed = 1; Ignored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-ipc', '--lib',
           'startup_replaces_only_regular_stale_token') },
    @{ Name = 'cli-lib'; Expect = $CliLibTests; Single = $true
       Passed = 17; Ignored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-cli', '--lib') },
    @{ Name = 'cli-cleanup-units'; Expect = $CliCleanupUnitTests; Single = $true
       Passed = 3; Ignored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-cli', '--lib', 'cleanup') },
    @{ Name = 'cli-session-gone-guard'; Expect = $CliSessionGoneTests; Single = $true
       Passed = 1; Ignored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-cli', '--lib',
           'session_gone_guard_refuses_missing_alive_and_unknown_classifications') },
    @{ Name = 'cli-cleanup-safety'; Expect = $CliCleanupSafetyTests; Single = $true
       Passed = 3; Ignored = 0; Isolated = $true; TimeoutSeconds = 600
       Arguments = @('test', '--locked', '-p', 'orchestrator-cli',
           '--test', 'cleanup_safety') },
    @{ Name = 'cli-acceptance'; Expect = $CliAcceptanceTests; Single = $true
       Passed = 2; Ignored = 0; Isolated = $true; TimeoutSeconds = 600
       Arguments = @('test', '--locked', '-p', 'orchestrator-cli',
           '--test', 'portable_acceptance') },
    # P6 gates. The engine gate runs with the Isolated environment like cleanup_safety, so a
    # fake agent compiles with rustc on PATH and any survivor lies under the private TEMP.
    @{ Name = 'git-delivery-acceptance-contract'; Expect = $GitDeliveryAcceptanceTests; Single = $true
       Passed = 6; Ignored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-git',
           '--test', 'delivery_acceptance_contract') },
    @{ Name = 'store-delivery-acceptance-contract'; Expect = $StoreDeliveryAcceptanceTests; Single = $true
       Passed = 3; Ignored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-store',
           '--test', 'delivery_acceptance_contract') },
    @{ Name = 'cli-engine-reconciliation-gate'; Expect = $CliEngineReconciliationTests; Single = $true
       Passed = 22; Ignored = 0; Isolated = $true; TimeoutSeconds = 600
       Arguments = @('test', '--locked', '-p', 'orchestrator-cli',
           '--test', 'engine_reconciliation_gate') }
)

$Rank = @{ PASS = 0; INCONCLUSIVE = 1; FAIL = 2 }

function Get-CheckVerdict($Check, $Result) {
    $State = @{ Status = 'PASS'; Notes = [Collections.Generic.List[string]]::new() }
    $Raise = {
        param($Level, $Note)
        if ($Rank[$Level] -gt $Rank[$State.Status]) { $State.Status = $Level }
        $State.Notes.Add($Note)
    }
    $Running = @(); $Passed = $null; $Failed = $null; $Ignored = $null

    if ($Result.timed_out -or $Result.error -or $null -eq $Result.exit_code) {
        & $Raise 'FAIL' 'Timeout or process error.'
    } elseif ($Result.exit_code -ne 0) {
        & $Raise 'FAIL' "Exit code $($Result.exit_code)."
    }

    $StdoutPath = Join-Path $EvidenceDir "$($Check.Name).stdout.txt"
    $Out = ''
    if (Test-Path -LiteralPath $StdoutPath) { $Out = [IO.File]::ReadAllText($StdoutPath) }
    $Lines = [regex]::Matches($Out, '(?m)^(?:running \d+ tests?|test result: .*)$') |
        ForEach-Object { $_.Value.TrimEnd() }
    [IO.File]::WriteAllLines(
        (Join-Path $EvidenceDir "$($Check.Name).summary.txt"), [string[]]@($Lines))

    $Running = @([regex]::Matches($Out, '(?m)^running (\d+) tests?\s*$') |
        ForEach-Object { [int]$_.Groups[1].Value })
    $Summary = [regex]::Matches($Out,
        '(?m)^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored')
    if ($Summary.Count -gt 0) {
        $Passed = ($Summary | ForEach-Object { [int]$_.Groups[2].Value } | Measure-Object -Sum).Sum
        $Failed = ($Summary | ForEach-Object { [int]$_.Groups[3].Value } | Measure-Object -Sum).Sum
        $Ignored = ($Summary | ForEach-Object { [int]$_.Groups[4].Value } | Measure-Object -Sum).Sum
    }

    if ($Summary.Count -eq 0) {
        & $Raise 'INCONCLUSIVE' 'No test result line.'
    } else {
        if ($Summary.Count -ne $Running.Count) {
            & $Raise 'INCONCLUSIVE' "$($Running.Count) 'running' lines but $($Summary.Count) result lines."
        }
        if ($Failed -gt 0 -or @($Summary | Where-Object { $_.Groups[1].Value -eq 'FAILED' }).Count -gt 0) {
            & $Raise 'FAIL' "$Failed test(s) failed."
        }
        if (($Running | Measure-Object -Sum).Sum -eq 0) {
            & $Raise 'INCONCLUSIVE' 'running 0 tests. No test ran.'
        }
        if ($Check.Single) {
            if ($Summary.Count -ne 1) {
                & $Raise 'INCONCLUSIVE' "Found $($Summary.Count) test binaries. Expected 1."
            }
            if ($Running -contains 0) {
                & $Raise 'INCONCLUSIVE' 'The filter ran zero tests (running 0 tests).'
            }
        }
        if ($null -ne $Check.Passed -and $Passed -ne $Check.Passed) {
            & $Raise 'INCONCLUSIVE' "$Passed passed. Expected $($Check.Passed) (calculated, not measured before this run)."
        }
        if ($null -ne $Check.Ignored -and $Ignored -ne $Check.Ignored) {
            & $Raise 'INCONCLUSIVE' "$Ignored ignored. Expected $($Check.Ignored) (calculated, not measured before this run)."
        }
        foreach ($Test in $Check.Expect) {
            $Pattern = '(?m)^test (?:\S+::)*' + [regex]::Escape($Test) + ' \.\.\. ok\s*$'
            if (-not [regex]::IsMatch($Out, $Pattern)) {
                & $Raise 'INCONCLUSIVE' "No 'ok' line for $Test."
            }
        }
    }
    [PSCustomObject]@{
        name = $Check.Name; status = $State.Status; exit_code = $Result.exit_code
        timed_out = $Result.timed_out; duration_seconds = $Result.duration_seconds
        running = ($Running -join ',')
        passed = $Passed; failed = $Failed; ignored = $Ignored
        notes = (Protect-Text ($State.Notes -join ' '))
    }
}

# Windows may report the same directory as a long or an 8.3 short path, and
# TEMP itself can be a short path. Compare against every form of the owned path.
Add-Type -Namespace Native -Name Paths -MemberDefinition @'
[DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
public static extern uint GetLongPathNameW(string path, System.Text.StringBuilder buffer, uint size);
[DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
public static extern uint GetShortPathNameW(string path, System.Text.StringBuilder buffer, uint size);
'@
function Get-PathAliases([string] $Path) {
    $Long = [Text.StringBuilder]::new(32768)
    $Short = [Text.StringBuilder]::new(32768)
    $Aliases = @($Path)
    if ([Native.Paths]::GetLongPathNameW($Path, $Long, 32768) -gt 0) { $Aliases += $Long.ToString() }
    if ([Native.Paths]::GetShortPathNameW($Path, $Short, 32768) -gt 0) { $Aliases += $Short.ToString() }
    @($Aliases | ForEach-Object { $_.TrimEnd('\') + '\' } | Sort-Object -Unique)
}

# A survivor is a process whose image lies under one of $TmpPrefixes (this
# run's TestTmp directory), or whose command line contains one of $RootPrefixes
# (this run's RunRoot directory). The wrapper image lives in the cargo target
# directory, so the command-line test is what finds it. Comparison is
# case-insensitive. Only the unique RunRoot is matched, never a base name.
function Find-OwnedSurvivors($Processes, [string[]] $TmpPrefixes, [string[]] $RootPrefixes) {
    $Ordinal = [StringComparison]::OrdinalIgnoreCase
    foreach ($Process in $Processes) {
        $ByImage = $Process.ExecutablePath -and
            @($TmpPrefixes | Where-Object { $Process.ExecutablePath.StartsWith($_, $Ordinal) }).Count -gt 0
        $ByCommandLine = $Process.CommandLine -and
            @($RootPrefixes | Where-Object { $Process.CommandLine.IndexOf($_, $Ordinal) -ge 0 }).Count -gt 0
        if ($ByImage -or $ByCommandLine) {
            $Process | Select-Object ProcessId, ParentProcessId, Name, ExecutablePath, CommandLine
        }
    }
}

$Verdicts = @()
foreach ($Check in $Checks) {
    $Timeout = if ($Check.TimeoutSeconds) { [int]$Check.TimeoutSeconds } else { 1800 }
    if ($Check.Isolated) {
        # Private temp directory and rustc on PATH for the test's fake agent.
        $CheckTmp = Join-Path $TestTmp $Check.Name
        New-Item -ItemType Directory -Path $CheckTmp | Out-Null
        $SavedEnv = @{ TEMP = $env:TEMP; TMP = $env:TMP; PATH = $env:PATH }
        $env:TEMP = $CheckTmp; $env:TMP = $CheckTmp
        $env:PATH = (Split-Path -Parent $Rustc) + ';' + $env:PATH
        try {
            $Run = Invoke-CapturedCargo -Name $Check.Name -Arguments $Check.Arguments `
                -TimeoutSeconds $Timeout -Isolated $true
        } finally {
            $env:TEMP = $SavedEnv.TEMP; $env:TMP = $SavedEnv.TMP; $env:PATH = $SavedEnv.PATH
        }
    } else {
        $Run = Invoke-CapturedCargo -Name $Check.Name -Arguments $Check.Arguments `
            -TimeoutSeconds $Timeout
    }
    if (-not $Run.root_process_stopped) {
        throw "An owned process may still run. Preserve $RunRoot and report it."
    }
    $Verdict = Get-CheckVerdict $Check $Run
    # Read-only listing of this run's processes after EVERY check. No process is
    # stopped. A survivor makes the check FAIL.
    $Survivors = @(Find-OwnedSurvivors (Get-CimInstance Win32_Process) `
        (Get-PathAliases $TestTmp) (Get-PathAliases $RunRoot))
    # Out-String -Width stops line wrapping from splitting a path across
    # lines, where the redaction could not see it.
    [IO.File]::WriteAllText(
        (Join-Path $EvidenceDir "$($Check.Name).survivors.txt"),
        (Protect-Text ($Survivors | Format-List | Out-String -Width 32768)))
    if ($Survivors.Count -ne 0) {
        $Verdict.status = 'FAIL'
        $Verdict.notes = ($Verdict.notes + " $($Survivors.Count) process(es) of this run survived this check; see $($Check.Name).survivors.txt.").Trim()
    }
    $Verdicts += $Verdict
}

Assert-Head 'after the checks'
$HeadAfter = Invoke-Git 'source-head-after' @('rev-parse', 'HEAD')
if ($HeadAfter.Code -ne 0 -or $HeadAfter.Out.Count -ne 1 -or $HeadAfter.Out[0] -cne $SourceSha) {
    throw "Recorded HEAD after the checks is not the source commit. Preserve $RunRoot and report it."
}
$After = Invoke-Git 'source-status-after' @('status', '--porcelain=v1', '--untracked-files=all')
$DiffCheck = Invoke-Git 'diff-check' @('diff', '--check')
```

`Invoke-Bounded` ends only the process tree that it started, and only on timeout. The survivor scan
after each check lists, and never stops, any process of this run. Each exit status is the exit code
of one process, never the status of a pipeline. `Assert-Head` throws if `HEAD` changes during the run.

Status rules for each command:

- `FAIL`: timeout, process error, non-zero exit code, a failed test, or any process of this run
  (image under the private temp directory, or a command line naming this run's directory) that is
  still running after the check.
- `INCONCLUSIVE`: no `test result` line, or the `running` and `test result` lines do not pair up.
  Also: every binary reports `running 0 tests`, or a `Single` check reports `running 0 tests` or more
  than one binary. Also: a required test has no `ok` line, or a passed or ignored count differs from
  the calculated count. A skipped or zero-test selection is never a pass.
- `PASS`: exit code zero, every required test reports `ok`, zero tests failed, no survivor, and the
  counts match the calculated counts.

A timeout stays `FAIL`. The first check compiles everything into a new target directory, so a cold
build can exceed a limit. In your report, say whether the preserved stdout of a timed-out check ends
in `Compiling` or `Finished` output with no `running N tests` line (the build was still running: an
environment or resource limit) or shows tests that had started (a product or test hang). That label is
a note only. Do not change the verdict, retry in the same evidence directory, or raise a limit.

## 4. Write the result

```powershell
# Set this to what you observed about your own agent runtime and model. If you
# could not observe Sonnet 5.5 with high reasoning, keep 'unverified'.
$AgentRuntime = 'unverified'
$Overall = 'PASS'
foreach ($Verdict in $Verdicts) {
    if ($Rank[$Verdict.status] -gt $Rank[$Overall]) { $Overall = $Verdict.status }
}
$Extra = [Collections.Generic.List[string]]::new()
foreach ($Check in $Checks) {
    if (@($Verdicts | Where-Object { $_.name -ceq $Check.Name }).Count -ne 1) {
        $Overall = 'FAIL'; $Extra.Add("No single verdict for $($Check.Name).")
    }
}
if ($After.Code -ne 0 -or $After.Out.Count -ne 0) {
    $Overall = 'FAIL'; $Extra.Add('Source status after the run is not clean.')
}
if ($DiffCheck.Code -ne 0) { $Overall = 'FAIL'; $Extra.Add('git diff --check failed.') }

$Table = ($Verdicts | ForEach-Object {
    "| $($_.name) | $($_.status) | $($_.exit_code) | $($_.timed_out) | $($_.duration_seconds) | " +
    "$($_.running) | $($_.passed) | $($_.failed) | $($_.ignored) | $($_.notes) |"
}) -join "`n"
$Fence = '`' * 3
$Summaries = ($Verdicts | ForEach-Object {
    "### $($_.name)`n`n${Fence}text`n" +
    ([IO.File]::ReadAllText((Join-Path $EvidenceDir "$($_.name).summary.txt")).TrimEnd()) +
    "`n$Fence"
}) -join "`n`n"
# Expandable here-string: a backtick inside it is an escape (`r, `t, ...).
# Keep backticks out of this text.
$Report = @"
# Priority 6 Windows acceptance of $SourceSha (M2-06 engine reconciliation, M3-04 delivery)

This run does not close M2-06 or M3-04. Both items stay open until independent Mac, source, and
native acceptance, and the coordinator decides acceptance. The expected counts are
calculated from source for Windows. They are not measured before this run, and no macOS or review
result is Windows evidence.

- Run ID: $RunId
- UTC date: $([DateTime]::UtcNow.ToString('o'))
- Source branch: $SourceBranch
- Branch tip at clone time: $BranchTip
- Source commit: $SourceSha
- HEAD matched the source commit before the checks and after them: yes
  (source-head-before.stdout.txt, source-head-after.stdout.txt)
- Baseline ancestry (693dd9d8), commit count, and changed-file set: base-ancestry,
  range-count, range-files (consistency gates, not patch-equivalence proof)
- Runbook identity (operator-declared, not byte proof): runbook-identity.txt
- Overall result: $Overall $($Extra -join ' ')
- Toolchain: see git-version.stdout.txt, rustc-version.stdout.txt,
  cargo-version.stdout.txt, git-autocrlf.stdout.txt (exit code in
  git-autocrlf.exit.txt)
- OS, CPU, PowerShell: see os.txt, cpu.txt, powershell-version.txt
- Agent runtime and Sonnet 5.5/high setting: $AgentRuntime
- Survivor scan: every check has a survivors file. Processes whose image lies under this run's
  private temp directory, or whose command line names this run's directory (long or 8.3 form), are
  listed and fail the check. Nothing is stopped by the scan.
- Raw stdout and stderr bytes are kept outside the checkout and outside the published files. Their
  size and SHA-256 are in each result.json.

| Command | Status | Exit | Timed out | Seconds | Running (N per binary) | Passed | Failed | Ignored | Notes |
|---|---|---|---|---|---|---|---|---|---|
$Table

## Test result lines

$Summaries

## Coverage gaps

- Unix-only tests did not run on Windows: the orphan-tree test, the fault-injection startup test, the
  runtime process-group survivor test, the store symlink test, and the Unix-only unit tests. Their
  macOS acceptance is separate and is not Windows evidence.
- cli.rs, multi_task_review.rs, recovery.rs and daemon.rs are Unix-only (cfg unix) and compile to
  zero tests on Windows. Windows coverage of the CLI engine's reconciliation gate is only the
  named engine_reconciliation_gate tests, portable_acceptance and cleanup_safety. There is no Windows
  crash recovery (recover-owner), multi-task review, or daemon integration coverage.
- M3-06 write and resume scope is not included in this candidate and is not claimed. No delivery
  to a real remote ran: the delivery tests use throwaway local repositories.
- Windows job-object orphan closure is covered by the contract in source and by the existing
  cancel_terminates_agent_and_descendant gate. No separate orphan-survival test exists.
"@
# Structured redaction before anything is written. Then write both files.
$Report = Protect-Text $Report
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'result.md'), $Report)
[IO.File]::WriteAllText(
    (Join-Path $EvidenceDir 'verdicts.json'), ($Verdicts | ConvertTo-Json -Depth 4)
)
Write-Output "overall=$Overall"
```

Use PASS only when every command is PASS, `HEAD` matches the full source commit, both source status
checks are empty, and the diff check exits zero. A PASS shows that these checks passed on this host.
It does not show Windows coverage for any scenario that no test ran.

If a command fails, preserve its complete output and report its exit code. If a process does not
stop, report it and do not start another attempt. For a retry, use a new run ID and preserve both
attempts.

## 5. Publish evidence only

Publish the evidence for every result, including FAIL and INCONCLUSIVE. Publish only flat text files
from `$EvidenceDir`. Do not publish `raw`, `cargo-target`, `test-tmp`, the source clone, session
files, agent memory, or any other path. Do not edit the source or `board.json`. Publication fails
closed: any scan hit or missing file stops it before a worktree exists.

```powershell
$EvidenceBranch = "evidence/priority-6-windows-acceptance-$Short-$RunId"
$EvidenceWorktree = Join-Path $env:TEMP "p6ev-$Short-$($RunId.Substring($RunId.Length - 8))"
$EvidencePath = "docs/evidence/priority-6-windows-acceptance/$Short-$RunId"

$Run = Invoke-Git 'publish-fetch' @('fetch', 'origin', 'refs/heads/main:refs/remotes/origin/main')
if ($Run.Code -ne 0) { throw "Fetch failed. Preserve $RunRoot and report it." }

$Files = @(Get-ChildItem -LiteralPath $EvidenceDir -Force)
foreach ($File in $Files) {
    if ($File.PSIsContainer -or $File.Name -notmatch '^[a-z0-9][a-z0-9.-]*\.(txt|json|md)$') {
        throw "Unexpected evidence entry: $($File.Name)"
    }
}
if (-not (Test-Path -LiteralPath (Join-Path $EvidenceDir 'result.md') -PathType Leaf)) {
    throw 'Create the result summary before publishing evidence.'
}
# Completeness only. It is not an acceptance gate and does not change a verdict.
$Required = @('result.md', 'verdicts.json', 'runbook-identity.txt', 'protect-text-selfcheck.txt',
    'source-sha.txt', 'branch-tip.txt', 'tool-paths.txt', 'os.txt', 'cpu.txt',
    'windows-version.txt', 'powershell-version.txt')
foreach ($Step in 'clone', 'branch-tip', 'candidate-containment', 'base-ancestry',
    'range-count', 'range-files', 'switch',
    'source-head-before', 'source-head-after', 'source-status-before', 'source-status-after',
    'diff-check', 'git-version', 'rustc-version', 'cargo-version', 'git-autocrlf', 'publish-fetch') {
    $Required += "$Step.stdout.txt", "$Step.stderr.txt", "$Step.exit.txt", "$Step.result.json"
}
foreach ($Check in $Checks) {
    foreach ($Suffix in 'stdout.txt', 'stderr.txt', 'exit.txt', 'result.json', 'summary.txt', 'survivors.txt') {
        $Required += "$($Check.Name).$Suffix"
    }
}
$Missing = @($Required | Where-Object { -not (Test-Path -LiteralPath (Join-Path $EvidenceDir $_) -PathType Leaf) })
if ($Missing.Count -ne 0) {
    throw "Evidence is incomplete. Missing: $($Missing -join ', '). Do not push. Report it."
}
if ((Get-Content -LiteralPath (Join-Path $EvidenceDir 'runbook-identity.txt') -Raw) -notmatch '(?m)^runbook commit \(operator-declared, not byte proof\): [0-9a-f]{40}$') {
    throw 'The runbook commit was not declared. Do not push. Report it.'
}

$SecretPattern = 'ghp_[A-Za-z0-9]{20,}|github_pat_|gho_[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}|sk-[A-Za-z0-9]{20,}|-----BEGIN [A-Z ]*PRIVATE KEY|(?i)(password|passwd|secret|api[_-]?key|bearer)\s*[:=](?!:)|(?i)authorization:(?!:)'
$UrlPattern = '://[^/\s@]*@|git@|ssh://'
$Hits = @(Select-String -Path (Join-Path $EvidenceDir '*') -Pattern $SecretPattern) +
    @(Select-String -Path (Join-Path $EvidenceDir '*') -Pattern $UrlPattern) +
    @(Select-String -Path (Join-Path $EvidenceDir '*') -SimpleMatch 'j-mckerracher/agent-orch') +
    @(Select-String -Path (Join-Path $EvidenceDir '*') -Pattern $ProfileLeakPatterns)
if ($Hits.Count -ne 0) {
    $Hits | Select-Object Path, LineNumber | Format-Table | Out-String | Write-Output
    throw 'Possible secret, remote URL, or profile name found. Do not push. Keep the raw files and report it.'
}
```

The pattern scan is a backstop. Also read every file before you publish. If a file holds a secret, a
remote URL, or a profile name, do not push it. Keep the raw file and report the blocker.

The scan fails closed on the profile name. `Protect-Text` already replaced the directory name after
each `Users` path segment with `<user>`. The scan blocks any `Users` segment that is not followed by
`<user>` (in single-backslash, slash, and doubled-backslash JSON forms) and any whole-word occurrence
of the account name, the profile directory name, or its 8.3 alias. A user account whose name is an
ordinary word that appears elsewhere in the evidence is also blocked. Report that as BLOCKED. Do not
edit the files to get past it. The scan prints file names and line numbers only, never the matching
text.

```powershell
if (Test-Path -LiteralPath $EvidenceWorktree) { throw "Worktree path exists: $EvidenceWorktree" }
# Main holds long paths. Windows needs core.longpaths and a short worktree path.
$Run = Invoke-Git 'publish-worktree' @(
    '-c', 'core.longpaths=true', 'worktree', 'add', '-b', $EvidenceBranch,
    $EvidenceWorktree, 'refs/remotes/origin/main')
if ($Run.Code -ne 0) { throw "Worktree creation failed. Preserve $RunRoot and report it." }
& $Git -C $EvidenceWorktree config core.longpaths true
if ($LASTEXITCODE -ne 0) { throw 'Cannot set core.longpaths in the evidence worktree.' }

$Destination = Join-Path $EvidenceWorktree $EvidencePath
if (Test-Path -LiteralPath $Destination) { throw "Evidence path exists: $Destination" }
[IO.Directory]::CreateDirectory($Destination) | Out-Null
foreach ($File in $Files) {
    Copy-Item -LiteralPath $File.FullName -Destination (Join-Path $Destination $File.Name)
}

& $Git -C $EvidenceWorktree add -- $EvidencePath
if ($LASTEXITCODE -ne 0) { throw 'Evidence staging failed.' }
$Staged = @(& $Git -C $EvidenceWorktree diff --cached --name-only)
if ($LASTEXITCODE -ne 0 -or $Staged.Count -eq 0) { throw 'Cannot read staged paths.' }
foreach ($Path in $Staged) {
    if (-not $Path.StartsWith("$EvidencePath/", [StringComparison]::Ordinal)) {
        throw "Unexpected staged path: $Path"
    }
}
& $Git -C $EvidenceWorktree commit `
    -m "evidence: record Priority 6 Windows run of $Short ($Overall)" `
    -m 'Co-authored-by: Copilot App <223556219+Copilot@users.noreply.github.com>'
if ($LASTEXITCODE -ne 0) { throw 'Evidence commit failed. If Git needs a user identity, report BLOCKED. Do not change credentials.' }
$EvidenceSha = & $Git -C $EvidenceWorktree rev-parse HEAD
$Changed = @(& $Git -C $EvidenceWorktree diff --name-only refs/remotes/origin/main HEAD)
foreach ($Path in $Changed) {
    if (-not $Path.StartsWith("$EvidencePath/", [StringComparison]::Ordinal)) {
        throw "Commit holds an unexpected path: $Path"
    }
}
$Run = Invoke-Bounded -Exe $Git -Name 'publish-push' -Directory $EvidenceWorktree `
    -Arguments @('push', 'origin', "HEAD:refs/heads/$EvidenceBranch")
if ($Run.Code -ne 0) { throw "Push failed. Preserve $RunRoot and report it." }
Write-Output "evidence_branch=$EvidenceBranch"
Write-Output "evidence_commit=$EvidenceSha"
Write-Output "overall=$Overall"
if ($Overall -ne 'PASS') {
    throw "Result is $Overall. Evidence is published. Report the failure."
}
```

The push output stays in `$EvidenceDir` as `publish-push.*.txt` (URL redacted). It is not published.
The only branch pushed is the evidence branch, from the evidence worktree, and it holds evidence files
only. Never push the source branch, never push to `main`, and never open a pull request.

The final `throw` makes a non-PASS result visible in the session. Report to the coordinator:

- the evidence branch and commit
- the overall result and the status of each of the 17 cargo commands, with its exit code,
  duration, and `running N tests` counts
- the OS, CPU, `rustc -Vv`, `cargo -V`, `git --version`, and `core.autocrlf` (with its origin)
- the branch tip and the pinned source commit, and that `HEAD` matched before and after
- the measured totals next to the calculated ones: `workspace-tests` 430 passed and
  1 ignored, `git-package` 73, and every per-check count in the
  table above. Label each as
  measured now versus calculated before
- the coverage gaps: Unix-only tests not run, no Windows crash-recovery (`recover-owner`),
  multi-task review, or daemon integration coverage
- for `cli-cleanup-safety`, `cli-acceptance` and `cli-engine-reconciliation-gate`: the `running N tests` line, the `survivors.txt`
  content (empty means none found), and for a failure the first failing assertion message exactly as
  printed
- for any timed-out check, the cold-build label from the status rules, as a note beside the unchanged
  `FAIL`
- that `protect-text-selfcheck.txt` and the publication scan passed
- any setting that you could not observe, including whether your agent runtime could run Sonnet 5.5
  with high reasoning

Do not edit `board.json`. The coordinator reviews the evidence and updates the board.



## What changed from the approved harness

The harness blocks derive from the Priority 5 runbook at `d976dc40a636c3462e71fb61cd6da75a1c62c72c`
(native QA accepted it on `693dd9d8…`, evidence `37bb201f…`). Preserved unchanged in behavior:
PowerShell 7 only, the pinned SHA and `HEAD` equality before and after, the declared runbook commit
(operator-declared, not byte proof), clean-checkout gates, unique run directories outside the checkout,
`Protect-Text` and its self-check (redaction before JSON serialization), `Invoke-Bounded`, the survivor
scan after every check, the fail-closed publication scan, the per-check `Expect`, `Single`, `Passed`,
`Ignored`, `Isolated`, `TimeoutSeconds` model, `PASS < INCONCLUSIVE < FAIL`, the
survivor-never-killed rule, and evidence-only branch publication. No helper was rewritten or
abstracted.

Changed:

- Re-pinned the title, source commit, source branch name (`orch/priority-6-...`), evidence branch and
  path names (`priority-6`), run-root prefix (`agent-orch-p6-acc-`), runbook file name, and links. The
  accepted baseline is now `693dd9d8…`. The `m1-ancestry` gate, the M1 pin, and the old 46497ac
  baseline are removed. The commit-count and changed-file gates are re-derived from the final source.
- Added three standalone gates after `cli-acceptance`: `git-delivery-acceptance-contract`
  (`-p orchestrator-git --test delivery_acceptance_contract`), `store-delivery-acceptance-contract`
  (`-p orchestrator-store --test delivery_acceptance_contract`), and `cli-engine-reconciliation-gate`
  (`-p orchestrator-cli --test engine_reconciliation_gate`, isolated TEMP, survivor scan, `rustc` on
  `PATH`, 10-minute limit). Each is `Single`, so a file or filter that matches nothing is caught.
- The 14 Priority 5 gates are retained with their exact argv, including `-- --nocapture` on
  `store-lock-identity-contract`. Their expected counts and name arrays are re-derived from the final
  source, not copied.
- The `git-package` and `workspace-tests` name arrays include the 8 added `delivery.rs` tests and the
  new acceptance tests.
- Not added: a standalone store `--test delivery` gate. Its tests are covered by `workspace-tests`, and
  the new store acceptance file has its own gate. A reviewer may ask for it.
- Not added: parsing of the `store-lock-identity-contract` stderr diagnostic. The approved verdict
  logic is unchanged and the reviewer reads the stderr.
- Coverage-gap and report text were updated for M2-06 and M3-04.

Known limits of this runbook:

- The check helper ends only the tree it started. It does not prove that processes started by a
  test and detached from that tree are gone. The survivor scan reports those by path and command
  line.
- The counts are calculated and must be reconciled with the measured native listing. A mismatch is
  `INCONCLUSIVE` and is a finding for the coordinator, not an edit of this file.
- The `workspace-tests` check is not isolated. The isolated standalone checks cover the tests that
  need an isolated `TEMP` and `rustc` on `PATH`.
- The survivor scan is report-only. It does not stop any process.
- The redaction patterns are anchored at the start of a word run so a very large output without a URL
  or scp form is scanned in linear time. A regression check with a large word run is part of the
  review of this runbook, not a gate of the native run.
- The declared runbook commit and the source pin are separate. The declaration is not an execution
  byte proof.
