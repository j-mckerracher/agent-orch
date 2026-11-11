## Native gates: win32 (FAIL)

Source b44f88af63cb1ea7c68b7a827465db466ef308a2 (tree 79820e90846b09a64edf25fe8593ab751abbb323).

OVERALL: FAIL (9 PASS, 17 FAIL, 8 SKIPPED; exit 1)

### Not measured, so NOT a pass: 8 gate(s) SKIPPED

- p12-windows-checks
- desktop-tsc
- desktop-build
- desktop-vitest
- tauri-build
- tauri-tests
- tauri-clippy
- desktop-package-win

A skipped gate gives the run the verdict INCONCLUSIVE (exit 2) at best. Fix the prerequisite and run again.

| Gate | Verdict | Exit | Seconds | Notes |
|---|---|---|---|---|
| node-self-tests | PASS | 0 | 2 |  |
| daemon-build | PASS | 0 | 80 |  |
| ipc-tests | FAIL | 0 | 21 | test not reported: authenticated_health_and_shutdown_remove_owned_files; test not reported: handshake_rejects_unknown_fields; test not reported: methods_require_handshake_and_negotiated_capability; test not reported: replay_backlog_beyond_the_live_buffer_is_delivered_in_order; test not reported: requests_reject_unknown_fields_and_control_params; test not reported: stalled_and_garbage_connections_do_not_block_health; test not reported: subscription_replays_then_streams_and_reconnects_from_observed_cursor; test not reported: stale_socket_is_reclaimed_after_refused_probe; test not reported: wrong_missing_and_unsupported_handshakes_are_sanitized; test not reported: timeout_applies_to_both_stream_directions; test not reported: live_server_rejects_competing_start_and_idle_cleanup_removes_files; test not reported: a_wrong_token_is_an_authentication_failure; test not reported: authenticated_request_resets_idle_deadline |
| daemon-tests | FAIL | 101 | 88 | a descendant kept the output pipes open after the process exited; the pipes were abandoned; exit code 101; 1 process(es) of this run survived the gate; see daemon-tests.survivors.txt |
| daemon-lifecycle | FAIL | 0 | 11 | 1 process(es) of this run survived the gate; see daemon-lifecycle.survivors.txt |
| coordinator-review-tests | FAIL | 0 | 11 | 1 process(es) of this run survived the gate; see coordinator-review-tests.survivors.txt |
| git-lib-tests | FAIL | 0 | 20 | 1 process(es) of this run survived the gate; see git-lib-tests.survivors.txt |
| cli-cleanup-safety-tests | FAIL | 0 | 57 | 1 process(es) of this run survived the gate; see cli-cleanup-safety-tests.survivors.txt |
| private-fs-tests | FAIL | 0 | 6 | 1 process(es) of this run survived the gate; see private-fs-tests.survivors.txt |
| store-tests | FAIL | 0 | 88 | 1 process(es) of this run survived the gate; see store-tests.survivors.txt |
| lock-takeover-test | FAIL | 0 | 1 | 1 process(es) of this run survived the gate; see lock-takeover-test.survivors.txt |
| diagnostics-tests | FAIL | 0 | 14 | 1 process(es) of this run survived the gate; see diagnostics-tests.survivors.txt |
| runtime-command-line-tests | FAIL | 0 | 16 | 1 process(es) of this run survived the gate; see runtime-command-line-tests.survivors.txt |
| cli-command-line-tests | FAIL | 0 | 6 | 1 process(es) of this run survived the gate; see cli-command-line-tests.survivors.txt |
| windows-transport-tests | FAIL | 0 | 6 | filtered out is 42, expected 33; 1 process(es) of this run survived the gate; see windows-transport-tests.survivors.txt |
| cli-build | FAIL | 0 | 0 | 1 process(es) of this run survived the gate; see cli-build.survivors.txt |
| p12-windows-checks | SKIPPED |  |  | prerequisite gate(s) did not pass: cli-build |
| clippy-scoped | FAIL | 0 | 41 | 1 process(es) of this run survived the gate; see clippy-scoped.survivors.txt |
| desktop-npm-ci | FAIL | 0 | 8 | 1 process(es) of this run survived the gate; see desktop-npm-ci.survivors.txt |
| desktop-tsc | SKIPPED |  |  | prerequisite gate(s) did not pass: desktop-npm-ci |
| desktop-build | SKIPPED |  |  | prerequisite gate(s) did not pass: desktop-npm-ci |
| desktop-vitest | SKIPPED |  |  | prerequisite gate(s) did not pass: desktop-npm-ci |
| tauri-build | SKIPPED |  |  | prerequisite gate(s) did not pass: desktop-build |
| tauri-tests | SKIPPED |  |  | prerequisite gate(s) did not pass: desktop-build |
| tauri-clippy-clean | FAIL | 0 | 1 | 1 process(es) of this run survived the gate; see tauri-clippy-clean.survivors.txt |
| tauri-clippy | SKIPPED |  |  | prerequisite gate(s) did not pass: desktop-build |
| desktop-package-win | SKIPPED |  |  | prerequisite gate(s) did not pass: desktop-npm-ci |
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
