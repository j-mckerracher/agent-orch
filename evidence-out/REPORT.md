# Windows acceptance report — M7 final r1

WINDOWS ACCEPTANCE: FAIL

Run: `20261111T155754Z` (UTC run identifier)  
Source commit: `6c17442eaee5243b48034dfc53d7f43d25f1ab82`  
Source tree: `9b9612ab6084c644e344603028149084118cc889`  
OS: Windows 11 Education, build 26200  
Toolchain: Node v22.20.0; npm 11.3.0; rustc/cargo 1.96.0; PowerShell 7.5.4  
WebView2 runtime: 157.0.4341.46 (registry also listed 157.0.4341.42; loader 1.3.315.17)  
checkout_clean_after_run: true (`git status --porcelain` empty; harness source-status and source-diff checks PASS). This refers to the pinned source checkout, not the separate evidence tree.  
Disk: C: had 61.15 GiB free at preflight; latest recorded before cargo work was 41.20 GiB. No free-space gate fell below 20 GiB.

## Section 2 — full native harness (MEASURED)

Harness verdict: `FAIL` — 33 PASS, 1 FAIL, 0 SKIPPED (exit 1). The native run had 27 named gates: 26 passed and one failed. The other seven PASS rows below are harness metadata and post-run checks.

| Gate | Verdict | Exit | Seconds | Notes |
|---|---|---:|---:|---|
| node-self-tests | PASS | 0 | 6 | |
| daemon-build | PASS | 0 | 293 | |
| ipc-tests | PASS | 0 | 32 | |
| daemon-tests | PASS | 0 | 145 | |
| daemon-lifecycle | PASS | 0 | 11 | |
| coordinator-review-tests | PASS | 0 | 11 | |
| git-lib-tests | PASS | 0 | 21 | |
| cli-cleanup-safety-tests | PASS | 0 | 67 | |
| private-fs-tests | PASS | 0 | 5 | |
| store-tests | PASS | 0 | 96 | |
| lock-takeover-test | PASS | 0 | 1 | |
| diagnostics-tests | PASS | 0 | 15 | |
| runtime-command-line-tests | PASS | 0 | 19 | |
| cli-command-line-tests | PASS | 0 | 6 | |
| windows-transport-tests | PASS | 0 | 6 | |
| cli-build | PASS | 0 | 1 | |
| p12-windows-checks | PASS | 0 | 3 | |
| clippy-scoped | PASS | 0 | 48 | |
| desktop-npm-ci | PASS | 0 | 18 | Node 22.20.0 is below the package engine minimum 22.22.2; npm emitted EBADENGINE. Gate still exited 0. |
| desktop-tsc | PASS | 0 | 6 | |
| desktop-build | PASS | 0 | 3 | Build emitted a chunk-size warning (>500 kB). |
| desktop-vitest | FAIL | 1 | 58 | 1 failed, 1,104 passed, 1,105 total across 51 files. |
| tauri-build | PASS | 0 | 168 | |
| tauri-tests | PASS | 0 | 224 | Three real-daemon tests took about 146 seconds. |
| tauri-clippy-clean | PASS | 0 | 1 | |
| tauri-clippy | PASS | 0 | 68 | |
| desktop-package-win | PASS | 0 | 302 | |
| git-config | PASS | — | — | `core.autocrlf=false`, `core.longpaths=true`. |
| gitattributes-before | PASS | — | — | |
| source-status-before | PASS | — | — | |
| gitattributes-after | PASS | — | — | |
| source-status-after | PASS | — | — | |
| source-diff-after | PASS | — | — | |
| privacy-scan | PASS | — | — | Harness scan passed; separate pre-push evidence scan also performed. |

The sole failing test was `ResourceStrip behaviour > stops ageing while the window is hidden and catches up when it is shown again`. At `src/ui/shell/ResourceStrip.test.tsx:333`, expected `aria-valuetext` containing `observed 2 s ago`; received `44%, scope CPU, observed 2 min ago`. Failure output is preserved in `gates/desktop-vitest.*` and the complete harness log.

Other coverage explicitly UNPROVEN by the harness: second-user named-pipe denial, genuinely remote-client rejection, slow-reader write above 64 KiB, Windows desktop E2E requiring a Unix socket, WebView/React approval click path, real GUI install/launch and daemon lifecycle across window close/reopen, stale-token relaunch timing, real-app idle keep-alive, and daemon-kill/sidecar-failure behavior.

## Section 3 — release/package/install smoke (MEASURED)

PASS for Windows packaging, `release_smoke`, install-smoke and silent uninstall: package artifact was unsigned; install-smoke counts PASS=9, FAIL=0, INFO=1, UNPROVEN=2. Silent per-user install, packaged-sidecar smoke, uninstall, app removal, and the sentinel-survives-uninstall assertion passed. Authenticode status is `NotSigned`; no signing or SmartScreen pass is claimed. N-1 upgrade is UNPROVEN because no older artifact was supplied. A separately launched UI app process started; actual rendered window was not observable. The manual install directory retained `orchestrator-daemon.exe` after its uninstaller exited 0; this residual is disclosed and left intact. The exact sentinel was absent on final path verification; `sentinel-cleanup.txt` records the cleanup command's false result, so this report does not claim that command removed it.

## Section 4 — conversation runtime diagnostic (MEASURED)

PASS for diagnostic builds and suites. `conversation_runtime`: 11 passed, 0 failed, 1 ignored (live Codex test run separately in Section 5). M7-08 `agent_failures_and_oversized_replies_fail_the_turn_with_stable_codes` passed. No `orchestrator-agents:`, `orchestrator-daemon:` or `orchestrator-ipc:` diagnostic lines appeared in that suite output.

Three `fake_agent.exe` processes (PIDs 30004, 35832, 47844) were present in the before snapshot and remained after the gates. Their parents were absent and each reported as in a job. They predated this run; the scoped cleanup attempt was blocked by execution policy, so they were left untouched. Exact snapshots and cleanup status are in `m7-diag/`. This diagnostic does not claim those processes were cleaned up. The pinned Codex CLI direct executable was version 0.167.0; version, login-status and `debug models` probes exited 0. The machine CLI was 0.20.0 and did not support `debug models`. Probe outputs, including exact raw output, are retained under `m7-diag/`; no selected model or agent setting is inferred.

## Section 5 — M7-25 live conversation exchange (MEASURED)

PASS on the documented rerun with the pinned local Codex 0.167.0 executable: `live_codex_answers_a_global_message` passed (1 passed, 0 failed, 6.28 seconds). The first attempt using the npm `.cmd` shim failed before launch with `runtime_unavailable` because batch-file arguments were invalid; both attempt logs are preserved, with the direct-executable retry rationale in `m7-25-live-rerun-note.txt`. The test asserted the reply includes `pong`; the exact reply byte count and in-flight process ancestry were NOT RUN because background process sampling was denied. No claim is made about those unobserved values.

## Section 6 — M7-26 estimated-cost checks (MEASURED)

PASS: store usage tests 29 passed; daemon usage tests 8 passed. Both exit codes were 0; full outputs are preserved.

## Section 7 — M7-32 Windows UI checklist (INCONCLUSIVE / NOT RUN)

The unsigned installer installed to the run-specific isolated directory and the desktop process launched. UI capture failed twice with `GetCursorPos failed: Access is denied. (0x80070005)`, including after refreshing app/window selection. Per tool recovery guidance, UI automation stopped after retry. All screenshot-dependent rows are recorded as NOT RUN; no GUI, keyboard, Narrator, theme, responsive/DPI, daemon-recovery or approval result is inferred. Exact row-by-row disposition is `WINDOWS-MANUAL-CHECKLIST.completed.md`; screenshot blocker is `screens/SCREENSHOT-ACCESS-BLOCKED.txt`. App-mode registry values were light=1 and system=0; `LogPixels` was not measured.

## Section 8 — N-1 upgrade (UNPROVEN)

No earlier different-version `package.mjs windows --out` artifact was found in the run-specific artifact location or current repository, so the runbook's N-1 upgrade was not run. This is `UNPROVEN: no N-1 artifact`, not a same-version rehearsal.

## Setup, evidence, and limits

SSH fetch was attempted and failed with `Permission denied (publickey)`; the permitted HTTPS fallback was used and recorded in `preflight/clone-method.txt`. Git settings were applied before checkout, and the full source commit/tree match the pinned values. Free-space evidence is in `preflight/`; the per-cargo-step log remained above 20 GiB.

The only test failure is preserved without changes to scripts, expected counts or tests. No screenshots could be captured, and the interactive checklist remains incomplete. No unsigned-build trust, clean-machine result, upgrade, or unrun GUI behavior is presented as PASS.

`evidence-out/` contains only test evidence and report files; build output, `node_modules` and `target` are excluded.

## Final section verdict lines

Section 3: PASS — Windows package, release smoke, install smoke and silent uninstall; unsigned, N-1 unproven.
Section 4: PASS — runtime/backend suites passed; three pre-existing fake-agent processes remain documented and untouched.
Section 5: PASS — M7-25 passed on the direct-executable rerun; process ancestry and exact reply length not run.
Section 6: PASS — 29 store usage tests and 8 daemon usage tests passed.
Section 7: INCONCLUSIVE — app process launched, but screenshot access was denied twice; interactive checklist rows remain not run.
Section 8: UNPROVEN — no earlier different-version N-1 artifact was available.