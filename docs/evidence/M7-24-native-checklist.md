# M7-24 Native Checklist

Run against the pushed PR head. These checks are deferred to the consolidated M7-32 native pass; no native checks were run for this PR. Preserve failed evidence and use a unique output directory per run.

1. [ ] **macOS — Global with a real agent:** In the desktop webview with the real daemon and an installed, launchable agent, send a Global message. Confirm the recorded user message precedes and correlates with the daemon-produced reply; no placeholder or simulated content appears.
2. [ ] **macOS — unavailable runtime:** With no launchable agent, send a Global message. Confirm it is recorded, `runtime_unavailable` is shown, and no reply text is fabricated.
3. [ ] **macOS — workstream conversation:** Send a message to a real workstream with a launchable agent. Confirm the workstream target, ordered transcript, and daemon-produced reply are correlated; cancel a running turn and confirm its final status.
4. [ ] **macOS — reconnect after restart:** Send a message, restart the daemon, and reconnect. Confirm the persisted transcript remains ordered and correlated, then send another message and confirm no duplicate entry appears.
5. [ ] **macOS — model selector:** With a real catalog, confirm listed models/efforts come from the daemon, the effective choice is checked by `agents.validate_selection`, and non-launchable providers are disabled. With no launchable provider, confirm no model/effort choice is available or selected.
6. [ ] **macOS — accessibility:** Use keyboard-only navigation and a screen reader. Confirm target/model controls have useful labels, focus is visible and retained, and runtime/catalog status changes are announced.
7. [ ] **macOS — display scaling:** At supported DPI/window sizes, open the target and model menus and confirm controls, status text, and transcript remain readable and unclipped.
8. [ ] **Windows — M7-32:** Run the applicable end-to-end, accessibility, and display-scaling checks on the native Windows worker; record per-row evidence against this same source SHA.
