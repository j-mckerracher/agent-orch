# Windows diagnostics for preliminary M7 P12 candidate 3a0c976

Updated: 2026-11-11T19:15:19Z

Run this as a preliminary Windows diagnostic against source commit `3a0c976adf15df35c58734749155d0bb7b3b9460`, tree `139fa70a8e1fe2614d54e6d54a2491bf1b67fe74`. This commit contains the ResourceStrip timing remediation. Its independent scoped review passed. It is not the final integrated M7 candidate and this run is not priority-12 acceptance. Run the complete Windows checks again against the final integrated candidate before making any acceptance claim. Required macOS checks remain separate.

The earlier Windows run tested `6c17442eaee5243b48034dfc53d7f43d25f1ab82`. It reported 1,104 passing tests and one failing ResourceStrip hidden-window timing test. Native capture failed with `GetCursorPos failed: Access is denied. (0x80070005)`. That runner used Node `v22.20.0`, below the locked `jsdom@31.0.0` and `w3c-xmlserializer@6.0.0` requirement `^22.22.2 || ^24.15.0 || >=26.0.0`. Preserve each new attempt separately. Do not transfer results from the earlier source commit.

## Create an isolated run

Use Windows PowerShell 5.1 or PowerShell 7. Set `$RunBase` to an existing short local path without spaces, such as `C:\t`. Use a local volume with at least 20 GiB free. Do not use a mapped drive, `SUBST` drive, junction, existing checkout, or shared evidence directory. Do not delete files to make room.

```powershell
$ErrorActionPreference = 'Stop'
$SourceSha = '3a0c976adf15df35c58734749155d0bb7b3b9460'
$ExpectedTree = '139fa70a8e1fe2614d54e6d54a2491bf1b67fe74'
$RunBase = 'C:\t'
if (!(Test-Path -LiteralPath $RunBase -PathType Container)) { throw 'Choose an existing short local run directory.' }
$RunId = [guid]::NewGuid().ToString('N')
$RunRoot = Join-Path $RunBase "m7p12-$RunId"
$Source = Join-Path $RunRoot 'src'
$Desktop = Join-Path $Source 'apps\desktop'
$Evidence = Join-Path $RunRoot 'evidence'
$PrivateData = Join-Path $RunRoot 'private-daemon-data'
$PublishInput = Join-Path $RunRoot 'publish-input'
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
  if ((Get-Content -Raw (Join-Path $Evidence "$Name-root.stdout.txt")).Trim() -ine $Source) {
    throw 'Command root is outside the isolated source checkout.'
  }
  if ((Get-Content -Raw (Join-Path $Evidence "$Name-head.stdout.txt")).Trim() -cne $SourceSha) {
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
$Head = (Get-Content -Raw (Join-Path $Evidence 'source-head.stdout.txt')).Trim()
$Object = Get-Content -Raw (Join-Path $Evidence 'source-object.stdout.txt')
$Tree = [regex]::Match($Object, '(?m)^tree ([0-9a-f]{40})$').Groups[1].Value
if ($Head -cne $SourceSha -or $Tree -cne $ExpectedTree) { throw 'Source SHA or tree differs. Stop.' }
$StatusExit = Invoke-Captured 'source-status-before' 'git status --porcelain=v1 --untracked-files=all' $Source
if ($StatusExit -ne 0 -or (Get-Content -Raw (Join-Path $Evidence 'source-status-before.stdout.txt')).Trim()) {
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
Invoke-Captured 'npm-version' 'npm.cmd --version' $RunRoot
Invoke-Captured 'rustc-version' 'rustc.exe -V' $RunRoot
Invoke-Captured 'rustc-host' 'rustc.exe -vV' $RunRoot
Invoke-Captured 'cargo-version' 'cargo.exe -V' $RunRoot
Invoke-Captured 'msvc-path' 'where.exe cl.exe' $RunRoot
```

The lockfile requires Node `^22.22.2 || ^24.15.0 || >=26.0.0` for both `jsdom@31.0.0` and `w3c-xmlserializer@6.0.0`. Check the installed version before any npm command. Do not change the machine runtime silently, install Node, or use `npx`.

```powershell
$NodeText = (Get-Content -Raw (Join-Path $Evidence 'node-version-plain.stdout.txt')).Trim()
$NodeVersion = [version]$NodeText
$SupportedNode = ($NodeVersion.Major -eq 22 -and $NodeVersion -ge [version]'22.22.2') -or `
  ($NodeVersion.Major -eq 24 -and $NodeVersion -ge [version]'24.15.0') -or $NodeVersion.Major -ge 26
if (-not $SupportedNode) { throw "Unsupported Node $NodeVersion. Preserve preflight and stop before npm test, build, or ci." }
```

Check the source root before every test or build. Run commands from `apps\desktop` for npm and from the source root for Cargo. If a check fails for a real assertion or compiler error, preserve it and continue only with independent checks. Do not turn a failure into a pass by installing or changing tools.

## Run the frontend checks

Run the full Vitest suite and production frontend build. The scripts are `vitest run` and `tsc -b && vite build` in `apps/desktop/package.json`.

```powershell
Assert-SourceWorktree 'before-npm-test' $Desktop
$TestExit = Invoke-Captured 'npm-test-first' 'npm.cmd test' $Desktop
```

If the first test fails because `vitest` or a locked package is missing, preserve that failure first. Run `npm.cmd ci` only when the checkout has no `apps\desktop\node_modules`. `npm ci` replaces that directory, so do not run it if the directory already exists. Do not install individual packages. After a successful `ci`, rerun the full test and build with new evidence names.

```powershell
if ($TestExit -ne 0) {
  $TestText = (Get-Content -Raw (Join-Path $Evidence 'npm-test-first.stdout.txt')) +
    (Get-Content -Raw (Join-Path $Evidence 'npm-test-first.stderr.txt'))
  if ($TestText -match '(?i)vitest.*not recognized|cannot find module|module not found|package .* not found') {
    if (Test-Path -LiteralPath (Join-Path $Desktop 'node_modules')) {
      throw 'node_modules already exists. Preserve it and do not run npm ci.'
    }
    $CiExit = Invoke-Captured 'npm-ci-after-missing-dependency' 'npm.cmd ci' $Desktop
    if ($CiExit -ne 0) { throw 'npm ci failed. Preserve output and stop dependency recovery.' }
    Assert-SourceWorktree 'before-npm-test-retry' $Desktop
    $TestExit = Invoke-Captured 'npm-test-retry' 'npm.cmd test' $Desktop
  }
}
Assert-SourceWorktree 'before-npm-build' $Desktop
$BuildExit = Invoke-Captured 'npm-build' 'npm.cmd run build' $Desktop
```

Do not replace this suite with selected tests. Report the actual test count and every failure. Run the build even when a real test assertion fails. Do not run `npm ci` after an assertion failure.

## Build and launch the native app

The repository README documents `npm run tauri dev` from `apps/desktop`. The debug host reads `AGENT_ORCH_DAEMON_BIN`, starts the daemon on first connection, and passes its data directory to the daemon. Do not start the daemon manually or invent flags.

Keep Cargo offline until a command reports a missing cached crate. This prevents automatic downloads before the failure is recorded.

```powershell
$env:CARGO_NET_OFFLINE = 'true'
Assert-SourceWorktree 'before-daemon-build' $Source
$DaemonBuildExit = Invoke-Captured 'daemon-build-first' 'cargo build --locked -p orchestrator-daemon' $Source
$DaemonText = (Get-Content -Raw (Join-Path $Evidence 'daemon-build-first.stdout.txt')) +
  (Get-Content -Raw (Join-Path $Evidence 'daemon-build-first.stderr.txt'))
if ($DaemonBuildExit -ne 0 -and $DaemonText -match '(?i)no matching package|failed to download|offline mode') {
  $env:CARGO_NET_OFFLINE = 'false'
  $FetchExit = Invoke-Captured 'daemon-fetch-after-missing-crate' 'cargo fetch --locked' $Source
  $env:CARGO_NET_OFFLINE = 'true'
  if ($FetchExit -eq 0) {
    Assert-SourceWorktree 'before-daemon-build-retry' $Source
    $DaemonBuildExit = Invoke-Captured 'daemon-build-retry' 'cargo build --locked -p orchestrator-daemon' $Source
  }
}
if ($DaemonBuildExit -ne 0) { throw 'Daemon build failed. Preserve output and stop.' }
$env:AGENT_ORCH_DAEMON_BIN = Join-Path $Source 'target\debug\orchestrator-daemon.exe'
if (!(Test-Path -LiteralPath $env:AGENT_ORCH_DAEMON_BIN)) { throw 'Daemon build output is missing. Preserve output and stop.' }
$env:AGENT_ORCH_DATA_DIR = $PrivateData
Assert-SourceWorktree 'before-tauri-dev' $Desktop
$TauriExit = Invoke-Captured 'tauri-dev-first' 'npm.cmd run tauri dev' $Desktop
$TauriText = (Get-Content -Raw (Join-Path $Evidence 'tauri-dev-first.stdout.txt')) +
  (Get-Content -Raw (Join-Path $Evidence 'tauri-dev-first.stderr.txt'))
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

Interact with the native window while the command waits. Close the window to end the Tauri dev session. The daemon is detached and may remain running. Record its process and data path; do not kill unrelated processes or remove the private data directory.

## Check the UI and source contracts

Record each check as `PASS`, `FAIL`, `UNVERIFIED`, or `NOT RUN`. Do not create tasks, approvals, agent sessions, or fixture mutations. Register only the isolated source checkout if S1 needs a project. Keep daemon data outside published evidence.

Create a screenshot directory before opening the app. If that path exists, preserve it and start a fresh run.

```powershell
$Screens = Join-Path $Evidence 'screens'
if (Test-Path -LiteralPath $Screens) { throw 'Screens directory already exists. Preserve it and use a new run.' }
New-Item -ItemType Directory -Path $Screens | Out-Null
```

1. Check the theme toggle in light and dark modes. Close and reopen the app to check the explicit choice persists. Record the initial appearance and restore it before ending the run. Check system-default behavior only when WebView2 has no saved `agent-orch.theme` choice. The isolated daemon data directory does not reset WebView2 storage. Do not clear a user profile or change Windows-wide appearance settings. Mark system-default behavior unverified when an earlier saved choice exists. Storage-failure tests and 4.5:1 text and 3:1 non-text contrast checks are automated.
2. Check Windows font rendering and text truncation. The source stack names Segoe UI and bundles no font. Check visible focus rings and reduced-motion behavior. Mark system-preference behavior unverified if testing it would require changing Windows settings.
3. Check that the native window opens at 1280 by 800. Resize it to 1280, 1150, 960, and 760 pixels wide. Check for horizontal scrolling, visible shell regions, activity-panel stacking, and the collapsed sidebar menu at 760. The configured minimum is 760 by 600 pixels. Record any size the host cannot reach.
4. Visit all global routes: Orchestrator Workspace, Workstreams, Agents & Sessions, and Review & Decisions. Check the default workspace, deep links, back and forward navigation, and the persistent ConnectionStrip on every global and legacy route. Native fixture and disconnected boot modes remain unverified unless you can observe them without changing settings.
5. Visit legacy S1 through S6 without changing source data: `#/projects`, `#/project/<project-id>`, `#/project/<project-id>/tasks/all`, `#/project/<project-id>/task/<real-task-id>`, `#/project/<project-id>/coordinator`, and `#/project/<project-id>/review/<real-task-id>`. Use IDs returned by the isolated app. Do not invent IDs or create records for empty views. Mark routes that need unavailable real data unverified.
6. Use Tab and Enter to reach the skip link and primary navigation. Check visible focus, selected-link `aria-current`, one `main` landmark, named `Primary views` and `Workstreams` navigation landmarks, and the `Workspace activity` complementary landmark. Use the native accessibility tree or screen reader when available. Preserve errors and mark unavailable checks unverified.
7. Check source contracts in `apps/desktop/src/ui/theme/theme.ts`, `apps/desktop/src/ui/theme/theme.test.ts`, `apps/desktop/src/ui/theme/ThemeToggle.test.tsx`, `apps/desktop/src/ui/theme/contrast.test.ts`, `apps/desktop/src/ui/theme/tokens.css`, `apps/desktop/index.html`, `apps/desktop/public/theme-init.js`, `apps/desktop/src/ui/router.ts`, `apps/desktop/src/ui/router.test.ts`, `apps/desktop/src/ui/shell/SidebarNav.tsx`, `apps/desktop/src/ui/shell/AppShell.test.tsx`, `apps/desktop/src/ui/shell/shell.css`, `apps/desktop/src/sources/surfaces/`, and `apps/desktop/src-tauri/tauri.conf.json`. Confirm the four global routes, legacy formats, theme-before-paint script, font stack and no `@font-face`, D9-D16 adapter contracts, landmarks, focus behavior, 1150/960/760 breakpoints, and 1280 by 800 default and 760 by 600 minimum window sizes. Source inspection is not native UI proof.
8. Check that live views show unavailable or unknown values honestly and do not fall back to fixture values. Record views that cannot connect to the isolated daemon.

Save screenshots only when the authorized native capture mechanism works. Keep the complete error if capture fails, including `GetCursorPos failed: Access is denied. (0x80070005)` when returned. Do not elevate, change security settings, substitute a browser screenshot, or infer acceptance. Mark affected native checks unverified.

Write `$Evidence\REPORT.md` with source SHA and tree, Windows build, tool versions, every command and exit code, test count and failures, build result, manual check status, capture result, source status before and after, and limitations. Label it preliminary diagnostics, not priority-12 acceptance.

Check the source status after all commands. Preserve unexpected changes and do not clean the checkout.

```powershell
Assert-SourceWorktree 'after-run' $Source
$StatusAfterExit = Invoke-Captured 'source-status-after' 'git status --porcelain=v1 --untracked-files=all' $Source
if ($StatusAfterExit -ne 0) { throw 'Could not capture final source status.' }
$StatusAfter = (Get-Content -Raw (Join-Path $Evidence 'source-status-after.stdout.txt')).Trim()
if ($StatusAfter) { throw 'Source checkout changed. Preserve it and report the changed paths.' }
```


## Publish evidence on a fresh ref

Keep raw evidence unchanged in `$Evidence`. Copy only the report, captured command output, and reviewed screenshots into `$PublishInput`. Exclude the checkout, `target`, `node_modules`, `private-daemon-data`, credentials, and unrelated files.

Scan text evidence for secrets and personal data before publishing. Add the operator's work or school email domain when prompted. Print only a count, never matching lines or values. Inspect text captures and screenshots for credentials, local paths, notifications, and personal data. If a scan finds a match, use `$Hits` to identify filenames without printing contents, sanitize only publication copies, and rerun the scan from `$PrivateNeedles` onward. Keep raw evidence unchanged.

```powershell
if (Test-Path -LiteralPath $PublishInput) { throw 'Publish input already exists. Preserve it and use a fresh run.' }
New-Item -ItemType Directory -Path $PublishInput | Out-Null
Copy-Item -LiteralPath (Join-Path $Evidence 'REPORT.md') -Destination $PublishInput
$CaptureFiles = Get-ChildItem -LiteralPath $Evidence -File | Where-Object { $_.Extension -eq '.txt' }
foreach ($File in $CaptureFiles) { Copy-Item -LiteralPath $File.FullName -Destination $PublishInput }
$ScreenshotFiles = @(Get-ChildItem -LiteralPath $Screens -File)
if ($ScreenshotFiles.Count -gt 0) {
  $ScreenshotReview = Read-Host 'Have you inspected every screenshot for private content? Type YES'
  if ($ScreenshotReview -cne 'YES') { throw 'Do not publish screenshots before review.' }
  foreach ($File in $ScreenshotFiles) { Copy-Item -LiteralPath $File.FullName -Destination $PublishInput }
}
$EmailDomain = Read-Host 'Work or school email domain to scan for'
$PrivateNeedles = @($env:USERNAME, $env:USERPROFILE, $EmailDomain) | Where-Object { $_ }
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
$Ref = "evidence/m7-p12-preliminary-3a0c976-$Utc-$Hex"
$Publish = Join-Path $RunRoot 'publish'
if (Test-Path -LiteralPath $Publish) { throw 'Publish worktree path exists. Keep it and stop.' }
$WorktreeExit = Invoke-Captured 'evidence-worktree' "git worktree add --detach $Publish $SourceSha" $Source
if ($WorktreeExit -ne 0) { throw 'Could not create evidence worktree. Preserve it and stop.' }
$EvidenceOut = Join-Path $Publish 'evidence-out'
if (Test-Path -LiteralPath $EvidenceOut) { throw 'evidence-out already exists. Do not overwrite it.' }
New-Item -ItemType Directory -Path $EvidenceOut | Out-Null
Copy-Item -Path (Join-Path $PublishInput '*') -Destination $EvidenceOut -Recurse
```

Use the operator's approved GitHub noreply identity for the evidence commit. Set it only in this PowerShell process. Do not change Git configuration or the `origin` remote. Stop if the target ref already exists or the remote check fails.

```powershell
$RemoteCheck = Invoke-Captured 'remote-ref-check' "git ls-remote --heads origin refs/heads/$Ref" $Publish
if ($RemoteCheck -ne 0) { throw 'Remote ref check failed. Preserve output and stop.' }
if ((Get-Content -Raw (Join-Path $Evidence 'remote-ref-check.stdout.txt')).Trim()) {
  throw 'Evidence ref already exists. Do not overwrite it; start a new run.'
}
$StatusExit = Invoke-Captured 'publish-status-before' 'git status --porcelain=v1 --untracked-files=all' $Publish
if ($StatusExit -ne 0 -or (Get-Content -Raw (Join-Path $Evidence 'publish-status-before.stdout.txt')).Trim() -cne '?? evidence-out/') {
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
$env:GIT_AUTHOR_NAME = '<approved GitHub noreply name>'
$env:GIT_AUTHOR_EMAIL = '<approved GitHub noreply address>'
$env:GIT_COMMITTER_NAME = $env:GIT_AUTHOR_NAME
$env:GIT_COMMITTER_EMAIL = $env:GIT_AUTHOR_EMAIL
$CommitExit = Invoke-Captured 'evidence-commit' 'git commit -m "Windows preliminary M7 P12 evidence for 3a0c976"' $Publish
if ($CommitExit -ne 0) { throw 'Evidence commit failed. Preserve the staged files and output.' }
$CommitExit = Invoke-Captured 'evidence-commit-sha' 'git rev-parse HEAD' $Publish
$ParentExit = Invoke-Captured 'evidence-commit-parent' 'git rev-parse HEAD~1' $Publish
if ($CommitExit -ne 0 -or $ParentExit -ne 0) { throw 'Could not verify evidence commit metadata.' }
$Commit = (Get-Content -Raw (Join-Path $Evidence 'evidence-commit-sha.stdout.txt')).Trim()
$Parent = (Get-Content -Raw (Join-Path $Evidence 'evidence-commit-parent.stdout.txt')).Trim()
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
$RemoteLine = (Get-Content -Raw (Join-Path $Evidence 'evidence-push-verify.stdout.txt')).Trim() -split '\s+'
if ($RemoteLine.Count -ne 2 -or $RemoteLine[0] -cne $Commit -or $RemoteLine[1] -cne "refs/heads/$Ref") {
  throw 'Published ref does not match the evidence commit.'
}
```

Report the ref, evidence commit SHA, source parent SHA, privacy-scan count, and push-verification output. Do not publish the private daemon data directory, user credentials, or raw files that failed the privacy check. Do not update `board.json`.
