# M7-30 evidence summary (macOS, attempt 20261110T230155Z)

Base: `integ/m7-adapters-20261110T224651Z` @ `e4ae8a0bb7e25503334eeaa32fd403b96e68a258`. Branch: `orch/m7-30-20261110T230155Z`.
The full REPORT.md (matrix verdicts) is outside the repo at `.agent-worktrees/evidence-m7-30-20261110T230155Z/REPORT.md`.
Windows, VoiceOver and Narrator/NVDA are **DEFERRED to M7-32** (not waived): see `../WINDOWS-MANUAL-CHECKLIST.md`.

## Method
Headless Chrome (puppeteer-core) + axe-core 4. A Node shim fakes `window.__TAURI_INTERNALS__` so the built app talks to a real
`orchestrator-daemon` (plus `seed_review`, built outside the repo) over its Unix socket. Three modes: fixture (`?fixture=1`),
no-bridge, live. Tools are in `tools/`; a sweep is 3 modes x 4 routes x 2 themes x 6 widths = 144 states per height.

## Layout
- `sweeps/*.results.json`: every sweep, including the failed pre-fix ones. `attempt5-*` is the final, post-fix build.
- `tabwalk/`: keyboard Tab walks. `attempt3-tabwalk` is final.
- `behave/`: theme, motion, navigation, keyboard, connection and honesty checks.
- `liveops/`: workstream create, tree keyboard, conversation send and approval Accept against a real daemon. `attempt4-liveops` is the passing run; the earlier two are preserved failures caused by harness mistakes.
- `rows/`: recovery, landmarks, role=meter, picker live region, 120-item tree, animation toggle. `attempt3-rows` is final.
- `csp-attempt1.json`: production CSP enforced as a response header, 0 violations.
- `shots/`: both themes at 1280/1150/960/760/480/390 (workspace, fixture and live) and 1280/390 for the other routes.
- `logs/`: command logs, including `npm-test-final.log`, `build-final.log`, `e2e-final.log`.

## Final results
- Sweep h900 (`attempt5-h900`): 144 states, 0 hard findings; 142 clean, 2 scroll-dependent target-size (fixture workspace 390, both themes).
- Sweep h700 (`attempt5-h700`): 144 states, 0 hard findings; 136 clean, 8 rest-state target-size (sticky strip partly covers an element that is clear once scrolled).
- All states: no horizontal scroll, no document vertical scroll, no Activity/main overlap, 0 console errors.
- Tab walk: 64 runs, 2 flagged (live agents 390x700 both themes): a 879px-tall scroll region whose centre sits under the sticky strip; its top is visible and focus-ringed. Not a defect.
- `npm test`: 1091 passed (51 files; baseline 1021). `npm run test:e2e`: 40 passed (8 files). `npm run build`: OK.

## Defects fixed
See REPORT.md. Fixes: `apps/desktop/src/ui/shell/shell.css`, `AppShell.tsx`; regression tests in `AppShell.test.tsx`,
`shell.resources.test.ts`, `ActivityPanel.test.tsx`, `WorkspaceView.test.tsx`, and new `daemon/fixtureGuard.test.ts`.
