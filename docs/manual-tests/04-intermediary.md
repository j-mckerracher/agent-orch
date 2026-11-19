# 04 Intermediary: the Global Orchestrator sits between the user, sub-orchestrators, and workers

The docs describe this role through these surfaces:

- The conversation target picker (user chooses who to talk to).
- Approval cards (Global asks the user before it acts).
- Wake notices and relayed scout reports (workers' results reach the user through Global).
- The "Routed here by the global orchestrator" strip (Global's routing is visible in the workstream).

The docs do not say that Global forwards chat messages to workers. These tests do not expect that. Questions for the user (`05-decisions.md`) and coordinator reports (`09-coordinators.md`) are two more of these surfaces.

Setup: [00-setup.md](00-setup.md). Run these tests with **Let the orchestrator act on its own** off (setup section 6, step 5).

Every step that sends a message and expects a card is **Model-dependent** (RUNNER.md rule 8). To reset a missed attempt, reject any card the reply made. Copy every reply into the evidence folder (RUNNER.md rule 9).

**Messages used here.** Send them word for word.

- **Workstream message:**

  ```text
  Propose exactly one action and nothing else: workstream.create with title "Post-MVP work", summary "Everything after the MVP" and ref "a".
  ```

- **Failing workstream task message** (NO-CHANGE, setup section 8b):

  ```text
  Propose exactly one action and nothing else: task.propose for project manual-test-project in the workstream "Post-MVP work" (give its workstream_id), task_kind Implement, delivery ReportOnly, objective "Read README.md. Do not create, change or delete any file."
  ```

- **Failing task message** (NO-CHANGE, no workstream):

  ```text
  Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Implement, delivery ReportOnly, objective "Read README.md. Do not create, change or delete any file."
  ```

---

## TC-I-01 The picker lists every target

**Source:** `docs/chat.md` (Global, project coordinators, workstreams).
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** Add a coordinator for `manual-test-project` (target picker, **Add coordinator…**, **Add**). In Global, send the workstream message and approve the card.

1. Click the target picker.
2. Read the list.

**Expected:** The list shows Global, `manual-test-project`, and `Post-MVP work`. The header always reads "Talking to:" followed by the chosen name. **Screenshot.**

## TC-I-02 The user can talk to a workstream directly

**Source:** `docs/chat.md`.
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** TC-I-01.

1. Choose `Post-MVP work`. Type `Status? Reply in one sentence and propose no action.` in the box "Message Post-MVP work". Click **Send message to Post-MVP work**.

**Expected:** The message and a reply appear in the workstream conversation. Nothing appears in the Global conversation.

## TC-I-03 A project coordinator answers

**Source:** `docs/chat.md` ("Project coordinators answer through the same runtime"; "A message you send in its conversation is answered there").
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** TC-I-01.

1. Choose `manual-test-project`. Send `Status? Reply in one sentence and propose no action.`

**Expected:** A reply appears in the coordinator's conversation. No card appears. Nothing appears in the Global conversation. If the reply proposed a card anyway, count a miss (RUNNER.md rule 8).

## TC-I-04 A ready scout report wakes Global

**Source:** `docs/m7-08-conversation-service.md` "reports" (automatic relay).
**Real agent:** Sonnet (alias), medium, for the chat turns and the scout run. Cost: medium.
**Preconditions:** Mode **On**.

1. In Global, send this message and approve the card:

   ```text
   Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Investigate, delivery ReportOnly, objective "Change no file. End your final message with this exact line: Found /Users/test/secret/file.txt and sk-abcdefghijklmnopqrstuvwxyz123456"
   ```

2. Switch to another target. Wait until the task is `Succeeded`, then up to 5 minutes.
3. Choose **Global**.

**Expected:** A wake notice for the report appears with an update badge. The reply text is from the model, not a raw dump. **Screenshot.**

**Model-dependent:** if the worker's report does not contain the line, TC-I-05 has nothing to check; count a miss.

## TC-I-05 Relayed copy is redacted

**Source:** same (paths become `[path]`, token-like strings are masked).
**Real agent:** none beyond TC-I-04. Cost: low.
**Preconditions:** TC-I-04.

1. Open the relayed report content from the notice, or the model's quoted text.

**Expected:** `[path]` replaces `/Users/test/secret/file.txt`. The token is masked, not shown. Note: the docs say email and IP addresses may survive. Do not fail on those.

## TC-I-06 Manual relay is available after a failed relay

**Source:** same; `docs/settings.md` ("failed or skipped updates leave the manual relay available").
**Real agent:** none. Cost: none.
**Preconditions:** `BLOCKED`. It needs the automatic relay to fail twice, and no trigger makes automatic turns fail with a real agent (setup section 8c). Record `BLOCKED` with that reason and do not run the steps.

1. Wait for the relay notice. The automatic relay is retried about a minute after the first failure. After the second failure the notice reads "Report from task `<task id>` is ready; automatic relay failed. Open the task to read it.", with "The automatic reply failed." under it.
2. Find the relay action on the notice and click it.

**Expected:** A relay button or link exists on that notice. Clicking it queues a Global turn with the report. The report is not marked relayed until a reply succeeds.

## TC-I-07 Only a successful, non-quiet reply marks the report relayed

**Source:** same (`relayed_at`).
**Real agent:** none. Cost: none.
**Preconditions:** `BLOCKED`. It needs quiet replies to a report relay, which a real model does not give on request (setup section 8c). Record `BLOCKED` with that reason and do not run the steps.

1. Let the automatic relay run. It gets a quiet reply, is retried about a minute later, and gets a quiet reply again.
2. Check the notice.

**Expected:** The notice reads "Report from task `<task id>` is ready; automatic relay failed. Open the task to read it." It does not read "Checked 1 update. Nothing needs you." The report is not marked relayed. A manual relay control stays available; after a manual relay that gets a normal reply, the control is gone, and a proposal made after that reply is labelled "Made after reading a report".

## TC-I-08 Approvals surface to the user as cards and notices

**Source:** `orchestrator-actions.md` (pending approvals are deterministic notices with a link).
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** In Global, send the failing task message. Leave its card pending. Switch to another target.

1. Wait for the reminder interval or the next wake, up to 15 minutes.
2. Look at the target picker and the Global conversation.

**Expected:** The Global entry shows a badge. A notice about a proposal waiting links to the card. The card is still pending. Nothing ran without your approval.

**Reset:** click **Reject**.

## TC-I-09 Worker events are reported up to Global

**Source:** `orchestrator-actions.md` (Phase 3 reports task events with a deterministic copy).
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** TC-I-01. In Global, send the failing workstream task message and approve the card.

1. Choose the `manual-test-project` target. Wait until the task has failed, then up to 5 minutes.
2. Choose **Global**.

**Expected:** Global shows a notice or wake reply about the failed workstream task. You did not have to open the workstream to learn of it.

## TC-I-10 End-to-end flow

**Source:** all of the above.
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** Clean data directory (setup section 9). Mode **On**.

1. In Global, send the workstream message. Approve the card.
2. In Global, send the failing workstream task message. Approve the task card (routed to the workstream).
3. Switch to the workstream. Confirm the "Routed here by the global orchestrator" strip lists the task.
4. Switch to `manual-test-project` (add its coordinator first if it is not in the picker). Wait until the task has failed, then up to 5 minutes.
5. Return to Global. Read the wake notice.
6. Send this message and approve the card:

   ```text
   Propose exactly one action and nothing else: task.retry for the failed task in the workstream "Post-MVP work".
   ```

7. Send this message and approve the card, then wait until the new task is finished:

   ```text
   Propose exactly one action and nothing else: task.propose for project manual-test-project in the workstream "Post-MVP work" (give its workstream_id), task_kind Implement, delivery LocalReview, objective "Add the line 'Manual test line.' at the end of README.md. Change no other file."
   ```

**Expected:**

- Each action needed your approval.
- The failure reached you in Global without a user message.
- The retry ran only after approval. It fails again, because it reruns the same objective; that does not fail this test.
- The task of step 7 reaches `Verifying` with a candidate.

**Screenshot** after steps 3, 5, and 7.

## TC-I-11 Wake and relay turns are Restricted

**Source:** `orchestrator-actions.md` (wake turns always run at Restricted); `docs/permission-levels.md`.
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** Acting on its own is off. Prompt log running. The Global permission control (Model settings chip) is at Auto, the default.

1. In Global, send the failing task message and approve the card. Wait until the task has failed and a wake notice appears.
2. Open the wake notice details if shown.
3. **Prompt log:** read the turn entry of the wake turn.

**Expected:** The automatic turn runs at Restricted, not at the level you chose. Proposals from it still need approval. Step 3: the entry has `--permission-mode dontAsk`, not `--permission-mode auto`.
