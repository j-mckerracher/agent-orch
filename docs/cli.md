# Command-line client

The CLI runs one Codex task in an isolated Git worktree and inspects stored tasks. It is not in the installers, so build it from source ([requirements](build-from-source.md#requirements)).

## Build it

```sh
cargo build --release -p orchestrator-cli
./target/release/orchestrator-cli --help
```

The binary is named `orchestrator-cli`, and its help text calls the command `agent-orch`. The examples below use `agent-orch` for short.

## Run a task

```sh
agent-orch run --repo /path/to/repo --objective "Add a --version flag"
```

The task runs in a new worktree under the data directory, never in your checkout. It needs `codex` on `PATH` and a signed-in Codex CLI.

### Options

| Option | Default | Meaning |
| --- | --- | --- |
| `--repo` | required | Git repository to work in |
| `--objective` | required | The work request passed to Codex |
| `--task-id` | generated | Stable task ID |
| `--base-ref` | `main` | Commit or branch to start the worktree from |
| `--agent-program` | `codex` | Codex executable path or name |

### Result

When Codex finishes, the CLI records the candidate commit and prints `candidate: <commit>`. It exits 0 on success, 130 after a confirmed cancellation, and nonzero on failure.

## Inspect and review

```sh
agent-orch tasks                 # list task IDs and states
agent-orch inspect <task-id>     # attempts, sessions, worktree, result
agent-orch diff <task-id>        # changes since the base commit
```

## Cancel a task

```sh
agent-orch cancel <task-id>
```

## Clean up

```sh
agent-orch cleanup <task-id>
```

Cleanup removes a worktree only when it is clean and its commit is reachable from the base. Otherwise it exits nonzero and keeps the worktree.

## Recover a stuck owner

If a run died and `inspect` still shows an unresolved owner, release it by hand once you have confirmed its processes exited. Copy the attempt ID, session ID, and generation from `inspect`.

```sh
agent-orch recover-owner <task-id> --attempt-id <id> --session-id <id> \
  --generation <hex> --confirm-processes-exited --reason "checked: no agent processes remain"
```

## Choose the data directory

Every command accepts `--data-dir <path>`. The default is the [data directory](configuration.md#data-directory). The data directory must be outside the repository you run against.
