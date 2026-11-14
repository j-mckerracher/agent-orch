# M6-04 signed install and upgrade runbook (clean machines)

Scope: install, upgrade, approvals, cancellation, recovery and uninstall of the packaged desktop
app on macOS and Windows. Labels: **MEASURED** (run on the stated machine, evidence named),
**DERIVED** (read from code or from another platform's result, not run), **UNPROVEN** (nothing in
this repository has shown it). Every result in this file was produced without a real signing
certificate and without a clean machine, so the **clean-machine, real-signing, notarization,
Gatekeeper and SmartScreen rows are UNPROVEN** until someone runs sections 4 and 5.

## 1. What is automated

| Piece | What it does | Runs where |
| --- | --- | --- |
| `crates/orchestrator-daemon/examples/release_smoke.rs` | Drives the **packaged** `orchestrator-daemon` over the real local transport with a fake `codex` agent. 12 `SMOKE` steps, last line `release_smoke: PASS`. | any platform; the packaged sidecar is the binary under test |
| `scripts/release/install-lib.mjs`, `install-smoke.mjs` | Checks an artifact directory (`SHA256SUMS`, `release-info.json`, name against the `signed` flag and the app version), installs it, verifies signatures, runs `release_smoke`, uninstalls (Windows). | macOS flow on macOS, Windows flow on Windows |
| `scripts/release/install-lib.test.mjs` | Self-tests of the pure checks and of both flows against a fake machine (command order, verdicts). | `node --test "scripts/**/*.test.mjs"` |
| `.github/workflows/release.yml` step `Install smoke` | Runs the two scripts on the artifact `Package` just built; `--require-signed` when the build was signed, otherwise `--allow-unsigned` with a `::warning::` (and a failure on a tag unless `ALLOW_UNSIGNED_RELEASE=true`). A failing step stops the artifact upload. | GitHub runners (UNPROVEN until one runs) |

`release_smoke` steps and what each proves:

| Step | Proves |
| --- | --- |
| 1 | the daemon starts, authenticates a client, registers a project and stores a coordinator message |
| 2 | a running agent is cancelled: task `Cancelled`, agent process gone |
| 3 | a daemon killed hard while an agent runs (Unix: the agent is left running; Windows: the kill-on-close job object ends it) |
| 4 | the new daemon starts over the old data directory; both `daemon_start` versions are printed (`UPGRADE: <old> -> <new>`), checked with `--expect-old-version` / `--expect-new-version` |
| 5 | the project, the coordinator message and the cancelled task written by the old daemon read back unchanged |
| 6 | no orphan survives; the interrupted task is `Failed` and is **not** relaunched; `tasks.retry` starts a new process |
| 7 | cancellation again, on the new daemon |
| 8-9 | two reviewable tasks are seeded, approvals are requested, and they survive a hard daemon kill and restart |
| 10 | approve then delivery creates `refs/heads/orchestrator/review/<task>` at exactly the candidate commit |
| 11 | reject delivers nothing; a later approve is a conflict and delivery is forbidden |
| 12 | orderly shutdown removes `daemon.token`; the token never appears in `daemon.log` |

Limits of `release_smoke`: it does not test signing, the installer, the GUI or Gatekeeper/SmartScreen.
Public RPC cannot create a reviewable task, so the review tasks of steps 8-11 are seeded with this
checkout's store code after the upgrade; a pending approval written by the OLD daemon is therefore
**not** exercised. The upgrade is only as real as the two binaries: with `--old-artifact-dir` pointing
at a real previous release it is an upgrade. `install-smoke` counts an upgrade as proven only when the old and the
new build report **different versions and have different sidecar bytes**. Otherwise (one artifact, or the same one
twice) the smoke step is labelled `SAME-VERSION rerun; NOT an upgrade` or `reinstall; NO upgrade`, the JSON result
says `"upgrade": {"kind": "same-version", "proven": false}`, `release_smoke` writes `"same_version": true`, and an
`UNPROVEN upgrade from a previous release (N-1 to N)` line is printed.

The smoke binary is not trusted by its exit code. `release_smoke --out <file>` writes a structured result only after
all 12 steps pass; `install-smoke` requires exit 0 **and** that file, and checks schema, `result == "PASS"`, steps
1..12 in order, both `daemon_start` versions, `same_version`, `upgrade_requested`, and that the daemons it ran are
the packaged sidecar paths. A stub that exits 0 fails. Residual risk: a stub that forges a complete, consistent
result still passes; this is a self-check of the harness, not a defence against a hostile binary.

## 2. Run it (on the platform under test)

```sh
# 1. package (signing secrets optional; without them the artifact is labelled unsigned)
CARGO_TARGET_DIR=<absolute, outside the checkout, >= 2 GiB> node scripts/release/package.mjs <macos|windows> --out <new dir>
# 2. build the smoke driver
CARGO_TARGET_DIR=<same> cargo build --locked -p orchestrator-daemon --example release_smoke
# 3. install-smoke; --old-artifact-dir is the `package.mjs --out` dir of the PREVIOUS release
node scripts/release/install-smoke.mjs <macos|windows> --artifact-dir <new dir> --work <new SHORT dir> \
  --smoke-bin <target>/debug/examples/release_smoke[.exe] [--old-artifact-dir <dir>] \
  (--require-signed | --allow-unsigned) --out result.json
```

Exactly one of `--require-signed` and `--allow-unsigned` is required (exit 2 otherwise), so an unsigned build can
never pass the install gate by omission. Keep `--work` short (a macOS Unix socket path is limited to about 100
bytes; `release_smoke` refuses a longer one). Exit 0 means no step is `FAIL`; `UNPROVEN` steps are listed and are
never passes. `--require-signed` makes an unsigned artifact a `FAIL`. A failed smoke keeps its smoke work directory
(`<work>/s`, named in the output) for inspection; a passing one deletes `<work>/s` only. `install-smoke` never
removes `--work` itself: on Windows it keeps `AgentOrchestrator/uninstall.exe` (the NSIS uninstaller stays when it
is pinned with `_?=`) and `release-smoke-result.json`; delete `--work` by hand, or use a throwaway user or VM. Fake
agents stop on their own at the end of a run, including a failed one. At the end the script prints the source commit,
`HEAD^{tree}` and what the run changed in the checkout (see `docs/release-ci.md`).

## 3. MEASURED on a Mac (unsigned, arm64, base `136646d95` plus the M6-04 changes)

Evidence: `evidence-m6-04-<UTC>/` next to the report (`install-smoke-mac-*.stdout.txt` and `.json`,
`package-mac.*`, `clippy-scoped.*`).

- `package.mjs macos` built `Agent-Orchestrator-0.1.0-macos-arm64-unsigned.zip` (exit 0).
- Remediation re-run (M6-04r, `evidence-m6-04r-<UTC>/`): the same artifact with `--allow-unsigned`: 5 PASS, 2 INFO,
  2 UNPROVEN, exit 0, `upgrade: NOT proven (none)`; with a copy as `--old-artifact-dir`: step labelled
  `SAME-VERSION rerun; NOT an upgrade`, `upgrade.kind` `same-version`; a stub smoke binary that exits 0: `FAIL`
  (`wrote no structured result`); no policy flag: exit 2.
- `install-smoke macos` on it (original run): 5 PASS, 2 INFO, 2 UNPROVEN, exit 0, including the full `release_smoke` (12 steps,
  `release_smoke: PASS`) against `Agent Orchestrator.app/Contents/MacOS/orchestrator-daemon`.
- Same run with `--old-artifact-dir` set to a copy of the same artifact: PASS with
  `UPGRADE: 0.1.0 -> 0.1.0` and the same-version warning. This rehearses the code path only; it is **not**
  an upgrade between releases.
- `--require-signed` on the unsigned artifact: exit 1 with `a signed artifact was required but
  release-info says signed=false`.
- The unsigned bundle is **not** a valid signature: `codesign --verify --deep --strict` exits 1 and `spctl
  --assess` reports `code has no resources but signature indicates they must be present` (the linker's ad hoc
  signature covers the executable, not the bundle). An unsigned build is a developer artifact.
- **A quarantined unsigned bundle is killed and deleted on first run.** After
  `xattr -w com.apple.quarantine ... "Agent Orchestrator.app"`, running `Contents/MacOS/orchestrator-daemon`
  ended with `Killed: 9` and the `.app` directory was emptied by macOS. A downloaded unsigned build therefore
  cannot be run without removing quarantine, and this is the behaviour a clean Mac will show. The harness
  therefore applies quarantine only to a throwaway copy.
- Scoped clippy (`-p orchestrator-ipc -p orchestrator-daemon --all-targets`): exit 0, 0 warnings (the two
  `supervisor.rs` baseline lines are `cfg(windows)` code and cannot appear on macOS).
- `cargo check --target x86_64-pc-windows-gnu -p orchestrator-daemon --examples`: could not run, the SQLite C build
  needs `x86_64-w64-mingw32-gcc`. The `cfg(not(unix))` branches of `release_smoke` are **UNPROVEN to compile**.

## 4. macOS clean-machine procedure

Use a freshly restored VM or a new macOS user on a machine that has never run this app: no Rust, no Node, no
Xcode command line tools, no previously quarantined copy, and a snapshot to return to. Test the **signed,
notarized** artifact from the release workflow (`release-macos-signed`), downloaded through a browser so it is
quarantined. Record macOS version, arch and the artifact sha256. Pass means every `expected` holds.

| # | Step | Expected | Status |
| --- | --- | --- | --- |
| M1 | `shasum -a 256` the zip against `SHA256SUMS` | equal | UNPROVEN |
| M2 | Extract with Archive Utility, drag to `/Applications` | the app is `Agent Orchestrator.app` | UNPROVEN |
| M3 | `codesign --verify --deep --strict --verbose=2`, `codesign -dvv` | exit 0; `Authority=Developer ID Application`, a `TeamIdentifier`, `flags` include `runtime`; the sidecar shows the same team | UNPROVEN |
| M4 | `spctl --assess --type execute --verbose=2`; `xcrun stapler validate` | accepted, `source=Notarized Developer ID`; ticket valid (works offline) | UNPROVEN |
| M5 | Double-click the app | opens with at most the standard "downloaded from the Internet" prompt; no "damaged" or "unidentified developer" block | UNPROVEN |
| M6 | `ps` while it runs; `ls ~/.local/share/agent-orch` | `orchestrator-daemon` runs from inside `/Applications/Agent Orchestrator.app`; `daemon.token`, `store.sqlite`, `daemon.log` exist | UNPROVEN |
| M7 | Register a project and send a coordinator message in the UI | both appear and survive quit and relaunch | UNPROVEN |
| M8 | Approvals and cancellation in the GUI | see `m5-05-native-lifecycle-runbook.md` section 5.6; a clean machine has no agent CLI, so only the daemon-level result of `release_smoke` steps 2, 7 and 8-11 is available | UNPROVEN in the GUI |
| M9 | `kill -9` the daemon, relaunch the app | the app starts a new daemon; a task that was running is `Failed`, not silently resumed | UNPROVEN in the GUI |
| M10 | **Upgrade.** Install release N-1 (signed), do M7, quit the app, wait more than 60 s (the idle daemon exits), replace the app in `/Applications` with release N, launch | N-1 data present; `daemon.log` has two `daemon_start` events with different versions | UNPROVEN |
| M11 | Repeat M10 but replace the app **immediately** after quitting, while the old daemon still runs | record what happens: the host starts a daemon only when none answers, so the new app may talk to the old daemon until it idles out. Expected for release: the new app works and the old daemon is gone within about 60 s. Anything else is a finding. | UNPROVEN, outcome unknown |
| M12 | Run `install-smoke macos` with N-1 and N on the machine, `--require-signed` | `no failed step`, `UPGRADE: <N-1> -> <N>` with different versions | UNPROVEN |
| M13 | Drag the app to the Trash | the app is gone; `~/.local/share/agent-orch` remains (nothing in the bundle deletes it) | UNPROVEN |

## 5. Windows clean-machine procedure

Use a clean Windows 11 (and Windows 10 if supported) VM, **standard user** (no administrator), a snapshot, Windows
Defender and SmartScreen on, and no Rust, Node, Visual Studio or earlier copy. WebView2 is present on current
Windows 11; on a machine without it record whether the installer installs it. Test `...-windows-x64-signed-setup.exe`
from the release workflow, downloaded through Edge (mark of the web).

| # | Step | Expected | Status |
| --- | --- | --- | --- |
| W1 | `Get-FileHash -Algorithm SHA256` the installer against `SHA256SUMS` | equal | UNPROVEN |
| W2 | `Get-AuthenticodeSignature` and `signtool verify /pa /all` on the installer | `Status: Valid`, the expected publisher, a countersignature timestamp | UNPROVEN |
| W3 | Run the installer interactively | per-user install with no UAC prompt (`installMode: currentUser`); SmartScreen may warn for a new certificate: record the exact dialog and whether "Run anyway" is needed | UNPROVEN |
| W4 | Inspect the install directory | `agent-orch-desktop.exe`, `orchestrator-daemon.exe`, `uninstall.exe`; both executables `Valid` (`install-smoke` fails otherwise; whether Tauri signs the sidecar is **not known**, the first real run decides) | UNPROVEN |
| W5 | Launch from the Start menu | opens; `Get-Process orchestrator-daemon` shows the sidecar from the install directory; `%LOCALAPPDATA%\agent-orch` holds `daemon.token`, `store.sqlite`, `daemon.log` | UNPROVEN |
| W6 | Register a project, send a coordinator message, quit, relaunch | both survive | UNPROVEN |
| W7 | Approvals and cancellation in the GUI | as M8; daemon-level coverage comes from `release_smoke` | UNPROVEN in the GUI |
| W8 | `Stop-Process -Force` the daemon, relaunch | new daemon; the running task is `Failed`; the agent process is gone (job object); no orphan | UNPROVEN |
| W9 | **Upgrade.** Install N-1, do W6, quit, run the N installer over it (same directory, no uninstall) | N-1 data present; `daemon.log` shows two versions; `ProductVersion` of the exe is N | UNPROVEN |
| W10 | Repeat W9 with the app still running | the installer must not leave a mixed install; record whether it asks to close the app. Anything that leaves old and new files mixed is a finding. | UNPROVEN, outcome unknown |
| W11 | `node scripts/release/install-smoke.mjs windows ... --old-artifact-dir <N-1> --require-signed` | `no failed step`; final line `upgrade: a different older build ran (N-1 to N)`; commands in order: `setup.exe /S /D=<dir>` (old), copy of the old sidecar, `setup.exe /S /D=<dir>` (new), `release_smoke`, `uninstall.exe /S _?=<dir>` | UNPROVEN |
| W12 | Uninstall from Settings > Apps, and `uninstall.exe /S` | the program is removed from Apps and the install directory; `%LOCALAPPDATA%\agent-orch` remains (DERIVED from Tauri's NSIS template: it deletes only the app-identifier directory, and only when "delete the application data" is ticked; the data directory here is `agent-orch`, not the identifier). `install-smoke` checks this with a sentinel file in the real `%LOCALAPPDATA%\agent-orch`; whether the silent `/S` uninstaller leaves it is **not known**, the first real run decides | UNPROVEN |

### Expected `install-smoke windows` result (DERIVED, not run on Windows)

`PASS` for each artifact-directory check, installer Authenticode (signed build), a seeded sentinel in
`%LOCALAPPDATA%\agent-orch`, the silent install, the product version, the three installed files, the `Valid`
status of the app and the sidecar, the copy of the old sidecar, the sentinel surviving the upgrade-over install,
the daemon smoke (with a valid structured result), the silent uninstall, "uninstall removed the app" and the
sentinel surviving the uninstall. One `UNPROVEN` line for SmartScreen, UAC and Defender on a clean machine is
always printed. Without `--old-artifact-dir`, or with an old build of the same version, an
`UNPROVEN upgrade from a previous release (N-1 to N)` line is printed and the final line says
`upgrade: NOT proven`. An unsigned artifact prints `INFO ... UNSIGNED installer (Authenticode status
NotSigned)` with `--allow-unsigned` and fails with `--require-signed`. If `LOCALAPPDATA` is unset the sentinel
step is a `FAIL` (fail closed). The sentinel is a file named `install-smoke-sentinel-<nonce>.txt`; it is removed
at the end and the data directory is removed only if the run created it and it is empty.

Windows-specific expectations of `release_smoke` (DERIVED from `orchestrator-runtime`, no Windows run): the named
pipe replaces the Unix socket; step 3 expects the agent to be gone after the daemon is terminated, because agents
run in a `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` job; step 6 expects the interrupted task `Failed` and no relaunch.
`tests/daemon.rs` has no Windows tests, so recovery after a hard kill has **no** native Windows proof before this
smoke runs; a failure at step 3 or 6 on Windows is a finding about the daemon, not a flaky test.

## 6. Not proven here (UNPROVEN)

- Real Apple Developer ID signing, notarization, stapling, Gatekeeper on a clean Mac; the quarantine prompt.
- Real Authenticode signing with a trusted certificate, the timestamp, SmartScreen reputation, Defender, UAC on a
  standard user, WebView2 provisioning.
- Whether Tauri signs the Windows sidecar `orchestrator-daemon.exe` (step W4 and the `install-smoke` check fail if not).
- Any upgrade between two real releases (no previous release artifact exists; the version is `0.1.0` everywhere).
  A same-version run is labelled as such and is never counted as an upgrade pass.
- A pending approval created by an older daemon; schema migration of a store written by an older release (nothing here
  opens one).
- Everything about the GUI: install-launch, review, approve, cancel, relaunch.
- Windows compilation and execution of `release_smoke` and of `install-lib.mjs` flows (they are unit-tested on a
  fake machine on macOS only); the `release.yml` `Install smoke` step on a real runner.

## 7. Risks to decide before a release

- An old daemon that is still running (up to 60 s idle) can answer a freshly upgraded host (M11, W10): no
  version check between host and daemon was found; measure it before shipping an upgrade path.
- `install-smoke` is a hard gate in `release.yml`: a first Windows failure at step 3 or 6 blocks the artifact
  upload until the cause is understood. That is intended, but expect the first real run to fail once.
- The macOS "upgrade" in `install-smoke` runs the old and the new **sidecar** against one data directory; it
  does not replace an installed app in `/Applications` (M10 and M11 cover that by hand).
- The Windows data-survival check writes a sentinel file into the real `%LOCALAPPDATA%\agent-orch` of the user who
  runs it (and removes it afterwards). Run it as a throwaway user or VM, not on a machine with a real store you
  cannot afford to touch. Nothing else in that directory is read, written or deleted by the script.

## 8. Native Windows run (for the Windows agent; H-1 and M-1 stay UNPROVEN until this is run)

Everything here is **UNPROVEN**: it has not been run on Windows. The PowerShell blocks were only parse-checked
(`[System.Management.Automation.Language.Parser]::ParseInput`) with `pwsh` on macOS. Use a throwaway Windows
user or VM with Git, Node (the version in `release.yml`) and the Rust toolchain from `release.yml` installed
(this is a build box, not the clean machine of section 5). Record the Windows version, the commit and every
output in an evidence directory.

```powershell
$ErrorActionPreference = 'Stop'
$Evidence = Join-Path $env:USERPROFILE 'evidence-m6-04-win'
$Work = Join-Path $env:USERPROFILE 'aor'            # keep it short
$Out = Join-Path $env:USERPROFILE 'pkg-new'
$env:CARGO_TARGET_DIR = Join-Path $env:USERPROFILE 'target-aor'
New-Item -ItemType Directory -Force -Path $Evidence | Out-Null
Remove-Item -Recurse -Force $Work, $Out -ErrorAction SilentlyContinue
git rev-parse HEAD | Tee-Object -FilePath (Join-Path $Evidence 'head.txt')
git rev-parse 'HEAD^{tree}' | Tee-Object -FilePath (Join-Path $Evidence 'tree.txt')
git status --porcelain=v1 --untracked-files=all | Tee-Object -FilePath (Join-Path $Evidence 'status-before.txt')
[System.Environment]::OSVersion.VersionString | Tee-Object -FilePath (Join-Path $Evidence 'os.txt')
"node=$(node -v) npm=$(npm.cmd -v)" | Tee-Object -FilePath (Join-Path $Evidence 'versions.txt')

# Use npm.cmd: the npm.ps1 shim fails in some shells with `Unknown command: "pm"`.
Push-Location apps/desktop; npm.cmd ci; Pop-Location
node scripts/release/package.mjs windows --out $Out 2>&1 | Tee-Object -FilePath (Join-Path $Evidence 'package.txt')
if ($LASTEXITCODE -ne 0) { throw "package.mjs exited $LASTEXITCODE" }
cargo build --locked -p orchestrator-daemon --example release_smoke 2>&1 | Tee-Object -FilePath (Join-Path $Evidence 'cargo-build.txt')
if ($LASTEXITCODE -ne 0) { throw "cargo build exited $LASTEXITCODE" }
```

Run the install smoke (the unsigned artifact needs `--allow-unsigned`; use `--require-signed` only with a signed
build). It installs per user, uninstalls, and touches `%LOCALAPPDATA%\agent-orch` with one sentinel file:

```powershell
$Smoke = Join-Path $env:CARGO_TARGET_DIR 'debug\examples\release_smoke.exe'
node scripts/release/install-smoke.mjs windows --artifact-dir $Out --work $Work --smoke-bin $Smoke `
  --allow-unsigned --out (Join-Path $Evidence 'install-smoke-win.json') 2>&1 |
  Tee-Object -FilePath (Join-Path $Evidence 'install-smoke-win.txt')
"exit=$LASTEXITCODE" | Tee-Object -FilePath (Join-Path $Evidence 'install-smoke-win.exit')
Get-Process orchestrator-daemon, release_smoke -ErrorAction SilentlyContinue |
  Tee-Object -FilePath (Join-Path $Evidence 'leftover-processes.txt')
# Explain the checkout state: an empty status-after.txt means clean; otherwise the paths are the explanation.
git status --porcelain=v1 --untracked-files=all | Tee-Object -FilePath (Join-Path $Evidence 'status-after.txt')
git diff --stat | Tee-Object -FilePath (Join-Path $Evidence 'diff-stat-after.txt')
```

`install-smoke` itself prints the commit, `HEAD^{tree}`, the porcelain status before and after, and `changed by this
run`, and writes them under `source` in its JSON. Do not report a bare `checkout_clean_after_run=False`: attach
`status-before.txt`, `status-after.txt` and the paths the run changed. `target/`, `node_modules/` and
`src-tauri/binaries/` are ignored and cannot make a checkout dirty; a listed path is a real change (a tracked file
rewritten, for example by line-ending conversion when `core.autocrlf` is `true`, or a new untracked file).

Notes from the first native run (`m6-04-windows`): `npm ci` printed `EBADENGINE` warnings because `release.yml`
pins Windows Node 22.20.0 while `jsdom` 31.0.0 and `w3c-xmlserializer` 6.0.0 declare `^22.22.2`. They are warnings,
not failures, and the pin was **not** changed in M6-04r (the Windows gates were proven on 22.20.0); record `node -v`
and `npm.cmd -v` (`versions.txt`) and treat a pin bump as its own change. Record the installer's Authenticode status
(`Get-AuthenticodeSignature`) yourself as well: `install-smoke` prints `NotSigned` for an unsigned installer and
`Unknown` plus the reason when PowerShell could not answer; it is never blank.

Expected (DERIVED, not run on Windows): exit 0 for an unsigned build with `--allow-unsigned`; `PASS` for the
artifact directory, the silent install, the product version, the three installed files, the daemon smoke
(`release_smoke: PASS`, 12 steps), the silent uninstall, "uninstall removed the app", and the sentinel survived
the uninstall; `INFO` for the unsigned installer (`Authenticode status NotSigned`); `UNPROVEN` for SmartScreen/UAC/Defender and for the N-1 upgrade
(no `--old-artifact-dir`); final line `upgrade: NOT proven (none)`. No `orchestrator-daemon` or `release_smoke`
process is left. A `FAIL` at smoke step 3 or 6 is a finding about the Windows daemon (job object, recovery), a
`FAIL` on the sentinel after the uninstall means the `/S` uninstaller deletes the user data directory (M-2's
open question; that is a product finding, not a flake).

To rehearse the same-version path add `--old-artifact-dir` pointing at a copy of `$Out`: expect the line
`UNPROVEN upgrade from a previous release (N-1 to N): old and new are both version ...`, a smoke step labelled
`SAME-VERSION rerun; NOT an upgrade`, and `upgrade: NOT proven (same-version)`. A real N-1 to N proof needs the
`package.mjs windows` output of an earlier release (a different version) as `--old-artifact-dir`.

Still UNPROVEN after this run, whatever it prints: Authenticode signing and timestamping of a real certificate
(H-1, needs signing secrets), whether Tauri signs the Windows sidecar, SmartScreen, Defender, UAC on a standard
user, a clean machine (M-1), and the GUI.
