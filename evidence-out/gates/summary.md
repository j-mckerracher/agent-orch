## Native gates: win32 (FAIL)

Source 6c17442eaee5243b48034dfc53d7f43d25f1ab82 (tree 9b9612ab6084c644e344603028149084118cc889).

OVERALL: FAIL (33 PASS, 1 FAIL, 0 SKIPPED; exit 1)

| Gate | Verdict | Exit | Seconds | Notes |
|---|---|---|---|---|
| node-self-tests | PASS | 0 | 6 |  |
| daemon-build | PASS | 0 | 293 |  |
| ipc-tests | PASS | 0 | 32 |  |
| daemon-tests | PASS | 0 | 145 |  |
| daemon-lifecycle | PASS | 0 | 11 |  |
| coordinator-review-tests | PASS | 0 | 11 |  |
| git-lib-tests | PASS | 0 | 21 |  |
| cli-cleanup-safety-tests | PASS | 0 | 67 |  |
| private-fs-tests | PASS | 0 | 5 |  |
| store-tests | PASS | 0 | 96 |  |
| lock-takeover-test | PASS | 0 | 1 |  |
| diagnostics-tests | PASS | 0 | 15 |  |
| runtime-command-line-tests | PASS | 0 | 19 |  |
| cli-command-line-tests | PASS | 0 | 6 |  |
| windows-transport-tests | PASS | 0 | 6 |  |
| cli-build | PASS | 0 | 1 |  |
| p12-windows-checks | PASS | 0 | 3 |  |
| clippy-scoped | PASS | 0 | 48 |  |
| desktop-npm-ci | PASS | 0 | 18 |  |
| desktop-tsc | PASS | 0 | 6 |  |
| desktop-build | PASS | 0 | 3 |  |
| desktop-vitest | FAIL | 1 | 58 | exit code 1 |
| tauri-build | PASS | 0 | 168 |  |
| tauri-tests | PASS | 0 | 224 |  |
| tauri-clippy-clean | PASS | 0 | 1 |  |
| tauri-clippy | PASS | 0 | 68 |  |
| desktop-package-win | PASS | 0 | 302 |  |
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
