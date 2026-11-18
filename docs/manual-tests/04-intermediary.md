# 04 Intermediary: the Global Orchestrator sits between the user, sub-orchestrators, and workers

The docs describe this role through these surfaces:
- The conversation target picker (user chooses who to talk to).
- Approval cards (Global asks the user before it acts).
- Wake notices and relayed scout reports (workers' results reach the user through Global).
- The "Routed here by the global orchestrator" strip (Global's routing is visible in the workstream).

The docs do not say that Global forwards chat messages to workers. These tests do not expect that.

Setup: [00-setup.md](00-setup.md).

---

## TC-I-01 The picker lists every target
**Source:** `docs/chat.md` (Global, project coordinators, workstreams).
**Preconditions:** A coordinator for `manual-test-project` and a workstream `Post-MVP work` exist.
1. Click the target picker.
2. Read the list.

**Expected:** The list shows Global, `manual-test-project`, and `Post-MVP work`. The header always reads "Talking to:" followed by the chosen name. **Screenshot.**

## TC-I-02 The user can talk to a workstream directly
**Source:** `docs/chat.md`.
**Preconditions:** TC-I-01.
1. Choose `Post-MVP work`. Type `Status?` in the box "Message Post-MVP work". Click **Send message to Post-MVP work**.

**Expected:** The message and a reply appear in the workstream conversation. Nothing appears in the Global conversation.

## TC-I-03 A project coordinator answers
**Source:** `docs/chat.md` ("Project coordinators answer through the same runtime").
**Preconditions:** TC-I-01.
1. Choose `manual-test-project`. Send `Status?`.

**Expected:** A reply appears. No approval card appears.

## TC-I-04 A ready scout report wakes Global
**Source:** `docs/m7-08-conversation-service.md` "reports" (automatic relay).
**Preconditions:** Mode **On**. `task-mode` is `succeed`. The fake agent returns a report containing the path `/Users/test/secret/file.txt` and the token-like string `sk-abcdefghijklmnopqrstuvwxyz123456`.
1. Make Global propose a `task.propose` with `task_kind` `Investigate`, `delivery` `ReportOnly`. Approve it.
2. Switch to another target. Wait up to two minutes.
3. Choose **Global**.

**Expected:** A wake notice for the report appears with an update badge. The reply text is from the model, not a raw dump. **Screenshot.**

## TC-I-05 Relayed copy is redacted
**Source:** same (paths become `[path]`, token-like strings are masked).
**Preconditions:** TC-I-04.
1. Open the relayed report content from the notice, or the model's quoted text.

**Expected:** `[path]` replaces `/Users/test/secret/file.txt`. The token is masked, not shown. Note: the docs say email and IP addresses may survive. Do not fail on those.

## TC-I-06 Manual relay is available after a failed relay
**Source:** same; `docs/settings.md` ("failed or skipped updates leave the manual relay available").
**Preconditions:** A ready report. Use `fake-codex-fail` so the automatic relay fails.
1. Wait for the failed wake notice ("The automatic reply failed.").
2. Find the relay action on the notice and click it.

**Expected:** A relay button or link exists after the failure. Clicking it queues a Global turn with the report. The report is not marked relayed until a reply succeeds. Mark `BLOCKED` if no relay control is visible and quote the notice text.

## TC-I-07 Only a successful, non-quiet reply marks the report relayed
**Source:** same (`relayed_at`).
**Preconditions:** Ready report. The fake agent answers with the quiet token.
1. Let the automatic relay run.
2. Check the notice.

**Expected:** The notice reads "Checked 1 update. Nothing needs you." The manual relay stays available. Then switch the fake agent to a normal reply and relay manually. The relay control is gone afterward. A proposal made after that reply is labelled "Made after reading a report".

## TC-I-08 Approvals surface to the user as cards and notices
**Source:** `orchestrator-actions.md` (pending approvals are deterministic notices with a link).
**Preconditions:** Leave a task card pending. Switch to another target.
1. Wait for the reminder interval or the next wake.
2. Look at the target picker and the Global conversation.

**Expected:** The Global entry shows a badge. A notice about a proposal waiting links to the card. The card is still pending. Nothing ran without your approval.

## TC-I-09 Worker events are reported up to Global
**Source:** `orchestrator-actions.md` (Phase 3 reports task events with a deterministic copy).
**Preconditions:** A task running inside the workstream `Post-MVP work` with `task-mode` `fail`.
1. Wait for the task to fail. Stay on the `manual-test-project` target.
2. Choose **Global**.

**Expected:** Global shows a notice or wake reply about the failed workstream task. You did not have to open the workstream to learn of it.

## TC-I-10 End-to-end flow
**Source:** all of the above.
**Preconditions:** Clean data directory (setup section 7). Mode **On**. `task-mode` `fail`.
1. In Global, send `Create a sub-orchestrator for post-MVP work`. Approve the card.
2. In Global, send `Give it a task`. Approve the task card (routed to the workstream).
3. Switch to the workstream. Confirm the "Routed here by the global orchestrator" strip lists the task.
4. Switch to `manual-test-project`. Wait for the task to fail and the wake debounce (60 seconds).
5. Return to Global. Read the wake notice.
6. Put a `task.retry` block in `next-reply.txt`. Send `Retry it`. Approve the card.
7. Write `succeed` to `task-mode`. Wait for the task to finish.

**Expected:**
- Each action needed your approval.
- The failure reached you in Global without a user message.
- The retry ran only after approval.
- The task finishes.

**Screenshot** after steps 3, 5, and 7.

## TC-I-11 Wake and relay turns are Restricted
**Source:** `orchestrator-actions.md` (wake turns always run at Restricted); `docs/permission-levels.md`.
**Preconditions:** Set the Global permission control (model picker) to the highest level, for example Auto.
1. Cause a wake event.
2. Open the wake notice details if shown.

**Expected:** The automatic turn runs at Restricted, not at the level you chose. Proposals from it still need approval. Mark `BLOCKED` if the level is not visible and the docs offer no UI for it.
