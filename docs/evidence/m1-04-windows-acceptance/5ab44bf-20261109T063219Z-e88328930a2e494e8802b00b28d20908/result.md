# M1-04 Windows CLI acceptance of 5ab44bf67a83387e9ffe2d6c8309a4870de92f60

This run does not close M1-04. The coordinator decides acceptance.
This candidate adds Git error detail to CLI errors. A FAIL here is expected to
carry the real Git or I/O cause. The cause is unproven until it is read from
cli-acceptance.stdout.txt and cli-acceptance.stderr.txt.

- Run ID: 20261109T063219Z-e88328930a2e494e8802b00b28d20908
- UTC date: 2026-11-09T06:38:15.4325322Z
- Source branch: orch/m1-04-windows-acceptance-5ab44bf
- Branch tip at clone time: 5ab44bf67a83387e9ffe2d6c8309a4870de92f60
- Source commit: 5ab44bf67a83387e9ffe2d6c8309a4870de92f60
- HEAD matched the source commit before the checks and after them: yes
  (source-head-before.stdout.txt, source-head-after.stdout.txt)
- Runbook identity (operator-declared, not verified): runbook-identity.txt
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

| Command | Status | Exit | Timed out | Running (N per binary) | Passed | Failed | Ignored | Notes |
|---|---|---|---|---|---|---|---|---|
| workspace-tests | FAIL | 101 | False | 13,10,10,24,10,9,7,0,0,0,2,0,43,12,8,17,17,0,24,13,7,6,1,11,0,7,27,9,2,13,8,11,2,4,1,0,0,0,0,0,0,0 | 324 | 3 | 1 | Exit code 101. 3 test(s) failed. 324 passed. Expected 327. |
| git-package | PASS | 0 | False | 24,13,7,6,1,0 | 51 | 0 | 0 |  |
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
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.62s
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.67s
running 24 tests
test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.49s
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.62s
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 2 tests
test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.94s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 43 tests
test result: ok. 43 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
running 12 tests
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 17 tests
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
running 17 tests
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.25s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 24 tests
test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 14.57s
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.95s
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.51s
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.70s
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.09s
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 7 tests
test result: FAILED. 6 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
running 27 tests
test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.23s
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.32s
running 8 tests
test result: ok. 7 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.09s
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
running 4 tests
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
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
running 24 tests
test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.76s
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.64s
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.21s
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.41s
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.10s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### store-data-lock-integration

```text
running 8 tests
test result: ok. 7 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.09s
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
test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.98s
```

## Coverage gaps

- Windows CLI coverage is reported in `cli-acceptance`: one run to a candidate, task inspection, diff, cleanup, primary-checkout invariance, and cancellation with a descendant process. The files `cli.rs`, `multi_task_review.rs`, and `recovery.rs` are Unix-only (`cfg(unix)`) and compile to zero tests on Windows.
- Windows crash recovery (`recover-owner`) and multi-task review coverage: none.
- Windows daemon integration coverage: none; `crates/orchestrator-daemon/tests/daemon.rs` is Unix-only (`cfg(unix)`) and compiles to zero tests on Windows.

## Failure details

- `workspace-tests`: 324 passed, 3 failed, 1 ignored, versus the calculated 327 passed and 1 ignored. Failures were `run_produces_isolated_diff_and_cleanup_preserves_primary`, `cancel_terminates_agent_and_descendant`, and `windows_owner_observation_uses_creation_time_not_only_pid`.
- The first assertion message from the runtime failure was `assertion left == right failed`: expected `Exited`, observed `Unknown` at `crates/orchestrator-runtime/tests/reconcile.rs:292:5`.
- `cli-acceptance`: `running 2 tests`; 0 passed, 2 failed. The survivor scan file is empty (0 bytes), so no process matched the run-specific scan after this check. This scan does not cover `workspace-tests`.
- Both CLI acceptance tests failed during worktree creation. The full tool line is preserved below; the cause is reported by the tool and has not yet been analyzed.

```text
stderr: agent-orch: cannot create worktree for task accept-run: worktree creation failed at \\?\C:\Users\<user>\AppData\Local\Temp\agent-orch-m1-04-acc-20261109T063219Z-e88328930a2e494e8802b00b28d20908\test-tmp\.tmpHVJoeM\data\worktrees\accept-run on branch orchestrator/accept-run; the partially created state was left in place for inspection: git add worktree exited with exit code: 128: Preparing worktree (new branch 'orchestrator/accept-run'); fatal: could not create leading directories of '//?/C:/Users/<user>/AppData/Local/Temp/agent-orch-m1-04-acc-20261109T063219Z-e88328930a2e494e8802b00b28d20908/test-tmp/.tmpHVJoeM/data/worktrees/accept-run/.git': Invalid argument
```