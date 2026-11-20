# 01 Dispatch: the Global Orchestrator dispatches agents to do work

Setup: [00-setup.md](00-setup.md). Everything here happens on the **Workspace** page with the **Global orchestrator** target unless a step says otherwise.

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

1. Click **Settings** in the sidebar. It opens on the **Agents** page.
2. Find **Default provider**, **Default model** and **Default effort**.
3. Check that **Default provider** is `Claude Code`, **Default model** is `Sonnet (alias)` and **Default effort** is `medium` (setup section 6).
4. Click **Save changes**. **Screenshot.**
5. Quit the app. Wait two minutes. Start the app again (setup section 4).
6. Open Settings.

**Expected:** The default provider, model, and effort are the values you saved. No "Fixture data" text.

## TC-D-02 Default agent is used when no agent is named

**Source:** `docs/settings.md` "Agent dispatch defaults" (the second paragraph: "then these defaults"); `crates/orchestrator-daemon/src/api_actions.rs` (`apply_dispatch_defaults`).
**Real agent:** Sonnet (alias), medium, for the chat turn and the task run. Cost: medium.
**Preconditions:** TC-D-01 saved Claude Code as the default provider.

1. Open Workspace. Choose **Global orchestrator** in the target picker.
2. Type the standard task message in the message box and click **Send message to Global orchestrator**.
3. Wait for the approval card. Read its **Agent** field.
4. Click **Approve**.
5. Open **Agents**, open the task, and open **Technical details**.

**Expected:**

- Step 3: the **Agent** field reads `(none)`. The card does not name the default.
- Step 5: the **Agent** row reads `Claude Code (claude-code)`.

**Reset:** none. The task ends `Failed`, as in TC-D-05.

## TC-D-03 Explicit agent beats the default

**Source:** `docs/settings.md` "Agent dispatch defaults" ("Explicit task choices take precedence").
**Real agent:** Sonnet (alias), medium, for the chat turn. Cost: low. Codex only appears on the card; do not approve it.
**Preconditions:** Default provider is Claude Code.

1. Send this message to **Global orchestrator**:

   ```text
   Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Implement, delivery ReportOnly, agent codex, objective "Read README.md. Do not create, change or delete any file."
   ```

2. Read the approval card.

**Expected:** The card's **Agent** field reads `codex`. It does not read `claude-code`. If Codex is not installed, the card or a failure message says so and nothing is run with Claude Code. Mark `BLOCKED` only if neither case can be seen.

**Reset:** click **Reject**.

## TC-D-04 Task proposal shows an approval card

**Source:** `docs/design/orchestrator-actions.md` "Decisions", N1.
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** No pending card in **Global orchestrator**.

1. Send the standard task message to **Global orchestrator**.
2. Wait for the reply.

**Expected:**

- The reply appears in the conversation.
- A section named "Waiting for your decision" appears with one action card.
- The card has an **Approve** button and a **Reject** button. Their accessible names start with `Approve:` and `Reject:`.
- No task has started. Open **Agents** and confirm no new task is running.

**Screenshot.**

## TC-D-05 Approving a proposal starts the task

**Source:** `docs/design/orchestrator-actions.md` "Decisions", N1.
**Real agent:** Sonnet (alias), medium, for the task run. Cost: medium.
**Preconditions:** TC-D-04 card is pending.

1. In the card, click **Approve**.
2. Watch the card until the task is finished (RUNNER.md rule 7).
3. Click **Open task** on the card. On the task page, click the **Attempts N** tab (N is the number of attempts).
4. Open **Agents**.

**Expected:**

- The card shows "Approved, running", then "Queued".
- The task changes no file, so it ends `Failed`. On the task page, the **Attempts** tab reads "Failure reason: The agent exited without changing any files." This does not fail the test; the test checks only that approval starts the task.
- A task for `manual-test-project` is listed on the Agents page with the objective `Read README.md. Do not create, change or delete any file.`

**Screenshot** after steps 2 and 4.

## TC-D-06 Rejecting a proposal changes nothing

**Source:** `docs/design/orchestrator-actions.md` "Decisions", N1.
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** Send the standard task message to **Global orchestrator**. A pending task card appears.

1. Click **Reject** on the card.
2. Open **Agents**.

**Expected:**

- The card shows "Rejected" and the text "You rejected this. Nothing was changed."
- No new task is listed on the Agents page.

## TC-D-07 Dependent cards must be approved in order

**Source:** `apps/desktop/src/ui/views/workspace/actionCardModel.ts` (`pendingView` and `parentLabel`).
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** None.

1. Send this message to **Global orchestrator**:

   ```text
   Propose exactly two actions and nothing else, in this order: first workstream.create with title "Order test", summary "Card order check" and ref "a"; then task.propose for project manual-test-project with workstream_ref "a", task_kind Implement, delivery ReportOnly, objective "Read README.md. Do not create, change or delete any file."
   ```

2. Look at the second card before you click anything.
3. Click **Approve** on the first card.
4. Look at the second card again.

**Expected:**

- In step 2, the second card cannot be approved. It says `Approve “Create workstream: Order test” first. This task goes into that workstream.`
- In step 4, the second card can be approved.

**Reset:** click **Reject** on the second card. Keep the workstream `Order test` for TC-D-14.

## TC-D-08 Home directory is used when the project is unclear

**Source:** `docs/settings.md` "Home directory".
**Real agent:** Sonnet (alias), medium, for the chat turn and the task run. Cost: medium.
**Preconditions:** Home directory is set to `~/agent-orch-manual-test/projects/manual-test-project` (setup section 6). That folder is the registered `manual-test-project`.

1. Send this message to **Global orchestrator**:

   ```text
   Propose exactly one action and nothing else: task.propose with no project field, task_kind Investigate, delivery ReportOnly, objective "Read README.md and report its first line. Change no file."
   ```

2. Read the card.
3. Click **Approve**.
4. Open **Agents** and open the task.

**Expected:**

- The card's **Project** field names `manual-test-project`, the project at the home directory.
- **Delivery** reads `ReportOnly`.

## TC-D-09 No home directory: the orchestrator asks where to run

**Source:** `docs/settings.md` "Home directory" ("asks where to run instead").
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** TC-D-08 is done.

1. On the **Repositories** page, clear the **Home directory** field and click **Save changes**.
2. Send the message of TC-D-08 step 1 to **Global orchestrator** again.

**Expected:** The result reads `Cleared. A task with no clear project will ask where to run.` No card for a home-directory task appears. Either the proposal is refused with an error shown in the card, or the reply asks where to run. Quote what you saw. Both outcomes pass, so this step is not model-dependent.

**Reset:** on the **Repositories** page, enter `~/agent-orch-manual-test/projects/manual-test-project` again and click **Save changes**.

## TC-D-10 Load gate closed: proposal card warns

**Source:** `docs/design/orchestrator-actions.md` "Machine load gate: respect free resources" (the "Load gate" bullet under "Tests").
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** `BLOCKED`. The app does not warn on the card. The daemon sends a `machine_busy` flag with a task proposal, but no app code reads it (`crates/orchestrator-daemon/src/api_actions.rs`, `proposal_target`). The queue wait shows only after approval; TC-D-11 covers it. Record `BLOCKED` with that reason and do not run the steps.

1. Send the standard task message to **Global orchestrator**.
2. Read the card.

**Expected:** The card reads "Pending approval" and "Nothing runs until you approve." No load warning appears.

## TC-D-11 Load gate: task waits, then runs

**Source:** `docs/design/orchestrator-actions.md` "Decisions", N12 ("queued work waits until load drops").
**Real agent:** Sonnet (alias), medium, for the task run. Cost: medium.
**Preconditions:** On the **Limits** page, section **Machine load limits**, set **CPU limit (percent)** to `50` and click **Save changes**. Generate CPU load above 50 percent for 60 seconds, for example by running `yes > /dev/null` in four terminals. Keep it running until step 3.

1. Click **Approve** on the card.
2. Open **Agents** and read the task's row.
3. Stop the CPU load. Wait up to three minutes, then read the task's row again.

**Expected:**

- After step 2, the task is `Queued` and its row reads `Waiting for CPU capacity (<n>% used)`. The number varies. On the task page (reached from the card's **Open task** link), the **Waiting to start** section reads `Waiting for machine capacity: CPU <n> percent, limit 50.`
- After step 3, the task starts. It then ends `Failed` as in TC-D-05, which does not fail this test.

**Reset:** on the **Limits** page, set **CPU limit (percent)** back to `80` and click **Save changes**.

## TC-D-12 Cancel, revise, and remove always need approval

**Source:** `docs/settings.md` "Actions that always need approval".
**Real agent:** Sonnet (alias), medium, for the held task run. Cost: medium; the held task starts a run.
**Preconditions:** Start a held task first (setup section 8c). Run `sh docs/manual-tests/manual-test.sh hold HOLD-1`, send the message below to **Global orchestrator**, click **Approve**, then open the task from the card's **Open task** link and wait until its page reads `Running`. Only then create a standing approval for **Propose task** in `manual-test-project` (TC-D-13 steps 1 to 3). That grant covers `task.propose` only. The three kinds in this test are never grantable, so no standing approval can cover them.

```text
Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Investigate, delivery ReportOnly, objective "HOLD-1: Read README.md and report its first line. Change no file."
```

1. Send this message to **Global orchestrator**:

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

**Expected:** Each of the three cards reads "Pending approval" and asks for your decision. None shows "Approved by a standing approval".

**Reset:** click **Reject** on each card. Run `sh docs/manual-tests/manual-test.sh release HOLD-1`, then click **Cancel** on the task page. On **Settings**, page **Autonomy**, click **Revoke** on the standing approval.

## TC-D-13 Standing approval auto-approves a granted action

**Source:** `docs/settings.md` "Standing approvals".
**Real agent:** Sonnet (alias), medium, for the chat turn and the task run. Cost: medium.
**Preconditions:** The switch **Let the orchestrator act on its own** is off. With it on, the task starts on its own first and the grant is never used.

1. Open **Settings**, open the **Autonomy** page, and find the section **Standing approvals**.
2. Click **Add**. In **Add a standing approval**, choose **One project** and **Project** `manual-test-project`. Under **Kinds of action**, tick **Propose task**. Set **Uses** to `3`.
3. Click **Create standing approval**.
4. Go to Workspace and choose **Global orchestrator**. Send the standard task message to **Global orchestrator**.
5. Read the card.

**Expected:**

- Step 2: the **Kinds of action** list does not offer **Create workstream**.
- Step 5: the card's message reads `Approved by a standing approval (<grant id>).` No click was needed.
- After the task starts, on **Settings**, page **Autonomy**, the standing approval's row reads `2 of 3 uses left`.

**Reset:** on the **Autonomy** page, click **Revoke** on the standing approval.

## TC-D-14 Proposal states

**Source:** `apps/desktop/src/daemon/types.ts` (`ProposalStateWire`: pending, approved, executed, failed, rejected, superseded, expired); `apps/desktop/src/ui/views/workspace/actionCardModel.ts` (`viewOf`).
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** The workstream `Order test` exists (TC-D-07).

1. Send this message to **Global orchestrator** and leave its card pending:

   ```text
   Propose exactly one action and nothing else: workstream.update for the workstream "Order test" with title "Order test renamed".
   ```

2. Send this message to **Global orchestrator**:

   ```text
   Propose exactly one action and nothing else: workstream.update for the workstream "Order test" with title "Order test renamed again".
   ```

3. Read the first card.

**Expected:** The older card reads **Superseded** with "A newer proposal replaced this one. Nothing was changed." Or, for a stale change, its message reads "A newer change to this workstream exists. Ask the orchestrator to propose again." Quote the text. Mark `BLOCKED` if neither state appears and say why.

**Reset:** reject any pending card.

## TC-D-15 A project coordinator proposes actions in its project

**Source:** `docs/chat.md` "Work with a project coordinator" ("It proposes actions like Global does, but only inside its project").
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** The switch **Let the orchestrator act on its own** is off. Add a coordinator for `manual-test-project`: open the target picker, choose **Add coordinator…**, and click **Add** next to `manual-test-project` in the **Add a coordinator** dialog.

1. Choose `manual-test-project` in the target picker. The header reads "Talking to: manual-test-project". Open the **Model settings** chip and set **Provider** to `Claude Code`, **Model** to `Sonnet (alias)` and **Effort** to `medium`. The chip reads `Claude Code · Sonnet (alias) · medium`.
2. Send the standard task message.

**Expected:** A card "Proposed: Create task" for `manual-test-project` appears under the reply, with **Approve** and **Reject**. Nothing runs until you approve. A message that names another project, or a `workstream.create`, is not accepted: see TC-C-04 in `09-coordinators.md`.

**Reset:** click **Reject**.

## TC-D-16 Unavailable agent is refused, not replaced

**Source:** `docs/settings.md` ("An unavailable saved selection is refused"); `docs/m7-08-conversation-service.md` (`runtime_unavailable`); `crates/orchestrator-daemon/src/api_conversation_runtime.rs` (lines 16 to 17).
**Real agent:** none can run; that is the test. Cost: low.
**Preconditions:** No task is running.

1. Quit the app. Start it with the unavailable-agent trigger (setup section 8c): `AGENT_ORCH_CLAUDE_PROGRAM` points to a path that does not exist.
2. In **Global orchestrator**, send `Hello`.

**Expected:** The turn fails. The conversation shows the failure text "Orchestrator runtime unavailable. The daemon recorded your message, but no installed agent could be launched to answer it, so no reply was produced." Quote what you see. No reply comes from another agent, and there is no silent use of Codex.

**Reset:** quit the app, wait two minutes, and start it as in setup section 4, without `AGENT_ORCH_CLAUDE_PROGRAM`.
