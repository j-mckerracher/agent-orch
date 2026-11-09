# Priority 10 remediation, round 2 (P10r2: M5-02b scheduler and executor, M5-02c coordinator and review, M5-03 and M5-04 desktop screens over the daemon) native Windows acceptance runbook, second rerun (candidate e555c2c922ce00ee93673452f67848c7f888b4f1)

This runbook is the native Windows **second rerun** of Priority 10, after two remediations. Priority 10 is the
combined candidate that adds the daemon scheduler and executor, the coordinator and review RPC
surface, and the desktop UI that talks to the daemon. It is adapted from the first rerun runbook
`docs/evidence/priority-10r-windows-acceptance-abd3ab2-runbook.md` (docs commit
`674e40b8ce78a7e1322433918e227ce8aef78182`, docs branch `docs/priority-10r-windows-acceptance-abd3ab2`,
source `abd3ab25ee96a6ca1c0face830a83af25835b253`), whose native run on Windows found one failing IPC
unit test (see "Prior native evidence"). That runbook derives from the first Priority 10 runbook
`docs/evidence/priority-10-windows-acceptance-67c1181-runbook.md` (docs branch
`docs/priority-10-windows-acceptance-67c1181`, source `67c1181db7cd44890cd229eee648033525f64c3e`), which
derives from the accepted Priority 9 remediation runbook
`docs/evidence/priority-9r-windows-acceptance-7435ea3-runbook.md` (docs commit
`ec77133c91c34dbbc69518eca2b1bdfca710fe96`, source `7435ea32bcb541044e61ef49e863d394f0d49e12`), and in
turn from the Priority 7 and Priority 8 runbooks. Those are immutable git objects and are absent from
the source checkout. The helper blocks (`Protect-Text`, `Invoke-Bounded`, the survivor scan, the
publication scan, the verdict model) are reused with the changes listed in "What changed from the
approved harness". No new framework is introduced.

Do not run an old runbook against a new candidate. In particular, do not run the first Priority 10
runbook or the first rerun runbook (`abd3ab2`) against this candidate: their pins stop on any other
source.

## Prior native evidence and what it does not prove

**First Priority 10 run (source `67c1181db7cd44890cd229eee648033525f64c3e`, FAIL).** The first
Priority 10 runbook was run natively on Windows 11 (build 10.0.26200). The evidence commit is
`42f63e4b581c96db329e849ea88270df74f1cb13` on the branch
`evidence/priority-10-windows-acceptance-67c1181-20261109T223022Z-55d89a66abca41d7b0aec48a1bcfc6fa`.
It measured:

- `daemon-build` PASS (the first native build of the daemon with the M5-02b and M5-02c code compiled);
- `ipc-tests` PASS with `running 38` then `running 0` (38 passed), which confirmed the derived count;
- `windows-transport-tests` PASS with `running 13` only (13 passed), which confirmed the filtered
  shape;
- `desktop-npm-ci`, `desktop-tsc`, `desktop-build` PASS and `desktop-vitest` PASS (268 passed);
- `clippy-scoped` PASS with the two baseline `single_match` diagnostics plus one NEW diagnostic,
  `method blocked_reason is never used` at `crates\orchestrator-daemon\src\executor.rs:130`;
- `daemon-tests` **FAIL**: `running 42`, 41 passed, 1 failed, exit 101, test
  `api::review_tests::evidence_snapshot_lists_at_most_one_thousand_files` panicking at
  `api_review_tests.rs:1039` with `left: Bool(true) right: false`. Cargo stops after the failing test
  binary, so the two daemon integration binaries never ran and the `42,0,0` sequence was not
  measured;
- `git-autocrlf` printed `file:C:/Program Files/Git/etc/gitconfig	true`. Git for Windows sets
  `core.autocrlf=true` in its **system** config by default.

The cause was a test-fixture defect, not a production bug. The fixture's own `git` helper ignores the
global and system config, but the task worktree is created by the production worktree code, which
does not. Under `core.autocrlf=true` the worktree checked `base.txt` out with CRLF line endings, the
fixture's `git add -A` then staged it, and the commit held 1001 changed paths instead of 1000. The
other 41 daemon tests stage files by name and are unaffected.

**What the first remediation changed** (two commits on top of `67c1181d…`, 2 files, the rest of the tree is
byte-identical): `f246fe36b3009a412ac8b0ee3ee149d879fd9077` fixes the fixture in
`crates/orchestrator-daemon/src/api_review_tests.rs` (it sets the fixture repository's local
`core.autocrlf=false` before the worktree is created, stages only the `many-*.txt` files, and checks
the committed path count independently), and `abd3ab25ee96a6ca1c0face830a83af25835b253` gates the
`blocked_reason` test helper in `executor.rs` with `cfg(all(test, unix))` so the dead-code warning
goes away on Windows. No test was added or removed, so every expected count is unchanged.

**First rerun (source `abd3ab25ee96a6ca1c0face830a83af25835b253`, tree
`6b46c8b779957176ff85bceb596146e1b828cbac`, FAIL).** The first rerun runbook (docs commit
`674e40b8ce78a7e1322433918e227ce8aef78182`) was run natively on Windows 11 on one host, one account,
not elevated, with system `core.autocrlf=true`. The run id is
`20261109T230542Z-8de793432a174f9e81d2737ad45ee7d6`, and the evidence commit is
`8c3b623c7355903947574cff62c6d0960e6db4c0`. The coordinator re-derived the verdict from the raw
published files. It measured:

- `daemon-build` PASS (exit 0);
- `daemon-tests` PASS: `running 42`, 42 passed, 0 failed, then `running 0` and `running 0`. **The `42,0,0`
  sequence was measured natively on `abd3ab25…`.** Both first-remediation fixes held: the 1000-file
  fixture passed under `core.autocrlf=true`, and the `blocked_reason` dead-code warning was gone;
- `windows-transport-tests` PASS: `running 13` only, 13 passed, 25 filtered out, no doc-test stage;
- `clippy-scoped` PASS: only the two baseline `single_match` diagnostics (`supervisor.rs:401` and
  `:424`), no NEW diagnostic;
- `desktop-npm-ci`, `desktop-tsc`, `desktop-build` PASS and `desktop-vitest` PASS (14 files, 268 tests);
- `ipc-tests` **FAIL**: `running 38`, 37 passed, 1 failed, exit 101. The failing test is
  `coordinator_review_tests::server_gates_each_method_on_its_own_capability`, panicking at
  `coordinator_review_tests.rs:256` on the final dispatched-methods assertion:
  `left: ["coordinator.send_message", "coordinator.propose", "coordinator.get"]` and
  `right: [... , "review.request_approval", "review.evidence", "review.request_changes"]`. Cargo stops
  after the failing test binary, so the IPC doc-test stage was not run: **the IPC `38,0` sequence was not
  observed on `abd3ab25…`**. It was observed natively on `67c1181d…`, whose IPC source differs from this
  candidate in the two test files named below.

**Cause (a test-fixture defect, not a product capability-gating bug; confirmed only by Mac emulation, not
by a native run).** That test built its server with a **50 ms** stream timeout
(`.with_stream_timeout(Duration::from_millis(50))`). The server closes an unsubscribed connection
after one stream timeout of silence (`crates/orchestrator-ipc/src/session.rs:118`, fed by
`protocol.rs` `read_optional` returning `None` on a timeout). The test connected the `review_only`
client first and left it idle while it connected and used the `coordinator_only` client. On Windows,
named-pipe creation, the connect poll and the handshake take longer than 50 ms, so the server dropped
`review_only`. Its later calls then failed with a transport I/O error (`BrokenPipe`), which the weak
`!matches!(.., UnsupportedCapability)` assertion accepted, and the handler never saw the review calls.
On Unix the same gap is a few milliseconds, so macOS never failed. The `event_stream_tests` fixture
used the same 50 ms timeout and was exposed to the same hazard; it did not fail on Windows in that run.

**What the second remediation changed** (one commit, `e555c2c922ce00ee93673452f67848c7f888b4f1`, on top
of `abd3ab25…`, 2 files; the other 112 changed files and the rest of the tree are byte-identical):

- `crates/orchestrator-ipc/src/coordinator_review_tests.rs`: the stream timeout is now **5 s**
  (`STREAM_TIMEOUT`, documented). The weak `!matches!` check is replaced by strict helpers
  (`assert_unsupported`, `assert_dispatched`): a call without the capability must return exactly
  `UnsupportedCapability`, and a call with it must reach the handler, which is checked by the handler's
  call list growing by exactly that method. A dropped connection can no longer pass as dispatched. A
  deliberate 200 ms idle sleep on `review_only` makes every platform exercise the idle connection.
- `crates/orchestrator-ipc/src/event_stream_tests.rs`: the stream timeout is now **1 s**
  (`STREAM_TIMEOUT`, documented). It also is the polling period of a subscribed connection, so it was not
  raised higher.
- **Failures now print dispatch diagnostics.** The strict assertions include the `{result:?}` of the
  call, the method, and the handler's call list, so a dropped connection is reported as a transport
  error and is not mistaken for a capability result. If `server_gates_each_method_on_its_own_capability`
  or any other `coordinator_review_tests` or `event_stream_tests` test fails again, report those messages
  verbatim.

No test was added or removed, no production source changed, and every expected count is unchanged. **The
second remediation is verified only on the Mac** (the whole workspace test suite and clippy, a Mac
emulation of the cause in which a 150 ms delay between connect and first call reproduced the failure with
the old timeout, and Windows-target compile checks of `orchestrator-ipc --tests`). **No native Windows run
of `e555c2c9…` exists.** The remediation owner also lists residual risk: the 1 s event-stream timeout still
has a finite idle-gap tolerance, `write_to_a_closed_peer_fails_promptly` has a one-second bound and
`windows_transport` tests use a 500 ms `start()` timeout (both judged low risk and unchanged), and the
Unix-only IPC tests (`integration_tests`, `lifecycle_tests`) still use 50 ms and 250 ms timeouts, which
do not run on Windows.

If `evidence_snapshot_lists_at_most_one_thousand_files` fails again, its assertions print
`files.len()` and `files_truncated`, and a check names any stray committed path. Report those
lines verbatim.

**Priority 9 remediation (`7435ea32…`, the base of this candidate)** was run natively on Windows and
accepted as PASS with limits. Its measured counts were: IPC `running 31` then `running 0` (31 passed),
daemon `running 20` then `running 0` (20 passed), `windows_transport` filter `running 13` only (13
passed, 18 filtered out), and clippy with only the two baseline `single_match` warnings at
`crates\orchestrator-runtime\src\supervisor.rs:401` and `:424`. Its second-user, genuinely-remote-client
and above-64-KiB slow-reader items stayed UNPROVEN. The accepted evidence commit is
`9a9f3bf688bd41a5b5faa797a6d8e243e6d4fef5`.

That evidence proves only `7435ea32`, the first Priority 10 run proves only `67c1181d…`, and the first
rerun proves only `abd3ab25…`. None is evidence for this candidate. The Priority 10 range changes the IPC
crate, the daemon, the store, the engine, the git crate and the whole desktop app. This runbook does not
carry any earlier row forward. The Priority 9 native probe (the real daemon on a live named pipe, 10 rows,
3 rounds) is **not part of this run**: its rows are listed as `UNPROVEN` below for this tree and the
Priority 9 probe result is not a claim about it.

## Status of this run

**Native Windows acceptance of this candidate is unproven. No native Windows run of `e555c2c9…` exists,
and this document does not claim one.** Nothing in this runbook has been executed on Windows by its
author. Mac results, the earlier native runs on different commits, and the static derivations below
are not Windows evidence for this commit.

**The daemon built natively on Windows at `67c1181d…` and at `abd3ab25…`**, so the compile risk is
reduced. It is not removed: the two IPC test files changed here are compiled again, and they are
compiled for the Windows cfg set only by this run. The daemon cannot be
cross-compiled from a Mac, because `libsqlite3-sys` compiles bundled C source and needs the target C
toolchain, and the Mac attempt stopped there (exit 101, environmental). A compile failure here is
still a finding, not a reason to stop publishing. See "If the build fails (compile-failure triage)".

This run records how the candidate behaves on native Windows. It does not close any board item. The
coordinator decides acceptance. The plan is `docs/technical-implementation-plan.md`.

Items under test, in the board's words:

- **M5-02b.** Run daemon-owned tasks through the scheduler: an executor thread claims queued tasks,
  launches agents, reconciles on restart, and bounds `agents.capabilities` probes. Orphaned-agent
  termination is Unix only; on Windows the supervising Job Object has already killed the tree.
- **M5-02c.** Coordinator conversation and pinned review support: the `coordinator.*` and `review.*`
  RPC methods and their capabilities, the pinned diff in `orchestrator-git`, migration `0010`
  (`coordinator_review`), and the store epoch (migration `0007`) that makes the journal identity
  visible to clients.
- **M5-03 and M5-04 (desktop).** Screens S1 to S6 and the daemon-backed adapters D1, D2, D4, D5, D6, D7
  and D8 in `apps/desktop`, the negotiation of all 11 daemon capabilities, the journal-epoch handling,
  and the `epoch-mismatch` error kind.

## Candidate

- Repository: `https://github.com/j-mckerracher/agent-orch.git` (`origin`, the only remote and the
  only push target)
- Source branch (the coordinator publishes it): `orch/priority-10r2-m5-02b-02c-03-04-e555c2c`
- Source commit: `e555c2c922ce00ee93673452f67848c7f888b4f1`
- Source tree: `fae7306c87e4bb81eef56faaee9351c721d04f06`
- Source parent: `abd3ab25ee96a6ca1c0face830a83af25835b253` (the first rerun candidate)
- Accepted baseline (the Priority 9 remediation candidate): `7435ea32bcb541044e61ef49e863d394f0d49e12`
- Runbook branch (the coordinator publishes it): `docs/priority-10r2-windows-acceptance-e555c2c`
- Evidence branch (you create it): `evidence/priority-10r2-windows-acceptance-e555c2c-<RunId>`
- Runbook: file `docs/evidence/priority-10r2-windows-acceptance-e555c2c-runbook.md`. The coordinator
  gives the branch, the full runbook commit, and the URL in the dispatch message. The runbook is read
  from the **tip of the docs branch**. The source under test stays `e555c2c9…`. The docs branch is
  exactly one docs-only commit directly on the source: the runbook commit has the source commit
  `e555c2c9…` as its **only parent**, and the docs branch tip is the runbook commit. A runbook text
  cannot contain its own commit id, so the pin is self-referencing: the operator **declares** the
  runbook commit as the docs branch tip (the coordinator gives it in the dispatch message and the
  operator reads it from `git rev-parse origin/docs/priority-10r2-windows-acceptance-e555c2c`). Step 1
  then verifies, without trusting the declaration, that the declared commit is a commit, that its
  **only parent is exactly `e555c2c9…`** (source -> declared runbook commit, one commit, no merge),
  and that the whole difference from the source is this one file. Step 1 also records the declared
  commit, its parent, the blob id, the byte length, and the SHA-256 of the exact runbook bytes. That
  pins the bytes in evidence. It still does not prove that the commands you pasted match them. Nothing
  in the docs chain is part of the source under test.
- If the source branch tip is later than the source commit, test the pinned commit and report the tip.

Test only the exact source commit. Before any other command, `HEAD` must equal the full source
commit. If the branch does not contain the commit, stop and report the mismatch.

### Ref names

Every ref has one namespace, and none is reused for another purpose. The default `$SourceBranch` in
step 1 is the published name below. Use another only if the dispatch message names it.

| Ref | Pattern | Who creates it |
|---|---|---|
| Source | `orch/priority-10r2-m5-02b-02c-03-04-e555c2c` (the block accepts `orch/priority-10r2-[a-z0-9-]+`) | coordinator |
| Runbook | `docs/priority-10r2-windows-acceptance-e555c2c` | coordinator |
| Evidence | `evidence/priority-10r2-windows-acceptance-e555c2c-<RunId>` where `<RunId>` is `yyyyMMddTHHmmssZ-<32 hex>` (UTC time plus a GUID, so it is unique) | you |

A local capture of refs must be named by its full namespace, never only its last segment: `orch/<name>`
and `docs/<name>` would otherwise collide. You push only the evidence ref.

### What the candidate changes

The source is 17 linear commits above `7435ea32…` (114 files, 18682 insertions, 590 deletions; no
merge commit). The first 14 are the first Priority 10 candidate (`67c1181d…`), the next 2 are the first
remediation (`api_review_tests.rs` and `executor.rs` only, ending at `abd3ab25…`), and the last one is the
second remediation (`coordinator_review_tests.rs` and `event_stream_tests.rs` only, `e555c2c9…`). The
changed-file set is the same 114 paths as the first candidate. The `$ExpectedRangeFiles` gate checks the exact set of 114 changed paths.

| Area | Files | Compiled or run on Windows by this runbook? |
|---|---|---|
| `crates/orchestrator-daemon` | 12: `executor.rs` (scheduler thread and launch path), `api_coordinator.rs`, `api_review.rs`, `api.rs`, `main.rs`, three unit-test files (`api_review_tests.rs`, `api_tests.rs`, `executor_tests.rs`), two integration test files (`tests/coordinator_review.rs`, `tests/daemon.rs`), `examples/seed_review.rs`, `Cargo.toml` (adds `orchestrator-agents`, `orchestrator-engine`, `tokio`, `uuid`) | **Yes**: `daemon-build`, `daemon-tests` (unit), `clippy-scoped`. `executor_tests.rs` and both integration test files are `cfg(unix)`. |
| `crates/orchestrator-ipc` | 7: `client.rs`, `session.rs`, `protocol.rs`, `lib.rs`, `unsupported.rs`, new `coordinator_review_tests.rs` (5 tests), `event_stream_tests.rs` (7 -> 9 tests) | **Yes**: `ipc-tests`, `windows-transport-tests`. |
| `crates/orchestrator-store` | 8: migrations `0007_store_epoch.sql` and `0010_coordinator_review.sql`, `sqlite_review.rs`, approvals and events changes, two test files | Compiled as a dependency of the daemon and exercised only through the daemon unit tests. The store's own tests are **not** run here. |
| `crates/orchestrator-engine` | 3: new crate with the reconcile gate (`reconcile.rs`) | Compiled as a dependency. Its tests are **not** run here. |
| `crates/orchestrator-git` | 3: pinned diff (`repository.rs`, `error.rs`, `tests/pinned_diff.rs`) | Compiled as a dependency. Its tests are **not** run here. |
| `crates/orchestrator-cli` | 2: `workflow.rs` slimmed (logic moved to the engine), `Cargo.toml` | **Not built** by this runbook. |
| `apps/desktop` | 77: screens, daemon adapters, review UI, test harness, vitest and tsconfig changes, `e2e/` | **Yes**: `npm ci`, `tsc -b`, `npm run build`, `vitest run`. The `e2e/` suite is **NOT RUN**. |
| `Cargo.toml`, `Cargo.lock` | 2: new workspace member `orchestrator-engine`, the new daemon dependencies | `--locked` resolves against the updated lock file. The host needs network access to the crate registry for the new crates. |

The Windows-relevant behaviour changes in the daemon are small in surface and large in new code: the
executor thread, `terminate_orphaned_agents` (compiled out of Windows; its Windows twin is a no-op),
the engine reconcile gate, and the store migrations that run when the daemon opens a data directory.
No Windows named-pipe transport source changed in this range; `windows_transport.rs` is identical to
the accepted Priority 9 remediation tree. The IPC changes (`client.rs`, `session.rs`, the capability
set) run over that transport and are exercised natively by the new tests.

Primer and plan at the pinned commit:
[orchestration primer](https://github.com/j-mckerracher/agent-orch/blob/e555c2c922ce00ee93673452f67848c7f888b4f1/orch-impl-m.md)
and
[implementation plan](https://github.com/j-mckerracher/agent-orch/blob/e555c2c922ce00ee93673452f67848c7f888b4f1/docs/technical-implementation-plan.md).
These links resolve only after the coordinator publishes the source branch. Follow the primer's skill
requirements. Report any unavailable guidance and any setting that you cannot observe as unverified.

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
host needs Windows 11 or later, Git for Windows, the MSVC Rust toolchain with the `clippy` component,
and Node.js with npm (`node.exe`, `npm.cmd`, `npx.cmd` on `PATH`). `rustc` must be on `PATH` for the
isolated gates. The daemon links SQLite through `libsqlite3-sys`, which compiles C source and needs the
MSVC C toolchain (Visual Studio Build Tools with the C++ workload) as `cl.exe` resolves it. The
Mac run of the desktop checks used Node `v26.13.0`; the Vite 8 and Vitest 5 toolchain needs a current
Node. Record what the host has. A missing tool is `BLOCKED`: report it and do not install anything. Use a
short checkout path and `core.longpaths` as the blocks do.

**Elevation.** Run the checks once from a standard (non-elevated) session and record the elevation
state in `environment.txt` (the block in step 2 does it). If an elevated session is also available, a
second run with a new `RunId` is welcome; preserve both runs. If only one elevation state is
available, say which one in your report.

## Checks

Each check runs once per run. Each records its command, environment summary, exit code, duration, full
stdout and stderr, and every `running N tests` and `test result` line (cargo test gates), every
`Compiling`, `Checking`, `Finished`, `warning`, and `error` line (cargo build and lint gates), or the
Vitest summary lines and build output (desktop gates).

### Where the expected counts come from (Mac-measured, derived for Windows, never edited)

Four classes stay separate and are never mixed:

- **Mac-measured** (reported by the integration and remediation owners on macOS, **not independently
  confirmed by the runbook author**; none is Windows evidence). The remediation owner reported at
  `e555c2c9…`: `cargo test --locked --workspace` exit 0 with 0 failures; `cargo clippy --locked
  --workspace --all-targets` exit 0 with only the 7 known `orchestrator-cli` warnings; and `cargo check
  --locked -p orchestrator-ipc --tests` for `x86_64-pc-windows-gnu` and `-msvc` exit 0. The remaining
  numbers were reported by the integration owner for the candidate before the two IPC test-file changes
  (no test was added or removed since, so they are carried as expectations, not re-measured at
  `e555c2c9…`): `orchestrator-ipc` unit **37**; `orchestrator-daemon` unit **59**, `tests/daemon.rs`
  **9**, `tests/coordinator_review.rs` **1**; `cargo test --locked --workspace` 626 passed, 0 failed,
  1 ignored; Vitest **14 files, 268 passed**; `npm ci`, `tsc -b` and `npm run build` exit 0; and the Node
  e2e suite 19 passed. A Windows-target compile of the daemon is not possible on the Mac
  (`libsqlite3-sys` needs the target C toolchain), so **no compile of the daemon for Windows exists
  before this run** other than the native builds at `67c1181d…` and `abd3ab25…`.
- **Derived for Windows** (this document): the Mac attribute counts, minus every test that the `cfg`
  set removes on Windows, plus the Windows-only tests, read from the source tree `fae7306c…` by `#[test]`
  and `cfg` attributes. The derivation below shows the arithmetic. These are the **expected** counts the
  gates enforce. **They are derived and unmeasured for this commit**: no Windows test of this tree has
  run. The first Priority 10 run on `67c1181d…` agreed with the derived IPC (`38,0`), filter (`13`) and
  Vitest (268) values, and with `running 42` for the daemon unit binary. The first rerun on `abd3ab25…`
  agreed with the daemon `42,0,0`, the filter (`13` and `25 filtered out`), Vitest (14 files, 268) and the
  two-diagnostic clippy shape, and measured `running 38` for IPC. Those trees differ from this one in two
  IPC test files at most. That is history, not evidence for this commit. The Vitest numbers are the Mac-measured numbers;
  the suite runs under jsdom and has no platform-specific test count, so they are expected to match.
- **Prior native measurement** (history, different trees): the first Priority 10 run at `67c1181d…`
  measured IPC `running 38` then `running 0`, `windows_transport` filter `running 13` only, daemon
  `running 42` with 1 failure (the integration binaries never ran), and Vitest 14 files and 268 tests.
  The first rerun at `abd3ab25…` measured daemon `running 42` then `running 0` and `running 0` (42 passed),
  the `windows_transport` filter `running 13` only, Vitest 14 files and 268 tests, and IPC `running 38`
  with 1 failure (the IPC doc-test stage did not run, so the IPC `38,0` sequence was last observed
  natively at `67c1181d…`). The two daemon trailing `0,0` stages and the IPC trailing `0` stage are
  therefore **not measured on this commit**, and the `42,0,0` daemon measurement comes from the earlier
  native run on `abd3ab25…`, which stopped at the IPC failure only after the daemon stage had passed. The
  accepted Priority 9 remediation run at
  `7435ea32…` measured IPC `running 31` then `running 0`, `windows_transport` filter `running 13` only,
  and daemon `running 20` then `running 0`. Neither is evidence for this commit.
- **Native-measured**: only what this run records. **Windows counts differ from the Mac counts and are
  reported as measured. They are not matched to the Mac counts, to the earlier native counts, or to the
  derived counts.** A measured count that differs from the derived one gives `INCONCLUSIVE`; the
  operator reports the measured value exactly as printed and never edits an expected value or a
  measured value.

Every number maps to one argv, package, binary, and platform. `lib.rs` of the IPC crate gates `auth`,
`session`, `client`, and `server` with `cfg(any(unix, windows))`, `unix_transport` with `cfg(unix)`,
`windows_transport` with `cfg(windows)`, `coordinator_review_tests` and `event_stream_tests` with
`cfg(all(test, any(unix, windows)))`, and `integration_tests` and `lifecycle_tests` with
`cfg(all(test, unix))`; `timeout_tests` lives inside `unix_transport`. `windows_transport_tests.rs` is
the `#[cfg(test)] #[path]` module of `windows_transport.rs`. In the daemon, `executor_tests.rs` is
`cfg(all(test, unix))`, `tests/daemon.rs` and `tests/coordinator_review.rs` both start with
`#![cfg(unix)]`, and `api_tests.rs` and `api_safety_tests.rs` hold the `cfg(unix)` tests named below.
No test in either crate carries `#[ignore]`.

| Target | Mac measured | Removed on Windows by `cfg` | Added on Windows | Windows derived (expected) |
|---|---|---|---|---|
| IPC lib unit tests (`src/lib.rs`) | 37 (protocol 6, auth 5, coordinator_review 5, event_stream 9, timeout 1, lifecycle 3, integration 8) | 12 (8 `integration_tests` + 3 `lifecycle_tests` + 1 `timeout_tests`) | 13 `windows_transport::tests` | **38** (protocol 6, auth 5, coordinator_review 5, event_stream 9, windows_transport 13) |
| IPC doc-tests | 0 | none | none | **0** (`running 0 tests`) |
| IPC total, unfiltered | 37 | 12 | 13 | **38 passed, 0 failed, 0 ignored**; `running` sequence **38,0** (unit, then doc-tests) |
| IPC filter `windows_transport` | not compiled on macOS | n/a | n/a | **13**; `running` sequence **13** only (see the note on filtered runs) |
| Daemon unit tests (`src/main.rs`) | 59 (`main` 1, `api_tests` 15, `api_safety_tests` 8, `api_review_tests` 22, `executor_tests` 13) | 17: 3 `api_tests` + 1 `api_safety_tests` (`cfg(unix)`) + 13 `executor_tests` (module is `cfg(unix)`) | none | **42** (`main` 1, `api_tests` 12, `api_safety_tests` 7, `api_review_tests` 22) |
| Daemon integration (`tests\coordinator_review.rs`, `tests\daemon.rs`, both `#![cfg(unix)]`) | 1 and 9 | all 10 | none | **0** in each binary (`running 0 tests` twice) |
| Daemon total, unfiltered | 69 | 27 | none | **42 passed, 0 failed, 0 ignored**; `running` sequence **42,0,0** |

Notes on the `running` sequences (these are the exact reasons a count can be wrong):

- **A filtered cargo run prints no doc-test stage.** `cargo test --locked -p orchestrator-ipc
  windows_transport` prints exactly one `running 13 tests` block and one `test result` line (`13 passed;
  0 failed; 0 ignored; 0 measured; 25 filtered out`, derived: 38 - 13). The accepted Priority 9 run
  measured this shape (`running 13 tests`, `13 passed … 18 filtered out`, no second `running 0 tests`).
  An earlier runbook expected `13,0` and that wrongly gave `INCONCLUSIVE`; this one expects `13`.
- **An unfiltered IPC run prints a doc-test stage.** The IPC crate is a library, so the sequence is the
  unit binary and then `running 0 tests` for the doc-tests (`38,0`). The Priority 9 run measured `31,0`.
- **The daemon is a binary crate, so it has no doc-test stage.** Its zeros come from the two
  integration test binaries, each of which is an empty crate on Windows because of `#![cfg(unix)]`. They
  run after the unit binary, in file-name order (`coordinator_review`, then `daemon`). Priority 9
  measured `20,0` when only `tests\daemon.rs` existed; this tree adds `tests\coordinator_review.rs`,
  hence `42,0,0`. **The extra `0` is derived from that reasoning and from the file list.** It was
  observed natively once, on `abd3ab25…` (the daemon crate is byte-identical to this candidate's), and
  is **not measured on this commit**. The first Priority 10 run stopped at the failing daemon unit binary
  (`running 42`) and printed no integration stage. If the measured sequence is
  different, the gate says `INCONCLUSIVE` and you report the
  exact lines; never edit the expectation.
- `examples\seed_review.rs` is an **example**, not a test. `cargo test` builds it and does not run it,
  so it adds a `Compiling`/`Finished` step and no `running` block. It is portable code (no `cfg`).

The Unix-only tests, by file, are never Windows proof and none of them runs here:

- IPC `integration_tests.rs` (8), `lifecycle_tests.rs` (3), and `timeout_tests.rs` (1).
- Daemon `executor_tests.rs` (13): the whole executor test module, including the orphan-termination
  tests. **The scheduler and executor therefore have no native Windows test at all in this run.**
- Daemon `api_tests.rs` (3): `running_task_cancellation_requests_owner_without_mutating_run_state`,
  `running_task_cancellation_with_crashed_owner_conflicts_and_retires_generation`,
  `running_task_without_owner_rejects_cancellation_without_partial_updates`.
- Daemon `api_safety_tests.rs` (1): `workspace_diff_rejects_symlink_escape_before_git`.
- Daemon `tests\daemon.rs` (9) and `tests\coordinator_review.rs` (1).

Windows-run tests by name (the arrays in the `$Checks` block hold these exact names; every name must
report `ok`):

- **IPC (38).** `protocol::tests::` `frame_round_trip_uses_versioned_big_endian_header`,
  `oversized_length_is_rejected_before_body_read`, `truncated_header_and_body_are_typed`,
  `wrong_version_and_malformed_json_are_typed_without_payload`,
  `non_eof_read_errors_are_not_reported_as_truncated`, `json_rpc_string_request_ids_are_supported`.
  `auth::tests::` `chooses_highest_common_version_and_rejects_unknown_capability_safely`,
  `absent_token_is_authentication_failure`, `startup_replaces_only_regular_stale_token`,
  `reports_no_common_version`, `token_encoding_requires_lowercase_hex`. `coordinator_review_tests::`
  `capability_set_names_are_unique_and_round_trip`,
  `client_wrappers_send_the_documented_methods_and_parameters`,
  `methods_are_unavailable_without_the_negotiated_capability`,
  `a_handler_without_review_never_exposes_review_methods`,
  `server_gates_each_method_on_its_own_capability`. `event_stream_tests::`
  `replay_larger_than_the_live_buffer_is_paged_across_reconnect`,
  `cursor_ahead_of_the_journal_is_a_typed_replay_gap`,
  `failed_resume_keeps_the_cursor_until_a_resume_succeeds`,
  `replaced_journal_with_a_longer_history_is_an_epoch_mismatch_not_a_silent_resume`,
  `resume_in_the_same_epoch_still_works_and_a_wrong_epoch_is_refused_at_subscribe`,
  `call_during_a_large_replay_keeps_events_and_the_connection_in_sync`,
  `live_lag_is_typed_and_reconnect_catches_up_in_order`,
  `reconnect_follows_a_daemon_restart_with_a_new_token`, `reconnect_still_requires_the_current_token`.
  `windows_transport::tests::` the 13 names listed under the next bullet.
- **`windows_transport` (13).** `pipe_and_token_dacls_grant_only_the_current_user`,
  `pipe_limits_and_remote_rejection_are_configured`,
  `second_listener_for_the_same_data_directory_is_refused`, `instance_limit_is_enforced_and_released`,
  `pipe_name_stays_bound_when_a_client_connects_and_leaves`, `write_to_a_closed_peer_fails_promptly`,
  `every_connection_must_complete_the_authenticated_handshake`,
  `stalled_connection_does_not_block_other_clients`,
  `read_waits_for_data_instead_of_reporting_end_of_file`,
  `read_reports_end_of_file_only_after_the_peer_closes`,
  `data_written_before_the_writer_closes_is_still_readable`,
  `handshake_waits_for_a_client_that_writes_after_connecting`,
  `idle_connections_hold_their_instances_until_they_close`.
- **Daemon (42).** `main::tests::lock_conflict_reports_live_holder_pid`. `api::tests::` (12)
  `every_rpc_method_rejects_malformed_parameters`,
  `task_submission_is_deduplicated_and_hides_project_path`, `missing_task_returns_typed_not_found`,
  `session_events_replay_task_and_only_selected_session_events`,
  `queued_task_cancellation_is_idempotent`,
  `running_task_cancellation_rejects_owner_without_process_identity`,
  `verifying_task_cancellation_is_rejected_without_partial_attempt_update`,
  `approval_resolution_uses_allowlisted_actor_and_is_idempotent`,
  `running_task_keeps_daemon_alive_without_connected_clients`,
  `idempotency_fingerprint_normalizes_omitted_optional_fields`,
  `project_registration_validates_the_repository_and_is_idempotent`,
  `session_events_page_and_report_a_cursor_beyond_the_journal`. `api::tests::safety::` (7)
  `workspace_diff_accepts_candidate_path_inside_task_worktree`,
  `workspace_diff_rejects_empty_path_before_git`, `workspace_diff_rejects_absolute_path_before_git`,
  `workspace_diff_rejects_parent_component_before_git`,
  `workspace_diff_rejects_option_prefix_before_git`,
  `delivery_refuses_unapproved_local_review_without_changing_refs`,
  `approved_local_review_delivery_creates_the_review_ref_once`. `api::review_tests::` (22)
  `new_methods_reject_malformed_parameters`, `capabilities_include_coordinator_and_review`,
  `coordinator_messages_are_idempotent_ordered_and_user_authored`,
  `coordinator_enforces_project_existence_and_size_limits`,
  `coordinator_get_pages_at_one_hundred_entries`,
  `valid_proposal_is_validated_without_authority_or_side_effects`,
  `invalid_proposals_report_stable_codes`, `proposals_cannot_smuggle_authority_fields`,
  `request_approval_binds_the_delivery_digest_and_is_replayable`,
  `request_approval_refuses_unsafe_requests`,
  `request_approval_requires_passed_verification_of_the_current_head`,
  `evidence_without_an_approval_is_missing_and_pinned_to_head`,
  `evidence_is_current_for_a_fresh_approval_and_stale_after_the_workspace_moves`,
  `evidence_marks_an_unverified_unknown_candidate_stale`,
  `evidence_snapshot_is_size_bounded_and_path_filterable`,
  `evidence_for_unknown_task_or_approval_is_not_found`,
  `request_changes_blocks_approval_but_not_rejection`,
  `request_changes_refuses_stale_duplicate_and_unknown_requests`,
  `request_changes_refuses_a_decided_approval`, `approval_from_review_request_delivers_end_to_end`,
  `evidence_marks_a_digest_that_no_longer_matches_its_parameters_stale`,
  `evidence_snapshot_lists_at_most_one_thousand_files`.

### Check table

| Check | Command | Expected on Windows |
|---|---|---|
| `daemon-build` | `cargo build --locked -p orchestrator-daemon` (new target directory) | Exit 0. `Compiling orchestrator-ipc`, `orchestrator-engine`, `orchestrator-store` and `orchestrator-daemon` lines and a `Finished` line. `debug\orchestrator-daemon.exe` exists; its size and SHA-256 are recorded. **Windows build of this commit: see the compile-failure triage.** |
| `ipc-tests` | `cargo test --locked -p orchestrator-ipc` | Exit 0. `running` sequence `38,0`, 38 passed, 0 failed, 0 ignored, all 38 names `ok`. |
| `daemon-tests` | `cargo test --locked -p orchestrator-daemon` | Exit 0. `running` sequence `42,0,0`, 42 passed, 0 failed, 0 ignored, all 42 names `ok`. |
| `windows-transport-tests` | `cargo test --locked -p orchestrator-ipc windows_transport` | Exit 0. `running` sequence `13` (one block, no doc-test stage), 13 passed, all 13 names `ok`. |
| `clippy-scoped` | `cargo clippy --locked -p orchestrator-ipc -p orchestrator-daemon --all-targets` (no `-D warnings`) | Exit 0 with `Checking orchestrator-ipc`, `Checking orchestrator-daemon` and `Finished`. The baseline is the two `single_match` warnings at `supervisor.rs:401` and `:424`, and they are the only diagnostics expected for this commit (the `blocked_reason` dead-code warning of the first run should be gone). **Every other diagnostic is recorded verbatim and does not change the verdict** (see the status rules). |
| `desktop-npm-ci` | `npm.cmd ci` in `apps\desktop` | Exit 0. |
| `desktop-tsc` | `npx.cmd tsc -b` in `apps\desktop` | Exit 0, no diagnostics. |
| `desktop-build` | `npm.cmd run build` in `apps\desktop` | Exit 0 and a `built in` line from Vite. The bundle size is recorded and is **not** a gate (Mac: main JS `328.41 kB`). |
| `desktop-vitest` | `npx.cmd vitest run` in `apps\desktop` | Exit 0. `Test Files  14 passed (14)` and `Tests  268 passed (268)`, nothing failed or skipped. |

After the checks the block records five rows that are **NOT RUN or UNPROVEN** and never raise the
verdict (see below). The runbook starts no daemon and runs no probe.

### Not run and UNPROVEN rows

An overall `PASS` never covers these. `result.md` lists each of them under "UNPROVEN, not covered by
this result". Do not try to exercise them from this runbook, and do not fabricate a result.

| Row | Why it is not covered |
|---|---|
| `unproven-second-user` | Denial of the named pipe to a second, different local user. The runbook has no second account and never asks for credentials. |
| `unproven-remote-client` | Rejection of a genuinely remote client. No remote host and no SMB loopback probe are used here. The Priority 9 loopback probe is not part of this run. |
| `unproven-slow-reader-64kib` | A single daemon-to-client write above the 64 KiB pipe buffer to a slow-reading client. No gate here sends one. |
| `unproven-tauri-native-bridge` | The Tauri shell and its native bridge to the daemon. The shipped app has no connector until M5-05 and shows Offline; the daemon adapters run only under unit tests and the Node e2e. `apps\desktop\src-tauri` is not built by this runbook. |
| `unproven-e2e-windows` | The desktop e2e suite (`npm run test:e2e`, 19 tests on the Mac) starts the real daemon over a **Unix domain socket** (`apps\desktop\e2e\harness.ts`). It cannot run on Windows. **NOT RUN.** Do not run it, and do not report a Windows e2e number. |

### Scope and disclosures (what these gates prove and do not prove)

- **Native Windows proof here is:** the shared protocol, authentication, event-stream, coordinator and
  review capability tests of the IPC crate (38), the 13 named-pipe tests, the daemon API, review and
  safety unit tests (42), a Windows build of the daemon and its link, a clippy pass over the IPC and
  daemon scopes, and the desktop install, type-check, bundle build and unit-test suite on Windows.
- **Not covered, and no claim is made:** the five rows above; the daemon scheduler and executor (their
  tests are `cfg(unix)`); daemon cancellation through a real run owner; the Windows Job Object path of
  `orchestrator-runtime` under the new executor; the store, engine, git and CLI crate tests; a live
  named pipe against the built daemon; server identity verification by the client; the data-directory
  ACL and the token owner; and any scenario in a test that is `cfg(unix)`.
- The daemon binary is built but **never started** by this runbook.
- Fixtures and recorded paths use the **Git top-level path form** (`C:/…` from `git rev-parse
  --show-toplevel`). They never use a verbatim canonical path (`\\?\C:\…` or `//?/C:/…` from
  `fs::canonicalize`). Git for Windows rejects a verbatim worktree destination. Step 1 records the
  top-level and refuses a verbatim form.
- Mac counts and the earlier native counts are not Windows counts for this commit.
- Known Mac-side facts that you may see again: the shipped app has no connector until M5-05; D3 recovery
  status is a dummy; idempotency keys are in memory only; `rustfmt --check` reports pre-existing diffs
  in `orchestrator-git/src/{lib,worktree}.rs` (no `rustfmt` gate runs here).

### If the build fails (compile-failure triage)

The daemon built natively on Windows at `67c1181d…` but not yet at this commit, so `daemon-build`, `daemon-tests` or
`clippy-scoped` can fail to compile. Do this, in this order, and nothing else:

1. **Capture and stop.** `Invoke-Bounded` has already written the full stdout and stderr of the failed
   command (`<name>.stderr.txt` and the raw bytes in `$RawDir`). Do not edit any source file, manifest,
   or lock file. Do not install a toolchain, a component, or a package. Do not retry in this evidence
   directory.
2. **Read the first `error`.** Classify it in your report and in nothing else:
   - **Host toolchain**: `link.exe` not found, `cl.exe` or `lib.exe` not found, a missing Windows SDK,
     `failed to run custom build command for libsqlite3-sys`, or a missing `rustc`. This is a host
     problem, not a candidate defect. Report `BLOCKED` for the host and name the missing tool.
   - **Network or registry**: `failed to download`, `failed to get`, or a registry error while resolving
     `tokio`, `uuid` or another new crate. `--locked` forbids lock changes; report it as an
     environment failure. Do not remove `--locked`.
   - **Rust compile error in the candidate**: an `error[E…]` in `crates\orchestrator-daemon`,
     `crates\orchestrator-engine`, `crates\orchestrator-store`, `crates\orchestrator-git` or
     `crates\orchestrator-ipc`. Copy the whole diagnostic, including the `-->` location, unedited.
     This is a **candidate defect for Windows** and the most valuable result of this run.
3. **Continue where the runbook allows.** A compile failure makes that gate `FAIL`. The harness still
   runs the remaining gates, writes `result.md` with the failure and the UNPROVEN rows, and publishes
   the evidence. The ipc and `windows-transport-tests` gates compile only the IPC crate and may still
   pass; the desktop gates do not depend on Rust at all. Do not skip a gate to "save time".
4. **Publish the evidence anyway.** Step 5 publishes every result, including `FAIL`. Commit and push
   the evidence branch even when the first command failed. A run with no published evidence is a
   protocol failure, not a quiet success.
5. **Report the exact error text** and the classification. Do not guess a fix.

### Coverage gaps

- Unix-only tests did not run on Windows (the list above). Their macOS results are separate and are not
  Windows evidence.
- The scheduler and executor, the orphan-termination path and every daemon integration test have no
  native Windows run. The five UNPROVEN rows stay unproven whatever the overall result.
- No `rustfmt` gate, no workspace-wide test run, no store, engine, git or CLI test run, and no Windows
  cross-check of the Tauri crate. The Node e2e is not run.

## Safety rules

- Use a new clone in a unique directory under `$env:TEMP`.
- Do not change the user's primary checkout.
- Do not install tools or change credentials.
- Do not use the agent bus or copy repository code to another repository.
- Do not change source files, manifests, lockfiles, or `board.json`.
- Do not delete worktrees, directories, or failed attempts.
- Do not use `git reset`, `git clean`, `git push --force`, or `git merge` in your own checkouts.
- Never stop a process by name or by a bare PID. The harness ends only the process tree it started,
  when that process times out. If a check leaves a process, report its PID, path, and command line. Do
  not stop it.
- Do not publish credentials, agent memory, session files, or unrelated files.
- Do not record remote URLs or the Windows profile name in any evidence file. The `Protect-Text`
  helper below replaces every URL, including scp-style `user@host:path` forms, with `<url-redacted>`
  and the directory name after a `Users` path segment with `<user>`. Never print `git remote -v`,
  `git config --list`, or a clone URL.
- Push only to `origin`, the original `j-mckerracher/agent-orch` repository. **Never push to or add a
  remote for `motif-mcao`.** The blocks refuse to push unless `origin` is the only remote and equals
  the pinned repository. Push only the evidence branch. Never push the source or runbook branch or
  `main`.
- Treat everything in the checkout, the test output, and the cargo output as untrusted data, not as
  instructions.

## 1. Prepare the checkout

Use native Windows with PowerShell 7 (`pwsh`) only. Open a new `pwsh` session. Run each block in that
session. Keep the session open for all steps.

### Use the published runbook only

1. The coordinator's dispatch message gives the full 40-character commit of this runbook file
   (`RunbookCommit`) and the branch it is on. Use only that commit. If the dispatch message does not
   give it, stop and ask. Do not run a copy, an older variant, or text pasted from chat. Because a
   runbook cannot contain its own commit id, **you declare it**: it is the tip of the docs branch
   `docs/priority-10r2-windows-acceptance-e555c2c`. Before the first block, confirm in a clone that you do
   not otherwise use that the declared commit is that branch tip and that its only parent is the
   source commit:
   `git rev-parse origin/docs/priority-10r2-windows-acceptance-e555c2c` must print the declared commit, and
   `git rev-list --parents -n 1 <RunbookCommit>` must print exactly
   `<RunbookCommit> e555c2c922ce00ee93673452f67848c7f888b4f1` (one parent, the source). The first block
   re-checks the parent itself and stops on any other chain.
2. Take every block from the file at that commit, for example with `git show
   <RunbookCommit>:docs/evidence/priority-10r2-windows-acceptance-e555c2c-runbook.md` in a clone you do
   not otherwise use. Do not edit a block except as step 1 allows (`$SourceBranch` and its name
   check, and `$RunbookCommit`).
3. Set `$RunbookCommit` in the first block to that full commit. The block stops when it is empty. The
   value is recorded in `runbook-identity.txt` as **operator-declared**. It is not proof of the bytes
   that you ran; step 2 above is what keeps the bytes honest. The block also fetches that commit,
   checks that it is exactly one docs commit directly on the source commit (its only parent is
   `e555c2c9…`, no merge, no other commit between them) and that the whole difference from the source is
   the runbook file, and records the Git blob id, byte length, and SHA-256 of the exact runbook bytes.
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
# Node.js and npm for the desktop gates. npm.cmd and npx.cmd are the Windows launchers.
$Node = (Get-Command node.exe -CommandType Application | Select-Object -First 1).Source
$Npm = (Get-Command npm.cmd -CommandType Application | Select-Object -First 1).Source
$Npx = (Get-Command npx.cmd -CommandType Application | Select-Object -First 1).Source
$Repository = 'https://github.com/j-mckerracher/agent-orch.git'
# The published source branch name. Use another only if the dispatch message names it.
$SourceBranch = 'orch/priority-10r2-m5-02b-02c-03-04-e555c2c'
$SourceSha = 'e555c2c922ce00ee93673452f67848c7f888b4f1'
$SourceTree = 'fae7306c87e4bb81eef56faaee9351c721d04f06'
# The runbook file path in the runbook commit (see Candidate).
$RunbookPath = 'docs/evidence/priority-10r2-windows-acceptance-e555c2c-runbook.md'
# The accepted baseline: the Priority 9 remediation candidate. Its native evidence proves only itself.
$BaseSha = '7435ea32bcb541044e61ef49e863d394f0d49e12'
$ExpectedRangeCount = 17
# The seventeen commits above the baseline, oldest first. Their order and ids are pinned. The last is
# the source (e555c2c9, the ipc test timeout fix). The one before it (abd3ab25, the first remediated
# candidate, native FAIL on one ipc test) is the source's only parent. The one before that (f246fe36,
# the CRLF test-fixture fix) is its parent, and the fourteenth (67c1181d) is the first Priority 10
# candidate.
$ExpectedRangeCommits = @(
    '2f3d30b360b7c3d9e6bd585824f100fdbc5e91c4',
    '738ab400537c429e942b3f9e3bd29b14a6c94362',
    '6b9022dcca6307572eac83de3993e38a454ab15e',
    '459aae7958ea245395e7ae2bab2bdd6f59df8b90',
    'de6116945de4f48c9e87143a7617f1a8b81a0b73',
    'f2ffa1f98b236fa0218a320a8be908b44706890f',
    '139bcf10668ef0b4628f5b438486ba287451474a',
    '883a5120704ff12b40f8c169f36245aca80e5eeb',
    'ebb8a1f1d113d40446c6acacd19af094ce9805a5',
    '16b4f1eb8e8b1e3ae325cff50f1b3b7eed7f5da5',
    '47d5417fe8a8383944514f107bcb0a6a73b731ad',
    'f0c126db3063c2d6183725220845a74059364937',
    '4a502c32624b8f7c294781efdca74b232d5a045f',
    '67c1181db7cd44890cd229eee648033525f64c3e',
    'f246fe36b3009a412ac8b0ee3ee149d879fd9077',
    'abd3ab25ee96a6ca1c0face830a83af25835b253',
    'e555c2c922ce00ee93673452f67848c7f888b4f1'
)
$ExpectedRangeFiles = @(
    'Cargo.lock',
    'Cargo.toml',
    'apps/desktop/e2e/daemon.e2e.test.ts',
    'apps/desktop/e2e/harness.ts',
    'apps/desktop/package-lock.json',
    'apps/desktop/package.json',
    'apps/desktop/src/App.tsx',
    'apps/desktop/src/daemon/api.ts',
    'apps/desktop/src/daemon/appData.test.ts',
    'apps/desktop/src/daemon/appData.ts',
    'apps/desktop/src/daemon/client.test.ts',
    'apps/desktop/src/daemon/connection.ts',
    'apps/desktop/src/daemon/errors.ts',
    'apps/desktop/src/daemon/idempotency.ts',
    'apps/desktop/src/daemon/session.ts',
    'apps/desktop/src/daemon/store.ts',
    'apps/desktop/src/daemon/types.ts',
    'apps/desktop/src/fixtures/ui/agent-capabilities.json',
    'apps/desktop/src/fixtures/ui/change-request.json',
    'apps/desktop/src/fixtures/ui/coordinator.json',
    'apps/desktop/src/fixtures/ui/journal-identity.json',
    'apps/desktop/src/fixtures/ui/recovery-status.json',
    'apps/desktop/src/fixtures/ui/review-demo-daemon.json',
    'apps/desktop/src/fixtures/ui/review-evidence.json',
    'apps/desktop/src/fixtures/ui/task-execution.json',
    'apps/desktop/src/fixtures/ui/task-retry.json',
    'apps/desktop/src/main.tsx',
    'apps/desktop/src/review/ChangeRequestPanel.tsx',
    'apps/desktop/src/review/EvidencePanel.tsx',
    'apps/desktop/src/review/ReviewRoute.tsx',
    'apps/desktop/src/review/ReviewScreen.test.tsx',
    'apps/desktop/src/review/ReviewScreen.tsx',
    'apps/desktop/src/review/binding.test.ts',
    'apps/desktop/src/review/binding.ts',
    'apps/desktop/src/review/client.ts',
    'apps/desktop/src/review/daemonSources.test.ts',
    'apps/desktop/src/review/daemonSources.ts',
    'apps/desktop/src/review/diff.test.ts',
    'apps/desktop/src/review/diff.ts',
    'apps/desktop/src/review/fixtureDaemon.ts',
    'apps/desktop/src/review/integration.test.tsx',
    'apps/desktop/src/review/review.css',
    'apps/desktop/src/review/sources.test.tsx',
    'apps/desktop/src/review/sources.ts',
    'apps/desktop/src/review/testing.ts',
    'apps/desktop/src/review/useReview.ts',
    'apps/desktop/src/sources/daemon.test.ts',
    'apps/desktop/src/sources/daemon.ts',
    'apps/desktop/src/sources/fixtures.test.ts',
    'apps/desktop/src/sources/fixtures.ts',
    'apps/desktop/src/sources/types.ts',
    'apps/desktop/src/test/fakeDaemon.ts',
    'apps/desktop/src/test/render.tsx',
    'apps/desktop/src/test/setup.ts',
    'apps/desktop/src/ui/ConnectionStrip.tsx',
    'apps/desktop/src/ui/Coordinator.tsx',
    'apps/desktop/src/ui/ProjectDetail.tsx',
    'apps/desktop/src/ui/ProjectList.tsx',
    'apps/desktop/src/ui/TaskDetail.tsx',
    'apps/desktop/src/ui/TaskList.tsx',
    'apps/desktop/src/ui/app.css',
    'apps/desktop/src/ui/createServices.ts',
    'apps/desktop/src/ui/daemonScreens.test.tsx',
    'apps/desktop/src/ui/drafts.ts',
    'apps/desktop/src/ui/format.ts',
    'apps/desktop/src/ui/logic.test.ts',
    'apps/desktop/src/ui/router.ts',
    'apps/desktop/src/ui/screens1.test.tsx',
    'apps/desktop/src/ui/screens2.test.tsx',
    'apps/desktop/src/ui/services.tsx',
    'apps/desktop/src/ui/submission.ts',
    'apps/desktop/src/ui/widgets.tsx',
    'apps/desktop/src/vite-env.d.ts',
    'apps/desktop/tsconfig.app.json',
    'apps/desktop/tsconfig.e2e.json',
    'apps/desktop/tsconfig.json',
    'apps/desktop/tsconfig.node.json',
    'apps/desktop/vitest.config.ts',
    'apps/desktop/vitest.e2e.config.ts',
    'crates/orchestrator-cli/Cargo.toml',
    'crates/orchestrator-cli/src/workflow.rs',
    'crates/orchestrator-daemon/Cargo.toml',
    'crates/orchestrator-daemon/examples/seed_review.rs',
    'crates/orchestrator-daemon/src/api.rs',
    'crates/orchestrator-daemon/src/api_coordinator.rs',
    'crates/orchestrator-daemon/src/api_review.rs',
    'crates/orchestrator-daemon/src/api_review_tests.rs',
    'crates/orchestrator-daemon/src/api_tests.rs',
    'crates/orchestrator-daemon/src/executor.rs',
    'crates/orchestrator-daemon/src/executor_tests.rs',
    'crates/orchestrator-daemon/src/main.rs',
    'crates/orchestrator-daemon/tests/coordinator_review.rs',
    'crates/orchestrator-daemon/tests/daemon.rs',
    'crates/orchestrator-engine/Cargo.toml',
    'crates/orchestrator-engine/src/lib.rs',
    'crates/orchestrator-engine/src/reconcile.rs',
    'crates/orchestrator-git/src/error.rs',
    'crates/orchestrator-git/src/repository.rs',
    'crates/orchestrator-git/tests/pinned_diff.rs',
    'crates/orchestrator-ipc/src/client.rs',
    'crates/orchestrator-ipc/src/coordinator_review_tests.rs',
    'crates/orchestrator-ipc/src/event_stream_tests.rs',
    'crates/orchestrator-ipc/src/lib.rs',
    'crates/orchestrator-ipc/src/protocol.rs',
    'crates/orchestrator-ipc/src/session.rs',
    'crates/orchestrator-ipc/src/unsupported.rs',
    'crates/orchestrator-store/migrations/0007_store_epoch.sql',
    'crates/orchestrator-store/migrations/0010_coordinator_review.sql',
    'crates/orchestrator-store/src/lib.rs',
    'crates/orchestrator-store/src/sqlite_approvals.rs',
    'crates/orchestrator-store/src/sqlite_events.rs',
    'crates/orchestrator-store/src/sqlite_review.rs',
    'crates/orchestrator-store/tests/reconnectable_events.rs',
    'crates/orchestrator-store/tests/review.rs'
)
if ($ExpectedRangeFiles.Count -ne 114) { throw 'The pinned changed-file list is not 114 entries.' }
# Required: the full 40-character commit of THIS runbook file, as the
# coordinator states it in the dispatch message. It is recorded as declared.
# It does not prove that the commands pasted here match that commit.
$RunbookCommit = ''
if ($RunbookCommit -cnotmatch '^[0-9a-f]{40}$') {
    throw 'Set RunbookCommit to the full lowercase 40-character commit from the dispatch message.'
}
foreach ($Sha in (@($SourceSha, $BaseSha, $SourceTree) + $ExpectedRangeCommits)) {
    if ($Sha -cnotmatch '^[0-9a-f]{40}$') {
        throw 'A pinned object id is not a full lowercase 40-character SHA.'
    }
}
if ($ExpectedRangeCommits.Count -ne $ExpectedRangeCount -or $ExpectedRangeCommits[-1] -cne $SourceSha) {
    throw 'The pinned range commits do not match the range count and the source commit.'
}
if ($SourceBranch -cnotmatch '^orch/priority-10r2-[a-z0-9-]+$') {
    throw 'The source branch name is not valid.'
}
$Short = $SourceSha.Substring(0, 7)
if ($RunbookPath -cne "docs/evidence/priority-10r2-windows-acceptance-$Short-runbook.md") {
    throw 'RunbookPath is not the Priority 10 runbook file for this source commit.'
}
$RunId = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ') + '-' +
    [Guid]::NewGuid().ToString('N')
$RunRoot = Join-Path $env:TEMP "agent-orch-p10r2-acc-$RunId"
$SourceDir = Join-Path $RunRoot 'source'
# Published, redacted evidence. Lives outside the measured checkout.
$EvidenceDir = Join-Path $RunRoot 'evidence'
# Raw, byte-exact stdout and stderr. Never published. Outside the checkout.
$RawDir = Join-Path $RunRoot 'raw'
$BuildDir = Join-Path $RunRoot 'cargo-target'
# Isolated checks run with TEMP/TMP below this directory. Their fake agents then
# live under a path that no other program uses, so survivors can be found by path.
$TestTmp = Join-Path $RunRoot 'test-tmp'
# Never a verbatim (\\?\) path: Git for Windows rejects it as a worktree destination.
if ($RunRoot.StartsWith('\\?\') -or $RunRoot.StartsWith('//?/')) {
    throw 'TEMP is a verbatim path. Use the plain drive-letter form. Report BLOCKED.'
}

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
# Plain output from the Node tools, so the Vitest summary lines parse. Colour codes are also stripped.
$env:NO_COLOR = '1'
$env:FORCE_COLOR = '0'
$env:CI = '1'

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
    'https://user:pass@example.test/org/repo.git', 'git@example.test:org/repo.git')
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

# Refuses to continue unless origin is the only remote and its fetch URL and its push
# URL are both the pinned repository. The URLs are compared in memory and are never
# printed or written to evidence.
function Assert-Origin([string] $Where) {
    $Remotes = @(& $Git -C $SourceDir remote)
    if ($LASTEXITCODE -ne 0 -or $Remotes.Count -ne 1 -or $Remotes[0] -cne 'origin') {
        throw "origin is not the only remote ($Where). Do not push. Report BLOCKED."
    }
    $FetchUrls = @(& $Git -C $SourceDir remote get-url origin)
    if ($LASTEXITCODE -ne 0 -or $FetchUrls.Count -ne 1 -or $FetchUrls[0] -cne $Repository) {
        throw "origin is not the pinned repository ($Where). Do not push. Report BLOCKED."
    }
    $PushUrls = @(& $Git -C $SourceDir remote get-url --push origin)
    if ($LASTEXITCODE -ne 0 -or $PushUrls.Count -ne 1 -or $PushUrls[0] -cne $Repository) {
        throw "The push URL of origin is not the pinned repository ($Where). Do not push. Report BLOCKED."
    }
}

$Run = Invoke-Bounded -Exe $Git -Name 'clone' -Arguments @(
    'clone', '--no-checkout', '--single-branch', '--branch', $SourceBranch,
    $Repository, $SourceDir)
if ($Run.Code -ne 0) { throw "Clone failed. Preserve $RunRoot and report it." }
Assert-Origin 'after clone'

$Run = Invoke-Git 'branch-tip' @('rev-parse', "refs/remotes/origin/$SourceBranch")
if ($Run.Code -ne 0) { throw 'Cannot read the branch tip.' }
$BranchTip = $Run.Out[0]
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'branch-tip.txt'), "$BranchTip`n")

$Run = Invoke-Git 'candidate-containment' @(
    'merge-base', '--is-ancestor', $SourceSha, "refs/remotes/origin/$SourceBranch")
if ($Run.Code -ne 0) { throw 'The branch does not contain the source commit.' }
$Run = Invoke-Git 'base-ancestry' @('merge-base', '--is-ancestor', $BaseSha, $SourceSha)
if ($Run.Code -ne 0) { throw "The source commit does not descend from $BaseSha." }
# The source has exactly one parent, the third pinned range commit, and a pinned tree.
$Run = Invoke-Git 'source-parent' @('rev-list', '--parents', '-n', '1', $SourceSha)
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or $Run.Out[0] -cne "$SourceSha $($ExpectedRangeCommits[-2])") {
    throw 'The source commit does not have the pinned previous range commit as its only parent.'
}
$Run = Invoke-Git 'source-tree' @('rev-parse', "$SourceSha^{tree}")
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or $Run.Out[0] -cne $SourceTree) {
    throw 'The source tree is not the pinned tree.'
}
# Consistency gates only. They do not prove the patch is the reviewed patch.
$Run = Invoke-Git 'range-count' @('rev-list', '--count', "$BaseSha..$SourceSha")
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or [int]$Run.Out[0] -ne $ExpectedRangeCount) {
    throw "The commit count above the baseline is not $ExpectedRangeCount."
}
# The range is linear (no merge commit) and holds exactly the pinned commits in the pinned order.
$Run = Invoke-Git 'range-merges' @('rev-list', '--merges', "$BaseSha..$SourceSha")
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 0) { throw 'The range above the baseline holds a merge commit.' }
$Run = Invoke-Git 'range-commits' @('rev-list', '--reverse', "$BaseSha..$SourceSha")
if ($Run.Code -ne 0 -or ($Run.Out -join ',') -cne ($ExpectedRangeCommits -join ',')) {
    throw 'The commits above the baseline are not the pinned commits.'
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
# The docs chain above the source: exactly one docs commit, whose only parent is the source.
# It is the declared runbook commit, and the docs branch tip.
$Run = Invoke-Git 'runbook-parent' @('rev-list', '--parents', '--reverse', "$SourceSha..$RunbookCommit")
$ChainOk = ($Run.Code -eq 0 -and $Run.Out.Count -eq 1 -and
    $Run.Out[0] -ceq "$RunbookCommit $SourceSha")
if (-not $ChainOk) {
    throw 'The runbook commit is not exactly one docs commit whose only parent is the source commit.'
}
$RunbookParent = $Run.Out[-1].Substring(41)
$RunbookChainLength = $Run.Out.Count
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
# The Git top-level path form. A verbatim canonical form (//?/C:/...) is refused.
$Run = Invoke-Git 'source-toplevel' @('rev-parse', '--show-toplevel')
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or $Run.Out[0] -cnotmatch '^[A-Za-z]:/') {
    throw 'The Git top-level is not a plain drive-letter path. Report BLOCKED.'
}
$Run = Invoke-Git 'source-status-before' @('status', '--porcelain=v1', '--untracked-files=all')
if ($Run.Code -ne 0) { throw 'Cannot read source status.' }
if ($Run.Out.Count -ne 0) { throw 'Source checkout is not clean.' }
```

If Git, Cargo, Rust, Clippy, Node.js, npm, or PowerShell is missing, report the missing tool. Do not
install it. Preserve any output that exists. A setup failure gives the result `BLOCKED`.

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
    [string[]]@($Git, $Cargo, $Rustc, $Node, $Npm, $Npx | ForEach-Object { Protect-Text $_ })
)
$GitVersion = Invoke-Bounded -Exe $Git -Name 'git-version' -Arguments @('--version')
$RustcVersion = Invoke-Bounded -Exe $Rustc -Name 'rustc-version' -Arguments @('-Vv') -Directory $SourceDir
$CargoVersion = Invoke-Bounded -Exe $Cargo -Name 'cargo-version' -Arguments @('-V') -Directory $SourceDir
$ClippyVersion = Invoke-Bounded -Exe $Cargo -Name 'clippy-version' -Arguments @('clippy', '-V') -Directory $SourceDir
$NodeVersion = Invoke-Bounded -Exe $Node -Name 'node-version' -Arguments @('-v') -Directory $SourceDir
$NpmVersion = Invoke-Bounded -Exe $Npm -Name 'npm-version' -Arguments @('-v') -Directory $SourceDir
# Exit code 1 means the key is not set. That is not an error.
$Autocrlf = Invoke-Git 'git-autocrlf' @('config', '--show-origin', '--get-all', 'core.autocrlf')
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'source-sha.txt'), "$SourceSha`n")
# Declared by the operator. It does not prove that the pasted commands match
# the runbook file of that commit.
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'runbook-identity.txt'),
    "runbook file: $RunbookPath`n" +
    "runbook commit (operator-declared, not byte proof): $RunbookCommit`n" +
    "runbook commit parent (verified): $RunbookParent`n" +
    "runbook docs commits above the source (verified): $RunbookChainLength`n" +
    "runbook blob id: $RunbookBlob`n" +
    "runbook bytes: $RunbookBytes`n" +
    "runbook sha256: $RunbookSha256`n" +
    "pasted commands vs these bytes: not verified`n" +
    "source commit: $SourceSha`n" +
    "source tree (verified): $SourceTree`n")
if ($GitVersion.Code -ne 0 -or $RustcVersion.Code -ne 0 -or $CargoVersion.Code -ne 0) {
    throw "Toolchain check failed. Preserve $RunRoot and report it."
}
if ($ClippyVersion.Code -ne 0) {
    throw "Clippy is not available. Do not install it. Preserve $RunRoot and report BLOCKED."
}
if ($NodeVersion.Code -ne 0 -or $NpmVersion.Code -ne 0) {
    throw "Node.js or npm is not usable. Do not install it. Preserve $RunRoot and report BLOCKED."
}
```

The files `git-version.stdout.txt`, `rustc-version.stdout.txt`, `cargo-version.stdout.txt`,
`clippy-version.stdout.txt`, `node-version.stdout.txt`, `npm-version.stdout.txt`, and
`git-autocrlf.stdout.txt` hold the toolchain record. An empty
`git-autocrlf.stdout.txt` with exit code 1 means `core.autocrlf` is not set.

## 3. Run the checks

`Invoke-Bounded` records the stdout, stderr, exit code, duration, environment summary, and timeout of
each command. Each command has a limit (30 minutes by default). A timeout is a failure, even if the
process later exits with code zero. Run each command once. Do not retry a failed command in this
evidence directory. The nine gates run in the order listed: the standalone `daemon-build` goes
first, in the new, empty target directory, so its `Compiling` lines are real. The four test gates
(three of them cargo test) and the clippy gate follow, then the four desktop gates (`npm ci` first,
because it creates the `node_modules` directory that the other three use; it is inside the checkout
and Git ignores it). Nothing starts the daemon. The first Windows build of
the daemon needs an MSVC C toolchain, because `libsqlite3-sys` compiles bundled C. A missing
toolchain fails the build gate (see the status rules and the compile-failure triage).

```powershell
# Runs one native command for a gate. The default is cargo in the source checkout. The desktop
# gates pass the Node launcher and apps\desktop as the working directory.
function Invoke-CapturedCargo {
    param([string] $Name, [string[]] $Arguments, [int] $TimeoutSeconds = 1800,
          [bool] $Isolated = $false, [string] $Exe = $Cargo, [string] $Directory = $SourceDir)
    Assert-Head "before $Name"
    Invoke-Bounded -Exe $Exe -Name $Name -Arguments $Arguments -Directory $Directory `
        -TimeoutSeconds $TimeoutSeconds -Isolated $Isolated
}

# Test names. Each is a real test of the pinned source for the Windows cfg set (no Unix-only
# test). A module prefix such as protocol::tests:: or api::tests::safety:: is matched loosely.
# The thirteen Windows-only tests of windows_transport_tests.rs (cfg(windows)).
$WindowsTransportTests = @(
    'pipe_and_token_dacls_grant_only_the_current_user',
    'pipe_limits_and_remote_rejection_are_configured',
    'second_listener_for_the_same_data_directory_is_refused',
    'instance_limit_is_enforced_and_released',
    'pipe_name_stays_bound_when_a_client_connects_and_leaves',
    'write_to_a_closed_peer_fails_promptly',
    'every_connection_must_complete_the_authenticated_handshake',
    'stalled_connection_does_not_block_other_clients',
    'read_waits_for_data_instead_of_reporting_end_of_file',
    'read_reports_end_of_file_only_after_the_peer_closes',
    'data_written_before_the_writer_closes_is_still_readable',
    'handshake_waits_for_a_client_that_writes_after_connecting',
    'idle_connections_hold_their_instances_until_they_close'
)
$IpcTests = @(
    'frame_round_trip_uses_versioned_big_endian_header',
    'oversized_length_is_rejected_before_body_read',
    'truncated_header_and_body_are_typed',
    'wrong_version_and_malformed_json_are_typed_without_payload',
    'non_eof_read_errors_are_not_reported_as_truncated',
    'json_rpc_string_request_ids_are_supported',
    'chooses_highest_common_version_and_rejects_unknown_capability_safely',
    'absent_token_is_authentication_failure',
    'startup_replaces_only_regular_stale_token',
    'reports_no_common_version',
    'token_encoding_requires_lowercase_hex',
    'capability_set_names_are_unique_and_round_trip',
    'client_wrappers_send_the_documented_methods_and_parameters',
    'methods_are_unavailable_without_the_negotiated_capability',
    'a_handler_without_review_never_exposes_review_methods',
    'server_gates_each_method_on_its_own_capability',
    'replay_larger_than_the_live_buffer_is_paged_across_reconnect',
    'cursor_ahead_of_the_journal_is_a_typed_replay_gap',
    'failed_resume_keeps_the_cursor_until_a_resume_succeeds',
    'replaced_journal_with_a_longer_history_is_an_epoch_mismatch_not_a_silent_resume',
    'resume_in_the_same_epoch_still_works_and_a_wrong_epoch_is_refused_at_subscribe',
    'call_during_a_large_replay_keeps_events_and_the_connection_in_sync',
    'live_lag_is_typed_and_reconnect_catches_up_in_order',
    'reconnect_follows_a_daemon_restart_with_a_new_token',
    'reconnect_still_requires_the_current_token'
) + $WindowsTransportTests
$DaemonTests = @(
    'lock_conflict_reports_live_holder_pid',
    'every_rpc_method_rejects_malformed_parameters',
    'task_submission_is_deduplicated_and_hides_project_path',
    'missing_task_returns_typed_not_found',
    'session_events_replay_task_and_only_selected_session_events',
    'queued_task_cancellation_is_idempotent',
    'running_task_cancellation_rejects_owner_without_process_identity',
    'verifying_task_cancellation_is_rejected_without_partial_attempt_update',
    'approval_resolution_uses_allowlisted_actor_and_is_idempotent',
    'running_task_keeps_daemon_alive_without_connected_clients',
    'idempotency_fingerprint_normalizes_omitted_optional_fields',
    'project_registration_validates_the_repository_and_is_idempotent',
    'session_events_page_and_report_a_cursor_beyond_the_journal',
    'workspace_diff_accepts_candidate_path_inside_task_worktree',
    'workspace_diff_rejects_empty_path_before_git',
    'workspace_diff_rejects_absolute_path_before_git',
    'workspace_diff_rejects_parent_component_before_git',
    'workspace_diff_rejects_option_prefix_before_git',
    'delivery_refuses_unapproved_local_review_without_changing_refs',
    'approved_local_review_delivery_creates_the_review_ref_once',
    'new_methods_reject_malformed_parameters',
    'capabilities_include_coordinator_and_review',
    'coordinator_messages_are_idempotent_ordered_and_user_authored',
    'coordinator_enforces_project_existence_and_size_limits',
    'coordinator_get_pages_at_one_hundred_entries',
    'valid_proposal_is_validated_without_authority_or_side_effects',
    'invalid_proposals_report_stable_codes',
    'proposals_cannot_smuggle_authority_fields',
    'request_approval_binds_the_delivery_digest_and_is_replayable',
    'request_approval_refuses_unsafe_requests',
    'request_approval_requires_passed_verification_of_the_current_head',
    'evidence_without_an_approval_is_missing_and_pinned_to_head',
    'evidence_is_current_for_a_fresh_approval_and_stale_after_the_workspace_moves',
    'evidence_marks_an_unverified_unknown_candidate_stale',
    'evidence_snapshot_is_size_bounded_and_path_filterable',
    'evidence_for_unknown_task_or_approval_is_not_found',
    'request_changes_blocks_approval_but_not_rejection',
    'request_changes_refuses_stale_duplicate_and_unknown_requests',
    'request_changes_refuses_a_decided_approval',
    'approval_from_review_request_delivers_end_to_end',
    'evidence_marks_a_digest_that_no_longer_matches_its_parameters_stale',
    'evidence_snapshot_lists_at_most_one_thousand_files'
)

# Mode: 'test' (default), 'build', 'lint', 'exit', or 'vitest'.
# Expect (test): tests that must report ok.
# Passed, Ignored (test): exact counts. All are DERIVED from the source for Windows and
#   read from the cfg and #[test] attributes at the pinned source. They are not Mac counts and are
#   not measured before this run. A different measured count is INCONCLUSIVE, never edited here.
# RunningSequence (test): the exact 'running N tests' values, in order, one per binary.
#   A FILTERED cargo run prints no doc-test stage, so its sequence has one entry.
# Crates (build, lint): crates whose Compiling (build) or Checking (lint) line must appear.
# Baseline (lint): the diagnostics that are known. Any other diagnostic is recorded as NEW and
#   does not change the verdict (see Get-CheckVerdict).
# Artifact (build): a file that must exist after the build. Its size and SHA-256 are recorded.
# Isolated: run with TEMP/TMP under $TestTmp and rustc on PATH.
# Exe, Directory (any mode): the program and working directory. The default is cargo in the
#   source checkout.
# Pattern (exit): a regex that must match standard output (Vite prints 'built in <time>').
# Files, Tests (vitest): the derived Vitest file and test counts. They are Mac-measured and
#   believed platform-neutral, and they are unmeasured on Windows.
# TimeoutSeconds: limit for the command (default 1800).
$UiDir = Join-Path $SourceDir 'apps\desktop'
$Checks = @(
    @{ Name = 'daemon-build'; Mode = 'build'; Expect = @(); Single = $false
       Crates = @('orchestrator-ipc', 'orchestrator-store', 'orchestrator-engine', 'orchestrator-daemon')
       Artifact = (Join-Path $BuildDir 'debug\orchestrator-daemon.exe')
       Arguments = @('build', '--locked', '-p', 'orchestrator-daemon') },
    @{ Name = 'ipc-tests'; Mode = 'test'; Expect = $IpcTests; Single = $false
       Passed = 38; Ignored = 0; Isolated = $true; TimeoutSeconds = 1800
       RunningSequence = @(38, 0)
       Arguments = @('test', '--locked', '-p', 'orchestrator-ipc') },
    @{ Name = 'daemon-tests'; Mode = 'test'; Expect = $DaemonTests; Single = $false
       Passed = 42; Ignored = 0; Isolated = $true; TimeoutSeconds = 1800
       RunningSequence = @(42, 0, 0)
       Arguments = @('test', '--locked', '-p', 'orchestrator-daemon') },
    @{ Name = 'windows-transport-tests'; Mode = 'test'; Expect = $WindowsTransportTests; Single = $false
       Passed = 13; Ignored = 0; Isolated = $true; TimeoutSeconds = 1800
       RunningSequence = @(13)
       Arguments = @('test', '--locked', '-p', 'orchestrator-ipc', 'windows_transport') },
    @{ Name = 'clippy-scoped'; Mode = 'lint'; Expect = @(); Single = $false
       Crates = @('orchestrator-ipc', 'orchestrator-daemon')
       Baseline = '(?i)crates[\\/]orchestrator-runtime[\\/]src[\\/]supervisor\.rs:(?:401|424):\d+'
       BaselineMessage = 'you seem to be trying to use `match` for destructuring a single pattern'
       Arguments = @('clippy', '--locked', '-p', 'orchestrator-ipc', '-p', 'orchestrator-daemon',
           '--all-targets') },
    @{ Name = 'desktop-npm-ci'; Mode = 'exit'; Expect = @(); Single = $false
       Exe = $Npm; Directory = $UiDir; TimeoutSeconds = 1200
       Arguments = @('ci') },
    @{ Name = 'desktop-tsc'; Mode = 'exit'; Expect = @(); Single = $false
       Exe = $Npx; Directory = $UiDir; TimeoutSeconds = 900
       Arguments = @('tsc', '-b') },
    @{ Name = 'desktop-build'; Mode = 'exit'; Expect = @(); Single = $false
       Exe = $Npm; Directory = $UiDir; TimeoutSeconds = 900
       Pattern = '(?m)\bbuilt in \d'
       Arguments = @('run', 'build') },
    @{ Name = 'desktop-vitest'; Mode = 'vitest'; Expect = @(); Single = $false
       Exe = $Npx; Directory = $UiDir; TimeoutSeconds = 1200
       Files = 14; Tests = 268
       Arguments = @('vitest', 'run') }
)

# UNPROVEN marks a row the host could not exercise. It never raises the overall
# verdict, and an overall PASS never covers it: result.md lists every UNPROVEN row.
$Rank = @{ PASS = 0; UNPROVEN = 0; INCONCLUSIVE = 1; FAIL = 2 }

# Splits cargo stderr into distinct diagnostics (warning or error blocks with a --> location),
# skipping the per-crate summary lines. Each is {Level, Message, Location}.
function Get-ClippyDiagnostics([string] $Text) {
    $Found = [Collections.Generic.List[object]]::new()
    $Seen = [Collections.Generic.HashSet[string]]::new()
    $Matches2 = [regex]::Matches($Text,
        '(?m)^(?<lvl>warning|error)(?:\[[^\]\r\n]*\])?: (?<msg>[^\r\n]*)\r?\n(?:[ \t]*-->[ \t]*(?<loc>[^\r\n]*))?')
    foreach ($M in $Matches2) {
        $Msg = $M.Groups['msg'].Value.Trim()
        if ($Msg -match '^`[^`]+` \(.*\) generated \d+ warnings?\b' -or
            $Msg -match '^aborting due to' -or $Msg -match '^could not compile') { continue }
        $Loc = $M.Groups['loc'].Value.Trim()
        if ($Seen.Add("$Loc|$Msg")) {
            $Found.Add([PSCustomObject]@{ Level = $M.Groups['lvl'].Value; Message = $Msg; Location = $Loc })
        }
    }
    , @($Found)
}

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

    if ($Check.Mode -ceq 'build' -or $Check.Mode -ceq 'lint') {
        # Cargo writes Compiling, Checking, Finished, and diagnostics to stderr. [ \t]* is used
        # instead of \s* so a pattern never crosses a line end.
        $ErrPath = Join-Path $EvidenceDir "$($Check.Name).stderr.txt"
        $Err = ''
        if (Test-Path -LiteralPath $ErrPath) { $Err = [IO.File]::ReadAllText($ErrPath) }
        $Lines = [regex]::Matches($Err, '(?m)^[ \t]*(?:Compiling|Checking|Finished|warning|error)\b.*$') |
            ForEach-Object { $_.Value.Trim() }
        [IO.File]::WriteAllLines(
            (Join-Path $EvidenceDir "$($Check.Name).summary.txt"), [string[]]@($Lines))
        $Verb = if ($Check.Mode -ceq 'build') { 'Compiling' } else { 'Checking' }
        foreach ($Crate in $Check.Crates) {
            $Pattern = '(?m)^[ \t]*' + $Verb + '[ \t]+' + [regex]::Escape($Crate) + '[ \t]+v\d'
            if (-not [regex]::IsMatch($Err, $Pattern)) {
                & $Raise 'INCONCLUSIVE' "No '$Verb $Crate' line."
            }
        }
        if (-not [regex]::IsMatch($Err, '(?m)^[ \t]*Finished[ \t]')) {
            & $Raise 'INCONCLUSIVE' "No 'Finished' line."
        }
        $Warnings = [regex]::Matches($Err, '(?m)^[ \t]*warning(?:\[[^\]\r\n]*\])?:').Count
        if ($Check.Mode -ceq 'lint') {
            # Classify every distinct diagnostic. The recorded baseline is BASELINE. Any other
            # warning is NEW: it is recorded verbatim and does not change the verdict. A
            # compile error also exits non-zero, which is FAIL above.
            $Diags = Get-ClippyDiagnostics $Err
            $Report = [Collections.Generic.List[string]]::new()
            $NewCount = 0; $BaselineSeen = 0
            foreach ($D in $Diags) {
                $IsBaseline = ($D.Level -ceq 'warning') -and ($D.Location -match $Check.Baseline) -and
                    $D.Message.Contains($Check.BaselineMessage)
                if ($IsBaseline) { $BaselineSeen++ } else { $NewCount++ }
                $Report.Add("$(if ($IsBaseline) { 'BASELINE' } else { 'NEW' }) $($D.Level): $($D.Message) @ $($D.Location)")
            }
            if ($Warnings -gt 0 -and $Diags.Count -eq 0) {
                $State.Notes.Add("$Warnings warning line(s) were seen but none could be classified. Report the stderr verbatim.")
            }
            # Report-only reconciliation of the per-crate summary lines. It never changes the verdict.
            $Declared = 0
            foreach ($S in [regex]::Matches($Err, '(?m)^warning: `[^`\r\n]+` \([^)\r\n]*\) generated (\d+) warnings?\b')) {
                $Declared += [int]$S.Groups[1].Value
            }
            $Distinct = @($Diags | Where-Object { $_.Level -ceq 'warning' }).Count
            $Report.Add("RECONCILIATION (report only): summary lines declare $Declared warning(s) in total; $Distinct distinct warning diagnostic(s) were classified; $BaselineSeen baseline; $NewCount new.")
            [IO.File]::WriteAllLines((Join-Path $EvidenceDir "$($Check.Name).diagnostics.txt"),
                [string[]]@($Report))
            $State.Notes.Add("$BaselineSeen baseline diagnostic(s) (supervisor.rs single_match in orchestrator-runtime, a dependency of the daemon) and $NewCount NEW diagnostic(s) recorded in $($Check.Name).diagnostics.txt. NEW diagnostics do not change the verdict; report each one verbatim.")
        } elseif ($Warnings -gt 0) {
            $State.Notes.Add("$Warnings warning line(s) noted. A build warning does not change the verdict.")
        }
        if ($Check.Artifact) {
            if (Test-Path -LiteralPath $Check.Artifact -PathType Leaf) {
                $Item = Get-Item -LiteralPath $Check.Artifact
                $Hash = (Get-FileHash -LiteralPath $Check.Artifact -Algorithm SHA256).Hash.ToLowerInvariant()
                [IO.File]::WriteAllText((Join-Path $EvidenceDir 'daemon-binary.txt'),
                    "artifact: debug\orchestrator-daemon.exe (relative to the target directory)`n" +
                    "bytes: $($Item.Length)`nsha256: $Hash`n")
            } else {
                [IO.File]::WriteAllText((Join-Path $EvidenceDir 'daemon-binary.txt'),
                    "artifact: debug\orchestrator-daemon.exe (relative to the target directory)`nmissing`n")
                & $Raise 'INCONCLUSIVE' 'The daemon binary does not exist after the build.'
            }
        }
    } elseif ($Check.Mode -ceq 'exit' -or $Check.Mode -ceq 'vitest') {
        # Desktop gates (Node tools). The output is stripped of ANSI colour codes before parsing.
        $StdoutPath = Join-Path $EvidenceDir "$($Check.Name).stdout.txt"
        $Out = ''
        if (Test-Path -LiteralPath $StdoutPath) { $Out = [IO.File]::ReadAllText($StdoutPath) }
        $Out = [regex]::Replace($Out, '\x1B\[[0-9;?]*[ -/]*[@-~]', '')
        $Lines = [regex]::Matches($Out, '(?m)^[ \t]*(?:Test Files|Tests|Duration|.*\bbuilt in \d.*)[ \t]*.*$') |
            ForEach-Object { $_.Value.Trim() }
        [IO.File]::WriteAllLines(
            (Join-Path $EvidenceDir "$($Check.Name).summary.txt"), [string[]]@($Lines))
        if ($Check.Pattern -and -not [regex]::IsMatch($Out, $Check.Pattern)) {
            & $Raise 'INCONCLUSIVE' "The output has no line matching $($Check.Pattern)."
        }
        if ($Check.Mode -ceq 'vitest') {
            # Vitest prints 'Test Files  14 passed (14)' and 'Tests  268 passed (268)'. A failed
            # or skipped test adds 'N failed' or 'N skipped' to the same line.
            foreach ($Row in @(@('Test Files', $Check.Files), @('Tests', $Check.Tests))) {
                $Label = $Row[0]; $Expected = [int]$Row[1]
                $Line = [regex]::Match($Out, '(?m)^[ \t]*' + [regex]::Escape($Label) + '[ \t]+(?<rest>.*)$')
                if (-not $Line.Success) {
                    & $Raise 'INCONCLUSIVE' "No '$Label' summary line."
                    continue
                }
                $Rest = $Line.Groups['rest'].Value
                $Total = [regex]::Match($Rest, '\((\d+)\)\s*$')
                $PassedN = [regex]::Match($Rest, '(\d+) passed')
                $FailedN = [regex]::Match($Rest, '(\d+) failed')
                $SkippedN = [regex]::Match($Rest, '(\d+) (?:skipped|todo)')
                if ($FailedN.Success -and [int]$FailedN.Groups[1].Value -gt 0) {
                    & $Raise 'FAIL' "$Label : $($FailedN.Groups[1].Value) failed."
                }
                if ($SkippedN.Success -and [int]$SkippedN.Groups[1].Value -gt 0) {
                    & $Raise 'INCONCLUSIVE' "$Label : $($SkippedN.Groups[1].Value) skipped or todo."
                }
                if (-not $PassedN.Success -or -not $Total.Success) {
                    & $Raise 'INCONCLUSIVE' "$Label line could not be parsed: $Rest"
                } else {
                    if ([int]$PassedN.Groups[1].Value -ne $Expected -or [int]$Total.Groups[1].Value -ne $Expected) {
                        & $Raise 'INCONCLUSIVE' "$Label : $($PassedN.Groups[1].Value) passed of $($Total.Groups[1].Value). Expected $Expected (Mac-measured, not measured on Windows before this run)."
                    }
                    if ($Label -ceq 'Tests') { $Passed = [int]$PassedN.Groups[1].Value }
                }
                if ($Label -ceq 'Tests' -and $FailedN.Success) { $Failed = [int]$FailedN.Groups[1].Value }
            }
            if ($null -ne $Passed -and $null -eq $Failed) { $Failed = 0 }
        }
    } else {
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
                & $Raise 'INCONCLUSIVE' "Per-binary running counts $($Running -join ',') differ from the derived $($Check.RunningSequence -join ',')."
            }
            if ($null -ne $Check.Passed -and $Passed -ne $Check.Passed) {
                & $Raise 'INCONCLUSIVE' "$Passed passed. Expected $($Check.Passed) (derived for Windows, not measured before this run)."
            }
            if ($null -ne $Check.Ignored -and $Ignored -ne $Check.Ignored) {
                & $Raise 'INCONCLUSIVE' "$Ignored ignored. Expected $($Check.Ignored) (derived for Windows, not measured before this run)."
            }
            foreach ($Test in $Check.Expect) {
                $Pattern = '(?m)^test (?:\S+::)*' + [regex]::Escape($Test) + ' \.\.\. ok\s*$'
                if (-not [regex]::IsMatch($Out, $Pattern)) {
                    & $Raise 'INCONCLUSIVE' "No 'ok' line for $Test."
                }
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
    $RunExe = if ($Check.Exe) { $Check.Exe } else { $Cargo }
    $RunDir = if ($Check.Directory) { $Check.Directory } else { $SourceDir }
    if ($Check.Isolated) {
        # Private temp directory and rustc on PATH for the test's fake agent.
        $CheckTmp = Join-Path $TestTmp $Check.Name
        New-Item -ItemType Directory -Path $CheckTmp | Out-Null
        $SavedEnv = @{ TEMP = $env:TEMP; TMP = $env:TMP; PATH = $env:PATH }
        $env:TEMP = $CheckTmp; $env:TMP = $CheckTmp
        $env:PATH = (Split-Path -Parent $Rustc) + ';' + $env:PATH
        try {
            $Run = Invoke-CapturedCargo -Name $Check.Name -Arguments $Check.Arguments `
                -TimeoutSeconds $Timeout -Isolated $true -Exe $RunExe -Directory $RunDir
        } finally {
            $env:TEMP = $SavedEnv.TEMP; $env:TMP = $SavedEnv.TMP; $env:PATH = $SavedEnv.PATH
        }
    } else {
        $Run = Invoke-CapturedCargo -Name $Check.Name -Arguments $Check.Arguments `
            -TimeoutSeconds $Timeout -Exe $RunExe -Directory $RunDir
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

# ---- Rows this runbook does not exercise ------------------------------------------------
# NOT RUN or UNPROVEN rows. Each gets a one-line file and a verdict row with status UNPROVEN.
# UNPROVEN never raises the overall verdict, and an overall PASS never covers these rows:
# result.md lists every one of them. No daemon is started, no probe is run, no account is
# created, and no credential is read.
$UnprovenDefinitions = [ordered]@{
    'unproven-second-user' = 'Denial of the named pipe to a second local user was not exercised. No second account is used.'
    'unproven-remote-client' = 'Rejection of a genuinely remote client was not exercised. No remote host or SMB loopback is used.'
    'unproven-slow-reader-64kib' = 'A daemon-to-client write above 64 KiB to a slow reader was not exercised.'
    'unproven-tauri-native-bridge' = 'The Tauri shell and its native bridge to the daemon were not built or run. The shipped app has no connector until M5-05.'
    'unproven-e2e-windows' = 'NOT RUN. The desktop e2e suite (npm run test:e2e) uses a Unix domain socket and cannot run on Windows. No Windows e2e number exists.'
}
foreach ($Name in $UnprovenDefinitions.Keys) {
    [IO.File]::WriteAllText((Join-Path $EvidenceDir "$Name.txt"),
        "status: UNPROVEN`n$($UnprovenDefinitions[$Name])`n")
    $Verdicts += [PSCustomObject]@{
        name = $Name; status = 'UNPROVEN'; exit_code = $null; timed_out = $false
        duration_seconds = 0; running = ''; passed = $null; failed = $null; ignored = $null
        notes = "$($UnprovenDefinitions[$Name]) See $Name.txt."
    }
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

- `FAIL`: timeout, process error, missing exit code, non-zero exit code, a failed test, or any
  process of this run (image under the private temp directory, or a command line naming this run's
  directory) that is still running after the check.
- `INCONCLUSIVE`, test gates: no `test result` line, or the `running` and `test result` lines do not
  pair up. Also: every binary reports `running 0 tests`. Also: the `running N tests` sequence, or a
  passed or ignored count, differs from the derived count (a **wrong or missing count**, including a
  count that equals the Mac count when the derived Windows count is different). Also: a required test
  has no `ok` line. A skipped or zero-test selection is never a pass.
- `INCONCLUSIVE`, build gate: a required `Compiling` or `Finished` line is missing, or the daemon
  binary is absent after an exit-0 build. A non-zero build exit, including a missing MSVC C
  toolchain for `libsqlite3-sys`, is `FAIL`. Preserve the output so that the coordinator can tell a
  host problem from a product problem. A warning in the build gate is noted and does not change its
  verdict.
- `INCONCLUSIVE`, clippy gate: a required `Checking` or `Finished` line is missing. **Diagnostics do
  not change the verdict.** This is a change from the Priority 9 remediation runbook, which treated
  any diagnostic outside its baseline as `INCONCLUSIVE`; it is disclosed under "What changed from the
  approved harness". Each distinct warning is classified on its own: `BASELINE` (exactly the two
  `single_match` warnings at `supervisor.rs` lines 401 and 424 in `orchestrator-runtime`, a dependency
  that the daemon scope also lints) or `NEW`. Every `NEW` diagnostic is written verbatim to
  `clippy-scoped.diagnostics.txt` and named in the notes, and the operator reports each one verbatim.
  A non-zero exit (a compile error, which clippy also reports as an `error` diagnostic) is `FAIL`.
- `INCONCLUSIVE`, desktop exit gates: `desktop-build` prints no `built in` line. The gates
  `desktop-npm-ci` and `desktop-tsc` are `PASS` on exit code zero and `FAIL` otherwise.
- `INCONCLUSIVE`, `desktop-vitest`: no `Test Files` or `Tests` summary line, a line that does not
  parse, or a passed count or total that is not 14 files and 268 tests. A skipped or `todo` test is
  `INCONCLUSIVE`. A failed test or file is `FAIL`. The numbers are Mac-measured and unmeasured on
  Windows.
- Rows `unproven-second-user`, `unproven-remote-client`, `unproven-slow-reader-64kib`,
  `unproven-tauri-native-bridge`, and `unproven-e2e-windows` are `UNPROVEN` by construction (the last
  one is also NOT RUN). `UNPROVEN` never changes the overall verdict, and the report lists every
  `UNPROVEN` row.
- `PASS`: exit code zero, every required line and test present, zero tests failed, no survivor, and
  the counts match the derived counts.

Derived counts are never edited. If the measured count differs, the verdict is `INCONCLUSIVE`, the
measured number is reported exactly as printed, and the coordinator decides what it means. Never
change `Passed`, `Ignored`, `RunningSequence`, `Files`, or `Tests` to match what you saw.

Per-gate notes on how the counts print (these are the reasons a correct run can still be
`INCONCLUSIVE`, so read them before you report one):

- `ipc-tests` is unfiltered, so cargo prints the unit binary and then the doc-test stage:
  `running 38 tests`, then `running 0 tests`. The sequence is `38,0`.
- `windows-transport-tests` is filtered (`windows_transport`). **A filtered cargo run prints no
  doc-test stage**, so the sequence is the single value `13`, and the result line ends in `25 filtered
  out` (derived, 38 - 13). The Priority 9 remediation run measured exactly this shape at `7435ea32`
  (`13 passed … 18 filtered out`), and its runbook expected `13,0`, which was wrong.
- `daemon-tests` runs the unit binary and the two integration binaries, both empty on Windows
  (`#![cfg(unix)]`). The sequence `42,0,0` is derived from the file list and from the Priority 9
  measurement (`20,0` with one such binary); the two trailing zeros have never been measured (the first
  Priority 10 run stopped at the failing unit binary).
- The ipc and daemon counts are **derived and unmeasured**. A different measured value is reported
  as printed, with the full `running` and `test result` lines.

A compile failure is not a reason to stop: the gate is `FAIL` (exit 101), the harness runs the
remaining gates, and Step 5 still publishes. See "If the build fails (compile-failure triage)".

A timeout stays `FAIL`. The first check compiles everything into a new target directory, so a cold
build can exceed a limit. In your report, say whether the preserved stdout or stderr of a timed-out
check ends in `Compiling` or `Finished` output with no `running N tests` line (the build was still
running: an environment or resource limit) or shows tests that had started (a product or test hang).
That label is a note only. Do not change the verdict, retry in the same evidence directory, or raise
a limit.

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
foreach ($Name in (@($Checks | ForEach-Object { $_.Name }) + @($UnprovenDefinitions.Keys))) {
    if (@($Verdicts | Where-Object { $_.name -ceq $Name }).Count -ne 1) {
        $Overall = 'FAIL'; $Extra.Add("No single verdict for $Name.")
    }
}
# UNPROVEN rows never raise the verdict. They are listed so a PASS cannot be read as covering them.
$UnprovenRows = @($Verdicts | Where-Object { $_.status -ceq 'UNPROVEN' } | ForEach-Object { $_.name })
$UnprovenText = if ($UnprovenRows.Count -gt 0) { $UnprovenRows -join ', ' } else { 'none' }
if ($UnprovenRows.Count -gt 0) {
    $Extra.Add("UNPROVEN, not covered by this result: $UnprovenText.")
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
# A command has <name>.summary.txt. An UNPROVEN row has <name>.txt.
$Summaries = ($Verdicts | ForEach-Object {
    $SummaryPath = Join-Path $EvidenceDir "$($_.name).summary.txt"
    if (-not (Test-Path -LiteralPath $SummaryPath -PathType Leaf)) {
        $SummaryPath = Join-Path $EvidenceDir "$($_.name).txt"
    }
    "### $($_.name)`n`n${Fence}text`n" +
    ([IO.File]::ReadAllText($SummaryPath).TrimEnd()) +
    "`n$Fence"
}) -join "`n`n"
# Expandable here-string: a backtick inside it is an escape (`r, `t, ...).
# Keep backticks out of this text.
$Report = @"
# Priority 10r2 Windows acceptance of $SourceSha (M5-02b scheduler and executor, M5-02c coordinator and review, M5-03 and M5-04 desktop, with the CRLF fixture remediation)

This run covers the remediated Priority 10 candidate: the IPC coordinator and review capabilities and
epoch handling, the daemon (executor, coordinator and review API) built natively on Windows, the
named-pipe transport tests, and the desktop install, type-check, build and unit
tests. The coordinator decides acceptance. The expected counts are derived from the cfg and test
attributes at the pinned source for Windows (IPC 38, daemon 42, windows_transport 13) and from the
Mac Vitest run (14 files, 268 tests). They are derived and unmeasured for this commit; the first
Priority 10 run on the previous tree agreed with the IPC, filter, and Vitest values. They are not the
Mac counts (IPC 37, daemon 59), and they are not the
Priority 9 remediation native counts (a different tree). Counts are reported as measured, not
matched. No macOS or review result is Windows evidence. This run is not a native probe, not a
second-user, genuinely remote-client, slow-reader, Tauri bridge, or Windows e2e claim, and not an M6
threat-review claim.

- Run ID: $RunId
- UTC date: $([DateTime]::UtcNow.ToString('o'))
- Source branch: $SourceBranch
- Branch tip at clone time: $BranchTip
- Source commit: $SourceSha
- Source tree and only parent (verified): source-tree.stdout.txt, source-parent.stdout.txt
- HEAD matched the source commit before the checks and after them: yes
  (source-head-before.stdout.txt, source-head-after.stdout.txt)
- Git top-level path form (plain drive-letter form required): source-toplevel.stdout.txt
- Baseline ancestry (7435ea32), commit count, the seventeen pinned commits, the linear range, and the
  changed-file set (114 files): base-ancestry, range-count, range-merges, range-commits, range-files
  (consistency gates, not patch-equivalence proof)
- Runbook identity (operator-declared; exact bytes pinned by blob id, size, and SHA-256; not proof that
  the pasted commands match): runbook-identity.txt
- Overall result: $Overall $($Extra -join ' ')
- UNPROVEN rows (never covered by an overall PASS): $UnprovenText
- Toolchain: see git-version.stdout.txt, rustc-version.stdout.txt,
  cargo-version.stdout.txt, clippy-version.stdout.txt, node-version.stdout.txt,
  npm-version.stdout.txt, git-autocrlf.stdout.txt (exit code in git-autocrlf.exit.txt)
- Daemon binary record (size and SHA-256 of the dev-profile build output): daemon-binary.txt
- Clippy diagnostics, each classified BASELINE or NEW (NEW does not change the verdict):
  clippy-scoped.diagnostics.txt
- UNPROVEN and NOT RUN rows: one unproven-*.txt file each. No native probe ran, and no daemon was
  started.
- OS, CPU, PowerShell: see os.txt, cpu.txt, powershell-version.txt
- Agent runtime and Sonnet 5.5/high setting: $AgentRuntime
- Survivor scan: every check has a survivors file. Processes whose image lies under
  this run's private temp directory, or whose command line names this run's directory (long or 8.3
  form), are listed and fail the check. Nothing is stopped by the scan.
- Raw stdout and stderr bytes are kept outside the checkout and outside the published files. Their
  size and SHA-256 are in each result.json.

| Command or row | Status | Exit | Timed out | Seconds | Running (N per binary) | Passed | Failed | Ignored | Notes |
|---|---|---|---|---|---|---|---|---|---|
$Table

## Result lines and observations

$Summaries

## Coverage gaps

- Unix-only tests did not run on Windows: IPC integration_tests.rs, lifecycle_tests.rs and
  timeout_tests.rs, the whole daemon executor_tests.rs module, tests/daemon.rs,
  tests/coordinator_review.rs, three daemon cancellation unit tests, and the daemon symlink test.
  Their macOS results are separate and are not Windows evidence. The scheduler and executor have no
  native Windows test in this run.
- NOT RUN: the desktop e2e suite (Unix domain socket only). No Windows e2e number exists.
- UNPROVEN, whatever the overall result: denial of a second, different local user; rejection of a
  genuinely remote client; a single daemon-to-client write above 64 KiB to a slow reader; and the
  Tauri shell and native bridge.
- The daemon was built but never started. Server identity verification by the client, the
  data-directory ACL, and the token owner are not validated on Windows.
- The Vitest counts (14 files, 268 tests) are Mac-measured. They are believed platform-neutral
  (jsdom with an in-memory fake daemon) and are unmeasured on Windows.
- No rustfmt gate, no workspace-wide test run, and no store, engine, git or CLI test run ran.
"@
# Redaction before anything is written. Then write both files.
$Report = Protect-Text $Report
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'result.md'), $Report)
[IO.File]::WriteAllText(
    (Join-Path $EvidenceDir 'verdicts.json'), ($Verdicts | ConvertTo-Json -Depth 4)
)
Write-Output "overall=$Overall"
```

Use PASS only when every gate is PASS (the UNPROVEN rows are the only other status allowed), `HEAD`
matches the full source commit, both source status checks are empty, and the diff check exits zero.
PASS here never covers an UNPROVEN or NOT RUN row: the report lists them. A PASS shows that the 9
gates passed on this host. It does not show Windows coverage for any scenario that no test step
ran, and it does not exercise the daemon.

If a command fails, preserve its complete output and report its exit code. If a process does not
stop, report it and do not start another attempt. For a retry, use a new run ID and preserve both
attempts.

## 5. Publish evidence only

Publish the evidence for every result, including FAIL and INCONCLUSIVE. Publish only flat text files
from `$EvidenceDir`. Do not publish `raw`, `cargo-target`, `test-tmp`, the source clone, session
files, agent memory, or any other path. Do not edit the source or `board.json`. Publication fails
closed: any scan hit or missing file stops it before a worktree exists.

The push goes to `origin` only, and only when `origin` is the one remote and equals the pinned
repository `j-mckerracher/agent-orch`. **Never push to or add a remote for `motif-mcao`.** The branch
that you push is `evidence/priority-10r2-windows-acceptance-e555c2c-<RunId>`. It holds evidence files
only. It is never `main`, `orch/…`, or `docs/…`.

```powershell
$EvidenceBranch = "evidence/priority-10r2-windows-acceptance-$Short-$RunId"
if ($EvidenceBranch -cnotmatch '^evidence/priority-10r2-windows-acceptance-e555c2c-\d{8}T\d{6}Z-[0-9a-f]{32}$') {
    throw 'The evidence branch name is not in the evidence/ namespace. Do not push.'
}
$EvidenceWorktree = Join-Path $env:TEMP "p10r2ev-$Short-$($RunId.Substring($RunId.Length - 8))"
$EvidencePath = "docs/evidence/priority-10r2-windows-acceptance/$Short-$RunId"

Assert-Origin 'before publish fetch'
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
    'windows-version.txt', 'powershell-version.txt', 'daemon-binary.txt',
    'clippy-scoped.diagnostics.txt')
foreach ($Name in $UnprovenDefinitions.Keys) { $Required += "$Name.txt" }
foreach ($Step in 'clone', 'branch-tip', 'candidate-containment', 'base-ancestry',
    'source-parent', 'source-tree', 'range-count', 'range-merges', 'range-commits', 'range-files',
    'runbook-fetch', 'runbook-type', 'runbook-parent', 'runbook-files',
    'runbook-blob', 'switch',
    'source-head-before', 'source-toplevel', 'source-head-after',
    'source-status-before', 'source-status-after',
    'diff-check', 'git-version', 'rustc-version', 'cargo-version', 'clippy-version',
    'node-version', 'npm-version',
    'git-autocrlf', 'publish-fetch') {
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
# Any e-mail address is a privacy hit. The commit identity below is the only one that may exist.
$EmailPattern = '[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}'
$Hits = @(Select-String -Path (Join-Path $EvidenceDir '*') -Pattern $SecretPattern) +
    @(Select-String -Path (Join-Path $EvidenceDir '*') -Pattern $UrlPattern) +
    @(Select-String -Path (Join-Path $EvidenceDir '*') -Pattern $EmailPattern) +
    @(Select-String -Path (Join-Path $EvidenceDir '*') -SimpleMatch 'j-mckerracher/agent-orch') +
    @(Select-String -Path (Join-Path $EvidenceDir '*') -SimpleMatch 'motif-mcao') +
    @(Select-String -Path (Join-Path $EvidenceDir '*') -Pattern $ProfileLeakPatterns)
if ($Hits.Count -ne 0) {
    $Hits | Select-Object Path, LineNumber | Format-Table | Out-String | Write-Output
    throw 'Possible secret, remote URL, e-mail address, or profile name found. Do not push. Keep the raw files and report it.'
}
```

The pattern scan is a backstop. Also read every file before you publish. If a file holds a secret, a
remote URL, an e-mail address, or a profile name, do not push it. Keep the raw file and report the
blocker.

The scan fails closed on the profile name. `Protect-Text` already replaced the directory name after
each `Users` path segment with `<user>`. The scan blocks any `Users` segment that is not followed by
`<user>` (in single-backslash, slash, and doubled-backslash JSON forms) and any whole-word occurrence
of the account name, the profile directory name, or its 8.3 alias. A user account whose name is an
ordinary word that appears elsewhere in the evidence is also blocked. Report that as BLOCKED. Do not
edit the files to get past it. The scan prints file names and line numbers only, never the matching
text.

**Commit identity (privacy).** The evidence commit must carry only the pinned public identity. Git for
Windows and the host may have a personal name and e-mail address in the global or system config, and
a commit made with it publishes that address. The next block sets the identity with
`git config --local` only, in the clone that the evidence worktree shares. **Never use `--global` or
`--system`**, and never change any other config. It refuses to run if a `GIT_AUTHOR_*` or
`GIT_COMMITTER_*` variable is set, because those override the repository config. It then checks the
effective identity before the commit, and checks author and committer of every new commit before the
push. The `Co-authored-by` trailer already holds a GitHub noreply address.

**Scope of the privacy check.** The evidence branch is created from `origin/main`, and the source
range inherits older commits (for example `a9bfbd6…` and the main tip `973b7ffd…`) whose author and
committer fields carry a personal e-mail address. This runbook does not rewrite or scan that history
and makes no claim about it. The e-mail scan above covers the evidence files only, and the identity
check below covers the evidence commit's own author and committer only.

```powershell
$PublishName = 'j-mckerracher'
$PublishEmail = '135927658+j-mckerracher@users.noreply.github.com'
foreach ($Var in 'GIT_AUTHOR_NAME', 'GIT_AUTHOR_EMAIL', 'GIT_COMMITTER_NAME', 'GIT_COMMITTER_EMAIL') {
    if (Test-Path -LiteralPath "Env:$Var") {
        throw "$Var is set and would override the commit identity. Do not commit. Report it (name only, not the value)."
    }
}
& $Git -C $SourceDir config --local user.name $PublishName
if ($LASTEXITCODE -ne 0) { throw 'Cannot set the repository-local user.name.' }
& $Git -C $SourceDir config --local user.email $PublishEmail
if ($LASTEXITCODE -ne 0) { throw 'Cannot set the repository-local user.email.' }
$EffectiveName = (& $Git -C $SourceDir config --get user.name)
$EffectiveEmail = (& $Git -C $SourceDir config --get user.email)
if ($EffectiveName -cne $PublishName -or $EffectiveEmail -cne $PublishEmail) {
    throw 'The effective git identity is not the pinned identity. Do not commit. Report it.'
}
```

```powershell
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
    -m "evidence: record Priority 10r2 Windows run of $Short ($Overall; UNPROVEN: $(($UnprovenRows | ForEach-Object { $_ -replace '^unproven-', '' }) -join ', '))" `
    -m 'Co-authored-by: Copilot App <223556219+Copilot@users.noreply.github.com>'
if ($LASTEXITCODE -ne 0) { throw 'Evidence commit failed. Report BLOCKED. Do not use --global or --system, and do not change credentials.' }
$EvidenceSha = & $Git -C $EvidenceWorktree rev-parse HEAD
$Identity = @(& $Git -C $EvidenceWorktree log -1 '--format=%an%n%ae%n%cn%n%ce')
if ($LASTEXITCODE -ne 0 -or $Identity.Count -ne 4) { throw 'Cannot read the commit identity.' }
$Identity | Set-Content -LiteralPath (Join-Path $EvidenceDir 'commit-identity.txt') -Encoding utf8
if ($Identity[0] -cne $PublishName -or $Identity[2] -cne $PublishName -or
    $Identity[1] -cne $PublishEmail -or $Identity[3] -cne $PublishEmail) {
    throw 'The commit identity is not the pinned identity. Do not push. Report it and keep the commit local.'
}
$Changed = @(& $Git -C $EvidenceWorktree diff --name-only refs/remotes/origin/main HEAD)
foreach ($Path in $Changed) {
    if (-not $Path.StartsWith("$EvidencePath/", [StringComparison]::Ordinal)) {
        throw "Commit holds an unexpected path: $Path"
    }
}
Assert-Origin 'before push'
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
- the overall result and the status of each of the 9 gates (`daemon-build`, `ipc-tests`,
  `daemon-tests`, `windows-transport-tests`, `clippy-scoped`, `desktop-npm-ci`, `desktop-tsc`,
  `desktop-build`, `desktop-vitest`) with exit code, duration, and, for the cargo test gates, the
  `running N tests` counts. List the 5 UNPROVEN rows (`unproven-second-user`,
  `unproven-remote-client`, `unproven-slow-reader-64kib`, `unproven-tauri-native-bridge`,
  `unproven-e2e-windows`) separately. The e2e suite is NOT RUN, and no e2e number exists
- the OS, CPU, `rustc -Vv`, `cargo -V`, `cargo clippy -V`, `git --version`, `node --version`,
  `npm --version`, and `core.autocrlf` (with its origin)
- the branch tip and the pinned source commit, and that `HEAD` matched before and after
- the Git top-level path form that `source-toplevel` recorded, and the source tree, parent, range
  count, range commits, and changed-file checks
- the **measured** IPC, daemon, `windows_transport` and Vitest counts, exactly as printed (the
  `running N tests` sequence, passed, failed, and ignored; Vitest `Test Files` and `Tests` lines),
  next to the derived counts (IPC 38 with sequence `38,0`; `windows_transport` 13 with sequence
  `13`; daemon unit 42 with the two integration binaries `running 0 tests`, sequence `42,0,0`;
  Vitest 14 files and 268 tests). Label each as measured now, derived, Mac, or prior native. The
  Mac counts are IPC 37 and daemon 59 (not Windows evidence). The `42,0,0` and `38,0` sequences are
  derived from the cfg attributes and file list; they were observed only on earlier trees (`42,0,0` at
  `abd3ab25…`, `38,0` at `67c1181d…`) and never on this commit. A mismatch is a finding,
  not a reason to edit anything. State that the Windows counts differ from the Mac counts by design
  and that you reported the measurement and did not match it to any other number
- whether the daemon built natively with the M5-02b and M5-02c code (`daemon-build`). If it did
  not, the full stderr, the first error verbatim, and the triage record from "If the build fails
  (compile-failure triage)"
- the daemon binary size and SHA-256 from `daemon-binary.txt`, every build warning line verbatim, and
  the full text of `clippy-scoped.diagnostics.txt` (BASELINE and NEW diagnostics)
- the coverage gaps: Unix-only tests not run, the UNPROVEN and NOT RUN rows, and the items not
  validated on Windows
- the runbook identity record: blob id, byte length, and SHA-256 of the pinned runbook bytes
- for `ipc-tests`, `daemon-tests`, and `windows-transport-tests`: the `running N tests` lines, the
  `survivors.txt` content (empty means none found), and for a failure the first failing assertion
  message exactly as printed
- for any timed-out check, the cold-build label from the status rules, as a note beside the unchanged
  `FAIL`
- that `protect-text-selfcheck.txt` and the publication scan (including the e-mail scan) passed
- the commit identity: the four lines of `git log -1 --format=%an%n%ae%n%cn%n%ce` for the evidence
  commit, which must be exactly `j-mckerracher`, `135927658+j-mckerracher@users.noreply.github.com`,
  `j-mckerracher`, and the same noreply address; and that you set it with `git config --local` only.
  Do not paste any other e-mail address, name, or profile directory into the report
- any setting that you could not observe, including whether your agent runtime could run Sonnet 5.5
  with high reasoning

Do not edit `board.json`. The coordinator reviews the evidence and updates the board.

## What changed from the approved harness

The harness blocks derive from the Priority 9 remediation runbook (accepted on Windows at evidence
commit `9a9f3bf688bd41a5b5faa797a6d8e243e6d4fef5`), which derives from the Priority 8 and Priority 7
runbooks. Preserved unchanged in behavior: PowerShell 7 only, the pinned SHA and `HEAD` equality
before and after, the declared runbook commit and the runbook byte pin, clean-checkout gates, unique
run directories outside the checkout, `Protect-Text` and its self-check, `Invoke-Bounded` (byte-exact
raw capture, tree kill on timeout only), the survivor scan after every check, the fail-closed
publication scan, the `Expect`, `Passed`, `Ignored`, `RunningSequence`, `Isolated`, `TimeoutSeconds`
model, the never-kill-a-survivor rule, the origin-only `Assert-Origin` checks, and evidence-only
branch publication to `origin`.

Changed for Priority 10 (relative to the Priority 9 remediation runbook):

- **Native probe dropped.** The 10 probe rows, `Protect-Principal`, the probe context, the probe
  tokens, and the probe daemon rounds are gone. The Priority 9 remediation probe evidence proves only
  that older tree. For this tree, instance-limit, large-frame, connect-close, daemon-exit and every
  other probe-type scenario are not run and are not claimed. Only the 5 UNPROVEN or NOT RUN rows are
  recorded, as stub files that state the row was not run.
- **Filtered-run sequence fixed.** The `windows-transport-tests` gate expects `RunningSequence`
  `@(13)`. A filtered cargo run prints no doc-test stage, so the Priority 9 remediation value `13,0`
  was wrong (the measured run printed `13 passed; 18 filtered out` and no second stage). It is a
  correction, not a tolerance.
- **Clippy policy reversed for NEW diagnostics.** In the Priority 9 remediation runbook any
  diagnostic outside the two `single_match` baselines made the gate `INCONCLUSIVE`. Here each
  diagnostic is classified BASELINE or NEW and recorded verbatim in
  `clippy-scoped.diagnostics.txt`, and NEW diagnostics do not change the verdict. A non-zero cargo
  exit is still `FAIL`. The reason is that this candidate adds about 18,000 lines that were never
  linted on Windows, and the request was to record what is new, not to fail on it.
- **UI gates added.** `desktop-npm-ci`, `desktop-tsc`, `desktop-build`, and `desktop-vitest` run in
  `apps\desktop`. They use new `exit` and `vitest` verdict modes, and `Invoke-CapturedCargo` was
  generalized to take an executable and a working directory. Node and npm are new required tools
  (`Get-Command` for `node.exe`, `npm.cmd`, `npx.cmd`), and `NO_COLOR`, `FORCE_COLOR`, and `CI` are
  set so the output parses.
- **Clippy scope and daemon gates.** `daemon-build` also checks that `orchestrator-ipc`,
  `orchestrator-store`, `orchestrator-engine`, and `orchestrator-daemon` compiled, because the daemon
  now links the store and engine and `libsqlite3-sys` builds C code.
- **Re-pinned (Priority 10r2, from the first rerun runbook `abd3ab2`).** Title, source commit
  `e555c2c9…`, tree `fae7306c…`, base `7435ea32…`, source parent `abd3ab25…`, branch name check
  (`orch/priority-10r2-…`), the range count (17), the 17 commit ids in order (the 14 of the first
  Priority 10 candidate, then `f246fe36…`, `abd3ab25…` and `e555c2c9…`), the linear range (zero
  merges), the 114 changed files (the same set as before), evidence branch and path names
  (`priority-10r2-windows-acceptance-e555c2c-…`), run-root prefix (`agent-orch-p10r2-acc-`), evidence
  worktree prefix (`p10r2ev-`), runbook file name, and links.
- **Remediations.** The first Priority 10 Windows run on `67c1181d…` failed one daemon unit test
  (`api::review_tests::evidence_snapshot_lists_at_most_one_thousand_files`) because the test fixture
  staged a CRLF-converted checkout file under the system `core.autocrlf=true`, so the commit held 1001
  paths instead of 1000. The first remediation (`f246fe36…`) fixes the fixture, and `abd3ab25…` gates the
  `blocked_reason` helper in `executor.rs` with `cfg(all(test, unix))`, which removes the Windows
  dead-code warning; both held in the first rerun. That rerun failed one IPC test,
  `coordinator_review_tests::server_gates_each_method_on_its_own_capability` (a 50 ms stream timeout
  dropped an idle connection on Windows). The second remediation (`e555c2c9…`) raises the
  `coordinator_review_tests` stream timeout to 5 s and the `event_stream_tests` one to 1 s, makes the
  capability assertions strict, and prints dispatch diagnostics on failure. No production behavior
  changed. The second remediation was verified on the Mac only.
- **Privacy scan scope for inherited commits.** The evidence branch starts at `origin/main`, and the
  source range inherits older commits (for example `a9bfbd6…` and the main tip `973b7ffd…`) whose
  author and committer fields carry a personal e-mail address. That is history this runbook cannot
  and must not rewrite. The publication scan and the identity check therefore cover only (a) the
  flat evidence files that are published and (b) the author and committer of the evidence commit
  itself. They do not scan, and do not claim anything about, the inherited parent commits. Do not
  paste those parents' author lines into the report.
- **Privacy: commit identity.** The first Priority 10 evidence commit carried a personal name and
  e-mail address from the host's git config. Step 5 now sets a repository-local identity
  (`j-mckerracher` and the GitHub noreply address, `git config --local` only), refuses to commit if a
  `GIT_AUTHOR_*` or `GIT_COMMITTER_*` variable is set, verifies author and committer after the commit
  and before the push, and the publication scan now also fails on any e-mail address in the evidence
  files.
- **Notes for the run.** If the daemon unit test above fails again, its message now prints
  `files.len()` and `files_truncated`, and the commit helper names stray paths. If an IPC
  `coordinator_review_tests` or `event_stream_tests` test fails, its message now prints the call
  result, the method and the handler's call list (dispatch diagnostics). Report those values
  exactly. Git for Windows sets `core.autocrlf=true` in the system config (the first run printed
  `file:C:/Program Files/Git/etc/gitconfig` and `true`). The `git-autocrlf` record is expected to
  show `true` and the runbook does not change it, because the fixture must hold under that setting.
- **Clippy expectation.** Only the two baseline `single_match` diagnostics (`supervisor.rs:401` and
  `:424`) are expected. The first Priority 10 run also showed `method blocked_reason is never used` at
  `executor.rs:130`, which `abd3ab25…` removed (the first rerun showed only the two baseline
  diagnostics). If it or any other diagnostic appears, it is
  recorded as NEW, reported, and does not change the verdict.
- **Derived counts.** IPC 31 -> 38, daemon unit 20 -> 42, `windows_transport` stays 13. The daemon
  integration binaries are now two (`tests/daemon.rs` and `tests/coordinator_review.rs`), both
  `cfg(unix)`. The first Priority 10 Windows run measured IPC `38,0`, `windows_transport` `13`, and
  Vitest 14 files and 268 tests, and daemon `running 42`, all on `67c1181d…`. The first rerun measured
  daemon `42,0,0` on `abd3ab25…` (the daemon crate is unchanged since), `13`, and Vitest 14 files and
  268 tests, but its IPC run failed, so the IPC doc-test stage was not observed there. Nothing is
  measured natively on `e555c2c9…`.
- **Editing method.** The runbook was copied from the first rerun runbook (docs commit `674e40b8…`),
  re-pinned with one `cp` and one `sed -i` pass of literal string replacements (SHAs, tree, short id,
  branch and prefix names), and then edited with the edit tool. No heredoc or generator wrote
  runbook text.

Known limits of this runbook:

- Nothing in it has been run on Windows for this candidate (`e555c2c9…`). The daemon cannot be
  cross-compiled from macOS, because `libsqlite3-sys` needs the target C headers. The daemon did build
  natively at `67c1181d…` and `abd3ab25…`, but this commit's two IPC test-file changes have only
  been checked on macOS (including a Windows-target compile of `orchestrator-ipc --tests`). A
  compile failure is a finding. Follow "If the build fails
  (compile-failure triage)", capture the full stderr, stop, and still publish the evidence. The PowerShell
  was parse-checked on macOS only, and a run may show a harness defect. That is a
  finding, not a reason to edit the file mid-run.
- The counts are derived by reading `cfg` and `#[test]` attributes at the pinned source. For this
  commit they are unmeasured on Windows. Earlier native runs on previous trees observed `38,0` (at
  `67c1181d…`), `13`, and `42,0,0` (at `abd3ab25…`), and the trailing daemon `0,0` stages have not been
  observed on this commit. A mismatch is `INCONCLUSIVE` and is a finding for the coordinator, not an edit of
  this file.
- The Vitest counts (14 files, 268 tests) were measured on macOS with Node v26.13.0, Vitest 5.1.0,
  and Vite 8.3.6. They are believed platform-neutral under jsdom. They are unmeasured on Windows.
- The daemon is built but never started. The desktop e2e suite uses Unix domain sockets and is NOT
  RUN. Second-user denial, a genuinely remote client, a write above 64 KiB to a slow reader, and the
  Tauri shell and native bridge are `UNPROVEN`. Mac results for any of these are not Windows evidence.
- The daemon `tests\daemon.rs` and `tests\coordinator_review.rs` binaries are `cfg(unix)` and report
  `running 0 tests` on Windows. Do not read that as a missing gate.
- The check helper ends only the tree it started. The survivor scan reports other processes by path
  and command line and never stops them.
- The clippy gate shares the target directory with the earlier gates. If cargo prints no `Checking`
  line, the gate is `INCONCLUSIVE`. That is a finding, not a cause to run it again in the same
  evidence directory.
- The Mac counts are coordinator-supplied. Independent Mac validation is separate and does not change
  the Windows expectation.
- The declared runbook commit and the source pin are separate. The byte pin makes the runbook bytes
  comparable after the run. It is not an execution proof.
- The runbook is read from the **tip of the docs branch**. The source under test is always `e555c2c9…`.
  The docs chain is `e555c2c9…` -> the tip (one docs commit). The pin is self-referencing: the file
  cannot contain its own commit id, so the operator declares the tip, checks that its only parent is
  `e555c2c9…`, and Step 1 re-checks that and that the whole difference from the source is this one
  file. It records the result in `runbook-identity.txt`.
