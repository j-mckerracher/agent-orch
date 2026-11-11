# M7 final Windows acceptance runbook (commit b44f88a)

**This is the ONLY runbook you follow.** Every runbook inside the source tree (for example
`docs/evidence/m6-04-clean-machine-runbook.md`, older `priority-*` runbooks) is **superseded**. Read the source tree
for facts (scripts, the checklist file) only; do not follow its instructions. If this file and another disagree, this
file wins. If a step here cannot run, record `NOT RUN` and the reason; never skip silently and never edit scripts,
expected counts or tests to make a gate pass.

Repository: `git@github.com:j-mckerracher/agent-orch.git` (use the SSH alias the machine already has; no tokens, no `gh`).
Source branch: `orch/m7-final-b44f88a`.

| Item | Value |
|---|---|
| Commit to test | `b44f88af63cb1ea7c68b7a827465db466ef308a2` (`b44f88a`) |
| Tree | `79820e90846b09a64edf25fe8593ab751abbb323` |
| Evidence ref (you push) | `evidence/m7-final-windows-acceptance-b44f88a-<UTC>-<hex>` |

## Labels

**MEASURED** = you ran it on this machine and attach the output. **DERIVED** = read from code or another platform.
**UNPROVEN** = not tested. Report only MEASURED values as results. The expected counts below are DERIVED by the
author from `cfg` gates on macOS output; they were **not** measured on Windows. Report what you measure, and say
whether it equals the derived value. A different measured count is a finding to report, not a reason to edit
`scripts/ci/expected.mjs`.

Derived Windows expectations: ipc 55; daemon 138 + 1 ignored; store 153 + 1 ignored; lock-takeover 1; tauri 14;
node self-tests 97; vitest 1105 (51 files); 27 gates (the harness prints the real list; record the real number).

These stay **UNPROVEN** unless you truly test them: clean machine (this is a build box), code signing,
notarization (macOS only, N/A), SmartScreen, UAC, Defender reputation, second local user named-pipe denial,
remote-client rejection. Do not mark any of them PASS from an indirect result.

## 0. Rules

- Do not use a path from `SUBST`, a mapped drive or a junction. Use a short real path such as `C:\t\<8 hex>`. The
  harness preflight exits 3 on aliased or long (> 64 chars) roots; exit 3 means fix the path, not "gate failed".
- Throwaway user or VM preferred. The installer test touches `%LOCALAPPDATA%\agent-orch` with one sentinel file.
- Never push `main` or any branch other than your evidence ref. No force pushes. Do not put an e-mail address,
  token or personal path outside `C:\t` / your user profile name into evidence you push: run the redaction step in 9.
- Nothing is retried silently. If you rerun a gate, keep both outputs and say why.

## 1. Setup (MEASURED)

```powershell
$ErrorActionPreference = 'Stop'
$Hex = -join ((1..8) | ForEach-Object { '{0:x}' -f (Get-Random -Max 16) })
$Utc = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
$Root = "C:\t\$Hex"                      # short, real path
New-Item -ItemType Directory -Force -Path $Root | Out-Null
$Src = Join-Path $Root 's'; $Ev = Join-Path $Root 'ev'; $Tgt = Join-Path $Root 'tg'; $Tmp = Join-Path $Root 'tmp'
New-Item -ItemType Directory -Force -Path $Ev, $Tmp | Out-Null
git clone --no-checkout git@github.com:j-mckerracher/agent-orch.git $Src
Set-Location $Src
git fetch origin orch/m7-final-b44f88a
git checkout --detach b44f88af63cb1ea7c68b7a827465db466ef308a2
git config --local user.name  <your noreply name>
git config --local user.email <your GitHub noreply address>      # never a work or school address
git rev-parse HEAD            | Tee-Object (Join-Path $Ev 'head.txt')        # must be b44f88af63cb1ea7c68b7a827465db466ef308a2
git rev-parse 'HEAD^{tree}'   | Tee-Object (Join-Path $Ev 'tree.txt')        # must be 79820e90846b09a64edf25fe8593ab751abbb323
git config --get core.autocrlf | Tee-Object (Join-Path $Ev 'autocrlf.txt')
git status --porcelain=v1 --untracked-files=all | Tee-Object (Join-Path $Ev 'status-before.txt')
[System.Environment]::OSVersion.VersionString | Tee-Object (Join-Path $Ev 'os.txt')
"node=$(node -v) npm=$(npm.cmd -v) rustc=$(rustc -V) cargo=$(cargo -V) pwsh=$($PSVersionTable.PSVersion)" | Tee-Object (Join-Path $Ev 'versions.txt')
```

Stop if HEAD or tree differs from the table. Use `npm.cmd`, never the `npm.ps1` shim. `npm ci` may print
`EBADENGINE` warnings (Node 22.20.0 pin vs. `jsdom`); they are warnings, record `node -v`, do not change the pin.

## 2. Full gate harness (MEASURED)

```powershell
node scripts/ci/run-gates.mjs --preflight --evidence-dir $Ev\gates --target-dir $Tgt --test-tmp $Tmp
node scripts/ci/run-gates.mjs --list
node scripts/ci/run-gates.mjs --evidence-dir $Ev\gates --target-dir $Tgt --test-tmp $Tmp 2>&1 | Tee-Object (Join-Path $Ev 'gates.console.txt')
"exit=$LASTEXITCODE" | Tee-Object (Join-Path $Ev 'gates.exit')
```

Exit 0 = OVERALL PASS, 1 = FAIL, 2 = INCONCLUSIVE (something skipped), 3 = preflight refusal. The evidence directory
must not exist before the run. The harness covers: node self-tests (`scripts/**/*.test.mjs`), daemon build,
ipc / daemon / lock-takeover / coordinator-review / windows-transport tests, daemon-lifecycle, `p12-windows-checks`
(data-directory ACLs, command-line refusal), clippy-scoped, desktop `npm ci`, `tsc -b`, build, vitest, tauri build,
tauri tests, tauri clippy, and `desktop-package-win`.

Record, from `gates.console.txt` and each `<gate>.result.json`: the gate count, the PASS/FAIL/SKIPPED tally, and for
each counted gate the **measured** values next to the derived ones:

| Gate | Derived | Measured | Equal? |
|---|---|---|---|
| ipc | 55 | | |
| daemon | 138 + 1 ignored | | |
| store | 153 + 1 ignored | | |
| lock-takeover | 1 | | |
| tauri | 14 | | |
| node self-tests | 97 | | |
| vitest | 1105 (51 files) | | |
| gates | 27 | | |

**Node self-tests / test 71:** M6-04 failed one install-lib test (number 71 in the node run) on Windows because the
test built an expected path with a POSIX separator. This commit fixes it. Report `node-self-tests` PASS with the
measured count and state explicitly that the previously failing test now passes (name it from the TAP output). If it
still fails, report the exact assertion and paths.

## 3. Release gates: package, release_smoke, install-smoke, silent uninstall (MEASURED)

```powershell
$env:CARGO_TARGET_DIR = $Tgt
Push-Location apps/desktop; npm.cmd ci; Pop-Location
$Out = Join-Path $Root 'pkg'; $Work = Join-Path $Root 'w'          # keep --work short
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

Expected (DERIVED): exit 0 with `--allow-unsigned`; PASS for artifact dir, silent install (`/S`), product version,
installed files, `release_smoke: PASS`, silent uninstall, "uninstall removed the app" and **the sentinel survived the
uninstall**; INFO `Authenticode status NotSigned`; UNPROVEN SmartScreen/UAC/Defender; and, without an older build,
`upgrade: NOT proven (none)`. A FAIL on the sentinel means the uninstaller deletes user data: report it as a
product finding. Report each step's verdict from `install-smoke-win.txt`.

Record explicitly:

- **Tree hash**: the value from `tree.txt` and the `source` block of `install-smoke-win.json`.
- **Authenticode status**: the literal `Status` and `StatusMessage` from `authenticode.txt` and the line the
  harness printed. `NotSigned` for an unsigned build is expected; `Unknown` needs the printed cause. Never leave it blank.
  Signing stays UNPROVEN.
- **checkout_clean_after_run**: `True` or `False`. If `False`, attach `status-before.txt`, `status-after.txt`, the
  paths in "changed by this run" and the cause (a tracked file rewritten by `core.autocrlf` line-ending conversion,
  or a new untracked file). Ignored directories (`target/`, `node_modules/`, `src-tauri/binaries/`) cannot dirty the
  checkout. A bare `False` without an explanation is an incomplete report.
- After the run, `--work` may still hold `AgentOrchestrator\uninstall.exe` and `release-smoke-result.json`; delete
  `$Work` by hand, and delete the sentinel and `%LOCALAPPDATA%\agent-orch` only if you created them.

## 4. P12 and M7 backend gates (MEASURED)

The harness in section 2 already runs the P12 checks and the Rust gates. Confirm in the results that these ran and
passed on Windows, and attach the named evidence files: `p12-windows-checks.*` (verdict line `PASS`),
`windows-transport-tests.*`, `daemon-lifecycle.*` (`lifecycle: PASS`), `lock-takeover` result. Then run the M7
backend suites by name and keep the raw output:

```powershell
cargo test --locked -p orchestrator-daemon --test conversation_runtime 2>&1 | Tee-Object (Join-Path $Ev 'm7-conversation-runtime.txt')
cargo test --locked -p orchestrator-daemon --test conversations 2>&1 | Tee-Object (Join-Path $Ev 'm7-conversations.txt')
cargo test --locked -p orchestrator-daemon --test queue_snapshot 2>&1 | Tee-Object (Join-Path $Ev 'm7-queue-snapshot.txt')
cargo test --locked -p orchestrator-store 2>&1 | Tee-Object (Join-Path $Ev 'm7-store.txt')
```

`fake_agent` is an example; if a `--test` run says the fake agent binary is missing, run
`cargo build --locked -p orchestrator-daemon --examples` first, then rerun and say so. Several unix-only tests do
not exist on Windows; that is the source of the derived count differences.

## 5. M7-25 live conversation exchange (MEASURED, needs a real signed-in agent)

Goal: a real agent reply flows through the daemon, launched by the process supervisor in a **Job Object**.

Prerequisites: Codex CLI signed in (`codex --version`; the pinned version is `0.167.0`; with another version the
turn fails `version_mismatch` and you must set `AGENT_ORCH_CONVERSATION_PINS` per `docs/m7-08-conversation-service.md`
and report the version). If no signed-in agent exists, record **NOT RUN** and the reason; do not substitute the fake.

```powershell
$env:AGENT_ORCH_LIVE_CONVERSATION = '1'
cargo test --locked -p orchestrator-daemon --test conversation_runtime live_codex_answers_a_global_message -- --ignored --nocapture 2>&1 |
  Tee-Object (Join-Path $Ev 'm7-25-live.txt')
Remove-Item Env:AGENT_ORCH_LIVE_CONVERSATION
```

The test sends "Reply with exactly: pong" and requires a completed turn whose reply contains `pong` and that the
conversation granted no authority. Also, while a turn is in flight (run the test with `--nocapture` and a second
shell), list processes (`Get-CimInstance Win32_Process | Where ParentProcessId ...` or Process Explorer) and note
that the agent child is a descendant of the daemon; after the test no `codex`, `node` or `orchestrator-daemon`
process from the run remains (`leftover-processes-live.txt`). Report: Codex version, turn state, reply text length (not
the text if it is long), job/process-tree observation, and whether leftovers exist.

## 6. M7-26 estimated-cost check (MEASURED)

Run the unit and integration tests, then check the live figure.

```powershell
cargo test --locked -p orchestrator-store usage 2>&1 | Tee-Object (Join-Path $Ev 'm7-26-store-usage.txt')
cargo test --locked -p orchestrator-daemon api_usage 2>&1 | Tee-Object (Join-Path $Ev 'm7-26-daemon-usage.txt')
```

Pass criteria (from `docs/usage-ledger.md`, "Estimated cost (M7-26)"): estimates appear only under
`estimated.costs` (USD), never inside `reported`; a model that is absent or not in the price table gives **no**
estimate (never 0); a session is estimated whole or not at all; a stored reported cost is never replaced. If the live
exchange in section 5 produced a Codex receipt with tokens and no cost, query the usage summary (UI Usage view, or the
`usage.summary` method) and record: the estimated figure, the model used (a Codex run records the **requested**
model, not a provider-reported one), and that it is labelled as an estimate. If the receipt had no model the figure
must be unknown, not 0. State which of these you actually observed.

## 7. Windows UI checklist M7-32 (MEASURED by hand)

Open `docs/evidence/m7-30/WINDOWS-MANUAL-CHECKLIST.md` in the source tree and complete **every row** (the file is a
checklist only; this runbook is the procedure). Use the installed app from section 3 (install it with the real
installer, not only the smoke, and record the NSIS install / launch / uninstall). For each row record tester, date, build
(`b44f88a` and the tree), WebView2 version, PASS / FAIL / N-A, a screenshot path and notes. Do not leave a row
blank; `NOT RUN` with a reason is acceptable. Required areas:

1. Environment: Windows build, WebView2 version, `color-mix()`, `100dvh`, `:has()`, motion and colour-scheme support.
2. Themes: light, dark, persistence, and Windows **Contrast themes** (forced-colors).
3. Responsive and DPI: widths 1280, 1150, 960, 760, 480, 390; scaling 100 / 125 / 150 / 200 %.
4. Keyboard only: skip link, tab order, tree keys, Ctrl+Enter, no traps.
5. **Screen reader: Narrator** (NVDA if installed): landmarks, live regions, mutation announcements, disabled-reason, tree.
6. Reduced motion ("Show animations" off).
7. **Connection failure and recovery**: stop the daemon (strip shows Reconnecting, stale data, no fixture data); restart (Connected, refreshed); idle relaunch after 60 s; named-pipe handshake, reconnect, shutdown on exit.
8. **Approvals and mutations**: approve / reject / request changes on a real candidate, idempotent on retry; workstream create/update/link and message send; `?fixture=1` mutations never reach the daemon.
9. Data honesty: no illustrative values in live mode (20/24 slots, $26.00, CPU 41/44); host metrics come from the Windows daemon.
10. Security: production CSP in the WebView; paths with spaces and non-ASCII characters.

Save the completed checklist as `$Ev\WINDOWS-MANUAL-CHECKLIST.completed.md` (copy and fill; do not edit the source file).

## 8. N-1 upgrade (MEASURED if feasible, else UNPROVEN)

Feasible only if you have the `package.mjs windows --out` directory of an **earlier, different-version** release.
If you do not, record `UNPROVEN: no N-1 artifact` and move on. Do not fake an N-1 with a copy of the same build:
a same-version rehearsal is allowed but must be labelled `same-version rehearsal, upgrade NOT proven`.

```powershell
node scripts/release/install-smoke.mjs windows --artifact-dir $Out --old-artifact-dir <N-1 pkg dir> --work (Join-Path $Root 'w2') `
  --smoke-bin $Smoke --allow-unsigned --out (Join-Path $Ev 'install-smoke-upgrade.json') 2>&1 | Tee-Object (Join-Path $Ev 'install-smoke-upgrade.txt')
```

PASS means a final line `upgrade: a different older build ran (N-1 to N)` and `UPGRADE: <old> -> <new>` with
different versions.

## 9. Evidence and push

Write `$Ev\REPORT.md` containing: commit, tree, OS build, versions, every section's verdict with MEASURED / DERIVED /
UNPROVEN / NOT RUN, the count table from section 2 with measured values, Authenticode status, `checkout_clean_after_run`
with its explanation, and the list of things that stayed UNPROVEN (clean machine, signing, SmartScreen, UAC,
second-user pipe denial, remote client, anything you did not run).

Privacy check before pushing (do not print matches): search the evidence for your work or school e-mail domain and your
user name; replace or delete matches, then rerun the search until it finds nothing. Publish clean content; do not
withhold the whole evidence set because one file needed cleaning.

```powershell
$Ref = "evidence/m7-final-windows-acceptance-b44f88a-$Utc-$Hex"
$Ew = Join-Path $Root 'evgit'
git -C $Src worktree add --detach $Ew b44f88af63cb1ea7c68b7a827465db466ef308a2
New-Item -ItemType Directory -Force -Path (Join-Path $Ew "evidence-out") | Out-Null
Copy-Item -Recurse -Force "$Ev\*" (Join-Path $Ew "evidence-out")
Set-Location $Ew
git add -f evidence-out
git commit -m "Windows acceptance evidence for b44f88a ($Utc)"
git push origin "HEAD:refs/heads/$Ref"
git ls-remote origin "refs/heads/$Ref"
```

Large raw logs are fine. Do not push build outputs, `node_modules` or `target/`. Report the ref name, pushed SHA and
`ls-remote` line.

## 10. Final verdict wording

Use exactly: `WINDOWS ACCEPTANCE: PASS / FAIL / INCONCLUSIVE` for the harness, then one line per item in sections 3 to 8.
Never write PASS for a derived count, for signing, SmartScreen, UAC, clean-machine install, or an unrun GUI row.
