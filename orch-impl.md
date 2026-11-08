# Orchestration learnings (durable)

Recorded by the coordinator on 2026-11-08 (America/Denver, -07:00). Add only learnings that change how future orchestration or implementation is done. Routine status belongs in board.json and the execution plan.

## Worker runtime

- The OMP provider ID for gpt-6-luna is `openai-codex`. The `codex/gpt-6-luna` form in orch-impl-m.md is not resolved by OMP ("Model not found"). Verify the real model, provider, and thinking level with RPC `get_state` (`model.provider`, `model.id`, `thinkingLevel`) before sending a prompt.
- The RPC driver must pass absolute paths for `--session-dir` and `--resume`. OMP runs with its working directory set to the worktree, so a relative path fails with "Session not found" and exit 4. A resumed session keeps its original sessions directory, so resume from the first run's `sessions/` folder.
- Interactive requests are the only extension UI frames that need a reply (select, confirm, input, editor). Presentation frames such as the caveman `setStatus` spinner arrive several times a second and grew event logs to hundreds of megabytes. Log only the interactive methods.
- A worker can stall inside one very large generation: no bytes and no events for more than 20 minutes. Sample `events.log` byte growth over 60 seconds to tell a stall from progress. Recover by stopping the driver, keeping the worktree, and resuming the same session with a note that asks for writes of about 150 lines or fewer per call.
- Driver stop: a background job started with `&` from a non-interactive shell ignores SIGINT. Stop the driver with SIGTERM, then confirm its OMP child exited. Better, start drivers as attached async jobs so they can be stopped and monitored. Use absolute paths; in `cd X && cmd &` the `cd` runs in a subshell and the next command starts in the wrong directory.
- Always pass `--deadline-minutes` explicitly and record it in the board. The driver's default is 240 minutes, which is not a box. A stalled run with a 180-minute deadline kept writing uncommitted code for over an hour.
- A worker that spends 30 minutes or more on reads with no writes needs intervention. The M5-02 worker made about 250 read calls before its first write. Stop it, keep the session, and resume with a write-first instruction.

- Verification briefs must not halt on the first environment-class failure. The M5-01 verifier stopped at a Windows C cross-build failure and skipped the code review. Tell verifiers to classify an environment failure, record it, and continue with every independent step.
- Windows cross-checks on macOS cannot compile crates that link C code. The daemon depends on libsqlite3-sys, and `cargo check --target x86_64-pc-windows-msvc` fails on macOS for lack of Windows C headers. Verify that crate on the Windows machine. The ipc crate's cross-check is valid on macOS.
- Cross-platform modules gated with `cfg(unix)` hide the Windows implementation. In crates/orchestrator-ipc/src/lib.rs, the transport, session, server, auth, and client modules are `cfg(unix)`. Adding a Windows transport requires changing those gates to `any(unix, windows)`, so plan that scope up front.

## Briefs and scope

- Three workers were blocked by briefs that asked for a write outside their writable scope: an evidence directory, a runtime manifest, and a stale-lock store change. Every path a brief asks a worker to write must be listed in its writable scope. Amend the scope and resume the same session rather than letting the worker stop.
- Copy each item's `details` from board.json verbatim into its brief. One brief was written from plan text, not from the board, and omitted required scope (the data-directory lock and process identity). Check each brief against the board before dispatch.

## Cross-machine handoffs

- A worker or agent on another machine can read only what is in Git. A runbook kept in the coordinator's session folder is invisible to it, and the Windows agent ran a different procedure on an older commit instead. Publish every runbook as a file on a pushed branch, pin the source commit, and give the agent the exact branch and a direct blob URL. Require it to publish its evidence on its own branch, and verify that branch before counting any result.

## Integration and board edits

- Parallel `edit` calls on board.json in one response can race. After a batch of edits, reparse the file and check every required key and every dependency reference. One edit once dropped the `id`, `milestone`, and `title` lines of an item.
- Candidates that each add dependencies conflict in Cargo.lock. Resolve by keeping one side, running an unlocked `cargo metadata` to add the new entries (existing pins are preserved), confirming the lock diff only adds lines, then verifying with `cargo build --locked`.
- Verify each candidate independently before merging: scope against the brief, `cargo test --locked --workspace`, and clippy for the touched crates, judged by each command's own exit code.

## Safety-critical behavior

- Crash recovery needs a stale-lock takeover. A lock from a dead holder must be reclaimed by renaming it to a private directory, verifying the inode and token, and only then removing it, restoring it if verification fails. Never remove a lock by name alone.
- An adapter must never report a completed turn as success when the process then exits non-zero. The conformance suite found this in Codex, Claude Code, and Pi, and it must stay tested. A malformed protocol line on an exit-0 run is a protocol violation, not success.
- Process identity (PID, process group, start time) must be checked before any signal is sent. A PID alone is not proof of ownership.
