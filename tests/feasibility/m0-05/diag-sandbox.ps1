# M0-05 Windows sandbox diagnostic.
# Determines whether Codex can spawn helper processes in each sandbox mode
# on this host, in a fresh temp repo. Preserves per-case JSONL/stderr/exit.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File tests/feasibility/m0-05/diag-sandbox.ps1 `
#     -CodexPath "<codex.exe>" -OutDir "<evidence dir>"
param(
  [Parameter(Mandatory=$true)][string]$CodexPath,
  [Parameter(Mandatory=$true)][string]$OutDir
)
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
$cases = @("read-only", "workspace-write", "danger-full-access")
foreach ($case in $cases) {
  $dir = Join-Path $OutDir $case
  New-Item -ItemType Directory -Path $dir -Force | Out-Null
  $repo = Join-Path $env:TEMP ("agent-orch-m0-05-diag-" + [guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Path $repo | Out-Null
  & git -C $repo init -q -b main | Out-Null
  & git -C $repo config core.autocrlf false
  & git -C $repo config core.eol lf
  [System.IO.File]::WriteAllText((Join-Path $repo "TASK.txt"), "M0_TASK_BEFORE`n")
  & git -C $repo add TASK.txt
  & git -C $repo commit -q -m "init"
  $prompt = 'Run exactly this one shell command and report its exit code verbatim: echo SANDBOX_PROBE_OK. Do not write any file. Reply with only the exit code.'
  $args = @("exec", "--json", "--color", "never", "-C", $repo, "--sandbox", $case, $prompt)
  $prev = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    $null | & $CodexPath @args 2> (Join-Path $dir "stderr.txt") |
      Out-File -FilePath (Join-Path $dir "stdout.jsonl") -Encoding utf8
    $code = $LASTEXITCODE
  } finally { $ErrorActionPreference = $prev }
  $sb = [System.IO.File]::ReadAllBytes((Join-Path $dir "stdout.jsonl"))
  if ($sb.Length -ge 3 -and $sb[0] -eq 0xEF -and $sb[1] -eq 0xBB -and $sb[2] -eq 0xBF) {
    [System.IO.File]::WriteAllBytes((Join-Path $dir "stdout.jsonl"), $sb[3..($sb.Length - 1)])
  }
  Set-Content -Path (Join-Path $dir "exit.txt") -Value "$code" -NoNewline
  Write-Host "$case exit=$code"
}
