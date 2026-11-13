# M7-32 Windows consolidated native runbook

| Item | Value |
|---|---|
| Pinned source commit | `92f56c3af9cd34741a09667f499164edcafec696` |
| Pinned tree | `a3012626c6808b337a719d0afcd4d887f5a504ea` |
| Runbook URL (fetch this file from here) | `https://github.com/j-mckerracher/agent-orch/blob/runbook/m7-32-windows-92f56c3af/docs/evidence/M7-32-windows-runbook.md` |
| Evidence ref to create | `evidence/m7-32-win-92f56c3af-<UTC timestamp>`, timestamp format `yyyyMMddTHHmmssZ` |
| Machine | Windows 11, interactive desktop, Rust, Visual Studio 2022 C++ tools, WebView2 Runtime. Node is provided by §1.5 if the machine's Node is too old. |

This runbook is self-contained. Run it top to bottom on one Windows 11 machine. You are testing the pinned commit only. You do not change code, board files or any branch other than the evidence ref in §5. You do not dispatch work to other machines.

Run order: §0 rules, §1 preflight, §2 automated gates, §3 native setup, §4 native rows, §5 results and publication, §6 cleanup.

## 0. Rules

1. Test the pinned commit only. Report defects. Do not fix code, do not edit tracked files, do not run `git clean`, `git reset` or `git stash` in the checkout.
2. Machine settings you change (Windows display scale, Animation effects, Contrast themes, Narrator or NVDA on/off, theme mode) must be recorded before and after and restored before you finish. Do not run `setx`, do not change global Git configuration, do not change machine or user PATH. Use process-scope environment variables only.
3. Never elevate. Do not install software or start an installer based only on a PATH check. If a required tool is missing, mark the affected rows `BLOCKED` and continue with the rows that do not need it.
4. No secrets, tokens, credentials, personal data or real project data in evidence. Use only the isolated daemon data directory created in §1.1. Use disposable test repositories only.
5. Every run uses a fresh GUID run root. If a path already exists, stop and choose a new GUID. Never delete a shared, existing or user path. Delete only what this run created, and only in §6.
6. Stop only the daemon whose command line contains this run's `$DataDir`. Never kill other processes.
7. Status values per row: `PASS` (observed the expected result), `FAIL` (observed a different result; say what), `BLOCKED` (a named prerequisite is missing or the check could not be run), `BLOCKED-EXTERNAL` (needs signing material or two released artifacts this run does not have). Do not write PASS for anything you did not observe.
8. Preserve failed output, logs and screenshots. Never overwrite a file. If an expected file already exists, stop that row and write `BLOCKED` with the path.

## 1. Preflight

### 1.1 Run root, helpers, free space, isolated environment

Run this in Windows PowerShell 5.1 or PowerShell 7. If script execution is blocked, run `Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass` (process scope only).

```powershell
$ErrorActionPreference = 'Continue'
$SourceSha = '92f56c3af9cd34741a09667f499164edcafec696'
$ExpectedTree = 'a3012626c6808b337a719d0afcd4d887f5a504ea'
$RunBase = 'C:\t'   # an existing short local folder; no spaces, no cmd.exe metacharacters, not a mapped drive
if ($RunBase -match '[&|<>^()%!"\s]' -or -not (Test-Path -LiteralPath $RunBase -PathType Container)) {
  throw 'BLOCKED: choose an existing short local folder without spaces or cmd.exe metacharacters.'
}
$RunRoot = Join-Path $RunBase ('m7-32-' + [guid]::NewGuid().ToString('N'))
$Src = Join-Path $RunRoot 'src'
$Out = Join-Path $RunRoot 'evidence-out'
$DataDir = Join-Path $RunRoot 'daemon-data'
New-Item -ItemType Directory -Path $RunRoot | Out-Null
New-Item -ItemType Directory -Path $Out | Out-Null

function Invoke-Gate {
  param([string]$Id, [string]$Dir, [string]$Exe, [string[]]$Arguments)
  $Prefix = Join-Path $Out $Id
  if (Test-Path -LiteralPath "$Prefix.exit.txt") { throw "BLOCKED: evidence for $Id already exists; start a new run." }
  Set-Content -LiteralPath "$Prefix.command.txt" -Value @("cwd=$Dir", "command=$Exe $($Arguments -join ' ')")
  Push-Location -LiteralPath $Dir
  try {
    & $Exe @Arguments 1> "$Prefix.stdout.txt" 2> "$Prefix.stderr.txt"
    $Code = $LASTEXITCODE
  } finally { Pop-Location }
  Set-Content -LiteralPath "$Prefix.exit.txt" -Value $Code
  return $Code
}

$Drive = Get-PSDrive -Name $RunBase.Substring(0, 1)
@("utc=$([DateTime]::UtcNow.ToString('o'))", "free_gib=$([math]::Round($Drive.Free / 1GB, 2))") |
  Set-Content -LiteralPath (Join-Path $Out 'P01-free-space.txt')
if ($Drive.Free -lt 20GB) { throw 'BLOCKED: less than 20 GiB free on the run volume.' }

$FmNames = @(Get-ChildItem Env: | Where-Object { $_.Name -like 'FM_*' } | ForEach-Object { $_.Name })
$FmNames | Set-Content -LiteralPath (Join-Path $Out 'P01-fm-variable-names.txt')
foreach ($Name in $FmNames) { Remove-Item -LiteralPath "Env:$Name" }
$env:AGENT_ORCH_DATA_DIR = $DataDir
$env:CARGO_TARGET_DIR = Join-Path $RunRoot 'cargo-target'
```

Record the names only of any `FM_*` variables; never their values.

### 1.2 Fresh isolated checkout at the pinned commit

```powershell
if ((Invoke-Gate 'P02-clone' $RunRoot 'git' @('clone', '--no-checkout', 'https://github.com/j-mckerracher/agent-orch.git', $Src)) -ne 0) {
  throw 'BLOCKED: clone failed; see P02 files (check GitHub credentials on this machine).'
}
Invoke-Gate 'P03-config-autocrlf' $Src 'git' @('config', '--local', 'core.autocrlf', 'false') | Out-Null
Invoke-Gate 'P03-config-longpaths' $Src 'git' @('config', '--local', 'core.longpaths', 'true') | Out-Null
if ((Invoke-Gate 'P04-checkout' $Src 'git' @('checkout', '--detach', $SourceSha)) -ne 0) { throw 'BLOCKED: pinned commit checkout failed.' }
Invoke-Gate 'P05-head' $Src 'git' @('rev-parse', 'HEAD') | Out-Null
Invoke-Gate 'P06-tree' $Src 'git' @('rev-parse', 'HEAD^{tree}') | Out-Null
$Head = (Get-Content -LiteralPath (Join-Path $Out 'P05-head.stdout.txt') -TotalCount 1).Trim()
$Tree = (Get-Content -LiteralPath (Join-Path $Out 'P06-tree.stdout.txt') -TotalCount 1).Trim()
if ($Head -cne $SourceSha -or $Tree -cne $ExpectedTree) { throw 'BLOCKED: HEAD or tree differs from the pinned values; stop.' }
Invoke-Gate 'P07-status' $Src 'git' @('status', '--porcelain=v1', '--untracked-files=all') | Out-Null
if (((Get-Content -LiteralPath (Join-Path $Out 'P07-status.stdout.txt')) -join '').Trim()) {
  throw 'BLOCKED: fresh checkout is not clean; stop.'
}
```

Row P07 passes only if the tree and HEAD match and the status is empty.

### 1.3 Tools, WebView2 Runtime, session, Visual Studio C++

```powershell
Invoke-Gate 'P08-git-version' $RunRoot 'git' @('--version') | Out-Null
Invoke-Gate 'P09-cargo-version' $RunRoot 'cargo' @('-V') | Out-Null
Invoke-Gate 'P10-rustc-host' $RunRoot 'rustc' @('-vV') | Out-Null
Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber | Format-List |
  Out-String | Set-Content -LiteralPath (Join-Path $Out 'P11-windows.txt')
$WebView2 = Get-ItemProperty -Path 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' -ErrorAction SilentlyContinue
if (-not $WebView2.pv) { $WebView2 = Get-ItemProperty -Path 'HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' -ErrorAction SilentlyContinue }
"webview2_runtime_version=$($WebView2.pv)" | Set-Content -LiteralPath (Join-Path $Out 'P12-webview2.txt')
"user_interactive=$([Environment]::UserInteractive)" | Set-Content -LiteralPath (Join-Path $Out 'P13-desktop.txt')
Invoke-Gate 'P13-session' $RunRoot 'query.exe' @('session') | Out-Null
$Vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$VsPath = & $Vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null | Select-Object -First 1
if (-not $VsPath) { throw 'BLOCKED: Visual Studio C++ tools not found by vswhere; Rust and daemon rows cannot run.' }
Invoke-Gate 'P14-where-cl-before' $RunRoot 'where.exe' @('cl.exe') | Out-Null
& (Join-Path $VsPath 'Common7\Tools\Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation
Invoke-Gate 'P15-where-cl-devshell' $RunRoot 'where.exe' @('cl.exe') | Out-Null
```

Row P12 needs a WebView2 version. If it is empty, the native rows are `BLOCKED` (not run), but the automated gates still run.

### 1.4 Node version gate

The lockfile requires Node `^22.22.2 || ^24.15.0 || >=26.0.0`. Node 22 below 22.22.2 is unsupported.

```powershell
function Test-NodeSupported([string]$Text) {
  $Version = $null
  if (-not [version]::TryParse($Text.Trim().TrimStart('v'), [ref]$Version)) { return $false }
  return (($Version.Major -eq 22 -and $Version -ge [version]'22.22.2') -or
          ($Version.Major -eq 24 -and $Version -ge [version]'24.15.0') -or
          $Version.Major -ge 26)
}
Invoke-Gate 'P16-node-version' $RunRoot 'node' @('-p', 'process.versions.node') | Out-Null
$NodeText = (Get-Content -LiteralPath (Join-Path $Out 'P16-node-version.stdout.txt') -TotalCount 1)
Invoke-Gate 'P17-npm-version' $RunRoot 'npm.cmd' @('--version') | Out-Null
```

### 1.5 Node 24 remedy (only if §1.4 is not supported)

Use the official Node 24 LTS x64 zip for this run only, unzipped under the run root, prepended to this process's PATH. Do not install it and do not change PATH outside this PowerShell process. The zip and `SHASUMS256.txt` come from nodejs.org over HTTPS. Checksum mismatch is a `BLOCKED` result; stop and report.

```powershell
if (-not (Test-NodeSupported $NodeText)) {
  [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
  $Index = Invoke-RestMethod -Uri 'https://nodejs.org/dist/index.json'
  $Release = @($Index | Where-Object { $_.version -match '^v24\.\d+\.\d+$' -and $_.lts -and ($_.files -contains 'win-x64-zip') } | Select-Object -First 1)
  if ($Release.Count -ne 1) { throw 'BLOCKED: no Node 24 LTS win-x64 zip in the official index.' }
  $V = $Release[0].version
  $Zip = "node-$V-win-x64.zip"
  $NodeDir = Join-Path $RunRoot "node-$V-win-x64"
  Invoke-WebRequest -Uri "https://nodejs.org/dist/$V/$Zip" -OutFile (Join-Path $RunRoot $Zip) -UseBasicParsing
  Invoke-WebRequest -Uri "https://nodejs.org/dist/$V/SHASUMS256.txt" -OutFile (Join-Path $RunRoot 'SHASUMS256.txt') -UseBasicParsing
  $Line = @(Get-Content -LiteralPath (Join-Path $RunRoot 'SHASUMS256.txt') | Where-Object { $_ -match ('  ' + [regex]::Escape($Zip) + '$') })
  if ($Line.Count -ne 1) { throw 'BLOCKED: checksum line for the zip is missing or ambiguous.' }
  $Expected = ($Line[0] -split '\s+')[0].ToLowerInvariant()
  $Actual = (Get-FileHash -LiteralPath (Join-Path $RunRoot $Zip) -Algorithm SHA256).Hash.ToLowerInvariant()
  @("zip=https://nodejs.org/dist/$V/$Zip", "sums=https://nodejs.org/dist/$V/SHASUMS256.txt",
    "expected_sha256=$Expected", "actual_sha256=$Actual") | Set-Content -LiteralPath (Join-Path $Out 'P18-node-download.txt')
  if ($Actual -cne $Expected) { throw 'BLOCKED: Node zip SHA-256 mismatch.' }
  Expand-Archive -LiteralPath (Join-Path $RunRoot $Zip) -DestinationPath $RunRoot
  $env:PATH = "$NodeDir;$env:PATH"
  Invoke-Gate 'P19-node-remedy-version' $RunRoot 'node' @('-p', 'process.versions.node') | Out-Null
  $NodeText = (Get-Content -LiteralPath (Join-Path $Out 'P19-node-remedy-version.stdout.txt') -TotalCount 1)
  if (-not (Test-NodeSupported $NodeText)) { throw 'BLOCKED: downloaded Node 24 still fails the version gate.' }
}
```

### 1.6 Interactive desktop

Row P13 passes only if `user_interactive=True` and the `query session` output shows an active console or RDP session with a display. If not, native rows are `BLOCKED`.

## 2. Automated gates

Each gate writes `G..-<name>.command.txt`, `.stdout.txt`, `.stderr.txt`, `.exit.txt` in `evidence-out`. Gates are independent: run each one even if an earlier gate failed. Record the exit code and pass counts (`Tests  N passed`, `test result: ok. N passed`, `# pass N`).

```powershell
$Desk = Join-Path $Src 'apps\desktop'
if ((Invoke-Gate 'G01-npm-ci' $Desk 'npm.cmd' @('ci')) -eq 0) {
  Invoke-Gate 'G02-npm-test' $Desk 'npm.cmd' @('test') | Out-Null
  Invoke-Gate 'G03-npm-build' $Desk 'npm.cmd' @('run', 'build') | Out-Null
  Invoke-Gate 'G04-contract-test' $Desk (Join-Path $Desk 'node_modules\.bin\vitest.cmd') @('run', 'src/sources/surfaces/contract.test.ts') | Out-Null
} else {
  'NOT RUN: npm ci failed (G01).' | Set-Content -LiteralPath (Join-Path $Out 'G02-G04-not-run.txt')
}
Invoke-Gate 'G05-scripts-tests' $Src 'node' @('--test', 'scripts/**/*.test.mjs') | Out-Null
Invoke-Gate 'G06-cargo-test-workspace' $Src 'cargo' @('test', '--locked', '--workspace') | Out-Null
Invoke-Gate 'G07-cargo-clippy' $Src 'cargo' @('clippy', '--locked', '--workspace', '--', '-D', 'warnings') | Out-Null
Invoke-Gate 'G08-daemon-build' $Src 'cargo' @('build', '--locked', '-p', 'orchestrator-daemon', '--examples', '--bins') | Out-Null
```

| Row | Command (cwd) | Expected |
|---|---|---|
| G01 | `npm.cmd ci` (`apps\desktop`) | exit 0 |
| G02 | `npm.cmd test` (`apps\desktop`) | exit 0; record test-file and test counts |
| G03 | `npm.cmd run build` (`apps\desktop`) | exit 0 |
| G04 | `node_modules\.bin\vitest.cmd run src/sources/surfaces/contract.test.ts` (`apps\desktop`) | exit 0 |
| G05 | `node --test "scripts/**/*.test.mjs"` (repo root; the glob is passed to Node unexpanded) | exit 0; record `# pass` and `# fail` |
| G06 | `cargo test --locked --workspace` (repo root) | exit 0; record every `test result:` line |
| G07 | `cargo clippy --locked --workspace -- -D warnings` (repo root) | exit 0 |
| G08 | `cargo build --locked -p orchestrator-daemon --examples --bins` (repo root) | exit 0; `orchestrator-daemon.exe` exists under `$env:CARGO_TARGET_DIR\debug\` |

If G06 fails, keep the full output, quote the first compiler or test failure block in RESULTS.md, and continue. Do not use a text filter as the command status; the status is always `.exit.txt`.

## 3. Native setup

Native rows need G08 to pass, an interactive desktop (P13), and WebView2 (P12).

```powershell
$env:AGENT_ORCH_DATA_DIR = $DataDir
$env:AGENT_ORCH_DAEMON_BIN = Join-Path $env:CARGO_TARGET_DIR 'debug\orchestrator-daemon.exe'
if (-not (Test-Path -LiteralPath $env:AGENT_ORCH_DAEMON_BIN)) { throw 'BLOCKED: daemon binary missing; all live and native rows are BLOCKED.' }
$env:AGENT_ORCH_CONVERSATION_AGENT = 'codex'
Set-Location -LiteralPath (Join-Path $Src 'apps\desktop')
npm.cmd run tauri dev *> (Join-Path $Out 'NAT-tauri-dev-1.log')
```

Run the command in the foreground. It returns when the operator closes the app window. Record the close time and the window size in `NAT-tauri-dev-1.log` notes (write them in RESULTS.md, not into the raw log).

Live rows need Codex CLI signed in at the approved pin `0.167.0`. Check `codex --version` and record it as `P20-codex-version`. If the version differs or Codex is not signed in, mark every live row `BLOCKED`. Do not add a pin and do not substitute fixtures. Live rows must not use `?fixture=1`.

Screenshots: use Windows Snipping Tool (Win+Shift+S) and save PNG files into `$Out` with the row ID as prefix. If capture is denied (for example `GetCursorPos failed: Access is denied`), record the full error text as the row's note and mark the row `BLOCKED`. Do not elevate and do not substitute a browser screenshot.

Daemon process lookup for this run only (PID and command line are recorded; never kill other processes):

```powershell
Get-CimInstance Win32_Process -Filter "Name='orchestrator-daemon.exe'" |
  Where-Object { $_.CommandLine -like "*$DataDir*" } |
  Select-Object ProcessId, ExecutablePath, CommandLine | Format-List | Out-String |
  Set-Content -LiteralPath (Join-Path $Out 'NAT-daemon-process.txt')
```

## 4. Native rows

Rows marked `(macOS only)` in the source checklists are not part of this runbook. VoiceOver, WKWebView, Cmd+Q, Retina, `codesign`, `spctl`, `xcrun stapler` and the macOS install-smoke rows are not run on Windows.

Source key: `N##` = `docs/evidence/m7-30/native-matrix.md`; `M7-##` = the item checklist in `docs/evidence/`; `M7-30 §n` = `docs/evidence/m7-30/WINDOWS-MANUAL-CHECKLIST.md`; `M6-04 W##` = Windows rows of `docs/evidence/M6-04-native-checklist.md`. Each row lists its sources; duplicated checks appear once.

Evidence files are written to `$Out` (`evidence-out`). Each file name starts with the row ID. The RESULTS.md line for a row lists these files.

### 4.1 Matrix rows (N01–N12)

**N01 Package install, launch and uninstall (unsigned NSIS).** Sources: N01; M7-30 §1; M7-18 row 2 (install part); M6-04 W11 and W15 (unsigned variant).
- Steps: (1) `npm.cmd run package:win` in `apps\desktop`; record exit code and the NSIS `.exe` path under `src-tauri\target\release\bundle\nsis\` (path redacted in notes). (2) Record `Get-FileHash -Algorithm SHA256` of the installer. (3) Install for the current user without elevation. Record any SmartScreen prompt text. (4) Launch from the Start menu. Record the default window size and the minimum window size reached. (5) Confirm the daemon runs from the installed folder (`NAT-daemon-process.txt` style lookup, installed path). (6) Uninstall from Settings > Apps.
- Expected: no UAC prompt; app and daemon install and launch to the connected UI; uninstall removes the app folder; `%LOCALAPPDATA%\agent-orch` remains. Signature validity: `BLOCKED-EXTERNAL` (the build is unsigned).
- Evidence: `N01-package.txt`, `N01-sha256.txt`, `N01-install-window.png`, `N01-uninstall.txt`.

**N02 Themes, contrast and forced colors.** Sources: N02; M7-01 steps 1–6 (Windows parts); M7-15 row 5; M7-30 §2.
- Steps: (1) Record the saved theme (`agent-orch.theme`) state. If an earlier choice exists, system-default behavior is `BLOCKED` (do not clear the WebView profile). Otherwise set Windows app mode to Light, then Dark (Settings > Personalization > Colors), confirm the UI follows, restore the original mode. (2) Use the theme toggle to choose dark, close, relaunch with the same command in §3, confirm dark remains; repeat with light. (3) Enable each Windows Contrast theme (Aquatic, Desert, Dusk, Night sky) one at a time (Settings > Accessibility > Contrast themes), check text, borders, focus rings, status strip, badges, PLACEHOLDER labels and disabled controls; restore the original setting after. (4) Check forced-colors rendering (same settings). (5) Confirm every state has text or an icon in addition to color. (6) Confirm the computed font is a legible fallback when Avenir Next is unavailable (DevTools, F12, computed font). (7) In DevTools console run `CSS.supports('background','color-mix(in srgb, red 50%, blue)')`, `CSS.supports('height','100dvh')`, `CSS.supports('selector(:has(*))')`, `matchMedia('(prefers-reduced-motion: reduce)').media`, `matchMedia('(prefers-color-scheme: dark)').media`; record the results.
- Expected: light and dark follow the OS mode when no choice is saved; the explicit choice persists across relaunch; contrast themes and forced colors keep text, borders and focus visible; no status is conveyed by color alone; the four CSS checks return `true`.
- Evidence: `N02-light.png`, `N02-dark.png`, `N02-contrast-<theme>.png`, `N02-forced-colors.png`, `N02-css-support.txt`, `N02-theme-persist.txt`.

**N03 Responsive widths, top bar and DPI.** Sources: N03; M7-10 row 8; M7-12 row 2; M7-14 row 6; M7-15 row 7; M7-16 rows 2 and 7; M7-17 row 2; M7-23 row 4; M7-30 §3.
- Steps: (1) At window heights 700 and 900 resize to viewport widths 1280, 1150, 960, 760 and 480 CSS px, then 390 (`window.innerWidth` in DevTools console). Record each exact `innerWidth`. (2) At each width check: no horizontal document scroll; no clipped or overlapping content; the sticky resource strip never hides a focused control at 960 or narrower; the composer is not covered by the activity panel at 960 or narrower; the resource strip sits in normal flow after main content; main has bottom clearance. (3) At 961 the activity column is right of main; at 960 it stacks above main (measure in CSS px). (4) At 760 the top bar order is usage, search, counts, with search on its own full-width row; save the `getBoundingClientRect()` of the three `.shell-topbar` children as JSON. (5) Set Settings > System > Display > Scale to 100, 125, 150 and 200 percent (restart the app only if Windows requests it), repeat steps 3 and 4 at each scale, record `devicePixelRatio`; restore the original scale at the end. (6) Long titles, IDs and paths truncate or wrap without overlapping neighbours (Segoe UI fallback).
- Expected: as listed; no overlap or clipping at any scale; breakpoints measured in CSS px.
- Evidence: `N03-w<width>-h<height>.png`, `N03-760-geometry.json`, `N03-dpi<percent>-<width>.png`, `N03-results.txt` (innerWidth, devicePixelRatio, scale and PASS/FAIL per check).

**N04 Keyboard-only operation.** Sources: N04; M7-10 target picker; M7-12 row 2 (disclosures); M7-23 row 3 (tree keys); M7-30 §4.
- Steps: With no mouse: (1) first Tab stop is "Skip to main content"; Enter moves focus to main. (2) Tab and Shift+Tab reach every control with a visible focus ring, in light, dark and one contrast theme. (3) Workstream tree: Up, Down, Home, End, Enter. (4) Target picker opens with Enter or Space; Escape returns focus to its trigger; no focus trap. (5) Ctrl+Enter sends a Workspace message. (6) Review: Accept, Reject and Request changes work from the keyboard; destructive actions require confirmation. (7) Disclosures (Workspace activity, Execution queue and subgroups, Attention, Agent slots) toggle with Enter or Space. (8) No keyboard trap in menus or the activity panel.
- Expected: every step works; focus is visible and returned correctly.
- Evidence: `N04-keyboard-log.txt` (key sequence and focus observation per step), `N04-focus-<control>.png` for any failure.

**N05 Screen readers: Narrator and NVDA.** Sources: N05; M7-10 row 6; M7-14 row 4; M7-16 row 6; M7-17 row 3; M7-23 row 3; M7-15 row 6; M7-30 §5.
- Steps: Narrator (Win+Ctrl+Enter to start and stop). Check: (1) landmarks banner, primary navigation, main and the Workspace activity complementary region are announced with names. (2) Route headings are announced on navigation. (3) Tree level, position and expanded state are announced. (4) Connection changes (Connected, Reconnecting, stale data) are announced through status or alert regions. (5) Mutation outcomes (created, linked, sent, accepted, rejected, failed) are announced. (6) Target picker: open, type a query with no matches, hear "No matching conversations", select a result with ArrowDown and radio-list arrows, Escape returns focus. (7) Device resources: expand, move through CPU, Memory, GPU, Disk; each reading announces name, value, unit, scope and age; unavailable and unsupported are distinct text; fixture values are identified as placeholder; live mode has no placeholder badge. (8) Disabled controls expose their reason. Repeat (1)–(8) with NVDA if NVDA is installed; if not, NVDA rows are `BLOCKED` (do not install it).
- Expected: each item announced as described; transcribe the spoken output verbatim.
- Evidence: `N05-narrator-transcript.txt`, `N05-nvda-transcript.txt` (or BLOCKED note).

**N06 Reduced motion, hidden pause and idle resource cost.** Sources: N06; M7-18 rows 3 and 5; M7-21 row 3; M7-30 §6.
- Steps: (1) Settings > Accessibility > Visual effects > Animation effects: Off (record the original value and restore it at the end). Confirm `matchMedia('(prefers-reduced-motion: reduce)').matches === true`. Turn Background on: the layer reports `data-engine="static"` and shows a still dot field; no transitions run. (2) With Animation effects On and Background On, record `document.hidden` while visible; minimize the app for 60 seconds, restore, and record `document.hidden` and the `visibilitychange` transitions; animation work pauses while hidden and resumes on restore. (3) Record process CPU and GPU utilisation every 5 seconds for 60 seconds visible-idle and 60 seconds hidden, with Background on and then off (Task Manager Details, or `Get-Counter '\Process(*orchestrator*)\% Processor Time'`). Save raw samples. No numeric budget is defined for M7-32; do not give a PASS or FAIL budget verdict. (4) M7-21 row 3: if a daemon-side RPC trace of `host.metrics` dispatch timestamps is available, confirm no dispatch while hidden and a fresh one on restore; otherwise `BLOCKED`.
- Expected: static under reduced motion; hidden pause and resume; raw samples saved.
- Evidence: `N06-reduced-motion.txt`, `N06-visibility.txt`, `N06-resource-samples.csv`, `N06-host-metrics-trace.txt` (or BLOCKED note).

**N07 Native navigation and deep links.** Sources: N07; M7-15 routes; M7-30 navigation.
- Steps: In the packaged WebView (N01) and the dev app, visit `#/workspace`, `#/workstreams`, `#/agents`, `#/review`, and the legacy routes `#/projects`, `#/project/<id>`, `#/project/<id>/tasks/all`, `#/project/<id>/task/<task-id>`, `#/project/<id>/coordinator`, `#/project/<id>/review/<task-id>`. Use IDs returned by the isolated app only; if none exist, mark those routes `BLOCKED`. Visit `#/nope` (unknown route fallback). Use Alt+Left and Alt+Right for back and forward; confirm route and focus. The ConnectionStrip is present on every route.
- Expected: each route renders; unknown route falls back; back and forward work; no console errors.
- Evidence: `N07-routes.txt`, `N07-<route>.png` for any failure.

**N08 Daemon connection failure and recovery (Windows named pipe).** Sources: N08; M7-15 row 3; M7-20 (drop and restore the connection); M7-30 §7.
- Steps: (1) With the app connected, record this run's daemon PID with the §3 lookup. (2) Stop only that PID (`Stop-Process -Id <pid>`). Observe Reconnecting, stale indicators, mutations disabled, and no fixture data. (3) Restart the app (or wait for the host to relaunch the daemon). Confirm Connected and data refresh. (4) Leave no client connected for more than 60 seconds; record that the daemon exits and that the host relaunches it on the next connection. (5) Record the named pipe endpoint from the daemon log (redact the user part) and confirm handshake, reconnect and shutdown on app exit. (6) Slow reader and event-epoch reset: if the native UI cannot induce them, mark `BLOCKED`.
- Expected: stale and reconnecting states visible; live failures never show fixtures; recovery to Connected; named pipe handshake, reconnect and shutdown work.
- Evidence: `N08-stop-restart.txt`, `N08-reconnecting.png`, `N08-connected.png`, `N08-daemon-log-excerpt.txt` (redacted).

**N09 Real recovery lifecycle with a live agent.** Sources: N09; M6-04 W12 (lifecycle part); M7-08 row 3 (restart); M7-10 row 2; M7-23 row 2 (persistence); M7-25 row 3.
- Steps: Needs Codex 0.167.0 signed in (P20). (1) In a disposable test repository registered in the isolated app, start a long real Codex task; record daemon and agent PIDs. (2) Close the app window, reopen; the same agent and task are still running; task state and transcript preserved. (3) Cancel the task: `Cancelled`, and the agent process exits with no orphan. (4) Start a second long task; stop only this run's daemon PID; relaunch the app; the interrupted task becomes `Failed` and is not resumed silently; no orphan process remains. (5) Targets and drafts: select two conversation targets, type a distinct unsent draft in each, quit with the app menu or Alt+F4, relaunch; the selected target and both drafts are restored; each transcript stays under its own target. (6) Route and theme persist. (7) Record whether a "history incomplete" label appears; if not reproduced, say so.
- Expected: as listed.
- Evidence: `N09-pids.txt`, `N09-lifecycle.txt`, `N09-restart-drafts.png`, `N09-daemon-log-excerpt.txt` (redacted).

**N10 Real approvals and the fixture boundary.** Sources: N10; M7-13 row 2; M6-04 W12 (approvals part); M7-10 row 4 (approvals); M7-30 §8.
- Steps: Create two verified candidates from real agent tasks in the disposable repository. (1) In Review & Decisions, Accept the first candidate: the daemon records the approval; no delivery is requested (check delivery state and no new ref). Refresh; the approval persists. (2) Request changes on the second with non-empty feedback: the daemon records `changes requested`, not rejection. (3) Retry the same approval: one approval record only (idempotent). (4) Change a candidate and confirm a fresh approval is required; if the test cannot change a candidate, mark `BLOCKED`. (5) Fixture boundary: open the app at `#/review?fixture=1`, attempt Accept; confirm no approval request reaches the daemon (daemon log or RPC trace); if no trace is available, mark `BLOCKED`.
- Expected: as listed.
- Evidence: `N10-approve.png`, `N10-request-changes.png`, `N10-approval-state.txt`, `N10-fixture-boundary.txt`.

**N11 Live data honesty.** Sources: N11; M7-16 row 4; M7-17 row 4; M7-20 row 2; M7-21 row 2; M7-22 row 2; M7-14 row 1 (unknown usage); M7-30 §9.
- Steps: In live mode (no `?fixture=1`, daemon connected): (1) Search the page text (DevTools console: `document.body.innerText.includes('$26.00')`, `...includes('20/24')`, `...includes('CPU 41/44')`); all three must be false. (2) Device resources: each reading matches the daemon `host.metrics` response within the tolerances in `docs/host-metrics.md` (in the checkout); GPU shows "unsupported" text, never 0. (3) Usage pill with no reported usage: "not available", never `$0` or `$26.00`; any shown amount is labelled reported or estimated. (4) Unknown or unavailable never displays as zero. (5) No PLACEHOLDER badge in live mode.
- Expected: as listed.
- Evidence: `N11-live-readings.png`, `N11-host-metrics-response.json` (sanitized), `N11-usage.png`, `N11-checks.txt`.

**N12 Installed package security: CSP, bundled assets, paths.** Sources: N12; M7-18 rows 2 and 4 (console and CSP); M7-30 §10; M6-04 (artifact paths).
- Steps: In the installed app (N01): (1) read the CSP (`document.querySelector('meta[http-equiv="Content-Security-Policy"]')?.content`); it must contain `script-src 'self'` and no `unsafe-eval` and no external origins. (2) Record console CSP violations (expected none). (3) Turn Background on: the `ocean-renderer-*.js` asset loads from the installed app origin (DevTools Network). (4) No external network requests during the session (Network tab). (5) Fonts load or fall back without network. (6) Register a disposable repository under a path with spaces and a non-ASCII folder name (for example `C:\t\m7 repo ü`) and confirm it opens; if the path cannot be created on this machine, mark that check `BLOCKED`.
- Expected: as listed.
- Evidence: `N12-csp.txt`, `N12-console.txt`, `N12-network.txt`, `N12-paths.txt`.

### 4.2 Per-item rows (de-duplicated against 4.1)

**F08 Daemon IPC, live Codex and native conversation (M7-08 rows 1–4).**
- Steps: (1) `cargo test --locked -p orchestrator-daemon --test conversation_runtime -- --exact replies_are_ordered_correlated_and_carry_history_and_workstream_context`, then the same with `--exact replies_survive_a_restart_replay_by_sequence_and_stay_idempotent`, `--exact cancel_stops_a_streaming_reply_and_later_output_is_discarded`, `--exact a_turn_the_daemon_lost_is_failed_on_restart_not_left_running`. Use the same `$env:CARGO_TARGET_DIR`. (2) With `$env:AGENT_ORCH_LIVE_CONVERSATION = '1'`: `cargo test --locked -p orchestrator-daemon --test conversation_runtime -- --ignored --exact live_codex_answers_a_global_message --nocapture` (must run, not be skipped; reply contains `pong`). (3) In the native app: Global orchestrator, `Reply with exactly: pong`, reply appears and completes; send a long prompt, cancel after the first chunk, confirm Cancelled and no later chunks; send `Reply with exactly: resume-check` and confirm completion; then stop only this run's daemon and confirm the transcript persists in order. (4) Negative: `cargo test --locked -p orchestrator-agents codex_without_credentials_offers_nothing`; `cargo test --locked -p orchestrator-daemon runtime_failures_fail_the_turn_with_a_stable_code_and_keep_the_message`; `cargo test --locked -p orchestrator-daemon --test conversation_runtime a_version_that_is_not_pinned_never_receives_the_message`.
- Expected: four daemon tests pass; live test runs and returns `pong`; cancel freezes the transcript; restart preserves order; negative tests pass.
- Evidence: `F08-daemon-tests.log`, `F08-live-codex.log`, `F08-webview-*.png`, `F08-negative.log`.

**F10 Conversation targets: real-daemon notes and Global truthfulness (M7-10 rows 2–4; restart, Narrator and layout are in N09, N05, N03).**
- Steps: In a dedicated test project, send a unique benign message to its coordinator; the daemon records a note, no reply is presented, and Agents & Sessions and Review counts are unchanged (record before and after). Select Global and send a unique benign message: a reply shows only if the daemon recorded it; if `runtime_unavailable` appears, the UI shows the unavailable status and no reply. Open Model settings: non-launchable providers are disabled; after a launchable local selection, a message sends.
- Expected: as listed.
- Evidence: `F10-live.png`, `F10-live.txt` (counts before and after, daemon transcript excerpt).

**F11 Workstreams with live data (M7-11 row 2).**
- Steps: Two disposable projects, each with a real-agent task; link tasks into daemon workstreams. `#/workstreams` shows the daemon workstreams (not project-derived groups); header, list, tree and task counts agree; worker rows show real session adapter and state; unlinked tasks are not invented as groups; disabled actions show reasons; keyboard works; counts survive refresh; independent scrolling works for 100 or more live tasks. If fewer than 100 live tasks exist, mark that check `BLOCKED` (do not use fixtures).
- Evidence: `F11-workstreams.png`, `F11-counts.txt`, `F11-scroll.txt` (or BLOCKED).

**F12 Live activity panel (M7-12 row 2).**
- Steps: Two real running tasks and one pending approval appear; badges equal the Attention list; top-bar counts equal the lists; submit a task or cause an approval without reloading and confirm counts update; check 961 and 960 layout at 100 and 200 percent (values recorded in N03); each disclosure works by keyboard.
- Evidence: `F12-live-activity.png`, `F12-event-refresh.png`, `F12-results.txt`.

**F14 Agents and Sessions inventory (M7-14 rows 2 and 6; Narrator in N05; DPI in N03).**
- Steps: Real daemon and real agents; the page is Connected and live with no PLACEHOLDER. Each inventory flag (installed, launchable, capabilities) matches the `agents.capabilities` response. Pi reports installed and `launchable: false` when reported. "Available" equals the count of adapters with both installed and launchable true. Headline and five tab labels match real tasks and sessions. Select the running and failed tasks: attempt history and session feed are shown. Cancel on the running task: the acknowledgement does not claim the process stopped. Retry on the failed task creates a new queued attempt. Disabled controls (Configure providers, Amend TaskContract, Request new decomposition, Resume session, Inspect retained checkpoint, Review routing proposal, Choose manual assignment) show readable reasons.
- Evidence: `F14-agents.png`, `F14-agents.md`.

**F15 Launch surface and five-route visual scan (M7-15 rows 4 and 5).**
- Steps: Record that the launched surface is WebView2 (`navigator.userAgent` contains `Edg/`). Visit `#/projects`, `#/project/<id>`, `#/project/<id>/tasks/all`, `#/project/<id>/task/<task-id>`, `#/project/<id>/coordinator` in light and dark; the ConnectionStrip is visible on all five; no clipped text; no color-only status. Record Connected, Reconnecting and stale states as they occur.
- Evidence: `F15-launch.txt`, `F15-s1-projects-light.png` through `F15-s5-coordinator-dark.png`, `F15-connection-states.md`.

**F16 Top bar counts, search and usage (M7-16 rows 3 and 4; 760 layout in N03; NVDA or Narrator in N05).**
- Steps: Disposable project with queued and running tasks and an approval. Admit, start and complete one harmless task; create and resolve one test approval; counts update to match the lists. Search one project, task, session and approval: results are grouped by type, the loaded-records-only note is shown, ArrowDown and Enter open the selected record. Usage pill with no reported usage says "not available", never `$0` or `$26.00`; the info disclosure opens and closes with keyboard and Escape.
- Evidence: `F16-live-counts-search.png`, `F16-count-events.json` (sanitized), `F16-usage.png`.

**F17 Device resources flow at 125 and 150 percent (M7-17 row 2).**
- Steps: At 125 and then 150 percent display scale, with heights 900 and widths 1280, 1150, 960, 760, 480: resource strip in normal flow after main; no overlap; expand Device resources; scroll to the end; open and close the composer and a dialog. Live metric values are checked in N11.
- Evidence: `F17-dpi125-<width>.png`, `F17-dpi150-<width>.png` (record measured CSS width in notes).

**F18 Background engine and packaged renderer (M7-18 rows 2, 3 and 4).**
- Steps: In the installed app (N01): (1) Background is Off on a fresh profile; turn it On, relaunch: still On; turn it Off, relaunch: still Off. (2) Check `navigator.gpu`. If an adapter is available, opt in and confirm `data-engine="webgpu"` and a rendered first frame. Otherwise record `WebGPU NOT RUN` (not PASS). (3) Before opting in, run `Object.defineProperty(window.navigator, "gpu", { configurable: true, value: undefined })` in DevTools, opt in, confirm `data-engine="canvas2d"`. (4) Check the console during both paths and theme toggles: no errors, no CSP violations.
- Evidence: `F18-package.txt`, `F18-settings.txt`, `F18-renderer-console.txt`.

**F20 Capacity and dispatch panel (M7-20 row 2).**
- Steps: Real daemon and real agents. Occupy both default worker slots with harmless bounded tasks. Queue at least three more including one dependency-blocked task. The panel shows the daemon's slot limit and usage, capacity and dependency reasons from the queue, does not label live data as PLACEHOLDER, and keeps "Pause new dispatch" disabled. Drop the daemon connection and restore it; the panel replaces stale data with the new snapshot.
- Evidence: `F20-capacity.png`, `F20-capacity.txt` (task IDs, observed values).

**F22 Usage popover with a real session (M7-22 row 2).**
- Steps: A real agent session records usage. Open the top-bar usage popover. Reported amount is labelled reported; estimate is labelled separately or unknown and is never added to the reported amount; local-day window, timezone, currency and provider coverage are visible; a provider with no cost data is shown as unreported or unknown, never `$0`.
- Evidence: `F22-popover.png`, `F22-run.txt`.

**F23 Migrated data (M7-23 rows 2–4).**
- Steps: Needs a pre-migration source data directory (provided by the owner; do not use an active user data directory). If not provided, `BLOCKED`. If provided: copy it to a fresh folder under the run root and point `AGENT_ORCH_DATA_DIR` at the copy; the daemon applies its migrations; record that migration `0030_workstreams.sql` succeeded; original project and task IDs remain. In the Workstreams tree, daemon workstreams are listed (not one group per project). Create a disposable workstream, add and revise an objective, set a supported gate, link and unlink a task; restart the app: changes persist; disabled-action reasons are shown. Accessibility: N05. DPI 100, 125, 150, 200: N03.
- Evidence: `F23-migration-log.txt`, `F23-workstreams.png`, `F23-persist.txt`.

**F24 Global and workstream conversations (M7-24 rows 1–7 on Windows; row 8 is this row).**
- Steps: (1) Global with a launchable agent: the recorded user message precedes and correlates with the daemon reply; no placeholder content. (2) With no launchable agent: send a Global message; it is recorded, `runtime_unavailable` is shown and no reply is fabricated. (3) Workstream conversation: target, ordered transcript and reply are correlated; cancel a running turn; final status shown. (4) Restart the daemon and reconnect: transcript remains ordered; send another message; no duplicate entry. (5) Model selector: choices come from the daemon catalog; the effective choice is validated; non-launchable providers are disabled; with no launchable provider no model or effort is selected. (6) Keyboard and screen reader: N04 and N05. (7) Display scaling: N03.
- Evidence: `F24-global.png`, `F24-workstream.png`, `F24-results.txt`.

**F25 Runtime, authority and live replies (M7-25 rows 1–5).**
- Steps: (1) `cargo test --locked -p orchestrator-daemon -- --test-threads=1` (repo root, same `CARGO_TARGET_DIR`); the integration test proves ordered and correlated Global and workstream replies, persistence, replay and idempotency, and unavailable-runtime failure. Pass counts recorded. (2) Live replies (Codex 0.167.0): send a unique nonce-bearing prompt to Global and to an existing non-fixture workstream; wait for each turn to complete; `conversations.get` for each shows a reply containing the nonce, strictly increasing sequence numbers, every entry correlated to its turn ID, and no history shared between targets. (3) Recovery: record both transcripts' entries and cursors; stop only this run's daemon; relaunch with the same data directory; read from the prior cursors and from sequence 0. Completed replies keep their IDs and sequences; the cursor at `latest_sequence` returns no entries. (4) Runtime unavailable: `cargo test --locked -p orchestrator-daemon --test conversation_runtime -- --exact without_an_installed_agent_the_shipped_runtime_fails_closed_as_unavailable` passes. (The macOS-only `a_runtime_that_is_present_but_not_executable_fails_closed` is not run on Windows.) (5) Authority: `cargo test --locked -p orchestrator-daemon --test api -- --exact conversation_text_and_replies_create_no_task_approval_or_delivery` (use the test target that contains that name; record which target) passes; compare task, approval and delivery lists before and after the live exchanges; they must not change.
- Evidence: `F25-daemon-tests.log`, `F25-live-global.json`, `F25-live-workstream.json`, `F25-recovery.txt`, `F25-authority.txt`.

**F26 Usage summary with a real Codex task (M7-26 row 2).**
- Steps: `cargo test --locked -p orchestrator-store -p orchestrator-daemon -- --test-threads=1` (log and exit code). Run a real Codex task that reports token counts but no cost. Inspect `usage.summary`: reported costs unchanged; estimates labelled estimated with table version and source; unknown or unpriced data reported as unknown, never `$0`. Capture the summary, the UI and the test output.
- Evidence: `F26-tests.log`, `F26-usage-summary.json` (sanitized), `F26-ui.png`.

### 4.3 M6-04 Windows rows that need signed or released artifacts

- **M6-04 W10 signed baseline (BLOCKED-EXTERNAL for the signature part).** Record the Windows build, WebView2 version and the installer SHA-256 (`Get-FileHash -Algorithm SHA256`). Run `Get-AuthenticodeSignature` and `signtool verify /pa /all`; the expected Valid signature needs signing material this run does not have. Evidence: `M6-04W10-baseline.txt`.
- **M6-04 W13 N-1 to N upgrade, idle and running (BLOCKED-EXTERNAL).** Needs two released artifacts with different versions. Evidence: none; status line only.
- **M6-04 W14 signed installer smoke (BLOCKED-EXTERNAL).** Needs signed N-1 and N artifacts and `scripts/release/install-smoke.mjs windows --require-signed`. Evidence: none; status line only.
- M6-04 W11 (N-1 install) and W15 (uninstall) are covered by N01. M6-04 W12 (lifecycle and approvals) is covered by N09 and N10.

## 5. Results and publication

### 5.1 RESULTS.md

Write `$Out\RESULTS.md` (the `evidence-out` folder). One line per row, in this exact form (fields separated by ` | `):

```
row id | PASS/FAIL/BLOCKED/BLOCKED-EXTERNAL | note (one line, no usernames, host names, paths or secrets) | evidence file names under evidence-out
```

Include a line for every gate G01–G08, every preflight row P01–P20, every row N01–N12, F08–F26, and M6-04 W10, W13 and W14. A row that was not run is `BLOCKED` with the missing prerequisite. Keep the pass counts from G02, G05 and G06 in the notes. End the file with the output of `git -C $Src rev-parse HEAD` and `git -C $Src status --porcelain=v1 --untracked-files=all` so the reviewer can confirm the checkout stayed clean. Do not edit RESULTS.md after the privacy scan in §5.2.

### 5.2 Privacy scan and redaction

Redaction rules:
- JSON doubles Windows backslashes. Check every escaped form of a path (`C:\Users\name`, `C:\\Users\\name`, `C:/Users/name`), UNC forms and 8.3 short forms before publishing.
- Do not replace usernames or host names as unrestricted substrings: short names such as `main` corrupt Git evidence. Replace whole known values only, with `<USER_PROFILE>` or `<HOST>`, and only in publication copies.
- Do not publish the daemon data directory, the Git credential helper output, GitHub tokens, or any file that fails the scan.

```powershell
$Needles = @($env:USERNAME, $env:COMPUTERNAME, $env:USERPROFILE,
  ($env:USERPROFILE -replace '\\', '\\'), ($env:USERPROFILE -replace '\\', '/'),
  (New-Object -ComObject Scripting.FileSystemObject).GetFolder($env:USERPROFILE).ShortPath) | Where-Object { $_ }
$Secret = '(?i)-----BEGIN [A-Z ]*PRIVATE KEY-----|(authorization|bearer|token|api[_-]?key|password|secret)\s*[:=]\s*\S+'
$Scanned = 0
$Hits = @()
foreach ($File in Get-ChildItem -LiteralPath $Out -File -Recurse | Where-Object { $_.Extension -in '.txt', '.md', '.json', '.csv', '.log' }) {
  $Scanned++
  $Text = [IO.File]::ReadAllText($File.FullName)
  if ($Text -match $Secret -or ($Needles | Where-Object { $Text.IndexOf($_, [StringComparison]::OrdinalIgnoreCase) -ge 0 })) { $Hits += $File.Name }
}
"files_scanned=$Scanned matches=$($Hits.Count)" | Set-Content -LiteralPath (Join-Path $Out 'P99-privacy-scan.txt')
if ($Hits.Count -ne 0) { throw "BLOCKED: privacy scan matched $($Hits -join ', '); sanitize only the publication copies, rerun, or do not publish." }
```

Screenshots: open each PNG and confirm it shows no personal data, credentials, real project names or real task content. Crop or remove any image that does. If a screenshot cannot be reviewed, do not publish it.

### 5.3 Publish the evidence ref

Publish only `evidence-out/` onto a new ref. Use a detached worktree of the pinned commit, add one commit whose parent is the pinned commit, and push it with the `j-mckerracher` author and committer. Never force-push. If the push is denied, mark the publication row `BLOCKED-EXTERNAL` and report the local `evidence-out` path. Do not edit `board.json`. Do not change the `origin` remote.

```powershell
$Utc = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ')
$Ref = "evidence/m7-32-win-92f56c3af-$Utc"
$Pub = Join-Path $RunRoot 'publish'
if ((Invoke-Gate 'P98-remote-ref-check' $Src 'git' @('ls-remote', '--heads', 'origin', "refs/heads/$Ref")) -ne 0) { throw 'BLOCKED: remote ref check failed.' }
if (((Get-Content -LiteralPath (Join-Path $Out 'P98-remote-ref-check.stdout.txt')) -join '').Trim()) { throw 'BLOCKED: ref already exists; start a new run.' }
if ((Invoke-Gate 'P98-worktree' $Src 'git' @('worktree', 'add', '--detach', $Pub, $SourceSha)) -ne 0) { throw 'BLOCKED: could not create the publish worktree.' }
$PubEvidence = Join-Path $Pub 'evidence-out'
New-Item -ItemType Directory -Path $PubEvidence | Out-Null
Copy-Item -Path (Join-Path $Out '*') -Destination $PubEvidence -Recurse
Invoke-Gate 'P98-stage' $Pub 'git' @('add', '--', 'evidence-out') | Out-Null
Invoke-Gate 'P98-staged' $Pub 'git' @('diff', '--cached', '--name-only') | Out-Null
$Staged = @(Get-Content -LiteralPath (Join-Path $Out 'P98-staged.stdout.txt') | Where-Object { $_ })
if ($Staged.Count -eq 0 -or ($Staged | Where-Object { $_ -notlike 'evidence-out/*' })) { throw 'BLOCKED: staged paths are empty or outside evidence-out.' }
$env:GIT_AUTHOR_NAME = 'j-mckerracher'
$env:GIT_AUTHOR_EMAIL = '135927658+j-mckerracher@users.noreply.github.com'
$env:GIT_COMMITTER_NAME = $env:GIT_AUTHOR_NAME
$env:GIT_COMMITTER_EMAIL = $env:GIT_AUTHOR_EMAIL
if ((Invoke-Gate 'P98-commit' $Pub 'git' @('commit', '-m', 'Windows native evidence for M7-32 at 92f56c3a', '-m', 'Co-authored-by: Copilot App <223556219+Copilot@users.noreply.github.com>')) -ne 0) { throw 'BLOCKED: evidence commit failed.' }
if ((Invoke-Gate 'P98-push' $Pub 'git' @('push', 'origin', "HEAD:refs/heads/$Ref")) -ne 0) { throw 'BLOCKED-EXTERNAL: evidence push denied; keep the commit and report the path.' }
Invoke-Gate 'P98-verify' $Pub 'git' @('ls-remote', 'origin', "refs/heads/$Ref") | Out-Null
Invoke-Gate 'P98-parent' $Pub 'git' @('rev-parse', 'HEAD~1') | Out-Null
```

Report back: the evidence ref, the evidence commit SHA (`git rev-parse HEAD` in the publish worktree), the parent SHA (must be `92f56c3af9cd34741a09667f499164edcafec696`), the privacy scan count, and the output of `P98-verify`.

## 6. Cleanup

After the evidence ref is published, remove only what this run created: `$Pub` (`git -C $Src worktree remove $Pub`), `$env:CARGO_TARGET_DIR`, and `$Src\apps\desktop\node_modules`. Keep `$Out` and `$RunRoot\daemon-data` until the owner confirms the evidence. Do not delete any other folder.

## 7. Not run on Windows

- macOS-only rows: VoiceOver, WKWebView, Cmd+Q, Retina scaling, `codesign`, `spctl`, `xcrun stapler`, `npm run package:mac`, and the macOS install-smoke rows of M6-04.
- M7-02 macOS runbook: not part of this run.
- M6-04 signed rows: BLOCKED-EXTERNAL (§4.3).
- macOS VoiceOver rows in M7-10, M7-14, M7-16 and M7-23: not run here; the Windows Narrator and NVDA rows cover the Windows side.
