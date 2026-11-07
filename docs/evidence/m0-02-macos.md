# M0-02 native macOS Rust probe

Recorded on 2026-11-07. Result: compilation failed, exit 101. No probe
scenarios ran. M0-02 remains incomplete.

Read the updated runbook from `origin/codex/m0-02-priority2` and ran the
unchanged branch in the detached worktree `/private/tmp/agent-orch-m0-02-macos`.
The source commit is recorded in [environment.txt](m0-02-macos/environment.txt).

Command, from `tests/feasibility/m0-02` in that worktree:

```sh
cargo run --release --locked --offline
```

The native host is macOS 26.7.1, build 25G241, arm64, with Rust and Cargo
1.95.0. Full environment details and command output are preserved without
truncation:

- [Environment and source commit](m0-02-macos/environment.txt)
- [Complete stdout](m0-02-macos/stdout.txt), empty because compilation failed
- [Complete stderr](m0-02-macos/stderr.txt)
- [Exit status](m0-02-macos/exit.txt)

The compiler reported E0133 at `src/main.rs:306`: calling
`CommandExt::pre_exec` requires an unsafe function or block. The inner
`unsafe { setpgid(0, 0) }` does not cover the outer `cmd.pre_exec(...)` call.

The probe was not edited or rerun, in accordance with the runbook's failure
instructions. The task worktree remains clean; build artifacts are ignored.
Correcting the Unix compilation error and rerunning on native macOS are
required before M0-02 can pass. The earlier Python subprocess check is
evidence for a native OS primitive and does not establish that this Rust
probe passes.

The updated M0-02 instructions explicitly say the Codex check need not be
repeated for this follow-up. The earlier native macOS Codex result remains
documented in [the laptop verification results](../laptop-verification-plan.md).
