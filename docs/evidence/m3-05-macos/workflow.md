# M3-05 macOS workflow verification

Updated: 2026-11-09T07:17:00-07:00.

## Workflow

`crates/orchestrator-cli/tests/multi_task_review.rs` contains six integration tests. Each test uses a unique temporary SQLite database and Git repository. The tests compose public core, Git, and store APIs. They do not launch agent processes or contact a remote repository.

1. A JSON proposal defines independent tasks A and B, plus task C depending on both. Different proposal-local IDs produce the same canonical plan IDs. C's dependencies reference canonical A and B IDs.
2. The scheduler starts A and B from a controlled queued snapshot. It rejects C while either dependency remains incomplete. It starts C after both dependencies succeed.
3. The verifier records a trusted `git rev-parse --verify --quiet refs/heads/missing` validation command's exit code 1 as a failure. SQLite does not consider that run eligible for delivery. The exact candidate is approved first, and local-review delivery still refuses it because verification failed, leaving all refs unchanged. A separate candidate changing `config/ci/verification.yml` fails protected-path verification and names that path.
4. Delivery approval binds local-review action kind, a 32-byte parameter digest, candidate SHA, and task ID. Tests reject action-kind, digest, and candidate changes. An agent cannot approve. After a candidate changes, the store is reopened from disk; delivery then refuses the old approval, and the earlier verification run no longer matches the changed candidate. A user approves the new candidate before delivery succeeds. The digests are opaque test constants: these tests do not derive them from destination, base SHA, or target ref the way the daemon does, so they do not prove binding to those values.
5. The restart test drops its only `SqliteStore` handle and opens the same database after task admission, scheduling, each verification stage, approvals, and delivery. Task records, scheduler results, verification evidence, approval records and events, delivery reports, and delivery audit events match their pre-restart values. After the approval-stage reopen, the persisted approval rejects a different candidate and a different digest, and an unapproved delivery attempt is refused; the unapproved attempt uses a nonexistent approval ID.
6. A delivery attempt without an approval persists a refusal and leaves all Git refs unchanged. Approved delivery creates one review ref per task. Existing refs, `main`, the primary checkout's `HEAD`, and its working-tree status remain unchanged.

The final database contains three passed verification records, three approved action records, and three successful local-review delivery reports. It also retains one refused report for the unapproved attempt. The final Git ref delta contains only `refs/heads/orchestrator/review/` refs.

## Historical verification commands (2026-11-08, macOS, superseded)

This table records the original candidate `5b85878` with the Unix-shell validation command. It is not evidence for the pinned candidates below.

| Command | Exit code | Result |
| --- | ---: | --- |
| `cargo test --locked --workspace` before edits | 0 | 249 passed across 33 suites. One test ignored. |
| `cargo test --locked --workspace --no-run` | 0 | All workspace test targets compiled. |
| First `cargo test --locked -p orchestrator-cli --test multi_task_review` | 101 | Compile errors exposed an incorrect `TaskRecord` import and callback lifetime types. |
| Second `cargo test --locked -p orchestrator-cli --test multi_task_review` | 101 | Compile error exposed a missing independent callback borrow lifetime. |
| Final `cargo test --locked -p orchestrator-cli --test multi_task_review` | 0 | 6 passed. |
| `rustfmt --edition 2021 crates/orchestrator-cli/tests/multi_task_review.rs` | 0 | Formatted the new test file. |
| `rustfmt --check --edition 2021 crates/orchestrator-cli/tests/multi_task_review.rs` | 0 | Formatting check passed after the final code edit. |
| First `cargo clippy --locked -p orchestrator-cli --all-targets --no-deps` | 0 | Reported one `clippy::useless_vec` warning. Replaced the fixed-size vector with an array. |
| Final `cargo test --locked --workspace` | 0 | 255 passed across 34 suites. One test ignored. |
| Final `cargo clippy --locked -p orchestrator-cli --all-targets --no-deps` | 0 | Passed without warnings. |
| `git diff --cached --check` | 0 | Staged test and evidence files passed whitespace checks. |
| `git diff --check` | 0 | No unstaged whitespace errors. |

Scope limits: "dependency execution" is scheduler decisions over controlled store snapshots; the tests advance tasks by hand and there is no engine or CLI delivery path. "Restart" is dropping the store handle and reopening the database in the same process, not a process restart. Candidates for all three tasks branch from the same base.

The 2026-11-08 run above covered macOS only and used a Unix shell validation command. On 2026-11-09 the file lost its `cfg(unix)` gate and the failing validation command became `git rev-parse`, so the tests are no longer gated out on Windows. A native Windows run at the pinned commit is still pending and no Windows result is recorded here; a real remote was never tested. The tests use local temporary repositories.

## Pinned macOS reverification (2026-11-09, base `b5db71a3b395d49c0767d8019c502d9fadfd0aa3`)

Both runs above used one shared Cargo target under the coordinator's `.agent-worktrees/cargo-target-m2-06-reverify-20261109`. The coordinator granted an exclusive lease; that grant is not captured in the raw artifacts. Each run started with `cargo clean --locked -p orchestrator-cli`. The first run compiled six local crates, because the worktree sat at a new path. The second compiled only `orchestrator-cli`. The two SHAs differ only in the test file and this document, so dependency crates are identical. Disk free space was 41 GiB. The task worktree HEAD matched the pinned SHA and `git status` was clean before and after each run. Raw stdout, stderr, exit codes, argv, and timestamps are kept outside the checkout in the evidence directories named below.

| Pinned SHA | Evidence directory | Command | Exit | Captured result |
| --- | --- | --- | ---: | --- |
| `e518be2bfd26a4494fc4ed7f7c3d12433f0861b0` | `m3-05-evidence-20261109T140326Z-11742` | `cargo clean --locked -p orchestrator-cli` | 0 | Removed 3041 files, 419.0 MiB. |
| | | `cargo test --locked -p orchestrator-cli --test multi_task_review` | 0 | 6 passed, 0 failed, 0 ignored, 0 filtered out, in 4.17 s. |
| | | `rustfmt --check --edition 2021 crates/orchestrator-cli/tests/multi_task_review.rs` | 0 | No output. |
| | | `cargo clippy --locked -p orchestrator-cli --all-targets --no-deps` | 0 | Warnings, listed below. |
| `54d469e1b5b27a514cfe1d650af4ef62bc4549d3` | `m3-05-evidence-r2-20261109T140503Z-17634` | `cargo clean --locked -p orchestrator-cli` | 0 | Removed 781 files, 141.1 MiB. |
| | | `cargo test --locked -p orchestrator-cli --test multi_task_review` | 0 | 6 passed, 0 failed, 0 ignored, 0 filtered out, in 4.59 s. |
| | | `rustfmt --check --edition 2021 crates/orchestrator-cli/tests/multi_task_review.rs` | 0 | No output. |
| | | `cargo clippy --locked -p orchestrator-cli --all-targets --no-deps` | 0 | Same warnings as the `e518be2b` run. |

`54d469e1` is the exact-approval, stale-verification, and disclosure follow-up to `e518be2b`. Only the `54d469e1` run covers its test changes. Each run is an exit-0 result; no run failed.

Clippy exited 0 in both runs but printed these warnings, none in `multi_task_review.rs`: unused import `AuditEvent`; function `cancel_command` is never used; a collapsible `if` statement; unused imports `CancellationStatus` and `StoreError`. The summary lines attribute 1 warning to the `orchestrator-cli` library (one duplicate in its unit-test target) and 3 to the `recovery` test target. They belong to code outside this item's scope and were left unchanged.

### Independent QA run at `1916ed35f120113911cf496280091b5e4c2a9c6a`

`1916ed35` adds the reopened-store assertions: the stale-candidate refusal in the approval test, and the candidate and digest mismatch after the restart test's approval-stage reopen. The independent QA agent, not the implementer, ran it on 2026-11-09 from 14:14:24Z to 14:14:41Z in this worktree, using the same shared Cargo target. Raw argv, stdout, stderr, exit codes, timestamps, toolchain, and HEAD/status are in `m3-05-qa-1916ed35-20261109T141500Z-41852` under the coordinator's `.agent-worktrees`. The toolchain was rustc 1.95.0, cargo 1.95.0, rustfmt 1.9.0-stable, clippy 0.1.95, git 2.56.0. HEAD was `1916ed35f120113911cf496280091b5e4c2a9c6a` with a clean status before and after, and 41 GiB was free.

| Command | Exit | Captured result |
| --- | ---: | --- |
| `cargo clean --locked -p orchestrator-agents -p orchestrator-cli` | 0 | Removed 1976 files, 258.3 MiB. |
| `cargo test --locked -p orchestrator-cli --test multi_task_review` | 0 | 6 passed, 0 failed, 0 ignored, 0 filtered out, in 4.74 s. Compiled `orchestrator-agents` and `orchestrator-cli`. |
| `rustfmt --check --edition 2021 crates/orchestrator-cli/tests/multi_task_review.rs` | 0 | No output. |
| `cargo clippy --locked -p orchestrator-cli --test multi_task_review --no-deps` | 0 | One `collapsible_if` warning at `crates/orchestrator-cli/src/workflow.rs:938`, in production code. None in `multi_task_review.rs`. |

This clippy command checks only the `multi_task_review` test target and the library, narrower than the `--all-targets` runs above. The `--all-targets` selector was not run at `1916ed35`, so the other warnings listed above were not re-observed.

The raw artifact shows that this run passed at `1916ed35`. It does not show that the new assertions fail when the invariant breaks, because no mutation check was run.

### Not covered by these results

- Native Windows. All six scenarios are enabled for Windows, but no Windows run exists for any SHA above. Mac results do not transfer. The final combined candidate needs its own native run on its exact SHA.
- The Windows-target `cargo check` was skipped on the coordinator's instruction, because native acceptance is required anyway; that instruction is not in the raw artifacts.
- Parameter digests are opaque constants. No test derives them from destination, base SHA, or target ref.
- No mutation check was run, such as swapping the verification and approval order. No test redelivers after the final restart or delivers with an old verification run plus a new approval.
- Restart means reopening the store in one process. Dependency execution means scheduler decisions over controlled snapshots. There is no CLI delivery command, so delivery is exercised through library calls.
- Commits after `1916ed35` change only this document. Their text was not itself tested.
- The final combined candidate will be validated separately; these runs are not that validation.
