# M7-32 Native Checklist

**Status: Deferred to the consolidated native pass.** The headless axe scan passed; native-only checks below remain unrun.

For each row, record tester, date, OS/WebView and screen-reader versions, tested commit, daemon state, result, and evidence paths. Omit credentials.

1. **macOS — native Tauri webview and VoiceOver.** With the app connected to a real daemon, verify the accessibility tree exposes the named “Connection status” region and its `connection-status` child remains a polite live status during connection changes. Confirm VoiceOver announces the region/status without a duplicate nested banner and the strip has no visual shift. Capture the accessibility tree, announcement, and screenshot.
2. **Windows — native Tauri WebView2 and screen reader.** Repeat row 1 with the supported Windows screen reader and a real daemon. Capture the accessibility tree, announcement, and screenshot.
3. **DPI / scaling — native app.** At 100%, 150%, and 200% scaling on each supported platform, verify the connection strip remains visible, readable, and visually unchanged with no unexpected overflow. Capture screenshots and scaling settings.
