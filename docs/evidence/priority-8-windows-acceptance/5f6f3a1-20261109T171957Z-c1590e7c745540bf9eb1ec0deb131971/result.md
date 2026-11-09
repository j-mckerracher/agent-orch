# Priority 8 Windows acceptance of 5f6f3a117089fe6c08936bc2b21c5d76b83cdb39 (M5-01 daemon and authenticated local IPC)

This run does not close M5-01. It does not cover M5-01b (Windows named-pipe transport, pipe DACL
and token or SID handling, remote-client rejection, instance cap, and native security coverage).
The coordinator decides acceptance. The expected counts are derived from source for Windows. They
are not the Mac counts and they are not the Priority 7 native counts. Windows counts differ from the
Mac counts (Mac IPC 23, daemon 26) and are reported here as measured, not matched. No macOS or
review result is Windows evidence.

- Run ID: 20261109T171957Z-c1590e7c745540bf9eb1ec0deb131971
- UTC date: 2026-11-09T17:23:52.6174072Z
- Source branch: orch/priority-8-m5-01-5f6f3a1
- Branch tip at clone time: 5f6f3a117089fe6c08936bc2b21c5d76b83cdb39
- Source commit: 5f6f3a117089fe6c08936bc2b21c5d76b83cdb39
- Source tree and only parent (verified): source-tree.stdout.txt, source-parent.stdout.txt
- HEAD matched the source commit before the checks and after them: yes
  (source-head-before.stdout.txt, source-head-after.stdout.txt)
- Git top-level path form (plain drive-letter form required): source-toplevel.stdout.txt
- Baseline ancestry (cee8a81f), commit count, and changed-file set: base-ancestry,
  range-count, range-files (consistency gates, not patch-equivalence proof)
- Runbook identity (operator-declared; exact bytes pinned by blob id, size, and SHA-256; not proof that
  the pasted commands match): runbook-identity.txt
- Overall result: INCONCLUSIVE 
- Toolchain: see git-version.stdout.txt, rustc-version.stdout.txt,
  cargo-version.stdout.txt, clippy-version.stdout.txt, git-autocrlf.stdout.txt (exit code in
  git-autocrlf.exit.txt)
- Daemon binary record (size and SHA-256 of the dev-profile build output): daemon-binary.txt
- OS, CPU, PowerShell: see os.txt, cpu.txt, powershell-version.txt
- Agent runtime and Sonnet 5.5/high setting: unverified
- Survivor scan: every check has a survivors file. Processes whose image lies under this run's
  private temp directory, or whose command line names this run's directory (long or 8.3 form), are
  listed and fail the check. Nothing is stopped by the scan.
- Raw stdout and stderr bytes are kept outside the checkout and outside the published files. Their
  size and SHA-256 are in each result.json.

| Command | Status | Exit | Timed out | Seconds | Running (N per binary) | Passed | Failed | Ignored | Notes |
|---|---|---|---|---|---|---|---|---|---|
| daemon-build | PASS | 0 | False | 121.972 |  |  |  |  |  |
| ipc-tests | PASS | 0 | False | 20.755 | 11,0 | 11 | 0 | 0 |  |
| daemon-tests | PASS | 0 | False | 18.996 | 17,0 | 17 | 0 | 0 |  |
| clippy-scoped | INCONCLUSIVE | 0 | False | 44.949 |  |  |  |  | 6 warning line(s). Expected 0. Report each one verbatim. |

## Test result lines

### daemon-build

```text
Compiling proc-macro2 v1.0.107
Compiling unicode-ident v1.0.27
Compiling quote v1.0.47
Compiling windows-link v0.2.1
Compiling stable_deref_trait v1.2.1
Compiling pin-project-lite v0.2.17
Compiling serde_core v1.0.229
Compiling version_check v0.9.5
Compiling writeable v0.6.4
Compiling smallvec v1.16.5
Compiling litemap v0.8.3
Compiling futures-core v0.3.34
Compiling utf_types v1.0.0
Compiling icu_normalizer_data v2.4.0
Compiling icu_properties_data v2.4.0
Compiling memchr v2.8.3
Compiling serde v1.0.229
Compiling utf8_iter v1.1.0
Compiling bytes v1.12.1
Compiling find-msvc-tools v0.1.15
Compiling generic-array v0.14.9
Compiling typenum v1.20.1
Compiling scopeguard v1.2.0
Compiling shlex v2.0.1
Compiling vcpkg v0.2.15
Compiling futures-sink v0.3.34
Compiling parking_lot_core v0.9.12
Compiling cfg-if v1.0.5
Compiling pkg-config v0.3.34
Compiling lock_api v0.4.14
Compiling cc v1.11.1
Compiling windows-sys v0.61.2
Compiling crossbeam-utils v0.8.23
Compiling equivalent v1.0.3
Compiling autocfg v1.5.1
Compiling syn v3.0.9
Compiling block-buffer v0.10.4
Compiling crypto-common v0.1.6
Compiling syn v2.0.119
Compiling once_cell v1.21.4
Compiling futures-io v0.3.34
Compiling futures-task v0.3.34
Compiling foldhash v0.2.0
Compiling zmij v1.0.23
Compiling slab v0.4.12
Compiling allocator-api2 v0.2.21
Compiling percent-encoding v2.3.2
Compiling tracing-core v0.1.36
Compiling num-traits v0.2.19
Compiling form_urlencoded v1.2.2
Compiling futures-util v0.3.34
Compiling digest v0.10.7
Compiling hashbrown v0.16.1
Compiling parking_lot v0.12.5
Compiling parking v2.2.1
Compiling serde_json v1.0.154
Compiling hashbrown v0.17.1
Compiling log v0.4.35
Compiling thiserror v2.0.21
Compiling cpufeatures v0.2.17
Compiling crc-catalog v2.5.0
Compiling hashlink v0.11.1
Compiling sha2 v0.10.9
Compiling crossbeam-queue v0.3.14
Compiling futures-intrusive v0.5.0
Compiling crc v3.4.0
Compiling indexmap v2.14.2
Compiling event-listener v5.4.2
Compiling mio v1.2.4
Compiling socket2 v0.6.5
Compiling synstructure v0.14.0
Compiling itoa v1.0.18
Compiling base64 v0.22.1
Compiling tokio v1.54.1
Compiling libsqlite3-sys v0.37.0
Compiling spin v0.9.9
Compiling futures-channel v0.3.34
Compiling heck v0.5.0
Compiling zerofrom-derive v0.1.8
Compiling yoke-derive v0.8.4
Compiling zerovec-derive v0.11.7
Compiling displaydoc v0.2.7
Compiling thiserror-impl v2.0.21
Compiling serde_derive v1.0.229
Compiling tracing-attributes v0.1.31
Compiling tokio-macros v2.7.2
Compiling flume v0.12.0
Compiling futures-executor v0.3.34
Compiling atoi v2.0.0
Compiling hex v0.4.3
Compiling dotenvy v0.15.7
Compiling libc v0.2.191
Compiling subtle v2.6.1
Compiling zerofrom v0.1.8
Compiling tracing v0.1.44
Compiling orchestrator-git v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p8-acc-20261109T171957Z-c1590e7c745540bf9eb1ec0deb131971\source\crates\orchestrator-git)
Compiling yoke v0.8.4
Compiling zerovec v0.11.9
Compiling zerotrie v0.2.5
Compiling either v1.19.0
Compiling orchestrator-core v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p8-acc-20261109T171957Z-c1590e7c745540bf9eb1ec0deb131971\source\crates\orchestrator-core)
Compiling orchestrator-ipc v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p8-acc-20261109T171957Z-c1590e7c745540bf9eb1ec0deb131971\source\crates\orchestrator-ipc)
Compiling tinystr v0.8.5
Compiling potential_utf v0.1.6
Compiling icu_collections v2.4.0
Compiling icu_locale_core v2.4.0
Compiling tokio-stream v0.1.19
Compiling orchestrator-runtime v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p8-acc-20261109T171957Z-c1590e7c745540bf9eb1ec0deb131971\source\crates\orchestrator-runtime)
Compiling icu_provider v2.4.0
Compiling icu_normalizer v2.4.0
Compiling icu_properties v2.4.0
Compiling idna_adapter v1.2.2
Compiling idna v1.1.0
Compiling url v2.5.8
Compiling sqlx-core v0.9.0
Compiling sqlx-sqlite v0.9.0
Compiling sqlx-macros-core v0.9.0
Compiling sqlx-macros v0.9.0
Compiling sqlx v0.9.0
Compiling orchestrator-store v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p8-acc-20261109T171957Z-c1590e7c745540bf9eb1ec0deb131971\source\crates\orchestrator-store)
Compiling orchestrator-daemon v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p8-acc-20261109T171957Z-c1590e7c745540bf9eb1ec0deb131971\source\crates\orchestrator-daemon)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 2m 01s
```

### ipc-tests

```text
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### daemon-tests

```text
running 17 tests
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.18s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### clippy-scoped

```text
Compiling stable_deref_trait v1.2.1
Checking windows-link v0.2.1
Compiling utf_types v1.0.0
Compiling litemap v0.8.3
Compiling smallvec v1.16.5
Compiling writeable v0.6.4
Compiling zerofrom v0.1.8
Compiling futures-core v0.3.34
Compiling pin-project-lite v0.2.17
Checking serde_core v1.0.229
Checking windows-sys v0.61.2
Compiling typenum v1.20.1
Checking yoke v0.8.4
Compiling utf8_iter v1.1.0
Compiling scopeguard v1.2.0
Checking cfg-if v1.0.5
Checking memchr v2.8.3
Checking zerovec v0.11.9
Checking zerotrie v0.2.5
Compiling lock_api v0.4.14
Compiling icu_properties_data v2.4.0
Compiling generic-array v0.14.9
Compiling icu_normalizer_data v2.4.0
Compiling futures-sink v0.3.34
Checking tinystr v0.8.5
Checking potential_utf v0.1.6
Checking icu_collections v2.4.0
Checking icu_locale_core v2.4.0
Compiling equivalent v1.0.3
Compiling crypto-common v0.1.6
Compiling block-buffer v0.10.4
Checking serde v1.0.229
Compiling parking_lot_core v0.9.12
Checking thiserror v2.0.21
Compiling futures-io v0.3.34
Checking icu_provider v2.4.0
Compiling futures-task v0.3.34
Compiling allocator-api2 v0.2.21
Compiling bytes v1.12.1
Compiling once_cell v1.21.4
Compiling percent-encoding v2.3.2
Compiling foldhash v0.2.0
Compiling slab v0.4.12
Compiling mio v1.2.4
Compiling socket2 v0.6.5
Compiling icu_properties v2.4.0
Compiling icu_normalizer v2.4.0
Compiling futures-util v0.3.34
Compiling hashbrown v0.16.1
Compiling form_urlencoded v1.2.2
Compiling tracing-core v0.1.36
Compiling tokio v1.54.1
Compiling parking_lot v0.12.5
Compiling digest v0.10.7
Compiling crossbeam-utils v0.8.23
Compiling hashbrown v0.17.1
Compiling crc-catalog v2.5.0
Compiling parking v2.2.1
Compiling log v0.4.35
Compiling cpufeatures v0.2.17
Compiling idna_adapter v1.2.2
Compiling event-listener v5.4.2
Compiling sha2 v0.10.9
Compiling tracing v0.1.44
Compiling crossbeam-queue v0.3.14
Compiling idna v1.1.0
Compiling indexmap v2.14.2
Compiling crc v3.4.0
Compiling either v1.19.0
Compiling futures-intrusive v0.5.0
Compiling hashlink v0.11.1
Compiling url v2.5.8
Compiling spin v0.9.9
Compiling num-traits v0.2.19
Checking zmij v1.0.23
Compiling base64 v0.22.1
Checking itoa v1.0.18
Compiling futures-executor v0.3.34
Checking serde_json v1.0.154
Compiling atoi v2.0.0
Compiling flume v0.12.0
Compiling futures-channel v0.3.34
Checking libsqlite3-sys v0.37.0
Compiling tokio-stream v0.1.19
Checking orchestrator-core v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p8-acc-20261109T171957Z-c1590e7c745540bf9eb1ec0deb131971\source\crates\orchestrator-core)
Checking subtle v2.6.1
Checking getrandom v0.4.3
Checking orchestrator-git v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p8-acc-20261109T171957Z-c1590e7c745540bf9eb1ec0deb131971\source\crates\orchestrator-git)
Checking fastrand v2.5.0
Checking libc v0.2.191
Checking orchestrator-ipc v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p8-acc-20261109T171957Z-c1590e7c745540bf9eb1ec0deb131971\source\crates\orchestrator-ipc)
Compiling sqlx-core v0.9.0
Checking tempfile v3.27.0
warning: manually reimplementing `div_ceil`
warning: `orchestrator-ipc` (lib) generated 1 warning (run `cargo clippy --fix --lib -p orchestrator-ipc -- ` to apply 1 suggestion)
Checking orchestrator-runtime v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p8-acc-20261109T171957Z-c1590e7c745540bf9eb1ec0deb131971\source\crates\orchestrator-runtime)
warning: `orchestrator-ipc` (lib test) generated 1 warning (1 duplicate)
warning: you seem to be trying to use `match` for destructuring a single pattern. Consider using `if let`
warning: you seem to be trying to use `match` for destructuring a single pattern. Consider using `if let`
warning: `orchestrator-runtime` (lib) generated 2 warnings (run `cargo clippy --fix --lib -p orchestrator-runtime -- ` to apply 2 suggestions)
Compiling sqlx-sqlite v0.9.0
Compiling sqlx-macros-core v0.9.0
Compiling sqlx-macros v0.9.0
Checking sqlx v0.9.0
Checking orchestrator-store v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p8-acc-20261109T171957Z-c1590e7c745540bf9eb1ec0deb131971\source\crates\orchestrator-store)
Checking orchestrator-daemon v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p8-acc-20261109T171957Z-c1590e7c745540bf9eb1ec0deb131971\source\crates\orchestrator-daemon)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 44.15s
```

## Coverage gaps

- Unix-only tests did not run on Windows: the IPC integration, lifecycle, and timeout tests; the
  whole of daemon tests/daemon.rs including the new descriptor-exhaustion test; three daemon
  cancellation unit tests; and the daemon symlink test. Their macOS results are separate and are not
  Windows evidence.
- The accept-error retry fix in unix_transport.rs is not compiled on Windows. Nothing here shows it.
- Only the server.rs thread-spawn failure branch is a shared change. It was compiled, linted, and
  covered by the platform-neutral unit tests only. It was never triggered.
- M5-01b is not covered: windows_transport.rs has no tests. Named-pipe security was not exercised.
- There is no Windows IPC timeout, handshake, lifecycle, or end-to-end daemon claim.
- No rustfmt gate and no workspace-wide test run ran.