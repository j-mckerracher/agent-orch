# M1-05 cleanup-safety verification (base 46497ac)

## Finding
`cleanup_task` checked only Git state (registered, clean, HEAD reachable from base). The run owner
finishes without the data lock and closes its cancellation owner last, so cleanup could remove a
clean, landed worktree while the owner was alive. Genuine; red-tested, then fixed.

## Contract after the fix (`workflow.rs::cleanup_task`)
- Every open cancellation owner of the task must be observed `Exited` (`observe_owner_process`,
  exact identity). `Alive`, `Unknown`, mismatched or missing identity: refuse, worktree kept, error
  names the owner and points to `cancel` / `inspect` / `recover-owner`.
- No owner, or only demonstrably exited owners: existing contract (registered, clean incl.
  untracked, HEAD reachable from base ref, `git worktree remove` without `--force`).
- No open owner and task Created/Queued/Running (a startup that crashed or failed before publishing
  its owner; `run_task` holds the data lock through owner registration, so no live-run
  interleaving race is claimed or demonstrated; the lock protocol is unchanged): refuse, worktree kept. Settled
  tasks (Verifying/Succeeded/Failed/VerificationFailed/Cancelled) with no owner follow the
  existing contract.
- An Exited owner is not enough: the recorded session process identity (and its process group,
  same classification as `recover-owner`) must also be Exited/IdentityMismatch, because killing
  the owner does not stop the supervised wrapper/agent/descendants. Alive/Unknown: refuse.
  Missing session identity also refuses (fail closed, `require_session_gone`); only a settled task
  with no owner skips this check.
- Read-only: no owner retired, no process signalled, no new stale-owner deletion path.

## Red / green
| Test | Base 46497ac | Fixed |
| --- | --- | --- |
| `workflow::tests::cleanup_refuses_clean_worktree_when_owner_identity_is_unprovable` | FAIL (removed) | ok |
| `workflow::tests::cleanup_refuses_clean_worktree_while_owner_is_alive` (unix) | FAIL (check disabled) | ok |
| `workflow::tests::cleanup_follows_existing_contract_for_demonstrably_exited_owner` (unix) | ok | ok |
| `workflow::tests::cleanup_refuses_clean_worktree_of_unsettled_task_without_a_published_owner` | FAIL (removed) | ok |
| `workflow::tests::cleanup_removes_clean_landed_worktree_of_settled_task_without_an_owner` | ok | ok |
| `cleanup_safety::cleanup_refuses_running_task_that_never_registered_a_run_owner` (QA, c89c1eb, feature fault-injection) | FAIL on 8f63da4b (QA) | ok |
| `cleanup_safety::cleanup_follows_existing_contract_for_provably_exited_run_owner` (QA, 11d3e29) | ok | ok |
| `cleanup_safety::cleanup_refuses_exited_run_owner_while_its_agent_tree_is_alive` (QA, 5816c9b, unix) | FAIL on 24de6136 (QA) | ok |
| `workflow::tests::session_gone_guard_refuses_missing_alive_and_unknown_classifications` (portable) | not run on prior code (new pure guard) | ok |
| `workflow::tests::cleanup_refuses_exited_owner_without_a_recorded_session_identity` (unix) | not run on prior code (prior rule allowed a missing identity) | ok |
| `workflow::tests::cleanup_refuses_exited_owner_while_its_session_process_is_alive` (unix) | FAIL (session check disabled) | ok |
| `cleanup_safety::cleanup_refuses_clean_worktree_while_run_owner_is_live` (QA, f96832d) | FAIL | ok |
| `cleanup_safety::cleanup_refuses_staged_deleted_and_unlanded_work_in_a_landed_worktree` (QA) | ok | ok |

## Gates (macOS, `--locked`, unique target dir)
- `cargo test -p orchestrator-cli --features fault-injection` at the final commit: lib 19,
  cleanup_safety 4, cli 20, multi_task_review 6, portable_acceptance 2, recovery 7, all pass.
- `rustfmt --check` on `workflow.rs`: clean.
- `cargo clippy -p orchestrator-cli --all-targets -D warnings`: one error,
  `collapsible_if` at workflow.rs:928 (`cancel_task`), untouched pre-existing code under the
  newer toolchain (clippy 1.95). Not changed by M1-05.

## Reused evidence (unchanged code/tests)
`docs/evidence/m1-05-macos/` (real Codex run, primary HEAD/status/hashes identical) and native
Windows M1-04 evidence 5d26c659 on 46497ac (portable_acceptance 2) still prove isolation and
cancellation; `cleanup_task` changed, so cleanup behaviour needs fresh native proof.

## Remaining native gap
Windows and macOS runs of `cleanup_safety` (2) + `portable_acceptance` (2) on the final integrated
candidate. The exited-owner and alive-owner inline unit tests are `cfg(unix)` (PID/start-time
fixtures); the unprovable-identity inline test and both QA tests are portable.

## Combined M1-05 + M2-04 candidate 921203bb: fixture correction (no production change)
Three unix inline tests failed at the owner guard with `Some(Unknown)`. Cause: their "exited
owner" was this test process's own PID with `start_time_micros + 1` and the test's still-live
process group. M2-04 `observe_owner_process` now maps a same-PID mismatch to Exited only when the
recorded group is gone (otherwise Unknown, fail closed), so the forged identity is correctly
Unknown. Fixtures now use `reaped_child_identity()` (a spawned, killed and reaped child whose PID
is absent), which classifies Exited. Guards are unchanged; the Alive/Unknown owner refusals still pass.
