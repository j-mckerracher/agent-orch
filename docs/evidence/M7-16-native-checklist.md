# M7-16 native acceptance checklist

Run each platform row on the exact candidate commit containing this checklist. Do not report a pass from a simulator or browser-only test. Record the full commit SHA, OS version, native webview version, display scaling, result, and evidence path. Use a disposable local project and harmless agent task; do not submit production work or expose secrets.

## Setup on each platform

1. From the worktree root, record `git rev-parse HEAD` and the OS version. Create a unique evidence directory named `docs/evidence/M7-16-native/<platform>-<UTC timestamp>-<short SHA>`; use plain `mkdir`/`New-Item` so a collision fails rather than reusing evidence.
2. In `apps/desktop`, run `npm ci` and `npm run tauri dev`. Confirm the launched app is the native Tauri window and is connected to the real local daemon. Use the configured real agent only for the harmless disposable task in row 3; do not use fixture mode.
3. Capture screenshots/logs in the evidence directory. For each row record PASS/FAIL, exact webview inner width where relevant, and any failure output. Preserve failed evidence.

## Checks

1. **macOS / WKWebView responsive top bar:** At `window.innerWidth === 760` CSS px, verify the top-bar order is usage, search, counts; search occupies its own full-width row between usage and counts, with no clipped content or horizontal overflow. Capture `M7-16-macos-760.png` and a Web Inspector measurement of the three `.shell-topbar` child rectangles as `M7-16-macos-760-geometry.json`.
2. **Windows / WebView2 responsive top bar:** Repeat row 1 at `window.innerWidth === 760` CSS px in the native Tauri window. Capture `M7-16-windows-760.png` and `M7-16-windows-760-geometry.json`.
3. **Live counts, events, and local search (both OSes):** With the real daemon and configured agent, load a disposable project containing tasks in queued/running states and an approval requiring attention. Compare the top-bar counts against the loaded Workstreams, Agents & Sessions, and Review contents. Admit/start/complete one harmless task and create/resolve one test approval; wait for real daemon events and verify each affected count updates to match the lists. Search for one loaded project, task, session, and approval; verify results are grouped by type, the loaded-records-only note is present, and ArrowDown/Enter opens the selected record. Capture `M7-16-<platform>-live-counts-search.png` and the sanitized event/count record `M7-16-<platform>-live-counts-search.txt`.
4. **Live usage (both OSes):** In live mode, inspect the pill and info disclosure with no reported usage available. It must say “not available”, not `$0` or fixture `$26.00`; any displayed amount must be backed by the live source and labelled reported or estimated. Open/close the disclosure with keyboard and Escape. Capture `M7-16-<platform>-live-usage.png`.
5. **macOS / VoiceOver:** With VoiceOver enabled, Tab to Search, hear its accessible name and loaded-records-only hint, type a query, hear the result count from the status region and grouped results, use ArrowDown/Enter to open a result, and confirm focus/route behavior. Also open and dismiss the usage information with keyboard. Record the spoken-result sequence as `M7-16-macos-voiceover.txt`.
6. **Windows / Narrator or NVDA:** Repeat row 5 in WebView2 with Narrator or NVDA. Record the spoken-result sequence as `M7-16-windows-screen-reader.txt`.
7. **DPI / text scaling (both OSes):** Repeat the 760-CSS-pixel top-bar check at the platform's default scale and one increased scale (Windows 150%; macOS one alternate display scaling). Verify the search input, all counts, usage label, and popover remain readable, reachable, and unclipped. Capture `M7-16-<platform>-dpi-default.png` and `M7-16-<platform>-dpi-increased.png`.

Do not mark native acceptance complete until every applicable row has a result and evidence on both required operating systems. These runs are deferred to the consolidated native pass.