# M7-22 Native Checklist

**Status: Deferred to M7-32; not run by this worker.**

Record tester, date, OS/WebView version, tested commit, daemon and agent/provider versions, result, and evidence path for each row. Preserve screenshots and relevant logs; omit credentials.

1. **macOS — real desktop app, daemon, and agent.** Run a real agent session that records usage, then open the top-bar usage popover. Verify the reported amount is labelled reported; the estimate is separately labelled (or explicitly unknown) and never added to the reported amount; the local-day window, timezone, currency, and provider coverage are visible; and a provider with no cost data is shown as unreported/unknown, never `$0`. Capture the popover and the real run's usage evidence.
2. **Windows — real desktop app, daemon, and agent.** Repeat row 1 in the Windows WebView using a real agent session that records usage. Verify the same labels and unknown/no-data behavior, and capture the popover and run evidence.
