# Setup

Complete this once before any test. Use a throwaway data directory so no real data changes.

## 1. Build and launch

Run in a terminal from the repo root. Rust 1.96.0 and Node 22.20.0 are required (`docs/build-from-source.md`).

```sh
cargo build --workspace
npm --prefix apps/desktop install
npm --prefix apps/desktop run build
cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml
```

## 2. Create the fake agent

The fake agent stops the tests from spending money. Create the directory `$HOME/agent-orch-manual-test-bin` and put two executable scripts in it. Keep these scripts outside the repo.

- `fake-codex`: the controllable agent. It must answer the `--version` probe in under 500 ms and print a version. For a conversation turn it must print an agent reply whose final lines are the action block the test needs. Each test says which action block to emit. Control the block with a file: the script reads `$HOME/agent-orch-manual-test-bin/next-reply.txt` and prints it. If the file is empty, it prints a short text reply.
- `fake-codex-fail`: same `--version` probe, but every conversation turn exits non-zero. Tests TC-W-09 and TC-W-10 use it.

Task runs started by the fake agent must finish on a file switch too: if `$HOME/agent-orch-manual-test-bin/task-mode` contains `succeed`, `fail`, or `hang`, the task ends successfully, ends with an error, or runs without output until killed.

If you cannot build the fake agent, mark every test that needs it `BLOCKED` and say so in `RESULTS.md`. Do not substitute a real agent.

## 3. Start the app

```sh
AGENT_ORCH_DAEMON_BIN="$PWD/target/debug/orchestrator-daemon" \
AGENT_ORCH_DATA_DIR="$HOME/agent-orch-manual-test" \
AGENT_ORCH_CODEX_PROGRAM="$HOME/agent-orch-manual-test-bin/fake-codex" \
AGENT_ORCH_CONVERSATION_AGENT=codex \
npm --prefix apps/desktop run tauri dev
```

The daemon inherits the app environment and exits about a minute after it goes idle. To change an environment variable, quit the app, wait two minutes, and start again (`docs/configuration.md`).

Expected: the app window opens. The sidebar has Workspace, Workstreams, Agents, Review, and Settings. The Workspace page does not show "Fixture data: nothing was saved".

## 4. Create a test project

1. Create the folder `$HOME/agent-orch-manual-test-project` and run `git init` in it. Make one commit.
2. In the app, open the Projects page. Use its add-project control and choose that folder.
3. Name it `manual-test-project`.

Expected: `manual-test-project` is listed.

## 5. Set the settings the tests assume

1. Open Settings from the sidebar.
2. In **Agent dispatch defaults**, choose Codex as the default provider. Click **Save** for that group.
3. In **Home directory**, enter `~/agent-orch-manual-test-project` and click **Save home directory**. (TC-D-09 clears this.)
4. In **Automatic replies**, leave the caps at their defaults.

## 6. How to create wake events

| Event | How |
|-------|-----|
| Failed task | Write `fail` to `task-mode`, then approve a task proposal. |
| Stalled task | Write `hang` to `task-mode`, approve a task, and wait 5 minutes with no output. |
| Task delivery waiting | Write `succeed` to `task-mode` and approve a task with a delivery. |
| Scout report ready | Approve a `task.propose` with `task_kind` `Investigate` and `delivery` `ReportOnly`, with `task-mode` set to `succeed`. |
| Failing agent | Quit the app, set `AGENT_ORCH_CODEX_PROGRAM` to `fake-codex-fail`, start the app, and send three user messages. |

## 7. Reset between tests

Each test lists its own preconditions. To start clean, quit the app, wait two minutes, delete `$HOME/agent-orch-manual-test`, and repeat sections 3 to 5.

## 8. Clean up after the run

Quit the app. Delete `$HOME/agent-orch-manual-test`, `$HOME/agent-orch-manual-test-bin`, and `$HOME/agent-orch-manual-test-project`. Keep `docs/evidence/manual-tests/<run-UTC>/`.
