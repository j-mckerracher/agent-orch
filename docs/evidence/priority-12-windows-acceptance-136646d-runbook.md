# Priority 12 (M6-01 security hardening, M6-02 and M6-03 diagnostics and reliability, on top of M5-05 through M5-05r5) native Windows acceptance runbook (candidate 136646d95c86b050ca6f6617bb75bf10758826bb)

This runbook is the first native Windows run of Priority 12. Priority 12 is the Priority 11 round 5
tree (M5-05r5, `a5e1f43b…`) merged with three milestones: M6-01 (security and threat-model hardening:
refuse read-only autonomy because the agent would write, bound and NUL-check contract fields, a redacted
launch-failure explanation in the desktop host, an environment policy for spawned processes, and the R11
data-directory ACL and R12 large-objective limits documented in `docs/security/m6-01-threat-model-and-dependency-review.md`),
M6-02 (release CI: the native gate harness `scripts/ci/` and its expected counts) and M6-03 (structured diagnostics,
redaction and recovery observability: the `orchestrator-diagnostics` crate, a visible cause for internal errors,
and a failed task listing that no longer reads as nothing running). The candidate is `136646d9…` (tree
`99d432d1…`, only parent `e2853204…`), 39 commits and 136 files after the accepted Priority 10 round 2
baseline `e555c2c9…`. The published Priority 11 round 5 runbook
(`docs/evidence/priority-11r5-windows-acceptance-a5e1f43-runbook.md`, docs branch
`docs/priority-11r5-windows-acceptance-a5e1f43`, docs commit `867788c30…`) was published and never run
natively, so it is superseded by this one. This runbook keeps its structure and helpers and is re-pinned
to `136646d9…` with the Priority 12 counts, extra per-crate gates, a bridge-test timing capture and three
by-hand checks. The text below that still describes round 5 and earlier rounds is the history of the
ancestors of this candidate; where it names `a5e1f43b…` or an older count, the Priority 12 pins, the
`$Checks` table and the status rules in the PowerShell blocks are authoritative.

Priority 11 round 5 itself ("11r5") was the fifth native Windows run of Priority 11. Priority 11 is the
M5-05 desktop integration (the Tauri host `apps/desktop/src-tauri`, the raw-frame `Relay` in
`orchestrator-ipc`, the daemon-backed UI boot, the `tests/lifecycle.rs` daemon proof, and the sidecar
and installer scripts) plus its remediations: M5-05r (event subscriptions keep the daemon alive, a
fail-fast connect for a stale Windows token, longer relay timeouts, no fixture data in the native app),
M5-05r2 (long-path task worktrees, a self-explaining lifecycle test, a CI-safe packaging wrapper and LF
pins for generated files), M5-05r3 (process-level coordinator and review tests against the real
daemon that run on Unix and Windows, and Tauri host tests that drive the real daemon through the host
`Bridge`), M5-05r4 (a configurable IPC client I/O timeout, two fewer git processes per
`review.evidence` call, and per-call timing plus a daemon log tail printed by the tests when they
fail) and M5-05r5 (this candidate: a 120 s per-call bound and per-call timing in the Tauri bridge
tests, opt-in daemon request tracing, an idle-exit fix for a long handler call, and one git process
fewer per `Repository::new`). It sits on the accepted Priority 10 round 2 tree (`e555c2c9…`, native
PASS with limits). It is adapted from the published Priority 11 round 4 runbook
`docs/evidence/priority-11r4-windows-acceptance-1bb24bf-runbook.md` (docs commit
`60996cffd1055637b545ea2cd25284d9ffe5b5d7`, docs branch
`docs/priority-11r4-windows-acceptance-1bb24bf`, runbook sha256
`990643a4197e942f0b5bf73fa010d4b415dc22613f8003cc9ae82b682206a6ba`) and its native adjudication
(`.agent-worktrees/p11r4-native-qa-20261110T064057Z/QA-REPORT.md`, verdict FAIL on one gate), which
derive from the published Priority 11 round 3 runbook
`docs/evidence/priority-11r3-windows-acceptance-efa5e8a-runbook.md` (docs commit
`4cb6684334fc56f8bbb1f67299b6c5adc6df234f`, runbook sha256
`19955f78288bd42ef6be15b9ab55ebd4c12466cc41b9ba7f2e11337efa0d1d4c`) and the Priority 11 round 2 runbook
`docs/evidence/priority-11r2-windows-acceptance-6b9cf7c-runbook.md` (docs commit
`740cb367366652ba723bf8da428f96eeef748ee7`, runbook sha256
`36daccfe7800e1f538636648207aa8f3accf2a9335e7d32cb7cf3c825b18904e`), the Priority 11
first-run runbook, the accepted Priority 10r2 runbook and the Priority 10, 10r, 9r, 8 and 7 runbooks.
Those are immutable git objects and are absent from the source checkout. The helper blocks
(`Protect-Text`, `Invoke-Bounded`, the survivor scan, the publication scan, the verdict model) are
reused with the changes listed in "What changed from the approved harness". No new framework is
introduced. The in-repo native lifecycle runbook `docs/evidence/m5-05-native-lifecycle-runbook.md`
(part of the source under test) is incorporated below and is not contradicted.

Do not run an old runbook against this candidate. In particular, do not run the Priority 10r2 runbook,
the Priority 11 runbook (`…-3df3b58-runbook.md`), the Priority 11r2 runbook (`…-6b9cf7c-runbook.md`),
the Priority 11r3 runbook (`…-efa5e8a-runbook.md`) or the Priority 11r4 runbook (`…-1bb24bf-runbook.md`)
against it: their pins stop on any other source, the Priority 11 runbook sets `$env:CI = '1'`, which
makes `tauri build` fail, the r2 runbook does not run the `coordinator-review-tests` gate and expects 6
Tauri tests, the r3 runbook does not capture the per-call timing and `daemon log tail` diagnostics, and
the r4 runbook expects 45 IPC tests, has no `git-discovery-tests` gate and does not capture the
`[bridge-test]` timing lines or the daemon request trace.

## Prior native evidence and what it does not prove

All earlier native runs are history for other trees. None is evidence for this commit.

| Run | Source | Result | Evidence commit | What it showed |
|---|---|---|---|---|
| Priority 9 first run | before `7435ea32…` | FAIL | n/a | Named-pipe `PIPE_NOWAIT` made `ReadFile` return error 232 (`ERROR_NO_DATA`) when the pipe was empty, which looked like end of file. Fixed by the `ReadFile`-based read in `7435ea32…`. |
| Priority 9 remediation (9r) | `7435ea32bcb541044e61ef49e863d394f0d49e12` | PASS with limits | `9a9f3bf688bd41a5b5faa797a6d8e243e6d4fef5` | IPC 31, daemon 20, `windows_transport` 13; two baseline clippy `single_match` diagnostics. |
| Priority 10 first run | `67c1181db7cd44890cd229eee648033525f64c3e` | FAIL | `42f63e4b581c96db329e849ea88270df74f1cb13` | A daemon test fixture committed 1001 paths instead of 1000, because system `core.autocrlf=true` converted a checked-out file to CRLF. |
| Priority 10 first rerun (10r) | `abd3ab25ee96a6ca1c0face830a83af25835b253` | FAIL | `8c3b623c7355903947574cff62c6d0960e6db4c0` (runbook docs `674e40b8ce78a7e1322433918e227ce8aef78182`) | A 50 ms stream timeout in an IPC test dropped an idle connection on Windows (named-pipe create, connect and handshake take longer than 50 ms). |
| Priority 10 second rerun (10r2) | `e555c2c922ce00ee93673452f67848c7f888b4f1` | PASS with limits | `d87d56b555fb92ad2bb623898bddc30218f1f968` (runbook docs `08531389c488fe29edd2107990921be16e31c1d5`, runbook sha256 `8f5e101478eabdf7e5030215350e4a096cebcf28bf6bca4a7b49af8be9cc9d1c`, run id `20261109T234640Z-a20a123edce542ad8b94a65ecac24400`) | Measured on Windows 11 Education 10.0.26200, git 2.46.2.windows.1, rustc 1.96.0 (msvc), node 22.20.0, npm 11.3.0, system `core.autocrlf=true`: IPC `38,0`, `windows_transport` `13`, daemon `42,0,0`, Vitest 14 files and 268 tests, clippy with the two baseline diagnostics and 0 NEW. |
| Priority 11 first run (11) | `3df3b5831ffab764aeeddf14de2caef17b00cf4a` | FAIL | `00b4fef7c67d12fd27e5c8edd8c54cec8e8147a4` (runbook docs `2a3b120b3055db129dbcf1122c53c9baf4309b95`, run id `20261110T013944Z-af8e1243a24441778525a7e8a32266c7`) | Same host class as 10r2. Everything compiled natively (daemon, Tauri host, release sidecar). IPC `45,0`, `windows_transport` 13 (32 filtered out), daemon unit `42,0,0`, `tauri-tests` `6,0,0`, Vitest 15 files and 287 tests all matched the derived counts. Four findings: (1) `daemon-lifecycle` (and `daemon-tests`, which runs it) panicked at `lifecycle.rs:87` after 30 s waiting for the agent pid, after STEP 2, with the cause hidden because the daemon output went to `Stdio::null()`; (2) `desktop-package-win` aborted with `invalid value '1' for '--ci'` because the harness set `$env:CI = '1'`; (3) `source-status-after` listed the three `permissions/autogenerated/daemon_*.toml` files as modified (3 LF-to-CRLF warnings) and the evidence had no `git diff`; (4) `tauri-clippy` was `INCONCLUSIVE`: no `Checking agent-orch-desktop` line. |
| Priority 11 second run (11r2) | `6b9cf7cdee06dd439492b15d8d9f6c7138059b2b` | PASS with limits | `e3d3ccc39ac848bc2b70f546090843611b1bacd2` (runbook docs `740cb367366652ba723bf8da428f96eeef748ee7`, runbook sha256 `36daccfe7800e1f538636648207aa8f3accf2a9335e7d32cb7cf3c825b18904e`, run id `20261110T024313Z-b570be2e0f964794aba04c9bc9120ea4`) | Same host class (Windows 11 Education 10.0.26200, git 2.46.2.windows.1). All 15 gates passed with the exact derived counts: IPC `45,0`, `windows_transport` 13 (32 filtered out), daemon unit `42,0,0` (the `coordinator_review` binary ran **0** tests, it was Unix-only), `lifecycle: PASS` (STEP 1 to STEP 7) in `daemon-lifecycle` and inside `daemon-tests`, `tauri-tests` `6,0,0`, Vitest 15 files and 287 tests, `tauri-clippy` clean for both crate names, and `package:win` produced the NSIS installer. The adjudication was PASS with limits: all 11 UNPROVEN rows stayed UNPROVEN, and the M5-02c release gate (the real daemon `coordinator.*` and `review.*` approval flow from the Windows desktop) stayed **UNPROVEN**. |
| Priority 11 third run (11r3) | `efa5e8af3469082226c61cccfea167cc607d6a0f` | FAIL | `5577fb99004a83885dafd14d0e39bc8fe3c63213` (runbook docs `4cb6684334fc56f8bbb1f67299b6c5adc6df234f`, runbook sha256 `19955f78288bd42ef6be15b9ab55ebd4c12466cc41b9ba7f2e11337efa0d1d4c`, runbook blob `7dacecd0…`, evidence branch `evidence/priority-11r3-windows-acceptance-efa5e8a-20261110T045110Z-db4b57304c7345aba3df530574fa394a`) | Same host class. The evidence identity was correct (the noreply identity). 14 gates PASS, **2 FAIL**, 12 UNPROVEN rows. `coordinator-review-tests` passed 2 of 4 tests and `daemon-tests` printed the `running` sequence `42,4` and exited 101 on the `coordinator_review` failures, so the `lifecycle` test never ran inside it. The failures were `Frame(Io(TimedOut))` on varying calls: the IPC `Client` had a hard-coded 2 s read and write timeout, and the daemon runs many Git processes per `review.evidence` call (about 91 per test flow), which are slow on Windows with real-time antivirus scanning. The daemon logs were empty, so the failure carried no daemon-side cause. `tauri-tests` passed 9 of 9 but each test took over 60 s (137 s in total) for the same reason. Everything else matched the derived counts. |
| Priority 11 fourth run (11r4) | `1bb24bf98716a22e8955b50cd28643d063674240` | FAIL | `f3f367404659916a4c6b8258c2145f8dab716f10` (runbook docs `60996cffd1055637b545ea2cd25284d9ffe5b5d7`, runbook sha256 `990643a4197e942f0b5bf73fa010d4b415dc22613f8003cc9ae82b682206a6ba`, runbook blob `ecfda5a0…`, evidence branch `evidence/priority-11r4-windows-acceptance-1bb24bf-20261110T060713Z-8b6c4f68f0734a7599dba117d219ff2a`) | Windows 11 Education 10.0.26200. 15 gates PASS, **1 FAIL**, 12 UNPROVEN rows. Both r3 failures were fixed: `coordinator-review-tests` 4 of 4 and `daemon-tests` `42,4,0` with `lifecycle: PASS`. IPC `45,0`, `windows_transport` 13, Vitest 287 in 15 files, clippy (2 baseline diagnostics, 0 NEW), the installer all passed. **`tauri-tests` failed** (exit 101, 8 passed, 1 failed, 367.7 s for the gate): `bridge::real_tests::the_host_launches_the_real_daemon_and_drives_an_approval_to_delivery` panicked at `bridge_real_tests.rs:240` with `frame within 30 s: Timeout` on the success-path `delivery.request`. The daemon log was empty and the test printed no per-call timing, so the daemon-side cause was not observed. The 30 s bound was the only limit that failed; the same `delivery.request` passed in `coordinator-review-tests` in 17 s. |

**Failure history that this run must not repeat blindly.** Every earlier native failure was a
Windows-only timing, line-ending, path-length or pipe-semantics effect that the Mac could not show.
The Priority 11 first run added four more (above); the second run (r2) passed all 15 gates with
limits; the third run (r3) failed 2 gates on a Windows-only timing effect (a 2 s client timeout against
slow Git spawns); the fourth run (r4) fixed both and failed 1 gate (`tauri-tests`, a 30 s bound on a
slow `delivery.request` through the host bridge, cause unobserved). This candidate (M5-05r5) changes the
bridge-test bound and diagnostics, adds opt-in daemon request tracing, fixes the daemon idle exit for a
long handler call and removes one Git process from each `Repository::new`, and **none of that has ever
run on Windows**: the Mac derivation is a Mac run plus a Windows `cargo check` (msvc and gnu) of the
`orchestrator-ipc` and `orchestrator-git` crates only. The daemon and `src-tauri` changes were **not**
compiled for Windows on the Mac. **Every claim below about the new behaviour on Windows is derived,
none is proven on Windows.** A native failure of a test is a finding, not a harness defect. The r3 and
r4 test code has run on Windows (r3, r4) and its pass or fail behaviour is recorded in the rows above.

**What M5-05r5 changed (source `a5e1f43b…`, one commit above `1bb24bf9…`).** It changes production code
in two crates (`orchestrator-ipc` and `orchestrator-git`) and one test file, from the Mac investigation
of the r4 `tauri-tests` failure. **The Windows cause of that failure is inferred, not observed**: a slow
`delivery.request` against a hard 30 s bound. On the Mac a daemon-only 3 s-per-git shim reproduced the
same panic text with the r4 test (`frame within 30 s: Timeout`) and passed with the r5 test (the
success-path `delivery.request` took 46 s). This run is the confirmation. If `tauri-tests` fails again,
the evidence must show which call was slow and what the daemon was doing.

- `apps/desktop/src-tauri/src/bridge_real_tests.rs`: the per-call deadline `WAIT` is **120 s** (it was
  30 s; the environment variable `AGENT_ORCH_TEST_TIMEOUT_SCALE` multiplies it, and this runbook does not
  set it). Every call prints `[bridge-test] <window> <method> answered in <elapsed>` to stderr (libtest
  keeps it in the failing test's captured output). A panic names the method, request id and limit
  (`<method> (id N, limit 120s): no frame before the deadline`). On a panic the fixture prints the last 80
  lines of the daemon log, with the token redacted, between `--- daemon log tail (<K> of <N> lines,
  <path>) ---` and `--- end daemon log ---`. The test sets `AGENT_ORCH_TRACE_REQUESTS=1` before any daemon
  starts, so each daemon it launches writes a request trace to its `daemon.log`. Assertions are
  unchanged. The shutdown client still uses the 15 s `STOP`.
- `crates/orchestrator-ipc/src/session.rs`: `AGENT_ORCH_TRACE_REQUESTS` (off by default, any value turns
  it on) writes `[<epoch ms>] request start id=<id> method=<m>` and `[<epoch ms>] request end id=<id>
  method=<m> elapsed_ms=<n> status=ok|error <code>` lines to the daemon log. It never writes parameters
  or results. An in-flight counter counts a running handler call as activity.
- `crates/orchestrator-ipc/src/server.rs`: the idle exit also requires no in-flight call. The old idle
  rule used only the last request time and live subscriptions, so a handler call longer than the 60 s
  idle timeout with no subscription could let the daemon exit under the call. The idle clock restarts
  when the call ends.
- `crates/orchestrator-ipc/src/idle_subscription_tests.rs`: new test
  `a_call_longer_than_the_idle_timeout_does_not_stop_the_server`, so the IPC lib has **46** tests (it had
  45).
- `crates/orchestrator-git/src/repository.rs`: `Repository::new` resolves the top-level and the common
  directory in one `rev-parse` spawn and falls back to the old two-spawn flow if the output is not
  trustworthy (for example a relative `--git-common-dir` or a path-form mismatch on Windows). The
  success-path `delivery.request` ran 19 git processes before; the Mac saved about 15 of them (derived).
- `crates/orchestrator-git/tests/repository_discovery.rs` (new, **2** tests):
  `discovery_agrees_from_root_subdirectory_and_linked_worktree` and
  `a_directory_outside_any_repository_is_refused`. They pass on the old and the new code on the Mac.
  They run natively in the new **`git-discovery-tests`** gate (below). `orchestrator-git` had no gate
  before.

Production defaults are unchanged: the relay's 10 s and 25 ms timeouts, the server's 2 s stream
timeout and the `Client` default I/O timeout. One known risk is **not** fixed and is production
behaviour: a connection that is not subscribed and sends nothing for the 2 s stream timeout is closed by
the daemon (`session.rs`). A 2 s gap between two bridge-test calls on a very loaded host could therefore
show as a `closed` frame. If `tauri-tests` fails with `closed` instead of a timeout, say so.

Expected counts: IPC **`46,0`** (was 45), `windows_transport` 13 (unchanged; the 32 filtered out
become 33), daemon `42,4,0` (46 passed), `coordinator-review-tests` 4, `git-discovery-tests` **2**
(new), `tauri-tests` `9,0,0`, Vitest 15 files and 287 tests, `lifecycle: PASS`. The Mac measured `cargo
test --locked --workspace` 641 passed, 0 failed, 1 ignored (638 at r4: +2 git, +1 ipc), the `src-tauri`
suite 10 passed (three bridge tests in 8 s), Vitest 287 in 15 files, e2e 19 passed (Unix socket only),
and no flake.

**What M5-05r4 changed (source `1bb24bf9…`, one commit above `efa5e8af…`; proven natively at r4 except as
the r4 row says).** It changes production
code in two crates, so this is **not** a test-only change (the r3 one was).

- `crates/orchestrator-ipc/src/client.rs`: new `pub const DEFAULT_IO_TIMEOUT` (2 s, the old fixed
  value) and new `Client::connect_with_timeout(data_dir, requested, io_timeout)`.
  `connect_with_capabilities` calls it with the default, so every existing caller keeps the 2 s
  timeout. `reconnect()` keeps the chosen value. `lib.rs` re-exports the constant, and
  `unsupported.rs` (the non-Unix, non-Windows stub) gains the same entry point.
- `crates/orchestrator-git/src/repository.rs`: new `Repository::snapshot_between`, which validates
  the commit pair once and returns the changed files and the diff. `crates/orchestrator-daemon/src/api_review.rs`
  uses it, so `review.evidence` spawns **9** Git processes instead of 11 (a whole review flow, 85
  instead of 91). Results and errors are unchanged.
- `crates/orchestrator-daemon/tests/support/{mod.rs, process.rs}`: every test call goes through a
  `timed!` macro that records the elapsed time and ok or ERR per call, and the test client uses a
  **30 s** I/O timeout (`CALL_TIMEOUT`). When a test panics, `Daemon::drop` prints
  `--- per-call timing (client I/O timeout 30s) ---` ... `--- end per-call timing ---` (lines
  `<ms> ms  ok|ERR  <label>`) and `--- daemon log tail (<path>) ---` ... `--- end daemon log ---`
  (the last 60 lines of `daemon.log`) to stderr. **The r3 marker was `--- daemon log (<path>) ---`;
  the r4 marker has the word `tail`.** The harness below captures both forms and the timing block.
- `crates/orchestrator-daemon/tests/coordinator_review.rs`: the frame wait is 30 s (it was 10 s).
  `tests/lifecycle.rs` uses `connect_with_timeout` with 30 s.
- `apps/desktop/src-tauri/src/bridge_real_tests.rs`: waits are 30 s and the shutdown client uses a
  15 s `STOP` (r5 raises the wait to 120 s, see above).

No test was added or removed in r4. The r4 expected counts were therefore unchanged from r3: IPC `45,0`,
`windows_transport` 13, daemon `42,4,0` (46 passed), `coordinator_review` 4, `tauri-tests` `9,0,0`,
Vitest 15 files and 287 tests, `lifecycle: PASS`. (r5 changes the IPC count to `46,0`.)

**What M5-05r3 changed (source `efa5e8af…`, one commit above `6b9cf7cd…`; it is an ancestor of this candidate).** It is a test-only
change. No daemon, IPC, store, git or UI source changed, and no `Cargo.toml` or `Cargo.lock` changed.

- `crates/orchestrator-daemon/tests/coordinator_review.rs` (no `#![cfg(unix)]` any more, so it runs on
  Windows) holds **4** process-level tests against the real `orchestrator-daemon` binary over the
  platform transport (a Windows named pipe here). Tasks are seeded into the store of a stopped
  daemon, and the review flow itself runs only through `review.*`, `approvals.*` and `delivery.*`.
  Support code is `tests/support/{mod.rs, process.rs, seed.rs}` (a module directory, not a test
  binary), and `examples/seed_review.rs` is a thin wrapper over `support/seed.rs`.
- `apps/desktop/src-tauri/src/bridge_real_tests.rs` (module `bridge::real_tests`, **3** tests) drives
  the real daemon through the host `Bridge`. The host starts the daemon itself through `launch::spawn`
  (the detached launch path of the installed app, `DETACHED_PROCESS` on Windows) over a seeded data
  directory. No in-process fake and no fixture. The daemon and the `seed_review` example are built
  **once per test process** by a nested `cargo build --locked` into `<target>\real-daemon`, unless
  `AGENT_ORCH_DAEMON_BIN` and `AGENT_ORCH_SEED_BIN` name prebuilt binaries. **The first Tauri test
  run therefore spends several minutes on that nested daemon build** (the `tauri-tests` gate has a
  generous but bounded timeout for it). A failed nested build fails the test with the build output;
  nothing is skipped and nothing falls back.
- `bridge.rs` gains only the `#[cfg(test)] #[path = "bridge_real_tests.rs"] mod real_tests;`
  declaration.
- `docs/evidence/m5-05-native-lifecycle-runbook.md` is updated (the manual checklist, sections 5.5
  and 5.6, is optional here, see "Optional manual GUI checklist").

Test behaviour that matters on Windows: waits are 30 s (frames, since r4; 10 s in r3), 30 s (daemon
ready and requests, because the first use of a named pipe on a loaded host can be slow) and 15 s (stop).
The daemon process tests' `Client` uses a 30 s I/O timeout (`CALL_TIMEOUT`, r4); the product default
`Client::connect` is still the 2 s `DEFAULT_IO_TIMEOUT`. Every test root is `<TEMP>\ao*` with one-letter
children (`d`, `r`, `w`) to stay under `MAX_PATH`. Test git calls pin `core.autocrlf=false`. The daemon's
stdout and stderr go to `daemon.log`. When a test fails it prints, to stderr, the per-call timing
between `--- per-call timing (client I/O timeout 30s) ---` and `--- end per-call timing ---`
(one `<ms> ms  ok|ERR  <label>` line per call) and the last 60 lines of the daemon log between
`--- daemon log tail (<path>) ---` and `--- end daemon log ---` (the Tauri `bridge::real_tests` fixture
still prints `--- daemon log (<path>) ---` ... `--- end daemon log ---`). The harness saves all of these
on a failed gate. The token-tamper step of
`daemon_gates_the_coordinator_and_review_methods_on_token_and_capability` rewrites `daemon.token`
in place (same length) and relies on the owner's DACL allowing a write; if Windows denies it, only
that step fails and the failure is a finding.

**What M5-05r2 changed (carried from the second run, now at `6b9cf7cd…`, three commits above `3df3b583…`).**

- `aff805b8` `fix(daemon)`: every git call now passes `-c core.longpaths=true` (`orchestrator-git`
  `git.rs` and `worktree.rs`, and the daemon `executor.rs` git helper). A new cross-platform test
  `worktree_under_a_path_longer_than_max_path_is_created` makes the `orchestrator-git` unit suite one
  test larger. `tests/lifecycle.rs` now sends the daemon's stdout and stderr to `base\daemon.log`,
  fails fast if the daemon exits or the task becomes `Failed` or `Cancelled`, writes the agent pid
  atomically, and panics on any timeout or failure with a diagnostics report (base path and length,
  the daemon log tail, the agent breadcrumbs `agent.launch` and `agent.err`, the longest path under the
  base, the worktree listing, and `tasks.get`, `sessions.list` or `tasks.list`). The `STEP 1` to
  `STEP 7` lines and `lifecycle: PASS` are unchanged.
- `8c8481b6` `build(desktop)`: `apps/desktop/scripts/package.mjs` normalizes `CI` (`1` or `true`
  becomes `true`, `0` or `false` becomes `false`, anything else is removed), stages the sidecar, then
  runs `tauri build`. `package:mac` and `package:win` use it. `.gitattributes` pins
  `permissions/autogenerated/*.toml`, both `Cargo.lock` files and `package-lock.json` to
  `text eol=lf`. `src-tauri/Cargo.toml` is committed in the form that `tauri build` rewrites it to.
- `6b9cf7cd` `docs`: the in-repo lifecycle runbook.

The r2 run proved all of these natively (see the prior evidence table). They are not re-derived here
and are re-checked by the same gates.

**What is new at this commit and has never run natively on Windows.** The r4 run compiled and ran the
r4 code on Windows (15 gates passed, `tauri-tests` failed, see the r4 row). What has **never** run
natively on Windows is the r5 change: the 120 s bridge-test deadline and its per-call timing and
daemon log tail, the `AGENT_ORCH_TRACE_REQUESTS` request trace, the in-flight idle guard, the
one-spawn `Repository::new` and its two discovery tests. The daemon and `src-tauri` changes were not
compiled for Windows on the Mac (no sqlite C cross compiler there), so a **compile failure in those two
crates is possible**. Follow "If a build fails (compile-failure triage)" for any compile failure. On
Windows a relative `--git-common-dir` or a path-form mismatch takes the two-spawn fallback in
`Repository::new`, so the cost there is at most the old spawn count.

## Status of this run

**Native Windows acceptance of this candidate is unproven. No native Windows run of `136646d9…`
exists, and this document does not claim one.** Nothing in this runbook has been executed on Windows
by its author. Mac results, earlier native runs on other commits, and the static derivations below are
not Windows evidence for this commit.

This run records how the candidate builds and tests on native Windows and whether the installer
bundle builds. It does not close any board item. The coordinator decides acceptance. The plan is
`docs/technical-implementation-plan.md`.

**On any gate failure the run does not stop.** The harness runs every remaining gate, saves the
diagnostics of each failed gate (the `daemon log tail`, the per-call timing block, the `[bridge-test]`
timing lines, the daemon request trace and the lifecycle report) into the evidence, and then publishes
the evidence. A failed gate never ends the run early, and a failed gate never skips the evidence
publication. The only things that stop a run early are a still-running owned process (a safety stop) and
a pre-flight check in Step 1 or 2.

Items under test, in the board's words:

- **Priority 12 (this candidate): M6-01, M6-02, M6-03 on top of M5-05r5.** The questions for Windows are:
  (1) do all the new and changed Rust crates build and pass natively (daemon 55, git 47 with `env_policy` 11,
  core `contract` 17, runtime `env_isolation` 1, store 32, diagnostics 28, cli 2/4/22/6, src-tauri 12);
  (2) does the 8-step lifecycle test print `lifecycle: PASS`; (3) do the three `bridge::real_tests` pass, and how
  long does each call take (`tauri-bridge-timing` keeps the `[bridge-test]` lines; the r4 run failed on a 30 s
  `delivery.request` timeout and r5 raised the bound to 120 s, so r5's bound has still never run natively);
  (4) do the Node self-tests (36) and the harness gate list (16) behave on Windows; and (5) the three by-hand
  checks in "By-hand checks (Priority 12)": the data-directory ACL (R11), the redacted launch failure, and a
  ~64 KiB objective (R12). Nothing in the three by-hand checks is run by the script.

- **M5-05r5 (an ancestor of this candidate).** After r4, `tauri-tests` failed natively on the success-path
  `delivery.request` (`frame within 30 s: Timeout`) and the daemon log was empty. r5 raises the bridge
  test bound to 120 s, makes the test print per-call timing and a daemon log tail, adds an opt-in
  request trace to the daemon, fixes the idle exit for a long call and removes one Git spawn from each
  `Repository::new`. The expected result is that `tauri-tests` passes natively with `12,0,0` (9 at r5, plus the 3 redaction tests). If it still
  fails, the evidence must name the slow or lost call: the `[bridge-test]` lines give the per-call
  elapsed time, the daemon log tail holds the `request start` and `request end` trace lines (a
  `request start` with no matching `request end` is a slow or hung handler; a `request end` with no
  matching `[bridge-test] answered` line is a lost response), and the first failing call is named in
  the panic message. The first `tauri-tests` run builds the daemon inside the gate and takes minutes;
  the 3600 s gate limit is unchanged.

- **M5-05r4.** After r3, two gates failed natively on `Frame(Io(TimedOut))` under a
  2 s client timeout. r4 gave the test clients 30 s and removed two Git spawns from each
  `review.evidence`. The r4 run passed `coordinator-review-tests` and `daemon-tests`.

- **M5-05r3.** The M5-02c release gate for M5-05 is the real daemon
  `coordinator.*` and `review.*` approval flow from the Windows desktop. The r2 adjudication left it
  UNPROVEN, and the r3 run did not prove it (r3 FAIL). The two gates (`coordinator-review-tests`,
  `tauri-tests`) can prove the **host-level
  half** natively: the real daemon, over the Windows named pipe, driven first by a client and then by
  the Tauri host `Bridge`, approves and delivers a task, and refuses a missing token, a missing
  capability and a wrong token. They are **not** a proof of the installed app, the webview, the
  `daemon_open/send/close` Tauri commands as the webview invokes them, or the React review click
  path. Those stay UNPROVEN (see the UNPROVEN table). If the release rule requires the desktop UI
  itself to drive the approval flow, these tests alone do not satisfy it.

- **M5-05.** Connect the UI to the daemon through the Tauri host. The host forwards authenticated
  JSON-RPC frames over the daemon's local transport (a named pipe on Windows), starts the daemon only
  when none answers and never stops it, and refuses `shutdown` and `handshake` from the webview. The
  webview never sees the token. A daemon-owned task survives a UI close and reopen.
- **M5-05r.** Live event subscriptions keep an otherwise idle daemon running. A stale token with a
  missing pipe now fails in about 0.3 s per attempt instead of about 5 s. Relay handshake and write
  timeouts are 10 s. The native app never shows fixture data and shows D3 recovery status as
  "Unavailable" for real tasks. `daemon.log` rotates at launch.

## Candidate

- Repository: `https://github.com/j-mckerracher/agent-orch.git` (`origin`, the only remote and the
  only push target)
- Source branch (the coordinator publishes it): `orch/priority-12-integration-136646d`
- Source commit: `136646d95c86b050ca6f6617bb75bf10758826bb`
- Source tree: `99d432d1b2ceb48fe99ada088a5fd8864a4038dd`
- Source parent: `e28532048…` (the gated commit above the merge `1ae44f475…` of the previous Priority 12 integration `aed1b24dd…` and M5-05r5 `a5e1f43b9…`)
- Accepted baseline (the Priority 10r2 tree, native PASS with limits): `e555c2c922ce00ee93673452f67848c7f888b4f1`
- Last native PASS before this candidate (Priority 11r2, an ancestor of the source): `6b9cf7cdee06dd439492b15d8d9f6c7138059b2b`
- Failed earlier candidates (ancestors of the source): `3df3b5831ffab764aeeddf14de2caef17b00cf4a` (Priority 11 first run), `efa5e8af3469082226c61cccfea167cc607d6a0f` (Priority 11r3) and `1bb24bf98716a22e8955b50cd28643d063674240` (Priority 11r4). The Priority 11r5 candidate `a5e1f43b9516b172247197c6c126acbc6aaf8d54` was published and never run natively.
- Runbook branch (the coordinator publishes it): `docs/priority-12-windows-acceptance-136646d`
- Evidence branch (you create it): `evidence/priority-12-windows-acceptance-136646d-<RunId>`
- Runbook: file `docs/evidence/priority-12-windows-acceptance-136646d-runbook.md`. The coordinator
  gives the branch, the full runbook commit, and the URL in the dispatch message. The runbook is read
  from the **tip of the docs branch**. The source under test stays `136646d9…`. The docs branch is
  exactly one docs-only commit directly on the source: the runbook commit has the source commit
  `136646d9…` as its **only parent**, and the docs branch tip is the runbook commit. A runbook text
  cannot contain its own commit id, so the pin is self-referencing: the operator **declares** the
  runbook commit as the docs branch tip (the coordinator gives it in the dispatch message and the
  operator reads it from `git rev-parse origin/docs/priority-12-windows-acceptance-136646d`). Step 1
  then verifies, without trusting the declaration, that the declared commit is a commit, that its
  **only parent is exactly `136646d9…`**, and that the whole difference from the source is this one
  file. Step 1 also records the declared commit, its parent, the blob id, the byte length, and the
  SHA-256 of the exact runbook bytes. That pins the bytes in evidence. It still does not prove that
  the commands you pasted match them. Nothing in the docs chain is part of the source under test.
- If the source branch tip is later than the source commit, test the pinned commit and report the tip.

Test only the exact source commit. Before any other command, `HEAD` must equal the full source
commit. If the branch does not contain the commit, stop and report the mismatch.

### Ref names

Every ref has one namespace, and none is reused for another purpose. The default `$SourceBranch` in
step 1 is the published name below. Use another only if the dispatch message names it.

| Ref | Pattern | Who creates it |
|---|---|---|
| Source | `orch/priority-12-integration-136646d` (the block accepts `orch/priority-12-integration-[a-z0-9-]+`) | coordinator |
| Runbook | `docs/priority-12-windows-acceptance-136646d` | coordinator |
| Evidence | `evidence/priority-12-windows-acceptance-136646d-<RunId>` where `<RunId>` is `yyyyMMddTHHmmssZ-<32 hex>` (UTC time plus a GUID, so it is unique) | you |

A local capture of refs must be named by its full namespace, never only its last segment: `orch/<name>`
and `docs/<name>` would otherwise collide. You push only the evidence ref.

### What the candidate changes

At Priority 12 the source is 39 commits (4 merges) and 136 files above `e555c2c9…` (+11226, −344 lines).
Above the Priority 11r5 tree (`a5e1f43b…`) it adds M6-01 (security: `submit` refusals and field limits,
the environment policy in `orchestrator-git` and `orchestrator-runtime`, the redacted launch-failure
explanation in `apps/desktop/src-tauri/src/launch.rs`, the threat model and dependency review), M6-02
(`scripts/ci/` harness, `docs/release-ci.md`, `.github` workflow) and M6-03 (the `orchestrator-diagnostics`
crate, daemon diagnostics sink, `crates/orchestrator-daemon/tests/diagnostics_log.rs`, a failed task listing
that is logged and never reads as nothing running). The `$ExpectedRangeFiles` gate checks the exact set of
136 changed paths. The description that follows is the Priority 11 round 5 history of the first 61 files and
16 commits (the part that is the M5-05 series); the Priority 12 additions are the list above.

The M5-05 part of the source is 16 linear commits above `e555c2c9…` (61 files, 5215 insertions, 237 deletions; no merge
commit). Oldest first: `64d2c9f7` the IPC `Relay`; `cbbb9e8d` the Tauri host bridge; `598848a0` the UI
connection to the daemon through the bridge; `1e78ee00` the daemon `tests/lifecycle.rs`; `e8c75bbc` the
sidecar staging and package scripts; `c8b8451d` the in-repo lifecycle runbook (the M5-05 owner tip);
then the M5-05r remediation: `8991e324` live subscriptions keep the daemon from idling out,
`b5a31428` the fail-fast stale-token connect and relaxed relay timeouts, `1a895121` no fixture mode or
fixture recovery facts in the native app, and `3df3b583` the runbook update (the Priority 11 first-run
candidate); then the M5-05r2 remediation: `aff805b8` long-path worktrees and the self-explaining
lifecycle test, `8c8481b6` the CI-safe packaging wrapper and the LF pins, and `6b9cf7cd` the lifecycle
runbook update (the Priority 11r2 candidate); then the M5-05r3 remediation: `efa5e8af` the real-daemon
coordinator and review tests and the Tauri host tests (the Priority 11r3 candidate, native FAIL); then
the M5-05r4 remediation: `1bb24bf9` the configurable client I/O timeout, `snapshot_between` and the
failure diagnostics (the Priority 11r4 candidate, native FAIL); then the M5-05r5 remediation: `a5e1f43b`
the bridge delivery test bound, request tracing and fewer git spawns (the source). The
`$ExpectedRangeFiles` gate checks the exact set of 61 changed paths. The part above `1bb24bf9…` is 6
files (290 insertions, 9 deletions); the part above `efa5e8af…` is 14 files (508 insertions, 92
deletions); the part above `6b9cf7cd…` is 18 files (1889 insertions, 176 deletions); the part above
`3df3b583…` is 25 files (2225 insertions, 198 deletions).

| Area | Files | Compiled or run on Windows by this runbook? |
|---|---|---|
| `crates/orchestrator-ipc` | 12: new `relay.rs` and `relay_tests.rs` (5 tests, 1 `cfg(unix)`), new `idle_subscription_tests.rs` (4 tests: 3, plus 1 in r5), `windows_transport.rs` (`connect_fail_fast`, missing-pipe grace, `connect_within` now `cfg(test)`), `client.rs`, `protocol.rs`, `server.rs`, `session.rs`, `transport.rs`, `unix_transport.rs`, `lib.rs`, and (r4) `unsupported.rs`. **Unchanged by r2 and r3.** r4 changes `client.rs` (`DEFAULT_IO_TIMEOUT`, `connect_with_timeout`), `lib.rs` (re-export) and `unsupported.rs` (the stub entry point), no test. r5 changes `session.rs` (request trace, in-flight guard), `server.rs` (idle exit) and `idle_subscription_tests.rs` (one new test). | **Yes**: `ipc-tests`, `windows-transport-tests`, `clippy-scoped`. |
| `crates/orchestrator-daemon` | 9: `Cargo.toml` (registers the harness-free `lifecycle` test), `tests/lifecycle.rs` (rewritten by r2; r4: `connect_with_timeout` 30 s), `src/executor.rs` (r2: a one-line `core.longpaths` pass-through in the git helper), (r3) `tests/coordinator_review.rs` (4 cross-platform process tests; r4: 30 s wait), `tests/support/{mod.rs, process.rs, seed.rs}` (r4: `timed!`, 30 s `CALL_TIMEOUT`, per-call timing and `daemon log tail` on panic) and `examples/seed_review.rs` (a wrapper over `support/seed.rs`), and (r4) `src/api_review.rs` (`review.evidence` uses `snapshot_between`). | **Yes**: `daemon-build`, `daemon-tests`, `daemon-lifecycle`, **`coordinator-review-tests`**, `clippy-scoped`. |
| `crates/orchestrator-git` | 4: (r2) `src/git.rs` (`-c core.longpaths=true` on every git call) and `src/worktree.rs` (`add_worktree` passes it; new test `worktree_under_a_path_longer_than_max_path_is_created`), (r4) `src/repository.rs` (new `snapshot_between`; `changed_files_between` and `diff_between` keep their behaviour), and (r5) `src/repository.rs` (`Repository::new` resolves the top-level and the common directory in one `rev-parse` spawn, with a two-spawn fallback) and new `tests/repository_discovery.rs` (2 tests). | **Yes, one binary**: `git-discovery-tests` runs only `tests\repository_discovery.rs`. The rest of the crate's tests do not run here. The crate is also exercised indirectly by `daemon-lifecycle` and `coordinator-review-tests` through the real daemon. |
| `apps/desktop/src-tauri` | 14: new Tauri host crate (`bridge.rs`, `bridge_tests.rs`, `launch.rs`, `lib.rs`, `build.rs`, `Cargo.toml`, `Cargo.lock`, `tauri.conf.json`, `tauri.bundle.conf.json`, `capabilities/default.json`, 3 autogenerated permission files) and, new in r3, `src/bridge_real_tests.rs` (3 tests, declared by a 3-line `#[cfg(test)]` module in `bridge.rs`; r4: 30 s waits, 15 s `STOP`; r5: 120 s per-call deadline, `[bridge-test]` per-call timing, daemon log tail, request trace on). `Cargo.toml` is rewritten once more by r2 (the form that `tauri build` produces). | **Yes**: `tauri-build`, `tauri-tests`, `tauri-clippy`, `desktop-package-win`. The host's runtime behaviour against the real daemon is exercised by `tauri-tests`; the webview and installed app stay **UNPROVEN**. |
| `apps/desktop` (UI) | 20: `src/native/{boot.ts, tauriConnector.ts, native.test.tsx}`, `src/daemon/{connection.ts, errors.ts}`, `src/sources/{daemon.ts, daemon.test.ts}`, `src/ui/*`, `src/main.tsx`, `src/review/ReviewRoute.tsx`, `package.json` (r2: `package:mac` and `package:win` call `scripts/package.mjs`), `.gitignore`, `scripts/prepare-daemon-sidecar.mjs`, `scripts/package.mjs` (new in r2). **Unchanged by r3 and r4.** | **Yes**: `npm ci`, `tsc -b`, `npm run build`, `vitest run`, `desktop-package-win`. The `e2e/` suite is **NOT RUN**. |
| repository root | 1 (r2): new `.gitattributes` (LF pins for the autogenerated permission files, both `Cargo.lock` files and `package-lock.json`) | Checked by the `gitattributes-check` step and the clean-tree gate. |
| `docs/evidence` | 1: `m5-05-native-lifecycle-runbook.md` (updated by r2 and r3) | Read and incorporated below. |

The daemon `src` differs from `e555c2c9…` by the one-line `executor.rs` change (r2) and the
`api_review.rs` change that calls `snapshot_between` (r4). No store or engine source changed, and no
CLI source changed. The changes that matter on Windows are the client I/O timeout and the Git spawn
count (r4), the 120 s bridge-test deadline, the request trace, the idle-exit guard and the one-spawn
`Repository::new` (r5), the real-daemon process tests, the Tauri host tests, the long-path git pass-through, the
lifecycle test, the packaging wrapper, the IPC relay and its timeouts, the transport connect path, and
the Tauri host.

Primer and plan at the pinned commit:
[orchestration primer](https://github.com/j-mckerracher/agent-orch/blob/a5e1f43b9516b172247197c6c126acbc6aaf8d54/orch-impl-m.md)
and
[implementation plan](https://github.com/j-mckerracher/agent-orch/blob/a5e1f43b9516b172247197c6c126acbc6aaf8d54/docs/technical-implementation-plan.md).
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
MSVC C toolchain (Visual Studio Build Tools with the C++ workload) as `cl.exe` resolves it. The Tauri
host also needs the Windows SDK resource compiler (`tauri-winres`, part of the same Build Tools) and,
to run the installer or the app, the WebView2 runtime (preinstalled on Windows 11). The installer gate
`desktop-package-win` needs network access: the Tauri CLI downloads NSIS and its plugins on first use.
The Mac run of the desktop checks used Node `v26.13.0`; the Vite 8 and Vitest 5 toolchain needs a
current Node. Record what the host has. A missing tool is `BLOCKED`: report it and do not install
anything. Use a short checkout path and `core.longpaths` as the blocks do.

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

**Priority 12 expected Windows counts (authoritative; they supersede the Priority 11r5 numbers in the
derivation text below, which is kept as the history of the unchanged parts).** They come from the
integration report (`INTEGRATION-REPORT.md`, source `136646d9…`), `scripts/ci/expected.mjs`,
`scripts/ci/windows-test-names.json` and the cfg and test attributes. They are derived, unmeasured on
Windows, and never edited to match a run.

| Gate | Command (in the source checkout unless stated) | Expected on Windows |
|---|---|---|
| `ipc-tests` | `cargo test --locked -p orchestrator-ipc` | `46,0`, 46 passed |
| `windows-transport-tests` | `... -p orchestrator-ipc windows_transport` | `13`, 13 passed, 33 filtered out |
| `daemon-tests` | `... -p orchestrator-daemon` | `50,4,0,1`, 55 passed (50 unit, 4 `coordinator_review`, `daemon.rs` 0 because `cfg(unix)`, 1 `diagnostics_log`), plus `lifecycle: PASS` |
| `coordinator-review-tests` | `... --test coordinator_review` | `4` |
| `daemon-lifecycle` | `... --test lifecycle -- --nocapture` | exit 0, `STEP 1` to `STEP 8`, one `lifecycle: PASS`, no libtest line |
| `git-lib-tests` / `git-env-policy-tests` / `git-discovery-tests` | `-p orchestrator-git --lib` / `--lib env_policy` / `--test repository_discovery` | 47 / 11 / 2 |
| `core-contract-tests` | `-p orchestrator-core --lib contract` | 17 |
| `runtime-env-isolation-test` | `-p orchestrator-runtime --test env_isolation` | 1 |
| `store-tests` | `-p orchestrator-store --lib` | 32 |
| `diagnostics-tests` | `-p orchestrator-diagnostics` | `0,6,7,15,0`, 28 passed |
| `cli-*` | `-p orchestrator-cli --test portable_acceptance` / `cleanup_safety` / `engine_reconciliation_gate` / `crash_concurrency` | 2 / 4 / 22 / 6 |
| `clippy-scoped` | `cargo clippy --locked -p orchestrator-ipc -p orchestrator-daemon --all-targets` | exit 0; the only baseline is `single_match` at `crates/orchestrator-runtime/src/supervisor.rs` lines 409 and 432; NEW diagnostics are reported, not judged |
| `desktop-vitest` | `npx vitest run` in `apps\desktop` | 15 files, 289 tests |
| `tauri-tests` | `cargo test --locked` in `apps\desktop\src-tauri` | `12,0,0`, 12 passed |
| `tauri-bridge-timing` | `cargo test --locked bridge::real_tests -- --nocapture --test-threads=1` in `apps\desktop\src-tauri` | 3 passed; `[bridge-test]` lines saved |
| `node-self-tests` | `node --test --test-reporter=tap scripts/**/*.test.mjs` | 36 pass, 0 fail |
| `harness-list` | `node scripts/ci/run-gates.mjs --list` | exactly 16 lines on Windows |

The Mac workspace `cargo test` total is 708; the Windows workspace total is not a gate (the whole-workspace run
is judged by exit code only, and this runbook does not run it). The harness lists 16 gates on Windows; this runbook
runs 30 gates (the harness's 16, `git-discovery-tests` as part of the per-crate set, the other per-crate test gates,
`tauri-bridge-timing`, `node-self-tests` and `harness-list`). The gate count in the report is 30, not 16.

### Priority 11r5 derivation (history)
Four classes stay separate and are never mixed:

- **Mac-measured** (reported by the M5-05r5 remediation owner on macOS at `a5e1f43b…`, **not
  independently confirmed by the runbook author**, except where stated; none is Windows evidence):
  `cargo test --locked --workspace` exit 0, 641 passed, 0 failed, 1 ignored (the daemon unit suite 59,
  the IPC suite 46, daemon `coordinator_review` **4**, `orchestrator-git` `repository_discovery` **2**,
  daemon `daemon.rs` 9); the lifecycle
  test exit 0 printing `lifecycle: PASS`; the `src-tauri` `cargo test --locked` **10** passed (7
  existing, including the Unix-only `/usr/bin/false` test, plus the 3 `bridge::real_tests`; 8 s with
  prebuilt daemon binaries);
  clippy for the workspace and for `src-tauri` with 0 new warnings (the workspace clippy exits 0 with 7
  warning lines, all in `orchestrator-cli`, baseline); Vitest **15 files, 287 passed**;
  the desktop e2e suite 19 passed (Unix socket only, **not run on Windows**). A `cargo check
  --target x86_64-pc-windows-msvc` and `x86_64-pc-windows-gnu` of the IPC and git crates (`--all-targets`)
  exit 0 and are **type and `cfg` checking
  only: nothing was linked or run, so Windows runtime behaviour is not proven**. The same check was
  **not possible** for the daemon and `src-tauri` crates on the Mac (no sqlite C cross compiler), so
  those changes were never compiled for Windows by the author. r5 adds three tests (2 git, 1 IPC), so
  the Mac counts are the r4 counts plus those. The runbook author re-ran
  `cargo test --locked -p orchestrator-daemon --test coordinator_review` on the Mac at `efa5e8af…`
  (the r3 source): `running 4 tests`, 4 passed, 0 failed (exit 0).
- **Derived for Windows** (this document): the Mac attribute counts, minus every test that the `cfg`
  set removes on Windows, plus the Windows-only tests, read from the source tree `b8460109…` by
  `#[test]` and `cfg` attributes. The derivation below shows the arithmetic. These are the
  **expected** counts the gates enforce. **They are derived and unmeasured for this commit.** The
  Vitest numbers are the Mac-measured numbers; the suite runs under jsdom and has no
  platform-specific test count, so they are expected to match.
- **Prior native measurement** (history, a different tree): at `6b9cf7cd…` (the Priority 11r2 run)
  the run measured IPC `45,0`, `windows_transport` `13` (32 filtered out), daemon unit `42,0,0` (the
  `coordinator_review` binary ran 0 tests), `tauri-tests` `6,0,0`, Vitest 15 files and 287 tests, and
  `lifecycle: PASS`. At `efa5e8af…` (the Priority 11r3 run) the IPC, `windows_transport`, `tauri-tests`
  (`9` passed, 137 s) and Vitest counts matched the derived ones, `coordinator-review-tests` ran 4
  tests and passed 2, and `daemon-tests` printed the `running` sequence `42,4`, exited 101 on the
  `coordinator_review` failures and never ran `lifecycle`. At `1bb24bf9…` (the Priority 11r4 run) IPC
  `45,0`, `windows_transport` 13, daemon `42,4,0` with `lifecycle: PASS`, `coordinator-review-tests` 4
  and Vitest 15 files and 287 tests all matched; `tauri-tests` ran 9 tests, passed 8 and failed 1
  (exit 101), so its `9,0,0` sequence was never measured. The **expected** `coordinator_review` count
  (4) and the `tauri-tests` count (9) are therefore the same at r5; the IPC count is **46** (r5 adds one
  test) and `git-discovery-tests` is new with **2**. (At
  `e555c2c9…` the 10r2 run measured IPC `38,0`, daemon `42,0,0`, Vitest 14 files and 268 tests.)
- **Native-measured**: only what this run records. **Windows counts differ from the Mac counts and are
  reported as measured. They are not matched to the Mac counts, to the earlier native counts, or to the
  derived counts.** A measured count that differs from the derived one gives `INCONCLUSIVE`; the
  operator reports the measured value exactly as printed and never edits an expected value or a
  measured value. A wrong count, whichever way it is wrong, is `INCONCLUSIVE` for the adjudicator.

Every number maps to one argv, package, binary, and platform. `lib.rs` of the IPC crate gates `auth`,
`session`, `client`, `server` and `relay` with `cfg(any(unix, windows))`, `unix_transport` with
`cfg(unix)`, `windows_transport` with `cfg(windows)`, `coordinator_review_tests`,
`event_stream_tests` and the new `idle_subscription_tests` with `cfg(all(test, any(unix, windows)))`,
and `integration_tests` and `lifecycle_tests` with `cfg(all(test, unix))`; `timeout_tests` lives inside
`unix_transport`. `relay_tests.rs` is the `#[cfg(test)] #[path]` module of `relay.rs`; one of its five
tests, `a_wrong_token_is_an_authentication_failure`, is `#[cfg(unix)]`, so Windows runs **4**.
`windows_transport_tests.rs` is the `#[cfg(test)] #[path]` module of `windows_transport.rs`. In the
daemon, `executor_tests.rs` is `cfg(all(test, unix))`, `tests/daemon.rs` starts with `#![cfg(unix)]`,
and `api_tests.rs` and `api_safety_tests.rs` hold the `cfg(unix)` tests named below.
`tests/coordinator_review.rs` has **no** `cfg` gate since r3 (it runs on Windows, **4** tests), and
its `tests/support/` directory is a module directory (`mod support;`), not a test binary. The
`tests/lifecycle.rs` has **no
`cfg` gate** (it is portable on purpose) and is declared `[[test]] name = "lifecycle", harness =
false` in the daemon `Cargo.toml`. In `src-tauri`, `bridge_tests.rs` holds 6 tests, one of which
(`a_daemon_that_exits_at_once_is_reported_with_its_log_location`, which runs `/usr/bin/false`) is
`#[cfg(unix)]`, `launch.rs` holds 1, and the new `bridge_real_tests.rs` (module `bridge::real_tests`)
holds 3 with no `cfg(unix)` test attribute (it has `cfg(unix)` only on a data-directory permission
step inside its fixture). No test in any of these crates carries `#[ignore]`.

| Target | Mac measured or derived | Removed on Windows by `cfg` | Added on Windows | Windows derived (expected) |
|---|---|---|---|---|
| IPC lib unit tests (`src/lib.rs`) | 46 (protocol 6, auth 5, coordinator_review 5, event_stream 9, idle_subscription 4, relay 5, timeout 1, lifecycle 3, integration 8) | 12 (8 `integration_tests` + 3 `lifecycle_tests` + 1 `timeout_tests`) plus 1 (`a_wrong_token_is_an_authentication_failure`) = **13** | 13 `windows_transport::tests` | **46** (protocol 6, auth 5, coordinator_review 5, event_stream 9, idle_subscription 4, relay 4, windows_transport 13) |
| IPC doc-tests | 0 | none | none | **0** (`running 0 tests`) |
| IPC total, unfiltered | 46 | 13 | 13 | **46 passed, 0 failed, 0 ignored**; `running` sequence **46,0** (unit, then doc-tests) |
| IPC filter `windows_transport` | not compiled on macOS | n/a | n/a | **13**; `running` sequence **13** only; `33 filtered out` (46 - 13) |
| Daemon unit tests (`src/main.rs`) | 59 (`main` 1, `api_tests` 15, `api_safety_tests` 8, `api_review_tests` 22, `executor_tests` 13), unchanged by r2 and r3 | 17: 3 `api_tests` + 1 `api_safety_tests` (`cfg(unix)`) + 13 `executor_tests` (module is `cfg(unix)`) | none | **42** (`main` 1, `api_tests` 12, `api_safety_tests` 7, `api_review_tests` 22) |
| Daemon integration `tests\coordinator_review.rs` (no `cfg` since r3) | 4 (it was 1) | none | none, it runs unchanged | **4** (`running 4 tests`; the 4 names are listed below) |
| Daemon integration `tests\daemon.rs` (`#![cfg(unix)]`) | 9 | all 9 | none | **0** (`running 0 tests`) |
| Daemon `lifecycle` (`harness = false`) | prints 7 `STEP` lines and `lifecycle: PASS`; **0 libtest tests** | none | the whole binary (no `cfg`) | **no `running` line, no `test result` line**; the verdict is exit 0 plus the line `lifecycle: PASS` |
| Daemon total, unfiltered | 72 libtest tests (plus lifecycle) | 26 | none | **46 passed, 0 failed, 0 ignored** (42 + 4); `running` sequence **42,4,0**; `lifecycle: PASS` in the same stdout |
| Daemon filter `--test coordinator_review` (the `coordinator-review-tests` gate) | 4 | none | none | **4 passed, 0 failed, 0 ignored, 0 filtered out**; `running` sequence **4** only (one binary, no doc-test stage, no unit binary) |
| `src-tauri` unit tests (`agent_orch_desktop_lib`) | 10 (`bridge_tests` 6, `launch` 1, `bridge::real_tests` 3) | 1 (`bridge_tests`, `cfg(unix)`) | none | **9** (`bridge_tests` 5, `launch` 1, `real_tests` 3) |
| Git integration `tests\repository_discovery.rs` (new in r5; the `git-discovery-tests` gate) | 2 (no `cfg`) | none | none, it runs unchanged | **2 passed, 0 failed, 0 ignored, 0 filtered out**; `running` sequence **2** only (one binary, no doc-test stage, no unit binary) |
| `src-tauri` total | 10; sequence `10,0,0` (lib, `main.rs`, doc-tests) | 1 | none | **9 passed, 0 failed, 0 ignored**; `running` sequence **9,0,0** (derived from the Mac layout, see the notes) |
| Vitest (`apps/desktop`) | 287 in 15 files (Mac) | none (jsdom) | none | **15 files, 287 tests** (Mac-measured, unmeasured on Windows) |

The Windows IPC count (46) equals the Mac IPC count (46) by coincidence: the Mac runs 8
`integration_tests`, 3 `lifecycle_tests`, 1 `timeout_tests` and the wrong-token relay test that
Windows does not run (13 tests), and Windows runs the 13 `windows_transport` tests that the Mac does
not. The composition differs. Do not read the equal totals as a match of the same tests.

Notes on the `running` sequences (these are the exact reasons a count can be wrong):

- **A filtered cargo run prints no doc-test stage.** `cargo test --locked -p orchestrator-ipc
  windows_transport` prints exactly one `running 13 tests` block and one `test result` line (`13
  passed; 0 failed; 0 ignored; 0 measured; 33 filtered out`, derived: 46 - 13). The 10r2 run measured
  this shape at `e555c2c9…` with `25 filtered out` (38 - 13).
- **An unfiltered IPC run prints a doc-test stage.** The IPC crate is a library, so the sequence is the
  unit binary and then `running 0 tests` for the doc-tests (`46,0`).
- **The daemon is a binary crate, so it has no doc-test stage.** Its integration test binaries run
  after the unit binary, in file-name order: `coordinator_review` (4 tests on Windows since r3), then
  `daemon` (an empty crate on Windows because of `#![cfg(unix)]`, 0 tests). The `lifecycle` binary runs
  last and, having no libtest harness, prints **no** `running N tests` line, so it adds nothing to the
  sequence (`42,4,0`). Cargo counts it as 0 tests. `42,0,0` was measured natively at `e555c2c9…`,
  `3df3b583…` and `6b9cf7cd…`; the `4` between is derived from r3 and **has no native measurement**.
- **A single-binary run prints one block.** `cargo test --locked -p orchestrator-daemon --test
  coordinator_review` builds and runs only that integration binary, so the sequence is `4` (one
  `running 4 tests` line, one `test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered
  out` line). The Mac printed exactly that.
- **`lifecycle` is not judged by libtest counts.** Its verdict needs exit code 0 and the line
  `lifecycle: PASS` (it prints a `STEP n` line for each of 7 steps first). A run that prints a
  `running` or `test result` line for it is a different shape and is `INCONCLUSIVE`.
- **`src-tauri` is a separate cargo package with its own `Cargo.lock` and an empty `[workspace]`
  table**, so it is outside the root workspace. Its library target is `agent_orch_desktop_lib`
  (`staticlib`, `cdylib`, `rlib`) and it has a `main.rs` binary. The sequence `9,0,0` is the lib unit
  tests, the `main.rs` binary (0 tests) and the doc-tests (0). The Mac printed `10,0,0` for the same
  layout; the doc-test stage and the binary stage are carried over, and the `9` is derived. This
  stage layout is **derived, not measured on Windows**. The 3 `bridge::real_tests` need the daemon
  binary: by default they build it (and the `seed_review` example) once, with a nested `cargo build
  --locked` into `<CARGO_TARGET_DIR or src-tauri\target>\real-daemon`, **which takes several minutes on
  the first run** (a cold build of the daemon and SQLite). If `AGENT_ORCH_DAEMON_BIN` and
  `AGENT_ORCH_SEED_BIN` both name existing binaries, the nested build is skipped. This runbook does
  not set them (the nested build is part of what is under test).
- `examples\seed_review.rs` is an **example**, not a test. `cargo test` builds it and does not run it.

Per-file Vitest counts (Mac-measured at the pinned source; they sum to 287): `daemon/appData` 30,
`daemon/client` 16, `native/native.test.tsx` 18, `review/ReviewScreen` 44, `review/binding` 26,
`review/daemonSources` 8, `review/diff` 17, `review/integration` 2, `review/sources` 20,
`sources/daemon` 14, `sources/fixtures` 9, `ui/daemonScreens` 7, `ui/logic` 19, `ui/screens1` 30,
`ui/screens2` 27. That is 15 files. The per-file numbers are context and are **not** a gate; the gate
is the two summary lines.

The Unix-only tests, by file, are never Windows proof and none of them runs here:

- IPC `integration_tests.rs` (8), `lifecycle_tests.rs` (3), `timeout_tests.rs` (1), and the relay test
  `a_wrong_token_is_an_authentication_failure` (1).
- Daemon `executor_tests.rs` (13): the whole executor test module, including the orphan-termination
  tests. **The scheduler and executor therefore have no native Windows test at all in this run.**
- Daemon `api_tests.rs` (3): `running_task_cancellation_requests_owner_without_mutating_run_state`,
  `running_task_cancellation_with_crashed_owner_conflicts_and_retires_generation`,
  `running_task_without_owner_rejects_cancellation_without_changing_state`.
- Daemon `api_safety_tests.rs` (1): `workspace_diff_rejects_symlink_escape_before_git`.
- Daemon `tests\daemon.rs` (9). (`tests\coordinator_review.rs` is no longer Unix-only: it runs on
  Windows since r3, 4 tests.)
- `src-tauri` `bridge_tests.rs` (1): `a_daemon_that_exits_at_once_is_reported_with_its_log_location`.

Windows-run tests by name (the arrays in the `$Checks` block hold these exact names; every name must
report `ok`):

- **IPC (46).** `protocol::tests::` `frame_round_trip_uses_versioned_big_endian_header`,
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
  `idle_subscription_tests::` (4, one new in r5) `live_subscription_keeps_an_otherwise_idle_server_running`,
  `server_idles_out_after_its_last_subscriber_leaves`,
  `unsubscribed_connection_does_not_keep_the_server_alive`,
  `a_call_longer_than_the_idle_timeout_does_not_stop_the_server`. `relay::tests::` (4, new)
  `closing_a_relay_is_not_a_daemon_shutdown_and_a_new_relay_resumes_the_stream`,
  `a_daemon_that_stops_ends_the_relay_with_a_remote_close`,
  `no_daemon_is_unavailable_not_an_authentication_failure`,
  `the_welcome_reports_only_the_capabilities_the_daemon_grants`. `windows_transport::tests::` the 13
  names listed under the next bullet.
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
- **Daemon unit (42).** `main::tests::lock_conflict_reports_live_holder_pid`. `api::tests::` (12)
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
- **Daemon `coordinator_review` integration binary (4, new on Windows in r3).** These run in
  `daemon-tests` (after the 42 unit tests) and alone in `coordinator-review-tests`; every name must
  report `ok` in both. `coordinator_conversation_survives_daemon_restart_and_grants_no_authority`,
  `daemon_gates_the_coordinator_and_review_methods_on_token_and_capability`,
  `approval_flow_approves_and_delivers_through_the_real_daemon`,
  `approval_flow_rejects_and_requests_changes_through_the_real_daemon`.
- **`orchestrator-git` `repository_discovery` integration binary (2, new in r5).** These run only in
  `git-discovery-tests`. `discovery_agrees_from_root_subdirectory_and_linked_worktree`,
  `a_directory_outside_any_repository_is_refused`.
- **`src-tauri` (9).** `bridge::tests::` (5) `forwards_frames_and_closing_a_window_leaves_the_daemon_running`,
  `the_ui_cannot_shut_the_daemon_down_or_use_another_windows_connection`,
  `a_daemon_that_stops_is_reported_as_a_closed_connection_not_a_task_state`,
  `no_daemon_and_no_executable_is_daemon_unavailable`,
  `a_daemon_executable_that_cannot_start_is_daemon_unavailable`. `launch::tests::` (1)
  `an_oversized_log_is_rotated_and_a_small_one_is_kept`. `bridge::real_tests::` (3, new in r3)
  `the_host_launches_the_real_daemon_and_drives_an_approval_to_delivery`,
  `the_host_drives_reject_and_request_changes_through_the_real_daemon`,
  `the_host_forwards_capability_and_token_gating_from_the_real_daemon`.

### Check table

The 30 gates run in this order (the table lists the Priority 11 gates; the per-crate Priority 12 gates, `tauri-bridge-timing`, `node-self-tests` and `harness-list` are in the Priority 12 table above and are run in the order of the `$Checks` block). Every gate is one process with one exit code.

| Check | Command | Expected on Windows |
|---|---|---|
| `daemon-build` | `cargo build --locked -p orchestrator-daemon` (new target directory) | Exit 0. `Compiling orchestrator-ipc`, `orchestrator-engine`, `orchestrator-store` and `orchestrator-daemon` lines and a `Finished` line. `debug\orchestrator-daemon.exe` exists; its size and SHA-256 are recorded. **First native Windows build with this code: see the compile-failure triage.** |
| `ipc-tests` | `cargo test --locked -p orchestrator-ipc` | Exit 0. `running` sequence `46,0`, 46 passed, 0 failed, 0 ignored, all 46 names `ok` (including the r5 `a_call_longer_than_the_idle_timeout_does_not_stop_the_server`). |
| `daemon-tests` | `cargo test --locked -p orchestrator-daemon` | Exit 0. `running` sequence `50,4,0,1`, 55 passed (50 unit + 4 `coordinator_review` + 1 `diagnostics_log`), 0 failed, 0 ignored, all 55 names `ok`, and the line `lifecycle: PASS` (the harness-free lifecycle binary runs last in this command). |
| `daemon-lifecycle` | `cargo test --locked -p orchestrator-daemon --test lifecycle -- --nocapture` | Exit 0. The `STEP` lines `1` to `8` in order, then exactly one `lifecycle: PASS` line. **No libtest count: cargo counts 0 tests for this binary and the gate does not use one.** **On any failure** the test panics with a block that starts `--- lifecycle diagnostics ---`. The harness copies that block (from stdout and stderr), the per-call timing block and the `daemon log tail` block when present, into `daemon-lifecycle.diagnostics.txt`, and any leftover `daemon.log` found under the isolated TEMP into `daemon-lifecycle.daemon-log.txt`, and publishes both. The same is done for `daemon-tests`, which runs the same binary. |
| `coordinator-review-tests` (**new in r3**) | `cargo test --locked -p orchestrator-daemon --test coordinator_review` (isolated TEMP, gate limit 600 s) | Exit 0. One binary, so `running` sequence `4`, `test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`, all 4 names `ok`. Each test spawns the real `orchestrator-daemon.exe` over a private `ao*\d` data directory and drives it over the named pipe with a 30 s client I/O timeout (r4; it was 2 s at r3); its own waits are up to 30 s, so a healthy run takes seconds, the binary is already built by `daemon-tests`, and a hung daemon fails at the test's own timeout instead of stalling the gate. **On any failure** the test prints, to stderr, the per-call timing block (`--- per-call timing (client I/O timeout 30s) ---` ... `--- end per-call timing ---`, one `<ms> ms  ok|ERR  <label>` line per call) and the daemon log tail (`--- daemon log tail (<path>) ---` ... `--- end daemon log ---`, the last 60 lines). The harness copies every such block (and every r3-form `--- daemon log (<path>) ---` block), and the end of stdout and stderr, into `coordinator-review-tests.diagnostics.txt` and `coordinator-review-tests.daemon-log.txt` (and for `daemon-tests` the same two files), and publishes them. **A failure is a finding, and it never stops the remaining gates.** The r3 run failed 2 of the 4 tests here with `Frame(Io(TimedOut))`. |
| `windows-transport-tests` | `cargo test --locked -p orchestrator-ipc windows_transport` | Exit 0. `running` sequence `13` (one block, no doc-test stage), 13 passed, 33 filtered out, all 13 names `ok`. |
| `git-discovery-tests` (**new in r5**) | `cargo test --locked -p orchestrator-git --test repository_discovery` (isolated TEMP, gate limit 900 s) | Exit 0. One test binary and no doc-test stage, so `running` sequence `2`, `test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`, both names `ok`: `discovery_agrees_from_root_subdirectory_and_linked_worktree` and `a_directory_outside_any_repository_is_refused`. The tests are cross-platform, create their own repositories (and a linked worktree) under the isolated TEMP and run the real `git.exe`; they prove that `Repository::new` (now one `rev-parse` spawn) resolves the same top-level and common directory from the root, a subdirectory and a linked worktree, and refuses a directory outside any repository. **A failure is a finding and never stops the remaining gates.** |
| `clippy-scoped` | `cargo clippy --locked -p orchestrator-ipc -p orchestrator-daemon --all-targets` (no `-D warnings`) | Exit 0 with `Checking orchestrator-ipc`, `Checking orchestrator-daemon` and `Finished`. The baseline is the two `single_match` warnings at `supervisor.rs:409` and `:432` (in `orchestrator-runtime`, a dependency that the scope also lints). **Every other diagnostic is recorded as NEW, verbatim, and does not change the verdict.** The relay, the transport changes, `tests/lifecycle.rs` and the r3 `tests/coordinator_review.rs` and `tests/support/` were never linted on Windows. |
| `desktop-npm-ci` | `npm.cmd ci` in `apps\desktop` | Exit 0. |
| `desktop-tsc` | `npx.cmd tsc -b` in `apps\desktop` | Exit 0, no diagnostics. |
| `desktop-build` | `npm.cmd run build` in `apps\desktop` | Exit 0 and a `built in` line from Vite. Creates `apps\desktop\dist`, which the Tauri host needs. The bundle size is recorded and is **not** a gate. |
| `desktop-vitest` | `npx.cmd vitest run` in `apps\desktop` | Exit 0. `Test Files  15 passed (15)` and `Tests  289 passed (289)`, nothing failed or skipped. |
| `tauri-build` | `cargo build --locked` in `apps\desktop\src-tauri` (same target directory) | Exit 0. `Compiling agent-orch-desktop` and `Finished`. `debug\agent-orch-desktop.exe` exists; its size and SHA-256 are recorded. **The Tauri host has never been compiled on Windows with this code.** |
| `tauri-tests` | `cargo test --locked` in `apps\desktop\src-tauri` (isolated TEMP, gate limit 3600 s) | Exit 0. `running` sequence `12,0,0`, 12 passed, 0 failed, 0 ignored, all 12 names `ok` (5 `bridge::tests`, 1 `launch::tests`, 3 `bridge::real_tests`, 3 launch-failure redaction tests). **The first run needs several minutes of nested daemon build**: the 3 `real_tests` run `cargo build --locked` for `orchestrator-daemon` and the `seed_review` example into `<CARGO_TARGET_DIR>\real-daemon` once per process, so a cold build of the daemon and the bundled SQLite happens inside this gate. The 3600 s limit is generous but bounded; the nested build is not skipped (the harness clears the `AGENT_ORCH_DAEMON_BIN` and `AGENT_ORCH_SEED_BIN` variables). A failed nested build fails the test with the build output, and nothing is skipped. On failure the harness keeps every `--- daemon log (<path>) ---` and `--- daemon log tail (<K> of <N> lines, <path>) ---` block, every `[bridge-test] <window> <method> answered in <elapsed>` timing line, and every daemon request-trace line (`request start id=… method=…` and `request end id=… method=… elapsed_ms=… status=…`, written by the daemon because the test sets `AGENT_ORCH_TRACE_REQUESTS=1`), in `tauri-tests.diagnostics.txt` and `tauri-tests.daemon-log.txt`. The r5 panic text names the call: `<method> (id N, limit 120s): no frame before the deadline`. libtest captures the `[bridge-test]` lines of passing tests, so they appear only for failed tests; the harness does not pass `--nocapture`. At r3 all 9 tests passed but each took over 60 s (137 s in all, excluding the nested build); the gate duration is recorded in `result.md` and is **not** a gate. |
| `tauri-clippy-clean` | `cargo clean -p agent-orch-desktop` in `apps\desktop\src-tauri` (same target directory) | Exit 0. It removes only this package's cached build output, so that the next gate really lints it. Without it a cached build makes cargo print no line for the package. Nothing else is removed. |
| `tauri-clippy` | `cargo clippy --locked --all-targets --verbose` in `apps\desktop\src-tauri` (no `-D warnings`) | Exit 0 and a `Finished` line. **Both** `--crate-name agent_orch_desktop_lib` and `--crate-name agent_orch_desktop` must appear in the `Running` lines (twice each at the Mac measurement: the library and the binary, plus their test targets). A `Checking agent-orch-desktop` line is **not** required and never appears: cargo prints one status line per package, and for this package it prints `Compiling` (its build script) instead. The Priority 11 first run read that as `INCONCLUSIVE`. Baseline: none (Mac: 0 warnings). **Every diagnostic is recorded as NEW and does not change the verdict.** |
| `desktop-package-win` | `npm.cmd run package:win` in `apps\desktop` | Exit 0. `package:win` runs `node scripts/package.mjs nsis`, which normalizes `CI` (`1` or `true` becomes `true`, `0` or `false` becomes `false`, anything else is removed), stages the release daemon as the sidecar (`prepare-daemon-sidecar.mjs`) and builds the NSIS installer (`tauri build --config src-tauri/tauri.bundle.conf.json --bundles nsis`, per-user install mode). The harness also sets `CI=true`, so a direct `tauri build` would parse too. The installer `release\bundle\nsis\Agent Orchestrator_0.1.0_x64-setup.exe` exists in the target directory; its size and SHA-256 are recorded. **Building the installer is not installing or running it.** Do not install it from this harness. |

The 30 gates are followed by three **harness checks** that are not gates and each have a status in `result.md`: `gitattributes-check` (run before the gates and again after them, as `gitattributes-before` and `gitattributes-after`: `git check-attr`, and a search for carriage returns in the pinned LF files), `source-status-after` (`git status`) and `source-diff-after` (`git diff --stat` and `git diff`). They decide the clean-tree verdict. They are described under the status rules.

After the checks the block records fourteen rows that are **NOT RUN or UNPROVEN by construction** and one
derived row (`tauri-native-bridge-host-level`) whose status depends on two gates (see below). None of
them raises the verdict. The runbook starts no daemon of its own, installs nothing and drives no window.
The daemons that run are started only by the test gates themselves.

### Not run and UNPROVEN rows

An overall `PASS` never covers these. `result.md` lists each of them under "UNPROVEN, not covered by
this result". Do not try to exercise them from the harness, and do not fabricate a result.

| Row | Why it is not covered |
|---|---|
| `unproven-second-user` | Denial of the named pipe to a second, different local user. The runbook has no second account and never asks for credentials. |
| `unproven-remote-client` | Rejection of a genuinely remote client. No remote host and no SMB loopback probe are used here. |
| `unproven-slow-reader-64kib` | A single daemon-to-client write above the 64 KiB pipe buffer to a slow-reading client. No gate here sends one. |
| `unproven-e2e-windows` | The desktop e2e suite (`npm run test:e2e`, 19 tests on the Mac) starts the real daemon over a **Unix domain socket** and needs `ORCH_DAEMON_BIN`. It cannot run on Windows. **NOT RUN.** Do not run it, and do not report a Windows e2e number. |
| `unproven-tauri-webview-click-path` | The webview and React click path: the window, the webview, the `daemon_open` / `daemon_send` / `daemon_close` capability as the webview invokes it, and a person (or a UI driver) clicking through the approval flow. The r3 tests drive the Rust `Bridge` directly, never a window. **UNPROVEN by construction.** |
| `unproven-gui-install-launch` | Installing the NSIS installer, launching "Agent Orchestrator", the host starting the sidecar daemon, and the window showing "Connected". The harness builds the installer only. |
| `unproven-gui-task-survives-close` | A real daemon-owned task surviving the UI window close and reopen (Windows window close exits the app; the detached daemon and the agent must keep running; the reopened window shows the same task `Running`). The headless `daemon-lifecycle` gate proves the daemon-side behaviour with a fake agent and the UI-side relay, not the window. |
| `unproven-gui-event-reconnect` | The window's event stream reconnecting and resuming at the saved cursor and epoch after a close and reopen, and drafts surviving. |
| `unproven-stale-token-relaunch-timing` | The derived timing (about 0.3 s per relay attempt through `connect_fail_fast`, about 0.6 s before the host relaunches the daemon, then 200 ms retries for 15 s; relay handshake and write timeouts 10 s; a plain `Client::connect` waits up to 5 s) was read from code and is **UNMEASURED** on Windows. |
| `unproven-daemon-idle-behaviour` | The 60 s true-idle exit, the live subscription keeping a daemon alive and the idle clock restarting when the last subscriber leaves, observed on the real app. Only the Mac `idle_subscription_tests` (4, in `ipc-tests` here) cover the logic; the GUI behaviour is unproven. |
| `unproven-daemon-kill-and-sidecar-failure` | Killing the daemon pid (the host starts a new daemon on the next reconnect; the previous agent is not assumed alive) and a sidecar that fails to start ("Daemon unavailable"). Both stop or break a process, which this runbook never does. |
| `unproven-data-dir-acl` (**new at Priority 12**) | M6-01 R11. The ACLs of the Windows data directory and its files were never inspected. By-hand check 1 below. |
| `unproven-launch-failure-redaction` (**new at Priority 12**) | M6-01. The desktop host quoting only the redacted tail of `daemon.log` when the daemon fails to launch. The `tauri-tests` unit tests cover the logic; the real window is not driven. By-hand check 2 below. |
| `unproven-objective-64kib-r12` (**new at Priority 12**) | M6-01 R12. A ~64 KiB objective on Windows, where the 32,767-character command-line limit can make the agent spawn fail. By-hand check 3 below. |

### By-hand checks (Priority 12)

These three checks are **not run by the script**. A person runs them after Step 3 and before Step 5, then writes
one plain-text file, `manual-p12.txt`, in `$EvidenceDir`: for each check, its number, `PASS`, `FAIL`,
`FINDING` or `NOT RUN`, the exact commands that were run, and what was seen. The publication scan reads it like
every other evidence file, so write no tokens, no profile names, no home paths and no objective text. The
three UNPROVEN rows stay `UNPROVEN` whatever `manual-p12.txt` says; the coordinator reads the file. If nobody
runs the checks, write `manual-p12.txt` with `NOT RUN` for all three, or leave it absent. Use a new, empty
data directory under `$RunRoot` for each check. Stop only a process that you started yourself, by its pid.
Do not touch the default data directory (`%LOCALAPPDATA%\agent-orch`).

1. **Data-directory ACL (R11).** The Windows code sets no DACL on the data directory, `store.sqlite`,
   `daemon.token` or `daemon.log`, so they inherit the ACL of the parent directory. Run the built daemon with an
   explicit data directory under the run root (`orchestrator-daemon.exe --data-dir <dir>`, from the
   `daemon-build` target `debug\orchestrator-daemon.exe`), wait until `<dir>\daemon.token` exists, then run
   `icacls <dir>`, `icacls <dir>\store.sqlite`, `icacls <dir>\daemon.token` and `icacls <dir>\daemon.log`, and stop
   the daemon pid that you started. Expected: only the current user, `SYSTEM` and `BUILTIN\Administrators`
   (inherited from the profile). Any other principal (for example `BUILTIN\Users`, `Everyone` or
   `Authenticated Users`) with read access is a **FINDING**: record the line, do not change any ACL. Record the
   parent directory's ACL as well, because a data directory outside the profile can inherit more.
2. **Redacted launch failure.** Debug builds of the Tauri host honour `AGENT_ORCH_DAEMON_BIN` and
   `AGENT_ORCH_DATA_DIR`. Make a data directory whose `daemon.log` contains a planted line with a fake secret
   (for example `token=FAKE-SECRET-12345`; never a real credential). Point `AGENT_ORCH_DAEMON_BIN` at a
   program that exits at once with a non-zero code (for example `cmd.exe /c exit 3` wrapped in a copy named
   with `.exe`, or any small program that exits 1), set `AGENT_ORCH_DATA_DIR` to that directory, and run the
   debug host `agent-orch-desktop.exe` from the `tauri-build` target. Expected: the window shows a launch error
   that quotes only the **redacted** end of `daemon.log`, with no `FAKE-SECRET-12345` and no daemon token.
   Close the window. A visible fake secret or a real token is a **FINDING**. If no window can be driven, record
   `NOT RUN`; the `tauri-tests` redaction tests are the only evidence then.
3. **~64 KiB objective (R12).** Build the CLI (`cargo build --locked -p orchestrator-cli` in the source checkout,
   binary `orchestrator-cli.exe` in the target `debug` directory) and run, with a new data directory and a small
   throwaway git repository that has a `main` branch, `orchestrator-cli.exe --data-dir <dir> run --repo <repo>
   --objective <text of about 65000 characters>` (generate the text in the shell from a repeated word, and do not
   paste it into the evidence). If the flags differ, `orchestrator-cli.exe run --help` governs. Expected: the
   objective is accepted by validation (it is at most 64 KiB), then the agent spawn **fails closed** with a clear
   spawn or argument-length error, and the command returns. It must not hang, must not crash the daemon, and must
   not print the objective. Record the exit code and the first error line. A hang, or a spawn that silently
   truncates the objective, is a **FINDING**. If the agent program is not installed, the error names the missing
   program instead; say so.

### Bridge-test timing capture

`tauri-bridge-timing` re-runs only the three `bridge::real_tests` with `--nocapture` and `--test-threads=1`.
libtest hides the `[bridge-test] <window> <method> answered in <elapsed>` lines of a passing test, so without
this gate no timing exists for a pass. The harness saves those lines and the daemon `request start` and
`request end` trace lines, redacted, to `tauri-bridge-timing.bridge-lines.txt` whatever the verdict. Quote the
slowest call per test in the report. The r4 run failed on a 30 s `delivery.request` timeout
(`bridge_real_tests.rs:240`); a call near 30 s or 120 s here is a finding even when the gate passes. The gate
re-uses the target directory, so it does not rebuild the daemon.

### The derived row `tauri-native-bridge-host-level` (r3)

This row replaces the r2 row `unproven-tauri-native-bridge` and is the one row whose status the
harness computes from two gates. It is `PASS` only if **both** `coordinator-review-tests` and
`tauri-tests` are `PASS` in this run: the real `orchestrator-daemon.exe` served the `coordinator.*`,
`review.*`, `approvals.*`, `workspaces.diff` and `delivery.request` methods over the Windows named pipe,
and the Tauri host's `Bridge` launched that daemon through its real detached launch path and drove an
approval to delivery, a reject and a request-changes, and the capability and token gating. In any other
case (a gate is `FAIL` or `INCONCLUSIVE`) the row is `UNPROVEN`, and the notes name the gate that did
not pass. A `PASS` here is **host-level only**. It is not a webview, window, installer or click-path
proof: those are `unproven-tauri-webview-click-path`, `unproven-gui-install-launch`,
`unproven-gui-task-survives-close` and `unproven-gui-event-reconnect`, which stay `UNPROVEN` whatever the
overall result. The release rule (section 5.5 of the in-repo runbook) says M5-05 must not be marked done
on these tests alone if the release rule requires the desktop UI itself to have driven the flow. This
runbook cannot make that call, and a `PASS` here does not make it.

### Optional manual GUI checklist (human steps, not run by the harness)

These steps are optional and need a person at an interactive Windows desktop. They are **not** gates and
the harness never runs them. They are sections 5.5 and 5.6 of `docs/evidence/m5-05-native-lifecycle-runbook.md`
in the source:

- Section 5.5 is the automated real-daemon approval flow. Its three commands are exactly the
  `coordinator-review-tests`, `daemon-tests` and `tauri-tests` gates here, so a human does not repeat
  them. Read it for what those tests do and do not prove.
- Section 5.6 is the manual Windows checklist (install the per-user NSIS installer with no UAC prompt;
  see "Connected" and one daemon; open the review screen for a seeded task and see daemon data, not
  fixtures; request approval and approve in the UI and read `refs/heads/orchestrator/review/<task>`;
  reject and request changes on a second task; close the window while a task runs and reopen; kill only
  the daemon pid and rename the sidecar; uninstall). Step 7 (kill the daemon, break the sidecar) is
  the one step this runbook forbids the Windows agent to perform. A human who runs section 5.6 records
  PASS, FAIL or UNPROVEN and the observed pids for each step in `manual-lifecycle.txt`.

If nobody runs them, write every step as `NOT RUN`. The observations do not change a harness verdict.
The rows `unproven-gui-install-launch`, `unproven-gui-task-survives-close`, `unproven-gui-event-reconnect`
and `unproven-tauri-webview-click-path` stay `UNPROVEN` in `result.md` either way.

### Native lifecycle procedure (incorporated from `docs/evidence/m5-05-native-lifecycle-runbook.md`)

This section restates the in-repo runbook for Windows. It does not replace it, and where the in-repo
runbook says a step is UNPROVEN on Windows, that stays true. The harness automates only the build,
test and installer-bundle parts (the gates above). The graphical steps below are not automatable by
this harness and the rows above stay `UNPROVEN` whatever you do.

**What the app is.** `agent-orch-desktop` (the Tauri host and webview) and `orchestrator-daemon` (the
sidecar) are separate processes. The host relays authenticated frames over the daemon's local
transport (a named pipe on Windows). The webview never sees the token; the Tauri capability grants
only `daemon_open`, `daemon_send` and `daemon_close` (no shell, fs or `core:default`). The host
starts the daemon only when no daemon answers and never stops it, and refuses `shutdown` and
`handshake`. On Windows the host starts the daemon with `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`
and **no** `CREATE_BREAKAWAY_FROM_JOB`. The installed app places the sidecar next to the app
executable as `orchestrator-daemon.exe`. A release app always uses that sidecar. `AGENT_ORCH_DAEMON_BIN`
affects debug builds only.

**Data directory.** `AGENT_ORCH_DATA_DIR` overrides it; the Windows default is
`%LOCALAPPDATA%\agent-orch`. It holds `daemon.log` (daemon stdout and stderr; rotated once at host
launch to `daemon.log.1` when over 4 MiB), `store.sqlite`, `daemon.token` and `ipc\`. The Windows
data directory has no `0700`-style restriction in the host code and its ACL is not checked here.

**Idle behaviour (from the in-repo runbook).** The daemon exits by itself after 60 s of true idleness:
no request, no open event subscription, no pending executor pass and no active task. A live UI event
subscription counts as activity, and the 60 s clock restarts when the last subscriber disconnects.
Running agents also keep the daemon alive. Verified only by the Mac IPC idle tests (3); not driven
through the GUI.

**Error strip labels (from the in-repo runbook).** Starting: "Connecting..."; connection lost and
retrying: "Reconnecting..."; retries exhausted: "Disconnected (daemon may still be running)", never
to be read as "agents stopped"; no daemon and the host could not start one: "Daemon unavailable"
(read `daemon.log`); wrong or stale token: "Authentication failed" (restart the app); version or
capability mismatch: "Incompatible version or capability"; connected: "Connected" with the note that
a connection does not prove any agent is running; always: "Recovery: unknown (D3 placeholder)".
Fixture data appears only in a plain browser with `?fixture=1` and never in the native app.

**Windows connect timing (derived from code, UNMEASURED).** When a stale `daemon.token` exists but the
pipe is gone, the host relay fails fast: about 0.3 s per attempt (instead of about 5 s), so roughly
0.6 s before the host relaunches the daemon, after which `launch_and_open` retries every 200 ms for
15 s. Relay handshake and write timeouts are 10 s. A plain `Client::connect` (CLI, tests) still waits
up to 5 s for a pipe that has not appeared yet, because the daemon writes the token before it creates
the pipe. None of these numbers was measured on Windows.

**Headless lifecycle proof (automated here).** `daemon-tests` and `daemon-lifecycle` run
`tests/lifecycle.rs`. The test has no libtest harness: it re-executes its own binary as the fake agent
(selected by the `AGENT_ORCH_LIFECYCLE_AGENT_PID_FILE` environment variable; as an agent it answers
`--version` and otherwise sleeps up to 90 s), so cargo reports 0 tests for it and the result is the
printed `STEP` lines and `lifecycle: PASS` (non-zero exit code on failure). The seven steps:

1. start a real daemon with a fake agent binary;
2. submit a task; the daemon launches the agent process (the agent writes its own pid and the OS
   reports it alive; a separate fresh connection reads the task as `Running`);
3. open the UI-side relay (the same transport code the Tauri host uses) and subscribe to the event
   stream;
4. close the UI client: this must not stop the daemon or the agent;
5. reopen: authenticate again, report the same journal epoch, and resume the stream at the saved
   cursor with no gap;
6. cancel through another client; the reopened stream sees the transition with no gap, the task
   becomes `Cancelled` and the agent process exits;
7. a relay on a stopped daemon reports a remote close, and the daemon shuts down cleanly (and removes
   `daemon.token`).

It does **not** prove anything about the Tauri window, the webview or the capability configuration. A
passing fixture test, an open pipe, or "Connected" in the UI is not execution proof. It uses a base
path that is not canonicalized on non-Unix, a 30 s wait, and `Daemon::drop` ignores the exit status of
its kill and wait. At `3df3b583…` it failed natively after STEP 2 (waiting for the agent pid); r2 makes
it fail fast and print a diagnostics block instead of timing out blind. The most likely further Windows
failures are `is_process_running` after the task is `Cancelled` (whether the OS reports the fake agent
gone) and the `daemon.token` removal assertion. A Windows failure here is a **finding, not a flake**:
capture the full output, do not retry in this evidence directory.

**Build and install (Windows, from the in-repo runbook; build automated, install not).**

```powershell
cd apps\desktop
npm ci
npm run package:win
# installer: <target>\release\bundle\nsis\Agent Orchestrator_0.1.0_x64-setup.exe
```

`prepare:sidecar` builds `orchestrator-daemon` in release mode and copies it to
`src-tauri\binaries\orchestrator-daemon-<host-triple>.exe` (git-ignored). The staged sidecar is added
as `externalBin` by `src-tauri\tauri.bundle.conf.json` (kept out of the base `tauri.conf.json`, so a
plain `cargo check` needs no staged binary). Not packaged yet: code signing, auto-update, other
bundle types. The Tauri CLI once rewrote `src-tauri\Cargo.toml` features on the Mac, so the final
`source-status-after` check also catches a modified tracked file after `desktop-package-win`.

**Manual native window lifecycle (not run by the harness).** If an operator has an interactive
desktop and the agent CLI named in the task (such as `codex`) installed, steps 1 to 5 and 9 can be
attempted. Do **not** perform steps 6 and 7 (they stop or break a process, which the safety rules
forbid); they stay `UNPROVEN`. Use a short `AGENT_ORCH_DATA_DIR`, set in the session that launches
the app. Never stop the daemon: it detaches by design and exits by itself about 60 s after its last
client leaves. Record its pid and later absence.

1. Install the installer (per-user) and launch "Agent Orchestrator". Expect "Connected" within a few
   seconds and the daemon as a separate process.
2. Register a repository in the Projects screen and start a long-running task from the UI. Note the
   agent pid.
3. Type text into a draft field without submitting.
4. Close the window. On Windows this exits the app. The daemon (`Get-Process orchestrator-daemon`)
   and the agent must still be running with the same pids.
5. Reopen the app. Expect "Connected", the task list reloaded from the daemon, the same task
   `Running` with the same agent pid, and the draft intact.
6. Not run: kill the daemon pid.
7. Not run: make the sidecar fail to start.
8. macOS only (Cmd+Q): does not apply on Windows.
9. Leave a window connected for more than 60 s with no task: the daemon pid must not change. Then
   quit the app and wait about 60 s: the daemon may exit; a later launch starts a new one.

If you do run any of these, write what you actually observed (step number, PASS, FAIL or NOT RUN,
pids and timestamps, no paths, no URLs, no profile names) to `manual-lifecycle.txt` in `$EvidenceDir`
before step 4, as the block in step 4 describes. Those notes are operator observations. They do not
change the harness verdicts, and the rows above stay `UNPROVEN`: the coordinator decides whether to
accept them. If you do not run them, record every step as `NOT RUN`. Never write a step as PASS that
you did not observe.

### Windows hazards that explain a failure (read before you report one)

- **`PIPE_NOWAIT` and `ReadFile` error 232.** Reading an empty named pipe with `PIPE_NOWAIT` returns
  `ERROR_NO_DATA` (232), which is not end of file. This is the Priority 9 failure. The transport now
  reads with `ReadFile` and the relay polls (25 ms) instead of blocking. A reappearance would show up
  as a relay or event test reporting an unexpected close.
- **Tight timeouts.** Windows named-pipe creation, connect and handshake take tens of milliseconds
  longer than on Unix. The 10r failure was a 50 ms stream timeout. **The 11r3 failure was the 2 s
  `Client` I/O timeout** (`Frame(Io(TimedOut))` on varying calls): the daemon runs many Git processes
  per `review.evidence` call, and process creation on Windows with real-time antivirus scanning is
  slow, so one daemon call can exceed 2 s. r4 gives the test clients 30 s and removes two Git processes
  per `review.evidence`, so a timeout of the 30 s bound now means a call that took far longer than the
  r3 calls did; that is a finding, and the per-call timing block shows which call. **The r4 `tauri-tests`
  failure was this shape in `bridge_real_tests.rs` (a 30 s wait on `delivery.request`, an empty daemon
  log); r5 raises that wait to 120 s, prints `[bridge-test]` per-call timings and the daemon log tail
  and turns the daemon request trace on, so a repeat shows which call was slow and for how long the
  daemon worked on it.** The new idle tests use a 2 s idle
  timeout and a 1 s stream timeout; `relay_tests` uses a 10 s wait and a 60 s idle timeout. The
  `windows_transport` suite uses a 500 ms `start()` timeout and a one-second bound on
  `write_to_a_closed_peer_fails_promptly`. A timing-shaped failure is a finding: report the message.
- **`core.autocrlf=true`** (Git for Windows sets it in the system config) can rewrite line endings of
  checked-out files. This caused the 10 first-run failure, and at `3df3b583…` the three autogenerated
  `permissions\autogenerated\daemon_*.toml` files showed as modified (LF to CRLF warnings) after the
  build (the cause there is unproven: the autocrlf rewrite and a rewrite by `tauri build` are both
  candidates). The source now has a `.gitattributes` that pins `text eol=lf` for those files, both
  `Cargo.lock` files and `package-lock.json`. The `gitattributes-check` and the clean-tree checks
  test whether the pin works on Windows. The lifecycle test sets the fixture repository's
  `core.autocrlf=false`. Do not change the setting; the `git-autocrlf` record shows it. Never compare
  file contents byte for byte in a test that runs on Windows because of autocrlf.
- **`MAX_PATH` (260 characters).** A task id is `task-` plus 64 hex characters, and the daemon names
  the task worktree and branch with it, so the path of a task worktree under a long base can exceed
  260 characters. This is the **medium-low-confidence, unproven** hypothesis for the `3df3b583…`
  lifecycle failure (the test timed out waiting for the agent pid, after STEP 2, with the daemon output
  discarded). r2 passes `-c core.longpaths=true` to every git call, and this runbook uses a short run
  root and sets `core.longpaths` in the checkout. Other causes are **not** excluded: a Job Object or
  suspended-launch failure in the runtime supervisor, process-identity capture, or the restart gate. If
  `daemon-lifecycle` fails again, read `daemon-lifecycle.diagnostics.txt` first. Do not guess.
- **No `CREATE_BREAKAWAY_FROM_JOB`.** The detached daemon stays in the parent's job if the job forbids
  breakaway, and a job that kills its children on close would take the daemon with it. Not tested
  here. The NSIS installer also cannot replace a running detached sidecar.
- **`cfg(unix)` tests are silent no-ops on Windows** (`running 0 tests` binaries). That is by design
  and not a missing gate.
- **`spawnSync("cargo")` and the `.exe` path in `prepare-daemon-sidecar.mjs`** ran on Windows at
  `3df3b583…` only as far as the `CI` argument error (`invalid value '1' for '--ci'`), so the sidecar
  staging and the NSIS bundle step have **never completed natively**. A failure in `desktop-package-win`
  may be in the scripts, not in the Tauri build.
- **`CI` must be `true` or `false` (or unset) for the Tauri CLI.** Its `--ci` flag binds the `CI`
  environment variable and rejects `1`. This harness sets `$env:CI = 'true'`, and `package.mjs`
  normalizes it too. If you run `tauri build` by hand for any reason, keep it `true`.
- **`tauri build` rewrites `src-tauri\Cargo.toml`** (the features table form). The source is committed
  in that form, so a second run must leave it unchanged. A modified `Cargo.toml` after the build is
  reported by the clean-tree checks, not corrected.
- **The real-daemon tests print the per-call timing and the daemon log on failure.** `Daemon::drop` in
  `crates/orchestrator-daemon/tests/support/process.rs` prints, to stderr, `--- per-call timing (client
  I/O timeout 30s) ---` … `--- end per-call timing ---` (lines `<ms> ms  ok|ERR  <label>`) and `--- daemon
  log tail (<path>) ---` … `--- end daemon log ---` (the last 60 lines) when the test is failing.
  `Fixture::drop` in `apps/desktop/src-tauri/src/bridge_real_tests.rs` prints, at r5, `--- daemon log tail
  (<K> of <N> lines, <path>) ---` … `--- end daemon log ---` (the last 80 lines, the token redacted) when a
  test panics, and each call prints `[bridge-test] <window> <method> answered in <elapsed>` (libtest shows
  these only for a failed test). The daemon, started with `AGENT_ORCH_TRACE_REQUESTS=1`, writes
  `[<epoch ms>] request start id=<id> method=<m>` and `[<epoch ms>] request end id=<id> method=<m>
  elapsed_ms=<n> status=ok|error <code>` to its log (no parameters, no results). The r3 and r4 form `---
  daemon log (<path>) ---` is still accepted. The harness accepts every marker form. The daemon runs from a short `ao*`
  directory under the isolated TEMP (the data directory is `d`, the repository `r`, the worktrees `w`), so
  `MAX_PATH` stays within limits. Read these blocks first (they are copied into the diagnostics files).
  An empty daemon log means the daemon wrote nothing (r3 had empty logs): say so.
- **Seconds-scale waits.** The `coordinator_review` and lifecycle tests wait at most 30 s for the daemon
  to become ready (the token file, then the pipe), 15 s for it to stop and 30 s for each other wait; the
  `bridge::real_tests` wait 120 s for each frame (r5; 30 s at r4), and the test clients' I/O timeout is
  30 s (r3: 2 s). A first named-pipe
  connection on a cold machine can be slower than on the Mac, so a timeout of one of these waits is a
  finding that you report with the daemon log, not a flake and not a reason to raise a limit. **A known,
  unfixed r5 risk:** the daemon closes a connection that never subscribed after 2 s of idleness, which
  could surface as a `closed` frame instead of a timeout; report the frame as printed.
- **The token-tamper step rewrites `daemon.token` in place.** The two capability and token gating tests
  (`coordinator_review` and `bridge::real_tests`) write a wrong token of the same length into the
  existing file, then restore it. This relies on the owner's DACL allowing a write. If Windows denies
  it, only that step fails; the failure text says which call failed. Report it verbatim.
- **The nested daemon build.** The first `tauri-tests` run compiles `orchestrator-daemon` and the
  `seed_review` example again with a nested `cargo build --locked` into
  `<CARGO_TARGET_DIR>\real-daemon` (several minutes, once per process). The harness sets one
  `CARGO_TARGET_DIR` for the whole run, so the nested build does not touch `src-tauri\target`.
  If the output ends in `Compiling` lines with no `running 9 tests` line, say that the build was still
  running when the limit was reached.
- **Deferred remediation findings (not fixed in this candidate):** the CSP still allows `style-src
  'unsafe-inline'`; the host has no method allowlist (it forwards any method except `shutdown` and
  `handshake`); the journal cursor and epoch are not persisted across a restart; the Windows data
  directory has no checked ACL. Tauri-on-Windows test binaries can also fail to start with
  `STATUS_ENTRYPOINT_NOT_FOUND` (exit code `-1073741511`, `0xC0000139`) when a test binary lacks the
  expected application manifest. If `tauri-tests` ends that way, report it as a host or crate finding
  and do not change anything.

### Scope and disclosures (what these gates prove and do not prove)

- **Native Windows proof here is:** the shared protocol, authentication, event-stream, coordinator
  and review capability tests, the live-subscription idle tests and the relay tests of the IPC crate
  (46), the 13 named-pipe tests, the daemon API, review and safety unit tests (42), the real
  daemon-plus-fake-agent lifecycle proof (`lifecycle: PASS`, headless), the 4 `coordinator_review`
  process tests that drive the real daemon over the named pipe (r3), the 2 `orchestrator-git`
  `repository_discovery` tests (r5), a Windows build of the daemon
  and of the Tauri host, the host's 9 tests (6 unit tests and, r3, 3 `bridge::real_tests` that launch
  the real daemon and drive the approval flow through the host's `Bridge`, headless, no window), a
  clippy pass over the IPC, daemon and host scopes,
  the desktop install, type-check, bundle build and unit-test suite, and whether the NSIS installer
  builds.
- **Not covered, and no claim is made:** the fourteen UNPROVEN rows above; the daemon scheduler and
  executor (their tests are `cfg(unix)`); the store, engine and CLI crate tests and every
  `orchestrator-git` test except `repository_discovery`; the running Tauri
  window and webview, and the webview or React click path; installing or launching the installer; a
  daemon started by the installed app; server identity verification by the client; the data-directory
  ACL and the token owner; and any scenario in a test that is `cfg(unix)`. The row
  `tauri-native-bridge-host-level` can be `PASS` only at host level (the Rust `Bridge` and a real
  daemon, no window); it never covers the webview path.
- The daemon binary built by `daemon-build` is **never started** by the harness. The daemon runs only
  inside `daemon-tests`, `daemon-lifecycle` and `coordinator-review-tests` (the daemon side) and
  `tauri-tests` (the host side), started by the tests themselves.
- Fixtures and recorded paths use the **Git top-level path form** (`C:/…` from `git rev-parse
  --show-toplevel`). They never use a verbatim canonical path (`\\?\C:\…` or `//?/C:/…` from
  `fs::canonicalize`). Git for Windows rejects a verbatim worktree destination. Step 1 records the
  top-level and refuses a verbatim form.
- Mac counts and earlier native counts are not Windows counts for this commit.

### If a build fails (compile-failure triage)

**The r4 and r5 changes have not been compiled natively on Windows by their authors, and the daemon and
`src-tauri` parts were not even type-checked for Windows.** At `6b9cf7cd…` (r2) the whole candidate compiled and passed
natively, at `efa5e8af…` (r3) the whole candidate compiled natively (r3 failed only at run time), and at
`1bb24bf9…` (r4) the whole candidate compiled and 15 of 16 gates passed. r5 changes production code in
`orchestrator-ipc` (`session.rs` request trace and in-flight counter, `server.rs`) and
`orchestrator-git` (`repository.rs`, one-spawn discovery), plus the new `repository_discovery.rs`, the
IPC idle test and `bridge_real_tests.rs`. r4 changed production code in `orchestrator-ipc` (`client.rs`, `lib.rs`, `unsupported.rs`),
`orchestrator-git` (`repository.rs`) and `orchestrator-daemon` (`api_review.rs`), plus the daemon test
support code and `bridge_real_tests.rs`. `daemon-build`, `ipc-tests` and the desktop gates are
affected only through those crates. Any of `daemon-build`, `ipc-tests`, `daemon-tests`,
`coordinator-review-tests`, `git-discovery-tests`, `clippy-scoped`, `tauri-build`, `tauri-tests` or `tauri-clippy` can fail to
compile or fail at run time because of that code, and `tauri-tests` can also fail in its nested daemon
build. Do this, in this order, and nothing else:

1. **Capture; touch nothing.** `Invoke-Bounded` has already written the full stdout and stderr of the failed
   command (`<name>.stderr.txt` and the raw bytes in `$RawDir`). Do not edit any source file, manifest,
   or lock file. Do not install a toolchain, a component, or a package. Do not retry in this evidence
   directory. Do not run the failing command by hand to "see more".
2. **Read the first `error`.** Classify it in your report and in nothing else:
   - **Host toolchain**: `link.exe` not found, `cl.exe`, `lib.exe` or `rc.exe` not found, a missing
     Windows SDK, `failed to run custom build command for libsqlite3-sys` or for `tauri-winres` or
     the `agent-orch-desktop` build script, a missing WebView2 loader, or a missing `rustc`. This is
     a host problem, not a candidate defect. Report `BLOCKED` for the host and name the missing tool.
   - **Network or registry**: `failed to download`, `failed to get`, or a registry error while
     resolving a crate (the Tauri host pulls a large new dependency tree), or an NSIS download
     failure in `desktop-package-win`. `--locked` forbids lock changes; report it as an environment
     failure. Do not remove `--locked`.
   - **Rust compile error in the candidate**: an `error[E…]` in `crates\orchestrator-ipc`,
     `crates\orchestrator-daemon`, `crates\orchestrator-store`, `crates\orchestrator-engine`,
     `crates\orchestrator-git` or `apps\desktop\src-tauri`. Copy the whole diagnostic, including the
     `-->` location, unedited. This is a **candidate defect for Windows** and the most valuable
     result of this run.
   - **Node packaging error** in `desktop-package-win`: a failure in `prepare-daemon-sidecar.mjs`
     (the release daemon build or the copy) or in the Tauri CLI. Copy the first error line verbatim.
3. **Continue where the runbook allows.** A compile failure makes that gate `FAIL`. The harness still
   runs the remaining gates, writes `result.md` with the failure and the UNPROVEN rows, and publishes
   the evidence. Gates that depend on a failed gate may fail for that reason (the Tauri gates need
   `apps\desktop\dist` from `desktop-build`). Report the first failure as the cause. Do not skip a gate
   to "save time".
4. **Publish the evidence anyway.** Step 5 publishes every result, including `FAIL`. Commit and push
   the evidence branch even when the first command failed. A run with no published evidence is a
   protocol failure, not a quiet success.
5. **Report the exact error text** and the classification. Do not guess a fix.

### Coverage gaps

- Unix-only tests did not run on Windows (the list above). Their macOS results are separate and are
  not Windows evidence.
- The scheduler and executor, the orphan-termination path and the daemon integration test
  `tests/daemon.rs` have no native Windows run (`coordinator_review` and `lifecycle` do). The fourteen
  UNPROVEN or NOT RUN rows stay unproven whatever the overall result.
- No `rustfmt` gate, no workspace-wide test run, no store, engine or CLI test run, and only the
  `repository_discovery` test of the git crate. The Node e2e
  is not run. No installer is installed and no window is driven by the harness.

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
   `docs/priority-11r5-windows-acceptance-a5e1f43`. Before the first block, confirm in a clone that you do
   not otherwise use that the declared commit is that branch tip and that its only parent is the
   source commit:
   `git rev-parse origin/docs/priority-11r5-windows-acceptance-a5e1f43` must print the declared commit, and
   `git rev-list --parents -n 1 <RunbookCommit>` must print exactly
   `<RunbookCommit> a5e1f43b9516b172247197c6c126acbc6aaf8d54` (one parent, the source). The first block
   re-checks the parent itself and stops on any other chain.
2. Take every block from the file at that commit, for example with `git show
   <RunbookCommit>:docs/evidence/priority-11r5-windows-acceptance-a5e1f43-runbook.md` in a clone you do
   not otherwise use. Do not edit a block except as step 1 allows (`$SourceBranch` and its name
   check, and `$RunbookCommit`).
3. Set `$RunbookCommit` in the first block to that full commit. The block stops when it is empty. The
   value is recorded in `runbook-identity.txt` as **operator-declared**. It is not proof of the bytes
   that you ran; step 2 above is what keeps the bytes honest. The block also fetches that commit,
   checks that it is exactly one docs commit directly on the source commit (its only parent is
   `a5e1f43b…`, no merge, no other commit between them) and that the whole difference from the source is
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
$SourceBranch = 'orch/priority-12-integration-136646d'
$SourceSha = '136646d95c86b050ca6f6617bb75bf10758826bb'
$SourceTree = '99d432d1b2ceb48fe99ada088a5fd8864a4038dd'
# The runbook file path in the runbook commit (see Candidate).
$RunbookPath = 'docs/evidence/priority-12-windows-acceptance-136646d-runbook.md'
# The accepted baseline: the Priority 10r2 tree (native PASS with limits). Its evidence proves only itself.
$BaseSha = 'e555c2c922ce00ee93673452f67848c7f888b4f1'
$ExpectedRangeCount = 39
# The 39 commits above the baseline in `git rev-list --reverse --topo-order`, oldest first. The ids and
# the count are pinned. The first fifteen are the linear Priority 11 chain (the last of them, 1bb24bf9, is
# the M5-05r4 candidate). The next eleven (39f482c9 to e01edbb7) are M6-01 (security) and its
# remediation, merged by e01edbb7; then M6-02 (CI harness; dcf277c5, 7a9752f8, a43f2408, merged by
# e40ef7de), M6-03 (diagnostics; 92ffcd25, 14618891, merged by 376b1733), aed1b24d (CI counts for the
# Priority 12 merge), a5e1f43b (M5-05r5, merged by 1ae44f47), e2853204 (CI counts for round 5) and the
# source 136646d9 (the `windows_transport` filtered-out count, 33). Four of the 39 are merge commits
# (e01edbb7, e40ef7de, 376b1733, 1ae44f47). The source's only parent is e2853204.
$ExpectedRangeMerges = @(
    'e01edbb7addfdce15e7f0fa57adc517a0dca4157',
    'e40ef7de6cf70e5fa75264adf178f2c8a769689d',
    '376b17330a8a74f7189e74b5139137dd500aaa98',
    '1ae44f4759dbb6469b892b9ec0e016e7911a7940'
)
$ExpectedRangeCommits = @(
    '64d2c9f7f295a1f453d7dc9ad9c2b14fe7eba3cc',
    'cbbb9e8d390980ed82508bcfc83a627992c9e9c0',
    '598848a001277ff46e2a75520a09772d99f607d4',
    '1e78ee00604d33257a16d9840d962e6364e53fe8',
    'e8c75bbcd6038e14d411a7bf2dfaa0ac5cd68fc3',
    'c8b8451d1cf3d9899e840b0182085e7eeb62e957',
    '8991e324482fdd0ad8325f63dee2f19819136b4f',
    'b5a31428b5a7ea34d8880b27885835182b02caa3',
    '1a895121febb9a4925d2fe6130a6966363ee2494',
    '3df3b5831ffab764aeeddf14de2caef17b00cf4a',
    'aff805b8d74f78e2115dd6e72cb8ca36a5206ce9',
    '8c8481b67499d83b2ee87264ab4729e131bb1aa5',
    '6b9cf7cdee06dd439492b15d8d9f6c7138059b2b',
    'efa5e8af3469082226c61cccfea167cc607d6a0f',
    '1bb24bf98716a22e8955b50cd28643d063674240',
    '39f482c99ae5e9752c8b532bb88cad6c56fd56b8',
    'e4d2b48f0f90ee17238b328ef314ae1efe81e4b6',
    'e7c027c28e0eae81ec7217d4518a174d6d58ab3f',
    'be590e233659c9a92821db26504b8b13515c20dc',
    'fcc9bf2ff251f8aa7b5fcfe5f52e1b9d05f93e3f',
    '756b47e75762aac92305dbcb2c0e79721897159a',
    'b3081cb7bc4e26664ff908c94d9a7a52ef936ebb',
    '5ce3965f6f9177087b8305d35c3ead20bbc40396',
    '0bf2149200a4cd3173bc21be187dd20c6cdce383',
    '1cb834b9afafdb5113522dd07da36717f70ed493',
    '4f2e0821064e3b9725219241f01183c0622f81bd',
    'e01edbb7addfdce15e7f0fa57adc517a0dca4157',
    'dcf277c5d335d2f779d8b0f571a92b20a73c636f',
    '7a9752f813570212628b7ae2fc5df991768f2d4f',
    'a43f2408b13ad21cf8abc3d89ee9bfc584b27162',
    'e40ef7de6cf70e5fa75264adf178f2c8a769689d',
    '92ffcd25482b7b97a4d85b38f5849687a0f611dd',
    '1461889152cba5a482d98da027c8a6901cfd4176',
    '376b17330a8a74f7189e74b5139137dd500aaa98',
    'aed1b24dd99d24ed0bdbe1aabd74f85a457644cc',
    'a5e1f43b9516b172247197c6c126acbc6aaf8d54',
    '1ae44f4759dbb6469b892b9ec0e016e7911a7940',
    'e2853204866b91b4250cd71573e00dd1796e667f',
    '136646d95c86b050ca6f6617bb75bf10758826bb'
)
$ExpectedRangeFiles = @(
    '.cargo/audit.toml',
    '.gitattributes',
    '.github/workflows/audit.yml',
    '.github/workflows/ci.yml',
    '.github/workflows/release.yml',
    'Cargo.lock',
    'Cargo.toml',
    'apps/desktop/.gitignore',
    'apps/desktop/package.json',
    'apps/desktop/scripts/package.mjs',
    'apps/desktop/scripts/prepare-daemon-sidecar.mjs',
    'apps/desktop/src-tauri/Cargo.lock',
    'apps/desktop/src-tauri/Cargo.toml',
    'apps/desktop/src-tauri/build.rs',
    'apps/desktop/src-tauri/capabilities/default.json',
    'apps/desktop/src-tauri/permissions/autogenerated/daemon_close.toml',
    'apps/desktop/src-tauri/permissions/autogenerated/daemon_open.toml',
    'apps/desktop/src-tauri/permissions/autogenerated/daemon_send.toml',
    'apps/desktop/src-tauri/src/bridge.rs',
    'apps/desktop/src-tauri/src/bridge_real_tests.rs',
    'apps/desktop/src-tauri/src/bridge_tests.rs',
    'apps/desktop/src-tauri/src/launch.rs',
    'apps/desktop/src-tauri/src/lib.rs',
    'apps/desktop/src-tauri/tauri.bundle.conf.json',
    'apps/desktop/src-tauri/tauri.conf.json',
    'apps/desktop/src/daemon/connection.ts',
    'apps/desktop/src/daemon/errors.ts',
    'apps/desktop/src/main.tsx',
    'apps/desktop/src/native/boot.ts',
    'apps/desktop/src/native/native.test.tsx',
    'apps/desktop/src/native/tauriConnector.ts',
    'apps/desktop/src/review/ReviewRoute.tsx',
    'apps/desktop/src/sources/daemon.test.ts',
    'apps/desktop/src/sources/daemon.ts',
    'apps/desktop/src/ui/ConnectionStrip.tsx',
    'apps/desktop/src/ui/ProjectDetail.tsx',
    'apps/desktop/src/ui/app.css',
    'apps/desktop/src/ui/createServices.ts',
    'apps/desktop/src/ui/daemonScreens.test.tsx',
    'apps/desktop/src/ui/drafts.ts',
    'apps/desktop/src/ui/logic.test.ts',
    'apps/desktop/src/ui/screens1.test.tsx',
    'apps/desktop/src/ui/services.tsx',
    'apps/desktop/src/ui/submission.ts',
    'crates/orchestrator-agents/Cargo.toml',
    'crates/orchestrator-agents/src/claude_code_adapter.rs',
    'crates/orchestrator-agents/src/codex_adapter.rs',
    'crates/orchestrator-agents/src/pi_rpc_adapter.rs',
    'crates/orchestrator-agents/tests/codex_adapter.rs',
    'crates/orchestrator-cli/Cargo.toml',
    'crates/orchestrator-cli/src/workflow.rs',
    'crates/orchestrator-cli/tests/cleanup_safety.rs',
    'crates/orchestrator-cli/tests/crash_concurrency.rs',
    'crates/orchestrator-cli/tests/engine_reconciliation_gate.rs',
    'crates/orchestrator-cli/tests/portable_acceptance.rs',
    'crates/orchestrator-cli/tests/recovery.rs',
    'crates/orchestrator-core/Cargo.toml',
    'crates/orchestrator-core/src/contract.rs',
    'crates/orchestrator-daemon/Cargo.toml',
    'crates/orchestrator-daemon/examples/seed_review.rs',
    'crates/orchestrator-daemon/src/api.rs',
    'crates/orchestrator-daemon/src/api_review.rs',
    'crates/orchestrator-daemon/src/api_safety_tests.rs',
    'crates/orchestrator-daemon/src/api_tests.rs',
    'crates/orchestrator-daemon/src/executor.rs',
    'crates/orchestrator-daemon/src/executor_tests.rs',
    'crates/orchestrator-daemon/src/idempotency.rs',
    'crates/orchestrator-daemon/src/main.rs',
    'crates/orchestrator-daemon/tests/coordinator_review.rs',
    'crates/orchestrator-daemon/tests/daemon.rs',
    'crates/orchestrator-daemon/tests/diagnostics_log.rs',
    'crates/orchestrator-daemon/tests/lifecycle.rs',
    'crates/orchestrator-daemon/tests/support/mod.rs',
    'crates/orchestrator-daemon/tests/support/process.rs',
    'crates/orchestrator-daemon/tests/support/seed.rs',
    'crates/orchestrator-diagnostics/Cargo.toml',
    'crates/orchestrator-diagnostics/src/lib.rs',
    'crates/orchestrator-diagnostics/src/redact.rs',
    'crates/orchestrator-diagnostics/src/secrets.rs',
    'crates/orchestrator-diagnostics/tests/diagnostics.rs',
    'crates/orchestrator-diagnostics/tests/known_secrets.rs',
    'crates/orchestrator-diagnostics/tests/redaction.rs',
    'crates/orchestrator-engine/Cargo.toml',
    'crates/orchestrator-engine/src/reconcile.rs',
    'crates/orchestrator-git/Cargo.toml',
    'crates/orchestrator-git/src/env_policy.rs',
    'crates/orchestrator-git/src/git.rs',
    'crates/orchestrator-git/src/lib.rs',
    'crates/orchestrator-git/src/repository.rs',
    'crates/orchestrator-git/src/verification.rs',
    'crates/orchestrator-git/src/worktree.rs',
    'crates/orchestrator-git/tests/repository_discovery.rs',
    'crates/orchestrator-ipc/Cargo.toml',
    'crates/orchestrator-ipc/src/client.rs',
    'crates/orchestrator-ipc/src/idle_subscription_tests.rs',
    'crates/orchestrator-ipc/src/lib.rs',
    'crates/orchestrator-ipc/src/protocol.rs',
    'crates/orchestrator-ipc/src/relay.rs',
    'crates/orchestrator-ipc/src/relay_tests.rs',
    'crates/orchestrator-ipc/src/server.rs',
    'crates/orchestrator-ipc/src/session.rs',
    'crates/orchestrator-ipc/src/transport.rs',
    'crates/orchestrator-ipc/src/unix_transport.rs',
    'crates/orchestrator-ipc/src/unsupported.rs',
    'crates/orchestrator-ipc/src/windows_transport.rs',
    'crates/orchestrator-runtime/Cargo.toml',
    'crates/orchestrator-runtime/src/supervisor.rs',
    'crates/orchestrator-runtime/tests/env_isolation.rs',
    'crates/orchestrator-runtime/tests/reconcile.rs',
    'crates/orchestrator-store/Cargo.toml',
    'crates/orchestrator-store/src/data_lock.rs',
    'crates/orchestrator-store/src/lib.rs',
    'crates/orchestrator-store/src/private.rs',
    'crates/orchestrator-store/src/sqlite.rs',
    'deny.toml',
    'docs/evidence/m5-05-native-lifecycle-runbook.md',
    'docs/release-ci.md',
    'docs/security.md',
    'docs/security/m6-01-threat-model-and-dependency-review.md',
    'scripts/ci/bounded.mjs',
    'scripts/ci/bounded.test.mjs',
    'scripts/ci/expected.mjs',
    'scripts/ci/expected.test.mjs',
    'scripts/ci/gates.mjs',
    'scripts/ci/macos-test-names.json',
    'scripts/ci/parse.mjs',
    'scripts/ci/parse.test.mjs',
    'scripts/ci/run-gates.mjs',
    'scripts/ci/survivors.mjs',
    'scripts/ci/survivors.test.mjs',
    'scripts/ci/windows-test-names.json',
    'scripts/release/env.mjs',
    'scripts/release/env.test.mjs',
    'scripts/release/package.mjs',
    'scripts/release/signing.mjs',
    'scripts/release/signing.test.mjs'
)
if ($ExpectedRangeFiles.Count -ne 136) { throw 'The pinned changed-file list is not 136 entries.' }
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
if ($SourceBranch -cnotmatch '^orch/priority-12-integration-[a-z0-9-]+$') {
    throw 'The source branch name is not valid.'
}
$Short = $SourceSha.Substring(0, 7)
if ($RunbookPath -cne "docs/evidence/priority-12-windows-acceptance-$Short-runbook.md") {
    throw 'RunbookPath is not the Priority 12 runbook file for this source commit.'
}
$RunId = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ') + '-' +
    [Guid]::NewGuid().ToString('N')
# A short root: a long one pushes task worktrees past MAX_PATH. Use a short TEMP too if yours is long.
$RunRoot = Join-Path $env:TEMP ('ao12-' + $RunId.Substring(0, 16) + '-' + $RunId.Substring(17, 8))
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
# The Tauri CLI binds --ci to this variable and accepts only true or false. '1' made tauri build fail at 3df3b583.
$env:CI = 'true'
# Prebuilt daemon binaries would skip the nested build of the src-tauri real-daemon tests. Clear them,
# so every run builds and tests the daemon of the pinned source.
Remove-Item -LiteralPath Env:AGENT_ORCH_DAEMON_BIN, Env:AGENT_ORCH_SEED_BIN -ErrorAction SilentlyContinue

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
# Repo-local only (never --global or --system), set before the checkout so long paths work in it.
$Run = Invoke-Git 'config-longpaths' @('config', '--local', 'core.longpaths', 'true')
if ($Run.Code -ne 0) { throw 'Cannot set core.longpaths in the checkout.' }
$Run = Invoke-Git 'config-longpaths-check' @('config', '--local', '--get', 'core.longpaths')
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or $Run.Out[0] -cne 'true') {
    throw 'core.longpaths is not true in the checkout.'
}
$Run = Invoke-Git 'branch-tip' @('rev-parse', "refs/remotes/origin/$SourceBranch")
if ($Run.Code -ne 0) { throw 'Cannot read the branch tip.' }
$BranchTip = $Run.Out[0]
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'branch-tip.txt'), "$BranchTip`n")

$Run = Invoke-Git 'candidate-containment' @(
    'merge-base', '--is-ancestor', $SourceSha, "refs/remotes/origin/$SourceBranch")
if ($Run.Code -ne 0) { throw 'The branch does not contain the source commit.' }
$Run = Invoke-Git 'base-ancestry' @('merge-base', '--is-ancestor', $BaseSha, $SourceSha)
if ($Run.Code -ne 0) { throw "The source commit does not descend from $BaseSha." }
# The source has exactly one parent, the previous pinned range commit, and a pinned tree.
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
# The range holds exactly the pinned commits (compared as a set, so the walk order cannot matter) and
# exactly the four pinned merge commits.
$Run = Invoke-Git 'range-merges' @('rev-list', '--merges', "$BaseSha..$SourceSha")
if ($Run.Code -ne 0 -or (@($Run.Out | Sort-Object) -join ',') -cne (@($ExpectedRangeMerges | Sort-Object) -join ',')) {
    throw 'The merge commits above the baseline are not the pinned merge commits.'
}
$Run = Invoke-Git 'range-commits' @('rev-list', '--topo-order', '--reverse', "$BaseSha..$SourceSha")
if ($Run.Code -ne 0 -or (@($Run.Out | Sort-Object) -join ',') -cne (@($ExpectedRangeCommits | Sort-Object) -join ',')) {
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

# The .gitattributes pins: these files must be LF in the working tree even under core.autocrlf=true.
# Run once right after the checkout and once after the build. A failure is recorded and makes the
# run FAIL; it never stops the run.
$GitAttrFiles = @(
    'apps/desktop/src-tauri/permissions/autogenerated/daemon_close.toml',
    'apps/desktop/src-tauri/permissions/autogenerated/daemon_open.toml',
    'apps/desktop/src-tauri/permissions/autogenerated/daemon_send.toml',
    'Cargo.lock',
    'apps/desktop/src-tauri/Cargo.lock',
    'apps/desktop/package-lock.json'
)
function Test-GitAttributes([string] $Label) {
    $Problems = [Collections.Generic.List[string]]::new()
    $Lines = [Collections.Generic.List[string]]::new()
    $Check = Invoke-Git "gitattributes-$Label" (@('check-attr', 'text', 'eol', '--') + $GitAttrFiles)
    if ($Check.Code -ne 0) { $Problems.Add('git check-attr failed.') }
    foreach ($File in $GitAttrFiles) {
        foreach ($Pair in @(@('text', 'set'), @('eol', 'lf'))) {
            $Expected = "${File}: $($Pair[0]): $($Pair[1])"
            if ($Check.Out -cnotcontains $Expected) { $Problems.Add("Missing '$Expected'.") }
        }
        $Path = Join-Path $SourceDir ($File -replace '/', '\')
        if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
            $Problems.Add("$File is absent.")
            continue
        }
        $Bytes = [IO.File]::ReadAllBytes($Path)
        $HasCr = [Array]::IndexOf($Bytes, [byte]13) -ge 0
        $Lines.Add("$File : $($Bytes.Length) bytes, carriage return present: $HasCr")
        if ($HasCr) { $Problems.Add("$File holds a carriage return in the working tree.") }
    }
    $Status = if ($Problems.Count -eq 0) { 'PASS' } else { 'FAIL' }
    [IO.File]::WriteAllLines((Join-Path $EvidenceDir "gitattributes-$Label.summary.txt"),
        [string[]](@("status: $Status") + $Lines + $Problems))
    [PSCustomObject]@{ Status = $Status; Notes = (Protect-Text ($Problems -join ' ')) }
}
$GitAttrBefore = Test-GitAttributes 'before'
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
evidence directory. The seventeen gates run in the order listed: the standalone `daemon-build` goes
first, in the new, empty target directory, so its `Compiling` lines are real. The six test gates
(`ipc-tests`, `daemon-tests`, `daemon-lifecycle`, `coordinator-review-tests`, `windows-transport-tests`, `git-discovery-tests`) and the clippy gate follow, then the four desktop gates (`npm ci` first,
because it creates the `node_modules` directory that the other three use; it is inside the checkout
and Git ignores it), then the four Tauri host gates (`tauri-build`, `tauri-tests`, `tauri-clippy-clean`,
`tauri-clippy`,
which need `apps\desktop\dist` from `desktop-build`), and last `desktop-package-win`, the slowest, which
needs network access. Nothing starts the daemon except the tests that start their own (the lifecycle test,
the `coordinator_review` tests and the `bridge::real_tests` host tests). **The first `tauri-tests` run
needs several minutes of nested daemon build**, and the gate limits are bounded: 1800 s by default,
600 s for `coordinator-review-tests` (its binary is already built, and its own waits are seconds) and
3600 s for `tauri-tests`. A cold build can still exceed a limit; do not raise one (see the timeout note
under the status rules). The first
Windows build of the daemon and the host needs an MSVC C toolchain, because `libsqlite3-sys` compiles
bundled C and `tauri-winres` needs the resource compiler. A missing
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
    'reconnect_still_requires_the_current_token',
    'live_subscription_keeps_an_otherwise_idle_server_running',
    'server_idles_out_after_its_last_subscriber_leaves',
    'unsubscribed_connection_does_not_keep_the_server_alive',
    'a_call_longer_than_the_idle_timeout_does_not_stop_the_server',
    'closing_a_relay_is_not_a_daemon_shutdown_and_a_new_relay_resumes_the_stream',
    'a_daemon_that_stops_ends_the_relay_with_a_remote_close',
    'no_daemon_is_unavailable_not_an_authentication_failure',
    'the_welcome_reports_only_the_capabilities_the_daemon_grants'
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
    'evidence_snapshot_lists_at_most_one_thousand_files',
    # Eight more unit tests since M6-01, M6-02 and M6-03 (src/api_safety_tests.rs, src/idempotency.rs,
    # src/executor.rs and src/main.rs).
    'submit_refuses_read_only_autonomy_because_the_agent_would_write',
    'submit_rejects_oversized_and_nul_contract_fields',
    'idempotency_store_failures_report_their_cause_too',
    'internal_causes_do_not_leak_into_the_next_request',
    'internal_error_causes_reach_the_injected_redacting_sink',
    'a_failed_listing_is_logged_and_never_reads_as_nothing_running',
    'a_readable_empty_listing_is_quiet_and_empty',
    'interrupted_tasks_are_unknown_when_either_listing_failed'
)
# The four process tests of tests/coordinator_review.rs. They have no cfg(unix) and spawn the real daemon.
$CoordinatorReviewTests = @(
    'coordinator_conversation_survives_daemon_restart_and_grants_no_authority',
    'daemon_gates_the_coordinator_and_review_methods_on_token_and_capability',
    'approval_flow_approves_and_delivers_through_the_real_daemon',
    'approval_flow_rejects_and_requests_changes_through_the_real_daemon'
)
# daemon-tests runs the 50 unit tests, the 4 coordinator_review tests and the diagnostics_log test.
$DiagnosticsLogTests = @(
    'a_failed_launch_is_explained_in_the_log_without_the_credential'
)
$DaemonTests = $DaemonTests + $CoordinatorReviewTests + $DiagnosticsLogTests
# The two cross-platform tests of orchestrator-git tests/repository_discovery.rs (new in r5).
$GitDiscoveryTests = @(
    'discovery_agrees_from_root_subdirectory_and_linked_worktree',
    'a_directory_outside_any_repository_is_refused'
)
# The twelve Windows tests of the src-tauri host crate: 6 unit tests (the cfg(unix) bridge tests are not
# run), the 3 bridge::real_tests that launch the real daemon (r3) and 3 launch-failure redaction tests (M6-01).
$TauriTests = @(
    'forwards_frames_and_closing_a_window_leaves_the_daemon_running',
    'the_ui_cannot_shut_the_daemon_down_or_use_another_windows_connection',
    'a_daemon_that_stops_is_reported_as_a_closed_connection_not_a_task_state',
    'no_daemon_and_no_executable_is_daemon_unavailable',
    'a_daemon_executable_that_cannot_start_is_daemon_unavailable',
    'an_oversized_log_is_rotated_and_a_small_one_is_kept',
    'the_host_launches_the_real_daemon_and_drives_an_approval_to_delivery',
    'the_host_drives_reject_and_request_changes_through_the_real_daemon',
    'the_host_forwards_capability_and_token_gating_from_the_real_daemon',
    'a_launch_failure_quotes_only_the_redacted_end_of_the_log',
    'the_whole_message_is_redacted_and_the_daemon_token_is_masked_exactly',
    'a_daemon_that_exits_at_once_shows_the_redacted_end_of_its_log'
)

# Mode: 'test' (default), 'build', 'lint', 'exit', 'vitest', or 'lifecycle'.
# Expect (test): tests that must report ok.
# Passed, Ignored (test): exact counts. All are DERIVED from the source for Windows and
#   read from the cfg and #[test] attributes at the pinned source. They are not Mac counts and are
#   not measured before this run. A different measured count is INCONCLUSIVE, never edited here.
# RunningSequence (test): the exact 'running N tests' values, in order, one per binary.
#   A FILTERED cargo run prints no doc-test stage, so its sequence has one entry.
# Crates (build, lint): crates whose Compiling (build) or Checking (lint) line must appear.
# Baseline (lint): the diagnostics that are known. Any other diagnostic is recorded as NEW and
#   does not change the verdict (see Get-CheckVerdict).
# Artifact (any mode): a file that must exist after the gate. Its size and SHA-256 are recorded in
#   the file named by ArtifactRecord (for example daemon-binary.txt).
# Isolated: run with TEMP/TMP under $TestTmp and rustc on PATH.
# Exe, Directory (any mode): the program and working directory. The default is cargo in the
#   source checkout.
# Pattern (exit, test, lifecycle): a regex that must match standard output (Vite prints
#   'built in <time>'; the harness-free lifecycle test prints 'lifecycle: PASS').
# Files, Tests (vitest): the derived Vitest file and test counts. They are Mac-measured and
#   believed platform-neutral, and they are unmeasured on Windows.
# lifecycle: the daemon lifecycle test has no libtest harness. The verdict is exit 0, exactly one
#   'lifecycle: PASS' line and STEP 1 to STEP 8 in order. A libtest 'running' or 'test result' line
#   is a different shape and is INCONCLUSIVE.
# TimeoutSeconds: limit for the command (default 1800).
$UiDir = Join-Path $SourceDir 'apps\desktop'
$TauriDir = Join-Path $UiDir 'src-tauri'
$Checks = @(
    @{ Name = 'daemon-build'; Mode = 'build'; Expect = @(); Single = $false
       Crates = @('orchestrator-ipc', 'orchestrator-store', 'orchestrator-engine', 'orchestrator-daemon')
       Artifact = (Join-Path $BuildDir 'debug\orchestrator-daemon.exe'); ArtifactRecord = 'daemon-binary.txt'
       Arguments = @('build', '--locked', '-p', 'orchestrator-daemon') },
    @{ Name = 'ipc-tests'; Mode = 'test'; Expect = $IpcTests; Single = $false
       Passed = 46; Ignored = 0; Isolated = $true; TimeoutSeconds = 1800
       RunningSequence = @(46, 0)
       Arguments = @('test', '--locked', '-p', 'orchestrator-ipc') },
    @{ Name = 'daemon-tests'; Mode = 'test'; Expect = $DaemonTests; Single = $false
       Passed = 55; Ignored = 0; Isolated = $true; TimeoutSeconds = 1800
       RunningSequence = @(50, 4, 0, 1)
       Pattern = '(?m)^lifecycle: PASS[ \t]*\r?$'
       Arguments = @('test', '--locked', '-p', 'orchestrator-daemon') },
    @{ Name = 'daemon-lifecycle'; Mode = 'lifecycle'; Expect = @(); Single = $false
       Isolated = $true; TimeoutSeconds = 1800
       Pattern = '(?m)^lifecycle: PASS[ \t]*\r?$'
       Arguments = @('test', '--locked', '-p', 'orchestrator-daemon', '--test', 'lifecycle', '--',
           '--nocapture') },
    @{ Name = 'coordinator-review-tests'; Mode = 'test'; Expect = $CoordinatorReviewTests; Single = $true
       Passed = 4; Ignored = 0; Isolated = $true; TimeoutSeconds = 600
       RunningSequence = @(4)
       Arguments = @('test', '--locked', '-p', 'orchestrator-daemon', '--test', 'coordinator_review') },
    @{ Name = 'windows-transport-tests'; Mode = 'test'; Expect = $WindowsTransportTests; Single = $false
       Passed = 13; Ignored = 0; Isolated = $true; TimeoutSeconds = 1800
       RunningSequence = @(13)
       Arguments = @('test', '--locked', '-p', 'orchestrator-ipc', 'windows_transport') },
    @{ Name = 'git-discovery-tests'; Mode = 'test'; Expect = $GitDiscoveryTests; Single = $true
       Passed = 2; Ignored = 0; Isolated = $true; TimeoutSeconds = 900
       RunningSequence = @(2)
       Arguments = @('test', '--locked', '-p', 'orchestrator-git', '--test', 'repository_discovery') },
    @{ Name = 'git-lib-tests'; Mode = 'test'; Expect = @(); Single = $true
       Passed = 47; Ignored = 0; Isolated = $true; TimeoutSeconds = 1800
       RunningSequence = @(47)
       Arguments = @('test', '--locked', '-p', 'orchestrator-git', '--lib') },
    @{ Name = 'git-env-policy-tests'; Mode = 'test'; Expect = @(); Single = $true
       Passed = 11; Ignored = 0; Isolated = $true; TimeoutSeconds = 900
       RunningSequence = @(11)
       Arguments = @('test', '--locked', '-p', 'orchestrator-git', '--lib', 'env_policy') },
    @{ Name = 'core-contract-tests'; Mode = 'test'; Expect = @(); Single = $true
       Passed = 17; Ignored = 0; Isolated = $true; TimeoutSeconds = 900
       RunningSequence = @(17)
       Arguments = @('test', '--locked', '-p', 'orchestrator-core', '--lib', 'contract') },
    @{ Name = 'runtime-env-isolation-test'; Mode = 'test'; Expect = @(); Single = $true
       Passed = 1; Ignored = 0; Isolated = $true; TimeoutSeconds = 900
       RunningSequence = @(1)
       Arguments = @('test', '--locked', '-p', 'orchestrator-runtime', '--test', 'env_isolation') },
    @{ Name = 'store-tests'; Mode = 'test'; Expect = @(); Single = $true
       Passed = 32; Ignored = 0; Isolated = $true; TimeoutSeconds = 1800
       RunningSequence = @(32)
       Arguments = @('test', '--locked', '-p', 'orchestrator-store', '--lib') },
    @{ Name = 'diagnostics-tests'; Mode = 'test'; Expect = @(); Single = $false
       Passed = 28; Ignored = 0; Isolated = $true; TimeoutSeconds = 1800
       RunningSequence = @(0, 6, 7, 15, 0)
       Arguments = @('test', '--locked', '-p', 'orchestrator-diagnostics') },
    @{ Name = 'cli-portable-acceptance-tests'; Mode = 'test'; Expect = @(); Single = $true
       Passed = 2; Ignored = 0; Isolated = $true; TimeoutSeconds = 1800
       RunningSequence = @(2)
       Arguments = @('test', '--locked', '-p', 'orchestrator-cli', '--test', 'portable_acceptance') },
    @{ Name = 'cli-cleanup-safety-tests'; Mode = 'test'; Expect = @(); Single = $true
       Passed = 4; Ignored = 0; Isolated = $true; TimeoutSeconds = 1800
       RunningSequence = @(4)
       Arguments = @('test', '--locked', '-p', 'orchestrator-cli', '--test', 'cleanup_safety') },
    @{ Name = 'cli-engine-reconciliation-tests'; Mode = 'test'; Expect = @(); Single = $true
       Passed = 22; Ignored = 0; Isolated = $true; TimeoutSeconds = 1800
       RunningSequence = @(22)
       Arguments = @('test', '--locked', '-p', 'orchestrator-cli', '--test', 'engine_reconciliation_gate') },
    @{ Name = 'cli-crash-concurrency-tests'; Mode = 'test'; Expect = @(); Single = $true
       Passed = 6; Ignored = 0; Isolated = $true; TimeoutSeconds = 1800
       RunningSequence = @(6)
       Arguments = @('test', '--locked', '-p', 'orchestrator-cli', '--test', 'crash_concurrency') },
    @{ Name = 'clippy-scoped'; Mode = 'lint'; Expect = @(); Single = $false
       Crates = @('orchestrator-ipc', 'orchestrator-daemon')
       Baseline = '(?i)crates[\\/]orchestrator-runtime[\\/]src[\\/]supervisor\.rs:(?:409|432):\d+'
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
       Files = 15; Tests = 289
       Arguments = @('vitest', 'run') },
    @{ Name = 'tauri-build'; Mode = 'build'; Expect = @(); Single = $false
       Directory = $TauriDir; TimeoutSeconds = 3600
       Crates = @('agent-orch-desktop')
       Artifact = (Join-Path $BuildDir 'debug\agent-orch-desktop.exe'); ArtifactRecord = 'tauri-binary.txt'
       Arguments = @('build', '--locked') },
    @{ Name = 'tauri-tests'; Mode = 'test'; Expect = $TauriTests; Single = $false
       Directory = $TauriDir
       Passed = 12; Ignored = 0; Isolated = $true; TimeoutSeconds = 3600
       RunningSequence = @(12, 0, 0)
       Arguments = @('test', '--locked') },
    # Timing capture for the bridge::real_tests (the r4 run failed on a 30 s delivery.request timeout).
    # It re-runs only the three real-daemon tests with --nocapture, one at a time, so the [bridge-test]
    # lines are saved after the loop. With --nocapture a test's own output can sit between its name and
    # 'ok', so the per-name check is left to tauri-tests and this gate counts 3 passed.
    @{ Name = 'tauri-bridge-timing'; Mode = 'test'; Expect = @(); Single = $false
       Directory = $TauriDir
       Passed = 3; Ignored = 0; Isolated = $true; TimeoutSeconds = 3600
       Arguments = @('test', '--locked', 'bridge::real_tests', '--', '--nocapture', '--test-threads=1') },
    @{ Name = 'tauri-clippy-clean'; Mode = 'exit'; Expect = @(); Single = $false
       Directory = $TauriDir; TimeoutSeconds = 900
       Arguments = @('clean', '-p', 'agent-orch-desktop') },
    @{ Name = 'tauri-clippy'; Mode = 'lint'; Expect = @(); Single = $false
       Directory = $TauriDir; TimeoutSeconds = 3600
       Crates = @()
       CrateNames = @('agent_orch_desktop_lib', 'agent_orch_desktop')
       Baseline = '(?!)'
       BaselineMessage = 'no baseline: every diagnostic in the Tauri host is NEW'
       Arguments = @('clippy', '--locked', '--all-targets', '--verbose') },
    @{ Name = 'desktop-package-win'; Mode = 'exit'; Expect = @(); Single = $false
       Exe = $Npm; Directory = $UiDir; TimeoutSeconds = 3600
       Artifact = (Join-Path $BuildDir 'release\bundle\nsis\Agent Orchestrator_0.1.0_x64-setup.exe')
       ArtifactRecord = 'installer.txt'
       Arguments = @('run', 'package:win') },
    # The repo's own Node self-tests (TAP). 36 pass, 0 fail on macOS.
    @{ Name = 'node-self-tests'; Mode = 'exit'; Expect = @(); Single = $false
       Exe = $Node; Directory = $SourceDir; Isolated = $true; TimeoutSeconds = 900
       Pattern = '(?ms)^# pass 36\r?$.*^# fail 0\r?$'
       Arguments = @('--test', '--test-reporter=tap', 'scripts/**/*.test.mjs') },
    # The harness lists exactly 16 gates on Windows. This runbook runs those 16 and adds per-crate
    # gates (see the Check table), so its own gate count is higher.
    @{ Name = 'harness-list'; Mode = 'exit'; Expect = @(); Single = $false
       Exe = $Node; Directory = $SourceDir; TimeoutSeconds = 300
       Pattern = '\A(?:[^\r\n]+\r?\n){16}\s*\z'
       Arguments = @('scripts/ci/run-gates.mjs', '--list') }
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
        # Cargo prints one status line per package, so a package whose build script is the only
        # thing it compiles shows 'Compiling' and never 'Checking'. With --verbose every rustc
        # invocation is printed, so the crate names are required there instead.
        $CrateNameLines = [Collections.Generic.List[string]]::new()
        foreach ($Name in $Check.CrateNames) {
            $Pattern = '--crate-name[ \t]+' + [regex]::Escape($Name) + '(?=[ \t\r\n])'
            $Count = [regex]::Matches($Err, $Pattern).Count
            $CrateNameLines.Add("--crate-name ${Name}: $Count occurrence(s)")
            if ($Count -lt 1) {
                & $Raise 'INCONCLUSIVE' "No '--crate-name $Name' in the verbose output. The package was not linted."
            }
        }
        if ($Check.CrateNames) {
            [IO.File]::WriteAllLines((Join-Path $EvidenceDir "$($Check.Name).crate-names.txt"),
                [string[]]@($CrateNameLines))
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
            $State.Notes.Add("$BaselineSeen baseline diagnostic(s) and $NewCount NEW diagnostic(s) recorded in $($Check.Name).diagnostics.txt. The only baseline is the supervisor.rs single_match pair in orchestrator-runtime (a dependency of the daemon); the Tauri host has none. NEW diagnostics do not change the verdict; report each one verbatim.")
        } elseif ($Warnings -gt 0) {
            $State.Notes.Add("$Warnings warning line(s) noted. A build warning does not change the verdict.")
        }
    } elseif ($Check.Mode -ceq 'lifecycle') {
        # The daemon lifecycle test has no libtest harness: cargo counts 0 tests for it and it
        # prints STEP lines and 'lifecycle: PASS'. The verdict is the exit code plus those lines.
        $StdoutPath = Join-Path $EvidenceDir "$($Check.Name).stdout.txt"
        $Out = ''
        if (Test-Path -LiteralPath $StdoutPath) { $Out = [IO.File]::ReadAllText($StdoutPath) }
        $Lines = [regex]::Matches($Out, '(?m)^(?:STEP \d.*|lifecycle: .*|running \d+ tests?.*|test result: .*)$') |
            ForEach-Object { $_.Value.TrimEnd() }
        [IO.File]::WriteAllLines(
            (Join-Path $EvidenceDir "$($Check.Name).summary.txt"), [string[]]@($Lines))
        if ([regex]::Matches($Out, $Check.Pattern).Count -ne 1) {
            & $Raise 'INCONCLUSIVE' "Expected exactly one line matching $($Check.Pattern)."
        }
        $Steps = @([regex]::Matches($Out, '(?m)^STEP (\d+)\b') | ForEach-Object { $_.Groups[1].Value })
        if (($Steps -join ',') -cne '1,2,3,4,5,6,7,8') {
            & $Raise 'INCONCLUSIVE' "STEP lines were '$($Steps -join ',')'. Expected 1,2,3,4,5,6,7,8."
        }
        if ([regex]::IsMatch($Out, '(?m)^(?:running \d+ tests?|test result: )')) {
            & $Raise 'INCONCLUSIVE' 'A libtest running or test result line appeared. The lifecycle binary is expected to have no libtest harness.'
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
            # Vitest prints 'Test Files  15 passed (15)' and 'Tests  289 passed (289)'. A failed
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
        if ($Check.Pattern -and -not [regex]::IsMatch($Out, $Check.Pattern)) {
            & $Raise 'INCONCLUSIVE' "The output has no line matching $($Check.Pattern)."
        }
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
    if ($Check.Artifact) {
        $ArtifactName = $Check.Artifact.Substring($BuildDir.Length).TrimStart('\')
        $ArtifactPath = Join-Path $EvidenceDir $Check.ArtifactRecord
        if (Test-Path -LiteralPath $Check.Artifact -PathType Leaf) {
            $Item = Get-Item -LiteralPath $Check.Artifact
            $Hash = (Get-FileHash -LiteralPath $Check.Artifact -Algorithm SHA256).Hash.ToLowerInvariant()
            [IO.File]::WriteAllText($ArtifactPath,
                "artifact: $ArtifactName (relative to the target directory)`n" +
                "bytes: $($Item.Length)`nsha256: $Hash`n")
        } else {
            [IO.File]::WriteAllText($ArtifactPath,
                "artifact: $ArtifactName (relative to the target directory)`nmissing`n")
            & $Raise 'INCONCLUSIVE' "The artifact $ArtifactName does not exist after the gate."
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

# On any failed or inconclusive gate, keep what explains it, and never stop the run. The lifecycle test
# writes the daemon output to <base>\daemon.log and, on any failure, panics with a block that starts
# '--- lifecycle diagnostics ---' (base path and length, the daemon log tail, the agent breadcrumbs, the
# longest path, the worktrees, and the task, session and task lists). The coordinator_review tests
# (support/process.rs, r4) print, when they fail, the per-call timing between
# '--- per-call timing (client I/O timeout 30s) ---' and '--- end per-call timing ---' (one
# '<ms> ms  ok|ERR  <label>' line per call) and the last 60 lines of the daemon log between
# '--- daemon log tail (<path>) ---' and '--- end daemon log ---'. The src-tauri bridge::real_tests print
# '--- daemon log (<path>) ---' ... '--- end daemon log ---' (r4) or, at r5, '--- daemon log tail (<K> of <N>
# lines, <path>) ---' ... '--- end daemon log ---', plus '[bridge-test]' per-call lines, and the daemon logs
# request start and request end trace lines. All of them are captured.
# The base directory is a temporary directory that the test deletes, so the text on stdout or stderr is
# the carrier; every such block, the failed test names, and any daemon.log that survives under the
# isolated TEMP are copied too. Nothing is started or stopped.
function Save-LifecycleDiagnostics([string] $Name) {
    $Report = [Collections.Generic.List[string]]::new()
    foreach ($Stream in 'stdout', 'stderr') {
        $Path = Join-Path $EvidenceDir "$Name.$Stream.txt"
        if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { continue }
        $Text = [IO.File]::ReadAllText($Path)
        $Index = $Text.IndexOf('--- lifecycle diagnostics ---')
        $TimingBlocks = @([regex]::Matches($Text,
            '(?s)--- per-call timing \([^\r\n]*\) ---.*?--- end per-call timing ---') | ForEach-Object { $_.Value })
        $DaemonLogBlocks = @([regex]::Matches($Text,
            '(?s)--- daemon log(?: tail)? \([^\r\n]*\) ---.*?--- end daemon log ---') | ForEach-Object { $_.Value })
        # r5: the host tests print '[bridge-test] <window> <method> answered in <elapsed>' per call, and the
        # daemon (AGENT_ORCH_TRACE_REQUESTS=1) writes 'request start' and 'request end' lines to its log.
        $BridgeLines = @([regex]::Matches($Text, '(?m)^\[bridge-test\][^\r\n]*') | ForEach-Object { $_.Value.TrimEnd() })
        $TraceLines = @([regex]::Matches($Text,
            '(?m)^\[\d+\] request (?:start|end) id=[^\r\n]*') | ForEach-Object { $_.Value.TrimEnd() })
        $FailedTests = @([regex]::Matches($Text, '(?m)^test .+ \.\.\. FAILED\r?$') | ForEach-Object { $_.Value.TrimEnd() })
        $Report.Add("===== $Name.$Stream =====")
        if ($FailedTests.Count -gt 0) {
            $Report.Add("failed tests ($($FailedTests.Count) in this stream):")
            $Report.Add(($FailedTests -join "`n"))
        }
        if ($BridgeLines.Count -gt 0) {
            $Report.Add("[bridge-test] per-call timing lines ($($BridgeLines.Count) in this stream):")
            $Report.Add(($BridgeLines -join "`n"))
        }
        if ($TraceLines.Count -gt 0) {
            $Report.Add("daemon request trace lines ($($TraceLines.Count) in this stream):")
            $Report.Add(($TraceLines -join "`n"))
        }
        if ($Index -ge 0) {
            $Report.Add('diagnostics block (from the marker to the end of the stream):')
            $Report.Add($Text.Substring($Index))
        }
        foreach ($Block in $TimingBlocks) {
            $Report.Add("per-call timing block printed by a failing test ($($TimingBlocks.Count) in this stream):")
            $Report.Add($Block)
        }
        foreach ($Block in $DaemonLogBlocks) {
            $Report.Add("daemon log block printed by a failing test ($($DaemonLogBlocks.Count) in this stream):")
            $Report.Add($Block)
        }
        if ($Index -lt 0 -and $TimingBlocks.Count -eq 0 -and $DaemonLogBlocks.Count -eq 0 -and $BridgeLines.Count -eq 0 -and $TraceLines.Count -eq 0) {
            $Tail = @($Text -split "\r?\n") | Select-Object -Last 200
            $Report.Add('NO diagnostics marker, no per-call timing, no bridge-test line, no request trace and no daemon log block found. Last 200 lines of the stream:')
            $Report.Add(($Tail -join "`n"))
        } else {
            $Tail = @($Text -split "\r?\n") | Select-Object -Last 50
            $Report.Add('Last 50 lines of the stream:')
            $Report.Add(($Tail -join "`n"))
        }
    }
    [IO.File]::WriteAllText((Join-Path $EvidenceDir "$Name.diagnostics.txt"),
        (Protect-Text ($Report -join "`n")) + "`n")
    $Logs = [Collections.Generic.List[string]]::new()
    try {
        if (-not (Test-Path -LiteralPath (Join-Path $TestTmp $Name) -PathType Container)) {
            throw 'this gate has no isolated TEMP'
        }
        $Found = @(Get-ChildItem -LiteralPath (Join-Path $TestTmp $Name) -Recurse -Filter 'daemon.log*' `
            -File -ErrorAction Stop)
        foreach ($Log in $Found) {
            $Logs.Add("===== $($Log.Name) ($($Log.Length) bytes; the last 1 MiB follows) =====")
            $Stream = [IO.File]::Open($Log.FullName, 'Open', 'Read', 'ReadWrite')
            try {
                $Start = [Math]::Max(0, $Stream.Length - 1048576)
                $null = $Stream.Seek($Start, 'Begin')
                $Buffer = [byte[]]::new([int]($Stream.Length - $Start))
                $null = $Stream.Read($Buffer, 0, $Buffer.Length)
                $Logs.Add([Text.UTF8Encoding]::new($false).GetString($Buffer))
            } finally { $Stream.Dispose() }
        }
        if ($Found.Count -eq 0) { $Logs.Add('No daemon.log survives under the isolated TEMP (the test deletes its base directory).') }
    } catch {
        $Logs.Add("Could not search for daemon.log: $($_.Exception.Message)")
    }
    [IO.File]::WriteAllText((Join-Path $EvidenceDir "$Name.daemon-log.txt"),
        (Protect-Text ($Logs -join "`n")) + "`n")
}

# Every gate runs, in order, whatever an earlier gate did. A FAIL or INCONCLUSIVE verdict is recorded
# (with its diagnostics) and the loop continues; the only thing that stops the run is a process of this
# run that may still be alive, because the rest of the run would then be unsafe. Step 5 publishes the
# evidence of a failed run too.
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
    $DaemonGates = @('daemon-lifecycle', 'daemon-tests', 'coordinator-review-tests', 'git-discovery-tests', 'tauri-tests', 'tauri-bridge-timing')
    if (($DaemonGates -ccontains $Check.Name) -and $Verdict.status -cne 'PASS') {
        Save-LifecycleDiagnostics $Check.Name
    }
    if ($Check.Name -ceq 'tauri-bridge-timing') {
        # Keep the [bridge-test] timing lines of the three real-daemon tests, pass or fail.
        $BridgeLines = [Collections.Generic.List[string]]::new()
        foreach ($Stream in @("$($Check.Name).stdout.txt", "$($Check.Name).stderr.txt")) {
            $StreamPath = Join-Path $EvidenceDir $Stream
            if (Test-Path -LiteralPath $StreamPath) {
                foreach ($Line in [IO.File]::ReadAllLines($StreamPath)) {
                    if ($Line -match '\[bridge-test\]|request (?:start|end)') { $BridgeLines.Add($Line) }
                }
            }
        }
        if ($BridgeLines.Count -eq 0) { $BridgeLines.Add('No [bridge-test] line was captured.') }
        [IO.File]::WriteAllText((Join-Path $EvidenceDir 'tauri-bridge-timing.bridge-lines.txt'),
            (Protect-Text ($BridgeLines -join "`n")) + "`n")
    }
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
    'unproven-tauri-webview-click-path' = 'NOT RUN. The webview and the React click path were not driven: no window, no webview, no daemon_open, daemon_send or daemon_close call as the webview makes it, and nobody clicked through the approval flow. The bridge::real_tests drive the Rust Bridge directly, with no window.'
    'unproven-e2e-windows' = 'NOT RUN. The desktop e2e suite (npm run test:e2e) uses a Unix domain socket and cannot run on Windows. No Windows e2e number exists.'
    'unproven-gui-install-launch' = 'NOT RUN. The NSIS installer is built by desktop-package-win but never installed or launched by this harness. No window shows Connected and no sidecar daemon is started by the app.'
    'unproven-gui-task-survives-close' = 'NOT RUN. A real daemon-owned task surviving a window close and reopen was not driven. The headless daemon-lifecycle gate covers the daemon side with a fake agent and the UI-side relay, not the window.'
    'unproven-gui-event-reconnect' = 'NOT RUN. The window event stream reconnecting and resuming at the saved cursor and epoch, and drafts surviving a reopen, were not driven.'
    'unproven-stale-token-relaunch-timing' = 'UNMEASURED. About 0.3 s per relay attempt, about 0.6 s before relaunch, then 200 ms retries for 15 s, and 10 s relay handshake and write timeouts, are derived from code. Nothing was timed on Windows.'
    'unproven-daemon-idle-behaviour' = 'NOT RUN. The 60 s true-idle exit, a live subscription keeping the daemon alive and the idle clock restarting were not observed on the app. Only the idle_subscription_tests in ipc-tests cover the logic.'
    'unproven-daemon-kill-and-sidecar-failure' = 'NOT RUN. Killing the daemon pid and a sidecar that fails to start were not exercised. Both stop or break a process, which this runbook never does.'
    'unproven-data-dir-acl' = 'NOT RUN by this script (M6-01 R11). The ACLs of the Windows data directory, store.sqlite, daemon.token and daemon.log were not inspected with icacls. Do the by-hand check and record it in manual-p12.txt.'
    'unproven-launch-failure-redaction' = 'NOT RUN by this script (M6-01). The desktop host quoting only the redacted tail of daemon.log, with the daemon token masked, when the daemon fails to launch was not checked in a real window. The tauri-tests unit tests cover the logic. Do the by-hand check and record it in manual-p12.txt.'
    'unproven-objective-64kib-r12' = 'NOT RUN by this script (M6-01 R12). A run with an objective near 64 KiB was not submitted on Windows, where the 32,767-character command-line limit may make the agent spawn fail. Do the by-hand check and record it in manual-p12.txt.'
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

# The one derived row. The host-level bridge is provable only by the two gates that start the real
# daemon: coordinator-review-tests (the daemon side) and tauri-tests (the host's Bridge against it).
# It is PASS only when both gates are PASS in this run, and UNPROVEN otherwise. It is never a
# webview, window or installer proof, and it never raises the overall verdict.
$BridgeRow = 'tauri-native-bridge-host-level'
$BridgeGateStatus = [ordered]@{}
foreach ($Gate in @('coordinator-review-tests', 'tauri-tests')) {
    $Found = @($Verdicts | Where-Object { $_.name -ceq $Gate })
    $BridgeGateStatus[$Gate] = if ($Found.Count -eq 1) { $Found[0].status } else { 'MISSING' }
}
$BridgeProven = @($BridgeGateStatus.Values | Where-Object { $_ -ceq 'PASS' }).Count -eq 2
$BridgeGateText = ($BridgeGateStatus.Keys | ForEach-Object { "$_ $($BridgeGateStatus[$_])" }) -join ', '
$BridgeText = if ($BridgeProven) {
    "Host level only, no window and no webview: the real orchestrator-daemon.exe served the coordinator and review methods over the Windows named pipe (coordinator-review-tests) and the Tauri host Bridge launched it and drove an approval to delivery, a reject, a request-changes and the capability and token gating (tauri-tests). Both gates PASS in this run ($BridgeGateText). Not a webview, installer or click-path proof."
} else {
    "UNPROVEN: the host-level bridge is provable only when coordinator-review-tests and tauri-tests both PASS natively in this run. Gate statuses: $BridgeGateText."
}
$BridgeStatusValue = if ($BridgeProven) { 'PASS' } else { 'UNPROVEN' }
[IO.File]::WriteAllText((Join-Path $EvidenceDir "$BridgeRow.txt"), "status: $BridgeStatusValue`n$BridgeText`n")
$Verdicts += [PSCustomObject]@{
    name = $BridgeRow; status = $BridgeStatusValue; exit_code = $null; timed_out = $false
    duration_seconds = 0; running = ''; passed = $null; failed = $null; ignored = $null
    notes = "$BridgeText See $BridgeRow.txt."
}

Assert-Head 'after the checks'
$HeadAfter = Invoke-Git 'source-head-after' @('rev-parse', 'HEAD')
if ($HeadAfter.Code -ne 0 -or $HeadAfter.Out.Count -ne 1 -or $HeadAfter.Out[0] -cne $SourceSha) {
    throw "Recorded HEAD after the checks is not the source commit. Preserve $RunRoot and report it."
}
$After = Invoke-Git 'source-status-after' @('status', '--porcelain=v1', '--untracked-files=all')
# The plain forms and the diffs are evidence only. They show what changed if the tree is not clean
# (the Priority 11 first run listed three modified files and kept no diff).
$null = Invoke-Git 'source-status-after-plain' @('status')
$DiffStat = Invoke-Git 'source-diff-after-stat' @('diff', '--stat')
$null = Invoke-Git 'source-diff-after' @('diff')
$null = Invoke-Git 'source-diff-after-ignore-cr' @('diff', '--ignore-cr-at-eol')
$DiffCheck = Invoke-Git 'diff-check' @('diff', '--check')
$GitAttrAfter = Test-GitAttributes 'after'
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
  `single_match` warnings at `supervisor.rs` lines 409 and 432 in `orchestrator-runtime`, a dependency
  that the daemon scope also lints) or `NEW`. Every `NEW` diagnostic is written verbatim to
  `clippy-scoped.diagnostics.txt` and named in the notes, and the operator reports each one verbatim.
  A non-zero exit (a compile error, which clippy also reports as an `error` diagnostic) is `FAIL`.
- `INCONCLUSIVE`, desktop exit gates: `desktop-build` prints no `built in` line. The gates
  `desktop-npm-ci` and `desktop-tsc` are `PASS` on exit code zero and `FAIL` otherwise.
- `INCONCLUSIVE`, `desktop-vitest`: no `Test Files` or `Tests` summary line, a line that does not
  parse, or a passed count or total that is not 15 files and 289 tests. A skipped or `todo` test is
  `INCONCLUSIVE`. A failed test or file is `FAIL`. The numbers are Mac-measured and unmeasured on
  Windows.
- `INCONCLUSIVE`, `daemon-lifecycle`: the output does not hold exactly one `lifecycle: PASS` line,
  the `STEP` lines are not `1` to `8` in order, or a libtest `running` or `test result` line appears
  (the binary has no libtest harness, so that is a different shape). Exit code non-zero is `FAIL`.
  The gate is **not** judged by a libtest count. When `daemon-lifecycle`, `daemon-tests`,
  `coordinator-review-tests`, `git-discovery-tests` or `tauri-tests` is not `PASS`, the harness writes `<gate>.diagnostics.txt`
  (the failed test names, the `--- lifecycle diagnostics ---` block that the lifecycle test prints on
  failure, every `--- per-call timing (…) ---` block and every `--- daemon log tail (<path>) ---` or
  `--- daemon log (<path>) ---` block that a failing `coordinator_review` or `bridge::real_tests` test
  prints, every `[bridge-test]` per-call timing line, every daemon `request start` and `request end`
  trace line, and the last lines of stdout and stderr, or the last 200 lines of each stream if no marker is
  present) and `<gate>.daemon-log.txt` (any `daemon.log` that survives under the isolated TEMP; the
  tests normally delete their base directory, so the file may say that none survives). Both files go
  into the evidence, and you quote the diagnostics block in the report.
- `INCONCLUSIVE`, `git-discovery-tests`: more than one test binary, a `running` value other than `2`,
  a passed count other than 2, an ignored count other than 0, or a missing `ok` line for either name.
  A failed test or a non-zero exit is `FAIL`. A timeout (900 s) is `FAIL`.
- `INCONCLUSIVE`, `coordinator-review-tests`: more than one test binary, a `running` value other than
  `4`, a passed count other than 4, an ignored count other than 0, or a missing `ok` line for any of
  the four names. A failed test or a non-zero exit is `FAIL`. A timeout (600 s) is `FAIL`: say in the
  report whether the output ends at `Compiling` lines (a build still running) or shows started tests (a
  hang in a test or in the daemon).
- `INCONCLUSIVE`, `tauri-tests`: a `running` sequence other than `12,0,0`, a passed count other than 12,
  or a missing `ok` line for any of the twelve names (the three `bridge::real_tests` names and the three
  launch-failure redaction names included).
  A failed test or a non-zero exit is `FAIL`. A timeout (3600 s) is `FAIL`; say whether the nested
  daemon build was still running.
- `INCONCLUSIVE`, the per-crate gates (`git-lib-tests` 47, `git-env-policy-tests` 11, `core-contract-tests` 17,
  `runtime-env-isolation-test` 1, `store-tests` 32, `diagnostics-tests` 28 with sequence `0,6,7,15,0`,
  `cli-portable-acceptance-tests` 2, `cli-cleanup-safety-tests` 4, `cli-engine-reconciliation-tests` 22,
  `cli-crash-concurrency-tests` 6): a `running` sequence, passed count or ignored count other than the derived
  one, or more than one test binary for the single-binary gates. A failed test or a non-zero exit is `FAIL`.
- `tauri-bridge-timing`: re-runs the three `bridge::real_tests` with `--nocapture` and one test thread, so
  the `[bridge-test]` timing lines print for passing tests too. `INCONCLUSIVE` unless 3 tests pass. No `ok`
  name check (with `--nocapture` a test's output can sit between its name and `ok`). The harness writes
  `tauri-bridge-timing.bridge-lines.txt` (the `[bridge-test]` lines and the daemon request-trace lines,
  redacted) whatever the verdict. It never changes `tauri-tests`.
- `node-self-tests`: `node --test --test-reporter=tap scripts/**/*.test.mjs`. `FAIL` on a non-zero exit.
  `INCONCLUSIVE` unless the TAP summary shows `# pass 36` and `# fail 0`.
- `harness-list`: `node scripts/ci/run-gates.mjs --list`. `FAIL` on a non-zero exit. `INCONCLUSIVE` unless it
  prints exactly 16 gate lines (the harness gate count on Windows).
- `tauri-native-bridge-host-level`: derived. `PASS` only when `coordinator-review-tests` and
  `tauri-tests` are both `PASS`; otherwise `UNPROVEN`. It never changes the overall verdict, and a
  `PASS` here is a host-level result, not a webview proof.
- `FAIL` or `INCONCLUSIVE`, `desktop-package-win`: a non-zero exit is `FAIL`. Exit 0 with no
  installer at `release\bundle\nsis\Agent Orchestrator_0.1.0_x64-setup.exe` in the target directory is
  `INCONCLUSIVE`. The installer is built, never installed.
- `INCONCLUSIVE`, `tauri-build`: no `Compiling agent-orch-desktop` or `Finished` line, or the
  `debug\agent-orch-desktop.exe` binary is absent after an exit-0 build.
- `tauri-clippy-clean`: `cargo clean -p agent-orch-desktop` in `apps\desktop\src-tauri`, so that the next gate
  really compiles the host crate. A non-zero exit is `FAIL`. It has no other check.
- `tauri-clippy`: like `clippy-scoped`, but with **no baseline**: every diagnostic is `NEW`, recorded
  verbatim in `tauri-clippy.diagnostics.txt`, and never changes the verdict. A non-zero exit is `FAIL`.
  It runs `cargo clippy --locked --all-targets --verbose` and requires a `Finished` line and a
  `--crate-name` argument for both `agent_orch_desktop_lib` and `agent_orch_desktop` in the output
  (cargo prints `Compiling`, never `Checking`, for the host crate because of its build script, so a
  `Checking agent-orch-desktop` line is not required). A missing one is `INCONCLUSIVE`; the names that
  were found are in `tauri-clippy.crate-names.txt`.
- Harness checks (not gates): `gitattributes-check` is `FAIL` if `git check-attr text eol` does not
  report `text: set` and `eol: lf` for each pinned file, or if a pinned file holds a carriage return
  in the working tree, before the gates or after them. `source-status-after` is `FAIL` if
  `git status --porcelain` is not empty after the gates (a rewritten tracked file, such as the
  `Cargo.toml` that `tauri build` rewrites, or an untracked file). `source-diff-after` is `FAIL` if
  `git diff --check` fails or `git diff --stat` is not empty. The plain `git status`, `git diff --stat`,
  `git diff` and `git diff --ignore-cr-at-eol` outputs are always kept, so a dirty tree can be
  explained from the evidence.
- Rows `unproven-second-user`, `unproven-remote-client`, `unproven-slow-reader-64kib`,
  `unproven-tauri-webview-click-path`, `unproven-e2e-windows`, `unproven-gui-install-launch`,
  `unproven-gui-task-survives-close`, `unproven-gui-event-reconnect`,
  `unproven-stale-token-relaunch-timing`, `unproven-daemon-idle-behaviour`, and
  `unproven-daemon-kill-and-sidecar-failure`, `unproven-data-dir-acl`, `unproven-launch-failure-redaction`
  and `unproven-objective-64kib-r12` are `UNPROVEN` by construction (the e2e, GUI, idle and
  kill rows are also NOT RUN, the timing row is UNMEASURED, and the last three are by-hand checks whose
  result is recorded in `manual-p12.txt`). `UNPROVEN` never changes the overall
  verdict, and the report lists every `UNPROVEN` row.
- `PASS`: exit code zero, every required line and test present, zero tests failed, no survivor, and
  the counts match the derived counts.

Derived counts are never edited. If the measured count differs, the verdict is `INCONCLUSIVE`, the
measured number is reported exactly as printed, and the coordinator decides what it means. Never
change `Passed`, `Ignored`, `RunningSequence`, `Files`, or `Tests` to match what you saw.

Per-gate notes on how the counts print (these are the reasons a correct run can still be
`INCONCLUSIVE`, so read them before you report one):

- `ipc-tests` is unfiltered, so cargo prints the unit binary and then the doc-test stage:
  `running 46 tests`, then `running 0 tests`. The sequence is `46,0`. The Mac also totals 46, with
  a different mix of tests (the Unix-only ones instead of the 13 named-pipe ones).
- `windows-transport-tests` is filtered (`windows_transport`). **A filtered cargo run prints no
  doc-test stage**, so the sequence is the single value `13`, and the result line ends in `33 filtered
  out` (derived, 46 - 13). The 10r2 run measured this shape at `e555c2c9…` (`13 passed … 25 filtered
  out`).
- `git-discovery-tests` runs only the `repository_discovery` binary of `orchestrator-git`: the single
  value `2`, and the result line `2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`. A
  single-binary cargo run prints no doc-test stage. It is new in r5, and measured on the Mac only (as part of
  the workspace run), so the Windows behaviour is unmeasured.
- `daemon-tests` runs the unit binary (50 on Windows), `coordinator_review` (4), `tests/daemon.rs`
  (empty on Windows, `#![cfg(unix)]`), the `diagnostics_log` test binary (1) and the harness-free `lifecycle` binary, which prints no
  `running` line and adds nothing to the sequence. The sequence is `50,4,0,1` and 55 tests pass. `42,0,0`
  was measured natively at `e555c2c9…`, `3df3b583…` and `6b9cf7cd…`; the `4` first passed natively at `1bb24bf9…` (r4, `42,4,0` with `lifecycle: PASS`). At `efa5e8af…`
  (r3) the native run printed `42,4` and exited 101 with 2 of the 4 `coordinator_review` tests passing.
  The 50 and the trailing `1` are new at this commit (M6-01, M6-02, M6-03) and unmeasured on Windows.
  `lifecycle: PASS` must also appear in the stdout.
- `coordinator-review-tests` runs only the `coordinator_review` binary: the single value `4`, and the
  result line `4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`. A filtered or single-binary
  cargo run prints no doc-test stage. The Mac measured the same line (4 passed in 2.61 s), so only the
  Windows behaviour is unmeasured.
- `daemon-lifecycle` runs only the `lifecycle` binary with `--nocapture`. Cargo prints
  `Running tests\lifecycle.rs` on stderr and the binary prints the `STEP` lines and
  `lifecycle: PASS` on stdout. There is no count to match.
- `tauri-tests` runs the `agent_orch_desktop_lib` unit tests, the `main.rs` binary and the doc-tests:
  sequence `12,0,0`, derived from the Mac layout minus the `cfg(unix)` bridge test (6 unit tests,
  the 3 `bridge::real_tests` and the 3 launch-failure redaction tests on Windows). The doc-test and binary stages are carried over from the
  Mac layout and are unmeasured on Windows. If `AGENT_ORCH_DAEMON_BIN` or `AGENT_ORCH_SEED_BIN` is set
  in your environment, the nested build is skipped; the harness clears both variables in Step 1.
- The ipc, daemon and `src-tauri` counts are **derived and unmeasured on this commit**. A different
  measured value is reported as printed, with the full `running` and `test result` lines.

A compile failure is not a reason to stop: the gate is `FAIL` (exit 101), the harness runs the
remaining gates, and Step 5 still publishes. See "If a build fails (compile-failure triage)".

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
foreach ($Name in (@($Checks | ForEach-Object { $_.Name }) + @($UnprovenDefinitions.Keys) + @($BridgeRow))) {
    if (@($Verdicts | Where-Object { $_.name -ceq $Name }).Count -ne 1) {
        $Overall = 'FAIL'; $Extra.Add("No single verdict for $Name.")
    }
}
# UNPROVEN rows never raise the verdict. They are listed so a PASS cannot be read as covering them.
# The derived row tauri-native-bridge-host-level is listed there too whenever it is UNPROVEN.
$UnprovenRows = @($Verdicts | Where-Object { $_.status -ceq 'UNPROVEN' } | ForEach-Object { $_.name })
$UnprovenText = if ($UnprovenRows.Count -gt 0) { $UnprovenRows -join ', ' } else { 'none' }
if ($UnprovenRows.Count -gt 0) {
    $Extra.Add("UNPROVEN, not covered by this result: $UnprovenText.")
}
# A failed gate never stopped the run: every gate ran. Name each one that is not PASS and where its
# explanation is, so the report cannot be read as stopping at the first failure.
$GateIssues = @($Verdicts | Where-Object { @('FAIL', 'INCONCLUSIVE') -ccontains $_.status } |
    ForEach-Object { "$($_.name) $($_.status)" })
if ($GateIssues.Count -gt 0) {
    $Extra.Add("Gates that are not PASS (every gate still ran): $($GateIssues -join ', '). For a real-daemon gate see <gate>.diagnostics.txt (failed tests, per-call timing, daemon log tail) and <gate>.daemon-log.txt.")
}
if ($After.Code -ne 0 -or $After.Out.Count -ne 0) {
    $Overall = 'FAIL'; $Extra.Add('Source status after the run is not clean (see source-status-after.stdout.txt, source-status-after-plain.stdout.txt and source-diff-after.stdout.txt).')
}
if ($DiffStat.Code -ne 0 -or $DiffStat.Out.Count -ne 0) {
    $Overall = 'FAIL'; $Extra.Add('git diff --stat after the run is not empty (see source-diff-after-stat.stdout.txt).')
}
if ($DiffCheck.Code -ne 0) { $Overall = 'FAIL'; $Extra.Add('git diff --check failed.') }
if ($GitAttrBefore.Status -cne 'PASS') {
    $Overall = 'FAIL'; $Extra.Add("gitattributes-before: $($GitAttrBefore.Notes)")
}
if ($GitAttrAfter.Status -cne 'PASS') {
    $Overall = 'FAIL'; $Extra.Add("gitattributes-after: $($GitAttrAfter.Notes)")
}

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
# Priority 12 Windows acceptance of $SourceSha (M6-01 security hardening, M6-02 and M6-03 diagnostics and reliability, on top of the M5-05 Tauri desktop host, IPC relay and native lifecycle proof)

This run covers the Priority 12 candidate: the M6-01 security and redaction changes (read-only autonomy
refusal, oversized and NUL contract fields, redacted launch-failure explanations, environment policy), the
M6-02 and M6-03 daemon diagnostics and listing reliability changes, and the M5-05r5 desktop host with its IPC
relay and timeouts, the live-subscription idle logic, the one-spawn Git repository
discovery, the daemon built natively on Windows with its headless
lifecycle proof (STEP 1 to 8), the real-daemon coordinator and review process tests (coordinator_review), the Tauri host
built, unit-tested and linted natively together with its real-daemon bridge tests (120 s per-call deadline,
per-call timing, daemon log tail and request trace), a separate timing capture of the three
bridge::real_tests (tauri-bridge-timing), the desktop install,
type-check, build and unit tests, and whether the NSIS installer builds (npm run package:win). The coordinator
decides acceptance. The expected counts are derived from the cfg and test attributes at the pinned source
for Windows (IPC 46 with sequence 46,0, windows_transport 13, daemon 50 unit tests plus 4 coordinator_review tests
plus 1 diagnostics_log test = 55 with the running sequence 50,4,0,1, git 47 and env_policy 11 and repository_discovery 2,
core contract 17, runtime env_isolation 1, store 32, diagnostics 28, cli 2/4/22/6, src-tauri 12 with the sequence
12,0,0, lifecycle prints lifecycle: PASS with no libtest count, coordinator-review-tests 4, node self-tests 36,
harness list 16) and from the Mac Vitest run (15 files, 289 tests). They are
derived and unmeasured for this commit. They are not the Mac counts, and they are not the Priority 10r2
native counts (a different tree). Counts are reported as measured, not matched. No macOS or review result is
Windows evidence. The row tauri-native-bridge-host-level is PASS only when coordinator-review-tests and
tauri-tests both pass natively, and then it is a host-level result (the Rust Bridge against the real daemon
over the named pipe), not a webview, window or installer result. This run is not a native probe, not a
second-user, genuinely remote-client, slow-reader or Windows e2e claim, not a webview or click-path claim, not
a GUI install, launch, close and reopen, reconnect, idle or relaunch-timing claim. The data-directory ACL, the
redacted launch failure and the ~64 KiB objective (R12) are by-hand checks recorded in manual-p12.txt.

- Run ID: $RunId
- UTC date: $([DateTime]::UtcNow.ToString('o'))
- Source branch: $SourceBranch
- Branch tip at clone time: $BranchTip
- Source commit: $SourceSha
- Source tree and only parent (verified): source-tree.stdout.txt, source-parent.stdout.txt
- HEAD matched the source commit before the checks and after them: yes
  (source-head-before.stdout.txt, source-head-after.stdout.txt)
- Git top-level path form (plain drive-letter form required): source-toplevel.stdout.txt
- Baseline ancestry (e555c2c9), commit count, the thirty-nine pinned commits, the linear range, and the
  changed-file set (136 files): base-ancestry, range-count, range-merges, range-commits, range-files
  (consistency gates, not patch-equivalence proof)
- Runbook identity (operator-declared; exact bytes pinned by blob id, size, and SHA-256; not proof that
  the pasted commands match): runbook-identity.txt
- Overall result: $Overall $($Extra -join ' ')
- UNPROVEN rows (never covered by an overall PASS): $UnprovenText
- Toolchain: see git-version.stdout.txt, rustc-version.stdout.txt,
  cargo-version.stdout.txt, clippy-version.stdout.txt, node-version.stdout.txt,
  npm-version.stdout.txt, git-autocrlf.stdout.txt (exit code in git-autocrlf.exit.txt)
- Artifact records (size and SHA-256): daemon-binary.txt (dev-profile daemon), tauri-binary.txt
  (dev-profile Tauri host) and installer.txt (the NSIS installer, built and never installed)
- Clippy diagnostics, each classified BASELINE or NEW (NEW does not change the verdict):
  clippy-scoped.diagnostics.txt and tauri-clippy.diagnostics.txt. The Tauri host lint must name both
  crates (tauri-clippy.crate-names.txt).
- Lifecycle proof: daemon-lifecycle.summary.txt (the STEP lines and the lifecycle: PASS line). On a
  failure also daemon-lifecycle.diagnostics.txt and daemon-lifecycle.daemon-log.txt (and the same two
  files for daemon-tests, coordinator-review-tests and tauri-tests): the failed test names, the
  diagnostics block that the test prints, the per-call timing block (--- per-call timing ... ---) and
  every daemon log or daemon log tail block that a failing real-daemon test prints, and any surviving
  daemon.log. A failed gate never stops the run: every later gate still ran and is in the table.
- Real-daemon tests: coordinator-review-tests.summary.txt (4 tests) and tauri-tests.summary.txt (12 tests,
  3 of them bridge::real_tests). tauri-bridge-timing.bridge-lines.txt holds the [bridge-test] per-call timing
  lines of the three bridge::real_tests (run with --nocapture, one thread). The derived row
  tauri-native-bridge-host-level.txt records whether both
  passed. It is host level only.
- By-hand checks (Priority 12): manual-p12.txt (data-directory ACL, redacted launch failure, ~64 KiB
  objective). Operator observation only; it does not change a verdict. Missing means NOT RUN.
- Harness checks (not gates): gitattributes-before.summary.txt and gitattributes-after.summary.txt
  (LF pins), source-status-after.stdout.txt and source-status-after-plain.stdout.txt (git status),
  source-diff-after-stat.stdout.txt (git diff --stat), source-diff-after.stdout.txt and
  source-diff-after-ignore-cr.stdout.txt (git diff), diff-check.stdout.txt. gitattributes before:
  $($GitAttrBefore.Status). gitattributes after: $($GitAttrAfter.Status). git status after the run
  (porcelain lines): $($After.Out.Count). git diff --stat after the run (lines): $($DiffStat.Out.Count).
- UNPROVEN and NOT RUN rows: one unproven-*.txt file each, and tauri-native-bridge-host-level.txt. No
  native probe ran. No window was driven and no installer was installed. No daemon was started except by
  the tests themselves (lifecycle, coordinator_review and bridge::real_tests).
- Manual lifecycle notes, if an operator performed any step: manual-lifecycle.txt (operator
  observation only; it does not change a verdict)
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
  timeout_tests.rs, the IPC wrong-token relay test, the whole daemon executor_tests.rs module,
  tests/daemon.rs, three daemon cancellation unit tests, the daemon symlink test, and one src-tauri
  bridge test. Their macOS results are separate and are not Windows
  evidence. The scheduler and executor have no native Windows test in this run.
- NOT RUN: the desktop e2e suite (Unix domain socket only). No Windows e2e number exists.
- UNPROVEN or NOT RUN, whatever the overall result: denial of a second, different local user;
  rejection of a genuinely remote client; a single daemon-to-client write above 64 KiB to a slow
  reader; the webview and click path of the Tauri host (the host-level bridge is covered only by the
  derived row tauri-native-bridge-host-level); installing and launching the app; a daemon-owned task
  surviving a window close and reopen; the window event reconnect; the stale-token relaunch timing (UNMEASURED);
  the 60 s daemon idle behaviour on the app; killing the daemon or breaking the sidecar; and the three
  by-hand Priority 12 checks (data-directory ACL, redacted launch failure, a ~64 KiB objective) unless
  manual-p12.txt records them.
- The daemon built by daemon-build was never started by the harness. Server identity verification by
  the client and the token owner are not validated on Windows. The optional
  manual GUI checklist (sections 5.5 and 5.6 of the in-repo runbook) is not part of this result unless
  manual-lifecycle.txt records it.
- The Vitest counts (15 files, 289 tests) are Mac-measured. They are believed platform-neutral
  (jsdom with an in-memory fake daemon) and are unmeasured on Windows.
- No rustfmt gate and no workspace-wide test run ran (the Windows workspace total is not a gate). Of the other crates, only the per-crate gates in the table ran.
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
matches the full source commit, both gitattributes checks pass, both source status checks are empty,
`git diff --stat` is empty, and the diff check exits zero.
PASS here never covers an UNPROVEN or NOT RUN row: the report lists them. A PASS shows that the 16
gates passed on this host. It does not show Windows coverage for any scenario that no test step
ran. The harness starts no daemon itself; the daemon runs only inside the lifecycle, coordinator_review
and bridge::real_tests tests.

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
that you push is `evidence/priority-12-windows-acceptance-136646d-<RunId>`. It holds evidence files
only. It is never `main`, `orch/…`, or `docs/…`.

```powershell
$EvidenceBranch = "evidence/priority-12-windows-acceptance-$Short-$RunId"
if ($EvidenceBranch -cnotmatch '^evidence/priority-12-windows-acceptance-136646d-\d{8}T\d{6}Z-[0-9a-f]{32}$') {
    throw 'The evidence branch name is not in the evidence/ namespace. Do not push.'
}
$EvidenceWorktree = Join-Path $env:TEMP "p12ev-$Short-$($RunId.Substring($RunId.Length - 8))"
$EvidencePath = "docs/evidence/priority-12-windows-acceptance/$Short-$RunId"

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
    'windows-version.txt', 'powershell-version.txt', 'daemon-binary.txt', 'tauri-binary.txt',
    'installer.txt', 'clippy-scoped.diagnostics.txt', 'tauri-clippy.diagnostics.txt',
    'daemon-lifecycle.summary.txt', 'gitattributes-before.summary.txt',
    'gitattributes-after.summary.txt')
foreach ($Name in $UnprovenDefinitions.Keys) { $Required += "$Name.txt" }
$Required += "$BridgeRow.txt"
foreach ($Step in 'clone', 'branch-tip', 'candidate-containment', 'base-ancestry',
    'source-parent', 'source-tree', 'range-count', 'range-merges', 'range-commits', 'range-files',
    'runbook-fetch', 'runbook-type', 'runbook-parent', 'runbook-files',
    'runbook-blob', 'switch', 'config-longpaths', 'config-longpaths-check',
    'source-head-before', 'source-toplevel', 'source-head-after',
    'source-status-before', 'source-status-after', 'source-status-after-plain',
    'source-diff-after-stat', 'source-diff-after', 'source-diff-after-ignore-cr',
    'gitattributes-before', 'gitattributes-after',
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
    @(Select-String -Path (Join-Path $EvidenceDir '*') -Pattern '(?i)jmckerra@purdue\.edu') +
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
    -m "evidence: record Priority 12 Windows run of $Short ($Overall; UNPROVEN: $(($UnprovenRows | ForEach-Object { $_ -replace '^unproven-', '' }) -join ', '))" `
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
# Every new commit on the evidence branch: author, committer and message must not hold the personal
# address. Only the address is tested, so the value is never printed.
$NewCommitText = (@(& $Git -C $EvidenceWorktree log 'refs/remotes/origin/main..HEAD' '--format=%an%n%ae%n%cn%n%ce%n%B')) -join "`n"
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($NewCommitText)) { throw 'Cannot read the new commits.' }
if ($NewCommitText -match '(?i)jmckerra@purdue\.edu') {
    throw 'A new commit author, committer or message names the personal address. Do not push. Report it (the fact only) and keep the commit local.'
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
- the overall result and the status of each of the 30 runbook gates (`daemon-build`, `ipc-tests`,
  `daemon-tests`, `daemon-lifecycle`, `coordinator-review-tests`, `windows-transport-tests`, `git-discovery-tests`,
  `git-lib-tests`, `git-env-policy-tests`, `core-contract-tests`, `runtime-env-isolation-test`, `store-tests`,
  `diagnostics-tests`, `cli-portable-acceptance-tests`, `cli-cleanup-safety-tests`,
  `cli-engine-reconciliation-tests`, `cli-crash-concurrency-tests`, `clippy-scoped`, `desktop-npm-ci`,
  `desktop-tsc`, `desktop-build`, `desktop-vitest`, `tauri-build`, `tauri-tests`, `tauri-bridge-timing`,
  `tauri-clippy-clean`, `tauri-clippy`, `desktop-package-win`, `node-self-tests`, `harness-list`; the
  harness itself lists 16 gates on Windows, and this runbook runs those 16 plus the per-crate and
  extra gates) with exit code, duration, and, for the cargo test gates, the
  `running N tests` counts. List the 14 UNPROVEN rows (`unproven-second-user`,
  `unproven-remote-client`, `unproven-slow-reader-64kib`, `unproven-tauri-webview-click-path`,
  `unproven-e2e-windows`, `unproven-gui-install-launch`, `unproven-gui-task-survives-close`,
  `unproven-gui-event-reconnect`, `unproven-stale-token-relaunch-timing`,
  `unproven-daemon-idle-behaviour`, `unproven-daemon-kill-and-sidecar-failure`,
  `unproven-data-dir-acl`, `unproven-launch-failure-redaction`, `unproven-objective-64kib-r12`) separately,
  with the result of each of the three by-hand checks of "By-hand checks (Priority 12)" from `manual-p12.txt`
  (or NOT RUN), and
  the derived row `tauri-native-bridge-host-level` with its status (PASS only if `coordinator-review-tests`
  and `tauri-tests` both passed; it is host level, not a webview result). The e2e
  suite is NOT RUN, and no e2e number exists. Attach the `[bridge-test]` timing lines of
  `tauri-bridge-timing.bridge-lines.txt` (the slowest call, per test)
- the OS, CPU, `rustc -Vv`, `cargo -V`, `cargo clippy -V`, `git --version`, `node --version`,
  `npm --version`, and `core.autocrlf` (with its origin)
- the branch tip and the pinned source commit, and that `HEAD` matched before and after
- the Git top-level path form that `source-toplevel` recorded, and the source tree, parent, range
  count, range commits, and changed-file checks
- the **measured** counts and the lifecycle result, exactly as printed (the `running N tests` sequence, passed, failed,
  and ignored; the `lifecycle: PASS` line; Vitest `Test Files` and `Tests` lines), next to the derived counts
  (IPC 46 with sequence `46,0`; `windows_transport` 13 with sequence `13` and 33 filtered out;
  `git-discovery-tests` 2; daemon unit 50 plus 4 `coordinator_review` tests plus 1 `diagnostics_log` test
  (55 in all), sequence `50,4,0,1` (`tests/daemon.rs` is empty), plus `lifecycle: PASS` with STEP 1 to 8;
  `coordinator-review-tests` 4; git `--lib` 47, `env_policy` 11; core `contract` 17; runtime `env_isolation` 1;
  store 32; diagnostics 28 with sequence `0,6,7,15,0`; cli `portable_acceptance` 2, `cleanup_safety` 4,
  `engine_reconciliation_gate` 22, `crash_concurrency` 6; src-tauri 12 with sequence `12,0,0`; Vitest 15 files
  and 289 tests; node self-tests 36 pass; clippy baseline `supervisor.rs` 409 and 432). Label each as
  measured now, derived, Mac, or prior native. The Mac workspace total is 708 (not Windows evidence); the
  Windows workspace total is not a gate. Every
  sequence here is derived from the cfg attributes and the file list; `42,4,0` was observed natively
  at `1bb24bf9…` (r4), `42,0,0` at `e555c2c9…`, `3df3b583…` and `6b9cf7cd…`,
  and `13` at `e555c2c9…` with 25
  filtered out of 38. None was observed on this commit. A mismatch is a finding,
  not a reason to edit anything. State that the Windows counts differ from the Mac counts by design
  and that you reported the measurement and did not match it to any other number
- whether the daemon and the Tauri host built natively with the M5-05 through M5-05r5 and M6-01, M6-02 and M6-03 code
  (`daemon-build`, `tauri-build`). If either did not, the full stderr, the first error verbatim,
  and the triage record from "If a build fails (compile-failure triage)"
- the daemon binary, Tauri binary and installer sizes and SHA-256 values from `daemon-binary.txt`,
  `tauri-binary.txt` and `installer.txt`, every build warning line verbatim, and the full text of
  `clippy-scoped.diagnostics.txt` and `tauri-clippy.diagnostics.txt` (BASELINE and NEW diagnostics)
- the coverage gaps: Unix-only tests not run, the UNPROVEN and NOT RUN rows, and the items not
  validated on Windows
- the runbook identity record: blob id, byte length, and SHA-256 of the pinned runbook bytes
- for `ipc-tests`, `daemon-tests`, `daemon-lifecycle`, `coordinator-review-tests`, `windows-transport-tests`, `git-discovery-tests` and `tauri-tests`:
  the `running N tests` lines (the `STEP` and `lifecycle: PASS` lines for the lifecycle gate), the
  `survivors.txt` content (empty means none found), and for a failure the first failing assertion
  message exactly as printed. For any failing gate (the harness ran every gate regardless), quote from
  `<gate>.diagnostics.txt` the failed test names, the `--- per-call timing (…) ---` block (the slowest
  calls and every `ERR` line) and the `--- daemon log tail (…) ---` or `--- daemon log (…) ---` block;
  for a failing `tauri-tests`, also quote every `[bridge-test]` line, the `request start` and `request end`
  trace lines of the failing call (and its `elapsed_ms`) and the failing panic text
  (`<method> (id N, limit 120s): …`), and say how long the
  nested daemon build took (the gap between the first `Compiling` line and the first `running` line)
- the host-level bridge row: the status of `tauri-native-bridge-host-level` and, if it is `UNPROVEN`, which
  of `coordinator-review-tests` and `tauri-tests` did not pass. State that the webview and click path
  stay `UNPROVEN` either way
- if you performed any manual lifecycle step from the native lifecycle procedure, a flat
  `manual-lifecycle.txt` in `$EvidenceDir` before Step 5 (plain facts only: which step, what you saw,
  times if you took them, no paths with a user name, no tokens, no screenshots). Without it, the
  GUI rows stay UNPROVEN and NOT RUN. It is optional, scanned like every other evidence file, and
  never changes a verdict
- for any timed-out check, the cold-build label from the status rules, as a note beside the unchanged
  `FAIL`
- that `protect-text-selfcheck.txt` and the publication scan (including the e-mail scan) passed
- the commit identity: the four lines of `git log -1 --format=%an%n%ae%n%cn%n%ce` for the evidence
  commit, which must be exactly `j-mckerracher`, `135927658+j-mckerracher@users.noreply.github.com`,
  `j-mckerracher`, and the same noreply address; that you set it with `git config --local` only; and
  that the check of the new commit's author, committer and message for the personal address passed.
  Do not paste any other e-mail address, name, or profile directory into the report
- any setting that you could not observe, including whether your agent runtime could run Sonnet 5.5
  with high reasoning

Do not edit `board.json`. The coordinator reviews the evidence and updates the board.

## What changed from the approved harness

**Priority 12 re-pin (authoritative for this runbook).** The harness blocks derive from the published, never-run
Priority 11r5 runbook (branch `docs/priority-11r5-windows-acceptance-a5e1f43`) and are re-pinned to
`136646d95c86b050ca6f6617bb75bf10758826bb`. Changes: the pin block and ref names; the `ao12-` run root, the
`p12ev-` worktree prefix and the `evidence/priority-12-windows-acceptance-136646d-<UTC>-<hex>` branch; the
expected counts (ipc `46,0`, `windows_transport` 13 with 33 filtered, daemon `50,4,0,1` with 55 passed,
`coordinator_review` 4, lifecycle STEP 1 to 8, `tauri-tests` `12,0,0`, Vitest 15 files and 289 tests, clippy
baseline `supervisor.rs:409` and `:432`); new gates `tauri-bridge-timing`, `git-lib-tests` 47,
`git-env-policy-tests` 11, `core-contract-tests` 17, `runtime-env-isolation-test` 1, `store-tests` 32,
`diagnostics-tests` 28, four `cli` gates (2, 4, 22, 6), `node-self-tests` 36 and `harness-list` (30 gates in all);
the bridge-lines capture; three new UNPROVEN rows (14 in all); and the by-hand checks. The noreply identity
setup, the refusal on `GIT_AUTHOR_*` and `GIT_COMMITTER_*`, the privacy scan, `CI=true`, `core.longpaths`,
`core.autocrlf false` and continue-past-failed-gates are unchanged. Everything below is ancestor history.

The harness blocks derive from the Priority 11r2 runbook (docs commit
`740cb367366652ba723bf8da428f96eeef748ee7`, PASS with limits at evidence commit
`e3d3ccc39ac848bc2b70f546090843611b1bacd2`), which derives from the accepted Priority 10 round 2 runbook (docs commit
`08531389c488fe29edd2107990921be16e31c1d5`, native PASS with limits at evidence commit
`d87d56b555fb92ad2bb623898bddc30218f1f968`), which derives from the Priority 10, 10r, 9r, 8 and 7
runbooks. Preserved unchanged in behavior: PowerShell 7 only, the pinned SHA and `HEAD` equality
before and after, the declared runbook commit and the runbook byte pin, clean-checkout gates, unique
run directories outside the checkout, `Protect-Text` and its self-check, `Invoke-Bounded` (byte-exact
raw capture, tree kill on timeout only), the survivor scan after every check, the fail-closed
publication scan, the `Expect`, `Passed`, `Ignored`, `RunningSequence`, `Isolated`, `TimeoutSeconds`
model, the never-kill-a-survivor rule, the origin-only `Assert-Origin` checks, the clippy
BASELINE and NEW classification (NEW never changes the verdict), and evidence-only branch publication
to `origin`.

Changed for Priority 11 round 5 (relative to the Priority 11r4 runbook `…-1bb24bf-runbook.md`). All 16 r4
gates, their fixes, expected counts (except IPC, see below), the 11 static UNPROVEN rows and the derived bridge row are kept
unchanged (`CI=true`, `package:win`, `tauri-clippy-clean` and the verbose `tauri-clippy`,
`core.longpaths`, `core.autocrlf` false, the `.gitattributes` and clean-tree checks, the noreply identity
procedure with its refusal on `GIT_AUTHOR_*` and `GIT_COMMITTER_*`, the privacy scan including the
evidence commit's author, committer and message, the `daemon.log` capture, and the continue-after-a-failed-gate rule):

- **Re-pinned** to source `a5e1f43b…` (tree `b8460109…`, parent `1bb24bf9…`), branch check
  `orch/priority-11r5-…`, 16 range commits, 61 changed files (regenerated with `git diff --name-only
  e555c2c9 a5e1f43b`; the new path is `crates/orchestrator-git/tests/repository_discovery.rs`), evidence branch
  `evidence/priority-11r5-windows-acceptance-a5e1f43-<RunId>`,
  evidence path `docs/evidence/priority-11r5-windows-acceptance/…`, run-root prefix `ao11r5-`, evidence
  worktree prefix `p11r5ev-`, runbook file name and docs branch.
- **IPC expects 46** (`46,0`, was 45): the new `a_call_longer_than_the_idle_timeout_does_not_stop_the_server`
  name is required. `windows_transport` stays 13 and now prints `33 filtered out`.
- **New gate `git-discovery-tests`** (seventeen gates): `cargo test --locked -p orchestrator-git --test
  repository_discovery`, isolated, 900 s, sequence `2`, 2 passed, both names required. It sits after
  `windows-transport-tests` and also writes `<gate>.diagnostics.txt` on failure.
- **`tauri-tests` failure capture.** On a non-`PASS` result `Save-LifecycleDiagnostics` also copies every
  `[bridge-test]` per-call timing line and every daemon `request start` and `request end` trace line
  (from `AGENT_ORCH_TRACE_REQUESTS=1`), next to the daemon log tail blocks (the existing regex already
  matches the r5 `--- daemon log tail (<K> of <N> lines, <path>) ---` form). The gate command, the
  limit (3600 s), `Isolated`, the `9,0,0` sequence and the cleared `AGENT_ORCH_DAEMON_BIN` and
  `AGENT_ORCH_SEED_BIN` are unchanged; the harness does not pass `--nocapture`.
- **Prose** describes the 120 s per-call deadline, the in-flight call guard for the idle exit, the one-spawn
  `Repository::new`, and the known unfixed risk of a 2 s idle close of an unsubscribed connection.

Changed for Priority 11 round 4 (history, relative to the Priority 11r3 runbook `…-efa5e8a-runbook.md`; the pins named in this list are the r4 pins and no longer apply). All 16 r3
gates, their fixes, expected counts, the 11 static UNPROVEN rows and the derived bridge row are kept
unchanged (`CI=true`, `package:win`, `tauri-clippy-clean` and the verbose `tauri-clippy`,
`core.longpaths`, `core.autocrlf` false, the `.gitattributes` and clean-tree checks, the noreply identity
procedure with its refusal on `GIT_AUTHOR_*` and `GIT_COMMITTER_*`, the privacy scan including the
evidence commit's author, committer and message, and the `daemon.log` capture):

- **Re-pinned** to source `1bb24bf9…` (tree `ac47834e…`, parent `efa5e8af…`), branch check
  `orch/priority-11r4-…`, 15 range commits, 60 changed files (regenerated with `git diff --name-only
  e555c2c9 1bb24bf9`), evidence branch `evidence/priority-11r4-windows-acceptance-1bb24bf-<RunId>`,
  evidence path `docs/evidence/priority-11r4-windows-acceptance/…`, run-root prefix `ao11r4-`, evidence
  worktree prefix `p11r4ev-`, runbook file name and docs branch.
- **A failed gate never stops the run.** The loop already continued past a failed gate; r4 states it in
  the harness and the Status section, and the result report now names every gate that is not PASS. Only
  a possibly-still-running owned process stops the run. Step 5 publishes a FAIL run.
- **Failure diagnostics carry the new test output.** `Save-LifecycleDiagnostics` also captures the
  `--- per-call timing (…) ---` block and the `--- daemon log tail (<path>) ---` block that the r4
  `coordinator_review` tests print (the r3 regex matched only `--- daemon log (<path>) ---`, which the
  r4 tests no longer print), the failed test names, and still the `bridge::real_tests` block and the
  lifecycle block. The reporting list asks for these blocks for every failing gate.
- **Prose only for the test changes.** The 30 s client I/O timeout, the 30 s waits and 15 s `STOP`, the
  9-spawn `snapshot_between` and the 85-spawn count are described where they matter on Windows. No gate
  command, limit (600 s and 3600 s are kept), count or required test name changed.

Changed for Priority 11 round 3 (history, relative to the Priority 11r2 runbook `…-6b9cf7c-runbook.md`; the pins named in this list are the r3 pins and no longer apply). All 15 r2
gates and their fixes are kept unchanged (`CI=true`, `package:win`, `tauri-clippy-clean` and the verbose
`tauri-clippy`, `core.longpaths`, `core.autocrlf` false, the `.gitattributes` and clean-tree checks, the
noreply identity procedure with its refusal on `GIT_AUTHOR_*` and `GIT_COMMITTER_*`, the privacy scan
and the lifecycle `daemon.log` capture):

- **Re-pinned** to source `efa5e8af…` (tree `4587f616…`, parent `6b9cf7cd…`), branch check
  `orch/priority-11r3-…`, 14 range commits, 57 changed files (regenerated with `git diff --name-only
  e555c2c9 efa5e8af`), evidence branch `evidence/priority-11r3-windows-acceptance-efa5e8a-<RunId>`,
  evidence path `docs/evidence/priority-11r3-windows-acceptance/…`, run-root prefix `ao11r3-`,
  evidence worktree prefix `p11r3ev-`, runbook file name and docs branch.
- **New gate `coordinator-review-tests`** (sixteen gates): `cargo test --locked -p orchestrator-daemon
  --test coordinator_review`, isolated TEMP, a 600 s limit, `Single`, sequence `4`, 4 passed, the four
  names required `ok`. It runs after `daemon-lifecycle`.
- **`daemon-tests` expects 46** (sequence `42,4,0`) and requires the four `coordinator_review` names.
- **`tauri-tests` expects 9** (sequence `9,0,0`) and requires the three `bridge::real_tests` names. Its
  3600 s limit is kept, and the text says that the first run needs several minutes of nested daemon
  build. The harness clears `AGENT_ORCH_DAEMON_BIN` and `AGENT_ORCH_SEED_BIN`, so the nested build is
  never skipped.
- **Failure diagnostics widened.** `coordinator-review-tests` and `tauri-tests` also write
  `<gate>.diagnostics.txt` and `<gate>.daemon-log.txt` when they are not `PASS`, and the diagnostics
  carry every `--- daemon log (<path>) ---` block that a failing test prints. The lifecycle block capture
  is unchanged.
- **UNPROVEN rows reclassified honestly.** `unproven-tauri-native-bridge` is replaced by the derived row
  `tauri-native-bridge-host-level`, which is `PASS` only when both new real-daemon gates pass natively and
  is `UNPROVEN` otherwise, and by the new always-UNPROVEN row `unproven-tauri-webview-click-path`.
  `unproven-gui-install-launch`, `unproven-gui-task-survives-close`, `unproven-gui-event-reconnect` and
  the rest stay UNPROVEN or NOT RUN, so the static UNPROVEN rows are still eleven. Sections 5.5 and 5.6
  of the in-repo lifecycle runbook are listed as an optional manual GUI checklist.
- **Privacy.** The evidence commit's author, committer and message are explicitly tested for the
  personal address before the push, and the evidence-file scan names that address too. The e-mail scan
  and the noreply identity procedure are unchanged.
- **Counts.** IPC `45,0`, `windows_transport` 13 (32 filtered out), daemon unit 42 (`42,4,0` with the 4
  `coordinator_review` tests), `src-tauri` 9 (`9,0,0`), Vitest 15 files and 287 tests, and
  `lifecycle: PASS`. The four `coordinator_review` tests were run on the Mac while this runbook was
  written (4 passed in 2.61 s). Nothing was run on Windows.

Changed for Priority 11 round 2 (history, relative to the Priority 11 runbook `…-3df3b58-runbook.md`; the pins named in this list are the r2 pins and no longer apply):

- **Re-pinned** to source `6b9cf7cd…` (tree `4ff162c1…`, parent `8c8481b6…`), branch check
  `orch/priority-11r2-…`, 13 range commits, 51 changed files, evidence branch
  `evidence/priority-11r2-windows-acceptance-6b9cf7c-<RunId>`, evidence path
  `docs/evidence/priority-11r2-windows-acceptance/…`, run-root prefix `ao11r2-`, evidence worktree
  prefix `p11r2ev-`, runbook file name and docs branch.
- **`$env:CI = 'true'`** instead of `'1'`. The Tauri CLI binds `--ci` to `CI` and accepts only `true`
  or `false`; `'1'` aborted `tauri build` at `3df3b583…`.
- **`desktop-package-win` runs `npm run package:win`** (`apps/desktop/scripts/package.mjs`), which
  normalizes `CI`, prepares the release sidecar and runs `tauri build` for the NSIS bundle.
- **Tauri host lint.** New `tauri-clippy-clean` gate (`cargo clean -p agent-orch-desktop`), and
  `tauri-clippy` now runs `cargo clippy --locked --all-targets --verbose` and requires both
  `--crate-name agent_orch_desktop_lib` and `--crate-name agent_orch_desktop`. Fourteen gates became
  fifteen.
- **Lifecycle diagnostics.** A failed or inconclusive `daemon-lifecycle` or `daemon-tests` writes
  `<gate>.diagnostics.txt` and `<gate>.daemon-log.txt` into the evidence, and both are in the report
  text. The daemon output is no longer sent to `Stdio::null()` in the test.
- **Long paths.** `git config --local core.longpaths true` on the checkout before the first checkout
  (evidence `config-longpaths*`), and a shorter run root.
- **Clean-tree evidence.** After the gates the harness keeps `git status`, `git status --porcelain`,
  `git diff --stat`, `git diff` and `git diff --ignore-cr-at-eol`. A non-empty status or stat is `FAIL`.
- **`.gitattributes` check**, before and after the gates (`gitattributes-before` and
  `gitattributes-after`): the pinned LF files must report `text: set` and `eol: lf` and hold no
  carriage return.
- **Counts.** Unchanged from the Priority 11 run: IPC `45,0`, `windows_transport` 13, daemon `42,0,0`,
  `src-tauri` `6,0,0`, Vitest 15 files and 287 tests, and `lifecycle: PASS`.
- **UNPROVEN rows** stay at eleven and stay unproven. The lifecycle root cause at `3df3b583…` is not
  established. The most probable cause (medium-low confidence, static analysis only) is Windows MAX_PATH
  when the task worktree is created; a Job Object or suspended-launch failure, identity capture and the
  restart gate are not excluded. r2 passes `core.longpaths=true` on every git call and adds the
  diagnostics. A lifecycle PASS is conditional on that cause and is not assumed.

Changed for Priority 11 (history, relative to the Priority 10r2 runbook; the pins named in this list are the
Priority 11 pins and no longer apply):

- **Re-pinned.** Title, source commit `3df3b583…`, tree `58a35582…`, base `e555c2c9…` (the accepted
  Priority 10r2 tree, replacing `7435ea32…`), source parent `1a895121…`, the branch name check
  (`orch/priority-11-…`), the range count (10), the 10 commit ids in order, the linear range (zero
  merges), the 46 changed files (regenerated with `git diff --name-only e555c2c9 3df3b58`), evidence
  branch and path names (`priority-11-windows-acceptance-3df3b58-…`), run-root prefix
  (`agent-orch-p11-acc-`), evidence worktree prefix (`p11ev-`), runbook file name, and links.
- **Five gates added (nine to fourteen).** `daemon-lifecycle`, `tauri-build`, `tauri-tests`,
  `tauri-clippy` and `desktop-package-win` are new. The `tauri-*` gates and the installer gate run in
  `apps\desktop\src-tauri` and `apps\desktop`, share the run's `CARGO_TARGET_DIR`, and run after
  `desktop-build` (the Tauri host needs `apps\desktop\dist`). `desktop-package-win` runs last because
  it is the slowest and needs network.
- **New `lifecycle` verdict mode.** The daemon `lifecycle` test has no libtest harness (`harness =
  false`). Cargo counts 0 tests for it, and it prints no `running` or `test result` line. The gate
  requires exit 0, exactly one `lifecycle: PASS` line and the `STEP 1` to `STEP 7` lines in order, and
  treats any libtest `running` or `test result` line as `INCONCLUSIVE` (a different shape). It is not
  judged by a libtest count.
- **`Pattern` check in test mode.** `daemon-tests` also requires the line `lifecycle: PASS` in the same
  stdout, because the harness-free lifecycle binary runs as part of `cargo test -p
  orchestrator-daemon`.
- **Artifact records generalized.** Besides `daemon-binary.txt`, the harness records
  `tauri-binary.txt` (`debug\agent-orch-desktop.exe`) and `installer.txt` (the NSIS installer) with
  size and SHA-256. Artifact handling is no longer limited to the build and lint modes. The three
  records are in the required file list in step 5.
- **Counts re-derived.** IPC 38 -> 45 (3 idle, 4 relay on Windows; the fifth relay test is
  `cfg(unix)`), `windows_transport` stays 13 (filtered out 25 -> 32), the daemon stays `42,0,0`
  (source unchanged), `src-tauri` is new (`6,0,0`, derived from the Mac `7,0,0`), and Vitest 14 files
  and 268 tests -> 15 files and 287 tests (Mac-measured at the pinned tree).
- **UNPROVEN rows five -> eleven.** `unproven-gui-install-launch`, `unproven-gui-task-survives-close`,
  `unproven-gui-event-reconnect`, `unproven-stale-token-relaunch-timing`,
  `unproven-daemon-idle-behaviour` and `unproven-daemon-kill-and-sidecar-failure` are new.
  `unproven-tauri-native-bridge` is reworded: the host is built and unit-tested now, and only the
  running bridge is unproven. `unproven-e2e-windows` stays `NOT RUN`. The three retained rows
  (second user, remote client, slow reader above 64 KiB) are unchanged.
- **Optional manual lifecycle notes.** An operator who actually performs the manual window lifecycle
  writes the observations to `manual-lifecycle.txt` (scanned like every other evidence file). It
  never changes a harness verdict and never turns an UNPROVEN row into a pass. If nothing was
  performed, the file records every step as `NOT RUN`.
- **Tauri host clippy.** There is no baseline; every diagnostic is recorded as NEW in
  `tauri-clippy.diagnostics.txt` and does not change the verdict.
- **Privacy.** Unchanged from 10r2: a repository-local noreply identity (`j-mckerracher` and
  `135927658+j-mckerracher@users.noreply.github.com`) for the evidence commit, refusal when a
  `GIT_AUTHOR_*` or `GIT_COMMITTER_*` variable is set, author and committer verified after the commit
  and before the push, and an e-mail scan of the published evidence files. The inherited parent commits
  (for example the main tip `973b7ffd…`) carry other identities and are out of scope; the scan and the
  identity check cover only the evidence files and the evidence commit.
- **Editing method.** The runbook head (everything before "Safety rules") and this section were
  newly written. The code and publication blocks between them were taken from the 10r2 runbook, which
  was copied with `cp` and spliced with `cat` and `sed -n` line ranges (the template was extracted with
  `git show`), and then edited with the edit tool. No heredoc or generator wrote runbook text.

Known limits of this runbook:

- Nothing in it has been run on Windows for this candidate (`a5e1f43b…`). The daemon and the Tauri host
  cannot be compiled from macOS (`libsqlite3-sys` needs the target C toolchain; `tauri-winres` needs
  `llvm-rc`). Only the IPC and Git crates were Windows-target type-checked on macOS, so the r4 and r5 daemon and
  `src-tauri` changes were never Windows-compiled by their authors (r4 compiled and ran natively at `1bb24bf9…`). A compile failure
  is a finding. Follow "If a build fails (compile-failure triage)", capture the full stderr, and
  still run every remaining gate and publish the evidence. The PowerShell was parse-checked on macOS only, and a run may show a
  harness defect. That is a finding, not a reason to edit the file mid-run.
- The counts are derived by reading `cfg` and `#[test]` attributes at the pinned source. They were
  measured natively on Windows at the previous candidates (daemon `42,0,0` and `windows_transport` `13`
  at `e555c2c9…`; IPC `45,0`, `windows_transport` 13 with 32 filtered out, daemon `42,0,0`, `src-tauri`
  `6,0,0` and Vitest 15 files and 287 tests at `3df3b583…`; all 15 r2 gates at `6b9cf7cd…`; at `efa5e8af…`
  (r3) 14 gates passed, `tauri-tests` `9,0,0` among them; at `1bb24bf9…` (r4) 15 of 16 gates passed, the
  failure being `tauri-tests` with 8 passed and 1 failed), but they
  are unmeasured on this commit. The IPC `46`, the `git-discovery-tests` `2` and the `tauri-tests` `9,0,0` sequence have
  never been measured natively with passing results. The `4` of `coordinator_review` passed natively at r4.
  A mismatch is
  `INCONCLUSIVE` and is a finding for the coordinator, not an edit of this file.
- The Vitest counts (15 files, 287 tests) were measured on macOS with Node v26.13.0 and natively at
  `3df3b583…`.
- The `daemon-lifecycle` gate is the native run of a real daemon launching a real child process
  on Windows in this repository. It failed at `3df3b583…` and passed at `efa5e8af…`. It uses a 30 s wait and tight assertions
  about process exit and token removal. A failure is a finding about the candidate or the host, not a
  flake, and its diagnostics and daemon log go into the evidence.
- `tauri build` (inside `npm run package:win`) rewrites `apps\desktop\src-tauri\Cargo.toml`, which is
  committed in that form, so a clean tree after the build is expected. The NSIS bundle has never been
  completed natively; the first run downloads NSIS tooling and needs network access.
- The daemon built by `daemon-build` is never started by the harness. The installer is built and never
  installed. No window is driven. The desktop e2e suite uses Unix domain sockets and is NOT RUN.
  Second-user denial, a genuinely remote client, a write above 64 KiB to a slow reader, the webview
  and click path of the Tauri host (the host-level bridge is only the derived row), the GUI install, launch, close and reopen, event reconnect, the stale-token relaunch
  timing, the daemon idle behaviour and the kill and sidecar-failure cases are `UNPROVEN` or `NOT RUN`.
  Mac results for any of these are not Windows evidence.
- The daemon `tests\daemon.rs` binary is `cfg(unix)` and reports `running 0 tests` on Windows. Do not
  read that as a missing gate. `tests\coordinator_review.rs` is not `cfg(unix)` and runs four tests. The
  scheduler and executor have no native test here.
- The `coordinator_review` and `bridge::real_tests` tests spawn the real daemon with 30 s waits (120 s per frame in
  `bridge::real_tests` at r5), a 30 s
  client I/O timeout and a 15 s `STOP`, and a token-rewrite step that needs a writable `daemon.token`.
  `coordinator_review` passed natively at r4 (it failed 2 of 4 at r3 on the 2 s timeout);
  `bridge::real_tests` passed natively at r3 (`tauri-tests` took 137 s) and at r4 one of them failed on a 30 s frame wait with an empty daemon log. A
  Windows failure is a finding, with the per-call timing and the daemon log tail in the evidence, and not a reason to retry.
- The release rule (section 5.5 of the in-repo runbook) may require the desktop UI itself to have
  driven the approval flow. This runbook cannot satisfy that. A `PASS` of `tauri-native-bridge-host-level`
  is a host-level result only.
- The check helper ends only the tree it started. The survivor scan reports other processes by path
  and command line and never stops them. A detached daemon left by a gate is reported, not killed.
- The cargo gates share one target directory. If cargo prints no `Compiling` or `Checking` line where
  one is required, the gate is `INCONCLUSIVE`. That is a finding, not a cause to run it again in the
  same evidence directory.
- The Mac counts are coordinator-supplied except the Vitest run that the runbook author repeated.
  Independent Mac validation is separate and does not change the Windows expectation.
- The declared runbook commit and the source pin are separate. The byte pin makes the runbook bytes
  comparable after the run. It is not an execution proof.
- The runbook is read from the **tip of the docs branch**. The source under test is always `a5e1f43b…`.
  The docs chain is `a5e1f43b…` -> the tip (one docs commit). The pin is self-referencing: the file
  cannot contain its own commit id, so the operator declares the tip, checks that its only parent is
  `a5e1f43b…`, and Step 1 re-checks that and that the whole difference from the source is this one
  file. It records the result in `runbook-identity.txt`.
