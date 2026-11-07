# M0-02 native macOS runbook

Updated: 2026-11-07T13:39:39-07:00 (America/Denver).

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
cd tests/feasibility/m0-02
uname -a
rustc --version
cargo --version
cargo run --release --locked --offline
```

The offline build uses only the Rust standard library. A passing run prints
`TEST=... PASS` for `launch-stream`, `stderr-exit`, `fanout`, and `kill-tree`,
then `RESULT ALL PASS`, and exits with status 0. `kill-tree` starts a child in
its own process group and checks that terminating the group also ends its
grandchild.

If a scenario fails or the process hangs, stop it and report the complete
output and exit status; do not rerun by changing the probe. Please send back:

- macOS version (`sw_vers` output), architecture (`uname -m`), `rustc` and
  `cargo` versions;
- the complete probe output and exit status;
- any failure or timeout details.

The macOS evidence is required before M0-02 can be marked complete.
