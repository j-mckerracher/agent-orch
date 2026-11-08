# M3-05 macOS workflow verification

Updated: 2026-11-08T03:47:46-07:00.

## Workflow

`crates/orchestrator-cli/tests/multi_task_review.rs` contains six integration tests. Each test uses a unique temporary SQLite database and Git repository. The tests compose public core, Git, and store APIs. They do not launch agent processes or contact a remote repository.

1. A JSON proposal defines independent tasks A and B, plus task C depending on both. Different proposal-local IDs produce the same canonical plan IDs. C's dependencies reference canonical A and B IDs.
2. The scheduler starts A and B from a controlled queued snapshot. It rejects C while either dependency remains incomplete. It starts C after both dependencies succeed.
3. The verifier records a trusted validation command's exit code 7 as a failure. SQLite does not consider that run eligible for delivery, and local-review delivery refuses it. A separate candidate changing `config/ci/verification.yml` fails protected-path verification and names that path.
4. Delivery approval binds local-review action kind, a 32-byte parameter digest, candidate SHA, and task ID. Tests reject action-kind, digest, and candidate changes. An agent cannot approve. After a candidate changes, delivery refuses the old approval. A user approves the new candidate before delivery succeeds.
5. The restart test drops its only `SqliteStore` handle and opens the same database after task admission, scheduling, each verification stage, approvals, and delivery. Task records, scheduler results, verification evidence, approval records and events, delivery reports, and delivery audit events match their pre-restart values.
6. A delivery attempt without an approval persists a refusal and leaves all Git refs unchanged. Approved delivery creates one review ref per task. Existing refs, `main`, the primary checkout's `HEAD`, and its working-tree status remain unchanged.

The final database contains three passed verification records, three approved action records, and three successful local-review delivery reports. It also retains one refused report for the unapproved attempt. The final Git ref delta contains only `refs/heads/orchestrator/review/` refs.

## Verification commands

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

Windows and a real remote were not tested. The tests use Unix shell commands and local temporary repositories.
