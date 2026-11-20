# 02 Wake: the Global Orchestrator wakes to check on its agents

Setup: [00-setup.md](00-setup.md). Wake events are listed in setup section 8d. Run these tests with **Let the orchestrator act on its own** off (setup section 6, step 5), so "approve the task" means a click on **Approve**.

Before TC-W-01, add a coordinator for `manual-test-project` (open the target picker, choose **Add coordinator…**, click **Add** next to `manual-test-project`). It gives you a second target to stay on while you wait. A coordinator with no work stays idle and costs nothing.

UI parts used here, all on the **Workspace** page:

- **Monitoring chip.** A button in the **Global orchestrator** header reading `Monitoring: <mode>`. Its accessible name is that text. Click it to open the mode menu, which is labelled "Monitoring mode".
- **Wake badge.** A count on the **Global orchestrator** entry in the target picker, such as "1 update".
- **Wake notice.** A block in the Global orchestrator conversation labelled "Update, not a reply to you".
- **Open task.** A link inside a task notice. It opens the task's page, which has the tabs Conversation, Attempts and Timeline.

Wake turns always run at Restricted permission. They run only when the Global conversation is not busy with a user turn.

**Messages used here.** Send them to **Global orchestrator**, word for word. Each makes Global propose one task; the step that sends one is **Model-dependent** (RUNNER.md rule 8). The objectives are from setup section 8b.

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

**Source:** `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" (bullet **What the user sees**); `apps/desktop/src/ui/views/workspace/monitoringModel.ts` (`chipLabel`).
**Real agent:** none. Cost: low.
**Preconditions:** Global orchestrator selected.

1. Look at the **Global orchestrator** header.

**Expected:** A chip reads `Monitoring: on` (or `quiet`, `off`, `away`). It is never blank. **Screenshot.**

## TC-W-02 Mode menu offers four modes

**Source:** `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" (bullet **Modes**); `apps/desktop/src/ui/views/workspace/monitoringModel.ts` (`MODE_OPTIONS`).
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

**Source:** `apps/desktop/src/daemon/api.ts` (`monitoring.set`); `apps/desktop/src/ui/views/workspace/MonitoringChip.tsx`.
**Real agent:** none. Cost: low.
**Preconditions:** None.

1. Click the chip and choose **Quiet**.
2. Read the chip.
3. Quit the app, wait two minutes, start the app, and open **Global orchestrator**.

**Expected:** The chip reads `Monitoring: quiet` in both step 2 and step 3.

**Reset:** choose **On** in the mode menu.

## TC-W-04 A failed task wakes Global with a reply

**Source:** `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" (event table, row "Task failed, verification failed, retries exhausted").
**Real agent:** Sonnet (alias), medium, for the chat turns and two task attempts. Cost: medium.
**Preconditions:** Mode **On**.

1. Send the failing task message to **Global orchestrator** and click **Approve**.
2. Choose `manual-test-project` in the target picker and stay there.
3. Wait until the task has failed (Agents page), then up to 5 minutes.
4. Look at the target picker.
5. Choose **Global orchestrator**.

**Expected:**

- Step 4: the **Global orchestrator** entry has a badge reading "1 update" (or a larger count).
- Step 5: the conversation shows a wake notice labelled "Update, not a reply to you" that reports the failure. A reply from the orchestrator follows it. Copy the reply; any wording passes.
- No message from you triggered it.

**Screenshot** after steps 4 and 5.

## TC-W-05 Stalled task and waiting delivery give notices only

**Source:** `crates/orchestrator-store/src/sqlite_wake.rs` (`needs_model`: "A stall is a notice only"); `crates/orchestrator-core/src/supervision.rs` (`STALL_QUIET_FLOOR`, `stalled`); `crates/orchestrator-daemon/src/wake_events.rs` (the "is ready for your approval." notice).
**Real agent:** Sonnet (alias), medium, for the task runs. Cost: medium.
**Preconditions:** Mode **On**. Prompt log running (setup section 7).

1. Start the hold helper for `HOLD-1` (setup section 8c). Send the held task message to **Global orchestrator** and click **Approve**. When the task reads `Running` on the **Agents** page, stop the helper.
2. Wait up to 20 minutes for the stall notice. The stall needs 10 minutes of silence at the default base interval (the quiet window is `max(interval, 5 minutes)`), and checks run once per interval, so this test has its own limit.
3. Open **Global orchestrator**.

**Expected:** A deterministic notice reads `Task <id> (<objective>) has been silent for <N> minutes.` It was not written by the model. No new assistant reply was generated for it. **Prompt log:** no turn entry is for the stall. A scheduled work-check turn for the running task may appear; it does not count.

**Reset:** release `HOLD-1` (setup section 8c), then cancel the task on the **Agents** page with **Actions** > **Cancel attempt**.

Repeat with the writing task message, so the task changes a file and its delivery waits for review. Expected: a notice reads `Task <id> (<objective>) is ready for your approval.`

**Reset for the repeat:** on the task page, choose **Discard changes** if the page offers it.

## TC-W-06 Quiet answer to a work check

**Source:** `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" (bullet **Output**); `apps/desktop/src/ui/views/workspace/monitoringModel.ts` (`quietLine`).
**Real agent:** Sonnet (alias), medium, for the work checks and the held task run. Cost: medium.
**Preconditions:** Mode **On**. On the **Limits** page, section **Checks on running work**: **Check interval with one task running (minutes)** set to `1` and saved with **Save changes**. Then start the hold helper for `HOLD-1`, send the held task message to **Global orchestrator**, click **Approve**, and stop the helper when the task reads `Running`.

1. Wait 3 minutes while the task runs. A work check runs about every minute.
2. Open **Global orchestrator**.

**Expected:** One collapsed line reads "Checked N updates. Nothing needs you." N is the number of checks so far, at least 1; "Checked 1 update. Nothing needs you." is correct after one check. The token `MCAO_QUIET` is not shown. A stalled-task notice may also appear. It is a separate notice and does not change this result.

**Model-dependent:** the model chooses whether a work check needs a reply. If a check got a text reply instead, count a miss and wait for the next check as the next attempt (no reset needed). Copy each reply.

**Reset:** release `HOLD-1` and cancel the task on the **Agents** page with **Actions** > **Cancel attempt**. On the **Limits** page, set **Check interval with one task running (minutes)** back to `10` and click **Save changes**.

## TC-W-07 A user message cancels a running wake

**Source:** `apps/desktop/src/ui/views/workspace/monitoringModel.ts` (`WAKE_RUNNING_TEXT`).
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** Mode **On**.

1. Send the failing task message to **Global orchestrator** and click **Approve**. Stay on **Global orchestrator**.
2. When the task has failed, watch the text near the message box. Read it as soon as it changes.
3. While that text shows, type `Hello` and click **Send message to Global orchestrator**.

**Expected:**

- Step 2: "The orchestrator is checking an update. Sending cancels it."
- Step 3: your message is handled first. The wake turn is cancelled. The wake notice does not show a reply from the cancelled turn.

**Timing:** a real wake turn may finish before you can send. If the text in step 2 is gone before step 3, count a miss and repeat from step 1 (RUNNER.md rule 8).

## TC-W-08 Reply limit pauses monitoring

**Source:** `docs/settings.md` "Automatic replies"; `apps/desktop/src/ui/views/workspace/monitoringModel.ts` (`PAUSED_TEXT`); `apps/desktop/src/ui/views/workspace/MonitoringChip.tsx` (the **Change limits** link).
**Real agent:** Sonnet (alias), medium. Cost: medium (two failing tasks).
**Preconditions:** None.

1. Open **Settings**, open the **Limits** page, and find the section **Automatic replies**. Set **Replies per hour, per conversation** to `1`. Click **Save changes**.
2. Send the failing task message to **Global orchestrator** and click **Approve**. Wait for its wake notice.
3. Send the failing task message again and click **Approve**. Wait until it has failed, then 5 minutes. Do not use work checks: a work check past half of the cap is not made at all, so it shows no notice.
4. Open **Global orchestrator**.

**Expected:**

- The first failure gets a reply.
- The header shows "Monitoring paused: limit reached". It may end with ", resumes about `<time>`". Beside it, a **Change limits** link opens Settings.
- The second failure still shows a notice, with the note "Monitoring paused: limit reached" and no reply.

**Reset:** on the **Limits** page, set **Replies per hour, per conversation** back to `20` and click **Save changes**.

## TC-W-09 Three failed wake turns pause automatic updates

**Source:** `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" (bullet **Agent failures**); `apps/desktop/src/ui/views/workspace/monitoringModel.ts` (`skipText`).
**Real agent:** Sonnet (alias), medium, for the task runs. Cost: medium.
**Preconditions:** `BLOCKED`. No trigger makes automatic turns fail while your own messages and tasks still work (setup section 8c, "Failing automatic turns"). Record `BLOCKED` with that reason and do not run the steps.

1. Cause three wake events that need a model turn, spaced at least 70 seconds apart (the debounce is 60 seconds).
2. After each, open **Global orchestrator** and read the notice.
3. Cause a fourth event.

**Expected:**

- Each of the first three notices reads "The automatic reply failed."
- After the third, automatic updates are paused. The fourth event produces a notice that names the agent and an error code, not a model turn. The skip text reads "Automatic updates are paused while the agent is failing".

## TC-W-10 The pause lifts when the default agent changes

**Source:** `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" (bullet **Agent failures**: "as soon as Settings change the default agent").
**Real agent:** Sonnet (alias), medium, for the task run. Cost: medium.
**Preconditions:** `BLOCKED`: it needs the pause of TC-W-09.

1. On **Settings**, page **Agents**, change **Default provider** and click **Save changes**.
2. Cause another wake event.

**Expected:** The event gets a model turn again. The pause notice no longer appears.

## TC-W-11 Away mode and return brief

**Source:** `docs/m7-08-conversation-service.md` "Away mode and the return brief"; `apps/desktop/src/ui/views/workspace/briefModel.ts`.
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** Mode **On**.

1. Click the chip and choose **Away**.
2. Send the failing task message to **Global orchestrator** and click **Approve**. Wait until the task has failed, then 5 minutes.
3. Open **Global orchestrator** and read the notice.
4. Click the chip and choose **On**.
5. Look for a return brief in **Global orchestrator**.
6. Click **Dismiss** on the brief.

**Expected:**

- Step 3: the reply is stored quietly. There is no notification badge.
- Step 5: a block titled "While you were away" and the label "Return brief ready" appear.
- Step 6: the label changes to "Return brief acknowledged". The brief does not return after a reload.

## TC-W-12 Check interval settings

**Source:** `docs/settings.md` "Checks on running work"; `apps/desktop/src/ui/views/settings/supervisionModel.ts` (field labels and ranges).
**Real agent:** none. Cost: low.
**Preconditions:** None.

1. Open **Settings**, open the **Limits** page, and find the section **Checks on running work**.
2. Set **Check interval with one task running (minutes)** to `10`, **Fastest check interval (seconds)** to `60`, and **Longest time a task may run before it is flagged (minutes)** to `30`. Click **Save changes**.
3. Reopen Settings after a restart.

**Expected:** The saved values persist. Out-of-range values (a check interval of `0` or `241` minutes) are refused with an error on the field. There is no "Check now" button anywhere in Workspace or Settings.

## TC-W-13 Wake badge clears when seen

**Source:** `apps/desktop/src/ui/views/workspace/monitoringModel.ts` (`badgeLabel`, `targetNameWithBadge`); `apps/desktop/src/ui/views/workspace/useMonitoring.ts`.
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** One unread wake notice: do TC-W-04 steps 1 to 4.

1. Choose **Global orchestrator** and keep it open for 10 seconds.
2. Switch to another target.

**Expected:** The "1 update" badge on **Global orchestrator** is gone. The entry's accessible name is `Global orchestrator`, with no `, 1 new update` suffix.

## TC-W-14 Mode Off sends notices only

**Source:** `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" (bullet **Modes**); `apps/desktop/src/ui/views/workspace/monitoringModel.ts` (`skipText`: "Monitoring was off").
**Real agent:** Sonnet (alias), medium, for the chat turn and the task. Cost: medium.
**Preconditions:** None.

1. Click the chip and choose **Off**.
2. Send the failing task message to **Global orchestrator** and click **Approve**. Wait until the task has failed, then 5 minutes.
3. Open **Global orchestrator**.

**Expected:** A deterministic notice for the failure exists. No model reply. The skip text reads "Monitoring was off".

**Reset:** choose **On** in the mode menu.

## TC-W-15 An empty change set fails with a reason

**Source:** `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" (bullet **Output**: "An Implement run that exits 0 and changes no files fails with a reason"); `crates/orchestrator-daemon/src/executor.rs` (the failure text and its "Last message:" suffix, lines 2842 to 2845).
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** Mode **On**.

1. Send the failing task message to **Global orchestrator** and click **Approve**.
2. Choose `manual-test-project` and wait until the task has failed, then up to 5 minutes. Read the **Global orchestrator** badge.
3. Choose **Global orchestrator** and read the notice.
4. Click **Open task** in the notice. On the task page, click its **Attempts N** tab (N is the number of attempts).

**Expected:**

- Step 2: the **Global orchestrator** entry has the badge "1 update".
- Step 3: a notice says the task failed and gives the reason. It contains "The agent exited without changing any files." (It may continue with "Last message:" text.)
- Step 4: the task is `Failed`. Its **Attempts** tab reads "Failure reason: The agent exited without changing any files." It does not read `unavailable.`

**Model-dependent:** if the worker changed a file anyway, the task does not fail; count a miss.

**Screenshot** after steps 3 and 4.

## TC-W-16 A candidate-ready update says what was done

**Source:** `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" (event table, row "Candidate ready (task Verifying)"); `crates/orchestrator-daemon/src/wake_events.rs` (`candidate_notice`).
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** Mode **On**.

1. Send the writing task message to **Global orchestrator** and click **Approve**.
2. Choose `manual-test-project` and wait until the task is `Verifying`, then up to 5 minutes. Read the **Global orchestrator** badge.
3. Choose **Global orchestrator** and read the notice and the reply.
4. Click **Open task** in the notice.

**Expected:**

- Step 2: the **Global orchestrator** entry has the badge "1 update".
- Step 3: a notice reads `Task <id> (<objective>) finished with a candidate (1 files changed) and waits for review.` A reply from the orchestrator follows it; any wording passes.
- Step 3: "No automatic explanation was given" does not appear.
- Step 4: the task is `Verifying`, not `Failed`.

**Model-dependent:** if the worker did not change `README.md`, the task fails instead; count a miss.

## TC-W-17 A quiet reply does not hide a failure

**Source:** `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" (bullet **Output**: "stays visible as `unanswered`"); `apps/desktop/src/ui/views/workspace/WakeNotice.tsx` (`stateNote`).
**Real agent:** Sonnet (alias), medium, for the task run. Cost: medium.
**Preconditions:** `BLOCKED`. It needs a quiet reply to a failure, which a real model does not give on request (setup section 8c). Record `BLOCKED` with that reason and do not run the steps.

1. Approve a failing task.
2. Stay on `manual-test-project` and wait for the failure and the wake. Read the **Global orchestrator** badge.
3. Choose **Global orchestrator** and read the notice.

**Expected:**

- Step 2: the **Global orchestrator** entry has the badge "1 update".
- Step 3: the failure notice stays visible. Directly under it reads "No automatic explanation was given. Open the task."
- Step 3: no line reads "Checked 1 update. Nothing needs you." for this failure. The token `MCAO_QUIET` is not shown.
- Step 3: the notice has an **Open task** link.

## TC-W-18 A report relay that gets quiet or failed replies twice

**Source:** `docs/m7-08-conversation-service.md` "Relay" (bullet **Relay.**); `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" (bullet **Output**: a relay "is retried once"); `crates/orchestrator-daemon/src/wake_events.rs` (the relay-failed notice).
**Real agent:** Sonnet (alias), medium, for the scout task run. Cost: medium.
**Preconditions:** `BLOCKED`. It needs two quiet or failed replies to a report relay, and neither can be caused with a real model (setup section 8c). Record `BLOCKED` with that reason and do not run the steps.

1. Approve a scout task (REPORT, setup section 8b). Stay on `manual-test-project`.
2. Wait for the relay and its one retry. Read the **Global orchestrator** badge.
3. Choose **Global orchestrator** and read the notice.

**Expected:**

- Step 3: a notice reads "Report from task `<task id>` is ready; automatic relay failed. Open the task to read it." Under it reads "The automatic reply failed." The notice has an **Open task** link.
- Step 2: the **Global orchestrator** entry has the badge "1 update".
- Only two wake turns ran for this report.
