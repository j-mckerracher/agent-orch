# Priority 9 (M5-01b Windows named-pipe transport and M5-02 reconnectable event stream) native Windows acceptance runbook (candidate dd0bbe33f5e64843f5eef9ab7e52937ac26eda58)

This runbook is adapted from the Priority 8 runbook `docs/evidence/priority-8-windows-acceptance-5f6f3a1-runbook.md`
(blob `e29207ca3c3c9d9c23e14314a580f2a4a5af90b3`, docs commit `fc41237c17f837e49d2af54d4a0aa571d1915e51`,
review verdict PASS). That runbook derives from the accepted Priority 7 runbook
`docs/evidence/priority-7-windows-acceptance-cee8a81-runbook.md` (blob
`3d38dd015417d37c1dd6e8f2ba83ad1e61889c95`, docs commit `4d8371ec2d6193edf623c4fb84eda52e7c3ff35f`,
source `cee8a81f63f1d876ef26c8920a052910cdd7fe3d`, accepted native evidence
`5b9cf88af03aeb2578c59e0d560794c2df4ddd97`). Those are immutable git objects and are absent from the
source checkout. The helper blocks (`Protect-Text`, `Invoke-Bounded`, the survivor scan, the publication
scan, the verdict model) are reused with the changes listed in "What changed from the approved harness".
The Priority 8 review findings (L1 clippy scope wording, L2 push-URL check, N1 Mac-count labelling, N2
self-check sample disclosure) and the Priority 8 default-branch defect (the default clone branch did not
match the published branch) are applied. No new framework is introduced.

Do not run an old runbook against a new candidate.

## Status of this run

**Native Windows acceptance of this candidate is unproven. No native Windows run of it exists, and this
document does not claim one.** Nothing in this runbook, including the probe block, has been executed on
Windows by its author. The accepted Priority 7 and the Priority 8 native evidence prove only their own
sources. They are not evidence for this candidate, because `windows_transport.rs`, `client.rs`,
`session.rs`, `lib.rs`, and the daemon API changed. Mac results and the static derivation below are not
Windows evidence.

This run records how the candidate behaves on native Windows. It does not close M5-01b or M5-02. The
coordinator decides acceptance. The plan is `docs/technical-implementation-plan.md`.

Items under test, in the board's words:

- **M5-01 (Windows part, tracked as M5-01b).** Build and reverify on-demand daemon foundations for
  reliable scheduling, versioned length-prefixed JSON-RPC, authorization, capability negotiation and
  authenticated Unix-socket transport. Windows named-pipe transport and native security coverage are
  tracked separately in M5-01b. Shared protocol and authentication and Windows daemon build
  compatibility remain required.
- **M5-02.** Implement reconnectable event stream and clients: support event subscriptions,
  sequence-based replay, reconnect and the project, task, session, approval, workspace and delivery
  methods.

This is a combined candidate: two independently reviewed items integrated onto one base by four
cherry-picks. The runbook proves the integrated tree only.

## Candidate

- Repository: `https://github.com/j-mckerracher/agent-orch.git` (`origin`, the only remote and the
  only push target)
- Source branch (the coordinator publishes it after review): `orch/priority-9-m5-01b-m5-02-dd0bbe3`
- Source commit: `dd0bbe33f5e64843f5eef9ab7e52937ac26eda58`
- Source tree: `961aadde0c7a2cd0a23d1af729a6773b42ccddbd`
- Accepted baseline (the parent of the first commit above it; the Priority 8 candidate):
  `5f6f3a117089fe6c08936bc2b21c5d76b83cdb39`
- Runbook branch (the coordinator publishes it): `docs/priority-9-windows-acceptance-dd0bbe3`
- Evidence branch (you create it): `evidence/priority-9-windows-acceptance-dd0bbe3-<RunId>`
- Runbook: file `docs/evidence/priority-9-windows-acceptance-dd0bbe3-runbook.md`. The coordinator
  gives the branch, the full runbook commit, and the URL in the dispatch message. The runbook is read
  from the **tip of the docs branch**. The source under test stays `dd0bbe33`. The docs branch is a
  chain of docs-only commits above the source: the first revision (`8b0f3e92…`) has the source commit
  as its only parent, and a later revision of this file (a fix on top of the reviewed revision)
  has `8b0f3e92…` as its only parent. A runbook text cannot contain its own commit id, so the
  coordinator gives the tip commit in the dispatch message. Step 1 requires every commit between the
  source and the declared runbook commit to be a single linear docs commit (at most two, no merge, the
  first one `8b0f3e92…`) and requires the whole difference from the source to be this one file. The
  declared runbook commit is operator-declared. Step 1 fetches it, checks its shape, and records the
  SHA-256 of the exact runbook bytes. That pins the bytes in evidence. It still does not prove that the
  commands you pasted match them. Nothing in the docs chain is part of the source under test.
- If the source branch tip is later than the source commit, test the pinned commit and report the tip.

Test only the exact source commit. Before any other command, `HEAD` must equal the full source
commit. If the branch does not contain the commit, stop and report the mismatch.

### Ref names

Every ref has one namespace, and none is reused for another purpose. The default `$SourceBranch` in
step 1 is the published name below. Use another only if the dispatch message names it.

| Ref | Pattern | Who creates it |
|---|---|---|
| Source | `orch/priority-9-m5-01b-m5-02-dd0bbe3` (the block accepts `orch/priority-9-[a-z0-9-]+`) | coordinator |
| Runbook | `docs/priority-9-windows-acceptance-dd0bbe3` | coordinator |
| Evidence | `evidence/priority-9-windows-acceptance-dd0bbe3-<RunId>` where `<RunId>` is `yyyyMMddTHHmmssZ-<32 hex>` (UTC time plus a GUID, so it is unique) | you |

A local capture of refs must be named by its full namespace, never only its last segment: `orch/<name>`
and `docs/<name>` would otherwise collide. You push only the evidence ref.

### What the candidate changes

The source is four commits above `5f6f3a11…` (the original commits are in parentheses). The
`$ExpectedRangeFiles` gate checks the exact set of 11 changed paths (994 insertions, 50 deletions). There
is no manifest, lock, or dependency change, so `--locked` resolves exactly as it did at the baseline.

| Commit | Item | Subject |
|---|---|---|
| `96718d6c2` (`8767e26c1`) | M5-01b | Windows named-pipe transport tests |
| `3bb2db3bf` (`150e8d3c0`) | M5-01b | keep the Windows pipe name bound |
| `591db2187` (`acedbc9bd`) | M5-02 | paged event replay, replay-gap error, client reconnect, `projects.register` |
| `dd0bbe33f` (`b160e4839`) | M5-02 | keep the reconnect cursor on a failed resume and sync calls during replay |

| Path | Change | Compiled on Windows? |
|---|---|---|
| `crates/orchestrator-ipc/src/windows_transport.rs` | Replacement pipe instance is created before the old one is dropped, so the pipe name never loses its last instance; a client that connects and leaves is handled; a write to a closed peer fails promptly; the manual `div_ceil` is fixed. | **Yes** (`cfg(windows)`). |
| `crates/orchestrator-ipc/src/windows_transport_tests.rs` | 8 Windows-only tests (see below). | **Yes**, test profile only. |
| `crates/orchestrator-ipc/src/{client,session,lib}.rs` | Paged replay (at most 256 buffered events per page), typed replay-gap error, `Client::reconnect` with a cursor, the `event_stream_tests` module. | **Yes** (`cfg(any(unix, windows))`). |
| `crates/orchestrator-ipc/src/event_stream_tests.rs` | 7 platform-neutral tests over the public `Client` and `Server` API. | **Yes**, test profile only (`cfg(all(test, any(unix, windows)))`). |
| `crates/orchestrator-ipc/src/integration_tests.rs` | Assertion changes. | **No.** `cfg(all(test, unix))`. |
| `crates/orchestrator-ipc/src/unsupported.rs` | Stub for targets that are neither Unix nor Windows. | **No.** |
| `crates/orchestrator-daemon/src/api.rs` | `projects.register`, `sessions.events` paging and the replay cursor, delivery review ref. | **Yes.** |
| `crates/orchestrator-daemon/src/{api_tests,api_safety_tests}.rs` | Three new Windows-run tests (`project_registration_validates_the_repository_and_is_idempotent`, `session_events_page_and_report_a_cursor_beyond_the_journal`, `approved_local_review_delivery_creates_the_review_ref_once`). | **Yes**, test profile only. |

**Consequence.** On Windows this candidate compiles and runs the named-pipe transport, its 8 tests, the
event-stream tests, and the daemon API tests. The native probe below additionally starts the real daemon
over a real named pipe. Descriptor and Unix-socket paths are not compiled here.

Primer and plan at the pinned commit:
[orchestration primer](https://github.com/j-mckerracher/agent-orch/blob/dd0bbe33f5e64843f5eef9ab7e52937ac26eda58/orch-impl-m.md)
and
[implementation plan](https://github.com/j-mckerracher/agent-orch/blob/dd0bbe33f5e64843f5eef9ab7e52937ac26eda58/docs/technical-implementation-plan.md).
These links resolve only after the coordinator publishes the source branch. Follow the primer's skill
requirements. Report any unavailable guidance and any setting that you cannot observe as unverified.

## Assignment

Run the checks below on native Windows. Publish the evidence on a separate branch. Do not edit the
candidate source or `board.json`. Do not request operator action beyond running this runbook. Do not
open a pull request or merge.

The user requires GitHub Copilot subagents configured for Claude Sonnet 5.5 with high reasoning,
including descendants. This replaces the worker model setting in the primer for this run. Do not
start an OMP worker. The Windows host may run the commands below itself even if its own subagent
runtime cannot select Sonnet 5.5 with high reasoning, because the checks are shell commands whose
result does not depend on the model. If the host cannot select or observe that setting, write
`agent runtime: <what you observed>; Sonnet 5.5/high unavailable or unverified`. Never state that a
Sonnet agent ran when you did not observe it.

**PowerShell 7 (`pwsh`) only.** Do not use Windows PowerShell 5.1. The first block stops on it. The
host needs Windows 11 or later, Git for Windows, and the MSVC Rust toolchain with the `clippy`
component. `rustc` must be on `PATH` for the isolated gates. The daemon links SQLite through
`libsqlite3-sys`, which compiles C source and needs the MSVC C toolchain (Visual Studio Build Tools
with the C++ workload) on `PATH` as `cl.exe` resolves it. The Priority 8 native run compiled the daemon at
the baseline, but this is the first native compile at this tree, so a missing C toolchain is a plausible
`daemon-build` failure. Report it as is. Do not install anything. Use a short checkout path and
`core.longpaths` as the blocks do.

**Elevation.** The run records whether the session is elevated (`probe-context.txt`). Run it once from a
standard (non-elevated) session. If an elevated session is also available, run it a second time with a new
`RunId` and preserve both runs. The source notes that an elevated process owns its files as
`BUILTIN\Administrators` by default; the token ACL probe reports what the ACL actually is. If only one
elevation state is available, say which one in your report and that the other is not covered.

## Checks

Each check runs once per run. Each records its command, environment summary, exit code, duration, full
stdout and stderr, and every `running N tests` and `test result` line (test gates) or every
`Compiling`, `Checking`, `Finished`, `warning`, and `error` line (build and lint gates). The native
probe records its own evidence files.

### Where the expected counts come from (Mac-measured, derived for Windows, never edited)

Four classes stay separate and are never mixed:

- **Mac-measured** (reported by the item owners and the integrator at `dd0bbe33…` on macOS, **not
  independently confirmed by the coordinator**; none is Windows evidence): `orchestrator-ipc` **30
  passed, 0 failed, 0 ignored**; `orchestrator-daemon` **24 unit plus 5 integration**, 0 failed, 0
  ignored; clippy with `--all-targets` over both crates with 0 warnings; rustfmt clean on the 11
  changed files; a compile-only Windows MSVC target check and clippy of the IPC crate with `--tests`
  (exit 0, 0 warnings). A compile-only Windows check of the daemon on the Mac exited 101 because `cc`
  could not cross-compile the C source of `libsqlite3-sys`. That is environmental, so **the daemon's
  Windows compile at this tree is unverified until this run.**
- **Derived for Windows** (this document): the Mac attribute counts, minus every test that the `cfg`
  set removes on Windows, plus the Windows-only tests, read from the source tree `961aadde…` by
  `#[test]` and `cfg` attributes. The derivation below shows the arithmetic. These are the **expected**
  counts the gates enforce.
- **Prior native measurement** (history, a different tree): P8 native evidence `c6be3ea1…` at
  `5f6f3a11…` measured **IPC 11** and **daemon 17 unit** (`tests\daemon.rs` `running 0 tests`). It
  measured no named-pipe test, because there was none. It is not evidence for this commit.
- **Native-measured**: only what this run records. **Windows counts differ from the Mac counts and are
  reported as measured. They are not matched to the Mac counts, to the P8 counts, or to the derived
  counts.** A measured count that differs from the derived one gives `INCONCLUSIVE`; the operator
  reports the measured value exactly as printed and never edits an expected value or a measured value.

Every number maps to one argv, package, binary, and platform. `lib.rs` gates `auth`, `session`,
`client`, and `server` with `cfg(any(unix, windows))`, `unix_transport` with `cfg(unix)`,
`windows_transport` with `cfg(windows)`, `event_stream_tests` with `cfg(all(test, any(unix, windows)))`,
and `integration_tests` and `lifecycle_tests` with `cfg(all(test, unix))`; `timeout_tests` lives inside
`unix_transport`. `windows_transport_tests.rs` is the `#[cfg(test)] #[path]` module of
`windows_transport.rs`. No test in either crate carries `#[ignore]`.

| Target | Mac measured | Removed on Windows by `cfg` | Added on Windows | Windows derived (expected) |
|---|---|---|---|---|
| IPC lib unit tests (`src/lib.rs`) | 30 (protocol 6, auth 5, event_stream 7, timeout 1, lifecycle 3, integration 8) | 12 (8 `integration_tests` + 3 `lifecycle_tests` + 1 `timeout_tests`) | 8 `windows_transport::tests` | **26** (protocol 6, auth 5, event_stream 7, windows_transport 8) |
| IPC doc-tests | 0 | none | none | **0** (`running 0 tests`) |
| IPC total | 30 passed, 0 failed, 0 ignored | 12 | 8 | **26 passed, 0 failed, 0 ignored** |
| IPC filter `windows_transport` | not compiled on macOS | n/a | n/a | **8** (`running 8 tests`, then `running 0 tests` for doc-tests) |
| Daemon unit tests (`src/main.rs`) | 24 (`main` 1, `api_tests` 15, `api_safety_tests` 8) | 3 `api_tests` + 1 `api_safety_tests` (`cfg(unix)`) = 4 | none | **20** (`main` 1, `api_tests` 12, `api_safety_tests` 7) |
| Daemon integration (`tests\daemon.rs`, `#![cfg(unix)]`) | 5 | all 5 | none | **0** (`running 0 tests`) |
| Daemon total | 29 passed, 0 failed, 0 ignored | 9 | none | **20 passed, 0 failed, 0 ignored** |

The Unix-only tests, by file, are never Windows proof and none of them runs here:

- IPC `integration_tests.rs` (8), `lifecycle_tests.rs` (3), and `timeout_tests.rs` (1). There is **no**
  native IPC timeout test beyond what the Windows transport tests and the probe exercise.
- Daemon `api_tests.rs` (3): `running_task_cancellation_requests_owner_without_mutating_run_state`,
  `running_task_cancellation_with_crashed_owner_conflicts_and_retires_generation`,
  `running_task_without_owner_rejects_cancellation_without_partial_updates`.
- Daemon `api_safety_tests.rs` (1): `workspace_diff_rejects_symlink_escape_before_git`.
- Daemon `tests\daemon.rs` (5): all.

Windows-run tests by name (the arrays in the `$Checks` block hold these exact names; every name must
report `ok`):

- **IPC (26).** `protocol::tests::` `frame_round_trip_uses_versioned_big_endian_header`,
  `oversized_length_is_rejected_before_body_read`, `truncated_header_and_body_are_typed`,
  `wrong_version_and_malformed_json_are_typed_without_payload`,
  `non_eof_read_errors_are_not_reported_as_truncated`, `json_rpc_string_request_ids_are_supported`.
  `auth::tests::` `chooses_highest_common_version_and_rejects_unknown_capability_safely`,
  `absent_token_is_authentication_failure`, `startup_replaces_only_regular_stale_token`,
  `reports_no_common_version`, `token_encoding_requires_lowercase_hex`.
  `event_stream_tests::` `replay_larger_than_the_live_buffer_is_paged_across_reconnect`,
  `cursor_ahead_of_the_journal_is_a_typed_replay_gap`,
  `failed_resume_keeps_the_cursor_until_a_resume_succeeds`,
  `call_during_a_large_replay_keeps_events_and_the_connection_in_sync`,
  `live_lag_is_typed_and_reconnect_catches_up_in_order`,
  `reconnect_follows_a_daemon_restart_with_a_new_token`, `reconnect_still_requires_the_current_token`.
  `windows_transport::tests::` `pipe_and_token_dacls_grant_only_the_current_user`,
  `pipe_limits_and_remote_rejection_are_configured`,
  `second_listener_for_the_same_data_directory_is_refused`,
  `instance_limit_is_enforced_and_released`,
  `pipe_name_stays_bound_when_a_client_connects_and_leaves`,
  `write_to_a_closed_peer_fails_promptly`, `every_connection_must_complete_the_authenticated_handshake`,
  `stalled_connection_does_not_block_other_clients`.
- **Daemon (20).** `tests::lock_conflict_reports_live_holder_pid`; twelve `api::tests::` names (the 15
  `api_tests` minus the 3 Unix-only) and seven `api::tests::safety::` names (the 8 minus the
  symlink-escape test). The full list is in the `$DaemonTests` array.

### Check table

All cargo commands use `--locked` and literal argument lists. The five cargo gates run in this order,
then the native probe.

| Check | Command (after `cargo`) | Expect (Windows) |
|---|---|---|
| `daemon-build` | `build --locked -p orchestrator-daemon` | Exit 0 in a new, empty target directory. Stderr has a `Compiling orchestrator-ipc` line, a `Compiling orchestrator-daemon` line, and a `Finished` line. The file `debug\orchestrator-daemon.exe` exists in the target directory. Its size and SHA-256 go to `daemon-binary.txt`, and the probe launches this exact file. A cold build compiles `libsqlite3-sys` C source and may be slow. |
| `ipc-tests` | `test --locked -p orchestrator-ipc` | Exit 0. `running` lines in order: 26, 0 (lib, doc-tests). 26 passed, 0 failed, 0 ignored. Every IPC name above reports `ok`. Isolated. |
| `daemon-tests` | `test --locked -p orchestrator-daemon` | Exit 0. `running` lines in order: 20, 0 (`src\main.rs`, `tests\daemon.rs`). 20 passed, 0 failed, 0 ignored. Every daemon name above reports `ok`. Isolated. |
| `windows-transport-tests` | `test --locked -p orchestrator-ipc windows_transport` | Exit 0. `running` lines in order: 8, 0. 8 passed, 0 failed, 0 ignored. The 8 `windows_transport::tests` names report `ok`. Isolated. |
| `clippy-scoped` | `clippy --locked -p orchestrator-ipc -p orchestrator-daemon --all-targets` | Exit 0. Stderr has `Checking orchestrator-ipc`, `Checking orchestrator-daemon`, and `Finished` lines. No `-D warnings`, no `--no-deps`. Diagnostics are classified (see below). |
| `native-probe` | live daemon probe (the PowerShell block in step 3) | Ten rows, each repeated in three rounds (the second-user row once); see "Native probe" below. |

**Clippy scope and wording (review item L1).** The gate lints the two named packages **and, because it
does not pass `--no-deps`, the workspace crates they depend on** (the daemon depends on
`orchestrator-runtime`, `orchestrator-store`, `orchestrator-core`, and `orchestrator-git`; the IPC
crate has no workspace dependency). It runs without `-D warnings`, so the exit code and the
diagnostics are judged separately. The Mac clippy results in the owner reports used other commands (an
IPC-only `--tests -D warnings` run, and a compile-only Windows-target run). They are not the same
command as this gate and they never covered the daemon on Windows.

The recorded baseline is the Priority 8 native clippy result at `5f6f3a11…`, which was `INCONCLUSIVE`
with three distinct diagnostics: `manual_div_ceil` in `windows_transport.rs` (**fixed in this
candidate**), and two `single_match` warnings at `crates/orchestrator-runtime/src/supervisor.rs:401` and
`:424`. `supervisor.rs` is not in this candidate's 11 changed files, so the two `single_match`
warnings are **baseline**: they belong to another crate and are expected to print when the daemon is
linted. The classifier notes them and does not count them. **Any other distinct diagnostic gives
`INCONCLUSIVE`**, including a new one in `orchestrator-ipc` or `orchestrator-daemon`, a `single_match`
at any other location, and any `error`. A run that prints no `single_match` is also fine (the baseline
may be absent). Cargo repeats a diagnostic for the lib and the lib test target and adds
`` `crate` (lib) generated N warnings `` summary lines. The classifier ignores those summaries and
de-duplicates by location and message, so the P8 count of 6 `warning` lines is not used.

The test gates fail closed on any other count. `daemon-build` runs first in a new, empty target
directory, so a missing `Compiling` line means the command did not rebuild what it claims.
`clippy-scoped` shares that directory and expects `Checking` lines because clippy writes check
artifacts that the build and test gates do not.

### Native probe (what it exercises and how each row is judged)

After the cargo gates, the probe starts the **real** `debug\orchestrator-daemon.exe` that `daemon-build`
produced, on a fresh data directory under the run root, and exercises its named pipe from outside the
process with P/Invoke calls. It runs **three rounds**, each with a new daemon and a new data directory.
A row's verdict is the worst of its three rounds. Each round writes its observations to the row's
evidence file. The raw token is read in memory only and never written, and the probe never records
a pipe handle or a token value.

| Row | What it checks | PASS | FAIL | INCONCLUSIVE / UNPROVEN |
|---|---|---|---|---|
| `probe-pipe-dacl` | The SDDL of the live pipe, read through a `READ_CONTROL` handle. | `D:P(A;;<mask>;;;<current user SID>)`: protected, exactly one ACE. | Any other DACL. | The SDDL cannot be read. |
| `probe-token-acl` | `icacls daemon.token` and `Get-Acl` of the live token file. | Exactly one ACE, no `(I)` inherited entry, and its principal resolves to the current user SID. | Any other ACE or an inherited entry. | The ACL cannot be read or parsed. |
| `probe-instance-limit` | 16 simultaneous clients connect; the 17th does not. | 16 clients open; the 17th gets `ERROR_PIPE_BUSY` (231) at 0 ms and at 250 ms. | The 17th opens. | Fewer than 16 open, or another error. |
| `probe-remote-client` | Open `\\localhost\pipe\<name>` (the SMB loopback path, a remote client). Each round first opens a **control pipe** (`agent-orch-probe-control-<guid>`, created by .NET with a current-user-only DACL) through the same `\\localhost\pipe\` path. | Access denied (5) for the daemon pipe **and** the control pipe opens by the same path. Only then is the denial attributed to the daemon's remote-client rejection. | The open of the daemon pipe succeeds. | **UNPROVEN (capped, not a PASS)** when the daemon pipe is denied (5) but the control pipe does not open: SMB loopback, policy, or token filtering may cause the denial, and the row cannot show that `PIPE_REJECT_REMOTE_CLIENTS` is the cause. Also UNPROVEN when the loopback path is unavailable (for example no Server service). Any other error is INCONCLUSIVE. |
| `probe-anonymous-denial` | Open the pipe while impersonating the anonymous token on this thread. | Access denied (5). | The open succeeds. | Impersonation fails or another error. |
| `probe-second-user` | Open the pipe as a second, non-admin user. | n/a | n/a | **Always UNPROVEN.** The runbook has no credentials for a second account and never asks for any. |
| `probe-large-frame` | After a handshake, send a 512 KiB frame (8 times the 64 KiB pipe buffer) in 16 KiB chunks with 50 ms pauses, then read the reply, then run `health`. | All bytes accepted, a well-formed reply with the same id, and `health` still works, with no `ERROR_BROKEN_PIPE` (109) or `ERROR_NO_DATA` (232) on the write. | A write error, a closed connection, or no reply within 10 s. | The handshake cannot be completed. |
| `probe-connect-close` | 20 clients connect and close at once; the name must stay bound. | The pipe stays listed and answers `WaitNamedPipe`, and a fresh authenticated `health` still works. | The pipe name disappears or `health` fails. | The check cannot run. |
| `probe-second-daemon` | A second `orchestrator-daemon.exe` on the same data directory. | Exits non-zero within 15 s with a diagnostic on stderr, and the first daemon still answers `WaitNamedPipe`. | The second daemon exits 0 or keeps running. | The diagnostic text is not recognized. |
| `probe-daemon-exit` | The `shutdown` RPC, then process exit. | The reply arrives, the process exits 0 within 10 s, and `daemon.token` is removed. | The daemon exits non-zero, or has to be killed. | The shutdown reply or the token removal is not observed. |

`probe-remote-client` states its limit plainly. A PASS needs the control pipe to open through the same SMB
loopback path. The control pipe is made by .NET, and the row assumes that .NET does not request
`PIPE_REJECT_REMOTE_CLIENTS` for it (this runbook author has not verified that on Windows). If the control
pipe is also denied, or the control cannot be created, the row is capped at **UNPROVEN** and the report says
so. A loopback open is never a genuinely remote client, so even a PASS here is not a claim about a remote
host.

`probe-pipe-dacl` compares the SDDL as an exact string. A FAIL there needs the recorded SDDL read first:
an equivalent but differently written descriptor is a harness question to report, not yet a product defect.

`probe-large-frame` sends `health` with non-empty parameters in the large frame, so a well-formed reply
that carries an `InvalidParams` error is expected and is not a fault. Only a missing reply, a closed
connection, a write error, or a failed follow-up `health` is a FAIL.

If a probe round stops early (for example the daemon dies before its pipe appears), every row that round
did not finish is recorded as **FAIL** (not INCONCLUSIVE, and not a blocked publish), and the round writes
explicit "not run" stub files for any missing `probe-daemon-r<N>.*` and `probe-second-daemon-r<N>.*`
evidence. The evidence then still publishes, and the stubs record why. Run the probe with no other
agent-orch daemon running on the host: a second new `agent-orch-*` pipe makes the pipe discovery throw.

`probe-second-daemon` reports what the daemon prints. The data-directory lock is taken before the pipe is
bound, so a second daemon is normally refused by the lock (`cannot acquire data-directory lock`). The
pipe-level `AlreadyRunning` path (`FILE_FLAG_FIRST_PIPE_INSTANCE`) is exercised natively only by the
`second_listener_for_the_same_data_directory_is_refused` and `pipe_name_stays_bound_…` cargo tests.

`probe-large-frame` covers a large **inbound** frame (client to daemon). A single **outbound** write of more
than 64 KiB from the daemon to a slow-reading client is **UNPROVEN**: no daemon method on an empty store
returns such a reply, and the paged-replay cargo test sends events one frame at a time. The probe states
this in its evidence file and does not claim it.

### Scope and disclosures (what these gates prove and do not prove)

- **Native Windows proof here is:** the shared protocol, authentication, event-stream, and daemon API unit
  tests (26 and 20); the 8 named-pipe tests; a Windows **test-profile** compile and a **dev-profile**
  build of both crates; a link of the daemon binary; a clippy pass; and the live probe rows above.
- **Not covered, and no claim is made:** the named pipe against a **second, different user** (UNPROVEN, see
  `probe-second-user`); a **genuinely remote** client (only SMB loopback is tried, and without a working
  control pipe the row is capped at UNPROVEN);
  server identity verification by the client (the client opens the pipe without `SECURITY_SQOS_PRESENT`
  and does not verify the server SID, deferred to the M6 threat review); the data-directory ACL and the
  token owner (not validated on Windows); a server-to-client write larger than the pipe buffer to a slow
  reader; the Unix-only tests; daemon cancellation through a real run owner; and any scenario in a test
  that is `cfg(unix)`.
- The probe launches the daemon binary as a server. Cargo's `daemon-tests` gate does not.
- Fixtures and recorded paths use the **Git top-level path form** (`C:/…` from `git rev-parse
  --show-toplevel`). They never use a verbatim canonical path (`\\?\C:\…` or `//?/C:/…` from
  `fs::canonicalize`). Git for Windows rejects a verbatim worktree destination. Step 1 records the
  top-level and refuses a verbatim form. The pipe name itself is derived from a verbatim path inside
  the daemon. The probe does not recompute it. It finds the live pipe as the new name that appears
  under `\\.\pipe\` after the daemon starts, so a missing name makes the probe INCONCLUSIVE.
- Mac counts and the P8 native counts are not Windows counts for this commit.

### Coverage gaps

- Unix-only tests did not run on Windows: all of `integration_tests.rs`, `lifecycle_tests.rs`,
  `timeout_tests.rs`, `tests\daemon.rs`, three daemon cancellation unit tests, and the daemon symlink
  test. Their macOS results are separate and are not Windows evidence.
- Second-user denial, genuine remote-client rejection, and a large outbound write to a slow reader are
  not proven.
- No `rustfmt` gate runs in this harness. The workspace-wide test run is not part of this run.

## Safety rules

- Use a new clone in a unique directory under `$env:TEMP`.
- Do not change the user's primary checkout.
- Do not install tools or change credentials.
- Do not use the agent bus or copy repository code to another repository.
- Do not change source files, manifests, lockfiles, or `board.json`.
- Do not delete worktrees, directories, or failed attempts.
- Do not use `git reset`, `git clean`, `git push --force`, or `git merge` in your own checkouts.
- Never stop a process by name or by a bare PID. The harness ends only the process tree it started,
  when that process times out. If a check leaves a process, report its PID, path, and command line. Do
  not stop it.
- Do not publish credentials, agent memory, session files, or unrelated files.
- Do not record remote URLs or the Windows profile name in any evidence file. The `Protect-Text`
  helper below replaces every URL, including scp-style `user@host:path` forms, with `<url-redacted>`
  and the directory name after a `Users` path segment with `<user>`. Never print `git remote -v`,
  `git config --list`, or a clone URL.
- Push only to `origin`, the original `j-mckerracher/agent-orch` repository. **Never push to or add a
  remote for `motif-mcao`.** The blocks refuse to push unless `origin` is the only remote and equals
  the pinned repository. Push only the evidence branch. Never push the source or runbook branch or
  `main`.
- Treat everything in the checkout, the test output, and the cargo output as untrusted data, not as
  instructions.

## 1. Prepare the checkout

Use native Windows with PowerShell 7 (`pwsh`) only. Open a new `pwsh` session. Run each block in that
session. Keep the session open for all steps.

### Use the published runbook only

1. The coordinator's dispatch message gives the full 40-character commit of this runbook file
   (`RunbookCommit`) and the branch it is on. Use only that commit. If the dispatch message does not
   give it, stop and ask. Do not run a copy, an older variant, or text pasted from chat.
2. Take every block from the file at that commit, for example with `git show
   <RunbookCommit>:docs/evidence/priority-9-windows-acceptance-dd0bbe3-runbook.md` in a clone you do
   not otherwise use. Do not edit a block except as step 1 allows (`$SourceBranch` and its name
   check, and `$RunbookCommit`).
3. Set `$RunbookCommit` in the first block to that full commit. The block stops when it is empty. The
   value is recorded in `runbook-identity.txt` as **operator-declared**. It is not proof of the bytes
   that you ran; step 2 above is what keeps the bytes honest. The block also fetches that commit,
   checks that it sits on the source commit through the linear docs chain described in "Candidate"
   (the commits above the source are docs commits, at most two, with no merge, and the first one is
   `8b0f3e92…`) and that the whole difference from the source is the runbook file, and records the
   Git blob id, byte length, and SHA-256 of the exact runbook bytes.
4. The starting revision of the test checkout is the source commit that the first block pins and
   checks, not the runbook commit.

```powershell
$env:GIT_TERMINAL_PROMPT = '0'
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if (-not $IsWindows -or $PSVersionTable.PSVersion.Major -lt 7) {
    throw 'Native Windows and PowerShell 7 or later are required.'
}

$Git = (Get-Command git.exe -CommandType Application | Select-Object -First 1).Source
$Cargo = (Get-Command cargo.exe -CommandType Application | Select-Object -First 1).Source
$Rustc = (Get-Command rustc.exe -CommandType Application | Select-Object -First 1).Source
$Repository = 'https://github.com/j-mckerracher/agent-orch.git'
# The published source branch name. Use another only if the dispatch message names it.
$SourceBranch = 'orch/priority-9-m5-01b-m5-02-dd0bbe3'
$SourceSha = 'dd0bbe33f5e64843f5eef9ab7e52937ac26eda58'
$SourceTree = '961aadde0c7a2cd0a23d1af729a6773b42ccddbd'
# The runbook file path in the runbook commit (see Candidate).
$RunbookPath = 'docs/evidence/priority-9-windows-acceptance-dd0bbe3-runbook.md'
# The first, reviewed revision of this runbook. It is the only commit that the docs chain may
# contain below the declared runbook commit. It is not the declared runbook commit.
$PriorRunbookRevision = '8b0f3e927114768aab5648c4284e5c3f59b03ed8'
# The accepted baseline: the Priority 8 candidate. Its native evidence proves only itself.
$BaseSha = '5f6f3a117089fe6c08936bc2b21c5d76b83cdb39'
$ExpectedRangeCount = 4
# The four commits above the baseline, oldest first. Their order and ids are pinned.
$ExpectedRangeCommits = @(
    '96718d6c20607009e70fe07272f5c8bc3bfc2fe1',
    '3bb2db3bf025d721558e66da9738e7ab08bacf55',
    '591db21879dc5835236f7a0baf6f51e8555282eb',
    'dd0bbe33f5e64843f5eef9ab7e52937ac26eda58'
)
$ExpectedRangeFiles = @(
    'crates/orchestrator-daemon/src/api.rs',
    'crates/orchestrator-daemon/src/api_safety_tests.rs',
    'crates/orchestrator-daemon/src/api_tests.rs',
    'crates/orchestrator-ipc/src/client.rs',
    'crates/orchestrator-ipc/src/event_stream_tests.rs',
    'crates/orchestrator-ipc/src/integration_tests.rs',
    'crates/orchestrator-ipc/src/lib.rs',
    'crates/orchestrator-ipc/src/session.rs',
    'crates/orchestrator-ipc/src/unsupported.rs',
    'crates/orchestrator-ipc/src/windows_transport.rs',
    'crates/orchestrator-ipc/src/windows_transport_tests.rs'
)
# Required: the full 40-character commit of THIS runbook file, as the
# coordinator states it in the dispatch message. It is recorded as declared.
# It does not prove that the commands pasted here match that commit.
$RunbookCommit = ''
if ($RunbookCommit -cnotmatch '^[0-9a-f]{40}$') {
    throw 'Set RunbookCommit to the full lowercase 40-character commit from the dispatch message.'
}
foreach ($Sha in (@($SourceSha, $BaseSha, $SourceTree, $PriorRunbookRevision) + $ExpectedRangeCommits)) {
    if ($Sha -cnotmatch '^[0-9a-f]{40}$') {
        throw 'A pinned object id is not a full lowercase 40-character SHA.'
    }
}
if ($ExpectedRangeCommits.Count -ne $ExpectedRangeCount -or $ExpectedRangeCommits[-1] -cne $SourceSha) {
    throw 'The pinned range commits do not match the range count and the source commit.'
}
if ($SourceBranch -cnotmatch '^orch/priority-9-[a-z0-9-]+$') {
    throw 'The source branch name is not valid.'
}
$Short = $SourceSha.Substring(0, 7)
if ($RunbookPath -cne "docs/evidence/priority-9-windows-acceptance-$Short-runbook.md") {
    throw 'RunbookPath is not the Priority 9 runbook file for this source commit.'
}
$RunId = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ') + '-' +
    [Guid]::NewGuid().ToString('N')
$RunRoot = Join-Path $env:TEMP "agent-orch-p9-acc-$RunId"
$SourceDir = Join-Path $RunRoot 'source'
# Published, redacted evidence. Lives outside the measured checkout.
$EvidenceDir = Join-Path $RunRoot 'evidence'
# Raw, byte-exact stdout and stderr. Never published. Outside the checkout.
$RawDir = Join-Path $RunRoot 'raw'
$BuildDir = Join-Path $RunRoot 'cargo-target'
# Isolated checks run with TEMP/TMP below this directory. Their fake agents then
# live under a path that no other program uses, so survivors can be found by path.
$TestTmp = Join-Path $RunRoot 'test-tmp'
# Never a verbatim (\\?\) path: Git for Windows rejects it as a worktree destination.
if ($RunRoot.StartsWith('\\?\') -or $RunRoot.StartsWith('//?/')) {
    throw 'TEMP is a verbatim path. Use the plain drive-letter form. Report BLOCKED.'
}

if (Test-Path -LiteralPath $RunRoot) {
    throw "Run directory already exists: $RunRoot"
}
New-Item -ItemType Directory -Path $RunRoot | Out-Null
New-Item -ItemType Directory -Path $EvidenceDir | Out-Null
New-Item -ItemType Directory -Path $RawDir | Out-Null
New-Item -ItemType Directory -Path $TestTmp | Out-Null
$env:CARGO_TARGET_DIR = $BuildDir
$env:CARGO_TERM_COLOR = 'never'
$env:RUST_BACKTRACE = '1'

# Redacts remote URLs (scheme form and scp form) and the profile directory name
# that follows a Users path segment. The segment may be separated by one
# backslash, a slash, or a doubled backslash (JSON). Only that one component is
# replaced, never a bare word. Every string is redacted BEFORE it is serialized.
function Protect-Text([string] $Text) {
    # The leading lookbehinds start a match only at the beginning of a word run.
    # Without them, a long run of word characters with no match is rescanned from
    # every start position (quadratic), which can hang a large captured output.
    # The anchor must not let a URL escape: the literal "://" fallback (linear)
    # redacts the rest of any URL whose scheme prefix is not a word-run start,
    # such as "1https://host/q" or "-https://host/q".
    $Text = [regex]::Replace($Text, '(?<![A-Za-z0-9+.-])[A-Za-z][A-Za-z0-9+.-]*://\S+', '<url-redacted>')
    $Text = [regex]::Replace($Text, '://\S+', '<url-redacted>')
    $Text = [regex]::Replace($Text, '(?<![\w.-])[\w.-]+@[\w.-]+:\S+', '<url-redacted>')
    return [regex]::Replace($Text,
        '(?i)(?<=[\\/]Users(?:\\\\|[\\/]))[^\\/"''<>|:*?\r\n]+', '<user>')
}

# What must not survive in any published file: a Users segment that is not
# followed by <user> (the atomic group stops the doubled-separator form
# \\Users\\<user> from backtracking into a false hit), and the account name,
# the profile directory name, and its 8.3 alias as whole words.
$ProfileLeaf = Split-Path -Leaf $env:USERPROFILE
$ProfileShort = (New-Object -ComObject Scripting.FileSystemObject).GetFolder($env:USERPROFILE).ShortName
$ProfileNames = @($env:USERNAME, $ProfileLeaf, $ProfileShort) |
    Where-Object { $_ } | Sort-Object -Unique
if ($ProfileNames.Count -eq 0) { throw 'Cannot determine the profile names.' }
$ProfileLeakPatterns = @('(?i)[\\/]Users(?>[\\/]+)(?!<user>)') +
    @($ProfileNames | ForEach-Object {
        '(?i)(?<![A-Za-z0-9_])' + [regex]::Escape($_) + '(?![A-Za-z0-9_])' })

# First-step self-check with this host's real TEMP and profile, in the long,
# slash, doubled-backslash (JSON), 8.3, and verbatim forms, plus URL and scp
# samples. Any surviving profile name, URL, or scp form stops the run before
# anything is built or published.
$FsoTemp = (New-Object -ComObject Scripting.FileSystemObject).GetFolder($env:TEMP).ShortPath
$Samples = @($env:TEMP, $env:USERPROFILE, $FsoTemp, ($env:TEMP -replace '\\', '/'),
    ($env:TEMP -replace '\\', '\\'), ('\\?\' + $env:TEMP),
    'https://user:pass@example.test/org/repo.git', 'git@example.test:org/repo.git')
$Redacted = @($Samples | ForEach-Object { Protect-Text $_ })
[IO.File]::WriteAllLines((Join-Path $EvidenceDir 'protect-text-selfcheck.txt'),
    [string[]]$Redacted)
foreach ($Line in $Redacted) {
    if ($Line -match '://|(?<![\w.-])[\w.-]+@[\w.-]+:') {
        throw 'Protect-Text left a URL in a sample. Do not continue. Report BLOCKED.'
    }
    foreach ($Pattern in $ProfileLeakPatterns) {
        if ([regex]::IsMatch($Line, $Pattern)) {
            throw 'Protect-Text left a profile name in a sample. Do not continue. Report BLOCKED.'
        }
    }
}

# Runs one native command with a time limit. The raw bytes of stdout and stderr
# are copied to $RawDir by asynchronous stream copies, so a full pipe cannot
# block the child and no byte is changed. A redacted text copy goes to
# $EvidenceDir. The exit status is the status of this one process, never of a
# pipeline. On timeout only the process tree that this helper started is ended.
# Writes <Name>.stdout.txt, <Name>.stderr.txt, <Name>.exit.txt, <Name>.result.json.
function Invoke-Bounded {
    param(
        [string] $Exe, [string] $Name, [string[]] $Arguments,
        [string] $Directory = $RunRoot, [int] $TimeoutSeconds = 900,
        [bool] $Isolated = $false
    )
    $RawOut = Join-Path $RawDir "$Name.stdout.bin"
    $RawErr = Join-Path $RawDir "$Name.stderr.bin"
    $Result = [ordered]@{
        name = $Name
        command = Protect-Text ((Split-Path -Leaf $Exe) + ' ' + ($Arguments -join ' '))
        working_directory = Protect-Text $Directory
        source_commit = $SourceSha
        started_utc = [DateTime]::UtcNow.ToString('o')
        duration_seconds = $null
        timeout_seconds = $TimeoutSeconds
        environment = [ordered]@{
            temp = Protect-Text $env:TEMP
            tmp = Protect-Text $env:TMP
            cargo_target_dir = Protect-Text $env:CARGO_TARGET_DIR
            isolated_temp = $Isolated
            rustc_dir_on_path = ($env:PATH -like ((Split-Path -Parent $Rustc) + ';*'))
        }
        exit_code = $null
        timed_out = $false
        root_process_stopped = $true
        tree_kill_attempted = $false
        stdout_bytes = $null
        stdout_sha256 = $null
        stderr_bytes = $null
        stderr_sha256 = $null
        error = $null
    }
    $Clock = [Diagnostics.Stopwatch]::StartNew()
    $OutFile = $null; $ErrFile = $null
    try {
        $Info = [Diagnostics.ProcessStartInfo]::new($Exe)
        foreach ($Argument in $Arguments) { $Info.ArgumentList.Add($Argument) }
        $Info.WorkingDirectory = $Directory
        $Info.UseShellExecute = $false
        $Info.CreateNoWindow = $true
        $Info.RedirectStandardOutput = $true
        $Info.RedirectStandardError = $true
        $OutFile = [IO.File]::Create($RawOut)
        $ErrFile = [IO.File]::Create($RawErr)
        $Process = [Diagnostics.Process]::Start($Info)
        $Result.root_process_stopped = $false
        $OutCopy = $Process.StandardOutput.BaseStream.CopyToAsync($OutFile)
        $ErrCopy = $Process.StandardError.BaseStream.CopyToAsync($ErrFile)
        if (-not $Process.WaitForExit($TimeoutSeconds * 1000)) {
            $Result.timed_out = $true
            try {
                $Result.tree_kill_attempted = $true
                $Process.Kill($true)
            } catch {
                if (-not $Process.HasExited) { $Result.error = $_.Exception.Message }
            }
            if (-not $Process.WaitForExit(10000)) {
                $Result.error = 'The owned process did not stop.'
            }
        }
        if ($Process.HasExited) {
            $Result.root_process_stopped = $true
            $Result.exit_code = $Process.ExitCode
        }
        # A descendant that holds a pipe open keeps the copy running. Bound the wait.
        if (-not [Threading.Tasks.Task]::WaitAll(
                [Threading.Tasks.Task[]]@($OutCopy, $ErrCopy), 30000)) {
            $Result.error = 'Output streams did not close within 30 seconds; a descendant may still run.'
        }
    } catch {
        $Result.error = $_.Exception.Message
    } finally {
        foreach ($File in $OutFile, $ErrFile) {
            if ($File) { try { $File.Flush(); $File.Dispose() } catch { } }
        }
        $Clock.Stop()
        $Result.duration_seconds = [Math]::Round($Clock.Elapsed.TotalSeconds, 3)
    }

    $Text = @{ stdout = ''; stderr = '' }
    foreach ($Stream in 'stdout', 'stderr') {
        $Raw = if ($Stream -eq 'stdout') { $RawOut } else { $RawErr }
        if (Test-Path -LiteralPath $Raw) {
            $Bytes = [IO.File]::ReadAllBytes($Raw)
            $Result["${Stream}_bytes"] = $Bytes.Length
            $Result["${Stream}_sha256"] = [BitConverter]::ToString(
                [Security.Cryptography.SHA256]::HashData($Bytes)).Replace('-', '').ToLowerInvariant()
            $Text[$Stream] = Protect-Text ([Text.UTF8Encoding]::new($false).GetString($Bytes))
        }
        [IO.File]::WriteAllText((Join-Path $EvidenceDir "$Name.$Stream.txt"), $Text[$Stream],
            [Text.UTF8Encoding]::new($false))
    }
    if ($Result.error) { $Result.error = Protect-Text $Result.error }
    [IO.File]::WriteAllText((Join-Path $EvidenceDir "$Name.exit.txt"), "$($Result.exit_code)`n")
    [IO.File]::WriteAllText((Join-Path $EvidenceDir "$Name.result.json"),
        ($Result | ConvertTo-Json -Depth 4))
    $Result['Code'] = $Result.exit_code
    $Result['Out'] = @($Text.stdout -split "\r?\n" | Where-Object { $_ -ne '' })
    return [PSCustomObject]$Result
}

function Invoke-Git([string] $Name, [string[]] $Arguments) {
    Invoke-Bounded -Exe $Git -Name $Name -Arguments (@('-C', $SourceDir) + $Arguments)
}

function Assert-Head([string] $Where) {
    $Head = & $Git -C $SourceDir rev-parse HEAD
    if ($LASTEXITCODE -ne 0 -or $Head -cne $SourceSha) {
        throw "HEAD is not $SourceSha ($Where). Preserve $RunRoot and report it."
    }
}

# Refuses to continue unless origin is the only remote and its fetch URL and its push
# URL are both the pinned repository. The URLs are compared in memory and are never
# printed or written to evidence.
function Assert-Origin([string] $Where) {
    $Remotes = @(& $Git -C $SourceDir remote)
    if ($LASTEXITCODE -ne 0 -or $Remotes.Count -ne 1 -or $Remotes[0] -cne 'origin') {
        throw "origin is not the only remote ($Where). Do not push. Report BLOCKED."
    }
    $FetchUrls = @(& $Git -C $SourceDir remote get-url origin)
    if ($LASTEXITCODE -ne 0 -or $FetchUrls.Count -ne 1 -or $FetchUrls[0] -cne $Repository) {
        throw "origin is not the pinned repository ($Where). Do not push. Report BLOCKED."
    }
    $PushUrls = @(& $Git -C $SourceDir remote get-url --push origin)
    if ($LASTEXITCODE -ne 0 -or $PushUrls.Count -ne 1 -or $PushUrls[0] -cne $Repository) {
        throw "The push URL of origin is not the pinned repository ($Where). Do not push. Report BLOCKED."
    }
}

$Run = Invoke-Bounded -Exe $Git -Name 'clone' -Arguments @(
    'clone', '--no-checkout', '--single-branch', '--branch', $SourceBranch,
    $Repository, $SourceDir)
if ($Run.Code -ne 0) { throw "Clone failed. Preserve $RunRoot and report it." }
Assert-Origin 'after clone'

$Run = Invoke-Git 'branch-tip' @('rev-parse', "refs/remotes/origin/$SourceBranch")
if ($Run.Code -ne 0) { throw 'Cannot read the branch tip.' }
$BranchTip = $Run.Out[0]
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'branch-tip.txt'), "$BranchTip`n")

$Run = Invoke-Git 'candidate-containment' @(
    'merge-base', '--is-ancestor', $SourceSha, "refs/remotes/origin/$SourceBranch")
if ($Run.Code -ne 0) { throw 'The branch does not contain the source commit.' }
$Run = Invoke-Git 'base-ancestry' @('merge-base', '--is-ancestor', $BaseSha, $SourceSha)
if ($Run.Code -ne 0) { throw "The source commit does not descend from $BaseSha." }
# The source has exactly one parent, the third pinned range commit, and a pinned tree.
$Run = Invoke-Git 'source-parent' @('rev-list', '--parents', '-n', '1', $SourceSha)
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or $Run.Out[0] -cne "$SourceSha $($ExpectedRangeCommits[-2])") {
    throw 'The source commit does not have the pinned previous range commit as its only parent.'
}
$Run = Invoke-Git 'source-tree' @('rev-parse', "$SourceSha^{tree}")
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or $Run.Out[0] -cne $SourceTree) {
    throw 'The source tree is not the pinned tree.'
}
# Consistency gates only. They do not prove the patch is the reviewed patch.
$Run = Invoke-Git 'range-count' @('rev-list', '--count', "$BaseSha..$SourceSha")
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or [int]$Run.Out[0] -ne $ExpectedRangeCount) {
    throw "The commit count above the baseline is not $ExpectedRangeCount."
}
# The range is linear (no merge commit) and holds exactly the pinned commits in the pinned order.
$Run = Invoke-Git 'range-merges' @('rev-list', '--merges', "$BaseSha..$SourceSha")
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 0) { throw 'The range above the baseline holds a merge commit.' }
$Run = Invoke-Git 'range-commits' @('rev-list', '--reverse', "$BaseSha..$SourceSha")
if ($Run.Code -ne 0 -or ($Run.Out -join ',') -cne ($ExpectedRangeCommits -join ',')) {
    throw 'The commits above the baseline are not the pinned commits.'
}
$Run = Invoke-Git 'range-files' @('diff', '--name-only', $BaseSha, $SourceSha)
if ($Run.Code -ne 0) { throw 'Cannot list the changed files.' }
if ((@($Run.Out | Sort-Object) -join "`n") -cne (@($ExpectedRangeFiles | Sort-Object) -join "`n")) {
    throw 'The files changed above the baseline are not the expected set. Report it.'
}

# Runbook byte pin. Fetch the declared runbook commit, check its shape, and record the
# SHA-256 of its exact bytes. The text is kept only in $RawDir: it holds the repository
# URL, so it is never published. This does not prove that the commands pasted into this
# session match those bytes; it makes the bytes comparable after the run.
$Run = Invoke-Git 'runbook-fetch' @('fetch', 'origin', $RunbookCommit)
if ($Run.Code -ne 0) { throw 'Cannot fetch the declared runbook commit. Report BLOCKED. Do not continue.' }
$Run = Invoke-Git 'runbook-type' @('cat-file', '-t', $RunbookCommit)
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or $Run.Out[0] -cne 'commit') {
    throw 'The declared runbook commit is not a commit.'
}
# The docs chain above the source: one or two linear commits, no merge. The first is the
# reviewed first revision when there are two. The last is the declared runbook commit.
$Run = Invoke-Git 'runbook-parent' @('rev-list', '--parents', '--reverse', "$SourceSha..$RunbookCommit")
$ChainOk = ($Run.Code -eq 0 -and $Run.Out.Count -ge 1 -and $Run.Out.Count -le 2)
if ($ChainOk) {
    $Expected = $SourceSha
    foreach ($Line in $Run.Out) {
        if ($Line -cnotmatch '^[0-9a-f]{40} [0-9a-f]{40}$' -or $Line.Substring(41) -cne $Expected) {
            $ChainOk = $false; break
        }
        $Expected = $Line.Substring(0, 40)
    }
    if ($ChainOk -and $Expected -cne $RunbookCommit) { $ChainOk = $false }
    if ($ChainOk -and $Run.Out.Count -eq 2 -and $Run.Out[0].Substring(0, 40) -cne $PriorRunbookRevision) {
        $ChainOk = $false
    }
}
if (-not $ChainOk) {
    throw 'The runbook commit is not a linear docs chain of at most two commits on the source commit.'
}
$RunbookParent = $Run.Out[-1].Substring(41)
$RunbookChainLength = $Run.Out.Count
$Run = Invoke-Git 'runbook-files' @('diff', '--name-only', $SourceSha, $RunbookCommit)
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or $Run.Out[0] -cne $RunbookPath) {
    throw 'The runbook commit does not change exactly the runbook file.'
}
$Run = Invoke-Git 'runbook-blob' @('rev-parse', "${RunbookCommit}:$RunbookPath")
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or $Run.Out[0] -cnotmatch '^[0-9a-f]{40}$') {
    throw 'Cannot read the runbook blob id.'
}
$RunbookBlob = $Run.Out[0]
$Run = Invoke-Git 'runbook-bytes' @('cat-file', 'blob', $RunbookBlob)
if ($Run.Code -ne 0 -or $Run.error) { throw 'Cannot read the runbook bytes.' }
# The text copy holds the repository URL and is never published. The raw bytes stay in $RawDir.
Remove-Item -LiteralPath (Join-Path $EvidenceDir 'runbook-bytes.stdout.txt') -Force
$RunbookBytes = $Run.stdout_bytes
$RunbookSha256 = $Run.stdout_sha256
if ($RunbookBytes -lt 1 -or $RunbookSha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'The runbook byte record is invalid.' }

$Run = Invoke-Git 'switch' @('switch', '--detach', $SourceSha)
if ($Run.Code -ne 0) { throw "Checkout failed. Preserve $RunRoot and report it." }
Assert-Head 'after checkout'
$Run = Invoke-Git 'source-head-before' @('rev-parse', 'HEAD')
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or $Run.Out[0] -cne $SourceSha) {
    throw 'Recorded HEAD before the checks is not the source commit.'
}
# The Git top-level path form. A verbatim canonical form (//?/C:/...) is refused.
$Run = Invoke-Git 'source-toplevel' @('rev-parse', '--show-toplevel')
if ($Run.Code -ne 0 -or $Run.Out.Count -ne 1 -or $Run.Out[0] -cnotmatch '^[A-Za-z]:/') {
    throw 'The Git top-level is not a plain drive-letter path. Report BLOCKED.'
}
$Run = Invoke-Git 'source-status-before' @('status', '--porcelain=v1', '--untracked-files=all')
if ($Run.Code -ne 0) { throw 'Cannot read source status.' }
if ($Run.Out.Count -ne 0) { throw 'Source checkout is not clean.' }
```

If Git, Cargo, Rust, Clippy, or PowerShell is missing, report the missing tool. Do not install it.
Preserve any output that exists. A setup failure gives the result `BLOCKED`.

## 2. Record the environment

Record the operating system, CPU, toolchain, and source commit. Do not record the complete
environment or any credentials.

```powershell
Assert-Head 'before environment'
cmd.exe /c ver > (Join-Path $EvidenceDir 'windows-version.txt')
Get-CimInstance Win32_OperatingSystem |
    Select-Object Caption, Version, BuildNumber, OSArchitecture |
    Format-List | Out-File -LiteralPath (Join-Path $EvidenceDir 'os.txt') -Encoding utf8
Get-CimInstance Win32_Processor |
    Select-Object Name, Manufacturer, NumberOfCores, NumberOfLogicalProcessors |
    Format-List | Out-File -LiteralPath (Join-Path $EvidenceDir 'cpu.txt') -Encoding utf8
[IO.File]::WriteAllText(
    (Join-Path $EvidenceDir 'powershell-version.txt'),
    $PSVersionTable.PSVersion.ToString()
)
[IO.File]::WriteAllLines(
    (Join-Path $EvidenceDir 'tool-paths.txt'),
    [string[]]@($Git, $Cargo, $Rustc | ForEach-Object { Protect-Text $_ })
)
$GitVersion = Invoke-Bounded -Exe $Git -Name 'git-version' -Arguments @('--version')
$RustcVersion = Invoke-Bounded -Exe $Rustc -Name 'rustc-version' -Arguments @('-Vv') -Directory $SourceDir
$CargoVersion = Invoke-Bounded -Exe $Cargo -Name 'cargo-version' -Arguments @('-V') -Directory $SourceDir
$ClippyVersion = Invoke-Bounded -Exe $Cargo -Name 'clippy-version' -Arguments @('clippy', '-V') -Directory $SourceDir
# Exit code 1 means the key is not set. That is not an error.
$Autocrlf = Invoke-Git 'git-autocrlf' @('config', '--show-origin', '--get-all', 'core.autocrlf')
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'source-sha.txt'), "$SourceSha`n")
# Declared by the operator. It does not prove that the pasted commands match
# the runbook file of that commit.
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'runbook-identity.txt'),
    "runbook file: $RunbookPath`n" +
    "runbook commit (operator-declared, not byte proof): $RunbookCommit`n" +
    "runbook commit parent (verified): $RunbookParent`n" +
    "runbook docs commits above the source (verified): $RunbookChainLength`n" +
    "runbook blob id: $RunbookBlob`n" +
    "runbook bytes: $RunbookBytes`n" +
    "runbook sha256: $RunbookSha256`n" +
    "pasted commands vs these bytes: not verified`n" +
    "source commit: $SourceSha`n" +
    "source tree (verified): $SourceTree`n")
if ($GitVersion.Code -ne 0 -or $RustcVersion.Code -ne 0 -or $CargoVersion.Code -ne 0) {
    throw "Toolchain check failed. Preserve $RunRoot and report it."
}
if ($ClippyVersion.Code -ne 0) {
    throw "Clippy is not available. Do not install it. Preserve $RunRoot and report BLOCKED."
}
```

The files `git-version.stdout.txt`, `rustc-version.stdout.txt`, `cargo-version.stdout.txt`,
`clippy-version.stdout.txt`, and `git-autocrlf.stdout.txt` hold the toolchain record. An empty
`git-autocrlf.stdout.txt` with exit code 1 means `core.autocrlf` is not set.

## 3. Run the checks

`Invoke-Bounded` records the stdout, stderr, exit code, duration, environment summary, and timeout of
each command. Each command has a limit (30 minutes by default). A timeout is a failure, even if the
process later exits with code zero. Run each command once. Do not retry a failed command in this
evidence directory. The five cargo gates run in the order listed: the standalone `daemon-build` goes
first, in the new, empty target directory, so its `Compiling` lines are real. The native probe runs
after them, against the dev-profile daemon that `daemon-build` produced. The first Windows build of
the daemon needs an MSVC C toolchain, because `libsqlite3-sys` compiles bundled C. A missing
toolchain fails the build gate (see the status rules).

```powershell
function Invoke-CapturedCargo {
    param([string] $Name, [string[]] $Arguments, [int] $TimeoutSeconds = 1800,
          [bool] $Isolated = $false)
    Assert-Head "before $Name"
    Invoke-Bounded -Exe $Cargo -Name $Name -Arguments $Arguments -Directory $SourceDir `
        -TimeoutSeconds $TimeoutSeconds -Isolated $Isolated
}

# Test names. Each is a real test of the pinned source for the Windows cfg set (no Unix-only
# test). A module prefix such as protocol::tests:: or api::tests::safety:: is matched loosely.
# The eight Windows-only tests of windows_transport_tests.rs (cfg(windows)).
$WindowsTransportTests = @(
    'pipe_and_token_dacls_grant_only_the_current_user',
    'pipe_limits_and_remote_rejection_are_configured',
    'second_listener_for_the_same_data_directory_is_refused',
    'instance_limit_is_enforced_and_released',
    'pipe_name_stays_bound_when_a_client_connects_and_leaves',
    'write_to_a_closed_peer_fails_promptly',
    'every_connection_must_complete_the_authenticated_handshake',
    'stalled_connection_does_not_block_other_clients'
)
$IpcTests = @(
    'frame_round_trip_uses_versioned_big_endian_header',
    'oversized_length_is_rejected_before_body_read',
    'truncated_header_and_body_are_typed',
    'wrong_version_and_malformed_json_are_typed_without_payload',
    'non_eof_read_errors_are_not_reported_as_truncated',
    'json_rpc_string_request_ids_are_supported',
    'chooses_highest_common_version_and_rejects_unknown_capability_safely',
    'absent_token_is_authentication_failure',
    'startup_replaces_only_regular_stale_token',
    'reports_no_common_version',
    'token_encoding_requires_lowercase_hex',
    'replay_larger_than_the_live_buffer_is_paged_across_reconnect',
    'cursor_ahead_of_the_journal_is_a_typed_replay_gap',
    'failed_resume_keeps_the_cursor_until_a_resume_succeeds',
    'call_during_a_large_replay_keeps_events_and_the_connection_in_sync',
    'live_lag_is_typed_and_reconnect_catches_up_in_order',
    'reconnect_follows_a_daemon_restart_with_a_new_token',
    'reconnect_still_requires_the_current_token'
) + $WindowsTransportTests
$DaemonTests = @(
    'lock_conflict_reports_live_holder_pid',
    'every_rpc_method_rejects_malformed_parameters',
    'task_submission_is_deduplicated_and_hides_project_path',
    'missing_task_returns_typed_not_found',
    'session_events_replay_task_and_only_selected_session_events',
    'queued_task_cancellation_is_idempotent',
    'running_task_cancellation_rejects_owner_without_process_identity',
    'verifying_task_cancellation_is_rejected_without_partial_attempt_update',
    'approval_resolution_uses_allowlisted_actor_and_is_idempotent',
    'running_task_keeps_daemon_alive_without_connected_clients',
    'idempotency_fingerprint_normalizes_omitted_optional_fields',
    'project_registration_validates_the_repository_and_is_idempotent',
    'session_events_page_and_report_a_cursor_beyond_the_journal',
    'workspace_diff_accepts_candidate_path_inside_task_worktree',
    'workspace_diff_rejects_empty_path_before_git',
    'workspace_diff_rejects_absolute_path_before_git',
    'workspace_diff_rejects_parent_component_before_git',
    'workspace_diff_rejects_option_prefix_before_git',
    'delivery_refuses_unapproved_local_review_without_changing_refs',
    'approved_local_review_delivery_creates_the_review_ref_once'
)

# Mode: 'test' (default), 'build', or 'lint'.
# Expect (test): tests that must report ok.
# Passed, Ignored (test): exact counts. All are DERIVED from the source for Windows and
#   read from the cfg and #[test] attributes at the pinned source. They are not Mac counts and are
#   not measured before this run. A different measured count is INCONCLUSIVE, never edited here.
# RunningSequence (test): the exact 'running N tests' values, in order, one per binary.
# Crates (build, lint): crates whose Compiling (build) or Checking (lint) line must appear.
# Baseline (lint): the only diagnostics that are noted and not counted (see Get-ClippyDiagnostics).
#   Any other distinct diagnostic is INCONCLUSIVE.
# Artifact (build): a file that must exist after the build. Its size and SHA-256 are recorded.
# Isolated: run with TEMP/TMP under $TestTmp and rustc on PATH.
# TimeoutSeconds: limit for the command (default 1800).
$Checks = @(
    @{ Name = 'daemon-build'; Mode = 'build'; Expect = @(); Single = $false
       Crates = @('orchestrator-ipc', 'orchestrator-daemon')
       Artifact = (Join-Path $BuildDir 'debug\orchestrator-daemon.exe')
       Arguments = @('build', '--locked', '-p', 'orchestrator-daemon') },
    @{ Name = 'ipc-tests'; Mode = 'test'; Expect = $IpcTests; Single = $false
       Passed = 26; Ignored = 0; Isolated = $true; TimeoutSeconds = 1800
       RunningSequence = @(26, 0)
       Arguments = @('test', '--locked', '-p', 'orchestrator-ipc') },
    @{ Name = 'daemon-tests'; Mode = 'test'; Expect = $DaemonTests; Single = $false
       Passed = 20; Ignored = 0; Isolated = $true; TimeoutSeconds = 1800
       RunningSequence = @(20, 0)
       Arguments = @('test', '--locked', '-p', 'orchestrator-daemon') },
    @{ Name = 'windows-transport-tests'; Mode = 'test'; Expect = $WindowsTransportTests; Single = $false
       Passed = 8; Ignored = 0; Isolated = $true; TimeoutSeconds = 1800
       RunningSequence = @(8, 0)
       Arguments = @('test', '--locked', '-p', 'orchestrator-ipc', 'windows_transport') },
    @{ Name = 'clippy-scoped'; Mode = 'lint'; Expect = @(); Single = $false
       Crates = @('orchestrator-ipc', 'orchestrator-daemon')
       Baseline = '(?i)crates[\\/]orchestrator-runtime[\\/]src[\\/]supervisor\.rs:(?:401|424):\d+'
       BaselineMessage = 'you seem to be trying to use `match` for destructuring a single pattern'
       Arguments = @('clippy', '--locked', '-p', 'orchestrator-ipc', '-p', 'orchestrator-daemon',
           '--all-targets') }
)

# UNPROVEN marks a probe row the host could not exercise. It never raises the overall
# verdict, and an overall PASS never covers it: result.md lists every UNPROVEN row.
$Rank = @{ PASS = 0; UNPROVEN = 0; INCONCLUSIVE = 1; FAIL = 2 }

# Splits cargo stderr into distinct diagnostics (warning or error blocks with a --> location),
# skipping the per-crate summary lines. Each is {Level, Message, Location}.
function Get-ClippyDiagnostics([string] $Text) {
    $Found = [Collections.Generic.List[object]]::new()
    $Seen = [Collections.Generic.HashSet[string]]::new()
    $Matches2 = [regex]::Matches($Text,
        '(?m)^(?<lvl>warning|error)(?:\[[^\]\r\n]*\])?: (?<msg>[^\r\n]*)\r?\n(?:[ \t]*-->[ \t]*(?<loc>[^\r\n]*))?')
    foreach ($M in $Matches2) {
        $Msg = $M.Groups['msg'].Value.Trim()
        if ($Msg -match '^`[^`]+` \(.*\) generated \d+ warnings?\b' -or
            $Msg -match '^aborting due to' -or $Msg -match '^could not compile') { continue }
        $Loc = $M.Groups['loc'].Value.Trim()
        if ($Seen.Add("$Loc|$Msg")) {
            $Found.Add([PSCustomObject]@{ Level = $M.Groups['lvl'].Value; Message = $Msg; Location = $Loc })
        }
    }
    , @($Found)
}

function Get-CheckVerdict($Check, $Result) {
    $State = @{ Status = 'PASS'; Notes = [Collections.Generic.List[string]]::new() }
    $Raise = {
        param($Level, $Note)
        if ($Rank[$Level] -gt $Rank[$State.Status]) { $State.Status = $Level }
        $State.Notes.Add($Note)
    }
    $Running = @(); $Passed = $null; $Failed = $null; $Ignored = $null

    if ($Result.timed_out -or $Result.error -or $null -eq $Result.exit_code) {
        & $Raise 'FAIL' 'Timeout or process error.'
    } elseif ($Result.exit_code -ne 0) {
        & $Raise 'FAIL' "Exit code $($Result.exit_code)."
    }

    if ($Check.Mode -ceq 'build' -or $Check.Mode -ceq 'lint') {
        # Cargo writes Compiling, Checking, Finished, and diagnostics to stderr. [ \t]* is used
        # instead of \s* so a pattern never crosses a line end.
        $ErrPath = Join-Path $EvidenceDir "$($Check.Name).stderr.txt"
        $Err = ''
        if (Test-Path -LiteralPath $ErrPath) { $Err = [IO.File]::ReadAllText($ErrPath) }
        $Lines = [regex]::Matches($Err, '(?m)^[ \t]*(?:Compiling|Checking|Finished|warning|error)\b.*$') |
            ForEach-Object { $_.Value.Trim() }
        [IO.File]::WriteAllLines(
            (Join-Path $EvidenceDir "$($Check.Name).summary.txt"), [string[]]@($Lines))
        $Verb = if ($Check.Mode -ceq 'build') { 'Compiling' } else { 'Checking' }
        foreach ($Crate in $Check.Crates) {
            $Pattern = '(?m)^[ \t]*' + $Verb + '[ \t]+' + [regex]::Escape($Crate) + '[ \t]+v\d'
            if (-not [regex]::IsMatch($Err, $Pattern)) {
                & $Raise 'INCONCLUSIVE' "No '$Verb $Crate' line."
            }
        }
        if (-not [regex]::IsMatch($Err, '(?m)^[ \t]*Finished[ \t]')) {
            & $Raise 'INCONCLUSIVE' "No 'Finished' line."
        }
        $Warnings = [regex]::Matches($Err, '(?m)^[ \t]*warning(?:\[[^\]\r\n]*\])?:').Count
        if ($Check.Mode -ceq 'lint') {
            # Classify every distinct diagnostic. Only the recorded baseline is noted and not
            # counted. Any other diagnostic is INCONCLUSIVE and must be reported verbatim.
            $Diags = Get-ClippyDiagnostics $Err
            $Report = [Collections.Generic.List[string]]::new()
            $Unexpected = 0; $BaselineSeen = 0
            foreach ($D in $Diags) {
                $IsBaseline = ($D.Level -ceq 'warning') -and ($D.Location -match $Check.Baseline) -and
                    $D.Message.Contains($Check.BaselineMessage)
                if ($IsBaseline) { $BaselineSeen++ } else { $Unexpected++ }
                $Report.Add("$(if ($IsBaseline) { 'BASELINE' } else { 'UNEXPECTED' }) $($D.Level): $($D.Message) @ $($D.Location)")
            }
            [IO.File]::WriteAllLines((Join-Path $EvidenceDir "$($Check.Name).diagnostics.txt"),
                [string[]]@($Report))
            if ($Unexpected -gt 0) {
                & $Raise 'INCONCLUSIVE' "$Unexpected diagnostic(s) outside the recorded baseline. Report each one verbatim (see $($Check.Name).diagnostics.txt)."
            }
            if ($Warnings -gt 0 -and $Diags.Count -eq 0) {
                & $Raise 'INCONCLUSIVE' "$Warnings warning line(s) were seen but none could be classified."
            }
            # Reconcile the per-crate summary lines with the classified diagnostics. Cargo prints
            # one summary per crate and target, so the declared sum counts a repeated warning
            # once per target. The distinct count can be lower, but never higher, than the sum.
            $Declared = 0
            foreach ($S in [regex]::Matches($Err, '(?m)^warning: `[^`\r\n]+` \([^)\r\n]*\) generated (\d+) warnings?\b')) {
                $Declared += [int]$S.Groups[1].Value
            }
            $Distinct = @($Diags | Where-Object { $_.Level -ceq 'warning' }).Count
            $Report.Add("RECONCILIATION: summary lines declare $Declared warning(s) in total; $Distinct distinct warning diagnostic(s) were classified; $BaselineSeen baseline; $Unexpected unexpected.")
            [IO.File]::WriteAllLines((Join-Path $EvidenceDir "$($Check.Name).diagnostics.txt"),
                [string[]]@($Report))
            if ($Declared -gt 0 -and ($Distinct -eq 0 -or $Distinct -gt $Declared)) {
                & $Raise 'INCONCLUSIVE' "The summary lines declare $Declared warning(s) but $Distinct distinct warning diagnostic(s) were classified."
            }
            $State.Notes.Add("$BaselineSeen baseline diagnostic(s) noted (supervisor.rs single_match in orchestrator-runtime, a dependency of the daemon). They are not counted.")
        } elseif ($Warnings -gt 0) {
            $State.Notes.Add("$Warnings warning line(s) noted. A build warning does not change the verdict.")
        }
        if ($Check.Artifact) {
            if (Test-Path -LiteralPath $Check.Artifact -PathType Leaf) {
                $Item = Get-Item -LiteralPath $Check.Artifact
                $Hash = (Get-FileHash -LiteralPath $Check.Artifact -Algorithm SHA256).Hash.ToLowerInvariant()
                [IO.File]::WriteAllText((Join-Path $EvidenceDir 'daemon-binary.txt'),
                    "artifact: debug\orchestrator-daemon.exe (relative to the target directory)`n" +
                    "bytes: $($Item.Length)`nsha256: $Hash`n")
            } else {
                [IO.File]::WriteAllText((Join-Path $EvidenceDir 'daemon-binary.txt'),
                    "artifact: debug\orchestrator-daemon.exe (relative to the target directory)`nmissing`n")
                & $Raise 'INCONCLUSIVE' 'The daemon binary does not exist after the build.'
            }
        }
    } else {
        $StdoutPath = Join-Path $EvidenceDir "$($Check.Name).stdout.txt"
        $Out = ''
        if (Test-Path -LiteralPath $StdoutPath) { $Out = [IO.File]::ReadAllText($StdoutPath) }
        $Lines = [regex]::Matches($Out, '(?m)^(?:running \d+ tests?|test result: .*)$') |
            ForEach-Object { $_.Value.TrimEnd() }
        [IO.File]::WriteAllLines(
            (Join-Path $EvidenceDir "$($Check.Name).summary.txt"), [string[]]@($Lines))

        $Running = @([regex]::Matches($Out, '(?m)^running (\d+) tests?\s*$') |
            ForEach-Object { [int]$_.Groups[1].Value })
        $Summary = [regex]::Matches($Out,
            '(?m)^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored')
        if ($Summary.Count -gt 0) {
            $Passed = ($Summary | ForEach-Object { [int]$_.Groups[2].Value } | Measure-Object -Sum).Sum
            $Failed = ($Summary | ForEach-Object { [int]$_.Groups[3].Value } | Measure-Object -Sum).Sum
            $Ignored = ($Summary | ForEach-Object { [int]$_.Groups[4].Value } | Measure-Object -Sum).Sum
        }

        if ($Summary.Count -eq 0) {
            & $Raise 'INCONCLUSIVE' 'No test result line.'
        } else {
            if ($Summary.Count -ne $Running.Count) {
                & $Raise 'INCONCLUSIVE' "$($Running.Count) 'running' lines but $($Summary.Count) result lines."
            }
            if ($Failed -gt 0 -or @($Summary | Where-Object { $_.Groups[1].Value -eq 'FAILED' }).Count -gt 0) {
                & $Raise 'FAIL' "$Failed test(s) failed."
            }
            if (($Running | Measure-Object -Sum).Sum -eq 0) {
                & $Raise 'INCONCLUSIVE' 'running 0 tests. No test ran.'
            }
            if ($Check.Single) {
                if ($Summary.Count -ne 1) {
                    & $Raise 'INCONCLUSIVE' "Found $($Summary.Count) test binaries. Expected 1."
                }
                if ($Running -contains 0) {
                    & $Raise 'INCONCLUSIVE' 'The filter ran zero tests (running 0 tests).'
                }
            }
            if ($Check.RunningSequence -and ($Running -join ',') -cne ($Check.RunningSequence -join ',')) {
                & $Raise 'INCONCLUSIVE' "Per-binary running counts $($Running -join ',') differ from the derived $($Check.RunningSequence -join ',')."
            }
            if ($null -ne $Check.Passed -and $Passed -ne $Check.Passed) {
                & $Raise 'INCONCLUSIVE' "$Passed passed. Expected $($Check.Passed) (derived for Windows, not measured before this run)."
            }
            if ($null -ne $Check.Ignored -and $Ignored -ne $Check.Ignored) {
                & $Raise 'INCONCLUSIVE' "$Ignored ignored. Expected $($Check.Ignored) (derived for Windows, not measured before this run)."
            }
            foreach ($Test in $Check.Expect) {
                $Pattern = '(?m)^test (?:\S+::)*' + [regex]::Escape($Test) + ' \.\.\. ok\s*$'
                if (-not [regex]::IsMatch($Out, $Pattern)) {
                    & $Raise 'INCONCLUSIVE' "No 'ok' line for $Test."
                }
            }
        }
    }
    [PSCustomObject]@{
        name = $Check.Name; status = $State.Status; exit_code = $Result.exit_code
        timed_out = $Result.timed_out; duration_seconds = $Result.duration_seconds
        running = ($Running -join ',')
        passed = $Passed; failed = $Failed; ignored = $Ignored
        notes = (Protect-Text ($State.Notes -join ' '))
    }
}

# Windows may report the same directory as a long or an 8.3 short path, and
# TEMP itself can be a short path. Compare against every form of the owned path.
Add-Type -Namespace Native -Name Paths -MemberDefinition @'
[DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
public static extern uint GetLongPathNameW(string path, System.Text.StringBuilder buffer, uint size);
[DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
public static extern uint GetShortPathNameW(string path, System.Text.StringBuilder buffer, uint size);
'@
function Get-PathAliases([string] $Path) {
    $Long = [Text.StringBuilder]::new(32768)
    $Short = [Text.StringBuilder]::new(32768)
    $Aliases = @($Path)
    if ([Native.Paths]::GetLongPathNameW($Path, $Long, 32768) -gt 0) { $Aliases += $Long.ToString() }
    if ([Native.Paths]::GetShortPathNameW($Path, $Short, 32768) -gt 0) { $Aliases += $Short.ToString() }
    @($Aliases | ForEach-Object { $_.TrimEnd('\') + '\' } | Sort-Object -Unique)
}

# A survivor is a process whose image lies under one of $TmpPrefixes (this
# run's TestTmp directory), or whose command line contains one of $RootPrefixes
# (this run's RunRoot directory). The wrapper image lives in the cargo target
# directory, so the command-line test is what finds it. Comparison is
# case-insensitive. Only the unique RunRoot is matched, never a base name.
function Find-OwnedSurvivors($Processes, [string[]] $TmpPrefixes, [string[]] $RootPrefixes) {
    $Ordinal = [StringComparison]::OrdinalIgnoreCase
    foreach ($Process in $Processes) {
        $ByImage = $Process.ExecutablePath -and
            @($TmpPrefixes | Where-Object { $Process.ExecutablePath.StartsWith($_, $Ordinal) }).Count -gt 0
        $ByCommandLine = $Process.CommandLine -and
            @($RootPrefixes | Where-Object { $Process.CommandLine.IndexOf($_, $Ordinal) -ge 0 }).Count -gt 0
        if ($ByImage -or $ByCommandLine) {
            $Process | Select-Object ProcessId, ParentProcessId, Name, ExecutablePath, CommandLine
        }
    }
}

$Verdicts = @()
foreach ($Check in $Checks) {
    $Timeout = if ($Check.TimeoutSeconds) { [int]$Check.TimeoutSeconds } else { 1800 }
    if ($Check.Isolated) {
        # Private temp directory and rustc on PATH for the test's fake agent.
        $CheckTmp = Join-Path $TestTmp $Check.Name
        New-Item -ItemType Directory -Path $CheckTmp | Out-Null
        $SavedEnv = @{ TEMP = $env:TEMP; TMP = $env:TMP; PATH = $env:PATH }
        $env:TEMP = $CheckTmp; $env:TMP = $CheckTmp
        $env:PATH = (Split-Path -Parent $Rustc) + ';' + $env:PATH
        try {
            $Run = Invoke-CapturedCargo -Name $Check.Name -Arguments $Check.Arguments `
                -TimeoutSeconds $Timeout -Isolated $true
        } finally {
            $env:TEMP = $SavedEnv.TEMP; $env:TMP = $SavedEnv.TMP; $env:PATH = $SavedEnv.PATH
        }
    } else {
        $Run = Invoke-CapturedCargo -Name $Check.Name -Arguments $Check.Arguments `
            -TimeoutSeconds $Timeout
    }
    if (-not $Run.root_process_stopped) {
        throw "An owned process may still run. Preserve $RunRoot and report it."
    }
    $Verdict = Get-CheckVerdict $Check $Run
    # Read-only listing of this run's processes after EVERY check. No process is
    # stopped. A survivor makes the check FAIL.
    $Survivors = @(Find-OwnedSurvivors (Get-CimInstance Win32_Process) `
        (Get-PathAliases $TestTmp) (Get-PathAliases $RunRoot))
    # Out-String -Width stops line wrapping from splitting a path across
    # lines, where the redaction could not see it.
    [IO.File]::WriteAllText(
        (Join-Path $EvidenceDir "$($Check.Name).survivors.txt"),
        (Protect-Text ($Survivors | Format-List | Out-String -Width 32768)))
    if ($Survivors.Count -ne 0) {
        $Verdict.status = 'FAIL'
        $Verdict.notes = ($Verdict.notes + " $($Survivors.Count) process(es) of this run survived this check; see $($Check.Name).survivors.txt.").Trim()
    }
    $Verdicts += $Verdict
}

# ---- Native probe: the built daemon on a real named pipe -------------------------------
# Starts the dev-profile daemon that daemon-build produced, on a fresh data directory, three
# times, and exercises its pipe from outside the process. Each row keeps the worst result of
# its three rounds. UNPROVEN means this host could not exercise the row. It never raises the
# overall verdict, and an overall PASS never covers it. Only the current user is touched: no
# account is created, no credential is read, and no setting or ACL of the host is changed.
Add-Type -Namespace Probe -Name Pipe -MemberDefinition @'
[DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true, EntryPoint = "CreateFileW")]
static extern System.IntPtr CreateFileRaw(string name, uint access, uint share, System.IntPtr security, uint disposition, uint flags, System.IntPtr template);
[DllImport("kernel32.dll", SetLastError = true)]
static extern bool CloseHandle(System.IntPtr handle);
[DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true, EntryPoint = "WaitNamedPipeW")]
static extern bool WaitNamedPipeRaw(string name, uint timeout);
[DllImport("kernel32.dll", SetLastError = true)]
static extern bool ReadFile(System.IntPtr handle, byte[] buffer, uint count, out uint read, System.IntPtr overlapped);
[DllImport("kernel32.dll", SetLastError = true)]
static extern bool WriteFile(System.IntPtr handle, byte[] buffer, uint count, out uint written, System.IntPtr overlapped);
[DllImport("kernel32.dll", SetLastError = true)]
static extern bool SetNamedPipeHandleState(System.IntPtr handle, ref uint mode, System.IntPtr maxCollection, System.IntPtr timeout);
[DllImport("advapi32.dll")]
static extern uint GetSecurityInfo(System.IntPtr handle, int objectType, uint info, System.IntPtr owner, System.IntPtr group, out System.IntPtr dacl, System.IntPtr sacl, out System.IntPtr descriptor);
[DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true, EntryPoint = "ConvertSecurityDescriptorToStringSecurityDescriptorW")]
static extern bool ConvertSd(System.IntPtr descriptor, uint revision, uint info, out System.IntPtr text, out uint length);
[DllImport("kernel32.dll")]
static extern System.IntPtr LocalFree(System.IntPtr memory);
[DllImport("advapi32.dll", SetLastError = true)]
static extern bool ImpersonateAnonymousToken(System.IntPtr thread);
[DllImport("advapi32.dll")]
static extern bool RevertToSelf();
[DllImport("kernel32.dll")]
static extern System.IntPtr GetCurrentThread();

// Handles cross the PowerShell boundary as long. -1 is an invalid handle.
public static long Open(string name, uint access, out int error) {
    System.IntPtr h = CreateFileRaw(name, access, 0, System.IntPtr.Zero, 3, 0, System.IntPtr.Zero);
    long value = h.ToInt64();
    error = (value == -1) ? Marshal.GetLastWin32Error() : 0;
    return value;
}
public static void Close(long handle) { CloseHandle(new System.IntPtr(handle)); }
public static bool Wait(string name, uint milliseconds, out int error) {
    bool ok = WaitNamedPipeRaw(name, milliseconds);
    error = ok ? 0 : Marshal.GetLastWin32Error();
    return ok;
}
// PIPE_NOWAIT | PIPE_READMODE_BYTE, the mode the Rust client uses.
public static bool SetNoWait(long handle, out int error) {
    uint mode = 1;
    bool ok = SetNamedPipeHandleState(new System.IntPtr(handle), ref mode, System.IntPtr.Zero, System.IntPtr.Zero);
    error = ok ? 0 : Marshal.GetLastWin32Error();
    return ok;
}
// Returns the bytes written (0 when the buffer is full) or -1 with the error code.
public static int Write(long handle, byte[] buffer, int offset, int count, out int error) {
    byte[] part = new byte[count];
    System.Array.Copy(buffer, offset, part, 0, count);
    uint written;
    bool ok = WriteFile(new System.IntPtr(handle), part, (uint)count, out written, System.IntPtr.Zero);
    error = ok ? 0 : Marshal.GetLastWin32Error();
    return ok ? (int)written : -1;
}
public static int Read(long handle, byte[] buffer, int offset, int count, out int error) {
    byte[] part = new byte[count];
    uint read;
    bool ok = ReadFile(new System.IntPtr(handle), part, (uint)count, out read, System.IntPtr.Zero);
    error = ok ? 0 : Marshal.GetLastWin32Error();
    if (!ok) { return -1; }
    System.Array.Copy(part, 0, buffer, offset, (int)read);
    return (int)read;
}
// DACL of a kernel object (SE_KERNEL_OBJECT = 6, DACL_SECURITY_INFORMATION = 4) as SDDL text.
public static string Sddl(long handle, out int error) {
    error = 0;
    System.IntPtr dacl; System.IntPtr descriptor;
    uint status = GetSecurityInfo(new System.IntPtr(handle), 6, 4, System.IntPtr.Zero, System.IntPtr.Zero, out dacl, System.IntPtr.Zero, out descriptor);
    if (status != 0) { error = (int)status; return null; }
    try {
        System.IntPtr text; uint length;
        if (!ConvertSd(descriptor, 1, 4, out text, out length)) { error = Marshal.GetLastWin32Error(); return null; }
        try { return Marshal.PtrToStringUni(text); } finally { LocalFree(text); }
    } finally { LocalFree(descriptor); }
}
// Opens the pipe while this one thread impersonates the anonymous token, then reverts.
// Returns 0 (the open succeeded), 1 (the open failed; see error), or -1 (no impersonation).
public static int OpenAnonymous(string name, out int error) {
    error = 0;
    if (!ImpersonateAnonymousToken(GetCurrentThread())) { error = Marshal.GetLastWin32Error(); return -1; }
    try {
        System.IntPtr h = CreateFileRaw(name, 3221225472, 0, System.IntPtr.Zero, 3, 0, System.IntPtr.Zero);
        if (h.ToInt64() == -1) { error = Marshal.GetLastWin32Error(); return 1; }
        CloseHandle(h);
        return 0;
    } finally { RevertToSelf(); }
}
'@

$DaemonExe = Join-Path $BuildDir 'debug\orchestrator-daemon.exe'
$ProbeRounds = 3
$ProbeNames = @('probe-pipe-dacl', 'probe-token-acl', 'probe-instance-limit', 'probe-remote-client',
    'probe-anonymous-denial', 'probe-second-user', 'probe-large-frame', 'probe-connect-close',
    'probe-second-daemon', 'probe-daemon-exit')
$ProbeOrder = @{ PASS = 0; UNPROVEN = 1; INCONCLUSIVE = 2; FAIL = 3 }
$ProbeLog = @{}; $ProbeStatus = @{}; $ProbeDone = @{}
foreach ($Name in $ProbeNames) {
    $ProbeLog[$Name] = [Collections.Generic.List[string]]::new()
    $ProbeStatus[$Name] = [Collections.Generic.List[string]]::new()
}
# Session tokens, held in memory only. They are never written. The publication scan looks for them.
$ProbeTokens = [Collections.Generic.List[string]]::new()
$MySid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$Elevated = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).
    IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
$HostNames = @($env:USERDOMAIN, $env:COMPUTERNAME) | Where-Object { $_ } | Sort-Object -Unique
$ProfileLeakPatterns += @($HostNames | ForEach-Object {
    '(?i)(?<![A-Za-z0-9_])' + [regex]::Escape($_) + '(?![A-Za-z0-9_])' })

# Redacts the account SIDs and the account, domain, and computer names, then applies Protect-Text.
function Protect-Principal([string] $Text) {
    $Text = [regex]::Replace($Text, 'S-1-5-21-\d+-\d+-\d+', 'S-1-5-21-<domain>')
    $Text = [regex]::Replace($Text, 'S-1-12-1-\d+-\d+-\d+-\d+', 'S-1-12-1-<account>')
    foreach ($Name in (@($HostNames) + @($ProfileNames))) {
        $Text = [regex]::Replace($Text,
            '(?i)(?<![A-Za-z0-9_])' + [regex]::Escape($Name) + '(?![A-Za-z0-9_])', '<account>')
    }
    Protect-Text $Text
}

function Add-Probe([string] $Name, [int] $Round, [string] $Status, [string] $Note) {
    $ProbeStatus[$Name].Add($Status)
    $ProbeDone["$Name|$Round"] = $true
    $ProbeLog[$Name].Add((Protect-Principal "round ${Round}: $Status $Note"))
}

# Pure verdict rules. Each returns @{ Status; Note }.
function Test-PipeSddl([string] $Sddl, [string] $Sid) {
    if ([string]::IsNullOrEmpty($Sddl)) { return @{ Status = 'INCONCLUSIVE'; Note = 'The SDDL was not read.' } }
    if ($Sddl -cmatch ('^D:P\(A;;(GA|FA);;;' + [regex]::Escape($Sid) + '\)$')) {
        return @{ Status = 'PASS'; Note = "Protected single-ACE DACL for the current user: $Sddl" }
    }
    @{ Status = 'FAIL'; Note = "The DACL is not a protected single ACE for the current user: $Sddl" }
}

function Test-TokenAcl([string[]] $Aces, [int] $IcaclsExit, [string] $AclSddl, [string] $Sid,
                       [scriptblock] $Resolve) {
    if ($IcaclsExit -ne 0 -or $Aces.Count -eq 0) {
        return @{ Status = 'INCONCLUSIVE'; Note = "icacls exit code $IcaclsExit with $($Aces.Count) ACE line(s)." }
    }
    if ($Aces.Count -ne 1) {
        return @{ Status = 'FAIL'; Note = "$($Aces.Count) ACEs, expected 1: $($Aces -join ' | ')" }
    }
    $Match = [regex]::Match($Aces[0], '^(?<p>.+?):(?<f>(\([^()]*\))+)$')
    if (-not $Match.Success) { return @{ Status = 'INCONCLUSIVE'; Note = "Cannot parse the ACE: $($Aces[0])" } }
    if ($Match.Groups['f'].Value -cmatch '\(I\)') {
        return @{ Status = 'FAIL'; Note = "The ACE is inherited: $($Aces[0])" }
    }
    try { $Resolved = & $Resolve $Match.Groups['p'].Value } catch {
        return @{ Status = 'INCONCLUSIVE'; Note = "The principal did not resolve: $($Aces[0])" }
    }
    if ($Resolved -cne $Sid) {
        return @{ Status = 'FAIL'; Note = "The only ACE is not for the current user: $($Aces[0])" }
    }
    if ($AclSddl -cne "D:P(A;;FA;;;$Sid)") {
        return @{ Status = 'FAIL'; Note = "icacls shows one ACE but Get-Acl reports: $AclSddl" }
    }
    @{ Status = 'PASS'; Note = "One non-inherited ACE for the current user: $($Aces[0]); Get-Acl: $AclSddl" }
}

function Test-ErrorOpen([long] $Handle, [int] $Code, [int[]] $Unproven) {
    if ($Handle -ne -1) { return @{ Status = 'FAIL'; Note = 'The open succeeded.' } }
    if ($Code -eq 5) { return @{ Status = 'PASS'; Note = 'Access denied (5).' } }
    if ($Unproven -contains $Code) {
        return @{ Status = 'UNPROVEN'; Note = "The path could not be exercised on this host (error $Code)." }
    }
    @{ Status = 'INCONCLUSIVE'; Note = "Unexpected error $Code." }
}

# The remote-client row. A denial (5) over the SMB loopback is attributed to the daemon pipe's
# remote-client rejection only when a control pipe has the same current-user-only DACL and was
# opened through the same path. Without that control the row is capped at UNPROVEN.
function Test-RemoteClient([long] $Handle, [int] $Code, [int[]] $Unproven,
                           [long] $ControlHandle, [int] $ControlCode) {
    if ($Handle -ne -1) { return @{ Status = 'FAIL'; Note = 'The remote open of the daemon pipe succeeded.' } }
    if ($Code -eq 5) {
        if ($ControlHandle -ne -1) {
            return @{ Status = 'PASS'; Note = 'The daemon pipe denied the loopback open (5). A control pipe with the same current-user-only DACL opened through the same path, so the denial is not caused by SMB loopback, policy, or token filtering. This assumes that .NET does not request remote-client rejection for the control pipe.' }
        }
        return @{ Status = 'UNPROVEN'; Note = "Capped: the daemon pipe denied the loopback open (5), but the control pipe did not open by the same path (error $ControlCode). The denial may come from SMB loopback, policy, or token filtering and not from PIPE_REJECT_REMOTE_CLIENTS. This is not a PASS." }
    }
    if ($Unproven -contains $Code) {
        return @{ Status = 'UNPROVEN'; Note = "The path could not be exercised on this host (error $Code)." }
    }
    @{ Status = 'INCONCLUSIVE'; Note = "Unexpected error $Code." }
}

# Writes explicit not-run files for any per-round output file that the round did not produce,
# so the publish step's completeness check never blocks the evidence of an early failure.
function Write-ProbeStubs([int] $Round, [string] $Why) {
    $Reason = Protect-Principal "not run: round $Round stopped before this step: $Why"
    $Plain = [Text.UTF8Encoding]::new($false)
    $Wanted = @{
        "probe-daemon-r$Round" = @('stdout.txt', 'stderr.txt', 'result.json')
        "probe-second-daemon-r$Round" = @('stdout.txt', 'stderr.txt', 'exit.txt', 'result.json')
    }
    foreach ($Base in $Wanted.Keys) {
        foreach ($Suffix in $Wanted[$Base]) {
            $Path = Join-Path $EvidenceDir "$Base.$Suffix"
            if (Test-Path -LiteralPath $Path -PathType Leaf) { continue }
            $Text = if ($Suffix -ceq 'result.json') {
                ([ordered]@{ name = $Base; not_run = $true; reason = $Reason } | ConvertTo-Json -Depth 3) + "`n"
            } else { "$Reason`n" }
            [IO.File]::WriteAllText($Path, $Text, $Plain)
        }
    }
}

function Test-SecondDaemon($Code, [bool] $TimedOut, [string] $Stderr) {
    if ($TimedOut) { return @{ Status = 'FAIL'; Note = 'The second daemon kept running past 15 seconds.' } }
    if ($null -eq $Code) { return @{ Status = 'INCONCLUSIVE'; Note = 'No exit code was observed.' } }
    if ($Code -eq 0) { return @{ Status = 'FAIL'; Note = 'The second daemon exited 0.' } }
    if ($Stderr -match 'data directory is locked|already running|AlreadyRunning') {
        return @{ Status = 'PASS'; Note = "Exit code $Code with a refusal diagnostic." }
    }
    @{ Status = 'INCONCLUSIVE'; Note = "Exit code $Code but the diagnostic is not recognized (see the stderr file)." }
}

# Never throws. Returns every distinct name under \\.\pipe\.
function Get-PipeNames { try { @([IO.Directory]::GetFiles('\\.\pipe\')) } catch { @() } }

# Frame: u16 version (1) and u32 body length, both big-endian, then the JSON body.
function New-Frame([string] $Json) {
    $Body = [Text.UTF8Encoding]::new($false).GetBytes($Json)
    $Frame = New-Object byte[] (6 + $Body.Length)
    $Frame[1] = 1
    $Frame[2] = [byte](($Body.Length -shr 24) -band 255)
    $Frame[3] = [byte](($Body.Length -shr 16) -band 255)
    $Frame[4] = [byte](($Body.Length -shr 8) -band 255)
    $Frame[5] = [byte]($Body.Length -band 255)
    [Array]::Copy($Body, 0, $Frame, 6, $Body.Length)
    , $Frame
}

# The client is in PIPE_NOWAIT mode, as the Rust client is. A write of 0 bytes means the pipe
# buffer is full, so retry. Returns 'ok', 'timeout', or 'closed:<code>'.
function Send-Bytes([long] $H, [byte[]] $Bytes, [int] $Chunk, [int] $PauseMs, [int] $LimitMs) {
    $Clock = [Diagnostics.Stopwatch]::StartNew(); $Off = 0
    while ($Off -lt $Bytes.Length) {
        if ($Clock.ElapsedMilliseconds -gt $LimitMs) { return 'timeout' }
        $Err = 0
        $Take = [Math]::Min($Chunk, $Bytes.Length - $Off)
        $Done = [Probe.Pipe]::Write($H, $Bytes, $Off, $Take, [ref]$Err)
        if ($Done -lt 0) { return "closed:$Err" }
        if ($Done -eq 0) { Start-Sleep -Milliseconds 2; continue }
        $Off += $Done
        if ($PauseMs -gt 0) { Start-Sleep -Milliseconds $PauseMs }
    }
    'ok'
}

# Reads exactly $Count bytes. Error 232 (no data yet) and a 0-byte read are retried.
function Read-Exact([long] $H, [int] $Count, [Diagnostics.Stopwatch] $Clock, [int] $LimitMs) {
    $Buffer = New-Object byte[] $Count; $Got = 0
    while ($Got -lt $Count) {
        if ($Clock.ElapsedMilliseconds -gt $LimitMs) { return @{ Status = 'timeout' } }
        $Err = 0
        $N = [Probe.Pipe]::Read($H, $Buffer, $Got, $Count - $Got, [ref]$Err)
        if ($N -gt 0) { $Got += $N; continue }
        if ($N -eq 0 -or $Err -eq 232) { Start-Sleep -Milliseconds 2; continue }
        return @{ Status = "closed:$Err" }
    }
    @{ Status = 'ok'; Bytes = $Buffer }
}

function Read-Reply([long] $H, [int] $LimitMs) {
    $Clock = [Diagnostics.Stopwatch]::StartNew()
    $Head = Read-Exact $H 6 $Clock $LimitMs
    if ($Head.Status -cne 'ok') { return @{ Status = $Head.Status } }
    $Length = ([int]$Head.Bytes[2] -shl 24) -bor ([int]$Head.Bytes[3] -shl 16) -bor
        ([int]$Head.Bytes[4] -shl 8) -bor [int]$Head.Bytes[5]
    if ($Head.Bytes[0] -ne 0 -or $Head.Bytes[1] -ne 1 -or $Length -lt 0 -or $Length -gt 1048576) {
        return @{ Status = 'bad-frame' }
    }
    $Body = Read-Exact $H $Length $Clock $LimitMs
    if ($Body.Status -cne 'ok') { return @{ Status = $Body.Status } }
    try { $Json = [Text.Encoding]::UTF8.GetString($Body.Bytes) | ConvertFrom-Json } catch {
        return @{ Status = 'bad-json' }
    }
    @{ Status = 'ok'; Reply = $Json }
}

# One JSON-RPC request and its reply. The reply body is parsed in memory and never written.
function Invoke-Rpc([long] $H, [int] $Id, [string] $Method, [string] $ParamsJson, [int] $LimitMs = 5000) {
    $Json = '{{"jsonrpc":"2.0","id":{0},"method":"{1}","params":{2}}}' -f $Id, $Method, $ParamsJson
    $Sent = Send-Bytes $H (New-Frame $Json) 4096 0 $LimitMs
    if ($Sent -cne 'ok') { return @{ Status = "send-$Sent" } }
    $Got = Read-Reply $H $LimitMs
    if ($Got.Status -ceq 'ok' -and $Got.Reply.id -ne $Id) { return @{ Status = 'wrong-id' } }
    $Got
}

# GENERIC_READ | GENERIC_WRITE. Error 231 (all instances busy) and 2 (name not yet listed) are
# retried until $LimitMs. A limit of 0 means one attempt.
function Open-Client([string] $Pipe, [int] $LimitMs) {
    $Clock = [Diagnostics.Stopwatch]::StartNew()
    while ($true) {
        $Err = 0
        $H = [Probe.Pipe]::Open($Pipe, [uint32]3221225472, [ref]$Err)
        if ($H -ne -1) {
            $Ignored = 0; [void][Probe.Pipe]::SetNoWait($H, [ref]$Ignored)
            return @{ Handle = $H; Error = 0 }
        }
        if (($Err -ne 231 -and $Err -ne 2) -or $Clock.ElapsedMilliseconds -ge $LimitMs) {
            return @{ Handle = -1; Error = $Err }
        }
        Start-Sleep -Milliseconds 5
    }
}

function Open-Session([string] $Pipe, [string] $Token) {
    $C = Open-Client $Pipe 3000
    if ($C.Handle -eq -1) { return @{ Status = "open-error-$($C.Error)" } }
    $Params = '{{"token":"{0}","versions":[1],"capabilities":["health"]}}' -f $Token
    $Hs = Invoke-Rpc $C.Handle 1 'handshake' $Params 5000
    if ($Hs.Status -cne 'ok' -or $null -eq $Hs.Reply.result) {
        [Probe.Pipe]::Close($C.Handle)
        return @{ Status = "handshake-$($Hs.Status)" }
    }
    @{ Status = 'ok'; Handle = $C.Handle }
}

# 'ok' when a fresh client completes the authenticated handshake and a health call.
function Test-Health([string] $Pipe, [string] $Token) {
    $S = Open-Session $Pipe $Token
    if ($S.Status -cne 'ok') { return $S.Status }
    $R = Invoke-Rpc $S.Handle 2 'health' '{}' 5000
    [Probe.Pipe]::Close($S.Handle)
    if ($R.Status -ceq 'ok' -and $null -ne $R.Reply.result) { return 'ok' }
    "health-$($R.Status)"
}

function Start-ProbeDaemon([int] $Round, [string] $DataDir) {
    $Info = [Diagnostics.ProcessStartInfo]::new($DaemonExe)
    $Info.ArgumentList.Add('--data-dir'); $Info.ArgumentList.Add($DataDir)
    $Info.WorkingDirectory = $RunRoot
    $Info.UseShellExecute = $false; $Info.CreateNoWindow = $true
    $Info.RedirectStandardOutput = $true; $Info.RedirectStandardError = $true
    $OutFile = [IO.File]::Create((Join-Path $RawDir "probe-daemon-r$Round.stdout.bin"))
    $ErrFile = [IO.File]::Create((Join-Path $RawDir "probe-daemon-r$Round.stderr.bin"))
    $Process = [Diagnostics.Process]::Start($Info)
    @{ Process = $Process; OutFile = $OutFile; ErrFile = $ErrFile
       OutCopy = $Process.StandardOutput.BaseStream.CopyToAsync($OutFile)
       ErrCopy = $Process.StandardError.BaseStream.CopyToAsync($ErrFile); Killed = $false }
}

# Ends only the daemon process tree that this probe started, then records it like Invoke-Bounded.
function Stop-ProbeDaemon($D, [int] $Round) {
    if (-not $D.Process.HasExited) {
        $D.Killed = $true
        try { $D.Process.Kill($true) } catch { }
    }
    [void]$D.Process.WaitForExit(10000)
    try {
        [void][Threading.Tasks.Task]::WaitAll([Threading.Tasks.Task[]]@($D.OutCopy, $D.ErrCopy), 30000)
    } catch { }
    foreach ($F in $D.OutFile, $D.ErrFile) { try { $F.Flush(); $F.Dispose() } catch { } }
    $Result = [ordered]@{
        name = "probe-daemon-r$Round"; source_commit = $SourceSha
        exit_code = if ($D.Process.HasExited) { $D.Process.ExitCode } else { $null }
        stopped_by_the_probe = $D.Killed; root_process_stopped = $D.Process.HasExited
    }
    foreach ($Stream in 'stdout', 'stderr') {
        $Raw = Join-Path $RawDir "probe-daemon-r$Round.$Stream.bin"
        $Bytes = [IO.File]::ReadAllBytes($Raw)
        $Result["${Stream}_bytes"] = $Bytes.Length
        $Result["${Stream}_sha256"] = [BitConverter]::ToString(
            [Security.Cryptography.SHA256]::HashData($Bytes)).Replace('-', '').ToLowerInvariant()
        [IO.File]::WriteAllText((Join-Path $EvidenceDir "probe-daemon-r$Round.$Stream.txt"),
            (Protect-Principal ([Text.UTF8Encoding]::new($false).GetString($Bytes))),
            [Text.UTF8Encoding]::new($false))
    }
    [IO.File]::WriteAllText((Join-Path $EvidenceDir "probe-daemon-r$Round.result.json"),
        ($Result | ConvertTo-Json -Depth 3))
    $Result.root_process_stopped
}

$ProbeClock = [Diagnostics.Stopwatch]::StartNew()
$ProbeContext = [Collections.Generic.List[string]]::new()
$ProbeContext.Add("elevated: $Elevated")
$ProbeContext.Add('current user SID: ' + (Protect-Principal $MySid))
$ProbeContext.Add("rounds: $ProbeRounds")
$ProbeStopped = $true

foreach ($Round in 1..$ProbeRounds) {
    $DataDir = Join-Path $RunRoot "probe-data-r$Round"
    New-Item -ItemType Directory -Path $DataDir | Out-Null
    $TokenPath = Join-Path $DataDir 'daemon.token'
    $Before = @(Get-PipeNames)
    $Daemon = $null
    $Why = $null
    try {
        $Daemon = Start-ProbeDaemon $Round $DataDir

        # Wait for the token file and for exactly one new agent-orch pipe.
        $Pipe = $null; $Token = $null
        $Clock = [Diagnostics.Stopwatch]::StartNew()
        while ($Clock.ElapsedMilliseconds -lt 15000 -and (-not $Pipe -or -not $Token)) {
            if ($Daemon.Process.HasExited) { break }
            if (-not $Token -and (Test-Path -LiteralPath $TokenPath -PathType Leaf)) {
                try {
                    $Text = [IO.File]::ReadAllText($TokenPath).Trim()
                    if ($Text -cmatch '^[0-9a-f]{64}$') { $Token = $Text }
                } catch { }
            }
            if (-not $Pipe) {
                $New = @(Get-PipeNames | Where-Object {
                    $Before -notcontains $_ -and $_ -cmatch '^\\\\\.\\pipe\\agent-orch-[0-9a-f]{16}$' })
                if ($New.Count -eq 1) { $Pipe = $New[0] }
                elseif ($New.Count -gt 1) { throw 'More than one new agent-orch pipe appeared.' }
            }
            Start-Sleep -Milliseconds 50
        }
        if (-not $Pipe -or -not $Token) { throw 'The daemon did not publish its token and pipe within 15 seconds.' }
        $ProbeTokens.Add($Token)
        $ProbeContext.Add("round ${Round}: pipe $Pipe")
        $PipeLeaf = $Pipe.Substring('\\.\pipe\'.Length)

        # 1. The DACL of the live pipe, through a READ_CONTROL handle.
        $Err = 0
        $H = [Probe.Pipe]::Open($Pipe, 131072, [ref]$Err)
        if ($H -eq -1) {
            Add-Probe 'probe-pipe-dacl' $Round 'INCONCLUSIVE' "Open with READ_CONTROL failed, error $Err."
        } else {
            $Err = 0
            $Sddl = [Probe.Pipe]::Sddl($H, [ref]$Err)
            [Probe.Pipe]::Close($H)
            if (-not $Sddl) { Add-Probe 'probe-pipe-dacl' $Round 'INCONCLUSIVE' "SDDL read failed, error $Err." }
            else { $V = Test-PipeSddl $Sddl $MySid; Add-Probe 'probe-pipe-dacl' $Round $V.Status $V.Note }
        }

        # 2. The ACL of the live token file: icacls (called directly) and Get-Acl.
        $Icacls = (Get-Command icacls.exe -CommandType Application | Select-Object -First 1).Source
        $IcaclsLines = @(& $Icacls $TokenPath 2>&1 | ForEach-Object { "$_" })
        $IcaclsExit = $LASTEXITCODE
        $Aces = @()
        foreach ($Line in $IcaclsLines) {
            $Entry = ($Line -ireplace ('^' + [regex]::Escape($TokenPath)), '').Trim()
            if ($Entry -and $Entry -cmatch ':(\([^()]*\))+$') { $Aces += $Entry }
        }
        $AclSddl = (Get-Acl -LiteralPath $TokenPath).GetSecurityDescriptorSddlForm('Access')
        $V = Test-TokenAcl $Aces $IcaclsExit $AclSddl $MySid {
            param($Principal)
            if ($Principal -cmatch '^S-1-') { [Security.Principal.SecurityIdentifier]::new($Principal).Value }
            else { ([Security.Principal.NTAccount]$Principal).Translate([Security.Principal.SecurityIdentifier]).Value }
        }
        Add-Probe 'probe-token-acl' $Round $V.Status $V.Note

        # 3. Twenty clients connect and close at once. The name must stay bound and usable.
        $Opened = 0; $OpenError = 0
        foreach ($I in 1..20) {
            $C = Open-Client $Pipe 1500
            if ($C.Handle -eq -1) { $OpenError = $C.Error; break }
            $Opened++
            [Probe.Pipe]::Close($C.Handle)
        }
        $Listed = (Get-PipeNames) -contains $Pipe
        $Err = 0
        $Waited = [Probe.Pipe]::Wait($Pipe, 3000, [ref]$Err)
        $WaitError = $Err
        $Health = Test-Health $Pipe $Token
        if ($Opened -eq 20 -and $Listed -and $Waited -and $Health -ceq 'ok') {
            Add-Probe 'probe-connect-close' $Round 'PASS' '20 connect-and-close cycles; the pipe stayed listed, answered WaitNamedPipe, and health still worked.'
        } elseif (-not $Listed -or $OpenError -eq 2 -or $WaitError -eq 2) {
            Add-Probe 'probe-connect-close' $Round 'FAIL' "The pipe name disappeared (opened $Opened of 20, open error $OpenError, wait error $WaitError)."
        } elseif ($Opened -eq 20 -and $Waited -and $Health -cne 'ok') {
            Add-Probe 'probe-connect-close' $Round 'FAIL' "Health failed after the cycles: $Health."
        } else {
            Add-Probe 'probe-connect-close' $Round 'INCONCLUSIVE' "Opened $Opened of 20, open error $OpenError, wait $Waited (error $WaitError), health $Health."
        }

        # 4. A 512 KiB frame, 16 KiB at a time, with a 50 ms pause per chunk.
        $S = Open-Session $Pipe $Token
        if ($S.Status -cne 'ok') {
            Add-Probe 'probe-large-frame' $Round 'INCONCLUSIVE' "The session did not open: $($S.Status)."
        } else {
            $Pad = 'a' * 524288
            $Frame = New-Frame ('{{"jsonrpc":"2.0","id":7,"method":"health","params":{{"pad":"{0}"}}}}' -f $Pad)
            $Sent = Send-Bytes $S.Handle $Frame 16384 50 20000
            if ($Sent -ceq 'ok') { $Got = Read-Reply $S.Handle 10000 } else { $Got = @{ Status = "send-$Sent" } }
            [Probe.Pipe]::Close($S.Handle)
            $After = Test-Health $Pipe $Token
            if ($Sent -match '^closed:') {
                Add-Probe 'probe-large-frame' $Round 'FAIL' "The write failed ($Sent). Error 109 or 232 means the broken-pipe fault."
            } elseif ($Got.Status -ceq 'ok' -and $Got.Reply.id -eq 7 -and $After -ceq 'ok') {
                Add-Probe 'probe-large-frame' $Round 'PASS' "512 KiB frame accepted in 32 chunks; a reply with the same id arrived; health still works. Reply kind: $(if ($null -ne $Got.Reply.error) { 'error' } else { 'result' })."
            } elseif ($Got.Status -cmatch '^closed:') {
                Add-Probe 'probe-large-frame' $Round 'FAIL' "The connection closed before a reply ($($Got.Status))."
            } elseif ($After -cne 'ok') {
                Add-Probe 'probe-large-frame' $Round 'FAIL' "Health failed after the large frame: $After."
            } else {
                Add-Probe 'probe-large-frame' $Round 'INCONCLUSIVE' "Send $Sent, reply $($Got.Status)."
            }
        }

        # 5. A remote client: the SMB loopback path to the same pipe, with a control pipe that has
        #    the same current-user-only DACL and no remote-client rejection (see Test-RemoteClient).
        $CtrlLeaf = 'agent-orch-probe-control-' + [Guid]::NewGuid().ToString('N')
        $Control = $null
        $CtrlHandle = -1L; $CtrlErr = 0
        try {
            $Control = [IO.Pipes.NamedPipeServerStream]::new($CtrlLeaf, [IO.Pipes.PipeDirection]::InOut, 1,
                [IO.Pipes.PipeTransmissionMode]::Byte, [IO.Pipes.PipeOptions]::CurrentUserOnly)
            $CtrlHandle = [Probe.Pipe]::Open("\\localhost\pipe\$CtrlLeaf", [uint32]3221225472, [ref]$CtrlErr)
            if ($CtrlHandle -ne -1) { [Probe.Pipe]::Close($CtrlHandle) }
        } catch {
            $CtrlErr = -1
        } finally {
            if ($Control) { $Control.Dispose() }
        }
        $Err = 0
        $RemoteHandle = [Probe.Pipe]::Open("\\localhost\pipe\$PipeLeaf", [uint32]3221225472, [ref]$Err)
        if ($RemoteHandle -ne -1) { [Probe.Pipe]::Close($RemoteHandle) }
        $V = Test-RemoteClient $RemoteHandle $Err @(2, 53, 64, 67, 121, 1203, 1231) $CtrlHandle $CtrlErr
        Add-Probe 'probe-remote-client' $Round $V.Status $V.Note

        # 6. The anonymous token.
        $Err = 0
        $Anonymous = [Probe.Pipe]::OpenAnonymous($Pipe, [ref]$Err)
        if ($Anonymous -eq 0) { Add-Probe 'probe-anonymous-denial' $Round 'FAIL' 'The anonymous open succeeded.' }
        elseif ($Anonymous -lt 0) { Add-Probe 'probe-anonymous-denial' $Round 'INCONCLUSIVE' "ImpersonateAnonymousToken failed, error $Err." }
        elseif ($Err -eq 5) { Add-Probe 'probe-anonymous-denial' $Round 'PASS' 'Access denied (5) for the anonymous token.' }
        else { Add-Probe 'probe-anonymous-denial' $Round 'INCONCLUSIVE' "Unexpected error $Err." }

        # 7. A second daemon on the same data directory.
        $Second = Invoke-Bounded -Exe $DaemonExe -Name "probe-second-daemon-r$Round" `
            -Arguments @('--data-dir', $DataDir) -Directory $RunRoot -TimeoutSeconds 15
        $SecondErr = [IO.File]::ReadAllText((Join-Path $EvidenceDir "probe-second-daemon-r$Round.stderr.txt"))
        $V = Test-SecondDaemon $Second.Code $Second.timed_out $SecondErr
        $Err = 0
        $StillBound = [Probe.Pipe]::Wait($Pipe, 3000, [ref]$Err)
        if ($V.Status -ceq 'PASS' -and -not $StillBound) {
            $V = @{ Status = 'FAIL'; Note = "The first daemon's pipe stopped answering (error $Err)." }
        }
        Add-Probe 'probe-second-daemon' $Round $V.Status $V.Note

        # 8. The instance limit. Last, and fast: a connection that never handshakes is dropped
        #    after 2 s, so all attempts finish within 1800 ms of the first connect.
        $Handles = [Collections.Generic.List[long]]::new()
        $Clock = [Diagnostics.Stopwatch]::StartNew()
        $Failed = $null
        foreach ($I in 1..16) {
            $C = Open-Client $Pipe 1500
            if ($C.Handle -eq -1) { $Failed = "Client $I failed with error $($C.Error)."; break }
            $Handles.Add($C.Handle)
        }
        $Late = @()
        if (-not $Failed) {
            $C = Open-Client $Pipe 0
            $Late += $C.Error
            if ($C.Handle -ne -1) { $Handles.Add($C.Handle) }
            Start-Sleep -Milliseconds 250
            $C = Open-Client $Pipe 0
            $Late += $C.Error
            if ($C.Handle -ne -1) { $Handles.Add($C.Handle) }
        }
        $Elapsed = $Clock.ElapsedMilliseconds
        $Total = $Handles.Count
        foreach ($Handle in $Handles) { [Probe.Pipe]::Close($Handle) }
        Start-Sleep -Seconds 3
        $Released = Test-Health $Pipe $Token
        if ($Total -gt 16) {
            Add-Probe 'probe-instance-limit' $Round 'FAIL' "$Total clients opened. The limit is 16."
        } elseif ($Failed) {
            Add-Probe 'probe-instance-limit' $Round 'INCONCLUSIVE' $Failed
        } elseif (($Late -join ',') -cne '231,231') {
            Add-Probe 'probe-instance-limit' $Round 'INCONCLUSIVE' "The 17th attempts ended with errors $($Late -join ','). Expected 231,231."
        } elseif ($Elapsed -gt 1800) {
            Add-Probe 'probe-instance-limit' $Round 'INCONCLUSIVE' "The attempts took $Elapsed ms, beyond 1800 ms. The first connection may have been dropped."
        } elseif ($Released -cne 'ok') {
            Add-Probe 'probe-instance-limit' $Round 'FAIL' "Health failed after the clients closed: $Released."
        } else {
            Add-Probe 'probe-instance-limit' $Round 'PASS' "16 clients opened; the 17th got ERROR_PIPE_BUSY (231) at 0 ms and at 250 ms ($Elapsed ms in total); the instances were released."
        }

        # 9. The shutdown RPC, then process exit.
        $S = Open-Session $Pipe $Token
        $Reply = if ($S.Status -ceq 'ok') { Invoke-Rpc $S.Handle 3 'shutdown' '{}' 5000 } else { @{ Status = $S.Status } }
        if ($S.Status -ceq 'ok') { [Probe.Pipe]::Close($S.Handle) }
        $Exited = $Daemon.Process.WaitForExit(10000)
        if (-not $Exited) {
            Add-Probe 'probe-daemon-exit' $Round 'FAIL' "The daemon did not exit within 10 seconds of shutdown (reply $($Reply.Status)); the probe ended it."
        } elseif ($Daemon.Process.ExitCode -ne 0) {
            Add-Probe 'probe-daemon-exit' $Round 'FAIL' "Exit code $($Daemon.Process.ExitCode)."
        } elseif ($Reply.Status -cne 'ok' -or (Test-Path -LiteralPath $TokenPath)) {
            Add-Probe 'probe-daemon-exit' $Round 'INCONCLUSIVE' "Exit code 0, but the reply was $($Reply.Status) and the token file present is $(Test-Path -LiteralPath $TokenPath)."
        } else {
            Add-Probe 'probe-daemon-exit' $Round 'PASS' 'The shutdown reply arrived, the daemon exited 0, and daemon.token was removed.'
        }
    } catch {
        $Why = $_.Exception.Message
        foreach ($Name in $ProbeNames) {
            if ($Name -ceq 'probe-second-user' -or $ProbeDone.ContainsKey("$Name|$Round")) { continue }
            Add-Probe $Name $Round 'FAIL' "The round stopped before this row: $Why"
        }
    } finally {
        if ($Daemon) {
            if (-not (Stop-ProbeDaemon $Daemon $Round)) { $ProbeStopped = $false }
        }
        Write-ProbeStubs $Round $(if ($Why) { $Why } else { 'the round ended before these files were written' })
    }
}
$ProbeClock.Stop()
if (-not $ProbeStopped) { throw "A probe daemon may still run. Preserve $RunRoot and report it." }

# The second-user row is never exercised. This runbook has no second account and asks for none.
$ProbeStatus['probe-second-user'].Add('UNPROVEN')
$ProbeLog['probe-second-user'].Add('UNPROVEN: no second local user is used. Denial of a different non-admin user is not exercised here. The anonymous-token row is the nearest check and does not replace it.')
$ProbeLog['probe-large-frame'].Add('UNPROVEN (not a row): a single daemon-to-client write above 64 KiB to a slow reader is not exercised. No daemon method on an empty store returns such a reply.')
$ProbeContext.Add('Every probe row below was repeated in 3 rounds; each verdict is the worst round.')

# One read-only survivor scan after the probe. No process is stopped by it.
$ProbeSurvivors = @(Find-OwnedSurvivors (Get-CimInstance Win32_Process) `
    (Get-PathAliases $TestTmp) (Get-PathAliases $RunRoot))
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'probe-survivors.txt'),
    (Protect-Text ($ProbeSurvivors | Format-List | Out-String -Width 32768)))
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'probe-context.txt'),
    ((Protect-Principal ($ProbeContext -join "`n")) + "`n"))

foreach ($Name in $ProbeNames) {
    $Worst = 'PASS'
    foreach ($Status in $ProbeStatus[$Name]) { if ($ProbeOrder[$Status] -gt $ProbeOrder[$Worst]) { $Worst = $Status } }
    $Expected = if ($Name -ceq 'probe-second-user') { 1 } else { $ProbeRounds }
    if ($ProbeStatus[$Name].Count -ne $Expected -and $ProbeOrder[$Worst] -lt $ProbeOrder['INCONCLUSIVE']) {
        $Worst = 'INCONCLUSIVE'; $ProbeLog[$Name].Add("Expected $Expected result(s), found $($ProbeStatus[$Name].Count).")
    }
    if ($ProbeSurvivors.Count -ne 0) {
        $Worst = 'FAIL'; $ProbeLog[$Name].Add("$($ProbeSurvivors.Count) process(es) of this run survived the probe; see probe-survivors.txt.")
    }
    $ProbeLog[$Name].Add("Verdict: $Worst")
    [IO.File]::WriteAllText((Join-Path $EvidenceDir "$Name.txt"), (($ProbeLog[$Name] -join "`n") + "`n"))
    $Verdicts += [PSCustomObject]@{
        name = $Name; status = $Worst; exit_code = $null; timed_out = $false
        duration_seconds = [Math]::Round($ProbeClock.Elapsed.TotalSeconds, 3)
        running = ''; passed = $null; failed = $null; ignored = $null
        notes = "Rounds: $($ProbeStatus[$Name] -join ', '). See $Name.txt."
    }
}

Assert-Head 'after the checks'
$HeadAfter = Invoke-Git 'source-head-after' @('rev-parse', 'HEAD')
if ($HeadAfter.Code -ne 0 -or $HeadAfter.Out.Count -ne 1 -or $HeadAfter.Out[0] -cne $SourceSha) {
    throw "Recorded HEAD after the checks is not the source commit. Preserve $RunRoot and report it."
}
$After = Invoke-Git 'source-status-after' @('status', '--porcelain=v1', '--untracked-files=all')
$DiffCheck = Invoke-Git 'diff-check' @('diff', '--check')
```

`Invoke-Bounded` ends only the process tree that it started, and only on timeout. The survivor scan
after each check lists, and never stops, any process of this run. Each exit status is the exit code
of one process, never the status of a pipeline. `Assert-Head` throws if `HEAD` changes during the run.

Status rules for each command:

- `FAIL`: timeout, process error, missing exit code, non-zero exit code, a failed test, or any
  process of this run (image under the private temp directory, or a command line naming this run's
  directory) that is still running after the check.
- `INCONCLUSIVE`, test gates: no `test result` line, or the `running` and `test result` lines do not
  pair up. Also: every binary reports `running 0 tests`. Also: the `running N tests` sequence, or a
  passed or ignored count, differs from the derived count (a **wrong or missing count**, including a
  count that equals the Mac count when the derived Windows count is different). Also: a required test
  has no `ok` line. A skipped or zero-test selection is never a pass.
- `INCONCLUSIVE`, build gate: a required `Compiling` or `Finished` line is missing, or the daemon
  binary is absent after an exit-0 build. A non-zero build exit, including a missing MSVC C
  toolchain for `libsqlite3-sys`, is `FAIL`. Preserve the output so that the coordinator can tell a
  host problem from a product problem. A warning in the build gate is noted and does not change its
  verdict.
- `INCONCLUSIVE`, clippy gate: a required `Checking` or `Finished` line is missing, or any diagnostic
  that is not in the recorded baseline. Each distinct diagnostic is classified on its own. The
  baseline is exactly two `single_match` warnings at `supervisor.rs` lines 401 and 424 in
  `orchestrator-runtime`, a dependency that the daemon scope also lints. They are noted, not
  failed. The `manual_div_ceil` warning that Priority 8 recorded in `windows_transport.rs` was
  fixed in this source, so it is no longer baseline. The operator reports every diagnostic verbatim.
- Native probe rows: the probe runs 3 rounds. Each row takes its worst round (PASS, then UNPROVEN,
  then INCONCLUSIVE, then FAIL). A row is `FAIL` when it observes the unsafe outcome (for example, a
  DACL that grants another principal, a 17th client that connects, a remote or anonymous open that
  succeeds, a daemon that must be killed, or a survivor). It is `INCONCLUSIVE` when it cannot observe
  the outcome that it needs. It is `UNPROVEN` when the path cannot be exercised on this host (no
  second account, no SMB loopback) or when the remote-client denial has no working control pipe. A
  probe round that stops early makes the rows it did not finish `FAIL`. `UNPROVEN` never changes the
  overall verdict, and the report lists every `UNPROVEN` row.
- `PASS`: exit code zero, every required line and test present, zero tests failed, no survivor, and
  the counts match the derived counts.

Derived counts are never edited. If the measured count differs, the verdict is `INCONCLUSIVE`, the
measured number is reported exactly as printed, and the coordinator decides what it means. Never
change `Passed`, `Ignored`, or `RunningSequence` to match what you saw.

A timeout stays `FAIL`. The first check compiles everything into a new target directory, so a cold
build can exceed a limit. In your report, say whether the preserved stdout or stderr of a timed-out
check ends in `Compiling` or `Finished` output with no `running N tests` line (the build was still
running: an environment or resource limit) or shows tests that had started (a product or test hang).
That label is a note only. Do not change the verdict, retry in the same evidence directory, or raise
a limit.

## 4. Write the result

```powershell
# Set this to what you observed about your own agent runtime and model. If you
# could not observe Sonnet 5.5 with high reasoning, keep 'unverified'.
$AgentRuntime = 'unverified'
$Overall = 'PASS'
foreach ($Verdict in $Verdicts) {
    if ($Rank[$Verdict.status] -gt $Rank[$Overall]) { $Overall = $Verdict.status }
}
$Extra = [Collections.Generic.List[string]]::new()
foreach ($Name in (@($Checks | ForEach-Object { $_.Name }) + $ProbeNames)) {
    if (@($Verdicts | Where-Object { $_.name -ceq $Name }).Count -ne 1) {
        $Overall = 'FAIL'; $Extra.Add("No single verdict for $Name.")
    }
}
# UNPROVEN rows never raise the verdict. They are listed so a PASS cannot be read as covering them.
$UnprovenRows = @($Verdicts | Where-Object { $_.status -ceq 'UNPROVEN' } | ForEach-Object { $_.name })
$UnprovenText = if ($UnprovenRows.Count -gt 0) { $UnprovenRows -join ', ' } else { 'none' }
if ($UnprovenRows.Count -gt 0) {
    $Extra.Add("UNPROVEN, not covered by this result: $UnprovenText.")
}
if ($After.Code -ne 0 -or $After.Out.Count -ne 0) {
    $Overall = 'FAIL'; $Extra.Add('Source status after the run is not clean.')
}
if ($DiffCheck.Code -ne 0) { $Overall = 'FAIL'; $Extra.Add('git diff --check failed.') }

$Table = ($Verdicts | ForEach-Object {
    "| $($_.name) | $($_.status) | $($_.exit_code) | $($_.timed_out) | $($_.duration_seconds) | " +
    "$($_.running) | $($_.passed) | $($_.failed) | $($_.ignored) | $($_.notes) |"
}) -join "`n"
$Fence = '`' * 3
# A command has <name>.summary.txt. A probe row has <name>.txt.
$Summaries = ($Verdicts | ForEach-Object {
    $SummaryPath = Join-Path $EvidenceDir "$($_.name).summary.txt"
    if (-not (Test-Path -LiteralPath $SummaryPath -PathType Leaf)) {
        $SummaryPath = Join-Path $EvidenceDir "$($_.name).txt"
    }
    "### $($_.name)`n`n${Fence}text`n" +
    ([IO.File]::ReadAllText($SummaryPath).TrimEnd()) +
    "`n$Fence"
}) -join "`n`n"
# Expandable here-string: a backtick inside it is an escape (`r, `t, ...).
# Keep backticks out of this text.
$Report = @"
# Priority 9 Windows acceptance of $SourceSha (M5-01b Windows named pipe and M5-02 paged event replay)

This run covers the integrated Priority 9 candidate: the Windows named-pipe transport (M5-01b) and
paged event replay (M5-02), in orchestrator-ipc and orchestrator-daemon. The coordinator decides
acceptance. The expected counts are derived from the cfg and test attributes at the pinned source
for Windows (IPC 26, daemon 20, windows_transport 8). They are not the Mac counts (the coordinator
reports Mac IPC 30 and daemon 24 plus 5 integration tests; this runbook did not confirm those) and
they are not the Priority 8 native counts. Counts are reported as measured, not matched. No macOS
or review result is Windows evidence. This run is also not a second-user, a genuinely remote-client,
or an M6 threat-review claim.

- Run ID: $RunId
- UTC date: $([DateTime]::UtcNow.ToString('o'))
- Source branch: $SourceBranch
- Branch tip at clone time: $BranchTip
- Source commit: $SourceSha
- Source tree and only parent (verified): source-tree.stdout.txt, source-parent.stdout.txt
- HEAD matched the source commit before the checks and after them: yes
  (source-head-before.stdout.txt, source-head-after.stdout.txt)
- Git top-level path form (plain drive-letter form required): source-toplevel.stdout.txt
- Baseline ancestry (5f6f3a11), commit count, the four pinned commits, the linear range, and the
  changed-file set: base-ancestry, range-count, range-merges, range-commits, range-files
  (consistency gates, not patch-equivalence proof)
- Runbook identity (operator-declared; exact bytes pinned by blob id, size, and SHA-256; not proof that
  the pasted commands match): runbook-identity.txt
- Overall result: $Overall $($Extra -join ' ')
- UNPROVEN rows (never covered by an overall PASS): $UnprovenText
- Toolchain: see git-version.stdout.txt, rustc-version.stdout.txt,
  cargo-version.stdout.txt, clippy-version.stdout.txt, git-autocrlf.stdout.txt (exit code in
  git-autocrlf.exit.txt)
- Daemon binary record (size and SHA-256 of the dev-profile build output): daemon-binary.txt
- Clippy diagnostics, classified one by one against the recorded baseline: clippy-scoped.diagnostics.txt
- Native probe: probe-context.txt (elevation, redacted SID, pipe names), one probe-*.txt file per row,
  probe-daemon-r1..r3 (daemon output), probe-second-daemon-r1..r3, and probe-survivors.txt
- OS, CPU, PowerShell: see os.txt, cpu.txt, powershell-version.txt
- Agent runtime and Sonnet 5.5/high setting: $AgentRuntime
- Survivor scan: every check and the probe have a survivors file. Processes whose image lies under
  this run's private temp directory, or whose command line names this run's directory (long or 8.3
  form), are listed and fail the check. Nothing is stopped by the scan.
- Raw stdout and stderr bytes are kept outside the checkout and outside the published files. Their
  size and SHA-256 are in each result.json.

| Command or probe row | Status | Exit | Timed out | Seconds | Running (N per binary) | Passed | Failed | Ignored | Notes |
|---|---|---|---|---|---|---|---|---|---|
$Table

## Test result lines and probe observations

$Summaries

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
"@
# Structured redaction before anything is written. Then write both files.
$Report = Protect-Principal $Report
[IO.File]::WriteAllText((Join-Path $EvidenceDir 'result.md'), $Report)
[IO.File]::WriteAllText(
    (Join-Path $EvidenceDir 'verdicts.json'), ($Verdicts | ConvertTo-Json -Depth 4)
)
Write-Output "overall=$Overall"
```

Use PASS only when every cargo command and every probe row is PASS or UNPROVEN, `HEAD` matches the
full source commit, both source status checks are empty, and the diff check exits zero. PASS here
never covers an UNPROVEN row: the report lists them. A PASS shows that the 5 cargo gates and the
native probe passed on this host. It does not show Windows coverage for any scenario that no test
or probe step ran.

If a command fails, preserve its complete output and report its exit code. If a process does not
stop, report it and do not start another attempt. For a retry, use a new run ID and preserve both
attempts.

## 5. Publish evidence only

Publish the evidence for every result, including FAIL and INCONCLUSIVE. Publish only flat text files
from `$EvidenceDir`. Do not publish `raw`, `cargo-target`, `test-tmp`, the source clone, session
files, agent memory, or any other path. Do not edit the source or `board.json`. Publication fails
closed: any scan hit or missing file stops it before a worktree exists.

The push goes to `origin` only, and only when `origin` is the one remote and equals the pinned
repository `j-mckerracher/agent-orch`. **Never push to or add a remote for `motif-mcao`.** The branch
that you push is `evidence/priority-9-windows-acceptance-dd0bbe3-<RunId>`. It holds evidence files
only. It is never `main`, `orch/…`, or `docs/…`.

```powershell
$EvidenceBranch = "evidence/priority-9-windows-acceptance-$Short-$RunId"
if ($EvidenceBranch -cnotmatch '^evidence/priority-9-windows-acceptance-dd0bbe3-\d{8}T\d{6}Z-[0-9a-f]{32}$') {
    throw 'The evidence branch name is not in the evidence/ namespace. Do not push.'
}
$EvidenceWorktree = Join-Path $env:TEMP "p9ev-$Short-$($RunId.Substring($RunId.Length - 8))"
$EvidencePath = "docs/evidence/priority-9-windows-acceptance/$Short-$RunId"

Assert-Origin 'before publish fetch'
$Run = Invoke-Git 'publish-fetch' @('fetch', 'origin', 'refs/heads/main:refs/remotes/origin/main')
if ($Run.Code -ne 0) { throw "Fetch failed. Preserve $RunRoot and report it." }

$Files = @(Get-ChildItem -LiteralPath $EvidenceDir -Force)
foreach ($File in $Files) {
    if ($File.PSIsContainer -or $File.Name -notmatch '^[a-z0-9][a-z0-9.-]*\.(txt|json|md)$') {
        throw "Unexpected evidence entry: $($File.Name)"
    }
}
if (-not (Test-Path -LiteralPath (Join-Path $EvidenceDir 'result.md') -PathType Leaf)) {
    throw 'Create the result summary before publishing evidence.'
}
# Completeness only. It is not an acceptance gate and does not change a verdict.
$Required = @('result.md', 'verdicts.json', 'runbook-identity.txt', 'protect-text-selfcheck.txt',
    'source-sha.txt', 'branch-tip.txt', 'tool-paths.txt', 'os.txt', 'cpu.txt',
    'windows-version.txt', 'powershell-version.txt', 'daemon-binary.txt',
    'clippy-scoped.diagnostics.txt', 'probe-context.txt', 'probe-survivors.txt')
foreach ($Name in $ProbeNames) { $Required += "$Name.txt" }
foreach ($Round in 1..$ProbeRounds) {
    $Required += "probe-daemon-r$Round.stdout.txt", "probe-daemon-r$Round.stderr.txt",
        "probe-daemon-r$Round.result.json"
    foreach ($Suffix in 'stdout.txt', 'stderr.txt', 'exit.txt', 'result.json') {
        $Required += "probe-second-daemon-r$Round.$Suffix"
    }
}
foreach ($Step in 'clone', 'branch-tip', 'candidate-containment', 'base-ancestry',
    'source-parent', 'source-tree', 'range-count', 'range-merges', 'range-commits', 'range-files',
    'runbook-fetch', 'runbook-type', 'runbook-parent', 'runbook-files',
    'runbook-blob', 'switch',
    'source-head-before', 'source-toplevel', 'source-head-after',
    'source-status-before', 'source-status-after',
    'diff-check', 'git-version', 'rustc-version', 'cargo-version', 'clippy-version',
    'git-autocrlf', 'publish-fetch') {
    $Required += "$Step.stdout.txt", "$Step.stderr.txt", "$Step.exit.txt", "$Step.result.json"
}
# runbook-bytes.stdout.txt is deleted on purpose: it holds the runbook text and a repository URL.
$Required += 'runbook-bytes.stderr.txt', 'runbook-bytes.exit.txt', 'runbook-bytes.result.json'
foreach ($Check in $Checks) {
    foreach ($Suffix in 'stdout.txt', 'stderr.txt', 'exit.txt', 'result.json', 'summary.txt', 'survivors.txt') {
        $Required += "$($Check.Name).$Suffix"
    }
}
$Missing = @($Required | Where-Object { -not (Test-Path -LiteralPath (Join-Path $EvidenceDir $_) -PathType Leaf) })
if ($Missing.Count -ne 0) {
    throw "Evidence is incomplete. Missing: $($Missing -join ', '). Do not push. Report it."
}
if ((Get-Content -LiteralPath (Join-Path $EvidenceDir 'runbook-identity.txt') -Raw) -notmatch '(?m)^runbook commit \(operator-declared, not byte proof\): [0-9a-f]{40}$') {
    throw 'The runbook commit was not declared. Do not push. Report it.'
}
if ((Get-Content -LiteralPath (Join-Path $EvidenceDir 'runbook-identity.txt') -Raw) -notmatch '(?m)^runbook sha256: [0-9a-f]{64}$') {
    throw 'The runbook byte hash was not recorded. Do not push. Report it.'
}

$SecretPattern = 'ghp_[A-Za-z0-9]{20,}|github_pat_|gho_[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}|sk-[A-Za-z0-9]{20,}|-----BEGIN [A-Z ]*PRIVATE KEY|(?i)(password|passwd|secret|api[_-]?key|bearer)\s*[:=](?!:)|(?i)authorization:(?!:)'
$UrlPattern = '://[^/\s@]*@|git@|ssh://'
$Hits = @(Select-String -Path (Join-Path $EvidenceDir '*') -Pattern $SecretPattern) +
    @(Select-String -Path (Join-Path $EvidenceDir '*') -Pattern $UrlPattern) +
    @(Select-String -Path (Join-Path $EvidenceDir '*') -SimpleMatch 'j-mckerracher/agent-orch') +
    @(Select-String -Path (Join-Path $EvidenceDir '*') -SimpleMatch 'motif-mcao') +
    @(Select-String -Path (Join-Path $EvidenceDir '*') -Pattern $ProfileLeakPatterns)
# The probe's session tokens only ever lived in memory. None may appear in any file.
foreach ($ProbeToken in $ProbeTokens) {
    $Hits += @(Select-String -Path (Join-Path $EvidenceDir '*') -SimpleMatch $ProbeToken)
}
if ($Hits.Count -ne 0) {
    $Hits | Select-Object Path, LineNumber | Format-Table | Out-String | Write-Output
    throw 'Possible secret, remote URL, or profile name found. Do not push. Keep the raw files and report it.'
}
```

The pattern scan is a backstop. Also read every file before you publish. If a file holds a secret, a
remote URL, or a profile name, do not push it. Keep the raw file and report the blocker.

The scan fails closed on the profile name. `Protect-Text` already replaced the directory name after
each `Users` path segment with `<user>`. The scan blocks any `Users` segment that is not followed by
`<user>` (in single-backslash, slash, and doubled-backslash JSON forms) and any whole-word occurrence
of the account name, the profile directory name, or its 8.3 alias. A user account whose name is an
ordinary word that appears elsewhere in the evidence is also blocked. Report that as BLOCKED. Do not
edit the files to get past it. The scan prints file names and line numbers only, never the matching
text.

```powershell
if (Test-Path -LiteralPath $EvidenceWorktree) { throw "Worktree path exists: $EvidenceWorktree" }
# Main holds long paths. Windows needs core.longpaths and a short worktree path.
$Run = Invoke-Git 'publish-worktree' @(
    '-c', 'core.longpaths=true', 'worktree', 'add', '-b', $EvidenceBranch,
    $EvidenceWorktree, 'refs/remotes/origin/main')
if ($Run.Code -ne 0) { throw "Worktree creation failed. Preserve $RunRoot and report it." }
& $Git -C $EvidenceWorktree config core.longpaths true
if ($LASTEXITCODE -ne 0) { throw 'Cannot set core.longpaths in the evidence worktree.' }

$Destination = Join-Path $EvidenceWorktree $EvidencePath
if (Test-Path -LiteralPath $Destination) { throw "Evidence path exists: $Destination" }
[IO.Directory]::CreateDirectory($Destination) | Out-Null
foreach ($File in $Files) {
    Copy-Item -LiteralPath $File.FullName -Destination (Join-Path $Destination $File.Name)
}

& $Git -C $EvidenceWorktree add -- $EvidencePath
if ($LASTEXITCODE -ne 0) { throw 'Evidence staging failed.' }
$Staged = @(& $Git -C $EvidenceWorktree diff --cached --name-only)
if ($LASTEXITCODE -ne 0 -or $Staged.Count -eq 0) { throw 'Cannot read staged paths.' }
foreach ($Path in $Staged) {
    if (-not $Path.StartsWith("$EvidencePath/", [StringComparison]::Ordinal)) {
        throw "Unexpected staged path: $Path"
    }
}
& $Git -C $EvidenceWorktree commit `
    -m "evidence: record Priority 9 Windows run of $Short ($Overall; UNPROVEN: $(($UnprovenRows | ForEach-Object { $_ -replace '^probe-', '' }) -join ', '))" `
    -m 'Co-authored-by: Copilot App <223556219+Copilot@users.noreply.github.com>'
if ($LASTEXITCODE -ne 0) { throw 'Evidence commit failed. If Git needs a user identity, report BLOCKED. Do not change credentials.' }
$EvidenceSha = & $Git -C $EvidenceWorktree rev-parse HEAD
$Changed = @(& $Git -C $EvidenceWorktree diff --name-only refs/remotes/origin/main HEAD)
foreach ($Path in $Changed) {
    if (-not $Path.StartsWith("$EvidencePath/", [StringComparison]::Ordinal)) {
        throw "Commit holds an unexpected path: $Path"
    }
}
Assert-Origin 'before push'
$Run = Invoke-Bounded -Exe $Git -Name 'publish-push' -Directory $EvidenceWorktree `
    -Arguments @('push', 'origin', "HEAD:refs/heads/$EvidenceBranch")
if ($Run.Code -ne 0) { throw "Push failed. Preserve $RunRoot and report it." }
Write-Output "evidence_branch=$EvidenceBranch"
Write-Output "evidence_commit=$EvidenceSha"
Write-Output "overall=$Overall"
if ($Overall -ne 'PASS') {
    throw "Result is $Overall. Evidence is published. Report the failure."
}
```

The push output stays in `$EvidenceDir` as `publish-push.*.txt` (URL redacted). It is not published.
The only branch pushed is the evidence branch, from the evidence worktree, and it holds evidence files
only. Never push the source branch, never push to `main`, and never open a pull request.

The final `throw` makes a non-PASS result visible in the session. Report to the coordinator:

- the evidence branch and commit
- the overall result and the status of each of the 5 cargo commands (`daemon-build`, `ipc-tests`,
  `daemon-tests`, `windows-transport-tests`, `clippy-scoped`) and each of the 10 probe rows, with exit
  code, duration, and `running N tests` counts. List the UNPROVEN rows separately
- the OS, CPU, `rustc -Vv`, `cargo -V`, `cargo clippy -V`, `git --version`, and `core.autocrlf` (with
  its origin)
- the branch tip and the pinned source commit, and that `HEAD` matched before and after
- the Git top-level path form that `source-toplevel` recorded, and the source tree, parent, range
  count, range commits, and changed-file checks
- the **measured** IPC, daemon, and `windows_transport` counts, exactly as printed (the `running N
  tests` sequence, passed, failed, and ignored), next to the derived counts (IPC 26, daemon 20,
  `windows_transport` 8), the Mac counts that the coordinator reported (IPC 30, daemon 24 plus 5; not
  confirmed here), and the Priority 8 native counts (IPC 11, daemon 17 unit). Label each as measured
  now, derived, Mac, or prior native. State that the Windows counts differ from the Mac counts by
  design and that you reported the measurement and did not match it to any other number
- the daemon binary size and SHA-256 from `daemon-binary.txt`, every build warning line verbatim, and
  the full text of `clippy-scoped.diagnostics.txt` (baseline hits and any other diagnostic)
- from `probe-context.txt`: elevation state, whether a second local account existed, and the verdict
  of each probe round
- the coverage gaps: Unix-only tests not run, the UNPROVEN rows, and the items not validated on Windows
- the runbook identity record: blob id, byte length, and SHA-256 of the pinned runbook bytes
- for `ipc-tests`, `daemon-tests`, and `windows-transport-tests`: the `running N tests` lines, the
  `survivors.txt` content (empty means none found), and for a failure the first failing assertion
  message exactly as printed
- for any timed-out check, the cold-build label from the status rules, as a note beside the unchanged
  `FAIL`
- that `protect-text-selfcheck.txt` and the publication scan passed
- any setting that you could not observe, including whether your agent runtime could run Sonnet 5.5
  with high reasoning

Do not edit `board.json`. The coordinator reviews the evidence and updates the board.

## What changed from the approved harness

The harness blocks derive from the Priority 8 runbook (docs commit `fc41237c…`, review verdict PASS),
which derives from the accepted Priority 7 runbook. Preserved unchanged in behavior: PowerShell 7
only, the pinned SHA and `HEAD` equality before and after, the declared runbook commit and the runbook
byte pin, clean-checkout gates, unique run directories outside the checkout, `Protect-Text` and its
self-check, `Invoke-Bounded` (byte-exact raw capture, tree kill on timeout only), the survivor scan
after every check, the fail-closed publication scan, the `Expect`, `Passed`, `Ignored`,
`RunningSequence`, `Isolated`, `TimeoutSeconds` model, the never-kill-a-survivor rule, and
evidence-only branch publication to `origin`.

Review findings from the Priority 8 review, applied here:

- **L1.** The clippy gate runs the scoped, full-dependency form `clippy --locked -p orchestrator-ipc
  -p orchestrator-daemon --all-targets` with no `--no-deps`. The description, the check definition,
  and the baseline say the same thing. Diagnostics from the dependency crate `orchestrator-runtime`
  appear in this scope, and the two `single_match` warnings in its `supervisor.rs` are the recorded
  baseline.
- **L2.** `Assert-Origin` checks the fetch URL and the push URL (`remote get-url --push origin`)
  against the pinned repository, besides the single remote name. It never prints either URL.
- **N1.** The Mac counts are reported as coordinator-supplied and not independently confirmed here.
- **N2.** The `$Samples` line of the `Protect-Text` self-check uses a different sample credential
  from the Priority 7 runbook. The Priority 8 runbook made that change without disclosing it. It is
  disclosed here. The line is carried byte-identical from the Priority 8 runbook and still exercises
  the masking. The statement "no helper was rewritten" is true of the logic only.
- **Published branch names.** The default clone and fetch instructions use the names that the
  coordinator publishes: `orch/priority-9-m5-01b-m5-02-dd0bbe3` for the source and
  `docs/priority-9-windows-acceptance-dd0bbe3` for this runbook. The Priority 8 default-branch
  mistake is not repeated, and the runbook byte pin still decides what was run.

Changed for Priority 9:

- Re-pinned the title, source commit, tree, and parent, branch name check (`orch/priority-9-…`),
  baseline `5f6f3a11…`, range count (4), the four commit ids in order, linear range (zero merges),
  the 11 changed files, evidence branch and path names (`priority-9`), run-root prefix
  (`agent-orch-p9-acc-`), evidence worktree prefix (`p9ev-`), runbook file name, and links.
- Added `windows-transport-tests` (8 tests, `RunningSequence @(8,0)`). Raised the derived counts for
  Windows to IPC 26 and daemon 20.
- Replaced the zero-warning rule of `clippy-scoped` with a per-diagnostic classifier. It writes
  `clippy-scoped.diagnostics.txt`. A baseline hit is noted. Any other distinct diagnostic is
  `INCONCLUSIVE`. The Priority 8 `manual_div_ceil` baseline is fixed in this source.
- Added the native probe: 10 rows in 3 rounds, with a worst-of-rounds verdict per row. The probe reads
  the pipe DACL and the `daemon.token` ACL, drives 16 clients and a 17th, exercises connect-and-close
  cycles, a large inbound frame, an `\\localhost\pipe` open, an anonymous open, a second daemon, and
  the shutdown RPC. It uses a small `Add-Type` C# P/Invoke class for the pipe calls.
- Added the `UNPROVEN` status. It ranks as PASS for the overall verdict, and `result.md` lists every
  `UNPROVEN` row so that an overall PASS cannot be read as covering it.
- Extended the leak patterns with the host and domain names. Added a scan that looks for the probe's
  in-memory session tokens in the evidence files. Added `Protect-Principal` for SID and account text.
- Not added: a rustfmt gate, a workspace-wide test run, a Windows cross-compile, a comparison of the
  pasted text with the pinned bytes, and a recomputation of the pipe name.

Known limits of this runbook:

- Nothing in it has been run on Windows. The `Add-Type` P/Invoke code, the probe timings, and the
  SDDL and ACL text patterns were checked only by reading source and, on macOS, by parsing the
  PowerShell and compiling the C#. A first Windows run may show a harness defect. That is a finding,
  not a reason to edit the file mid-run.
- The counts are derived by reading `cfg` and `#[test]` attributes at the pinned source. A mismatch is
  `INCONCLUSIVE` and is a finding for the coordinator, not an edit of this file.
- The daemon `tests\daemon.rs` binary is `cfg(unix)` and reports `running 0 tests` on Windows. Do not
  read that as a missing gate.
- The probe covers one host, one account, and the recorded elevation state. Second-user denial, a
  truly remote client (a loopback denial without a working control pipe is capped at `UNPROVEN`), and
  an outbound write above 64 KiB to a slow reader stay `UNPROVEN` or
  unreported unless the host allows them. Run the probe elevated and not elevated as separate runs if
  both are available.
- The 16/17 instance probe depends on timing. The 17th open happens within 1800 ms of the first
  connect, because the daemon drops idle connections after about 2 seconds.
- The check helper ends only the tree it started. The survivor scan reports other processes by path
  and command line and never stops them.
- The clippy gate shares the target directory with the earlier gates. If cargo prints no `Checking`
  line, the gate is `INCONCLUSIVE`. That is a finding, not a cause to run it again in the same
  evidence directory.
- The Mac counts are coordinator-supplied. Independent Mac validation is separate and does not change
  the Windows expectation.
- The declared runbook commit and the source pin are separate. The byte pin makes the runbook bytes
  comparable after the run. It is not an execution proof.
- The runbook is read from the **tip of the docs branch**. The source under test is always `dd0bbe33`.
  The docs chain is `dd0bbe33` -> `8b0f3e92` (reviewed first revision) -> the tip (revision 2). Step 1
  accepts the declared runbook commit only when it is that docs-only chain (or the first revision itself
  on its own) and records which one it found in `runbook-identity.txt`.

## What changed in revision 2 (review of `8b0f3e92`: PASS with advisories)

This revision is one docs-only commit on top of `8b0f3e92`. The source under test is unchanged.

- **M1.** `probe-remote-client` now has a positive control: a .NET control pipe with a current-user-only
  DACL, opened through the same `\\localhost\pipe\` path. PASS needs the daemon pipe denied (5) and the
  control opened. A denial without a control opening is capped at `UNPROVEN` and is stated as not a
  PASS. The assumption that .NET does not request remote-client rejection for the control pipe is
  stated and is not verified on Windows.
- **M2.** A probe round that stops early marks every unfinished row `FAIL` (it was `INCONCLUSIVE`) and
  writes explicit "not run" stub files for any missing `probe-daemon-r<N>.*` and
  `probe-second-daemon-r<N>.*` evidence. The publish step's completeness check no longer blocks the
  evidence of an early failure.
- **Docs chain.** `runbook-parent` now accepts the docs chain `dd0bbe33` -> `8b0f3e92` -> tip, in
  addition to a single commit on `dd0bbe33`. `runbook-files` still requires the whole difference from
  the source to be this one file.
- **L1 (icacls).** The ACE filter keeps lines that end in a parenthesized permission group, so it no
  longer depends on the English trailer text of `icacls`.
- **L2, L3.** Notes: a pipe-SDDL FAIL needs the recorded SDDL read first; an `InvalidParams` reply to the
  large frame is expected.
- **L4.** The evidence commit subject lists the `UNPROVEN` rows.
- **L5.** The clippy diagnostics file ends with a reconciliation line (declared warnings, distinct
  classified warnings, baseline, unexpected). A declared count with no, or fewer, distinct diagnostics is
  `INCONCLUSIVE`.
- **L6.** Note: run the probe with no other agent-orch daemon running.
- Not changed: N1 (equal `duration_seconds` on every probe row).
