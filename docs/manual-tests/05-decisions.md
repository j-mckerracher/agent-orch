# 05 Decisions: questions for the user are held until the user answers

Setup: [00-setup.md](00-setup.md), sections 6 and 8. **Let the orchestrator act on its own** is on.

A question is raised by a `decision.ask` action (this file) or by a worker that stops to ask (`06-workers.md`). It is listed in the **Needs you** tab of the **Review** page. Each row shows the question, the project and "Task: <title>", one button per option, the box "Your answer" with **Send answer**, and a **Defer** disclosure with the "Defer until" date box. Deferring moves the row to the **Held** tab, with the pill "Held until <date>".

Sources: `docs/tasks.md` "Task states"; `docs/design/firstmate-behavior-decisions.md` "Phase 3: user decisions (DPES-1108, branch `feat/DPES-1108_user-decisions`)".

**Messages used here.** Send them to Global word for word. A task's title is its objective; below, `<title>` stands for the title as the app shows it.

- **Question task message.** It starts a REPORT task (setup section 8b) on its own, for the question to be about:

  ```text
  Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Investigate, delivery ReportOnly, objective "Q-TASK: Read README.md and report its first line. Change no file."
  ```

- **Question message:**

  ```text
  Propose exactly one action and nothing else: decision.ask for the task whose objective starts with "Q-TASK:", question "Should the README line say alpha or beta?", options ["alpha", "beta"], key "readme-word".
  ```

Every step that sends a message and expects a card is **Model-dependent** (RUNNER.md rule 8).

---

## TC-Q-01 A question appears in the Needs you tab

Purpose: a question reaches the user as a record, not as chat text.
**Source:** `docs/design/firstmate-behavior-decisions.md` "Phase 3: user decisions (DPES-1108, branch `feat/DPES-1108_user-decisions`)", item 16 ("Needs your decision" reads only `decisions.list`).
**Real agent:** Sonnet (alias), medium, for the chat turns and one scout run. Cost: medium.
**Preconditions:** Send the question task message. Its card reads "Done on its own". Wait until the task is finished.

1. Send the question message to Global.
2. Wait 30 seconds. Read the card.
3. Open **Review** from the sidebar and read the **Needs you** tab.

**Expected:**

- Step 2: the card "Proposed: Ask for your decision" reads "Done on its own" and "Asked you about “`<title>`”. Answer it under Needs your decision in Review & Decisions." Its fields show the question and the options `alpha` and `beta`.
- Step 3: one row reads "Should the README line say alpha or beta?" with "manual-test-project · Task: `<title>`", and the buttons `alpha` and `beta`.

**Screenshot** after step 3.

## TC-Q-02 Asking again with the same key does not add a second question

Purpose: one open question per task and key.
**Source:** `docs/design/firstmate-behavior-decisions.md` "Phase 3: user decisions (DPES-1108, branch `feat/DPES-1108_user-decisions`)", item 6.
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** TC-Q-01 left its question open.

1. Send this message to Global:

   ```text
   Propose exactly one action and nothing else: decision.ask for the task whose objective starts with "Q-TASK:", question "Alpha or beta for the README?", options ["alpha", "beta"], key "readme-word".
   ```

2. Read the **Needs you** tab.

**Expected:** Still one row for that task, with the first wording.

## TC-Q-03 An answer is sent exactly as typed

Purpose: the user's words are the answer; an option only fills the box.
**Source:** `docs/design/firstmate-behavior-decisions.md` "Phase 3: user decisions (DPES-1108, branch `feat/DPES-1108_user-decisions`)", items 13 and 17 ("Send answer sends the box text verbatim").
**Real agent:** none. Cost: low.
**Preconditions:** TC-Q-01 left its question open.

1. Click the option `beta`. Read the box "Your answer".
2. Type a space and `but keep it short` after `beta`, so the box reads `beta but keep it short`.
3. Click **Send answer**.

**Expected:**

- Step 1: the box holds `beta`. Nothing is sent yet.
- Step 3: the status line reads "Answered: `<title>`." The row leaves the **Needs you** tab. If no other item waits there, the tab shows the empty state "Nothing needs you".

**Known limitations:** After **Send answer**, the **Review** page no longer lists the question. On the task page (Conversation tab), the answer stays as your turn until the app window reloads. A waiting worker receives it word for word (TC-B-04 checks that in the prompt log). There is no reconcile control: settling a question as moot (`decisions.reconcile`) has no UI, so it cannot be tested here.

## TC-Q-04 A deferral keeps the question open

Purpose: "later" is an answer with a date, not a close.
**Source:** `docs/design/firstmate-behavior-decisions.md` "Phase 3: user decisions (DPES-1108, branch `feat/DPES-1108_user-decisions`)", item 5.
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** An open question. Send this message to Global:

```text
Propose exactly one action and nothing else: decision.ask for the task whose objective starts with "Q-TASK:", question "Should the README line say alpha or beta?", options ["alpha", "beta"], key "readme-word-2".
```

1. In its row, open the **Defer** disclosure. Pick tomorrow under "Defer until". Click **Defer**.
2. Read the row.
3. Reload the page and read the **Held** tab again.

**Expected:**

- Step 1: the status line reads "Deferred: `<title>` until" and tomorrow's date. The text beside **Defer** reads "The task stays blocked until you answer. Up to 60 days."
- Step 2: the row is on the **Held** tab, with the pill "Held until" and tomorrow's date, and the line "manual-test-project · Task: `<title>`".
- Step 3: the row is still there and can still be answered.

**Reset:** answer it with `ok`.

## TC-Q-05 An open question keeps a queued task from starting

Purpose: a task held for the user does not run.
**Source:** `docs/design/firstmate-behavior-plan.md` "5.4 User decisions" ("A task with an open question is not schedulable"); `docs/design/firstmate-behavior-decisions.md` "Phase 3: user decisions (DPES-1108, branch `feat/DPES-1108_user-decisions`)", item 7.
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** No other task is running. Start the hold helper for `HOLD-` (setup section 8c); it holds every task whose objective starts with `HOLD-`.

1. Send this message to Global:

   ```text
   Propose exactly three actions and nothing else, each a task.propose for project manual-test-project, task_kind Investigate, delivery ReportOnly, with these objectives in order: "HOLD-1: Read README.md and report its first line. Change no file.", "HOLD-2: Read README.md and report its first line. Change no file.", "HOLD-3 Wait for my answer: Read README.md and report its first line. Change no file."
   ```

2. Open the task `HOLD-3 Wait for my answer: …` on its project task page. It is `Queued`. Stop the hold helper with Ctrl-C.
3. Send this message to Global:

   ```text
   Propose exactly one action and nothing else: decision.ask for the task whose objective starts with "HOLD-3", question "Should the README line say alpha or beta?", options ["alpha", "beta"], key "readme-word".
   ```

4. Run `sh docs/manual-tests/manual-test.sh release HOLD-1`. Open the `HOLD-1` task and click **Cancel** on its project task page.
5. Open the `HOLD-3` task on its project task page, Conversation tab. Click **Refresh queue status** after 10 seconds. Read the section "Waiting to start".
6. Start the hold helper for `HOLD-3`. On the **Review** page, **Needs you** tab, answer the question with `alpha` and **Send answer**.
7. Open the `HOLD-3` task again and wait up to 30 seconds.

**Expected:**

- Step 5: the task is still `Queued`. "Waiting to start" reads "This task has an open question for you." and "Next step: Answer or defer it under Needs your decision in Review & Decisions."
- Step 7: the task is `Running`.

**Reset:** stop the hold helper. Release `HOLD-` and cancel the two running tasks.

## TC-Q-06 A cancelled task keeps its open question

Purpose: only the user closes a question.
**Source:** `docs/design/firstmate-behavior-decisions.md` "Phase 3: user decisions (DPES-1108, branch `feat/DPES-1108_user-decisions`)", item 8.
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** A held running task with an open question. Start the hold helper for `HOLD-4`, send the first message below, stop the helper when the task reads `Running`, then send the second message.

```text
Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Investigate, delivery ReportOnly, objective "HOLD-4: Read README.md and report its first line. Change no file."
```

```text
Propose exactly one action and nothing else: decision.ask for the task whose objective starts with "HOLD-4:", question "Should the README line say alpha or beta?", options ["alpha", "beta"], key "readme-word".
```

1. Run `sh docs/manual-tests/manual-test.sh release HOLD-4`. On the project task page, click **Cancel**. Wait until the task is `Canceled`.
2. Open **Review** and read the **Needs you** tab.

**Expected:** The question is still in the **Needs you** tab. It leaves only when you answer it.

**Reset:** answer it with `cancelled`.

## TC-Q-07 Only the app that started the daemon can answer

Purpose: answers are the user's, so other sessions cannot read or send them.
**Source:** `docs/design/firstmate-behavior-decisions.md` "Phase 3: user decisions (DPES-1108, branch `feat/DPES-1108_user-decisions`)", items 10 and 17.
**Real agent:** none. Cost: low.
**Preconditions:** The running daemon was not started by this app, so the app is not its approver (`docs/settings.md` "When settings are locked"). Mark `BLOCKED` if you cannot arrange that.

1. Open **Review**.

**Expected:** The Review page shows the alert "Questions are not available: <reason>" and no question rows in the **Needs you** tab.
