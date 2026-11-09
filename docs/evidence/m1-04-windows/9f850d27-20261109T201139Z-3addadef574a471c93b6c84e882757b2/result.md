# M1-04 Windows verification of 9f850d27

This run is not final integrated acceptance evidence. The candidate is provisional and under independent review.

- Harness setup failures before the test run: a runner parse error and ambiguous multiple Git paths; neither started a clone or tests. See preflight-error-1.txt and preflight-error-2.txt.

- Run ID: 20261109T201139Z-3addadef574a471c93b6c84e882757b2
- UTC date: 2026-11-09T20:12:33.9548095Z
- Source branch: orch/m1-04-windows-owner-9f850d27
- Branch tip at clone time: 9f850d27677b8c33ce3743b11fca09ae3ea6d50e
- Source commit: 9f850d27677b8c33ce3743b11fca09ae3ea6d50e
- Overall result: FAIL
- OS, CPU, PowerShell, Git, Rust, Cargo: see os.txt, cpu.txt, powershell-version.txt, git-version.txt, rustc-version.txt, cargo-version.txt
- Published paths redact the local Windows profile; see redaction-note.txt.
- Evidence publication needed a second clone because of a Windows long-path checkout failure; see preflight-error-3.txt. No tests were rerun.
- Copilot subagent model/reasoning configuration: UNVERIFIED in this execution; this Codex session ran checks directly without a configured Copilot worker.

| Command | Status | Exit | Timed out | Passed | Failed | Ignored | Notes |
|---|---|---|---|---|---|---|---|
| store-owner-exact | FAIL | 101 | False |  |  |  | Exit code 101. |
| runtime-reconcile-exact | PASS | 0 | False | 4 | 0 | 0 |  |
| store-lock-exact | FAIL | 101 | False |  |  |  | Exit code 101. |
| store-owner-all | FAIL | 101 | False |  |  |  | Exit code 101. |
| runtime-reconcile-all | PASS | 0 | False | 7 | 0 | 0 |  |
| store-lock-all | FAIL | 101 | False |  |  |  | Exit code 101. |

## Failure analysis

The Windows build of orchestrator-store stopped with E0658: Metadata.volume_serial_number() and Metadata.file_index() use unstable library feature windows_by_handle in crates/orchestrator-store/src/data_lock.rs lines 379-380. Rust 1.96.0 stable rejected both APIs. The three store check commands therefore exited 101 before their test bodies ran; they are build-blocked, not test assertion failures.

- Windows inherited-child coverage: none. The reported test is Unix-gated.
- Windows-gated test windows_owner_observation_uses_creation_time_not_only_pid: see the runtime-reconcile-exact row.
- Orchestrator-store owner tests use synthetic identities. They do not capture a Windows process.