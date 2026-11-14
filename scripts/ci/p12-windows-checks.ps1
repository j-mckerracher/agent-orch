<#
.SYNOPSIS
  The Priority 12 by-hand Windows checks, scripted: data-directory ACLs and the command-line refusal.

.DESCRIPTION
  Check 1 (acl-*): starts the built orchestrator-daemon.exe on a new data directory, prints `icacls` for the
  directory and every file in it (daemon.token, store.sqlite and its -shm/-wal, .orchestrator.lock), then
  asserts that each holds exactly one ACE, for the current user SID only, not inherited, with the DACL
  protected, and that no CodexSandboxUsers, SYSTEM, Administrators, Users, Everyone or Authenticated Users
  appear. The daemon is stopped by the pid this script started. daemon.log is NOT checked: only the desktop
  host creates it (it redirects the daemon's output there), so the bare daemon never does; that is printed
  as an INFO line, not hidden.

  Check 3 (cmdline-*): runs the built orchestrator-cli.exe with an objective that fits the CLI's own command
  line but not the 8,191-unit cmd.exe limit of a `.cmd` agent launcher (a stand-in codex.cmd that the script
  creates; it is never run). The CLI must refuse it at once with a message that names the limit, must not echo
  the objective and must create no store.sqlite. A literal 65,000-character objective cannot reach the CLI on
  Windows at all (CreateProcess refuses a command line over 32,767 units), so that attempt is recorded as
  INFO; the 65,000-character path through the daemon is covered by the daemon unit test
  `agent_commands_carry_the_whole_objective_and_windows_refuses_a_65000_character_one`.

  Check 2 (the redacted launch failure in the real window) needs a person to drive the window and is reported as
  NOT RUN here. The `tauri-tests` gate covers its logic.

  THIS SCRIPT MUST NOT HALT THE RUN. It never throws and never exits early: every check is wrapped, a failure is
  printed and the next check still runs. The verdict is the last line:
    P12-WINDOWS-CHECKS: PASS | FAIL (n of m failed) | INCOMPLETE (a check could not run)
  The exit code is 0 whatever the verdict, so a caller that stops on a non-zero exit is not stopped. Pass
  -Strict to get exit code 1 on FAIL. Do not wrap the call in `$ErrorActionPreference = 'Stop'`.

.PARAMETER TargetDir
  The cargo target directory that holds debug\orchestrator-daemon.exe and debug\orchestrator-cli.exe.
.PARAMETER RunRoot
  A directory for this script's data directories and throwaway repository. Keep it SHORT (see
  docs/release-ci.md, maximum path length). Default: a new directory under $env:TEMP.
.PARAMETER EvidenceDir
  When set, the output is also written to <EvidenceDir>\manual-p12.txt. All output (stdout too) has the user name,
  Windows domain, host name, DNS domain, profile path and any e-mail address replaced by placeholders.
.PARAMETER ObjectiveLength
  Characters of the objective for the command-line check. Default 9000, over the 8,191 of a .cmd launcher.
#>
param(
  [Parameter(Mandatory = $true)][string]$TargetDir,
  [string]$RunRoot = "",
  [string]$EvidenceDir = "",
  [int]$ObjectiveLength = 9000,
  [switch]$Strict
)

$ErrorActionPreference = 'Continue'
$script:Lines = New-Object System.Collections.Generic.List[string]
$script:Results = New-Object System.Collections.Generic.List[object]

# Replaces who and where the run happened with placeholders, so nothing that names the operator, their
# Windows domain, their machine or an e-mail address reaches stdout or the evidence. Whole-name matches
# only, so a short account name does not corrupt unrelated words. Mirrors scripts/ci/redact.mjs.
function Protect-Text([string]$Text) {
  if ([string]::IsNullOrEmpty($Text)) { return $Text }
  $Text = [regex]::Replace($Text, '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}', '<email>')
  $pairs = New-Object System.Collections.Generic.List[object]
  $id = $null
  try { $id = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name } catch { }
  if ($id) { $pairs.Add(@($id, '<user>')) }
  if ($env:USERDOMAIN -and $env:USERNAME) { $pairs.Add(@("$($env:USERDOMAIN)\$($env:USERNAME)", '<user>')) }
  if ($env:USERPROFILE) { $pairs.Add(@($env:USERPROFILE, '<profile>')) }
  if ($env:USERDNSDOMAIN) { $pairs.Add(@($env:USERDNSDOMAIN, '<dnsdomain>')) }
  if ($env:USERDOMAIN) { $pairs.Add(@($env:USERDOMAIN, '<domain>')) }
  if ($env:COMPUTERNAME) { $pairs.Add(@($env:COMPUTERNAME, '<host>')) }
  if ($env:USERNAME) { $pairs.Add(@($env:USERNAME, '<user>')) }
  foreach ($pair in $pairs) {
    if ($pair[0].Length -lt 3) { continue }
    $pattern = '(?<![A-Za-z0-9])' + [regex]::Escape($pair[0]) + '(?![A-Za-z0-9])'
    $Text = [regex]::Replace($Text, $pattern, $pair[1], [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
  }
  $Text = [regex]::Replace($Text, '(?<![A-Za-z0-9._\\/:-])[A-Z][A-Z0-9._-]{2,14}\\<user>', '<user>')
  return $Text
}

function Say([string]$Text) {
  $Text = Protect-Text $Text
  $script:Lines.Add($Text)
  Write-Output $Text
}

function Record([string]$Name, [string]$Status, [string]$Detail) {
  $script:Results.Add([pscustomobject]@{ Name = $Name; Status = $Status })
  Say ("CHECK {0}: {1} - {2}" -f $Name, $Status, $Detail)
}

# Runs one check body; an exception becomes a FAIL, never a halt.
function Check([string]$Name, [scriptblock]$Body) {
  try {
    & $Body
  } catch {
    Record $Name 'FAIL' ("exception: " + $_.Exception.Message)
  }
}

function Start-Captured([string]$Exe, [string]$ArgLine, [string]$OutFile, [string]$ErrFile) {
  $p = Start-Process -FilePath $Exe -ArgumentList $ArgLine -PassThru -NoNewWindow `
    -RedirectStandardOutput $OutFile -RedirectStandardError $ErrFile
  $null = $p.Handle
  return $p
}

function Quote([string]$Value) {
  if ($Value -match '[\s"]') { return '"' + $Value.Replace('"', '\"') + '"' }
  return $Value
}

$onWindows = ($env:OS -eq 'Windows_NT')
$daemon = Join-Path $TargetDir 'debug\orchestrator-daemon.exe'
$cli = Join-Path $TargetDir 'debug\orchestrator-cli.exe'
if (-not $onWindows) {
  $daemon = Join-Path $TargetDir 'debug/orchestrator-daemon'
  $cli = Join-Path $TargetDir 'debug/orchestrator-cli'
}
if ($RunRoot -eq "") {
  $RunRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("p12-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
}
$null = New-Item -ItemType Directory -Force -Path $RunRoot -ErrorAction SilentlyContinue
Say "INFO run-root length $($RunRoot.Length) characters"
Say "INFO platform $(if ($onWindows) { 'windows' } else { 'not windows (ACL checks are NOT RUN)' })"

# ---- Check 1: data-directory ACLs -----------------------------------------------------------------
Check 'acl' {
  if (-not $onWindows) { Record 'acl' 'NOT RUN' 'ACLs are a Windows check'; return }
  if (-not (Test-Path $daemon)) { Record 'acl' 'NOT RUN' "daemon binary missing: $daemon"; return }
  $dir = Join-Path $RunRoot 'acl-data'
  $proc = $null
  try {
    $proc = Start-Captured $daemon ("--data-dir " + (Quote $dir)) (Join-Path $RunRoot 'acl-daemon.out') (Join-Path $RunRoot 'acl-daemon.err')
    Say "acl: started daemon pid $($proc.Id)"
    $deadline = (Get-Date).AddSeconds(30)
    # daemon.log is created by the desktop host, not by the bare daemon this script starts (the host
    # redirects the daemon's output into it). It is reported as NOT CHECKED below, not waited for.
    $wanted = 'daemon.token', 'store.sqlite'
    while ((Get-Date) -lt $deadline) {
      if (($wanted | Where-Object { -not (Test-Path (Join-Path $dir $_)) }).Count -eq 0) { break }
      if ($proc.HasExited) { break }
      Start-Sleep -Milliseconds 250
    }
    $missing = @($wanted | Where-Object { -not (Test-Path (Join-Path $dir $_)) })
    if ($missing.Count -gt 0) {
      Record 'acl-files-present' 'FAIL' ("missing after 30 s: " + ($missing -join ', ') + "; daemon exited: " + $proc.HasExited)
    } else {
      Record 'acl-files-present' 'PASS' ($wanted -join ', ')
    }
    Say 'INFO acl-daemon.log: NOT CHECKED here; only the desktop host creates daemon.log. Its owner-only DACL is covered by the tauri-tests test a_new_and_an_existing_log_are_owner_only_on_every_platform and is UNPROVEN on a real host'

    $me = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
    $forbidden = 'CodexSandboxUsers', 'NT AUTHORITY\SYSTEM', 'BUILTIN\Administrators', 'BUILTIN\Users', 'Everyone', 'Authenticated Users'
    $targets = @($dir) + @(Get-ChildItem -LiteralPath $dir -Force -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName })
    foreach ($path in $targets) {
      $leaf = if ($path -eq $dir) { 'data-dir' } else { Split-Path $path -Leaf }
      $icacls = (& icacls.exe $path 2>&1 | Out-String)
      Say "---- icacls $leaf"
      Say $icacls.TrimEnd()
      $problems = New-Object System.Collections.Generic.List[string]
      if ($icacls -match '\(I\)') { $problems.Add('an inherited ACE (I) is present') }
      foreach ($name in $forbidden) {
        if ($icacls.IndexOf($name, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) { $problems.Add("$name is present") }
      }
      $acl = Get-Acl -LiteralPath $path
      $rules = @($acl.GetAccessRules($true, $true, [System.Security.Principal.SecurityIdentifier]))
      if ($rules.Count -ne 1) { $problems.Add("$($rules.Count) ACEs, expected exactly 1") }
      foreach ($rule in $rules) {
        if ($rule.IdentityReference.Value -ne $me.Value) { $problems.Add("ACE is for SID $($rule.IdentityReference.Value), not the current user") }
        if ($rule.IsInherited) { $problems.Add('ACE is inherited') }
        if ($rule.AccessControlType -ne 'Allow') { $problems.Add("ACE is $($rule.AccessControlType)") }
        if (($rule.FileSystemRights -band [System.Security.AccessControl.FileSystemRights]::FullControl) -ne [System.Security.AccessControl.FileSystemRights]::FullControl) {
          $problems.Add("rights are $($rule.FileSystemRights), expected FullControl")
        }
      }
      if (-not $acl.AreAccessRulesProtected) { $problems.Add('the DACL is not protected from inheritance') }
      $isDir = (Get-Item -LiteralPath $path -Force).PSIsContainer
      if ($isDir -and $icacls -notmatch '\(OI\)\(CI\)\(F\)') { $problems.Add('the directory ACE is not (OI)(CI)(F)') }
      if (-not $isDir -and $icacls -notmatch '\(F\)') { $problems.Add('the file ACE is not (F)') }
      if ($problems.Count -eq 0) { Record "acl-$leaf" 'PASS' 'only the current user, full control, protected, nothing inherited' }
      else { Record "acl-$leaf" 'FAIL' ($problems -join '; ') }
    }
    Say "---- icacls parent (information only, not asserted)"
    Say ((& icacls.exe $RunRoot 2>&1 | Out-String).TrimEnd())
  } finally {
    if ($proc -ne $null -and -not $proc.HasExited) {
      Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
      Say "acl: stopped daemon pid $($proc.Id)"
    }
  }
}

# ---- Check 3: the CLI refuses an objective the agent launcher cannot take -------------------------
Check 'cmdline' {
  if (-not (Test-Path $cli)) { Record 'cmdline' 'NOT RUN' "CLI binary missing: $cli"; return }
  $repo = Join-Path $RunRoot 'cmdline-repo'
  $dir = Join-Path $RunRoot 'cmdline-data'
  $null = New-Item -ItemType Directory -Force -Path $repo
  $null = & git -C $repo init -q -b main 2>&1
  $null = & git -C $repo -c user.name=check -c user.email=check@example.invalid commit -q --allow-empty -m base 2>&1
  $launcher = if ($onWindows) { Join-Path $RunRoot 'fake-codex.cmd' } else { Join-Path $RunRoot 'fake-codex' }
  Set-Content -LiteralPath $launcher -Value $(if ($onWindows) { '@exit /b 0' } else { '#!/bin/sh' })
  $objective = ((@('word') * ([Math]::Ceiling($ObjectiveLength / 5.0) + 1)) -join ' ').Substring(0, $ObjectiveLength)
  $argLine = "--data-dir $(Quote $dir) run --repo $(Quote $repo) --agent-program $(Quote $launcher) --objective $(Quote $objective)"
  $out = Join-Path $RunRoot 'cmdline.out'
  $err = Join-Path $RunRoot 'cmdline.err'
  $watch = [System.Diagnostics.Stopwatch]::StartNew()
  $proc = Start-Captured $cli $argLine $out $err
  $exited = $proc.WaitForExit(60000)
  $watch.Stop()
  if (-not $exited) {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    Record 'cmdline-refused' 'FAIL' 'the CLI did not return within 60 s (a hang)'
    return
  }
  $text = ((Get-Content -LiteralPath $out -Raw -ErrorAction SilentlyContinue) + (Get-Content -LiteralPath $err -Raw -ErrorAction SilentlyContinue))
  $first = ($text -split "`r?`n" | Where-Object { $_.Trim() -ne '' } | Select-Object -First 1)
  $problems = New-Object System.Collections.Generic.List[string]
  if ($proc.ExitCode -eq 0) { $problems.Add('exit code 0, the objective was not refused') }
  if ($text -notmatch 'Shorten the task objective') { $problems.Add('the message does not say to shorten the objective') }
  if ($text -notmatch 'allows at most \d+') { $problems.Add('the message does not name the limit') }
  if ($onWindows -and $text -notmatch 'allows at most 8191') { $problems.Add('the message does not name the 8191 cmd.exe limit') }
  if ($text -match 'word word word word') { $problems.Add('the objective text was echoed') }
  if (Test-Path (Join-Path $dir 'store.sqlite')) { $problems.Add('store.sqlite exists: a record may have been made') }
  if ($watch.Elapsed.TotalSeconds -gt 30) { $problems.Add("took $([int]$watch.Elapsed.TotalSeconds) s, not at once") }
  $firstShort = if ($first.Length -gt 200) { $first.Substring(0, 200) } else { $first }
  Say "cmdline: exit code $($proc.ExitCode) after $([int]$watch.Elapsed.TotalMilliseconds) ms; first line: $firstShort"
  if ($problems.Count -eq 0) { Record 'cmdline-refused' 'PASS' 'refused before any record, with the limit named and no objective echoed' }
  else { Record 'cmdline-refused' 'FAIL' ($problems -join '; ') }

  if ($onWindows) {
    # Information only: the OS itself refuses to start a process with a 65,000-character argument.
    $huge = ('y' * 65000)
    try {
      $null = Start-Process -FilePath $cli -ArgumentList "run --objective $huge" -PassThru -NoNewWindow -ErrorAction Stop
      Say 'INFO cmdline-65k-literal: the OS started a 65,000-character command line (unexpected on Windows)'
    } catch {
      $reason = $_.Exception.Message
      if ($reason.Length -gt 160) { $reason = $reason.Substring(0, 160) }
      Say "INFO cmdline-65k-literal: the OS refused to start it, as expected: $reason"
    }
  }
}

# ---- Check 2 ---------------------------------------------------------------------------------------
Record 'launch-failure-window' 'NOT RUN' 'needs a person to drive the desktop window; the tauri-tests gate covers the redaction logic'

# ---- Verdict ---------------------------------------------------------------------------------------
$failed = @($script:Results | Where-Object { $_.Status -eq 'FAIL' }).Count
$notRun = @($script:Results | Where-Object { $_.Status -eq 'NOT RUN' -and $_.Name -ne 'launch-failure-window' }).Count
$total = @($script:Results | Where-Object { $_.Status -ne 'NOT RUN' }).Count
if ($failed -gt 0) { $verdict = "FAIL ($failed of $total failed)" }
elseif ($notRun -gt 0 -or $total -eq 0) { $verdict = "INCOMPLETE ($notRun check(s) could not run)" }
else { $verdict = 'PASS' }
Say "P12-WINDOWS-CHECKS: $verdict"

if ($EvidenceDir -ne "") {
  try {
    $null = New-Item -ItemType Directory -Force -Path $EvidenceDir
    $text = ($script:Lines -join "`r`n")
    Set-Content -LiteralPath (Join-Path $EvidenceDir 'manual-p12.txt') -Value $text
  } catch {
    Write-Output "p12-windows-checks: could not write manual-p12.txt: $($_.Exception.Message)"
  }
}

if ($Strict -and $failed -gt 0) { exit 1 }
exit 0
