# M1-03 native macOS verification

- Run date: 2026-11-07 (America/Boise)
- macOS version/build and architecture: macOS 26.7.1, build 25G241, arm64
- Rust version: rustc 1.95.0 (59807616e 2026-04-14)
- Cargo version: cargo 1.95.0 (f2d3ce0bd 2026-03-21)
- Source commit: `82023712efb9865e07586d6b6a3b0aa5860a725c`
- Command: `cargo test --locked --workspace`
- Result: FAIL
- Cargo exit code: 101
- Timed out: false
- Process group required SIGKILL: false
- Test totals and failures: In `orchestrator-agents`, `codex_parser` passed 9/9. In `codex_adapter`, 9 passed and 1 failed. The run stopped at that integration-test failure; later workspace crates did not run.
- Limitations or failure summary: `cancel_reports_cancelled_exit` panicked at `crates/orchestrator-agents/tests/codex_adapter.rs:430` with `child alive before reap`. `orchestrator-runtime` emitted an unused `std::os::unix::process::CommandExt` import warning. This attempt was preserved unchanged and was not rerun.

The command completed in 10.72 seconds. Complete stdout and stderr are preserved byte-for-byte as base64 in stdout.b64 and stderr.b64. The directory also contains the environment, command, supervisor, timeout status, group-kill status, and clean source status. No candidate files or `board.json` were changed.
