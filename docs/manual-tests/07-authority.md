# 07 Authority: the orchestrator acts on its own within bounds

Setup: [00-setup.md](00-setup.md), sections 2a and 2b. Everything happens in **Orchestrator Workspace** with the **Global** target unless a step says otherwise. Turn **Let the orchestrator act on its own** back on before this file if files `01` to `04` left it off.

Sources: `docs/security.md` section 6; `docs/settings.md` "Preferences and learnings"; `docs/design/firstmate-behavior-decisions.md` "Phase 5" and "Integration" (I2 to I6).

Words used here:

- **Done on its own.** The card's pill reads "Done on its own" and, under the message, "The orchestrator did this on its own. You did not need to approve it." The card has no **Approve** or **Reject** button. It sits under the reply of the turn that issued it, which is the record of where it came from.
- **Waits for you.** The pill reads "Pending approval", the message reads "Nothing runs until you approve.", and the card has **Approve** and **Reject**.

---

## TC-A-01 The switch is on by default and can be saved

Purpose: the user's switch exists, starts on, and keeps its value.
**Source:** `docs/security.md` section 6 ("The user's switch `autonomous_actions` (Settings, on by default)").
**Preconditions:** A new data directory (setup section 7), sections 3 and 4 done, section 5 step 5 not done.

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
**Preconditions:** Switch on. `task-mode` is `hang`. `next-reply.txt` holds one block with two actions: `workstream.create` (title `Auto stream`, summary `Made on its own`, ref `a`) and the standard `task.propose` (setup section 2a).

1. Send `Start the README task and a workstream` to Global.
2. Wait for the reply. Clear `next-reply.txt`.
3. Wait five seconds. Read both cards.
4. On the task card, click **Open task**.

**Expected:**

- Step 3: both cards read "Done on its own", with "The orchestrator did this on its own. You did not need to approve it." The task card's message reads "Created and queued task “Add a README line”. Open the task to follow it." The workstream card's reads "Created workstream “Auto stream”." Neither card has **Approve** or **Reject**.
- Step 4: the task page shows the task `Running` (or `Queued` while slots are busy).
- **Workstreams** lists `Auto stream`.

**Screenshot** after step 3.
**Reset:** cancel the task from its page (**Request cancel**).

## TC-A-03 With the switch off, every action waits for you

Purpose: off falls back to proposal-only.
**Source:** `docs/security.md` section 6 ("Off makes every action a card again").
**Preconditions:** Switch off and saved. `next-reply.txt` holds the standard `task.propose`. Prompt log (setup section 2).

1. Send `Run the README task` to Global. Clear `next-reply.txt`.
2. Wait five seconds and read the card.
3. **Prompt log:** read the last `turn` entry.

**Expected:**

- Step 2: the card waits for you. No task starts until you click **Approve**.
- Step 3: the prompt contains the line "Run at once: none. Every action waits for the user's approval."

**Reset:** click **Reject**. Turn the switch on and save.

## TC-A-04 Actions that need your word still wait

Purpose: user-word actions never run on their own, even with the switch on.
**Source:** `docs/security.md` section 6 ("What is explicitly not autonomous").
**Preconditions:** Switch on. A running task (`task-mode` `hang`) and its id. The workstream `Auto stream` from TC-A-02, its id and its revision. `next-reply.txt` holds one block with six actions:

- `task.cancel` and `task.revise` (`added_intent` `Also add a heading`) for the running task;
- `decision.hold` for the running task, `until` tomorrow as `YYYY-MM-DD`;
- `workstream.update` for `Auto stream` with `state` `archived` and its `expected_revision`;
- `memory.set` with `document` `shared`, `key` `pr-size`, `text` `Keep pull requests small`;
- `project.remove` for the project code.

1. Send `Clean up` to Global. Clear `next-reply.txt`.
2. Wait five seconds. Read every card.

**Expected:**

- All six cards wait for you. None reads "Done on its own".
- The headings are "Proposed: Cancel task", "Proposed: Revise task", "Proposed: Hold decision", "Proposed: Update workstream", "Proposed: Save a shared preference" and "Proposed: Remove project".
- The running task is still `Running`. `Auto stream` is not archived. **Settings > Preferences and learnings > Shared preferences** has no `pr-size`.

**Reset:** click **Reject** on each card.
**Known limitations:** `delivery.merge` also always waits, but it needs a recorded pull request with passing checks, which the fake agent cannot make.

## TC-A-05 A pull-request task waits for you

Purpose: a task that would publish to a remote is never started on its own.
**Source:** `docs/security.md` section 6 ("a pull-request task stays a card (it publishes to a remote)").
**Preconditions:** Switch on. In **Settings > Pull requests from chat**, tick "Let the orchestrator propose tasks that end in a pull request. You still approve each task." and click **Save pull request setting**. On the project page of `manual-test-project`, under "Defaults for this project", set the delivery mode to **Pull request, run checks** (`direct-pr`) and click **Save project defaults**. Mark `BLOCKED` if the mode cannot be saved without a remote. `next-reply.txt` holds the standard `task.propose` with `delivery` `PullRequest`.

1. Send `Open a pull request for the README line` to Global. Clear `next-reply.txt`.
2. Wait five seconds. Read the card.

**Expected:** The card waits for you and shows Delivery `PullRequest`. No task starts.

**Reset:** click **Reject**. Set the mode back to **Review only** and clear the pull request setting.

## TC-A-06 An automatic turn cannot start new work, but can retry and ask

Purpose: a wake turn has no user words, so it acts only on existing work.
**Source:** `docs/security.md` section 6 ("A wake turn, or a relay of a scout report, may only retry, message, ask … a task, workstream, objective, route or learning it proposes stays a card").
**Preconditions:** Switch on. Monitoring **On**. `task-mode` is `fail`. Prompt log.

1. Put the standard `task.propose` in `next-reply.txt`. Send `Run the README task`. The task starts on its own.
2. As soon as the reply arrives, write `next-reply.txt` with one block holding a `task.retry` for that task and a second `task.propose` with objective `Wake-made task`. The wake turn waits 60 seconds after the failure, which leaves time.
3. Switch to the `manual-test-project` target if it exists, else stay. Wait up to two minutes for the failure and the wake turn.
4. Clear `next-reply.txt`. Choose **Global**. Read the wake notice and the cards under it.
5. **Prompt log:** read the `turn` entry of the wake turn.

**Expected:**

- Step 4: the retry card reads "Done on its own" and "Retry queued for “Add a README line”." The `Wake-made task` card waits for you.
- Step 5: the prompt contains a line that starts "Run at once: task.retry, decision.ask, task.message, at most 5".

**Reset:** reject the `Wake-made task` card. Write `succeed` to `task-mode`.

## TC-A-07 At most 5 actions a reply run on their own

Purpose: the per-turn bound.
**Source:** `docs/security.md` section 6 ("At most 5 actions a turn run on their own").
**Preconditions:** Switch on. `next-reply.txt` holds one block with six `memory.set` actions, `document` `learnings`, keys `learn-1` to `learn-6`, each with the text `Checks take about two minutes`.

1. Send `Record what you learned` to Global. Clear `next-reply.txt`.
2. Wait five seconds. Read the six cards.
3. Open **Settings > Preferences and learnings > Learnings**, "Learnings for" **Global**.

**Expected:**

- Step 2: five cards read "Done on its own" with "Recorded the learning “learn-N”." One card waits for you.
- Step 3: five of the six keys are listed, each "Written by the orchestrator".

**Reset:** reject the waiting card. Remove the five learnings in Settings.

## TC-A-08 A one-task departure from the project default is recorded on that task

Purpose: the user's one-off choice is kept on that task only.
**Source:** `docs/design/firstmate-behavior-decisions.md` Phase 5, items 14 and 15.
**Preconditions:** Switch on. `manual-test-project` is in its default mode, local-only, whose default delivery is local review. `task-mode` is `succeed`.

1. Put the standard `task.propose` (delivery `ReportOnly`) in `next-reply.txt`.
2. Send `Just report on the README, do not prepare a change for review` to Global. Clear `next-reply.txt`.
3. Wait five seconds. Read the card.
4. Put the same block in `next-reply.txt` but with `delivery` `LocalReview`. Send `Now the normal way`. Clear `next-reply.txt`.

**Expected:**

- Step 3: the card reads "Done on its own" and "For this task only: delivery report only instead of the project's local review." It also has the row "Below this project's default".
- Step 4: the new card has no "For this task only" line. The override was not reused.

**Known limitations:** the daemon stores your whole message as the words behind the override, but the app does not show them. Only `delivery` is recorded; `task.propose` has no base-branch or merge field.

## TC-A-09 Archive is refused for committed work that was not delivered

Purpose: finished work is never thrown away by archiving.
**Source:** `docs/security.md` section 6 ("Unlanded work"); `docs/design/firstmate-behavior-decisions.md` Phase 5, items 16 to 18.
**Preconditions:** `task-mode` is `write`. **BLOCKED until supported:** the fake agent has no `write` mode yet (`00-setup.md` section 2).

1. Start the standard Implement task with `delivery` `ReportOnly` from your message, so it starts on its own. It commits one changed file, and a report-only delivery puts that commit on no review ref and no pull request. Wait until the task is finished (`Succeeded`).
2. Open the task page. Click **Archive task**.
3. Click **Discard changes**.
4. Click **Keep changes**, then **Discard changes** again, then **Confirm discard**.
5. Click **Archive task**.

**Expected:**

- Step 2: "This task has changes that were not delivered. Deliver them, or discard them first." The task is not archived.
- Step 3: "Discard this task's changes? The branch stays in the repository, but the app will no longer keep the task's workspace for it." with **Confirm discard** and **Keep changes**.
- Step 4: "Changes discarded. You can archive the task now."
- Step 5: "Archived. The task leaves the task lists; its history is kept." The task branch still exists in the project repository (`git branch --list 'moa/*'`).

**Known limitations:** the bulk archive in the Workstreams view shows the generic conflict text, not this one. Discard is the user's own control; no action block can discard.
