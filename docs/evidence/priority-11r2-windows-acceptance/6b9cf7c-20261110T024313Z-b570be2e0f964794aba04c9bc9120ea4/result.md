# Priority 11 round 2 Windows acceptance of 6b9cf7cdee06dd439492b15d8d9f6c7138059b2b (M5-05 Tauri desktop host, IPC relay and native lifecycle proof, with the M5-05r and M5-05r2 remediation)

This run covers the Priority 11 round 2 candidate: the IPC relay and its timeouts, the live-subscription idle
logic and the fail-fast stale-token connect, the daemon built natively on Windows with its headless
lifecycle proof, the Tauri host built, unit-tested and linted natively, the desktop install,
type-check, build and unit tests, and whether the NSIS installer builds (npm run package:win). The coordinator decides
acceptance. The expected counts are derived from the cfg and test attributes at the pinned source for
Windows (IPC 45, daemon 42, windows_transport 13, src-tauri 6, lifecycle prints lifecycle: PASS with
no libtest count) and from the Mac Vitest run (15 files, 287 tests). They are derived and unmeasured
for this commit. They are not the Mac counts (IPC 45 with a different mix of tests, daemon 59,
src-tauri 7), and they are not the Priority 10r2 native counts (a different tree: IPC 38, Vitest 14
files and 268 tests). Counts are reported as measured, not matched. No macOS or review result is
Windows evidence. This run is not a native probe, not a second-user, genuinely remote-client,
slow-reader, running-bridge or Windows e2e claim, not a GUI install, launch, close and reopen,
reconnect, idle or relaunch-timing claim, and not an M6 threat-review claim.

- Run ID: 20261110T024313Z-b570be2e0f964794aba04c9bc9120ea4
- UTC date: 2026-11-10T03:04:21.9428686Z
- Source branch: orch/priority-11r2-m5-05-6b9cf7c
- Branch tip at clone time: 6b9cf7cdee06dd439492b15d8d9f6c7138059b2b
- Source commit: 6b9cf7cdee06dd439492b15d8d9f6c7138059b2b
- Source tree and only parent (verified): source-tree.stdout.txt, source-parent.stdout.txt
- HEAD matched the source commit before the checks and after them: yes
  (source-head-before.stdout.txt, source-head-after.stdout.txt)
- Git top-level path form (plain drive-letter form required): source-toplevel.stdout.txt
- Baseline ancestry (e555c2c9), commit count, the thirteen pinned commits, the linear range, and the
  changed-file set (51 files): base-ancestry, range-count, range-merges, range-commits, range-files
  (consistency gates, not patch-equivalence proof)
- Runbook identity (operator-declared; exact bytes pinned by blob id, size, and SHA-256; not proof that
  the pasted commands match): runbook-identity.txt
- Overall result: PASS UNPROVEN, not covered by this result: unproven-second-user, unproven-remote-client, unproven-slow-reader-64kib, unproven-tauri-native-bridge, unproven-e2e-windows, unproven-gui-install-launch, unproven-gui-task-survives-close, unproven-gui-event-reconnect, unproven-stale-token-relaunch-timing, unproven-daemon-idle-behaviour, unproven-daemon-kill-and-sidecar-failure.
- UNPROVEN rows (never covered by an overall PASS): unproven-second-user, unproven-remote-client, unproven-slow-reader-64kib, unproven-tauri-native-bridge, unproven-e2e-windows, unproven-gui-install-launch, unproven-gui-task-survives-close, unproven-gui-event-reconnect, unproven-stale-token-relaunch-timing, unproven-daemon-idle-behaviour, unproven-daemon-kill-and-sidecar-failure
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
  files for daemon-tests): the diagnostics block that the test prints and any surviving daemon.log.
- Harness checks (not gates): gitattributes-before.summary.txt and gitattributes-after.summary.txt
  (LF pins), source-status-after.stdout.txt and source-status-after-plain.stdout.txt (git status),
  source-diff-after-stat.stdout.txt (git diff --stat), source-diff-after.stdout.txt and
  source-diff-after-ignore-cr.stdout.txt (git diff), diff-check.stdout.txt. gitattributes before:
  PASS. gitattributes after: PASS. git status after the run
  (porcelain lines): 0. git diff --stat after the run (lines): 0.
- UNPROVEN and NOT RUN rows: one unproven-*.txt file each. No native probe ran. No window was driven
  and no installer was installed. No daemon was started except by the lifecycle tests themselves.
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
| daemon-build | PASS | 0 | False | 118.778 |  |  |  |  |  |
| ipc-tests | PASS | 0 | False | 29.759 | 45,0 | 45 | 0 | 0 |  |
| daemon-tests | PASS | 0 | False | 57.999 | 42,0,0 | 42 | 0 | 0 |  |
| daemon-lifecycle | PASS | 0 | False | 12.299 |  |  |  |  |  |
| windows-transport-tests | PASS | 0 | False | 6.328 | 13 | 13 | 0 | 0 |  |
| clippy-scoped | PASS | 0 | False | 62.706 |  |  |  |  | 2 baseline diagnostic(s) and 0 NEW diagnostic(s) recorded in clippy-scoped.diagnostics.txt. The only baseline is the supervisor.rs single_match pair in orchestrator-runtime (a dependency of the daemon); the Tauri host has none. NEW diagnostics do not change the verdict; report each one verbatim. |
| desktop-npm-ci | PASS | 0 | False | 38.059 |  |  |  |  |  |
| desktop-tsc | PASS | 0 | False | 8.24 |  |  |  |  |  |
| desktop-build | PASS | 0 | False | 3.617 |  |  |  |  |  |
| desktop-vitest | PASS | 0 | False | 26.49 |  | 287 | 0 |  |  |
| tauri-build | PASS | 0 | False | 227.897 |  |  |  |  |  |
| tauri-tests | PASS | 0 | False | 120.038 | 6,0,0 | 6 | 0 | 0 |  |
| tauri-clippy-clean | PASS | 0 | False | 1.148 |  |  |  |  |  |
| tauri-clippy | PASS | 0 | False | 122.506 |  |  |  |  | 0 baseline diagnostic(s) and 0 NEW diagnostic(s) recorded in tauri-clippy.diagnostics.txt. The only baseline is the supervisor.rs single_match pair in orchestrator-runtime (a dependency of the daemon); the Tauri host has none. NEW diagnostics do not change the verdict; report each one verbatim. |
| desktop-package-win | PASS | 0 | False | 399.383 |  |  |  |  |  |
| unproven-second-user | UNPROVEN |  | False | 0 |  |  |  |  | Denial of the named pipe to a second local user was not exercised. No second account is used. See unproven-second-user.txt. |
| unproven-remote-client | UNPROVEN |  | False | 0 |  |  |  |  | Rejection of a genuinely remote client was not exercised. No remote host or SMB loopback is used. See unproven-remote-client.txt. |
| unproven-slow-reader-64kib | UNPROVEN |  | False | 0 |  |  |  |  | A daemon-to-client write above 64 KiB to a slow reader was not exercised. See unproven-slow-reader-64kib.txt. |
| unproven-tauri-native-bridge | UNPROVEN |  | False | 0 |  |  |  |  | The Tauri host was built and its 6 unit tests ran, but the running bridge was not exercised: no window, no webview, no daemon_open, daemon_send or daemon_close call and no live bridge to a daemon. See unproven-tauri-native-bridge.txt. |
| unproven-e2e-windows | UNPROVEN |  | False | 0 |  |  |  |  | NOT RUN. The desktop e2e suite (npm run test:e2e) uses a Unix domain socket and cannot run on Windows. No Windows e2e number exists. See unproven-e2e-windows.txt. |
| unproven-gui-install-launch | UNPROVEN |  | False | 0 |  |  |  |  | NOT RUN. The NSIS installer is built by desktop-package-win but never installed or launched by this harness. No window shows Connected and no sidecar daemon is started by the app. See unproven-gui-install-launch.txt. |
| unproven-gui-task-survives-close | UNPROVEN |  | False | 0 |  |  |  |  | NOT RUN. A real daemon-owned task surviving a window close and reopen was not driven. The headless daemon-lifecycle gate covers the daemon side with a fake agent and the UI-side relay, not the window. See unproven-gui-task-survives-close.txt. |
| unproven-gui-event-reconnect | UNPROVEN |  | False | 0 |  |  |  |  | NOT RUN. The window event stream reconnecting and resuming at the saved cursor and epoch, and drafts surviving a reopen, were not driven. See unproven-gui-event-reconnect.txt. |
| unproven-stale-token-relaunch-timing | UNPROVEN |  | False | 0 |  |  |  |  | UNMEASURED. About 0.3 s per relay attempt, about 0.6 s before relaunch, then 200 ms retries for 15 s, and 10 s relay handshake and write timeouts, are derived from code. Nothing was timed on Windows. See unproven-stale-token-relaunch-timing.txt. |
| unproven-daemon-idle-behaviour | UNPROVEN |  | False | 0 |  |  |  |  | NOT RUN. The 60 s true-idle exit, a live subscription keeping the daemon alive and the idle clock restarting were not observed on the app. Only the idle_subscription_tests in ipc-tests cover the logic. See unproven-daemon-idle-behaviour.txt. |
| unproven-daemon-kill-and-sidecar-failure | UNPROVEN |  | False | 0 |  |  |  |  | NOT RUN. Killing the daemon pid and a sidecar that fails to start were not exercised. Both stop or break a process, which this runbook never does. See unproven-daemon-kill-and-sidecar-failure.txt. |

## Result lines and observations

### daemon-build

```text
Compiling proc-macro2 v1.0.107
Compiling quote v1.0.47
Compiling unicode-ident v1.0.27
Compiling windows-link v0.2.1
Compiling stable_deref_trait v1.2.1
Compiling serde_core v1.0.229
Compiling pin-project-lite v0.2.17
Compiling serde v1.0.229
Compiling writeable v0.6.4
Compiling cfg-if v1.0.5
Compiling futures-core v0.3.34
Compiling smallvec v1.16.5
Compiling utf_types v1.0.0
Compiling litemap v0.8.3
Compiling version_check v0.9.5
Compiling memchr v2.8.3
Compiling icu_properties_data v2.4.0
Compiling icu_normalizer_data v2.4.0
Compiling bytes v1.12.1
Compiling shlex v2.0.1
Compiling utf8_iter v1.1.0
Compiling find-msvc-tools v0.1.15
Compiling generic-array v0.14.9
Compiling scopeguard v1.2.0
Compiling typenum v1.20.1
Compiling windows-sys v0.61.2
Compiling futures-sink v0.3.34
Compiling cc v1.11.1
Compiling parking_lot_core v0.9.12
Compiling vcpkg v0.2.15
Compiling lock_api v0.4.14
Compiling pkg-config v0.3.34
Compiling equivalent v1.0.3
Compiling zmij v1.0.23
Compiling crossbeam-utils v0.8.23
Compiling once_cell v1.21.4
Compiling serde_json v1.0.154
Compiling syn v3.0.9
Compiling syn v2.0.119
Compiling block-buffer v0.10.4
Compiling crypto-common v0.1.6
Compiling percent-encoding v2.3.2
Compiling futures-io v0.3.34
Compiling foldhash v0.2.0
Compiling slab v0.4.12
Compiling autocfg v1.5.1
Compiling futures-task v0.3.34
Compiling allocator-api2 v0.2.21
Compiling thiserror v2.0.21
Compiling form_urlencoded v1.2.2
Compiling digest v0.10.7
Compiling num-traits v0.2.19
Compiling futures-util v0.3.34
Compiling parking_lot v0.12.5
Compiling hashbrown v0.16.1
Compiling tracing-core v0.1.36
Compiling parking v2.2.1
Compiling crc-catalog v2.5.0
Compiling cpufeatures v0.2.17
Compiling itoa v1.0.18
Compiling hashbrown v0.17.1
Compiling log v0.4.35
Compiling synstructure v0.14.0
Compiling hashlink v0.11.1
Compiling sha2 v0.10.9
Compiling crc v3.4.0
Compiling event-listener v5.4.2
Compiling futures-intrusive v0.5.0
Compiling crossbeam-queue v0.3.14
Compiling mio v1.2.4
Compiling indexmap v2.14.2
Compiling socket2 v0.6.5
Compiling base64 v0.22.1
Compiling libsqlite3-sys v0.37.0
Compiling tracing-attributes v0.1.31
Compiling zerofrom-derive v0.1.8
Compiling yoke-derive v0.8.4
Compiling zerovec-derive v0.11.7
Compiling displaydoc v0.2.7
Compiling serde_derive v1.0.229
Compiling thiserror-impl v2.0.21
Compiling tokio-macros v2.7.2
Compiling tokio v1.54.1
Compiling spin v0.9.9
Compiling futures-executor v0.3.34
Compiling tracing v0.1.44
Compiling flume v0.12.0
Compiling futures-channel v0.3.34
Compiling zerofrom v0.1.8
Compiling atoi v2.0.0
Compiling heck v0.5.0
Compiling hex v0.4.3
Compiling dotenvy v0.15.7
Compiling getrandom v0.3.4
Compiling orchestrator-git v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\crates\orchestrator-git)
Compiling libc v0.2.191
Compiling async-trait v0.1.89
Compiling subtle v2.6.1
Compiling yoke v0.8.4
Compiling zerovec v0.11.9
Compiling zerotrie v0.2.5
Compiling either v1.19.0
Compiling orchestrator-core v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\crates\orchestrator-core)
Compiling orchestrator-ipc v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\crates\orchestrator-ipc)
Compiling tinystr v0.8.5
Compiling potential_utf v0.1.6
Compiling icu_collections v2.4.0
Compiling uuid v1.18.1
Compiling icu_locale_core v2.4.0
Compiling tokio-stream v0.1.19
Compiling orchestrator-runtime v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\crates\orchestrator-runtime)
Compiling icu_provider v2.4.0
Compiling orchestrator-agents v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\crates\orchestrator-agents)
Compiling icu_properties v2.4.0
Compiling icu_normalizer v2.4.0
Compiling idna_adapter v1.2.2
Compiling idna v1.1.0
Compiling url v2.5.8
Compiling sqlx-core v0.9.0
Compiling sqlx-sqlite v0.9.0
Compiling sqlx-macros-core v0.9.0
Compiling sqlx-macros v0.9.0
Compiling sqlx v0.9.0
Compiling orchestrator-store v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\crates\orchestrator-store)
Compiling orchestrator-engine v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\crates\orchestrator-engine)
Compiling orchestrator-daemon v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\crates\orchestrator-daemon)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 58s
```

### ipc-tests

```text
running 45 tests
test result: ok. 45 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.16s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### daemon-tests

```text
running 42 tests
test result: ok. 42 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 28.49s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
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

### windows-transport-tests

```text
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 32 filtered out; finished in 5.09s
```

### clippy-scoped

```text
Compiling zerofrom v0.1.8
Compiling stable_deref_trait v1.2.1
Checking windows-link v0.2.1
Checking cfg-if v1.0.5
Checking serde_core v1.0.229
Compiling smallvec v1.16.5
Compiling utf_types v1.0.0
Compiling writeable v0.6.4
Compiling litemap v0.8.3
Compiling yoke v0.8.4
Checking windows-sys v0.61.2
Compiling futures-core v0.3.34
Checking memchr v2.8.3
Checking pin-project-lite v0.2.17
Compiling utf8_iter v1.1.0
Compiling zerovec v0.11.9
Compiling zerotrie v0.2.5
Compiling typenum v1.20.1
Compiling scopeguard v1.2.0
Compiling icu_properties_data v2.4.0
Compiling lock_api v0.4.14
Compiling icu_normalizer_data v2.4.0
Compiling futures-sink v0.3.34
Compiling tinystr v0.8.5
Compiling potential_utf v0.1.6
Checking thiserror v2.0.21
Compiling icu_locale_core v2.4.0
Compiling icu_collections v2.4.0
Compiling generic-array v0.14.9
Compiling equivalent v1.0.3
Checking serde v1.0.229
Compiling parking_lot_core v0.9.12
Compiling crypto-common v0.1.6
Compiling block-buffer v0.10.4
Compiling icu_provider v2.4.0
Checking bytes v1.12.1
Compiling futures-io v0.3.34
Checking once_cell v1.21.4
Compiling mio v1.2.4
Compiling icu_properties v2.4.0
Compiling icu_normalizer v2.4.0
Compiling socket2 v0.6.5
Compiling slab v0.4.12
Compiling foldhash v0.2.0
Compiling futures-task v0.3.34
Compiling allocator-api2 v0.2.21
Compiling percent-encoding v2.3.2
Compiling tracing-core v0.1.36
Compiling form_urlencoded v1.2.2
Compiling tokio v1.54.1
Compiling futures-util v0.3.34
Compiling hashbrown v0.16.1
Compiling digest v0.10.7
Compiling parking_lot v0.12.5
Compiling crossbeam-utils v0.8.23
Checking zmij v1.0.23
Compiling idna_adapter v1.2.2
Compiling log v0.4.35
Compiling cpufeatures v0.2.17
Compiling idna v1.1.0
Compiling hashbrown v0.17.1
Checking itoa v1.0.18
Compiling crc-catalog v2.5.0
Compiling parking v2.2.1
Compiling sha2 v0.10.9
Compiling tracing v0.1.44
Compiling crossbeam-queue v0.3.14
Checking serde_json v1.0.154
Compiling url v2.5.8
Compiling event-listener v5.4.2
Compiling crc v3.4.0
Compiling hashlink v0.11.1
Compiling indexmap v2.14.2
Compiling futures-intrusive v0.5.0
Compiling either v1.19.0
Compiling spin v0.9.9
Compiling num-traits v0.2.19
Compiling base64 v0.22.1
Compiling flume v0.12.0
Compiling futures-executor v0.3.34
Compiling atoi v2.0.0
Compiling futures-channel v0.3.34
Compiling tokio-stream v0.1.19
Compiling sqlx-core v0.9.0
Checking libsqlite3-sys v0.37.0
Checking orchestrator-core v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\crates\orchestrator-core)
Checking orchestrator-git v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\crates\orchestrator-git)
Checking getrandom v0.3.4
Checking subtle v2.6.1
Checking getrandom v0.4.3
Checking fastrand v2.5.0
Checking uuid v1.18.1
Checking orchestrator-ipc v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\crates\orchestrator-ipc)
Checking libc v0.2.191
Checking tempfile v3.27.0
Checking orchestrator-runtime v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\crates\orchestrator-runtime)
warning: you seem to be trying to use `match` for destructuring a single pattern. Consider using `if let`
warning: you seem to be trying to use `match` for destructuring a single pattern. Consider using `if let`
warning: `orchestrator-runtime` (lib) generated 2 warnings (run `cargo clippy --fix --lib -p orchestrator-runtime -- ` to apply 2 suggestions)
Checking orchestrator-agents v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\crates\orchestrator-agents)
Compiling sqlx-sqlite v0.9.0
Compiling sqlx-macros-core v0.9.0
Compiling sqlx-macros v0.9.0
Checking sqlx v0.9.0
Checking orchestrator-store v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\crates\orchestrator-store)
Checking orchestrator-engine v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\crates\orchestrator-engine)
Checking orchestrator-daemon v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\crates\orchestrator-daemon)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 00s
```

### desktop-npm-ci

```text

```

### desktop-tsc

```text

```

### desktop-build

```text
✓ built in 437ms
```

### desktop-vitest

```text
Test Files  15 passed (15)
Tests  287 passed (287)
Duration  19.64s (environment 62%, setup 17%, tests 14%, import 3%, transform 2%)
```

### tauri-build

```text
Compiling proc-macro2 v1.0.107
Compiling serde_core v1.0.229
Compiling siphasher v1.0.4
Compiling fastrand v2.5.0
Compiling stable_deref_trait v1.2.1
Compiling serde v1.0.229
Compiling num-conv v0.2.2
Compiling time-core v0.1.9
Compiling ident_case v1.0.1
Compiling strsim v0.11.1
Compiling winnow v1.0.5
Compiling deranged v0.5.9
Compiling typeid v1.0.3
Compiling powerfmt v0.2.1
Compiling phf_shared v0.13.1
Compiling windows-sys v0.61.2
Compiling erased-serde v0.4.11
Compiling toml_writer v1.2.0+spec-1.1.0
Compiling aho-corasick v1.1.5
Compiling phf_generator v0.13.1
Compiling toml_parser v1.1.5+spec-1.1.0
Compiling new_debug_unreachable v1.0.6
Compiling regex-syntax v0.8.11
Compiling web-time v1.1.0
Compiling fnv v1.0.7
Compiling phf_codegen v0.13.1
Compiling alloc-no-stdlib v3.0.0
Compiling serde_json v1.0.154
Compiling precomputed-hash v0.1.1
Compiling getrandom v0.4.3
Compiling alloc-stdlib v0.3.0
Compiling quick-xml v0.42.0
Compiling anyhow v1.0.104
Compiling base64 v0.23.1
Compiling quote v1.0.47
Compiling dunce v1.0.5
Compiling brotli-decompressor v6.3.0
Compiling syn v3.0.9
Compiling syn v2.0.119
Compiling string_cache_codegen v0.6.1
Compiling string_cache v0.9.0
Compiling regex-automata v0.4.18
Compiling semver v1.0.28
Compiling serde_spanned v1.1.3
Compiling toml_datetime v1.1.3+spec-1.1.0
Compiling web_atoms v0.2.6
Compiling http v1.5.0
Compiling dtoa v1.0.11
Compiling percent-encoding v2.3.2
Compiling bitflags v2.13.2
Compiling indexmap v1.9.3
Compiling form_urlencoded v1.2.2
Compiling dtoa-short v0.3.5
Compiling toml v1.2.1+spec-1.1.0
Compiling uuid v1.29.1
Compiling brotli v9.1.1
Compiling selectors v0.38.0
Compiling tendril v0.5.1
Compiling windows-result v0.4.1
Compiling windows-strings v0.5.1
Compiling synstructure v0.14.0
Compiling darling_core v0.24.1
Compiling regex v1.13.1
Compiling ctor v1.0.13
Compiling camino v1.2.6
Compiling glob v0.3.4
Compiling serde_derive_internals v0.29.1
Compiling vswhom-sys v0.1.3
Compiling winapi-util v0.1.11
Compiling servo_arc v0.4.3
Compiling phf_macros v0.13.1
Compiling windows-interface v0.59.3
Compiling zerofrom-derive v0.1.8
Compiling yoke-derive v0.8.4
Compiling zerovec-derive v0.11.7
Compiling displaydoc v0.2.7
Compiling serde_derive v1.0.229
Compiling thiserror-impl v2.0.21
Compiling cssparser-macros v0.7.1
Compiling derive_more-impl v2.1.1
Compiling phf v0.13.1
Compiling windows-implement v0.60.2
Compiling rustc-hash v2.1.3
Compiling cssparser v0.37.0
Compiling hashbrown v0.12.3
Compiling bit-vec v0.8.0
Compiling schemars v0.8.22
Compiling zerofrom v0.1.8
Compiling markup5ever v0.39.0
Compiling windows-core v0.62.2
Compiling bit-set v0.8.0
Compiling thiserror v2.0.21
Compiling yoke v0.8.4
Compiling derive_more v2.1.1
Compiling html5ever v0.39.0
Compiling schemars_derive v0.8.22
Compiling zerovec v0.11.9
Compiling zerotrie v0.2.5
Compiling darling_macro v0.24.1
Compiling same-file v1.0.6
Compiling cfb v0.14.0
Compiling darling v0.24.1
Compiling time v0.3.55
Compiling tinystr v0.8.5
Compiling potential_utf v0.1.6
Compiling serde_with_macros v3.24.0
Compiling jsonptr v0.7.1
Compiling icu_collections v2.4.0
Compiling icu_locale_core v2.4.0
Compiling cargo-platform v0.1.9
Compiling dyn-clone v1.0.20
Compiling foldhash v0.2.0
Compiling option-ext v0.2.0
Compiling tauri-utils v2.10.2
Compiling dom_query v0.28.0
Compiling icu_provider v2.4.0
Compiling json-patch v4.2.0
Compiling cargo_metadata v0.19.2
Compiling infer v0.22.0
Compiling icu_normalizer v2.4.0
Compiling icu_properties v2.4.0
Compiling serde_with v3.24.0
Compiling serde-untagged v0.1.9
Compiling walkdir v2.5.0
Compiling generic-array v0.14.7
Compiling windows-threading v0.2.1
Compiling plist v1.10.3
Compiling simd-adler32 v0.3.10
Compiling windows-future v0.3.2
Compiling vswhom v0.1.0
Compiling windows-collections v0.3.2
Compiling windows-numerics v0.3.1
Compiling rustc_version v0.4.1
Compiling winreg v0.56.0
Compiling time-macros v0.2.32
Compiling adler2 v2.0.1
Compiling crc32fast v1.5.4
Compiling embed-resource v3.0.12
Compiling windows v0.62.2
Compiling dirs-sys v0.5.0
Compiling webview2-com-sys v0.39.1
Compiling heck v0.5.0
Compiling raw-window-handle v0.6.2
Compiling tauri-winres v0.3.7
Compiling dirs v7.0.0
Compiling miniz_oxide v0.9.1
Compiling cargo_toml v1.0.1
Compiling dpi v0.1.2
Compiling cookie v0.18.2
Compiling static_vcruntime v3.0.0
Compiling flate2 v1.1.11
Compiling crypto-common v0.1.7
Compiling block-buffer v0.10.4
Compiling miniz_oxide v0.8.9
Compiling fdeflate v0.3.7
Compiling idna_adapter v1.2.2
Compiling webview2-com-macros v0.8.1
Compiling idna v1.1.0
Compiling windows-version v0.1.7
Compiling url v2.5.8
Compiling png v0.18.1
Compiling urlpattern v0.6.0
Compiling digest v0.10.7
Compiling crossbeam-channel v0.5.17
Compiling tauri-runtime v2.12.2
Compiling byteorder v1.5.0
Compiling tracing-core v0.1.36
Compiling wry v0.57.1
Compiling ico v0.6.0
Compiling sha2 v0.10.9
Compiling tauri-runtime-wry v2.12.3
Compiling muda v0.20.0
Compiling tracing v0.1.44
Compiling unicode-segmentation v1.13.3
Compiling keyboard-types v0.8.3
Compiling serialize-to-javascript-impl v0.1.2
Compiling tokio v1.54.1
Compiling softbuffer v0.4.8
Compiling window-vibrancy v0.8.1
Compiling serde_repr v0.1.21
Compiling mime v0.3.18
Compiling orchestrator-ipc v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\crates\orchestrator-ipc)
Compiling serialize-to-javascript v0.1.2
Compiling tauri-build v2.7.4
Compiling tauri-codegen v2.7.4
Compiling tauri v2.12.5
Compiling tauri-macros v2.7.4
Compiling agent-orch-desktop v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\apps\desktop\src-tauri)
Compiling tao v0.37.2
Compiling webview2-com v0.39.1
Finished `dev` profile [unoptimized + debuginfo] target(s) in 3m 47s
```

### tauri-tests

```text
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.07s
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
Compiling num-conv v0.2.2
Compiling time-core v0.1.9
Compiling thiserror v2.0.21
Compiling windows-sys v0.61.2
Compiling zerofrom v0.1.8
Checking serde v1.0.229
Compiling winnow v1.0.5
Compiling serde_json v1.0.154
Compiling getrandom v0.4.3
Compiling yoke v0.8.4
Compiling phf_shared v0.13.1
Compiling serde_spanned v1.1.3
Compiling toml_datetime v1.1.3+spec-1.1.0
Compiling toml_writer v1.2.0+spec-1.1.0
Compiling form_urlencoded v1.2.2
Compiling zerovec v0.11.9
Compiling zerotrie v0.2.5
Compiling phf_generator v0.13.1
Compiling toml_parser v1.1.5+spec-1.1.0
Compiling string_cache v0.9.0
Compiling uuid v1.29.1
Compiling aho-corasick v1.1.5
Compiling phf_codegen v0.13.1
Compiling phf_macros v0.13.1
Compiling tinystr v0.8.5
Compiling potential_utf v0.1.6
Compiling string_cache_codegen v0.6.1
Compiling selectors v0.38.0
Compiling icu_locale_core v2.4.0
Compiling icu_collections v2.4.0
Compiling toml v1.2.1+spec-1.1.0
Checking windows-strings v0.5.1
Checking windows-result v0.4.1
Compiling regex-syntax v0.8.11
Compiling web_atoms v0.2.6
Compiling alloc-no-stdlib v3.0.0
Compiling phf v0.13.1
Checking windows-core v0.62.2
Compiling icu_provider v2.4.0
Compiling cssparser v0.37.0
Compiling alloc-stdlib v0.3.0
Compiling typeid v1.0.3
Compiling icu_properties v2.4.0
Compiling icu_normalizer v2.4.0
Compiling winapi-util v0.1.11
Compiling deranged v0.5.9
Compiling powerfmt v0.2.1
Compiling web-time v1.1.0
Compiling fnv v1.0.7
Compiling same-file v1.0.6
Compiling erased-serde v0.4.11
Compiling brotli-decompressor v6.3.0
Compiling cfb v0.14.0
Compiling regex-automata v0.4.18
Compiling jsonptr v0.7.1
Compiling libc v0.2.191
Compiling time v0.3.55
Compiling markup5ever v0.39.0
Compiling idna_adapter v1.2.2
Compiling quick-xml v0.42.0
Compiling idna v1.1.0
Compiling base64 v0.23.1
Compiling dunce v1.0.5
Compiling http v1.5.0
Compiling html5ever v0.39.0
Compiling url v2.5.8
Compiling brotli v9.1.1
Compiling vswhom-sys v0.1.3
Compiling cargo_metadata v0.19.2
Compiling infer v0.22.0
Compiling json-patch v4.2.0
Compiling schemars v0.8.22
Compiling serde-untagged v0.1.9
Compiling plist v1.10.3
Compiling dom_query v0.28.0
Compiling walkdir v2.5.0
Compiling serde_with v3.24.0
Compiling anyhow v1.0.104
Checking windows-threading v0.2.1
Compiling ctor v1.0.13
Compiling glob v0.3.4
Compiling regex v1.13.1
Checking windows-future v0.3.2
Compiling vswhom v0.1.0
Compiling urlpattern v0.6.0
Checking windows-numerics v0.3.1
Checking windows-collections v0.3.2
Compiling winreg v0.56.0
Compiling time-macros v0.2.32
Compiling option-ext v0.2.0
Checking windows v0.62.2
Compiling embed-resource v3.0.12
Compiling tauri-utils v2.10.2
Compiling dirs-sys v0.5.0
Compiling webview2-com-sys v0.39.1
Checking raw-window-handle v0.6.2
Compiling dirs v7.0.0
Compiling tauri-winres v0.3.7
Compiling crc32fast v1.5.4
Compiling cargo_toml v1.0.1
Checking dpi v0.1.2
Compiling flate2 v1.1.11
Checking bitflags v2.13.2
Checking windows-version v0.1.7
Compiling png v0.18.1
Checking crossbeam-channel v0.5.17
Checking cookie v0.18.2
Checking tracing-core v0.1.36
Checking semver v1.0.28
Compiling ico v0.6.0
Compiling sha2 v0.10.9
Checking tracing v0.1.44
Checking unicode-segmentation v1.13.3
Checking softbuffer v0.4.8
Checking keyboard-types v0.8.3
Checking serialize-to-javascript v0.1.2
Checking tokio v1.54.1
Checking window-vibrancy v0.8.1
Checking mime v0.3.18
Checking heck v0.5.0
Checking orchestrator-ipc v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\crates\orchestrator-ipc)
Checking tempfile v3.27.0
Checking muda v0.20.0
Compiling tauri-build v2.7.4
Compiling tauri-codegen v2.7.4
Compiling tauri v2.12.5
Compiling tauri-macros v2.7.4
Compiling agent-orch-desktop v0.1.0 (C:\Users\<user>\AppData\Local\Temp\ao11r2-20261110T024313Z-b570be2e\source\apps\desktop\src-tauri)
Checking tao v0.37.2
Checking webview2-com v0.39.1
Checking tauri-runtime v2.12.2
Checking wry v0.57.1
Checking tauri-runtime-wry v2.12.3
Finished `dev` profile [unoptimized + debuginfo] target(s) in 2m 01s
```

### desktop-package-win

```text
✓ built in 364ms
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

### unproven-tauri-native-bridge

```text
status: UNPROVEN
The Tauri host was built and its 6 unit tests ran, but the running bridge was not exercised: no window, no webview, no daemon_open, daemon_send or daemon_close call and no live bridge to a daemon.
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

## Coverage gaps

- Unix-only tests did not run on Windows: IPC integration_tests.rs, lifecycle_tests.rs and
  timeout_tests.rs, the IPC wrong-token relay test, the whole daemon executor_tests.rs module,
  tests/daemon.rs, tests/coordinator_review.rs, three daemon cancellation unit tests, the daemon
  symlink test, and one src-tauri bridge test. Their macOS results are separate and are not Windows
  evidence. The scheduler and executor have no native Windows test in this run.
- NOT RUN: the desktop e2e suite (Unix domain socket only). No Windows e2e number exists.
- UNPROVEN or NOT RUN, whatever the overall result: denial of a second, different local user;
  rejection of a genuinely remote client; a single daemon-to-client write above 64 KiB to a slow
  reader; the running Tauri bridge; installing and launching the app; a daemon-owned task surviving a
  window close and reopen; the window event reconnect; the stale-token relaunch timing (UNMEASURED);
  the 60 s daemon idle behaviour on the app; and killing the daemon or breaking the sidecar.
- The daemon built by daemon-build was never started by the harness. Server identity verification by
  the client, the data-directory ACL, and the token owner are not validated on Windows.
- The Vitest counts (15 files, 287 tests) are Mac-measured. They are believed platform-neutral
  (jsdom with an in-memory fake daemon) and are unmeasured on Windows.
- No rustfmt gate, no workspace-wide test run, and no store, engine, git or CLI test run ran.