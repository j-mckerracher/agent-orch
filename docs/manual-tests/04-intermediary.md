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

**Source:** `docs/chat.md` "Choose who you talk to".
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** Add a coordinator for `manual-test-project` (target picker, **Add coordinator…**, **Add**). In Global, send the workstream message and approve the card.

1. Click the target picker.
2. Type `Post-MVP` in the search box "Search orchestrators and coordinators". Only `Post-MVP work orchestrator` stays in the list. Clear the box.
3. Read the full list.

**Expected:** The list shows `Global orchestrator`, `manual-test-project coordinator`, and `Post-MVP work orchestrator`. The picker button shows the chosen name. **Screenshot.**

## TC-I-02 The user can talk to a workstream directly

**Source:** `docs/chat.md` "Choose who you talk to".
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** TC-I-01.

1. Choose `Post-MVP work orchestrator`. Type `Status? Reply in one sentence and propose no action.` in the box "Message Post-MVP work orchestrator". Click the send button (glyph `↑`, labelled "Send message to Post-MVP work orchestrator").

**Expected:** The message and a reply appear in the workstream conversation. Nothing appears in the Global conversation.

## TC-I-03 A project coordinator answers

**Source:** `docs/chat.md` "Choose who you talk to" ("Project coordinators answer through the same agent runtime"); `docs/chat.md` "Work with a project coordinator" ("A message you send in its conversation is answered there").
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** TC-I-01.

1. Choose `manual-test-project coordinator`. Send `Status? Reply in one sentence and propose no action.` with the send button (glyph `↑`, labelled "Send message to manual-test-project coordinator").

**Expected:** A reply appears in the coordinator's conversation. No card appears. Nothing appears in the Global conversation. If the reply proposed a card anyway, count a miss (RUNNER.md rule 8).

## TC-I-04 A ready scout report wakes Global

**Source:** `docs/m7-08-conversation-service.md` "Project defaults, reports and holds (DPES-1105, phase 4)" (**Relay** item).
**Real agent:** Sonnet (alias), medium, for the chat turns and the scout run. Cost: medium.
**Preconditions:** Mode **On**.

1. In Global, send this message and approve the card:

   ```text
   Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Investigate, delivery ReportOnly, objective "Change no file. End your final message with this exact line: Found /Users/test/secret/file.txt and sk-abcdefghijklmnopqrstuvwxyz123456"
   ```

2. Switch to another target. Wait until the task is `Succeeded`, then up to 5 minutes.
3. Choose **Global orchestrator**.

**Expected:** A wake notice for the report appears with an update badge. The reply text is from the model, not a raw dump. **Screenshot.**

**Model-dependent:** if the worker's report does not contain the line, TC-I-05 has nothing to check; count a miss.

## TC-I-05 Relayed copy is redacted

**Source:** `docs/m7-08-conversation-service.md` "Project defaults, reports and holds (DPES-1105, phase 4)" (**Relay** item: paths become `[path]`, token-like words are redacted); `docs/design/orchestrator-actions.md` "Residual risks, stated honestly" (the email and IP caveat).
**Real agent:** none beyond TC-I-04. Cost: low.
**Preconditions:** TC-I-04.

1. Read the model's reply in the Global transcript, and any text it quotes from the report. The app has no report viewer.

**Expected:** `[path]` replaces `/Users/test/secret/file.txt`. The token is masked, not shown. Note: the docs say email and IP addresses may survive. Do not fail on those.

## TC-I-08 Approvals surface to the user as cards and notices

**Source:** `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" (event table, "Proposal waiting past a threshold"); the reminder timing is `PROPOSAL_WAIT_MS` in `crates/orchestrator-daemon/src/wake.rs`.
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** In Global, send the failing task message. Leave its card pending. Switch to another target.

1. Wait for the reminder. It comes about 30 minutes after the card was proposed. This test's own limit is 35 minutes. If no reminder appears by then, the result is `BLOCKED` with "no proposal notice in 35 minutes".
2. Look at the target picker and the Global conversation.

**Expected:** The Global entry shows a badge. A notice reads "A task.propose proposal is waiting for your decision." It has no **Open task** link. The card is still pending. Nothing ran without your approval.

**Reset:** click **Reject**.

## TC-I-09 Worker events are reported up to Global

**Source:** `docs/design/orchestrator-actions.md` "Secondmates and the "sub-orchestrator" request" (Phase 3 paragraph: task events reported up to Global with a deterministic copy).
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** TC-I-01. In Global, send the failing workstream task message and approve the card.

1. Choose the `manual-test-project coordinator` target. Wait until the task has failed, then up to 5 minutes.
2. Choose **Global orchestrator**.

**Expected:** Global shows the deterministic task notice only; no model reply follows. You did not have to open the workstream to learn of it.

## TC-I-10 End-to-end flow

**Source:** `docs/design/orchestrator-actions.md` "Secondmates and the "sub-orchestrator" request" (Phase 3 and Phase 6a paragraphs).
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** Clean data directory (setup section 9). Mode **On**.

1. In Global, send the workstream message. Approve the card.
2. In Global, send the failing workstream task message. Approve the task card (routed to the workstream).
3. Choose `Post-MVP work orchestrator`. Confirm the "Routed here by the global orchestrator" strip, below the transcript, lists the task.
4. Choose `manual-test-project coordinator` (add its coordinator first if it is not in the picker). Wait until the task has failed, then up to 5 minutes.
5. Choose **Global orchestrator**. Read the wake notice.
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

**Source:** `docs/design/orchestrator-actions.md` "Wake-up loop: the daemon is the watcher" (**Level** bullet: wake turns run at Restricted); `docs/permission-levels.md`.
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** The switch "Let the orchestrator act on its own" is off (Settings > Autonomy). Prompt log running. The Global permission control (Model settings chip) is at Auto, the default.

1. In Global, send the failing task message and approve the card. Wait until the task has failed and a wake notice appears.
2. Open the wake notice details if shown.
3. **Prompt log:** read the turn entry of the wake turn.

**Expected:** The automatic turn runs at Restricted, not at the level you chose. Proposals from it still need approval. Step 3: the entry has `--permission-mode dontAsk`, not `--permission-mode auto`.
