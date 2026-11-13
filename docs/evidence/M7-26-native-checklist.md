# M7-26 Native Checklist

**Status: Deferred to the consolidated native pass.** This worker ran local Rust gates only; real-agent and native-webview checks below remain unrun.

For each row, record tester, date, OS/WebView version, tested commit, daemon/provider versions, result, and evidence path. Preserve command output, exit status, daemon logs, and screenshots; omit credentials.

1. **macOS — native daemon and real Codex agent.** On the tested commit, run `cargo test --locked -p orchestrator-store -p orchestrator-daemon -- --test-threads=1`, then run a real Codex task that reports token counts but no cost. Inspect `usage.summary` and the native app usage pill/Agents table: reported costs stay unchanged; estimates are labelled estimated with table version/source; unknown or unpriced data is unknown, never `$0`. Capture the summary, UI, and test evidence.
2. **Windows — native daemon and real Codex agent.** Repeat row 1 on native Windows and the Windows WebView. Capture the summary, UI, full test output and exit status, plus daemon/provider versions and evidence paths.
