# Release CI (M6-02)

Native macOS and Windows gates, dependency audits, and signed or unsigned release builds on
GitHub Actions. Nothing here publishes a release.

## Workflows

| File | Runs on | Triggers | What it does |
| --- | --- | --- | --- |
| `.github/workflows/ci.yml` | `macos-15`, `windows-2025` | pull request, push to `main`, manual, called by `release.yml` | Sets `core.autocrlf=false` and `core.longpaths=true`, then runs every native gate once with `CI=true`, its own target directory and an isolated test temp directory. |
| `.github/workflows/audit.yml` | `ubuntu-24.04` | changes to manifests, lockfiles or `deny.toml`; push to `main`; daily; manual; called by `release.yml` | `cargo deny` for both manifests, `cargo audit` for both lockfiles, `npm audit --audit-level=high`. |
| `.github/workflows/release.yml` | `macos-15`, `windows-2025` | tag `v*`, manual | Requires `ci.yml` and `audit.yml` to pass, then builds the macOS app and the Windows NSIS installer. Signs when secrets are present. Uploads artifacts only. |

Every third-party action is pinned to a commit SHA. Workflows have `contents: read` and no
other permission. Secrets are mapped only into the `Package` step of `release.yml`. `ci.yml` and
`audit.yml` cancel a superseded run only for a pull request; a scheduled, `main` or release run
always completes.

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
| `node-self-tests` | `node --test "scripts/**/*.test.mjs"` (71 tests on both; the exit code is in `node-self-tests.result.json`) | both |
| `daemon-build` | `cargo build --locked -p orchestrator-daemon` | both |
| `ipc-tests` | `cargo test --locked -p orchestrator-ipc` (46 tests) | both |
| `windows-transport-tests` | `cargo test --locked -p orchestrator-ipc windows_transport` (13 tests) | Windows |
| `cli-build` | `cargo build --locked -p orchestrator-cli` (shape: a `Finished` line and the binary; cached output is fine) | Windows |
| `p12-windows-checks` | `pwsh.exe -NoProfile -File scripts\ci\p12-windows-checks.ps1 -TargetDir {target} -RunRoot {testTmp}\p12 -EvidenceDir {evidence}`; the last `P12-WINDOWS-CHECKS:` line must be `PASS` (see below) | Windows |
| `daemon-tests` | `cargo test --locked -p orchestrator-daemon` (Windows 57 = 52+4+0+1, macOS 88 = 73+4+10+1: unit, `coordinator_review`, `daemon.rs`, `diagnostics_log`) | both |
| `daemon-lifecycle` | `cargo test --locked -p orchestrator-daemon --test lifecycle -- --nocapture` (`STEP 1` to `STEP 8`, one `lifecycle: PASS`) | both |
| `coordinator-review-tests` | `cargo test --locked -p orchestrator-daemon --test coordinator_review` (4 tests) | both |
| `git-lib-tests` | `cargo test --locked -p orchestrator-git --lib` (49 on both: Windows 47 portable + 2 `cfg(windows)`, macOS 47 portable + 2 `cfg(unix)`) | both |
| `cli-cleanup-safety-tests` | `cargo test --locked -p orchestrator-cli --test cleanup_safety` (Windows 3, macOS 4: one test is `cfg(unix)`; a fifth is `cfg(all(unix, feature = "fault-injection"))` and runs nowhere by default) | both |
| `private-fs-tests` | `cargo test --locked -p orchestrator-private-fs` (Windows `[11,0]`: 4 portable + 7 DACL; macOS `[8,0]`: 4 portable + 4 Unix; the `0` is the doc-test binary) | both |
| `store-tests` | `cargo test --locked -p orchestrator-store` (Windows 100 passed + 1 ignored, derived; macOS 104 passed + 1 ignored, measured: 4 tests are `cfg(unix)`; the ignored test is in `data_lock`) | both |
| `lock-takeover-test` | `cargo test --locked -p orchestrator-store --lib the_lock_file_is_owner_only_when_created_and_when_taken_over` (1 passed; 32 filtered out on Windows, 34 on macOS) | both |
| `diagnostics-tests` | `cargo test --locked -p orchestrator-diagnostics` (28 on both: no test is `cfg`-gated) | both |
| `runtime-command-line-tests` | `cargo test --locked -p orchestrator-runtime --lib command_line` (8 tests; the pure Windows rules run on every host) | both |
| `cli-command-line-tests` | `cargo test --locked -p orchestrator-cli --lib -- os_command_line_limit 65000_character_objective` (Windows 2, macOS 1: the 65,000-character test is `cfg(windows)`) | both |
| `clippy-scoped` | `cargo clippy --locked -p orchestrator-ipc -p orchestrator-daemon --all-targets` (baseline: two `single_match` warnings in `supervisor.rs`) | both |
| `workspace-tests` | `cargo test --locked --workspace` (726 passed, 1 ignored) | macOS |
| `desktop-npm-ci`, `desktop-tsc`, `desktop-build`, `desktop-vitest` | in `apps/desktop`: `npm ci`, `npx tsc -b`, `npm run build`, `npx vitest run` (15 files, 290 tests) | both |
| `desktop-e2e` | `npm run test:e2e` (1 file, 19 tests, real daemon over a Unix socket) | macOS |
| `tauri-build`, `tauri-tests`, `tauri-clippy-clean`, `tauri-clippy` | in `apps/desktop/src-tauri`: `cargo build --locked`, `cargo test --locked` (Windows 14, macOS 16), `cargo clean -p agent-orch-desktop`, `cargo clippy --locked --all-targets --verbose` | both |
| `desktop-package-win` / `desktop-package-mac` | `npm run package:win` (NSIS installer) / `npm run package:mac` (`.app`) | Windows / macOS |

Windows and macOS counts differ because tests are gated by `cfg`. Expected counts are never edited
to fit a run; a change needs a reason in the commit that edits it.

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

Edit `scripts/ci/expected.mjs` and the matching list in `scripts/ci/windows-test-names.json` /
`macos-test-names.json` together; `node --test "scripts/**/*.test.mjs"` fails when a total, a
sequence and a name list disagree. Measure on the platform; do not edit to fit a run. The two
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

- `bounded.test.mjs` `EBUSY` on cleanup: Node's synchronous `rmSync` ignores `maxRetries` for `EBUSY`, so the
  round 3 retry option did nothing. Cleanup is now `scripts/ci/remove-dir.mjs`, an awaited retry loop over the
  asynchronous `rm`, the test's grandchild no longer starts with the temp directory as its working directory,
  and the timeout test waits until the grandchild is gone before it removes anything. `runBounded` now returns
  the pid it launched.
- The by-hand checks' verdict was read as an empty string: the runbook's filter was case-insensitive, so the
  script's two banner lines (`p12-windows-checks: ...`) counted as verdicts. The banners now start with `INFO`,
  the harness has a `p12-windows-checks` gate, and `parse.mjs#checkP12` reads the last
  `P12-WINDOWS-CHECKS: ` line, case-sensitively, from a real round 3 Windows output kept in
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
  the summary (`# pass N`, or `ℹ pass N`) checked for 71 tests and no fail, cancelled, skipped or todo, and the exit
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
`diagnostics-tests` 28; `lock-takeover-test` 1. Node self-tests: 71, measured on macOS (no test is
platform-gated).

### Maximum path lengths on Windows

- **Worktree path: 215 characters.** `git worktree add` runs its checkout with `GIT_DIR=<worktree>\.git`, and
  Git dies with ``'$GIT_DIR' too big`` when that exceeds `PATH_MAX - 40` = 220 characters, whatever `core.longpaths`
  says (observed with Git 2.46.2 at a worktree path over 260 characters; the 215 boundary is derived from
  Git's source and its macOS analogue, 969 accepted and 981 refused, not yet measured on Windows).
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
- `npm audit --audit-level=high` runs against `apps/desktop/package-lock.json`.
- **Known exception:** `RUSTSEC-2026-0459` (bundled SQLite before 3.54.0 in `libsqlite3-sys`
  0.37.0, reached through `sqlx` 0.9.0) is ignored in `deny.toml` and `.cargo/audit.toml` with a
  reason. It is a tracked finding, not an acceptance. Remove both entries once the lockfile has
  `libsqlite3-sys` 0.39 or later.
- MPL-2.0 is in the license allow-list because the desktop graph contains it. Legal needs to
  decide this before a public release.

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
| Tag build (`v*`) and no secret is set | Error, like `REQUIRE_SIGNING=true` (on macOS this also requires the notarization secrets). To build an unsigned release on purpose, set the repository variable `ALLOW_UNSIGNED_RELEASE` to `true`; the artifact is then still labelled `unsigned`. |
| All signing secrets set | Signed build; verified afterwards. |

Secrets live in the GitHub environment `release`. Configure it before the first release:

- **Deployment branches and tags**: select *Selected branches and tags* and allow only the tag
  pattern `v*` (add `main` only if manual signed runs from `main` are wanted). An environment that
  does not exist is created on first use **without protection**, so any branch could then run
  `workflow_dispatch` and read the secrets; the restriction is the control, the workflow cannot
  enforce it.
- **Required reviewers**: add at least one, so a tag push waits for approval before the secrets are
  released to the job.
- **Tag protection**: add a repository ruleset that restricts who can create or delete `v*` tags;
  a pushed tag is what starts a release.
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

`CARGO_TARGET_DIR` must be absolute; any separator style is accepted and normalized, so
`${{ runner.temp }}/target` (`D:\a\_temp/target`) works on Windows.

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
  `runBounded` pipe and timeout behaviour, which only the self-tests exercise, on macOS;
- Windows gate counts and the Windows clippy baseline at these toolchain versions (they are the
  runbook's derived values until a Windows host measures them);
- the NSIS download, the Windows installer build and the sidecar being bundled;
- that `taiki-e/install-action` installs the pinned `cargo-deny` and `cargo-audit` and that the
  advisory database is reachable from the runner;
- the Apple and Windows signing, notarization and timestamp paths with real certificates;
- artifact upload, `environment: release` behaviour, and wall-clock time against the 180-minute
  and 120-minute limits;
- that reusable-workflow `workflow_call` composition in `release.yml` resolves on the default
  branch.

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
exist and validate (12 ordered steps, versions, packaged sidecar paths). A same-version run is labelled
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
(`node --test "scripts/**/*.test.mjs"`); they add no `run-gates.mjs` gate, so the gate counts above are
unchanged.
