# M7-32 Windows / assistive-technology manual checklist — partial hands-on record

Tester: Windows acceptance automation session (model/agent settings not recorded).
Date: 2026-11-11 UTC.
Build: b44f88a; tree 79820e90846b09a64edf25fe8593ab751abbb323.
Environment: Windows 11 Education build 26200; WebView2 Runtime 157.0.4341.42.
Screenshots: nine supplemental screenshots are listed in manual-ui-session.txt and stored beside this checklist.
The user explicitly authorized the interactive install and launch. This is a partial hands-on record: each untested item is marked NOT RUN or PARTIAL; the silent install-smoke is recorded separately in REPORT.md.

# M7-30 Windows / assistive-technology manual checklist

**Status: DEFERRED to M7-32.** Windows UI testing is not waived. It runs after all of M7 is implemented.
macOS VoiceOver and Windows Narrator/NVDA are also DEFERRED to M7-32 (not run in M7-30).

M7-30 covered macOS only (headless Chrome + axe-core, fixtures and a real daemon).
Evidence: `docs/evidence/m7-30/`.

Record for each row: tester, date, build (git HEAD/tree), WebView2 runtime version, result (PASS/FAIL/N-A), screenshot path, notes.

## 1. Environment
- [x] Windows 11 (and Windows 10 22H2 if supported). Record OS build. — PASS: Windows 11 Education, build 26200, measured. Windows 10 22H2 was not tested.
- [x] WebView2 runtime version recorded (`edge://version` equivalent or the installer manifest). — PASS: WebView2 Runtime 157.0.4341.42 measured from EdgeUpdate registry.
- [ ] Confirm the WebView supports `color-mix()`, `100dvh`, `:has()`, `prefers-reduced-motion`, `prefers-color-scheme`. — NOT RUN: CSS feature support was not queried directly in the production WebView.
- [ ] NSIS installer installs, launches, upgrades and uninstalls cleanly (M7-32 packaging). — PARTIAL: Interactive unsigned installer install and launch succeeded; prior silent install-smoke passed separately. GUI upgrade/uninstall and N-1 upgrade were not exercised.

## 2. Themes
- [ ] Light and dark follow the Windows app-mode setting when no preference is stored. — PARTIAL: First run rendered light. The Windows app-mode registry setting was not measured, so system-default matching is unproven.
- [x] The theme toggle persists across restarts (WebView profile storage persists `agent-orch.theme`). — PASS: Dark and light theme toggles persisted across close/reopen; see the theme screenshots.
- [ ] Windows **Contrast themes** (Aquatic, Desert, Dusk, Night sky): text, borders, focus rings and status badges stay visible. Check `forced-colors` rendering of the status strip, badges, PLACEHOLDER labels and disabled controls. — NOT RUN: The four Windows contrast themes and forced-colors rendering were not selected.
- [ ] Color is never the only status carrier (icon or text accompanies every state). — PARTIAL: Sampled status included text and icon; every state carrier was not checked.

## 3. Responsive and DPI
- [ ] Window widths 1280, 1150, 960, 760, 480 and 390 (resize the window; note the minimum window size). — PARTIAL: Checked at 1282x831; sampled workspace/workstreams views had no horizontal overflow. Resize attempts did not change bounds, so requested widths remain unverified.
- [ ] Display scaling 100%, 125%, 150% and 200%. Check that no content is clipped and the page has no horizontal scroll and no document-level vertical scroll. — NOT RUN: Display scaling at 100%, 125%, 150%, and 200% was not tested.
- [ ] Segoe UI fallback metrics: long titles, ids and paths truncate or wrap without overlapping neighbours. — NOT RUN: Segoe UI fallback behavior for long titles, IDs, and paths was not measured.
- [ ] The sticky resource strip never hides a focused control at <=960 wide. — NOT RUN: The sticky strip and focused controls were not checked at 960px or narrower.
- [ ] The composer is not covered by the Activity panel at <=960 wide. — NOT RUN: Composer coverage by the Activity panel was not checked at 960px or narrower.

## 4. Keyboard only
- [x] The first Tab stop is "Skip to main content" and it moves focus to the main region. — PASS: First Tab focused Skip to main content; Enter moved focus to the main region.
- [ ] Every control reachable by Tab/Shift+Tab, with a visible focus ring, in both themes. — PARTIAL: Visible focus was observed in a limited light-theme sequence. Full Tab/Shift+Tab coverage in both themes was not performed.
- [ ] The workstream tree supports Up/Down/Home/End/Enter. — NOT RUN: Workstream tree Up/Down/Home/End/Enter behavior was not tested.
- [x] The target picker opens with Enter and Escape returns focus to the trigger. — PASS: Enter opened the target picker; Escape closed it and restored focus to the trigger.
- [ ] Ctrl+Enter sends a conversation message. — NOT RUN: No conversation message was sent. The selected model was unavailable and Send was disabled.
- [ ] Review: Accept, Reject and Request changes work by keyboard; destructive actions need confirmation. — NOT RUN: No real approval candidate was available; Accept, Reject, and Request changes were not exercised.
- [ ] No keyboard trap in dialogs, menus or the activity panel. — PARTIAL: Escape exited the target picker. Other dialogs, menus, and the Activity panel were not exhaustively checked for traps.

## 5. Screen readers (Narrator, NVDA; macOS VoiceOver also deferred)
- [ ] Landmarks: banner, primary navigation, main, complementary announced with names. — NOT RUN: Narrator, NVDA, and VoiceOver were not run.
- [ ] Connection state changes announce through the `status`/`alert` live regions (Connected, Reconnecting, stale data). — NOT RUN: Live-region announcements for connection changes were not checked with a screen reader.
- [ ] Mutation outcomes announce (created, linked, sent, accepted, rejected, failed). — NOT RUN: Mutation announcements were not checked with a screen reader; no mutations were attempted.
- [ ] PLACEHOLDER, DAEMON and unsupported labels are read with their reasons. — PARTIAL: Placeholder, daemon, and unsupported labels showed visible reasons; screen-reader output was not tested.
- [ ] Disabled controls expose their reason (`aria-describedby`). — PARTIAL: The disabled Send control showed that the selected model was unavailable; aria-describedby was not inspected.
- [ ] The workstream tree announces level, position, expanded/collapsed. — NOT RUN: Workstream tree level, position, and expanded/collapsed announcements were not checked.
- [ ] Route changes: confirm the focus and title behaviour (M7-30 recorded: focus stays on `body`; `document.title` is constant). — NOT RUN: Route-change focus and title behavior were not measured in this run.

## 6. Reduced motion
- [ ] Windows "Show animations" off maps to `prefers-reduced-motion: reduce`: no transition or animation; the ambient background stays off. — NOT RUN: Windows Show animations state was not measured and the media preference was not directly asserted.

## 7. Connection failure and recovery
- [ ] Stop the daemon: the strip shows Reconnecting with stale data; no fixture data appears. — NOT RUN: The daemon was not stopped to observe reconnecting and stale-data behavior.
- [ ] Restart it: Connected, data refreshes. — NOT RUN: The daemon was not manually restarted during the UI session.
- [ ] The host relaunches a daemon that exited when idle (the daemon exits after 60 s with no client). — NOT RUN: The idle-exit path was not forced during the UI session.
- [ ] Windows named-pipe/transport path (`#[cfg(windows)]`): handshake, reconnect, and shutdown on app exit. — PARTIAL: App startup showed Connected. Manual reconnect/shutdown was not exercised. Separate transport tests recorded 13 passed and 42 filtered; the harness expected 33 filtered and marked its gate FAIL.

## 8. Approvals and mutations
- [ ] Approve, reject and request changes on a real candidate: one `approvals.resolve` per decision, idempotent on retry. — NOT RUN: No real approval decisions were attempted.
- [ ] Create/update/link workstream, send message: success and failure notices; no fixture ids on the wire. — NOT RUN: Workstream mutation and message sending were not exercised in the UI.
- [ ] `?fixture=1` mutations never reach the daemon. — NOT RUN: Fixture-query mutation isolation was not exercised.

## 9. Data honesty
- [x] In live mode no illustrative value appears (20/24 slots, $26.00, CPU 41/44). — PASS: No illustrative checklist values appeared in the live views captured; fixture mode was not used.
- [x] Host metrics come from the Windows daemon; unavailable GPU/CPU states say why. — PASS: Device Resources displayed measurement scope/timestamps and explained unsupported GPU utilization.
- [ ] Unsupported actions are visibly disabled with a reason; PLACEHOLDER labels appear only on placeholder data. — PARTIAL: Some disabled and placeholder states/reasons were visible; the full set of actions and data states was not audited.

## 10. Security and packaging
- [ ] The production CSP applies in the Tauri WebView: no `eval`, no external URLs, and fonts load. — NOT RUN: Production CSP, eval/external URL behavior, and font loading were not inspected in the live WebView.
- [ ] Paths with spaces and non-ASCII characters work for repositories and worktrees. — PARTIAL: A repository path containing spaces and an accented character opened in the app; a Git worktree with those characters was created directly. App-mediated worktree creation was not tested.
