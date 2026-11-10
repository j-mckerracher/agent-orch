# M1-04 Windows verification of 9f850d27

This run is not final integrated acceptance evidence. The candidate is provisional and under independent review.

- Run ID: 20261110T182802Z-6b4717e3f42b42668d97734a58e89f91
- UTC date: 2026-11-10T18:29:20.2797556Z
- Source branch: orch/m1-04-windows-owner-9f850d27
- Branch tip at clone time: 9f850d27677b8c33ce3743b11fca09ae3ea6d50e
- Source commit: 9f850d27677b8c33ce3743b11fca09ae3ea6d50e
- Overall result: FAIL
- OS, CPU, PowerShell, Git, Rust, Cargo: see os.txt, cpu.txt, powershell-version.txt, rustc-version.txt, cargo-version.txt
- Agent settings: see agent-settings.txt; the requested Copilot/Claude worker configuration is unverified.

| Command | Status | Exit | Timed out | Passed | Failed | Ignored | Notes |
|---|---|---|---|---|---|---|---|
| store-owner-exact | FAIL | 101 | False |  |  |  | Exit code 101. |
| runtime-reconcile-exact | PASS | 0 | False | 4 | 0 | 0 |  |
| store-lock-exact | FAIL | 101 | False |  |  |  | Exit code 101. |
| store-owner-all | FAIL | 101 | False |  |  |  | Exit code 101. |
| runtime-reconcile-all | PASS | 0 | False | 7 | 0 | 0 |  |
| store-lock-all | FAIL | 101 | False |  |  |  | Exit code 101. |

- Windows inherited-child coverage: none. The reported test is Unix-gated.
- Windows-gated test windows_owner_observation_uses_creation_time_not_only_pid: see the runtime-reconcile-exact row.
- Orchestrator-store owner tests use synthetic identities. They do not capture a Windows process.
- Store-check failure detail: all four store commands exited 101 before tests ran because stable Rust rejected `Metadata.file_index()` as unstable feature `windows_by_handle` at `crates/orchestrator-store/src/data_lock.rs:380`. Full compiler output is preserved in each stderr file.
- Publication recovery: the first evidence-worktree checkout failed on an unrelated main-branch path with Windows Filename too long. The original source clone and local evidence branch are preserved. A separate publisher clone with repository-local core.longpaths=true created the recovery worktree successfully.
- Publication staging failure: the first git add was invoked from the no-checkout publisher root, so the evidence pathspec did not match there. A corrected add in the recovery worktree succeeded. No tests were rerun; evidence remains in the run directory and recovery worktree.
- Evidence diff-check diagnostic: an extra git diff --cached --check found trailing blank lines at EOF in captured Cargo stdout and formatted OS/CPU files. The raw test logs remain unchanged; the runbook-required source checkout diff-check passed with exit 0.
