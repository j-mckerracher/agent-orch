# Windows preliminary baseline for M7 P12 integration head 53c0272

Run a preliminary baseline diagnostic against source commit `53c0272b3e5df60f48dba90aad1c7a14cf3a19a9`, tree `1150197734de6ffdc8a672f2422649fe3dca6eda`. This is not P12 acceptance. Final candidates from the in-flight M7-00..03 closures need a fresh complete rerun. Required macOS checks remain separate.

The prior 3a0c976 attempt stopped at preflight: Node 22.20.0 did not satisfy `^22.22.2 || ^24.15.0 || >=26.0.0`; free space was 40.17 GiB, `where cl.exe` exited 1, and vswhere found Visual Studio. The earlier 6c17442 native capture failed with `GetCursorPos failed: Access is denied`. Start a new isolated run and do not carry results over.

## Create an isolated run

Use Windows PowerShell 5.1 or PowerShell 7. Set `$RunBase` to an existing short local path without whitespace or `cmd.exe` metacharacters, such as `C:\t`. Use a local volume with at least 20 GiB free. Do not use a mapped drive, `SUBST` drive, junction, existing checkout, or shared evidence directory. Do not delete files to make room.

```powershell
$ErrorActionPreference = 'Stop'
$SourceSha = '53c0272b3e5df60f48dba90aad1c7a14cf3a19a9'
$ExpectedTree = '1150197734de6ffdc8a672f2422649fe3dca6eda'
$RunBase = 'C:\t'
if ($RunBase -match '[&|<>^()%!"\s]') {
  throw 'Choose a local path without whitespace or cmd.exe metacharacters.'
}
if (!(Test-Path -LiteralPath $RunBase -PathType Container)) { throw 'Choose an existing short local run directory.' }
$RunId = [guid]::NewGuid().ToString('N')
$RunRoot = Join-Path $RunBase "m7p12-$RunId"
$Source = Join-Path $RunRoot 'src'
$Desktop = Join-Path $Source 'apps\desktop'
$Evidence = Join-Path $RunRoot 'evidence'
$PrivateData = Join-Path $RunRoot 'private-daemon-data'
$PublishInput = Join-Path $RunRoot 'publish-input'
$Screens = Join-Path $Evidence 'screens'
if (Test-Path -LiteralPath $RunRoot) { throw 'Run path already exists. Keep it and start a new run with a new ID.' }
New-Item -ItemType Directory -Path $RunRoot | Out-Null
New-Item -ItemType Directory -Path $Evidence | Out-Null
```

Record free space before work. Stop if the run volume has less than 20 GiB free.

```powershell
$DriveName = ([IO.Path]::GetPathRoot($RunBase)).Substring(0, 1)
$Drive = Get-PSDrive -Name $DriveName
$FreeGiB = [math]::Round($Drive.Free / 1GB, 2)
"utc=$([DateTime]::UtcNow.ToString('o'))`r`nvolume=$DriveName`r`nfree_gib=$FreeGiB" |
  Set-Content -LiteralPath (Join-Path $Evidence 'free-space-start.txt')
if ($Drive.Free -lt 20GB) { throw 'Less than 20 GiB free. Preserve this preflight and stop.' }
```

Use this helper for every native command. It records the command, working directory, unmodified stdout and stderr files, and the process exit code. It does not pipe output through PowerShell.

```powershell
function Invoke-Captured {
  param([string]$Name, [string]$CommandLine, [string]$WorkingDirectory)
  $Prefix = Join-Path $Evidence $Name
  foreach ($Suffix in 'command.txt', 'stdout.txt', 'stderr.txt', 'exit.txt') {
    if (Test-Path -LiteralPath "$Prefix.$Suffix") { throw "Evidence already exists for $Name. Use a new name." }
  }
  [IO.File]::WriteAllText("$Prefix.command.txt", "cwd=$WorkingDirectory`r`ncommand=$CommandLine`r`n", [Text.Encoding]::UTF8)
  $Process = Start-Process -FilePath $env:ComSpec -ArgumentList @('/d', '/s', '/c', $CommandLine) `
    -WorkingDirectory $WorkingDirectory -RedirectStandardOutput "$Prefix.stdout.txt" `
    -RedirectStandardError "$Prefix.stderr.txt" -Wait -PassThru -NoNewWindow
  [IO.File]::WriteAllText("$Prefix.exit.txt", "$($Process.ExitCode)`r`n", [Text.Encoding]::ASCII)
  return $Process.ExitCode
}

function Assert-SourceWorktree {
  param([string]$Name, [string]$WorkingDirectory)
  $RootExit = Invoke-Captured "$Name-root" 'git rev-parse --show-toplevel' $WorkingDirectory
  $HeadExit = Invoke-Captured "$Name-head" 'git rev-parse HEAD' $WorkingDirectory
  if ($RootExit -ne 0 -or $HeadExit -ne 0) { throw 'Could not verify the source worktree.' }
  $ActualRoot = [IO.Path]::GetFullPath(([string](Get-Content -Raw (Join-Path $Evidence "$Name-root.stdout.txt"))).Trim())
  $ExpectedRoot = [IO.Path]::GetFullPath($Source)
  if ($ActualRoot -ine $ExpectedRoot) {
    throw 'Command root is outside the isolated source checkout.'
  }
  if (([string](Get-Content -Raw (Join-Path $Evidence "$Name-head.stdout.txt"))).Trim() -cne $SourceSha) {
    throw 'Source HEAD changed. Stop and preserve this run.'
  }
}
```

Clear inherited `FM_*` values only in this PowerShell process. The repository has no declared `FM_*` variables. Record names, never values. Set the daemon data directory in this process too. Do not use `setx` or change machine or user settings.

```powershell
$FMNames = @(Get-ChildItem Env: | Where-Object { $_.Name -like 'FM_*' } | Select-Object -ExpandProperty Name)
[IO.File]::WriteAllLines((Join-Path $Evidence 'fm-variable-names.txt'), [string[]]$FMNames)
foreach ($Name in $FMNames) { Remove-Item -LiteralPath "Env:$Name" }
$env:AGENT_ORCH_DATA_DIR = $PrivateData
```

Clone without checkout, set only this clone's Git options, then fetch the exact candidate. Do not change the remote or global Git configuration.

```powershell
$CloneExit = Invoke-Captured 'clone' "git -c core.autocrlf=false -c core.longpaths=true clone --no-checkout https://github.com/j-mckerracher/agent-orch.git $Source" $RunRoot
if ($CloneExit -ne 0) { throw 'Clone failed. Preserve output and stop.' }
$AutoCrlfExit = Invoke-Captured 'git-autocrlf' 'git config --local core.autocrlf false' $Source
$LongPathsExit = Invoke-Captured 'git-longpaths' 'git config --local core.longpaths true' $Source
if ($AutoCrlfExit -ne 0 -or $LongPathsExit -ne 0) { throw 'Could not set isolated checkout options.' }
$FetchExit = Invoke-Captured 'fetch-source' "git fetch origin $SourceSha" $Source
if ($FetchExit -ne 0) { throw 'Exact source commit is unavailable. Preserve output and stop.' }
$CheckoutExit = Invoke-Captured 'checkout-source' "git checkout --detach $SourceSha" $Source
if ($CheckoutExit -ne 0) { throw 'Exact source checkout failed. Preserve output and stop.' }
$HeadExit = Invoke-Captured 'source-head' 'git rev-parse HEAD' $Source
$ObjectExit = Invoke-Captured 'source-object' 'git cat-file -p HEAD' $Source
if ($HeadExit -ne 0 -or $ObjectExit -ne 0) { throw 'Could not verify source commit.' }
$Head = ([string](Get-Content -Raw (Join-Path $Evidence 'source-head.stdout.txt'))).Trim()
$Object = Get-Content -Raw (Join-Path $Evidence 'source-object.stdout.txt')
$Tree = [regex]::Match($Object, '(?m)^tree ([0-9a-f]{40})\r?$').Groups[1].Value
if ($Head -cne $SourceSha -or $Tree -cne $ExpectedTree) { throw 'Source SHA or tree differs. Stop.' }
$StatusExit = Invoke-Captured 'source-status-before' 'git status --porcelain=v1 --untracked-files=all' $Source
if ($StatusExit -ne 0 -or ([string](Get-Content -Raw (Join-Path $Evidence 'source-status-before.stdout.txt'))).Trim()) {
  throw 'Checkout is not clean. Preserve it and stop.'
}
```

## Check tools and runtime

Record Windows build, PowerShell version, and the WebView2 Runtime version shown in Settings > Apps > Installed apps. Save that version in `webview2.txt`. Record Git, Node, npm, Rust, Cargo, and `cl.exe` too. Use `where.exe cl.exe` as a PATH check only. If it is absent, inspect Visual Studio with `vswhere.exe` and use its `VsDevCmd.bat` or Developer PowerShell. Do not start an installer based only on a PATH check. If WebView2, Rust, Cargo, or the required MSVC tools are unavailable, preserve the evidence and report native launch as not run.

```powershell
(Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber, OSArchitecture | Format-List | Out-String) |
  Set-Content -LiteralPath (Join-Path $Evidence 'windows.txt')
$PSVersionTable.PSVersion.ToString() | Set-Content -LiteralPath (Join-Path $Evidence 'powershell.txt')
$WebView2Version = Read-Host 'WebView2 Runtime version from Settings > Apps > Installed apps'
$WebView2Version | Set-Content -LiteralPath (Join-Path $Evidence 'webview2.txt')
Invoke-Captured 'git-version' 'git --version' $RunRoot
Invoke-Captured 'node-version' 'node.exe --version' $RunRoot
Invoke-Captured 'node-version-plain' 'node.exe -p process.versions.node' $RunRoot
Invoke-Captured 'rustc-version' 'rustc.exe -V' $RunRoot
Invoke-Captured 'rustc-host' 'rustc.exe -vV' $RunRoot
Invoke-Captured 'cargo-version' 'cargo.exe -V' $RunRoot
Invoke-Captured 'msvc-path' 'where.exe cl.exe' $RunRoot
```

The lockfile requires Node `^22.22.2 || ^24.15.0 || >=26.0.0`. Check before npm. If unsupported or absent, use the official Node 24 LTS x64 zip in this run only. Do not use installers, change machine or user PATH, or use `npx`. A download or checksum failure ends preflight; preserve the failure and report all gates not run. The checksum comes from nodejs.org over HTTPS; record its URL and both hashes.

```powershell
function Test-NodeSupported([string]$Text) {
  $Version = $null
  if (-not [version]::TryParse($Text, [ref]$Version)) { return $false }
  return (($Version.Major -eq 22 -and $Version -ge [version]'22.22.2') -or `
    ($Version.Major -eq 24 -and $Version -ge [version]'24.15.0') -or $Version.Major -ge 26)
}
$NodeText = ([string](Get-Content -Raw (Join-Path $Evidence 'node-version-plain.stdout.txt'))).Trim()
if (-not (Test-NodeSupported $NodeText)) {
  try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $IndexUrl = 'https://nodejs.org/dist/index.json'
    $Index = Invoke-RestMethod -Uri $IndexUrl
    $Release = @($Index | Where-Object { $_.version -match '^v24\.' -and $_.lts -and $_.files -contains 'win-x64-zip' } | Select-Object -First 1)
    if ($Release.Count -ne 1 -or $Release[0].version -notmatch '^v24\.\d+\.\d+$') { throw 'No Node 24 LTS zip found.' }
    $NodeRelease = $Release[0].version
    $ZipName = "node-$NodeRelease-win-x64.zip"
    $ZipUrl = "https://nodejs.org/dist/$NodeRelease/$ZipName"
    $SumsUrl = "https://nodejs.org/dist/$NodeRelease/SHASUMS256.txt"
    $Zip = Join-Path $RunRoot $ZipName
    $Sums = Join-Path $RunRoot 'SHASUMS256.txt'
    Invoke-WebRequest -Uri $ZipUrl -OutFile $Zip -UseBasicParsing
    Invoke-WebRequest -Uri $SumsUrl -OutFile $Sums -UseBasicParsing
    $Matches = @(Get-Content -LiteralPath $Sums | Where-Object { $_ -match "^([0-9a-fA-F]{64})  $([regex]::Escape($ZipName))$" })
    if ($Matches.Count -ne 1) { throw 'Checksum entry missing or ambiguous.' }
    $ExpectedHash = ($Matches[0] -split '\s+')[0].ToLowerInvariant()
    $ActualHash = (Get-FileHash -LiteralPath $Zip -Algorithm SHA256).Hash.ToLowerInvariant()
    "index=$IndexUrl`r`nzip=$ZipUrl`r`nsums=$SumsUrl`r`nexpected_sha256=$ExpectedHash`r`nactual_sha256=$ActualHash" | Set-Content -LiteralPath (Join-Path $Evidence 'node-download.txt')
    if ($ActualHash -cne $ExpectedHash) { throw 'Node zip checksum mismatch.' }
    Expand-Archive -LiteralPath $Zip -DestinationPath $RunRoot
    $env:PATH = (Join-Path $RunRoot "node-$NodeRelease-win-x64") + ';' + $env:PATH
    $NodeExit = Invoke-Captured 'node-remedy-version' 'node.exe -p process.versions.node' $RunRoot
    $NodeText = ([string](Get-Content -Raw (Join-Path $Evidence 'node-remedy-version.stdout.txt'))).Trim()
    if ($NodeExit -ne 0 -or -not (Test-NodeSupported $NodeText)) { throw 'Downloaded Node failed the version gate.' }
  } catch {
    $_.ToString() | Set-Content -LiteralPath (Join-Path $Evidence 'node-remedy-error.txt')
    throw 'Node remedy failed. Preserve preflight and stop.'
  }
}
Invoke-Captured 'npm-version' 'npm.cmd --version' $RunRoot
$env:CARGO_TARGET_DIR = Join-Path $RunRoot 'cargo-target'
```

If preflight throws after the exact checkout is verified, stop running gates, mark them NOT RUN, write the report and perform the final status and evidence publication sections. If the checkout cannot be verified, preserve local evidence and report the blocker; do not create a source-parent evidence commit from an unverified checkout.

Check the source root before every gate. Capture each full command and exit code; never use a filter pipeline as the command status. Preserve failures and continue independent checks. Run `npm ci` only in this fresh checkout with no existing `node_modules`.

## Run frontend, CI script, and native workspace checks

```powershell
if (Test-Path -LiteralPath (Join-Path $Desktop 'node_modules')) { throw 'node_modules already exists; preserve it and stop.' }
Assert-SourceWorktree 'before-npm-ci' $Desktop
$CiExit = Invoke-Captured 'npm-ci' 'npm.cmd ci' $Desktop
if ($CiExit -eq 0) {
  Assert-SourceWorktree 'before-npm-test' $Desktop
  $TestExit = Invoke-Captured 'npm-test' 'npm.cmd test' $Desktop
  Assert-SourceWorktree 'before-npm-build' $Desktop
  $BuildExit = Invoke-Captured 'npm-build' 'npm.cmd run build' $Desktop
} else { 'npm test and build NOT RUN: npm ci failed' | Set-Content -LiteralPath (Join-Path $Evidence 'frontend-not-run.txt') }
Assert-SourceWorktree 'before-node-ci' $Source
$NodeCiExit = Invoke-Captured 'node-ci' 'node --test scripts/ci' $Source
Assert-SourceWorktree 'before-cargo-test' $Source
$CargoTestExit = Invoke-Captured 'cargo-test-workspace' 'cargo test --workspace' $Source
```

This is the first native compilation of the new daemon code. On Cargo failure preserve the full output and quote the first compiler error block in the report, along with the actual exit code. Continue independent source and UI checks where possible. Record test counts and failures; do not replace full suites with selected tests.

Create a screenshot directory before opening the app. If that path exists, preserve it and start a fresh run.

```powershell
$Screens = Join-Path $Evidence 'screens'
if (Test-Path -LiteralPath $Screens) { throw 'Screens directory already exists. Preserve it and use a new run.' }
New-Item -ItemType Directory -Path $Screens | Out-Null
```


## Build and launch the native app

The repository README documents `npm run tauri dev` from `apps/desktop`. The debug host reads `AGENT_ORCH_DAEMON_BIN`, starts the daemon on first connection, and passes its data directory to the daemon. Do not start the daemon manually or invent flags.

The workspace test above may download dependencies. For the native launch build below, keep Cargo offline until a command reports a missing cached crate, then record and fetch the missing dependencies.

```powershell
$env:CARGO_NET_OFFLINE = 'true'
Assert-SourceWorktree 'before-daemon-build' $Source
$DaemonBuildExit = Invoke-Captured 'daemon-build-first' 'cargo build --locked -p orchestrator-daemon' $Source
$DaemonText = ([string](Get-Content -Raw (Join-Path $Evidence 'daemon-build-first.stdout.txt'))) +
  ([string](Get-Content -Raw (Join-Path $Evidence 'daemon-build-first.stderr.txt')))
if ($DaemonBuildExit -ne 0 -and $DaemonText -match '(?i)no matching package|failed to download|offline mode') {
  $env:CARGO_NET_OFFLINE = 'false'
  Assert-SourceWorktree 'before-daemon-fetch' $Source
  $FetchExit = Invoke-Captured 'daemon-fetch-after-missing-crate' 'cargo fetch --locked' $Source
  $env:CARGO_NET_OFFLINE = 'true'
  if ($FetchExit -eq 0) {
    Assert-SourceWorktree 'before-daemon-build-retry' $Source
    $DaemonBuildExit = Invoke-Captured 'daemon-build-retry' 'cargo build --locked -p orchestrator-daemon' $Source
  }
}
if ($DaemonBuildExit -ne 0) { throw 'Daemon build failed. Skip the rest of this block, mark native launch NOT RUN, and continue with source contracts, final status, report and evidence.' }
$env:AGENT_ORCH_DAEMON_BIN = Join-Path $env:CARGO_TARGET_DIR 'debug\orchestrator-daemon.exe'
if (!(Test-Path -LiteralPath $env:AGENT_ORCH_DAEMON_BIN)) { throw 'Daemon output missing. Skip the rest of this block, mark launch NOT RUN, and continue with source contracts, final status, report and evidence.' }
$env:AGENT_ORCH_DATA_DIR = $PrivateData
Assert-SourceWorktree 'before-tauri-dev' $Desktop
$TauriExit = Invoke-Captured 'tauri-dev-first' 'npm.cmd run tauri dev' $Desktop
$TauriText = ([string](Get-Content -Raw (Join-Path $Evidence 'tauri-dev-first.stdout.txt'))) +
  ([string](Get-Content -Raw (Join-Path $Evidence 'tauri-dev-first.stderr.txt')))
if ($TauriExit -ne 0 -and $TauriText -match '(?i)no matching package|failed to download|offline mode') {
  $env:CARGO_NET_OFFLINE = 'false'
  Assert-SourceWorktree 'before-tauri-fetch' $Source
  $TauriFetchExit = Invoke-Captured 'tauri-fetch-after-missing-crate' `
    'cargo fetch --locked --manifest-path apps/desktop/src-tauri/Cargo.toml' $Source
  $env:CARGO_NET_OFFLINE = 'true'
  if ($TauriFetchExit -eq 0) {
    Assert-SourceWorktree 'before-tauri-dev-retry' $Desktop
    $TauriExit = Invoke-Captured 'tauri-dev-retry' 'npm.cmd run tauri dev' $Desktop
  }
}
$RunDaemon = @(Get-CimInstance Win32_Process -Filter "Name='orchestrator-daemon.exe'" |
  Where-Object { $_.CommandLine -like "*$PrivateData*" } |
  Select-Object ProcessId, ExecutablePath, CommandLine)
$RunDaemon | ConvertTo-Json -Depth 3 |
  Set-Content -LiteralPath (Join-Path $Evidence 'run-daemon-after.txt')
```

Interact with the native window while the command waits. Close the window to end the Tauri dev session. For theme persistence, only after a successful first launch and if native capture did not fail, start a second session using the following separately captured command, then close it. Skip this block after a failed launch, a prior throw, or a capture access denial.

```powershell
if ($TauriExit -eq 0) {
  Assert-SourceWorktree 'before-tauri-reopen' $Desktop
  $TauriReopenExit = Invoke-Captured 'tauri-dev-reopen' 'npm.cmd run tauri dev' $Desktop
}
```

 The daemon is detached and may remain running. Record its process and data path; do not kill unrelated processes or remove the private data directory.

## Check the UI and source contracts

Record each check as `PASS`, `FAIL`, `UNVERIFIED`, or `NOT RUN`. Do not create tasks, approvals, agent sessions, or fixture mutations. Register only the isolated source checkout if S1 needs a project. Keep daemon data outside published evidence.


1. Check the theme toggle in light and dark modes. Close and reopen the app using the separately captured launch above to check the explicit choice persists. Record the initial appearance and restore it before ending the run. Check system-default behavior only when WebView2 has no saved `agent-orch.theme` choice. The isolated daemon data directory does not reset WebView2 storage. Do not clear a user profile or change Windows-wide appearance settings. Mark system-default behavior unverified when an earlier saved choice exists. Storage-failure tests and 4.5:1 text and 3:1 non-text contrast checks are automated.
2. Check Windows font rendering and text truncation. The source stack names Segoe UI and bundles no font. Check visible focus rings and reduced-motion behavior. Mark system-preference behavior unverified if testing it would require changing Windows settings.
3. Check that the native window opens at 1280 by 800. Resize it to 1280, 1150, 960, and 760 pixels wide. Check for horizontal scrolling, visible shell regions, activity-panel stacking, and the collapsed sidebar menu at 760. The configured minimum is 760 by 600 pixels. Record any size the host cannot reach.
4. Visit all global routes: `#/workspace`, `#/workstreams`, `#/agents`, and `#/review`. Check the default workspace, deep links, back and forward navigation, and the persistent ConnectionStrip on every global and legacy route. Native fixture and disconnected boot modes remain unverified unless you can observe them without changing settings.
5. Visit legacy S1 through S6 without changing source data: `#/projects`, `#/project/<project-id>`, `#/project/<project-id>/tasks/all`, `#/project/<project-id>/task/<real-task-id>`, `#/project/<project-id>/coordinator`, and `#/project/<project-id>/review/<real-task-id>`. Use IDs returned by the isolated app. Do not invent IDs or create records for empty views. Mark routes that need unavailable real data unverified.
6. Use Tab and Enter to reach the skip link and primary navigation. Check visible focus, selected-link `aria-current`, one `main` landmark, named `Primary views` and `Workstreams` navigation landmarks, and the `Workspace activity` complementary landmark. Use the native accessibility tree or screen reader when available. Preserve errors and mark unavailable checks unverified.
7. Check source contracts in `apps/desktop/src/ui/theme/theme.ts`, `apps/desktop/src/ui/theme/theme.test.ts`, `apps/desktop/src/ui/theme/ThemeToggle.test.tsx`, `apps/desktop/src/ui/theme/contrast.test.ts`, `apps/desktop/src/ui/theme/tokens.css`, `apps/desktop/index.html`, `apps/desktop/public/theme-init.js`, `apps/desktop/src/ui/router.ts`, `apps/desktop/src/ui/router.test.ts`, `apps/desktop/src/ui/shell/SidebarNav.tsx`, `apps/desktop/src/ui/shell/AppShell.test.tsx`, `apps/desktop/src/ui/shell/shell.css`, and `apps/desktop/src-tauri/tauri.conf.json`. Confirm the four global routes, legacy formats, theme-before-paint script, font stack and no `@font-face`, landmarks, focus behavior, 1150/960/760 breakpoints, and 1280 by 800 default and 760 by 600 minimum window sizes. Source inspection is not native UI proof.
8. Check that live views show unavailable or unknown values honestly and do not fall back to fixture values. Record views that cannot connect to the isolated daemon.

Save screenshots only when the authorized native capture mechanism works. Keep the complete error if capture fails, including `GetCursorPos failed: Access is denied. (0x80070005)` when returned. Do not elevate, change security settings, substitute a browser screenshot, or infer acceptance. Mark native capture NOT RUN and affected checks unverified; do not retry with elevated rights.

Check the source status after all commands. Preserve unexpected changes and do not clean the checkout.

```powershell
Assert-SourceWorktree 'after-run' $Source
$StatusAfterExit = Invoke-Captured 'source-status-after' 'git status --porcelain=v1 --untracked-files=all' $Source
if ($StatusAfterExit -ne 0) { throw 'Could not capture final source status.' }
$StatusAfter = ([string](Get-Content -Raw (Join-Path $Evidence 'source-status-after.stdout.txt'))).Trim()
if ($StatusAfter) { 'Checkout changed; preserve it and report paths. Do not clean it.' | Set-Content -LiteralPath (Join-Path $Evidence 'source-changed.txt') }
```


Write `$Evidence\REPORT.md` with source SHA and tree, Windows build, tool versions, every command and exit code, test count and failures, build result, manual check status, capture result, source status before and after, and limitations. Label it preliminary diagnostics, not priority-12 acceptance.

## Publish evidence on a fresh ref

Keep raw evidence unchanged in `$Evidence`. Copy only the report, captured command output, and reviewed screenshots into `$PublishInput`. Exclude the checkout, `target`, `node_modules`, `private-daemon-data`, credentials, and unrelated files.

Scan text evidence for secrets and personal data before publishing. Manually inspect every publication copy for local paths (including JSON-escaped, UNC, short and scp-style forms), usernames, hostnames, credentials and notifications; refuse publication if any remain. Redact structured values before serialization, not by replacing unrestricted username or hostname substrings. Add the operator's work or school email domain when prompted. Print only a count, never matching lines or values. Inspect text captures and screenshots for credentials, local paths, notifications, and personal data. If a scan finds a match, use `$Hits` to identify filenames without printing contents, sanitize only publication copies, and rerun the scan from `$PrivateNeedles` onward. Keep raw evidence unchanged.

```powershell
if (Test-Path -LiteralPath $PublishInput) { throw 'Publish input already exists. Preserve it and use a fresh run.' }
New-Item -ItemType Directory -Path $PublishInput | Out-Null
Copy-Item -LiteralPath (Join-Path $Evidence 'REPORT.md') -Destination $PublishInput
$CaptureFiles = Get-ChildItem -LiteralPath $Evidence -File | Where-Object { $_.Extension -eq '.txt' }
foreach ($File in $CaptureFiles) { Copy-Item -LiteralPath $File.FullName -Destination $PublishInput }
$ScreenshotFiles = @()
if (Test-Path -LiteralPath $Screens) { $ScreenshotFiles = @(Get-ChildItem -LiteralPath $Screens -File) }
if ($ScreenshotFiles.Count -gt 0) {
  $ScreenshotReview = Read-Host 'Have you inspected every screenshot for private content? Type YES'
  if ($ScreenshotReview -cne 'YES') { throw 'Do not publish screenshots before review.' }
  foreach ($File in $ScreenshotFiles) { Copy-Item -LiteralPath $File.FullName -Destination $PublishInput }
}
$EmailDomain = Read-Host 'Work or school email domain to scan for'
$ProfilePath = $env:USERPROFILE
$PrivateNeedles = @($env:USERNAME, $ProfilePath, $ProfilePath.Replace('\', '\\'),
  $ProfilePath.Replace('\', '/'), $EmailDomain) | Where-Object { $_ }
$TextReview = Read-Host 'Have you inspected every publication text file for escaped, UNC, short and scp-style paths and all private data? Type YES'
if ($TextReview -cne 'YES') { throw 'Do not publish text before review.' }
$SecretPattern = '(?i)-----BEGIN .*PRIVATE KEY-----|(?:authorization|bearer|access[_-]?token|refresh[_-]?token|api[_-]?key|password|secret)\s*[:=]\s*\S+'
$TextFiles = Get-ChildItem -LiteralPath $PublishInput -Recurse -File |
  Where-Object { $_.Extension -in '.txt', '.log', '.md', '.json', '.csv' }
$Hits = @()
foreach ($File in $TextFiles) {
  $Text = [IO.File]::ReadAllText($File.FullName)
  $PrivateHit = $false
  foreach ($Needle in $PrivateNeedles) {
    if ($Text.IndexOf($Needle, [StringComparison]::OrdinalIgnoreCase) -ge 0) { $PrivateHit = $true }
  }
  if ($PrivateHit -or $Text -match $SecretPattern) { $Hits += $File.Name }
}
$ScanName = "privacy-scan-$([guid]::NewGuid().ToString('N')).txt"
"files_scanned=$($TextFiles.Count) matches=$($Hits.Count)" |
  Set-Content -LiteralPath (Join-Path $PublishInput $ScanName)
if ($Hits.Count -ne 0) { throw 'Privacy scan found a match. Inspect filenames in $Hits without printing matching content.' }
```

Create a detached evidence worktree at the exact source SHA. Copy only the scanned publication input into a new `evidence-out` directory. The evidence commit must have the source SHA as its parent and contain files only under `evidence-out/`.

```powershell
$Utc = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ')
$Hex = [guid]::NewGuid().ToString('N').Substring(0, 8)
$Ref = "evidence/m7-p12-baseline-53c0272-$Utc-$Hex"
$Publish = Join-Path $RunRoot 'publish'
if (Test-Path -LiteralPath $Publish) { throw 'Publish worktree path exists. Keep it and stop.' }
$WorktreeExit = Invoke-Captured 'evidence-worktree' "git worktree add --detach $Publish $SourceSha" $Source
if ($WorktreeExit -ne 0) { throw 'Could not create evidence worktree. Preserve it and stop.' }
$EvidenceOut = Join-Path $Publish 'evidence-out'
if (Test-Path -LiteralPath $EvidenceOut) { throw 'evidence-out already exists. Do not overwrite it.' }
New-Item -ItemType Directory -Path $EvidenceOut | Out-Null
Copy-Item -Path (Join-Path $PublishInput '*') -Destination $EvidenceOut -Recurse
```

Use `j-mckerracher <135927658+j-mckerracher@users.noreply.github.com>` as author and committer for the evidence commit. Set it only in this PowerShell process. Do not change Git configuration or the `origin` remote. Stop if the target ref already exists or the remote check fails.

```powershell
$RemoteCheck = Invoke-Captured 'remote-ref-check' "git ls-remote --heads origin refs/heads/$Ref" $Publish
if ($RemoteCheck -ne 0) { throw 'Remote ref check failed. Preserve output and stop.' }
if (([string](Get-Content -Raw (Join-Path $Evidence 'remote-ref-check.stdout.txt'))).Trim()) {
  throw 'Evidence ref already exists. Do not overwrite it; start a new run.'
}
$StatusExit = Invoke-Captured 'publish-status-before' 'git status --porcelain=v1 --untracked-files=all' $Publish
if ($StatusExit -ne 0 -or ([string](Get-Content -Raw (Join-Path $Evidence 'publish-status-before.stdout.txt'))).Trim() -cne '?? evidence-out/') {
  throw 'Evidence worktree contains unexpected files. Preserve it and stop.'
}
$AddExit = Invoke-Captured 'publish-stage' 'git add -- evidence-out' $Publish
$StagedExit = Invoke-Captured 'publish-staged-paths' 'git diff --cached --name-only' $Publish
if ($AddExit -ne 0 -or $StagedExit -ne 0) { throw 'Could not stage evidence. Preserve output and stop.' }
$Staged = @(Get-Content (Join-Path $Evidence 'publish-staged-paths.stdout.txt') | Where-Object { $_ })
if ($Staged.Count -eq 0 -or ($Staged | Where-Object { $_ -notlike 'evidence-out/*' })) {
  throw 'Staged paths are empty or outside evidence-out. Stop.'
}
$DiffCheckExit = Invoke-Captured 'publish-diff-check' 'git diff --cached --check' $Publish
if ($DiffCheckExit -ne 0) { throw 'Staged evidence has whitespace errors. Preserve output and stop.' }
$env:GIT_AUTHOR_NAME = 'j-mckerracher'
$env:GIT_AUTHOR_EMAIL = '135927658+j-mckerracher@users.noreply.github.com'
$env:GIT_COMMITTER_NAME = $env:GIT_AUTHOR_NAME
$env:GIT_COMMITTER_EMAIL = $env:GIT_AUTHOR_EMAIL
$CommitExit = Invoke-Captured 'evidence-commit' 'git commit -m "Windows preliminary M7 P12 baseline evidence for 53c0272"' $Publish
if ($CommitExit -ne 0) { throw 'Evidence commit failed. Preserve the staged files and output.' }
$CommitExit = Invoke-Captured 'evidence-commit-sha' 'git rev-parse HEAD' $Publish
$ParentExit = Invoke-Captured 'evidence-commit-parent' 'git rev-parse HEAD~1' $Publish
if ($CommitExit -ne 0 -or $ParentExit -ne 0) { throw 'Could not verify evidence commit metadata.' }
$Commit = ([string](Get-Content -Raw (Join-Path $Evidence 'evidence-commit-sha.stdout.txt'))).Trim()
$Parent = ([string](Get-Content -Raw (Join-Path $Evidence 'evidence-commit-parent.stdout.txt'))).Trim()
$IdentityExit = Invoke-Captured 'evidence-identity' 'git cat-file -p HEAD' $Publish
$Identity = [string](Get-Content -Raw (Join-Path $Evidence 'evidence-identity.stdout.txt'))
if ($IdentityExit -ne 0 -or $Identity -cnotmatch '(?m)^author j-mckerracher <135927658\+j-mckerracher@users\.noreply\.github\.com> \d+ [+-]\d{4}\r?$' -or $Identity -cnotmatch '(?m)^committer j-mckerracher <135927658\+j-mckerracher@users\.noreply\.github\.com> \d+ [+-]\d{4}\r?$') { throw 'Evidence commit identity differs; preserve it and stop.' }
if ($Parent -cne $SourceSha) { throw 'Evidence commit parent differs from the pinned source.' }
$ChangedExit = Invoke-Captured 'evidence-commit-paths' 'git diff-tree --no-commit-id --name-only -r HEAD' $Publish
if ($ChangedExit -ne 0) { throw 'Could not verify evidence commit paths.' }
$Changed = @(Get-Content (Join-Path $Evidence 'evidence-commit-paths.stdout.txt') | Where-Object { $_ })
if ($Changed.Count -eq 0 -or ($Changed | Where-Object { $_ -notlike 'evidence-out/*' })) {
  throw 'Evidence commit contains an empty or non-evidence path.'
}
$PushExit = Invoke-Captured 'evidence-push' "git push origin HEAD:refs/heads/$Ref" $Publish
if ($PushExit -ne 0) { throw 'Evidence push failed. Preserve the commit and output; do not force-push.' }
$VerifyExit = Invoke-Captured 'evidence-push-verify' "git ls-remote origin refs/heads/$Ref" $Publish
if ($VerifyExit -ne 0) { throw 'Could not verify the published ref. Preserve output.' }
$RemoteLine = ([string](Get-Content -Raw (Join-Path $Evidence 'evidence-push-verify.stdout.txt'))).Trim() -split '\s+'
if ($RemoteLine.Count -ne 2 -or $RemoteLine[0] -cne $Commit -or $RemoteLine[1] -cne "refs/heads/$Ref") {
  throw 'Published ref does not match the evidence commit.'
}
```

Report the ref, evidence commit SHA, source parent SHA, privacy-scan count, and push-verification output. Do not publish the private daemon data directory, user credentials, or raw files that failed the privacy check. Do not update `board.json`.
