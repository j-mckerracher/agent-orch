# M7-30 Windows / assistive-technology manual checklist

**Status: DEFERRED to M7-32.** Windows UI testing is not waived. It runs after all of M7 is implemented.
macOS VoiceOver and Windows Narrator/NVDA are also DEFERRED to M7-32 (not run in M7-30).

M7-30 covered macOS only (headless Chrome + axe-core, fixtures and a real daemon).
Evidence: `docs/evidence/m7-30/`.

Record for each row: tester, date, build (git HEAD/tree), WebView2 runtime version, result (PASS/FAIL/N-A), screenshot path, notes.

## 1. Environment
- [ ] Windows 11 (and Windows 10 22H2 if supported). Record OS build.
- [ ] WebView2 runtime version recorded (`edge://version` equivalent or the installer manifest).
- [ ] Confirm the WebView supports `color-mix()`, `100dvh`, `:has()`, `prefers-reduced-motion`, `prefers-color-scheme`.
- [ ] NSIS installer installs, launches, upgrades and uninstalls cleanly (M7-32 packaging).

## 2. Themes
- [ ] Light and dark follow the Windows app-mode setting when no preference is stored.
- [ ] The theme toggle persists across restarts (WebView profile storage persists `agent-orch.theme`).
- [ ] Windows **Contrast themes** (Aquatic, Desert, Dusk, Night sky): text, borders, focus rings and status badges stay visible. Check `forced-colors` rendering of the status strip, badges, PLACEHOLDER labels and disabled controls.
- [ ] Color is never the only status carrier (icon or text accompanies every state).

## 3. Responsive and DPI
- [ ] Window widths 1280, 1150, 960, 760, 480 and 390 (resize the window; note the minimum window size).
- [ ] Display scaling 100%, 125%, 150% and 200%. Check that no content is clipped and the page has no horizontal scroll and no document-level vertical scroll.
- [ ] Segoe UI fallback metrics: long titles, ids and paths truncate or wrap without overlapping neighbours.
- [ ] The sticky resource strip never hides a focused control at <=960 wide.
- [ ] The composer is not covered by the Activity panel at <=960 wide.

## 4. Keyboard only
- [ ] The first Tab stop is "Skip to main content" and it moves focus to the main region.
- [ ] Every control reachable by Tab/Shift+Tab, with a visible focus ring, in both themes.
- [ ] The workstream tree supports Up/Down/Home/End/Enter.
- [ ] The target picker opens with Enter and Escape returns focus to the trigger.
- [ ] Ctrl+Enter sends a conversation message.
- [ ] Review: Accept, Reject and Request changes work by keyboard; destructive actions need confirmation.
- [ ] No keyboard trap in dialogs, menus or the activity panel.

## 5. Screen readers (Narrator, NVDA; macOS VoiceOver also deferred)
- [ ] Landmarks: banner, primary navigation, main, complementary announced with names.
- [ ] Connection state changes announce through the `status`/`alert` live regions (Connected, Reconnecting, stale data).
- [ ] Mutation outcomes announce (created, linked, sent, accepted, rejected, failed).
- [ ] PLACEHOLDER, DAEMON and unsupported labels are read with their reasons.
- [ ] Disabled controls expose their reason (`aria-describedby`).
- [ ] The workstream tree announces level, position, expanded/collapsed.
- [ ] Route changes: confirm the focus and title behaviour (M7-30 recorded: focus stays on `body`; `document.title` is constant).

## 6. Reduced motion
- [ ] Windows "Show animations" off maps to `prefers-reduced-motion: reduce`: no transition or animation; the ambient background stays off.

## 7. Connection failure and recovery
- [ ] Stop the daemon: the strip shows Reconnecting with stale data; no fixture data appears.
- [ ] Restart it: Connected, data refreshes.
- [ ] The host relaunches a daemon that exited when idle (the daemon exits after 60 s with no client).
- [ ] Windows named-pipe/transport path (`#[cfg(windows)]`): handshake, reconnect, and shutdown on app exit.

## 8. Approvals and mutations
- [ ] Approve, reject and request changes on a real candidate: one `approvals.resolve` per decision, idempotent on retry.
- [ ] Create/update/link workstream, send message: success and failure notices; no fixture ids on the wire.
- [ ] `?fixture=1` mutations never reach the daemon.

## 9. Data honesty
- [ ] In live mode no illustrative value appears (20/24 slots, $26.00, CPU 41/44).
- [ ] Host metrics come from the Windows daemon; unavailable GPU/CPU states say why.
- [ ] Unsupported actions are visibly disabled with a reason; PLACEHOLDER labels appear only on placeholder data.

## 10. Security and packaging
- [ ] The production CSP applies in the Tauri WebView: no `eval`, no external URLs, and fonts load.
- [ ] Paths with spaces and non-ASCII characters work for repositories and worktrees.
