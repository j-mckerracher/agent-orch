# 07 Authority: the orchestrator acts on its own within bounds

Setup: [00-setup.md](00-setup.md), sections 6, 7 and 8. Everything happens in **Orchestrator Workspace** with the **Global** target unless a step says otherwise. **Let the orchestrator act on its own** is on (setup section 6, step 5).

Sources: `docs/security.md` section 6; `docs/settings.md` "Preferences and learnings"; `docs/design/firstmate-behavior-decisions.md` "Phase 5" and "Integration" (I2 to I6).

Words used here:

- **Done on its own.** The card's pill reads "Done on its own" and, under the message, "The orchestrator did this on its own. You did not need to approve it." The card has no **Approve** or **Reject** button. It sits under the reply of the turn that issued it, which is the record of where it came from.
- **Waits for you.** The pill reads "Pending approval", the message reads "Nothing runs until you approve.", and the card has **Approve** and **Reject**.

**Standard task message** (NO-CHANGE, setup section 8b; the task ends `Failed`, which does not fail these tests):

```text
Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Implement, delivery ReportOnly, objective "Read README.md. Do not create, change or delete any file."
```

Every step that sends a message and expects a card is **Model-dependent** (RUNNER.md rule 8). A task's title is its objective; `<title>` stands for the title as the app shows it.

---

## TC-A-01 The switch is on by default and can be saved

Purpose: the user's switch exists, starts on, and keeps its value.
**Source:** `docs/security.md` section 6 ("The user's switch `autonomous_actions` (Settings, on by default)").
**Real agent:** none. Cost: low.
**Preconditions:** A new data directory (setup section 9), with setup section 6 done; step 5 left the switch on.

1. Open **Settings**. Find the group **Acting on its own**.
2. Read the switch and the text above it.
3. Clear the switch. Click **Save acting on its own**.
4. Quit the app, wait two minutes, start it again, and open **Settings**.

**Expected:**

- Step 2: the switch "Let the orchestrator act on its own" is on. The text starts "When this is on, the orchestrator starts tasks, retries them, sends messages to waiting tasks" and contains "Cancelling or revising a task, merging, removing a project, holding a decision and saving a preference are never done on its own."
- Step 3: the result reads "Saved.".
- Step 4: the switch is off.

**Reset:** turn the switch on and click **Save acting on its own**.

## TC-A-02 A task and a workstream from your message run at once

Purpose: autonomous actions run without a click and say so.
**Source:** `docs/security.md` section 6 ("What runs on its own"; "The card shows 'Done on its own'").
**Real agent:** Sonnet (alias), medium. Cost: low; the held task does no work.
**Preconditions:** Switch on. Start the hold helper for `HOLD-1` (setup section 8c).

1. Send this message to Global:

   ```text
   Propose exactly two actions and nothing else, in this order: workstream.create with title "Auto stream", summary "Made on its own" and ref "a"; then task.propose for project manual-test-project, task_kind Investigate, delivery ReportOnly, objective "HOLD-1: Read README.md and report its first line. Change no file."
   ```

2. Wait for the reply.
3. Wait 30 seconds. Read both cards. Stop the hold helper once the task reads `Running`.
4. On the task card, click **Open task**.

**Expected:**

- Step 3: both cards read "Done on its own", with "The orchestrator did this on its own. You did not need to approve it." The task card's message reads "Created and queued task “`<title>`”. Open the task to follow it." The workstream card's reads "Created workstream “Auto stream”." Neither card has **Approve** or **Reject**.
- Step 4: the task page shows the task `Running` (or `Queued` while slots are busy).
- **Workstreams** lists `Auto stream`.

**Screenshot** after step 3.
**Reset:** release `HOLD-1` and cancel the task from its page (**Request cancel**). Keep `Auto stream` for TC-A-04.

## TC-A-03 With the switch off, every action waits for you

Purpose: off falls back to proposal-only.
**Source:** `docs/security.md` section 6 ("Off makes every action a card again").
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** Switch off and saved. Prompt log running.

1. Send the standard task message to Global.
2. Wait 30 seconds and read the card.
3. **Prompt log:** read the turn entry of this turn.

**Expected:**

- Step 2: the card waits for you. No task starts until you click **Approve**.
- Step 3: the prompt contains the text "Run at once: none. Every action waits for the user's approval."

**Reset:** click **Reject**. Turn the switch on and save.

## TC-A-04 Actions that need your word still wait

Purpose: user-word actions never run on their own, even with the switch on.
**Source:** `docs/security.md` section 6 ("What is explicitly not autonomous").
**Real agent:** Sonnet (alias), medium. Cost: low; the held task does no work.
**Preconditions:** Switch on. The workstream `Auto stream` from TC-A-02. A held running task: start the hold helper for `HOLD-2`, send the message below, and stop the helper when the task reads `Running`.

```text
Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Investigate, delivery ReportOnly, objective "HOLD-2: Read README.md and report its first line. Change no file."
```

1. Send this message to Global. Replace `<tomorrow>` with tomorrow's date as `YYYY-MM-DD`.

   ```text
   Propose exactly six actions and nothing else, in this order: task.cancel for the task whose objective starts with "HOLD-2:"; task.revise for that task with added_intent "Also add a heading"; decision.hold for that task until <tomorrow>; workstream.update for the workstream "Auto stream" with state archived and its current expected_revision; memory.set with document shared, key "pr-size" and text "Keep pull requests small"; project.remove for project manual-test-project.
   ```

2. Wait 30 seconds. Read every card.

**Expected:**

- All six cards wait for you. None reads "Done on its own".
- The headings are "Proposed: Cancel task", "Proposed: Revise task", "Proposed: Hold decision", "Proposed: Update workstream", "Proposed: Save a shared preference" and "Proposed: Remove project".
- The running task is still `Running`. `Auto stream` is not archived. **Settings > Preferences and learnings > Shared preferences** has no `pr-size`.

**Reset:** click **Reject** on each card. Release `HOLD-2` and click **Request cancel** on its task page.
**Known limitations:** `delivery.merge` also always waits, but it needs a recorded pull request with passing checks, which the fixture project cannot make (it has no remote).

## TC-A-05 A pull-request task waits for you

Purpose: a task that would publish to a remote is never started on its own.
**Source:** `docs/security.md` section 6 ("a pull-request task stays a card (it publishes to a remote)").
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** Switch on. In **Settings > Pull requests from chat**, tick "Let the orchestrator propose tasks that end in a pull request. You still approve each task." and click **Save pull request setting**. On the project page of `manual-test-project`, under "Defaults for this project", set the delivery mode to **Pull request, run checks** (`direct-pr`) and click **Save project defaults**. Mark `BLOCKED` if the mode cannot be saved without a remote.

1. Send this message to Global:

   ```text
   Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Implement, delivery PullRequest, objective "Read README.md. Do not create, change or delete any file."
   ```

2. Wait 30 seconds. Read the card.

**Expected:** The card waits for you and shows Delivery `PullRequest`. No task starts.

**Reset:** click **Reject**. Set the mode back to **Review only** and clear the pull request setting.

## TC-A-06 An automatic turn cannot start new work, but can retry and ask

Purpose: a wake turn has no user words, so it acts only on existing work.
**Source:** `docs/security.md` section 6 ("A wake turn, or a relay of a scout report, may only retry, message, ask … a task, workstream, objective, route or learning it proposes stays a card").
**Real agent:** Sonnet (alias), medium, for the chat turns and up to four NO-CHANGE runs. Cost: medium.
**Preconditions:** Switch on. Monitoring **On**. Prompt log running. A wake turn reads the shared preferences (TC-P-06), so a preference tells it what to do. In **Settings > Preferences and learnings > Shared preferences**, add key `failure-handling` with text `When a task fails, retry it once and also propose a new task with the objective Wake-made task` and click **Add shared preference**.

1. Send the standard task message. The task starts on its own.
2. Choose another target if one exists, else stay. Wait until the task has failed, then up to 5 minutes for the wake turn.
3. Choose **Global**. Read the wake notice and the cards under it.
4. **Prompt log:** read the turn entry of the wake turn.

**Expected:**

- Step 3: the retry card reads "Done on its own" and "Retry queued for “`<title>`”." The `Wake-made task` card waits for you.
- Step 4: the prompt contains text that starts "Run at once: task.retry, decision.ask, task.message, at most 5".

**Model-dependent:** the wake turn's model chooses whether to follow the preference. If it proposes neither card, count a miss.

**Reset:** remove the `failure-handling` preference at once, so later failures are not retried. Reject the `Wake-made task` card. Cancel the retried task if it still runs.

## TC-A-07 At most 5 actions a reply run on their own

Purpose: the per-turn bound.
**Source:** `docs/security.md` section 6 ("At most 5 actions a turn run on their own").
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** Switch on.

1. Send this message to Global:

   ```text
   Propose exactly six actions and nothing else: six memory.set actions with document learnings, keys "learn-1", "learn-2", "learn-3", "learn-4", "learn-5" and "learn-6", each with the text "Checks take about two minutes".
   ```

2. Wait 30 seconds. Read the six cards.
3. Open **Settings > Preferences and learnings > Learnings**, "Learnings for" **Global**.

**Expected:**

- Step 2: five cards read "Done on its own" with "Recorded the learning “learn-N”." One card waits for you.
- Step 3: five of the six keys are listed, each "Written by the orchestrator".

**Reset:** reject the waiting card. Remove the five learnings in Settings.

## TC-A-08 A one-task departure from the project default is recorded on that task

Purpose: the user's one-off choice is kept on that task only.
**Source:** `docs/design/firstmate-behavior-decisions.md` Phase 5, items 14 and 15.
**Real agent:** Sonnet (alias), medium, for the chat turns and two NO-CHANGE runs. Cost: medium.
**Preconditions:** Switch on. `manual-test-project` is in its default mode, local-only, whose default delivery is local review.

1. Send this message to Global:

   ```text
   Just report on the README, do not prepare a change for review. Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Implement, delivery ReportOnly, objective "Read README.md. Do not create, change or delete any file."
   ```

2. Wait 30 seconds. Read the card.
3. Send this message to Global:

   ```text
   Now the normal way. Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Implement, delivery LocalReview, objective "Read README.md. Do not create, change or delete any file."
   ```

**Expected:**

- Step 2: the card reads "Done on its own" and "For this task only: delivery report only instead of the project's local review." It also has the row "Below this project's default".
- Step 3: the new card has no "For this task only" line. The override was not reused.

**Known limitations:** the daemon stores your whole message as the words behind the override, but the app does not show them. Only `delivery` is recorded; `task.propose` has no base-branch or merge field.

## TC-A-09 Archive is refused for committed work that was not delivered

Purpose: finished work is never thrown away by archiving.
**Source:** `docs/security.md` section 6 ("Unlanded work"); `docs/design/firstmate-behavior-decisions.md` Phase 5, items 16 to 18.
**Real agent:** Sonnet (alias), medium, for the chat turn and one run that changes a file. Cost: medium.
**Preconditions:** Switch on.

1. Send this message to Global. The task starts on its own. It commits one changed file, and a report-only delivery puts that commit on no review ref and no pull request. Wait until the task is finished (`Succeeded`).

   ```text
   Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Implement, delivery ReportOnly, objective "Add the line 'Manual test line.' at the end of README.md. Change no other file. This change is wanted even though the delivery is report only."
   ```

2. Open the task page. Click **Archive task**.
3. Click **Discard changes**.
4. Click **Keep changes**, then **Discard changes** again, then **Confirm discard**.
5. Click **Archive task**.

**Expected:**

- Step 2: "This task has changes that were not delivered. Deliver them, or discard them first." The task is not archived.
- Step 3: "Discard this task's changes? The branch stays in the repository, but the app will no longer keep the task's workspace for it." with **Confirm discard** and **Keep changes**.
- Step 4: "Changes discarded. You can archive the task now."
- Step 5: "Archived. The task leaves the task lists; its history is kept." The task branch still exists in the project repository (`git -C "$HOME/agent-orch-manual-test/projects/manual-test-project" branch --list 'moa/*'`).

**Model-dependent:** the report-only brief tells the worker to change no files. If the worker obeys the brief and the task ends `Failed` with no change, count a miss.

**Known limitations:** the bulk archive in the Workstreams view shows the generic conflict text, not this one. Discard is the user's own control; no action block can discard.
