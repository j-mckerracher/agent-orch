# 03 Sub-orchestrators: the Global Orchestrator starts sub-orchestrators

In the docs a sub-orchestrator is a **workstream conversation**. Global creates one with a `workstream.create` proposal. See the README for the mapping.

Setup: [00-setup.md](00-setup.md). Run these tests with **Let the orchestrator act on its own** off (setup section 6, step 5). With it on, a `workstream.create` from your message runs at once and its card reads "Done on its own" (TC-A-02).

Every step that sends a message and expects a card is **Model-dependent** (RUNNER.md rule 8). To reset a missed attempt, reject any card the reply made. Workstream conversations use the same Model settings as Global: check the chip shows Claude Code, `Sonnet (alias)` and `medium` before you send.

---

## TC-S-01 Global proposes a workstream

**Source:** `docs/design/orchestrator-actions.md` "Secondmates and the "sub-orchestrator" request".
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** No workstream named `Post-MVP work` exists.

1. Open **Workspace** and choose **Global orchestrator** in the target picker.
2. Send this message:

   ```text
   Propose exactly one action and nothing else: workstream.create with title "Post-MVP work", summary "Everything after the MVP" and ref "a".
   ```

3. Wait for the reply.

**Expected:** An approval card headed "Proposed: Create workstream" appears below the Global reply that proposed it, with **Approve** and **Reject**. Open **Workstreams** and confirm no new workstream exists yet. **Screenshot.**

## TC-S-02 Approving creates the workstream

**Source:** `docs/design/orchestrator-actions.md` "Secondmates and the "sub-orchestrator" request".
**Real agent:** none; approval runs no turn. Cost: low.
**Preconditions:** TC-S-01 card pending.

1. Click **Approve** on the card.
2. Open **Workstreams** from the sidebar.

**Expected:**

- The card shows "Approved, running", then "Created".
- `Post-MVP work` is listed by its title. Its summary shows on its detail page.

**Screenshot.**

## TC-S-03 Rejecting creates nothing

**Source:** `docs/design/orchestrator-actions.md` "Secondmates and the "sub-orchestrator" request".
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** Send this message to Global. A pending card for `Rejected stream` appears.

```text
Propose exactly one action and nothing else: workstream.create with title "Rejected stream", summary "This one is rejected" and ref "a".
```

1. Click **Reject**.
2. Open **Workstreams**.

**Expected:** "You rejected this. Nothing was changed." No workstream `Rejected stream`.

## TC-S-05 Global routes a task to the workstream

**Source:** `docs/design/orchestrator-actions.md` "Secondmates and the "sub-orchestrator" request" (Phase 6a paragraph); `docs/m7-08-conversation-service.md` "Task proposals (DPES-1105, phase 2)".
**Real agent:** Sonnet (alias), medium, for the chat turn and the task run. Cost: medium.
**Preconditions:** TC-S-02 done.

1. Choose **Global orchestrator**.
2. Send this message:

   ```text
   Propose exactly one action and nothing else: task.propose for project manual-test-project in the workstream "Post-MVP work" (give its workstream_id), task_kind Implement, delivery ReportOnly, objective "Read README.md. Do not create, change or delete any file."
   ```

3. Approve the card.

**Expected:** The task starts. The card names the workstream. The task changes no file, so it then ends `Failed` with "The agent exited without changing any files." That outcome does not fail this test.

## TC-S-06 The workstream shows the routed task

**Source:** `apps/desktop/src/ui/views/workspace/routedInModel.ts` (:5-14); `apps/desktop/src/ui/views/workspace/RoutedInStrip.tsx`.
**Real agent:** Sonnet (alias), medium, for the chat turn and three task runs. Cost: medium.
**Preconditions:** TC-S-05 done.

1. Choose `Post-MVP work orchestrator` in the target picker.
2. Look for the strip below the transcript and above the composer. Open **Technical details** to read its help text.
3. Choose **Global orchestrator** and send this message, then approve all three cards:

   ```text
   Propose exactly three actions and nothing else, each a task.propose for project manual-test-project in the workstream "Post-MVP work" (give its workstream_id), task_kind Investigate, delivery ReportOnly, with these objectives in order: "Route test 2: Read README.md and report its first line. Change no file.", "Route test 3: Read README.md and report its first line. Change no file.", "Route test 4: Read README.md and report its first line. Change no file."
   ```

4. Choose `Post-MVP work orchestrator` again and read the strip.

**Expected:**

- Step 2: a section titled "Routed here by the global orchestrator", with a row reading `<taskId> (<title>), <state>, <time>`. Under **Technical details**, the help text reads "This list is built from approved proposals. Clearing the global conversation removes its rows."
- Step 4: no more than 3 rows, although four tasks were routed.

**Screenshot** after step 4.

## TC-S-07 Workstream conversation cannot create workstreams

**Source:** `docs/m7-08-conversation-service.md` "Scope in a workstream conversation" (`workstream.create` only from Global).
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** Chosen target is `Post-MVP work orchestrator`.

1. Send this message:

   ```text
   Propose exactly one action and nothing else: workstream.create with title "Nested stream", summary "Should be refused" and ref "b".
   ```

**Expected:** No create card appears, or the action is refused. A refusal shows "The orchestrator's action block in this reply could not be read. No action was proposed. Nothing was changed." No new entry in **Workstreams**. Both outcomes pass, so this step is not model-dependent. Quote what you saw.

## TC-S-08 Workstream tasks stay in the workstream

**Source:** `docs/m7-08-conversation-service.md` "Scope in a workstream conversation" (`task.propose` must name the workstream by id; `task.cancel` and `task.retry` only for that workstream's tasks).
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** A second workstream `Other` with a held task. In Global, send the first message below and approve it. Start the hold helper for `HOLD-2` (setup section 8c), send the second message, approve it, and stop the helper when the task reads `Running`.

```text
Propose exactly one action and nothing else: workstream.create with title "Other", summary "A second workstream" and ref "a".
```

```text
Propose exactly one action and nothing else: task.propose for project manual-test-project in the workstream "Other" (give its workstream_id), task_kind Investigate, delivery ReportOnly, objective "HOLD-2: Read README.md and report its first line. Change no file."
```

1. Choose `Post-MVP work orchestrator`. Send this message:

   ```text
   Propose exactly one action and nothing else: task.cancel for the task whose objective starts with "HOLD-2:".
   ```

**Expected:** The action is refused, or no card appears. The `HOLD-2` task keeps running. Both outcomes pass. Quote what you saw.

**Reset:** release `HOLD-2`, then click **Cancel** on its project task page.

## TC-S-09 Workstreams count as work for check timing

**Source:** `docs/settings.md` "Checks on running work" ("Global counts running tasks plus active workstreams").
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** **Settings** > **Limits** > **Checks on running work** at the defaults (setup section 6, step 4): "Check interval with one task running (minutes)" `10`, "Fastest check interval (seconds)" `60`. Prompt log running. A held task in `Post-MVP work`: start the hold helper for `HOLD-3`, send this message to Global, approve it, and stop the helper when the task reads `Running`.

```text
Propose exactly one action and nothing else: task.propose for project manual-test-project in the workstream "Post-MVP work" (give its workstream_id), task_kind Investigate, delivery ReportOnly, objective "HOLD-3: Read README.md and report its first line. Change no file."
```

1. Note the time. Leave mode **On**.
2. Watch Global for scheduled wake checks for 15 minutes. **Prompt log:** note the time of each turn entry.

**Expected:** With the running task plus the active workstreams, checks come about every 5 minutes, not 10. Allow a few minutes of tolerance. Mark `BLOCKED` if the schedule cannot be observed in 15 minutes.

**Reset:** release `HOLD-3`, then click **Cancel** on its project task page.

## TC-S-11 Archived workstream refuses updates

**Source:** `docs/m7-08-conversation-service.md` "Action approval and execution (DPES-1105, PR 1c)" (failure code `conflict` for an archived workstream).
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** Archive `Post-MVP work`: open it in **Workstreams**, then **Actions** > **Archive workstream** > **Confirm archive (cannot be undone)**.

1. In Global, send this message, then approve the card:

   ```text
   Propose exactly one action and nothing else: workstream.update for the workstream "Post-MVP work" with title "Post-MVP renamed".
   ```

**Expected:** The card shows the "Failed" pill and the text "The workstream is not in a state that allows this change, for example it is archived. Nothing was changed." The title does not change. If the model declines to propose an update to an archived workstream, count a miss (RUNNER.md rule 8).
