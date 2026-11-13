# M7-32 browser-proxy results

**Tested source:** `8f66dc9f175a60bf739fcf8e8c8b66e5feeb554e`<br>
**Method:** headless Chromium against Vite with an isolated, real daemon; results are browser-proxy evidence only, not WKWebView/WebView2 proof.<br>
**Screenshots/data:** `docs/evidence/m7-32/browser/` and `proxy-observations.json`.

| row id | PASS/FAIL/BLOCKED | evidence file |
| --- | --- | --- |
| N01 | BLOCKED | `M7-32-macos-results.md` — packaged install/native window not verifiable in Chrome. |
| N02 | PASS | `workspace-system-light.png`, `agents-explicit-dark.png`, `workspace-system-dark.png` — explicit themes, persistence, and system scheme. |
| N03 | FAIL | `workspace-760x700.png`, `proxy-observations.json` — all 10 viewport pairs had no horizontal overflow and correct shell tiers/activity stacking; at 760×700 the focused main is obscured by the connection strip. |
| N04 | PASS | `proxy-observations.json` — skip link, global nav, workstream tree, target picker, and review tabs keyboard traversal. |
| N05 | BLOCKED | Native VoiceOver/WebView accessibility semantics require a native window and assistive technology. |
| N06 | PASS | `ambient-reduced-motion-static.png`, `ambient-hidden-paused.png` — reduced-motion canvas stayed static; pause/resume verified with a synthetic `document.hidden` visibility event because headless Chromium did not hide the background page. |
| N07 | PASS | `routes-live-review.png`, `proxy-observations.json` — 11 global/legacy routes, unknown fallback, and back/forward. |
| N08 | PASS | `daemon-stopped-stale.png`, `daemon-restarted-connected.png` — stale cached state without fixture fallback, then reconnect. |
| N09 | BLOCKED | Native app close/reopen continuity with a real running agent is not proven by a browser page. |
| N10 | BLOCKED | `approval-candidate-before.png`, `approval-idempotent-retry.png`, `proxy-observations.json` — real daemon `seed_review` candidate accepted; duplicate `approvals.resolve` returned the same receipt/key and cleared the pending item. No agent-generated review candidate was available, so the fake-agent criterion remains blocked. |
| N11 | PASS | `workspace-live-connected.png`, `proxy-observations.json` — live usage is “not available”; recovery/journal are explicitly unknown; no fixture mode or fabricated usage. |
| N12 | BLOCKED | `proxy-observations.json` — Vite dev response had no CSP header and no CSP violations; three generic console 500s occurred during daemon stop/restart, not policy violations. Packaged/native enforcement remains unverified. |

**Summary:** 6 PASS, 1 FAIL, 5 BLOCKED. N03 is reported to the orchestrator; no product code was changed. The macOS native results remain unchanged.

**Local runs:** `npm ci` — 105 packages added; daemon build (`CARGO_TARGET_DIR=target/m7-32-browser cargo build --locked -p orchestrator-daemon --examples --bins`) — pass; `npm run test:browser -- --reporter=line` — 4 passed, 6 failed (no-bridge fallback overflow; 217 px at 760 CSS px and 52 px at 1151 CSS px). Browser-proxy pass: 6 PASS, 1 FAIL, 5 BLOCKED. `npm test` and `npm run build` were not run because this change is evidence-only. CI disabled by owner; local gates only.
