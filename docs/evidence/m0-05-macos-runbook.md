# M0-05 macOS Runbook (agent-facing)

For the agent on a **native macOS machine** with the Codex CLI installed and logged in
(`codex login status` must show an active login, e.g. "Logged in using ChatGPT").
Recorded: 2026-11-07T14:50:00-07:00 (America/Denver).

## What you are proving

One real Codex CLI one-shot performs the fixed M0-05 task — replacing the single line of
`TASK.txt` inside a fresh disposable Git repository — and every acceptance check passes on
macOS. This complements the Windows PASS in `docs/evidence/m0-05-windows/REPORT.md`. The
task, prompt file, and check list are identical to Windows (single canonical prompt:
`tests/feasibility/m0-05/task-prompt.txt`).

This is a feasibility demo. Do not implement orchestrator product code. Do not claim the
product CLI was tested.

## Hard rules (from the M0-05 task scope)

- Fresh unique temporary Git repository per run (the runner does this via `mktemp -d`).
- Never run the coding agent in the primary checkout.
- Never recursively remove a fixed/shared directory. The runner leaves its temp repo in
  place; do not `rm -rf` anything shared to "clean up".
- Never commit temporary test-repo content into the task branch or any branch.
- Do not edit `board.json`.
- Do not merge the task branch.
- If `codex exec` cannot run on this host for any reason, **report M0-05 macOS as blocked**
  with the exact error; do not fake or no-op the agent and do not claim success.

## Step 1 — Fetch the task branch

From the Mac agent's Mac primary checkout of `agent-orch` (any path, e.g.
`~/Code/agent-orch`), or a fresh clone, create one collision-safe run ID and worktree path:

```
RUN_ID="$(date +%Y%m%d-%H%M%S)-$$"
WORKTREE="/tmp/m0-05-macos-$RUN_ID"
git fetch origin codex/m0-05-priority3
TASK_BRANCH_SHA="$(git rev-parse FETCH_HEAD)"
echo "task_branch_sha=$TASK_BRANCH_SHA"
test ! -e "$WORKTREE" || { echo "worktree path already exists: $WORKTREE" >&2; exit 2; }
```

Work from a detached worktree so the task branch itself is never touched:

```
git worktree add --detach "$WORKTREE" "$TASK_BRANCH_SHA"
cd "$WORKTREE"
```

Verify the prompt file matches the canonical version committed on the branch:

```
git rev-parse HEAD:tests/feasibility/m0-05/task-prompt.txt
# record the blob SHA; it must match the one in the Windows report's commit
```

## Step 2 — Locate and record the native Codex CLI

```
command -v python3        # required by the runner for bounded launch and verification
python3 --version         # record exactly
command -v codex          # inspect whether this is the native desktop binary
codex --version           # record exactly; JSONL framing can vary by version
codex login status        # must show an active login
```

If `command -v codex` is empty or points at an npm wrapper, also try
`~/Library/Application Support/Codex/bin/<build-id>/codex` (the native desktop layout
mirrors Windows: `.../Codex/bin/<build-id>/codex.exe` becomes `.../Codex/bin/<build-id>/codex`).
Set the full native path once in `CODEX`; do not use an npm wrapper:

```
CODEX="$(command -v codex)"
# If that is an npm wrapper, replace CODEX with the executable under:
# "$HOME/Library/Application Support/Codex/bin/<build-id>/codex"
"$CODEX" --version
"$CODEX" login status
```

## Step 3 (optional, recommended) — Runner self-test with the bundled mock

Before spending a real Codex run, verify the runner mechanics on the host:

```
chmod +x tests/feasibility/m0-05/mock-codex.sh
SELFTEST_OUT="docs/evidence/m0-05-macos/selftest-$RUN_ID"
bash tests/feasibility/m0-05/run-m0-05.sh "$PWD/tests/feasibility/m0-05/mock-codex.sh" "$PWD" "$SELFTEST_OUT"
```

This must print `M0-05 MACOS RUN: PASS` with all nine checks (mocked agent, real
runner: temp repo, sandbox flag, JSONL/byte capture, all check logic). It commits
nothing in the primary checkout. Commit this self-test dir with the evidence update
in Step 5. Do not treat a self-test PASS as the demo result — it only proves the
runner works on this host.

## Step 4 — Run the demo

```
OUT="docs/evidence/m0-05-macos/raw/run-$RUN_ID"
bash tests/feasibility/m0-05/run-m0-05.sh "$CODEX" "$PWD" "$OUT"
echo "RUNNER_EXIT=$?"
```

Notes:

- Argument order: `<codex path> <primary checkout> <out dir>`.
- The runner passes `--sandbox workspace-write` with `-C <temp repo>`: on macOS the
  native Seatbelt sandbox should apply this directly. **Do not** add any
  `-c windows.sandbox=...`-style override — that pin was a Windows-host-specific
  workaround (elevated sandbox token failure 1385) and does not belong on macOS.
- If a sandbox-related error appears (e.g. the environment rejects the write),
  record it exactly, re-run into a **new** timestamped `OUT` dir, and report the failure.
  Do not escalate to `danger-full-access` to force a pass; that would not demonstrate the
  workspace-write policy and would not be comparable to the Windows run.
- Expected wall time: under ~60 s.

## Step 5 — Expected result and checks

The runner prints and writes `result.md` with exactly these checks (same set as Windows):

```
M0-05 MACOS RUN: PASS
jsonl-all-lines-parse              PASS
exit-status-0                      PASS  exit=0
turn-completed-present             PASS  count=1
last-message-exact                 PASS  value='TASK_DEMO_OK'
single-agent-message-exact         PASS  value='TASK_DEMO_OK'
task-file-exact                    PASS  hex=4d305f5441534b5f41465445520a
git-diff-check                     PASS
only-task-txt-changed              PASS  changed=['TASK.txt']
primary-checkout-isolated          PASS
```

`TASK.txt` must be byte-exact `M0_TASK_AFTER\n` (hex `4d 30 5f 54 41 53 4b 5f 41 46 54 45 52 0a`).
The runner exits 0 only if every check passed. If it exits nonzero, read `result.md` in the
`OUT` dir, preserve the dir, and report the failing checks.

## Step 6 — Publish an evidence-only update (no board edit, no task-branch merge)

From the detached worktree stored in `$WORKTREE`, create the evidence branch once and use
that same variable for both branch creation and push:

```
EVIDENCE_BRANCH="codex/m0-05-macos-evidence-$RUN_ID"
git switch -c "$EVIDENCE_BRANCH"
git add docs/evidence/m0-05-macos
git diff --cached --check
git commit -m "evidence: M0-05 macOS single-task run ($(date +%Y%m%d-%H%M%S))"
git push origin "$EVIDENCE_BRANCH"
```

- Push **only** `$EVIDENCE_BRANCH`. Do not push `codex/m0-05-priority3` or merge anything;
  the coordinator publishes reviewed updates.
- Do not edit `board.json`, docs outside `docs/evidence/m0-05-macos/`, app source, or
  `tests/feasibility/`.
- Keep the raw run dir complete: `environment.txt`, `primary-status-before.txt`,
  `primary-status-after.txt`, `init-commit.txt`, `post-status.txt`, `post-diff.txt`,
  `diff-check.txt`, `stdout.jsonl`, `stderr.txt`, `last-message.txt`, `exit.txt`,
  `task-file.txt`, `task-file-hex.txt`, `result.md`.

## Step 7 — Report back

Send the coordinator:

1. The recorded Mac `codex` path and version, and `codex login status` output.
2. The runner exit code and the full printed check table.
3. The evidence branch name and its commit SHA.
4. The task repo path the runner printed (`task_repo=...`); do not delete it.
5. Any sandbox, auth, or framing differences vs the Windows evidence
   (`docs/evidence/m0-05-windows/REPORT.md`), even if all checks passed.

## Failure handling

- Save every attempt in its own `raw/<descriptive-name>/` dir under
  `docs/evidence/m0-05-macos/` (mirror the Windows `attempt-N-*` convention).
- If codex is not logged in: run `codex login` interactively if the host allows; otherwise
  report blocked with the exact `login status` output.
- If the JSONL shape differs from Windows (e.g. version-specific envelope), still record
  the raw files verbatim; the coordinator decides whether the framing difference is
  material. Do not "normalize" the capture.
- If the demo cannot run at all, report **M0-05 macOS: blocked** with the exact stderr and
  exit code. M0-05 stays open until native macOS evidence is reviewed.
