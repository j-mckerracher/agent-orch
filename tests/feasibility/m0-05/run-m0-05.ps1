# M0-05 single-task feasibility runner (Windows).
# Runs one fixed Codex CLI task in a fresh temporary Git repository and
# verifies every acceptance check. Preserves raw artifacts in -OutDir.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File tests/feasibility/m0-05/run-m0-05.ps1 `
#     -CodexPath "C:\Users\jmckerra\AppData\Local\OpenAI\Codex\bin\14b838c1d074ff01\codex.exe" `
#     -PrimaryCheckout "C:\Users\jmckerra\Code\agent-orch-m0-05-priority3" `
#     -OutDir "docs\evidence\m0-05-windows\raw\run-<timestamp>"
#
# Safety: creates its own temp repo (git init in a unique temp directory),
# never removes shared or fixed directories, never commits into the primary
# checkout, and only --sandbox workspace-writes inside the temp repo via -C.
param(
  [Parameter(Mandatory=$true)][string]$CodexPath,
  [Parameter(Mandatory=$true)][string]$PrimaryCheckout,
  [Parameter(Mandatory=$true)][string]$OutDir
)
$ErrorActionPreference = "Stop"
$repo = Join-Path $env:TEMP ("agent-orch-m0-05-task-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $repo | Out-Null
# The single canonical M0-05 task, shared with the macOS runner.
# Trimmed to match bash `$(cat ...)` behavior on the mac side.
$taskLine = (Get-Content -Raw (Join-Path $PSScriptRoot 'task-prompt.txt')).Trim()

function Save-File([string]$name, [string]$content) {
  Set-Content -Path (Join-Path $OutDir $name) -Value $content -Encoding utf8 -NoNewline
}

# codex writes status text to stderr; PowerShell surfaces native stderr as
# error records, which stop under $ErrorActionPreference="Stop".
function Invoke-Capture([scriptblock]$cmd) {
  $prev = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try { $cmd.Invoke() -join " " } finally { $ErrorActionPreference = $prev }
}

New-Item -ItemType Directory -Path $OutDir -Force | Out-Null

# Environment
$envLines = @()
$envLines += "host=" + [System.Environment]::MachineName
$envLines += "os=" + [System.Environment]::OSVersion.Version + " " + $env:PROCESSOR_ARCHITECTURE
$envLines += "codex_path=" + $CodexPath
$envLines += "codex_version=" + (Invoke-Capture { & $CodexPath --version 2>&1 })
$envLines += "git_version=" + (& git --version 2>&1)
$envLines += "powershell=" + $PSVersionTable.PSVersion.ToString()
$envLines += "primary_checkout=" + (Resolve-Path $PrimaryCheckout).Path
$envLines += "task_repo=" + $repo
$envLines += "source_head=" + (& git -C $PrimaryCheckout rev-parse HEAD 2>&1)
$envLines += "source_branch=" + (& git -C $PrimaryCheckout rev-parse --abbrev-ref HEAD 2>&1)
$envLines += "login_status=" + (Invoke-Capture { & $CodexPath login status 2>&1 })
Save-File "environment.txt" ($envLines -join "`r`n")

# Primary checkout status BEFORE (isolation proof)
$primaryBefore = git -C $PrimaryCheckout status --porcelain=v1 -b
Save-File "primary-status-before.txt" ($primaryBefore -join "`n")

# Fresh disposable Git repository
& git -C $repo init -q -b main
# This host's global Git config has core.autocrlf=true; the task repo must
# store and read bytes unchanged for the exact-byte checks.
& git -C $repo config core.autocrlf false
& git -C $repo config core.eol lf
# WriteAllText: UTF-8 without BOM, LF line ending, no trailing newline added.
[System.IO.File]::WriteAllText((Join-Path $repo "TASK.txt"), "M0_TASK_BEFORE`n")
& git -C $repo add TASK.txt
& git -C $repo commit -q -m "init: TASK.txt with M0_TASK_BEFORE"
Save-File "init-commit.txt" (& git -C $repo rev-parse HEAD)

# Fixed task run
# The host user config pins [windows] sandbox = "elevated"; that mode fails
# on this machine with CreateProcessWithLogonW 1385 for every sandboxed spawn.
# The unelevated (token-restriction) sandbox works and still enforces the
# workspace-write policy, so pin it per invocation instead of editing
# ~/.codex/config.toml (shared state; see evidence report, attempt 2).
$codexArgs = @(
  "exec", "--json", "--color", "never",
  "-c", 'windows.sandbox="unelevated"',
  "-C", $repo,
  "--sandbox", "workspace-write",
  "-o", (Join-Path $OutDir "last-message.txt"),
  $taskLine
)
# Empty piped stdin: codex would otherwise open the console and block.
# Diagnostics go to stderr; treat them as text, not as terminating errors.
$prevPref = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
  $null | & $CodexPath @codexArgs 2> (Join-Path $OutDir "stderr.txt") |
    Out-File -FilePath (Join-Path $OutDir "stdout.jsonl") -Encoding utf8
  $exitCode = $LASTEXITCODE
} finally {
  $ErrorActionPreference = $prevPref
}
# Out-File utf8 adds a BOM; strict JSONL consumers must not see one.
$sp = Join-Path $OutDir "stdout.jsonl"
$sb = [System.IO.File]::ReadAllBytes($sp)
if ($sb.Length -ge 3 -and $sb[0] -eq 0xEF -and $sb[1] -eq 0xBB -and $sb[2] -eq 0xBF) {
  [System.IO.File]::WriteAllBytes($sp, $sb[3..($sb.Length - 1)])
}
Save-File "exit.txt" "$exitCode`r`n"

# Post-run state of the task repo
Save-File "post-status.txt" ((& git -C $repo status --porcelain=v1) -join "`n")
Save-File "post-diff.txt" ((& git -C $repo diff) -join "`n")
Save-File "diff-check.txt" ((& git -C $repo diff --check 2>&1) -join "`n")
$taskFileBytes = [System.IO.File]::ReadAllBytes((Join-Path $repo "TASK.txt"))
Save-File "task-file.txt" ([System.Text.Encoding]::ASCII.GetString($taskFileBytes))
Save-File "task-file-hex.txt" ((($taskFileBytes | ForEach-Object { $_.ToString("X2") }) -join " ") + "`r`n")

# Primary checkout status AFTER (isolation proof)
$primaryAfter = git -C $PrimaryCheckout status --porcelain=v1 -b
Save-File "primary-status-after.txt" ($primaryAfter -join "`n")

# Verification
$checks = @()
function Add-Check([string]$name, [bool]$ok, [string]$detail) {
  $script:checks += ("{0,-34} {1}  {2}" -f $name, $(if ($ok) { "PASS" } else { "FAIL" }), $detail)
}
$expectedAfter = [System.Text.Encoding]::ASCII.GetBytes("M0_TASK_AFTER`n")
$lastMsgRaw = if (Test-Path (Join-Path $OutDir "last-message.txt")) {
  [System.IO.File]::ReadAllText((Join-Path $OutDir "last-message.txt"))
} else { "<missing>" }
# Last message may carry a trailing newline from the CLI writer; the required
# exact response is TASK_DEMO_OK, so compare after trimming line endings.
Add-Check "exit-status-0" ($exitCode -eq 0) "exit=$exitCode"

$lines = Get-Content (Join-Path $OutDir "stdout.jsonl")
$jsonOk = $true; $jsonErr = ""
$i = 0
foreach ($l in $lines) {
  if ([string]::IsNullOrWhiteSpace($l)) { continue }
  $i++
  try { $null = $l | ConvertFrom-Json } catch { $jsonOk = $false; $jsonErr = "line ${i}: $($_.Exception.Message)" }
}
Add-Check "jsonl-all-lines-parse" ($jsonOk -and $i -gt 0) "lines=$i" $jsonErr

$parsed = foreach ($l in $lines) { if (-not [string]::IsNullOrWhiteSpace($l)) { $l | ConvertFrom-Json } }
$turn = @($parsed | Where-Object { $_.type -eq "turn.completed" }).Count
Add-Check "turn-completed-present" ($turn -eq 1) "count=$turn"

$lastTrimmed = $lastMsgRaw.Trim("`r`n")
Add-Check "last-message-exact" ($lastTrimmed -eq "TASK_DEMO_OK") "value='$lastTrimmed'"
$msgs = @($parsed | Where-Object { $_.type -eq "item.completed" -and $_.item.type -eq "agent_message" })
$msgText = if ($msgs.Count -eq 1) { $msgs[0].item.text.Trim("`r`n") } else { "<count=" + $msgs.Count + ">" }
Add-Check "single-agent-message-exact" ($msgs.Count -eq 1 -and $msgText -eq "TASK_DEMO_OK") "value='$msgText'"

$expected = [System.Text.Encoding]::ASCII.GetString($expectedAfter)
Add-Check "task-file-exact" ($taskFileBytes.Length -eq $expectedAfter.Length -and
  ($taskFileBytes -join ",") -eq ($expectedAfter -join ",")) "bytes='$([System.Text.Encoding]::ASCII.GetString($taskFileBytes))'"

$diffCheck = (& git -C $repo diff --check 2>&1)
Add-Check "git-diff-check" (-not $diffCheck) ((($diffCheck -join " ") -replace "`r","").Trim())

$trackedChanges = @((& git -C $repo status --porcelain=v1) | Where-Object { $_ -notmatch '^\?\?' })
$paths = @($trackedChanges | ForEach-Object { $_.Substring(3) })
Add-Check "only-task-txt-changed" ($paths.Count -eq 1 -and $paths[0] -eq "TASK.txt") ("changed=" + ($paths -join ","))

# Isolation: no tracked-file changes in the primary checkout. Untracked
# lines differ only by this run's own evidence artifacts under
# docs/evidence/m0-05-windows/ and tests/feasibility/m0-05/.
$beforeTracked = @($primaryBefore | Where-Object { $_ -notmatch '^\?\?' -and $_ -notmatch '^##' })
$afterTracked = @($primaryAfter | Where-Object { $_ -notmatch '^\?\?' -and $_ -notmatch '^##' })
$afterNew = @($primaryAfter | Where-Object { $_ -match '^\?\?' } | Where-Object { $_ -notmatch 'm0-05' })
$isoOk = ($beforeTracked -join "`n") -eq ($afterTracked -join "`n") -and $afterNew.Count -eq 0
Add-Check "primary-checkout-isolated" $isoOk "tracked identical=$($beforeTracked -eq $afterTracked); new-non-m0-05-untracked=$($afterNew -join ',')"

$allOk = (@($checks | Where-Object { $_ -match "FAIL" }).Count -eq 0)
$result = "M0-05 WINDOWS RUN: " + $(if ($allOk) { "PASS" } else { "FAIL" })
$report = @()
$report += $result
$report += ""
$report += "Run artifacts in the same directory as this file."
$report += ""
$report += "Checks:"
$report += $checks
$report += ""
$report += "Task repo (disposable, created by this run): $repo"
$report += "Task repo init commit: " + (Get-Content (Join-Path $OutDir "init-commit.txt"))
$report += "Codex exit: $exitCode"
$report += "Codex last message: '$lastMsgRaw'"
$report += "TASK.txt bytes: " + (Get-Content (Join-Path $OutDir "task-file-hex.txt"))
$report += "Primary checkout tracked status identical before/after: " + $isoOk
Save-File "result.md" (($report -join "`r`n") + "`r`n")
Write-Host $result
Write-Host ("evidence=" + $OutDir)
Write-Host ("task_repo=" + $repo)
if (-not $allOk) { exit 1 }
