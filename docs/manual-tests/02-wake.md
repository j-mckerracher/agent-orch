# 02 Wake: the Global Orchestrator wakes to check on its agents

Setup: [00-setup.md](00-setup.md). Wake events are listed in setup section 8d. Run these tests with **Let the orchestrator act on its own** off (setup section 6, step 5), so "approve the task" means a click on **Approve**.

Before TC-W-01, add a coordinator for `manual-test-project` (open the target picker, choose **Add coordinator…**, click **Add** next to `manual-test-project`). It gives you a second target to stay on while you wait. A coordinator with no work stays idle and costs nothing.

UI parts used here, all on the **Workspace** page:

- **Monitoring chip.** A chip in the Global header reading `Monitoring: <mode>`. Its accessible name is "Monitoring mode". Click it to open the mode menu.
- **Wake badge.** A count on the Global entry in the target picker, such as "1 update".
- **Wake notice.** A block in the Global conversation labelled "Update, not a reply to you".
- **Open task.** A link inside a task notice that opens that task.

Wake turns always run at Restricted permission. They run only when the Global conversation is not busy with a user turn.

**Messages used here.** Send them to Global, word for word. Each makes Global propose one task; the step that sends one is **Model-dependent** (RUNNER.md rule 8). The objectives are from setup section 8b.

- **Failing task message** (NO-CHANGE; the task ends `Failed`, after the daemon's one retry):

  ```text
  Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Implement, delivery ReportOnly, objective "Read README.md. Do not create, change or delete any file."
  ```

- **Writing task message** (WRITE; the task ends `Verifying` with a candidate):

  ```text
  Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Implement, delivery LocalReview, objective "Add the line 'Manual test line.' at the end of README.md. Change no other file."
  ```

- **Held task message** (HOLD; start the hold helper first, setup section 8c):

  ```text
  Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Investigate, delivery ReportOnly, objective "HOLD-1: Read README.md and report its first line. Change no file."
  ```

Replies to wake turns come from a real model. Judge the notices, badges and labels the app writes, and copy each reply into the evidence folder (RUNNER.md rule 9).

---

## TC-W-01 Monitoring chip shows the mode

**Source:** `monitoringModel.ts`; `orchestrator-actions.md` "Wake" (UI).
**Real agent:** none. Cost: low.
**Preconditions:** Global selected.

1. Look at the Global header.

**Expected:** A chip reads `Monitoring: on` (or `quiet`, `off`, `away`). It is never blank. **Screenshot.**

## TC-W-02 Mode menu offers four modes

**Source:** same.
**Real agent:** none. Cost: low.
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
**Real agent:** none. Cost: low.
**Preconditions:** None.

1. Click the chip and choose **Quiet**.
2. Read the chip.
3. Quit the app, wait two minutes, start the app, and open Global.

**Expected:** The chip reads `Monitoring: quiet` in both step 2 and step 3.

**Reset:** set it back to **On**.

## TC-W-04 A failed task wakes Global with a reply

**Source:** `orchestrator-actions.md` "Wake" (task failed or retries exhausted: model turn).
**Real agent:** Sonnet (alias), medium, for the chat turns and two task attempts. Cost: medium.
**Preconditions:** Mode **On**.

1. Send the failing task message to Global and click **Approve**.
2. Choose `manual-test-project` in the target picker and stay there.
3. Wait until the task has failed (Agents page), then up to 5 minutes.
4. Look at the target picker.
5. Choose **Global**.

**Expected:**

- Step 4: the Global entry has a badge reading "1 update" (or a larger count).
- Step 5: the conversation ends with a wake notice labelled "Update, not a reply to you". It reports the failure and is followed by a reply from the orchestrator. Copy the reply; any wording passes.
- No message from you triggered it.

**Screenshot** after steps 4 and 5.

## TC-W-05 Stalled task and waiting delivery give notices only

**Source:** `orchestrator-actions.md` "Wake" (stalled: notice only, no model turn; delivery waiting: notice only).
**Real agent:** Sonnet (alias), medium. Cost: medium (the repeat runs a WRITE task).
**Preconditions:** Mode **On**. Prompt log running (setup section 7).

1. Start the hold helper for `HOLD-1`. Send the held task message to Global and click **Approve**. When the task reads `Running`, stop the helper.
2. Wait 8 minutes (RUNNER.md rule 7).
3. Open Global.

**Expected:** A deterministic notice says the task is stalled. It was not written by the model. No new assistant reply was generated for it. **Prompt log:** no turn entry starts after the stalled notice's time.

**Reset:** release `HOLD-1` and click **Request cancel** on the task page.

Repeat with the writing task message, so the task changes a file and a delivery waits for review. Expected: a notice says a delivery is waiting.

## TC-W-06 Quiet answer to a work check

**Source:** `orchestrator-actions.md` "Output" (only heartbeat batches, work check and daemon start, may be answered quietly).
**Real agent:** Sonnet (alias), medium. Cost: low; each work check is one turn.
**Preconditions:** Mode **On**. Settings, **Checks on running work**: base interval `1` minute, saved. A held task: start the hold helper for `HOLD-1`, send the held task message, click **Approve**, and stop the helper when the task reads `Running`.

1. Wait 3 minutes while the task runs. A work check runs about every minute.
2. Open Global.

**Expected:** One collapsed line reads "Checked N updates. Nothing needs you." N is the number of checks so far, at least 1; "Checked 1 update. Nothing needs you." is correct after one check. The token `MCAO_QUIET` is not shown. A stalled-task notice may also appear. It is a separate notice and does not change this result.

**Model-dependent:** the model chooses whether a work check needs a reply. If a check got a text reply instead, count a miss and wait for the next check as the next attempt (no reset needed). Copy each reply.

**Reset:** release `HOLD-1` and click **Request cancel**. Set the base interval back to `10` minutes.

## TC-W-07 A user message cancels a running wake

**Source:** `monitoringModel.ts` ("The orchestrator is checking an update. Sending cancels it.").
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** Mode **On**.

1. Send the failing task message to Global and click **Approve**. Stay on Global.
2. When the task has failed, watch the text near the message box. Read it as soon as it changes.
3. While that text shows, type `Hello` and click **Send message to Global**.

**Expected:**

- Step 2: "The orchestrator is checking an update. Sending cancels it."
- Step 3: your message is handled first. The wake turn is cancelled. The wake notice does not show a reply from the cancelled turn.

**Timing:** a real wake turn may finish before you can send. If the text in step 2 is gone before step 3, count a miss and repeat from step 1 (RUNNER.md rule 8).

## TC-W-08 Reply limit pauses monitoring

**Source:** `docs/settings.md` "Automatic replies"; `monitoringModel.ts`.
**Real agent:** Sonnet (alias), medium. Cost: medium (two failing tasks).
**Preconditions:** None.

1. Open Settings, group **Automatic replies**. Set the per-conversation hourly cap to `1`. Click **Save**.
2. Send the failing task message to Global and click **Approve**. Wait for its wake notice.
3. Send the failing task message again and click **Approve**. Wait until it has failed, then 5 minutes. Do not use work checks: a work check past half of the cap is not made at all, so it shows no notice.
4. Open Global.

**Expected:**

- The first failure gets a reply.
- The header shows "Monitoring paused: limit reached". It may end with ", resumes about `<time>`".
- The second failure still shows a notice. The notice offers a link or text "Change limits" that leads to the Settings page.

**Reset:** set the hourly cap back to `20` and click **Save**.

## TC-W-09 Three failed wake turns pause automatic updates

**Source:** `orchestrator-actions.md` "Wake" (agent failures).
**Real agent:** none. Cost: none.
**Preconditions:** `BLOCKED`. No trigger makes automatic turns fail while your own messages and tasks still work (setup section 8c, "Failing automatic turns"). Record `BLOCKED` with that reason and do not run the steps.

1. Cause three wake events that need a model turn, spaced at least 70 seconds apart (debounce).
2. After each, open Global and read the notice.
3. Cause a fourth event.

**Expected:**

- Each of the first three notices reads "The automatic reply failed."
- After the third, automatic updates are paused. The fourth event produces a notice that names the agent and an error code, not a model turn. The skip text reads "Automatic updates are paused while the agent is failing".

## TC-W-10 The pause lifts when the default agent changes

**Source:** same ("or when the default agent changes in Settings").
**Real agent:** none. Cost: none.
**Preconditions:** `BLOCKED`: it needs the pause of TC-W-09.

1. In Settings, change the default provider and click **Save dispatch defaults**.
2. Cause another wake event.

**Expected:** The event gets a model turn again. The pause notice no longer appears.

## TC-W-11 Away mode and return brief

**Source:** `docs/m7-08-conversation-service.md` (away mode); `briefModel.ts`.
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** Mode **On**.

1. Click the chip and choose **Away**.
2. Send the failing task message to Global and click **Approve**. Wait until the task has failed, then 5 minutes.
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
**Real agent:** none. Cost: low.
**Preconditions:** None.

1. Open Settings, group **Checks on running work**.
2. Set base interval `10` minutes, fastest interval `60` seconds, wall-time limit `30` minutes. Click **Save**.
3. Reopen Settings after a restart.

**Expected:** The saved values persist. Out-of-range values (base interval `0` or `241`) are refused with an error on the field. There is no "Check now" button anywhere in Workspace or Settings.

## TC-W-13 Wake badge clears when seen

**Source:** `monitoringModel.ts`; `useMonitoring.ts`.
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** One unread wake notice: do TC-W-04 steps 1 to 4.

1. Choose **Global** and keep it open for 10 seconds.
2. Switch to another target.

**Expected:** The "1 update" badge on Global is gone. The badge's accessible name no longer has ", N new update(s)".

## TC-W-14 Mode Off sends notices only

**Source:** `orchestrator-actions.md` "Wake" (modes).
**Real agent:** Sonnet (alias), medium, for the chat turn and the task. Cost: medium.
**Preconditions:** None.

1. Click the chip and choose **Off**.
2. Send the failing task message to Global and click **Approve**. Wait until the task has failed, then 5 minutes.
3. Open Global.

**Expected:** A deterministic notice for the failure exists. No model reply. The skip text reads "Monitoring was off".

**Reset:** set the mode back to **On**.

## TC-W-15 An empty change set fails with a reason

**Source:** `orchestrator-actions.md` "Output" (an Implement run that exits 0 and changes no files fails with a reason).
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** Mode **On**.

1. Send the failing task message to Global and click **Approve**.
2. Choose `manual-test-project` and wait until the task has failed, then up to 5 minutes. Read the Global badge.
3. Choose **Global** and read the notice.
4. Click **Open task** in the notice.

**Expected:**

- Step 2: the Global entry has the badge "1 update".
- Step 3: a notice says the task failed and gives the reason. It contains "The agent exited without changing any files." (It may continue with "Last message:" text.)
- Step 4: the task is `Failed`. The task page reads "Failure reason: The agent exited without changing any files." It does not read "unavailable".

**Model-dependent:** if the worker changed a file anyway, the task does not fail; count a miss.

**Screenshot** after steps 3 and 4.

## TC-W-16 A candidate-ready update says what was done

**Source:** `orchestrator-actions.md` event table ("Candidate ready": model turn, waits for review).
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** Mode **On**.

1. Send the writing task message to Global and click **Approve**.
2. Choose `manual-test-project` and wait until the task is `Verifying`, then up to 5 minutes. Read the Global badge.
3. Choose **Global** and read the notice and the reply.
4. Click **Open task** in the notice.

**Expected:**

- Step 2: the Global entry has the badge "1 update".
- Step 3: a notice says the task finished with a candidate and gives its file count, `1`. A reply from the orchestrator follows it; any wording passes.
- Step 3: "No automatic explanation was given" does not appear.
- Step 4: the task is `Verifying`, not `Failed`.

**Model-dependent:** if the worker did not change `README.md`, the task fails instead; count a miss.

## TC-W-17 A quiet reply does not hide a failure

**Source:** `orchestrator-actions.md` "Output" (an outcome record a quiet reply does not cover stays visible as unanswered, with its notify flag).
**Real agent:** none. Cost: none.
**Preconditions:** `BLOCKED`. It needs a quiet reply to a failure, which a real model does not give on request (setup section 8c). Record `BLOCKED` with that reason and do not run the steps.

1. Approve a failing task.
2. Stay on `manual-test-project` and wait for the failure and the wake. Read the Global badge.
3. Choose **Global** and read the notice.

**Expected:**

- Step 2: the Global entry has the badge "1 update".
- Step 3: the failure notice stays visible. Directly under it reads "No automatic explanation was given. Open the task."
- Step 3: no line reads "Checked 1 update. Nothing needs you." for this failure. The token `MCAO_QUIET` is not shown.
- Step 3: the notice has an **Open task** link.

## TC-W-18 A report relay that gets quiet or failed replies twice

**Source:** `orchestrator-actions.md` "Output" (a report relay that gets a quiet or failed reply is retried once, then a deterministic notice); `docs/m7-08-conversation-service.md` "Relay".
**Real agent:** none. Cost: none.
**Preconditions:** `BLOCKED`. It needs two quiet or failed replies to a report relay, and neither can be caused with a real model (setup section 8c). Record `BLOCKED` with that reason and do not run the steps.

1. Approve a scout task (REPORT, setup section 8b). Stay on `manual-test-project`.
2. Wait for the relay and its one retry.
3. Choose **Global** and read the notice.

**Expected:**

- Step 3: a notice reads "Report from task `<task id>` is ready; automatic relay failed. Open the task to read it." Under it reads "The automatic reply failed." The notice has an **Open task** link.
- Step 3: the Global entry had the badge "1 update" in step 2.
- Only two wake turns ran for this report.
