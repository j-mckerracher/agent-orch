# M7-32 Workstreams Gate Native Checklist

**Status: Deferred to the consolidated native pass.** This assignment corrects the real-daemon e2e assertion; no native checks were run.

Use a disposable workstream and task on the exact candidate commit. Record tester, date, full SHA, OS/WebView version, daemon and agent versions, result, and evidence path. Preserve failed output and screenshots; omit credentials.

1. **macOS — WKWebView, live daemon, and real agent.** Run a real agent task, link it to a disposable workstream, add an objective and gate, select that task as evidence, and mark the gate verified. Confirm the visible gate state and daemon `evidence_task_id` agree before and after refreshing/reopening the view. Capture the UI, daemon record, and logs.
2. **Windows — WebView2, live daemon, and real agent.** Repeat row 1 on native Windows using the supported daemon transport. Capture the UI, daemon record, logs, and full test output.
3. **Assistive technology.** With VoiceOver on macOS and Narrator or NVDA on Windows, use keyboard navigation to select a task as gate evidence and verify the gate. Confirm the gate state and evidence selector labels are announced and the controls remain operable.
4. **Windows DPI.** Repeat the gate/evidence interaction at 100%, 125%, 150%, and 200% scaling. Confirm the gate state, evidence selector, labels, and action do not overlap or clip; capture a screenshot at each scale.

Mark unrun rows **UNVERIFIED**; automated e2e results do not establish native acceptance.
