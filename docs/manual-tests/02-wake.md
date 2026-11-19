# 02 Wake: the Global Orchestrator wakes to check on its agents

Setup: [00-setup.md](00-setup.md). Wake events are listed in setup section 6. Run these tests with **Let the orchestrator act on its own** off (setup section 5, step 5), so "approve the task" means a click on **Approve**.

UI parts used here, all on the **Workspace** page:

- **Monitoring chip.** A chip in the Global header reading `Monitoring: <mode>`. Its accessible name is "Monitoring mode". Click it to open the mode menu.
- **Wake badge.** A count on the Global entry in the target picker, such as "1 update".
- **Wake notice.** A block in the Global conversation labelled "Update, not a reply to you".
- **Open task.** A link inside a task notice that opens that task.

Wake turns always run at Restricted permission. They run only when the Global conversation is not busy with a user turn.

---

## TC-W-01 Monitoring chip shows the mode

**Source:** `monitoringModel.ts`; `orchestrator-actions.md` "Wake" (UI).
**Preconditions:** Global selected.

1. Look at the Global header.

**Expected:** A chip reads `Monitoring: on` (or `quiet`, `off`, `away`). It is never blank. **Screenshot.**

## TC-W-02 Mode menu offers four modes

**Source:** same.
**Preconditions:** TC-W-01.

1. Click the **Monitoring** chip.
2. Read the menu.

**Expected:** The menu lists, with this wording:

- "On: tell me about updates"
- "Quiet: keep updates, do not notify me"
- "Off: notices only, no automatic replies"
- "Away: replies stay quiet, standing approvals keep working, and you get a summary when you return. It adds no permissions."

**Screenshot.**

## TC-W-03 Changing the mode persists

**Source:** same; RPC `monitoring.set`.
**Preconditions:** None.

1. Click the chip and choose **Quiet**.
2. Read the chip.
3. Quit the app, wait two minutes, start the app, and open Global.

**Expected:** The chip reads `Monitoring: quiet` in both step 2 and step 3.

Set it back to **On** afterward.

## TC-W-04 A failed task wakes Global with a reply

**Source:** `orchestrator-actions.md` "Wake" (task failed or retries exhausted: model turn).
**Preconditions:** Mode **On**. `task-mode` is `fail`. `next-reply.txt` contains a short text reply: `The task failed. I suggest a retry.`

1. Make Global propose the standard task and click **Approve**.
2. Open Workspace and stay on a different target, for example `manual-test-project`.
3. Wait up to two minutes (60-second debounce).
4. Look at the target picker.
5. Choose **Global**.

**Expected:**

- Step 4: the Global entry has a badge reading "1 update" (or a larger count).
- Step 5: the conversation ends with a wake notice labelled "Update, not a reply to you". It contains the reply text from `next-reply.txt`.
- No message from you triggered it.

**Screenshot** after steps 4 and 5.

## TC-W-05 Stalled task and waiting delivery give notices only

**Source:** `orchestrator-actions.md` "Wake" (stalled: notice only, no model turn; delivery waiting: notice only).
**Preconditions:** Mode **On**. `task-mode` is `hang`. Clear `next-reply.txt`.

1. Approve a task. Wait 5 minutes with no output.
2. Open Global.

**Expected:** A deterministic notice says the task is stalled. It was not written by the model. No new assistant reply was generated for it. Confirm the fake agent log shows no conversation turn for it, or that `next-reply.txt` was not consumed.

Repeat with `task-mode` set to `write`, so the task changes a file and a delivery waits for review. Expected: a notice says a delivery is waiting. **BLOCKED until supported:** the fake agent has no `write` mode yet (`00-setup.md` section 2). Mark this repeat `BLOCKED` until it does.

## TC-W-06 Quiet answer to a work check

**Source:** `orchestrator-actions.md` "Output" (only heartbeat batches, work check and daemon start, may be answered quietly).
**Preconditions:** Mode **On**. `next-reply.txt` contains `MCAO_QUIET`. Settings, **Checks on running work**: base interval `1` minute. Approve the standard task as in TC-W-04 step 1, with `task-mode` set to `hang`. If the fake agent cannot print `MCAO_QUIET`, mark `BLOCKED`.

1. Wait about two minutes while the task runs. A work check runs about every minute.
2. Open Global.

**Expected:** One collapsed line reads "Checked N updates. Nothing needs you." N is the number of checks so far, at least 1; "Checked 1 update. Nothing needs you." is correct after one check. The token `MCAO_QUIET` is not shown. A stalled-task notice may also appear. It is a separate notice and does not change this result.

Cancel the task and set the base interval back to `10` minutes afterward.

## TC-W-07 A user message cancels a running wake

**Source:** `monitoringModel.ts` ("The orchestrator is checking an update. Sending cancels it.").
**Preconditions:** A wake turn in progress. Make the fake agent take 30 seconds to reply to a wake turn.

1. Cause a wake event. Open Global while the turn runs.
2. Read the text near the message box.
3. Type `Hello` and click **Send message to Global**.

**Expected:**

- Step 2: "The orchestrator is checking an update. Sending cancels it."
- Step 3: your message is handled first. The wake turn is cancelled. The wake notice does not show a reply from the cancelled turn.

## TC-W-08 Reply limit pauses monitoring

**Source:** `docs/settings.md` "Automatic replies"; `monitoringModel.ts`.
**Preconditions:** None.

1. Open Settings, group **Automatic replies**. Set the per-conversation hourly cap to `1`. Click **Save**.
2. Cause two wake events that need a model turn, at least 70 seconds apart, such as two failed tasks. Do not use work checks: a work check past half of the cap is not made at all, so it shows no notice.
3. Open Global.

**Expected:**

- The first event gets a reply.
- The header shows "Monitoring paused: limit reached". It may end with ", resumes about `<time>`".
- The second event still shows a notice. The notice offers a link or text "Change limits" that leads to the Settings page.

Reset the cap afterward.

## TC-W-09 Three failed wake turns pause automatic updates

**Source:** `orchestrator-actions.md` "Wake" (agent failures).
**Preconditions:** App started with `fake-codex-fail` as the Codex program. Mode **On**.

1. Cause three wake events that need a model turn, spaced at least 70 seconds apart (debounce).
2. After each, open Global and read the notice.
3. Cause a fourth event.

**Expected:**

- Each of the first three notices reads "The automatic reply failed."
- After the third, automatic updates are paused. The fourth event produces a notice that names the agent and an error code, not a model turn. The skip text reads "Automatic updates are paused while the agent is failing".

## TC-W-10 The pause lifts when the default agent changes

**Source:** same ("or when the default agent changes in Settings").
**Preconditions:** TC-W-09 left the pause in place. A working second agent is available, or restart with a working Codex.

1. In Settings, change the default provider and click **Save** in **Agent dispatch defaults**.
2. Cause another wake event.

**Expected:** The event gets a model turn again. The pause notice no longer appears.

## TC-W-11 Away mode and return brief

**Source:** `docs/m7-08-conversation-service.md` (away mode); `briefModel.ts`.
**Preconditions:** Mode **On**.

1. Click the chip and choose **Away**.
2. Cause a wake event.
3. Open Global and read the notice.
4. Click the chip and choose **On**.
5. Look for a return brief in Global.
6. Acknowledge the brief using its button.

**Expected:**

- Step 3: the reply is stored quietly. There is no notification badge.
- Step 5: a block titled "While you were away" and the label "Return brief ready" appear.
- Step 6: the label changes to "Return brief acknowledged". The brief does not return after a reload.

## TC-W-12 Check interval settings

**Source:** `docs/settings.md` "Checks on running work".
**Preconditions:** None.

1. Open Settings, group **Checks on running work**.
2. Set base interval `10` minutes, fastest interval `60` seconds, wall-time limit `30` minutes. Click **Save**.
3. Reopen Settings after a restart.

**Expected:** The saved values persist. Out-of-range values (base interval `0` or `241`) are refused with an error on the field. There is no "Check now" button anywhere in Workspace or Settings.

## TC-W-13 Wake badge clears when seen

**Source:** `monitoringModel.ts`; `useMonitoring.ts`.
**Preconditions:** One unread wake notice, as in TC-W-04.

1. Choose **Global** and keep it open for 10 seconds.
2. Switch to another target.

**Expected:** The "1 update" badge on Global is gone. The badge's accessible name no longer has ", N new update(s)".

## TC-W-14 Mode Off sends notices only

**Source:** `orchestrator-actions.md` "Wake" (modes).
**Preconditions:** None.

1. Click the chip and choose **Off**.
2. Cause a failed-task event.
3. Open Global.

**Expected:** A deterministic notice for the failure exists. No model reply. The skip text reads "Monitoring was off".

## TC-W-15 An empty change set fails with a reason

**Source:** `orchestrator-actions.md` "Output" (an Implement run that exits 0 and changes no files fails with a reason).
**Preconditions:** Mode **On**. `task-mode` is `succeed`, which exits 0 and changes no file. Clear `next-reply.txt`.

1. Approve the standard task as in TC-W-04 step 1.
2. Stay on `manual-test-project` and wait up to two minutes. Read the Global badge.
3. Choose **Global** and read the notice.
4. Click **Open task** in the notice.

**Expected:**

- Step 2: the Global entry has the badge "1 update".
- Step 3: a notice says the task failed and gives the reason. It contains "The agent exited without changing any files." (It may continue with "Last message:" text.)
- Step 4: the task is `Failed`. The task page reads "Failure reason: The agent exited without changing any files." It does not read "unavailable".

**Screenshot** after steps 3 and 4.

## TC-W-16 A candidate-ready update says what was done

**Source:** `orchestrator-actions.md` event table ("Candidate ready": model turn, waits for review).
**Preconditions:** Mode **On**. `task-mode` is `write`, which changes one file and exits 0. `next-reply.txt` contains `The change is ready. It waits for your review.` **BLOCKED until supported:** the fake agent has no `write` mode yet (`00-setup.md` section 2). Mark TC-W-16 `BLOCKED` until it does.

1. Approve the standard task as in TC-W-04 step 1.
2. Stay on `manual-test-project` and wait up to two minutes. Read the Global badge.
3. Choose **Global** and read the notice and the reply.
4. Click **Open task** in the notice.

**Expected:**

- Step 2: the Global entry has the badge "1 update".
- Step 3: a notice says the task finished with a candidate and gives its file count. Then the reply reads "The change is ready. It waits for your review."
- Step 3: "No automatic explanation was given" does not appear.
- Step 4: the task is `Verifying`, not `Failed`.

## TC-W-17 A quiet reply does not hide a failure

**Source:** `orchestrator-actions.md` "Output" (an outcome record a quiet reply does not cover stays visible as unanswered, with its notify flag).
**Preconditions:** Mode **On**. `task-mode` is `fail`. `next-reply.txt` contains `MCAO_QUIET`.

1. Approve the standard task as in TC-W-04 step 1.
2. Stay on `manual-test-project` and wait up to two minutes. Read the Global badge.
3. Choose **Global** and read the notice.

**Expected:**

- Step 2: the Global entry has the badge "1 update".
- Step 3: the failure notice stays visible. Directly under it reads "No automatic explanation was given. Open the task."
- Step 3: no line reads "Checked 1 update. Nothing needs you." for this failure. The token `MCAO_QUIET` is not shown.
- Step 3: the notice has an **Open task** link.

## TC-W-18 A report relay that gets quiet replies twice

**Source:** `orchestrator-actions.md` "Output" (a report relay that gets a quiet or failed reply is retried once, then a deterministic notice); `docs/m7-08-conversation-service.md` "Relay".
**Preconditions:** Mode **On**. `next-reply.txt` contains `MCAO_QUIET`. Approve a scout as in `00-setup.md` section 6 (`Investigate`, `ReportOnly`), with `task-mode` set to `succeed`.

1. Approve the scout task. Stay on `manual-test-project`.
2. Wait up to four minutes. The first relay gets a quiet reply. The app retries once after about a minute and gets another quiet reply.
3. Choose **Global** and read the notice.

**Expected:**

- Step 3: a notice reads "Report from task `<task id>` is ready; automatic relay failed. Open the task to read it." Under it reads "The automatic reply failed." The notice has an **Open task** link.
- Step 3: the Global entry had the badge "1 update" in step 2.
- Only two wake turns ran for this report. If the fake agent keeps a log, it shows two turns for this report and no third.
