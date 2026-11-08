# Windows verification runbook for 5fd2edc (agent-facing)

Recorded: 2026-11-08 (America/Denver, -07:00). Coordinator session 798b2154-8e8a-4d39-af3e-0b1bd7f1cb71.
Canonical location: https://github.com/j-mckerracher/agent-orch/blob/orch/windows-runbook-5fd2edc/docs/evidence/windows-5fd2edc-runbook.md
Branch: `orch/windows-runbook-5fd2edc`. Its first parent is the pinned source commit below.

## Pinned source

- Source commit (the first parent of this runbook's commit): `5fd2edcddc207d753a10179195262213ce41d638`.
- Every test in this runbook must run on that commit. Do not use any other commit or branch as the source.

## Read this first: wrong turns that already happened

An earlier Windows run pushed evidence to `codex/m1-03-windows-evidence-20261108T140653Z-3e636f72-acc3-46f1-96e5-e64f81051b9c` (commit f132fab). That run is NOT valid for this runbook:
- It tested source commit `649c137d7bcfcf2445d2a107162862cde0a1c0ed` (the M1-03 candidate), not `5fd2edc`.
- It was written under `docs/evidence/m1-03-windows/`, which is the M1-03 folder, not this one.
- It did not run the conformance suite (M4-04) or the end-to-end task (M0 and M1 criteria).
Its result (62 passed on Windows at 649c137) is kept as M1-03 evidence only.

Do NOT:
- use the `codex/m1-03-windows-evidence-*` branches or the `docs/evidence/m1-03-windows/` folders as a source or a template;
- use a source other than the pinned commit;
- use `danger-full-access` for Codex, or count any run as acceptance evidence if it used it;
- run anything inside a checkout that you did not clone from the URL in this runbook.

## Step 0 — Preflight

PowerShell 7 or newer is required. Windows PowerShell 5.1 can turn native stderr into a terminating error.

```powershell
$PSVersionTable.PSVersion     # must show 7.x; if not, stop and report
```

## Step 1 — Clone the runbook branch (the pinned source is its parent)

```powershell
$RUN_ID = "$((Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ'))-$PID"
$REPO = Join-Path $env:USERPROFILE "orch-win-$RUN_ID"      # must not already exist
git clone --branch orch/windows-runbook-5fd2edc https://github.com/j-mckerracher/agent-orch.git $REPO
git -C $REPO rev-parse HEAD^                                 # must print 5fd2edcddc207d753a10179195262213ce41d638
git -C $REPO diff --name-only HEAD^ HEAD                     # must print exactly: docs/evidence/windows-5fd2edc-runbook.md
```

If the parent is not `5fd2edc…`, or the diff lists any other file, STOP and report. Do not continue with a different source.

## Step 2 — Evidence folder (new, outside the repository)

```powershell
$RUN = Join-Path $env:USERPROFILE "orch-windows-evidence\win-5fd2edc-$RUN_ID"
New-Item -ItemType Directory -Path $RUN       # no -Force: it must not already exist
"run_id=$RUN_ID" | Set-Content "$RUN\run.txt"
git -C $REPO rev-parse HEAD | Set-Content "$RUN\head.txt"
git -C $REPO status --porcelain | Set-Content "$RUN\status-before.txt"   # must be empty
```

## Step 3 — Toolchain

```powershell
cargo --version > "$RUN\cargo.txt" 2>&1
rustc --version > "$RUN\rustc.txt" 2>&1
where.exe codex > "$RUN\codex-where.txt" 2>&1
codex --version > "$RUN\codex-version.txt" 2>&1; $LASTEXITCODE | Set-Content "$RUN\codex-version.exit.txt"
codex login status > "$RUN\codex-login.txt" 2>&1; $LASTEXITCODE | Set-Content "$RUN\codex-login.exit.txt"
where.exe claude > "$RUN\claude-where.txt" 2>&1
where.exe pi > "$RUN\pi-where.txt" 2>&1
```

Record every path. If `codex` resolves to more than one binary, use the one `where.exe` lists first, and say which one.

## Step 4 — Workspace tests and clippy (no pipes; read $LASTEXITCODE directly)

```powershell
Push-Location $REPO
cargo test --locked --workspace > "$RUN\test.stdout.txt" 2> "$RUN\test.stderr.txt"
$LASTEXITCODE | Set-Content "$RUN\test.exit.txt"
cargo clippy --locked --workspace --all-targets > "$RUN\clippy.stdout.txt" 2> "$RUN\clippy.stderr.txt"
$LASTEXITCODE | Set-Content "$RUN\clippy.exit.txt"
git diff --check HEAD^ HEAD > "$RUN\diff-check.txt" 2>&1
$LASTEXITCODE | Set-Content "$RUN\diff-check.exit.txt"
Pop-Location
```

Expected on macOS for this exact source commit: `test.exit.txt` = 0 with 295 passed and 0 failed across all suites; `clippy.exit.txt` = 0 with zero warnings. Record the actual totals. To read the totals, search `test.stdout.txt` for lines beginning with `test result:` and add up the passed, failed, and ignored counts, as the coordinator did.

## Step 5 — Conformance suite (M4-04)

```powershell
Push-Location $REPO
cargo test --locked -p orchestrator-agents --test conformance > "$RUN\conformance.stdout.txt" 2> "$RUN\conformance.stderr.txt"
$LASTEXITCODE | Set-Content "$RUN\conformance.exit.txt"
Pop-Location
```

Expected: exit 0, with 24 tests passing (8 scenarios each for Codex, Claude Code, and Pi). If a test fails, save the failing names and messages exactly, and do not edit code.

## Step 6 — One real product task (M0 and M1 criteria)

Use the exact `--objective` value from line 28 of `docs/evidence/m1-04-macos/run.md` in this clone. Do not retype it; copy it.

```powershell
$E2E = Join-Path $env:TEMP "orch-win-e2e-$RUN_ID"
New-Item -ItemType Directory -Path $E2E                      # no -Force
New-Item -ItemType Directory -Path "$E2E\repo"
git -C "$E2E\repo" init > "$RUN\e2e-init.txt" 2>&1
git -C "$E2E\repo" config user.email "orch-runbook@example.invalid"
git -C "$E2E\repo" config user.name "orch runbook"
Set-Content -Path "$E2E\repo\README.md" -Value "Windows runbook seed."
git -C "$E2E\repo" add README.md
git -C "$E2E\repo" commit -m "seed" > "$RUN\e2e-seed.txt" 2>&1
git -C "$E2E\repo" status --porcelain | Set-Content "$RUN\e2e-primary-before.txt"

Push-Location $REPO
cargo run --locked -p orchestrator-cli -- --data-dir "$E2E\data" run --repo "$E2E\repo" --task-id win-e2e --objective "<paste the objective here>" > "$RUN\e2e-run.stdout.txt" 2> "$RUN\e2e-run.stderr.txt"
$LASTEXITCODE | Set-Content "$RUN\e2e-run.exit.txt"
cargo run --locked -p orchestrator-cli -- --data-dir "$E2E\data" inspect win-e2e > "$RUN\e2e-inspect.txt" 2>&1
cargo run --locked -p orchestrator-cli -- --data-dir "$E2E\data" diff win-e2e > "$RUN\e2e-diff.txt" 2>&1
Pop-Location
git -C "$E2E\repo" status --porcelain | Set-Content "$RUN\e2e-primary-after.txt"
Compare-Object (Get-Content "$RUN\e2e-primary-before.txt") (Get-Content "$RUN\e2e-primary-after.txt")   # the primary repo must not change
```

Expected: `e2e-run.exit.txt` = 0; `e2e-diff.txt` adds exactly one line to README.md; the `Compare-Object` prints nothing. If the Codex sandbox fails (for example `CreateProcessWithLogonW` error 1385), keep every file, record the exit code and stderr, and stop. Do not switch sandbox modes.

## Step 7 — Publish the evidence on its own branch (required)

Evidence must reach GitHub, so the coordinator can inspect it. Commit it in the clone on a new branch, and touch no other file:

```powershell
$EVIDENCE_BRANCH = "evidence/windows-5fd2edc-$RUN_ID"
$DEST = Join-Path $REPO "docs\evidence\windows-5fd2edc\$RUN_ID"
git -C $REPO switch -c $EVIDENCE_BRANCH
New-Item -ItemType Directory -Path $DEST                    # no -Force
Copy-Item "$RUN\*" $DEST -Recurse
git -C $REPO add "docs/evidence/windows-5fd2edc/$RUN_ID"
git -C $REPO commit -m "evidence: Windows run of 5fd2edc $RUN_ID" -m "Co-authored-by: Copilot App <223556219+Copilot@users.noreply.github.com>"
git -C $REPO push -u origin $EVIDENCE_BRANCH
git -C $REPO rev-parse HEAD                                   # report this SHA
```

If the push is denied, keep the folder and report the exact error. Do not force anything.

## Step 8 — Report back

Send the coordinator: the evidence branch name and its commit SHA; the `head.txt` value; each `*.exit.txt` value; the test totals; the `where.exe` output for codex; the Step 5 failures, if any, with messages; the Step 6 outcome and diff; and anything that did not match the expected outcomes.

## What this runbook does not cover

- Windows named-pipe IPC (M5-01): not implemented yet.
- Unix-only behavior such as process groups; that stays macOS-only.
- Signed installers (M6-04).
