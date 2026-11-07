# M0-05 Evidence: One real task on one platform (Windows)

Recorded: 2026-11-07T16:10:00-07:00 (America/Denver).
Platform: Windows 11 Education, build 10.0.26200.0 x64, host `X-LT-JMCKERRA`.
Worker: OMP session, Purdue `qwen3.8:27b`, thinking high, full Ponytail.
Branch: `codex/m0-05-priority3` (candidate = the commit that adds this paragraph).
Earlier runs A/B used source head `24c3d03f5cc59fbda22b8f7dfb6bfa0f450512c8`; the
corrected-runner rerun below was run from the exact code commit `cd0134b`, and the runner
source (`tests/feasibility/m0-05/`) is byte-identical at the candidate to `cd0134b`
(verified by `git diff cd0134b <candidate> -- tests/feasibility/`), so the rerun validates
the candidate's runner.
Worktree: `C:\Users\jmckerra\Code\agent-orch-m0-05-priority3`.

## Verdict

PASS for Windows: a real Codex CLI one-shot (native desktop executable, not the old npm
wrapper) performed the fixed M0-05 task — replacing the single line of `TASK.txt` in a fresh
disposable Git repository — and every acceptance check passed. Two earlier consecutive runs
(A `run-20261107-143134`, B `run-20261107-144044`) demonstrated reproducibility. The hardened
runner was then tested from its exact code commit in
`raw/20261107-154412-2768843b9995481eaafe738054a7f5b1/`; all checks passed. macOS: not yet
run; the companion instructions are `docs/evidence/m0-05-macos-runbook.md`. M0-05 stays open
until native macOS evidence is reviewed.

This is a feasibility proof for the orchestrator's execution model. It does not test the
product CLI (still a scaffold) and implements no M1-03/M1-04 features.

## Executable and environment

| Item | Value |
|---|---|
| Codex binary (native desktop) | `C:\Users\jmckerra\AppData\Local\OpenAI\Codex\bin\14b838c1d074ff01\codex.exe` |
| Codex version | `codex-cli 0.167.0-alpha.3.2` |
| Auth | `codex login status` = "Logged in using ChatGPT" |
| Git | `git version 2.46.2.windows.1` |
| PowerShell | 5.1.26100.8655 |
| Model | CLI default for this account (no `-m` override; see M0-03: the config-pinned `gpt-6.1-terra` is rejected on the 0.20.0 wrapper, the native 0.167.0 exe runs its default without issue) |

## Fixed task (identical on both OSes)

Task file: `TASK.txt`, initial content exactly `M0_TASK_BEFORE` + LF.
Prompt (single canonical file `tests/feasibility/m0-05/task-prompt.txt`, read by both
runners verbatim):

> The file TASK.txt currently contains exactly the ASCII bytes "M0_TASK_BEFORE" followed by
> one LF (0x0A) byte. Rewrite TASK.txt to contain exactly the ASCII bytes "M0_TASK_AFTER"
> followed by one LF (0x0A) byte. No BOM, no CRLF, no trailing spaces. Change no other file.
> When done, reply with exactly TASK_DEMO_OK and nothing else.

Acceptance checks (enforced by the runners, not eyeballed):

1. Codex exit status 0.
2. Every stdout JSONL line parses as JSON.
3. Exactly one `turn.completed` event.
4. Last agent message is exactly `TASK_DEMO_OK` (both the `-o` file and the single
   `agent_message` event).
5. `TASK.txt` is byte-exact `M0_TASK_AFTER` + LF (`4d 30 5f 54 41 53 4b 5f 41 46 54 45 52 0a`).
6. `git diff --check` clean.
7. The only changed path is `TASK.txt`.
8. Primary-checkout isolation: tracked-file status identical before/after; no untracked file
   outside the M0-05 evidence/runner paths.

## Windows sandbox finding (material, affects the adapter design)

The host user config `~\.codex\config.toml` pins `[windows] sandbox = "elevated"`. In
elevated mode **every** sandboxed helper spawn fails on this machine:

```
Failed to create unified exec process: CreateProcessWithLogonW failed: 1385
```

Evidence: `raw/diag-sandbox-1/` probes all three sandbox modes with a one-line
`echo SANDBOX_PROBE_OK` task. `read-only` and `workspace-write` both fail with 1385
(`command_execution` exit_code -1, `status: failed`); `danger-full-access` succeeds.
So the failure is the elevated-sandbox token mechanism, not the write policy — 1385 is a
known Codex Windows issue class (openai/codex #16780, #32649: elevated sandbox token
creation failures).

`codex doctor --summary` reports `sandbox: restricted fs + restricted network` as configured,
so doctor does not detect the elevated-token failure; only a real sandboxed spawn does.

**Fix used:** the runner pins the unelevated token-restriction sandbox per invocation with
`-c 'windows.sandbox="unelevated"'`. That mode enforces the `--sandbox workspace-write`
policy normally, and the probe (`raw/diag-sandbox-2/unelevated-*.txt/.jsonl`) shows the
shell command completing with exit 0. The shared `~\.codex\config.toml` was **not** edited.
This is host-specific: the macOS runbook must not inherit this override, and the production
adapter needs to treat "sandbox mode selected by host config" as a conformance variable
(detect via a probe spawn, fall back to unelevated, and record which mode actually ran).

One `danger-full-access` diagnostic was run once with only `echo SANDBOX_PROBE_OK`. It is
preserved as diagnostic evidence and is not M0-05 acceptance evidence. Do not repeat it.

## Attempts (preserved separately)

| Dir | Result | Why |
|---|---|---|
| `raw/attempt-1-failed-runner-bug/` | FAIL (runner) | First runner revision crashed before the codex call: PS 5.1 treated the native codex stderr line "Reading additional input from stdin..." as a terminating `NativeCommandError` under `$ErrorActionPreference='Stop'`. No demo data in this dir is valid. |
| `raw/attempt-2-workspace-write-sandbox-rejected/` | FAIL (sandbox) | Runner fixed, but elevated sandbox rejected every helper spawn (1385). Codex reported "Unable to update TASK.txt: the execution environment rejected the write." JSONL is intact (10 lines, `turn.completed` present, exit 0) — a good example of why exit code alone must never be trusted. |
| `raw/diag-sandbox-1/` | diagnostic | Three-mode spawn probe (one-line `echo SANDBOX_PROBE_OK` task, i.e. no file write); isolated the 1385 to elevated-sandbox token creation. |
| `raw/diag-sandbox-2/` | diagnostic | Unelevated-sandbox probe (`echo SANDBOX_PROBE_OK` only, no file write): the probe command `echo SANDBOX_PROBE_OK` executed; the agent replied with its exit code `0` (`unelevated-last.txt`) and codex itself exited 0 — sandboxed spawn works in this mode. |
| `raw/run-20261107-143134/` | **PASS** | Full demo, run A. |
| `raw/run-20261107-144044/` | **PASS** | Full demo, run B (after switching both runners to the shared prompt file). |

No fake or no-op agent was used; every captured run above invoked the real codex.exe.

## Earlier run B results — `raw/run-20261107-144044/`

```
exit-status-0                      PASS  exit=0
jsonl-all-lines-parse              PASS  lines=6
turn-completed-present             PASS  count=1
last-message-exact                 PASS  value='TASK_DEMO_OK'
single-agent-message-exact         PASS  value='TASK_DEMO_OK'
task-file-exact                    PASS  bytes='M0_TASK_AFTER\n'
git-diff-check                     PASS
only-task-txt-changed              PASS  changed=TASK.txt
primary-checkout-isolated          PASS  tracked identical; no new non-m0-05 untracked
```

Task repo: `C:\Users\jmckerra\AppData\Local\Temp\agent-orch-m0-05-task-c42a4a61ffaf436dab081a1d2da23c74`
(run A: `...task-b2258e8009ba41578a12b424cd6ee77b`). Fresh per run, never removed (left in
`%TEMP%` as a unique GUID-named dir; nothing shared is ever deleted).

Resulting diff (task repo):

```diff
--- a/TASK.txt
+++ b/TASK.txt
@@ -1 +1 @@
-M0_TASK_BEFORE
+M0_TASK_AFTER
```

`stdout.jsonl` event sequence (run B): `thread.started` → `turn.started` →
`item.started(command_execution)` → `item.completed(command_execution, exit_code 0)` →
`item.completed(agent_message "TASK_DEMO_OK")` → `turn.completed`. Stderr carried only
non-fatal plugin/skills warnings (same as M0-03).

## Corrected-runner validation

The hardened runner code commit is
`cd0134bbc7ea38ebfed925b649b1c6df6e3cc11c`. The final Windows rerun used that exact
`source_head` and the real native Codex executable. Evidence is in
`raw/20261107-154412-2768843b9995481eaafe738054a7f5b1/`. A later cosmetic runner change
(`c69f0606`, summary `.TrimEnd()`, no behavior change) was re-validated in
`raw/20261107-155123-e4204a6783274facb48ed1e17252aa19/` (its single isolation FAIL is the
mid-run worktree commit race documented under Limitations), and again, fully, by the final
clean run from the candidate head recorded in the "Final candidate validation" section.

| Check | Result |
|---|---|
| Codex | `codex-cli 0.167.0-alpha.3.2`, logged in with ChatGPT |
| PowerShell | 7.5.4 (`pwsh`; the runner requires 7.2 or later for `ArgumentList`) |
| Process exit / timeout | `0` / `false` |
| JSONL parse / completed turn | 6 lines parse / exactly 1 completed turn |
| Final agent message | exactly `TASK_DEMO_OK` |
| Task file | exact bytes `M0_TASK_AFTER` + LF (`4d 30 5f 54 41 53 4b 5f 41 46 54 45 52 0a`) |
| Changed task-repo paths | exactly `TASK.txt`, `git diff --check` clean |
| Primary checkout | tracked status and all non-output untracked paths identical before/after |

The runner captured stdout/stderr as raw bytes while draining both pipes asynchronously,
then enforced its 60-second deadline and process-tree cleanup. The new run produced
`M0-05 WINDOWS RUN: PASS` and runner exit 0.

## Final candidate validation

From clean candidate head `3a0a35de688addc7687ad5fd55b65a06e7959298` the runner was
executed once more with the same fixed task. Evidence is in
`raw/20261107-final-candidate-20261107-155733-441c6271e13c43d9838f7e26878d7ae7/`; all nine
checks passed, including `primary-checkout-isolated`, because the worktree was clean and
stable for the whole run (no commit landed mid-run). `environment.txt` in that dir records
`source_head=3a0a35de688addc7687ad5fd55b65a06e7959298`. Runner exit 0.

## Isolation proof

`primary-status-before.txt` / `primary-status-after.txt` (run B) are byte-identical:
`## codex/m0-05-priority3` with only the two M0-05 untracked paths (this report's own
evidence dir and the runner dir). Tracked-file status is identical by construction of the
check; no codex process ever ran with the primary checkout as its working directory — the
runner always passes `-C <temp repo>`.

## Reproduce on this Windows host

```
$RunId = (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N')
$OutDir = "docs/evidence/m0-05-windows/raw/run-$RunId"
pwsh -NoProfile -ExecutionPolicy Bypass -File tests/feasibility/m0-05/run-m0-05.ps1 `
  -CodexPath "C:\Users\jmckerra\AppData\Local\OpenAI\Codex\bin\14b838c1d074ff01\codex.exe" `
  -PrimaryCheckout "C:\Users\jmckerra\Code\agent-orch-m0-05-priority3" `
  -OutDir $OutDir
```

Exit 0 means all checks passed; `result.md` in the out dir lists each check. The runner is
deterministic except for the run ID, GUID temp-repo name, and Codex's internal thread IDs.

## Raw artifacts

- `raw/run-20261107-143134/` and `raw/run-20261107-144044/` (both PASS): `environment.txt`,
  `init-commit.txt`, `primary-status-before.txt`, `primary-status-after.txt`,
  `post-status.txt`, `post-diff.txt`, `diff-check.txt`, `stdout.jsonl`, `stderr.txt`,
  `last-message.txt`, `exit.txt`, `task-file.txt`, `task-file-hex.txt`, `result.md`.
- `raw/attempt-1-failed-runner-bug/`, `raw/attempt-2-workspace-write-sandbox-rejected/`:
  same file set, preserved as failures.
- `raw/diag-sandbox-1/{read-only,workspace-write,danger-full-access}/`,
  `raw/diag-sandbox-2/unelevated-*`: sandbox probes.

Note: historical run artifacts captured under PowerShell 5.1 may contain a UTF-8 BOM in
text evidence files. The corrected runner uses PowerShell 7.5.4 and captures stdout/stderr
as raw bytes. `task-file-hex.txt` is the byte-exact dump of the task repo's actual `TASK.txt`
in every run.

## Limitations

- Windows evidence only; macOS remains unverified pending the runbook.
- Single account (ChatGPT login), CLI-default model; other accounts/plans may map to
  different default models (M0-03 recorded a plan-dependent model rejection).
- The `windows.sandbox="unelevated"` pin is a per-invocation workaround for this host's
  broken elevated sandbox; a different host may not need it, and `danger-full-access` was
  deliberately **not** used for the demo runs (only for diagnosis).
- One task, one file, one turn: concurrency, long tasks, approvals, and multi-file diffs are
  out of scope for M0-05.
- Codex exits 0 even when the agent's task *itself* failed (attempt 2); the acceptance
  checks above are why the demo verifies content, not just exit status.
- The `primary-checkout-isolated` check compares worktree status before/after; a commit
  landing in this worktree mid-run legitimately fails it. Run
  `raw/20261107-155123-e4204a6783274facb48ed1e17252aa19/` (from `c69f0606`) is preserved
  as exactly that case: 8/9 checks passed and the before/after delta is precisely one
  evidence commit that landed mid-run. The final candidate run above was made from a
  clean, stable worktree.
