# Setup

Every test file starts from this setup. The runner follows [RUNNER.md](RUNNER.md), which owns the run rules: real app, real agents, fixed waits, retries and evidence. All paths are relative to the repository root, except the scratch folder under `$HOME`.

The tests run on the real app built from the tip of `main`, with real agents: Claude Code, model "Sonnet (alias)" at effort "medium", for chat and for tasks. There is no fake agent. Real agents cost money; each test file states its cost class.

## 1. Build

Run from the repository root, at the commit you test (`git rev-parse HEAD`). Rust 1.96.0 and Node.js 22.20.0 are required ([build-from-source.md](../build-from-source.md)).

```sh
cargo build --workspace
npm --prefix apps/desktop install
npm --prefix apps/desktop run build
cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml
```

## 2. Model probe

The app offers Claude Code models as aliases. This one call shows which model the alias `sonnet` resolves to today. It costs one short turn.

```sh
claude -p --model sonnet --effort medium --output-format json 'Reply with exactly: pong' \
  | python3 -c 'import json, sys; print(*[m for m in json.load(sys.stdin)["modelUsage"] if "sonnet" in m])'
```

Expected: it prints exactly one id, the expected model id from the runner prompt, `claude-sonnet-5-5` today. The CLI may also use a small helper model; the command filters it out. If it prints another Sonnet id, nothing, or an error, stop the run.

## 3. Folders and fixture

Everything a run creates outside the repository goes under one folder, `$HOME/agent-orch-manual-test`. It stays outside the repository so agents working in the test projects do not read this repository's instruction files.

| Path | Holds |
| --- | --- |
| `$HOME/agent-orch-manual-test/data` | The daemon data directory |
| `$HOME/agent-orch-manual-test/projects/manual-test-project` | The main test project |
| `$HOME/agent-orch-manual-test/projects/other-project` | A second project, for TC-C-04 |
| `$HOME/agent-orch-manual-test/logs` | The prompt log (section 7) |

Create both projects with the fixture command. It deletes and recreates them with the same file and the same commit every time.

```sh
sh docs/manual-tests/manual-test.sh fixture
```

Expected output, exactly:

```text
manual-test-project fc60d42dcc1b1a6ebd5a6315f5f11620107e45d0
other-project 02061f1dff348d6017c6d098178e7c6194ba4203
```

Each project holds one file, `README.md`, whose first line is `# manual-test-project` (or `# other-project`).

## 4. Start the app

```sh
AGENT_ORCH_DAEMON_BIN="$PWD/target/debug/orchestrator-daemon" \
AGENT_ORCH_DATA_DIR="$HOME/agent-orch-manual-test/data" \
AGENT_ORCH_CONVERSATION_AGENT=claude-code \
AGENT_ORCH_CONVERSATION_MODEL=sonnet \
AGENT_ORCH_CONVERSATION_EFFORT=medium \
npm --prefix apps/desktop run tauri dev
```

The daemon inherits the app environment and exits about a minute after it goes idle. To change an environment variable, quit the app, wait two minutes, check that `pgrep -fl orchestrator-daemon` prints nothing, and start again ([configuration.md](../configuration.md)). A daemon with a running task is not idle and does not exit.

Expected: the app window opens. The sidebar has Workspace, Workstreams, Agents, Review, and Settings. No page shows "Fixture data".

## 5. Register the projects

1. In the app, open **Settings**, then its **Repositories** page. Click **Add repository**.
2. In the dialog, type `$HOME/agent-orch-manual-test/projects/manual-test-project` in **Repository path** and click **Register repository**. The project takes its name from the folder, `manual-test-project`. There is no name field.

Expected: "Repository registered." appears, and `manual-test-project` is listed. Register `other-project` the same way only when a test asks for it.

## 6. Settings, in this order

Apply these at the start of every test file, in this order, and take the named screenshots.

1. Open **Settings**, then the **Agents** page. In **Defaults for new tasks**, choose `Claude Code` as **Default provider**, `Sonnet (alias)` as **Default model** and `medium` as **Default effort**. Click **Save changes**. The result reads "Saved. New tasks use these defaults." **Screenshot** `SETUP-<file number>-1`.
2. Open **Repositories**. In **Home directory**, enter `~/agent-orch-manual-test/projects/manual-test-project` and click **Save changes**.
3. Open **Limits**. In **Automatic replies**, check the defaults and change nothing: **Replies per hour, per conversation** is `20`, **Replies per day, per conversation** is `160`, and **Daily token budget for automatic replies** is empty.
4. On the same page, in **Checks on running work**, change nothing. Record the three values in `ENVIRONMENT.md`: **Check interval with one task running (minutes)**, **Fastest check interval (seconds)** and **Longest time a task may run before it is flagged (minutes)**.
5. Open **Autonomy**. For files `01` to `04`, clear the switch "Let the orchestrator act on its own". It saves on click, and the result reads "Saved.". For files `05` to `12`, leave the switch on. **Screenshot** `SETUP-<file number>-2`.
6. Open **Workspace** and choose **Global orchestrator** in the target picker. Open the **Model settings** chip. Choose `Claude Code` as **Provider**, `Sonnet (alias)` as **Model** and `medium` as **Effort**. Leave the permission level at its default, Auto. The Monitoring chip reads `Monitoring: on`. **Screenshot** `SETUP-<file number>-3`.

The app remembers the Model settings choice for every conversation target. If a target shows another choice, set it back before you send.

## 7. Prompt log

Some checks read what the daemon sent to an agent, which the app does not show. They are marked **Prompt log**.

Start the capture in its own terminal before the first such check of a file, and stop it with Ctrl-C at the end of the run:

```sh
sh docs/manual-tests/manual-test.sh capture
```

It copies the command line of every agent run the daemon starts into `$HOME/agent-orch-manual-test/logs/prompts.log`. It reads process arguments only. Each entry starts with `=== <UTC time> pid <pid> ===`. Line breaks in a prompt show as `\012`, so match the quoted text, not the start of a line.

- **Turn.** A chat or automatic turn. Its command line has `--no-session-persistence`.
- **Task.** A new task run. It has `--session-id <uuid>`, and its prompt starts with `# MCAO worker brief`.
- **Resume.** A resumed task run. It has `--resume <uuid>`, and its prompt starts with `# MCAO worker brief, continued`.

A task run's prompt is also in Claude Code's own session record. Find it with `grep -l '<task id>' ~/.claude/projects/*/*.jsonl`. If neither source has the entry, the check is `BLOCKED` with "prompt capture found no command line".

## 8. How to cause each situation with a real agent

### 8a. Prompts that make the orchestrator act

Each test gives the exact message to send. The messages name the action and every field, so the orchestrator has nothing to choose. The orchestrator reads project codes from its own Projects list, so a message can name `manual-test-project`. A step that needs the orchestrator to propose an action is **Model-dependent** (RUNNER.md rule 8).

With **Let the orchestrator act on its own** on, the daemon runs the actions it may run by itself just after the turn, at most 5 per reply, and their cards read "Done on its own". A turn the app started (a wake or a report relay) may only retry, message, ask, or (a coordinator) report and start tasks for a request routed to it. Everything else waits for **Approve** (`07-authority.md`). With the switch off, every action waits for **Approve** unless a standing approval covers it. A card can show "Pending approval" for a moment before it reads "Done on its own"; wait the 30 seconds of RUNNER.md rule 7.

### 8b. Worker objectives

A task's objective is the worker's instruction. These objectives make the worker end the same way every time. Tests use them word for word inside their messages. A test may put a short label in front, such as `Q-TASK:`, so a later message can name the task by the start of its objective.

| Name | Kind | Delivery | Objective | Outcome |
| --- | --- | --- | --- | --- |
| WRITE | Implement | LocalReview | `Add the line 'Manual test line.' at the end of README.md. Change no other file.` | A candidate with one changed file. The task is `Verifying` and waits for review. |
| NO-CHANGE | Implement | ReportOnly | `Read README.md. Do not create, change or delete any file.` | `Failed` with "The agent exited without changing any files." The daemon retries once; the retry fails the same way. |
| REPORT | Investigate | ReportOnly | `Read README.md and report its first line. Change no file.` | `Succeeded` with a report, no file changed. |
| ASK | Investigate | ReportOnly | `Change no file. Add the line 'needs-decision [key=readme-word] Should the README line say alpha or beta?' to the end of your status file, then end your turn. When MCAO resumes you, report the answer you got and finish.` | `Waiting for a reply`, with the question under **Needs your decision**. After the answer, the resumed run ends `Succeeded`. |
| BLOCK | Investigate | ReportOnly | `Change no file. Add the line 'blocked The test database does not answer.' to the end of your status file, then end your turn. When MCAO resumes you, report the first line of README.md and finish.` | `Waiting for a reply`, blocked. After a message, the resumed run ends `Succeeded`. |
| HOLD | Investigate | ReportOnly | `HOLD-<n>: Read README.md and report its first line. Change no file.` | With the hold helper (8c), `Running` until you release it. |

The worker writes the status file through its file tools; the daemon grants it that one folder outside the worktree. A run that ends without the expected status line is a model miss (RUNNER.md rule 8), not an app failure.

### 8c. Deterministic triggers

| Situation | How |
| --- | --- |
| A task that keeps running | Before you send the message, start `sh docs/manual-tests/manual-test.sh hold HOLD-<n>` in its own terminal. It pauses the task run whose objective starts with `HOLD-<n>:` as soon as it starts, so the task stays `Running` and writes nothing. When the task page reads `Running`, stop the helper with Ctrl-C; the run stays paused. |
| End a held task | Run `sh docs/manual-tests/manual-test.sh release HOLD-<n>`, then click **Cancel** on the task page (open it from the card's **Open task**). Always release before you cancel: a paused process does not act on a cancel. |
| A stalled task | Hold a task (above) and wait. The stall check waits the check interval, and never less than 5 minutes. At the default base of 10 minutes, a task with no output for about 10 minutes is stalled, and the daemon posts a notice that reads "<task> has been silent for N minutes." A held task also counts against its wall-time limit (Settings > Limits, **Checks on running work**, **Longest time a task may run before it is flagged (minutes)**). |
| An unavailable agent | Quit the app and wait two minutes. Start it as in section 4 with `AGENT_ORCH_CLAUDE_PROGRAM="$HOME/agent-orch-manual-test/missing/claude"` added. Restart without it afterwards. |
| Failing automatic turns | No trigger. The variables that make chat turns fail (`AGENT_ORCH_CONVERSATION_TIMEOUT_MS`, a missing program) take effect only after a daemon restart, need no running task, and also fail your own messages, so the orchestrator cannot start the tasks whose events would wake it. Tests that need this are `BLOCKED`. |
| A quiet reply to a failure or a report | No trigger. A real model answers quietly only when nothing needs the user; it does not answer a failure or a report quietly on request. Tests that need this are `BLOCKED`. |

### 8d. Wake events

| Event | How |
| --- | --- |
| Failed task | A NO-CHANGE task (8b). The wake comes after the retry also fails. |
| Stalled task | A held task (8c), then about 10 minutes at the default base. |
| Empty change set fails | A NO-CHANGE task. |
| Candidate ready | A WRITE task. |
| Task delivery waiting | A WRITE task; its LocalReview delivery waits for review. |
| Scout report ready | A REPORT task. |
| Worker needs a decision | An ASK task. |
| Worker blocked | A BLOCK task. |
| Failing agent | No trigger (8c). |

## 9. Reset between test files

Every test file starts clean, including file 01:

1. Release any held runs: `sh docs/manual-tests/manual-test.sh release HOLD-`.
2. Quit the app. Wait two minutes. `pgrep -fl orchestrator-daemon` prints nothing.
3. Delete the data directory: `rm -rf "$HOME/agent-orch-manual-test/data"`.
4. Run the fixture command (section 3). It prints the same two SHAs.
5. Do sections 4, 5 and 6.

Inside a file, a test's own **Reset** line says what to undo before the next test.

## 10. Clean up after the run

1. Stop the prompt capture with Ctrl-C. Release any held runs (section 9, step 1).
2. Quit the app. Wait two minutes.
3. Delete `$HOME/agent-orch-manual-test` and, if file 11 left it, `$HOME/agent-orch-manual-test-upgrade` and the `../mcao-before` worktree.
4. Delete the Claude Code session records of the test tasks: `rm -rf ~/.claude/projects/*agent-orch-manual-test*`.
5. Keep `docs/evidence/manual-tests/<run-UTC>/`.
