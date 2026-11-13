# M7-32 Native Checklist

**Status: Deferred to the consolidated native pass.** No native checks have been run.

For each row, record tester, date, full tested SHA, OS/WebView and screen-reader versions, daemon and agent versions, result, and evidence paths. Use a disposable workstream/task for gate scenarios; preserve failed output and screenshots; omit credentials.

1. **macOS — WKWebView, live daemon, real agent, and VoiceOver.** Run a real agent task on a disposable workstream, add an objective and gate, select that task as evidence, and verify the gate. Confirm the visible gate state and daemon `evidence_task_id` agree before and after refreshing/reopening the view. Also verify the accessibility tree exposes the named “Connection status” banner and its `connection-status` child remains a polite live status during connection changes; confirm VoiceOver announces the banner/status without a duplicate nested banner or visual shift. Capture the UI, accessibility tree, announcement, daemon record, logs, and screenshot.
2. **Windows — WebView2, live daemon, real agent, and screen reader.** Repeat row 1 on native Windows with the supported daemon transport and VoiceOver-equivalent supported screen reader. Verify the named “Connection status” banner and polite `connection-status` updates, plus matching gate/evidence state. Capture the UI, accessibility tree, announcement, daemon record, logs, full test output, and screenshot.
3. **Assistive technology.** With VoiceOver on macOS and Narrator or NVDA on Windows, use keyboard navigation to select a task as gate evidence and verify the gate. Confirm gate state, evidence selector labels, named connection banner, and polite status updates are announced and controls remain operable.
4. **DPI / scaling — native app.** At 100%, 150%, and 200% scaling on each supported platform, verify the connection strip remains visible, readable, and visually unchanged with no overflow. On Windows, also repeat the gate/evidence interaction at 100%, 125%, 150%, and 200%; confirm controls and labels do not overlap or clip. Capture screenshots at each applicable scale.

5. **Narrow-window keyboard focus — macOS and Windows.** In WKWebView and WebView2 at 760×700 CSS px, activate “Skip to main content” and verify the focused main region/indicator stays below the connection banner with no horizontal overflow. Record the OS/WebView build and capture the viewport.

Mark unrun rows **UNVERIFIED**; automated e2e results do not establish native acceptance.
