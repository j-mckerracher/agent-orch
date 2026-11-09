# Priority 5 Windows acceptance of c1be4bba27e819bf03427576b4c652b26067f4e1 (M1-05 cleanup safety, M2-04 reconciliation)

This run does not close M1-05 or M2-04. The coordinator decides acceptance. The expected counts are
calculated from source for Windows. They are not measured before this run, and no macOS or review
result is Windows evidence.

- Run ID: 20261109T085353Z-d524e742e2fa43908b137418f5504378
- UTC date: 2026-11-09T09:04:08.1486184Z
- Source branch: orch/priority-5-windows-acceptance-c1be4bb
- Branch tip at clone time: c1be4bba27e819bf03427576b4c652b26067f4e1
- Source commit: c1be4bba27e819bf03427576b4c652b26067f4e1
- HEAD matched the source commit before the checks and after them: yes
  (source-head-before.stdout.txt, source-head-after.stdout.txt)
- Baseline and M1 ancestry, commit count, and changed-file set: base-ancestry, m1-ancestry,
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
| workspace-tests | FAIL | 101 | False | 343.4 | 13,10,10,24,10,9,11,0,3,0,0,2,0,43,12,8,17,17,0,32,13,7,6,1,11,0,28,14,32,9,2,13,8,11,3,2,4,1,0,0,0,0,0,0,0 | 384 | 1 | 1 | Exit code 101. 1 test(s) failed. 384 passed. Expected 385 (calculated, not measured before this run). No 'ok' line for directory_at_the_lock_path_is_never_removed. |
| git-package | PASS | 0 | False | 73.301 | 32,13,7,6,1,0 | 59 | 0 | 0 |  |
| git-delivery-lib | PASS | 0 | False | 0.649 | 4 | 4 | 0 | 0 |  |
| store-data-lock-integration | PASS | 0 | False | 43.574 | 8 | 7 | 0 | 1 |  |
| store-data-lock-lib | PASS | 0 | False | 10.021 | 7 | 7 | 0 | 0 |  |
| store-lock-identity-contract | FAIL | 101 | False | 2.963 | 3 | 2 | 1 | 0 | Exit code 101. 1 test(s) failed. 2 passed. Expected 3 (calculated, not measured before this run). No 'ok' line for directory_at_the_lock_path_is_never_removed. |
| runtime-reconciliation-contract | PASS | 0 | False | 31.491 | 14 | 14 | 0 | 0 |  |
| runtime-reconcile | PASS | 0 | False | 1.84 | 28 | 28 | 0 | 0 |  |
| ipc-token-owner | PASS | 0 | False | 7.493 | 1 | 1 | 0 | 0 |  |
| cli-lib | PASS | 0 | False | 39.704 | 11 | 11 | 0 | 0 |  |
| cli-cleanup-units | PASS | 0 | False | 3.259 | 3 | 3 | 0 | 0 |  |
| cli-session-gone-guard | PASS | 0 | False | 0.618 | 1 | 1 | 0 | 0 |  |
| cli-cleanup-safety | PASS | 0 | False | 21.91 | 3 | 3 | 0 | 0 |  |
| cli-acceptance | PASS | 0 | False | 9.75 | 2 | 2 | 0 | 0 |  |

## Test result lines

### workspace-tests

```text
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.04s
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.89s
running 24 tests
test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 26.52s
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.12s
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.00s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 33.52s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.47s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 43 tests
test result: ok. 43 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s
running 12 tests
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 17 tests
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 17 tests
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.45s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 32 tests
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 15.80s
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 16.92s
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.93s
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.45s
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.46s
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 28 tests
test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 14 tests
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.66s
running 32 tests
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.24s
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.25s
running 8 tests
test result: ok. 7 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.08s
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.17s
running 3 tests
test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s
running 4 tests
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
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
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 17.17s
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 19.52s
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.22s
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.90s
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.29s
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
test result: ok. 7 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.11s
```

### store-data-lock-lib

```text
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 25 filtered out; finished in 0.01s
```

### store-lock-identity-contract

```text
running 3 tests
test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
```

### runtime-reconciliation-contract

```text
running 14 tests
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.15s
```

### runtime-reconcile

```text
running 28 tests
test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### ipc-token-owner

```text
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.04s
```

### cli-lib

```text
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.80s
```

### cli-cleanup-units

```text
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out; finished in 2.63s
```

### cli-session-gone-guard

```text
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.00s
```

### cli-cleanup-safety

```text
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.72s
```

### cli-acceptance

```text
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.34s
```

## Coverage gaps

- Unix-only tests did not run on Windows: the orphan-tree test, the fault-injection startup test, the
  runtime process-group survivor test, the store symlink test, and the Unix-only unit tests. Their
  macOS acceptance is separate and is not Windows evidence.
- cli.rs, multi_task_review.rs, recovery.rs and daemon.rs are Unix-only (cfg unix) and compile to
  zero tests on Windows. There is no Windows crash recovery (recover-owner), multi-task review, or
  daemon integration coverage.
- Windows job-object orphan closure is covered by the contract in source and by the existing
  cancel_terminates_agent_and_descendant gate. No separate orphan-survival test exists.