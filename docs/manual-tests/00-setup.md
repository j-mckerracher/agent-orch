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

Task runs started by the fake agent must finish on a file switch too: if `$HOME/agent-orch-manual-test-bin/task-mode` contains `succeed`, `fail`, or `hang`, the task exits 0 without changing a file, ends with an error, or runs without output until killed. An Implement task that exits 0 without a file change ends `Failed` with the reason "The agent exited without changing any files.", so `succeed` never reaches `Verifying`. Investigate tasks are not affected: they may finish without a file change.

**Missing: a `write` mode.** A candidate needs a task run that changes one file and exits 0. The contract above has no such mode. The fake agent script is not in the repo, so this section is the only specification. The Rust test fakes in `crates/orchestrator-daemon/tests/daemon.rs` and `crates/orchestrator-daemon/tests/work_classes.rs` print JSON events and never write a file, so they cannot stand in. To close the gap, add a `write` branch to `$HOME/agent-orch-manual-test-bin/fake-codex`: when `task-mode` contains `write`, create one file in the task's worktree and exit 0. Then add `write` to the list above. Until the branch exists, every test that needs a candidate is `BLOCKED`: TC-W-05 (delivery part), TC-W-16, and TC-I-10 step 7.

**Missing: the prompt log, session ids and the wait modes.** Tests from `05-decisions.md` on need four more behaviors. Like `write`, they are specified only here. Each test names the ones it needs; until the fake agent has them, that test is `BLOCKED`.

- **Prompt log.** Append every prompt to `$HOME/agent-orch-manual-test-bin/prompts.log`. The prompt is the last argument. Write a line `=== <UTC time> <kind> <session id or -> ===` before each one. `<kind>` is `task` when the prompt starts with `# MCAO worker brief`, `resume` when the arguments start with `exec resume` (the prompt then starts with `# MCAO worker brief, continued`, and the argument before it is the session id), and `turn` otherwise.
- **Session id.** For a task run, print `{"type":"thread.started","thread_id":"<uuid>"}` as the first event, with a new v4 UUID. For a resume, print the session id it was given. The daemon cannot resume a task without it.
- **`ask` and `block` modes.** When `task-mode` contains `ask`, find the status file the brief names (the path after `the end of the status file`), append the line `needs-decision [key=readme-word] Should the README line say alpha or beta?`, and exit 0. `block` appends `blocked The test database does not answer.` and exits 0. A run that exits non-zero raises no question.
- **Resume.** On `exec resume`, ignore `task-mode` and behave as `succeed`. Otherwise the task asks again forever.

If you cannot build the fake agent, mark every test that needs it `BLOCKED` and say so in `RESULTS.md`. Do not substitute a real agent. A step marked **Real agent** is the one exception: it names the CLI and permission level it needs, spends money, and runs only when the person who asked for the run allows it. Otherwise mark it `BLOCKED`.

## 2a. Write an action block

The fake agent prints `next-reply.txt` as the reply, so the file holds a short text and then one fenced block. A block that does not parse is shown as "The orchestrator's action block in this reply could not be read. No action was proposed. Nothing was changed."

````text
Here is the plan.
```mcao-actions
{
"mcao_actions": 1,
"actions": [
{ "kind": "task.propose", "project": "p-1a2b3c4d", "objective": "Add a README line", "agent": "codex", "task_kind": "Implement", "delivery": "ReportOnly" }
]
}
```
````

- **Project code.** `project` is the project's code, `p-` and 8 hex digits, never its name. A name does not resolve, and the action is refused or waits. Compute the code once: `printf '%s' "$(cd "$HOME/agent-orch-manual-test-project" && pwd -P)" | shasum -a 256 | cut -c1-8`, then put `p-` in front. A task card shows it too, in its **Project** field, as `manual-test-project (p-…)`. Where a test file says "project `manual-test-project`", use this code.
- **Kinds and fields.** They are listed in the preamble in `crates/orchestrator-daemon/src/api_conversation_agent.rs` (`INSTRUCTIONS_WITH_ACTIONS`). A block holds at most 10 actions. An unknown kind or field rejects the whole block.
- **The fake re-reads the file on every turn.** That includes wake turns, report relays and a coordinator's turn for routed work. A block left in the file is sent again by the next automatic turn. Each test says when to rewrite or clear the file. Clear it means: write one short line of text, such as `Noted.`

## 2b. Acting on its own

**Settings > Acting on its own** has the switch "Let the orchestrator act on its own" and the button **Save acting on its own**. It is on in a new data directory.

- **On.** After a turn that came from your message, the daemon runs the actions it may run by itself: tasks, retries, messages to waiting tasks, questions, workstreams, objectives, learnings and routes to a coordinator, at most 5 per reply. Their cards read "Done on its own". A turn the app started (a wake or a report relay) may only retry, message, ask, or (a coordinator) report and start tasks for a request routed to it, at most 5 per request. Everything else stays a card that waits for **Approve** (`07-authority.md`).
- **Off.** Every action waits for **Approve**, unless a standing approval covers it.
- **Timing.** The daemon runs these actions just after the turn completes. A card can show "Pending approval" for a moment before it reads "Done on its own". Wait a few seconds before you read the pill.

Files `01` to `04` test the approval card. Run them with the switch **off** (section 5, step 5).

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
4. In **Automatic replies**, leave the caps at their defaults: 20 replies per hour and 160 per day, per conversation.
5. For files `01` to `04`: in **Acting on its own**, clear the switch "Let the orchestrator act on its own" and click **Save acting on its own**. The result reads "Saved.". Files from `05` on say which setting they need.

## 6. How to create wake events

| Event | How |
| ------- | ----- |
| Failed task | Write `fail` to `task-mode`, then approve a task proposal. |
| Stalled task | Write `hang` to `task-mode`, approve a task, and wait 5 minutes with no output. |
| Empty change set fails | Write `succeed` to `task-mode` and approve an Implement task. The task ends `Failed` with "The agent exited without changing any files." |
| Candidate ready | Write `write` to `task-mode` and approve an Implement task. `BLOCKED` until the fake agent supports `write` (section 2). |
| Task delivery waiting | Write `write` to `task-mode` and approve a task with a delivery. `BLOCKED` until the fake agent supports `write` (section 2). |
| Scout report ready | Approve a `task.propose` with `task_kind` `Investigate` and `delivery` `ReportOnly`, with `task-mode` set to `succeed`. |
| Failing agent | Quit the app, set `AGENT_ORCH_CODEX_PROGRAM` to `fake-codex-fail`, start the app, and send three user messages. |
| Worker needs a decision | Write `ask` to `task-mode` and start a task. `BLOCKED` until the fake agent supports `ask` (section 2). |
| Worker blocked | Write `block` to `task-mode` and start a task. `BLOCKED` until the fake agent supports `block` (section 2). |

## 7. Reset between tests

Each test lists its own preconditions. To start clean, quit the app, wait two minutes, delete `$HOME/agent-orch-manual-test`, and repeat sections 3 to 5.

## 8. Clean up after the run

Quit the app. Delete `$HOME/agent-orch-manual-test`, `$HOME/agent-orch-manual-test-bin`, and `$HOME/agent-orch-manual-test-project`. Keep `docs/evidence/manual-tests/<run-UTC>/`.
