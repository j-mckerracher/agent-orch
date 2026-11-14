# M0-02 native macOS runbook

Updated: 2026-11-07T13:46:56-07:00 (America/Denver).

## Assignment for the macOS verification agent

This runbook is addressed to the agent operating on the Mac. The repository
owner is delegating the run and will not execute these commands. The agent is
responsible for fetching the task branch, running the probe, preserving the
full evidence, and pushing an evidence-only update. Do not edit the probe or
`board.json`; the implementation coordinator owns source fixes and board
status.

## Required follow-up

Run the task's **Rust** probe on macOS. The Python process-group smoke test in
`docs/laptop-verification-plan.md` verifies a native OS primitive, but it does
not compile or run this Rust probe and cannot close M0-02. The Windows run of
this same Rust probe passed. The Codex check is M0-03 and does not need to be
repeated for this follow-up.

The probe is self-contained and uses only the Rust standard library. It checks
Rust process launch, stdout/stderr capture, four concurrent children, a
non-zero child exit, and termination of a child process tree on the native
host OS.

## Retry after the first macOS compile failure

The first native macOS run used source commit
`8e213e4535e08e827e6204d69780b6b38fc25558` and failed to compile with E0133
because `CommandExt::pre_exec` was called without an unsafe block; no scenarios
ran. The complete report and raw output are recorded in
[`docs/evidence/m0-02-macos.md`](https://github.com/j-mckerracher/agent-orch/blob/main/docs/evidence/m0-02-macos.md).
The task branch now uses the safe Unix `CommandExt::process_group(0)` API to
put the child in its own process group. Please run a fresh clone of the
updated branch below and include its `git rev-parse HEAD` output so the tested
source revision is unambiguous.

## Get the probe

From a Terminal window, clone the current task branch into a fresh directory:

```sh
git clone --branch codex/m0-02-priority2 \
  https://github.com/j-mckerracher/agent-orch.git agent-orch-m0-02-macos-rerun
cd agent-orch-m0-02-macos-rerun
git rev-parse HEAD
```

If you use an existing clean clone instead, fetch and check out the updated
remote task branch in a separate worktree:

```sh
git fetch origin
git worktree add ../agent-orch-m0-02-macos-rerun origin/codex/m0-02-priority2
cd ../agent-orch-m0-02-macos-rerun
git rev-parse HEAD
```

## Run

```sh
mkdir -p docs/evidence/m0-02-macos/retry-1
{
  sw_vers
  uname -a
  uname -m
  rustc --version
  cargo --version
  git rev-parse HEAD
} > docs/evidence/m0-02-macos/retry-1/environment.txt

cd tests/feasibility/m0-02
set +e
cargo run --release --locked --offline \
  > ../../../docs/evidence/m0-02-macos/retry-1/stdout.txt \
  2> ../../../docs/evidence/m0-02-macos/retry-1/stderr.txt
probe_status=$?
printf '%s\n' "$probe_status" \
  > ../../../docs/evidence/m0-02-macos/retry-1/exit.txt
set -e
```

The offline build uses only the Rust standard library. A passing run prints
`TEST=... PASS` for `launch-stream`, `stderr-exit`, `fanout`, and `kill-tree`,
then `RESULT ALL PASS`, and exits with status 0. `kill-tree` starts a child in
its own process group and checks that terminating the group also ends its
grandchild.

## Preserve and publish the result

Preserve this retry separately from the first failed compile; do not overwrite
`docs/evidence/m0-02-macos/` files from that run. Capture the following under
`docs/evidence/m0-02-macos/retry-1/` in the task-branch clone:

- `environment.txt`: `sw_vers`, `uname -a`, `uname -m`, `rustc --version`,
  `cargo --version`, and `git rev-parse HEAD`;
- `stdout.txt` and `stderr.txt`: complete output from the probe command;
- `exit.txt`: the probe command's actual exit status;
- `result.md`: pass or failure summary, including any timeout details.

If any scenario fails or the command hangs, stop it, preserve the output and
exit status, and do not edit or rerun the probe. If all four scenarios pass,
record that all pass and exit status is zero. In either case, publish the
evidence:

1. Create a separate evidence checkout from the current `origin/main` (for
   example, a Git worktree); do not merge the task branch.
2. Copy only the new `retry-1/` artifacts into
   `docs/evidence/m0-02-macos/retry-1/` there. Update
   `docs/evidence/m0-02-macos.md` and `docs/laptop-verification-plan.md` to
   summarize this attempt and link the raw files. Preserve the first attempt.
3. Commit and push only those evidence/documentation paths. Do not change
   `board.json`; the implementation coordinator will update its status after
   reviewing the evidence. If the repository's push policy requires a branch,
   push the evidence-only branch and report its commit.

The macOS evidence is required before M0-02 can be marked complete.
