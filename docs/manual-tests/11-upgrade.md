# 11 Upgrade: a database from before this change keeps its data

Setup: [00-setup.md](00-setup.md). This file uses its own data directory, `$HOME/agent-orch-manual-test-upgrade`, and two builds.

Sources: `docs/design/firstmate-behavior-decisions.md` "Integration" I0 and I1, Phase 6 item 1, Phase 8 item 11; `crates/orchestrator-store/migrations/0064_decision_questions.sql` to `0072_*.sql`.

---

## TC-M-01 Notes, tasks, cards and caps survive the upgrade

Purpose: migrations 0064 to 0072 apply to a real older database and lose nothing.
**Source:** Integration I0 ("The stack's migrations are numbered 0064 to 0072"); Phase 6 item 1 (each note moves into the preferences of its own scope); Phase 8 item 11 (the old default caps 6 and 40 move to 20 and 160).
**Preconditions:** `sqlite3` is installed. A second checkout of the commit before this change: `git worktree add ../mcao-before 8eff2599^1`. Build it as in setup section 1, inside that checkout. The fake agent from setup section 2.

1. Start the old build (setup section 3 from `../mcao-before`), with `AGENT_ORCH_DATA_DIR="$HOME/agent-orch-manual-test-upgrade"`. Do setup sections 4 and 5, steps 1 to 4.
2. In **Settings > Notes for the orchestrator**, choose **Global** under "Notes for", type `Keep pull requests small` in "New note" and click **Add note**.
3. Create a workstream `Old stream` (TC-S-01 and TC-S-02). Back in **Notes for the orchestrator**, choose **One workstream**, pick `Old stream`, and add the note `Old stream note`.
4. In Global, make the agent propose the standard task with `task-mode` `fail`. Approve it and wait until it is `Failed`.
5. Make the agent propose a second standard task. Leave its card pending.
6. Quit the app. Wait two minutes.
7. Start the new build from this repository with the same `AGENT_ORCH_DATA_DIR`.
8. Run `sqlite3 "$HOME/agent-orch-manual-test-upgrade/store.sqlite" "SELECT version, success FROM _sqlx_migrations WHERE version >= 63 ORDER BY version"`.
9. Open **Settings > Preferences and learnings**. Read **Shared preferences**. Choose `Old stream` in "Preferences for".
10. Open **Settings > Automatic replies**.
11. Open the failed task. Open Global and find the pending card.
12. Open **Settings > Acting on its own**.

**Expected:**

- Step 7: the app opens with no error, and "Fixture data" does not appear.
- Step 8: one row each for 63 to 72, every one with `success` `1`.
- Step 9: Shared preferences lists `note-1` with `Keep pull requests small`, "Written by the user". `Old stream` lists `note-1` with `Old stream note`.
- Step 10: `20` and `160`.
- Step 11: the failed task is listed with its failure reason. The old card still reads "Pending approval" and has **Approve** and **Reject**: it was not run on its own after the upgrade.
- Step 12: the switch "Let the orchestrator act on its own" is on.

**Reset:** quit the app. Delete `$HOME/agent-orch-manual-test-upgrade`. Remove the old checkout with `git worktree remove ../mcao-before`.
**Known limitations:** a development database built from a DPES-1108 branch before the renumbering (a version 63 that is not main's, or phase 7's first 0070) does not open; delete it (Integration I0 and I1). There is no down migration.
