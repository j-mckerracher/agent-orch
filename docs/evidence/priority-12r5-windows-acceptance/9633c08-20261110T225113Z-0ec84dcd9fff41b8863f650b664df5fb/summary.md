## Native gates: win32 (PASS)

Source 9633c088df587a1d4db3b2dfe1ec770ed41af195 (tree d021497a93da28447d12e825d50030b9f8de1b8d).

OVERALL: PASS (34 PASS, 0 FAIL, 0 SKIPPED; exit 0)

| Gate | Verdict | Exit | Seconds | Notes |
|---|---|---|---|---|
| node-self-tests | PASS | 0 | 2 |  |
| daemon-build | PASS | 0 | 65 |  |
| ipc-tests | PASS | 0 | 21 |  |
| daemon-tests | PASS | 0 | 59 |  |
| daemon-lifecycle | PASS | 0 | 9 |  |
| coordinator-review-tests | PASS | 0 | 8 |  |
| git-lib-tests | PASS | 0 | 17 |  |
| cli-cleanup-safety-tests | PASS | 0 | 54 |  |
| private-fs-tests | PASS | 0 | 5 |  |
| store-tests | PASS | 0 | 77 |  |
| lock-takeover-test | PASS | 0 | 1 |  |
| diagnostics-tests | PASS | 0 | 14 |  |
| runtime-command-line-tests | PASS | 0 | 17 |  |
| cli-command-line-tests | PASS | 0 | 6 |  |
| windows-transport-tests | PASS | 0 | 6 |  |
| cli-build | PASS | 0 | 0 |  |
| p12-windows-checks | PASS | 0 | 3 |  |
| clippy-scoped | PASS | 0 | 43 |  |
| desktop-npm-ci | PASS | 0 | 14 |  |
| desktop-tsc | PASS | 0 | 4 |  |
| desktop-build | PASS | 0 | 2 |  |
| desktop-vitest | PASS | 0 | 21 |  |
| tauri-build | PASS | 0 | 152 |  |
| tauri-tests | PASS | 0 | 185 |  |
| tauri-clippy-clean | PASS | 0 | 1 |  |
| tauri-clippy | PASS | 0 | 73 |  |
| desktop-package-win | PASS | 0 | 312 |  |
| git-config | PASS |  |  | core.autocrlf=false core.longpaths=true |
| gitattributes-before | PASS |  |  |  |
| source-status-before | PASS |  |  |  |
| gitattributes-after | PASS |  |  |  |
| source-status-after | PASS |  |  |  |
| source-diff-after | PASS |  |  |  |
| privacy-scan | PASS |  |  |  |

### Not covered by this result (UNPROVEN by construction)

- unproven-second-user: named-pipe denial to a second local user
- unproven-remote-client: rejection of a genuinely remote client
- unproven-slow-reader-64kib: a daemon write above the 64 KiB pipe buffer to a slow reader
- unproven-e2e-windows: the desktop e2e suite needs a Unix socket and is not run on Windows
- unproven-tauri-webview-click-path: webview and React click path through the approval flow
- unproven-gui-install-launch: installing the installer and launching the app
- unproven-gui-task-survives-close: a daemon-owned task surviving window close and reopen
- unproven-gui-event-reconnect: event-stream reconnect at the saved cursor after reopen
- unproven-stale-token-relaunch-timing: stale-token relaunch timing on Windows
- unproven-daemon-idle-behaviour: 60 s idle exit and subscription keep-alive on the real app
- unproven-daemon-kill-and-sidecar-failure: daemon kill and sidecar start failure
