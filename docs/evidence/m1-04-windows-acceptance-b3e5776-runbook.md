# M1-04 native Windows CLI acceptance runbook (candidate b3e5776122b1a40e85c145169a17faf2270161a4)

**Review and QA of this revision are pending.** The coordinator re-reviews and
re-QAs the candidate `b3e5776122b1a40e85c145169a17faf2270161a4` after this
revision. This file does not claim that either is complete. If the candidate
changes, copy this runbook and replace the pinned SHA, the source branch, and
the file name before anyone runs it.

## Status of this run

This run records how the candidate behaves on native Windows. It does not
close M1-04. The coordinator reviews the evidence and decides acceptance.

This runbook supersedes
`docs/evidence/m1-04-windows-owner-1bef750-runbook.md`, which stays in place
unchanged. The candidate `b3e5776122b1a40e85c145169a17faf2270161a4` descends
from `1bef750a5c3189ddea929e540e294a5d06a57aa3`. Its changes relative to
`1bef750` are test files only: `crates/orchestrator-cli/tests/portable_acceptance.rs`
and `crates/orchestrator-cli/tests/support/fake_codex.rs`. No product source
changed. (Check with `git diff --stat 1bef750a5c3189ddea929e540e294a5d06a57aa3 b3e5776122b1a40e85c145169a17faf2270161a4`.)
Native Windows already passed the five checks below on `1bef750` (evidence
commit `ea0b28ab3ad13784c6bbf1beb8f6d4bdc5e3f8fc`). This run repeats them on the
new candidate and adds the check `cli-acceptance`. That check closes the gap
that `cli.rs`, `multi_task_review.rs`, `recovery.rs`, and `daemon.rs` are
`cfg(unix)`: until now no Windows test ran the real CLI end to end.

`cli-acceptance` runs two tests. Both drive the real `orchestrator-cli` binary
with a small fake agent program that the test compiles with `rustc`:

- `run_produces_isolated_diff_and_cleanup_preserves_primary`: `run` makes a
  candidate commit with a diff in a worktree outside the primary checkout.
  `tasks`, `inspect`, and `diff` show it. `cleanup` refuses the unlanded
  candidate, then removes the worktree after `git merge --ff-only`. The
  primary checkout starts with a tracked edit and an untracked file. Its HEAD,
  `git status --porcelain --untracked-files=all` output, and both file
  contents are unchanged by `run` and survive the merge and cleanup.
- `cancel_terminates_agent_and_descendant`: the fake agent starts a descendant
  and publishes both PIDs. `cancel` makes `run` exit with code 130 within 30
  seconds. The wrapper, agent, and descendant PIDs are all gone. `inspect`
  shows task status `Cancelled`, a confirmed cancellation request, the attempt
  state `Cancelled`, and the session state `Exited`. The
  worktree is preserved and the primary checkout is unchanged.

No native Windows run of this candidate exists yet, and this document does not
claim one. The coordinator reports that an independent code review of
`1bef750` passed. Neither review nor macOS testing is native Windows
verification. On the macOS host the new tests passed 2 of 2 in repeated runs,
the existing `cli` test binary passed 19 of 19, and the workspace passed 386
with 1 ignored; the coordinator's QA of the earlier revision `d612173` reported
the same shape. Review and QA of this revision are pending. The new tests were
not compiled for Windows on the macOS host (see the coverage gaps). If `cli-acceptance` fails, the failure is a finding about the
product or the test on Windows. Do not edit the test to make it pass.

The earlier Windows run of `03d6b139` (evidence commit `a80f87ce`) was
inconclusive. Do not reuse its results.

**PowerShell 7 (`pwsh`) only.** Do not use Windows PowerShell 5.1
(`powershell.exe`). The commands below do not work in it.

## Assignment

Run the checks below on native Windows. Publish the evidence on a separate
branch. Do not edit the candidate source or `board.json`.

The user requires GitHub Copilot subagents configured for Claude Sonnet 5.5
with high reasoning, including descendants. This replaces the worker model
setting in the primer for this run. Do not start an OMP worker. The
coordinator's implementation and review agents for this candidate ran as
Sonnet 5.5 with high reasoning.

The Windows host may run the commands below itself even if its own subagent
runtime cannot select Sonnet 5.5 with high reasoning. The checks are shell
commands. Their result does not depend on the model. If the host cannot
select or observe that setting, record this limitation in the report. Write
`agent runtime: <what you observed>; Sonnet 5.5/high unavailable or
unverified`. Never state that a Sonnet agent ran when you did not observe it.
Do not start another runtime to work around the limitation.

Read the
[orchestration primer](https://github.com/j-mckerracher/agent-orch/blob/b3e5776122b1a40e85c145169a17faf2270161a4/orch-impl-m.md)
and the
[implementation plan](https://github.com/j-mckerracher/agent-orch/blob/b3e5776122b1a40e85c145169a17faf2270161a4/docs/technical-implementation-plan.md).
Follow the primer's skill requirements. Report any unavailable guidance and
any setting that you cannot observe as unverified.

## Candidate

- Repository: `https://github.com/j-mckerracher/agent-orch.git`
- Source branch: `orch/m1-04-windows-acceptance-b3e5776` (the coordinator may push
  the candidate under another name; the dispatch message then states it and
  you replace `$SourceBranch` and its name check in step 1)
- Source commit: `b3e5776122b1a40e85c145169a17faf2270161a4`
- Base: the commit descends from `1bef750a5c3189ddea929e540e294a5d06a57aa3`.
- Evidence branch (you create it): `evidence/m1-04-windows-acceptance-b3e5776-<RunId>`
- Runbook location: the coordinator gives the branch and URL in the dispatch
  message. The runbook is published on a separate docs branch and is not part
  of the source commit. If the source branch tip is later than the source
  commit, test the pinned commit.

Test only the exact source commit. Before any other command, `HEAD` must
equal the full source commit. If the branch does not contain the commit, stop
and report the mismatch. If the branch tip is a later commit, still test the
pinned commit and report the tip.

## Checks

Nobody compiled `cli-acceptance` on Windows before this run. Each check runs
once. Each check records its exit code and every `running N tests` line.

| Check name | Command | Expected result |
|---|---|---|
| `workspace-tests` | `cargo test --locked --workspace --no-fail-fast` | Exit 0. Every `test result` line says `ok`. Exactly 323 passed and 1 ignored in total (the 321 passed and 1 ignored of the `1bef750` Windows evidence plus the 2 tests of `cli-acceptance`). A different total is `INCONCLUSIVE` with a note. Binaries that report `running 0 tests` are normal here (for example doc-tests and Unix-only test files). They are listed in the result, not flagged. |
| `git-package` | `cargo test --locked -p orchestrator-git` | Exit 0. All verification and delivery tests pass. These 7 tests must report `ok`: `timed_out_command_is_killed_and_recorded`, `trusted_command_runs_and_records_output_and_exit_code`, `trusted_nonzero_exit_is_recorded_as_failed`, `dirty_worktree_is_unsafe_and_survives_safety_check`, `report_only_preserves_refs_worktrees_and_repository_files`, `local_review_creates_only_review_ref`, `unlanded_worktree_is_unsafe_and_survives_safety_check`. |
| `store-data-lock-integration` | `cargo test --locked -p orchestrator-store --test data_lock` | Exit 0. One test binary: `running 8 tests`, 7 passed, 0 failed, 1 ignored. `dead_pid_lock_is_taken_over_and_new_holder_is_recorded`, `live_pid_with_different_start_identity_is_taken_over`, and `concurrent_takeovers_have_one_holder_and_one_named_conflict` must report `ok`. The ignored test is `lock_child_process_helper`. Its parent test starts it. |
| `store-data-lock-lib` | `cargo test --locked -p orchestrator-store --lib data_lock` | Exit 0. One test binary: `running 2 tests`, 2 passed, 0 failed, 0 ignored. `range_lock_does_not_block_other_handles_reading_the_record` and `identity_matches_hard_links_and_distinguishes_files` must report `ok`. |
| `ipc-token-owner` | `cargo test --locked -p orchestrator-ipc --lib startup_replaces_only_regular_stale_token` | Exit 0. One test binary: `running 1 test`, 1 passed, 0 failed, 0 ignored. `startup_replaces_only_regular_stale_token` must report `ok`. This test checks that dropping a `Secret` removes `daemon.token` and that a directory at that path is refused and left in place. It is the explicit check of the Windows token fix. It also runs inside `workspace-tests`. This separate check makes a zero-test filter visible. |
| `cli-acceptance` | `cargo test --locked -p orchestrator-cli --test portable_acceptance` | Exit 0. One test binary: `running 2 tests`, 2 passed, 0 failed, 0 ignored. `run_produces_isolated_diff_and_cleanup_preserves_primary` and `cancel_terminates_agent_and_descendant` must report `ok`. The check has a 10-minute limit. The step "Run the CLI acceptance check" below runs it with an isolated temp directory and then lists, without stopping anything, any process of this run that still runs from that directory or is a `__codex_capture` wrapper of this run; a survivor makes the check `FAIL`. The scan covers only this check. `workspace-tests` runs the same two tests with the default `TEMP` and has no survivor scan. `rustc` must be on `PATH` for the test, which compiles its fake agent. |

All six checks count in the overall result. A missing `ipc-token-owner` or
`cli-acceptance` verdict is a `FAIL`.

The test names above come from the evidence of `a80f87ce` and from the source
at the source commit. If the final candidate renamed a test, that check reports
`INCONCLUSIVE` with a note. Do not edit the runbook. Report the note.

`cli.rs`, `multi_task_review.rs`, and `recovery.rs` in `orchestrator-cli`, and
`daemon.rs` in `orchestrator-daemon`, stay `#![cfg(unix)]`. On Windows they
compile to zero tests. Do not run them separately and do not claim Windows
coverage for them. `portable_acceptance.rs` is the Windows CLI coverage. It
covers one `run` to a candidate, `tasks`, `inspect`, `diff`, `cleanup`,
primary-checkout invariance, and `cancel` with a descendant. It does not cover
crash recovery (`recover-owner`), multi-task review, or the daemon. Report
those as coverage gaps.

A filter that runs zero tests is never a pass. The two `data_lock` checks,
`ipc-token-owner`, and `cli-acceptance` use a filter or a single test binary. If
any reports `running 0 tests`, its status is `INCONCLUSIVE`.

## Safety rules

- Use a new clone in a unique directory under `$env:TEMP`.
- Do not change the user's primary checkout.
- Do not install tools or change credentials.
- Do not use the agent bus or copy repository code to another repository.
- Do not change source files, manifests, lockfiles, or `board.json`.
- Do not delete worktrees, directories, or failed attempts.
- Do not use `git reset`, `git clean`, `git push --force`, or `git merge` in
  your own checkouts. (`cli-acceptance` runs `git merge --ff-only` inside a
  throwaway repository that it creates itself. That is expected.)
- Never stop a process by name or by a bare PID. If `cli-acceptance` leaves a
  process, report its PID, path, and command line. Do not stop it.
- Do not publish credentials, agent memory, session files, or unrelated files.
- Do not record remote URLs in any evidence file. The `Invoke-Native` helper
  below replaces every URL in captured Git output with `<url-redacted>`.
  Never print `git remote -v`, `git config --list`, or a clone URL.

## 1. Prepare the checkout

Use native Windows with PowerShell 7 (`pwsh`) only. Open a new `pwsh` session. Run each
block in that session. Keep the session open for all steps.

```powershell
$env:GIT_TERMINAL_PROMPT = '0'
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if (-not $IsWindows -or $PSVersionTable.PSVersion.Major -lt 7) {
    throw 'Native Windows and PowerShell 7 or later are required.'
}

$Git = (Get-Command git.exe -CommandType Application | Select-Object -First 1).Source
$Cargo = (Get-Command cargo.exe -CommandType Application | Select-Object -First 1).Source
$Rustc = (Get-Command rustc.exe -CommandType Application | Select-Object -First 1).Source
$Repository = 'https://github.com/j-mckerracher/agent-orch.git'
$SourceBranch = 'orch/m1-04-windows-acceptance-b3e5776'
$SourceSha = 'b3e5776122b1a40e85c145169a17faf2270161a4'
$BaseSha = '1bef750a5c3189ddea929e540e294a5d06a57aa3'
if ($SourceSha -cnotmatch '^[0-9a-f]{40}$' -or $BaseSha -cnotmatch '^[0-9a-f]{40}$') {
    throw 'A pinned commit is not a full lowercase 40-character SHA.'
}
if ($SourceBranch -cnotmatch '^orch/m1-04-[a-z0-9-]+$') {
    throw 'The source branch name is not valid.'
}
$Short = $SourceSha.Substring(0, 7)
$RunId = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ') + '-' +
    [Guid]::NewGuid().ToString('N')
$RunRoot = Join-Path $env:TEMP "agent-orch-m1-04-acc-$RunId"
$SourceDir = Join-Path $RunRoot 'source'
$EvidenceDir = Join-Path $RunRoot 'evidence'
$BuildDir = Join-Path $RunRoot 'cargo-target'
# cli-acceptance runs with TEMP/TMP here. Its fake agents then live under a
# path that no other program uses, so survivors can be found by path.
$TestTmp = Join-Path $RunRoot 'test-tmp'

if (Test-Path -LiteralPath $RunRoot) {
    throw "Run directory already exists: $RunRoot"
}
New-Item -ItemType Directory -Path $RunRoot | Out-Null
New-Item -ItemType Directory -Path $EvidenceDir | Out-Null
$env:CARGO_TARGET_DIR = $BuildDir
$env:CARGO_TERM_COLOR = 'never'
$env:RUST_BACKTRACE = '1'

function Protect-Text([string] $Text) {
    $Text = [regex]::Replace($Text, '[A-Za-z][A-Za-z0-9+.-]*://\S+', '<url-redacted>')
    return [regex]::Replace($Text, '[\w.-]+@[\w.-]+:\S+', '<url-redacted>')
}

# Runs a native command. Writes <Name>.stdout.txt, <Name>.stderr.txt, and
# <Name>.exit.txt to $EvidenceDir. Every line is passed through Protect-Text.
function Invoke-Native {
    param(
        [string] $Exe, [string] $Name, [string[]] $Arguments,
        [string] $Directory = $RunRoot
    )
    $Out = [Collections.Generic.List[string]]::new()
    $Err = [Collections.Generic.List[string]]::new()
    $ErrorActionPreference = 'Continue'
    Push-Location -LiteralPath $Directory
    try {
        & $Exe @Arguments 2>&1 | ForEach-Object {
            if ($_ -is [Management.Automation.ErrorRecord]) { $Err.Add([string]$_) }
            else { $Out.Add([string]$_) }
        }
        $Code = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    [IO.File]::WriteAllLines((Join-Path $EvidenceDir "$Name.stdout.txt"),
        [string[]]@($Out | ForEach-Object { Protect-Text $_ }))
    [IO.File]::WriteAllLines((Join-Path $EvidenceDir "$Name.stderr.txt"),
        [string[]]@($Err | ForEach-Object { Protect-Text $_ }))
    [IO.File]::WriteAllText((Join-Path $EvidenceDir "$Name.exit.txt"), "$Code`n")
    return [PSCustomObject]@{ Code = $Code; Out = @($Out) }
}

function Invoke-Git([string] $Name, [string[]] $Arguments) {
    Invoke-Native -Exe $Git -Name $Name -Arguments (@('-C', $SourceDir) + $Arguments)
}

function Assert-Head([string] $Where) {
    $Head = & $Git -C $SourceDir rev-parse HEAD
    if ($LASTEXITCODE -ne 0 -or $Head -cne $SourceSha) {
        throw "HEAD is not $SourceSha ($Where). Preserve $RunRoot and report it."
    }
}

$Run = Invoke-Native -Exe $Git -Name 'clone' -Arguments @(
    'clone', '--no-checkout', '--single-branch', '--branch', $SourceBranch,
    $Repository, $SourceDir)
if ($Run.Code -ne 0) { throw "Clone failed. Preserve $RunRoot and report it." }

$Run = Invoke-Git 'branch-tip' @('rev-parse', "refs/remotes/origin/$SourceBranch")
if ($Run.Code -ne 0) { throw 'Cannot read the branch tip.' }
$BranchTip = $Run.Out[0]
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'branch-tip.txt'), "$BranchTip`n")

$Run = Invoke-Git 'candidate-containment' @(
    'merge-base', '--is-ancestor', $SourceSha, "refs/remotes/origin/$SourceBranch")
if ($Run.Code -ne 0) { throw 'The branch does not contain the source commit.' }
$Run = Invoke-Git 'base-ancestry' @('merge-base', '--is-ancestor', $BaseSha, $SourceSha)
if ($Run.Code -ne 0) { throw "The source commit does not descend from $BaseSha." }

$Run = Invoke-Git 'switch' @('switch', '--detach', $SourceSha)
if ($Run.Code -ne 0) { throw "Checkout failed. Preserve $RunRoot and report it." }
Assert-Head 'after checkout'

$Run = Invoke-Git 'source-status-before' @('status', '--porcelain=v1', '--untracked-files=all')
if ($Run.Code -ne 0) { throw 'Cannot read source status.' }
if ($Run.Out.Count -ne 0) { throw 'Source checkout is not clean.' }
```

If Git, Cargo, Rust, or PowerShell is missing, report the missing tool. Do not
install it. Preserve any output that exists. A setup failure gives the result
`BLOCKED`.

## 2. Record the environment

Record the operating system, CPU, toolchain, and source commit. Do not record
the complete environment or any credentials.

```powershell
Assert-Head 'before environment'
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
$GitVersion = Invoke-Native -Exe $Git -Name 'git-version' -Arguments @('--version')
$RustcVersion = Invoke-Native -Exe $Rustc -Name 'rustc-version' -Arguments @('-Vv') -Directory $SourceDir
$CargoVersion = Invoke-Native -Exe $Cargo -Name 'cargo-version' -Arguments @('-V') -Directory $SourceDir
# Exit code 1 means the key is not set. That is not an error.
$Autocrlf = Invoke-Git 'git-autocrlf' @('config', '--show-origin', '--get-all', 'core.autocrlf')
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'source-sha.txt'), "$SourceSha`n")
if ($GitVersion.Code -ne 0 -or $RustcVersion.Code -ne 0 -or $CargoVersion.Code -ne 0) {
    throw "Toolchain check failed. Preserve $RunRoot and report it."
}
```

The files `git-version.stdout.txt`, `rustc-version.stdout.txt`,
`cargo-version.stdout.txt`, and `git-autocrlf.stdout.txt` hold the toolchain
record. An empty `git-autocrlf.stdout.txt` with exit code 1 means
`core.autocrlf` is not set.

## 3. Run the checks

The helper records the stdout, stderr, exit code, and timeout of each command.
Each command has a 30-minute limit. A timeout is a failure, even if the
process later exits with code zero. Run each command once. Do not retry a
failed command in this evidence directory.

```powershell
function Invoke-CapturedCargo {
    param([string] $Name, [string[]] $Arguments, [int] $TimeoutSeconds = 1800)

    Assert-Head "before $Name"
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
            foreach ($File in @($Stdout, $Stderr)) {
                [IO.File]::WriteAllText($File,
                    (Protect-Text ([IO.File]::ReadAllText($File))))
            }
        }
    } catch {
        $Result.error = $_.Exception.Message
    }

    [IO.File]::WriteAllText(
        (Join-Path $EvidenceDir "$Name.result.json"), ($Result | ConvertTo-Json)
    )
    return [PSCustomObject]$Result
}

$GitTests = @(
    'timed_out_command_is_killed_and_recorded',
    'trusted_command_runs_and_records_output_and_exit_code',
    'trusted_nonzero_exit_is_recorded_as_failed',
    'dirty_worktree_is_unsafe_and_survives_safety_check',
    'report_only_preserves_refs_worktrees_and_repository_files',
    'local_review_creates_only_review_ref',
    'unlanded_worktree_is_unsafe_and_survives_safety_check'
)
$DataLockTests = @(
    'dead_pid_lock_is_taken_over_and_new_holder_is_recorded',
    'live_pid_with_different_start_identity_is_taken_over',
    'concurrent_takeovers_have_one_holder_and_one_named_conflict'
)
$DataLockLibTests = @(
    'range_lock_does_not_block_other_handles_reading_the_record',
    'identity_matches_hard_links_and_distinguishes_files'
)
$IpcTokenTests = @('startup_replaces_only_regular_stale_token')
$CliAcceptanceTests = @(
    'run_produces_isolated_diff_and_cleanup_preserves_primary',
    'cancel_terminates_agent_and_descendant'
)

# Expect: tests that must report ok.
# Single: the run must have exactly one test binary, and that binary must run
#   at least one test. This is how a filter that matches nothing is caught.
# Passed, Ignored: exact counts.
# Isolated: run with TEMP/TMP under $TestTmp and fail on survivor processes.
# TimeoutSeconds: limit for the command (default 1800).
$Checks = @(
    @{ Name = 'workspace-tests'; Expect = @(); Single = $false
       Passed = 323; Ignored = 1
       Arguments = @('test', '--locked', '--workspace', '--no-fail-fast') },
    @{ Name = 'git-package'; Expect = $GitTests; Single = $false
       Arguments = @('test', '--locked', '-p', 'orchestrator-git') },
    @{ Name = 'store-data-lock-integration'; Expect = $DataLockTests; Single = $true
       Passed = 7; Ignored = 1
       Arguments = @('test', '--locked', '-p', 'orchestrator-store',
           '--test', 'data_lock') },
    @{ Name = 'store-data-lock-lib'; Expect = $DataLockLibTests; Single = $true
       Passed = 2; Ignored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-store',
           '--lib', 'data_lock') },
    @{ Name = 'ipc-token-owner'; Expect = $IpcTokenTests; Single = $true
       Passed = 1; Ignored = 0
       Arguments = @('test', '--locked', '-p', 'orchestrator-ipc', '--lib',
           'startup_replaces_only_regular_stale_token') },
    @{ Name = 'cli-acceptance'; Expect = $CliAcceptanceTests; Single = $true
       Passed = 2; Ignored = 0; Isolated = $true; TimeoutSeconds = 600
       Arguments = @('test', '--locked', '-p', 'orchestrator-cli',
           '--test', 'portable_acceptance') }
)

$Rank = @{ PASS = 0; INCONCLUSIVE = 1; FAIL = 2 }

function Get-CheckVerdict($Check, $Result) {
    $State = @{ Status = 'PASS'; Notes = [Collections.Generic.List[string]]::new() }
    $Raise = {
        param($Level, $Note)
        if ($Rank[$Level] -gt $Rank[$State.Status]) { $State.Status = $Level }
        $State.Notes.Add($Note)
    }
    $Running = @(); $Passed = $null; $Failed = $null; $Ignored = $null

    if ($Result.timed_out -or $Result.error -or $null -eq $Result.exit_code) {
        & $Raise 'FAIL' 'Timeout or process error.'
    } elseif ($Result.exit_code -ne 0) {
        & $Raise 'FAIL' "Exit code $($Result.exit_code)."
    }

    $StdoutPath = Join-Path $EvidenceDir "$($Check.Name).stdout.txt"
    $Out = ''
    if (Test-Path -LiteralPath $StdoutPath) { $Out = [IO.File]::ReadAllText($StdoutPath) }
    $Lines = [regex]::Matches($Out, '(?m)^(?:running \d+ tests?|test result: .*)$') |
        ForEach-Object { $_.Value.TrimEnd() }
    [IO.File]::WriteAllLines(
        (Join-Path $EvidenceDir "$($Check.Name).summary.txt"), [string[]]@($Lines))

    $Running = @([regex]::Matches($Out, '(?m)^running (\d+) tests?\s*$') |
        ForEach-Object { [int]$_.Groups[1].Value })
    $Summary = [regex]::Matches($Out,
        '(?m)^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored')
    if ($Summary.Count -gt 0) {
        $Passed = ($Summary | ForEach-Object { [int]$_.Groups[2].Value } | Measure-Object -Sum).Sum
        $Failed = ($Summary | ForEach-Object { [int]$_.Groups[3].Value } | Measure-Object -Sum).Sum
        $Ignored = ($Summary | ForEach-Object { [int]$_.Groups[4].Value } | Measure-Object -Sum).Sum
    }

    if ($Summary.Count -eq 0) {
        & $Raise 'INCONCLUSIVE' 'No test result line.'
    } else {
        if ($Summary.Count -ne $Running.Count) {
            & $Raise 'INCONCLUSIVE' "$($Running.Count) 'running' lines but $($Summary.Count) result lines."
        }
        if ($Failed -gt 0 -or @($Summary | Where-Object { $_.Groups[1].Value -eq 'FAILED' }).Count -gt 0) {
            & $Raise 'FAIL' "$Failed test(s) failed."
        }
        if (($Running | Measure-Object -Sum).Sum -eq 0) {
            & $Raise 'INCONCLUSIVE' 'running 0 tests. No test ran.'
        }
        if ($Check.Single) {
            if ($Summary.Count -ne 1) {
                & $Raise 'INCONCLUSIVE' "Found $($Summary.Count) test binaries. Expected 1."
            }
            if ($Running -contains 0) {
                & $Raise 'INCONCLUSIVE' 'The filter ran zero tests (running 0 tests).'
            }
        }
        if ($null -ne $Check.Passed -and $Passed -ne $Check.Passed) {
            & $Raise 'INCONCLUSIVE' "$Passed passed. Expected $($Check.Passed)."
        }
        if ($null -ne $Check.Ignored -and $Ignored -ne $Check.Ignored) {
            & $Raise 'INCONCLUSIVE' "$Ignored ignored. Expected $($Check.Ignored)."
        }
        foreach ($Test in $Check.Expect) {
            $Pattern = '(?m)^test (?:\S+::)*' + [regex]::Escape($Test) + ' \.\.\. ok\s*$'
            if (-not [regex]::IsMatch($Out, $Pattern)) {
                & $Raise 'INCONCLUSIVE' "No 'ok' line for $Test."
            }
        }
    }
    [PSCustomObject]@{
        name = $Check.Name; status = $State.Status; exit_code = $Result.exit_code
        timed_out = $Result.timed_out; running = ($Running -join ',')
        passed = $Passed; failed = $Failed; ignored = $Ignored
        notes = ($State.Notes -join ' ')
    }
}

# A survivor is a process whose image lies under this run's TestTmp directory
# (case-insensitive full prefix), or whose command line is a __codex_capture
# wrapper that names this run's RunRoot directory. The wrapper image lives in
# the cargo target directory, so the command-line test is what finds it.
function Find-OwnedSurvivors($Processes) {
    $TmpPrefix = $TestTmp.TrimEnd('\') + '\'
    $RootPrefix = $RunRoot.TrimEnd('\') + '\'
    $Ordinal = [StringComparison]::OrdinalIgnoreCase
    foreach ($Process in $Processes) {
        $ByImage = $Process.ExecutablePath -and
            $Process.ExecutablePath.StartsWith($TmpPrefix, $Ordinal)
        $ByWrapper = $Process.CommandLine -and
            $Process.CommandLine.IndexOf('__codex_capture', $Ordinal) -ge 0 -and
            $Process.CommandLine.IndexOf($RootPrefix, $Ordinal) -ge 0
        if ($ByImage -or $ByWrapper) {
            $Process | Select-Object ProcessId, ParentProcessId, Name, ExecutablePath, CommandLine
        }
    }
}

$Verdicts = @()
foreach ($Check in $Checks) {
    $Timeout = if ($Check.TimeoutSeconds) { [int]$Check.TimeoutSeconds } else { 1800 }
    if ($Check.Isolated) {
        # Private temp directory and rustc on PATH for the test's fake agent.
        New-Item -ItemType Directory -Path $TestTmp | Out-Null
        $SavedEnv = @{ TEMP = $env:TEMP; TMP = $env:TMP; PATH = $env:PATH }
        $env:TEMP = $TestTmp; $env:TMP = $TestTmp
        $env:PATH = (Split-Path -Parent $Rustc) + ';' + $env:PATH
        try {
            $Run = Invoke-CapturedCargo -Name $Check.Name -Arguments $Check.Arguments `
                -TimeoutSeconds $Timeout
        } finally {
            $env:TEMP = $SavedEnv.TEMP; $env:TMP = $SavedEnv.TMP; $env:PATH = $SavedEnv.PATH
        }
    } else {
        $Run = Invoke-CapturedCargo -Name $Check.Name -Arguments $Check.Arguments `
            -TimeoutSeconds $Timeout
    }
    if (-not $Run.root_process_stopped) {
        throw "An owned process may still run. Preserve $RunRoot and report it."
    }
    $Verdict = Get-CheckVerdict $Check $Run
    if ($Check.Isolated) {
        # Read-only listing of this run's processes only. No process is stopped.
        $Survivors = @(Find-OwnedSurvivors (Get-CimInstance Win32_Process))
        $Survivors | Format-List | Out-File -LiteralPath `
            (Join-Path $EvidenceDir "$($Check.Name).survivors.txt") -Encoding utf8
        if ($Survivors.Count -ne 0) {
            $Verdict.status = 'FAIL'
            $Verdict.notes = ($Verdict.notes + " $($Survivors.Count) process(es) of this run survived the cancel scenario; see $($Check.Name).survivors.txt.").Trim()
        }
    }
    $Verdicts += $Verdict
}

Assert-Head 'after the checks'
$After = Invoke-Git 'source-status-after' @('status', '--porcelain=v1', '--untracked-files=all')
$DiffCheck = Invoke-Git 'diff-check' @('diff', '--check')
```

The helper checks the Cargo process only. It does not prove that every
descendant stopped. The wrapper above never converts a command failure into
success. It records the status of each command from its own exit code and
its own output. `Assert-Head` throws if `HEAD` changes during the run.

Status rules for each command:

- `FAIL`: timeout, process error, non-zero exit code, a failed test, or (for
  `cli-acceptance`) any process still running from the run's private temp
  directory after the test.
- `INCONCLUSIVE`: no `test result` line, or the `running` and `test result`
  lines do not pair up. Also: every binary reports `running 0 tests`, or a
  filtered check (`store-data-lock-integration`, `store-data-lock-lib`,
  `ipc-token-owner`, `cli-acceptance`) reports `running 0 tests`. Also: a required test has no `ok` line, or a
  passed or ignored count differs from the expected count. A skipped or
  zero-test selection is never a pass.
- `PASS`: exit code zero, every required test reports `ok`, zero tests
  failed, and the counts match the expected result in the Checks table.

## 4. Write the result

```powershell
# Set this to what you observed about your own agent runtime and model. If you
# could not observe Sonnet 5.5 with high reasoning, keep 'unverified'.
$AgentRuntime = 'unverified'
$Overall = 'PASS'
foreach ($Verdict in $Verdicts) {
    if ($Rank[$Verdict.status] -gt $Rank[$Overall]) { $Overall = $Verdict.status }
}
$Extra = [Collections.Generic.List[string]]::new()
foreach ($Check in $Checks) {
    if (@($Verdicts | Where-Object { $_.name -ceq $Check.Name }).Count -ne 1) {
        $Overall = 'FAIL'; $Extra.Add("No single verdict for $($Check.Name).")
    }
}
if ($After.Code -ne 0 -or $After.Out.Count -ne 0) {
    $Overall = 'FAIL'; $Extra.Add('Source status after the run is not clean.')
}
if ($DiffCheck.Code -ne 0) { $Overall = 'FAIL'; $Extra.Add('git diff --check failed.') }

$Table = ($Verdicts | ForEach-Object {
    "| $($_.name) | $($_.status) | $($_.exit_code) | $($_.timed_out) | " +
    "$($_.running) | $($_.passed) | $($_.failed) | $($_.ignored) | $($_.notes) |"
}) -join "`n"
$Fence = '`' * 3
$Summaries = ($Verdicts | ForEach-Object {
    "### $($_.name)`n`n${Fence}text`n" +
    ([IO.File]::ReadAllText((Join-Path $EvidenceDir "$($_.name).summary.txt")).TrimEnd()) +
    "`n$Fence"
}) -join "`n`n"
$Report = @"
# M1-04 Windows CLI acceptance of $SourceSha

This run does not close M1-04. The coordinator decides acceptance.

- Run ID: $RunId
- UTC date: $([DateTime]::UtcNow.ToString('o'))
- Source branch: $SourceBranch
- Branch tip at clone time: $BranchTip
- Source commit: $SourceSha
- HEAD matched the source commit before the checks and after them: yes
- Overall result: $Overall $($Extra -join ' ')
- Toolchain: see git-version.stdout.txt, rustc-version.stdout.txt,
  cargo-version.stdout.txt, git-autocrlf.stdout.txt (exit code in
  git-autocrlf.exit.txt)
- OS, CPU, PowerShell: see os.txt, cpu.txt, powershell-version.txt
- Agent runtime and Sonnet 5.5/high setting: $AgentRuntime
- Required IPC check: the ipc-token-owner row below covers
  startup_replaces_only_regular_stale_token and is part of the overall result.
- Required CLI check: the cli-acceptance row below covers both
  portable_acceptance tests and a report-only survivor scan. The scan covers
  processes under this run's private temp directory and `__codex_capture`
  wrappers that name this run's directory. It does not cover `workspace-tests`,
  which runs the same tests with the default TEMP. The row is part of the
  overall result.

| Command | Status | Exit | Timed out | Running (N per binary) | Passed | Failed | Ignored | Notes |
|---|---|---|---|---|---|---|---|---|
$Table

## Test result lines

$Summaries

## Coverage gaps

- Windows CLI coverage comes from the cli-acceptance row: one `run` to a
  candidate, `tasks`, `inspect`, `diff`, `cleanup`, primary-checkout
  invariance, and `cancel` with a descendant process. The files cli.rs,
  multi_task_review.rs, and recovery.rs are Unix-only (cfg unix) and compile to
  zero tests on Windows.
- Windows crash recovery (`recover-owner`) and multi-task review coverage: none.
- Windows daemon integration coverage: none. crates/orchestrator-daemon/tests/daemon.rs
  is Unix-only (cfg unix) and compiles to zero tests on Windows.
"@
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'result.md'), $Report)
[IO.File]::WriteAllText(
    (Join-Path $EvidenceDir 'verdicts.json'), ($Verdicts | ConvertTo-Json)
)
Write-Output "overall=$Overall"
```

Use PASS only when every command is PASS, `HEAD` matches the full source
commit, both source status checks are empty, and the diff check exits zero.
A PASS shows that these checks passed on this host. It does not show Windows
coverage for any scenario that no test ran.

If a command fails, preserve its complete output and report its exit code. If
a process does not stop, report it and do not start another attempt. For a
retry, use a new run ID and preserve both attempts.

## 5. Publish evidence only

Publish the evidence for every result, including FAIL and INCONCLUSIVE. Publish
only flat text files from `$EvidenceDir`. Do not publish `cargo-target`, the
source clone, session files, agent memory, or any other path.

```powershell
$EvidenceBranch = "evidence/m1-04-windows-acceptance-$Short-$RunId"
$EvidenceWorktree = Join-Path $env:TEMP "m104ev-$Short-$($RunId.Substring($RunId.Length - 8))"
$EvidencePath = "docs/evidence/m1-04-windows-acceptance/$Short-$RunId"

$Run = Invoke-Git 'publish-fetch' @('fetch', 'origin', 'refs/heads/main:refs/remotes/origin/main')
if ($Run.Code -ne 0) { throw "Fetch failed. Preserve $RunRoot and report it." }

$Files = @(Get-ChildItem -LiteralPath $EvidenceDir -Force)
foreach ($File in $Files) {
    if ($File.PSIsContainer -or $File.Name -notmatch '^[a-z0-9][a-z0-9.-]*\.(txt|json|md)$') {
        throw "Unexpected evidence entry: $($File.Name)"
    }
}
if (-not (Test-Path -LiteralPath (Join-Path $EvidenceDir 'result.md') -PathType Leaf)) {
    throw 'Create the result summary before publishing evidence.'
}

$SecretPattern = 'ghp_[A-Za-z0-9]{20,}|github_pat_|gho_[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}|sk-[A-Za-z0-9]{20,}|-----BEGIN [A-Z ]*PRIVATE KEY|(?i)(password|passwd|secret|api[_-]?key|bearer)\s*[:=](?!:)|(?i)authorization:(?!:)'
$UrlPattern = '://[^/\s@]*@|git@|ssh://'
$Hits = @(Select-String -Path (Join-Path $EvidenceDir '*') -Pattern $SecretPattern) +
    @(Select-String -Path (Join-Path $EvidenceDir '*') -Pattern $UrlPattern) +
    @(Select-String -Path (Join-Path $EvidenceDir '*') -SimpleMatch 'j-mckerracher/agent-orch')
if ($Hits.Count -ne 0) {
    $Hits | Select-Object Path, LineNumber | Format-Table | Out-String | Write-Output
    throw 'Possible secret or remote URL found. Do not push. Keep the raw files and report it.'
}
```

The pattern scan is a backstop. Also read every file before you publish. If a
file holds a secret or a remote URL, do not push it. Keep the raw file and
report the blocker.

```powershell
if (Test-Path -LiteralPath $EvidenceWorktree) { throw "Worktree path exists: $EvidenceWorktree" }
# Main holds long paths. Windows needs core.longpaths and a short worktree path.
$Run = Invoke-Git 'publish-worktree' @(
    '-c', 'core.longpaths=true', 'worktree', 'add', '-b', $EvidenceBranch,
    $EvidenceWorktree, 'refs/remotes/origin/main')
if ($Run.Code -ne 0) { throw "Worktree creation failed. Preserve $RunRoot and report it." }
& $Git -C $EvidenceWorktree config core.longpaths true
if ($LASTEXITCODE -ne 0) { throw 'Cannot set core.longpaths in the evidence worktree.' }

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
    -m "evidence: record M1-04 Windows run of $Short ($Overall)" `
    -m 'Co-authored-by: Copilot App <223556219+Copilot@users.noreply.github.com>'
if ($LASTEXITCODE -ne 0) { throw 'Evidence commit failed. If Git needs a user identity, report BLOCKED. Do not change credentials.' }
$EvidenceSha = & $Git -C $EvidenceWorktree rev-parse HEAD
$Changed = @(& $Git -C $EvidenceWorktree diff --name-only refs/remotes/origin/main HEAD)
foreach ($Path in $Changed) {
    if (-not $Path.StartsWith("$EvidencePath/", [StringComparison]::Ordinal)) {
        throw "Commit holds an unexpected path: $Path"
    }
}
$Run = Invoke-Native -Exe $Git -Name 'publish-push' -Directory $EvidenceWorktree `
    -Arguments @('push', 'origin', "HEAD:refs/heads/$EvidenceBranch")
if ($Run.Code -ne 0) { throw "Push failed. Preserve $RunRoot and report it." }
Write-Output "evidence_branch=$EvidenceBranch"
Write-Output "evidence_commit=$EvidenceSha"
Write-Output "overall=$Overall"
if ($Overall -ne 'PASS') {
    throw "Result is $Overall. Evidence is published. Report the failure."
}
```

The push output stays in `$EvidenceDir` as `publish-push.*.txt`. It is not
published. It has the URL redacted.

The final `throw` makes a non-PASS result visible in the session and gives a
non-zero exit code if you run the block as a script. Report to the coordinator:

- the evidence branch and commit
- the overall result and the status of each command, with its exit code and
  `running N tests` counts
- the OS, CPU, `rustc -Vv`, `cargo -V`, `git --version`, and
  `core.autocrlf` (with its origin)
- the branch tip and the pinned source commit, and that `HEAD` matched
- the coverage gaps: no Windows crash-recovery (`recover-owner`), multi-task
  review, or daemon integration coverage
- for `cli-acceptance`: the `running 2 tests` line, the `survivors.txt` content
  (empty means none found under this run's temp directory or wrappers naming
  this run; it says nothing about `workspace-tests`),
  and for a failure the first failing assertion message
- any setting that you could not observe, including whether your agent runtime
  could run Sonnet 5.5 with high reasoning

Do not edit `board.json`. The coordinator reviews the evidence and updates the
board.
