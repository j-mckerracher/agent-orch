# Priority 12r5 Windows run of 9633c08

overall: PASS
source: 9633c088df587a1d4db3b2dfe1ec770ed41af195 (tree d021497a93da28447d12e825d50030b9f8de1b8d)
runbook commit (operator-declared, not byte proof): c94c2cc55969a14d65d347dceabf7067eca1c183
runbook blob: c2736c9bf9319c1990691a023556ad311e16f8d0
run id: 20261110T225113Z-0ec84dcd9fff41b8863f650b664df5fb
run root base: fallback number 2 (C:\t could not be created)
OS: Microsoft Windows 11 Education 10.0.26200
preflight exit code: 0; harness exit code: 0; gates run: 27 of 27
overall sources: json 'PASS', summary 'PASS', exit 'PASS'
node self-tests: exit 0, pass 71 of 71, fail 0
p12 verdict lines: 1; last: P12-WINDOWS-CHECKS: PASS
skipped gates: none

## Gates
- cli-build: PASS (exit 0, 0 s) 
- cli-cleanup-safety-tests: PASS (exit 0, 54 s) 
- cli-command-line-tests: PASS (exit 0, 6 s) 
- clippy-scoped: PASS (exit 0, 43 s) 
- coordinator-review-tests: PASS (exit 0, 8 s) 
- daemon-build: PASS (exit 0, 65 s) 
- daemon-lifecycle: PASS (exit 0, 9 s) 
- daemon-tests: PASS (exit 0, 59 s) 
- desktop-build: PASS (exit 0, 2 s) 
- desktop-npm-ci: PASS (exit 0, 14 s) 
- desktop-package-win: PASS (exit 0, 312 s) 
- desktop-tsc: PASS (exit 0, 4 s) 
- desktop-vitest: PASS (exit 0, 21 s) 
- diagnostics-tests: PASS (exit 0, 14 s) 
- git-lib-tests: PASS (exit 0, 17 s) 
- ipc-tests: PASS (exit 0, 21 s) 
- lock-takeover-test: PASS (exit 0, 1 s) 
- node-self-tests: PASS (exit 0, 2 s) 
- p12-windows-checks: PASS (exit 0, 3 s) 
- private-fs-tests: PASS (exit 0, 5 s) 
- runtime-command-line-tests: PASS (exit 0, 17 s) 
- store-tests: PASS (exit 0, 77 s) 
- tauri-build: PASS (exit 0, 152 s) 
- tauri-clippy: PASS (exit 0, 73 s) 
- tauri-clippy-clean: PASS (exit 0, 1 s) 
- tauri-tests: PASS (exit 0, 185 s) 
- windows-transport-tests: PASS (exit 0, 6 s) 

## Harness checks
- git-config: PASS 
- gitattributes-after: PASS 
- gitattributes-before: PASS 
- privacy-scan: PASS 
- source-diff-after: PASS 
- source-status-after: PASS 
- source-status-before: PASS 

## Counts (derived expectation; a mismatch is a finding)
- ipc-tests: running [46,0], passed 46, ignored 0; expected [46,0], 46, 0: match
- daemon-tests: running [52,4,0,1], passed 57, ignored 0; expected [52,4,0,1], 57, 0: match
- coordinator-review-tests: running [4], passed 4, ignored 0; expected [4], 4, 0: match
- git-lib-tests: running [49], passed 49, ignored 0; expected [49], 49, 0: match
- cli-cleanup-safety-tests: running [3], passed 3, ignored 0; expected [3], 3, 0: match
- private-fs-tests: running [11,0], passed 11, ignored 0; expected [11,0], 11, 0: match
- store-tests: running [33,9,2,13,8,11,3,3,3,11,4,1,0], passed 100, ignored 1; expected [33,9,2,13,8,11,3,3,3,11,4,1,0], 100, 1: match
- lock-takeover-test: running [1], passed 1, ignored 0; expected [1], 1, 0: match
- diagnostics-tests: running [0,6,7,15,0], passed 28, ignored 0; expected [0,6,7,15,0], 28, 0: match
- runtime-command-line-tests: running [8], passed 8, ignored 0; expected [8], 8, 0: match
- cli-command-line-tests: running [2], passed 2, ignored 0; expected [2], 2, 0: match
- windows-transport-tests: running [13], passed 13, ignored 0; expected [13], 13, 0: match
- tauri-tests: running [14,0,0], passed 14, ignored 0; expected [14,0,0], 14, 0: match
- desktop-vitest: Test Files  15 passed (15); Tests  290 passed (290); expected 15 files, 290 tests

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
INFO run-root length 32 characters
INFO platform windows
INFO acl-daemon.log: NOT CHECKED here; only the desktop host creates daemon.log. Its owner-only DACL is covered by the tauri-tests test a_new_and_an_existing_log_are_owner_only_on_every_platform and is UNPROVEN on a real host
INFO cmdline-65k-literal: the OS refused to start it, as expected: This command cannot be run due to the error: The filename or extension is too long.
P12-WINDOWS-CHECKS: PASS

## Previously UNPROVEN, measured by this run only where the gates passed
- measured (PASS): TS2883 fix and desktop-build on Windows at a real directory (not shown to survive a SUBST root)
- measured (PASS): tauri-build, tauri-tests [14,0,0] and tauri-clippy
- measured (PASS): the Windows installer (desktop-package-win)
- measured (PASS): Windows store-tests (100 + 1 ignored), diagnostics-tests (28), lock-takeover-test (1)
- measured (PASS): node self-tests exit code and count (71) on the Windows Node, including node --test glob expansion
- measured (PASS): the preflight accepting a real local directory (positive case only)

## UNPROVEN (always)
- the preflight REFUSING a real SUBST drive, mapped drive, junction and 8.3 path; a GitHub runner path (D:\a\..., RUNNER_TEMP) passing
- daemon.log owner-only ACL through the real desktop host
- the launch-failure window (CHECK launch-failure-window: NOT RUN)
- the takeover temp directory ACL (.orchestrator.lock.takeover-<n>); lock-takeover-test covers the lock file only
- a real window: webview click path, install and launch, close and reopen, event reconnect
- the Windows survivor scan finding a true survivor, runBounded under a real stuck descendant, the EBUSY retry under real contention
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

## Reporting parser note
Step 3 report-parser issue: the first execution stopped when PowerShell ConvertFrom-Json rejected case-distinct keys SKIPPED and skipped in overall.result.json.
Report-only recovery: overall.result.json was excluded from gate/harness classification and its overall field was read with ConvertFrom-Json -AsHashtable. All other result JSON files used the runbook parser unchanged.
No harness gate was rerun, no source file or harness code was changed, and the original exception is preserved in runbook-parser-error.txt.

## Reporting identity-check note
The first Step 5 identity guard reported a mismatch after creating local commit 53715a2988003a37f2463bcd7e239eb619dfb1df. That commit was not pushed.
The exact four-field guard was replayed in a fresh PowerShell session and passed; direct git show checks also confirmed author and committer names and noreply addresses match the runbook-pinned repo identity.
The original local commit remains in this evidence branch history. This follow-up commit records the verifier diagnostic and does not change test results.

## Publication continuation note
A resumed Step 5 privacy-rescan attempt stopped before staging because the continuation session had not initialized the runbook UTF-8 encoder.
The exact runbook redaction and privacy-scan block is rerun with its required no-BOM UTF-8 encoder. No test gates or source code are rerun or changed.
