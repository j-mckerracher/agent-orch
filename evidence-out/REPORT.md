# M7 final Windows acceptance report

WINDOWS ACCEPTANCE: FAIL

Section 3: PASS — package and install-smoke (unsigned; N-1/GUI limits are unproven).
Section 4: FAIL — named conversation-runtime test failed; other M7/P12 named checks passed, with harness discrepancies recorded.
Section 5: FAIL — live Codex test returned repeated Conflict and no completed turn.
Section 6: PASS for targeted unit tests; live estimate NOT RUN because no live receipt was produced.
Section 7: NOT RUN — no action-time confirmation for interactive app installation/launch; every row is explicitly annotated.
Section 8: UNPROVEN — no earlier-version N-1 artifact was available.

## Pin and environment

- Source: `orch/m7-final-b44f88a` at `b44f88af63cb1ea7c68b7a827465db466ef308a2`; tree `79820e90846b09a64edf25fe8593ab751abbb323`.
- Runbook source: `docs/m7-final-windows-acceptance-b44f88a`; pinned source tree was checked exactly before tests. SSH fetch failed with `Permission denied (publickey)`; fetched without credentials through HTTPS. No token or `gh` was used.
- Windows 11 Education, build 26200 (`10.0.26200`). WebView2 Runtime `157.0.4341.42` (EdgeUpdate registry). Node `v22.20.0`; npm `11.3.0`; Rust/cargo `1.96.0`; PowerShell `7.5.4`.
- Codex CLI prerequisite: `codex-cli 0.20.0`; login status was signed in (account details withheld). Model/effort settings were not collected and are not asserted.
- Required example path `C:\t` could not be created (Access denied). Used a short real directory below the user profile; its parent had no reparse-point attribute. No SUBST, mapped drive, junction or UNC was used. First check found `core.autocrlf=true`; the corrected final run used `core.autocrlf=false` and `core.longpaths=true`.
- Checkout remained clean after all runs: `status-after-run.txt` and `status-after-all.txt` are empty; `diff-stat-after*.txt` is empty. The install smoke independently recorded clean before and after, with no changed files.

## 2. Full gate harness — MEASURED FAIL

- Final full run: `gates-run4.console.txt`, exit 1. The 27 native gates printed by `--list` equals the derived 27. Native gate row tally: 2 PASS, 17 FAIL, 8 SKIPPED. The harness's `OVERALL` line says 9 PASS / 17 FAIL / 8 SKIPPED because its tally also includes seven passing source/config/privacy checks; both tallies are preserved as printed.
- Earlier attempts are retained. One invocation stopped because I had pre-created its output directory; the first completed run used initial Git settings; an attempted rerun against a non-empty target was refused. The final run used a fresh target/evidence directory and corrected Git settings.
- Main blocking failure: `daemon-tests` exit 101. 118 unit tests passed; the `conversation_runtime` integration binary had 10 pass, 1 fail, 1 ignored. Failure: `agent_failures_and_oversized_replies_fail_the_turn_with_stable_codes` panicked at `conversation_runtime.rs:122:47` with `relay closed: Remote`. It left one test-owned `fake_agent.exe`; the isolated process was identified, recorded and terminated. Other harness gates were then marked failed for the same surviving descendant; this means those harness failures are not individual assertion failures. The separate named tests below are reported from their own outputs.
- `ipc-tests`: raw Rust output is 55 passed (equal to derived 55), but the harness gate reports FAIL because 13 named tests were “not reported.” A direct rerun also measured 55 passed, exit 0. `windows-transport-tests`: 13 passed, 42 filtered; harness expected 33 filtered and marks this gate FAIL.
- `daemon`: measured 128 passed, 1 failed, 1 ignored versus derived 138 passed, 1 ignored. `store`: 153 passed, 1 ignored (equal). `lock-takeover`: test body 1 passed; gate row was marked failed by the leaked descendant. `tauri`: 14 passed (equal). Node self-tests: 97 passed (equal). Vitest: 51 files / 1105 tests passed (equal, direct run; harness row was skipped after npm-ci survivor).
- Node self-test details: TAP #46 `Windows listing: path separators and case do not matter, a single object is accepted` passed; TAP #83 `Windows: installs silently per user, upgrades over the same directory, uninstalls pinned to the directory` passed. The runbook calls the earlier path-separator issue test #71, but this run's TAP #71 is `macOS unsigned: reported, never claimed signed, upgrade and clean-machine steps UNPROVEN`. I do not equate those different TAP numbers.
- Harness marked P12 and desktop/Tauri dependent rows skipped after prerequisite/survivor failures. Independent direct checks filled those gaps: `p12-windows-checks.manual.txt` says PASS; TSC, desktop build, Vitest, Tauri build, Tauri tests, Tauri clippy, `npm run package:win`, and `npm ci` each returned exit 0. Vitest and Tauri counts are above. `npm ci` installed 155 packages and reported zero vulnerabilities; it printed engine warnings because `jsdom`/`w3c-xmlserializer` expect Node >=22.22.2 while this machine has the pinned 22.20.0. The runner's scoped Clippy command itself returned 0; its harness row was failed only due to the descendant check.
- The runbook command `cargo test --locked -p orchestrator-daemon api_usage` returned 0 but matched zero tests under current names. I additionally ran `cargo test --locked -p orchestrator-daemon --bin orchestrator-daemon usage_tests`: 8 passed, exit 0. Store usage tests: 29 passed, exit 0. The broad package filter attempt's output and stall/interruption note are retained separately.
- P12 direct checks measured private ACLs and command-line refusal and ended PASS. This does not prove behavior under a second Windows user.

## 3. Release/package/install smoke — MEASURED PASS with limits

- `package.mjs windows`: exit 0; version `0.1.0`, unsigned installer SHA-256 `0d983fa02807fa48a132487c2cdc67d6df4198e217eb926d87b03c37aa146dbf`.
- `install-smoke windows --allow-unsigned`: exit 0, 9 PASS, 0 FAIL, 1 INFO, 2 UNPROVEN. Silent per-user install, app/daemon/uninstaller files, product version, packaged-sidecar `release_smoke: PASS`, silent uninstall, app removal, and survival of the run-created user-data sentinel all passed. This was a reinstall, not an upgrade.
- Authenticode literal status: `NotSigned`; installer is unsigned by configuration. Signing is UNPROVEN. SmartScreen, standard-user UAC behavior and Defender reputation remain UNPROVEN. N-1 upgrade is UNPROVEN because no earlier different-version package was available; no same-version copy was passed off as an upgrade.
- The run-created sentinel was absent after the smoke script finished. No `orchestrator-daemon` or `release_smoke` process remained after install-smoke. The script left its work directory; an explicit cleanup attempt was rejected by automatic approval review, so that local temporary directory remains outside the repository/evidence tree.

## 4. M7 backend/P12 — MEASURED MIXED

- `conversation_runtime`: exit 101, 10 passed / 1 failed / 1 ignored; reproduced the relay-closed failure above. `conversations`: 2 passed. `queue_snapshot`: PASS through steps 1–7. Full `orchestrator-store`: 153 passed / 1 ignored. IPC: 55 passed on direct rerun. P12: PASS direct. The harness-level descendant and test-report mismatches remain failures.

## 5. M7-25 live Codex exchange — MEASURED FAIL

- Signed-in Codex CLI `0.20.0` differs from the baked-in `0.167.0`; test was run with the observed pin `codex@0.20.0` as the runbook permits. It exited 101 before completing a turn: repeated `conversations.send` Conflict responses, then `runtime selection kept reporting a conflict`. No reply was returned; reply length and live usage receipt are therefore unavailable. No model setting, successful child Job Object ancestry, or in-flight process tree was observed. Post-run process inspection showed no daemon; Codex processes existed but could not be attributed to this test and were left untouched.

## 6. M7-26 estimated usage — MEASURED PARTIAL

- Store usage tests: 29 passed. Daemon usage unit tests: 8 passed under the actual `usage_tests` module filter. The runbook's literal `api_usage` filter selected zero tests (exit 0), retained as-is. The tests cover unknown/missing-model-as-unknown, whole-session estimates, estimates staying under `estimated.costs`, separation from `reported`, and preservation of stored reported cost. No live usage summary/estimated figure was observed because M7-25 did not complete; no model was assumed.

## 7. M7-32 hands-on Windows UI checklist — NOT RUN

- Every checklist row is filled in `WINDOWS-MANUAL-CHECKLIST.completed.md`. Windows build and installed WebView2 version are measured there. No interactive app launch, Narrator, theme/DPI/keyboard/connection/recovery/mutation/security UI checks, or screenshots were recorded. The runbook requires the real installed app; the computer-use skill requires action-time confirmation before installing/launching it. Confirmation was requested but had not arrived when this evidence was finalized. Silent install-smoke results are not represented as UI checklist passes.

## 8. N-1 upgrade — UNPROVEN

- No earlier-version `package.mjs windows --out` directory was available in the isolated checkout or the known local prior-run location. No fake N-1 was created.

## Remaining UNPROVEN

Clean-machine behavior; code signing; SmartScreen; standard-user UAC; Defender reputation; second-user named-pipe denial; manual remote-client rejection; Windows 10 22H2; GUI launch/install/uninstall/upgrade and every unchecked UI behavior; feature support by the app's WebView; live Codex reply/model/usage/Job Object process-tree observation; N-1 upgrade; and any property not explicitly listed as measured above.

## Evidence map

Raw harness attempts are under `gates-run2/`, `gates-run3.console.txt`, and `gates-run4/`. Direct backend, frontend, Tauri, package, install-smoke, usage, Codex-live and P12 outputs are adjacent to this report. `privacy-scan-final.txt` records the final redaction check.

## Final remote-assignment scan

- Fresh HTTPS fetch inspected 108 remote heads, including docs, source (`orch/`), and evidence refs. `docs/m7-final-windows-acceptance-b44f88a` still points to `5511597080d37a6043a2e96e81ba0433fa2a16fb`; `orch/m7-final-b44f88a` still points to the pinned source commit/tree above. No M7-final evidence ref existed before this run.
- The remote already contains a `priority-12r5` Windows evidence ref and an M7-backend evidence ref. The M7-backend report on that ref says overall FAIL (733 passed, 1 failed, 1 ignored against derived 747/0/1); it is an existing, adjudicated assignment and was not repeated. Older Windows acceptance assignments also have corresponding evidence refs. This run is only for the outstanding M7-final runbook.

Evidence ref to publish: `evidence/m7-final-windows-acceptance-b44f88a-20261111T054110Z-0d6bdcb3`.
