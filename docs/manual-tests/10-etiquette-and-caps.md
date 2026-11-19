# 10 Etiquette, pull request links and wake caps

Setup: [00-setup.md](00-setup.md), sections 2a and 2b. Monitoring **On**.

Sources: `docs/chat.md` "How the orchestrator writes to you"; `docs/settings.md` "Automatic replies"; `docs/design/firstmate-behavior-decisions.md` "Phase 8".

---

## TC-E-01 Every turn is told how to write to the user

Purpose: the reply rules reach the agent on user and wake turns alike.
**Source:** `docs/chat.md` ("Every reply, including automatic updates, follows the same rules"); Phase 8 items 1 and 2.
**Preconditions:** Prompt log. One user turn and one wake turn have run (for example TC-W-04).

1. **Prompt log:** read a `turn` entry from a user turn and one from a wake turn.

**Expected:** Both contain the paragraph that starts "How to write to the user: give outcomes, what they mean for the user, and the next decision the user has." and contains "Do not type pull request links; name a pull request by its number." It comes just before `Current message:`.

**Known limitations:** whether a real model follows these rules (outcomes first, no internal words such as slot, wake or attempt, a last paragraph that restates every outcome and open decision) can only be judged with a real agent. **Real agent:** with Codex or Claude Code as the chat agent, ask Global `What happened to my last task?` after a failed task, and check the reply against the rules in `docs/chat.md`.

## TC-E-02 A link the reply types is not a recorded link

Purpose: pull request links come from the app's records, never from the model's text.
**Source:** `docs/chat.md` ("The app builds each link from its own records, not from the reply text"); Phase 8 items 6 and 7.
**Preconditions:** `next-reply.txt` holds `The pull request is ready: https://github.com/example/repo/pull/999`, with no action block.

1. Send `Where is the pull request?` to Global.
2. Look under the reply. Click the URL in the reply text.

**Expected:** The reply text shows the URL, but there is no "Pull requests" list under the reply and no "Copy the link to pull request 999" button. Clicking the URL does not open a browser or change the app window.

## TC-E-03 A recorded pull request is listed under the reply

Purpose: a turn that acts on or reports a task with a pull request shows its link.
**Source:** `docs/chat.md` ("the links are listed under the reply as `Pull request 12: https://github.com/<owner>/<repo>/pull/12`"); Phase 8 items 4 to 7.
**Preconditions:** A task of a `direct-pr` project with a pull request MCAO recorded and bound to its GitHub repository. That needs a GitHub repository you may push to, the GitHub command line signed in, and a candidate (the fake agent's `write` mode). **BLOCKED** until all three are available.

1. Put a `task.retry` or `decision.ask` for that task in `next-reply.txt`. Send `Check on the pull request`. Clear `next-reply.txt`.
2. Look under the reply.
3. Click **Copy the link to pull request N** and paste the clipboard into a text editor.

**Expected:**

- Step 2: a list labelled "Pull requests" with "Pull request N:" and the URL `https://github.com/<owner>/<repo>/pull/N` as text, with a copy button. The URL is not a clickable link.
- Step 3: the pasted text is the same URL.

**Known limitations:** a turn that only talks about a task, with no action and no wake record for it, gets no link. A task the turn just created has no pull request yet.

## TC-E-04 New default caps

Purpose: wake caps were raised for autonomous supervision.
**Source:** `docs/settings.md` ("The defaults are 20 replies per hour and 160 per day, with no token budget").
**Preconditions:** A new data directory (setup section 7).

1. Open **Settings > Automatic replies**.

**Expected:** "Replies per hour, per conversation" reads `20`. "Replies per day, per conversation" reads `160`. The daily token budget is empty.

## TC-E-05 Work checks use only half of each cap

Purpose: periodic checks cannot spend the cap that outcome events need.
**Source:** `docs/settings.md` ("Checks on running work may use only half of each limit. Once they have, the next check is not made, and no notice is shown for it; events such as a failed task … still get a reply from the other half"); Phase 8 item 10.
**Preconditions:** **Settings > Automatic replies**: replies per hour `2`, saved with **Save reply limits**. **Checks on running work**: base interval `1` minute, saved with **Save check settings**. Acting on its own on.

1. Write `hang` to `task-mode`. Put the standard `task.propose` in `next-reply.txt` and send `Run the README task`. When the reply arrives, write `MCAO_QUIET` to `next-reply.txt`. A user turn never hides the token, so do not send a message while it is in the file.
2. Wait five minutes. Read Global.
3. Write `fail` to `task-mode`. Put the standard `task.propose` in `next-reply.txt` and send `Run it again`. When the reply arrives, write `The task failed.` to `next-reply.txt`.
4. Wait up to two minutes. Read Global.
5. Repeat step 3 for a third task. Wait up to two minutes. Read Global and its header.

**Expected:**

- Step 2: one collapsed line reads "Checked 1 update. Nothing needs you." It still says 1 after five minutes: half of 2 is 1, and later checks are not made or shown.
- Step 4: the failure notice is followed by the reply "The task failed.".
- Step 5: the third failure still shows its notice, but no reply. The header reads "Monitoring paused: limit reached" (it may end with ", resumes about" and a time).

**Reset:** cancel the running task. Set the caps back to `20` and `160`, the base interval to `10`, and `task-mode` to `succeed`.
**Known limitations:** caps are counted per conversation. Each workstream and each active coordinator has its own 20 and 160.

## TC-E-06 A quiet automatic update shows nothing but the fold

Purpose: an update with nothing to tell the user posts no reply text.
**Source:** `docs/chat.md` ("An automatic update about running work posts nothing when there is nothing to tell you"); Phase 8 item 3.
**Preconditions:** As TC-W-06.

1. Run TC-W-06.

**Expected:** As TC-W-06: only the collapsed line "Checked N updates. Nothing needs you.", and the token `MCAO_QUIET` is never shown. A quiet reply never hides a failure (TC-W-17) or a report (TC-W-18).
