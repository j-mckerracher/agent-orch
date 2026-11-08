# M1-04 native Windows verification runbook

## Assignment

Run the checks below on native Windows. Publish the evidence in a separate
branch. Do not edit the candidate source or `board.json`.

This is a **preliminary** candidate. The Windows cancellation path cannot yet
reach the running CLI owner. An earlier review also found unverified lock
identity error handling. A passing test command does not close M1-04.

Use a GitHub Copilot subagent configured for `gpt-6-luna` and `max`.
Do not start an OMP worker. Read the
[orchestration primer](https://github.com/j-mckerracher/agent-orch/blob/03d6b1394a91bdb51f2213dc5be47edad62548c7/orch-impl-m.md)
and the
[implementation plan](https://github.com/j-mckerracher/agent-orch/blob/03d6b1394a91bdb51f2213dc5be47edad62548c7/docs/technical-implementation-plan.md).
Follow the primer's skill requirements. Report unavailable guidance.

The user permits the configured model and effort when independent runtime
verification is unavailable. The user also permits the loaded Ponytail skill
when hook state cannot be observed. Report any unobserved setting as
unverified.

## Candidate

- Repository: `https://github.com/j-mckerracher/agent-orch.git`
- Source branch: `orch/m1-04-windows-candidate-03d6b139`
- Source commit: `03d6b1394a91bdb51f2213dc5be47edad62548c7`
- Runbook branch: `mckerracher-joshua-mclm-board-execution`
- Runbook path: `docs/evidence/m1-04-windows-runbook.md`
- Runbook URL: <https://github.com/j-mckerracher/agent-orch/blob/mckerracher-joshua-mclm-board-execution/docs/evidence/m1-04-windows-runbook.md>

Test only the exact source commit above. If the branch does not contain that
commit, stop and report the mismatch. Do not substitute another commit.

## Safety rules

- Use a new clone in a unique temporary directory.
- Do not change the user's primary checkout.
- Do not install tools or change credentials.
- Do not use the agent bus or copy repository code to another repository.
- Do not change source files, manifests, lockfiles, or `board.json`.
- Do not delete worktrees or failed attempts.
- Do not force-push or merge any branch.
- Do not publish credentials or other secrets in evidence.

## 1. Prepare the checkout

Use native Windows PowerShell 7 or later. Open a new `pwsh` session. Run this
block in that session. Keep the session open for the remaining steps.

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
$SourceBranch = 'orch/m1-04-windows-candidate-03d6b139'
$SourceSha = '03d6b1394a91bdb51f2213dc5be47edad62548c7'
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

& $Git clone --no-checkout --single-branch --branch $SourceBranch `
    $Repository $SourceDir `
    1> (Join-Path $EvidenceDir 'clone.stdout.txt') `
    2> (Join-Path $EvidenceDir 'clone.stderr.txt')
$CloneExit = $LASTEXITCODE
[IO.File]::WriteAllText(
    (Join-Path $EvidenceDir 'clone.exit.txt'), "$CloneExit`n"
)
if ($CloneExit -ne 0) {
    throw "Clone failed. Preserve $RunRoot and report the error."
}

& $Git -C $SourceDir merge-base --is-ancestor $SourceSha `
    "refs/remotes/origin/$SourceBranch"
$ContainmentExit = $LASTEXITCODE
[IO.File]::WriteAllText(
    (Join-Path $EvidenceDir 'candidate-containment.exit.txt'),
    "$ContainmentExit`n"
)
if ($ContainmentExit -ne 0) {
    throw "Candidate branch does not contain the source commit."
}

& $Git -C $SourceDir switch --detach $SourceSha `
    1> (Join-Path $EvidenceDir 'switch.stdout.txt') `
    2> (Join-Path $EvidenceDir 'switch.stderr.txt')
$SwitchExit = $LASTEXITCODE
[IO.File]::WriteAllText(
    (Join-Path $EvidenceDir 'switch.exit.txt'), "$SwitchExit`n"
)
if ($SwitchExit -ne 0) {
    throw "Checkout failed. Preserve $RunRoot and report the error."
}

$ActualSha = & $Git -C $SourceDir rev-parse HEAD
if ($LASTEXITCODE -ne 0 -or $ActualSha -ne $SourceSha) {
    throw "Source commit mismatch. Preserve $RunRoot and report it."
}
$Before = @(& $Git -C $SourceDir status --porcelain=v1 --untracked-files=all)
if ($LASTEXITCODE -ne 0) {
    throw "Cannot read source status. Preserve $RunRoot and report it."
}
[IO.File]::WriteAllLines(
    (Join-Path $EvidenceDir 'source-status-before.txt'), [string[]]$Before
)
if ($Before.Count -ne 0) {
    throw "Source checkout is not clean. Preserve $RunRoot and report it."
}
```

If Git, Cargo, Rust, or PowerShell is unavailable, report the missing tool.
Do not install it. Preserve any output that already exists.

## 2. Record the environment

Record the native operating system, architecture, toolchain, and source
commit. Do not record the complete environment or any credentials.

```powershell
cmd.exe /c ver > (Join-Path $EvidenceDir 'windows-version.txt')
[IO.File]::WriteAllText(
    (Join-Path $EvidenceDir 'architecture.txt'),
    [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
)
[IO.File]::WriteAllText(
    (Join-Path $EvidenceDir 'powershell-version.txt'),
    $PSVersionTable.PSVersion.ToString()
)
& $Git --version > (Join-Path $EvidenceDir 'git-version.txt')
& $Rustc -Vv > (Join-Path $EvidenceDir 'rustc-version.txt') `
    2> (Join-Path $EvidenceDir 'rustc-version.stderr.txt')
$RustcExit = $LASTEXITCODE
& $Cargo --version > (Join-Path $EvidenceDir 'cargo-version.txt') `
    2> (Join-Path $EvidenceDir 'cargo-version.stderr.txt')
$CargoExit = $LASTEXITCODE
[IO.File]::WriteAllText(
    (Join-Path $EvidenceDir 'tool-exits.txt'),
    "rustc=$RustcExit`ncargo=$CargoExit`n"
)
[IO.File]::WriteAllText(
    (Join-Path $EvidenceDir 'source-sha.txt'), "$ActualSha`n"
)
if ($RustcExit -ne 0 -or $CargoExit -ne 0) {
    throw "Toolchain check failed. Preserve $RunRoot and report it."
}
```

## 3. Run the checks

The helper records each command's stdout, stderr, exit code, and timeout.
Each command has a 30-minute limit. A timeout is a failure, even if the
process later exits with code zero.

```powershell
function Invoke-CapturedCargo {
    param(
        [string] $Name,
        [string[]] $Arguments,
        [int] $TimeoutSeconds = 1800
    )

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
                if (-not $Process.HasExited) {
                    $Result.error = $_.Exception.Message
                }
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

    [IO.File]::WriteAllText($ResultPath, ($Result | ConvertTo-Json))
    return [PSCustomObject]$Result
}

$Checks = @(
    @{ Name = 'workspace-tests'; Arguments = @(
        'test', '--locked', '--workspace'
    ) },
    @{ Name = 'cli-cancel-tests'; Arguments = @(
        'test', '--locked', '-p', 'orchestrator-cli', 'cancel', '--',
        '--nocapture'
    ) },
    @{ Name = 'store-data-lock'; Arguments = @(
        'test', '--locked', '-p', 'orchestrator-store', '--test', 'data_lock'
    ) },
    @{ Name = 'store-file-identity'; Arguments = @(
        'test', '--locked', '-p', 'orchestrator-store', '--lib',
        'identity_matches_hard_links_and_distinguishes_files'
    ) },
    @{ Name = 'git-delivery'; Arguments = @(
        'test', '--locked', '-p', 'orchestrator-git', '--test', 'delivery'
    ) },
    @{ Name = 'git-file-identity'; Arguments = @(
        'test', '--locked', '-p', 'orchestrator-git', '--lib',
        'identity_matches_hard_links_and_distinguishes_files'
    ) },
    @{ Name = 'agents-conformance'; Arguments = @(
        'test', '--locked', '-p', 'orchestrator-agents', '--test', 'conformance'
    ) },
    @{ Name = 'workspace-clippy'; Arguments = @(
        'clippy', '--locked', '--workspace', '--all-targets'
    ) }
)

foreach ($Check in $Checks) {
    $Result = Invoke-CapturedCargo -Name $Check.Name `
        -Arguments $Check.Arguments
    if (-not $Result.root_process_stopped) {
        throw "An owned process may still run. Preserve $RunRoot and report it."
    }
}

$After = @(& $Git -C $SourceDir status --porcelain=v1 --untracked-files=all)
if ($LASTEXITCODE -ne 0) {
    throw "Cannot read final source status. Preserve $RunRoot and report it."
}
[IO.File]::WriteAllLines(
    (Join-Path $EvidenceDir 'source-status-after.txt'), [string[]]$After
)
& $Git -C $SourceDir diff --check `
    1> (Join-Path $EvidenceDir 'diff-check.stdout.txt') `
    2> (Join-Path $EvidenceDir 'diff-check.stderr.txt')
[IO.File]::WriteAllText(
    (Join-Path $EvidenceDir 'diff-check.exit.txt'), "$LASTEXITCODE`n"
)
```

Run each command once for this attempt. Preserve all failures and timeouts.
Do not repeat a failed command in the same evidence directory.
The helper checks the Cargo process only. It does not prove that every
descendant stopped.

For each command, record its exit code and test totals. A zero exit code
with zero matching tests does not prove a behavior works. This candidate does not contain a dedicated Windows CLI cancellation test.
The existing CLI cancellation tests are macOS-only. Record that gap.
Identify the tests that exercise lock identity errors. Record any missing
coverage.

## 4. Write the result

Create `result.md` inside `$EvidenceDir`. Use this format:

```markdown
# M1-04 Windows verification

- Run ID:
- UTC date:
- Windows version and architecture:
- PowerShell, Git, Rust, and Cargo versions:
- Source branch:
- Source commit: `03d6b1394a91bdb51f2213dc5be47edad62548c7`
- Overall result: PASS, FAIL, BLOCKED, or INCONCLUSIVE
- Workspace test exit, timeout, and test totals:
- CLI cancellation test exit, timeout, and test totals:
- Store data-lock test exit, timeout, and test totals:
- Store file-identity test exit, timeout, and test totals:
- Git delivery test exit, timeout, and test totals:
- Git file-identity test exit, timeout, and test totals:
- Agent conformance test exit, timeout, and test totals:
- Clippy exit and timeout:
- Windows cancellation and descendant-cleanup coverage:
- Windows lock identity error coverage:
- Source status before and after:
- Failures, missing coverage, and other limitations:
```

Use PASS only when every required command exits zero without a timeout.
Both source status files must be empty. The diff check must exit zero.
Use INCONCLUSIVE if a required Windows scenario has no executed test.
Record any ignored or missing tests. A passing command does not establish
coverage for a scenario that it did not run.

If a command fails, preserve its complete output. Report its actual exit code.
If a process does not stop, report it and do not start another attempt.
For a retry, use a new run ID and preserve both attempts.

## 5. Publish evidence only

Inspect every evidence file for credentials and other secrets. If a file
contains a secret, do not push it. Keep the raw file locally and report the
blocker.

Create an evidence branch from the current `origin/main`. Stage only the
unique evidence directory. Do not stage source changes.

```powershell
$EvidenceBranch = "evidence/m1-04-windows-03d6b139-$RunId"
$EvidenceWorktree = Join-Path $RunRoot 'evidence-worktree'
$EvidencePath = "docs/evidence/m1-04-windows/$RunId"

if (-not (Test-Path -LiteralPath (Join-Path $EvidenceDir 'result.md') `
        -PathType Leaf)) {
    throw 'Create the result summary before publishing evidence.'
}
& $Git -C $SourceDir fetch origin `
    'refs/heads/main:refs/remotes/origin/main'
if ($LASTEXITCODE -ne 0) {
    throw "Fetch failed. Preserve $RunRoot and report it."
}
& $Git -C $SourceDir worktree add -b $EvidenceBranch `
    $EvidenceWorktree refs/remotes/origin/main
if ($LASTEXITCODE -ne 0) {
    throw "Evidence worktree creation failed. Preserve $RunRoot and report it."
}

$Destination = Join-Path $EvidenceWorktree $EvidencePath
if (Test-Path -LiteralPath $Destination) {
    throw "Evidence path already exists: $Destination"
}
[IO.Directory]::CreateDirectory($Destination) | Out-Null
Copy-Item -Path (Join-Path $EvidenceDir '*') -Destination $Destination -Recurse

& $Git -C $EvidenceWorktree add -- $EvidencePath
if ($LASTEXITCODE -ne 0) {
    throw 'Evidence staging failed.'
}
$Staged = @(& $Git -C $EvidenceWorktree diff --cached --name-only)
if ($LASTEXITCODE -ne 0 -or $Staged.Count -eq 0) {
    throw 'Cannot read the staged evidence paths.'
}
$Prefix = "$EvidencePath/"
foreach ($Path in $Staged) {
    if (-not $Path.StartsWith($Prefix, [StringComparison]::Ordinal)) {
        throw "Unexpected staged path: $Path"
    }
}
& $Git -C $EvidenceWorktree diff --cached --check -- "$EvidencePath/result.md"
if ($LASTEXITCODE -ne 0) {
    throw 'The result summary failed the whitespace check.'
}
& $Git -C $EvidenceWorktree commit `
    -m 'evidence: record M1-04 Windows verification' `
    -m 'Co-authored-by: Copilot App <223556219+Copilot@users.noreply.github.com>'
if ($LASTEXITCODE -ne 0) {
    throw 'Evidence commit failed.'
}
$EvidenceSha = & $Git -C $EvidenceWorktree rev-parse HEAD
if ($LASTEXITCODE -ne 0) {
    throw 'Cannot read the evidence commit.'
}
& $Git -C $EvidenceWorktree push origin "HEAD:refs/heads/$EvidenceBranch"
if ($LASTEXITCODE -ne 0) {
    throw "Push failed. Preserve $RunRoot and report it."
}
Write-Output "evidence_branch=$EvidenceBranch"
Write-Output "evidence_commit=$EvidenceSha"
```

Report the evidence branch and commit to the coordinator. Report any failure
or missing test coverage. The coordinator reviews the evidence and updates
the board.
