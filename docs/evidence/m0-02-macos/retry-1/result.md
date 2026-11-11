# M0-02 native macOS retry 1

Result: PASS on 2026-11-07. The unchanged Rust probe at source commit
`1b006b5053617eb8a7fa961dc4d88b870cc4c76c` compiled and passed all four
scenarios on macOS 26.7.1 arm64 with Rust 1.95.0.

Command: `cargo run --release --locked --offline` from
`tests/feasibility/m0-02` in the fresh detached worktree
`/private/tmp/agent-orch-m0-02-macos-rerun`.

- `launch-stream`: PASS, child stdout captured, exit 0.
- `stderr-exit`: PASS, child exit 7, stderr length 16484, stdout empty.
- `fanout`: PASS, four distinct child processes.
- `kill-tree`: PASS, child and grandchild both gone after tree kill.

Final output: `RESULT ALL PASS`. Actual command exit status: 0.
No timeout occurred; the external limit was 45 seconds. No probe source or
board status was changed. This retry preserves the original compile failure
in the parent evidence directory.

Complete artifacts: [environment](environment.txt), [stdout](stdout.txt),
[stderr](stderr.txt), and [exit status](exit.txt).
