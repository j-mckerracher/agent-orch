# Priority 10r2 Windows acceptance of e555c2c922ce00ee93673452f67848c7f888b4f1 (M5-02b scheduler and executor, M5-02c coordinator and review, M5-03 and M5-04 desktop, with the CRLF fixture remediation)

This run covers the remediated Priority 10 candidate: the IPC coordinator and review capabilities and
epoch handling, the daemon (executor, coordinator and review API) built natively on Windows, the
named-pipe transport tests, and the desktop install, type-check, build and unit
tests. The coordinator decides acceptance. The expected counts are derived from the cfg and test
attributes at the pinned source for Windows (IPC 38, daemon 42, windows_transport 13) and from the
Mac Vitest run (14 files, 268 tests). They are derived and unmeasured for this commit; the first
Priority 10 run on the previous tree agreed with the IPC, filter, and Vitest values. They are not the
Mac counts (IPC 37, daemon 59), and they are not the
Priority 9 remediation native counts (a different tree). Counts are reported as measured, not
matched. No macOS or review result is Windows evidence. This run is not a native probe, not a
second-user, genuinely remote-client, slow-reader, Tauri bridge, or Windows e2e claim, and not an M6
threat-review claim.

- Run ID: 20261109T234640Z-a20a123edce542ad8b94a65ecac24400
- UTC date: 2026-11-09T23:53:23.3661705Z
- Source branch: orch/priority-10r2-m5-02b-02c-03-04-e555c2c
- Branch tip at clone time: e555c2c922ce00ee93673452f67848c7f888b4f1
- Source commit: e555c2c922ce00ee93673452f67848c7f888b4f1
- Source tree and only parent (verified): source-tree.stdout.txt, source-parent.stdout.txt
- HEAD matched the source commit before the checks and after them: yes
  (source-head-before.stdout.txt, source-head-after.stdout.txt)
- Git top-level path form (plain drive-letter form required): source-toplevel.stdout.txt
- Baseline ancestry (7435ea32), commit count, the seventeen pinned commits, the linear range, and the
  changed-file set (114 files): base-ancestry, range-count, range-merges, range-commits, range-files
  (consistency gates, not patch-equivalence proof)
- Runbook identity (operator-declared; exact bytes pinned by blob id, size, and SHA-256; not proof that
  the pasted commands match): runbook-identity.txt
- Overall result: PASS UNPROVEN, not covered by this result: unproven-second-user, unproven-remote-client, unproven-slow-reader-64kib, unproven-tauri-native-bridge, unproven-e2e-windows.
- UNPROVEN rows (never covered by an overall PASS): unproven-second-user, unproven-remote-client, unproven-slow-reader-64kib, unproven-tauri-native-bridge, unproven-e2e-windows
- Toolchain: see git-version.stdout.txt, rustc-version.stdout.txt,
  cargo-version.stdout.txt, clippy-version.stdout.txt, node-version.stdout.txt,
  npm-version.stdout.txt, git-autocrlf.stdout.txt (exit code in git-autocrlf.exit.txt)
- Daemon binary record (size and SHA-256 of the dev-profile build output): daemon-binary.txt
- Clippy diagnostics, each classified BASELINE or NEW (NEW does not change the verdict):
  clippy-scoped.diagnostics.txt
- UNPROVEN and NOT RUN rows: one unproven-*.txt file each. No native probe ran, and no daemon was
  started.
- OS, CPU, PowerShell: see os.txt, cpu.txt, powershell-version.txt
- Agent runtime and Sonnet 5.5/high setting: unverified
- Survivor scan: every check has a survivors file. Processes whose image lies under
  this run's private temp directory, or whose command line names this run's directory (long or 8.3
  form), are listed and fail the check. Nothing is stopped by the scan.
- Raw stdout and stderr bytes are kept outside the checkout and outside the published files. Their
  size and SHA-256 are in each result.json.

| Command or row | Status | Exit | Timed out | Seconds | Running (N per binary) | Passed | Failed | Ignored | Notes |
|---|---|---|---|---|---|---|---|---|---|
| daemon-build | PASS | 0 | False | 89.883 |  |  |  |  |  |
| ipc-tests | PASS | 0 | False | 22.365 | 38,0 | 38 | 0 | 0 |  |
| daemon-tests | PASS | 0 | False | 45.896 | 42,0,0 | 42 | 0 | 0 |  |
| windows-transport-tests | PASS | 0 | False | 17.828 | 13 | 13 | 0 | 0 |  |
| clippy-scoped | PASS | 0 | False | 90.848 |  |  |  |  | 2 baseline diagnostic(s) (supervisor.rs single_match in orchestrator-runtime, a dependency of the daemon) and 0 NEW diagnostic(s) recorded in clippy-scoped.diagnostics.txt. NEW diagnostics do not change the verdict; report each one verbatim. |
| desktop-npm-ci | PASS | 0 | False | 24.923 |  |  |  |  |  |
| desktop-tsc | PASS | 0 | False | 5.778 |  |  |  |  |  |
| desktop-build | PASS | 0 | False | 2.242 |  |  |  |  |  |
| desktop-vitest | PASS | 0 | False | 54.278 |  | 268 | 0 |  |  |
| unproven-second-user | UNPROVEN |  | False | 0 |  |  |  |  | Denial of the named pipe to a second local user was not exercised. No second account is used. See unproven-second-user.txt. |
| unproven-remote-client | UNPROVEN |  | False | 0 |  |  |  |  | Rejection of a genuinely remote client was not exercised. No remote host or SMB loopback is used. See unproven-remote-client.txt. |
| unproven-slow-reader-64kib | UNPROVEN |  | False | 0 |  |  |  |  | A daemon-to-client write above 64 KiB to a slow reader was not exercised. See unproven-slow-reader-64kib.txt. |
| unproven-tauri-native-bridge | UNPROVEN |  | False | 0 |  |  |  |  | The Tauri shell and its native bridge to the daemon were not built or run. The shipped app has no connector until M5-05. See unproven-tauri-native-bridge.txt. |
| unproven-e2e-windows | UNPROVEN |  | False | 0 |  |  |  |  | NOT RUN. The desktop e2e suite (npm run test:e2e) uses a Unix domain socket and cannot run on Windows. No Windows e2e number exists. See unproven-e2e-windows.txt. |

## Result lines and observations

### daemon-build

```text
Compiling proc-macro2 v1.0.107
Compiling unicode-ident v1.0.27
Compiling quote v1.0.47
Compiling windows-link v0.2.1
Compiling stable_deref_trait v1.2.1
Compiling serde_core v1.0.229
Compiling pin-project-lite v0.2.17
Compiling serde v1.0.229
Compiling futures-core v0.3.34
Compiling memchr v2.8.3
Compiling cfg-if v1.0.5
Compiling version_check v0.9.5
Compiling utf_types v1.0.0
Compiling writeable v0.6.4
Compiling smallvec v1.16.5
Compiling litemap v0.8.3
Compiling icu_normalizer_data v2.4.0
Compiling utf8_iter v1.1.0
Compiling icu_properties_data v2.4.0
Compiling bytes v1.12.1
Compiling shlex v2.0.1
Compiling typenum v1.20.1
Compiling generic-array v0.14.9
Compiling scopeguard v1.2.0
Compiling find-msvc-tools v0.1.15
Compiling windows-sys v0.61.2
Compiling vcpkg v0.2.15
Compiling pkg-config v0.3.34
Compiling lock_api v0.4.14
Compiling futures-sink v0.3.34
Compiling cc v1.11.1
Compiling parking_lot_core v0.9.12
Compiling crossbeam-utils v0.8.23
Compiling equivalent v1.0.3
Compiling zmij v1.0.23
Compiling syn v3.0.9
Compiling syn v2.0.119
Compiling allocator-api2 v0.2.21
Compiling futures-task v0.3.34
Compiling crypto-common v0.1.6
Compiling block-buffer v0.10.4
Compiling thiserror v2.0.21
Compiling futures-io v0.3.34
Compiling percent-encoding v2.3.2
Compiling foldhash v0.2.0
Compiling slab v0.4.12
Compiling serde_json v1.0.154
Compiling once_cell v1.21.4
Compiling autocfg v1.5.1
Compiling hashbrown v0.16.1
Compiling futures-util v0.3.34
Compiling parking_lot v0.12.5
Compiling tracing-core v0.1.36
Compiling form_urlencoded v1.2.2
Compiling num-traits v0.2.19
Compiling digest v0.10.7
Compiling log v0.4.35
Compiling hashbrown v0.17.1
Compiling synstructure v0.14.0
Compiling cpufeatures v0.2.17
Compiling itoa v1.0.18
Compiling parking v2.2.1
Compiling crc-catalog v2.5.0
Compiling event-listener v5.4.2
Compiling crossbeam-queue v0.3.14
Compiling sha2 v0.10.9
Compiling crc v3.4.0
Compiling indexmap v2.14.2
Compiling mio v1.2.4
Compiling socket2 v0.6.5
Compiling hashlink v0.11.1
Compiling futures-intrusive v0.5.0
Compiling base64 v0.22.1
Compiling tokio v1.54.1
Compiling libsqlite3-sys v0.37.0
Compiling spin v0.9.9
Compiling zerofrom-derive v0.1.8
Compiling yoke-derive v0.8.4
Compiling zerovec-derive v0.11.7
Compiling displaydoc v0.2.7
Compiling serde_derive v1.0.229
Compiling thiserror-impl v2.0.21
Compiling tracing-attributes v0.1.31
Compiling tokio-macros v2.7.2
Compiling atoi v2.0.0
Compiling flume v0.12.0
Compiling futures-executor v0.3.34
Compiling futures-channel v0.3.34
Compiling zerofrom v0.1.8
Compiling tracing v0.1.44
Compiling yoke v0.8.4
Compiling heck v0.5.0
Compiling dotenvy v0.15.7
Compiling hex v0.4.3
Compiling getrandom v0.3.4
Compiling orchestrator-git v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p10r2-acc-20261109T234640Z-a20a123edce542ad8b94a65ecac24400\source\crates\orchestrator-git)
Compiling libc v0.2.191
Compiling async-trait v0.1.89
Compiling zerovec v0.11.9
Compiling zerotrie v0.2.5
Compiling subtle v2.6.1
Compiling tokio-stream v0.1.19
Compiling tinystr v0.8.5
Compiling potential_utf v0.1.6
Compiling either v1.19.0
Compiling orchestrator-core v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p10r2-acc-20261109T234640Z-a20a123edce542ad8b94a65ecac24400\source\crates\orchestrator-core)
Compiling orchestrator-ipc v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p10r2-acc-20261109T234640Z-a20a123edce542ad8b94a65ecac24400\source\crates\orchestrator-ipc)
Compiling icu_collections v2.4.0
Compiling icu_locale_core v2.4.0
Compiling uuid v1.18.1
Compiling icu_provider v2.4.0
Compiling icu_normalizer v2.4.0
Compiling icu_properties v2.4.0
Compiling orchestrator-runtime v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p10r2-acc-20261109T234640Z-a20a123edce542ad8b94a65ecac24400\source\crates\orchestrator-runtime)
Compiling orchestrator-agents v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p10r2-acc-20261109T234640Z-a20a123edce542ad8b94a65ecac24400\source\crates\orchestrator-agents)
Compiling idna_adapter v1.2.2
Compiling idna v1.1.0
Compiling url v2.5.8
Compiling sqlx-core v0.9.0
Compiling sqlx-sqlite v0.9.0
Compiling sqlx-macros-core v0.9.0
Compiling sqlx-macros v0.9.0
Compiling sqlx v0.9.0
Compiling orchestrator-store v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p10r2-acc-20261109T234640Z-a20a123edce542ad8b94a65ecac24400\source\crates\orchestrator-store)
Compiling orchestrator-engine v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p10r2-acc-20261109T234640Z-a20a123edce542ad8b94a65ecac24400\source\crates\orchestrator-engine)
Compiling orchestrator-daemon v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p10r2-acc-20261109T234640Z-a20a123edce542ad8b94a65ecac24400\source\crates\orchestrator-daemon)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 29s
```

### ipc-tests

```text
running 38 tests
test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.26s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### daemon-tests

```text
running 42 tests
test result: ok. 42 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 34.43s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### windows-transport-tests

```text
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 25 filtered out; finished in 5.08s
```

### clippy-scoped

```text
Compiling stable_deref_trait v1.2.1
Checking windows-link v0.2.1
Checking cfg-if v1.0.5
Checking windows-sys v0.61.2
Checking serde_core v1.0.229
Compiling utf_types v1.0.0
Compiling writeable v0.6.4
Compiling litemap v0.8.3
Compiling smallvec v1.16.5
Compiling zerofrom v0.1.8
Compiling futures-core v0.3.34
Compiling pin-project-lite v0.2.17
Checking memchr v2.8.3
Compiling utf8_iter v1.1.0
Compiling typenum v1.20.1
Compiling yoke v0.8.4
Compiling scopeguard v1.2.0
Compiling icu_properties_data v2.4.0
Compiling lock_api v0.4.14
Compiling icu_normalizer_data v2.4.0
Compiling futures-sink v0.3.34
Checking thiserror v2.0.21
Compiling zerovec v0.11.9
Compiling zerotrie v0.2.5
Compiling generic-array v0.14.9
Compiling equivalent v1.0.3
Compiling parking_lot_core v0.9.12
Compiling tinystr v0.8.5
Compiling potential_utf v0.1.6
Compiling mio v1.2.4
Checking serde v1.0.229
Compiling icu_collections v2.4.0
Compiling icu_locale_core v2.4.0
Compiling socket2 v0.6.5
Compiling block-buffer v0.10.4
Compiling crypto-common v0.1.6
Compiling percent-encoding v2.3.2
Checking bytes v1.12.1
Compiling allocator-api2 v0.2.21
Compiling foldhash v0.2.0
Checking once_cell v1.21.4
Compiling slab v0.4.12
Compiling futures-task v0.3.34
Compiling futures-io v0.3.34
Compiling tracing-core v0.1.36
Compiling form_urlencoded v1.2.2
Compiling hashbrown v0.16.1
Compiling futures-util v0.3.34
Compiling digest v0.10.7
Compiling tokio v1.54.1
Checking icu_provider v2.4.0
Compiling parking_lot v0.12.5
Compiling crossbeam-utils v0.8.23
Checking zmij v1.0.23
Compiling icu_normalizer v2.4.0
Compiling crc-catalog v2.5.0
Compiling cpufeatures v0.2.17
Compiling parking v2.2.1
Checking itoa v1.0.18
Compiling hashbrown v0.17.1
Compiling log v0.4.35
Compiling event-listener v5.4.2
Compiling icu_properties v2.4.0
Checking serde_json v1.0.154
Compiling crossbeam-queue v0.3.14
Compiling crc v3.4.0
Compiling tracing v0.1.44
Compiling sha2 v0.10.9
Compiling futures-intrusive v0.5.0
Compiling hashlink v0.11.1
Compiling either v1.19.0
Compiling indexmap v2.14.2
Compiling spin v0.9.9
Compiling num-traits v0.2.19
Compiling base64 v0.22.1
Compiling idna_adapter v1.2.2
Compiling idna v1.1.0
Compiling flume v0.12.0
Compiling url v2.5.8
Compiling atoi v2.0.0
Compiling futures-channel v0.3.34
Checking libsqlite3-sys v0.37.0
Checking orchestrator-core v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p10r2-acc-20261109T234640Z-a20a123edce542ad8b94a65ecac24400\source\crates\orchestrator-core)
Checking orchestrator-git v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p10r2-acc-20261109T234640Z-a20a123edce542ad8b94a65ecac24400\source\crates\orchestrator-git)
Checking getrandom v0.3.4
Checking subtle v2.6.1
Checking getrandom v0.4.3
Checking fastrand v2.5.0
Checking orchestrator-ipc v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p10r2-acc-20261109T234640Z-a20a123edce542ad8b94a65ecac24400\source\crates\orchestrator-ipc)
Checking libc v0.2.191
Checking uuid v1.18.1
Checking tempfile v3.27.0
Checking futures-executor v0.3.34
Checking tokio-stream v0.1.19
Checking orchestrator-runtime v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p10r2-acc-20261109T234640Z-a20a123edce542ad8b94a65ecac24400\source\crates\orchestrator-runtime)
warning: you seem to be trying to use `match` for destructuring a single pattern. Consider using `if let`
warning: you seem to be trying to use `match` for destructuring a single pattern. Consider using `if let`
warning: `orchestrator-runtime` (lib) generated 2 warnings (run `cargo clippy --fix --lib -p orchestrator-runtime -- ` to apply 2 suggestions)
Checking orchestrator-agents v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p10r2-acc-20261109T234640Z-a20a123edce542ad8b94a65ecac24400\source\crates\orchestrator-agents)
Compiling sqlx-core v0.9.0
Checking sqlx-sqlite v0.9.0
Compiling sqlx-macros-core v0.9.0
Compiling sqlx-macros v0.9.0
Checking sqlx v0.9.0
Checking orchestrator-store v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p10r2-acc-20261109T234640Z-a20a123edce542ad8b94a65ecac24400\source\crates\orchestrator-store)
Checking orchestrator-engine v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p10r2-acc-20261109T234640Z-a20a123edce542ad8b94a65ecac24400\source\crates\orchestrator-engine)
Checking orchestrator-daemon v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p10r2-acc-20261109T234640Z-a20a123edce542ad8b94a65ecac24400\source\crates\orchestrator-daemon)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 28s
```

### desktop-npm-ci

```text

```

### desktop-tsc

```text

```

### desktop-build

```text
✓ built in 271ms
```

### desktop-vitest

```text
Test Files  14 passed (14)
Tests  268 passed (268)
Duration  47.99s (environment 76%, setup 15%, tests 6%, import 2%, transform 1%)
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
The Tauri shell and its native bridge to the daemon were not built or run. The shipped app has no connector until M5-05.
```

### unproven-e2e-windows

```text
status: UNPROVEN
NOT RUN. The desktop e2e suite (npm run test:e2e) uses a Unix domain socket and cannot run on Windows. No Windows e2e number exists.
```

## Coverage gaps

- Unix-only tests did not run on Windows: IPC integration_tests.rs, lifecycle_tests.rs and
  timeout_tests.rs, the whole daemon executor_tests.rs module, tests/daemon.rs,
  tests/coordinator_review.rs, three daemon cancellation unit tests, and the daemon symlink test.
  Their macOS results are separate and are not Windows evidence. The scheduler and executor have no
  native Windows test in this run.
- NOT RUN: the desktop e2e suite (Unix domain socket only). No Windows e2e number exists.
- UNPROVEN, whatever the overall result: denial of a second, different local user; rejection of a
  genuinely remote client; a single daemon-to-client write above 64 KiB to a slow reader; and the
  Tauri shell and native bridge.
- The daemon was built but never started. Server identity verification by the client, the
  data-directory ACL, and the token owner are not validated on Windows.
- The Vitest counts (14 files, 268 tests) are Mac-measured. They are believed platform-neutral
  (jsdom with an in-memory fake daemon) and are unmeasured on Windows.
- No rustfmt gate, no workspace-wide test run, and no store, engine, git or CLI test run ran.