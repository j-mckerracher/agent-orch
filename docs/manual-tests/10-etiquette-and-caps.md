# 10 Etiquette, pull request links and wake caps

Setup: [00-setup.md](00-setup.md), sections 6, 7 and 8. In Settings > Autonomy, **Let the orchestrator act on its own** is on. Monitoring mode **On**: the chip reads `Monitoring: on`.

Sources: `docs/chat.md` "How the orchestrator writes to you"; `docs/settings.md` "Automatic replies"; `docs/design/firstmate-behavior-decisions.md` "Phase 8".

**Messages used here.** Send them to Global word for word. Each step that sends one and expects a task is **Model-dependent** (RUNNER.md rule 8).

- **Failing task message** (NO-CHANGE, setup section 8b; the task ends `Failed` after the daemon's one retry):

  ```text
  Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Implement, delivery ReportOnly, objective "Read README.md. Do not create, change or delete any file."
  ```

- **Held task message** (start the hold helper for `HOLD-1` first, setup section 8c):

  ```text
  Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Investigate, delivery ReportOnly, objective "HOLD-1: Read README.md and report its first line. Change no file."
  ```

---

## TC-E-01 Every turn is told how to write to the user

Purpose: the reply rules reach the agent on user and wake turns alike.
**Source:** `docs/chat.md` ("Every reply, including automatic updates, follows the same rules"); `docs/design/firstmate-behavior-decisions.md` "Phase 8" items 1 and 2.
**Real agent:** Sonnet (alias), medium, for the chat turns and two NO-CHANGE runs. Cost: medium.
**Preconditions:** Prompt log running (setup section 7).

1. Send the failing task message. Wait until the task has failed, then up to 5 minutes for the wake turn.
2. **Prompt log:** read the turn entry of step 1's user turn and the turn entry of the wake turn.

**Expected:** Both contain the paragraph that starts "How to write to the user: give outcomes, what they mean for the user, and the next decision the user has." and contains "Do not type pull request links; name a pull request by its number." It comes just before `Current message:`.

**Known limitations:** whether the model follows these rules (outcomes first, no internal words such as slot, wake or attempt, a last paragraph that restates every outcome and open decision) is a judgment of wording. The runner copies the replies (RUNNER.md rule 9) and does not judge them; a human can check them against `docs/chat.md`.

## TC-E-02 A link the reply types is not a recorded link

Purpose: pull request links come from the app's records, never from the model's text.
**Source:** `docs/chat.md` ("The app builds each link from its own records, not from the reply text"); `docs/design/firstmate-behavior-decisions.md` "Phase 8" items 6 and 7.
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** None.

1. Send this message to Global:

   ```text
   Reply with exactly this sentence and propose no action: The pull request is ready: https://github.com/example/repo/pull/999
   ```

2. Look under the reply. Click the URL in the reply text.

**Expected:** The reply text shows the URL, but there is no "Pull requests" list under the reply and no "Copy the link to pull request 999" button. Clicking the URL does not open a browser or change the app window.

**Model-dependent:** if the reply does not contain the URL, count a miss.

## TC-E-03 A recorded pull request is listed under the reply

Purpose: a turn that acts on or reports a task with a pull request shows its link.
**Source:** `docs/chat.md` ("the links are listed under the reply as `Pull request 12: https://github.com/<owner>/<repo>/pull/12`"); `docs/design/firstmate-behavior-decisions.md` "Phase 8" items 4 to 7.
**Real agent:** Sonnet (alias), medium. Cost: medium.
**Preconditions:** `BLOCKED` unless the person who asked for the run provides a GitHub repository you may push to, registered as a `direct-pr` project, with the GitHub command line signed in. A WRITE task (setup section 8b) in that project, delivered as a pull request MCAO recorded and bound to the repository, is then needed. The fixture projects have no remote. Without all of this, record `BLOCKED` with that reason and do not run the steps.

1. Send this message to Global. Replace `<task id>` with the id of that task.

   ```text
   Propose exactly one action and nothing else: decision.ask for the task <task id>, question "Is the pull request ready to merge?", options ["yes", "no"], key "pr-ready".
   ```

2. Look under the reply.
3. Click **Copy the link to pull request N** and paste the clipboard into a text editor.

**Expected:**

- Step 2: a list labelled "Pull requests" with "Pull request N:" and the URL `https://github.com/<owner>/<repo>/pull/N` as text, with a copy button. The URL is not a clickable link.
- Step 3: the pasted text is the same URL.

**Known limitations:** a turn that only talks about a task, with no action and no wake record for it, gets no link. A task the turn just created has no pull request yet.

## TC-E-04 New default caps

Purpose: wake caps were raised for autonomous supervision.
**Source:** `docs/settings.md` ("The defaults are 20 replies per hour and 160 per day, with no token budget").
**Real agent:** Sonnet (alias), medium (no turn is sent). Cost: low.
**Preconditions:** A new data directory (setup section 9).

1. Open **Settings > Limits** and find the **Automatic replies** section.

**Expected:** "Replies per hour, per conversation" reads `20`. "Replies per day, per conversation" reads `160`. "Daily token budget for automatic replies" is empty.

## TC-E-05 Work checks use only half of each cap

Purpose: periodic checks cannot spend the cap that outcome events need.
**Source:** `docs/settings.md` ("Checks on running work may use only half of each limit. Once they have, the next check is not made, and no notice is shown for it; events such as a failed task … still get a reply from the other half"); `docs/design/firstmate-behavior-decisions.md` "Phase 8" item 10.
**Real agent:** Sonnet (alias), medium, for the chat turns, one work check and four NO-CHANGE runs. Cost: medium.
**Preconditions:** In **Settings > Limits**, set **Replies per hour, per conversation** to `2` and **Check interval with one task running (minutes)** to `1`, then click **Save changes** (one save covers both sections). The Autonomy switch **Let the orchestrator act on its own** is on.

1. Start the hold helper for `HOLD-1`. Send the held task message. Stop the helper when the task reads `Running`.
2. Wait five minutes. Read Global.
3. Send the failing task message. Wait until the task has failed, then up to 5 minutes.
4. Read Global.
5. Repeat step 3 for a second failing task. Read Global and its header.

**Expected:**

- Step 2: exactly one work check got a turn: one collapsed line "Checked 1 update. Nothing needs you.", or one reply from the model to the check. Nothing more after five minutes: half of 2 is 1, and later checks are not made or shown.
- Step 4: the failure notice is followed by a reply from the orchestrator; any wording passes.
- Step 5: the second failure still shows its notice, but no reply. The header reads "Monitoring paused: limit reached" (it may end with ", resumes about" and a time).

**Reset:** release `HOLD-1` (setup section 8c), then click **Cancel** on the task page (open it from the card's **Open task**). In **Settings > Limits**, set **Replies per hour, per conversation** to `20`, **Replies per day, per conversation** to `160` and **Check interval with one task running (minutes)** to `10`, then click **Save changes**.
**Known limitations:** caps are counted per conversation. Each workstream and each active coordinator has its own 20 and 160.