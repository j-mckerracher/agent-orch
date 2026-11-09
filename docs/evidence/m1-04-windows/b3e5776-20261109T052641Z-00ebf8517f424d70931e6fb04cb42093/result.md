# M1-04 Windows CLI acceptance of b3e5776122b1a40e85c145169a17faf2270161a4

This run does not close M1-04. The coordinator decides acceptance.

- Run ID: 20261109T052641Z-00ebf8517f424d70931e6fb04cb42093
- UTC date: 2026-11-09T05:33:24.8950985Z
- Source branch: orch/m1-04-windows-acceptance-b3e5776
- Branch tip at clone time: b3e5776122b1a40e85c145169a17faf2270161a4
- Source commit: b3e5776122b1a40e85c145169a17faf2270161a4
- HEAD matched the source commit before the checks and after them: yes
- Overall result: FAIL 
- Toolchain: see git-version.stdout.txt, rustc-version.stdout.txt,
  cargo-version.stdout.txt, git-autocrlf.stdout.txt (exit code in
  git-autocrlf.exit.txt)
- OS, CPU, PowerShell: see os.txt, cpu.txt, powershell-version.txt
- Agent runtime and Sonnet 5.5/high setting: Codex desktop; Sonnet 5.5/high unverified
- Required IPC check: the ipc-token-owner row below covers
  startup_replaces_only_regular_stale_token and is part of the overall result.
- Required CLI check: the cli-acceptance row below covers both
  portable_acceptance tests and a report-only survivor scan. The scan covers
  processes whose image lies under this run's private temp directory and
  processes whose command line names this run's directory (long or 8.3 form). It does not cover workspace-tests,
  which runs the same tests with the default TEMP. The row is part of the
  overall result.
- First failing CLI assertion: `run_produces_isolated_diff_and_cleanup_preserves_primary` reported `run failed: ExitStatus(ExitStatus(1))`; stderr said worktree creation failed for task `accept-run` on branch `orchestrator/accept-run`, leaving partial state for inspection.
- `cli-acceptance.survivors.txt` is empty; the run-specific survivor scan found no processes.

| Command | Status | Exit | Timed out | Running (N per binary) | Passed | Failed | Ignored | Notes |
|---|---|---|---|---|---|---|---|---|
| workspace-tests | FAIL | 101 | False | 13,10,10,24,10,9,7,0,0,0,2,0,43,12,8,17,17,0,20,13,7,6,1,11,0,7,27,9,2,13,8,11,2,4,1,0,0,0,0,0,0,0 | 321 | 2 | 1 | Exit code 101. 2 test(s) failed. 321 passed. Expected 323. |
| git-package | PASS | 0 | False | 20,13,7,6,1,0 | 47 | 0 | 0 |  |
| store-data-lock-integration | PASS | 0 | False | 8 | 7 | 0 | 1 |  |
| store-data-lock-lib | PASS | 0 | False | 2 | 2 | 0 | 0 |  |
| ipc-token-owner | PASS | 0 | False | 1 | 1 | 0 | 0 |  |
| cli-acceptance | FAIL | 101 | False | 2 | 0 | 2 | 0 | Exit code 101. 2 test(s) failed. 0 passed. Expected 2. No 'ok' line for run_produces_isolated_diff_and_cleanup_preserves_primary. No 'ok' line for cancel_terminates_agent_and_descendant. |

## Test result lines

### workspace-tests

```text
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.41s
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.72s
running 24 tests
test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.30s
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.82s
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 2 tests
test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.04s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 43 tests
test result: ok. 43 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
running 12 tests
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 17 tests
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
running 17 tests
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.89s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 20 tests
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.76s
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.44s
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.72s
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.42s
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.38s
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
running 27 tests
test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.55s
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.38s
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.25s
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.64s
running 8 tests
test result: ok. 7 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.51s
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.56s
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.41s
running 4 tests
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
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
running 20 tests
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 23.79s
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.00s
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.12s
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.86s
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.19s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### store-data-lock-integration

```text
running 8 tests
test result: ok. 7 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.12s
```

### store-data-lock-lib

```text
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 25 filtered out; finished in 0.01s
```

### ipc-token-owner

```text
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.04s
```

### cli-acceptance

```text
running 2 tests
test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.39s
```

## Coverage gaps

- Windows CLI coverage comes from the cli-acceptance row: one un to a
  candidate, 	asks, inspect, diff, cleanup, primary-checkout
  invariance, and cancel with a descendant process. The files cli.rs,
  multi_task_review.rs, and recovery.rs are Unix-only (cfg unix) and compile to
  zero tests on Windows.
- Windows crash recovery (ecover-owner) and multi-task review coverage: none.
- Windows daemon integration coverage: none. crates/orchestrator-daemon/tests/daemon.rs
  is Unix-only (cfg unix) and compiles to zero tests on Windows.