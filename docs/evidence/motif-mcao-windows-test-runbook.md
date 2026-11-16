# motif-mcao Windows test runbook (latest main)

| Item | Value |
|---|---|
| Source | `mc-innersource/motif-mcao`, branch `main`, **latest at the start of your run** |
| Pin at authoring | commit `400d542f8faa035ad928c722f4470d73d72cca33`, tree `b56571f49d6492cac1ffd259b5b2e877e0a06921` (2026-11-16) |
| Runbook URL | `https://github.com/j-mckerracher/agent-orch/blob/runbook/motif-mcao-windows-tests/docs/evidence/motif-mcao-windows-test-runbook.md` |
| Machine | Windows 11, interactive desktop, Rust 1.96.0 (MSVC), Node 22.x, Visual Studio C++ Build Tools, Windows SDK |

You run the automated tests of motif-mcao natively on Windows and report. You do not change code, the `origin` remote, global Git config, PATH or credentials. Use process-scope environment variables only.

The source is private. This runbook holds no source. Do not copy source, logs or evidence into the public `j-mckerracher/agent-orch` repository.

## 0. Rules

1. Test latest `main`. Resolve it once in step 1, record the commit and tree, and test that commit only, even if `main` moves during the run.
2. Report defects. Do not fix code, do not edit `scripts/ci/expected.mjs`, do not run `git clean`, `git reset` or `git stash` in the checkout.
3. Never elevate. Do not install software. If a tool is missing, mark the affected steps `BLOCKED` and continue with the independent ones.
4. Status per step: `PASS` (observed), `FAIL` (observed otherwise; say what), `BLOCKED` (named prerequisite missing), `SKIPPED`. Never write `PASS` for anything you did not observe. A `SKIPPED` gate in the harness is `INCONCLUSIVE`, not a pass.
5. Use a fresh run root with a short real path, for example `C:\t\<8 hex>`. Never a `SUBST` or mapped drive, a junction or a UNC share. If the path exists, pick a new one. Delete only what this run created, and only in step 7.
6. Stop only processes this run started.
7. No secrets or personal data in evidence. Preserve failed output; never overwrite a file.

## 1. Get the source and pin it

Use whatever authenticated Git access this machine already has for `mc-innersource/motif-mcao`. Do not log in, add credentials or change Git config for this. If you cannot read the repository, mark the whole run `BLOCKED` with the exact error and stop.

```powershell
$Run = 'C:\t\' + ([guid]::NewGuid().ToString('N').Substring(0,8))
New-Item -ItemType Directory -Path $Run | Out-Null
git clone --no-tags --branch main https://github.com/mc-innersource/motif-mcao.git "$Run\src"
Set-Location "$Run\src"
git config --local core.autocrlf false
git config --local core.longpaths true
git rev-parse HEAD
git rev-parse 'HEAD^{tree}'
git status --porcelain
```

Record the commit and tree. `git status --porcelain` must print nothing. Report whether the commit equals the authoring pin above. A different commit is expected if `main` moved; test it anyway.

## 2. Toolchain and isolation

Record UTC time, Windows version, free disk space, and the versions of `git`, `rustc`, `cargo`, `node`, `npm`, `pwsh`, Visual Studio C++ tools and the Windows SDK. Do not record environment values or tokens.

Clear for this process: `FM_STATE_OVERRIDE`, `FM_HOME`, `FM_DATA_OVERRIDE`, `FM_CONFIG_OVERRIDE`, `FM_PROJECTS_OVERRIDE`. Every `--evidence-dir` you pass later must not exist yet, and every `--target-dir` must be new and empty, so create none of them now except the empty target and temp directories you name in each command. Set `$env:CI = 'true'`.

## 3. Gate harness (the main test run)

The checkout's own `docs/release-ci.md`, section "Windows verification", is the authority for expected results. Read it at the pinned commit and follow its steps 1 to 6 and 8. Do not rely on counts quoted anywhere else, including this file.

In order:

1. `node --test "scripts/**/*.test.mjs"`. Record the exit code and test count.
2. `node scripts/ci/run-gates.mjs --list`. Record the number of gates and confirm it matches the "Windows verification" text.
3. `node scripts/ci/run-gates.mjs --preflight --evidence-dir $Run\pf-evidence --target-dir $Run\pf-target --test-tmp $Run\pf-tt` (create `pf-target` empty and `pf-tt` first; leave `pf-evidence` absent). It must print `PREFLIGHT PASSED`. Exit code 3 means a path was refused: fix the path and retry with fresh directories, and say which path failed.
4. Full run, with a fresh empty `--target-dir` and `--test-tmp` and a **nonexistent** `--evidence-dir` (do not reuse the preflight directories). Create `target-full` and `tt-full` empty first:
   `$env:CI = 'true'; node scripts/ci/run-gates.mjs --evidence-dir $Run\evidence-full --target-dir $Run\target-full --test-tmp $Run\tt-full`
5. Exit codes: 0 `PASS`, 1 `FAIL`, 2 `INCONCLUSIVE`, 3 preflight refused. Keep `summary.md` and `overall.result.json`.
6. Confirm `git-config`, `gitattributes-before/after` and `source-status-before/after` pass, every `<gate>.survivors.txt` says `none`, and `git status --porcelain` is empty afterwards.

A count mismatch is a finding: report the gate, expected count and observed count. Do not edit expected values.

## 4. Tests outside the harness

Run these after step 3, in the same checkout, each independently so one failure does not hide another. Record command, working directory, exit code, duration and pass/fail counts.

- Repository root: `cargo test --locked --workspace` with `CARGO_TARGET_DIR=$Run\target-ws`. Report ignored tests.
- Repository root: `cargo clippy --locked --workspace --all-targets` with the same target directory. Report warning and error counts.
- `apps\desktop`: `npm ci`, `npx tsc -b`, `npm run build`, `npx vitest run`. Report Vitest file and test counts.
- `apps\desktop`: `npm run test:browser` and `npx playwright test` if the scripts exist, using the Microsoft Edge or Chrome already installed. Download no browser. If none is installed, `BLOCKED`.
- `npm run test:e2e` uses a Unix socket. Report it `N/A on Windows` unless the pinned source documents a Windows path.
- `apps\desktop`: `npm run package:win`. Record the installer path and its SHA-256 (`Get-FileHash`). Do not install it.

## 5. Results

Write `$Run\RESULTS.md` with:

- Source commit and tree, whether it matches the authoring pin, toolchain versions.
- One row per step in sections 3 and 4: status, command, exit code, counts, duration.
- Harness verdict (`PASS`/`FAIL`/`INCONCLUSIVE`) and the list of any gate not `PASS`.
- Each defect with reproduction steps and the log file that shows it.
- Coverage gaps: anything blocked, skipped or not run.

Redact before returning anything: remove user names from paths, tokens and any authentication output.

## 6. Return the results

Return `RESULTS.md` plus `summary.md`, `overall.result.json` and failing-gate logs through the channel that dispatched this task (`busctl result` with the files as artifacts). Do not push evidence to any Git remote. If the channel is unavailable, report the local `$Run` path and mark delivery `BLOCKED-EXTERNAL`.

## 7. Cleanup

Only after the results are delivered or the local path is reported: remove only directories this run created under `$Run`, plus any `git worktree` you added. Keep failed evidence until delivery is confirmed. Restore any process or display setting you changed.
