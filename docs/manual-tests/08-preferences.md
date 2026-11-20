# 08 Preferences and learnings

Setup: [00-setup.md](00-setup.md), sections 6, 7 and 8. **Let the orchestrator act on its own** is on.

**Settings > Memory** has the section **Preferences and learnings**. It has three lists, each with its own add form, entries, byte use and **Removed entries**:

- **Shared preferences**: read by every conversation.
- **Project and workstream preferences**: pick one in **Preferences for**. Projects show by folder name, workstreams by title.
- **Learnings**: pick a scope in **Learnings for**, which starts at **Global**.

Each add form sits in a closed disclosure. Its name is **Add shared preference**, **Add project or workstream preference** or **Add learning**, so expand it first. The shared preference form has **New shared preference key**, **New shared preference text** and **Add shared preference**. The other lists name their fields the same way.

Each entry shows its key, its text, "Written by the user" or "Written by the orchestrator", and "Confirmed" with a date, with **Edit** and **Remove**.

Sources: `docs/settings.md` "Preferences and learnings"; `docs/design/firstmate-behavior-decisions.md` "Phase 6: preferences and learnings" and "Integration" I2 and I8.

Every step that sends a message and expects a card is **Model-dependent** (RUNNER.md rule 8). To reset a missed attempt, reject any card the reply made and remove any entry it saved.

---

## TC-P-01 Add, change in place, remove and restore a shared preference

Purpose: one entry per key; nothing is deleted without a way back.
**Source:** `docs/settings.md` "Preferences and learnings" (quotes "Saving a key that already exists changes that entry" and "a removed or replaced entry moves to Removed entries").
**Real agent:** none. Cost: low.
**Preconditions:** None.

1. In **Shared preferences**, expand **Add shared preference**. Type key `pr-size` and text `Keep pull requests small`. Click **Add shared preference**.
2. Type key `pr-size` again. Read the hint under the key. Type text `Keep pull requests under 300 lines`. Click **Add shared preference**.
3. Open **Removed entries (1)**.
4. Click **Restore** on the old text.
5. Click **Remove** on `pr-size`. Then click **Restore** on the newest removed entry.

**Expected:**

- Step 1: "Added pr-size." The entry reads "Written by the user". The byte line reads "31 of 2048 bytes used." (key plus text).
- Step 2: the hint reads "This key is in use. Saving replaces its text, and the old text goes under Removed entries." The result reads "Changed pr-size. The old text is under Removed entries." Only one `pr-size` is listed, with the new text.
- Step 3: the old text, "Replaced", the date, "Written by the user.", and **Restore**.
- Step 4: "This entry cannot be restored: its key is in use again, or it was already restored. Nothing was changed."
- Step 5: "Removed pr-size. It is under Removed entries.", then "Restored pr-size." The restored row in Removed entries has the pill "Restored".

## TC-P-02 Text that looks like a secret or a path is refused

Purpose: no credentials or paths go into text that every prompt carries.
**Source:** `docs/settings.md` "Preferences and learnings" (quote "Do not put passwords, tokens, or file paths in an entry. The daemon refuses entries that look like these.").
**Real agent:** none. Cost: low.
**Preconditions:** None.

1. In **Shared preferences**, expand **Add shared preference**. Add key `repo` with text `the repo is at /Users/someone/code`.
2. Add key `api` with text `key sk-abcdefghijkl`.
3. Add key `Bad Key` with text `fine`.

**Expected:**

- Steps 1 and 2: "The daemon refused this entry. A key is lowercase letters, digits and hyphens. The text is one line and must not look like a password, a token or a file path. Nothing was changed." The typed text stays in the form. Nothing is added.
- Step 3: **Add shared preference** stays disabled and the key field reads "A key uses lowercase letters, digits and hyphens only, and starts with a letter or a digit."

## TC-P-03 A list cannot grow past its byte budget

Purpose: the budget is refused with the size, not trimmed.
**Source:** `docs/settings.md` "Preferences and learnings" (quote "2,048 bytes for shared preferences, 2,048 for one project or workstream, and 1,024 for one list of learnings").
**Real agent:** none. Cost: low.
**Preconditions:** **Learnings for** is **Global**, with no entries. Expand **Add learning**.

1. Add three learnings, keys `fact-1`, `fact-2` and `fact-3`, each with a text of 280 lowercase `x` characters.
2. Type key `fact-4` and a text of 280 `x` characters. Read the form.

**Expected:**

- Step 1: the byte line reads "858 of 1024 bytes used."
- Step 2: under the text the hint reads "With this entry: 1144 of 1024 bytes." **Add learning** is disabled and the form reads "Over the budget: 1144 of 1024 bytes. Shorten or remove an entry."

**Reset:** remove the three learnings.

## TC-P-04 The orchestrator records a learning on its own, but only proposes a preference

Purpose: learnings are data the orchestrator may keep; preferences are the user's word.
**Source:** `docs/settings.md` "Preferences and learnings" (quote "a preference is never saved on its own"); `docs/design/firstmate-behavior-decisions.md` "Integration", I2.
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** None.

1. Send this message to Global orchestrator:

   ```text
   Propose exactly two actions and nothing else: memory.set with document learnings, key "checks-time" and text "The checks take about two minutes"; and memory.set with document shared, key "tone" and text "Reply in short sentences".
   ```

2. Wait 30 seconds. Read both cards.
3. Click **Approve** on the waiting card.
4. Open **Settings > Memory**.

**Expected:**

- Step 2: "Proposed: Record a learning" reads "Done on its own" and "Recorded the learning “checks-time”." "Proposed: Save a shared preference" waits for you and shows the field "Saves to" with the value "Shared preferences", the full key and the full text.
- Step 3: the card reads "Saved the preference “tone”."
- Step 4: Shared preferences lists `tone`; Learnings for **Global** lists `checks-time`. Both read "Written by the orchestrator".

## TC-P-05 Only the global conversation may change shared preferences

Purpose: coordinators and workstreams read shared preferences but cannot write them.
**Source:** `docs/settings.md` "Preferences and learnings" (quote "A project coordinator or a workstream conversation can read them but not propose a change"); `docs/design/firstmate-behavior-decisions.md` "Phase 6: preferences and learnings", item 11.
**Real agent:** Sonnet (alias), medium. Cost: low.
**Preconditions:** TC-P-04 done. Add a coordinator for `manual-test-project` (target picker, **Add coordinator…**, **Add**). Create the workstream `Auto stream`: send this message to Global orchestrator; its card reads "Done on its own".

```text
Propose exactly one action and nothing else: workstream.create with title "Auto stream", summary "Made on its own" and ref "a".
```

1. Choose **manual-test-project coordinator** in the target picker. Send this message:

   ```text
   Propose exactly three actions and nothing else: memory.set with document shared, key "tone" and text "Use long sentences"; memory.set with document local, key "branch" and text "Branch from main"; memory.set with document learnings, key "test-cmd" and text "The tests run with make test".
   ```

2. Read the three cards.
3. Click **Approve** on the `local` card.
4. Choose **Auto stream orchestrator**. Send this message:

   ```text
   Propose exactly one action and nothing else: memory.set with document shared, key "tone" and text "Use long sentences".
   ```

5. Choose **Global orchestrator**. Send this message:

   ```text
   Propose exactly one action and nothing else: memory.set with document local, key "branch" and text "Branch from main".
   ```

6. Open **Settings > Memory**.

**Expected:**

- Step 2: the shared card reads "Denied" and "Shared preferences are read-only here. Report it to the global conversation, or set it in Settings." The local card, "Proposed: Save a preference for this conversation", waits for you. The learning card reads "Done on its own".
- Step 4: the card reads "Denied" with the same read-only text.
- Step 5: the card reads "Denied" and "The global conversation has no local preferences. Use a shared preference instead."
- A model may decline to propose a change it knows is read-only in that conversation. That is a miss for that step (RUNNER.md rule 8); after 3 misses the step is `BLOCKED`, not `FAIL`.
- Step 6: Shared preferences still has `tone` with `Reply in short sentences`. "Preferences for" `manual-test-project` lists `branch`. "Learnings for" `manual-test-project` lists `test-cmd`.

## TC-P-06 Automatic turns read the shared preferences

Purpose: wake turns now see the user's standing defaults.
**Source:** `docs/design/firstmate-behavior-decisions.md` "Phase 6: preferences and learnings", item 14 ("Wake turns see all three documents").
**Real agent:** Sonnet (alias), medium, for the chat turns and two NO-CHANGE runs. Cost: medium.
**Preconditions:** Shared preference `tone` exists (TC-P-04). Prompt log running (setup section 7). Monitoring **On**.

1. Send this message to Global orchestrator. The task starts on its own. Wait until it has failed, then up to 5 minutes for the wake turn.

   ```text
   Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Implement, delivery ReportOnly, objective "Read README.md. Do not create, change or delete any file."
   ```

2. **Prompt log:** read the turn entry of the wake turn.

**Expected:** The prompt contains the text "Shared preferences (" and, under it, the entry `tone` with its text. Every turn's prompt also contains "How to write to the user:".

## TC-P-07 A stale learning is marked and can be kept

Purpose: learnings nobody confirmed for 180 days are flagged, never removed.
**Source:** `docs/settings.md` "Preferences and learnings" (quote "A learning that nobody has confirmed for 180 days is marked stale. Click Keep to confirm it").
**Real agent:** none. Cost: low.
**Preconditions:** Expand **Add learning** in **Learnings for** **Global**. Type key `old-fact` and text `The checks take two minutes`, then click **Add learning**. Quit the app and wait two minutes, so the daemon has exited. Make it look 181 days old in the store, since no control can do that: `sqlite3 "$HOME/agent-orch-manual-test/data/store.sqlite" "UPDATE memory_entries SET confirmed_at_ms = confirmed_at_ms - 181*86400000 WHERE kind = 'learning' AND key = 'old-fact'"`. Start the app.

1. Open **Settings > Memory**, then the **Learnings** list.
2. Click **Keep** on `old-fact`.

**Expected:**

- Step 1: the caption reads "An entry nobody confirmed for 180 days is marked stale. Keep it if it is still true, or remove it." `old-fact` has the pill "Stale" and a **Keep** button. It is not removed.
- Step 2: "Kept old-fact. It is confirmed as of today." The pill and **Keep** are gone.

**Known limitations:** the precondition edits the store directly; the real trigger is 180 days without a confirmation. A preference never goes stale.

## TC-P-08 Clearing a conversation keeps its preferences

Purpose: preferences are managed in Settings, not by history.
**Source:** `docs/design/firstmate-behavior-decisions.md` "Phase 6: preferences and learnings", item 7 ("A history clear no longer deletes anything here").
**Real agent:** none. Cost: low.
**Preconditions:** Shared preference `tone` exists.

1. In the **Global orchestrator** conversation, open the **⋯** button in the header (its name is "Conversation actions for Global orchestrator"). Choose **Clear history…**, then **Clear history permanently**.
2. Open **Settings > Memory**.

**Expected:** `tone` is still listed.

**Known limitations:** this test checks only a clear. Removing a coordinator also keeps its project's preferences and learnings (Phase 6 item 7). The Remove coordinator dialog says so. This test does not remove a coordinator.
