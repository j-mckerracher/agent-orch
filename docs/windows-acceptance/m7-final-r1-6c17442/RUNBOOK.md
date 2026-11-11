# M7 final Windows acceptance runbook, round 1 (commit 6c17442)

**This is the ONLY runbook you follow.** Every other runbook (the `b44f88a` one, `docs/evidence/*-runbook.md`, older
`priority-*` files) is **superseded**; read the source tree for facts only. If a step cannot run, record `NOT RUN` and
the reason. Never edit scripts, expected counts or tests to make a gate pass. Never skip silently.

Repository: `git@github.com:j-mckerracher/agent-orch.git` (SSH alias the machine already has; no tokens, no `gh`).
Source branch: `orch/m7-final-r1-6c17442`. This branch (`docs/m7-final-r1-windows-acceptance-6c17442`) holds only this file.

| Item | Value |
|---|---|
| Commit to test | `6c17442eaee5243b48034dfc53d7f43d25f1ab82` (`6c17442`) |
| Tree | `9b9612ab6084c644e344603028149084118cc889` |
| Evidence ref (you push) | `evidence/m7-final-r1-windows-acceptance-6c17442-<UTC>-<hex>` |

## What changed since `b44f88a` (why you are rerunning)

The `b44f88a` run failed M7-08 (`relay closed: Remote`), M7-25 (`conversations.send` stuck in Conflict) and a cascade
of 17 gates caused by a leaked `fake_agent.exe`. This commit contains:

- **M7-08 fix (hypothesis, unproven on Windows):** the Windows named-pipe stream now offers at most 32 KiB per write,
  so a large frame (the post-flood `conversations.get`) is not refused whole by the 64 KiB non-blocking pipe.
  New test `a_write_larger_than_the_pipe_buffer_arrives_whole`. The session loop now prints
  `orchestrator-ipc: response write failed, closing connection: <error>` or `request read failed, closing connection`
  to the daemon stderr, which names the real trigger if the fix is not enough.
- **M7-25 diagnostics + locator fix:** one Windows-aware program locator (`.exe`, then `.cmd`; never the extensionless
  npm shim) is used for the version probe, the catalog and the launch. Failed probes (login status, debug models,
  auth status, `--help`, `--version`) now log `orchestrator-agents:` / `orchestrator-daemon:` lines with the probe,
  the resolved path, the exit code or failure kind and a masked 160-character stderr excerpt.
- **Survivor fix:** the test `Daemon` runs `taskkill /PID <pid> /T /F` before killing the daemon, on every failure path
  and on drop, so no `fake_agent.exe` outlives a test.
- **Harness counts** now use MEASURED Windows values (see section 2).

## Labels

**MEASURED** = you ran it here and attach the output. **DERIVED** = read from code or another platform.
**UNPROVEN** = not tested. Report only MEASURED values as results. If a measured count differs from the table, that is
a finding; do not edit `scripts/ci/expected.mjs`.

Expectations: ipc 56 (55 measured at `b44f88a` + 1 new test); windows-transport 14 passed / 42 filtered out;
daemon 138 + 1 ignored (118 lib + 11 conversation_runtime + others; DERIVED, the `b44f88a` run stopped inside
`conversation_runtime`); store 153 + 1 ignored; tauri 14; node self-tests 97; vitest 1105 (51 files);
the harness prints the real gate list (record the number).

Stay **UNPROVEN** unless truly tested: clean machine, code signing, SmartScreen, UAC, Defender reputation, second
local user named-pipe denial, remote-client rejection.

## 0. Rules

- No `SUBST`, mapped drive or junction. Use a short real path such as `C:\t\<8 hex>` (harness preflight exits 3 on
  aliased or > 64 character roots; exit 3 means fix the path).
- Throwaway user or VM preferred (the installer test touches `%LOCALAPPDATA%\agent-orch`).
- Never push `main` or any branch other than your evidence ref. No force pushes. No e-mail, token or personal path in
  pushed evidence (section 9).
- Nothing is retried silently: keep both outputs of a rerun and say why.

## 1. Setup (MEASURED)

```powershell
$ErrorActionPreference = 'Stop'
$Hex = -join ((1..8) | ForEach-Object { '{0:x}' -f (Get-Random -Max 16) })
$Utc = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
$Root = "C:\t\$Hex"
New-Item -ItemType Directory -Force -Path $Root | Out-Null
$Src = Join-Path $Root 's'; $Ev = Join-Path $Root 'ev'; $Tgt = Join-Path $Root 'tg'; $Tmp = Join-Path $Root 'tmp'
New-Item -ItemType Directory -Force -Path $Ev, $Tmp | Out-Null
git clone --no-checkout git@github.com:j-mckerracher/agent-orch.git $Src
Set-Location $Src
git fetch origin orch/m7-final-r1-6c17442
git checkout --detach 6c17442eaee5243b48034dfc53d7f43d25f1ab82
git config --local user.name  <your noreply name>
git config --local user.email <your GitHub noreply address>      # never a work or school address
git rev-parse HEAD            | Tee-Object (Join-Path $Ev 'head.txt')   # 6c17442eaee5243b48034dfc53d7f43d25f1ab82
git rev-parse 'HEAD^{tree}'   | Tee-Object (Join-Path $Ev 'tree.txt')   # 9b9612ab6084c644e344603028149084118cc889
git config --get core.autocrlf | Tee-Object (Join-Path $Ev 'autocrlf.txt')
git status --porcelain=v1 --untracked-files=all | Tee-Object (Join-Path $Ev 'status-before.txt')
[System.Environment]::OSVersion.VersionString | Tee-Object (Join-Path $Ev 'os.txt')
"node=$(node -v) npm=$(npm.cmd -v) rustc=$(rustc -V) cargo=$(cargo -V) pwsh=$($PSVersionTable.PSVersion)" | Tee-Object (Join-Path $Ev 'versions.txt')
```

Stop if HEAD or tree differs. Use `npm.cmd`, never `npm.ps1`. `EBADENGINE` warnings are fine; record `node -v`.

## 2. Full gate harness (MEASURED, at the new pin)

```powershell
node scripts/ci/run-gates.mjs --preflight --evidence-dir $Ev\gates --target-dir $Tgt --test-tmp $Tmp
node scripts/ci/run-gates.mjs --list
node scripts/ci/run-gates.mjs --evidence-dir $Ev\gates --target-dir $Tgt --test-tmp $Tmp 2>&1 | Tee-Object (Join-Path $Ev 'gates.console.txt')
"exit=$LASTEXITCODE" | Tee-Object (Join-Path $Ev 'gates.exit')
```

Exit 0 = PASS, 1 = FAIL, 2 = INCONCLUSIVE, 3 = preflight refusal. The evidence directory must not exist before the run.
After the run, list leftover processes: `Get-Process fake_agent, orchestrator-daemon -ErrorAction SilentlyContinue |
Tee-Object (Join-Path $Ev 'leftover-after-gates.txt')`. Any `fake_agent` there is a finding (the tree-kill failed).

Record the gate count, the tally, and per gate the measured value next to the expectation:

| Gate | Expected | Measured | Equal? |
|---|---|---|---|
| ipc | 56 | | |
| windows-transport | 14 passed, 42 filtered out | | |
| daemon | 138 + 1 ignored | | |
| store | 153 + 1 ignored | | |
| lock-takeover | 1 | | |
| tauri | 14 | | |
| node self-tests | 97 | | |
| vitest | 1105 (51 files) | | |
| gates | (record) | | |

If `daemon-tests` fails in `conversation_runtime`, go straight to section 4 and attach its output; the daemon stderr
now carries the diagnostic lines described there.

## 3. Release gates: package, release_smoke, install-smoke, silent uninstall (MEASURED)

```powershell
$env:CARGO_TARGET_DIR = $Tgt
Push-Location apps/desktop; npm.cmd ci; Pop-Location
$Out = Join-Path $Root 'pkg'; $Work = Join-Path $Root 'w'
node scripts/release/package.mjs windows --out $Out 2>&1 | Tee-Object (Join-Path $Ev 'package.txt')
if ($LASTEXITCODE -ne 0) { throw "package exited $LASTEXITCODE" }
cargo build --locked -p orchestrator-daemon --example release_smoke 2>&1 | Tee-Object (Join-Path $Ev 'release-smoke-build.txt')
$Smoke = Join-Path $Tgt 'debug\examples\release_smoke.exe'
node scripts/release/install-smoke.mjs windows --artifact-dir $Out --work $Work --smoke-bin $Smoke `
  --allow-unsigned --out (Join-Path $Ev 'install-smoke-win.json') 2>&1 | Tee-Object (Join-Path $Ev 'install-smoke-win.txt')
"exit=$LASTEXITCODE" | Tee-Object (Join-Path $Ev 'install-smoke-win.exit')
Get-Process orchestrator-daemon, release_smoke -ErrorAction SilentlyContinue | Tee-Object (Join-Path $Ev 'leftover-processes.txt')
Get-AuthenticodeSignature (Get-ChildItem $Out -Filter *.exe -Recurse | Select-Object -First 1).FullName |
  Format-List * | Out-String | Tee-Object (Join-Path $Ev 'authenticode.txt')
git status --porcelain=v1 --untracked-files=all | Tee-Object (Join-Path $Ev 'status-after.txt')
git diff --stat | Tee-Object (Join-Path $Ev 'diff-stat-after.txt')
```

Expected: exit 0 with `--allow-unsigned`; PASS for artifact dir, silent install, product version, installed files,
`release_smoke: PASS`, silent uninstall, uninstall removed the app, **sentinel survived**; INFO `NotSigned`;
`upgrade: NOT proven (none)` without an older build. Record the tree hash (`tree.txt` and the `source` block of
`install-smoke-win.json`), the literal Authenticode `Status`/`StatusMessage`, and `checkout_clean_after_run`
(`True`/`False`; if `False`, attach before/after status, the changed paths and the cause). Delete `$Work` by hand
afterwards, and the sentinel and `%LOCALAPPDATA%\agent-orch` only if you created them.

## 4. Conversation runtime diagnostic (MEASURED, run even if section 2 passed)

This pinpoints M7-08 and M7-25 in one run. Run it in the repo root, with `$env:CARGO_TARGET_DIR = $Tgt`.

```powershell
$D = Join-Path $Ev 'm7-diag'; New-Item -ItemType Directory -Force -Path $D | Out-Null
# 4a. kill anything left from earlier gates and record the baseline
Get-Process fake_agent, orchestrator-daemon, codex -ErrorAction SilentlyContinue | Tee-Object (Join-Path $D 'procs-before.txt')
# 4b. the conversation_runtime suite with all daemon stderr visible
cargo build --locked -p orchestrator-daemon --examples 2>&1 | Tee-Object (Join-Path $D 'examples-build.txt')
cargo test --locked -p orchestrator-daemon --test conversation_runtime -- --nocapture --test-threads=1 2>&1 |
  Tee-Object (Join-Path $D 'conversation_runtime.txt')
"exit=$LASTEXITCODE" | Tee-Object (Join-Path $D 'conversation_runtime.exit')
Get-Process fake_agent, orchestrator-daemon -ErrorAction SilentlyContinue | Tee-Object (Join-Path $D 'procs-after.txt')
# 4c. the other M7 backend suites
cargo test --locked -p orchestrator-daemon --test conversations 2>&1 | Tee-Object (Join-Path $D 'conversations.txt')
cargo test --locked -p orchestrator-daemon --test queue_snapshot 2>&1 | Tee-Object (Join-Path $D 'queue_snapshot.txt')
cargo test --locked -p orchestrator-ipc windows_transport 2>&1 | Tee-Object (Join-Path $D 'windows_transport.txt')
cargo test --locked -p orchestrator-store 2>&1 | Tee-Object (Join-Path $D 'store.txt')
```

What to read and report:

1. **M7-08** (`agent_failures_and_oversized_replies_fail_the_turn_with_stable_codes`): PASS or FAIL. If it fails,
   copy every `orchestrator-ipc:` line from `conversation_runtime.txt`. `response write failed ... <error>` means the
   pipe write path; `request read failed` means the framing; neither line means the daemon dropped the connection
   deliberately (then report the panic line and the last `orchestrator-daemon:` lines before it).
2. **Survivors:** `procs-after.txt` must list no `fake_agent`. If one remains, report its PID, parent PID and
   whether a Job Object holds it (`Get-CimInstance Win32_Process -Filter "Name='fake_agent.exe'"`).
3. The probe lines. They show which probe made the catalog Unknown, with the resolved path, exit code and a
   stderr excerpt (this is the M7-25 root cause if a real agent is installed):

```powershell
Select-String -Path (Join-Path $D 'conversation_runtime.txt') -Pattern 'orchestrator-agents:|orchestrator-daemon:|orchestrator-ipc:' |
  ForEach-Object { $_.Line } | Tee-Object (Join-Path $D 'diagnostic-lines.txt')
```

4. The real CLI probes, by hand, for the agent you will use in section 5 (record exact output and `$LASTEXITCODE`;
   redact anything token-like):

```powershell
$P = Join-Path $D 'codex-probes.txt'
where.exe codex                              2>&1 | Tee-Object -FilePath $P
(Get-Command codex).Source                   2>&1 | Tee-Object -FilePath $P -Append
codex --version                              2>&1 | Tee-Object -FilePath $P -Append; "exit=$LASTEXITCODE" | Tee-Object -FilePath $P -Append
codex login status                           2>&1 | Tee-Object -FilePath $P -Append; "exit=$LASTEXITCODE" | Tee-Object -FilePath $P -Append
codex debug models                           2>&1 | Select-Object -First 5 | Tee-Object -FilePath $P -Append; "exit=$LASTEXITCODE" | Tee-Object -FilePath $P -Append
```

The daemon pins Codex `0.167.0` (`KNOWN_PINS`). At `b44f88a` the machine had Codex `0.20.0`, which probably lacks
`codex login status`; the catalog then stays Unknown and `conversations.send` retries Conflict for 15 s. To test at
the pin: `npm.cmd i -g @openai/codex@0.167.0`, sign in, rerun the probes and section 5. To test the installed version
instead, set `$env:AGENT_ORCH_CONVERSATION_PINS` as described in `docs/m7-08-conversation-service.md` and report the
version. Report: resolved path (is it `.exe`, `.cmd` or a shim), version, each probe's exit code, and the first
`orchestrator-agents:` line.

## 5. M7-25 live conversation exchange (MEASURED, needs a real signed-in agent)

Prerequisite: section 4 shows `codex --version` = `0.167.0` (or a pin override you report) and `codex login status`
exiting 0. If no signed-in agent exists, record **NOT RUN** and the reason; never substitute the fake.

```powershell
$env:AGENT_ORCH_LIVE_CONVERSATION = '1'
cargo test --locked -p orchestrator-daemon --test conversation_runtime live_codex_answers_a_global_message -- --ignored --nocapture 2>&1 |
  Tee-Object (Join-Path $Ev 'm7-25-live.txt')
Remove-Item Env:AGENT_ORCH_LIVE_CONVERSATION
Get-Process codex, node, orchestrator-daemon -ErrorAction SilentlyContinue | Tee-Object (Join-Path $Ev 'leftover-processes-live.txt')
```

The test sends "Reply with exactly: pong" and requires a completed turn whose reply contains `pong` and no authority
granted. While the turn runs (second shell) list the process tree
(`Get-CimInstance Win32_Process | Select ProcessId, ParentProcessId, Name`) and note that the agent is a descendant of
the daemon. Report: Codex version, turn state, reply length, process-tree observation, leftovers. A repeat of the
Conflict loop is a failure; attach `m7-25-live.txt` and the `orchestrator-agents:` / `orchestrator-daemon:` lines.

## 6. M7-26 estimated-cost check (MEASURED)

```powershell
cargo test --locked -p orchestrator-store usage 2>&1 | Tee-Object (Join-Path $Ev 'm7-26-store-usage.txt')
cargo test --locked -p orchestrator-daemon usage_tests 2>&1 | Tee-Object (Join-Path $Ev 'm7-26-daemon-usage.txt')
```

Measured at `b44f88a`: store usage 29 tests, daemon `usage_tests` 8 tests. (The old filter `api_usage` matched 0 tests
because the module is `usage_tests`; do not use it.) Pass criteria (`docs/usage-ledger.md`, "Estimated cost (M7-26)"):
estimates only under `estimated.costs` (USD), never inside `reported`; a missing or unpriced model gives **no** estimate
(never 0); a session is estimated whole or not at all; a stored reported cost is never replaced. If section 5 produced a
Codex receipt with tokens and no cost, query the usage summary (UI Usage view or `usage.summary`) and record the
estimated figure, the model (a Codex run records the **requested** model) and that it is labelled an estimate; no model
means unknown, not 0. State what you observed.

## 7. Windows UI checklist M7-32 (MEASURED by hand, interactive)

**Operator launch needs no per-action confirmation from the user.** The agent running this runbook proceeds on the
user's standing authorization for local UI testing of this app: it installs and launches the app, drives it, and takes
screenshots without asking before each action. This covers only this app on this machine; it does not extend to other
applications, network changes, or anything outside the evidence directory.

Open `docs/evidence/m7-30/WINDOWS-MANUAL-CHECKLIST.md` (source tree) and complete **every row**. Use the app installed
by the real NSIS installer (record install, launch, uninstall). For each row record tester, date, build (`6c17442`,
tree), WebView2 version, PASS / FAIL / N-A, a **screenshot path** (save under `$Ev\screens\`) and notes. No blank rows;
`NOT RUN` with a reason is acceptable. Areas:

1. Environment: Windows build, WebView2 version, `color-mix()`, `100dvh`, `:has()`, motion and colour-scheme support.
2. Themes: light, dark, persistence, Windows **Contrast themes** (forced-colors).
3. Responsive and DPI: widths 1280, 1150, 960, 760, 480, 390; scaling 100 / 125 / 150 / 200 %.
4. Keyboard only: skip link, tab order, tree keys, Ctrl+Enter, no traps.
5. **Screen reader: Narrator** (NVDA if installed): landmarks, live regions, mutation announcements, disabled reason, tree.
6. Reduced motion ("Show animations" off).
7. **Connection failure and recovery**: stop the daemon (strip shows Reconnecting, stale data, no fixture data); restart
   (Connected, refreshed); idle relaunch after 60 s; named-pipe handshake, reconnect, shutdown on exit.
8. **Approvals and mutations**: approve / reject / request changes on a real candidate, idempotent on retry; workstream
   create/update/link and message send; `?fixture=1` mutations never reach the daemon.
9. Data honesty: no illustrative values in live mode (20/24 slots, $26.00, CPU 41/44); host metrics from the Windows daemon.
10. Security: production CSP in the WebView; paths with spaces and non-ASCII characters.

Save the filled copy as `$Ev\WINDOWS-MANUAL-CHECKLIST.completed.md` (do not edit the source file).

## 8. N-1 upgrade (MEASURED if feasible, else UNPROVEN)

Only with the `package.mjs windows --out` directory of an earlier, different-version release; otherwise record
`UNPROVEN: no N-1 artifact`. A same-version rehearsal must be labelled `same-version rehearsal, upgrade NOT proven`.

```powershell
node scripts/release/install-smoke.mjs windows --artifact-dir $Out --old-artifact-dir <N-1 pkg dir> --work (Join-Path $Root 'w2') `
  --smoke-bin $Smoke --allow-unsigned --out (Join-Path $Ev 'install-smoke-upgrade.json') 2>&1 | Tee-Object (Join-Path $Ev 'install-smoke-upgrade.txt')
```

## 9. Evidence and push

Write `$Ev\REPORT.md`: commit, tree, OS build, versions, each section's verdict (MEASURED / DERIVED / UNPROVEN /
NOT RUN), the section 2 table with measured values, the section 4 findings (M7-08 outcome and trigger lines, M7-25
probe outcome, survivors), Authenticode status, `checkout_clean_after_run` with explanation, and what stayed UNPROVEN.

Privacy check before pushing (do not print matches): search the evidence for your work or school e-mail domain and
your user name; replace or delete matches and rerun until empty. Do not withhold the whole set for one file.

```powershell
$Ref = "evidence/m7-final-r1-windows-acceptance-6c17442-$Utc-$Hex"
$Ew = Join-Path $Root 'evgit'
git -C $Src worktree add --detach $Ew 6c17442eaee5243b48034dfc53d7f43d25f1ab82
New-Item -ItemType Directory -Force -Path (Join-Path $Ew "evidence-out") | Out-Null
Copy-Item -Recurse -Force "$Ev\*" (Join-Path $Ew "evidence-out")
Set-Location $Ew
git add -f evidence-out
git commit -m "Windows acceptance evidence for 6c17442 ($Utc)"
git push origin "HEAD:refs/heads/$Ref"
git ls-remote origin "refs/heads/$Ref"
```

Do not push build output, `node_modules` or `target/`. Report the ref, pushed SHA and the `ls-remote` line.

## 10. Final verdict wording

Use exactly `WINDOWS ACCEPTANCE: PASS / FAIL / INCONCLUSIVE` for the harness, then one line per item in sections 3 to 8.
Never write PASS for a derived count, signing, SmartScreen, UAC, clean-machine install, or an unrun GUI row.
