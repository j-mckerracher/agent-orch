# M7 final r1 Windows disk-space diagnostic

Updated: 2026-11-11T11:10:58Z

> **DIAGNOSTIC ONLY.** This procedure measures free space and inventories eligible task-owned build caches. It does not establish acceptance, authorize cleanup, or replace the full Windows runbook.

## Source and reason

- Candidate source: `6c17442eaee5243b48034dfc53d7f43d25f1ab82`
- Expected source tree: `9b9612ab6084c644e344603028149084118cc889`
- Existing rerun addendum: `docs/windows-acceptance/m7-final-r1-6c17442/RERUN-ADDENDUM.md` at commit `eea5a3e3e5ab4d700c041c37b31353a5b9d83a41`
- Blocked preflight evidence: commit `80fd0bc893cc0aa3b9a1de3583b68e57a3905a5b`

The blocked preflight recorded 1,884,020,736 bytes free on C:. Its Temp alias resolved to C:, and no other filesystem volume was available. The addendum says 20 GB. The blocked report says 20 GiB. Report free bytes and shortfalls against both values. Do not treat this diagnostic as an acceptance gate.

## Safety limits

1. Do not delete, move, rename, modify, or overwrite any existing file, cache, worktree, ref, or evidence.
2. Do not run builds, tests, installers, UI checks, or cleanup commands. Do not change permissions, credentials, SSH, Git, GitHub, or environment settings. Do not clone, fetch, or check out the candidate source for this diagnostic.
3. Do not scan an entire drive or profile. Inspect only explicit candidate paths that have task-ownership evidence.
4. Treat unknown ownership, shared locations, network paths, reparse points, symlinks, junctions, and inaccessible directories as ineligible. Do not measure their contents.
5. The only local additions allowed are a unique run directory, its raw and sanitized evidence files, and a temporary evidence repository inside it. Before every write, confirm the destination does not exist. Do not alter existing files, worktrees, refs, or evidence. The only remote ref addition allowed is one new evidence-only ref after privacy review.
6. Keep raw paths, raw outputs, transcripts, and the placeholder map on the Windows host. Never copy them into the source checkout or publish repository.
7. Publish only sanitized copies. Remove credentials, private email addresses, usernames, hostnames, account identifiers, and identifying path components. If you cannot verify a redaction, do not publish that artifact or the evidence ref.

## Verify the Windows worker session

Before the disk audit, verify the active Windows worker's own OMP RPC `get_state`. Require provider `codex`, model `gpt-6-luna`, and thinking `max`. Observe Ponytail full load in that same session. Confirm successful reads of `skill://poteto-mode` and `skill://pstack-pi` in that session. A separate probe does not count.

Keep the raw session proof on Windows. Publish only `worker_runtime=verified`. If any requirement is missing, do not run the disk audit or publish evidence. Report `BLOCKED`.


## Create a unique local evidence directory

Use PowerShell 7. Keep raw and publishable files in separate directories. Do not use `-Force` to create, copy, or overwrite evidence. Use a new run directory.

```powershell
if ($PSVersionTable.PSVersion.Major -lt 7) { throw 'PowerShell 7 is required.' }
if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) { throw 'LOCALAPPDATA is unavailable.' }
$localDataItem = Get-Item -LiteralPath $env:LOCALAPPDATA -Force -ErrorAction Stop
$localRoot = [System.IO.Path]::GetFullPath($localDataItem.FullName)
$localDrive = [System.IO.DriveInfo]::new([System.IO.Path]::GetPathRoot($localRoot))
if ($localDrive.DriveType -ne [System.IO.DriveType]::Fixed) { throw 'Raw evidence root is not on a local fixed volume.' }
$current = $localRoot
while ($true) {
    $part = Get-Item -LiteralPath $current -Force -ErrorAction Stop
    if (($part.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Raw evidence root contains a reparse point.' }
    $parent = [System.IO.Directory]::GetParent($current)
    if ($null -eq $parent) { break }
    $current = $parent.FullName
}
$runId = '{0}-{1}' -f [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ'), ([Guid]::NewGuid().ToString('N').Substring(0, 12))
$auditRoot = Join-Path $localRoot 'm7-disk-audit'
if (Test-Path -LiteralPath $auditRoot) {
    $auditItem = Get-Item -LiteralPath $auditRoot -Force -ErrorAction Stop
    if (($auditItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Audit root is a reparse point.' }
} else {
    $null = New-Item -ItemType Directory -Path $auditRoot -ErrorAction Stop
}
$runRoot = Join-Path $auditRoot $runId
if (Test-Path -LiteralPath $runRoot) { throw 'Run directory already exists. Generate a new run ID.' }
$null = New-Item -ItemType Directory -Path $runRoot -ErrorAction Stop
$rawRoot = Join-Path $runRoot 'raw'
$publishRoot = Join-Path $runRoot 'publish'
$null = New-Item -ItemType Directory -Path $rawRoot -ErrorAction Stop
$null = New-Item -ItemType Directory -Path $publishRoot -ErrorAction Stop
Start-Transcript -Path (Join-Path $rawRoot 'transcript.txt') -NoClobber -IncludeInvocationHeader
```

If any creation step fails, keep any directory already created and stop. Do not reuse or remove it. The transcript and `$rawRoot` can contain private Windows paths. Keep them on this Windows host.
Record `Free` and `Used` in bytes for every filesystem volume. Do not publish drive roots or drive names. For each row, derive its volume root locally with `[System.IO.Path]::GetPathRoot($row.Root)`. Map aliases with the same root to one stable token, such as `VOLUME_001`. Keep the token-to-root map in `raw` only.

## Measure filesystem free space

Run this command without a pipeline that hides its status. The transcript is the raw record.

```powershell
$driveStatus = 'success'
try {
    $driveRows = @(Get-PSDrive -PSProvider FileSystem -ErrorAction Stop | Select-Object Name, Root, Free, Used)
    $driveRows | Format-Table -AutoSize
} catch {
    $driveStatus = 'failed'
    $_ | Out-String | Write-Host
}
Write-Host "Get-PSDrive command_status=$driveStatus"
```

Record `Free` and `Used` in bytes for every filesystem volume. Do not publish drive roots or drive names. Map aliases that resolve to the same volume to one stable token, such as `VOLUME_001`. Keep the token-to-root map in `raw` only.

Report both thresholds in the sanitized summary:

- 20 GB means `20,000,000,000` bytes.
- 20 GiB means `21,474,836,480` bytes.

For each volume, report `free_bytes`, `shortfall_to_20_GB_bytes`, and `shortfall_to_20_GiB_bytes`. Clamp a negative shortfall to zero. A volume with enough free space is only a possible future build location. Do not change `TEMP`, `TMP`, `TEST_TMP`, or `CARGO_TARGET_DIR` during this audit.

Record only `CARGO_TARGET_DIR`, `TEST_TMP`, `TEMP`, and `TMP` in the raw transcript. Do not dump the full environment or publish these values. Use `CARGO_TARGET_DIR` only as a cache candidate when task-exclusive ownership is proven. Use the other values only to map build and temp volumes. Do not enumerate their directory contents.

```powershell
$envRows = @('CARGO_TARGET_DIR', 'TEST_TMP', 'TEMP', 'TMP') | ForEach-Object {
    [pscustomobject]@{ Name = $_; Value = [Environment]::GetEnvironmentVariable($_, 'Process') }
}
$envRows | Format-Table -AutoSize
```

The transcript records the four environment values above. A command status of `success` means PowerShell completed the cmdlet. It is not a native process exit code.

## Identify eligible cache candidates

Use only explicit build-cache locations named by the assignment, such as `CARGO_TARGET_DIR`, or a `target` directory inside the confirmed task-owned worktree. An environment variable alone does not prove ownership. `TEST_TMP`, `TEMP`, and `TMP` are not cache candidates.

If an existing task checkout is available, use its exact path from the Windows assignment. Do not search for other checkouts. Capture the following read-only Git commands to separate raw stdout, raw stderr, and producer exit files in `$rawRoot`. These output files must not already exist. If any exists, stop and create a new run directory.

```powershell
$worktreesOut = Join-Path $rawRoot 'worktrees.stdout.txt'
$worktreesErr = Join-Path $rawRoot 'worktrees.stderr.txt'
if ((Test-Path -LiteralPath $worktreesOut) -or (Test-Path -LiteralPath $worktreesErr)) { throw 'Raw output exists. Start a new run.' }
git.exe -C $repoRoot worktree list --porcelain 1> $worktreesOut 2> $worktreesErr
$worktreesExit = $LASTEXITCODE
Write-Host "git worktree list exit_code=$worktreesExit"

$headOut = Join-Path $rawRoot 'head.stdout.txt'
$headErr = Join-Path $rawRoot 'head.stderr.txt'
if ((Test-Path -LiteralPath $headOut) -or (Test-Path -LiteralPath $headErr)) { throw 'Raw output exists. Start a new run.' }
git.exe -C $repoRoot rev-parse HEAD 1> $headOut 2> $headErr
$headExit = $LASTEXITCODE
Write-Host "git rev-parse HEAD exit_code=$headExit"

$treeOut = Join-Path $rawRoot 'tree.stdout.txt'
$treeErr = Join-Path $rawRoot 'tree.stderr.txt'
if ((Test-Path -LiteralPath $treeOut) -or (Test-Path -LiteralPath $treeErr)) { throw 'Raw output exists. Start a new run.' }
git.exe -C $repoRoot rev-parse 'HEAD^{tree}' 1> $treeOut 2> $treeErr
$treeExit = $LASTEXITCODE
Write-Host "git rev-parse HEAD^{tree} exit_code=$treeExit"
```

Set `$repoRoot` from the current assignment. If the assigned checkout is absent, record `NO_EXISTING_TASK_CHECKOUT`; do not clone or search other directories. A source checkout is not required to measure volume free space.

Inventory a cache only when all these facts are available:

- The assignment record names the task-owned worktree or unique run directory.
- The exact path is a direct descendant of that task-owned location, or an explicit task-only `CARGO_TARGET_DIR` value.
- The worktree metadata and source identity support the ownership claim.
- No other task, worktree, process, or user shares the path. If hard links or concurrent use cannot be ruled out, mark the path ineligible.
- The path is on a local fixed or removable volume.

Do not inspect shared Cargo registries, npm caches, user-wide temp roots, `node_modules`, other task worktrees, or other paths found by broad searches. If any ownership fact is unknown, exclude the path and record only an exclusion count and reason code. Do not publish the excluded path.

## Measure only eligible directories

Use this function only after ownership checks pass. It reads directory entries, attributes, and file lengths. It does not open file contents. It rejects reparse points in the candidate path, its ancestors, or its descendants. Any error makes the whole candidate ineligible. Do not use `Get-ChildItem -Recurse` on candidate paths.

```powershell
function Measure-TaskCache {
    param([Parameter(Mandatory)][string]$LiteralPath)

    $item = Get-Item -LiteralPath $LiteralPath -Force -ErrorAction Stop
    if (-not $item.PSIsContainer) { throw 'ineligible_not_directory' }
    $fullPath = [System.IO.Path]::GetFullPath($item.FullName)
    $driveRoot = [System.IO.Path]::GetPathRoot($fullPath)
    $drive = [System.IO.DriveInfo]::new($driveRoot)
    if ($drive.DriveType -notin @([System.IO.DriveType]::Fixed, [System.IO.DriveType]::Removable)) {
        throw 'ineligible_nonlocal_volume'
    }

    $current = $fullPath
    while ($true) {
        $part = Get-Item -LiteralPath $current -Force -ErrorAction Stop
        if (($part.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw 'ineligible_reparse_point'
        }
        $parent = [System.IO.Directory]::GetParent($current)
        if ($null -eq $parent) { break }
        $current = $parent.FullName
    }

    $stack = [System.Collections.Generic.Stack[System.IO.DirectoryInfo]]::new()
    $stack.Push([System.IO.DirectoryInfo]::new($fullPath))
    [long]$bytes = 0
    [long]$files = 0
    while ($stack.Count -gt 0) {
        $directory = $stack.Pop()
        foreach ($entry in $directory.EnumerateFileSystemInfos()) {
            if (($entry.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw 'ineligible_reparse_point'
            }
            if ($entry -is [System.IO.DirectoryInfo]) {
                $stack.Push([System.IO.DirectoryInfo]::new($entry.FullName))
            } elseif ($entry -is [System.IO.FileInfo]) {
                $bytes += $entry.Length
                $files++
            }
        }
    }

    [pscustomobject]@{ ResolvedPath = $fullPath; Bytes = $bytes; Files = $files }
}
```

After ownership checks pass, set `$candidatePath` to exactly one eligible path. Use the existing `CARGO_TARGET_DIR` value only when it is task-exclusive. Otherwise use `(Join-Path $repoRoot 'target')` only when `$repoRoot` is the confirmed task-owned checkout. Do not derive candidates from drive scans or temp variables.

```powershell
$measureStatus = 'success'
$measurement = $null
try {
    $measurement = Measure-TaskCache -LiteralPath $candidatePath
    $measurement | Format-List
} catch {
    $measureStatus = 'failed'
    $_ | Out-String | Write-Host
}
Write-Host "Measure-TaskCache command_status=$measureStatus"
```


## Build sanitized evidence

Keep the exact path and ownership evidence in `raw/redaction-map.local.tsv`. Assign stable `PATH_001`, `PATH_002`, `VOLUME_001`, and `OWNER_001` tokens. Reuse each token consistently. Never include this map in the publish directory.

Put only these sanitized files in `$publishRoot`:

- `SUMMARY.md` with `worker_runtime=verified`, the source and addendum SHAs, UTC run ID, per-volume tokens and free bytes, and both threshold shortfalls.
- `CACHE-INVENTORY.csv` with tokenized path and owner identifiers, logical bytes, file count, eligibility, and reason code.
- `COMMAND-EXITS.csv` with command name, exact producer exit code for native commands, and PowerShell command status for cmdlets.
- `SHA256SUMS.txt` with hashes of the sanitized files only.

Use these columns:

- `CACHE-INVENTORY.csv`: `path_token,purpose,logical_bytes,file_count,owner_token,owner_basis,eligible,reason_code`
- `COMMAND-EXITS.csv`: `command_name,producer_exit_code,powershell_status`

Populate `SUMMARY.md` with the run ID, source and addendum SHAs, per-volume tokens and free bytes, both threshold shortfalls, eligible candidate count, exclusion counts, and non-identifying owner proof descriptions. Do not add raw paths or identifying owner details.
Describe each owner token without exposing paths. For example, `OWNER_001` can mean that assignment metadata and worktree identity proved task ownership. Keep exact paths and raw Git output local.

Create `SHA256SUMS.txt` after the other three files are final. Hash only those three sanitized files.

```powershell
$hashFiles = @('SUMMARY.md', 'CACHE-INVENTORY.csv', 'COMMAND-EXITS.csv')
$sumPath = Join-Path $publishRoot 'SHA256SUMS.txt'
if (Test-Path -LiteralPath $sumPath) { throw 'Checksum manifest already exists. Start a new run.' }
$sumLines = foreach ($name in $hashFiles) {
    $path = Join-Path $publishRoot $name
    $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
    '{0}  {1}' -f $hash, $name
}
$sumLines | Set-Content -LiteralPath $sumPath -Encoding utf8
```

Recompute each hash and compare it with the manifest before publishing. Do not add the manifest's own hash.

After all four sanitized files are final, run this screening command. It prints a count, not matching lines.

```powershell
$privacyPatterns = @(
    '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}',
    '(?i)(?:^|[\\/])Users[\\/][^\\/]+',
    '\\\\[^\\]+\\[^\\]+',
    '(?i)\b(?:password|token|secret|authorization)\s*[:=]'
)
$privacyHits = @(Get-ChildItem -LiteralPath $publishRoot -File | Select-String -Pattern $privacyPatterns)
Write-Host "published_privacy_hit_count=$($privacyHits.Count)"
```

Review every published filename and file manually for usernames, hostnames, private email addresses, credentials, and identifying paths. The scan is only a screen. If any match or uncertainty remains, sanitize a new copy and scan it again. If you cannot verify the result, keep the affected evidence local and do not push the evidence ref.

## Publish one evidence-only ref

Publish only after the privacy review passes. Use an SSH endpoint already verified on this Windows host. Do not assume a host alias configured on another machine. Do not change credentials, SSH settings, Git configuration, or remotes.

Set `$repoUrl` to the exact repository URL from the previously verified Windows endpoint. Set `$sshTarget` to that endpoint's SSH target. Do not print either value. If no verified endpoint is available, keep evidence local and report publication `BLOCKED`.

Use this unique evidence ref format:

```text
evidence/m7-final-r1-rerun-6c17442-disk-audit-<UTC>-<random-hex>
```

Before creating the evidence repository, verify the SSH identity in the current Windows session. Save raw output and exit on Windows. Do not publish the greeting, account name, SSH target, or URL.

```powershell
$sshOut = Join-Path $rawRoot 'ssh.stdout.txt'
$sshErr = Join-Path $rawRoot 'ssh.stderr.txt'
if ((Test-Path -LiteralPath $sshOut) -or (Test-Path -LiteralPath $sshErr)) { throw 'Raw output exists. Start a new run.' }
ssh.exe -o BatchMode=yes -T $sshTarget 1> $sshOut 2> $sshErr
$sshExit = $LASTEXITCODE
Write-Host "ssh identity check exit_code=$sshExit"
```

Continue only if the raw greeting matches the authorized account for this Windows assignment. GitHub may return exit 1 after a successful greeting because it does not provide shell access. If the identity differs or cannot be verified, keep evidence local and report `BLOCKED`. Do not switch accounts or try another endpoint.

Check that the exact evidence ref is unused. Save stdout, stderr, and exit status locally. If the check fails, keep evidence local and report `BLOCKED`. If the ref exists, generate a new random suffix before creating the temporary repository.

```powershell
$refOut = Join-Path $rawRoot 'ref-check.stdout.txt'
$refErr = Join-Path $rawRoot 'ref-check.stderr.txt'
if ((Test-Path -LiteralPath $refOut) -or (Test-Path -LiteralPath $refErr)) { throw 'Raw output exists. Start a new run.' }
git.exe ls-remote --heads $repoUrl "refs/heads/$evidenceRef" 1> $refOut 2> $refErr
$refCheckExit = $LASTEXITCODE
Write-Host "git ls-remote pre-push exit_code=$refCheckExit"
```

Create a new temporary repository under `$runRoot`. Do not use the source checkout. This repository contains only the four sanitized files.

```powershell
$publishRepo = Join-Path $runRoot 'publish-repo'
if (Test-Path -LiteralPath $publishRepo) { throw 'Publish repository already exists. Start a new run.' }
git.exe init --initial-branch $evidenceRef $publishRepo
$initExit = $LASTEXITCODE
Write-Host "git init exit_code=$initExit"
if ($initExit -ne 0) { throw 'Could not create the evidence-only repository.' }

foreach ($name in @('SUMMARY.md', 'CACHE-INVENTORY.csv', 'COMMAND-EXITS.csv', 'SHA256SUMS.txt')) {
    $source = Join-Path $publishRoot $name
    $destination = Join-Path $publishRepo $name
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Missing sanitized file: $name" }
    if (Test-Path -LiteralPath $destination) { throw "Refusing to overwrite: $name" }
    Copy-Item -LiteralPath $source -Destination $destination -ErrorAction Stop
}

git.exe -C $publishRepo add -- SUMMARY.md CACHE-INVENTORY.csv COMMAND-EXITS.csv SHA256SUMS.txt
$addExit = $LASTEXITCODE
Write-Host "git add exit_code=$addExit"
if ($addExit -ne 0) { throw 'Could not stage the sanitized evidence files.' }
$stagedFiles = @(git.exe -C $publishRepo diff --cached --name-only)
$stagedExit = $LASTEXITCODE
$expectedFiles = @('CACHE-INVENTORY.csv', 'COMMAND-EXITS.csv', 'SHA256SUMS.txt', 'SUMMARY.md')
if ($stagedExit -ne 0 -or (Compare-Object -ReferenceObject $expectedFiles -DifferenceObject $stagedFiles)) {
    throw 'Staged file list differs from the four sanitized evidence files.'
}

$commitOut = Join-Path $rawRoot 'commit.stdout.txt'
$commitErr = Join-Path $rawRoot 'commit.stderr.txt'
if ((Test-Path -LiteralPath $commitOut) -or (Test-Path -LiteralPath $commitErr)) { throw 'Raw output exists. Start a new run.' }
git.exe -C $publishRepo -c 'user.name=M7 Windows disk audit' -c 'user.email=noreply@github.com' commit -m 'M7 Windows disk-space diagnostic' 1> $commitOut 2> $commitErr
$commitExit = $LASTEXITCODE
Write-Host "git commit exit_code=$commitExit"
if ($commitExit -ne 0) { throw 'Evidence commit failed. Keep the raw and sanitized files local.' }
$commitHashOut = Join-Path $rawRoot 'commit-hash.stdout.txt'
$commitHashErr = Join-Path $rawRoot 'commit-hash.stderr.txt'
if ((Test-Path -LiteralPath $commitHashOut) -or (Test-Path -LiteralPath $commitHashErr)) { throw 'Raw output exists. Start a new run.' }
git.exe -C $publishRepo rev-parse HEAD 1> $commitHashOut 2> $commitHashErr
$commitHashExit = $LASTEXITCODE
Write-Host "git rev-parse evidence commit exit_code=$commitHashExit"
if ($commitHashExit -ne 0) { throw 'Could not capture the evidence commit SHA.' }
```

The evidence commit uses a generic nonpersonal identity. Do not use a private email or add account names to the published files. Do not modify existing Git configuration.

Push only the sanitized evidence commit to the new ref. Do not push the source worktree, raw evidence, tags, or any other ref. Capture push and post-push output and producer exits locally.

```powershell
$pushOut = Join-Path $rawRoot 'push.stdout.txt'
$pushErr = Join-Path $rawRoot 'push.stderr.txt'
if ((Test-Path -LiteralPath $pushOut) -or (Test-Path -LiteralPath $pushErr)) { throw 'Raw output exists. Start a new run.' }
git.exe -C $publishRepo push --no-follow-tags $repoUrl "HEAD:refs/heads/$evidenceRef" 1> $pushOut 2> $pushErr
$pushExit = $LASTEXITCODE
Write-Host "git push exit_code=$pushExit"
if ($pushExit -ne 0) { throw 'Push failed. Preserve raw evidence and report BLOCKED.' }
$verifyOut = Join-Path $rawRoot 'ref-verify.stdout.txt'
$verifyErr = Join-Path $rawRoot 'ref-verify.stderr.txt'
if ((Test-Path -LiteralPath $verifyOut) -or (Test-Path -LiteralPath $verifyErr)) { throw 'Raw output exists. Start a new run.' }
git.exe ls-remote --heads $repoUrl "refs/heads/$evidenceRef" 1> $verifyOut 2> $verifyErr
$verifyExit = $LASTEXITCODE
Write-Host "git ls-remote post-push exit_code=$verifyExit"
if ($verifyExit -ne 0) { throw 'Could not verify the new evidence ref.' }
```

Verify that the remote SHA equals the captured commit SHA. Keep the raw verification output on Windows. Publish only the SHA and pass/fail result. Preserve the complete raw directory after publication.

Stop the transcript after all raw measurements and command outputs are captured. Do not copy the transcript or local redaction map to the publish directory.


## Report the result

Report the source SHA, addendum SHA, run ID, sanitized evidence ref and commit, free bytes by volume token, eligible cache sizes by path token, excluded candidate count by reason, and each producer exit. Report the raw evidence location only as a stable local placeholder such as `RAW_ROOT_001`. Keep its exact mapping on Windows. Never send or publish the raw path.

Report candidate remedies by path token, size, volume token, and owner proof. Examples include requesting separate authorization for cleanup of a confirmed task-owned cache or using a verified volume for a future build. Do not execute remedies or change environment variables.
