# 12 Settings

Setup: [00-setup.md](00-setup.md), sections 6 and 9. **Let the orchestrator act on its own** is on.

These tests walk every operation in **Settings**, page by page. Each test names the page, the control, the exact values, how to save, what the page shows, how to read the value back and how to reset it. Boundary values sit in the steps. Almost every test is cost **low**: no task run. A test that changes how a new task starts links to the test that starts one (TC-D-02, TC-D-11, TC-D-13, TC-Q-01, TC-W-08, TC-S-09).

How the settings save:

- Each page has one **Save changes** bar. It saves every section on the page that you changed. **Autonomy** has no Save button: its switches save when you click them.
- The values live in the daemon, so they stay across an app restart. **Theme** and **Background animation** live in the app's own storage instead. TC-ST-05 restarts the app and checks both.
- The app has no reload control. To read a value back, open another page and come back. For a restart, quit, wait and start as `00-setup.md` section 4 says. Do not use the reset of section 9: it deletes the data folder.

## Coverage matrix

`sh docs/manual-tests/manual-test.sh settings-coverage` lists every page, every **Find a setting** entry, every settings group the app sends, every `settings.*` method and every `settings.set` field in the source. It prints a line for each one that is not written in backticks in this table. It must print nothing but `settings-coverage: ok`.

| Operation | Written in the source as | Test cases |
| --- | --- | --- |
| Pages | `Agents` | TC-ST-07, TC-ST-08, TC-ST-09 |
| | `Limits` | TC-ST-10, TC-ST-11, TC-ST-12 |
| | `Autonomy` | TC-ST-13, TC-ST-14, TC-ST-15 |
| | `Repositories` | TC-ST-16, TC-ST-17 |
| | `Memory` | TC-ST-18, TC-ST-19 |
| | `Notifications` | TC-ST-20 |
| | `Appearance` | TC-ST-21 |
| Find a setting, page menu, page switching | **Find a setting** | TC-ST-01, TC-ST-02 |
| Default provider, model, effort; no default; Clear defaults | `Default provider`, `Default model`, `Default effort` | TC-ST-07 |
| Per-provider overrides, the "N set" count, reset, Clear provider choice | `Per-provider overrides` | TC-ST-08 |
| Installed adapters, Refresh models | `Installed adapters` | TC-ST-09 |
| Agent detection | `Agent detection` | TC-ST-09 |
| Current machine load | `Current machine load` | TC-ST-10 |
| Machine load limits | `Machine load limits` | TC-ST-10 |
| Checks on running work | `Checks on running work` | TC-ST-11 |
| Automatic replies | `Automatic replies` | TC-ST-12 |
| The three Autonomy switches | `Let the orchestrator act on its own`, `Propose tasks that end in a pull request`, `Read-only task tools in chat` | TC-ST-13 |
| Standing approvals: add, limits, revoke, each grant kind, unattended merges | `Standing approvals` | TC-ST-14, TC-ST-15 |
| Add repository | `Add repository` | TC-ST-16 |
| Home directory | `Home directory` | TC-ST-17 |
| Preferences: shared, project, workstream; add, edit, remove, restore | `Preferences` | TC-ST-18, TC-ST-19 |
| Learnings: remove, restore | `Learnings` | TC-ST-19 |
| Decision notifications | `Decision notifications` | TC-ST-20 |
| Theme | `Theme` | TC-ST-21 |
| Background animation | `Background animation` | TC-ST-21 |
| The save bar: dirty, saving, saved, failed, discard | the **Save changes** bar | TC-ST-03 |
| Locked states, with the stated reason | the reason text beside the bar | TC-ST-04 |
| Values stay across a restart | all of the above | TC-ST-05 |
| Older daemon hides what it cannot save | `settings.get` | TC-ST-06 |
| Group sent by **Agents** | `dispatchDefaults`, `dispatch_defaults` | TC-ST-07, TC-ST-08 |
| Group sent by **Limits** | `load`, `supervision`, `wakeLimits`, `wake_limits` | TC-ST-10, TC-ST-11, TC-ST-12 |
| Groups sent by **Autonomy** | `autonomousActions`, `autonomous_actions`, `chatPrDelivery`, `chat_pr_delivery`, `mcpEnabled`, `mcp` | TC-ST-13 |
| Group sent by **Repositories** | `homeDirectory`, `home_directory` | TC-ST-17 |
| Read and write of the settings | `settings.get`, `settings.set` | TC-ST-05, TC-ST-07 to TC-ST-13, TC-ST-17 |
| Work-class settings: no Settings control | `settings.workClasses.get`, `settings.workClasses.set` | TC-ST-22 |

Not reachable from **Settings**:

- `settings.workClasses.get` and `settings.workClasses.set` have no caller in the app (TC-ST-22).
- The daemon accepts `idempotency_key` in `settings.set`. The app sets it on its own. A tester cannot enter it.
- **Theme** has no System choice. After one change, no control returns the app to following the operating system (TC-ST-21).
- **Repositories** has **Add repository** and **Home directory** only. No control edits or removes a repository there.
- Project defaults (`projects.get_defaults`, `projects.set_defaults`) are on a project's own page, in its Defaults tab, not in **Settings** (TC-ST-22).

---

## TC-ST-01 Find a setting

Purpose: the search finds pages and settings by their words. Blank and no-match queries behave as the code says.
**Source:** `apps/desktop/src/ui/views/settings/SettingsView.tsx` (the **Find a setting** input, `open`, `onSearchKey`); `apps/desktop/src/ui/views/settings/settingsPages.tsx` (`SETTINGS`, `searchSettings`).
**Real agent:** none. Cost: low.
**Preconditions:** Setup in `00-setup.md` sections 4 to 6 is done. Sidebar **Settings** is open on any page.

1. Sidebar **Settings**. In the field **Find a setting** at the top of the Settings menu, type `limit`.
2. Clear the field. Type `wake hour`.
3. Clear the field. Type `ambient`.
4. Clear the field. Type `zzz`.
5. Clear the field. Type three spaces.
6. Clear the field. Type `limits`, then press Enter.
7. On the Limits page, click **Find a setting**. Type `limit`, then press Escape.
8. Clear the field. Type `ambient`, then click the result **Background animation**.

**Expected:**

- Step 1: the list **Search results** shows two rows, in this order: the page **Limits**, then **Machine load limits**. Each row shows **Limits** in small text beside it. The page menu is hidden while the query is not blank.
- Step 2: one row, **Automatic replies**, with **Limits** in small text beside it.
- Step 3: one row, **Background animation**, with **Appearance** in small text beside it.
- Step 4: the text **No setting matches.** shows, and no result rows.
- Step 5: the page menu comes back: **Agents**, **Limits**, **Autonomy**, **Repositories**, **Memory**, **Notifications**, **Appearance**. The results list is gone.
- Step 6: the **Limits** page opens and its menu item is the current one. The field is empty.
- Step 7: the field is empty after Escape. The page menu is back.
- Step 8: the **Appearance** page opens. The **Background** control (the ≈ summary) is on the page.

**Persistence:** not applicable. The search text is not saved, and this test saves nothing.

**Reset:** none. Nothing was saved.

**Known limitations:**

- Clicking a result opens its page. It does not scroll to or highlight the setting. No scroll call exists in `apps/desktop/src/ui/views/settings`. Record what you see. Do not count it as a failure.

## TC-ST-02 Page switching drops values that were not saved

Purpose: the sidebar and the page menu open the right page. Leaving a page drops values that were typed and not saved.
**Source:** `apps/desktop/src/ui/views/settings/SettingsView.tsx` (`PAGES.find(...) ?? PAGES[0]`, `key={page.id}`); `apps/desktop/src/ui/shell/SidebarNav.tsx` (the **Settings** item); `apps/desktop/src/ui/router.ts` (`navigate` replaces the route).
**Real agent:** none. Cost: low.
**Preconditions:** Setup in `00-setup.md` sections 4 to 6 is done. The data folder is fresh, so **CPU limit (percent)** reads `80`. The default comes from `crates/orchestrator-store/src/sqlite_supervision.rs` (`settings_default_to_the_phase_three_values`).

1. Sidebar **Settings**.
2. In the Settings menu, click **Limits**.
3. In **Machine load limits**, in the field **CPU limit (percent)**, type `77`. Do not click **Save changes**.
4. In the Settings menu, click **Autonomy**.
5. In the Settings menu, click **Limits**.
6. Sidebar **Settings**.

**Expected:**

- Step 1: the page heading reads **Agents**. The intro reads "Defaults for new tasks." The menu item **Agents** is the current one.
- Step 2: the heading reads **Limits**. The intro reads "How much the machine takes on, and how often work is checked."
- Step 4: the heading reads **Autonomy**. The switch **Let the orchestrator act on its own** shows.
- Step 5: **CPU limit (percent)** reads `80`. The typed `77` is not shown.
- Step 6: the **Agents** page opens.

**Persistence:** nothing was saved in this test. Step 5 shows the page's last daemon read.

**Reset:** none. Nothing was saved.

**Known limitations:**

- The hash routes, such as `#/settings?page=limits`, and an unknown page id cannot be entered here. The app window has no address bar, and no control produces an unknown page id. When the page id is unknown, the code opens **Agents**. This is not tested here.
- Candidate defect: the typed `77` is dropped with no warning when the page is left. Record what you see. Do not treat this as the expected result.

## TC-ST-03 One Save changes bar per page, and one invalid field blocks the whole page

Purpose: only pages with a saved field show **Save changes**. An invalid field on one section stops the save of every section on that page.
**Source:** `apps/desktop/src/ui/views/settings/pageSave.tsx` (`SettingsPage`, `save`, the **Save changes** button); `apps/desktop/src/ui/views/settings/settingsPages.tsx` (`PAGES`); `docs/settings.md` "Checks on running work" and "Automatic replies".
**Real agent:** none. Cost: low.
**Preconditions:** Setup in `00-setup.md` sections 4 to 6 is done. The data folder is fresh: **CPU limit (percent)** is `80`, **Replies per hour, per conversation** is `20`, **Replies per day, per conversation** is `160`, and the daily token budget is empty.

1. Sidebar **Settings**. Click **Autonomy**, **Notifications**, **Appearance** and **Memory** in turn. On each, look for a **Save changes** button. Then click **Agents**, **Limits** and **Repositories** in turn and look for the same button.
2. Click **Limits**. With no edits, click **Save changes**.
3. In **CPU limit (percent)**, clear the field and type `90`. In **Replies per hour, per conversation**, clear the field and type `0`. Click **Save changes**.
4. In the Settings menu, click **Autonomy**, then click **Limits**. Read **CPU limit (percent)**.
5. In **Replies per hour, per conversation**, clear the field so it is empty. Click **Save changes**.

**Expected:**

- Step 1: **Save changes** is absent on **Autonomy**, **Notifications**, **Appearance** and **Memory**. It is present on **Agents**, **Limits** and **Repositories**.
- Step 2: the result region (aria label **Save result**) shows three lines: "Saved. The new limits apply from the next measurement.", "Saved. New checks use these settings from the next check." and "Saved. New limits apply from the next automatic reply."
- Step 3: under **Replies per hour, per conversation** the message reads "Enter a whole number from 1 to 1000, or leave it empty for no limit." It appears as an alert. Under the Save bar, the alert reads "Fix the fields marked as invalid before saving." No save runs. The three lines from step 2 stay on screen, and no new line appears.
- Step 4: **CPU limit (percent)** reads `80`. The `90` was not saved, which shows that the machine-load section was blocked too.
- Step 5: the save runs all three sections. The result region shows the same three lines. **Replies per hour, per conversation** is empty. The note "No limit means a busy fleet can spend tokens on automatic replies without a cap." is visible. It was already visible before this step, because the daily token budget is empty. It stays.

**Persistence:** step 4 shows the page's last daemon read. The restart check for every area is TC-ST-05, steps 8 and 9.

**Reset:** in **Replies per hour, per conversation**, clear the field and type `20`. Click **Save changes**. The result region shows the three lines, and the field reads `20`.

**Known limitations:**

- When two sections are invalid at once, the bar shows one reason. Type `49` in **CPU limit (percent)** and `0` in **Replies per hour, per conversation**, then click **Save changes**. Record which text shows. The machine-load text is "Enter whole numbers from 50 to 100 before saving." The wake text is "Fix the fields marked as invalid before saving."

## TC-ST-04 Locked controls show their reasons

Purpose: an app that did not start the daemon cannot change approver-only settings. The page shows why, and it disables or makes read-only the controls that need the approver.
**Source:** `apps/desktop/src/daemon/errors.ts` (`APPROVER_REQUIRED_NOTICE`); `apps/desktop/src/ui/views/settings/MachineLoadSection.tsx` (`readOnly={locked}`); `apps/desktop/src/ui/views/settings/DispatchDefaultsSection.tsx` (`disabled={locked}`); `apps/desktop/src/ui/views/settings/AutonomySection.tsx` (switch `disabled={locked}`); `apps/desktop/src/ui/views/settings/PreferencesSection.tsx` (buttons `disabled={busy || accessReason !== null}`); `crates/orchestrator-ipc/src/approver.rs` (`USER_ONLY_METHODS`, which does not name `projects.register`); `docs/settings.md` "When settings are locked".
**Real agent:** none. Cost: low.
**Preconditions:** The running daemon was not started by this app, so the app is not its approver. The app connects to a daemon that already runs on its data folder, and without that daemon's approver secret it is a non-approver (`apps/desktop/src-tauri/src/bridge.rs`). To build the state, quit the app and wait two minutes. In its own terminal, run `target/debug/orchestrator-daemon --data-dir "$HOME/agent-orch-manual-test/data"` (usage line in `crates/orchestrator-daemon/src/main.rs`). Then start the app as in `00-setup.md` section 4. TC-Q-07 uses the same state. Mark BLOCKED with the reason "app attached as approver" if the Save bar shows no locked text at step 1. Stop the terminal daemon after the test, and wait for the app and daemon to stop before the next test.

1. Sidebar **Settings** > **Agents**. Read the text under the Save bar. Do not click it.
2. Sidebar **Settings** > **Limits**. In **CPU limit (percent)**, try to type `85`.
3. Sidebar **Settings** > **Autonomy**. Look at the switch **Let the orchestrator act on its own**.
4. Sidebar **Settings** > **Repositories**. Click **Add repository**.
5. On **Repositories**, look at the field in **Home directory**.
6. Sidebar **Settings** > **Memory**. Open the disclosure **Add shared preference**. Look at the button **Add shared preference**.
7. Sidebar **Settings** > **Notifications**. Look at **Reminder interval**.

**Expected:**

- Step 1: the **Default provider** select is disabled. The text under the Save bar reads "This app did not start the orchestrator service that is running, so it cannot approve or reject for it. Close the app and open it again once the service has stopped, about a minute after running work finishes. Nothing is lost."
- Step 2: the CPU field is read-only. Typing changes nothing.
- Step 3: the switch is disabled. The same text shows under the switch.
- Step 4: the dialog **Add a repository** opens. Repository registration is not approver-only.
- Step 5: the **Home directory** field is disabled. The reason text is in the Save bar.
- Step 6: the button **Add shared preference** is disabled, and the same reason text shows.
- Step 7: the select is enabled. Notifications has no approver check in its code, so it is not locked.

**Persistence:** not applicable. Nothing is saved.

**Reset:** stop the hand-started daemon with Ctrl-C in its terminal. Quit the app and wait two minutes. Check that `pgrep -fl orchestrator-daemon` prints nothing. Start the app as in `00-setup.md` section 4.

**Known limitations:**

- The not-connected reason ("The app is not connected to the orchestrator service, so nothing can be saved.") needs the daemon to stop while the app is open. No documented control does that. It is not tested here.

## TC-ST-05 Settings survive an app and daemon restart

Purpose: each saved value comes back after a restart. The daemon values come from the daemon. The theme, the background and the reminder interval come from the device.
**Source:** `docs/settings.md` "Agent dispatch defaults", "Machine load limits", "Automatic replies" and "Home directory"; `apps/desktop/src/ui/theme/theme.ts` (`THEME_STORAGE_KEY`); `apps/desktop/src/ui/background/settings.ts` (`AMBIENT_STORAGE_KEY`); `apps/desktop/src/ui/views/settings/NotificationsSection.tsx`.
**Real agent:** none. Cost: low.
**Preconditions:** Setup in `00-setup.md` sections 4 to 6 is done. **Let the orchestrator act on its own** is on.
Before step 1, record and write into `ENVIRONMENT.md`: the **Dark mode** state (pressed or not), the **Background animation** state (on or off), and the **Reminder interval** value. The data folder reset does not clear these three, so the Reset line restores the recorded values.

1. Sidebar **Settings** > **Agents**. In **Default provider**, choose `No default (tasks must choose a provider)`. Click **Save changes**.
2. Sidebar **Settings** > **Limits**. In **CPU limit (percent)**, clear the field and type `85`. In **Check interval with one task running (minutes)**, clear the field and type `5`. In **Replies per hour, per conversation**, clear the field and type `7`. Click **Save changes**.
3. Sidebar **Settings** > **Autonomy**. Click the switch **Let the orchestrator act on its own** to turn it off. It saves on click.
4. Sidebar **Settings** > **Repositories**. In **Home directory**, clear the field. Click **Save changes**.
5. Sidebar **Settings** > **Memory**. Open the disclosure **Add shared preference**. In **New shared preference key**, type `probe-pref`. In **New shared preference text**, type `Check settings persist`. Click **Add shared preference**.
6. Sidebar **Settings** > **Appearance**. Click **Dark mode**. Open the **Background animation** control (the ≈ summary), then click **Background**.
7. Sidebar **Settings** > **Notifications**. In **Reminder interval**, choose `120 minutes`. Click **Save reminder interval**.
8. Quit the app. Wait two minutes. Run `pgrep -fl orchestrator-daemon`; it must print nothing. Start the app again as `00-setup.md` section 4 says. Do not run section 9: it deletes the data folder. Wait for the window (2 minutes).
9. Open each page in the order of steps 1 to 7 and read the values.

**Expected:**

- Step 1: the result reads "Saved. New tasks use these defaults."
- Step 2: three lines appear: "Saved. The new limits apply from the next measurement.", "Saved. New checks use these settings from the next check." and "Saved. New limits apply from the next automatic reply."
- Step 3: the result region for the switch reads "Saved."
- Step 4: the result reads "Cleared. A task with no clear project will ask where to run."
- Step 5: the result reads "Added probe-pref."
- Step 6: **Dark mode** is pressed. **Background** shows "Off", with no "(this session)" text.
- Step 7: no error text shows. The select reads `120 minutes`.
- Step 9, after the restart:
  - **Agents**: **Default provider** reads `No default (tasks must choose a provider)`.
  - **Limits**: **CPU limit (percent)** reads `85`. **Check interval with one task running (minutes)** reads `5`. **Replies per hour, per conversation** reads `7`.
  - **Autonomy**: the switch **Let the orchestrator act on its own** is off.
  - **Repositories**: **Home directory** is empty.
  - **Memory**: **Shared preferences** lists `probe-pref`, with the text "Check settings persist" and "Written by the user".
  - **Appearance**: **Dark mode** is pressed. **Background** reads "Off".
  - **Notifications**: **Reminder interval** reads `120 minutes`.

**Persistence:** this is the restart test. Step 8 quits and starts the app with the data folder kept. Step 9 reads the values back. The daemon values come from `settings.get`. The theme and background come from browser storage (`agent-orch.theme`, `agent-orch.ambient-background`). The reminder interval comes from the native notification state, whose file path is UNVERIFIED (`apps/desktop/src-tauri/src/notifications.rs`).

**Reset:** restore the setup values and the recorded device values.

1. **Agents**: in **Default provider**, choose `Claude Code`. In **Default model**, choose `Sonnet (alias)`. In **Default effort**, choose `medium`. Click **Save changes**. The result reads "Saved. New tasks use these defaults."
2. **Limits**: in **CPU limit (percent)**, type `80`. In **Check interval with one task running (minutes)**, type `10`. In **Replies per hour, per conversation**, type `20`. Click **Save changes**.
3. **Autonomy**: click the switch **Let the orchestrator act on its own** to turn it on.
4. **Repositories**: in **Home directory**, type `~/agent-orch-manual-test/projects/manual-test-project`. Click **Save changes**. The result reads "Saved. A task with no clear project now runs in" followed by the full path.
5. **Memory**: under **Shared preferences**, click **Remove** on `probe-pref`. The result reads "Removed probe-pref. It is under Removed entries."
6. **Appearance**: set **Dark mode** and **Background animation** to the states recorded before step 1.
7. **Notifications**: set **Reminder interval** to the recorded value. Click **Save reminder interval**.

**Known limitations:**

- UNVERIFIED: the browser storage keys may not survive an app restart in a `tauri dev` build. If **Dark mode** reads not pressed after the restart, record what you see.
- After one toggle, the theme cannot go back to following the system. The switch only sets light or dark (`apps/desktop/src/ui/theme/theme.ts`). The Reset line sets the recorded state explicitly.
- **Remove** archives the entry under **Removed entries**. It does not delete it. The `probe-pref` row stays there until the data folder is reset. TC-ST-18, if it shares this file, must allow for it.

## TC-ST-06 An older daemon hides sections (BLOCKED)

Purpose: each section hides itself when the daemon lacks its capability. A partial daemon hides only the missing group.
**Source:** `apps/desktop/src/ui/views/settings/MachineLoadSection.tsx` (the unsupported state); `apps/desktop/src/ui/views/settings/SupervisionSection.tsx` (`SupervisionSections`); `apps/desktop/src/ui/views/settings/AutonomySection.tsx` (switch hidden when its key is absent); `apps/desktop/src/ui/views/settings/HomeDirectorySection.tsx` (hidden when `home_directory` is absent); `apps/desktop/src/ui/views/settings/PreferencesSection.tsx` and `GrantsSection.tsx` (hidden unless `standing_approvals` is negotiated); `docs/settings.md`.
**Real agent:** none. Cost: low.
**Preconditions:** BLOCKED: the test needs a daemon build that lacks the `settings`, `standing_approvals` or `home_directory` capability. Such a build is an older commit. `RUNNER.md` hard rule 1 forbids testing a build from another commit. The runner records this test as BLOCKED with that reason.

1. Start the app against a daemon without the `settings` capability. Open **Settings** > **Limits**.
2. Start the app against a daemon without `standing_approvals`. Open **Settings** > **Autonomy** and **Settings** > **Memory**.
3. Start the app against a daemon without `home_directory`. Open **Settings** > **Repositories**.

**Expected:**

- Step 1: **Machine load limits** shows the heading "Settings are not available on this daemon." and the text "It does not offer the settings capability, so there are no machine load limits to change here."
- Step 2: the **Standing approvals** section and the **Shared preferences** list are hidden. Switches whose key the daemon does not send are hidden.
- Step 3: the **Home directory** section is hidden. **Your repositories** still shows.

**Persistence:** not applicable.

**Reset:** restart the app with the daemon from `00-setup.md` section 9.

**Known limitations:**

- A partial daemon that hides only one group in **Automatic replies** or **Checks on running work** is not tested here. The steps above describe the case; a future run must cover it.

## TC-ST-07 Agents: default provider, model and effort save, and the no-default state

Purpose: the default provider, model and effort save together and read back. Choosing **No default** hides the model and effort rows.
**Source:** `apps/desktop/src/ui/views/settings/DispatchDefaultsSection.tsx` (**Default provider**, `PairRows`, the **Save changes** bar); `apps/desktop/src/ui/views/settings/settingsModel.ts` (`saveFailureText`); `docs/settings.md` "Agent dispatch defaults".
**Real agent:** none. Cost: low. The effect on a new task is checked in TC-D-02 (Cost: medium).
**Preconditions:** Setup in `00-setup.md` sections 4 to 6 is done. The saved default is **Default provider** `Claude Code`, **Default model** `Sonnet (alias)`, and **Default effort** `medium`.

1. Sidebar **Settings** > **Agents**. In **Default model**, choose `Haiku (alias)`. Click **Save changes**.
2. In **Default effort**, choose the first option other than `Provider default effort`. Record its label. Click **Save changes**.
3. In **Default model**, choose `Provider default model`. In **Default effort**, choose `Provider default effort`. Click **Save changes**.
4. In **Default provider**, choose `No default (tasks must choose a provider)`. Click **Save changes**.
5. In **Default provider**, choose `Claude Code`. In **Default model**, choose `Haiku (alias)`. Do not save. Click **Clear defaults**. Read the page. Click **Save changes**.
6. In **Default provider**, choose `Claude Code`. In **Default model**, choose `Sonnet (alias)`. In **Default effort**, choose `medium`. Click **Save changes**.
7. Click the **Default provider** list and read every option.

**Expected:**

- Steps 1 to 6: after each save the result reads "Saved. New tasks use these defaults."
- Step 1, read-back: after leaving **Agents** and opening it again, **Default model** reads `Haiku (alias)`.
- Step 2, read-back: **Default effort** reads the label recorded in step 2.
- Step 3: **Default model** reads `Provider default model`. **Default effort** reads `Provider default effort`.
- Step 4: after the save, the rows **Default model** and **Default effort** are gone.
- Step 5, before the save: **Default provider** reads `No default (tasks must choose a provider)`, the rows **Default model** and **Default effort** are gone, and every provider's choice under **Per-provider overrides** is cleared (no "N set" count). After the save, and after opening another page and coming back: the same.
- Step 6: the rows come back. **Default model** reads `Sonnet (alias)`. **Default effort** reads `medium`.
- Step 7: each provider that is not launchable shows its name with " (unavailable)" and cannot be chosen. Record which providers show this way. UNVERIFIED on the test machine.

**Persistence:** the value after each step is the page's last daemon read. Restart check: TC-ST-05, steps 8 and 9.

**Reset:** step 6 is the reset. It sets `Claude Code`, `Sonnet (alias)` and `medium`.

**Known limitations:**

- A saved default whose agent is gone shows `<id> (unavailable)`. Saving with an unavailable default is refused. The text is "The record is not in a state that allows this action. Refresh to see the current state before trying again." No control makes that state, so it is not tested here.

## TC-ST-08 Agents: per-provider overrides, the count and the reset

Purpose: a choice for a provider other than the default is counted, saved, and removed when both of its fields go back to the provider default.
**Source:** `apps/desktop/src/ui/views/settings/DispatchDefaultsSection.tsx` (**Per-provider overrides**, the `N set` count, `choose`); `docs/settings.md` "Agent dispatch defaults".
**Real agent:** none. Cost: low.
**Preconditions:** Setup in `00-setup.md` sections 4 to 6 is done. No per-provider choice is saved on a fresh data folder. The **Per-provider overrides** list shows **Codex**. If it does not, record BLOCKED with that reason.

1. Sidebar **Settings** > **Agents**. Click **Per-provider overrides** to open it.
2. Under **Codex**, in **Codex model**, choose the first model in the list. Record its label. In **Codex effort**, choose the first effort in the list. Record it. Click **Save changes**.
3. In **Codex model**, choose `Provider default model`. Click **Save changes**.
4. In **Codex effort**, choose `Provider default effort`. Click **Save changes**.

**Expected:**

- Step 1: the summary **Per-provider overrides** shows no count.
- Step 2: the summary shows `1 set`. The result reads "Saved. New tasks use these defaults."
- Step 3: the summary still shows `1 set`, because the effort is still set. The result reads "Saved. New tasks use these defaults."
- Step 4: after the save, the summary shows no count. The provider's entry is deleted, because both of its fields are back to the provider default.

**Persistence:** the value after step 4 is the page's last daemon read. Restart check: TC-ST-05, steps 8 and 9.

**Reset:** step 4 is the reset. If a step failed, set both **Codex** fields to their "Provider default" options and click **Save changes**.

**Known limitations:**

- The **Clear <provider> choice** button appears only for a saved provider id that is not in the catalog. No control creates that state. It is not tested here.
- The provider names, models and efforts come from the catalog on the test machine. They are recorded in the reply, not written here. UNVERIFIED.
- Opening **Agents** reads the model catalog, which runs local `codex` status commands (`crates/orchestrator-agents/src/catalog.rs`). These are probes, not task runs. `RUNNER.md` says Codex is used only by TC-D-03. The lead must decide whether this page is allowed.

## TC-ST-09 Agents: installed adapters, agent detection and Refresh models

Purpose: the adapter table shows the daemon's report as read-only text. **Refresh models** does not start a billable probe.
**Source:** `apps/desktop/src/ui/views/settings/AgentsInventorySection.tsx` (**Installed adapters**, the **Adapter inventory** table and its column headers); `apps/desktop/src/ui/views/settings/inventoryModel.ts` (the availability texts, "Not reported", the not-detected note); `apps/desktop/src/ui/views/settings/AgentDetectionSection.tsx`; `apps/desktop/src/ui/views/settings/DispatchDefaultsSection.tsx` (**Refresh models**); `crates/orchestrator-agents/src/catalog.rs` (Copilot's model list is static).
**Real agent:** none. Cost: low.
**Preconditions:** Setup in `00-setup.md` sections 4 to 6 is done.

1. Sidebar **Settings** > **Agents**. Click **Installed adapters** to open it.
2. Read the table **Adapter inventory**.
3. Open **Agent detection**, which sits inside **Installed adapters**. Read **Detected agents** and **Execution availability**.
4. Click **Refresh models**. Read the screen while it reloads.
5. In a terminal, run `pgrep -fl copilot` and note the output. Click **Refresh models** again. Run `pgrep -fl copilot` at once and again five seconds later.

**Expected:**

- Step 2: the columns are **Adapter**, **Availability**, **Resume**, **Interactive input** and **Capabilities as reported**. Each availability reads one of: "Installed and launchable", "Installed, not launchable", "Launchable, not detected" or "Not detected". Resume and Interactive input read Yes, No or Unknown. A row with no report shows **Not reported**. A row with an empty list shows **None reported**. Record which rows show each.
- Step 2, not detected: an agent that is not detected shows the note "Capabilities are not reported because the agent was not detected." UNVERIFIED on this machine, because it depends on which agents are installed.
- Step 3: **Detected agents** lists each agent with its availability. **Execution availability** reads "Launch available: yes." or "Launch available: no.", followed by a reason.
- Step 4: "Loading the model catalog..." shows while it reloads. Then the provider list shows again, with no error.
- Step 5: `pgrep -fl copilot` prints the same result before the click, during it and five seconds after it.

**Persistence:** not applicable. The page only reads.

**Reset:** none. Nothing was saved.

**Known limitations:**

- The failure state ("Adapter inventory could not be loaded" with **Retry**) needs the daemon to fail. No documented control stops the daemon with the app open, so it is not tested here.
- The app does not show the raw `agents.capabilities` reply. The table can be checked only against itself, not against the daemon's reply. UNVERIFIED.
- Step 5 proves something only if the Copilot CLI is installed on the test machine. Whether it is installed is UNVERIFIED. If it is not installed, step 5 proves nothing.

## TC-ST-10 Limits: machine load limits, validation and save

Purpose: each limit accepts a whole number from 50 to 100. Anything else shows a message on its field and blocks the save.
**Source:** `apps/desktop/src/ui/views/settings/settingsModel.ts` (`parseLimit`, `LIMIT_ERROR`, `OFF_NOTE`, `DISK_NOTE`, `SAVED_TEXT`, `INVALID_REASON`); `apps/desktop/src/ui/views/settings/MachineLoadSection.tsx` (`LimitRow`); `docs/settings.md` "Machine load limits".
**Real agent:** none. Cost: low. The gate's effect on a new task is checked in TC-D-11 (Cost: medium).
**Preconditions:** Setup in `00-setup.md` sections 4 to 6 is done. The data folder is fresh: **CPU limit (percent)** is `80`, **Memory limit (percent)** is `85`, and **Disk limit (percent)** is `90`.

1. Sidebar **Settings** > **Limits**. Find **Machine load limits**.
2. In **CPU limit (percent)**, clear the field and type `49`. Read the message. Click **Save changes**.
3. Clear the field, type `50`. Read the message.
4. Clear the field, type `100`. Read the message.
5. Clear the field, type `101`. Read the message.
6. Clear the field, leave it empty. Read the message.
7. Clear the field, type `50.5`. Read the message.
8. Clear the field, type `0100`. Read the message.
9. Clear the field, type `abc`. Record what the field holds.
10. Clear the field, type `85`. Click **Save changes**.
11. Open **Autonomy**, then open **Limits** again. Read **CPU limit (percent)**.
12. Find **Current machine load** on the same page. Read it.

**Expected:**

- Step 2: under the field: "Enter a whole number from 50 to 100." (an alert). Under the Save bar: "Enter whole numbers from 50 to 100 before saving." (an alert, because a save was tried). No result line appears.
- Step 3: no message.
- Step 4: no message. The text "Off: tasks start whatever the load." shows beside the field.
- Step 5: "Enter a whole number from 50 to 100."
- Step 6: "Enter a whole number from 50 to 100."
- Step 7: "Enter a whole number from 50 to 100."
- Step 8: no message. The value is accepted as 100, and the off text shows.
- Step 9: the field holds what the browser allowed. Record it. No expected value.
- Step 10: the result reads "Saved. The new limits apply from the next measurement."
- Step 11: **CPU limit (percent)** reads `85`. This is the daemon's value.
- Step 12: the heading **Current machine load** shows. When the daemon reports its agent slots, the line "<used> of <limit> agent slots in use · <n> available" shows above the readings. The readings (CPU, memory, GPU and disk) show open. Record them. They change with the machine, so there is no expected value.
- Throughout: under **Disk limit (percent)** the note "Measured on the disk that holds the app's data" shows.

**Persistence:** step 11 shows the value the page reads after the save. Restart check: TC-ST-05, steps 8 and 9.

**Reset:** clear the field, type `80`. Click **Save changes**. The result reads "Saved. The new limits apply from the next measurement."

**Known limitations:**

- The field is `type="number"`. The browser may refuse letters and spaces. Typing `abc` and ` 75 ` may not give the values this test lists. Record what the field holds.
- The failure state reads "The machine load limits could not be read.", followed by the reason and a **Retry** button. It needs the daemon to fail, and no documented control does that with the app open. It is not tested here.

## TC-ST-11 Limits: checks on running work

Purpose: the three fields, their ranges, the cross-field rule and the preview line. A value that is out of range is refused on its field, and a valid one is saved.
**Source:** `apps/desktop/src/ui/views/settings/supervisionModel.ts` (`supervisionErrors`, `previewText`, `durationText`, `CROSS_FIELD_ERROR`, `SUPERVISION_SAVED_TEXT`); `apps/desktop/src/ui/views/settings/SupervisionSection.tsx` (the field hints); `docs/settings.md` "Checks on running work".
**Real agent:** none. Cost: low to save and read back. The effect on a running task is checked in TC-S-09 (Cost: medium), which needs a held task. This test does not run a task.
**Preconditions:** Setup in `00-setup.md` sections 4 to 6 is done. The data folder is fresh: **Check interval with one task running (minutes)** is `10`, **Fastest check interval (seconds)** is `60`, and **Longest time a task may run before it is flagged (minutes)** is `120`. The preview at the start reads: "With 1 task: every 10 min. 2 tasks: 5 min. 3: 2 min 30 s. 4: 1 min 15 s. 5 or more: 1 min."

In each step, clear the field before you type.

1. Sidebar **Settings** > **Limits**. Find **Checks on running work**. Read the preview line.
2. In **Check interval with one task running (minutes)**, type `1`.
3. Type `240`.
4. Type `241`.
5. Type `10`.
6. In **Fastest check interval (seconds)**, type `9`.
7. Type `10`.
8. Type `601`.
9. Type `60`.
10. Type `120` in **Fastest check interval (seconds)**. Click **Save changes**.
11. In **Check interval with one task running (minutes)**, type `1`. Read the message under **Fastest check interval (seconds)**.
12. Type `10` in **Check interval with one task running (minutes)**.
13. In **Longest time a task may run before it is flagged (minutes)**, type `4`. Type `5`. Type `1440`. Type `1441`. Type `120`.
14. In **Fastest check interval (seconds)**, type `60`. Click **Save changes**.
15. Open **Autonomy**, then open **Limits** again. Read the three fields.

**Expected:**

- Step 1: "With 1 task: every 10 min. 2 tasks: 5 min. 3: 2 min 30 s. 4: 1 min 15 s. 5 or more: 1 min."
- Step 2: no message. The preview reads "Checks always run every 1 min." (the base equals the fastest interval).
- Step 3: no message. The preview reads "With 1 task: every 240 min. 2 tasks: 120 min. 3: 60 min. 4: 30 min. 5: 15 min. More tasks: down to 1 min."
- Step 4: under the base field: "Enter a whole number from 1 to 240." (an alert). The preview returns to the first text, because the saved 10 minutes is used.
- Step 5: no message. The preview reads "With 1 task: every 10 min. 2 tasks: 5 min. 3: 2 min 30 s. 4: 1 min 15 s. 5 or more: 1 min."
- Step 6: under **Fastest check interval (seconds)**: "Enter a whole number from 10 to 600." The base is 600 seconds. The preview line is empty.
- Step 7: no message. The preview reads "With 1 task: every 10 min. 2 tasks: 5 min. 3: 2 min 30 s. 4: 1 min 15 s. 5: 38 s. More tasks: down to 10 s."
- Step 8: under **Fastest check interval (seconds)**: "The fastest interval cannot be slower than the check interval."
- Step 9: no message.
- Step 10: the result reads "Saved. New checks use these settings from the next check."
- Step 11: under **Fastest check interval (seconds)**: "The fastest interval cannot be slower than the check interval." The saved `120` is now above the typed 1-minute base. No message under the base field.
- Step 12: the message clears.
- Step 13: `4` shows "Enter a whole number from 5 to 1440." under **Longest time a task may run before it is flagged (minutes)**. `5` and `1440` show no message. `1441` shows "Enter a whole number from 5 to 1440." `120` shows no message.
- Step 14: the result reads "Saved. New checks use these settings from the next check."
- Step 15: the fields read `10`, `60` and `120`.

**Persistence:** step 15 shows the value the page reads after the save. Restart check: TC-ST-05, steps 8 and 9.

**Reset:** none beyond step 14. After it the fields read `10`, `60` and `120`. If a step failed, type those values and click **Save changes**.

**Known limitations:**

- The effect on a running task is not checked here. TC-S-09 covers how checks are counted, but it needs a held task (Cost: medium).
- The preview numbers come from the same functions the page uses (`supervisionModel.ts`). They must match the screen. If they differ, record what you see.

## TC-ST-12 Limits: automatic replies caps and the token budget

Purpose: each cap accepts a number in range, or empty, `off` or `No limit` for no limit. Any other value is refused on its field and blocks the save.
**Source:** `apps/desktop/src/ui/views/settings/supervisionModel.ts` (`parseCap`, `wakeError`, `NO_LIMIT_NOTE`, `WAKE_SAVED_TEXT`, `WAKE_LABEL`, `WAKE_HINT`); `apps/desktop/src/ui/views/settings/SupervisionSection.tsx` (`RepliesSection`); `docs/settings.md` "Automatic replies".
**Real agent:** none. Cost: low. A cap's effect on a reply is checked in TC-W-08 (Cost: medium). The defaults are checked in TC-E-04 (Cost: low).
**Preconditions:** Setup in `00-setup.md` sections 4 to 6 is done. The data folder is fresh: **Replies per hour, per conversation** is `20`, **Replies per day, per conversation** is `160`, and **Daily token budget for automatic replies** is empty. The note "No limit means a busy fleet can spend tokens on automatic replies without a cap." is shown from the start, because the token budget is empty.

In each step, clear the field before you type.

1. Sidebar **Settings** > **Limits**. Find **Automatic replies**.
2. In **Replies per hour, per conversation**, type `off`. Read the message and the note.
3. Type `No limit`. Read the message.
4. Type `0`. Read the message.
5. Type `1`. Read the message.
6. Type `1000`. Read the message.
7. Type `1001`. Read the message.
8. In **Replies per day, per conversation**, type `10000`. Read the message. Type `10001`. Read the message.
9. In **Daily token budget for automatic replies**, type `0`. Read the message. Type `1`. Read the message.
10. Clear **Daily token budget for automatic replies**, so it is empty.
11. In **Replies per hour, per conversation**, type `0`. Click **Save changes**. Read the Save bar and the result region.
12. In **Replies per hour, per conversation**, type `20`. Clear **Replies per day, per conversation**. Click **Save changes**. Read the result. Then leave the page and open it again.
13. In **Replies per day, per conversation**, type `160`. Click **Save changes**. Read the result.

**Expected:**

- Step 2: no message. The field holds "off". The note is visible.
- Step 3: no message.
- Step 4: "Enter a whole number from 1 to 1000, or leave it empty for no limit."
- Step 5: no message.
- Step 6: no message.
- Step 7: "Enter a whole number from 1 to 1000, or leave it empty for no limit."
- Step 8: `10000` shows no message. `10001` shows "Enter a whole number from 1 to 10000, or leave it empty for no limit."
- Step 9: `0` shows "Enter a whole number above 0, or leave it empty for no budget." `1` shows no message.
- Step 10: no message. The token budget is empty again.
- Step 11: under the field: "Enter a whole number from 1 to 1000, or leave it empty for no limit." Under the Save bar: "Fix the fields marked as invalid before saving." (an alert). The result region shows no new line, and nothing is saved.
- Step 12: the result reads "Saved. New limits apply from the next automatic reply." After reopening, **Replies per hour, per conversation** reads `20`. **Replies per day, per conversation** is empty, which means the daemon stored no limit for it.
- Step 13: the result reads "Saved. New limits apply from the next automatic reply." The fields read `20`, `160` and empty.

**Persistence:** step 12 shows the value the page reads after the save. Restart check: TC-ST-05, steps 8 and 9.

**Reset:** step 13 restores the values. If a step failed, type `20` in **Replies per hour, per conversation** and `160` in **Replies per day, per conversation**. Leave **Daily token budget for automatic replies** empty. Click **Save changes**.

**Known limitations:**

- The token budget `1` is typed only. Step 11 is blocked and step 12 leaves the budget empty, so `1` is never saved here.
- The page checks each value before it sends the save. The daemon's own range check is not seen here.

## TC-ST-13 Autonomy switches save on click and keep their value

Purpose: each switch saves when you click it, shows its own result line, and keeps its value after a restart.
**Source:** `docs/settings.md` "Chat options"; `docs/security.md` section 6; `apps/desktop/src/ui/views/settings/AutonomySection.tsx`. The defaults are in `crates/orchestrator-daemon/src/api_settings.rs` (comments "On by default" and "Off by default").
**Real agent:** none. Cost: low.
**Preconditions:** Setup sections 6 and 9 done. The switch **Let the orchestrator act on its own** is on. The other two switches are off.

1. Click **Settings** in the sidebar, then **Autonomy** in **Settings pages**. In **Autonomy switches**, read the three switches.
2. Click the switch **Let the orchestrator act on its own**. Read the result line under it.
3. Open **Details** under that switch. Read the text.
4. Click the switch **Propose tasks that end in a pull request**. Read the result line under it.
5. Click the switch **Read-only task tools in chat**. Read the result line under it.
6. Quit the app. Wait two minutes and check that `pgrep -fl orchestrator-daemon` prints nothing. Start the app as `00-setup.md` section 4 says. Do not run section 9, because it deletes the data folder. Open **Settings > Autonomy** and read the three switches.
7. Look for a button named **Save changes** on the Autonomy page.

**Expected:**

- Step 1: **Let the orchestrator act on its own** is on, with the hint "Starts and retries tasks without asking." **Propose tasks that end in a pull request** is off, with the hint "You still approve each one." **Read-only task tools in chat** is off, with the hint "Lets chat look up a task's state and report. Read only."
- Step 2: the result line reads "Saved." The switch is off. There is no Save button.
- Step 3: the text starts "When this is on, the orchestrator starts tasks, retries them," and ends "When this is off, every action waits for your approval, unless a standing approval covers it."
- Step 4: the result line reads "Saved. New proposals follow this setting; proposals already waiting are checked again when you approve them."
- Step 5: the result line reads "Saved. The next message in the orchestrator chat follows this setting."
- Step 6: after the restart, **Let the orchestrator act on its own** is off. The other two switches are on. The page reads these values from the daemon's settings.
- Step 7: no such button. The switches save on click.

**Reset:** Turn **Let the orchestrator act on its own** on, its default. Turn **Propose tasks that end in a pull request** off and **Read-only task tools in chat** off, their defaults. Each click shows its "Saved." line.

**Known limitations:**

- This test does not check what the switches do to tasks. TC-A-03 checks the switch off. TC-A-05 checks the pull request switch. TC-A-01 checks the default. No test checks the effect of **Read-only task tools in chat**.
- If the daemon refuses a save, the switch should return to its saved value and show a failure notice (`AutonomySection.tsx`). The switches are disabled when the app is not connected or is not the approver, so a refused save cannot be made from the UI in a normal run. The locked states are in TC-ST-04.

## TC-ST-14 Standing approvals: create, check the limits, revoke

Purpose: a standing approval is made only with a project, at least one kind and valid limits. It can be revoked.
**Source:** `docs/settings.md` "Standing approvals"; `apps/desktop/src/ui/views/settings/GrantsSection.tsx`; `apps/desktop/src/ui/views/settings/grantsModel.ts`. The limits are in `crates/orchestrator-core/src/standing_grant.rs`.
**Real agent:** none. Cost: low.
**Preconditions:** Setup sections 5 and 6 done, so `manual-test-project` is registered. No standing approval exists.

1. Click **Settings**, then **Autonomy**. In **Standing approvals**, click **Add**.
2. In the dialog **Add a standing approval**, change nothing. Read the line above **Create standing approval**.
3. Read the boxes under **Kinds of action**. Read the fields **Expires in (hours)**, **Uses** and **Token budget**.
4. In **Covers**, keep **One project**. In **Project**, choose `manual-test-project`.
5. In **Expires in (hours)**, select the text and type `0`. Then type `1`, then `168`, then `169`, then `1e3`, then `2.5`. Read the field message after each.
6. In **Uses**, type `0`. Then type `1`, then `200`, then `201`. Read the field message after each.
7. In **Token budget**, type `0`. Then type `1`, then `1000000000`, then `1000000001`. Read the field message after each. Then clear the field.
8. Set **Expires in (hours)** to `8` and **Uses** to `20`. Set **Token budget** to `100000`. Tick **Propose a task** only. Click **Create standing approval**.
9. Read the new row. Quit the app, wait two minutes, and start it as `00-setup.md` section 4 says. Open **Settings > Autonomy** and read the row again.
10. Click **Revoke** on the row. Read the result line and the row.

**Expected:**

- Step 1: the dialog title is "Add a standing approval". The **Cancel** button closes it.
- Step 2: the line above the button reads "Choose a project or workstream, at least one kind of action, and limits within the ranges shown." **Create standing approval** is disabled. No field message shows yet.
- Step 3: **Kinds of action** shows six boxes, in this order: "Update a workstream (title and summary only)", "Add an objective", "Revise an objective", "Propose a task", "Retry a task", "Link a task to a workstream". "Create a workstream" and "Merge pull requests without asking" are not offered. **Expires in (hours)** holds `8`, with the hint "Default 8, at most 168 (7 days). The approval ends then." **Uses** holds `20`, with the hint "Default 20, at most 200. Each action it approves is one use." **Token budget** is empty. Its hint reads "The most tokens the tasks started under this approval may use. It counts the tokens reported so far, and tasks whose agent reports none count as zero. It blocks only new uses. It does not stop a task that is already running."
- Step 4: the **Project** list offers "Choose a project" and `manual-test-project`.
- Step 5: `0`, `169`, `1e3` and `2.5` each show "Enter a whole number of hours from 1 to 168." `1` and `168` show no message.
- Step 6: `0` and `201` each show "Enter a whole number of uses from 1 to 200." `1` and `200` show no message.
- Step 7: `0` and `1000000001` each show "Enter a whole number of tokens from 1 to 1,000,000,000." `1` and `1000000000` show no message. The cleared field shows the same message as `0`.
- Step 8: the line above the button is gone and **Create standing approval** is enabled. The dialog closes. The section shows "Standing approval created. It covers Project manual-test-project until" and the time 8 hours from now, as the app formats it. The row shows "Project manual-test-project", the pill "Active", the line "Propose a task", and "20 of 20 uses left · 8 hours left · 0 of 100,000 tokens reported so far".
- Step 9: after the restart, the row still shows "Active" and "20 of 20 uses left". The hours left are fewer than before the restart. The list is read from the daemon.
- Step 10: the **Revoke** button is there only while the row is Active. The result line reads "Standing approval revoked." The row shows the pill "Revoked", and **Revoke** is gone.

**Reset:** If the approval from step 8 is still Active, click **Revoke** on its row. The result line reads "Standing approval revoked."

**Known limitations:**

- The line "Standing approval revoked. N pending action(s) stopped." appears when an action waits on the approval. This test makes no such action. If you see that line, record its wording.
- The form checks the same limits as the daemon. The daemon refusals, "The daemon refused these terms" and "Only the app that started the service can create standing approvals", are not reached from the form in a normal run. The locked state is in TC-ST-04.
- The effect of an active approval, where an action runs without asking, needs a task run. It is in TC-D-13 (medium).

## TC-ST-15 Standing approvals: unattended merges

Purpose: a merge approval needs a project, a branch prefix, the confirmation and valid limits. It is made only when all of them are valid.
**Source:** `docs/settings.md` "Unattended merges" (under "Standing approvals"); `apps/desktop/src/ui/views/settings/GrantsSection.tsx`; `apps/desktop/src/ui/views/settings/grantsModel.ts`; `apps/desktop/src/sources/surfaces/projectDefaults.ts` (`isSafePrefix`). The limits are in `crates/orchestrator-core/src/standing_grant.rs`.
**Real agent:** none. Cost: low.
**Preconditions:** Setup sections 5 and 6 done. No standing approval exists.

1. Click **Settings**, then **Autonomy**. In **Standing approvals**, click **Unattended merges (advanced)**.
2. Read the form **Allow unattended merges**. Read the line above **Save unattended merge approval**.
3. Clear **Branch prefix**. Click **Save unattended merge approval**.
4. In **Branch prefix**, type `mcao` (no slash). Read the message. Then replace it with `orchestrator/`. Read the message.
5. Set **Branch prefix** to `mcao/`. In **Merge approval expires in (hours)**, type `0`, then `25`. Read each message. In **Merge approval uses**, type `0`, then `21`. Read each message. In **Merge approval token budget**, type `0`, then `1000000001`. Read each message. Then clear the field.
6. In **Project for unattended merges**, choose `manual-test-project`. Set **Merge approval expires in (hours)** to `8`, **Merge approval uses** to `5` and **Merge approval token budget** to `100000`. Leave the confirmation box unticked. Click **Save unattended merge approval**.
7. Tick the confirmation box. Click **Save unattended merge approval**.
8. Read the new row. Quit the app, wait two minutes, and start it as `00-setup.md` section 4 says. Open **Settings > Autonomy** and read the row again.

**Expected:**

- Step 1: the form opens. Its intro reads "Danger. While this approval is on, the orchestrator merges pull requests for one project without asking you each time. Nothing is on until you save it."
- Step 2: **Project for unattended merges** shows "Choose a project". **Branch prefix** holds `mcao/`, with the hint "Merge only pull requests whose branch starts with this, for example mcao/. Required." **Merge approval expires in (hours)** holds `8`, with the hint "Default 8, at most 24. The approval ends then." **Merge approval uses** holds `5`, with the hint "Default 5, at most 20. Each merge is one use." **Merge approval token budget** is empty, with the hint "A merge uses no tokens, so the budget never runs out; it is required for every grant." The confirmation text reads "I understand this merges pull requests without asking me each time, only when every required check has passed and the head commit is the one that was checked. A merge cannot be undone from this app." The line above the button reads "Choose a project, enter a branch prefix, tick the confirmation, and keep the limits within the ranges shown."
- Step 3: nothing is created. The field message reads "Enter the branch prefix this approval may merge, for example mcao/." The alert reads "Choose a project, enter a branch prefix, tick the confirmation, and keep the limits within the ranges shown."
- Step 4: `mcao` and `orchestrator/` each show "Use letters, digits, dot, dash, underscore and slash, ending with a slash. Do not start with orchestrator/."
- Step 5: hours `0` and `25` each show "Enter a whole number of hours from 1 to 24." Uses `0` and `21` each show "Enter a whole number of uses from 1 to 20." Token budget `0` and `1000000001` each show "Enter a whole number of tokens from 1 to 1,000,000,000."
- Step 6: nothing is created. The field message under the confirmation box reads "Tick the box to confirm."
- Step 7: the result line reads "Standing approval created. It covers Project manual-test-project until" and the time 8 hours from now. The row shows "Project manual-test-project", the pills "Active" and "Unattended merge", the line "Merge pull requests without asking", the line "Only branches starting with mcao/", and "5 of 5 uses left · 8 hours left · 0 of 100,000 tokens reported so far".
- Step 8: after the restart, the row still shows "Active", "Unattended merge" and "Only branches starting with mcao/".

**Reset:** Click **Revoke** on the merge row. The result line reads "Standing approval revoked."

**Known limitations:**

- No pull request is merged in this test. Whether the approval merges a real pull request needs a pull request and a task run. This file does not write that check.
- The form only shows **Unattended merges (advanced)** when the daemon sends `merge_kind_available` (`crates/orchestrator-daemon/src/api_grants.rs`). This test assumes the daemon sends it. The case where the form is absent is not tested here.
- The merge refusal text, "The daemon refused this unattended merge approval", is not reached from the form in a normal run.
- The 64-character limit on the prefix is not tested.

## TC-ST-16 Repositories: add a repository

Purpose: a repository is registered only from an absolute path to a Git repository. The list shows what is registered.
**Source:** `apps/desktop/src/ui/views/settings/RepositoriesSection.tsx`; `crates/orchestrator-daemon/src/api.rs` (`register_project`); `docs/manual-tests/00-setup.md` section 5. No section of `docs/settings.md` covers this.
**Real agent:** none. Cost: low.
**Preconditions:** Setup section 5 done, so `manual-test-project` is listed. `other-project` is not registered yet. Run `mkdir -p "$HOME/agent-orch-manual-test/not-a-repo"` to make an empty folder that is not a Git repository. The field needs a full path. Replace `$HOME` with your home folder, for example `/Users/<you>`. The field does not expand `~` or `$HOME`.

1. Click **Settings**, then **Repositories**. In **Your repositories**, click **Add repository**.
2. In the dialog **Add a repository**, read the form. Type `repo` in **Repository path**. Click **Register repository**.
3. Clear the field. Type `~/repo`. Click **Register repository**.
4. Clear the field. Type `C:\repo`. Click **Register repository**.
5. Clear the field. Type the full path of `not-a-repo` from the Preconditions. Click **Register repository**.
6. Clear the field. Type the full path of `$HOME/agent-orch-manual-test/projects/other-project`. Click **Register repository**.
7. Click **Add repository** again. Type the same full path of `other-project`. Click **Register repository**.
8. Quit the app, wait two minutes, and start it as `00-setup.md` section 4 says. Open **Settings > Repositories** and read **Registered repositories**.

**Expected:**

- Step 1: the dialog title is "Add a repository". The field **Repository path** has the placeholder `/path/to/repository`. Its hint reads "Enter the full path of a Git repository on this machine." **Register repository** is disabled while the field is empty.
- Step 2: the alert "Enter an absolute repository path." shows. The dialog stays open. Nothing is registered.
- Step 3: the same alert shows.
- Step 4: the dialog shows an error with the title "Registering the repository failed." Then it shows "The daemon rejected the request as invalid. Check the entered values and try again." Then a detail in brackets that starts with "(invalid-params". The dialog stays open.
- Step 5: the same error shows as in step 4.
- Step 6: the dialog closes. The section shows "Repository registered." **Registered repositories** lists `other-project` with its full path shown as code.
- Step 7: the dialog closes. "Repository registered." shows again. See Known limitations.
- Step 8: after the restart, **Registered repositories** still lists `other-project`, once. The list is read from the projects the daemon holds.

**Reset:** No Settings control removes a repository. `RepositoriesSection.tsx` only has **Add repository**. Leave `other-project` registered. Setup section 9 deletes the data folder before the next file.

**Known limitations:**

- Step 7 comes from code reading and has not been seen in a run. The daemon restores a path that was already registered, and does nothing when the project is active (`api.rs`, `register_project`). Record what you see. If the result differs, record it as a finding.
- A folder that is not a Git repository gets the same generic message as a bad path. The daemon reports it as invalid parameters, so the app does not say "not a Git repository". Record what you see.
- This test does not quote the daemon's own message in the bracketed detail. That text has not been seen in a run.
- The offline state (the draft is kept and the button is disabled) is not tested. No control in the app goes offline.
- Symbolic links and paths with a trailing slash are not tested. The code does not settle them.

## TC-ST-17 Repositories: home directory

Purpose: the home directory takes an existing folder only. An empty field or a field with only spaces clears it.
**Source:** `docs/settings.md` "Home directory"; `apps/desktop/src/ui/views/settings/HomeDirectorySection.tsx`; `crates/orchestrator-daemon/src/api_home_directory.rs`.
**Real agent:** none. Cost: low.
**Preconditions:** Setup sections 5 and 6 done. The home directory is `~/agent-orch-manual-test/projects/manual-test-project` (setup section 6, step 2).

1. Click **Settings**, then **Repositories**. In **Home directory**, read the field.
2. Clear the field. Type `~/agent-orch-manual-test/missing-folder`. Click **Save changes**.
3. Clear the field. Type `~/agent-orch-manual-test/projects/manual-test-project/README.md`. Click **Save changes**.
4. Clear the field. Type `projects/manual-test-project`. Click **Save changes**.
5. Clear the field. Type `~/agent-orch-manual-test/projects/manual-test-project`. Press **Enter** in the field. Do not click **Save changes**.
6. Clear the field so it is empty. Click **Save changes**.
7. Type three spaces in the field. Click **Save changes**.
8. Type `~/agent-orch-manual-test/projects/manual-test-project`. Click **Save changes**.
9. Quit the app, wait two minutes, and start it as `00-setup.md` section 4 says. Open **Settings > Repositories** and read **Home directory**.

**Expected:**

- Step 1: the field shows the full path of `manual-test-project`. The daemon writes the full path back. The hint reads "Enter the full path of a folder on this machine, for example ~/Code. Leave it empty to set none." The intro reads "Where a task runs when it is not clear which project it belongs to."
- Step 2: the **Save result** line on the page reads "That folder was not found. Enter the full path of a folder that exists on this machine." The field keeps the text you typed.
- Step 3: the same text shows.
- Step 4: the same text shows.
- Step 5: the **Save result** line reads "Saved. A task with no clear project now runs in" and then the full path of `manual-test-project`.
- Step 6: the **Save result** line reads "Cleared. A task with no clear project will ask where to run."
- Step 7: the same "Cleared." line shows. Spaces count as empty.
- Step 8: the result reads "Saved. A task with no clear project now runs in" and the full path of `manual-test-project`.
- Step 9: after the restart, the field shows the full path of `manual-test-project`. The page reads the value from the daemon's settings.

**Reset:** Step 8 leaves the home directory at `manual-test-project`. If steps 6 or 7 cleared it, run step 8 again. This matches setup section 6, step 2.

**Known limitations:**

- The folder becomes an in-place project when the setting is saved (`api_home_directory.rs`). Where it shows in the app is not checked here.
- `~` alone and `~name` are not tested. The code is not clear about `~` alone.
- The effect on a task is in TC-D-08 (medium), which checks a task run in the home directory. TC-D-09 checks the case with no home directory.

## TC-ST-18 Memory: entry size limits, edit and remove

Purpose: a key may be 40 bytes at most. Text may be 280 bytes at most. An edit or a remove keeps a way back.
**Source:** `docs/settings.md` "Preferences and learnings"; `apps/desktop/src/ui/views/settings/PreferencesSection.tsx`; `apps/desktop/src/ui/views/settings/preferencesModel.ts`. The limits are in `crates/orchestrator-core/src/memory.rs` (`MAX_KEY_BYTES`, `MAX_TEXT_BYTES`).
**Real agent:** none. Cost: low.
**Preconditions:** Setup sections 5 and 6 done. **Memory** has no entries.

1. Click **Settings**, then **Memory**. In **Shared preferences**, open **Add shared preference**. Type the key `abcdefghijabcdefghijabcdefghijabcdefghij` (40 characters) and the text `one`. Click **Add shared preference**.
2. Type the key `abcdefghijabcdefghijabcdefghijabcdefghijk` (41 characters). Do not type text.
3. Type the key `st-note`. Type the text as 281 lowercase `x` characters.
4. Replace the text with 141 copies of the letter `é`.
5. Replace the text with 140 copies of the letter `é`. Click **Add shared preference**.
6. Clear both fields. Type the key `st-paste`. Paste a text that contains a tab character. Copy one from a text editor, because a tab cannot be typed in this field. Read the field message. Then clear both fields.
7. In the list, click **Edit** on `st-note`. Do not change the text. Click **Save** on the same row.
8. Click **Edit** on `st-note` again. Replace the text with `new words`. Click **Save**.
9. Click **Remove** on `st-note`. No confirmation appears.
10. Open **Removed entries**. In the row whose text is `new words`, click **Restore**.
11. Quit the app, wait two minutes, and start it as `00-setup.md` section 4 says. Open **Settings > Memory** and read **Shared preferences**.

**Expected:**

- Step 1: the result line reads "Added abcdefghijabcdefghijabcdefghijabcdefghij." The entry reads "Written by the user." The byte line reads "43 of 2048 bytes used."
- Step 2: under the key field, an alert reads "This key is 41 bytes. The most is 40 bytes." **Add shared preference** is disabled.
- Step 3: under the text field, an alert reads "This text is 281 bytes. The most is 280 bytes, and an accented letter or an emoji takes more than one." **Add shared preference** is disabled.
- Step 4: the same kind of alert reads "This text is 282 bytes. The most is 280 bytes, and an accented letter or an emoji takes more than one."
- Step 5: no alert. The hint reads "One line, at most 280 bytes. With this entry: 330 of 2048 bytes." **Add shared preference** is enabled. After the click, the result line reads "Added st-note." The byte line reads "330 of 2048 bytes used."
- Step 6: the field message reads "An entry is one line. Remove the line break, tab or other control character." **Add shared preference** stays disabled.
- Step 7: the result line reads "st-note already says this. It is confirmed as of today." No Removed entries list appears.
- Step 8: the result line reads "Changed st-note. The old text is under Removed entries." **Removed entries (1)** lists the old text, which is 140 copies of `é`, with "Replaced" and a date, and "Written by the user."
- Step 9: the result line reads "Removed st-note. It is under Removed entries." **Removed entries (2)** is shown.
- Step 10: the result line reads "Restored st-note." That row shows the pill "Restored". The list shows `st-note` with the text `new words`.
- Step 11: after the restart, **Shared preferences** lists the 40-character key with the text `one`, and `st-note` with the text `new words`. The byte line reads "59 of 2048 bytes used."

**Reset:** Click **Remove** on `st-note`. Click **Remove** on the 40-character key. The Removed entries stay listed, because no control deletes them. Setup section 9 clears them before the next file.

**Known limitations:**

- Step 6 depends on a paste. Whether the field keeps a pasted tab is not confirmed. Record what you see.
- The "Over the budget" refusal for shared preferences, at 2,048 bytes, is not run here. TC-P-03 runs the same refusal with the learnings budget of 1,024 bytes and the same text.
- A restore that conflicts, "This entry cannot be restored", is TC-P-01, step 4.
- Two removed rows with the same key give two **Restore** buttons with the same name, "Restore shared preference st-note" (`PreferencesSection.tsx`). Record which row a click restores.
- The byte totals in steps 1, 5 and 11 count live entries only. This is not confirmed by a run.

## TC-ST-19 Memory: scopes and refused text

Purpose: each scope keeps its own entries. The picker shows which scope you are reading.
**Source:** `docs/settings.md` "Preferences and learnings"; `apps/desktop/src/ui/views/settings/PreferencesSection.tsx` (the three lists and their pickers); `apps/desktop/src/ui/views/settings/preferencesModel.ts`.
**Real agent:** Sonnet (alias), medium, for one chat turn in Preconditions that creates the workstream. Cost: low. No task run.
**Preconditions:** Setup sections 5 and 6 done. **Memory** has no entries. A workstream titled `Auto stream` must exist. Send this message to Global, the same message as TC-P-05, and wait until its card reads "Done on its own":

```text
Propose exactly one action and nothing else: workstream.create with title "Auto stream", summary "Made on its own" and ref "a".
```

This step is **Model-dependent** (RUNNER.md rule 8). If the model does not create the workstream in three attempts, mark this test BLOCKED with the reason "workstream not created in 3 attempts".

1. Click **Settings**, then **Memory**. Read **Preferences for** and **Learnings for**.
2. In **Preferences for**, choose `manual-test-project`. Open **Add project or workstream preference**. Type the key `pm-note` and the text `Keep reviews short`. Click **Add project or workstream preference**.
3. In **Preferences for**, choose `Auto stream`. Read the list.
4. Open **Add project or workstream preference**. Type the key `ws-note` and the text `Workstream note`. Click **Add project or workstream preference**.
5. In **Preferences for**, choose `manual-test-project` and read the list. Then choose `Auto stream` and read the list.
6. Leave **Learnings for** on **Global**. Open **Add learning**. Type the key `st-learn` and the text `Checks run on save`. Click **Add learning**.
7. In **Learnings for**, choose `manual-test-project`. Read the list.
8. In **Shared preferences**, open **Add shared preference**. Type the key `repo` and the text `the repo is at /Users/someone/code`. Click **Add shared preference**. Read the result. Then clear both fields.
9. Quit the app, wait two minutes, and start it as `00-setup.md` section 4 says. Open **Settings > Memory**. In **Preferences for**, choose `manual-test-project`. In **Learnings for**, leave **Global**. Read both lists.

**Expected:**

- Step 1: **Preferences for** shows "Choose a project or workstream". The list area shows only "Choose a project or workstream to see its preferences." **Learnings for** shows "Global". Its list shows "No entries here yet."
- Step 2: the result line reads "Added pm-note." The list shows `pm-note` with "Written by the user." The byte line reads "25 of 2048 bytes used."
- Step 3: the list shows "No entries here yet." The byte line reads "0 of 2048 bytes used."
- Step 4: the result line reads "Added ws-note." The byte line reads "22 of 2048 bytes used."
- Step 5: `manual-test-project` lists `pm-note` only. `Auto stream` lists `ws-note` only.
- Step 6: the result line reads "Added st-learn." **Learnings for** on Global lists `st-learn` with "Written by the user."
- Step 7: the list shows "No entries here yet." The Global learning does not appear for this project.
- Step 8: the result line for shared preferences reads "The daemon refused this entry. A key is lowercase letters, digits and hyphens. The text is one line and must not look like a password, a token or a file path. Nothing was changed." The list does not change. This is the same check as TC-P-02.
- Step 9: after the restart, `manual-test-project` lists `pm-note`. **Learnings for** on Global lists `st-learn`.

**Reset:** Click **Remove** on `pm-note`, `ws-note` and `st-learn`. Each removal shows its "Removed" line. The workstream `Auto stream` stays, because Settings has no control to remove it. Setup section 9 clears it.

**Known limitations:**

- Stale learnings, which are marked after 180 days, and **Keep** are in TC-P-07.
- The picker's empty option reads "No projects or workstreams to choose" when no project or workstream exists. That case is not run here.
- The card for the workstream step reads "Done on its own" only when **Let the orchestrator act on its own** is on. Setup section 6 leaves it on for the files it names.

## TC-ST-20 Notifications: reminder interval and permission

Purpose: the reminder interval takes one of the preset values or a number from 5 to 1440 minutes. It keeps its value after a restart.
**Source:** `apps/desktop/src/ui/views/settings/NotificationsSection.tsx`; `apps/desktop/src-tauri/src/notifications.rs` (interval 240 minutes and notifications off by default). No section of `docs/settings.md` covers notifications.
**Real agent:** none. Cost: low.
**Preconditions:** Setup sections 5 and 6 done. The start values are the defaults: notifications disabled and the interval 240 minutes. The desktop app keeps these values in `decision-notifications.json`, not in the daemon data folder, so setup section 9 does not reset them. If they differ from the defaults, set them back first. This is the Reset of the previous run.

1. Click **Settings**, then **Notifications**. Read **Decision notifications**.
2. In **Reminder interval**, choose `60 minutes`. Click **Save reminder interval**. Open another settings page, then **Notifications** again. Read **Reminder interval**.
3. Choose `120 minutes`. Click **Save reminder interval**. Choose `240 minutes`. Click **Save reminder interval**. Choose `Off`. Click **Save reminder interval**.
4. Choose `Custom`. In **Custom reminder interval (5–1440 minutes)**, type `4`. Click **Save reminder interval**. Read the alert.
5. Replace the field with `1441`. Click **Save reminder interval**. Read the alert.
6. Replace the field with `5`. Click **Save reminder interval**. Replace the field with `1440`. Click **Save reminder interval**.
7. Choose `60 minutes`. Click **Save reminder interval**. Quit the app, wait two minutes, and start it as `00-setup.md` section 4 says. Open **Settings > Notifications** and read **Reminder interval**.
8. Click **Enable notifications**. If macOS shows a notification prompt, allow it. Read the status line. Click **Disable notifications**. Read the status line.

**Expected:**

- Step 1: the status line reads "Notifications: disabled · Permission: <value> · Decisions needing attention: <number>". The permission value depends on macOS. Record it in `ENVIRONMENT.md`. **Reminder interval** shows `240 minutes`. **Enable notifications** is shown. The group **Local reminder actions** is not shown.
- Step 2: no message shows after the save. See Known limitations. After the page is opened again, **Reminder interval** shows `60 minutes`. The value is read back from the native notifications state.
- Step 3: after each save, the select shows the chosen value. After `Off` is saved and the page is opened again, the select shows `Off` and the custom field is not shown.
- Step 4: the alert "Enter a whole number from 5 to 1440 minutes." shows. The typed `4` stays in the field. The setting does not change.
- Step 5: the same alert shows for `1441`.
- Step 6: `5` and `1440` save with no alert.
- Step 7: after the restart, **Reminder interval** shows `60 minutes`.
- Step 8: after **Enable notifications** and an allow, the status line reads "Notifications: enabled · Permission: granted · Decisions needing attention: <number>". The button now reads **Disable notifications**. After the click, the status line reads "Notifications: disabled" again.

**Reset:** Choose `240 minutes`. Click **Save reminder interval**. If notifications are enabled, click **Disable notifications**. Expected: the select shows `240 minutes`. The status line reads "Notifications: disabled".

**Known limitations:**

- A successful **Save reminder interval** shows no message. Only an error shows an alert. Record what you see. Do not count "no message" as a fail.
- The denied permission state, with the status "Permission: denied" and the button **Retry permission**, depends on the macOS prompt. It is not run in this test. Record it if it appears.
- **Acknowledge reminders** and **Defer reminders 60 minutes** appear only when a decision is waiting. A waiting decision needs a task run (TC-Q-01, medium). Those two buttons are not checked here.
- The source of the number in "Decisions needing attention" is not confirmed. Record the number. Do not check it against Review & Decisions.
- The preview build shows "Native notifications are unavailable in this preview." and hides the controls. The Tauri app used in this run does not show that.

## TC-ST-21 Appearance: dark mode and background animation

Purpose: the theme and the background switch take effect at once. Both keep their values after a restart.
**Source:** `docs/settings.md` "Background animation"; `apps/desktop/src/ui/views/settings/AppearanceSection.tsx`; `apps/desktop/src/ui/theme/ThemeToggle.tsx`; `apps/desktop/src/ui/background/AmbientBackground.tsx`.
**Real agent:** none. Cost: low.
**Preconditions:** Setup sections 5 and 6 done. Note the starting state of **Dark mode**. It depends on the macOS appearance. **Background** starts on.

1. Click **Settings**, then **Appearance**. Read **Dark mode** and the other controls on the page.
2. Click **Dark mode** once. Read it.
3. Click the summary icon with the name "Background animation: on" in the Appearance row to open the panel. Read the panel.
4. Click **Background**. Read it. Look at the background.
5. Quit the app, wait two minutes, and start it as `00-setup.md` section 4 says. Open **Settings > Appearance**. Read **Dark mode** and **Background**.

**Expected:**

- Step 1: one theme button, **Dark mode**, with the state "On" or "Off". The page has no System option and no Light option.
- Step 2: the page switches to the other theme. **Dark mode** shows the other state, with no "(this session)" text.
- Step 3: the panel shows **Background** with the state "On". Below it is the note "Decorative only; it does not show activity." It also shows **Animation licenses**.
- Step 4: **Background** shows "Off", and its pressed state changes. The background animation is off. Record what you see on the screen.
- Step 5: **Dark mode** shows the same state as after step 2. **Background** shows "Off". The page reads both values from the app's local storage (keys `agent-orch.theme` and `agent-orch.ambient-background`).

**Reset:** Click **Background** so it shows "On". Click **Dark mode** until it shows the starting state from the Preconditions. Expected: **Background** shows "On", and **Dark mode** shows the starting state.

**Known limitations:**

- After one click on **Dark mode**, the choice is stored. No control returns the app to following the macOS setting. The code can follow the system (`apps/desktop/src/ui/theme/theme.ts`), but no control sets that. Record what you see. This is not a pass or fail item.
- "(this session)" shows when the choice cannot be stored. It needs the app's storage to be blocked. That case is not run here.

## TC-ST-22 Daemon work-class settings have no Settings control

Purpose: no Settings page shows or changes the named work-class settings. This test checks the app side only.
**Source:** `docs/settings.md` (first paragraph, "named work classes"); `apps/desktop/src/daemon/api.ts` (`settingsWorkClassesGet` and `settingsWorkClassesSet`); `crates/orchestrator-daemon/src/api_work_classes.rs`.
**Real agent:** none. Cost: low.
**Preconditions:** Setup sections 5 and 6 done.

1. Click **Settings**. Open each page in turn: **Agents**, **Limits**, **Autonomy**, **Repositories**, **Memory**, **Notifications** and **Appearance**. Scroll each page to its end.
2. In **Find a setting**, type `work class`. Read the result.

**Expected:**

- Step 1: no page shows a section or a control about work classes.
- Step 2: the result reads "No setting matches."

**Known limitations:**

- This test checks what the app shows. It does not call the daemon. The methods `settings.workClasses.get` and `settings.workClasses.set` have no caller under `apps/desktop/src/ui`. The daemon side is covered by the automated test `crates/orchestrator-daemon/tests/work_classes.rs`, not by this runner.
- `docs/settings.md` describes named work classes, but this build has no UI for them. A task's provider and model cannot be set per work class from the app.
- The project page's **Defaults** tab uses the project defaults calls (`projects.get_defaults` and `projects.set_defaults`). That is a project page, not a Settings page, and this test does not check it.
- This runner has no CLI or daemon-level step for this item, so none is written.
