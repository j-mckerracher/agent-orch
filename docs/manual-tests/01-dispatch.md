# 01 Dispatch: the Global Orchestrator dispatches agents to do work

Setup: [00-setup.md](00-setup.md). Everything here happens in the **Workspace** page with the **Global** target unless a step says otherwise.

How to make the Global agent propose a task: write the action block into `next-reply.txt` (see setup), then send any message. The block is one `task.propose` action with `project`, `objective`, `agent`, `task_kind`, and `delivery`. Use project `manual-test-project`, agent `codex`, `task_kind` `Implement`, `delivery` `ReportOnly`, and objective `Add a README line`, unless a test says otherwise.

---

## TC-D-01 Dispatch defaults are saved
**Source:** `docs/settings.md` "Agent dispatch defaults".
**Preconditions:** App running with a real daemon.
1. Click **Settings** in the sidebar.
2. Find the group **Agent dispatch defaults**.
3. Select Codex as the default provider. Select a model and an effort from the lists.
4. Click the **Save** button in that group. **Screenshot.**
5. Quit the app. Wait two minutes. Start the app again (setup section 3).
6. Open Settings.

**Expected:** The default provider, model, and effort are the values you saved. No "Fixture data" text.

## TC-D-02 Default agent order when no agent is named
**Source:** `docs/chat.md` (Settings default agent, then Codex, then Claude Code).
**Preconditions:** TC-D-01 saved Codex. `next-reply.txt` has a `task.propose` block with no `agent` field.
1. Open Workspace. Choose **Global** in the target picker.
2. Type `Run a task` in the message box and click **Send message to Global**.
3. Wait for the approval card.

**Expected:** The card shows a task for `manual-test-project`. The agent shown is Codex.

## TC-D-03 Explicit agent beats the default
**Source:** `docs/settings.md` ("Explicit task choices take precedence").
**Preconditions:** Default provider is Codex. Claude Code is installed or its program is set to a fake.
1. Put a `task.propose` block with `agent` `claude-code` in `next-reply.txt`.
2. Send `Run a task with Claude` to Global.
3. Read the approval card.

**Expected:** The card names Claude Code, not Codex. If Claude Code is unavailable the card or a failure message says so and nothing is run with Codex. Mark `BLOCKED` only if neither case can be set up.

## TC-D-04 Task proposal shows an approval card
**Source:** `docs/design/orchestrator-actions.md` N1.
**Preconditions:** Clean Global conversation.
1. Put the standard `task.propose` block in `next-reply.txt`.
2. Send `Please add a README line` to Global.
3. Wait for the reply.

**Expected:**
- The reply appears in the conversation.
- A section named "Waiting for your decision" appears with one action card.
- The card has an **Approve** button and a **Reject** button. Their accessible names start with `Approve:` and `Reject:`.
- No task has started. Open **Agents** and confirm no new task is running.

**Screenshot.**

## TC-D-05 Approving a proposal starts the task
**Source:** same.
**Preconditions:** TC-D-04 card is pending. `task-mode` contains `succeed`.
1. In the card, click **Approve**.
2. Watch the card.
3. Open **Agents**.

**Expected:**
- The card shows "Approved, running" and then a finished state.
- A task for `manual-test-project` is listed on the Agents page with the objective `Add a README line`.

**Screenshot** after steps 2 and 3.

## TC-D-06 Rejecting a proposal changes nothing
**Source:** same.
**Preconditions:** A pending task card.
1. Click **Reject** on the card.
2. Open **Agents**.

**Expected:**
- The card shows "Rejected" and the text "You rejected this. Nothing was changed."
- No new task is listed on the Agents page.

## TC-D-07 Dependent cards must be approved in order
**Source:** `actionCardModel.ts` ("Approve “X” first.").
**Preconditions:** `next-reply.txt` has two actions: a `workstream.create` for `Order test` and a `task.propose` that names that workstream as its `workstream_ref`.
1. Send `Make a workstream and a task in it` to Global.
2. Look at the second card before you click anything.
3. Click **Approve** on the first card.
4. Look at the second card again.

**Expected:**
- In step 2, the second card cannot be approved. It says `Approve “Order test” first.` (the name of the first card).
- In step 4, the second card can be approved.

## TC-D-08 Home directory is used when the project is unclear
**Source:** `docs/settings.md` "Home directory".
**Preconditions:** Home directory is set to `~/agent-orch-manual-test-project` (setup section 5). `next-reply.txt` has a `task.propose` with no `project`.
1. Send `Do a task, no project` to Global.
2. Read the card.
3. Click **Approve**.
4. Open **Agents** and open the task.

**Expected:**
- The card names the home directory as where the task runs.
- Delivery is ReportOnly.
- The task details show no worktree.

## TC-D-09 No home directory: the orchestrator asks where to run
**Source:** `docs/settings.md` "Home directory" ("the orchestrator then asks where to run instead").
**Preconditions:** TC-D-08 is done.
1. In Settings, clear the **Home directory** field and click **Save home directory**.
2. Use the same project-less `task.propose` and send `Do a task, no project` to Global.

**Expected:** No card for a home-directory task appears. Either the proposal is refused with an error shown in the card or the reply asks where to run. Quote what you saw.

## TC-D-10 Load gate closed: proposal card warns
**Source:** `docs/design/orchestrator-actions.md` "Load gate"; `docs/settings.md` "Machine load limits".
**Preconditions:** Settings **Machine load limits**: CPU limit set to `50`. Generate CPU load above 50 percent for 60 seconds, for example by running `yes > /dev/null` in four terminals. Stop it after the test.
1. Send a message that makes Global propose the standard task.
2. Read the card.

**Expected:** The card says "The machine is busy. This will queue until load drops."

## TC-D-11 Load gate: task waits, then runs
**Source:** same.
**Preconditions:** TC-D-10 load still on.
1. Click **Approve** on the card.
2. Open **Agents** and read the task state.
3. Stop the CPU load. Wait up to three minutes.

**Expected:**
- After step 2 the task is queued with text like "Waiting for machine capacity: CPU 91 percent, limit 50". The numbers vary.
- After step 3 the task starts.

Set the CPU limit back to `80` afterward and click **Save**.

## TC-D-12 Cancel, revise, and remove project always need approval
**Source:** `docs/settings.md` "Actions that always need approval".
**Preconditions:** A running task (`task-mode` is `hang`). Create a standing approval first (TC-D-13 steps 1 to 4) that tries to cover all action kinds.
1. Put a `task.cancel` block for the running task in `next-reply.txt`. Send `Cancel it` to Global.
2. Read the card.

**Expected:** The card is pending and asks for your decision. It does not show "Approved by a standing approval". Repeat with `task.revise` and `project.remove`. Each stays pending.

## TC-D-13 Standing approval auto-approves a granted action
**Source:** `docs/settings.md` "Standing approvals"; `orchestrator-actions.md` ("Standing approval: 3 of 20 used").
**Preconditions:** None.
1. Open Settings. Find **Standing approvals**.
2. Create a standing approval for `task.propose` in project `manual-test-project`. Set the count limit to `3`.
3. Save it.
4. Go to Workspace and Global. Send a message that makes Global propose the standard task.
5. Read the card.

**Expected:**
- The card shows "Approved by a standing approval" followed by details. No click was needed.
- A chip reads "Standing approval: 1 of 3 used" (the numbers match your setting).
- `workstream.create` is not offered as a grantable action in the creation form.

## TC-D-14 Proposal states
**Source:** `docs/design/orchestrator-actions.md` "Proposal states" (pending, approved, executed, failed, rejected, superseded, expired).
**Preconditions:** Pending task card.
1. Without clicking the card, make Global propose a second change for the same workstream.
2. Read the first card.

**Expected:** The older card is marked superseded, or, for a stale change, shows "A newer change to this workstream exists. Ask the orchestrator to propose again." Quote the text. Mark `BLOCKED` if the state cannot be reached and say why.

## TC-D-15 Project conversations do not propose
**Source:** `docs/chat.md` ("Project conversations do not propose actions").
**Preconditions:** A coordinator exists for `manual-test-project`. If not, use the add control in the target picker menu ("Add manual-test-project").
1. Choose `manual-test-project` in the target picker. The header reads "Talking to: manual-test-project".
2. Put a `task.propose` block in `next-reply.txt`. Send `Run a task`.

**Expected:** No approval card appears in the project conversation.

## TC-D-16 Unavailable agent is refused, not replaced
**Source:** `docs/settings.md` ("An unavailable saved selection is refused"); `docs/m7-08-conversation-service.md` (`runtime_unavailable`).
**Preconditions:** None.
1. Quit the app. Set `AGENT_ORCH_CODEX_PROGRAM` to a path that does not exist. Start the app.
2. In Global, send `Hello`.

**Expected:** The turn fails. The conversation shows a failure message, not a reply from another agent. Quote the message. No silent use of Claude Code.
