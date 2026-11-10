## Native gates: win32 (FAIL)

Source fd6cdfbd3eab4dc14977432a85dbb0c15c931511 (tree da64d184471356ab17fcf0d9b6328087a682c3c6).

| Gate | Verdict | Exit | Seconds | Notes |
|---|---|---|---|---|
| daemon-build | PASS | 0 | 125 |  |
| ipc-tests | PASS | 0 | 47 |  |
| daemon-tests | PASS | 0 | 117 |  |
| daemon-lifecycle | PASS | 0 | 13 |  |
| coordinator-review-tests | PASS | 0 | 18 |  |
| git-lib-tests | PASS | 0 | 28 |  |
| cli-cleanup-safety-tests | PASS | 0 | 134 |  |
| private-fs-tests | PASS | 0 | 13 |  |
| runtime-command-line-tests | PASS | 0 | 35 |  |
| cli-command-line-tests | PASS | 0 | 27 |  |
| windows-transport-tests | PASS | 0 | 6 |  |
| cli-build | PASS | 0 | 1 |  |
| p12-windows-checks | PASS | 0 | 5 |  |
| clippy-scoped | PASS | 0 | 66 |  |
| desktop-npm-ci | PASS | 0 | 28 |  |
| desktop-tsc | FAIL | 1 | 11 | exit code 1 |
| desktop-build | FAIL | 1 | 10 | exit code 1 |
| desktop-vitest | PASS | 0 | 58 |  |
| tauri-build | SKIPPED |  |  | prerequisite gate(s) did not pass: desktop-build |
| tauri-tests | SKIPPED |  |  | prerequisite gate(s) did not pass: desktop-build |
| tauri-clippy-clean | PASS | 0 | 9 |  |
| tauri-clippy | SKIPPED |  |  | prerequisite gate(s) did not pass: desktop-build |
| desktop-package-win | FAIL | 1 | 191 | exit code 1 |
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
