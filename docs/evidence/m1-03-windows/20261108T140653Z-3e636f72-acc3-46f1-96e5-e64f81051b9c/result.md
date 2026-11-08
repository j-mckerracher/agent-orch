# M1-03 Windows verification

- Run date UTC: 2026-11-08T14:07:18Z
- OS and architecture: Microsoft Windows 10.0.26200, x64
- Rust version: rustc 1.96.0 (ac68faa20 2026-05-25)
- Cargo version: cargo 1.96.0 (30a34c682 2026-05-25)
- Source commit: 649c137d7bcfcf2445d2a107162862cde0a1c0ed
- Command: cargo test --locked --workspace
- Result: PASS (Windows)
- Cargo exit code: 0
- Timed out: false
- Process tree required forced termination: false
- Test totals and failures: 62 passed, 0 failed, 0 ignored. Adapter parser: 9 passed. Codex adapter integration: 10 passed. Core authorization, contracts, and tasks: 35 passed. Git worktree integration: 8 passed. Remaining crates and doc-test targets completed with zero tests.
- Limitations or failure summary: This Windows run does not exercise the Unix process-group path required by the native macOS runbook. A separate passing native macOS run for this source commit is already recorded by remote evidence commit 29be290.
