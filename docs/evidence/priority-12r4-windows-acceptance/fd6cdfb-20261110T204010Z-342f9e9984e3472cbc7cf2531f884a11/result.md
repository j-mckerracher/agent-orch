# Priority 12r4 Windows run of fd6cdfb

overall: FAIL
source: fd6cdfbd3eab4dc14977432a85dbb0c15c931511 (tree da64d184471356ab17fcf0d9b6328087a682c3c6)
runbook commit (operator-declared, not byte proof): c04cff7f42414815f5375c9aec7633d9bcd46795
runbook blob: 3ef35d409b0157d39b3758dfc460a72736a154a7
run id: 20261110T204010Z-342f9e9984e3472cbc7cf2531f884a11
OS: Microsoft Windows 11 Education 10.0.26200
harness exit code: 1; gates run: 23 of 23
node self-tests: pass 52 of 52, fail 0
p12 verdict lines: 1; last: P12-WINDOWS-CHECKS: PASS

## Gates
- cli-build: PASS (exit 0, 1 s) 
- cli-cleanup-safety-tests: PASS (exit 0, 134 s) 
- cli-command-line-tests: PASS (exit 0, 27 s) 
- clippy-scoped: PASS (exit 0, 66 s) 
- coordinator-review-tests: PASS (exit 0, 18 s) 
- daemon-build: PASS (exit 0, 125 s) 
- daemon-lifecycle: PASS (exit 0, 13 s) 
- daemon-tests: PASS (exit 0, 117 s) 
- desktop-build: FAIL (exit 1, 10 s) exit code 1
- desktop-npm-ci: PASS (exit 0, 28 s) 
- desktop-package-win: FAIL (exit 1, 191 s) exit code 1
- desktop-tsc: FAIL (exit 1, 11 s) exit code 1
- desktop-vitest: PASS (exit 0, 58 s) 
- git-lib-tests: PASS (exit 0, 28 s) 
- ipc-tests: PASS (exit 0, 47 s) 
- p12-windows-checks: PASS (exit 0, 5 s) 
- private-fs-tests: PASS (exit 0, 13 s) 
- runtime-command-line-tests: PASS (exit 0, 35 s) 
- tauri-build: SKIPPED (exit ,  s) prerequisite gate(s) did not pass: desktop-build
- tauri-clippy: SKIPPED (exit ,  s) prerequisite gate(s) did not pass: desktop-build
- tauri-clippy-clean: PASS (exit 0, 9 s) 
- tauri-tests: SKIPPED (exit ,  s) prerequisite gate(s) did not pass: desktop-build
- windows-transport-tests: PASS (exit 0, 6 s) 

## Harness checks
- git-config: PASS 
- gitattributes-after: PASS 
- gitattributes-before: PASS 
- privacy-scan: PASS 
- source-diff-after: PASS 
- source-status-after: PASS 
- source-status-before: PASS 

## p12-windows-checks
CHECK acl-files-present: PASS - daemon.token, store.sqlite
CHECK acl-data-dir: PASS - only the current user, full control, protected, nothing inherited
CHECK acl-.orchestrator.lock: PASS - only the current user, full control, protected, nothing inherited
CHECK acl-daemon.token: PASS - only the current user, full control, protected, nothing inherited
CHECK acl-store.sqlite: PASS - only the current user, full control, protected, nothing inherited
CHECK acl-store.sqlite-shm: PASS - only the current user, full control, protected, nothing inherited
CHECK acl-store.sqlite-wal: PASS - only the current user, full control, protected, nothing inherited
CHECK cmdline-refused: PASS - refused before any record, with the limit named and no objective echoed
CHECK launch-failure-window: NOT RUN - needs a person to drive the desktop window; the tauri-tests gate covers the redaction logic
INFO run-root length 18 characters
INFO platform windows
INFO acl-daemon.log: NOT CHECKED here; only the desktop host creates daemon.log. Its owner-only DACL is covered by the tauri-tests test a_new_and_an_existing_log_are_owner_only_on_every_platform and is UNPROVEN on a real host
INFO cmdline-65k-literal: the OS refused to start it, as expected: This command cannot be run due to the error: The filename or extension is too long.
P12-WINDOWS-CHECKS: PASS

## UNPROVEN
- daemon.log owner-only ACL through the real desktop host
- the launch-failure window (CHECK launch-failure-window: NOT RUN)
- the takeover temp directory ACL (.orchestrator.lock.takeover-<n>)
- a real window: webview click path, install and launch, close and reopen, event reconnect
- every Windows-side change not run natively before this run (lock DACL, EBUSY retry, survivor scan, p12 gate launcher, cli-build, redaction)
- all Windows counts (derived, not measured before this run)
- unproven-second-user
- unproven-remote-client
- unproven-slow-reader-64kib
- unproven-tauri-webview-click-path
- unproven-e2e-windows
- unproven-gui-install-launch
- unproven-gui-task-survives-close
- unproven-gui-event-reconnect
- unproven-stale-token-relaunch-timing
- unproven-daemon-idle-behaviour
- unproven-daemon-kill-and-sidecar-failure
- unproven-data-dir-acl (real host)
- unproven-launch-failure-redaction
- unproven-objective-64kib-r12
