# M0-02 native macOS runbook

This std-only probe checks process launch, stdout/stderr capture, concurrent
children, and termination of a child process tree on the host OS. The Windows
run passed; the macOS result is still needed.

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
