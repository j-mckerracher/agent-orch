# M1-04 Windows stale-owner verification runbook (candidate 9f850d27)

## Status of this run

This run is **not final integrated acceptance evidence**. The candidate is
provisional and is under independent review. The run records how the
candidate behaves on native Windows. It does not close M1-04.

The earlier Windows run of `03d6b139` was inconclusive. It cannot establish
behavior for this candidate. Do not reuse its results.

## Assignment

Run the checks below on native Windows. Publish the evidence on a separate
branch. Do not edit the candidate source or `board.json`.

Use a GitHub Copilot subagent configured for Claude Sonnet 5.5 with high
reasoning. The user requires this setting for all subagents. It replaces the
worker model setting in the primer for this run. Do not start an OMP worker.
Read the
[orchestration primer](https://github.com/j-mckerracher/agent-orch/blob/9f850d27677b8c33ce3743b11fca09ae3ea6d50e/orch-impl-m.md)
and the
[implementation plan](https://github.com/j-mckerracher/agent-orch/blob/9f850d27677b8c33ce3743b11fca09ae3ea6d50e/docs/technical-implementation-plan.md).
Follow the primer's skill requirements. Report any unavailable guidance and
any setting that you cannot observe as unverified.

## Candidate

- Repository: `https://github.com/j-mckerracher/agent-orch.git`
- Source branch: `orch/m1-04-windows-owner-9f850d27`
- Source commit: `9f850d27677b8c33ce3743b11fca09ae3ea6d50e`
- Runbook location: the coordinator gives the branch and URL in the dispatch
  message.

Test only the exact source commit. If the branch does not contain it, stop
and report the mismatch. If the branch tip is a later commit, still test the
pinned commit and report the tip.

## Selected tests

The runbook author read the names and platform gates in the source at the
pinned commit. Nobody compiled these tests on Windows before this run.

| Command | Tests (`--exact`) | Windows gate |
|---|---|---|
| `orchestrator-store --test cancellation_owner` | `windows_owner_creation_time_precision_survives_reopen`, `stale_owner_recovery_requires_the_recorded_process_identity`, `stale_generation_is_retired_before_retry_owner_can_register` | None. These tests use synthetic identities and store logic. They do not capture a real Windows process. |
| `orchestrator-runtime --test reconcile` | `windows_owner_observation_uses_creation_time_not_only_pid` | `#[cfg(windows)]`. It captures and observes this real process. It runs zero tests on other systems. |
| `orchestrator-runtime --test reconcile` | `windows_identity_compares_creation_time_at_100ns_precision` | None. It compares synthetic identities. |
| `orchestrator-runtime --test reconcile` | `owner_observation_requires_a_creation_time`, `owner_observation_classifies_alive_and_reused_pid_by_creation_time` | None. These tests capture and observe this real process. |
| `orchestrator-store --test data_lock` | `dead_pid_lock_is_taken_over_and_new_holder_is_recorded`, `live_legacy_lock_is_refused_with_clear_error`, `live_pid_with_different_start_identity_is_taken_over` | None. |

The `data_lock` test `lock_child_process_helper` is `#[ignore]` by design.
Its parent test starts it. It is not a selected test.

**No Windows inherited-child test exists.** The reported inherited-child
test is `cancellation_owner_reconciliation_ignores_live_inherited_group_parent`
in `orchestrator-cli`. It is `#[cfg(unix)]`. Do not run it on Windows. Do not
claim Windows coverage for it. Report it as a coverage gap.

The command set also runs each test binary in full. These runs show
regressions beside the selected tests. They do not replace the exact runs.

## Safety rules

- Use a new clone in a unique directory under `$env:TEMP`.
- Do not change the user's primary checkout.
- Do not install tools or change credentials.
- Do not use the agent bus or copy repository code to another repository.
- Do not change source files, manifests, lockfiles, or `board.json`.
- Do not delete worktrees, directories, or failed attempts.
- Do not use `git reset`, `git clean`, `git push --force`, or `git merge`.
- Do not publish credentials, agent memory, session files, or unrelated files.

## 1. Prepare the checkout

Use native Windows PowerShell 7 or later. Open a new `pwsh` session. Run each
block in that session. Keep the session open for all steps.

```powershell
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if (-not $IsWindows -or $PSVersionTable.PSVersion.Major -lt 7) {
    throw 'Native Windows and PowerShell 7 or later are required.'
}

$Git = (Get-Command git.exe -CommandType Application).Source
$Cargo = (Get-Command cargo.exe -CommandType Application).Source
$Rustc = (Get-Command rustc.exe -CommandType Application).Source
$Repository = 'https://github.com/j-mckerracher/agent-orch.git'
$SourceBranch = 'orch/m1-04-windows-owner-9f850d27'
$SourceSha = '9f850d27677b8c33ce3743b11fca09ae3ea6d50e'
$RunId = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ') + '-' +
    [Guid]::NewGuid().ToString('N')
$RunRoot = Join-Path $env:TEMP "agent-orch-m1-04-windows-$RunId"
$SourceDir = Join-Path $RunRoot 'source'
$EvidenceDir = Join-Path $RunRoot 'evidence'
$BuildDir = Join-Path $RunRoot 'cargo-target'

if (Test-Path -LiteralPath $RunRoot) {
    throw "Run directory already exists: $RunRoot"
}
New-Item -ItemType Directory -Path $RunRoot | Out-Null
New-Item -ItemType Directory -Path $EvidenceDir | Out-Null
$env:CARGO_TARGET_DIR = $BuildDir
$env:CARGO_TERM_COLOR = 'never'
$env:RUST_BACKTRACE = '1'

function Write-Exit([string] $Name, [int] $Code) {
    [IO.File]::WriteAllText((Join-Path $EvidenceDir "$Name.exit.txt"), "$Code`n")
}

& $Git clone --no-checkout --single-branch --branch $SourceBranch `
    $Repository $SourceDir `
    1> (Join-Path $EvidenceDir 'clone.stdout.txt') `
    2> (Join-Path $EvidenceDir 'clone.stderr.txt')
$Code = $LASTEXITCODE
Write-Exit 'clone' $Code
if ($Code -ne 0) { throw "Clone failed. Preserve $RunRoot and report it." }

$BranchTip = & $Git -C $SourceDir rev-parse "refs/remotes/origin/$SourceBranch"
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'branch-tip.txt'), "$BranchTip`n")
& $Git -C $SourceDir merge-base --is-ancestor $SourceSha `
    "refs/remotes/origin/$SourceBranch"
$Code = $LASTEXITCODE
Write-Exit 'candidate-containment' $Code
if ($Code -ne 0) { throw 'The branch does not contain the source commit.' }

& $Git -C $SourceDir switch --detach $SourceSha `
    1> (Join-Path $EvidenceDir 'switch.stdout.txt') `
    2> (Join-Path $EvidenceDir 'switch.stderr.txt')
$Code = $LASTEXITCODE
Write-Exit 'switch' $Code
if ($Code -ne 0) { throw "Checkout failed. Preserve $RunRoot and report it." }

$ActualSha = & $Git -C $SourceDir rev-parse HEAD
if ($LASTEXITCODE -ne 0 -or $ActualSha -ne $SourceSha) {
    throw "Source commit mismatch. Preserve $RunRoot and report it."
}
$Before = @(& $Git -C $SourceDir status --porcelain=v1 --untracked-files=all)
if ($LASTEXITCODE -ne 0) { throw 'Cannot read source status.' }
[IO.File]::WriteAllLines(
    (Join-Path $EvidenceDir 'source-status-before.txt'), [string[]]$Before
)
if ($Before.Count -ne 0) { throw 'Source checkout is not clean.' }
```

If Git, Cargo, Rust, or PowerShell is missing, report the missing tool. Do not
install it. Preserve any output that exists. A setup failure gives the result
`BLOCKED`.

## 2. Record the environment

Record the operating system, CPU, toolchain, and source commit. Do not record
the complete environment or any credentials.

```powershell
cmd.exe /c ver > (Join-Path $EvidenceDir 'windows-version.txt')
Get-CimInstance Win32_OperatingSystem |
    Select-Object Caption, Version, BuildNumber, OSArchitecture |
    Format-List | Out-File -LiteralPath (Join-Path $EvidenceDir 'os.txt') -Encoding utf8
Get-CimInstance Win32_Processor |
    Select-Object Name, Manufacturer, NumberOfCores, NumberOfLogicalProcessors |
    Format-List | Out-File -LiteralPath (Join-Path $EvidenceDir 'cpu.txt') -Encoding utf8
[IO.File]::WriteAllText(
    (Join-Path $EvidenceDir 'powershell-version.txt'),
    $PSVersionTable.PSVersion.ToString()
)
[IO.File]::WriteAllLines(
    (Join-Path $EvidenceDir 'tool-paths.txt'), [string[]]@($Git, $Cargo, $Rustc)
)
& $Git --version > (Join-Path $EvidenceDir 'git-version.txt')
$GitExit = $LASTEXITCODE
& $Rustc -Vv > (Join-Path $EvidenceDir 'rustc-version.txt')
$RustcExit = $LASTEXITCODE
& $Cargo --version > (Join-Path $EvidenceDir 'cargo-version.txt')
$CargoExit = $LASTEXITCODE
[IO.File]::WriteAllText(
    (Join-Path $EvidenceDir 'tool-exits.txt'),
    "git=$GitExit`nrustc=$RustcExit`ncargo=$CargoExit`n"
)
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'source-sha.txt'), "$ActualSha`n")
if ($GitExit -ne 0 -or $RustcExit -ne 0 -or $CargoExit -ne 0) {
    throw "Toolchain check failed. Preserve $RunRoot and report it."
}
```

## 3. Run the checks

The helper records the stdout, stderr, exit code, and timeout of each command.
Each command has a 30-minute limit. A timeout is a failure, even if the
process later exits with code zero. Run each command once. Do not retry a
failed command in this evidence directory.

```powershell
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

    try {
        $Process = Start-Process -FilePath $Cargo -ArgumentList $Arguments `
            -WorkingDirectory $SourceDir -NoNewWindow -PassThru `
            -RedirectStandardOutput $Stdout -RedirectStandardError $Stderr
        $Result.root_process_stopped = $false
        if (-not $Process.WaitForExit($TimeoutSeconds * 1000)) {
            $Result.timed_out = $true
            try {
                $Result.tree_kill_attempted = $true
                $Process.Kill($true)
                if (-not $Process.WaitForExit(10000)) {
                    $Result.error = 'The owned process did not stop.'
                }
            } catch {
                if (-not $Process.HasExited) { $Result.error = $_.Exception.Message }
            }
        }
        $Process.Refresh()
        if ($Process.HasExited) {
            $Result.root_process_stopped = $true
            $Result.exit_code = $Process.ExitCode
        }
    } catch {
        $Result.error = $_.Exception.Message
    }

    [IO.File]::WriteAllText(
        (Join-Path $EvidenceDir "$Name.result.json"), ($Result | ConvertTo-Json)
    )
    return [PSCustomObject]$Result
}

$StoreOwner = @(
    'windows_owner_creation_time_precision_survives_reopen',
    'stale_owner_recovery_requires_the_recorded_process_identity',
    'stale_generation_is_retired_before_retry_owner_can_register'
)
$RuntimeReconcile = @(
    'windows_owner_observation_uses_creation_time_not_only_pid',
    'windows_identity_compares_creation_time_at_100ns_precision',
    'owner_observation_requires_a_creation_time',
    'owner_observation_classifies_alive_and_reused_pid_by_creation_time'
)
$StoreLock = @(
    'dead_pid_lock_is_taken_over_and_new_holder_is_recorded',
    'live_legacy_lock_is_refused_with_clear_error',
    'live_pid_with_different_start_identity_is_taken_over'
)

# Expect lists exact tests that must report ok. MaxIgnored is the most
# ignored tests that the run may report.
$Checks = @(
    @{ Name = 'store-owner-exact'; Expect = $StoreOwner; MaxIgnored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-store',
           '--test', 'cancellation_owner', '--', '--exact') + $StoreOwner +
           @('--nocapture') },
    @{ Name = 'runtime-reconcile-exact'; Expect = $RuntimeReconcile; MaxIgnored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-runtime',
           '--test', 'reconcile', '--', '--exact') + $RuntimeReconcile +
           @('--nocapture') },
    @{ Name = 'store-lock-exact'; Expect = $StoreLock; MaxIgnored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-store',
           '--test', 'data_lock', '--', '--exact') + $StoreLock +
           @('--nocapture') },
    @{ Name = 'store-owner-all'; Expect = @(); MaxIgnored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-store',
           '--test', 'cancellation_owner', '--', '--nocapture') },
    @{ Name = 'runtime-reconcile-all'; Expect = @(); MaxIgnored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-runtime',
           '--test', 'reconcile', '--', '--nocapture') },
    @{ Name = 'store-lock-all'; Expect = @(); MaxIgnored = 1
       Arguments = @('test', '--locked', '-p', 'orchestrator-store',
           '--test', 'data_lock', '--', '--nocapture') }
)

$Rank = @{ PASS = 0; INCONCLUSIVE = 1; FAIL = 2 }

function Get-CheckVerdict($Check, $Result) {
    $State = @{ Status = 'PASS'; Notes = [Collections.Generic.List[string]]::new() }
    $Raise = {
        param($Level, $Note)
        if ($Rank[$Level] -gt $Rank[$State.Status]) { $State.Status = $Level }
        $State.Notes.Add($Note)
    }
    $Passed = $null; $Failed = $null; $Ignored = $null

    if ($Result.timed_out -or $Result.error -or $null -eq $Result.exit_code) {
        & $Raise 'FAIL' 'Timeout or process error.'
    } elseif ($Result.exit_code -ne 0) {
        & $Raise 'FAIL' "Exit code $($Result.exit_code)."
    } else {
        $Out = [IO.File]::ReadAllText((Join-Path $EvidenceDir "$($Check.Name).stdout.txt"))
        $Summary = [regex]::Matches($Out,
            '(?m)^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored')
        if ($Summary.Count -ne 1) {
            & $Raise 'INCONCLUSIVE' "Found $($Summary.Count) test summaries. Expected 1."
        } else {
            $Passed = [int]$Summary[0].Groups[2].Value
            $Failed = [int]$Summary[0].Groups[3].Value
            $Ignored = [int]$Summary[0].Groups[4].Value
            if ($Failed -gt 0) { & $Raise 'FAIL' "$Failed test(s) failed." }
            if ($Passed -eq 0) { & $Raise 'INCONCLUSIVE' 'Zero tests ran.' }
            if ($Ignored -gt $Check.MaxIgnored) {
                & $Raise 'INCONCLUSIVE' "$Ignored ignored. At most $($Check.MaxIgnored) is allowed."
            }
            foreach ($Test in $Check.Expect) {
                $Pattern = '(?m)^test ' + [regex]::Escape($Test) + ' \.\.\. ok\s*$'
                if (-not [regex]::IsMatch($Out, $Pattern)) {
                    & $Raise 'INCONCLUSIVE' "No 'ok' line for $Test."
                }
            }
            if ($Check.Expect.Count -gt 0 -and $Passed -ne $Check.Expect.Count) {
                & $Raise 'INCONCLUSIVE' "$Passed passed. Expected $($Check.Expect.Count)."
            }
        }
    }
    [PSCustomObject]@{
        name = $Check.Name; status = $State.Status; exit_code = $Result.exit_code
        timed_out = $Result.timed_out; passed = $Passed; failed = $Failed
        ignored = $Ignored; notes = ($State.Notes -join ' ')
    }
}

$Verdicts = @()
foreach ($Check in $Checks) {
    $Run = Invoke-CapturedCargo -Name $Check.Name -Arguments $Check.Arguments
    if (-not $Run.root_process_stopped) {
        throw "An owned process may still run. Preserve $RunRoot and report it."
    }
    $Verdicts += Get-CheckVerdict $Check $Run
}

$After = @(& $Git -C $SourceDir status --porcelain=v1 --untracked-files=all)
$StatusExit = $LASTEXITCODE
[IO.File]::WriteAllLines(
    (Join-Path $EvidenceDir 'source-status-after.txt'), [string[]]$After
)
& $Git -C $SourceDir diff --check `
    1> (Join-Path $EvidenceDir 'diff-check.stdout.txt') `
    2> (Join-Path $EvidenceDir 'diff-check.stderr.txt')
$DiffExit = $LASTEXITCODE
Write-Exit 'diff-check' $DiffExit
```

The helper checks the Cargo process only. It does not prove that every
descendant stopped. The wrapper above never converts a command failure into
success. It records the status of each command from its own exit code and
its own output.

Status rules for each command:

- `FAIL`: timeout, process error, non-zero exit code, or a failed test.
- `INCONCLUSIVE`: zero tests ran, or a selected test has no `ok` line.
  Also: the run ignored too many tests, the passed count differs from the
  selection, or the test summary is missing or repeated. A skipped or
  zero-test selection is never a pass.
- `PASS`: exit code zero, every selected test reports `ok`, and zero tests
  failed or are ignored. The `store-lock-all` run may report one ignored
  test, `lock_child_process_helper`.

## 4. Write the result

```powershell
$Overall = 'PASS'
foreach ($Verdict in $Verdicts) {
    if ($Rank[$Verdict.status] -gt $Rank[$Overall]) { $Overall = $Verdict.status }
}
$Extra = [Collections.Generic.List[string]]::new()
if ($StatusExit -ne 0 -or $After.Count -ne 0) {
    $Overall = 'FAIL'; $Extra.Add('Source status after the run is not clean.')
}
if ($DiffExit -ne 0) { $Overall = 'FAIL'; $Extra.Add('git diff --check failed.') }

$Table = ($Verdicts | ForEach-Object {
    "| $($_.name) | $($_.status) | $($_.exit_code) | $($_.timed_out) | " +
    "$($_.passed) | $($_.failed) | $($_.ignored) | $($_.notes) |"
}) -join "`n"
$Report = @"
# M1-04 Windows verification of 9f850d27

This run is not final integrated acceptance evidence. The candidate is
provisional and under independent review.

- Run ID: $RunId
- UTC date: $([DateTime]::UtcNow.ToString('o'))
- Source branch: $SourceBranch
- Branch tip at clone time: $BranchTip
- Source commit: $SourceSha
- Overall result: $Overall $($Extra -join ' ')
- OS, CPU, PowerShell, Git, Rust, Cargo: see os.txt, cpu.txt,
  powershell-version.txt, git-version.txt, rustc-version.txt, cargo-version.txt

| Command | Status | Exit | Timed out | Passed | Failed | Ignored | Notes |
|---|---|---|---|---|---|---|---|
$Table

- Windows inherited-child coverage: none. The reported test is Unix-gated.
- Windows-gated test windows_owner_observation_uses_creation_time_not_only_pid:
  see the runtime-reconcile-exact row.
- Orchestrator-store owner tests use synthetic identities. They do not
  capture a Windows process.
"@
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'result.md'), $Report)
[IO.File]::WriteAllText(
    (Join-Path $EvidenceDir 'verdicts.json'), ($Verdicts | ConvertTo-Json)
)
Write-Output "overall=$Overall"
```

Use PASS only when every command is PASS, both source status checks are empty,
and the diff check exits zero. A PASS is provisional. It shows that the
selected tests passed on this host. It does not show Windows coverage for any
scenario that no test ran.

If a command fails, preserve its complete output and report its exit code. If
a process does not stop, report it and do not start another attempt. For a
retry, use a new run ID and preserve both attempts.

## 5. Publish evidence only

Publish the evidence for every result, including FAIL and INCONCLUSIVE. Publish
only flat text files from `$EvidenceDir`. Do not publish `cargo-target`, the
source clone, session files, agent memory, or any other path.

```powershell
$EvidenceBranch = "evidence/m1-04-windows-9f850d27-$RunId"
$EvidenceWorktree = Join-Path $RunRoot 'evidence-worktree'
$EvidencePath = "docs/evidence/m1-04-windows/9f850d27-$RunId"

$Files = @(Get-ChildItem -LiteralPath $EvidenceDir -Force)
foreach ($File in $Files) {
    if ($File.PSIsContainer -or $File.Name -notmatch '^[a-z0-9][a-z0-9.-]*\.(txt|json|md)$') {
        throw "Unexpected evidence entry: $($File.Name)"
    }
}
if (-not (Test-Path -LiteralPath (Join-Path $EvidenceDir 'result.md') -PathType Leaf)) {
    throw 'Create the result summary before publishing evidence.'
}

$SecretPattern = 'ghp_[A-Za-z0-9]{20,}|github_pat_|gho_[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}|sk-[A-Za-z0-9]{20,}|-----BEGIN [A-Z ]*PRIVATE KEY|(?i)(password|passwd|secret|api[_-]?key|authorization:|bearer)\s*[:=]'
$Hits = @(Select-String -Path (Join-Path $EvidenceDir '*') -Pattern $SecretPattern)
if ($Hits.Count -ne 0) {
    $Hits | Select-Object Path, LineNumber | Format-Table | Out-String | Write-Output
    throw 'Possible secret found. Do not push. Keep the raw files and report it.'
}
```

The pattern scan is a backstop. Also read every file before you publish. If a
file holds a secret, do not push it. Keep the raw file and report the blocker.

```powershell
& $Git -C $SourceDir fetch origin 'refs/heads/main:refs/remotes/origin/main'
if ($LASTEXITCODE -ne 0) { throw "Fetch failed. Preserve $RunRoot and report it." }
& $Git -C $SourceDir worktree add -b $EvidenceBranch $EvidenceWorktree refs/remotes/origin/main
if ($LASTEXITCODE -ne 0) { throw "Worktree creation failed. Preserve $RunRoot and report it." }

$Destination = Join-Path $EvidenceWorktree $EvidencePath
if (Test-Path -LiteralPath $Destination) { throw "Evidence path exists: $Destination" }
[IO.Directory]::CreateDirectory($Destination) | Out-Null
foreach ($File in $Files) {
    Copy-Item -LiteralPath $File.FullName -Destination (Join-Path $Destination $File.Name)
}

& $Git -C $EvidenceWorktree add -- $EvidencePath
if ($LASTEXITCODE -ne 0) { throw 'Evidence staging failed.' }
$Staged = @(& $Git -C $EvidenceWorktree diff --cached --name-only)
if ($LASTEXITCODE -ne 0 -or $Staged.Count -eq 0) { throw 'Cannot read staged paths.' }
foreach ($Path in $Staged) {
    if (-not $Path.StartsWith("$EvidencePath/", [StringComparison]::Ordinal)) {
        throw "Unexpected staged path: $Path"
    }
}
& $Git -C $EvidenceWorktree commit `
    -m "evidence: record M1-04 Windows run of 9f850d27 ($Overall)" `
    -m 'Co-authored-by: Copilot App <223556219+Copilot@users.noreply.github.com>'
if ($LASTEXITCODE -ne 0) { throw 'Evidence commit failed.' }
$EvidenceSha = & $Git -C $EvidenceWorktree rev-parse HEAD
$Changed = @(& $Git -C $EvidenceWorktree diff --name-only refs/remotes/origin/main HEAD)
foreach ($Path in $Changed) {
    if (-not $Path.StartsWith("$EvidencePath/", [StringComparison]::Ordinal)) {
        throw "Commit holds an unexpected path: $Path"
    }
}
& $Git -C $EvidenceWorktree push origin "HEAD:refs/heads/$EvidenceBranch"
if ($LASTEXITCODE -ne 0) { throw "Push failed. Preserve $RunRoot and report it." }
Write-Output "evidence_branch=$EvidenceBranch"
Write-Output "evidence_commit=$EvidenceSha"
Write-Output "overall=$Overall"
if ($Overall -ne 'PASS') {
    throw "Result is $Overall. Evidence is published. Report the failure."
}
```

The final `throw` makes a non-PASS result visible in the session and gives a
non-zero exit code if you run the block as a script. Report to the coordinator:

- the evidence branch and commit
- the overall result and the status of each command
- the OS, CPU, Rust, Cargo, and Git versions
- the branch tip and the pinned source commit
- the Windows inherited-child gap
- any setting that you could not observe

Do not edit `board.json`. The coordinator reviews the evidence and updates the
board.
