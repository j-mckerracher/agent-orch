# M7-32 Windows native checklist (v3)

**Status: UNVERIFIED.** This worker did not run Windows or native checks. Perform these rows during the consolidated native pass at commit `6f2fe17f6334a1fb61f1f27cd9765de2ffbb4d5d` (tree `62524d7c14d68bcca58c5b6d7cad762885eaf4f9`). Use [the v3 supplement](M7-32-windows-runbook-v3.md), v1 §§4.1–4.2, and v2 §4. Record tester, UTC date, tested SHA, Windows/WebView2/screen-reader versions, status, evidence paths and sanitized notes. Do not record credentials.

If N01 cannot launch an inspectable window or screenshot/capture/UI interaction is denied, mark every row below that requires a native window `BLOCKED-no-native-window` for that same observed reason; do not claim the check was attempted. Continue the v3 browser-proxy pass. Record P13, but none of its values blocks N01 or native rows. Restore every machine-wide setting changed during a check.

1. **N01 — Unsigned package install, launch, inspect and uninstall.** Verify current-user install without elevation, launch and inspect/capture the window, daemon origin, and uninstall behavior; use v1 §4.1.
2. **N02 — Native themes and contrast.** Verify OS/system theme, persisted explicit theme, contrast themes and forced colors; restore original settings.
3. **N03 — Native responsive layout and DPI.** Inspect the v1 §4.1 viewport/scale matrix, including focus visibility and overflow at narrow widths.
4. **N04 — Native keyboard operation.** Verify keyboard-only traversal and visible focus for the v1 §4.1 controls.
5. **N05 — Screen readers.** Verify Narrator/NVDA announcements and controls as specified in v1 §4.1.
6. **N06 — Reduced motion and hidden pause.** Verify native reduced-motion behavior, minimize/restore pause, and resource observations per v1 §4.1.
7. **N07 — Native navigation.** Verify packaged WebView2 routes, unknown-route fallback, back/forward and focus per v1 §4.1.
8. **N08 — Named-pipe daemon recovery.** Verify stop/reconnect/recovery, stale-state honesty and shutdown per v1 §4.1.
9. **N09 — Real-agent recovery lifecycle.** Verify close/reopen, cancellation, daemon-loss recovery and drafts per v1 §4.1; requires already-signed-in Codex 0.167.0.
10. **N10 — Real approvals and fixture boundary.** Verify real-agent approval, request-changes, idempotency and fixture isolation per v1 §4.1; requires already-signed-in Codex 0.167.0.
11. **N11 — Native live-data honesty.** Verify real daemon values, unavailable/unknown semantics and absence of fixture/placeholder values per v1 §4.1.
12. **N12 — Installed-package security.** Verify packaged CSP, bundled assets, network behavior and paths per v1 §4.1.
13. **F08 — Native live conversation.** Run only the v1 §4.2 native/live portion not already PASS in v2; requires already-signed-in Codex 0.167.0.
14. **F10 — Conversation targets and Global truthfulness.** Verify native live UI behavior per v1 §4.2; requires already-signed-in Codex 0.167.0.
15. **F11 — Live workstreams.** Verify native workstream/tree data and interaction per v1 §4.2; do not substitute fixtures.
16. **F12 — Live activity panel.** Verify activity/count updates and native responsive behavior per v1 §4.2.
17. **F14 — Agents and Sessions inventory.** Verify native inventory against live daemon data per v1 §4.2; requires already-signed-in Codex 0.167.0 where real agents are needed.
18. **F15 — WebView2 launch surface.** Verify the packaged surface and five legacy routes per v1 §4.2.
19. **F16 — Top-bar counts, search and usage.** Verify native counts/search and truthful usage per v1 §4.2; requires live data.
20. **F17 — Device resources at DPI.** Verify native resource-strip layout and scaling per v1 §4.2.
21. **F18 — Packaged background renderer.** Verify setting persistence, renderer selection, console and CSP in the installed app per v1 §4.2.
22. **F20 — Capacity and dispatch panel.** Verify live capacity and reconnect behavior per v1 §4.2; requires real daemon and agents.
23. **F22 — Usage popover.** Verify reported/estimated/unknown distinctions with a real session per v1 §4.2; requires already-signed-in Codex 0.167.0.
24. **F23 — Migrated data.** Verify migration and native workstream persistence only with the owner-provided pre-migration copy per v1 §4.2; otherwise BLOCKED for the missing prerequisite.
25. **F24 — Global/workstream conversations.** Verify native live conversation behavior per v1 §4.2; requires already-signed-in Codex 0.167.0.
26. **F25 — Runtime, authority and live replies.** Verify only the native/live portions not already PASS in v2 per v1 §4.2; requires already-signed-in Codex 0.167.0.
27. **F26 — Usage summary.** Verify the native summary with a real Codex task per v1 §4.2; requires already-signed-in Codex 0.167.0.

The seven `WIN-BP-*` rows are browser-proxy evidence, not native checks; run them as specified in the v3 supplement even if native rows pass. Mark signed-install rows WAIVED and the Windows real-daemon e2e harness N/A as specified in v2. Record actual row outcomes in `evidence-out/RESULTS.md`; do not infer PASS from this checklist.
