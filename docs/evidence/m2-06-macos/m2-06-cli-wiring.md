# M2-06 CLI persistence and process control

Updated: 2026-11-08T09:46:33Z.

## Task records

The CLI stores task, attempt, session, and launch-identity records in `<data-dir>/store.sqlite`. `SqliteStore` applies its embedded migrations when the CLI opens the database. Existing JSON `FileStore` task records are not migrated or read by the CLI. No migration command is provided.

The CLI keeps repository, worktree, and log paths in sidecar files under `<data-dir>/cli/<task-id>/`. An unavailable launch identity is written to `session.log` and printed to stderr. A later run treats an unfinished session without a stored identity as unresolved and refuses to schedule.

## Data-directory lock scope

`run` acquires `.orchestrator.lock` before reconciliation and scheduling. It holds the lock through task, worktree, attempt, session, and launch-identity writes. It releases the lock before it monitors the agent, then reacquires it to record the finish state. Agent execution has no fixed duration. Releasing the lock lets `cancel` acquire it while the agent runs.

`cancel` holds the lock while it loads the session identity, observes the process, signals the process group, and records cancellation through store transitions. It refuses to signal if PID, process group, or start time cannot be verified. On Unix, it sends `SIGTERM`, then sends `SIGKILL` after 500 ms only if a second observation still matches the stored identity. The running CLI records the session exit after it reaps the agent.

`cleanup` holds the lock through its worktree checks and removal. `tasks` and `inspect` do not acquire the lock. Mutating commands wait up to two seconds for a live holder and name its PID when known.

## Verification

`cargo test --locked -p orchestrator-cli --test cli` passed 18 tests on macOS.
