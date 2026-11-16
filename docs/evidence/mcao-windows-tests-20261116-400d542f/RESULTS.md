# Windows test report — motif MCao snapshot

## Target and environment

- Source branch: `snap/win-mcao-400d542f`
- Source commit: `ea5b5abca917ff9bf8799a486b2e425afa755a87`
- Git tree: `0a0f785bf6bda634fcf48b7c3befec07a29ebd56`
- Checkout file count: 1445; clean before and after checks.
- Host: Windows 11 Education 10.0.26200 x64; Windows SDK 10.0.22000.0; MSVC 19.38.33133.
- Tools: Git 2.46.2.windows.1; Rust/Cargo 1.96.0; Node 22.20.0; npm 11.3.0; PowerShell 7.5.4.
- Harness Git settings: `core.autocrlf=false`, `core.longpaths=true`; required LF attributes passed before and after.
- Initial free C: space: 32,814,673,920 bytes.

## Result

The pinned Windows gate harness ran all 27 gates: 25 PASS, 2 FAIL, 0 SKIPPED; overall `FAIL` (exit 1). All seven harness integrity/privacy checks passed, including checkout status, attributes, and privacy scan. The runner completed and wrote `summary.md` plus `overall.result.json`; a post-run PowerShell display wrapper then encountered the JSON key case collision between `FAIL` and `fail`, so the verdict/counts here are taken from the runner artifacts.

| Check | Result | Evidence |
|---|---|---|
| Harness self-tests: `node --test "scripts/**/*.test.mjs"` | FAIL, exit 1; 131 passed, 1 failed out of 132. The failing workflow-table test opens `.github/workflows/ci.yml`, which is absent from this history-free snapshot. The runbook documents this expected snapshot omission. The runbook expects 112 tests, while this snapshot reports 132. | `node-self-tests.stdout.txt`, `node-self-tests.stderr.txt`, `node-self-tests.result.json` |
| `cargo test --locked --workspace` | PASS, exit 0; 1,000 passed, 0 failed, 2 ignored; 903.078 s. | `cargo-workspace-test.log`, `cargo-workspace-test.result.json` |
| `cargo clippy --locked --workspace --all-targets` | PASS, exit 0; 0 errors, 9 warning diagnostics; 103.671 s. | `cargo-workspace-clippy.log`, `cargo-workspace-clippy.result.json` |
| Desktop `npm ci` | PASS, exit 0; 17.162 s. | `desktop-npm-ci.log`, `desktop-npm-ci.result.json` |
| Desktop `npx tsc -b` | PASS, exit 0; 17.161 s. | `desktop-tsc.log`, `desktop-tsc.result.json` |
| Desktop `npm run build` | PASS, exit 0; 3.451 s. | `desktop-build.log`, `desktop-build.result.json` |
| Desktop `npx vitest run` | PASS, 79 files and 1,662 tests; 124.626 s. | `desktop-vitest.log`, `desktop-vitest.result.json` |
| Desktop `npm run test:browser` | PASS, 37/37 tests using installed Chrome; 182.527 s. No browser was downloaded. | `desktop-browser.log`, `desktop-browser.result.json` |
| Windows NSIS `npm run package:win` | FAIL, exit 1; after compiling the release daemon, `prepare-codeburn-sidecar` rejected staged `node.exe`: Authenticode status was empty, expected `Valid`. No installer was produced, so no installer SHA-256 is available. | `desktop-package-win.log`, `desktop-package-win.result.json`; harness copy in `full/evidence/` |
| Windows `npm run test:e2e` | N/A on Windows: the runbook identifies this suite as Unix-socket based and macOS-only. | Runbook instruction |

The full per-gate results and raw stdout/stderr are in `full/evidence/`; the harness summary is `full/evidence/summary.md` and machine result is `full/evidence/overall.result.json`. The Windows E2E/UI scenarios listed by the harness as unproven remain unverified by construction.

## Delivery

The original agent-bus channel could not be used: neither `busctl` nor an agent-bus result tool was available, so no `busctl accept` or `busctl result` was issued. The hub task may still show pending.

At the user's later explicit request, this report and all sanitized test evidence are being published to `agent-orch` on branch `codex/windows-mcao-test-evidence-20261116-400d542f`. The private source snapshot and generated build/dependency caches are excluded. The separate agent-bus task record is not changed by this Git publication.
## Runbook step ledger and acceptance follow-up

| Runbook step | Status | Command / observation | Duration and evidence |
|---|---|---|---|
| 3.1 harness self-tests | FAIL (snapshot omits `.github/workflows`) | `node --test "scripts/**/*.test.mjs"`; 131/132 pass, expected 112 vs observed 132. One workflow-table test cannot open `.github/workflows/ci.yml`. | 4.696 s; `node-self-tests.log`, `full/evidence/node-self-tests.*` |
| 3.2 gate list | PASS | `node scripts/ci/run-gates.mjs --list`; 27 gates, matching the pinned Windows verification section. | `gate-list.log`, `gate-list.result.json` |
| 3.3 preflight | PASS | Full-run preflight printed `PREFLIGHT PASSED (win32)` for source, working directory, evidence, target, and test temp roots. | `full-preflight.log`, `full-preflight.result.json` |
| 3.4–3.5 full harness | FAIL | 27 gates; 25 PASS, 2 FAIL, 0 SKIPPED; exit 1. Failing gates: `node-self-tests`, `desktop-package-win`. | `full/evidence/summary.md`, `full/evidence/overall.result.json`, per-gate stdout/stderr/result files |
| 3.6 integrity/survivors | PASS | Git config, gitattributes before/after, source status before/after, source diff, privacy scan all PASS; all 27 survivor files contain `none`; pinned source remains clean. | `full/evidence/*-*.result.json`, `full/evidence/*.survivors.txt` |
| 4.1 workspace tests | PASS | `cargo test --locked --workspace`; 1,000 passed, 0 failed, 2 ignored. | 903.078 s; `cargo-workspace-test.log`, `.result.json` |
| 4.2 workspace Clippy | PASS | `cargo clippy --locked --workspace --all-targets`; exit 0, 9 warning diagnostics, 0 errors. | 103.671 s; `cargo-workspace-clippy.log`, `.result.json` |
| 4.3 desktop sequence | PASS | `cmd /c "cd /d apps\desktop && npm ci && npm test && npm run build"`; `npm test` reports 79 files and 1,662 tests. | 117.828 s; `accept-desktop-sequence.log`, `.result.json` |
| 4.3 individual desktop checks | PASS | `npm ci`, `npx tsc -b`, `npm run build`, `npx vitest run`; 79 files, 1,662 tests. | `desktop-*.log`, `.result.json` |
| 4.4 browser checks | PASS | `npm run test:browser` and `npx playwright test`; each 37/37 using installed Chrome. | 182.527 s and 182.386 s; `desktop-browser.*`, `desktop-playwright.*` |
| 4.5 Unix-socket E2E | N/A on Windows | `npm run test:e2e` is documented as Unix-socket based and macOS-only. | Not run per runbook |
| 4.6 Windows package | FAIL | `npm run package:win`; release daemon compiles, then `prepare-codeburn-sidecar` rejects staged `node.exe` because Authenticode status is empty, expected `Valid`. | 242.76 s; `desktop-package-win.log`, `.result.json`; harness copy under `full/evidence/` |
| Task acceptance Node command | FAIL (snapshot omits `.github/workflows`) | `node --test scripts/ci/*.test.mjs`; 71/72 pass; same workflow-table test fails with `ENOENT`. | 1.935 s; `accept-node-ci-only.log`, `.result.json` |

No installer was emitted, so there is no installer file or SHA-256. Task acceptance `busctl accept`/`busctl result` could not be run: neither `busctl` nor an agent-bus result tool is available in this environment. Hub task acceptance and return through agent-bus remain `BLOCKED-EXTERNAL`; the sanitized Git evidence publication is recorded above. Keep the original local run directory intact until the hub result is accepted.

## Task state and setup notes

- The hub `agent-bus` branch remains at commit `2a136717a8a387463f126f29803d99461243168d` with task `20261116-0002-win-mcao-tests-400d542f` newly assigned; that task has no claim/result artifact in the current hub state. `busctl` is unavailable, so this run could not accept or return it through the task channel.
- The related `20261116-0001-win-mcao-e2e-f257a874` task is explicitly cancelled in the hub with reason “superseded” and was not run. Its snapshot branch was not treated as an active assignment.
- An initial setup attempt was preserved separately at `%TEMP%\9dd98c98\attempt-1-post-clone-status.txt`; it hit a denied `C:\t` creation and an autocrlf-dirtied checkout before tests. The valid run was restarted with process-scoped/checkout-local Git settings in this report's run directory; no source files were changed.
- The public runbook branch was rechecked at `8264211237591d023f0a22f2fcf2b3c2f4d3817f`; the hub snapshot remains pinned at `ea5b5abca917ff9bf8799a486b2e425afa755a87`. No newer assignment or runbook replacement was found in the final read-only branch check.
