# M7-32 Windows / assistive-technology manual checklist — completed record

Tester: Windows acceptance automation session (model/agent settings not recorded).
Date: 2026-11-11 UTC.
Build: b44f88a; tree 79820e90846b09a64edf25fe8593ab751abbb323.
Environment: Windows 11 Education build 26200; WebView2 Runtime 157.0.4341.42.
Screenshots: none; no interactive app launch was performed.
Reason: the computer-use skill requires action-time confirmation before installing and launching the locally built application; confirmation was requested but not received before this record was finalized. Each NOT RUN row is explicit; the silent install-smoke is recorded separately in REPORT.md.

# M7-30 Windows / assistive-technology manual checklist

**Status: DEFERRED to M7-32.** Windows UI testing is not waived. It runs after all of M7 is implemented.
macOS VoiceOver and Windows Narrator/NVDA are also DEFERRED to M7-32 (not run in M7-30).

M7-30 covered macOS only (headless Chrome + axe-core, fixtures and a real daemon).
Evidence: `docs/evidence/m7-30/`.

Record for each row: tester, date, build (git HEAD/tree), WebView2 runtime version, result (PASS/FAIL/N-A), screenshot path, notes.

## 1. Environment
- [x] Windows 11 (and Windows 10 22H2 if supported). Record OS build. — PASS: Windows 11 Education, build 26200, measured. Windows 10 22H2 was not tested. Screenshot: none; notes: OS query.
- [x] WebView2 runtime version recorded (`edge://version` equivalent or the installer manifest). — PASS: 157.0.4341.42 measured from EdgeUpdate registry. Screenshot: none; notes: registry query.
- [ ] Confirm the WebView supports `color-mix()`, `100dvh`, `:has()`, `prefers-reduced-motion`, `prefers-color-scheme`. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] NSIS installer installs, launches, upgrades and uninstalls cleanly (M7-32 packaging). — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.

## 2. Themes
- [ ] Light and dark follow the Windows app-mode setting when no preference is stored. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] The theme toggle persists across restarts (WebView profile storage persists `agent-orch.theme`). — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] Windows **Contrast themes** (Aquatic, Desert, Dusk, Night sky): text, borders, focus rings and status badges stay visible. Check `forced-colors` rendering of the status strip, badges, PLACEHOLDER labels and disabled controls. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] Color is never the only status carrier (icon or text accompanies every state). — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.

## 3. Responsive and DPI
- [ ] Window widths 1280, 1150, 960, 760, 480 and 390 (resize the window; note the minimum window size). — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] Display scaling 100%, 125%, 150% and 200%. Check that no content is clipped and the page has no horizontal scroll and no document-level vertical scroll. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] Segoe UI fallback metrics: long titles, ids and paths truncate or wrap without overlapping neighbours. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] The sticky resource strip never hides a focused control at <=960 wide. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] The composer is not covered by the Activity panel at <=960 wide. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.

## 4. Keyboard only
- [ ] The first Tab stop is "Skip to main content" and it moves focus to the main region. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] Every control reachable by Tab/Shift+Tab, with a visible focus ring, in both themes. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] The workstream tree supports Up/Down/Home/End/Enter. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] The target picker opens with Enter and Escape returns focus to the trigger. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] Ctrl+Enter sends a conversation message. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] Review: Accept, Reject and Request changes work by keyboard; destructive actions need confirmation. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] No keyboard trap in dialogs, menus or the activity panel. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.

## 5. Screen readers (Narrator, NVDA; macOS VoiceOver also deferred)
- [ ] Landmarks: banner, primary navigation, main, complementary announced with names. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] Connection state changes announce through the `status`/`alert` live regions (Connected, Reconnecting, stale data). — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] Mutation outcomes announce (created, linked, sent, accepted, rejected, failed). — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] PLACEHOLDER, DAEMON and unsupported labels are read with their reasons. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] Disabled controls expose their reason (`aria-describedby`). — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] The workstream tree announces level, position, expanded/collapsed. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] Route changes: confirm the focus and title behaviour (M7-30 recorded: focus stays on `body`; `document.title` is constant). — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.

## 6. Reduced motion
- [ ] Windows "Show animations" off maps to `prefers-reduced-motion: reduce`: no transition or animation; the ambient background stays off. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.

## 7. Connection failure and recovery
- [ ] Stop the daemon: the strip shows Reconnecting with stale data; no fixture data appears. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] Restart it: Connected, data refreshes. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] The host relaunches a daemon that exited when idle (the daemon exits after 60 s with no client). — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] Windows named-pipe/transport path (`#[cfg(windows)]`): handshake, reconnect, and shutdown on app exit. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.

## 8. Approvals and mutations
- [ ] Approve, reject and request changes on a real candidate: one `approvals.resolve` per decision, idempotent on retry. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] Create/update/link workstream, send message: success and failure notices; no fixture ids on the wire. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] `?fixture=1` mutations never reach the daemon. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.

## 9. Data honesty
- [ ] In live mode no illustrative value appears (20/24 slots, $26.00, CPU 41/44). — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] Host metrics come from the Windows daemon; unavailable GPU/CPU states say why. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] Unsupported actions are visibly disabled with a reason; PLACEHOLDER labels appear only on placeholder data. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.

## 10. Security and packaging
- [ ] The production CSP applies in the Tauri WebView: no `eval`, no external URLs, and fonts load. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
- [ ] Paths with spaces and non-ASCII characters work for repositories and worktrees. — NOT RUN: no interactive app launch; action-time confirmation was not received. Screenshot: none; notes: see REPORT.md.
