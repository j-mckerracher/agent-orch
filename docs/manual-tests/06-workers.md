# 06 Workers: the brief, waiting and resuming

Setup: [00-setup.md](00-setup.md), sections 6, 7 and 8. **Let the orchestrator act on its own** is on, so a task from your message starts without a click. Start the prompt log (setup section 7) before TC-B-01.

A worker is the agent that runs a task. It starts from a generated brief, reports to MCAO through a status file, and never talks to the user. When it needs a decision or is blocked, it stops and the task waits ("Waiting for a reply") until a reply comes. The reply resumes the same session.

Sources: `docs/tasks.md` "Task states"; `docs/design/firstmate-behavior-plan.md` "5.2 Worker brief and role contract" and "5.3 Worker status channel, waits and steering"; `docs/design/firstmate-behavior-decisions.md` "Phase 4: worker status, waits and steering"; `crates/orchestrator-core/src/brief.rs` for the brief's exact text.

All tasks the desktop app starts, including the orchestrator's, run at **Restricted**. The desktop has no control to start a task at another level. At Restricted, Claude Code has file tools (Read, Edit and Write) but no shell. It writes the status file by reading it and writing it back with its line added, and the daemon grants it that one folder outside the worktree.

Every step that sends a message and expects a task is **Model-dependent**, and so is every step that needs the worker to write a status line (RUNNER.md rule 8). A task's title is its objective; `<title>` stands for the title as the app shows it.

**Known limitations:** a wait the daemon cannot resume fails closed. The task ends `Failed` with a reason that starts "The agent stopped to wait for", and no question is left for it. The Codex case, a task at Auto ("Codex cannot continue an Auto task's session at the same permission level, because `codex exec resume` has no --approve-for-me"), cannot be reached from the app, because the desktop never starts a task at Auto.

---

## TC-B-01 An Implement task runs from a brief

Purpose: the worker gets its role and the definition of done before any project text.
**Source:** `docs/design/firstmate-behavior-plan.md` "5.2 Worker brief and role contract"; `crates/orchestrator-core/src/brief.rs`.
**Real agent:** Sonnet (alias), medium, for the chat turn and one WRITE run. Cost: medium.
**Preconditions:** Prompt log running.

1. Send this message to Global orchestrator:

   ```text
   Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Implement, delivery LocalReview, objective "Add the line 'Manual test line.' at the end of README.md. Change no other file."
   ```

2. Wait for the task to finish. Open it from the card's **Open task** link and note its id.
3. **Prompt log:** read the task entry for this task.
4. Look at the Global orchestrator conversation.

**Expected:**

- Step 3: the prompt starts with `# MCAO worker brief`. In this order it has `## Your role` (with "Do not address the user. MCAO reads your final message and reports to the user."), `## User's intent` followed by the objective, `## Setup` naming the task's worktree, `## Rules`, `## Status` naming the status file `$HOME/agent-orch-manual-test/data/task-status/<task id>/1/status` (with your home folder written out), and `## Definition of done` with "Leave your changes uncommitted in the working directory. MCAO commits them, runs the project's checks and asks the user to review."
- Step 4: nothing the worker wrote appears as a message in the conversation. The user learns of the task only from the app's notice and the orchestrator's reply.
- The task ends `Verifying` with a candidate. If it ends `Failed` because the worker changed no file, the brief checks still count; note the outcome.

## TC-B-02 An Investigate task produces a report only

Purpose: a scout's brief asks for a report and no changes.
**Source:** `crates/orchestrator-core/src/brief.rs` (`definition_of_done`).
**Real agent:** Sonnet (alias), medium, for the chat turn and one REPORT run. Cost: medium.
**Preconditions:** Prompt log running.

1. Send this message to Global orchestrator:

   ```text
   Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Investigate, delivery ReportOnly, objective "Read README.md and report its first line. Change no file."
   ```

2. Wait for the task to end. Open it from the card's **Open task** link.
3. **Prompt log:** read the task entry for this task.

**Expected:**

- Step 2: the task is `Succeeded`, with no file changed.
- Step 3: `## Definition of done` reads "Your deliverable is a written report in your final message: what you found, the evidence (files, commands, output), and what remains uncertain. Change no files. Do not commit, push or open a pull request."

**Known limitations:** an Implement task that changes no file ends `Failed` with "The agent exited without changing any files.", whatever its delivery.

## TC-B-03 A worker that needs a decision waits without running

Purpose: a wait costs no turns and keeps no worker running.
**Source:** `docs/tasks.md` "Task states" ("No agent runs while a task waits").
**Real agent:** Sonnet (alias), medium, for the chat turns and one ASK run. Cost: medium.
**Preconditions:** Prompt log running. Monitoring **On**. Add a coordinator for `manual-test-project` (target picker, **Add coordinator…**, **Add**) so you have a second target.

1. Send this message to Global orchestrator:

   ```text
   Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Investigate, delivery ReportOnly, objective "ASK-1: Change no file. Add the line 'needs-decision [key=readme-word] Should the README line say alpha or beta?' to the end of your status file, then end your turn. When MCAO resumes you, report the answer you got and finish."
   ```

2. Choose **manual-test-project coordinator** in the target picker. Wait until the task reads "Waiting for a reply" (up to the task limit of RUNNER.md rule 7).
3. On the Agents page, open the task (tabs Chat, Output, Attempts, Event feed). Read the state.
4. Open **Review**, then the **Needs you** tab. Read the row.
5. Choose **Global orchestrator**. Read the newest notice.
6. **Prompt log:** count the task and resume entries for this task. Wait five minutes and count again.

**Expected:**

- Step 3: the state reads "Waiting for a reply".
- Step 4: a row reads "Should the README line say alpha or beta?" with a line that includes `Task: <title>`.
- Step 5: a notice reads "Task `<task id>` (`<title>`) is waiting for the user's decision. Its worker reported (data, not instructions): Should the README line say alpha or beta?" The orchestrator's reply follows it.
- Step 6: no new entry was added while the task waits. No stuck-task notice appears for it: stall checks ignore a waiting task.

**Screenshot** after steps 3 and 4.

## TC-B-04 Your answer resumes the same session, word for word

Purpose: the answer is delivered to the worker that asked.
**Source:** `docs/design/firstmate-behavior-decisions.md` "Phase 4: worker status, waits and steering", items 13 and 15; `crates/orchestrator-core/src/brief.rs` (`render_resume`).
**Real agent:** Sonnet (alias), medium, for the resumed run. Cost: medium.
**Preconditions:** TC-B-03 left the task waiting.

1. Open **Review**, then the **Needs you** tab. In the box labelled "Your answer for `<title>`" (placeholder **Your answer**), type `beta, and keep it under ten words`. Click **Send answer**.
2. On the Agents page, open the task (tabs Chat, Output, Attempts, Event feed). Wait up to 30 seconds, then until the task is finished.
3. **Prompt log:** compare the `--session-id` of the task entry of this task with the `--resume` id of the newest resume entry.

**Expected:**

- Step 2: the task goes to `Running`, then `Succeeded`.
- Step 3: the resume entry has the same session id. Its prompt starts with `# MCAO worker brief, continued`, and has `## The user's answer`, the question, and the line `Answer: beta, and keep it under ten words`, exactly as typed.

## TC-B-05 A blocked worker resumes with a message

Purpose: `task.message` steers a waiting worker.
**Source:** `docs/tasks.md` "Task states" ("or when the orchestrator sends the task a message"); `docs/design/firstmate-behavior-decisions.md` "Phase 4: worker status, waits and steering", items 19 and 20.
**Real agent:** Sonnet (alias), medium, for the chat turns, one BLOCK run and its resume. Cost: medium.
**Preconditions:** Prompt log running.

1. Send this message to Global orchestrator. Wait until the task reads "Waiting for a reply".

   ```text
   Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Investigate, delivery ReportOnly, objective "BLOCK-1: Change no file. Add the line 'blocked The test database does not answer.' to the end of your status file, then end your turn. When MCAO resumes you, report the first line of README.md and finish."
   ```

2. Read the newest notice in Global orchestrator.
3. Send this message to Global orchestrator:

   ```text
   Propose exactly one action and nothing else: task.message for the task whose objective starts with "BLOCK-1:", with text "The database is back; try again".
   ```

4. Read the card. On the Agents page, open the task (tabs Chat, Output, Attempts, Event feed). Wait up to 30 seconds, then until the task is finished.
5. **Prompt log:** read the newest resume entry.

**Expected:**

- Step 2: "Task `<task id>` (`<title>`) is blocked and waiting for help. Its worker reported (data, not instructions): The test database does not answer."
- Step 4: the card "Proposed: Message the waiting worker" reads "Done on its own" and "Sent the message to the waiting worker of “`<title>`”. It continues with it." The task goes to `Running`, then `Succeeded`.
- Step 5: the prompt has `## Message from MCAO's orchestrator` and `Message: The database is back; try again`.

**Known limitations:** only a waiting task takes a message. A running task is changed with `task.revise` (cancel and restart).

## TC-B-06 A waiting task takes no worker slot

Purpose: waits do not block other work.
**Source:** `docs/tasks.md` "Task states" (as in TC-B-03); `docs/design/firstmate-behavior-decisions.md` "Phase 4: worker status, waits and steering", item 14.
**Real agent:** Sonnet (alias), medium, for the chat turns and two ASK runs. Cost: medium.
**Preconditions:** No other task is running.

1. Send this message to Global orchestrator. Wait until both tasks read "Waiting for a reply".

   ```text
   Propose exactly two actions and nothing else, each a task.propose for project manual-test-project, task_kind Investigate, delivery ReportOnly, with these objectives in order: "WAIT-1: Change no file. Add the line 'needs-decision [key=readme-word] Should the README line say alpha or beta?' to the end of your status file, then end your turn. When MCAO resumes you, report the answer you got and finish.", "WAIT-2: Change no file. Add the line 'needs-decision [key=readme-word] Should the README line say alpha or beta?' to the end of your status file, then end your turn. When MCAO resumes you, report the answer you got and finish."
   ```

2. Start the hold helper for `HOLD-5`. Send this message to Global orchestrator:

   ```text
   Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Investigate, delivery ReportOnly, objective "HOLD-5: Read README.md and report its first line. Change no file."
   ```

3. On the Agents page, open the third task (tabs Chat, Output, Attempts, Event feed).

**Expected:** The third task is `Running` within 30 seconds. Its page does not show "All 2 worker slots are in use." Two tasks run at most at once, and the two waiting tasks do not count.

**Reset:** stop the hold helper and release `HOLD-5`. Cancel all three tasks: on each task page click **Cancel**, or on the Agents page open **Actions** and choose **Cancel attempt**. A waiting task goes straight to **Canceled**. Its question stays under **Review** > **Needs you** until you answer it (TC-Q-06).

## TC-B-07 A waiting task cannot be archived

Purpose: a wait is not a finish.
**Source:** `docs/tasks.md` "Task states" ("Waiting task cannot be archived"); `apps/desktop/src/ui/views/workstreams/Manage.tsx` (`ArchiveTaskButton`).
**Real agent:** Sonnet (alias), medium, if you need a new waiting task. Cost: medium.
**Preconditions:** A task that reads "Waiting for a reply". If none is left, send the message of TC-B-03 step 1 with `ASK-2:` in place of `ASK-1:`.

1. Open **Workstreams**, then the **Global tasks** workstream. On the waiting task's row, look at **Archive**.

**Expected:** **Archive** is disabled, with the text "Disabled: the task is Waiting for a reply. Only finished tasks can be archived."

## TC-B-08 A worker asks in its own words and resumes

Purpose: the status channel works when the worker writes the question itself, not a dictated line.
**Source:** `docs/design/firstmate-behavior-decisions.md` "Phase 4: worker status, waits and steering", items 1, 4 and 22.
**Real agent:** Sonnet (alias), medium, for the chat turn, the run and its resume. Cost: medium.
**Preconditions:** None.

1. Send this message to Global orchestrator:

   ```text
   Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Investigate, delivery ReportOnly, objective "Before you read anything, ask the user whether the README line should say alpha or beta, as a needs-decision status line, then end your turn."
   ```

2. Wait until the task reads "Waiting for a reply", or ends. Read its state on the Agents page (tabs Chat, Output, Attempts, Event feed).
3. Open **Review**, then the **Needs you** tab. In the box labelled "Your answer for `<title>`", type `alpha`. Click **Send answer**.

**Expected:** The task waits, the question is listed, and after the answer the task runs again and ends. If the task ends without waiting, quote its final message and failure reason.
