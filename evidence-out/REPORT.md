# Preliminary Windows baseline: M7 P12 pinned integration head

This is a preliminary diagnostic, not P12 acceptance. Final M7-00 through M7-03 candidates require a fresh complete rerun. Required macOS checks remain separate.

## Pinned source and checkout

- Source commit: `53c0272b3e5df60f48dba90aad1c7a14cf3a19a9`
- Source tree: `1150197734de6ffdc8a672f2422649fe3dca6eda`
- Fresh isolated HTTPS clone, detached at the exact SHA; tree matched.
- Source checkout was clean before and after checks.
- Free space at run start: 54.16 GiB on local C: (runbook minimum: 20 GiB).

## Environment

- Windows 11 Education, version 10.0.26200, build 26200, x64
- PowerShell 7.5.4
- Git 2.46.2.windows.1
- Host Node 22.20.0 was unsupported by the lockfile. Used official Node 24.22.3 x64 ZIP for this run only; SHA-256 matched the official `SHASUMS256.txt`. No machine/user PATH or installation settings were changed.
- npm 11.19.1 under remedied Node
- rustc 1.96.0; Cargo 1.96.0
- `where.exe cl.exe` did not find MSVC on the inherited PATH. `vswhere` located Visual Studio 2022 Community; its developer environment resolved MSVC 14.38.33130 and the Cargo checks ran with that environment.
- WebView2 Runtime version was not read from Settings > Apps > Installed apps; native launch/UI remains unverified.

## Required checks

| Command | Result |
| --- | --- |
| `npm ci` | PASS, exit 0 |
| `npm test` | PASS, exit 0; 50 files and 1,171 tests passed |
| `npm run build` | PASS, exit 0; 151 modules transformed |
| `node --test scripts/ci` | FAIL, exit 1; Node 24 reported `MODULE_NOT_FOUND` for the path `scripts/ci` |
| `cargo test --workspace` | EXIT NOT CAPTURED. The full captured output contains 73 successful `test result` summaries: 858 passed, 0 failed, 2 ignored. The helper's Start-Process tree wait remained open after Cargo and test processes exited; only the stalled PowerShell runner was interrupted. Do not treat this as a verified exit-0 result. Raw stdout/stderr are preserved. |
| `cargo build --locked -p orchestrator-daemon` (offline first) | PASS, exit 0; daemon artifact present |
| `npm run tauri dev` | NOT RUN; this run had no authorized native desktop interaction/capture tool to inspect and close the UI safely |

The two ignored Cargo tests include an integration case requiring a signed-in Codex CLI. No model or agent settings were inferred or changed.

## Source contracts and manual checks

Static source review passed the four global routes, legacy route formats, theme bootstrap ordering, Windows font stack/no bundled font face, named navigation/activity landmarks, visible focus behavior, 1150/960/760 breakpoints, and configured 1280x800 default / 760x600 minimum size. Theme storage/system behavior and contrast thresholds are covered by source tests; automated frontend tests passed.

Native theme persistence, rendering/truncation, viewport resizing, live values, focus and accessibility-tree behavior, and capture are UNVERIFIED / NOT RUN. Source inspection and automated tests are not native UI proof.

## Limits

This run has one Node test-command failure and one Cargo process-exit capture limitation. Cargo's recorded test summaries show no failing test binaries, but the workspace command is not reported as exit 0. Native UI and screenshot evidence are unavailable. This report is only the requested preliminary baseline diagnostic, not acceptance.

Evidence branch ref: `evidence/m7-p12-baseline-53c0272-20261113T045017Z-808cd540`
