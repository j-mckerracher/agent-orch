# Preliminary Windows diagnostics — M7 P12 candidate 3a0c976

**Status: BLOCKED at tool preflight; this is not priority-12 acceptance.** The pinned commit/tree and clean isolated checkout were verified. The runbook requires stopping before npm tests, build, or CI when Node is below the lockfile minimum. This host reported Node 22.20.0; the minimum for Node 22 is 22.22.2. No test, build, daemon launch, Tauri launch, or UI check was run. No result from another commit is transferred here.

## Pin and environment

- Runbook branch: `docs/m7-p12-windows-preliminary-3a0c976-0eca0ef2`, tip `a1b8f4aae4c2409bc7d8f001f52b393d2afb8863`.
- Source commit: `3a0c976adf15df35c58734749155d0bb7b3b9460`.
- Source tree: `139fa70a8e1fe2614d54e6d54a2491bf1b67fe74`.
- Windows 11 Education, build 26200, 64-bit.
- PowerShell 7.5.4.
- C: free space at preflight: 40.17 GiB (above the 20 GiB minimum).
- Git 2.46.2.windows.1; Node 22.20.0; npm 11.3.0; rustc/cargo 1.96.0.
- `where.exe cl.exe` returned exit 1 (not on PATH). `vswhere.exe` found Visual Studio 2022 Community at `C:\Program Files\Microsoft Visual Studio\2022\Community`; no developer environment was activated because testing stopped at the Node preflight.
- WebView2 Settings > Installed apps value was not observed. Registry entries reported runtime 157.0.4341.46; see `webview2-registry.txt`. The registry value is not presented as a Settings UI observation.
- No `FM_*` variable names were present.
- `core.autocrlf=false`; `core.longpaths=true`.
- Source status was empty before and after preflight commands. `checkout_clean_after_run=true` for the isolated source checkout.

## Commands and exits

| Command | Exit/result |
|---|---|
| `git -c core.autocrlf=false -c core.longpaths=true clone --no-checkout https://github.com/j-mckerracher/agent-orch.git <isolated-src>` | 0 |
| `git config --local core.autocrlf false` | 0 |
| `git config --local core.longpaths true` | 0 |
| `git fetch origin 3a0c976adf15df35c58734749155d0bb7b3b9460` | 0 |
| `git checkout --detach 3a0c976adf15df35c58734749155d0bb7b3b9460` | 0 |
| `git rev-parse HEAD` and `git rev-parse HEAD^{tree}` | 0; exact pin/tree match |
| `git status --porcelain=v1 --untracked-files=all` before/after | 0; empty |
| `git config --local --get core.autocrlf` / `core.longpaths` | 0; false / true |
| `git --version` | 0 |
| `node.exe --version` / `node.exe -p process.versions.node` | 0; v22.20.0 / 22.20.0 |
| `npm.cmd --version` | 0; 11.3.0 |
| `rustc.exe -V` / `rustc.exe -vV` | 0 |
| `cargo.exe -V` | 0 |
| `where.exe cl.exe` | 1; no `cl.exe` on PATH |
| `vswhere.exe -latest -products * -property installationPath` | 0; Visual Studio 2022 Community found |

The version gate evaluated false: Node 22.20.0 is below 22.22.2. Per the runbook, execution stopped before `npm test`, `npm run build`, `npm ci`, Cargo builds/tests, or `npm run tauri dev`.

## Checks not run

- Vitest suite and production frontend build: NOT RUN, required Node version unavailable.
- Daemon build, native app launch, Tauri dev, and daemon recording: NOT RUN after the required preflight stop.
- Manual route/theme/responsive/keyboard/accessibility/data-honesty UI checks: NOT RUN; the app was not launched.
- Screenshots: none; no app UI was opened or captured.
- Test count/failures: no test suite ran, so no test count or test verdict exists.
- Build result: not run.

This is a tool-version blocker for a preliminary diagnostic, not a product test failure and not an acceptance result. The machine runtime was not changed, no packages were installed, and no source files were edited.

Publication privacy scan: 96 text files scanned; 0 personal-data or secret-pattern matches after sanitization.

Final publication recheck: 97 files scanned for account path/domain/email and common token/private-key patterns; 0 matches.

Publication formatting: redundant blank terminal lines were removed from the published copies of windows.txt and webview2-registry.txt; the original command captures remain unchanged in local run evidence.
