# Priority 7 Windows acceptance of cee8a81f63f1d876ef26c8920a052910cdd7fe3d (M2-05, M3-05, M3-06, M4-04)

This run does not close M2-05, M3-05, M3-06, or M4-04. They stay open until independent Mac, source, and
native acceptance, and the coordinator decides acceptance. The expected counts are
calculated from source for Windows. They are not measured before this run, and no macOS or review
result is Windows evidence.

- Run ID: 20261109T150506Z-27afb45e1db64f7b81d3a6980b1753ec
- UTC date: 2026-11-09T15:30:07.6182119Z
- Source branch: orch/priority-7-windows-acceptance-cee8a81
- Branch tip at clone time: cee8a81f63f1d876ef26c8920a052910cdd7fe3d
- Source commit: cee8a81f63f1d876ef26c8920a052910cdd7fe3d
- HEAD matched the source commit before the checks and after them: yes
  (source-head-before.stdout.txt, source-head-after.stdout.txt)
- Baseline ancestry (b5db71a3), commit count, and changed-file set: base-ancestry,
  range-count, range-files (consistency gates, not patch-equivalence proof)
- Runbook identity (operator-declared; exact bytes pinned by blob id, size, and SHA-256; not proof that
  the pasted commands match): runbook-identity.txt
- Overall result: PASS 
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
| workspace-tests | PASS | 0 | False | 661.21 | 16,10,10,37,10,9,17,0,3,0,6,12,22,6,2,0,43,12,8,17,17,0,32,23,6,12,6,1,11,0,28,14,32,9,2,13,8,11,3,3,2,4,1,0,0,0,0,0,0,0 | 477 | 0 | 1 |  |
| git-package | PASS | 0 | False | 182.785 | 32,23,6,12,6,1,0 | 80 | 0 | 0 |  |
| git-delivery-lib | PASS | 0 | False | 7.545 | 4 | 4 | 0 | 0 |  |
| store-data-lock-integration | PASS | 0 | False | 94.791 | 8 | 7 | 0 | 1 |  |
| store-data-lock-lib | PASS | 0 | False | 15.847 | 7 | 7 | 0 | 0 |  |
| store-lock-identity-contract | PASS | 0 | False | 6.543 | 3 | 3 | 0 | 0 |  |
| runtime-reconciliation-contract | PASS | 0 | False | 42.845 | 14 | 14 | 0 | 0 |  |
| runtime-reconcile | PASS | 0 | False | 2.821 | 28 | 28 | 0 | 0 |  |
| ipc-token-owner | PASS | 0 | False | 11.684 | 1 | 1 | 0 | 0 |  |
| cli-lib | PASS | 0 | False | 57.163 | 17 | 17 | 0 | 0 |  |
| cli-cleanup-units | PASS | 0 | False | 3.734 | 3 | 3 | 0 | 0 |  |
| cli-session-gone-guard | PASS | 0 | False | 0.804 | 1 | 1 | 0 | 0 |  |
| cli-cleanup-safety | PASS | 0 | False | 24.735 | 3 | 3 | 0 | 0 |  |
| cli-acceptance | PASS | 0 | False | 11.13 | 2 | 2 | 0 | 0 |  |
| git-delivery-acceptance-contract | PASS | 0 | False | 11.129 | 6 | 6 | 0 | 0 |  |
| store-delivery-acceptance-contract | PASS | 0 | False | 4.506 | 3 | 3 | 0 | 0 |  |
| cli-engine-reconciliation-gate | PASS | 0 | False | 40.351 | 22 | 22 | 0 | 0 |  |
| cli-crash-concurrency | PASS | 0 | False | 22.828 | 6 | 6 | 0 | 0 |  |
| cli-crash-concurrency-fault-injection | PASS | 0 | False | 35.468 | 8 | 8 | 0 | 0 |  |
| cli-multi-task-review | PASS | 0 | False | 24.531 | 6 | 6 | 0 | 0 |  |
| cli-delivery-reconciliation-contract | PASS | 0 | False | 51.889 | 12 | 12 | 0 | 0 |  |
| git-delivery-integration | PASS | 0 | False | 32.41 | 23 | 23 | 0 | 0 |  |
| git-delivery-reconcile | PASS | 0 | False | 26.741 | 12 | 12 | 0 | 0 |  |
| agents-tests | PASS | 0 | False | 71.421 | 16,10,10,37,10,9 | 92 | 0 | 0 |  |
| agents-conformance | PASS | 0 | False | 24.028 | 37 | 37 | 0 | 0 |  |

## Test result lines

### workspace-tests

```text
running 16 tests
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.44s
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.97s
running 37 tests
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 31.00s
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.78s
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
running 17 tests
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.34s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 20.94s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 17.29s
running 12 tests
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 74.25s
running 22 tests
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 45.73s
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 21.54s
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.49s
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
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.11s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 32 tests
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 15.70s
running 23 tests
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 40.10s
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 30.56s
running 12 tests
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 71.07s
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.65s
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.33s
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 28 tests
test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
running 14 tests
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 22.71s
running 32 tests
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.33s
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.44s
running 8 tests
test result: ok. 7 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.14s
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.25s
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
running 4 tests
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
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
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 14.97s
running 23 tests
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 47.33s
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 27.52s
running 12 tests
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 56.11s
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.56s
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.95s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### git-delivery-lib

```text
running 4 tests
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 28 filtered out; finished in 0.03s
```

### store-data-lock-integration

```text
running 8 tests
test result: ok. 7 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.27s
```

### store-data-lock-lib

```text
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 25 filtered out; finished in 0.05s
```

### store-lock-identity-contract

```text
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.29s
```

### runtime-reconciliation-contract

```text
running 14 tests
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 15.46s
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
running 17 tests
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.87s
```

### cli-cleanup-units

```text
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 14 filtered out; finished in 2.83s
```

### cli-session-gone-guard

```text
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 0.00s
```

### cli-cleanup-safety

```text
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.92s
```

### cli-acceptance

```text
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.33s
```

### git-delivery-acceptance-contract

```text
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.53s
```

### store-delivery-acceptance-contract

```text
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.17s
```

### cli-engine-reconciliation-gate

```text
running 22 tests
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 36.11s
```

### cli-crash-concurrency

```text
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 17.87s
```

### cli-crash-concurrency-fault-injection

```text
running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 24.00s
```

### cli-multi-task-review

```text
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 18.31s
```

### cli-delivery-reconciliation-contract

```text
running 12 tests
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 46.71s
```

### git-delivery-integration

```text
running 23 tests
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 31.97s
```

### git-delivery-reconcile

```text
running 12 tests
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 26.22s
```

### agents-tests

```text
running 16 tests
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.98s
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.53s
running 37 tests
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 29.72s
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.51s
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### agents-conformance

```text
running 37 tests
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 23.55s
```

## Coverage gaps

- Unix-only tests did not run on Windows: recovery.rs, cli.rs, daemon.rs, the orphan-tree test, the
  runtime process-group survivor test, the store symlink test, and the Unix-only unit tests. Their
  macOS acceptance is separate and is not Windows evidence.
- M2-05: two tasks start sequentially with live overlap, not simultaneously. Only the Admit event
  insert is faulted. The Windows crash is a hook exit plus Job Object owner-death reaping proven
  without a test-issued kill. The old recovery.rs and external sqlite3 tests are not Windows proof.
- M3-05: one OS process with a durable store reopen and opaque digests. It is not daemon, engine-CLI,
  or process-crash proof.
- M3-06: live-store read-only snapshots with local Git. No delivery to a real remote ran.
- M4-04: fake children only, no live provider. taskkill /T /F is a forced kill, not a graceful
  SIGINT. CRLF is tolerance only. Approvals are no-grant and routing-refusal only.
- No clippy or rustfmt gate ran. The workspace run is not isolated.