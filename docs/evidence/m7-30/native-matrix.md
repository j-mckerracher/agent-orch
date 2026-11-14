# M7-30 accessibility, responsive and theme matrix

## Scope and evidence rules

This is the automated portion of M7-30 on the current integ2-based worktree. UI tests run in macOS jsdom; they do not prove native WebView rendering, real-daemon/agent behavior, assistive-technology output, or Windows DPI. The earlier M7-30 headless/axe attempt is pinned to `e4ae8a0bb7e25503334eeaa32fd403b96e68a258` and is historical, not evidence for this base. Fast mode defers native and Windows work to M7-32. No axe or e2e dependency was added, and e2e was not rerun.

## Automated matrix evidence

| Matrix area | Existing regression evidence exercised by `npm test` | Limitation / native row |
| --- | --- | --- |
| Themes | `ThemeToggle.test.tsx` covers light default, toggle, `prefers-color-scheme`, storage/reload, keyboard and `aria-pressed`; `contrast.test.ts` checks text 4.5:1 and UI 3:1 token pairings for light/dark; `primitives.test.tsx` checks non-colour state cues and meter text. | Actual WKWebView/WebView2 profile persistence, system appearance, rendered surfaces and Windows contrast themes: N02. |
| Responsive | `AppShell.test.tsx` checks shell tiers and controls at 1280/1150/960/760/480; `WorkstreamsView.test.tsx` checks narrow graph fallback and 150 tasks. Existing Playwright coverage is in `e2e/geometry.spec.ts` (not rerun under fast-mode gates). | Real geometry, heights 700/900, clipping/scroll, window minimum/default size, Retina and Windows DPI: N03. |
| Accessibility | `AppShell.test.tsx` checks landmarks, skip-link focus and nav keyboard use; `WorkstreamsView.test.tsx` checks tree keys; `WorkspaceView.test.tsx` checks target-picker keyboard/focus return and empty-result status; `TopBar.test.tsx` checks search/popover keyboard and announcements; `ReviewDecisionsView.test.tsx` checks tab keyboard use; `primitives.test.tsx`, `ThemeToggle.test.tsx` and `ActivityPanel.test.tsx` cover disclosure, meter, state and focus semantics; `engine.test.ts`/`AmbientBackground.test.tsx` cover reduced motion. | VoiceOver/Narrator/NVDA and actual native keyboard/focus rendering: N04–N06. The current UI has no `<dialog>` or `role="dialog"`; confirmations are inline, so no modal focus-trap behavior exists to verify. |
| Navigation | `AppShell.test.tsx` covers global and legacy routes, landmarks, deep links and back/forward; `router.test.ts` covers unknown/malformed routes; counts are checked against loaded lists. | Verify real packaged WebView route/history behavior: N07. |
| Connection failures | `AppShell.test.tsx` checks strip visibility on routes, fixture mode and unavailable daemon; `ActivityPanel.test.tsx` checks offline/degraded data; `WorkstreamsView.test.tsx` checks stale state and recovery. | Real daemon stop/restart, slow reader, event reset/epoch change and no live-to-fixture fallback: N08. |
| Recovery | `native.test.tsx` simulates Tauri close/reopen with a fake host/daemon and checks reconnect, durable sequence replay and draft restoration; theme and per-target draft reload are covered by `ThemeToggle.test.tsx` and `WorkspaceView.test.tsx`. | Real running agent/daemon and actual app restart, route/theme/profile and history-incomplete restoration: N09. |
| Approvals | `ReviewDecisionsView.test.tsx` covers approve/reject/request-changes, idempotency, delivery separation, fixture blocking and live-value honesty; `ReviewScreen.test.tsx`, `binding.test.ts` and `daemonSources.test.ts` cover stale/mismatched candidate evidence; `fixtureGuard.test.ts` dynamically accounts for every `DaemonApi` method and blocks fixture IDs for every ID-bearing action before transport. | Real daemon/agent approval, changed-candidate reapproval and native confirmation path: N10. |
| Data honesty | `ActivityPanel.test.tsx`, `TopBar.test.tsx`, `ReviewDecisionsView.test.tsx`, `ResourceStrip.test.tsx`, `liveMetrics.test.ts`, `usageLive.test.tsx` and `WorkstreamsView.test.tsx` cover fixture provenance, no illustrative live values, and unavailable/unknown rather than invented zeroes. | Validate against live native daemon telemetry and both platform adapters: N11. |
| Performance | `WorkstreamsView.test.tsx` checks all 150 tasks render, counts remain honest, and the task region is independently focusable; `engine.test.ts` checks animation pauses while hidden/reduced-motion. | Real scroll/paint behavior with large data and agreed idle CPU/GPU budget: N03 and N06. |
| CSP | `policy.test.ts` checks production `script-src 'self'`, rejects `unsafe-eval` and external/script injection constructs; the desktop build validates bundled assets. | Inspect enforced CSP and loaded assets in production WKWebView/WebView2: N12. |
| Packaging | Desktop `npm run build` is the automated web build gate; it does not produce or install native packages. | macOS app and Windows NSIS install/launch/upgrade/uninstall and M6-04 smoke: N01 and N12. |

## Local gates on macOS

Source code tested: `9b46c07788335f9bd81b7b582550ecf0444bc7f5` (integ2 before this documentation-only change).
- `npm ci` (`apps/desktop`): exit 0; 105 packages installed.
- `npm test` (`apps/desktop`): exit 0; 52 files and 1,252 tests passed.
- `npm run build` (`apps/desktop`): exit 0; `tsc -b` and Vite build passed. Vite emitted a chunk-size advisory above 500 kB.
- Not run: script tests (no test files added/renamed), Playwright/e2e, axe, native and Windows checks per fast-mode scope.

## Native-only checklist for M7-32

These checks were **not run** in M7-30. Record tester, date, tested commit, OS/WebView version, PASS/FAIL/N-A, screenshot/evidence path, and notes for each platform. Preserve failed evidence.

1. **N01 — Native package and environment (macOS + Windows):** Build/install the `.app` and NSIS package from the pinned M7-32 commit; record OS build, WebView/WKWebView version, install/upgrade/uninstall result, and minimum/default window dimensions. Confirm bundled fonts/assets and no network dependency.
2. **N02 — Themes and contrast (macOS + Windows):** Check system light/dark, explicit toggle, persistence after app restart, menus/status/badges/meters and disabled controls. On Windows also check forced-colors and Aquatic/Desert/Dusk/Night Sky contrast themes. Confirm state is not conveyed by colour alone.
3. **N03 — Responsive geometry and scale (macOS + Windows):** Resize to 1280/1150/960/760/480 px at heights 700 and 900; record minimum size, shell stacking/sidebar collapse, horizontal/document scroll, clipping, focus obscured by sticky regions, and 100+ task/workstream scroll. On Windows repeat at 100/125/150/200% DPI; on macOS record Retina scaling.
4. **N04 — Keyboard operation and focus (macOS + Windows):** Keyboard-only traverse skip link, global/legacy navigation, workstream tree, target picker, model controls, search, disclosures, review tabs and inline confirmations in both themes. Record visible focus and focus return; verify no trap. If a modal/dialog is introduced before M7-32, add and run its focus-containment/return scenario.
5. **N05 — Screen readers (macOS VoiceOver; Windows Narrator and NVDA):** Verify named banner/nav/main/aside, route headings, tree level/position/expanded state, status/live-region announcements, fixture/unavailable labels and disabled reasons, and approval outcomes.
6. **N06 — Reduced motion and rendering cost (macOS + Windows):** Enable OS reduced-motion, confirm decorative animation is off/static, and verify hidden-tab pause. Record idle CPU/GPU during the agreed observation interval; agree and record the budget before judging performance.
7. **N07 — Native navigation (macOS + Windows):** Exercise all global and legacy deep links, unknown route fallback and browser back/forward in the packaged WebView; record route and focus behavior.
8. **N08 — Real connection failure/recovery (macOS + Windows):** With a real daemon, stop/restart it, induce a slow reader and event epoch/reset path; confirm strip/stale indicators, refresh behavior and that live failures never show fixtures.
9. **N09 — Real recovery lifecycle (macOS + Windows):** Keep a real agent task running, close/reopen the app, and verify daemon/task continuity, durable history/incomplete-history label, route, theme and drafts per conversation target.
10. **N10 — Real approvals and fixture boundary (macOS + Windows):** Against a real candidate, approve, reject and request changes; verify idempotent retry, changed candidate requires fresh approval, delivery remains separate, and fixture IDs never reach mutation RPCs.
11. **N11 — Live data honesty (macOS + Windows):** Verify no illustrative usage/capacity/priority values in live mode; real unsupported/unavailable GPU and missing metrics say unavailable/unknown, never zero; fixture values remain visibly labelled.
12. **N12 — Production security/packaging (macOS + Windows):** In the installed app verify enforced CSP (`script-src 'self'`, no `unsafe-eval` or external assets), bundled animation module operation/fallback, and M6-04 install/upgrade smoke. Record CSP console violations and package artifact paths.
