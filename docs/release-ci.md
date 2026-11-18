# Release CI (M6-02)

Native macOS and Windows gates, dependency audits, and signed or unsigned release builds on
GitHub Actions. A merged release pull request, opened by release-please, creates a `v*` tag, and
the tag's run publishes its GitHub release (see [Publishing a release](#publishing-a-release)).
Versioning and the steps to release are in [RELEASE.md](RELEASE.md).

## Workflows

| File | Runs on | Triggers | What it does |
| --- | --- | --- | --- |
| `.github/workflows/ci.yml` | `macos-15`, `windows-2025` | pull request, push to `main`, nightly, manual | Sets `core.autocrlf=false` and `core.longpaths=true`, then runs every native gate once with `CI=true`, its own target directory and an isolated test temp directory. The Windows gates run as parallel jobs that a pull request's changed files select (see [Windows jobs](#windows-jobs-and-what-a-pull-request-runs)). |
| `.github/workflows/audit.yml` | `ubuntu-24.04` | changes to manifests, lockfiles or `deny.toml`; push to `main`; daily; manual | `cargo deny` for both manifests, `cargo audit` for both lockfiles, `npm audit --omit=dev --audit-level=high` for the shipped desktop tree (the dev tree is audited as a warning). |
| `.github/workflows/release.yml` | `macos-15`, `windows-2025`; `publish` on `ubuntu-latest` | tag `v*`; manual; dispatched by `release-please.yml` | Builds the macOS app and the Windows NSIS installer; it does not re-run `ci.yml` or `audit.yml`, because the release pull request's checks are the gate. Signs when secrets are present. Uploads each build's artifacts. For a `v*` tag with `publish` set (or a tag push), the `publish` job publishes the GitHub release (see [Publishing a release](#publishing-a-release)). |
| `.github/workflows/release-please.yml` | `ubuntu-24.04` | push to `main` | Opens or updates the release pull request. When a release pull request has merged, creates the annotated `v*` tag and dispatches `release.yml` on it with `publish=true`. Publishes nothing itself (see [RELEASE.md](RELEASE.md)). |

Every third-party action is pinned to a commit SHA. Workflows set top-level `permissions: {}`, and
each job grants only what it uses: `contents: read` for the builds, `contents: write` for `publish`
in `release.yml` (it publishes the release), `contents`, `issues` and `pull-requests: write` for the
release-please job, and `actions: write` plus `contents: write` for the job in `release-please.yml`
that tags and starts the release. Secrets are mapped only into the `Package` step of `release.yml`.
`ci.yml` and `audit.yml` cancel a superseded run only for a pull request; a scheduled or `main`
run always completes.

## Gates

`scripts/ci/gates.mjs` defines the gates as data; `scripts/ci/run-gates.mjs` runs them;
`scripts/ci/parse.mjs` judges the output; `scripts/ci/expected.mjs` holds every expected count and
the clippy baseline. A gate passes only when the command exits 0 within its time limit, **no
process of the run survives it**, **and** the output matches the expected shape: test counts, the `running N tests`
sequence, named tests, `Compiling` lines for a build gate (so a cached target cannot pass), the
Vitest file and test counts, and the artifact. Nothing is retried. A gate whose prerequisite did
not pass is `SKIPPED`: it was not measured, so it is never a pass. The run's overall verdict is `FAIL` (exit

1) if any gate or harness check failed, else `INCONCLUSIVE` (exit 2) if any gate was `SKIPPED` or none ran, else
`PASS` (exit 0). `summary.md` and `overall.result.json` carry the verdict, and `summary.md` lists the skipped
gates under "Not measured, so NOT a pass".

The Windows list is the published runbook
`docs/evidence/priority-11r3-windows-acceptance-efa5e8a-runbook.md` (branch
`docs/priority-11r3-windows-acceptance-efa5e8a`): same commands, order and derived counts.

| Gate | Command | Where |
| --- | --- | --- |
| `node-self-tests` | `node --test "scripts/**/*.test.mjs"` (112 tests on both; the exit code is in `node-self-tests.result.json`) | both |
| `daemon-build` | `cargo build --locked -p orchestrator-daemon` | both |
| `ipc-tests` | `cargo test --locked -p orchestrator-ipc` (55 tests) | both |
| `windows-transport-tests` | `cargo test --locked -p orchestrator-ipc windows_transport` (13 tests) | Windows |
| `cli-build` | `cargo build --locked -p orchestrator-cli` (shape: a `Finished` line and the binary; cached output is fine) | Windows |
| `p12-windows-checks` | `pwsh.exe -NoProfile -File scripts\ci\p12-windows-checks.ps1 -TargetDir {target} -RunRoot {testTmp}\p12 -EvidenceDir {evidence}`; the last `P12-WINDOWS-CHECKS:` line must be `PASS` (see below) | Windows |
| `daemon-tests` | `cargo test --locked -p orchestrator-daemon` (Windows 138 passed + 1 ignored = `[118,12,2,4,0,1,2]`, derived; macOS 208 passed + 1 ignored = `[175,14,2,4,11,1,2]`, measured: unit, `conversation_runtime`, `conversations`, `coordinator_review`, `daemon.rs`, `diagnostics_log`, `host_metrics`; the ignored test is in `conversation_runtime`) | both |
| `daemon-lifecycle` | `cargo test --locked -p orchestrator-daemon --test lifecycle -- --nocapture` (`STEP 1` to `STEP 8`, one `lifecycle: PASS`) | both |
| `coordinator-review-tests` | `cargo test --locked -p orchestrator-daemon --test coordinator_review` (4 tests) | both |
| `git-lib-tests` | `cargo test --locked -p orchestrator-git --lib` (49 on both: Windows 47 portable + 2 `cfg(windows)`, macOS 47 portable + 2 `cfg(unix)`) | both |
| `cli-cleanup-safety-tests` | `cargo test --locked -p orchestrator-cli --test cleanup_safety` (Windows 3, macOS 4: one test is `cfg(unix)`; a fifth is `cfg(all(unix, feature = "fault-injection"))` and runs nowhere by default) | both |
| `private-fs-tests` | `cargo test --locked -p orchestrator-private-fs` (Windows `[11,0]`: 4 portable + 7 DACL; macOS `[8,0]`: 4 portable + 4 Unix; the `0` is the doc-test binary) | both |
| `store-tests` | `cargo test --locked -p orchestrator-store` (Windows 153 passed + 1 ignored, derived; macOS 157 passed + 1 ignored, measured: 4 tests are `cfg(unix)`; the ignored test is in `data_lock`) | both |
| `lock-takeover-test` | `cargo test --locked -p orchestrator-store --lib the_lock_file_is_owner_only_when_created_and_when_taken_over` (1 passed; 62 filtered out on Windows, 64 on macOS) | both |
| `diagnostics-tests` | `cargo test --locked -p orchestrator-diagnostics` (28 on both: no test is `cfg`-gated) | both |
| `runtime-command-line-tests` | `cargo test --locked -p orchestrator-runtime --lib command_line` (8 tests; the pure Windows rules run on every host) | both |
| `cli-command-line-tests` | `cargo test --locked -p orchestrator-cli --lib -- os_command_line_limit 65000_character_objective` (Windows 2, macOS 1: the 65,000-character test is `cfg(windows)`) | both |
| `clippy-scoped` | `cargo clippy --locked -p orchestrator-ipc -p orchestrator-daemon --all-targets` (baseline: two `single_match` warnings in `supervisor.rs`) | both |
| `workspace-tests` | `cargo test --locked --workspace` (981 passed, 2 ignored) | macOS |
| `desktop-npm-ci`, `desktop-tsc`, `desktop-build`, `desktop-vitest` | in `apps/desktop`: `npm ci`, `npx tsc -b`, `npm run build`, `npx vitest run` (51 files, 1105 tests) | both |
| `desktop-e2e` | `npm run test:e2e` (8 files, 44 tests, real daemon over a Unix socket) | macOS |
| `desktop-geometry` | `npx playwright test` in `apps/desktop`: browser layout checks in `e2e/geometry.spec.ts` and `e2e/topbar-usage-viewport.spec.ts` (installed Chrome, Vite dev server) | macOS |
| `tauri-build`, `tauri-tests`, `tauri-clippy-clean`, `tauri-clippy` | in `apps/desktop/src-tauri`: `cargo build --locked`, `cargo test --locked` (Windows 14, macOS 16), `cargo clean -p agent-orch-desktop`, `cargo clippy --locked --all-targets --verbose` | both |
| `desktop-package-win` / `desktop-package-mac` | `npm run package:win` (NSIS installer) / `npm run package:mac` (`.app`) | Windows / macOS |

Windows and macOS counts differ because tests are gated by `cfg`. Expected counts are never edited
to fit a run; a change needs a reason in the commit that edits it.

### Windows jobs and what a pull request runs

The Windows gates run as seven parallel jobs beside a harness job: engine build, engine tests (a) and (b), tauri
build, tauri tests, desktop, and desktop package. `scripts/ci/windows-jobs.mjs` holds the job table, the rules that
pick the jobs for a change, and the verdict. `gates (windows)` is the one check over all of them, and it keeps the
name that branch protection watches.

| Changed files on a pull request | Jobs that run |
| --- | --- |
| only `docs/**` or a Markdown file at the repository root | harness |
| only `apps/desktop/src/**` (frontend source) | harness, desktop |
| other `apps/desktop/**` (vite config, `index.html`, `e2e/`, package scripts, `package.json`) | harness, desktop, desktop package |
| `apps/desktop/src-tauri/**` | harness, tauri build, tauri tests, desktop package |
| `crates/**` | harness, engine build, engine tests (a) and (b), tauri build, tauri tests, desktop package |
| anything under `scripts/` or `.github/`, any `Cargo.toml` or `Cargo.lock`, any `package.json` or `package-lock.json`, `rust-toolchain*`, `.cargo/`, `deny.toml`, or a path the table does not know | every job |

A changed file selects every job of its group. A selected job must succeed, and an unselected job must be skipped
(or succeed). The aggregator therefore fails on a failed selected job and on a failed job it did not select, while a
skipped group counts as a pass. A push to `main`, a manual run, the nightly `schedule` (04:23 UTC) and a call from
`release.yml` run every Windows job. The macOS job, `gates (macos)`, runs every gate on every event.

To force every Windows job on a pull request, start a manual run of `ci` on its branch:
`gh workflow run ci.yml --ref <branch>`. That run is a full run. There is no label trigger: a `labeled` event would
cancel the run in progress for the same ref, and its skipped checks would replace earlier results.

**What a pull request does not run, and where it is caught.** A frontend-only change skips the installer
(`desktop package`): the installer packages what `desktop-build` already builds, and the Tauri host is not
rebuilt. What it could miss is a problem only the installer or the release host build shows: a frontend asset the
Tauri bundler rejects, or a bundle step that fails for a new file. Those run on every push to `main`, in the
nightly schedule, and in the release call. A failure there shows on `main` after the merge, at the latest at the
nightly run, so the window is one merge plus at most a day. A Rust, Tauri or packaging-input change always runs the
installer on the PR itself.

### Cargo cache (Windows)

Each Rust Windows job restores `$RUNNER_TEMP/target` (dependency artifacts only), `~/.cargo/registry` and
`~/.cargo/git`. The key is `cargo-v2-windows-<job>-<toolchain>-<both lockfiles>`, and a miss falls back to the newest entry with the
same prefix. The `v2` prefix retires the entries written before the release-profile prune fix: the first version
cleaned only the debug profile, so a restore from it would have carried the release build of the local packages. A pull request only restores. An entry is saved only by a push to `main` whose job passed, and only
when its exact key is missing. A tag build (`release.yml`) neither restores nor saves, so a release never uses a
cache. Before a save, `scripts/ci/prune-target.mjs` runs `cargo clean -p` for every local package (workspace members
and path dependencies). The build and lint gates assert `Compiling` and `Checking` lines for their crates, and a
checkout makes Cargo rebuild local packages anyway.

The Windows Rust jobs also set `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_PROFILE_TEST_DEBUG=0`:
no incremental state, and no debug info, whose PDBs dominate MSVC link time. No gate reads debug info: they assert
counts, test names and build lines. `run-gates.mjs --target-from-cache` allows the restored target to be non-empty
for that reason only.

### Usage smoke step (extension point)

The usage smoke step (AI usage, DPES-1105) is written into the harness job and the `gates (macos)` job, after
`Harness self-tests`. It runs `node --test --test-reporter=tap "scripts/smoke/*.smoke.mjs"` and then
`node apps/desktop/src-tauri/sidecars/codeburn/require-tap-totals.mjs <tap> 4`, which requires exact TAP totals (pass 4,
fail 0, cancelled 0, skipped 0). Both jobs run with `if: always()`, so a skipped step leaves its job green, and the
aggregator still judges the Windows verdict through the harness job.

The smoke test is not under `scripts/**/*.test.mjs`, so `Harness self-tests` does not install the pinned reader. The
audit job runs the same file on Linux (`audit.yml`).

The step runs when `scripts/ci/windows-jobs.mjs` reports `smoke=true`. On a pull request that is only when
`crates/orchestrator-local-usage/**`, `scripts/smoke/**`, `apps/desktop/src-tauri/sidecars/codeburn/**`,
`apps/desktop/scripts/prepare-codeburn-sidecar.mjs`, or the daemon's or the store's usage code changes. Every other event
(a push to `main`, the nightly schedule, a manual run, a release call) runs it. Docs-only and frontend-only changes do
not select it.

**Switch.** `USAGE_SMOKE.enabled` in `scripts/ci/windows-jobs.mjs` is `true`. Setting it to `false` turns the step off;
no other line changes.

**Cost when it runs:** SMOKE_COST_PLACEHOLDER

### Treehouse fetch step (DPES-1044)

The gates that lease worktrees need the pinned treehouse binary: `engine-build`, `engine-tests-a`,
`engine-tests-b`, `tauri-tests` and the `gates (macos)` job. Each runs
`node apps/desktop/scripts/prepare-treehouse-sidecar.mjs --dest "$GITHUB_WORKSPACE/target/treehouse"`, which checks
the archive and binary SHA-256 pins and the version before it stages anything, then exports
`AGENT_ORCH_TREEHOUSE_PROGRAM` through `$GITHUB_ENV` for the gates after it. The `release.yml` build jobs run the
packaging script, which stages the same binary into the installer, so they need no separate fetch.

A change under `apps/desktop/src-tauri/sidecars/treehouse/` or to `prepare-treehouse-sidecar.mjs` selects the engine,
Tauri and package groups on a pull request.

### Survivor check and bounded runs

After every gate the harness lists processes (read-only: `ps` on macOS, `Get-CimInstance
Win32_Process` on Windows) and fails the gate if any process outside the harness and its
ancestors has this run's target or test-temp directory in its image path or command line. This is
the per-gate survivor scan of the Windows runbook (`Find-OwnedSurvivors`); it retries for about
5 s because an exiting process can still be listed, and it never stops a process. A process counts only
if it belongs to the gate: it descends from the process the harness launched for that gate, or it is an
orphan (its parent is gone or is PID 0/1). A process whose parent is alive and not a descendant of the
gate is not the gate's, even if its command line names the evidence or target directory: the Priority 12
round 3 scan flagged the operator's own monitor `pwsh` that way. One difference:
the runbook also matches 8.3 short-name aliases of its run root; this harness matches the paths as
given, which on a GitHub runner (`D:\a\_temp\...`) have no alias. A gate whose process exits
while a descendant keeps its output pipes open ends 5 s after the exit (the gate fails and the
survivor is reported) instead of waiting for the job timeout; a timeout kill ends the run within
10 s.

### Changing a count or the baseline

After a CI run reports a count mismatch, download its evidence and let the tool rewrite the pins:

```sh
gh run download <run-id> -D /tmp/evidence
node scripts/ci/sync-expected.mjs /tmp/evidence/gate-evidence-* --dry-run   # then again without --dry-run
```

It updates `scripts/ci/expected.mjs` and the matching list in `scripts/ci/windows-test-names.json` /
`macos-test-names.json` from the measured output, reads only gates that exited 0 with no failed test, and
refuses a run with fewer tests than the pins unless `--allow-removals` is given. Windows counts come from the
Windows artifacts, macOS counts from `gate-evidence-macos`; run it on both. `node --test "scripts/**/*.test.mjs"`
still fails when a total, a sequence and a name list disagree. Measure on the platform; do not edit to fit a run. The two
`single_match` baseline lines in `supervisor.rs` are `CLIPPY_BASELINE` there: they are
`#[cfg(windows)]` code, so only the Windows `clippy-scoped` gate can show them as new.

**Priority 12 merge** (M6-01 security, M6-02 CI and M6-03 diagnostics on the M5-05 round 5 source)
set the current values in `expected.mjs` and the name lists: `daemon-tests` Windows `[50,4,0,1]`/55
and macOS `[72,4,10,1]`/87 (M6-01 added the two `submit_` tests; M6-03 added the diagnostics
tests and the `diagnostics_log` test binary); `tauri-tests` Windows `[12,0,0]` and macOS
`[14,0,0]`; `ipc-tests` 46 (M5-05 round 5 added `a_call_longer_than_the_idle_timeout_does_not_stop_the_server`);
macOS `workspace-tests` 708 passed and 1 ignored (round 5 also added 2 `orchestrator-git`
`repository_discovery` tests, which no gate counts separately); vitest 289; `CLIPPY_BASELINE.lines`
409 and 432 (before the remediation); and the lifecycle test prints `STEP 1` to `STEP 8` (`LIFECYCLE_STEPS`). The macOS
figures were measured on the merged tree; the Windows figures are derived from the three owner
reports and are unproven until a Windows run. Re-measure on Windows; do not edit to fit.
Run on both platforms, outside the checkout: `node --test "scripts/**/*.test.mjs"` (harness
self-tests).

**Priority 12 Windows remediation** (owner-only data-directory DACL, launch-failure log tail in the
UI, command-line length check) moved the counts: `daemon-tests` Windows `[52,4,0,1]`/57 and macOS
`[73,4,10,1]`/88 (new `agent_commands_carry_the_whole_objective_and_windows_refuses_a_65000_character_one`
on both, and Windows-only `windows_submit_refuses_an_objective_that_cannot_be_passed_to_the_agent`);
`tauri-tests` Windows `[13,0,0]` and macOS `[15,0,0]` (new
`a_daemon_that_cannot_be_started_shows_the_redacted_end_of_the_log`); vitest 290; macOS
`workspace-tests` 721 passed and 1 ignored (+8 `orchestrator-runtime` `command_line` tests, +1
`orchestrator-cli` unit test, +1 daemon test, +6 new `orchestrator-private-fs` tests, -3 `orchestrator-store`
tests that moved into it); `CLIPPY_BASELINE.lines` 414 and 437 (the check added five lines above them).
The macOS figures were measured; the Windows ones are derived. `orchestrator-private-fs` runs 6 tests
on macOS (3 portable, 3 Unix) and 9 on Windows (3 portable, 6 DACL), and the Windows-only CLI test
is a 35th `orchestrator-cli` unit test there.

**Priority 12 round 3** (the first native Windows run of 4d5566d) corrected two derived counts and
made the per-crate suites harness gates, so they run automatically: `git-lib-tests`,
`cli-cleanup-safety-tests`, `private-fs-tests`, `runtime-command-line-tests` and
`cli-command-line-tests` (21 gates on Windows now, up from 16). The Windows run measured `orchestrator-git --lib`
at 46 and `cleanup_safety` at 3; the runbook had derived 47 and 4, which were the macOS counts
(`cfg(unix)` tests counted as if they ran on Windows, and the `cfg(windows)` ones missed). Derived now
from the `cfg` gates in the sources: `orchestrator-git --lib` 49 on both platforms (47 portable,
plus 2 `cfg(unix)` on macOS or 2 `cfg(windows)` on Windows); round 3 replaced the unachievable
over-`MAX_PATH` worktree test with three portable tests and added one `cfg(windows)` test (46 measured
becomes 49 derived on Windows: -1 replaced, +3 portable, +1 `cfg(windows)`). `cleanup_safety` 3 on Windows and 4 on macOS.
The macOS `workspace-tests` count is 723 passed (the two added `orchestrator-git` tests).

**Priority 12 round 4** (the native Windows FAIL of round 3) fixed six defects, each read from the raw Windows
logs:

- `bounded.test.mjs` `EBUSY` on cleanup: Node 22's synchronous `rmSync` retries `EBUSY` for a file and for a
  directory once its children are gone, but not for a directory's first `rmdir`, so the round 3 retry option
  could not cover that case (checked in Node 22.20.0's `lib/internal/fs/rimraf.js`). Cleanup is now
  `scripts/ci/remove-dir.mjs`, an awaited retry loop over the
  asynchronous `rm`, the test's grandchild no longer starts with the temp directory as its working directory,
  and the timeout test waits until the grandchild is gone before it removes anything. `runBounded` now returns
  the pid it launched.
- The by-hand checks' verdict was read as an empty string: the runbook's filter was case-insensitive, so the
  script's two banner lines (`p12-windows-checks: ...`) counted as verdicts. The banners now start with `INFO`,
  the harness has a `p12-windows-checks` gate, and `parse.mjs#checkP12` reads the last
  `P12-WINDOWS-CHECKS:` line (with its trailing space), case-sensitively, from a real round 3 Windows output kept in
  `scripts/ci/fixtures/`. A runbook that filters by hand must use `-cmatch` and take the last line.
- The survivor scan no longer counts a process that is not the gate's (see Survivor check).
- `.orchestrator.lock` is created owner-only (`restrict_to_owner`, no inherited ACE), also when a stale lock is
  taken over; before, it inherited `<user>:(I)(F)` from the data directory. The transient
  `.orchestrator.lock.takeover-<n>` directory is not changed.
- `p12-windows-checks.ps1` no longer waits for `daemon.log`: only the desktop host creates it (it redirects the
  daemon's output there), so the bare daemon never did. The script says so with an `INFO` line instead of a
  failing check; the host's `daemon.log` DACL is covered by a `tauri-tests` test and unproven on a real host.
- The script and the harness redact the Windows domain, host, DNS domain, account, profile path and any e-mail
  address in what they write (`scripts/ci/redact.mjs`, `Protect-Text`), and a `privacy-scan` harness check fails
  the run if one is left in the evidence. Raw `cargo`/`npm` output is not rewritten, and the account name is
  not checked in it (cargo prints the profile path); review it before publishing.

Counts moved: `private-fs-tests` Windows `[11,0]` (+2: the DACL test of an inherited-only owner ACE and the
`is_owner_only` test) and macOS `[8,0]` (+2); `tauri-tests` Windows 14 and macOS 16 (+1, the `daemon.log`
owner-only test, portable); macOS `workspace-tests` 726 (+1 `orchestrator-store`, +2 `orchestrator-private-fs`,
measured); `orchestrator-store` gains 1 portable test (not a harness gate; its Windows count is the runbook's plus 1,
derived); `--list` shows 23 Windows gates (`cli-build` and `p12-windows-checks` are new) and 22 on macOS. The Windows figures are
derived: the daemon and `src-tauri` cannot be built for Windows from macOS.

**Priority 12 round 5** (the native Windows run of round 4 stopped at `desktop-build`, so three gates were
`SKIPPED` and the run was not a pass) fixed six defects:

- D1, `TS2883` in `src/test/render.tsx`: the run root was a `Z:` `SUBST` of a directory on `C:`, so Vitest saw
  `Z:/...` and `tsc -b` (a `composite` project) the real `C:/Users/...` path, and the declaration emit named one
  module by two paths. `mount` now has an explicit return type, so the emitter has nothing to name (it is
  proven on macOS only; the alias itself is refused, see D3). The cause was the alias, not the code.
- D2, three gates `SKIPPED`: `tauri-build`, `tauri-tests` and `tauri-clippy` need `apps/desktop/dist`, so they
  still need `desktop-build` and are skipped when it fails. What changed is the verdict: a skipped gate now
  makes the run `INCONCLUSIVE` (exit 2), never a pass; see Gates. `verdict.mjs` holds the rule and
  `verdict.test.mjs` the cases (a failure plus a skip stays `FAIL`).
- D3, the run root: the runbook's `C:\t` could not be created, so the operator used a `SUBST` drive and the
  duplicated paths. `run-gates.mjs` now checks its paths first (`preflight.mjs`, Windows only) and stops with
  exit 3, creating nothing, if the source, working directory, evidence directory, target directory or test temp
  directory is a `SUBST` or mapped drive, a junction, a UNC path, or longer than 64 characters. It compares each
  given path with `realpathSync.native` of it (or of its nearest existing parent), case-insensitively and
  tolerating 8.3 short names. `node scripts/ci/run-gates.mjs --preflight` runs only this check. Making `tsc`
  tolerate an alias was not cheap or safe (it would need `preserveSymlinks` or a path mapping in every
  project), so the harness refuses instead.
- D4, the node self-tests' exit code was in no evidence: they are now the first gate (`node-self-tests`), with
  the summary (`# pass N`, or `ℹ pass N`) checked for 112 tests and no fail, canceled, skipped or todo, and the exit
  code in `node-self-tests.result.json`. `ci.yml` keeps its own "Harness self-tests" step.
- D5, uncounted tests: `store-tests`, `lock-takeover-test` and `diagnostics-tests` are gates, so the M6-01 (lock
  owner-only) and M6-03 (diagnostics) counts are measured by the harness. 27 Windows gates and 26 on macOS.
- D6, the real-host items stay unproven (the host's `daemon.log` DACL, the redacted launch failure in the real
  window, the ACL of the transient `.orchestrator.lock.takeover-<n>` directory): see the unproven list.

Counts: macOS (measured) `store-tests` `[35,9,2,13,8,11,3,4,3,11,5,1,0]` = 104 passed + 1 ignored; `diagnostics-tests`
`[0,6,7,15,0]` = 28; `lock-takeover-test` 1; the `workspace-tests` total is unchanged at 726 + 1 ignored (the
three gates select tests it already counts). Windows (derived from `cfg`, not measured) `store-tests`
`[33,9,2,13,8,11,3,3,3,11,4,1,0]` = 100 passed + 1 ignored (the four `cfg(unix)` tests are absent: file mode of the
database files, the private lock directory, the symlink at the lock path, and the real SQLite process exit);
`diagnostics-tests` 28; `lock-takeover-test` 1. Node self-tests: 112 (71 plus the 26 of M6-04's
`install-lib.test.mjs`: 24 from M6-04 and 2 added by M6-04r, which made 97; plus the 15 of DPES-1106's
`windows-jobs.test.mjs` (13) and `prune-target.test.mjs` (2)), measured on macOS (no test is platform-gated).
M6-04's native Windows run measured 94 of 95 passing: test 71 hard-coded POSIX `/w/...` paths while the macOS flow
builds them with `node:path`, which gives backslashes on Windows. M6-04r builds every expected path with
`path.join` and checks the file under a Windows-separator simulation (see "Windows path simulation" below); that
is a check of separators only, so the Windows count of 112 stays **DERIVED** until a Windows run measures it.

### Maximum path lengths on Windows

- **Worktree path: 215 characters.** `git worktree add` runs its checkout with `GIT_DIR=<worktree>\.git`, and
  Git dies with ``'$GIT_DIR' too big`` when that exceeds `PATH_MAX - 40` = 220 characters, whatever `core.longpaths`
  says (observed with Git 2.46.2 at a worktree path over 260 characters; the 215 boundary is derived from
  Git's source and its macOS analog, 969 accepted and 981 refused, not yet measured on Windows).
  `orchestrator-git` therefore refuses a longer path with `WorktreePathTooLong`, before any branch or
  directory exists. The daemon's worktree is `<data-dir>\worktrees\task-<64 hex>` (69 characters of
  task name), so the data directory must be at most about 130 characters; the CLI's
  `<data-dir>\worktrees\<task-id>` is shorter. Files inside a checkout may pass 260 characters:
  `core.longpaths=true` is passed to every Git call and the test
  `checkout_files_longer_than_max_path_are_created` covers it.
- **Harness paths.** `--test-tmp`, `--target-dir` and `--evidence-dir` should be at most about 60
  characters (`C:\ao\t`, `C:\ao\target`): the test temp directory, a task or fixture name and a test's
  worktree all go under it. The preflight enforces 64 characters for each root (and for the source). A harness that passes a long `TMP`/`TEMP` (such as
  `C:\Users\<name>\AppData\Local\Temp\<run-id>\test-tmp`) shortens at the source by choosing a short
  `--test-tmp`; do not rely on `core.longpaths`.

### Evidence

Each run writes, per gate, raw `<gate>.stdout.txt` and `<gate>.stderr.txt`, a `<gate>.result.json`
with the verdict and reasons, `environment.txt`, `summary.md` (also the job summary), and for
Windows the list of rows this CI cannot prove. The workflow uploads the directory as
`gate-evidence-<platform>`.

## Dependency audits

- `deny.toml` pins the graph to `aarch64-apple-darwin`, `x86_64-apple-darwin`,
  `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu` (the Linux target is the M6-01 policy:
  the Tauri graph's Linux-only crates are judged too). It denies unknown registries and
  git sources, yanked crates, and licenses outside an allow-list. `cargo deny` runs once per
  manifest because `apps/desktop/src-tauri` is outside the root workspace.
- `cargo audit --file <lockfile>` runs for both lockfiles; vulnerabilities fail, warnings (the
  unmaintained GTK3 crates that only Linux builds use) do not.
- `npm audit --package-lock-only --omit=dev --audit-level=high` runs against
  `apps/desktop/package-lock.json` and fails on the runtime tree that ships in the installers.
  The full tree, which adds dev-only build and test tooling (Vite, Vitest), is audited in a
  second step that reports high-severity findings as a warning and does not fail. Example:
  GHSA-fc5q-39xc-vjpf (picomatch ReDoS, no patched release) reaches only Vite and Vitest.
- **Known exception:** `RUSTSEC-2026-0459` (bundled SQLite before 3.54.0 in `libsqlite3-sys`
  0.37.0, reached through `sqlx` 0.9.0) is ignored in `deny.toml` and `.cargo/audit.toml` with a
  reason. It is a tracked finding, not an acceptance. Remove both entries once the lockfile has
  `libsqlite3-sys` 0.39 or later.
- MPL-2.0 is in the license allow-list because the desktop graph contains it. Legal needs to
  decide this before a public release.
- The treehouse scan record (`docs/third-party/treehouse.md`) is checked by
  `apps/desktop/src-tauri/sidecars/treehouse/treehouse-sidecar.test.mjs`. `audit.yml` runs it with the codeburn record
  test and requires exact totals of 9 (8 codeburn and 1 treehouse).

## Release builds and signing

`scripts/release/package.mjs <macos|windows> --out DIR` runs the same `npm run package:*` as the
CI gate, verifies the result, and writes the artifact, `SHA256SUMS` and `release-info.json`. The
artifact name ends in `signed` or `unsigned`.

`scripts/release/signing.mjs` decides from variable names only. No secret value is ever read into
a message.

| Situation | Result |
| --- | --- |
| No signing secret is set | Warning `No <platform> signing secrets are set ...; building UNSIGNED`; the build succeeds. |
| Some, but not all, required secrets are set | Error naming the missing variables. A half-configured release never ships unsigned. |
| `REQUIRE_SIGNING=true` and no secret is set | Error. Set it with the `require_signing` input of the manual run or the repository variable `REQUIRE_SIGNING`. |
| Tag build (`v*`) and no secret is set | Error, like `REQUIRE_SIGNING=true` (on macOS this also requires the notarization secrets). To build an unsigned release on purpose, set the repository variable `ALLOW_UNSIGNED_RELEASE` to `true`; the artifact is then still labeled `unsigned`. |
| All signing secrets set | Signed build; verified afterwards. |

The GitHub environment `release` gates the build jobs and the `publish` job, so configure it before the first
tag, even while releases are unsigned. Signing secrets, once added, also live there. Configure:

- **Deployment branches and tags**: select *Selected branches and tags* and allow only the tag
  pattern `v*`. The release-please flow needs no branch: its run is dispatched on the tag, so the
  build and publish jobs see a tag ref. An environment that does not exist is created on first use
  **without protection**, so any branch could then run `workflow_dispatch` and read the secrets; the
  restriction is the control, the workflow cannot enforce it.
- **Required reviewers**: none today. A release run therefore builds and publishes without an
  approval once its tag exists. Add reviewers only if an approval must gate the release; a required
  reviewer holds the release-please run at the build job until someone approves it.
- **Tag protection**: no tag ruleset exists today. A ruleset that restricts `v*` tag creation must
  allow the `github-actions[bot]` actor, or the release-please job cannot create the tag.
- Keep the secrets in the environment only, not at repository or organization level.

| Platform | Signing | Notarization |
| --- | --- | --- |
| macOS | `APPLE_CERTIFICATE` (base64 `.p12`), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` | `APPLE_ID`, `APPLE_PASSWORD` (app-specific), `APPLE_TEAM_ID`; all three or none |
| Windows | `WINDOWS_CERTIFICATE` (base64 `.pfx`), `WINDOWS_CERTIFICATE_PASSWORD` | not applicable |

Optional variable: `WINDOWS_TIMESTAMP_URL` (default `http://timestamp.digicert.com`). Empty
variables are dropped before Tauri runs, because Tauri treats a variable that is set but empty as
configured. On macOS Tauri imports the certificate into a temporary keychain and, with the
notarization variables, notarizes; afterwards `codesign --verify --deep --strict` (and `stapler
validate` plus `spctl` when notarized) must pass. On Windows the certificate is imported into the
current-user store, passed to Tauri as `certificateThumbprint` through `TAURI_CONFIG`, checked with
`Get-AuthenticodeSignature` (status `Valid`), and removed afterwards. `package.mjs` removes
`WINDOWS_CERTIFICATE` and `WINDOWS_CERTIFICATE_PASSWORD` from its own environment before it spawns
anything: only the PowerShell call that imports the certificate receives the password, so the
`npm run package:win` build, Tauri and cargo build scripts never see either value. The macOS
`APPLE_*` variables must reach the build, because Tauri signs and notarizes itself.

`apps/desktop/scripts/prepare-treehouse-sidecar.mjs` runs in `package.mjs` after the codeburn staging and
bundles `treehouse` (macOS) or `treehouse.exe` (Windows) beside the daemon. `scripts/release/package.mjs` fails a macOS
build whose app bundle lacks `Contents/MacOS/treehouse`. The install smoke requires that file in `Contents/MacOS`, checks
that it carries the app's team identifier, and on Windows checks its Authenticode status with the daemon's. Upstream
treehouse is unsigned, so its SHA-256 pin is enforced at staging only: Tauri re-signs the file when it bundles it.

`CARGO_TARGET_DIR` must be absolute; any separator style is accepted and normalized, so
`${{ runner.temp }}/target` (`D:\a\_temp/target`) works on Windows.

## Publishing a release

The `publish` job runs for a `v*` tag, and only after both platform builds passed, signed or unsigned.
It runs for a tag push, or for a `release.yml` run on a tag that set `publish`. A run on a branch never
reaches it, and neither does a manual run without `publish`. The job uses the `release` environment (see
[Release builds and signing](#release-builds-and-signing)) and holds no signing secrets.

A release starts in one of two ways:

- **From a merged release pull request (the normal path).** release-please makes a draft release for the
  new version when its pull request merges. `release-please.yml` then creates the annotated tag at the
  merge commit and dispatches `release.yml` on that tag with `publish=true`. The run's ref is the tag,
  so the release environment's `v*` rule applies to it, and `GITHUB_REF_TYPE` is `tag` for the signing
  and version checks.
- **From a tag push (manual).** A person pushes an annotated `v*` tag. No draft exists, so the job
  creates one.

The job then does the following:

1. Check out the tag and read the four version sources: `apps/desktop/src-tauri/tauri.conf.json`,
   `apps/desktop/package.json`, the `[workspace.package]` table of `Cargo.toml` and the `[package]`
   table of `apps/desktop/src-tauri/Cargo.toml`. The tag must equal `v` plus each of them.
2. Download the `release-*` artifacts of both builds. Each build's `release-info.json` must describe one
   macOS and one Windows build of that version, from one commit, with one installer each. Each installer
   must match the sha256 its build recorded.
3. Write `SHA256SUMS` and the release notes (`scripts/release/publish.mjs`, unit-tested by
   `scripts/release/publish.test.mjs`). The notes carry the version's section of `docs/changelog.md`.
   The script fails if that file has no section for the version.
4. In the `Publish the GitHub release` step, the only step that sees the token: create the release as a
   draft if none exists (`--verify-tag`), upload the installers and `SHA256SUMS`, then publish it with
   `gh release edit --draft=false --verify-tag`. The installers are attached before the release is public.

The release is named after the tag. Its assets are the two installers and `SHA256SUMS`, named
`MCAO-X.Y.Z-macos-arm64-<signed|unsigned>.zip` and `MCAO-X.Y.Z-windows-x64-<signed|unsigned>-setup.exe`.

While releases are unsigned (the current mode), every release is a prerelease. Its notes open with the
unsigned warning, then the version's changes, then the installers, the commit and how to check a
download (see [Unsigned releases](#unsigned-releases)). A release with only signed installers is not a
prerelease, and its notes carry no warning.

A failed check stops the job before anything is published, and a draft stays unpublished. To retry
after a fix to the workflow, re-run the job from the run page. If a release was published by mistake
and nobody has downloaded it, delete it with `gh release delete <tag> --yes` (the tag is kept), then
re-run the job.

## Unsigned releases

Releases are unsigned today. No signing secrets are set in the `release` environment, so the
installers are not code-signed or notarized. This is the current mode, not an exception.

- The repository variable `ALLOW_UNSIGNED_RELEASE` is `true`, set once for the repository; maintainers do
  not set it per release. Without it, a `v*` tag build fails at the signing check, on purpose.
- The variable is repository-wide. While it is `true`, any tag build with no signing secrets ships
  unsigned: labelled `unsigned`, and a prerelease. If the secrets are ever removed, set the variable to
  `false` to get the failure back.
- Every unsigned release is a prerelease, and its notes open with the warning and the install steps below.

These steps apply because the app is unsigned and internal:

- macOS: Gatekeeper blocks the first launch of an unsigned app. Control-click Open no longer bypasses it on
  macOS 15 and later. Either try to open MCAO once, then open System Settings, Privacy & Security, and click
  Open Anyway; or move `MCAO.app` to `/Applications` and run
  `xattr -dr com.apple.quarantine /Applications/MCAO.app` in Terminal.
- Windows: SmartScreen shows "Windows protected your PC" when the installer runs. Choose More info, then
  Run anyway.
- The install smoke of an unsigned build runs with `--allow-unsigned`. It proves the artifact installs and
  upgrades. It proves nothing about Gatekeeper or SmartScreen.

To turn signing on later:

1. Add the secrets from the table above to the `release` environment: the Windows pair, and for macOS the
   signing trio plus the notarization trio, all or none.
2. Set nothing else. A complete set makes the next build signed, and notarized on macOS. The notes and the
   prerelease flag follow the build. `ALLOW_UNSIGNED_RELEASE` has no effect on a signed build, so it can stay
   as it is. Signed on macOS without notarization is still labelled and still warns under Gatekeeper.

## Run it locally

```sh
node --test "scripts/**/*.test.mjs"
node scripts/ci/run-gates.mjs --list
# Windows: refuses a SUBST/mapped drive, a junction, a UNC path or a root over 64 characters (exit 3).
node scripts/ci/run-gates.mjs --preflight --evidence-dir <new> --target-dir <new empty> --test-tmp <new>
# Needs a clean, committed tree; the evidence dir must not exist; the target dir must be empty.
CI=true node scripts/ci/run-gates.mjs \
  --evidence-dir ../evidence-run --target-dir ../target-run --test-tmp ../tt
```

Exit codes: 0 `PASS`, 1 `FAIL`, 2 `INCONCLUSIVE` (a gate was skipped), 3 preflight refused the paths.
Keep the test temp path short: macOS Unix socket paths are limited to about 104 characters.
`--only a,b` runs a subset (later gates that need an earlier one are then `SKIPPED`).

## Windows verification

The Windows agent must run these natively on a Windows host. None of it has been run on Windows by
the M6-02 owner.

1. `git config --global core.autocrlf false` and `git config --global core.longpaths true`, then a
   clean checkout of the commit under test in a real directory with a short path (for example
   `C:\t\<8 hex>`). Never a `SUBST` or mapped drive, a junction or a UNC share: Round 4 failed with `TS2883`
   because of a `Z:` `SUBST`. If `C:\t` cannot be created, use another real short directory on a local drive and
   say which. Run `node scripts/ci/run-gates.mjs --preflight --evidence-dir <new> --target-dir <new empty> --test-tmp <new>`
   first; it must print `PREFLIGHT PASSED`.
2. Toolchain: Rust 1.96.0 (MSVC), Node 22.20.0, npm, Visual Studio Build Tools with the C++
   workload, Windows SDK, network access (NSIS download). Record versions; a missing tool is
   `BLOCKED`.
3. `node --test "scripts/**/*.test.mjs"`: the parser tests (including CRLF output) must pass; record the exit
   code. The harness runs them again as its first gate.
4. `node scripts/ci/run-gates.mjs --list` must show the 27 Windows gates, including `node-self-tests`,
   `windows-transport-tests`, `git-lib-tests`, `private-fs-tests`, `store-tests`, `lock-takeover-test`,
   `diagnostics-tests`, `cli-build`, `p12-windows-checks` and `desktop-package-win`.
5. `$env:CI = "true"; node scripts/ci/run-gates.mjs --evidence-dir <new> --target-dir <new empty> --test-tmp <new>`
   Expected: every gate `PASS` with the derived counts in the gate table, `OVERALL: PASS` and exit 0. Any
   `SKIPPED` gate is `INCONCLUSIVE`, not a pass: fix the failing prerequisite and run again. A count mismatch is a
   finding: report it; do not edit `scripts/ci/expected.mjs` to fit.
6. Confirm `git-config`, `gitattributes-before/after` and `source-status-before/after` pass, that
   every `<gate>.survivors.txt` says `none`, and that the run leaves the tree clean.
7. With a throwaway test certificate (never a real one), run
   `node scripts/release/package.mjs windows --out <new>` with `CARGO_TARGET_DIR` set to an
   absolute path written as the runner writes it, with mixed separators:
   `$env:CARGO_TARGET_DIR = "$env:RUNNER_TEMP/target"` (the old check rejected this). Run it:
   - with no secrets: expect an unsigned build and the warning;
   - with `REQUIRE_SIGNING=true`: expect an error;
   - with `GITHUB_REF_TYPE=tag` and `GITHUB_REF_NAME=v<version>` and no secrets: expect an error
     that names `ALLOW_UNSIGNED_RELEASE`; with `ALLOW_UNSIGNED_RELEASE=true` added, an unsigned
     build;
   - with a test `.pfx` in `WINDOWS_CERTIFICATE` and `WINDOWS_CERTIFICATE_PASSWORD`: expect
     `Valid` only if the test certificate chains to a trusted root (record the real status either
     way), and that the thumbprint is gone from `Cert:\CurrentUser\My` afterwards.
8. Run the by-hand checks that a script can do, after the harness and with the same target directory, from a
   short path (no `$ErrorActionPreference = 'Stop'` around it; it never halts and exits 0):
   `powershell -NoProfile -File scripts\ci\p12-windows-checks.ps1 -TargetDir <the harness --target-dir> -RunRoot C:\ao\p12 -EvidenceDir <evidence dir>`.
   It prints `icacls` for the data directory and every file in it, asserts only the current user with no
   inherited ACE and no `CodexSandboxUsers`, `SYSTEM`, Administrators, Users, Everyone or Authenticated Users, runs
   the CLI against an over-limit objective for a `.cmd` agent launcher (8,191 units), and ends with
   `P12-WINDOWS-CHECKS: PASS`, `FAIL (n of m failed)` or `INCOMPLETE`. The harness runs it as the
   `p12-windows-checks` gate; run it by hand only if you also read the last `P12-WINDOWS-CHECKS:` line
   case-sensitively (`-cmatch '^P12-WINDOWS-CHECKS: '`). It does not check `daemon.log` (only the desktop host
   creates it; printed as `INFO acl-daemon.log: NOT CHECKED`). It writes `manual-p12.txt` and redacts the user,
   domain, host and e-mail in every line it prints. The redacted
   launch failure in the real window is the one check it cannot do (`NOT RUN`). A literal 65,000-character
   objective cannot be given to the CLI on Windows: `CreateProcess` refuses a command line over 32,767 units
   before the CLI starts, so that path is only reachable through the daemon (`tasks.submit`), which the
   `daemon-tests` unit test covers.
9. Report the real counts, durations, and the first failing gate, if any.

## What only a real GitHub runner can prove

Local checks (below) prove the YAML, the shell and the harness logic. They do not prove:

- that the runner images `macos-15` and `windows-2025` have the expected Xcode, MSVC, Windows SDK,
  WebView2 and rustup, or that `setup-node` can install Node 26.13.0 and 22.20.0;
- the Windows survivor scan (`Get-CimInstance Win32_Process` through PowerShell) and the Windows
  `runBounded` pipe and timeout behavior, which only the self-tests exercise, on macOS;
- Windows gate counts and the Windows clippy baseline at these toolchain versions (they are the
  runbook's derived values until a Windows host measures them);
- the NSIS download, the Windows installer build and the sidecar being bundled;
- that `cargo install --locked` builds the pinned `cargo-deny` and `cargo-audit` within the job's
  time limit and that the advisory database is reachable from the runner;
- the Apple and Windows signing, notarization and timestamp paths with real certificates;
- artifact upload, `environment: release` behavior, and wall-clock time against the 180-minute
  and 120-minute limits;
- the `publish` job on a real tag: the download of both builds, the version check, the draft and upload,
  the publish step, the prerelease flag and the release assets. A branch run skips that job, so the first
  `v*` tag run (from a merged release pull request, or a tag push) is its first real run;
- the `release-please.yml` path end to end: the release pull request, the annotated tag, and the dispatch
  of `release.yml` on the tag, which depends on the `release` environment accepting a tag-ref run from a
  token-created dispatch.

Not covered by M6-02: updater signing keys and update metadata, and the Windows desktop e2e (it needs
a Unix socket). Installing the artifact is M6-04, below.

## Install and upgrade smoke (M6-04)

`release.yml` runs `scripts/release/install-smoke.mjs` on the artifact it just built, after `Package`
and before the artifact upload: it checks `SHA256SUMS`, `release-info.json`, the file name against the
`signed` flag and the app version; extracts (macOS) or silently installs (Windows) the artifact; checks
signatures (`--require-signed` when the build was signed, otherwise `--allow-unsigned` plus a warning; exactly one
of the two flags is mandatory); and runs the `release_smoke` example of
`orchestrator-daemon` against the **installed** sidecar through install, cancellation, hard-kill recovery,
approvals and orderly shutdown. The smoke binary's exit code is not trusted: its structured `--out` result must
exist and validate (12 ordered steps, versions, packaged sidecar paths). A same-version run is labeled
`SAME-VERSION rerun; NOT an upgrade`, recorded as `upgrade.kind` in `install-smoke.json`, and the N-1 to N upgrade
is reported UNPROVEN. A failure keeps the artifact from being uploaded; `install-smoke.json` is
uploaded as `install-smoke-<platform>` either way. The Windows flow also upgrades over an existing install,
checks with a sentinel file in the real `%LOCALAPPDATA%\agent-orch` that neither the upgrade nor the uninstall
removes user data (FAIL if `LOCALAPPDATA` is unset), and uninstalls with `/S _?=<dir>`.

It is not a clean machine and does not open the GUI. Real signing, notarization, Gatekeeper, SmartScreen,
a previous-release upgrade and the clean-machine procedures are **UNPROVEN** and written up with expected
results in `docs/evidence/m6-04-clean-machine-runbook.md`. The Windows flow, the `release_smoke` Windows
branches and the new workflow step have never run on Windows or on a runner. The pure logic and both
flows are covered by `scripts/release/install-lib.test.mjs` against a fake machine
(`node --test "scripts/**/*.test.mjs"`); they add no `run-gates.mjs` gate, but they raise the `node-self-tests`
count from 71 to 97 (95 after M6-04, plus the 2 M6-04r added). The other gate counts above are unchanged.

### Windows path simulation (M6-04r)

The first native Windows run failed `install-lib.test.mjs` (94 of 95): a test spelled the stand-in paths as `/w/...`,
the code builds them with `node:path`. Every expected path in the file is now built with `path.join`, and
`SHA256SUMS` and PowerShell output are parsed with CRLF line endings. To check that without a Windows host:

```sh
node --import ./scripts/release/win32-path-sim/register.mjs --test scripts/release/install-lib.test.mjs
```

The hook swaps `node:path` for `path.win32` in `install-lib.mjs` and its test only (the rest of the scripts touch the
real file system). Control: the M6-04 test file (`632c77d`) passes 24 of 24 on macOS and fails exactly the test
the Windows run failed (`macOS signed with a genuinely older build`) under the simulation. The simulation proves
separator agreement only; drive letters, the real file system, PowerShell and the installer are not exercised.

### What the install smoke records about itself (M6-04r)

- Authenticode status is measured with `-EncodedCommand` (no quoting of the script is possible) and without the
  parent's `PSModulePath`. A status that could not be read is `Unknown` with the exit code and the first line of
  stderr, never blank: `INFO new: UNSIGNED installer (Authenticode status Unknown)` followed by `the status was NOT
  measured: ...`. An unsigned installer that was measured reads `NotSigned`. A signed artifact whose status is
  `Unknown` fails.
- `install-smoke` prints, and writes under `source` in `--out`, the commit, `HEAD^{tree}`, the porcelain status of
  the checkout before and after the run, `changed by this run` (paths that appeared or vanished) and `cleanAfterRun`.
  A dirty checkout after a run is therefore explained by the paths it names; it is reported, never judged.

### M7 final integration counts

The M7 merge (adapters, conversation runtime, queue snapshot, host metrics, workstreams) moved the counts. macOS is
measured on the merged branch; Windows is derived from `cfg` gates, not measured:

| Gate | macOS (measured) | Windows (derived) |
| --- | --- | --- |
| `ipc-tests` | 55 `[55,0]` | 55 `[55,0]` (all new tests are `cfg(any(unix, windows))`) |
| `daemon-tests` | 208 + 1 ignored `[175,14,2,4,11,1,2]` | 138 + 1 ignored `[118,12,2,4,0,1,2]` |
| `store-tests` | 157 + 1 ignored `[65,9,2,13,11,8,11,3,4,3,11,5,1,12,0]` | 153 + 1 ignored `[63,9,2,13,11,8,11,3,3,3,11,4,1,12,0]` |
| `lock-takeover-test` | 1 (64 filtered out) | 1 (62 filtered out) |
| `workspace-tests` | 981 + 2 ignored | not a Windows gate |
| `desktop-vitest` | 51 files, 1105 tests | same |
| `desktop-e2e` | 8 files, 44 tests | not a Windows gate |
| `desktop-geometry` | 35 tests: 34 in `e2e/geometry.spec.ts`, 1 in `e2e/topbar-usage-viewport.spec.ts` (run as `playwright test`) | not a Windows gate |

On Windows the daemon library excludes `executor::tests`, `conversation_orphans`, `conversation_agent::launch` and
four other unix-only tests, and gains `windows_submit_refuses_an_objective_that_cannot_be_passed_to_the_agent`;
`conversation_runtime` loses its two unix-only tests; `tests/daemon.rs` is `cfg(unix)`. The M6 supervisor clears the
child environment and never forwards `AGENT_ORCH_*` names, so the conversation and queue-snapshot fake agents read
`FAKE_AGENT_LOG`, `FAKE_AGENT_PID_FILE` and `QUEUE_AGENT_DIR`, listed in `AGENT_ORCH_ENV_PASSTHROUGH`. The name lists
`macos-test-names.json` and `windows-test-names.json` were regenerated from the measured output.
