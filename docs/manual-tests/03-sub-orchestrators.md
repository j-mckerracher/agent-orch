# 03 Sub-orchestrators: the Global Orchestrator starts sub-orchestrators

In the docs a sub-orchestrator is a **workstream conversation**. Global creates one with a `workstream.create` proposal. See the README for the mapping.

Setup: [00-setup.md](00-setup.md).

How to make Global propose a workstream: put one `workstream.create` action in `next-reply.txt` with title `Post-MVP work` and a one-line summary `Everything after the MVP`. Then send a message to Global.

---

## TC-S-01 Global proposes a workstream

**Source:** `docs/design/orchestrator-actions.md` "Secondmates and the sub-orchestrator request".
**Preconditions:** No workstream named `Post-MVP work` exists.

1. Open Workspace and choose **Global**.
2. Send `Create a sub-orchestrator for post-MVP work`.
3. Wait for the reply.

**Expected:** An approval card for creating a workstream titled `Post-MVP work` appears under "Waiting for your decision". It has **Approve** and **Reject**. Open **Workstreams** and confirm no new workstream exists yet. **Screenshot.**

## TC-S-02 Approving creates the workstream

**Source:** same.
**Preconditions:** TC-S-01 card pending.

1. Click **Approve** on the card.
2. Open **Workstreams** from the sidebar.

**Expected:**

- The card shows "Approved, running" then done.
- `Post-MVP work` is listed with its summary.

**Screenshot.**

## TC-S-03 Rejecting creates nothing

**Source:** same.
**Preconditions:** A pending `workstream.create` card for `Rejected stream`.

1. Click **Reject**.
2. Open **Workstreams**.

**Expected:** "You rejected this. Nothing was changed." No workstream `Rejected stream`.

## TC-S-04 New workstream is a conversation target

**Source:** `docs/chat.md` (target picker lists Global, coordinators, and workstreams).
**Preconditions:** TC-S-02 done.

1. In Workspace, click the target picker (placeholder text "Search orchestrators and coordinators").
2. Type `Post-MVP`.
3. Choose `Post-MVP work`.

**Expected:** The picker lists `Post-MVP work`. The header reads "Talking to: Post-MVP work". The message box is labelled "Message Post-MVP work". **Screenshot.**

## TC-S-05 Global routes a task to the workstream

**Source:** `orchestrator-actions.md` Phase 6a; `docs/m7-08-conversation-service.md`.
**Preconditions:** TC-S-02 done. `task-mode` is `succeed`.

1. Choose **Global**.
2. Put a `task.propose` block with the workstream's id in `workstream_id` in `next-reply.txt`. Use the standard project and objective.
3. Send `Give the post-MVP stream a task`.
4. Approve the card.

**Expected:** The task starts. The card names the workstream.

## TC-S-06 The workstream shows the routed task

**Source:** `routedInModel.ts`.
**Preconditions:** TC-S-05 done.

1. Choose `Post-MVP work` in the target picker.
2. Look for a strip above the conversation.

**Expected:**

- A section titled "Routed here by the global orchestrator".
- A row reading `<taskId> (<title>), <state>, <time>`.
- Help text: "This list is built from approved proposals. Clearing the global conversation removes its rows."
- No more than 3 rows even after routing 4 tasks. Route four tasks and check.

**Screenshot.**

## TC-S-07 Workstream conversation cannot create workstreams

**Source:** `docs/m7-08-conversation-service.md` (workstream scope; `workstream.create` only from Global).
**Preconditions:** Chosen target is `Post-MVP work`.

1. Put a `workstream.create` block in `next-reply.txt`.
2. Send `Make another workstream`.

**Expected:** No create card appears, or the action is refused with an error. No new entry in **Workstreams**.

## TC-S-08 Workstream tasks stay in the workstream

**Source:** same (`task.propose` must name the workstream by id; `task.cancel` and `task.retry` only for that workstream's tasks).
**Preconditions:** Chosen target is `Post-MVP work`. A second workstream `Other` with its own task exists.

1. Put a `task.cancel` block for a task of `Other` in `next-reply.txt`.
2. Send `Cancel that task`.

**Expected:** The action is refused. The `Other` task keeps running.

## TC-S-09 Workstreams count as work for check timing

**Source:** `docs/settings.md` "Checks on running work" ("Global counts running tasks plus active workstreams").
**Preconditions:** Base interval `10`, fastest `1` (Settings). A workstream with one running task (`task-mode` `hang`).

1. Note the time. Leave mode **On**.
2. Watch Global for scheduled wake checks.

**Expected:** With Global's one running task plus the one workstream, the check comes about every 5 minutes, not 10. Allow a few minutes of tolerance. Mark `BLOCKED` if the schedule cannot be observed in 15 minutes.

## TC-S-10 Escalate to Global

**Source:** `docs/ui/wireframe.md` (chip "Escalate to Global", navigation only).
**Preconditions:** Chosen target is `manual-test-project` (a project coordinator). The chip shows only for project targets.

1. Find the button "Escalate to Global" under the conversation.
2. Try to click it.
3. Read the text beside it.

**Expected:** The button is disabled. The text reads "Escalate to Global is not available: the daemon has no operation for it." The target does not change and no message is sent. Links "Show dependency plan" and "Request review" are present and navigate to Workstreams and Review. If the button is enabled in a later build, expect the target to change to Global ("Talking to: Global") with no message sent.

## TC-S-11 Archived workstream refuses updates

**Source:** `docs/m7-08-conversation-service.md` (`workstream.update` on an archived workstream gives `conflict`).
**Preconditions:** Archive `Post-MVP work` from the **Workstreams** page.

1. In Global, put a `workstream.update` block with a new title in `next-reply.txt`.
2. Send `Rename it`. Approve the card.

**Expected:** The card ends in a failed state with a conflict message. The title does not change.
