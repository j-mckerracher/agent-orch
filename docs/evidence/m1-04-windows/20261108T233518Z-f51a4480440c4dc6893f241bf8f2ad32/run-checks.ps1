param(
    [Parameter(Mandatory=$true)][string] $Cargo,
    [Parameter(Mandatory=$true)][string] $Git,
    [Parameter(Mandatory=$true)][string] $SourceDir,
    [Parameter(Mandatory=$true)][string] $EvidenceDir,
    [Parameter(Mandatory=$true)][string] $BuildDir,
    [Parameter(Mandatory=$true)][string] $SourceSha
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$env:CARGO_TARGET_DIR = $BuildDir
function Invoke-CapturedCargo {
    param([string] $Name, [string[]] $Arguments, [int] $TimeoutSeconds = 1800)
    $Result = [ordered]@{
        command = 'cargo ' + ($Arguments -join ' ')
        source_commit = $SourceSha
        exit_code = $null
        timed_out = $false
        root_process_stopped = $true
        tree_kill_attempted = $false
        error = $null
    }
    $Stdout = Join-Path $EvidenceDir "$Name.stdout.txt"
    $Stderr = Join-Path $EvidenceDir "$Name.stderr.txt"
    $ResultPath = Join-Path $EvidenceDir "$Name.result.json"
    try {
        $Process = Start-Process -FilePath $Cargo -ArgumentList $Arguments -WorkingDirectory $SourceDir -NoNewWindow -PassThru -RedirectStandardOutput $Stdout -RedirectStandardError $Stderr
        $Result.root_process_stopped = $false
        if (-not $Process.WaitForExit($TimeoutSeconds * 1000)) {
            $Result.timed_out = $true
            try {
                $Result.tree_kill_attempted = $true
                $Process.Kill($true)
                if (-not $Process.WaitForExit(10000)) { $Result.error = 'The owned process did not stop.' }
            } catch {
                if (-not $Process.HasExited) { $Result.error = $_.Exception.Message }
            }
        }
        $Process.Refresh()
        if ($Process.HasExited) {
            $Result.root_process_stopped = $true
            $Result.exit_code = $Process.ExitCode
        }
    } catch { $Result.error = $_.Exception.Message }
    [IO.File]::WriteAllText($ResultPath, ($Result | ConvertTo-Json))
    [PSCustomObject]$Result
}
$Checks = @(
    @{ Name = 'workspace-tests'; Arguments = @('test','--locked','--workspace') },
    @{ Name = 'cli-cancel-tests'; Arguments = @('test','--locked','-p','orchestrator-cli','cancel','--','--nocapture') },
    @{ Name = 'store-data-lock'; Arguments = @('test','--locked','-p','orchestrator-store','--test','data_lock') },
    @{ Name = 'store-file-identity'; Arguments = @('test','--locked','-p','orchestrator-store','--lib','identity_matches_hard_links_and_distinguishes_files') },
    @{ Name = 'git-delivery'; Arguments = @('test','--locked','-p','orchestrator-git','--test','delivery') },
    @{ Name = 'git-file-identity'; Arguments = @('test','--locked','-p','orchestrator-git','--lib','identity_matches_hard_links_and_distinguishes_files') },
    @{ Name = 'agents-conformance'; Arguments = @('test','--locked','-p','orchestrator-agents','--test','conformance') },
    @{ Name = 'workspace-clippy'; Arguments = @('clippy','--locked','--workspace','--all-targets') }
)
foreach ($Check in $Checks) {
    $Result = Invoke-CapturedCargo -Name $Check.Name -Arguments $Check.Arguments
    $Result | ConvertTo-Json -Compress | Write-Output
    if (-not $Result.root_process_stopped) { throw "An owned process may still run. Preserve $EvidenceDir and report it." }
}
$After = @(& $Git -C $SourceDir status --porcelain=v1 --untracked-files=all)
if ($LASTEXITCODE -ne 0) { throw 'Cannot read final source status.' }
[IO.File]::WriteAllLines((Join-Path $EvidenceDir 'source-status-after.txt'), [string[]]$After)
& $Git -C $SourceDir diff --check 1> (Join-Path $EvidenceDir 'diff-check.stdout.txt') 2> (Join-Path $EvidenceDir 'diff-check.stderr.txt')
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'diff-check.exit.txt'), "$LASTEXITCODE`n")
