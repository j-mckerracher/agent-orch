# Priority 7 (M2-05 crash/concurrency, M3-05 multi-task review, M3-06 delivery reconciliation, M4-04 adapter conformance) native Windows acceptance runbook (candidate cee8a81f63f1d876ef26c8920a052910cdd7fe3d)

This runbook is adapted from the accepted Priority 6 runbook
`docs/evidence/priority-6-windows-acceptance-b5db71a-runbook.md` at docs commit
`c330e89460b32ea6103c1dd862d5fd84aebefe46` (source `b5db71a3b395d49c0767d8019c502d9fadfd0aa3`,
accepted native evidence `a77773b3c4640fdb09420d3f4cee57a45d52aa3a`). Those are immutable git objects
and are absent from the source checkout. The helper blocks (`Protect-Text`, `Invoke-Bounded`, the
survivor scan, the publication scan, the verdict model) are reused unchanged. No new framework is
introduced. The section "What changed from the approved harness" lists every difference.

Do not run an old runbook against a new candidate.

## Status of this run

**Native Windows acceptance of this candidate is unproven. No native Windows run of it exists, and
this document does not claim one.** The accepted Priority 6 native evidence `a77773b3…` proves
**only** source `b5db71a3…`. It is not evidence for this candidate, and none of its counts (430
passed workspace, 73 git package, 1 ignored) is reused. Mac results and the static calculation below
are not Windows evidence.

This run records how the candidate behaves on native Windows. It does not close M2-05, M3-05, M3-06,
or M4-04. The coordinator decides acceptance. The plan is
`docs/technical-implementation-plan.md`.

Items under test, in the board's words:

- **M2-05.** Inject process and runtime failures with two parallel tasks and verify no changes or
  audit events are silently lost.
- **M3-05.** Demonstrate dependency execution, independent verification and approval enforcement
  across restarts without unapproved merging.
- **M3-06.** Inspect persisted delivery steps and classify each as confirmed, not applied, or
  ambiguous against the actual Git ref. Preserve ambiguous results for review and never apply or
  remove a delivery during reconciliation.
- **M4-04.** Cover framing, launch, cancellation, errors, unexpected EOF, unsupported features,
  interruption and approvals for each adapter. Each supported agent completes the same conformance
  workflow on both operating systems.

## Candidate

- Repository: `https://github.com/j-mckerracher/agent-orch.git`
- Source branch (the coordinator publishes it after review): `orch/priority-7-windows-acceptance-cee8a81`
- Source commit: `cee8a81f63f1d876ef26c8920a052910cdd7fe3d`
- Accepted baseline (must be an ancestor): `b5db71a3b395d49c0767d8019c502d9fadfd0aa3`
  (Priority 6 source, history only; its evidence is not checked or reused)
- Evidence branch (you create it): `evidence/priority-7-windows-acceptance-cee8a81-<RunId>`
- Runbook: file `docs/evidence/priority-7-windows-acceptance-cee8a81-runbook.md`. The coordinator gives the branch, the full runbook commit, and the
  URL in the dispatch message. The runbook commit has the source commit as its **only parent** and
  changes only this file. The declared runbook commit is operator-declared. Step 1 fetches it,
  checks its shape, and records the SHA-256 of the exact runbook bytes. That pins the bytes in
  evidence. It still does not prove that the commands you pasted match them.
- If the source branch tip is later than the source commit, test the pinned commit and report the tip.

Test only the exact source commit. Before any other command, `HEAD` must equal the full source
commit. If the branch does not contain the commit, stop and report the mismatch.

### Ancestry and what the candidate changes

The source is five linear commits above `b5db71a3…`, in this order: `7deb03f6` (M2-05), `61b24459`
(M3-05), `620f3194` (M3-06), `517d54b4` (M4-04), and `cee8a81f` (M3-06 live-store no-write proof with a
tolerant snapshot and a sound baseline; it changes only
`delivery_reconciliation_contract.rs`). Tree `7c9817b0090bad4b32cb90ca81bc99700ea33c03`. The
integration re-created the owner commits as equivalent commits. **The raw owner commits are not ancestors, and the runbook
does not check them.** The commit-count and changed-file gates are consistency gates only. They do
not prove patch equivalence.

The 20 paths changed above `b5db71a3…` (the `$ExpectedRangeFiles` gate checks this exact set):

| Path | Item and change |
|---|---|
| `Cargo.lock` | M2-05: `orchestrator-cli` gains the `sqlx` dev-dependency (already locked version) |
| `crates/orchestrator-cli/Cargo.toml` | M2-05: `sqlx` dev-dependency |
| `crates/orchestrator-cli/src/workflow.rs` | M2-05: portable fault-injection hooks (feature `fault-injection`) |
| `crates/orchestrator-cli/tests/crash_concurrency.rs` | M2-05: new, 6 always-on and 2 `fault-injection` tests |
| `crates/orchestrator-cli/tests/support/m2_agent.rs` | M2-05: new fake agent used by the crash tests |
| `crates/orchestrator-cli/tests/multi_task_review.rs` | M3-05: file-wide `cfg(unix)` removed, 6 tests |
| `crates/orchestrator-git/src/delivery.rs`, `src/delivery_reconcile.rs` | M3-06: exact-name, symref, no-deref ref handling |
| `crates/orchestrator-git/tests/delivery.rs`, `tests/delivery_reconcile.rs` | M3-06: +2 and +5 tests |
| `crates/orchestrator-cli/tests/delivery_reconciliation_contract.rs` | M3-06: new live-store read-only snapshot tests, 12 |
| `crates/orchestrator-agents/src/{claude_code_adapter,codex_adapter,codex_parser,pi_rpc_adapter}.rs` | M4-04: failed provider turns and late cancel |
| `crates/orchestrator-agents/tests/conformance.rs`, `conformance/{fake_child,harness}.rs` | M4-04: conformance 24 to 37 |
| `docs/evidence/m3-05-macos/workflow.md`, `docs/evidence/m4-04-macos/conformance-report.md` | historical Mac notes (docs only) |

Primer and plan at the pinned commit:
[orchestration primer](https://github.com/j-mckerracher/agent-orch/blob/cee8a81f63f1d876ef26c8920a052910cdd7fe3d/orch-impl-m.md)
and
[implementation plan](https://github.com/j-mckerracher/agent-orch/blob/cee8a81f63f1d876ef26c8920a052910cdd7fe3d/docs/technical-implementation-plan.md).
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

**PowerShell 7 (`pwsh`) only.** Do not use Windows PowerShell 5.1. The first block stops on it. The
host needs Windows 11 or later, Git for Windows, and the MSVC Rust toolchain. `rustc` must be on
`PATH` for the tests that compile a fake agent or fake child. Use a short checkout path and
`core.longpaths` as the blocks do.

## Checks

Each check runs once. Each records its command, environment summary, exit code, duration, full
stdout and stderr, and every `running N tests` and `test result` line.

### How the Windows counts were derived (calculated, not measured)

Every count below is **calculated by static source reading of the source tree
`cee8a81f63f1d876ef26c8920a052910cdd7fe3d` for the Windows cfg set. None is measured on Windows.**
(The earlier union tree `cd507cec…` gives identical names and counts.)
Three classes stay separate and are never mixed:

- **Mac-measured** (reported by the item owners and coordinator, not Windows evidence): M2-05 default 6
  and fault-injection 8; M3-05 6; M3-06 CLI contract 12, git `delivery` 23, `delivery_reconcile` 12,
  store `delivery` 11; M4-04 conformance 37 and agents 92 (history only: RED at the tests-only commit
  `843bc40c…` was conformance 37 tests, 33 passed / 4 failed; GREEN at the fix commit `32563640…` was agents 92 passed).
  The authoritative combined Mac QA at `cee8a81f…` (raw evidence
  `.agent-worktrees/p7-combined-evidence-cee8a81f-20261109T143318Z-21874`) is now final and passed with
  `--locked`: workspace 550 passed / 0 failed / 1 ignored; agents per binary 16/10/10/37/10/9 = 92, plus the
  Claude error-reason unit test; `crash_concurrency` with `fault-injection` 8 (and 3 repeats of 8),
  `recovery` 7, `cleanup_safety` 5, full CLI package with `fault-injection` 114 passed; no lock or source
  change and no surviving processes. These are Mac numbers only. The Windows 477 / 1 ignored (default) and
  479 / 1 ignored (`fault-injection`) in this document stay calculated, not measured.
- **Calculated for Windows** (this document): from `#[test]` attributes and `cfg` attributes.
  `cfg(unix)` bodies and files compile to zero tests or to silent no-ops on Windows.
- **Native-measured**: only what this run records. A different measured count is `INCONCLUSIVE`,
  never edited here.

Each number maps to one argv, package, binary, selector, and platform. A filtered selector count
(`git-delivery-lib`, `--lib delivery`) is not a package total. Do not reuse the Priority 6
totals. Windows totals: default features 477 passed, 0 failed, 1 ignored (P6 430, plus 47). With
`--features fault-injection` the CLI crash binary has 8 tests instead of 6.

| Added above P6 on Windows | Tests |
|---|---|
| M2-05 `crash_concurrency` (default 6, `fault-injection` 8) | +6 |
| M3-05 `multi_task_review` (was `cfg(unix)`, zero on Windows at P6) | +6 |
| M3-06 git `delivery` +2, git `delivery_reconcile` +5, CLI `delivery_reconciliation_contract` +12 | +19 |
| M4-04 agents lib +3, conformance +13 | +16 |
| Total | +47 (430 to 477) |

For contrast only (Unix, calculated, not a Windows count): default 550 passed and 1 ignored. M3-05's
6 tests were already counted on Unix.

Per-binary Windows counts (calculated):

- agents: lib 16, `claude_code_adapter` 10, `codex_adapter` 10, `conformance` 37, `pi_rpc_adapter` 10,
  `routing` 9. Total 92, none ignored.
- cli: lib 17, `cleanup_safety` 3, `cli` 0, `recovery` 0, `portable_acceptance` 2,
  `engine_reconciliation_gate` 22, `multi_task_review` 6, `crash_concurrency` 6 (8 with
  `fault-injection`), `delivery_reconciliation_contract` 12.
- git: lib 32, `delivery` 23, `delivery_acceptance_contract` 6, `delivery_reconcile` 12, `reconcile` 6,
  `scoped_diff` 1. Package total 80 (P6: 73).
- store, runtime, ipc, core, daemon: unchanged from the P6 derivation.

### Check table

The Priority 6 gates keep their argv, `Single` and `Isolated` flags, and calculated counts. `git-package`
changes from 73 to 80 and `workspace-tests` from 430 to 477. The eight new gates are marked **new**.

| Check | Command (after `cargo`) | Expect (Windows, calculated) |
|---|---|---|
| `workspace-tests` | `test --locked --workspace --no-fail-fast` | Exit 0. 477 passed, 0 failed, 1 ignored. Every named test below reports `ok`. |
| `git-package` | `test --locked -p orchestrator-git` | Exit 0. 80 passed. |
| `git-delivery-lib` | `test --locked -p orchestrator-git --lib delivery` | One binary, 4 passed (filtered selector). |
| `store-data-lock-integration`, `store-data-lock-lib`, `store-lock-identity-contract`, `runtime-reconciliation-contract`, `runtime-reconcile`, `ipc-token-owner`, `cli-lib`, `cli-cleanup-units`, `cli-session-gone-guard`, `cli-cleanup-safety`, `cli-acceptance`, `git-delivery-acceptance-contract`, `store-delivery-acceptance-contract`, `cli-engine-reconciliation-gate` | unchanged from Priority 6 | 7 (1 ignored), 7, 3, 14, 28, 1, 17, 3, 1, 3, 2, 6, 3, 22 |
| `cli-crash-concurrency` **new** | `test --locked -p orchestrator-cli --test crash_concurrency` | One binary, 6 passed. |
| `cli-crash-concurrency-fault-injection` **new** | `test --locked -p orchestrator-cli --features fault-injection --test crash_concurrency` | One binary, 8 passed (the 6 above plus 2 fault tests). |
| `cli-multi-task-review` **new** | `test --locked -p orchestrator-cli --test multi_task_review` | One binary, 6 passed. |
| `cli-delivery-reconciliation-contract` **new** | `test --locked -p orchestrator-cli --test delivery_reconciliation_contract` | One binary, 12 passed. |
| `git-delivery-integration` **new** | `test --locked -p orchestrator-git --test delivery` | One binary, 23 passed. |
| `git-delivery-reconcile` **new** | `test --locked -p orchestrator-git --test delivery_reconcile` | One binary, 12 passed. |
| `agents-tests` **new** | `test --locked -p orchestrator-agents --tests` | 92 passed. Per-binary `running` lines in cargo order: 16, 10, 10, 37, 10, 9. |
| `agents-conformance` **new** | `test --locked -p orchestrator-agents --test conformance` | One binary, 37 passed. |

All 25 commands use `--locked` and literal argument lists. The new CLI and agents gates run with the
`Isolated` environment (private `TEMP`/`TMP`, `rustc` on `PATH`) and a survivor scan, like
`cli-cleanup-safety`. A `Single` gate fails closed on zero tests or more than one binary. The new
`RunningSequence` field makes `agents-tests` fail closed if the per-binary `running` lines differ from
the calculated sequence. A `Passed` or `Ignored` mismatch is `INCONCLUSIVE`.

### Required named tests (names read from the source tree at `cee8a81f…`)

The arrays in the `$Checks` block hold the exact Windows names. Every name must report `ok`, inside
its own gate and inside the workspace run. New names, by item:

- **M2-05 (8).** Always-on: `two_concurrent_runs_overlap_and_keep_their_changes`,
  `killed_run_owner_is_reconciled_while_parallel_task_completes`,
  `cancel_targets_one_run_and_leaves_the_parallel_task_untouched`,
  `agent_runtime_failure_keeps_changes_and_parallel_task_and_a_new_run_succeeds`,
  `event_write_failure_rolls_back_one_task_while_parallel_task_stays_live`,
  `running_task_with_no_live_runs_left_is_reconciled_by_the_next_run`. `fault-injection` only:
  `injected_owner_crash_is_reconciled_while_parallel_task_completes`,
  `prelaunch_crash_fails_closed_without_disturbing_the_parallel_task`.
- **M3-05 (6).** `multi_task_review`: `decomposition_replaces_proposal_ids_and_rewrites_dependencies`,
  `delivery_requires_exact_approval_and_reapproval_after_candidate_change`,
  `local_review_writes_only_approved_review_refs_without_merging`,
  `scheduler_starts_dependencies_before_dependent_task`,
  `verification_blocks_failed_commands_and_names_protected_paths`,
  `workflow_records_survive_reopen_after_each_stage`.
- **M3-06 (19).** 2 in git `delivery.rs`, 5 in git `delivery_reconcile.rs`, 12 in the CLI contract
  file (names in the arrays below).
- **M4-04 (16).** Conformance adds `{codex,claude_code,pi_rpc}_{crlf_framing,cancel_after_exit,cancellation_tree}`,
  `codex_nonterminal_warning`, `codex_item_outside_turn`, and `{claude_code,pi_rpc}_error_result`.
  The agents lib adds
  `claude_code_adapter::tests::error_result_with_clean_exit_is_not_completed`,
  `codex_parser::tests::turn_failed_is_an_unknown_type_and_never_completes_the_turn`, and
  `pi_rpc_adapter::tests::aborted_turn_with_clean_exit_is_not_completed`.

### Scope and disclosures (what these tests prove and do not prove)

- **M2-05.** The two tasks start sequentially with live overlap. They do not start simultaneously, and
  there is no write-contention proof. The SQLite fault covers only the Admit event insert, rolled
  back while the second task is live at a barrier. Later transitions are not faulted. A runtime
  failure retries with a fresh task. A rollback is retried by a manual store transition. There is no
  exit-0-to-CandidateReady fault. On Windows the crash is `Child::kill()` plus `process::exit(137)`
  hooks. The Job Object owner-death reaping is proven by a bounded wait with **no test-issued kill**.
  A test-side `taskkill /F /PID` runs only after the test re-observes the exact PID and start time.
  The old `recovery.rs` and external `sqlite3` tests stay Unix-only and are **not** Windows proof.
- **M3-05.** One OS process: real library composition, controlled scheduler snapshots, and a durable
  SqliteStore close and reopen with opaque digests. It is not daemon, engine-CLI, or process-crash
  proof. The six tests include stale-candidate and stale-digest negatives, persisted reopened
  approval negatives, approved-only review refs, and verifier failure. The failing validation command
  is `git rev-parse --verify --quiet refs/heads/missing`, which exits 1.
- **M3-06.** The CLI contract takes read-only snapshots of the live store, Git refs, and worktrees
  before and after reconciliation, with mutation-sensitivity tests. No delivery to a real remote ran.
- **M4-04.** Fake children only: no live provider and no credentials. Native `rustc` must resolve
  inside the tests. Arguments with spaces and metacharacters are literal (the `;$(echo must-not-run)`
  case). The executable resolver accepts `.exe` only, not `.cmd` or `.bat`. `taskkill /T /F` is a
  **forced kill**, not a graceful Unix SIGINT, so the interruption tests prove forced-kill behavior
  on Windows. CRLF framing is tolerance only. Approvals are no-grant and routing-refusal only. The
  adapter source blobs and the tests tree at the final source must match the reviewed blobs
  (`claude_code_adapter.rs` `e16d9041…`, `codex_adapter.rs` `4b50bb3c…`, `codex_parser.rs` `906c436e…`,
  `pi_rpc_adapter.rs` `0b9500ae…`, `tests/` tree `18d669e5…`) or the M4-04 QA is re-reviewed. They were
  verified identical at `cee8a81f…`.
- Mac counts and the historical 24, 76, and 277 conformance counts are not Windows counts.

### Coverage gaps

- Unix-only tests did not run on Windows: `recovery.rs`, `cli.rs`, `daemon.rs`, the orphan-tree test,
  the runtime process-group survivor test, the store symlink test, and the Unix-only unit tests. Their
  macOS results are separate and are not Windows evidence.
- Native Windows crash coverage is the `crash_concurrency` binary, not `recover-owner` or the daemon.
- The workspace run is not isolated. A fake child that needs `rustc` on `PATH` is covered by the
  isolated standalone gates. A `rustc_dir_on_path` of false in `workspace-tests/result.json` is a note.
- No lint or format gate (`clippy`, `rustfmt`) runs in this harness.

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
   <RunbookCommit>:docs/evidence/priority-7-windows-acceptance-cee8a81-runbook.md` in a clone you do
   not otherwise use. Do not edit a block except as step 1 allows (`$SourceBranch` and its name
   check, and `$RunbookCommit`).
3. Set `$RunbookCommit` in the first block to that full commit. The block stops when it is empty. The
   value is recorded in `runbook-identity.txt` as **operator-declared**. It is not proof of the bytes
   that you ran; step 2 above is what keeps the bytes honest. The block also fetches that commit,
   checks that its only parent is the source commit and that it changes only the runbook file, and
   records the Git blob id, byte length, and SHA-256 of the exact runbook bytes.
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
$SourceBranch = 'orch/priority-7-windows-acceptance-cee8a81'
$SourceSha = 'cee8a81f63f1d876ef26c8920a052910cdd7fe3d'
# The runbook file path in the runbook commit (see Candidate).
$RunbookPath = 'docs/evidence/priority-7-windows-acceptance-cee8a81-runbook.md'
# Accepted Priority 6 source. It proves only itself and must be an ancestor of the
# final source. Older pins (693dd9d8, 46497ac) are history and are not checked.
# Raw owner commits (M2-05, M3-05, M3-06, M4-04) are not ancestors because the
# integration re-created them as equivalent commits; never check one.
$BaseSha = 'b5db71a3b395d49c0767d8019c502d9fadfd0aa3'
$ExpectedRangeCount = 5
$ExpectedRangeFiles = @(
    'Cargo.lock',
    'crates/orchestrator-agents/src/claude_code_adapter.rs',
    'crates/orchestrator-agents/src/codex_adapter.rs',
    'crates/orchestrator-agents/src/codex_parser.rs',
    'crates/orchestrator-agents/src/pi_rpc_adapter.rs',
    'crates/orchestrator-agents/tests/conformance.rs',
    'crates/orchestrator-agents/tests/conformance/fake_child.rs',
    'crates/orchestrator-agents/tests/conformance/harness.rs',
    'crates/orchestrator-cli/Cargo.toml',
    'crates/orchestrator-cli/src/workflow.rs',
    'crates/orchestrator-cli/tests/crash_concurrency.rs',
    'crates/orchestrator-cli/tests/delivery_reconciliation_contract.rs',
    'crates/orchestrator-cli/tests/multi_task_review.rs',
    'crates/orchestrator-cli/tests/support/m2_agent.rs',
    'crates/orchestrator-git/src/delivery.rs',
    'crates/orchestrator-git/src/delivery_reconcile.rs',
    'crates/orchestrator-git/tests/delivery.rs',
    'crates/orchestrator-git/tests/delivery_reconcile.rs',
    'docs/evidence/m3-05-macos/workflow.md',
    'docs/evidence/m4-04-macos/conformance-report.md'
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
if ($SourceBranch -cnotmatch '^orch/priority-7-[a-z0-9-]+$') {
    throw 'The source branch name is not valid.'
}
$Short = $SourceSha.Substring(0, 7)
if ($RunbookPath -cne "docs/evidence/priority-7-windows-acceptance-$Short-runbook.md") {
    throw 'RunbookPath is not the Priority 7 runbook file for this source commit.'
}
$RunId = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ') + '-' +
    [Guid]::NewGuid().ToString('N')
$RunRoot = Join-Path $env:TEMP "agent-orch-p7-acc-$RunId"
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

# Runbook byte pin. Fetch the declared runbook commit, check its shape, and record the
# SHA-256 of its exact bytes. The text is kept only in $RawDir: it holds the repository
# URL, so it is never published. This does not prove that the commands pasted into this
# session match those bytes; it makes the bytes comparable after the run.
$Run = Invoke-Git 'runbook-fetch' @('fetch', 'origin', $RunbookCommit)
if ($Run.Code -ne 0) { throw 'Cannot fetch the declared runbook commit. Report BLOCKED. Do not continue.' }
$Run = Invoke-Git 'runbook-type' @('cat-file', '-t', $RunbookCommit)
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or $Run.Out[0] -cne 'commit') {
    throw 'The declared runbook commit is not a commit.'
}
$Run = Invoke-Git 'runbook-parent' @('rev-list', '--parents', '-n', '1', $RunbookCommit)
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or $Run.Out[0] -cne "$RunbookCommit $SourceSha") {
    throw 'The runbook commit does not have the source commit as its only parent.'
}
$Run = Invoke-Git 'runbook-files' @('diff', '--name-only', $SourceSha, $RunbookCommit)
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or $Run.Out[0] -cne $RunbookPath) {
    throw 'The runbook commit does not change exactly the runbook file.'
}
$Run = Invoke-Git 'runbook-blob' @('rev-parse', "${RunbookCommit}:$RunbookPath")
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or $Run.Out[0] -cnotmatch '^[0-9a-f]{40}$') {
    throw 'Cannot read the runbook blob id.'
}
$RunbookBlob = $Run.Out[0]
$Run = Invoke-Git 'runbook-bytes' @('cat-file', 'blob', $RunbookBlob)
if ($Run.Code -ne 0 -or $Run.error) { throw 'Cannot read the runbook bytes.' }
# The text copy holds the repository URL and is never published. The raw bytes stay in $RawDir.
Remove-Item -LiteralPath (Join-Path $EvidenceDir 'runbook-bytes.stdout.txt') -Force
$RunbookBytes = $Run.stdout_bytes
$RunbookSha256 = $Run.stdout_sha256
if ($RunbookBytes -lt 1 -or $RunbookSha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'The runbook byte record is invalid.' }

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
    "runbook file: $RunbookPath`n" +
    "runbook commit (operator-declared, not byte proof): $RunbookCommit`n" +
    "runbook commit parent (verified): $SourceSha`n" +
    "runbook blob id: $RunbookBlob`n" +
    "runbook bytes: $RunbookBytes`n" +
    "runbook sha256: $RunbookSha256`n" +
    "pasted commands vs these bytes: not verified`n" +
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
# Read from blob 67bed0953a1c78bee70d7dd9c9e18cc6e4a73c33 (M2 commit 1d66d648); the same 22 names
# are in blob c391be487c9601aa58a44c7cce91c40915a2cd36 at the candidate. 22 tests, no
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

# P7 additions. Names are read from the final source for the Windows cfg set, never from a
# candidate branch. The fault-injection names run only in the fault-injection gate.
$CliCrashConcurrencyTests = @(
    'agent_runtime_failure_keeps_changes_and_parallel_task_and_a_new_run_succeeds',
    'cancel_targets_one_run_and_leaves_the_parallel_task_untouched',
    'event_write_failure_rolls_back_one_task_while_parallel_task_stays_live',
    'killed_run_owner_is_reconciled_while_parallel_task_completes',
    'running_task_with_no_live_runs_left_is_reconciled_by_the_next_run',
    'two_concurrent_runs_overlap_and_keep_their_changes'
)
$CliCrashFaultInjectionTests = @(
    'injected_owner_crash_is_reconciled_while_parallel_task_completes',
    'prelaunch_crash_fails_closed_without_disturbing_the_parallel_task'
)
$CliMultiTaskReviewTests = @(
    'decomposition_replaces_proposal_ids_and_rewrites_dependencies',
    'delivery_requires_exact_approval_and_reapproval_after_candidate_change',
    'local_review_writes_only_approved_review_refs_without_merging',
    'scheduler_starts_dependencies_before_dependent_task',
    'verification_blocks_failed_commands_and_names_protected_paths',
    'workflow_records_survive_reopen_after_each_stage'
)
$CliDeliveryReconciliationTests = @(
    'ambiguous_records_stay_in_the_plan_and_in_the_store',
    'each_task_reconciles_only_its_own_steps',
    'per_task_evidence_summarises_the_live_rows',
    'reconciliation_leaves_live_store_git_and_worktrees_unchanged_including_after_reopen',
    'snapshot_detects_a_created_ref',
    'snapshot_detects_a_dangling_symref_write_through',
    'snapshot_detects_a_deleted_ref',
    'snapshot_detects_a_moved_ref',
    'snapshot_detects_a_removed_worktree',
    'snapshot_detects_a_store_write',
    'snapshot_detects_changed_worktree_bytes',
    'store_row_json_maps_to_the_classifier_record'
)
$GitDeliveryIntegrationTests = @(
    'candidate_changed_by_approval_time_is_refused_without_writing_a_ref',
    'destination_lock_refuses_second_concurrent_holder',
    'dirty_candidate_is_refused_before_verification_or_approval',
    'dirty_worktree_is_unsafe_and_survives_safety_check',
    'local_review_accepts_existing_ref_with_same_candidate',
    'local_review_creates_only_review_ref',
    'local_review_creates_the_exact_ref_when_only_a_lookalike_exists',
    'local_review_refuses_changed_approved_candidate_sha',
    'local_review_refuses_changed_parameter_digest',
    'local_review_refuses_existing_ref_with_different_commit',
    'local_review_refuses_missing_approval',
    'local_review_refuses_refs_outside_the_review_namespace_before_any_callback',
    'local_review_refuses_stale_verification_run',
    'local_review_refuses_symbolic_review_refs_without_writing_through_them',
    'local_review_refuses_when_worktree_candidate_sha_changed',
    'persistence_failure_is_reported_and_still_releases_the_destination_lock',
    'primary_checkout_and_foreign_repository_are_not_deliverable_candidates',
    'primary_checkout_is_retained_and_a_clean_landed_worktree_is_safe',
    'refused_delivery_persists_refusal_releases_lock_and_leaves_destination_reusable',
    'report_only_preserves_refs_worktrees_and_repository_files',
    'stale_destination_lock_is_reported_and_preserved',
    'unlanded_worktree_is_unsafe_and_survives_safety_check',
    'verification_is_revalidated_before_approval_and_before_any_write'
)
$GitDeliveryReconcileTests = @(
    'dangling_symbolic_review_ref_is_ambiguous',
    'dwim_lookalike_ref_is_not_the_review_ref',
    'incomplete_foreign_or_malformed_records_are_ambiguous',
    'incomplete_record_is_ambiguous_and_keeps_observed_ref',
    'missing_review_ref_is_not_applied',
    'packed_review_ref_at_candidate_is_confirmed',
    'plan_order_does_not_depend_on_input_order',
    'reconciliation_does_not_change_git_or_supplied_record_snapshot',
    'report_only_step_has_no_review_ref_to_apply',
    'review_ref_at_an_unrecorded_commit_is_ambiguous_and_blocks_gate',
    'review_ref_at_candidate_is_confirmed_and_does_not_block_gate',
    'symbolic_review_ref_is_ambiguous_and_keeps_observed_object'
)
$AgentsConformanceTests = @(
    'claude_code_approvals',
    'claude_code_cancel_after_exit',
    'claude_code_cancellation',
    'claude_code_cancellation_tree',
    'claude_code_crlf_framing',
    'claude_code_error_result',
    'claude_code_errors',
    'claude_code_framing',
    'claude_code_interruption',
    'claude_code_launch',
    'claude_code_unexpected_eof',
    'claude_code_unsupported_features',
    'codex_approvals',
    'codex_cancel_after_exit',
    'codex_cancellation',
    'codex_cancellation_tree',
    'codex_crlf_framing',
    'codex_errors',
    'codex_framing',
    'codex_interruption',
    'codex_item_outside_turn',
    'codex_launch',
    'codex_nonterminal_warning',
    'codex_unexpected_eof',
    'codex_unsupported_features',
    'pi_rpc_approvals',
    'pi_rpc_cancel_after_exit',
    'pi_rpc_cancellation',
    'pi_rpc_cancellation_tree',
    'pi_rpc_crlf_framing',
    'pi_rpc_error_result',
    'pi_rpc_errors',
    'pi_rpc_framing',
    'pi_rpc_interruption',
    'pi_rpc_launch',
    'pi_rpc_unexpected_eof',
    'pi_rpc_unsupported_features'
)
$AgentsTests = $AgentsConformanceTests + @(
    'claude_code_adapter::tests::error_result_with_clean_exit_is_not_completed',
    'claude_code_adapter::tests::nonzero_exit_after_completed_turn_fails_and_keeps_last_message',
    'codex_adapter::tests::nonzero_exit_after_completed_turn_fails_and_keeps_last_message',
    'codex_parser::tests::bad_thread_id_faults',
    'codex_parser::tests::blank_and_unknown_item_lines_are_skipped',
    'codex_parser::tests::full_happy_sequence',
    'codex_parser::tests::malformed_json_faults',
    'codex_parser::tests::missing_turn_completed_is_detected',
    'codex_parser::tests::non_object_line_faults',
    'codex_parser::tests::ordering_violations_fault',
    'codex_parser::tests::pre_turn_error_item_is_ignored_but_other_items_fault',
    'codex_parser::tests::turn_failed_is_an_unknown_type_and_never_completes_the_turn',
    'codex_parser::tests::unknown_type_faults',
    'pi_rpc_adapter::tests::aborted_turn_with_clean_exit_is_not_completed',
    'pi_rpc_adapter::tests::malformed_json_is_recoverable_and_fails_settled_turn',
    'pi_rpc_adapter::tests::nonzero_exit_after_settled_turn_fails',
    'arguments_keep_prompt_separate_and_restrict_tools',
    'cancellation_is_reported_as_a_transport_outcome',
    'detection_reports_verified_capabilities',
    'eof_mid_turn_is_not_reported_as_completion',
    'malformed_lines_report_a_protocol_violation_without_panicking',
    'malformed_process_output_fails_even_when_process_exits_zero',
    'missing_fields_in_known_events_are_protocol_violations',
    'nonzero_exit_after_completed_turn_fails_and_keeps_last_message',
    'parses_the_recorded_stream_json_output',
    'unknown_events_are_ignored',
    'cancel_reports_cancelled_exit',
    'deprecated_error_item_and_command_lifecycle_do_not_fail_a_successful_turn',
    'detect_reports_capabilities_for_found_binary',
    'detect_reports_not_found_for_missing_binary',
    'garbage_stdout_is_a_protocol_error_not_success',
    'happy_run_streams_events_and_completes',
    'oversized_stdout_and_stderr_lines_are_truncated_with_a_marker',
    'stderr_is_diagnostics_and_nonzero_exit_fails',
    'stream_lines_are_not_returned_after_exit_when_a_descendant_keeps_pipes_open',
    'supervisor_cancel_kills_descendants',
    'buffers_events_before_between_and_after_responses',
    'correlates_out_of_order_responses',
    'detects_only_the_rpc_command_it_verified',
    'does_not_reuse_previous_completion_after_new_prompt',
    'ignores_malformed_json_lines',
    'ignores_unknown_event_types',
    'keeps_stderr_separate_from_rpc_records',
    'matches_response_to_request_id',
    'preserves_failing_command_response',
    'reports_mid_turn_eof_as_transport_exit',
    'claude_code_unavailable_capabilities_are_rejected',
    'codex_unavailable_capabilities_are_rejected',
    'empty_preference_list_fails_closed',
    'empty_requirements_select_first_preference',
    'identical_inputs_always_select_same_adapter',
    'no_match_reports_missing_capabilities_for_each_candidate',
    'pi_unavailable_capabilities_are_rejected',
    'preference_order_decides_between_equally_capable_adapters',
    'selects_first_adapter_that_supports_every_requirement'
)

# Every named test must also report ok inside the full workspace run. The fault-injection
# names are not in it: the workspace run does not enable that feature.
$WorkspaceTests = $GitTests + $GitDeliveryTests + $DataLockTests + $DataLockLibTests +
    $LockContractTests + $RuntimeContractTests + $RuntimeReconcileTests + $IpcTokenTests +
    $CliLibTests + $CliCleanupSafetyTests + $CliAcceptanceTests +
    $GitDeliveryNewTests + $GitDeliveryAcceptanceTests + $StoreDeliveryAcceptanceTests +
    $CliEngineReconciliationTests + $CliCrashConcurrencyTests + $CliMultiTaskReviewTests +
    $CliDeliveryReconciliationTests + $GitDeliveryIntegrationTests + $GitDeliveryReconcileTests +
    $AgentsTests

# Expect: tests that must report ok.
# Single: the run must have exactly one test binary, and that binary must run
#   at least one test. This is how a filter that matches nothing is caught.
# Passed, Ignored: exact counts. All are CALCULATED from the source for Windows,
#   not measured. A different measured count is INCONCLUSIVE, never edited here.
# Isolated: run with TEMP/TMP under $TestTmp and rustc on PATH.
# RunningSequence: the exact 'running N tests' values, in order, one per binary.
# TimeoutSeconds: limit for the command (default 1800).
$Checks = @(
    @{ Name = 'workspace-tests'; Expect = $WorkspaceTests; Single = $false
       Passed = 477; Ignored = 1
       Arguments = @('test', '--locked', '--workspace', '--no-fail-fast') },
    @{ Name = 'git-package'; Expect = $GitTests + $GitDeliveryTests + $GitDeliveryNewTests + $GitDeliveryAcceptanceTests +
           $GitDeliveryIntegrationTests + $GitDeliveryReconcileTests; Single = $false
       Passed = 80
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
           '--test', 'engine_reconciliation_gate') },
    # P7 gates. Names come from the final source. Isolated: fake agents and fake children
    # compile with rustc on PATH and any survivor lies under the private TEMP.
    @{ Name = 'cli-crash-concurrency'; Expect = $CliCrashConcurrencyTests; Single = $true
       Passed = 6; Ignored = 0; Isolated = $true; TimeoutSeconds = 900
       Arguments = @('test', '--locked', '-p', 'orchestrator-cli', '--test', 'crash_concurrency') },
    @{ Name = 'cli-crash-concurrency-fault-injection'; Expect = ($CliCrashConcurrencyTests + $CliCrashFaultInjectionTests); Single = $true
       Passed = 8; Ignored = 0; Isolated = $true; TimeoutSeconds = 900
       Arguments = @('test', '--locked', '-p', 'orchestrator-cli', '--features', 'fault-injection', '--test', 'crash_concurrency') },
    @{ Name = 'cli-multi-task-review'; Expect = $CliMultiTaskReviewTests; Single = $true
       Passed = 6; Ignored = 0; Isolated = $true; TimeoutSeconds = 900
       Arguments = @('test', '--locked', '-p', 'orchestrator-cli', '--test', 'multi_task_review') },
    @{ Name = 'cli-delivery-reconciliation-contract'; Expect = $CliDeliveryReconciliationTests; Single = $true
       Passed = 12; Ignored = 0; Isolated = $true; TimeoutSeconds = 900
       Arguments = @('test', '--locked', '-p', 'orchestrator-cli', '--test', 'delivery_reconciliation_contract') },
    @{ Name = 'git-delivery-integration'; Expect = $GitDeliveryIntegrationTests; Single = $true
       Passed = 23; Ignored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-git', '--test', 'delivery') },
    @{ Name = 'git-delivery-reconcile'; Expect = $GitDeliveryReconcileTests; Single = $true
       Passed = 12; Ignored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-git', '--test', 'delivery_reconcile') },
    @{ Name = 'agents-tests'; Expect = $AgentsTests; Single = $false
       Passed = 92; Ignored = 0; Isolated = $true; TimeoutSeconds = 900
       RunningSequence = @(16, 10, 10, 37, 10, 9)
       Arguments = @('test', '--locked', '-p', 'orchestrator-agents', '--tests') },
    @{ Name = 'agents-conformance'; Expect = $AgentsConformanceTests; Single = $true
       Passed = 37; Ignored = 0; Isolated = $true; TimeoutSeconds = 900
       Arguments = @('test', '--locked', '-p', 'orchestrator-agents', '--test', 'conformance') }
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
        if ($Check.RunningSequence -and ($Running -join ',') -cne ($Check.RunningSequence -join ',')) {
            & $Raise 'INCONCLUSIVE' "Per-binary running counts $($Running -join ',') differ from the calculated $($Check.RunningSequence -join ',')."
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
# Priority 7 Windows acceptance of $SourceSha (M2-05, M3-05, M3-06, M4-04)

This run does not close M2-05, M3-05, M3-06, or M4-04. They stay open until independent Mac, source, and
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
- Baseline ancestry (b5db71a3), commit count, and changed-file set: base-ancestry,
  range-count, range-files (consistency gates, not patch-equivalence proof)
- Runbook identity (operator-declared; exact bytes pinned by blob id, size, and SHA-256; not proof that
  the pasted commands match): runbook-identity.txt
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

- Unix-only tests did not run on Windows: recovery.rs, cli.rs, daemon.rs, the orphan-tree test, the
  runtime process-group survivor test, the store symlink test, and the Unix-only unit tests. Their
  macOS acceptance is separate and is not Windows evidence.
- M2-05: two tasks start sequentially with live overlap, not simultaneously. Only the Admit event
  insert is faulted. The Windows crash is a hook exit plus Job Object owner-death reaping proven
  without a test-issued kill. The old recovery.rs and external sqlite3 tests are not Windows proof.
- M3-05: one OS process with a durable store reopen and opaque digests. It is not daemon, engine-CLI,
  or process-crash proof.
- M3-06: live-store read-only snapshots with local Git. No delivery to a real remote ran.
- M4-04: fake children only, no live provider. taskkill /T /F is a forced kill, not a graceful
  SIGINT. CRLF is tolerance only. Approvals are no-grant and routing-refusal only.
- No clippy or rustfmt gate ran. The workspace run is not isolated.
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
$EvidenceBranch = "evidence/priority-7-windows-acceptance-$Short-$RunId"
$EvidenceWorktree = Join-Path $env:TEMP "p7ev-$Short-$($RunId.Substring($RunId.Length - 8))"
$EvidencePath = "docs/evidence/priority-7-windows-acceptance/$Short-$RunId"

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
    'range-count', 'range-files', 'runbook-fetch', 'runbook-type', 'runbook-parent', 'runbook-files',
    'runbook-blob', 'switch',
    'source-head-before', 'source-head-after', 'source-status-before', 'source-status-after',
    'diff-check', 'git-version', 'rustc-version', 'cargo-version', 'git-autocrlf', 'publish-fetch') {
    $Required += "$Step.stdout.txt", "$Step.stderr.txt", "$Step.exit.txt", "$Step.result.json"
}
# runbook-bytes.stdout.txt is deleted on purpose: it holds the runbook text and a repository URL.
$Required += 'runbook-bytes.stderr.txt', 'runbook-bytes.exit.txt', 'runbook-bytes.result.json'
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
if ((Get-Content -LiteralPath (Join-Path $EvidenceDir 'runbook-identity.txt') -Raw) -notmatch '(?m)^runbook sha256: [0-9a-f]{64}$') {
    throw 'The runbook byte hash was not recorded. Do not push. Report it.'
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
    -m "evidence: record Priority 7 Windows run of $Short ($Overall)" `
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
- the overall result and the status of each of the 25 cargo commands, with its exit code,
  duration, and `running N tests` counts
- the OS, CPU, `rustc -Vv`, `cargo -V`, `git --version`, and `core.autocrlf` (with its origin)
- the branch tip and the pinned source commit, and that `HEAD` matched before and after
- the measured totals next to the calculated ones: `workspace-tests` 477 passed and
  1 ignored, `git-package` 80, the `agents-tests` per-binary sequence 16, 10, 10, 37, 10, 9, and
  every per-check count in the table above. Label each as
  measured now versus calculated before
- the coverage gaps: Unix-only tests not run, no Windows crash-recovery (`recover-owner`),
  multi-task review of a crashed process, or daemon integration coverage (the P7 coverage gaps above)
- the runbook identity record: blob id, byte length, and SHA-256 of the pinned runbook bytes
- for `cli-cleanup-safety`, `cli-acceptance`, `cli-engine-reconciliation-gate`, and every Isolated P7 gate: the `running N tests` line, the `survivors.txt`
  content (empty means none found), and for a failure the first failing assertion message exactly as
  printed
- for any timed-out check, the cold-build label from the status rules, as a note beside the unchanged
  `FAIL`
- that `protect-text-selfcheck.txt` and the publication scan passed
- any setting that you could not observe, including whether your agent runtime could run Sonnet 5.5
  with high reasoning

Do not edit `board.json`. The coordinator reviews the evidence and updates the board.



## What changed from the approved harness

The harness blocks derive from the accepted Priority 6 runbook at
`c330e89460b32ea6103c1dd862d5fd84aebefe46` (native QA accepted it on `b5db71a3…`, evidence
`a77773b3…`). Preserved unchanged in behavior: PowerShell 7 only, the pinned SHA and `HEAD` equality
before and after, the declared runbook commit, clean-checkout gates, unique run directories outside
the checkout, `Protect-Text` and its self-check, `Invoke-Bounded` (byte-exact raw capture, tree kill
on timeout only), the survivor scan after every check, the fail-closed publication scan, the
`Expect`, `Single`, `Passed`, `Ignored`, `Isolated`, `TimeoutSeconds` model, `PASS < INCONCLUSIVE <
FAIL`, the never-kill-a-survivor rule, and evidence-only branch publication. All 17 Priority 6 gates
keep their argv and flags. No helper was rewritten or abstracted.

Changed:

- Re-pinned the title, source commit, source branch name check (anchored to `orch/priority-7-…`, as at P6), baseline
  `b5db71a3…`, evidence branch and path names (`priority-7`), run-root prefix (`agent-orch-p7-acc-`),
  runbook file name, and links. The range count and changed-file set are re-derived from the final source.
- `workspace-tests` 430 to 477 passed. `git-package` 73 to 80. Both are calculated.
- Added eight standalone gates after `cli-engine-reconciliation-gate`: `cli-crash-concurrency`,
  `cli-crash-concurrency-fault-injection`, `cli-multi-task-review`,
  `cli-delivery-reconciliation-contract`, `git-delivery-integration`, `git-delivery-reconcile`,
  `agents-tests`, and `agents-conformance`. The CLI and agents gates run Isolated. The two Git gates
  and `git-package` run like the Priority 6 Git gates.
- Added an optional `RunningSequence` check field. Only `agents-tests` uses it. Without the field the
  verdict logic is unchanged.
- Added the runbook byte pin: fetch of the declared runbook commit, checks of its type, its sole parent,
  its single changed path, and the blob id, then the byte length and SHA-256 of the exact runbook
  bytes in `runbook-identity.txt`. The raw bytes stay in `$RawDir`, and the text copy is deleted
  before publication because it holds the repository URL. The existing declared-commit line and its
  check are unchanged. The pasted commands are still not compared with the bytes.
- Not added: clippy and rustfmt gates, a standalone store `--test delivery` gate, parsing of the
  `store-lock-identity-contract` stderr, and a comparison of the pasted text with the pinned bytes.
- Coverage-gap and report text were updated for M2-05, M3-05, M3-06, and M4-04.

Known limits of this runbook:

- The check helper ends only the tree it started. The survivor scan reports other processes by path
  and command line and never stops them.
- The counts are calculated and must be reconciled with the measured native listing. A mismatch is
  `INCONCLUSIVE` and is a finding for the coordinator, not an edit of this file.
- The `workspace-tests` check is not isolated. The isolated standalone checks cover the tests that
  need an isolated `TEMP` and `rustc` on `PATH`.
- The redaction patterns are anchored at the start of a word run so a very large output without a URL
  or scp form is scanned in linear time.
- The declared runbook commit and the source pin are separate. The byte pin makes the runbook bytes
  comparable after the run. It is not an execution proof.
