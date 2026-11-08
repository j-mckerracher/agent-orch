# M0-03 Codex one-shot feasibility runner.
# Usage (PowerShell):  powershell -NoProfile -ExecutionPolicy Bypass -File tests/feasibility/m0-03/run-codex-exec.ps1 [-CodexPath <path>] [-Model <name>]
# Bare Bash equivalent for macOS:
#   codex exec --json --skip-git-repo-check --sandbox read-only --output-last-message last-message.txt "Reply exactly PONG"
# Captures stdout JSONL, stderr, exit code, and the --output-last-message file.
param(
  [string]$CodexPath = "codex",
  [string]$Model = "",
  [string]$OutDir = (Split-Path -Parent $MyInvocation.MyCommand.Path),
  [string]$Prompt = "Reply exactly PONG"
)
$ErrorActionPreference = "Continue"
$stdout = Join-Path $OutDir "run-stdout.jsonl"
$stderr = Join-Path $OutDir "run-stderr.txt"
$last   = Join-Path $OutDir "run-last-message.txt"
$args = @("exec", "--json", "--skip-git-repo-check", "--sandbox", "read-only",
  "--output-last-message", $last, "--color", "never")
if ($Model -ne "") { $args += @("-m", $Model) }
$args += $Prompt
& $CodexPath @args 2> $stderr | Out-File -FilePath $stdout -Encoding utf8
$code = $LASTEXITCODE
Set-Content -Path (Join-Path $OutDir "run-exit.txt") -Value "$code"
Write-Host "EXIT=$code"
exit $code
