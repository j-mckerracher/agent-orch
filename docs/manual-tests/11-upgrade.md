# 11 Upgrade: a database from before this change keeps its data

Setup: [00-setup.md](00-setup.md). This file uses its own data directory, `$HOME/agent-orch-manual-test-upgrade`, and two builds. The old build runs steps 1 to 5. The new build (this repository at `main`) runs steps 7 to 12.

Sources: `docs/design/firstmate-behavior-decisions.md` "Integration" I0 and I1, "Phase 6" item 1, "Phase 8" item 11; `crates/orchestrator-store/migrations/0064_decision_questions.sql` to `0074_agent_activity.sql`.

---

## TC-M-01 Notes, tasks, cards and caps survive the upgrade

Purpose: migrations 0064 to 0074 apply to a real older database and lose nothing.
**Source:** `docs/design/firstmate-behavior-decisions.md` "Integration" I0 (a development database from a DPES-1108 branch has a version 63 that is not main's); "Phase 6" item 1 (each note moves into the preferences of its own scope); "Phase 8" item 11 (the old default caps 6 and 40 move to 20 and 160; the migration is `crates/orchestrator-store/migrations/0072_etiquette_and_wake_caps.sql`).
**Real agent:** Sonnet (alias), medium, for the chat turns of both builds and two NO-CHANGE runs. Cost: medium.
**Preconditions:** `sqlite3` is installed. Setup sections 1 to 3 done for this repository. The old build is the first parent of `8eff2599`, the merge that adds migrations 0064 to 0072 (`8eff2599^1`, `d9af1263`). Its migrations end at `0063`. Make a second checkout and build it there: `git worktree add ../mcao-before 8eff2599^1`, then build it as in setup section 1. Record its SHA in `ENVIRONMENT.md`. Both builds use the real agent of setup section 4.

**Old-build labels (steps 1 to 5).** The old build has one **Settings** page that holds every section. Its sidebar reads **Orchestrator Workspace**, **Agents & Sessions**, **Review & Decisions** and **Settings**. It has no **Repositories** page, no **Autonomy** page and no "Let the orchestrator act on its own" switch. On the old build, use these labels, not the labels of setup section 6:

- **Agent dispatch defaults**: **Default provider**, **Claude Code model**, **Claude Code effort**, **Save dispatch defaults**. The result reads "Saved. New tasks use these defaults."
- **Home directory**: **Save home directory**.
- **Automatic replies**: **Replies per hour, per conversation**, **Replies per day, per conversation**, **Save reply limits**. The old defaults are 6 an hour and 40 a day, so expect `6` and `40`, not `20` and `160`.
- **Checks on running work**: **Check interval with one task running (minutes)**, **Fastest check interval (seconds)**, **Longest time a task may run before it is flagged (minutes)**, **Save check settings**.
- **Repositories**: **Repository path** and **Register repository**. There is no name field; the project takes its name from the folder.
- **Notes for the orchestrator**: **Notes for** (**Global** or **One workstream**), **Workstream**, **New note**, **Add note**.
- Cards: **Approve**, **Reject**, and the pill **Pending approval**.

1. **Old build.** Start the old build with the command of setup section 4, run from `../mcao-before`, with `AGENT_ORCH_DATA_DIR="$HOME/agent-orch-manual-test-upgrade"`. Then register the project in the **Repositories** section of the old build's one **Settings** page: type `$HOME/agent-orch-manual-test/projects/manual-test-project` in **Repository path** and click **Register repository**. There is no **Add repository** button and no name field. Then do section 6 steps 1 to 4 with the old-build labels above. Skip section 6 step 5. The old build has no Autonomy switch and no "Done on its own" text, so its cards wait for **Approve**. If the old build lacks a setting, skip it and note that in `RESULTS.md`.
2. **Old build.** In **Settings**, under **Notes for the orchestrator**, choose **Global** under "Notes for", type `Keep pull requests small` in **New note** and click **Add note**.
3. **Old build.** In **Orchestrator Workspace**, choose **Global orchestrator** in the target picker and send the message below. Click **Approve** on its card (**Model-dependent**). Then in **Settings**, under **Notes for the orchestrator**, choose **One workstream**, pick `Old stream` under **Workstream**, type `Old stream note` in **New note** and click **Add note**.

   ```text
   Propose exactly one action and nothing else: workstream.create with title "Old stream", summary "Made before the upgrade" and ref "a".
   ```

4. **Old build.** In **Global orchestrator**, send the standard task message of `01-dispatch.md`, copied below. Click **Approve** on its card and wait until the task is **Failed** (**Model-dependent**).

   ```text
   Propose exactly one action and nothing else: task.propose for project manual-test-project, task_kind Implement, delivery ReportOnly, no agent field, objective "Read README.md. Do not create, change or delete any file."
   ```

5. **Old build.** Send the same message again. Leave its card at **Pending approval** (**Model-dependent**).
6. **Both builds.** Quit the app. Wait two minutes.
7. **New build.** Start the new build from this repository with the command of setup section 4, run from the repository root, with the same `AGENT_ORCH_DATA_DIR`.
8. **New build.** Run `sqlite3 "$HOME/agent-orch-manual-test-upgrade/store.sqlite" "SELECT version, success FROM _sqlx_migrations WHERE version >= 63 ORDER BY version"`.
9. **New build.** Open **Settings > Memory**. In the **Preferences and learnings** section, read **Shared preferences**. Choose `Old stream` in "Preferences for".
10. **New build.** Open **Settings > Limits**. In **Automatic replies**, read the two caps.
11. **New build.** In **Global orchestrator**, open the card of step 4 and click **Open task**. Read its **Attempts** tab. Then go back to **Global orchestrator** and find the card of step 5.
12. **New build.** Open **Settings > Autonomy**.

**Expected:**

- Step 7: the app opens with no error, and "Fixture data" does not appear.
- Step 8: one row each for 63 to 74, every one with `success` `1`.
- Step 9: **Shared preferences** lists `note-1` with `Keep pull requests small`, "Written by the user". `Old stream` lists `note-1` with `Old stream note`.
- Step 10: "Replies per hour, per conversation" reads `20`. "Replies per day, per conversation" reads `160`.
- Step 11: the **Attempts** tab of the failed task shows **Failure reason:**, whose text begins "The agent exited without changing any files." The card of step 5 still reads "Pending approval" and has **Approve** and **Reject**: it was not run on its own after the upgrade.
- Step 12: the switch "Let the orchestrator act on its own" is on.

**Reset:** quit the app. Delete `$HOME/agent-orch-manual-test-upgrade`. Remove the old checkout with `git worktree remove ../mcao-before`.
**Known limitations:** a development database built from a DPES-1108 branch before the renumbering (a version 63 that is not main's, or phase 7's first 0070) does not open; delete it (`docs/design/firstmate-behavior-decisions.md` "Integration" I0 and I1). No user database has these migrations. There is no down migration.
