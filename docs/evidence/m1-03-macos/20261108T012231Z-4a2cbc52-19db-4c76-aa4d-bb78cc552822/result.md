# M1-03 native macOS verification

- Run date: 2026-11-07 (America/Boise)
- macOS version/build and architecture: macOS 26.7.1, build 25G241, arm64
- Rust version: rustc 1.95.0 (59807616e 2026-04-14)
- Cargo version: cargo 1.95.0 (f2d3ce0bd 2026-03-21)
- Source commit: `649c137d7bcfcf2445d2a107162862cde0a1c0ed`
- Command: `cargo test --locked --workspace`
- Result: PASS
- Cargo exit code: 0
- Timed out: false
- Process group required SIGKILL: false
- Test totals and failures: 62 passed, 0 failed, 0 ignored. Adapter parser: 9 passed. Codex adapter integration: 10 passed, including `cancel_reports_cancelled_exit` and descendant cancellation. Core authorization/contracts/tasks: 35 passed. Git worktree integration: 8 passed. Remaining crates and doc-test targets completed with zero tests.
- Limitations or failure summary: None observed. The earlier candidate at `8202371` failed the transient child-liveness assertion; this run tested the corrected pinned candidate.

The suite completed in 11.60 seconds. This attempt used the runbook's external 900-second process-group supervisor. It did not time out, and no process-group SIGKILL was needed. Full stdout and stderr are preserved byte-for-byte as base64 in `stdout.b64` and `stderr.b64`. The directory also contains the environment, command, supervisor, timeout status, group-kill status, and clean candidate status. No source files or `board.json` were changed.
