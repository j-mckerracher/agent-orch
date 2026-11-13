# Windows assignment check, 2026-11-13

This report records the remote assignment changes, automated Windows checks, and native outcomes. Automated tests do not count as native acceptance.

## Remote scan

I compared full remote-head inventories, not only the watcher baseline. The initial scan at 05:10:56 UTC found 137 heads; the 07:08:25 UTC inventory and 07:17:13 UTC confirmation both found 147. The full 147-head inventory is in remote-heads-all-20261113T070825Z.tsv; relevant assignment refs are in remote-heads-relevant.tsv; changes are in remote-scan-delta.tsv.

During this pass, the remote added assignment refs M7-08, M7-10, M7-20, M7-21, M7-22, M7-23, M7-24, M7-25, M7-26, and M7-30. M7-15, M7-17, M7-18, and M6-04 refs also advanced. The integration ref moved from cb39970c286af18d1d7a46d90ea1d9ddbe9fb10d to 92f56c3af9cd34741a09667f499164edcafec696. No M7-19 or M7-27 through M7-29 refs were present in the final inventory. The last four full scans, 06:44:08, 06:52:46, 07:08:25, and 07:17:13 UTC, showed no changes.

The current relevant assignment heads were:

| Ref | Head |
|---|---|
| orch/0eca0ef2-integ2 | 92f56c3af9cd34741a09667f499164edcafec696 |
| orch/0eca0ef2-m6-04-verify | e970494b0a5c51bd9ffa9bff758aad28dc33fc75 |
| orch/m6-04-integ-632c77d | 632c77db2d41a72bd5283013aa444798278fa9be |
| docs/m6-04-windows-acceptance-632c77d | 9744a2790b765837e46409ff7f2f785de0e77c63 |
| evidence/m6-04-clean-machine-windows-acceptance-632c77d-20261111T010453Z-f652fb41 | 393636ac01a413541a4cd122eb037ac597ba4a0d |
| orch/m6-04-integ-632c77d | 632c77db2d41a72bd5283013aa444798278fa9be |
| orch/0eca0ef2-m7-08-verify | aca548dacd5c9f32ec9416eb72a2de4bb790cc50 |
| orch/0eca0ef2-m7-10-verify | bde4dcbe4304d597bc9862afc16d2a6f457f050f |
| orch/0eca0ef2-m7-11-verify | be76a54d2653fa2092bc0fea29c5446be44deb5e |
| orch/0eca0ef2-m7-12-verify | e3c27332cfb1c8bd2c0f70d9d0f75c7ed6fb0f75 |
| orch/0eca0ef2-m7-13-verify | 1fc64d88cc3d13ea6dbb01bd9aa98e1e19fb6132 |
| orch/0eca0ef2-m7-14-verify | c64cb6a42d9690854a4a1b5f613f62ad6ef228a4 |
| orch/0eca0ef2-m7-15-verify | d8022d7f5747a8e70dd21a57261d5b568b70c3e9 |
| orch/0eca0ef2-m7-16-verify | e52cc14e681622d52d2349fee37bf5decc99cb1d |
| orch/0eca0ef2-m7-17-verify | b6b7ac3c34b99842e68ec6e4453958a55823e301 |
| orch/0eca0ef2-m7-18-verify | 1543b8aa7447bb4b1dc08e499476ff5e6b038b4f |
| orch/0eca0ef2-m7-20 | 1b9818f11009b7f0a136a6a3185e15bb9a51b2e2 |
| orch/0eca0ef2-m7-21 | 0986196a381e10844761af001e617bd0bda626fe |
| orch/0eca0ef2-m7-22 | b86f2be9439ff7e731201b25549cb6c70d20cdc9 |
| orch/0eca0ef2-m7-23 | f018023ec68b19eb0f1790a9a3e90493197432d2 |
| orch/0eca0ef2-m7-24 | c35a5972f60c3dba33686c34c163996ca7a0216a |
| orch/0eca0ef2-m7-25 | 5c67246ce8834008e8fdb60cddc1fd8d1a32e765 |
| orch/0eca0ef2-m7-26 | 1fe8d9f7c4ceaab8b186655c0cce5de10fde8226 |
| orch/0eca0ef2-m7-30 | 24898ad11a38c3bf643ae43929fd338221399a76 |

The integration tree at 92f56c3 is a3012626c6808b337a719d0afcd4d887f5a504ea, the same tree as the exact M7-26 pin 1fe8d9f. M7-24's test pin 4ea4a7f has the same product/test code as updated branch c35a597. The later branch tip also adds changelog and M7-25/M7-30 documentation; it does not change the tested product or test files.

The new M7-30 matrix already records its automated npm test/build results on macOS at source 9b46c07. It explicitly defers Windows and native work to M7-32; no Windows UI, accessibility, DPI, package, or live-agent checks were repeated here. The M7-24, M7-25, and M7-26 checklists were read at their current assignment heads. M7-25 defers all native rows; its deterministic Rust commands below were run as supplementary Windows evidence and do not complete that assignment. M7-26 calls for local Rust gates before its deferred real-agent usage and native-WebView checks.

## Automated checks

| Pin | Command | Result |
|---|---|---|
| M7-13 runbook source dc33a7eb9a08b8ed253718243e3022f01a59806d | npm ci, then the two review suites and fixture guard | 3 files, 160 tests passed. npm ci exited 0. |
| M7-12 e3c27332cfb1c8bd2c0f70d9d0f75c7ed6fb0f75 | npm ci, then ActivityPanel suite | 1 file, 17 tests passed. |
| M7-16 e52cc14e681622d52d2349fee37bf5decc99cb1d | npm ci, then ActivityPanel, search, and native suites | 3 files, 42 tests passed. |
| M7-20 1b9818f11009b7f0a136a6a3185e15bb9a51b2e2 | npm ci, then ActivityPanel suite | 1 file, 18 tests passed. |
| M7-21 source pin 54adab543b4f150128d99f84795a1bd15aaea3ff | npm ci, then ResourceStrip and live metrics suites | 2 files, 59 tests passed. |
| M7-22 b86f2be9439ff7e731201b25549cb6c70d20cdc9 | npm ci, then live usage suite | 1 file, 47 tests passed. |
| Integration 930e0b89f7d44dc6a636f54a135c346991fdb270 | npm ci, then 11 targeted UI suites | 11 files, 298 tests passed. |
| M7-24 source pin 4ea4a7f7eeb2362b1d4c152e0e6db9b5538f7bfc | npm ci, then WorkspaceView suite | 1 file, 31 tests passed, including the new no-launchable-provider case. |
| M7-25 5c67246ce8834008e8fdb60cddc1fd8d1a32e765 | cargo build --locked -p orchestrator-daemon --examples --bins; serial daemon tests | Build exited 0; 138 passed, 0 failed, 1 ignored. |
| M7-26 1fe8d9f7c4ceaab8b186655c0cce5de10fde8226 | cargo test --locked -p orchestrator-store -p orchestrator-daemon -- --test-threads=1 | 291 passed, 0 failed, 2 ignored. The test pin has the same tree as integration 92f56c3. |
| M7-08 source at integration 7cf0a990ae5d579515ca5a231efb1c2fc2ca75b0 | targeted daemon conversation-authority test | 1 test passed. |

All npm ci commands exited 0. They reported EBADENGINE warnings for transitive jsdom 31.0.0 and w3c-xmlserializer 6.0.0: each requests Node 22.22.2 or later in that major line, and this machine has Node 22.20.0. The installs and tests passed. The project does not declare a Node engine requirement.

The 11-suite integration run covered Workstreams, ActivityPanel, Agents, screen and native states, ResourceStrip, live metrics, search, Workspace, and live usage. It ran before M7-24's product fix; the focused WorkspaceView suite was run at M7-24's source pin afterward. Rust M7-26 coverage was run on the final integration tree.

The M7-30 report's npm test and build were run on macOS at 9b46c07 (52 files/1,252 tests; build passed with a Vite chunk-size advisory). It says Playwright/e2e, axe, native, and Windows checks were not run under fast mode. This pass did not treat that macOS evidence as Windows evidence.

## Native outcomes

| Assignment | Status | Evidence |
|---|---|---|
| M7-11 | UNVERIFIED | Requires two disposable projects, a real configured agent, and at least 100 linked live tasks. No native scenario ran. |
| M7-12 | UNVERIFIED | Live tasks, approval, event refresh, 961/960 CSS-pixel, and keyboard checks did not run in WebView2. |
| M7-13 | UNVERIFIED | The app opened, but its default daemon data store failed with SQLite code 1546, a disk I/O error. No named pipe or real approval candidates appeared. Approve and request-changes did not run. This attempt did not use a fresh isolated data directory. |
| M7-14 | UNVERIFIED | The real running and failed sessions, agent checks, and action controls did not run in WebView2. |
| M7-15 | DEFERRED | The runbook defers native checks to M7-32. |
| M7-16 | DEFERRED | The runbook assigns native checks to the consolidated native pass. |
| M7-17 | UNVERIFIED | A real daemon started with an isolated data directory at source 350cd9dd993da25072fb7a7f4af6bcdd0a030af0. UI automation failed twice with GetCursorPos failed: Access is denied. (0x80070005). No metrics screenshot or UI-to-daemon comparison was captured. |
| M7-20 | UNVERIFIED | The live-agent run with both worker slots occupied, three queued tasks, and a dependency-blocked task did not run in the native app. |
| M7-21 | UNVERIFIED | Native host-metrics comparison and visibility-polling trace did not run. Passing tests do not establish WebView2 readings or polling behavior. |
| M7-22 | DEFERRED | The runbook defers native checks to M7-32. |
| M7-23 | DEFERRED | The runbook defers native checks to M7-32. |
| M7-24 | DEFERRED | The current checklist defers native checks to M7-32. Its 31 passing tests do not prove native provider/model controls. |
| M7-25 | DEFERRED | The checklist defers live Global/workstream agent exchanges, replay after daemon restart, and native UI inspection. Deterministic Windows Rust build/tests passed. |
| M7-26 | DEFERRED | Real Codex usage reporting and the native usage pill/Agents table were not exercised. The deterministic Windows Rust suite passed. |
| M7-30 | DEFERRED | The matrix explicitly defers Windows/native checks to M7-32. No Narrator/NVDA, Windows DPI, contrast themes, packaged WebView, or live-agent checks ran. |
| M7-08 | DEFERRED | The verification report defers native and live-agent checks. |
| M7-10 | DEFERRED | Its checklist says not to run native checks in this worker pass. |
| M7-18 | DEFERRED | Its checklist says not to run native checks in this worker pass. |
| M6-04 | UNPROVEN | Its checklist assigns signed clean-machine installation and upgrade checks to M7-32. This pass did not run the separate 27-gate M6-04 runbook at docs ref 9744a27. The existing evidence ref 393636a cites an older runbook blob (88bc1c0), so it does not complete the 27-gate runbook. That older report records one Node self-test failure (94 passed, 1 failed due to a Windows path separator expectation), an unsigned package/smoke pass, and clean-machine/upgrade/signing limits. The current profile already contains two agent-orch entries, so its installer-smoke precondition would refuse this profile. The separate M6-04 runbook remains pending. The existing 393636a report is not evidence for the 9744a27 runbook and was not used to mark it complete. |

The M7-13 and M7-17 app attempts are recorded in native-ui-observations.md. Computer Use failed twice with the cursor-access error; no more UI automation was attempted. Only processes tied to the isolated test checkout were stopped; a later scan found no remaining app, daemon, or dev-server process.

## Environment

Windows 11 Education, build 26200; WebView2 Runtime 157.0.4341.46; Node 22.20.0; npm 11.3.0; Git 2.46.2.windows.1. The M7-24/25/26 checkouts used detached exact commits. M7-26 used a unique CARGO_TARGET_DIR. The M7-13 UI attempt used the default daemon data directory and is not counted as an isolated-data native pass.

## Limits

No screenshot proves native acceptance. Native rows remain unverified or deferred as listed. Read the test table as automated evidence only; it does not replace real-daemon, real-agent, screen-reader, display, or accessibility checks.