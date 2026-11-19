# 01 Dispatch: the Global Orchestrator dispatches agents to do work

Setup: [00-setup.md](00-setup.md). Everything here happens in the **Workspace** page with the **Global** target unless a step says otherwise.

These tests check the approval card. Run them with **Let the orchestrator act on its own** off (setup section 6, step 5). With it on, a task from your message starts without a click; `07-authority.md` tests that.

**Standard task message.** Most tests send this message to make Global propose one task. Its objective is NO-CHANGE (setup section 8b), so an approved task ends `Failed` with "The agent exited without changing any files." That outcome does not fail these tests.

```text
Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Implement, delivery ReportOnly, no agent field, objective "Read README.md. Do not create, change or delete any file."
```

Every step that sends a message and expects a card is **Model-dependent** (RUNNER.md rule 8). To reset a missed attempt, reject any card the reply made.

In a local-only project, `ReportOnly` is below the project's default delivery, so every such task card also has the row "Below this project's default" reading "Project default: local review. This task: report only." It does not change these tests.

---

## TC-D-01 Dispatch defaults are saved

**Source:** `docs/settings.md` "Agent dispatch defaults".
**Real agent:** none; no turn runs. Cost: low.
**Preconditions:** App running with a real daemon.

1. Click **Settings** in the sidebar.
2. Find the group **Agent dispatch defaults**.
3. Check that **Default provider** is `Claude Code`, **Claude Code model** is `Sonnet (alias)` and **Claude Code effort** is `medium` (setup section 6).
4. Click **Save dispatch defaults**. **Screenshot.**
5. Quit the app. Wait two minutes. Start the app again (setup section 4).
6. Open Settings.

**Expected:** The default provider, model, and effort are the values you saved. No "Fixture data" text.

## TC-D-02 Default agent order when no agent is named

**Source:** `docs/chat.md` (Settings default agent, then Codex, then Claude Code).
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** TC-D-01 saved Claude Code as the default provider.

1. Open Workspace. Choose **Global** in the target picker.
2. Type the standard task message in the message box and click **Send message to Global**.
3. Wait for the approval card.

**Expected:** The card shows a task for `manual-test-project`. The agent shown is Claude Code, the Settings default.

**Reset:** click **Reject**.

## TC-D-03 Explicit agent beats the default

**Source:** `docs/settings.md` ("Explicit task choices take precedence").
**Real agent:** Sonnet (alias), medium, for the chat turn. Cost: low. Codex only appears on the card; do not approve it.
**Preconditions:** Default provider is Claude Code.

1. Send this message to Global:

   ```text
   Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Implement, delivery ReportOnly, agent codex, objective "Read README.md. Do not create, change or delete any file."
   ```

2. Read the approval card.

**Expected:** The card names Codex, not Claude Code. If Codex is not installed, the card or a failure message says so and nothing is run with Claude Code. Mark `BLOCKED` only if neither case can be seen.

**Reset:** click **Reject**.

## TC-D-04 Task proposal shows an approval card

**Source:** `docs/design/orchestrator-actions.md` N1.
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** No pending card in Global.

1. Send the standard task message to Global.
2. Wait for the reply.

**Expected:**

- The reply appears in the conversation.
- A section named "Waiting for your decision" appears with one action card.
- The card has an **Approve** button and a **Reject** button. Their accessible names start with `Approve:` and `Reject:`.
- No task has started. Open **Agents** and confirm no new task is running.

**Screenshot.**

## TC-D-05 Approving a proposal starts the task

**Source:** same.
**Real agent:** Sonnet (alias), medium, for the task run. Cost: medium.
**Preconditions:** TC-D-04 card is pending.

1. In the card, click **Approve**.
2. Watch the card until the task is finished (RUNNER.md rule 7).
3. Open **Agents**.

**Expected:**

- The card shows "Approved, running" and then a finished state.
- The task changes no file, so it ends `Failed`. Its failure reason reads "The agent exited without changing any files." This does not fail the test; the test checks only that approval starts the task.
- A task for `manual-test-project` is listed on the Agents page with the objective `Read README.md. Do not create, change or delete any file.`

**Screenshot** after steps 2 and 3.

## TC-D-06 Rejecting a proposal changes nothing

**Source:** same.
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** Send the standard task message to Global. A pending task card appears.

1. Click **Reject** on the card.
2. Open **Agents**.

**Expected:**

- The card shows "Rejected" and the text "You rejected this. Nothing was changed."
- No new task is listed on the Agents page.

## TC-D-07 Dependent cards must be approved in order

**Source:** `actionCardModel.ts` ("Approve “X” first.").
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** None.

1. Send this message to Global:

   ```text
   Propose exactly two actions and nothing else, in this order: first workstream.create with title "Order test", summary "Card order check" and ref "a"; then task.propose for project manual-test-project with workstream_ref "a", task_kind Implement, delivery ReportOnly, objective "Read README.md. Do not create, change or delete any file."
   ```

2. Look at the second card before you click anything.
3. Click **Approve** on the first card.
4. Look at the second card again.

**Expected:**

- In step 2, the second card cannot be approved. It says `Approve “Order test” first.` (the name of the first card).
- In step 4, the second card can be approved.

**Reset:** click **Reject** on the second card. Keep the workstream `Order test` for TC-D-14.

## TC-D-08 Home directory is used when the project is unclear

**Source:** `docs/settings.md` "Home directory".
**Real agent:** Sonnet (alias), medium, for the chat turn and the task run. Cost: medium.
**Preconditions:** Home directory is set to `~/agent-orch-manual-test/projects/manual-test-project` (setup section 6).

1. Send this message to Global:

   ```text
   Propose exactly one action and nothing else: task.propose with no project field, task_kind Investigate, delivery ReportOnly, objective "Read README.md and report its first line. Change no file."
   ```

2. Read the card.
3. Click **Approve**.
4. Open **Agents** and open the task.

**Expected:**

- The card names the home directory as where the task runs.
- Delivery is ReportOnly.
- The task details show no worktree.

## TC-D-09 No home directory: the orchestrator asks where to run

**Source:** `docs/settings.md` "Home directory" ("the orchestrator then asks where to run instead").
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** TC-D-08 is done.

1. In Settings, clear the **Home directory** field and click **Save home directory**.
2. Send the message of TC-D-08 step 1 to Global again.

**Expected:** No card for a home-directory task appears. Either the proposal is refused with an error shown in the card or the reply asks where to run. Quote what you saw. Both outcomes pass, so this step is not model-dependent.

**Reset:** enter `~/agent-orch-manual-test/projects/manual-test-project` again and click **Save home directory**.

## TC-D-10 Load gate closed: proposal card warns

**Source:** `docs/design/orchestrator-actions.md` "Load gate"; `docs/settings.md` "Machine load limits".
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** Settings **Machine load limits**: CPU limit set to `50`. Generate CPU load above 50 percent for 60 seconds, for example by running `yes > /dev/null` in four terminals. Stop it in TC-D-11.

1. Send the standard task message to Global.
2. Read the card.

**Expected:** The card says "The machine is busy. This will queue until load drops."

## TC-D-11 Load gate: task waits, then runs

**Source:** same.
**Real agent:** Sonnet (alias), medium, for the task run. Cost: medium.
**Preconditions:** TC-D-10 load still on.

1. Click **Approve** on the card.
2. Open **Agents** and read the task state.
3. Stop the CPU load. Wait up to three minutes.

**Expected:**

- After step 2 the task is queued with text like "Waiting for machine capacity: CPU 91 percent, limit 50". The numbers vary.
- After step 3 the task starts. It then ends `Failed` as in TC-D-05, which does not fail this test.

**Reset:** set the CPU limit back to `80` and click **Save**.

## TC-D-12 Cancel, revise, and remove project always need approval

**Source:** `docs/settings.md` "Actions that always need approval".
**Real agent:** Sonnet (alias), medium. Cost: low; the held task does no work.
**Preconditions:** Create a standing approval first (TC-D-13 steps 1 to 3) that tries to cover all action kinds. Then start a held task (setup section 8c): run `sh docs/manual-tests/manual-test.sh hold HOLD-1`, send the message below to Global, click **Approve**, and wait until the task page reads `Running`.

```text
Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Investigate, delivery ReportOnly, objective "HOLD-1: Read README.md and report its first line. Change no file."
```

1. Send this message to Global:

   ```text
   Propose exactly one action and nothing else: task.cancel for the task whose objective starts with "HOLD-1:".
   ```

2. Read the card.
3. Send this message, then read the card:

   ```text
   Propose exactly one action and nothing else: task.revise for the task whose objective starts with "HOLD-1:", with added_intent "Also add a heading".
   ```

4. Send this message, then read the card:

   ```text
   Propose exactly one action and nothing else: project.remove for project manual-test-project.
   ```

**Expected:** Each of the three cards is pending and asks for your decision. None shows "Approved by a standing approval".

**Reset:** click **Reject** on each card. Run `sh docs/manual-tests/manual-test.sh release HOLD-1` and click **Request cancel** on the task page. Delete the standing approval.

## TC-D-13 Standing approval auto-approves a granted action

**Source:** `docs/settings.md` "Standing approvals"; `orchestrator-actions.md` ("Standing approval: 3 of 20 used").
**Real agent:** Sonnet (alias), medium, for the chat turn and the task run. Cost: medium.
**Preconditions:** Acting on its own is off. With it on, the task starts on its own first and the grant is never used.

1. Open Settings. Find **Standing approvals**.
2. Create a standing approval for `task.propose` in project `manual-test-project`. Set the count limit to `3`.
3. Save it.
4. Go to Workspace and Global. Send the standard task message.
5. Read the card.

**Expected:**

- The card shows "Approved by a standing approval" followed by details. No click was needed.
- A chip reads "Standing approval: 1 of 3 used" (the numbers match your setting).
- `workstream.create` is not offered as a grantable action in the creation form.

**Reset:** delete the standing approval.

## TC-D-14 Proposal states

**Source:** `docs/design/orchestrator-actions.md` "Proposal states" (pending, approved, executed, failed, rejected, superseded, expired).
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** The workstream `Order test` exists (TC-D-07).

1. Send this message to Global and leave its card pending:

   ```text
   Propose exactly one action and nothing else: workstream.update for the workstream "Order test" with title "Order test renamed".
   ```

2. Send this message to Global:

   ```text
   Propose exactly one action and nothing else: workstream.update for the workstream "Order test" with title "Order test renamed again".
   ```

3. Read the first card.

**Expected:** The older card is marked superseded, or, for a stale change, shows "A newer change to this workstream exists. Ask the orchestrator to propose again." Quote the text. Mark `BLOCKED` if neither state appears and say why.

**Reset:** reject any pending card.

## TC-D-15 A project coordinator proposes actions in its project

**Source:** `docs/chat.md` "Work with a project coordinator" ("It proposes actions like Global does, but only inside its project"). This replaced the earlier rule that project conversations do not propose.
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** Acting on its own is off. Add a coordinator for `manual-test-project`: open the target picker, choose **Add coordinator…**, and click **Add** next to `manual-test-project` in the "Add a coordinator" dialog.

1. Choose `manual-test-project` in the target picker. The header reads "Talking to: manual-test-project". Check that its Model settings chip shows Claude Code, `Sonnet (alias)` and `medium`.
2. Send the standard task message.

**Expected:** A card "Proposed: Create task" for `manual-test-project` appears under the reply, with **Approve** and **Reject**. Nothing runs until you approve. A message that names another project, or a `workstream.create`, is not accepted: see TC-C-04 in `09-coordinators.md`.

**Reset:** click **Reject**.

## TC-D-16 Unavailable agent is refused, not replaced

**Source:** `docs/settings.md` ("An unavailable saved selection is refused"); `docs/m7-08-conversation-service.md` (`runtime_unavailable`).
**Real agent:** none can run; that is the test. Cost: low.
**Preconditions:** No task is running.

1. Quit the app. Start it with the unavailable-agent trigger (setup section 8c): `AGENT_ORCH_CLAUDE_PROGRAM` points to a path that does not exist.
2. In Global, send `Hello`.

**Expected:** The turn fails. The conversation shows a failure message, not a reply from another agent. Quote the message. No silent use of Codex.

**Reset:** quit the app, wait two minutes, and start it as in setup section 4.
