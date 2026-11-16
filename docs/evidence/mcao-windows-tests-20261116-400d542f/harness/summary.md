## Native gates: win32 (FAIL)

Source ea5b5abca917ff9bf8799a486b2e425afa755a87 (tree 0a0f785bf6bda634fcf48b7c3befec07a29ebd56).

OVERALL: FAIL (32 PASS, 2 FAIL, 0 SKIPPED; exit 1)

| Gate | Verdict | Exit | Seconds | Notes |
|---|---|---|---|---|
| node-self-tests | FAIL | 1 | 8 | exit code 1 |
| daemon-build | PASS | 0 | 88 |  |
| ipc-tests | PASS | 0 | 22 |  |
| daemon-tests | PASS | 0 | 136 |  |
| daemon-lifecycle | PASS | 0 | 29 |  |
| coordinator-review-tests | PASS | 0 | 12 |  |
| git-lib-tests | PASS | 0 | 27 |  |
| cli-cleanup-safety-tests | PASS | 0 | 67 |  |
| private-fs-tests | PASS | 0 | 4 |  |
| store-tests | PASS | 0 | 99 |  |
| lock-takeover-test | PASS | 0 | 1 |  |
| diagnostics-tests | PASS | 0 | 14 |  |
| runtime-command-line-tests | PASS | 0 | 17 |  |
| cli-command-line-tests | PASS | 0 | 5 |  |
| windows-transport-tests | PASS | 0 | 6 |  |
| cli-build | PASS | 0 | 1 |  |
| p12-windows-checks | PASS | 0 | 5 |  |
| clippy-scoped | PASS | 0 | 45 |  |
| desktop-npm-ci | PASS | 0 | 21 |  |
| desktop-tsc | PASS | 0 | 7 |  |
| desktop-build | PASS | 0 | 3 |  |
| desktop-vitest | PASS | 0 | 97 |  |
| tauri-build | PASS | 0 | 148 |  |
| tauri-tests | PASS | 0 | 139 |  |
| tauri-clippy-clean | PASS | 0 | 0 |  |
| tauri-clippy | PASS | 0 | 48 |  |
| desktop-package-win | FAIL | 1 | 245 | exit code 1 |
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
