# Priority 5 (M1-05 cleanup safety and M2-04 reconciliation) native Windows acceptance runbook (candidate 693dd9d8e1726f90b8b1c7a0a0c4c298834ae9bb)

This runbook is adapted from the approved privacy-safe harness
`docs/evidence/m1-04-windows-acceptance-46497ac-runbook.md` at docs commit
`ff8a2d3a5a7a8fa7d16cde7849ccb0bd6d9a303e`. That harness is not in the accepted source. It is
reused here. No new framework is introduced. The section "What changed from the approved
harness" lists every difference.

If the candidate changes, copy this runbook and replace the pinned SHAs, the branch names, the
counts, the test names, and the file name before anyone runs it. Do not run an old runbook
against a new candidate.

## Status of this run

**Native Windows acceptance of this candidate is unproven. No native Windows run of it exists,
and this document does not claim one.** This run records how the candidate behaves on native
Windows. It does not close M1-05 or M2-04. The coordinator reviews the evidence and decides
acceptance. Nothing here is proof from macOS, from a cross-compile, or from a review.

What the coordinator reported about the candidate (not re-run here, not Windows evidence):

- M1 item source `a017134ebc3c9900c314fb682a3e725fce55eb84`: independent review approved,
  independent QA passed on macOS. The Windows `KILL_ON_JOB_CLOSE` handle behavior (the handle is not
  inherited and is owned by the CLI) was established from source review, not from a native run.
- M1 fixture fix `4fc9ff3b83c0ceced1bc64717f1b1a94212fb8af`: review and QA passed.
- M2 item F1 completion-precedence fix (`8991c857`, plus the QA equivalents `782d177a` and
  `c1be4bba`): the independent M2 reviewer approved `c1be4bba27e819bf03427576b4c652b26067f4e1`,
  the parent of this candidate. Independent macOS QA on that commit passed with 34 runtime
  `reconcile` tests, 15 `reconciliation_contract` tests, and strict clippy.
- The coordinator's independent integration validator reported a macOS full-workspace run of
  `c1be4bba…` (`--workspace --no-fail-fast`): 460 passed, 0 failed, 1 ignored over 45 binaries,
  plus the fault-injection cleanup (5) and recovery (7) tests, with a clean `HEAD` and no survivors.
  The macOS total of 460 is not the Windows expectation and is not Windows evidence. That run is
  of the parent, not of this candidate.
- **Prior native Windows run of the parent `c1be4bba…` (evidence `ef59…`, reported by the
  coordinator): independently FAIL.** 12 of 14 gates passed and the source `HEAD` was clean before
  and after. One test, `directory_at_the_lock_path_is_never_removed`, over-constrained the Unix
  `HolderUnknown` result on Windows. That failed run is history. It is **not** acceptance proof for
  this candidate and none of its results is reused.
- This candidate `693dd9d8e1726f90b8b1c7a0a0c4c298834ae9bb` is a single-commit, single-file,
  fixture-only change on `c1be4bba…` (`crates/orchestrator-store/tests/lock_identity_contract.rs`,
  +49 −5, production code unchanged, no test name or `cfg` changed). The test now observes retention
  of the directory and its marker first, prints the actual outcome on stderr, and then asserts the
  typed result: exactly `HolderUnknown` on Unix, `HolderUnknown` or `Io` elsewhere. The owner reported
  targeted macOS tests green. Source evaluation and full macOS validation are running in parallel
  and are not claimed here. There is no native Windows result for this candidate.

## Candidate

- Repository: `https://github.com/j-mckerracher/agent-orch.git`
- Source branch (intended; the coordinator publishes it after review):
  `orch/priority-5-windows-acceptance-693dd9d`. If the dispatch message gives another name, replace
  `$SourceBranch` and its name check in step 1. Earlier published refs for `c1be4bb` are history and
  are neither reused nor overwritten.
- Source commit: `693dd9d8e1726f90b8b1c7a0a0c4c298834ae9bb`
- Accepted baseline (must be an ancestor): `46497ac98fd8d053b80b5d9b730d2fd2110c56f7`
- M1 item source (must be an ancestor): `a017134ebc3c9900c314fb682a3e725fce55eb84`
- Evidence branch (you create it): `evidence/priority-5-windows-acceptance-693dd9d-<RunId>`
- Runbook: file `docs/evidence/priority-5-windows-acceptance-693dd9d-runbook.md` on the docs branch
  `docs/priority-5-windows-acceptance-693dd9d` (intended). The coordinator gives the actual branch,
  the full runbook commit, and the URL in the dispatch message. The runbook is not part of the source
  commit. If the source branch tip is later than the source commit, test the pinned commit and
  report the tip.

Test only the exact source commit. Before any other command, `HEAD` must equal the full source
commit. If the branch does not contain the commit, stop and report the mismatch.

### Ancestry and what the candidate changes

`693dd9d8…` is 23 commits above the accepted baseline `46497ac98fd8d053b80b5d9b730d2fd2110c56f7`.
In order:

- 12 M1 commits ending at `a017134ebc3c9900c314fb682a3e725fce55eb84`.
- 6 M2 commits that were cherry-picked onto the M1 chain, the last being `921203bb48dba7758a052bd1a7beb47cf958055a`.
- `4fc9ff3b83c0ceced1bc64717f1b1a94212fb8af` (M1 inline-fixture fix), `8991c857` (M2 F1 source),
  `782d177a`, and `c1be4bba27e819bf03427576b4c652b26067f4e1` (QA equivalents).
- `693dd9d8e1726f90b8b1c7a0a0c4c298834ae9bb` (M2 store contract fixture fix, the candidate), whose
  only parent is `c1be4bba…`.

Both `46497ac9…` and `a017134e…` are ancestors of the source commit, and step 1 checks both. The
raw M2 item source `ee4eb8a800c574ff7ca873793bb39b16480c3c3a` is **not** an ancestor, because the
integration cherry-picked the M2 commits. Do not check for it. M2 inclusion is shown by patch and
file equivalence, which the integrator established and the reviewers inspected. Step 1 adds one
check that is only a consistency gate and not a proof: the set of files changed between the
baseline and the source commit must be exactly the 11 files below, and the commit count must be 23.

| File changed from the baseline | Change |
|---|---|
| `Cargo.lock` | one new edge (runtime depends on git) |
| `crates/orchestrator-cli/src/workflow.rs` | M1 cleanup classification, including the session-gone guard |
| `crates/orchestrator-cli/tests/cleanup_safety.rs` | new M1 integration test file |
| `crates/orchestrator-git/src/delivery.rs` | M2 lock identity: failure is an explicit error, the file is preserved |
| `crates/orchestrator-runtime/Cargo.toml` | the runtime-to-git dependency |
| `crates/orchestrator-runtime/src/reconcile.rs` | M2 classifier, Git evidence mapping, live-owner drift |
| `crates/orchestrator-runtime/tests/reconcile.rs` | M2 unit and classifier tests |
| `crates/orchestrator-runtime/tests/reconciliation_contract.rs` | new M2 contract test file |
| `crates/orchestrator-store/src/data_lock.rs` | M2 lock identity errors |
| `crates/orchestrator-store/tests/lock_identity_contract.rs` | new M2 store contract test file |
| `docs/evidence/m1-05-cleanup-safety/report.md` | M1 report (docs only) |

Primer and plan at the pinned commit:
[orchestration primer](https://github.com/j-mckerracher/agent-orch/blob/693dd9d8e1726f90b8b1c7a0a0c4c298834ae9bb/orch-impl-m.md)
and
[implementation plan](https://github.com/j-mckerracher/agent-orch/blob/693dd9d8e1726f90b8b1c7a0a0c4c298834ae9bb/docs/technical-implementation-plan.md).
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
for this candidate.** They were not copied from macOS totals, because many tests are `cfg(unix)`. They
were derived like this:

1. On a macOS host, `cargo check --offline --locked --workspace --tests --target
   x86_64-pc-windows-msvc` was run, and each workspace test crate was also expanded with
   `rustc -Zunpretty=expanded` through a `RUSTC_WORKSPACE_WRAPPER`. The listed tests are the
   `StaticTestName` entries of the expansion, with their `ignore` flag. This applies the real `cfg`
   gates for Windows: a `cfg(unix)` test is not listed. The `cli`, `recovery`, `multi_task_review`
   and daemon test files list zero tests.

   **Scope of that step, stated exactly.** It is **Rust-only: Windows `cfg` evaluation, Rust
   type-checking and macro expansion, nothing more.** The `libsqlite3-sys` build script needs a C
   compiler and Windows C headers for the MSVC target, which a macOS host does not have. To let
   `cargo check` proceed, the C compiler was replaced by `/usr/bin/true` (`CC_x86_64_pc_windows_msvc`)
   and the archiver by a stub that writes an 8-byte empty `!<arch>` file (`AR_x86_64_pc_windows_msvc`),
   so `libsqlite3.a` and `sqlite3.lib` were **empty placeholders**. Those two overrides were
   scratch-only. They never compiled SQLite C, never linked, never produced a runnable binary, and
   were never used to run any test. This step is therefore **not** a C build, **not** a link, **not**
   a native or cross build, **not** a test execution, and **not** acceptance proof. The wrapper and
   summarizer scripts are scratch tools and are not shipped with this runbook. The count summaries
   they produced had these SHA-256 values for the parent `c1be4bba…`: `1894cd70f3d9e0a87c9285f6a3beeaaffb4e1773d572e8c0d59a06de8c443972`,
   baseline `657a1656a3a8b3004ccb24a83b9bc2c421659748373869e298b6ffcfe65a0e34`.

   **Provenance for this candidate.** The expansion was run on the parent `c1be4bba…`, not on
   `693dd9d8…`. The candidate differs from the parent only by the body of one existing test function
   in `crates/orchestrator-store/tests/lock_identity_contract.rs` (+49 −5). A `git diff` between the
   two commits shows no added, removed, or renamed test, no change of any `#[test]`, `#[ignore]` or
   `cfg` attribute, and no other file. The Windows test descriptors are therefore the same by
   construction, and the counts below are carried over from the parent's expansion, not re-expanded
   or measured for this candidate.
2. The method was validated on the accepted baseline `46497ac98fd8d053b80b5d9b730d2fd2110c56f7`: it
   listed 334 tests with 1 ignored, which equals the measured native Windows result of the approved
   evidence commit `5d26c659c818c87ae58f2dc3d94a9ade59156f4e` (333 passed, 1 ignored). That match
   validates only the counting method. The counts below remain **expected, not measured**,
   until a native Windows run of this candidate.
3. At the parent commit the expansion lists **386 tests with 1 ignored**, so 385 passed are expected,
   and the candidate has the same descriptors: 52 more
   than the baseline. Per test binary: `cleanup_safety` +3, `orchestrator_cli` lib +4,
   `orchestrator-git` lib +3, runtime `reconcile` +20 (8 to 28), runtime `reconciliation_contract`
   +14 (new), store `lock_identity_contract` +3 (new), and `orchestrator-store` lib +5. No test of the
   baseline was removed or renamed.

The macOS counts differ because Unix-only tests exist there: 4 default `cleanup_safety` tests (3 on
Windows), 34 runtime `reconcile` tests (28), 15 contract tests (14), and 4 store contract tests (3).
A different measured Windows total is `INCONCLUSIVE` with a note, never an automatic pass or edit of
this runbook. The coordinator then reconciles the number.

Measured native baseline (evidence commit `5d26c659c818c87ae58f2dc3d94a9ade59156f4e`, candidate
`46497ac98fd8d053b80b5d9b730d2fd2110c56f7`, read from published evidence and not re-run): all six
checks passed. `workspace-tests` 333 passed and 1 ignored, `git-package` 56 passed,
`store-data-lock-integration` 7 passed and 1 ignored, `store-data-lock-lib` 2 passed,
`ipc-token-owner` 1 passed, `cli-acceptance` 2 passed.

### Check table

| Check name | Command (after `cargo`) | Expected (calculated) |
|---|---|---|
| `workspace-tests` | `test --locked --workspace --no-fail-fast` | Exit 0. 385 passed, 0 failed, 1 ignored. Every named test below reports `ok`. Binaries with `running 0 tests` are normal. |
| `git-package` | `test --locked -p orchestrator-git` | 59 passed (13 + 7 + 32 + 6 + 1 over the five test binaries; baseline 56 plus 3). |
| `git-delivery-lib` | `test --locked -p orchestrator-git --lib delivery` | One binary. 4 passed, 0 ignored. Covers lock identity `Err` and `None` failing explicitly with the file preserved, and a verified lock removed on drop. |
| `store-data-lock-integration` | `test --locked -p orchestrator-store --test data_lock` | One binary. `running 8 tests`, 7 passed, 1 ignored (`lock_child_process_helper`). Unchanged. |
| `store-data-lock-lib` | `test --locked -p orchestrator-store --lib data_lock` | One binary. 7 passed, 0 ignored (was 2). New: fresh creation `Err` and `None` fail explicitly, identity lookup failure is an I/O error and not a retry, a platform without identity still retries, and a verified creation is cleaned up on drop. |
| `store-lock-identity-contract` | `test --locked -p orchestrator-store --test lock_identity_contract -- --nocapture` | One binary. 3 passed (macOS 4; the Unix symlink test is not built on Windows). `--nocapture` is the only argument change: it makes `directory_at_the_lock_path_is_never_removed` print `directory_at_the_lock_path: acquire=<actual outcome> directory_kept=<bool> marker_intact=<bool>` on stderr. The stdout `running`, `test ... ok` and `test result` lines, and so the named and count gates, are unchanged. Read that stderr line in `store-lock-identity-contract.stderr.txt`; on PASS it must show `directory_kept=true marker_intact=true` and an outcome of `Err(HolderUnknown)` or `Err(Io(..))`. It is recorded for the reviewer. The verdict code does not parse it. |
| `runtime-reconciliation-contract` | `test --locked -p orchestrator-runtime --test reconciliation_contract` | One binary. 14 passed (macOS 15; the Unix process-group survivor test is not built on Windows). Includes the `git_state::*` matrix over real Git evidence. |
| `runtime-reconcile` | `test --locked -p orchestrator-runtime --test reconcile` | One binary. 28 passed (macOS 34). Includes the new `git_evidence::*` and `task_classification::*` tests and the preserved 100 ns identity and creation-time tests. |
| `ipc-token-owner` | `test --locked -p orchestrator-ipc --lib startup_replaces_only_regular_stale_token` | One binary. 1 passed. Unchanged. |
| `cli-lib` | `test --locked -p orchestrator-cli --lib` | One binary. 11 passed, 0 ignored (macOS 22). |
| `cli-cleanup-units` | `test --locked -p orchestrator-cli --lib cleanup` | One binary. 3 passed: the non-Unix cleanup classification unit tests. |
| `cli-session-gone-guard` | `test --locked -p orchestrator-cli --lib session_gone_guard_refuses_missing_alive_and_unknown_classifications` | One binary. 1 passed. A separate gate: the name is not matched by `--lib cleanup`. |
| `cli-cleanup-safety` | `test --locked -p orchestrator-cli --test cleanup_safety` | One binary. 3 passed (macOS 4). Isolated temp directory and survivor scan. |
| `cli-acceptance` | `test --locked -p orchestrator-cli --test portable_acceptance` | One binary. 2 passed, 0 ignored. Preserved. Isolated temp directory and survivor scan. 10-minute limit. |

All 14 checks count in the overall result. A missing verdict is a `FAIL`. A filter that runs zero
tests is never a pass: any `Single` check that reports `running 0 tests`, or more than one test
binary, is `INCONCLUSIVE`.

### Required named tests

These must report `ok` (the harness checks each by name in its own check and again in
`workspace-tests`). The names come from the source `cfg` expansion above. If the candidate renamed a
test, the check reports `INCONCLUSIVE` with a note. Do not edit the runbook.

- M1, default `cleanup_safety` (3): `cleanup_refuses_staged_deleted_and_unlanded_work_in_a_landed_worktree`,
  `cleanup_refuses_clean_worktree_while_run_owner_is_live`,
  `cleanup_follows_existing_contract_for_provably_exited_run_owner`.
- M1, non-Unix cleanup units (3): `cleanup_refuses_clean_worktree_of_unsettled_task_without_a_published_owner`,
  `cleanup_removes_clean_landed_worktree_of_settled_task_without_an_owner`,
  `cleanup_refuses_clean_worktree_when_owner_identity_is_unprovable`.
- M1, session classification guard (1): `session_gone_guard_refuses_missing_alive_and_unknown_classifications`.
- M1, preserved: `run_produces_isolated_diff_and_cleanup_preserves_primary` and
  `cancel_terminates_agent_and_descendant` (these exercise the real Windows job-object
  supervision, so no separate orphan-survival test exists or is invented for Windows).
- M2 runtime: all 14 `reconciliation_contract` and all 28 `reconcile` tests named in the arrays
  `$RuntimeContractTests` and `$RuntimeReconcileTests` in step 3, including
  `forged_windows_creation_time_is_a_mismatch_for_every_100ns_remainder`,
  `windows_identity_compares_creation_time_at_100ns_precision`, and
  `windows_owner_observation_uses_creation_time_not_only_pid`.
- M2 store: 3 `lock_identity_contract` tests, 7 `data_lock` lib tests, 7 `data_lock` integration tests.
- M2 Git: 4 `delivery` lib tests, and the 16 `git-package` tests of the earlier harness, including
  the canonicalized-root Git tests `verbatim_drive_and_unc_paths_become_plain_paths`,
  `verbatim_paths_with_other_meaning_are_left_alone`, `near_reserved_names_are_plain`,
  `for_git_is_a_no_op_off_windows`, and `verbatim_worktrees_root_creates_a_worktree`.
- IPC: `startup_replaces_only_regular_stale_token`.

### Coverage gaps (not run on Windows, not claimed)

- Unix-only tests are not built on Windows and are not Windows evidence: the orphan-tree test
  `cleanup_refuses_exited_run_owner_while_its_agent_tree_is_alive`, the fault-injection startup test
  `cleanup_refuses_running_task_that_never_registered_a_run_owner` (`cfg(all(unix, feature =
  "fault-injection"))`), the Unix process-group survivor test in `reconciliation_contract`, the Unix
  symlink test in `lock_identity_contract`, and the Unix-only unit tests in `workflow.rs` and the
  runtime `reconcile` file. Their exact integrated macOS acceptance is separate.
- `cli.rs`, `multi_task_review.rs`, `recovery.rs` in `orchestrator-cli` and `daemon.rs` in
  `orchestrator-daemon` are `cfg(unix)` and compile to zero tests. There is no Windows coverage of
  crash recovery (`recover-owner`) startup, multi-task review, or the daemon.
- The orphan-survival behavior on Windows rests on the job-object contract read from source and on
  the existing supervision gates (`cancel_terminates_agent_and_descendant`). It is not a native
  claim.
- Four known baseline lint warnings in the CLI are not a Windows gate.

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
   <RunbookCommit>:docs/evidence/priority-5-windows-acceptance-693dd9d-runbook.md` in a clone you do
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
$SourceBranch = 'orch/priority-5-windows-acceptance-693dd9d'
$SourceSha = '693dd9d8e1726f90b8b1c7a0a0c4c298834ae9bb'
# Accepted baseline and M1 item source. The source commit must descend from both.
# The raw M2 item source ee4eb8a8 is NOT an ancestor (cherry-picked); never check it.
$BaseSha = '46497ac98fd8d053b80b5d9b730d2fd2110c56f7'
$M1Sha = 'a017134ebc3c9900c314fb682a3e725fce55eb84'
$ExpectedRangeCount = 23
$ExpectedRangeFiles = @(
    'Cargo.lock',
    'crates/orchestrator-cli/src/workflow.rs',
    'crates/orchestrator-cli/tests/cleanup_safety.rs',
    'crates/orchestrator-git/src/delivery.rs',
    'crates/orchestrator-runtime/Cargo.toml',
    'crates/orchestrator-runtime/src/reconcile.rs',
    'crates/orchestrator-runtime/tests/reconcile.rs',
    'crates/orchestrator-runtime/tests/reconciliation_contract.rs',
    'crates/orchestrator-store/src/data_lock.rs',
    'crates/orchestrator-store/tests/lock_identity_contract.rs',
    'docs/evidence/m1-05-cleanup-safety/report.md'
)
# Required: the full 40-character commit of THIS runbook file, as the
# coordinator states it in the dispatch message. It is recorded as declared.
# It does not prove that the commands pasted here match that commit.
$RunbookCommit = ''
if ($RunbookCommit -cnotmatch '^[0-9a-f]{40}$') {
    throw 'Set RunbookCommit to the full lowercase 40-character commit from the dispatch message.'
}
foreach ($Sha in $SourceSha, $BaseSha, $M1Sha) {
    if ($Sha -cnotmatch '^[0-9a-f]{40}$') {
        throw 'A pinned commit is not a full lowercase 40-character SHA.'
    }
}
if ($SourceBranch -cnotmatch '^orch/priority-5-[a-z0-9-]+$') {
    throw 'The source branch name is not valid.'
}
$Short = $SourceSha.Substring(0, 7)
$RunId = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ') + '-' +
    [Guid]::NewGuid().ToString('N')
$RunRoot = Join-Path $env:TEMP "agent-orch-p5-acc-$RunId"
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
$Run = Invoke-Git 'm1-ancestry' @('merge-base', '--is-ancestor', $M1Sha, $SourceSha)
if ($Run.Code -ne 0) { throw "The source commit does not descend from $M1Sha." }
# Consistency gates only. They do not prove patch equivalence of the M2 cherry-picks.
$Run = Invoke-Git 'range-count' @('rev-list', '--count', "$BaseSha..$SourceSha")
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or [int]$Run.Out[0] -ne $ExpectedRangeCount) {
    throw "The commit count above the baseline is not $ExpectedRangeCount."
}
$Run = Invoke-Git 'range-files' @('diff', '--name-only', $BaseSha, $SourceSha)
if ($Run.Code -ne 0) { throw 'Cannot list the changed files.' }
if ((@($Run.Out | Sort-Object) -join "`n") -cne (@($ExpectedRangeFiles | Sort-Object) -join "`n")) {
    throw 'The files changed above the baseline are not the expected 11. Report it.'
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
    "runbook file: docs/evidence/priority-5-windows-acceptance-693dd9d-runbook.md`n" +
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
    'unresolved_legacy_windows_owner_has_audited_manual_recovery_path'
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
# Every named test must also report ok inside the full workspace run.
$WorkspaceTests = $GitTests + $GitDeliveryTests + $DataLockTests + $DataLockLibTests +
    $LockContractTests + $RuntimeContractTests + $RuntimeReconcileTests + $IpcTokenTests +
    $CliLibTests + $CliCleanupSafetyTests + $CliAcceptanceTests

# Expect: tests that must report ok.
# Single: the run must have exactly one test binary, and that binary must run
#   at least one test. This is how a filter that matches nothing is caught.
# Passed, Ignored: exact counts. All are CALCULATED from the source for Windows,
#   not measured. A different measured count is INCONCLUSIVE, never edited here.
# Isolated: run with TEMP/TMP under $TestTmp and rustc on PATH.
# TimeoutSeconds: limit for the command (default 1800).
$Checks = @(
    @{ Name = 'workspace-tests'; Expect = $WorkspaceTests; Single = $false
       Passed = 385; Ignored = 1
       Arguments = @('test', '--locked', '--workspace', '--no-fail-fast') },
    @{ Name = 'git-package'; Expect = $GitTests + $GitDeliveryTests; Single = $false
       Passed = 59
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
       Passed = 11; Ignored = 0
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
           '--test', 'portable_acceptance') }
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
# Priority 5 Windows acceptance of $SourceSha (M1-05 cleanup safety, M2-04 reconciliation)

This run does not close M1-05 or M2-04. The coordinator decides acceptance. The expected counts are
calculated from source for Windows. They are not measured before this run, and no macOS or review
result is Windows evidence.

- Run ID: $RunId
- UTC date: $([DateTime]::UtcNow.ToString('o'))
- Source branch: $SourceBranch
- Branch tip at clone time: $BranchTip
- Source commit: $SourceSha
- HEAD matched the source commit before the checks and after them: yes
  (source-head-before.stdout.txt, source-head-after.stdout.txt)
- Baseline and M1 ancestry, commit count, and changed-file set: base-ancestry, m1-ancestry,
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
  zero tests on Windows. There is no Windows crash recovery (recover-owner), multi-task review, or
  daemon integration coverage.
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
$EvidenceBranch = "evidence/priority-5-windows-acceptance-$Short-$RunId"
$EvidenceWorktree = Join-Path $env:TEMP "p5ev-$Short-$($RunId.Substring($RunId.Length - 8))"
$EvidencePath = "docs/evidence/priority-5-windows-acceptance/$Short-$RunId"

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
foreach ($Step in 'clone', 'branch-tip', 'candidate-containment', 'base-ancestry', 'm1-ancestry',
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
    -m "evidence: record Priority 5 Windows run of $Short ($Overall)" `
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
- the overall result and the status of each of the 14 commands, with its exit code, duration, and
  `running N tests` counts
- the OS, CPU, `rustc -Vv`, `cargo -V`, `git --version`, and `core.autocrlf` (with its origin)
- the branch tip and the pinned source commit, and that `HEAD` matched before and after
- the measured totals next to the calculated ones: `workspace-tests` 385 passed and 1 ignored
  (386 listed), `git-package` 59, and every per-check count in the table above. Label each as
  measured now versus calculated before
- the coverage gaps: Unix-only tests not run, no Windows crash-recovery (`recover-owner`),
  multi-task review, or daemon integration coverage
- for `cli-cleanup-safety` and `cli-acceptance`: the `running N tests` line, the `survivors.txt`
  content (empty means none found), and for a failure the first failing assertion message exactly as
  printed
- for any timed-out check, the cold-build label from the status rules, as a note beside the unchanged
  `FAIL`
- that `protect-text-selfcheck.txt` and the publication scan passed
- any setting that you could not observe, including whether your agent runtime could run Sonnet 5.5
  with high reasoning

Do not edit `board.json`. The coordinator reviews the evidence and updates the board.

## What changed from the approved harness

The harness blocks derive from `ff8a2d3a5a7a8fa7d16cde7849ccb0bd6d9a303e`. Preserved unchanged in
behavior: PowerShell 7 only, the pinned SHA and `HEAD` equality before and after, the declared
runbook commit (operator-declared, not byte proof), clean-checkout gates, unique run directories
outside the checkout, `Protect-Text` and its self-check, the fail-closed publication scan, the
per-check `Expect`, `Single`, `Passed`, `Ignored`, `Isolated`, `TimeoutSeconds` model, `PASS <
INCONCLUSIVE < FAIL`, the survivor-never-killed rule, and evidence-only branch publication.

Changed, because the approved harness assumed one candidate and a Git-fix change set:

- Re-pinned from the approved `9100160a` harness to candidate `693dd9d8…` (parent `c1be4bba…`): the
  title, SHAs, branch names, file names, links, ancestry prose, and the commit-count gate (22 to 23;
  the 11-file list is unchanged because the new commit edits a file already in the list). The
  prior failed native run of the parent is recorded as history. The only execution change is
  `-- --nocapture` on `store-lock-identity-contract`. Privacy, bounded-capture, redaction and
  survivor behavior, all 14 checks, their counts and their named-test gates are unchanged.
- Re-pinned the title, source commit, baseline, branch names, paths, links, ancestry prose, and
  changed-file list. Added the M1 ancestor `a017134e…`, a commit-count gate, and a changed-file gate.
  Raw `ee4eb8a8` is deliberately not checked.
- Replaced the 6 checks with 14. The 6 original gates are preserved (`workspace-tests`,
  `git-package`, `store-data-lock-integration`, `store-data-lock-lib`, `ipc-token-owner`,
  `cli-acceptance`). Their counts moved with the source: 333 to 385 passed, 56 to 59, `store-data-lock-lib`
  2 to 7. Eight checks are new: `git-delivery-lib`, `store-lock-identity-contract`,
  `runtime-reconciliation-contract`, `runtime-reconcile`, `cli-lib`, `cli-cleanup-units`,
  `cli-session-gone-guard` (separate because `--lib cleanup` does not match it), and
  `cli-cleanup-safety` (isolated).
- Stale assumptions in the approved harness, now fixed: `Invoke-Native` ran synchronously through a
  text pipeline with no time limit and so could hang on clone, fetch, or push, merged status with
  output handling, and re-encoded bytes. It is replaced by `Invoke-Bounded`: one process, one exit
  status, asynchronous byte-exact drain of both streams to a raw directory outside the checkout and
  evidence, a bounded wait, owned process-tree termination on timeout, and a redacted text copy.
  Every Git step now goes through it, so each has a captured stdout, stderr, exit, and duration.
- `result.json` now carries `started_utc`, `duration_seconds`, `timeout_seconds`, an environment
  summary (`TEMP`, `TMP`, `CARGO_TARGET_DIR`, isolation, `rustc` on `PATH`), and the size and SHA-256 of
  the raw bytes. Its `command`, `working_directory`, `error`, and the verdict notes are redacted
  before they are serialized, and the report is redacted before it is written.
- The self-check now also covers URL and scp-style samples.
- The survivor scan, previously for `cli-acceptance` only, runs after every check. Isolated TEMP is
  per check.
- A failure with a non-empty `error` such as a stream that never closed is a `FAIL`.
- The primer links point at the pinned commit. Stale statements about the earlier Git-fix candidate
  were removed.

Known limits of this runbook:

- The check helper ends only the tree it started. It does not prove that processes started by a
  test and detached from that tree are gone; the survivor scan reports those by path and command
  line, and the test cancel scenarios are the Windows evidence for job closure.
- The counts are calculated and must be reconciled with the measured native listing. A mismatch is
  `INCONCLUSIVE` and is a finding for the coordinator, not an edit of this file.
- The `workspace-tests` check is not isolated; the isolated standalone checks (`cli-cleanup-safety`,
  `cli-acceptance`) cover the tests that need an isolated `TEMP` and `rustc` on `PATH`.
- The survivor scan is report-only. It does not stop any process.
- The redaction patterns are anchored at the start of a word run so a very large output without a URL
  or scp form is scanned in linear time. A regression check with a large word run is part of the
  review of this runbook, not a gate of the native run.
