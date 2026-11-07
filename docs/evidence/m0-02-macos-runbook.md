# M0-02 native macOS runbook

Updated: 2026-11-07T13:25:20-07:00 (America/Denver).

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

## Get the probe

From a Terminal window, either clone the task branch:

```sh
git clone --branch codex/m0-02-priority2 \
  https://github.com/j-mckerracher/agent-orch.git agent-orch-m0-02-macos
```

Or, in an existing clone of this repository, fetch the branch and create a
separate worktree:

```sh
git fetch origin
git worktree add ../agent-orch-m0-02-macos origin/codex/m0-02-priority2
```

## Run

```sh
cd agent-orch-m0-02-macos/tests/feasibility/m0-02
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
