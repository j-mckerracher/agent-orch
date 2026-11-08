# Windows verification report for 5fd2edc

- Run ID: 20261108T144114Z-50988
- Platform: Windows 10.0.26200 x64; PowerShell 7.5.4
- Rust: rustc 1.96.0 (ac68faa20 2026-05-25)
- Cargo: cargo 1.96.0 (30a34c682 2026-05-25)
- Runbook branch: `orch/windows-runbook-5fd2edc`
- Tested source commit: `5fd2edcddc207d753a10179195262213ce41d638`
- Source checkout status after run: clean

## Results

- `cargo test --locked --workspace`: exit 101 during compilation; no workspace tests ran. Stable Rust rejected `std::fs::Metadata::volume_serial_number()` and `file_index()` in `crates/orchestrator-git/src/delivery.rs:662-663` as unstable feature `windows_by_handle` (issue 63010).
- `cargo clippy --locked --workspace --all-targets`: exit 101 for the same compiler errors. Clippy warnings were also captured in `clippy.stderr.txt`.
- `git diff --check HEAD^ HEAD`: exit 0.
- `cargo test --locked -p orchestrator-agents --test conformance`: exit 0; 24 passed, 0 failed, 0 ignored.
- End-to-end `run`, `inspect`, and `diff`: each exited 101 during compilation for the same `windows_by_handle` errors. The CLI never launched the Codex task. The seed repository was clean before and after, so no product task changes were made.

## Tool discovery

`where.exe codex` returned three entries. The first, selected by the runbook, was `C:\Users\jmckerra\AppData\Roaming\npm\codex`; `codex --version` reported `codex-cli 0.20.0` with exit 0, and `codex login status` reported logged in with exit 0. The desktop-bundled binary was also listed. `claude` and `pi` were found on PATH.

All command output, exit codes, environment details, and the exact objective are included in this evidence directory. No source files were edited.

Evidence note: git diff --cached --check flags a trailing blank line at EOF in the captured conformance.stdout.txt; the command output is preserved as captured.
