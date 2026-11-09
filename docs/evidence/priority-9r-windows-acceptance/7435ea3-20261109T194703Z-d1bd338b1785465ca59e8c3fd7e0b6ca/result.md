# Priority 9 remediation Windows acceptance rerun of 7435ea32bcb541044e61ef49e863d394f0d49e12 (M5-01b Windows named pipe and M5-02 paged event replay)

This run covers the remediated Priority 9 candidate: the Windows named-pipe transport (M5-01b) and
paged event replay (M5-02), in orchestrator-ipc and orchestrator-daemon. It reassesses the rows that
failed at dd0bbe33 (Truncated header, send-closed:232). The coordinator decides
acceptance. The expected counts are derived from the cfg and test attributes at the pinned source
for Windows (IPC 31, daemon 20, windows_transport 13). They are not the Mac counts (the owner
reports Mac IPC 30 and daemon 24 plus 5 integration tests; this runbook did not confirm those) and
they are not the failed dd0bbe33 native counts (IPC running 26, 9 failed). Counts are reported as measured, not matched. No macOS
or review result is Windows evidence. This run is also not a second-user, a genuinely remote-client,
or an M6 threat-review claim.

- Run ID: 20261109T194703Z-d1bd338b1785465ca59e8c3fd7e0b6ca
- UTC date: 2026-11-09T19:51:20.8934752Z
- Source branch: orch/priority-9r-m5-01b-m5-02-7435ea3
- Branch tip at clone time: 7435ea32bcb541044e61ef49e863d394f0d49e12
- Source commit: 7435ea32bcb541044e61ef49e863d394f0d49e12
- Source tree and only parent (verified): source-tree.stdout.txt, source-parent.stdout.txt
- HEAD matched the source commit before the checks and after them: yes
  (source-head-before.stdout.txt, source-head-after.stdout.txt)
- Git top-level path form (plain drive-letter form required): source-toplevel.stdout.txt
- Baseline ancestry (5f6f3a11), commit count, the five pinned commits, the linear range, and the
  changed-file set: base-ancestry, range-count, range-merges, range-commits, range-files
  (consistency gates, not patch-equivalence proof)
- Runbook identity (operator-declared; exact bytes pinned by blob id, size, and SHA-256; not proof that
  the pasted commands match): runbook-identity.txt
- Overall result: INCONCLUSIVE UNPROVEN, not covered by this result: probe-second-user.
- UNPROVEN rows (never covered by an overall PASS): probe-second-user
- Toolchain: see git-version.stdout.txt, rustc-version.stdout.txt,
  cargo-version.stdout.txt, clippy-version.stdout.txt, git-autocrlf.stdout.txt (exit code in
  git-autocrlf.exit.txt)
- Daemon binary record (size and SHA-256 of the dev-profile build output): daemon-binary.txt
- Clippy diagnostics, classified one by one against the recorded baseline: clippy-scoped.diagnostics.txt
- Native probe: probe-context.txt (elevation, redacted SID, pipe names), one probe-*.txt file per row,
  probe-daemon-r1..r3 (daemon output), probe-second-daemon-r1..r3, and probe-survivors.txt
- OS, CPU, PowerShell: see os.txt, cpu.txt, powershell-version.txt
- Agent runtime and Sonnet 5.5/high setting: unverified
- Survivor scan: every check and the probe have a survivors file. Processes whose image lies under
  this run's private temp directory, or whose command line names this run's directory (long or 8.3
  form), are listed and fail the check. Nothing is stopped by the scan.
- Raw stdout and stderr bytes are kept outside the checkout and outside the published files. Their
  size and SHA-256 are in each result.json.

| Command or probe row | Status | Exit | Timed out | Seconds | Running (N per binary) | Passed | Failed | Ignored | Notes |
|---|---|---|---|---|---|---|---|---|---|
| daemon-build | PASS | 0 | False | 92.958 |  |  |  |  |  |
| ipc-tests | PASS | 0 | False | 27.085 | 31,0 | 31 | 0 | 0 |  |
| daemon-tests | PASS | 0 | False | 34.502 | 20,0 | 20 | 0 | 0 |  |
| windows-transport-tests | INCONCLUSIVE | 0 | False | 6.324 | 13 | 13 | 0 | 0 | Per-binary running counts 13 differ from the derived 13,0. |
| clippy-scoped | PASS | 0 | False | 51.07 |  |  |  |  | 2 baseline diagnostic(s) noted (supervisor.rs single_match in orchestrator-runtime, a dependency of the daemon). They are not counted. |
| probe-pipe-dacl | PASS |  | False | 24.087 |  |  |  |  | Rounds: PASS, PASS, PASS. See probe-pipe-dacl.txt. |
| probe-token-acl | PASS |  | False | 24.087 |  |  |  |  | Rounds: PASS, PASS, PASS. See probe-token-acl.txt. |
| probe-instance-limit | PASS |  | False | 24.087 |  |  |  |  | Rounds: PASS, PASS, PASS. See probe-instance-limit.txt. |
| probe-remote-client | PASS |  | False | 24.087 |  |  |  |  | Rounds: PASS, PASS, PASS. See probe-remote-client.txt. |
| probe-anonymous-denial | PASS |  | False | 24.087 |  |  |  |  | Rounds: PASS, PASS, PASS. See probe-anonymous-denial.txt. |
| probe-second-user | UNPROVEN |  | False | 24.087 |  |  |  |  | Rounds: UNPROVEN. See probe-second-user.txt. |
| probe-large-frame | PASS |  | False | 24.087 |  |  |  |  | Rounds: PASS, PASS, PASS. See probe-large-frame.txt. |
| probe-connect-close | PASS |  | False | 24.087 |  |  |  |  | Rounds: PASS, PASS, PASS. See probe-connect-close.txt. |
| probe-second-daemon | PASS |  | False | 24.087 |  |  |  |  | Rounds: PASS, PASS, PASS. See probe-second-daemon.txt. |
| probe-daemon-exit | PASS |  | False | 24.087 |  |  |  |  | Rounds: PASS, PASS, PASS. See probe-daemon-exit.txt. |

## Test result lines and probe observations

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
Compiling utf_types v1.0.0
Compiling litemap v0.8.3
Compiling futures-core v0.3.34
Compiling icu_properties_data v2.4.0
Compiling serde v1.0.229
Compiling memchr v2.8.3
Compiling icu_normalizer_data v2.4.0
Compiling utf8_iter v1.1.0
Compiling bytes v1.12.1
Compiling scopeguard v1.2.0
Compiling shlex v2.0.1
Compiling generic-array v0.14.9
Compiling typenum v1.20.1
Compiling find-msvc-tools v0.1.15
Compiling lock_api v0.4.14
Compiling vcpkg v0.2.15
Compiling futures-sink v0.3.34
Compiling pkg-config v0.3.34
Compiling parking_lot_core v0.9.12
Compiling cfg-if v1.0.5
Compiling windows-sys v0.61.2
Compiling crossbeam-utils v0.8.23
Compiling cc v1.11.1
Compiling equivalent v1.0.3
Compiling slab v0.4.12
Compiling futures-io v0.3.34
Compiling syn v3.0.9
Compiling syn v2.0.119
Compiling foldhash v0.2.0
Compiling allocator-api2 v0.2.21
Compiling crypto-common v0.1.6
Compiling block-buffer v0.10.4
Compiling zmij v1.0.23
Compiling once_cell v1.21.4
Compiling percent-encoding v2.3.2
Compiling autocfg v1.5.1
Compiling futures-task v0.3.34
Compiling hashbrown v0.16.1
Compiling form_urlencoded v1.2.2
Compiling parking_lot v0.12.5
Compiling tracing-core v0.1.36
Compiling futures-util v0.3.34
Compiling digest v0.10.7
Compiling num-traits v0.2.19
Compiling log v0.4.35
Compiling cpufeatures v0.2.17
Compiling hashbrown v0.17.1
Compiling thiserror v2.0.21
Compiling crc-catalog v2.5.0
Compiling parking v2.2.1
Compiling serde_json v1.0.154
Compiling event-listener v5.4.2
Compiling crossbeam-queue v0.3.14
Compiling crc v3.4.0
Compiling indexmap v2.14.2
Compiling sha2 v0.10.9
Compiling hashlink v0.11.1
Compiling synstructure v0.14.0
Compiling futures-intrusive v0.5.0
Compiling mio v1.2.4
Compiling socket2 v0.6.5
Compiling base64 v0.22.1
Compiling itoa v1.0.18
Compiling tokio v1.54.1
Compiling libsqlite3-sys v0.37.0
Compiling tracing-attributes v0.1.31
Compiling spin v0.9.9
Compiling futures-executor v0.3.34
Compiling zerofrom-derive v0.1.8
Compiling yoke-derive v0.8.4
Compiling zerovec-derive v0.11.7
Compiling displaydoc v0.2.7
Compiling thiserror-impl v2.0.21
Compiling serde_derive v1.0.229
Compiling tokio-macros v2.7.2
Compiling flume v0.12.0
Compiling atoi v2.0.0
Compiling futures-channel v0.3.34
Compiling dotenvy v0.15.7
Compiling hex v0.4.3
Compiling zerofrom v0.1.8
Compiling tracing v0.1.44
Compiling heck v0.5.0
Compiling libc v0.2.191
Compiling subtle v2.6.1
Compiling yoke v0.8.4
Compiling orchestrator-git v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p9r-acc-20261109T194703Z-d1bd338b1785465ca59e8c3fd7e0b6ca\source\crates\orchestrator-git)
Compiling tokio-stream v0.1.19
Compiling zerovec v0.11.9
Compiling zerotrie v0.2.5
Compiling tinystr v0.8.5
Compiling potential_utf v0.1.6
Compiling either v1.19.0
Compiling orchestrator-core v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p9r-acc-20261109T194703Z-d1bd338b1785465ca59e8c3fd7e0b6ca\source\crates\orchestrator-core)
Compiling orchestrator-ipc v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p9r-acc-20261109T194703Z-d1bd338b1785465ca59e8c3fd7e0b6ca\source\crates\orchestrator-ipc)
Compiling icu_collections v2.4.0
Compiling icu_locale_core v2.4.0
Compiling icu_provider v2.4.0
Compiling icu_normalizer v2.4.0
Compiling icu_properties v2.4.0
Compiling orchestrator-runtime v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p9r-acc-20261109T194703Z-d1bd338b1785465ca59e8c3fd7e0b6ca\source\crates\orchestrator-runtime)
Compiling idna_adapter v1.2.2
Compiling idna v1.1.0
Compiling url v2.5.8
Compiling sqlx-core v0.9.0
Compiling sqlx-sqlite v0.9.0
Compiling sqlx-macros-core v0.9.0
Compiling sqlx-macros v0.9.0
Compiling sqlx v0.9.0
Compiling orchestrator-store v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p9r-acc-20261109T194703Z-d1bd338b1785465ca59e8c3fd7e0b6ca\source\crates\orchestrator-store)
Compiling orchestrator-daemon v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p9r-acc-20261109T194703Z-d1bd338b1785465ca59e8c3fd7e0b6ca\source\crates\orchestrator-daemon)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 32s
```

### ipc-tests

```text
running 31 tests
test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.17s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### daemon-tests

```text
running 20 tests
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 19.02s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### windows-transport-tests

```text
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 18 filtered out; finished in 5.07s
```

### clippy-scoped

```text
Compiling stable_deref_trait v1.2.1
Checking windows-link v0.2.1
Compiling writeable v0.6.4
Compiling smallvec v1.16.5
Compiling litemap v0.8.3
Compiling utf_types v1.0.0
Compiling zerofrom v0.1.8
Compiling futures-core v0.3.34
Compiling pin-project-lite v0.2.17
Checking serde_core v1.0.229
Checking windows-sys v0.61.2
Compiling scopeguard v1.2.0
Compiling yoke v0.8.4
Compiling utf8_iter v1.1.0
Checking memchr v2.8.3
Compiling typenum v1.20.1
Checking cfg-if v1.0.5
Compiling zerovec v0.11.9
Compiling zerotrie v0.2.5
Compiling lock_api v0.4.14
Compiling icu_properties_data v2.4.0
Compiling icu_normalizer_data v2.4.0
Compiling futures-sink v0.3.34
Compiling equivalent v1.0.3
Compiling generic-array v0.14.9
Compiling tinystr v0.8.5
Compiling potential_utf v0.1.6
Compiling icu_locale_core v2.4.0
Compiling icu_collections v2.4.0
Checking thiserror v2.0.21
Checking serde v1.0.229
Compiling crypto-common v0.1.6
Compiling block-buffer v0.10.4
Compiling parking_lot_core v0.9.12
Compiling bytes v1.12.1
Compiling futures-task v0.3.34
Compiling once_cell v1.21.4
Checking icu_provider v2.4.0
Compiling socket2 v0.6.5
Compiling mio v1.2.4
Compiling futures-io v0.3.34
Compiling foldhash v0.2.0
Compiling percent-encoding v2.3.2
Compiling slab v0.4.12
Compiling icu_properties v2.4.0
Compiling icu_normalizer v2.4.0
Compiling allocator-api2 v0.2.21
Compiling form_urlencoded v1.2.2
Compiling tokio v1.54.1
Compiling futures-util v0.3.34
Compiling tracing-core v0.1.36
Compiling hashbrown v0.16.1
Compiling parking_lot v0.12.5
Compiling digest v0.10.7
Compiling crossbeam-utils v0.8.23
Compiling cpufeatures v0.2.17
Compiling hashbrown v0.17.1
Compiling parking v2.2.1
Compiling log v0.4.35
Compiling crc-catalog v2.5.0
Compiling crossbeam-queue v0.3.14
Compiling hashlink v0.11.1
Compiling idna_adapter v1.2.2
Compiling tracing v0.1.44
Compiling crc v3.4.0
Compiling event-listener v5.4.2
Compiling sha2 v0.10.9
Compiling idna v1.1.0
Compiling either v1.19.0
Compiling indexmap v2.14.2
Compiling futures-intrusive v0.5.0
Compiling spin v0.9.9
Compiling num-traits v0.2.19
Checking zmij v1.0.23
Compiling url v2.5.8
Compiling base64 v0.22.1
Checking itoa v1.0.18
Compiling flume v0.12.0
Checking serde_json v1.0.154
Compiling atoi v2.0.0
Compiling futures-executor v0.3.34
Compiling futures-channel v0.3.34
Compiling tokio-stream v0.1.19
Checking libsqlite3-sys v0.37.0
Compiling sqlx-core v0.9.0
Checking orchestrator-core v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p9r-acc-20261109T194703Z-d1bd338b1785465ca59e8c3fd7e0b6ca\source\crates\orchestrator-core)
Checking subtle v2.6.1
Checking getrandom v0.4.3
Checking orchestrator-git v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p9r-acc-20261109T194703Z-d1bd338b1785465ca59e8c3fd7e0b6ca\source\crates\orchestrator-git)
Checking fastrand v2.5.0
Checking libc v0.2.191
Checking orchestrator-ipc v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p9r-acc-20261109T194703Z-d1bd338b1785465ca59e8c3fd7e0b6ca\source\crates\orchestrator-ipc)
Checking tempfile v3.27.0
Checking orchestrator-runtime v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p9r-acc-20261109T194703Z-d1bd338b1785465ca59e8c3fd7e0b6ca\source\crates\orchestrator-runtime)
warning: you seem to be trying to use `match` for destructuring a single pattern. Consider using `if let`
warning: you seem to be trying to use `match` for destructuring a single pattern. Consider using `if let`
warning: `orchestrator-runtime` (lib) generated 2 warnings (run `cargo clippy --fix --lib -p orchestrator-runtime -- ` to apply 2 suggestions)
Compiling sqlx-sqlite v0.9.0
Compiling sqlx-macros-core v0.9.0
Compiling sqlx-macros v0.9.0
Checking sqlx v0.9.0
Checking orchestrator-store v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p9r-acc-20261109T194703Z-d1bd338b1785465ca59e8c3fd7e0b6ca\source\crates\orchestrator-store)
Checking orchestrator-daemon v0.1.0 (C:\Users\<user>\AppData\Local\Temp\agent-orch-p9r-acc-20261109T194703Z-d1bd338b1785465ca59e8c3fd7e0b6ca\source\crates\orchestrator-daemon)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 50.27s
```

### probe-pipe-dacl

```text
round 1: PASS Protected single-ACE DACL for the current user: D:P(A;;FA;;;S-1-5-21-<domain>-377014)
round 2: PASS Protected single-ACE DACL for the current user: D:P(A;;FA;;;S-1-5-21-<domain>-377014)
round 3: PASS Protected single-ACE DACL for the current user: D:P(A;;FA;;;S-1-5-21-<domain>-377014)
Verdict: PASS
```

### probe-token-acl

```text
round 1: PASS One non-inherited ACE for the current user: <account>\<account>:(F); Get-Acl: D:P(A;;FA;;;S-1-5-21-<domain>-377014)
round 2: PASS One non-inherited ACE for the current user: <account>\<account>:(F); Get-Acl: D:P(A;;FA;;;S-1-5-21-<domain>-377014)
round 3: PASS One non-inherited ACE for the current user: <account>\<account>:(F); Get-Acl: D:P(A;;FA;;;S-1-5-21-<domain>-377014)
Verdict: PASS
```

### probe-instance-limit

```text
round 1: PASS 16 clients opened; the 17th got ERROR_PIPE_BUSY (231) at 0 ms and at 250 ms (645 ms in total); the instances were released.
round 2: PASS 16 clients opened; the 17th got ERROR_PIPE_BUSY (231) at 0 ms and at 250 ms (624 ms in total); the instances were released.
round 3: PASS 16 clients opened; the 17th got ERROR_PIPE_BUSY (231) at 0 ms and at 250 ms (651 ms in total); the instances were released.
Verdict: PASS
```

### probe-remote-client

```text
round 1: PASS The daemon pipe denied the loopback open (5). A control pipe with the same current-user-only DACL opened through the same path, so the denial is not caused by SMB loopback, policy, or token filtering. This assumes that .NET does not request remote-client rejection for the control pipe.
round 2: PASS The daemon pipe denied the loopback open (5). A control pipe with the same current-user-only DACL opened through the same path, so the denial is not caused by SMB loopback, policy, or token filtering. This assumes that .NET does not request remote-client rejection for the control pipe.
round 3: PASS The daemon pipe denied the loopback open (5). A control pipe with the same current-user-only DACL opened through the same path, so the denial is not caused by SMB loopback, policy, or token filtering. This assumes that .NET does not request remote-client rejection for the control pipe.
Verdict: PASS
```

### probe-anonymous-denial

```text
round 1: PASS Access denied (5) for the anonymous token.
round 2: PASS Access denied (5) for the anonymous token.
round 3: PASS Access denied (5) for the anonymous token.
Verdict: PASS
```

### probe-second-user

```text
UNPROVEN: no second local user is used. Denial of a different non-admin user is not exercised here. The anonymous-token row is the nearest check and does not replace it.
Verdict: UNPROVEN
```

### probe-large-frame

```text
round 1: PASS 512 KiB frame accepted in 32 chunks; a reply with the same id arrived; health still works. Reply kind: error.
round 2: PASS 512 KiB frame accepted in 32 chunks; a reply with the same id arrived; health still works. Reply kind: error.
round 3: PASS 512 KiB frame accepted in 32 chunks; a reply with the same id arrived; health still works. Reply kind: error.
UNPROVEN (not a row): a single daemon-to-client write above 64 KiB to a slow reader is not exercised. No daemon method on an empty store returns such a reply.
Verdict: PASS
```

### probe-connect-close

```text
round 1: PASS 20 connect-and-close cycles; the pipe stayed listed, answered WaitNamedPipe, and health still worked.
round 2: PASS 20 connect-and-close cycles; the pipe stayed listed, answered WaitNamedPipe, and health still worked.
round 3: PASS 20 connect-and-close cycles; the pipe stayed listed, answered WaitNamedPipe, and health still worked.
Verdict: PASS
```

### probe-second-daemon

```text
round 1: PASS Exit code 1 with a refusal diagnostic.
round 2: PASS Exit code 1 with a refusal diagnostic.
round 3: PASS Exit code 1 with a refusal diagnostic.
Verdict: PASS
```

### probe-daemon-exit

```text
round 1: PASS The shutdown reply arrived, the daemon exited 0, and daemon.token was removed.
round 2: PASS The shutdown reply arrived, the daemon exited 0, and daemon.token was removed.
round 3: PASS The shutdown reply arrived, the daemon exited 0, and daemon.token was removed.
Verdict: PASS
```

## Coverage gaps

- Unix-only tests did not run on Windows: all of integration_tests.rs, lifecycle_tests.rs,
  timeout_tests.rs, tests/daemon.rs, three daemon cancellation unit tests, and the daemon symlink
  test. Their macOS results are separate and are not Windows evidence.
- UNPROVEN, whatever the overall result: denial of a second, different local user; rejection of a
  genuinely remote client (at most SMB loopback is tried, and a remote-client denial without a working
  control pipe is capped at UNPROVEN, not PASS); and a single
  daemon-to-client write above 64 KiB to a slow reader.
- Server identity verification by the client, the data-directory ACL, and the token owner are not
  validated on Windows. The pipe-level AlreadyRunning path is covered only by the cargo test
  second_listener_for_the_same_data_directory_is_refused.
- The probe is one host, one account, and the elevation state recorded in probe-context.txt. Run it
  elevated and not elevated if both are available, as separate runs.
- No rustfmt gate and no workspace-wide test run ran.