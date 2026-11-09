# Priority 6 Windows acceptance of 6c83686c3c3cedf07f8fab50f4764d88f643a2c6 (M2-06 engine reconciliation, M3-04 delivery)

This run does not close M2-06 or M3-04. Both items stay open until independent Mac, source, and
native acceptance, and the coordinator decides acceptance. The expected counts are
calculated from source for Windows. They are not measured before this run, and no macOS or review
result is Windows evidence.

- Run ID: 20261109T125340Z-f1decab80f9340fd9cbb71ccba123900
- UTC date: 2026-11-09T13:04:21.1186916Z
- Source branch: orch/priority-6-windows-acceptance-6c83686
- Branch tip at clone time: 6c83686c3c3cedf07f8fab50f4764d88f643a2c6
- Source commit: 6c83686c3c3cedf07f8fab50f4764d88f643a2c6
- HEAD matched the source commit before the checks and after them: yes
  (source-head-before.stdout.txt, source-head-after.stdout.txt)
- Baseline ancestry (693dd9d8), commit count, and changed-file set: base-ancestry,
  range-count, range-files (consistency gates, not patch-equivalence proof)
- Runbook identity (operator-declared, not byte proof): runbook-identity.txt
- Overall result: FAIL 
- Toolchain: see git-version.stdout.txt, rustc-version.stdout.txt,
  cargo-version.stdout.txt, git-autocrlf.stdout.txt (exit code in
  git-autocrlf.exit.txt)
- OS, CPU, PowerShell: see os.txt, cpu.txt, powershell-version.txt
- Agent runtime and Sonnet 5.5/high setting: unverified
- Survivor scan: every check has a survivors file. Processes whose image lies under this run's
  private temp directory, or whose command line names this run's directory (long or 8.3 form), are
  listed and fail the check. Nothing is stopped by the scan.
- Raw stdout and stderr bytes are kept outside the checkout and outside the published files. Their
  size and SHA-256 are in each result.json.

| Command | Status | Exit | Timed out | Seconds | Running (N per binary) | Passed | Failed | Ignored | Notes |
|---|---|---|---|---|---|---|---|---|---|
| workspace-tests | FAIL | 101 | False | 316.628 | 13,10,10,24,10,9,17,0,3,0,22,0,2,0,43,12,8,17,17,0,32,21,6,7,6,1,11,0,28,14,32,9,2,13,8,11,3,3,2,4,1,0,0,0,0,0,0,0 | 425 | 5 | 1 | Exit code 101. 5 test(s) failed. 425 passed. Expected 430 (calculated, not measured before this run). No 'ok' line for settled_failed_task_with_unlanded_delivery_and_absent_workspace_does_not_block. No 'ok' line for confirmed_delivery_on_a_failed_task_blocks_and_preserves_everything. No 'ok' line for confirmed_delivery_on_a_failed_task_is_contradictory_and_refuses_scheduling. No 'ok' line for consistent_delivery_journals_do_not_block_scheduling. No 'ok' line for live_owner_with_consistent_not_applied_journal_does_not_block_scheduling. |
| git-package | PASS | 0 | False | 77.793 | 32,21,6,7,6,1,0 | 73 | 0 | 0 |  |
| git-delivery-lib | PASS | 0 | False | 0.406 | 4 | 4 | 0 | 0 |  |
| store-data-lock-integration | PASS | 0 | False | 41.975 | 8 | 7 | 0 | 1 |  |
| store-data-lock-lib | PASS | 0 | False | 9.3 | 7 | 7 | 0 | 0 |  |
| store-lock-identity-contract | PASS | 0 | False | 2.703 | 3 | 3 | 0 | 0 |  |
| runtime-reconciliation-contract | PASS | 0 | False | 30.403 | 14 | 14 | 0 | 0 |  |
| runtime-reconcile | PASS | 0 | False | 3.324 | 28 | 28 | 0 | 0 |  |
| ipc-token-owner | PASS | 0 | False | 7.99 | 1 | 1 | 0 | 0 |  |
| cli-lib | FAIL | 101 | False | 42.356 | 17 | 15 | 2 | 0 | Exit code 101. 2 test(s) failed. 15 passed. Expected 17 (calculated, not measured before this run). No 'ok' line for settled_failed_task_with_unlanded_delivery_and_absent_workspace_does_not_block. No 'ok' line for confirmed_delivery_on_a_failed_task_blocks_and_preserves_everything. |
| cli-cleanup-units | PASS | 0 | False | 3.615 | 3 | 3 | 0 | 0 |  |
| cli-session-gone-guard | PASS | 0 | False | 0.606 | 1 | 1 | 0 | 0 |  |
| cli-cleanup-safety | PASS | 0 | False | 23.059 | 3 | 3 | 0 | 0 |  |
| cli-acceptance | PASS | 0 | False | 10.348 | 2 | 2 | 0 | 0 |  |
| git-delivery-acceptance-contract | PASS | 0 | False | 10.299 | 6 | 6 | 0 | 0 |  |
| store-delivery-acceptance-contract | PASS | 0 | False | 3.949 | 3 | 3 | 0 | 0 |  |
| cli-engine-reconciliation-gate | FAIL | 101 | False | 38.022 | 22 | 19 | 3 | 0 | Exit code 101. 3 test(s) failed. 19 passed. Expected 22 (calculated, not measured before this run). No 'ok' line for confirmed_delivery_on_a_failed_task_is_contradictory_and_refuses_scheduling. No 'ok' line for consistent_delivery_journals_do_not_block_scheduling. No 'ok' line for live_owner_with_consistent_not_applied_journal_does_not_block_scheduling. |

## Test result lines

### workspace-tests

```text
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.67s
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.44s
running 24 tests
test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.16s
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.48s
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 17 tests
test result: FAILED. 15 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.68s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.32s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 22 tests
test result: FAILED. 19 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 34.15s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.31s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 43 tests
test result: ok. 43 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 12 tests
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 17 tests
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 17 tests
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.16s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 32 tests
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 14.56s
running 21 tests
test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 30.13s
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.72s
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.56s
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.72s
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.24s
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 28 tests
test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 14 tests
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 11.88s
running 32 tests
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.23s
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.26s
running 8 tests
test result: ok. 7 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.10s
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.17s
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
running 4 tests
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### git-package

```text
running 32 tests
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.98s
running 21 tests
test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 28.67s
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.63s
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.46s
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.44s
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.03s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### git-delivery-lib

```text
running 4 tests
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 28 filtered out; finished in 0.02s
```

### store-data-lock-integration

```text
running 8 tests
test result: ok. 7 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.08s
```

### store-data-lock-lib

```text
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 25 filtered out; finished in 0.01s
```

### store-lock-identity-contract

```text
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
```

### runtime-reconciliation-contract

```text
running 14 tests
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.13s
```

### runtime-reconcile

```text
running 28 tests
test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### ipc-token-owner

```text
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.05s
```

### cli-lib

```text
running 17 tests
test result: FAILED. 15 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.43s
```

### cli-cleanup-units

```text
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 14 filtered out; finished in 2.72s
```

### cli-session-gone-guard

```text
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 0.00s
```

### cli-cleanup-safety

```text
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.59s
```

### cli-acceptance

```text
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.99s
```

### git-delivery-acceptance-contract

```text
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.92s
```

### store-delivery-acceptance-contract

```text
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s
```

### cli-engine-reconciliation-gate

```text
running 22 tests
test result: FAILED. 19 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 33.94s
```

## Coverage gaps

- Unix-only tests did not run on Windows: the orphan-tree test, the fault-injection startup test, the
  runtime process-group survivor test, the store symlink test, and the Unix-only unit tests. Their
  macOS acceptance is separate and is not Windows evidence.
- cli.rs, multi_task_review.rs, recovery.rs and daemon.rs are Unix-only (cfg unix) and compile to
  zero tests on Windows. Windows coverage of the CLI engine's reconciliation gate is only the
  named engine_reconciliation_gate tests, portable_acceptance and cleanup_safety. There is no Windows
  crash recovery (recover-owner), multi-task review, or daemon integration coverage.
- M3-06 write and resume scope is not included in this candidate and is not claimed. No delivery
  to a real remote ran: the delivery tests use throwaway local repositories.
- Windows job-object orphan closure is covered by the contract in source and by the existing
  cancel_terminates_agent_and_descendant gate. No separate orphan-survival test exists.