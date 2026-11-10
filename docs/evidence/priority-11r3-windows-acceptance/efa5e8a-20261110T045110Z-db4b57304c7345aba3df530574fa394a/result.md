# Priority 11 round 3 Windows acceptance of efa5e8af3469082226c61cccfea167cc607d6a0f (M5-05 Tauri desktop host, IPC relay and native lifecycle proof, with the M5-05r, M5-05r2 and M5-05r3 remediation)

This run covers the Priority 11 round 3 candidate: the IPC relay and its timeouts, the live-subscription idle
logic and the fail-fast stale-token connect, the daemon built natively on Windows with its headless
lifecycle proof, the real-daemon coordinator and review process tests (coordinator_review), the Tauri host
built, unit-tested and linted natively together with its real-daemon bridge tests, the desktop install,
type-check, build and unit tests, and whether the NSIS installer builds (npm run package:win). The coordinator
decides acceptance. The expected counts are derived from the cfg and test attributes at the pinned source
for Windows (IPC 45, daemon 42 unit tests plus 4 coordinator_review tests = 46 with the running sequence
42,4,0, windows_transport 13, src-tauri 9 with the sequence 9,0,0, lifecycle prints lifecycle: PASS with
no libtest count, coordinator-review-tests 4) and from the Mac Vitest run (15 files, 287 tests). They are
derived and unmeasured for this commit. They are not the Mac counts (IPC 45 with a different mix of tests,
daemon 72, src-tauri 10), and they are not the Priority 10r2 native counts (a different tree: IPC 38,
Vitest 14 files and 268 tests). Counts are reported as measured, not matched. No macOS or review result is
Windows evidence. The row tauri-native-bridge-host-level is PASS only when coordinator-review-tests and
tauri-tests both pass natively, and then it is a host-level result (the Rust Bridge against the real daemon
over the named pipe), not a webview, window or installer result. This run is not a native probe, not a
second-user, genuinely remote-client, slow-reader or Windows e2e claim, not a webview or click-path claim, not
a GUI install, launch, close and reopen, reconnect, idle or relaunch-timing claim, and not an M6
threat-review claim.

- Run ID: 20261110T045110Z-db4b57304c7345aba3df530574fa394a
- UTC date: 2026-11-10T05:12:52.3257132Z
- Source branch: orch/priority-11r3-m5-05-efa5e8a
- Branch tip at clone time: efa5e8af3469082226c61cccfea167cc607d6a0f
- Source commit: efa5e8af3469082226c61cccfea167cc607d6a0f
- Source tree and only parent (verified): source-tree.stdout.txt, source-parent.stdout.txt
- HEAD matched the source commit before the checks and after them: yes
  (source-head-before.stdout.txt, source-head-after.stdout.txt)
- Git top-level path form (plain drive-letter form required): source-toplevel.stdout.txt
- Baseline ancestry (e555c2c9), commit count, the fourteen pinned commits, the linear range, and the
  changed-file set (57 files): base-ancestry, range-count, range-merges, range-commits, range-files
  (consistency gates, not patch-equivalence proof)
- Runbook identity (operator-declared; exact bytes pinned by blob id, size, and SHA-256; not proof that
  the pasted commands match): runbook-identity.txt
- Overall result: FAIL UNPROVEN, not covered by this result: unproven-second-user, unproven-remote-client, unproven-slow-reader-64kib, unproven-tauri-webview-click-path, unproven-e2e-windows, unproven-gui-install-launch, unproven-gui-task-survives-close, unproven-gui-event-reconnect, unproven-stale-token-relaunch-timing, unproven-daemon-idle-behaviour, unproven-daemon-kill-and-sidecar-failure, tauri-native-bridge-host-level.
- UNPROVEN rows (never covered by an overall PASS): unproven-second-user, unproven-remote-client, unproven-slow-reader-64kib, unproven-tauri-webview-click-path, unproven-e2e-windows, unproven-gui-install-launch, unproven-gui-task-survives-close, unproven-gui-event-reconnect, unproven-stale-token-relaunch-timing, unproven-daemon-idle-behaviour, unproven-daemon-kill-and-sidecar-failure, tauri-native-bridge-host-level
- Toolchain: see git-version.stdout.txt, rustc-version.stdout.txt,
  cargo-version.stdout.txt, clippy-version.stdout.txt, node-version.stdout.txt,
  npm-version.stdout.txt, git-autocrlf.stdout.txt (exit code in git-autocrlf.exit.txt)
- Artifact records (size and SHA-256): daemon-binary.txt (dev-profile daemon), tauri-binary.txt
  (dev-profile Tauri host) and installer.txt (the NSIS installer, built and never installed)
- Clippy diagnostics, each classified BASELINE or NEW (NEW does not change the verdict):
  clippy-scoped.diagnostics.txt and tauri-clippy.diagnostics.txt. The Tauri host lint must name both
  crates (tauri-clippy.crate-names.txt).
- Lifecycle proof: daemon-lifecycle.summary.txt (the STEP lines and the lifecycle: PASS line). On a
  failure also daemon-lifecycle.diagnostics.txt and daemon-lifecycle.daemon-log.txt (and the same two
  files for daemon-tests, coordinator-review-tests and tauri-tests): the diagnostics block that the test
  prints, every daemon log block that a failing real-daemon test prints, and any surviving daemon.log.
- Real-daemon tests: coordinator-review-tests.summary.txt (4 tests) and tauri-tests.summary.txt (9 tests,
  3 of them bridge::real_tests). The derived row tauri-native-bridge-host-level.txt records whether both
  passed. It is host level only.
- Harness checks (not gates): gitattributes-before.summary.txt and gitattributes-after.summary.txt
  (LF pins), source-status-after.stdout.txt and source-status-after-plain.stdout.txt (git status),
  source-diff-after-stat.stdout.txt (git diff --stat), source-diff-after.stdout.txt and
  source-diff-after-ignore-cr.stdout.txt (git diff), diff-check.stdout.txt. gitattributes before:
  PASS. gitattributes after: PASS. git status after the run
  (porcelain lines): 0. git diff --stat after the run (lines): 0.
- UNPROVEN and NOT RUN rows: one unproven-*.txt file each, and tauri-native-bridge-host-level.txt. No
  native probe ran. No window was driven and no installer was installed. No daemon was started except by
  the tests themselves (lifecycle, coordinator_review and bridge::real_tests).
- Manual lifecycle notes, if an operator performed any step: manual-lifecycle.txt (operator
  observation only; it does not change a verdict)
- OS, CPU, PowerShell: see os.txt, cpu.txt, powershell-version.txt
- Agent runtime and Sonnet 5.5/high setting: unverified
- Survivor scan: every check has a survivors file. Processes whose image lies under
  this run's private temp directory, or whose command line names this run's directory (long or 8.3
  form), are listed and fail the check. Nothing is stopped by the scan.
- Raw stdout and stderr bytes are kept outside the checkout and outside the published files. Their
  size and SHA-256 are in each result.json.

| Command or row | Status | Exit | Timed out | Seconds | Running (N per binary) | Passed | Failed | Ignored | Notes |
|---|---|---|---|---|---|---|---|---|---|
| daemon-build | PASS | 0 | False | 104.142 |  |  |  |  |  |
| ipc-tests | PASS | 0 | False | 37.642 | 45,0 | 45 | 0 | 0 |  |
| daemon-tests | FAIL | 101 | False | 105.151 | 42,4 | 44 | 2 | 0 | Exit code 101. The output has no line matching (?m)^lifecycle: PASS[ \t]*\r?$. 2 test(s) failed. Per-binary running counts 42,4 differ from the derived 42,4,0. 44 passed. Expected 46 (derived for Windows, not measured before this run). No 'ok' line for approval_flow_approves_and_delivers_through_the_real_daemon. No 'ok' line for approval_flow_rejects_and_requests_changes_through_the_real_daemon. |
| daemon-lifecycle | PASS | 0 | False | 16.637 |  |  |  |  |  |
| coordinator-review-tests | FAIL | 101 | False | 8.928 | 4 | 2 | 2 | 0 | Exit code 101. 2 test(s) failed. 2 passed. Expected 4 (derived for Windows, not measured before this run). No 'ok' line for approval_flow_approves_and_delivers_through_the_real_daemon. No 'ok' line for approval_flow_rejects_and_requests_changes_through_the_real_daemon. |
| windows-transport-tests | PASS | 0 | False | 6.25 | 13 | 13 | 0 | 0 |  |
| clippy-scoped | PASS | 0 | False | 53.988 |  |  |  |  | 2 baseline diagnostic(s) and 0 NEW diagnostic(s) recorded in clippy-scoped.diagnostics.txt. The only baseline is the supervisor.rs single_match pair in orchestrator-runtime (a dependency of the daemon); the Tauri host has none. NEW diagnostics do not change the verdict; report each one verbatim. |
| desktop-npm-ci | PASS | 0 | False | 25.057 |  |  |  |  |  |
| desktop-tsc | PASS | 0 | False | 6.939 |  |  |  |  |  |
| desktop-build | PASS | 0 | False | 11.326 |  |  |  |  |  |
| desktop-vitest | PASS | 0 | False | 48.952 |  | 287 | 0 |  |  |
| tauri-build | PASS | 0 | False | 223.013 |  |  |  |  |  |
| tauri-tests | PASS | 0 | False | 201.98 | 9,0,0 | 9 | 0 | 0 |  |
| tauri-clippy-clean | PASS | 0 | False | 0.99 |  |  |  |  |  |
| tauri-clippy | PASS | 0 | False | 74.469 |  |  |  |  | 0 baseline diagnostic(s) and 0 NEW diagnostic(s) recorded in tauri-clippy.diagnostics.txt. The only baseline is the supervisor.rs single_match pair in orchestrator-runtime (a dependency of the daemon); the Tauri host has none. NEW diagnostics do not change the verdict; report each one verbatim. |
| desktop-package-win | PASS | 0 | False | 331.559 |  |  |  |  |  |
| unproven-second-user | UNPROVEN |  | False | 0 |  |  |  |  | Denial of the named pipe to a second local user was not exercised. No second account is used. See unproven-second-user.txt. |
| unproven-remote-client | UNPROVEN |  | False | 0 |  |  |  |  | Rejection of a genuinely remote client was not exercised. No remote host or SMB loopback is used. See unproven-remote-client.txt. |
| unproven-slow-reader-64kib | UNPROVEN |  | False | 0 |  |  |  |  | A daemon-to-client write above 64 KiB to a slow reader was not exercised. See unproven-slow-reader-64kib.txt. |
| unproven-tauri-webview-click-path | UNPROVEN |  | False | 0 |  |  |  |  | NOT RUN. The webview and the React click path were not driven: no window, no webview, no daemon_open, daemon_send or daemon_close call as the webview makes it, and nobody clicked through the approval flow. The bridge::real_tests drive the Rust Bridge directly, with no window. See unproven-tauri-webview-click-path.txt. |
| unproven-e2e-windows | UNPROVEN |  | False | 0 |  |  |  |  | NOT RUN. The desktop e2e suite (npm run test:e2e) uses a Unix domain socket and cannot run on Windows. No Windows e2e number exists. See unproven-e2e-windows.txt. |
| unproven-gui-install-launch | UNPROVEN |  | False | 0 |  |  |  |  | NOT RUN. The NSIS installer is built by desktop-package-win but never installed or launched by this harness. No window shows Connected and no sidecar daemon is started by the app. See unproven-gui-install-launch.txt. |
| unproven-gui-task-survives-close | UNPROVEN |  | False | 0 |  |  |  |  | NOT RUN. A real daemon-owned task surviving a window close and reopen was not driven. The headless daemon-lifecycle gate covers the daemon side with a fake agent and the UI-side relay, not the window. See unproven-gui-task-survives-close.txt. |
| unproven-gui-event-reconnect | UNPROVEN |  | False | 0 |  |  |  |  | NOT RUN. The window event stream reconnecting and resuming at the saved cursor and epoch, and drafts surviving a reopen, were not driven. See unproven-gui-event-reconnect.txt. |
| unproven-stale-token-relaunch-timing | UNPROVEN |  | False | 0 |  |  |  |  | UNMEASURED. About 0.3 s per relay attempt, about 0.6 s before relaunch, then 200 ms retries for 15 s, and 10 s relay handshake and write timeouts, are derived from code. Nothing was timed on Windows. See unproven-stale-token-relaunch-timing.txt. |
| unproven-daemon-idle-behaviour | UNPROVEN |  | False | 0 |  |  |  |  | NOT RUN. The 60 s true-idle exit, a live subscription keeping the daemon alive and the idle clock restarting were not observed on the app. Only the idle_subscription_tests in ipc-tests cover the logic. See unproven-daemon-idle-behaviour.txt. |
| unproven-daemon-kill-and-sidecar-failure | UNPROVEN |  | False | 0 |  |  |  |  | NOT RUN. Killing the daemon pid and a sidecar that fails to start were not exercised. Both stop or break a process, which this runbook never does. See unproven-daemon-kill-and-sidecar-failure.txt. |
| tauri-native-bridge-host-level | UNPROVEN |  | False | 0 |  |  |  |  | UNPROVEN: the host-level bridge is provable only when coordinator-review-tests and tauri-tests both PASS natively in this run. Gate statuses: coordinator-review-tests FAIL, tauri-tests PASS. See tauri-native-bridge-host-level.txt. |

## Result lines and observations

### daemon-build

```text
Compiling proc-macro2 v1.0.107
Compiling unicode-ident v1.0.27
Compiling quote v1.0.47
Compiling windows-link v0.2.1
Compiling stable_deref_trait v1.2.1
Compiling pin-project-lite v0.2.17
Compiling serde_core v1.0.229
Compiling serde v1.0.229
Compiling cfg-if v1.0.5
Compiling futures-core v0.3.34
Compiling utf_types v1.0.0
Compiling version_check v0.9.5
Compiling litemap v0.8.3
Compiling memchr v2.8.3
Compiling smallvec v1.16.5
Compiling writeable v0.6.4
Compiling bytes v1.12.1
Compiling icu_properties_data v2.4.0
Compiling utf8_iter v1.1.0
Compiling icu_normalizer_data v2.4.0
Compiling find-msvc-tools v0.1.15
Compiling shlex v2.0.1
Compiling generic-array v0.14.9
Compiling scopeguard v1.2.0
Compiling typenum v1.20.1
Compiling windows-sys v0.61.2
Compiling futures-sink v0.3.34
Compiling vcpkg v0.2.15
Compiling cc v1.11.1
Compiling parking_lot_core v0.9.12
Compiling lock_api v0.4.14
Compiling pkg-config v0.3.34
Compiling equivalent v1.0.3
Compiling zmij v1.0.23
Compiling crossbeam-utils v0.8.23
Compiling foldhash v0.2.0
Compiling allocator-api2 v0.2.21
Compiling syn v3.0.9
Compiling syn v2.0.119
Compiling serde_json v1.0.154
Compiling futures-task v0.3.34
Compiling block-buffer v0.10.4
Compiling crypto-common v0.1.6
Compiling autocfg v1.5.1
Compiling slab v0.4.12
Compiling percent-encoding v2.3.2
Compiling futures-io v0.3.34
Compiling thiserror v2.0.21
Compiling once_cell v1.21.4
Compiling form_urlencoded v1.2.2
Compiling futures-util v0.3.34
Compiling digest v0.10.7
Compiling tracing-core v0.1.36
Compiling hashbrown v0.16.1
Compiling num-traits v0.2.19
Compiling parking_lot v0.12.5
Compiling log v0.4.35
Compiling cpufeatures v0.2.17
Compiling itoa v1.0.18
Compiling hashbrown v0.17.1
Compiling crc-catalog v2.5.0
Compiling parking v2.2.1
Compiling sha2 v0.10.9
Compiling hashlink v0.11.1
Compiling crc v3.4.0
Compiling mio v1.2.4
Compiling socket2 v0.6.5
Compiling event-listener v5.4.2
Compiling indexmap v2.14.2
Compiling futures-intrusive v0.5.0
Compiling synstructure v0.14.0
Compiling crossbeam-queue v0.3.14
Compiling base64 v0.22.1
Compiling libsqlite3-sys v0.37.0
Compiling spin v0.9.9
Compiling futures-executor v0.3.34
Compiling tokio v1.54.1
Compiling futures-channel v0.3.34
Compiling flume v0.12.0
Compiling hex v0.4.3
Compiling atoi v2.0.0
Compiling heck v0.5.0
Compiling dotenvy v0.15.7
Compiling getrandom v0.3.4
Compiling tracing-attributes v0.1.31
Compiling libc v0.2.191
Compiling zerofrom-derive v0.1.8
Compiling yoke-derive v0.8.4
Compiling zerovec-derive v0.11.7
Compiling displaydoc v0.2.7
Compiling serde_derive v1.0.229
Compiling thiserror-impl v2.0.21
Compiling tokio-macros v2.7.2
Compiling tokio-stream v0.1.19
Compiling tracing v0.1.44
Compiling async-trait v0.1.89
Compiling subtle v2.6.1
Compiling zerofrom v0.1.8
Compiling orchestrator-git v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\crates\orchestrator-git)
Compiling yoke v0.8.4
Compiling zerovec v0.11.9
Compiling zerotrie v0.2.5
Compiling either v1.19.0
Compiling orchestrator-core v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\crates\orchestrator-core)
Compiling uuid v1.18.1
Compiling orchestrator-ipc v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\crates\orchestrator-ipc)
Compiling tinystr v0.8.5
Compiling potential_utf v0.1.6
Compiling icu_locale_core v2.4.0
Compiling icu_collections v2.4.0
Compiling icu_provider v2.4.0
Compiling icu_properties v2.4.0
Compiling icu_normalizer v2.4.0
Compiling orchestrator-runtime v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\crates\orchestrator-runtime)
Compiling orchestrator-agents v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\crates\orchestrator-agents)
Compiling idna_adapter v1.2.2
Compiling idna v1.1.0
Compiling url v2.5.8
Compiling sqlx-core v0.9.0
Compiling sqlx-sqlite v0.9.0
Compiling sqlx-macros-core v0.9.0
Compiling sqlx-macros v0.9.0
Compiling sqlx v0.9.0
Compiling orchestrator-store v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\crates\orchestrator-store)
Compiling orchestrator-engine v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\crates\orchestrator-engine)
Compiling orchestrator-daemon v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\crates\orchestrator-daemon)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 43s
```

### ipc-tests

```text
running 45 tests
test result: ok. 45 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.70s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### daemon-tests

```text
running 42 tests
test result: ok. 42 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 51.07s
running 4 tests
test result: FAILED. 2 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 21.18s
```

### daemon-lifecycle

```text
STEP 1 start a real daemon with a fake agent binary
STEP 2 submit a task; the daemon launches the agent process
STEP 3 open the UI-side relay and subscribe to the event stream
STEP 4 close the UI client: this must not stop the daemon or the agent
STEP 5 reopen: authenticate again and resume the stream at the saved cursor and epoch
STEP 6 cancel through another client; the reopened stream sees the transition with no gap
STEP 7 a relay on a stopped daemon reports a remote close, then the daemon shuts down cleanly
lifecycle: PASS
```

### coordinator-review-tests

```text
running 4 tests
test result: FAILED. 2 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.83s
```

### windows-transport-tests

```text
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 32 filtered out; finished in 5.04s
```

### clippy-scoped

```text
Compiling stable_deref_trait v1.2.1
Checking windows-link v0.2.1
Checking cfg-if v1.0.5
Checking serde_core v1.0.229
Compiling litemap v0.8.3
Compiling utf_types v1.0.0
Compiling smallvec v1.16.5
Compiling zerofrom v0.1.8
Compiling writeable v0.6.4
Checking windows-sys v0.61.2
Compiling futures-core v0.3.34
Checking memchr v2.8.3
Compiling pin-project-lite v0.2.17
Compiling yoke v0.8.4
Compiling utf8_iter v1.1.0
Compiling scopeguard v1.2.0
Compiling zerovec v0.11.9
Compiling zerotrie v0.2.5
Compiling typenum v1.20.1
Compiling lock_api v0.4.14
Compiling icu_properties_data v2.4.0
Compiling icu_normalizer_data v2.4.0
Compiling futures-sink v0.3.34
Compiling tinystr v0.8.5
Compiling potential_utf v0.1.6
Checking thiserror v2.0.21
Compiling equivalent v1.0.3
Compiling icu_locale_core v2.4.0
Compiling icu_collections v2.4.0
Compiling generic-array v0.14.9
Checking serde v1.0.229
Compiling parking_lot_core v0.9.12
Compiling crypto-common v0.1.6
Compiling block-buffer v0.10.4
Compiling once_cell v1.21.4
Compiling futures-task v0.3.34
Compiling foldhash v0.2.0
Checking bytes v1.12.1
Checking icu_provider v2.4.0
Compiling mio v1.2.4
Compiling socket2 v0.6.5
Compiling slab v0.4.12
Compiling futures-io v0.3.34
Compiling icu_normalizer v2.4.0
Compiling icu_properties v2.4.0
Compiling allocator-api2 v0.2.21
Compiling percent-encoding v2.3.2
Compiling futures-util v0.3.34
Compiling form_urlencoded v1.2.2
Compiling hashbrown v0.16.1
Compiling tracing-core v0.1.36
Compiling tokio v1.54.1
Compiling digest v0.10.7
Compiling parking_lot v0.12.5
Compiling crossbeam-utils v0.8.23
Checking zmij v1.0.23
Compiling crc-catalog v2.5.0
Compiling parking v2.2.1
Compiling idna_adapter v1.2.2
Compiling hashbrown v0.17.1
Compiling log v0.4.35
Checking itoa v1.0.18
Compiling idna v1.1.0
Compiling cpufeatures v0.2.17
Compiling tracing v0.1.44
Compiling event-listener v5.4.2
Checking serde_json v1.0.154
Compiling crc v3.4.0
Compiling crossbeam-queue v0.3.14
Compiling sha2 v0.10.9
Compiling hashlink v0.11.1
Compiling url v2.5.8
Compiling indexmap v2.14.2
Compiling either v1.19.0
Compiling futures-intrusive v0.5.0
Compiling spin v0.9.9
Compiling num-traits v0.2.19
Compiling base64 v0.22.1
Compiling flume v0.12.0
Compiling futures-executor v0.3.34
Compiling atoi v2.0.0
Compiling futures-channel v0.3.34
Compiling tokio-stream v0.1.19
Checking libsqlite3-sys v0.37.0
Checking orchestrator-core v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\crates\orchestrator-core)
Checking orchestrator-git v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\crates\orchestrator-git)
Checking getrandom v0.3.4
Checking subtle v2.6.1
Checking getrandom v0.4.3
Checking fastrand v2.5.0
Compiling sqlx-core v0.9.0
Checking uuid v1.18.1
Checking orchestrator-ipc v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\crates\orchestrator-ipc)
Checking libc v0.2.191
Checking tempfile v3.27.0
Checking orchestrator-runtime v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\crates\orchestrator-runtime)
warning: you seem to be trying to use `match` for destructuring a single pattern. Consider using `if let`
warning: you seem to be trying to use `match` for destructuring a single pattern. Consider using `if let`
warning: `orchestrator-runtime` (lib) generated 2 warnings (run `cargo clippy --fix --lib -p orchestrator-runtime -- ` to apply 2 suggestions)
Checking orchestrator-agents v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\crates\orchestrator-agents)
Checking sqlx-sqlite v0.9.0
Compiling sqlx-macros-core v0.9.0
Compiling sqlx-macros v0.9.0
Checking sqlx v0.9.0
Checking orchestrator-store v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\crates\orchestrator-store)
Checking orchestrator-engine v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\crates\orchestrator-engine)
Checking orchestrator-daemon v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\crates\orchestrator-daemon)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 53.00s
```

### desktop-npm-ci

```text

```

### desktop-tsc

```text

```

### desktop-build

```text
✓ built in 1.81s
```

### desktop-vitest

```text
Test Files  15 passed (15)
Tests  287 passed (287)
Duration  31.29s (environment 73%, setup 14%, tests 9%, import 2%, transform 2%)
```

### tauri-build

```text
Compiling proc-macro2 v1.0.107
Compiling serde_core v1.0.229
Compiling siphasher v1.0.4
Compiling fastrand v2.5.0
Compiling stable_deref_trait v1.2.1
Compiling serde v1.0.229
Compiling time-core v0.1.9
Compiling num-conv v0.2.2
Compiling ident_case v1.0.1
Compiling winnow v1.0.5
Compiling typeid v1.0.3
Compiling deranged v0.5.9
Compiling phf_shared v0.13.1
Compiling strsim v0.11.1
Compiling powerfmt v0.2.1
Compiling windows-sys v0.61.2
Compiling toml_writer v1.2.0+spec-1.1.0
Compiling erased-serde v0.4.11
Compiling phf_generator v0.13.1
Compiling toml_parser v1.1.5+spec-1.1.0
Compiling aho-corasick v1.1.5
Compiling new_debug_unreachable v1.0.6
Compiling regex-syntax v0.8.11
Compiling fnv v1.0.7
Compiling web-time v1.1.0
Compiling phf_codegen v0.13.1
Compiling serde_json v1.0.154
Compiling alloc-no-stdlib v3.0.0
Compiling precomputed-hash v0.1.1
Compiling getrandom v0.4.3
Compiling quick-xml v0.42.0
Compiling dunce v1.0.5
Compiling alloc-stdlib v0.3.0
Compiling anyhow v1.0.104
Compiling base64 v0.23.1
Compiling quote v1.0.47
Compiling brotli-decompressor v6.3.0
Compiling string_cache v0.9.0
Compiling http v1.5.0
Compiling syn v3.0.9
Compiling syn v2.0.119
Compiling string_cache_codegen v0.6.1
Compiling semver v1.0.28
Compiling regex-automata v0.4.18
Compiling web_atoms v0.2.6
Compiling toml_datetime v1.1.3+spec-1.1.0
Compiling serde_spanned v1.1.3
Compiling bitflags v2.13.2
Compiling percent-encoding v2.3.2
Compiling dtoa v1.0.11
Compiling toml v1.2.1+spec-1.1.0
Compiling form_urlencoded v1.2.2
Compiling indexmap v1.9.3
Compiling dtoa-short v0.3.5
Compiling brotli v9.1.1
Compiling uuid v1.29.1
Compiling synstructure v0.14.0
Compiling darling_core v0.24.1
Compiling selectors v0.38.0
Compiling tendril v0.5.1
Compiling windows-result v0.4.1
Compiling windows-strings v0.5.1
Compiling ctor v1.0.13
Compiling camino v1.2.6
Compiling glob v0.3.4
Compiling regex v1.13.1
Compiling serde_derive_internals v0.29.1
Compiling vswhom-sys v0.1.3
Compiling phf_macros v0.13.1
Compiling zerofrom-derive v0.1.8
Compiling yoke-derive v0.8.4
Compiling zerovec-derive v0.11.7
Compiling serde_derive v1.0.229
Compiling displaydoc v0.2.7
Compiling thiserror-impl v2.0.21
Compiling phf v0.13.1
Compiling cssparser-macros v0.7.1
Compiling windows-implement v0.60.2
Compiling derive_more-impl v2.1.1
Compiling windows-interface v0.59.3
Compiling zerofrom v0.1.8
Compiling winapi-util v0.1.11
Compiling servo_arc v0.4.3
Compiling rustc-hash v2.1.3
Compiling cssparser v0.37.0
Compiling bit-vec v0.8.0
Compiling yoke v0.8.4
Compiling thiserror v2.0.21
Compiling schemars v0.8.22
Compiling hashbrown v0.12.3
Compiling windows-core v0.62.2
Compiling darling_macro v0.24.1
Compiling bit-set v0.8.0
Compiling zerovec v0.11.9
Compiling zerotrie v0.2.5
Compiling markup5ever v0.39.0
Compiling derive_more v2.1.1
Compiling darling v0.24.1
Compiling same-file v1.0.6
Compiling html5ever v0.39.0
Compiling schemars_derive v0.8.22
Compiling tinystr v0.8.5
Compiling potential_utf v0.1.6
Compiling serde_with_macros v3.24.0
Compiling jsonptr v0.7.1
Compiling cargo-platform v0.1.9
Compiling icu_locale_core v2.4.0
Compiling icu_collections v2.4.0
Compiling cfb v0.14.0
Compiling time v0.3.55
Compiling dyn-clone v1.0.20
Compiling tauri-utils v2.10.2
Compiling icu_provider v2.4.0
Compiling option-ext v0.2.0
Compiling foldhash v0.2.0
Compiling icu_normalizer v2.4.0
Compiling icu_properties v2.4.0
Compiling infer v0.22.0
Compiling dom_query v0.28.0
Compiling serde_with v3.24.0
Compiling serde-untagged v0.1.9
Compiling cargo_metadata v0.19.2
Compiling json-patch v4.2.0
Compiling walkdir v2.5.0
Compiling generic-array v0.14.7
Compiling windows-threading v0.2.1
Compiling simd-adler32 v0.3.10
Compiling idna_adapter v1.2.2
Compiling windows-future v0.3.2
Compiling idna v1.1.0
Compiling vswhom v0.1.0
Compiling windows-numerics v0.3.1
Compiling windows-collections v0.3.2
Compiling rustc_version v0.4.1
Compiling plist v1.10.3
Compiling winreg v0.56.0
Compiling url v2.5.8
Compiling time-macros v0.2.32
Compiling crc32fast v1.5.4
Compiling adler2 v2.0.1
Compiling windows v0.62.2
Compiling embed-resource v3.0.12
Compiling urlpattern v0.6.0
Compiling dirs-sys v0.5.0
Compiling webview2-com-sys v0.39.1
Compiling raw-window-handle v0.6.2
Compiling heck v0.5.0
Compiling dirs v7.0.0
Compiling tauri-winres v0.3.7
Compiling miniz_oxide v0.9.1
Compiling dpi v0.1.2
Compiling cargo_toml v1.0.1
Compiling cookie v0.18.2
Compiling static_vcruntime v3.0.0
Compiling crypto-common v0.1.7
Compiling block-buffer v0.10.4
Compiling flate2 v1.1.11
Compiling miniz_oxide v0.8.9
Compiling fdeflate v0.3.7
Compiling webview2-com-macros v0.8.1
Compiling windows-version v0.1.7
Compiling png v0.18.1
Compiling digest v0.10.7
Compiling crossbeam-channel v0.5.17
Compiling tauri-runtime v2.12.2
Compiling byteorder v1.5.0
Compiling tracing-core v0.1.36
Compiling wry v0.57.1
Compiling sha2 v0.10.9
Compiling muda v0.20.0
Compiling ico v0.6.0
Compiling tauri-runtime-wry v2.12.3
Compiling tracing v0.1.44
Compiling unicode-segmentation v1.13.3
Compiling keyboard-types v0.8.3
Compiling softbuffer v0.4.8
Compiling serialize-to-javascript-impl v0.1.2
Compiling tokio v1.54.1
Compiling window-vibrancy v0.8.1
Compiling serde_repr v0.1.21
Compiling mime v0.3.18
Compiling orchestrator-ipc v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\crates\orchestrator-ipc)
Compiling serialize-to-javascript v0.1.2
Compiling tauri-build v2.7.4
Compiling tauri-codegen v2.7.4
Compiling tauri v2.12.5
Compiling agent-orch-desktop v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\apps\desktop\src-tauri)
Compiling tauri-macros v2.7.4
Compiling tao v0.37.2
Compiling webview2-com v0.39.1
Finished `dev` profile [unoptimized + debuginfo] target(s) in 3m 42s
```

### tauri-tests

```text
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 137.48s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### tauri-clippy-clean

```text

```

### tauri-clippy

```text
Compiling siphasher v1.0.4
Compiling time-core v0.1.9
Compiling num-conv v0.2.2
Compiling thiserror v2.0.21
Compiling windows-sys v0.61.2
Compiling zerofrom v0.1.8
Checking serde v1.0.229
Compiling winnow v1.0.5
Compiling serde_json v1.0.154
Compiling getrandom v0.4.3
Compiling phf_shared v0.13.1
Compiling yoke v0.8.4
Compiling toml_datetime v1.1.3+spec-1.1.0
Compiling serde_spanned v1.1.3
Compiling toml_writer v1.2.0+spec-1.1.0
Compiling form_urlencoded v1.2.2
Compiling phf_generator v0.13.1
Compiling zerovec v0.11.9
Compiling zerotrie v0.2.5
Compiling toml_parser v1.1.5+spec-1.1.0
Compiling string_cache v0.9.0
Compiling uuid v1.29.1
Compiling phf_codegen v0.13.1
Compiling phf_macros v0.13.1
Compiling string_cache_codegen v0.6.1
Compiling aho-corasick v1.1.5
Checking windows-result v0.4.1
Compiling selectors v0.38.0
Compiling tinystr v0.8.5
Compiling potential_utf v0.1.6
Compiling toml v1.2.1+spec-1.1.0
Compiling icu_locale_core v2.4.0
Compiling icu_collections v2.4.0
Checking windows-strings v0.5.1
Compiling alloc-no-stdlib v3.0.0
Compiling web_atoms v0.2.6
Compiling regex-syntax v0.8.11
Compiling alloc-stdlib v0.3.0
Checking windows-core v0.62.2
Compiling phf v0.13.1
Compiling cssparser v0.37.0
Compiling typeid v1.0.3
Checking icu_provider v2.4.0
Compiling deranged v0.5.9
Compiling powerfmt v0.2.1
Compiling icu_properties v2.4.0
Compiling icu_normalizer v2.4.0
Compiling fnv v1.0.7
Compiling web-time v1.1.0
Compiling winapi-util v0.1.11
Compiling regex-automata v0.4.18
Compiling cfb v0.14.0
Compiling erased-serde v0.4.11
Compiling same-file v1.0.6
Compiling brotli-decompressor v6.3.0
Compiling time v0.3.55
Compiling jsonptr v0.7.1
Compiling libc v0.2.191
Compiling quick-xml v0.42.0
Compiling base64 v0.23.1
Compiling markup5ever v0.39.0
Compiling dunce v1.0.5
Compiling http v1.5.0
Compiling idna_adapter v1.2.2
Compiling idna v1.1.0
Compiling html5ever v0.39.0
Compiling json-patch v4.2.0
Compiling vswhom-sys v0.1.3
Compiling brotli v9.1.1
Compiling cargo_metadata v0.19.2
Compiling url v2.5.8
Compiling regex v1.13.1
Compiling infer v0.22.0
Compiling dom_query v0.28.0
Compiling serde-untagged v0.1.9
Compiling walkdir v2.5.0
Compiling serde_with v3.24.0
Compiling plist v1.10.3
Compiling schemars v0.8.22
Compiling urlpattern v0.6.0
Compiling anyhow v1.0.104
Checking windows-threading v0.2.1
Compiling ctor v1.0.13
Compiling glob v0.3.4
Checking windows-future v0.3.2
Compiling vswhom v0.1.0
Compiling winreg v0.56.0
Checking windows-numerics v0.3.1
Checking windows-collections v0.3.2
Compiling time-macros v0.2.32
Compiling option-ext v0.2.0
Compiling dirs-sys v0.5.0
Compiling embed-resource v3.0.12
Compiling tauri-utils v2.10.2
Checking windows v0.62.2
Compiling webview2-com-sys v0.39.1
Checking raw-window-handle v0.6.2
Compiling tauri-winres v0.3.7
Compiling dirs v7.0.0
Compiling crc32fast v1.5.4
Compiling cargo_toml v1.0.1
Checking dpi v0.1.2
Compiling flate2 v1.1.11
Checking bitflags v2.13.2
Checking windows-version v0.1.7
Compiling png v0.18.1
Checking crossbeam-channel v0.5.17
Checking tracing-core v0.1.36
Checking cookie v0.18.2
Checking semver v1.0.28
Checking tracing v0.1.44
Compiling ico v0.6.0
Compiling sha2 v0.10.9
Checking unicode-segmentation v1.13.3
Checking softbuffer v0.4.8
Checking keyboard-types v0.8.3
Checking serialize-to-javascript v0.1.2
Checking window-vibrancy v0.8.1
Checking tokio v1.54.1
Checking mime v0.3.18
Checking heck v0.5.0
Checking orchestrator-ipc v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\crates\orchestrator-ipc)
Checking tempfile v3.27.0
Checking muda v0.20.0
Compiling tauri-build v2.7.4
Compiling tauri-codegen v2.7.4
Compiling tauri v2.12.5
Compiling agent-orch-desktop v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r3-20261110T045110Z-db4b5730\source\apps\desktop\src-tauri)
Compiling tauri-macros v2.7.4
Checking tao v0.37.2
Checking webview2-com v0.39.1
Checking wry v0.57.1
Checking tauri-runtime v2.12.2
Checking tauri-runtime-wry v2.12.3
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 13s
```

### desktop-package-win

```text
✓ built in 211ms
```

### unproven-second-user

```text
status: UNPROVEN
Denial of the named pipe to a second local user was not exercised. No second account is used.
```

### unproven-remote-client

```text
status: UNPROVEN
Rejection of a genuinely remote client was not exercised. No remote host or SMB loopback is used.
```

### unproven-slow-reader-64kib

```text
status: UNPROVEN
A daemon-to-client write above 64 KiB to a slow reader was not exercised.
```

### unproven-tauri-webview-click-path

```text
status: UNPROVEN
NOT RUN. The webview and the React click path were not driven: no window, no webview, no daemon_open, daemon_send or daemon_close call as the webview makes it, and nobody clicked through the approval flow. The bridge::real_tests drive the Rust Bridge directly, with no window.
```

### unproven-e2e-windows

```text
status: UNPROVEN
NOT RUN. The desktop e2e suite (npm run test:e2e) uses a Unix domain socket and cannot run on Windows. No Windows e2e number exists.
```

### unproven-gui-install-launch

```text
status: UNPROVEN
NOT RUN. The NSIS installer is built by desktop-package-win but never installed or launched by this harness. No window shows Connected and no sidecar daemon is started by the app.
```

### unproven-gui-task-survives-close

```text
status: UNPROVEN
NOT RUN. A real daemon-owned task surviving a window close and reopen was not driven. The headless daemon-lifecycle gate covers the daemon side with a fake agent and the UI-side relay, not the window.
```

### unproven-gui-event-reconnect

```text
status: UNPROVEN
NOT RUN. The window event stream reconnecting and resuming at the saved cursor and epoch, and drafts surviving a reopen, were not driven.
```

### unproven-stale-token-relaunch-timing

```text
status: UNPROVEN
UNMEASURED. About 0.3 s per relay attempt, about 0.6 s before relaunch, then 200 ms retries for 15 s, and 10 s relay handshake and write timeouts, are derived from code. Nothing was timed on Windows.
```

### unproven-daemon-idle-behaviour

```text
status: UNPROVEN
NOT RUN. The 60 s true-idle exit, a live subscription keeping the daemon alive and the idle clock restarting were not observed on the app. Only the idle_subscription_tests in ipc-tests cover the logic.
```

### unproven-daemon-kill-and-sidecar-failure

```text
status: UNPROVEN
NOT RUN. Killing the daemon pid and a sidecar that fails to start were not exercised. Both stop or break a process, which this runbook never does.
```

### tauri-native-bridge-host-level

```text
status: UNPROVEN
UNPROVEN: the host-level bridge is provable only when coordinator-review-tests and tauri-tests both PASS natively in this run. Gate statuses: coordinator-review-tests FAIL, tauri-tests PASS.
```

## Coverage gaps

- Unix-only tests did not run on Windows: IPC integration_tests.rs, lifecycle_tests.rs and
  timeout_tests.rs, the IPC wrong-token relay test, the whole daemon executor_tests.rs module,
  tests/daemon.rs, three daemon cancellation unit tests, the daemon symlink test, and one src-tauri
  bridge test. Their macOS results are separate and are not Windows
  evidence. The scheduler and executor have no native Windows test in this run.
- NOT RUN: the desktop e2e suite (Unix domain socket only). No Windows e2e number exists.
- UNPROVEN or NOT RUN, whatever the overall result: denial of a second, different local user;
  rejection of a genuinely remote client; a single daemon-to-client write above 64 KiB to a slow
  reader; the webview and click path of the Tauri host (the host-level bridge is covered only by the
  derived row tauri-native-bridge-host-level); installing and launching the app; a daemon-owned task
  surviving a window close and reopen; the window event reconnect; the stale-token relaunch timing (UNMEASURED);
  the 60 s daemon idle behaviour on the app; and killing the daemon or breaking the sidecar.
- The daemon built by daemon-build was never started by the harness. Server identity verification by
  the client, the data-directory ACL, and the token owner are not validated on Windows. The optional
  manual GUI checklist (sections 5.5 and 5.6 of the in-repo runbook) is not part of this result unless
  manual-lifecycle.txt records it.
- The Vitest counts (15 files, 287 tests) are Mac-measured. They are believed platform-neutral
  (jsdom with an in-memory fake daemon) and are unmeasured on Windows.
- No rustfmt gate, no workspace-wide test run, and no store, engine, git or CLI test run ran.