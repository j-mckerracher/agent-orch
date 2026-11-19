# 05 Decisions: questions for the user are held until the user answers

Setup: [00-setup.md](00-setup.md), sections 2a and 2b. **Let the orchestrator act on its own** is on.

A question is raised by a `decision.ask` action (this file) or by a worker that stops to ask (`06-workers.md`). It is listed under **Needs your decision**, the first card of the queue on the **Review & Decisions** page. Each row shows the question, "Task:" and the task title, one button per option, the box "Your answer" with **Send answer**, and "Defer until" with **Defer**.

Sources: `docs/tasks.md` "Task states"; `docs/design/firstmate-behavior-decisions.md` "Phase 3".

The question block used here is one `decision.ask` action with `task_id` (the task's id, from its page), `question` `Should the README line say alpha or beta?`, `options` `["alpha", "beta"]` and `key` `readme-word`.

---

## TC-Q-01 A question appears under Needs your decision

Purpose: a question reaches the user as a record, not as chat text.
**Source:** `docs/design/firstmate-behavior-decisions.md` Phase 3, item 16 ("Needs your decision" reads only `decisions.list`).
**Preconditions:** A task of `manual-test-project` exists in any state, for example the failed task of TC-D-05. `next-reply.txt` holds the question block for it.

1. Send `Ask me about the README wording` to Global. Clear `next-reply.txt`.
2. Wait five seconds. Read the card.
3. Open **Review & Decisions**. Read **Needs your decision**.

**Expected:**

- Step 2: the card "Proposed: Ask for your decision" reads "Done on its own" and "Asked you about “Add a README line”. Answer it under Needs your decision in Review & Decisions." Its fields show the question and the options `alpha` and `beta`.
- Step 3: one row reads "Should the README line say alpha or beta?" with "Task: Add a README line", and the buttons `alpha` and `beta`.

**Screenshot** after step 3.

## TC-Q-02 Asking again with the same key does not add a second question

Purpose: one open question per task and key.
**Source:** same, Phase 3 item 6.
**Preconditions:** TC-Q-01 left its question open.

1. Put the question block in `next-reply.txt` again, with the question changed to `Alpha or beta for the README?` and the same key.
2. Send `Ask again`. Clear `next-reply.txt`.
3. Read **Needs your decision**.

**Expected:** Still one row for that task, with the first wording.

## TC-Q-03 An answer is sent exactly as typed

Purpose: the user's words are the answer; an option only fills the box.
**Source:** same, Phase 3 items 13 and 17 ("Send answer sends the box text verbatim").
**Preconditions:** TC-Q-01 left its question open.

1. Click the option `beta`. Read the box "Your answer".
2. Type a space and `but keep it short` after `beta`, so the box reads `beta but keep it short`.
3. Click **Send answer**.

**Expected:**

- Step 1: the box holds `beta`. Nothing is sent yet.
- Step 3: the status line reads "Answered: Add a README line." The row leaves the list. With no other question the card reads "No question is waiting for your answer."

**Known limitations:** the app shows the answer nowhere after it is sent. A waiting worker receives it word for word (TC-B-04 checks that in the prompt log). There is no reconcile control: settling a question as moot (`decisions.reconcile`) has no UI, so it cannot be tested here.

## TC-Q-04 A deferral keeps the question open

Purpose: "later" is an answer with a date, not a close.
**Source:** same, Phase 3 item 5.
**Preconditions:** An open question (repeat TC-Q-01 with key `readme-word-2`).

1. In its row, pick tomorrow under "Defer until". Click **Defer**.
2. Read the row.
3. Reload the page and read **Needs your decision** again.

**Expected:**

- Step 1: the status line reads "Deferred: Add a README line until" and tomorrow's date. The note beside **Defer** reads "Pick a day from today to 60 days from today. The task stays blocked until you answer."
- Step 2: the row is still listed, with the pill "Deferred" and "Task: Add a README line · Deferred until" and tomorrow's date.
- Step 3: the row is still there and can still be answered.

**Reset:** answer it with `ok`.

## TC-Q-05 An open question keeps a queued task from starting

Purpose: a task held for the user does not run.
**Source:** `docs/design/firstmate-behavior-plan.md` 5.4 ("A task with an open question is not schedulable"); Phase 3 item 7.
**Preconditions:** `task-mode` is `hang`. No other task is running.

1. Put a block with three standard `task.propose` actions in `next-reply.txt`, objectives `Hold slot one`, `Hold slot two` and `Wait for my answer`. Send `Start three tasks`. Clear `next-reply.txt`.
2. Open the task `Wait for my answer`. It is `Queued`: two tasks run at most at once.
3. Put the question block for `Wait for my answer` in `next-reply.txt`. Send `Ask me first`. Clear `next-reply.txt`.
4. Open `Hold slot one` and click **Request cancel**.
5. Open `Wait for my answer`. Click **Refresh queue status** after 10 seconds. Read the section "Waiting to start".
6. Answer the question in **Review & Decisions**.
7. Open `Wait for my answer` again and wait up to 30 seconds.

**Expected:**

- Step 5: the task is still `Queued`. "Waiting to start" reads "This task has an open question for you." and "Next step: Answer or defer it under Needs your decision in Review & Decisions."
- Step 7: the task is `Running`.

**Reset:** cancel the two running tasks. Write `succeed` to `task-mode`.

## TC-Q-06 A cancelled task keeps its open question

Purpose: only the user closes a question.
**Source:** same, Phase 3 item 8.
**Preconditions:** A running task (`task-mode` `hang`) with an open question raised as in TC-Q-01.

1. On the task page, click **Request cancel**. Wait until the task is `Canceled`.
2. Open **Review & Decisions**.

**Expected:** The question is still under **Needs your decision**. It leaves only when you answer it.

**Reset:** answer it with `cancelled`.

## TC-Q-07 Only the app that started the daemon can answer

Purpose: answers are the user's, so other sessions cannot read or send them.
**Source:** same, Phase 3 items 10 and 17.
**Preconditions:** The running daemon was not started by this app, so the app is not its approver (`docs/settings.md` "When settings are locked"). Mark `BLOCKED` if you cannot arrange that.

1. Open **Review & Decisions**.

**Expected:** **Needs your decision** shows "Questions are not available" and no rows.
