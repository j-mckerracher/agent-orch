// Every expected count and baseline in the native gates, in one place.
//
// When a merge adds or removes tests, or moves the baseline clippy sites, run
// `node scripts/ci/sync-expected.mjs <gate-evidence-dir>...` on the evidence of a CI run (the
// `gate-evidence-*` artifacts), or edit THIS file and the matching name list (windows-test-names.json /
// macos-test-names.json) by hand. Then run `node --test "scripts/**/*.test.mjs"`: expected.test.mjs checks
// that the numbers agree with each other and with the name lists. Never edit a value to fit a run without a
// reason in the commit.

// Windows: the published Priority 11r3 runbook's derived values (not yet measured on a runner).
// `sequence` is the `running N tests` series, one entry per test binary, in order.
export const WINDOWS = {
  // The M7 merge adds 9 portable ipc tests (host, queue, workstreams).
  // Measured on Windows (b44f88a): ipc 55 (13 of them in windows_transport); the M7-final-r1
  // large-write test makes 56. The daemon library measured 118; conversation_runtime ran 12
  // tests (10 passed, 1 failed, 1 ignored) and the run stopped there, so the daemon total is
  // still derived: 118 + 11 + 2 + 4 + 0 + 1 + 2 = 138 once M7-08 passes.
  // DPES-1105 adds 2 portable ipc tests (history capability), so 56 becomes 58.
  // The chat model choice (DPES-1105) adds 1 portable ipc test (selection capability): 58 becomes 59 (CI run 43734333632).
  // DPES-1105 action proposals (PR #113, CI run 43981399384, measured on Windows): 3 call-context tests, so 59 becomes 62.
  // DPES-1105 action resolve (PR #116, CI run 43998558420, measured on Windows): the ipc lib has 64 tests.
  // DPES-1105 approver credential and task actions (CI run 44023050296, measured on Windows): the ipc lib has 83 tests.
  ipc: { passed: 126, sequence: [126, 0] },
  // Derived from cfg gates: the library loses executor::tests,
  // conversation_orphans, conversation_agent::launch and 4 other unix-only tests and gains one
  // cfg(windows) test; conversation_runtime loses 2 unix tests (11 passed + 1 #[ignore]);
  // tests/daemon.rs is cfg(unix), so it runs 0 tests.
  // DPES-1105 adds 8 portable daemon unit tests (conversation clear, removal and the capability), 118 becomes 126.
  // The AI usage module (api::usage_local, three portable tests) landed on main in PR #43 without this manifest, 146 becomes 149.
  // The cfg(windows) Copilot refusal test (api_tests.rs) adds one more: 149 becomes 150 and the unit binary 130.
  // The Copilot executor tests are unix-only (under executor::tests), so they do not count here.
  // The chat model choice (DPES-1105) adds 6 portable unit tests and 8 process tests in conversation_runtime (fake agent),
  // so 150 becomes 164: the unit binary 130 becomes 136 and conversation_runtime 12 becomes 20 (CI run 43734333632).
  // The Copilot sandbox gate removal (PR #75) deletes the cfg(windows) refusal test (api_tests.rs), so 164 becomes 163 and the
  // unit binary 136 becomes 135 (CI run 43757529505).
  // DPES-1105 permission levels (PR #80, CI run 43767887059): the unit binary 135 becomes 138 (3 portable tests) and
  // conversation_runtime 20 becomes 24 (4 process tests), so 163 becomes 170 passed plus 1 ignored, sequence [138, 24, 2, 4, 0, 1, 2].
  // DPES-1123 adds three portable task-submission tests, so the unit binary is 141 and the total is 173.
  // WI-DPES-747: Three additional portable daemon token-warning tests; Windows totals are derived.
  // DPES-1105 task permission levels (PR #93, CI run 43915710007, measured on Windows): the unit binary 144 becomes 146 (2 portable
  // API tests; the executor tests are Unix-only), so 176 becomes 178 passed plus 1 ignored, sequence [146, 24, 2, 4, 0, 1, 2].
  // The chat model choice for GitHub Copilot (DPES-1105, PR #99, CI run 43951278199, measured on Windows): the unit binary 146 becomes 147.
  // The old Copilot refusal test is replaced by the Pi refusal and the missing-CLI tests, so 178 becomes 179 passed plus 1 ignored.
  // DPES-1106 unverified agent versions (PR #101, CI run 43965574766, measured on Windows): the unit binary 147 becomes 154 and
  // conversation_runtime 24 becomes 30. The not-pinned refusal tests are replaced by the unverified-run tests, so 179 becomes 192 passed.
  // DPES-1105 orchestrator preamble and snapshot (PR #108, CI run 43974417288, measured on Windows): the unit binary 154 becomes 165 and
  // conversation_runtime 30 becomes 31; one service test is renamed, so 192 becomes 204 passed.
  // DPES-1105 action proposals (PR #113, CI run 43981399384, measured on Windows): the unit binary 165 becomes 167 and
  // conversation_runtime 31 becomes 42, so 204 becomes 217 passed.
  // DPES-1105 action resolve (PR #116, CI run 43998558420, measured on Windows): the unit binary is 187 and
  // conversation_runtime 44, with 239 passed plus 1 ignored.
  // DPES-1105 approver credential, task actions and load gate core (CI run 44023050296, measured on Windows): the unit binary is 212 and
  // conversation_runtime 61, with 281 passed plus 1 ignored.
  // OX-W9 workstream plan editing adds 9 portable daemon unit tests: 601 becomes 610.
  // Opt-in project coordinators add 1 portable daemon unit test (registration adds no coordinator): 610 becomes 611.
  // Project coordinator replies add one portable conversation_runtime regression.
  // The home directory and Global tasks add 13 portable daemon unit tests (the 2 executor tests are Unix-only): 612 becomes 625.
  // Dispatch defaults add two portable unit tests: 625 becomes 627.
  // Adaptive wakes add six portable daemon regressions.
  // Settling the default agent before wake and relay turns adds one portable unit test and one conversation_runtime test.
  // Workstream priority adds one portable daemon unit test (workstreams.prioritize): 677 becomes 678.
  // The shared compatibility policy replaces six obsolete chat-only helper tests.
  // DPES-1108 phase 3 user decisions (CI run 44475972106, measured on Windows): the unit binary 593 becomes 600, so 696 becomes 703.
  // DPES-1108 phase 4 adds one portable wake classification test (the executor tests are Unix-only): 703 becomes 704.
  // DPES-1108 phase 5 authority model adds 12 daemon unit tests and 1 real-daemon test: 704 becomes 717 (CI run 44508913337).
  // DPES-1108 phase 6 preferences adds 7 daemon tests (608 vs 601 unit, 88 real-daemon): 724 in the phase 9 integration, re-synced from CI.
  // DPES-1108 phase 7 coordinators adds 8 daemon unit tests: 732 in the phase 9 integration, re-synced from CI.
  // DPES-1108 phase 8 adds 4 daemon unit tests: 736 in the phase 9 integration, re-synced from CI.
  // With main merged (DPES-1106): one real-daemon test and the agent_thinking store binary (5) plus one store lib test; re-synced from CI.
  // DPES-1106 adds one portable daemon test (tasks.message, api_authority_tests): 738 becomes 739, unit binary 633 becomes 634.
  // The wake brief keeps a worker's note (line cap 512) and adds one portable daemon test: 739 becomes 740, unit binary 634 becomes 635.
  daemon: { passed: 758, ignored: 1, sequence: [650, 93, 2, 5, 0, 1, 2, 2, 0, 4, 0] },
  coordinatorReview: { passed: 5, sequence: [5] },
  // The ipc lib is 59 on Windows (58 plus the selection capability test), so 59 - 14 = 45 are filtered out (CI run 43734333632).
  // PR #113, CI run 43981399384: the Windows ipc lib has 62 tests, so 48 are filtered out.
  // CI run 43998558420: the Windows ipc lib has 64 tests, so 50 are filtered out.
  // CI run 44023050296: the Windows ipc lib has 83 tests, so 69 are filtered out.
  windowsTransport: { passed: 14, sequence: [14], filteredOut: 112 },
  // DPES-1105 approver credential (CI run 44023050296, measured on Windows): four host tests (launch and the real-daemon bridge), 18 passed.
  tauri: { passed: 67, sequence: [67, 0, 0] },
  // Derived from the sources' cfg gates and checked against the Priority 12r2 Windows run, which
  // ran git-lib at 46 (the 47 once derived counted the two cfg(unix) tests and missed the
  // cfg(windows) one) and cleanup_safety at 3 (two of its five tests are cfg(unix)).
  // git-lib: 58 = 56 portable tests + 2 cfg(windows) tests (macOS: 56 + 2 cfg(unix) = 58).
  gitLib: { passed: 103, sequence: [103] },
  cliCleanupSafety: { passed: 3, sequence: [3] },
  // 4 portable tests + 7 in the cfg(windows) module (the DACL tests; the inherited-ACE regression for
  // `.orchestrator.lock` is one of them).
  privateFs: { passed: 31, ignored: 1, sequence: [20, 9, 3, 0] },
  runtimeCommandLine: { passed: 8, sequence: [8] },
  // One portable test and one cfg(windows) test.
  cliCommandLine: { passed: 2, sequence: [2] },
  // orchestrator-store: the macOS list minus 4 cfg(unix) tests (a lib test of the data directory modes, a lib test
  // of the database file modes, the lock-path symlink test, the real-process exit test). The `8` is data_lock.rs:
  // 7 passed and 1 `#[ignore]` child process. The Priority 12r3 Windows run measured the lib at 32; round 4 added
  // one portable test, so 33 is derived. The lock-path test `the_lock_file_is_owner_only_when_created_and_when_taken_over`
  // is the M6-01 takeover ACL check and is also its own gate.
  // Derived, not measured on Windows: the macOS list minus the same 4 cfg(unix) tests (2 lib tests, 1 in
  // lock_identity_contract, 1 in sqlite_reconcile); the sequence is the macOS one with those 4 removed.
  // DPES-1105 adds 1 lib test and the 10-test conversation_history binary (portable): lib 63 becomes 64.
  // DPES-1106 adds 1 lib test (the publish regression) and 1 data_lock.rs test (the racing-acquires stress): lib 64 becomes 65,
  // data_lock 8 becomes 9. Both are portable.
  // DPES-1105 permission levels (PR #80, CI run 43767887059, measured on Windows): the store lib is 68 (up from 65) and
  // the store totals 170 passed plus 1 ignored. The lib gains 3 portable tests (permission level and migration checks).
  // WI-DPES-736: Seven portable audit-envelope tests add the new binary after attempt_history. Windows counts are derived; native CI verifies them.
  // DPES-1105 task overview (PR #108, CI run 43974417288, measured on Windows): the lib 68 becomes 69, so 177 becomes 178 passed.
  // DPES-1105 action proposals (PR #113, CI run 43981399384, measured on Windows): new tests/action_proposals.rs (7) after the lib, and
  // conversation_history 10 becomes 13, so 178 becomes 188 passed.
  // DPES-1105 action resolve (PR #116, CI run 43998558420, measured on Windows): action_proposals.rs is 14 tests, so 195 passed.
  // DPES-1105 task actions and settings (CI run 44023050296, measured on Windows): action_proposals.rs is 23 tests and a new
  // orchestrator_settings.rs binary adds 12, so 216 passed.
  // Opt-in project coordinators add 2 portable conversation_history tests: 13 becomes 15, so 349 becomes 351 passed.
  // The home directory setting adds 1 portable store test (orchestrator_settings 12 becomes 13): 351 becomes 352.
  // Dispatch defaults add one orchestrator_settings test: 13 becomes 14, so 352 becomes 353 passed.
  // Workstream priority adds 2 portable workstreams.rs tests: 13 becomes 15, so 359 becomes 361 passed.
  // DPES-1108 phase 3 user decisions add the decision_questions binary (8 tests): 378 becomes 386 (CI run 44475972106).
  // DPES-1108 phase 4 adds the portable worker_status binary (11 tests): 386 becomes 397, in the macOS order.
  // DPES-1108 phase 5 adds the action_authority binary (6 tests): 397 becomes 403 (CI run 44508913337).
  // DPES-1108 phase 6 replaces the 5 orchestrator_notes tests with the 7 memory_documents tests (CI run 44507155149): 405.
  // DPES-1108 phase 7 adds the coordinators binary (8 tests): 413.
  // DPES-1108 phase 8 adds one store lib test and the turn_links binary (4 tests): 418.
  // DPES-1106 adds two worker_status tests (the author split and migration 0073): 428 becomes 430, worker_status 11 becomes 13.
    // Chat times leave the store as RFC 3339 UTC (DPES-1044) adds two store lib tests and one conversations test: 419 becomes 422.
store: { passed: 422, ignored: 0, sequence: [108, 6, 7, 31, 9, 5, 9, 3, 7, 9, 13, 1, 15, 12, 9, 8, 11, 3, 7, 14, 2, 7, 14, 7, 20, 3, 11, 2, 4, 14, 3, 7, 5, 2, 5, 1, 13, 15, 0] },
  // Windows lib: 68 tests compile (the 2 cfg(unix) tests are excluded), so 68 - 1 = 67 are filtered out.
  // PR #108 (CI run 43974417288): the Windows store lib has 69 tests, so 68 are filtered out.
  lockTakeover: { passed: 1, sequence: [1], filteredOut: 19 },
  // No cfg gate in the crate: the same 28 as the Priority 12r3 Windows run (6 + 7 + 15).
  diagnostics: { passed: 28, sequence: [0, 6, 7, 15, 0] },
};

// macOS: measured on the Priority 12 integration merge (M6-01 + M6-02 + M6-03 on the M5-05 round 5
// source). Not Windows evidence. daemon is unit, coordinator_review, daemon.rs, diagnostics_log.
export const MACOS = {
  // Measured on the M7 final integration merge (macOS, this branch).
  // Measured on the DPES-1105 branch: 55 plus the two history capability tests.
  // The chat model choice (DPES-1105) adds the selection capability test: 57 becomes 58 (CI run 43734333632).
  // DPES-1105 action proposals (PR #113, CI run 43981399384, measured on macOS): 3 call-context tests, so 58 becomes 61.
  // DPES-1105 action resolve (PR #116, CI run 43994386560, measured on macOS): the ipc lib has 63 tests.
  // DPES-1105 approver credential (CI run 44023050296, measured on macOS): the ipc lib has 82 tests.
  ipc: { passed: 125, sequence: [125, 0] },
  // The new fast-exit executor test is Unix-only; Windows counts remain unchanged.
  // Measured on the DPES-1105 branch: the unit binary gains the 8 history tests (176 becomes 184), then the
  // Claude Code model alias argv test (184 becomes 185). The AI usage module (api::usage_local, three tests) landed on
  // main in PR #43: 218 becomes 221 and the unit binary 188. The three Copilot executor tests then make the unit binary
  // 191 and the total 224. These executor tests are unix-only, so Windows is unchanged by them.
  // The chat model choice (DPES-1105) adds 6 unit tests and 8 process tests in conversation_runtime (fake agent): 224
  // becomes 238, the unit binary 191 becomes 197 and conversation_runtime 14 becomes 22 (CI run 43734333632).
  // DPES-1105 permission levels (PR #80, CI run 43767887059, measured on macOS): the unit binary 197 becomes 200 (3 in-process
  // daemon tests) and conversation_runtime 22 becomes 26 (4 process tests), so 238 becomes 245 passed plus 1 ignored.
  // DPES-1123 adds three portable API tests and one Unix-only executor retry test.
  // WI-DPES-747: Three additional portable daemon token-warning tests.
  // DPES-1105 task permission levels (PR #93, CI run 43915710007, measured on macOS): the unit binary 207 becomes 215 (2 portable
  // API tests and 6 Unix-only executor tests), so 252 becomes 260 passed plus 1 ignored.
  // The chat model choice for GitHub Copilot (DPES-1105, PR #99, CI run 43951278199, measured on macOS): the unit binary 215 becomes 217
  // and conversation_runtime 26 becomes 29 (the Copilot choice tests; the executor catalog test is Unix-only), so 260 becomes 265.
  // DPES-1106 unverified agent versions (PR #101, CI run 43965574766, measured on macOS): the unit binary 217 becomes 224 and
  // conversation_runtime 29 becomes 35, so 265 becomes 278 passed.
  // DPES-1105 orchestrator preamble and snapshot (PR #108, CI run 43974417288, measured on macOS): the unit binary 224 becomes 235 and
  // conversation_runtime 35 becomes 36; one service test is renamed, so 278 becomes 290 passed.
  // DPES-1105 action proposals (PR #113, CI run 43981399384, measured on macOS): the unit binary 235 becomes 237 and
  // conversation_runtime 36 becomes 47, so 290 becomes 303 passed.
  // DPES-1105 action resolve (PR #116, CI run 43994386560, measured on macOS): the unit binary is 257 and
  // conversation_runtime 49, with 325 passed plus 1 ignored.
  // DPES-1105 approver credential, task actions and load gate core (CI run 44023050296, measured on macOS): the unit binary is 282 and
  // conversation_runtime 66, daemon.rs 13, with 369 passed plus 1 ignored.
  // OX-W9 workstream plan editing adds 9 portable daemon unit tests: 696 becomes 705.
  // Opt-in project coordinators add 1 portable daemon unit test: 705 becomes 706.
  // Project coordinator replies add one conversation_runtime regression.
  // The home directory and Global tasks add 15 daemon unit tests (13 portable, 2 Unix-only executor): 707 becomes 722.
  // Dispatch defaults add two portable unit tests and three Unix fake-CLI tests: 722 becomes 727.
  // Adaptive wakes add six portable daemon regressions.
  // Settling the default agent before wake and relay turns adds one portable unit test and one conversation_runtime test.
  // Workstream priority adds one portable daemon unit test (workstreams.prioritize): 794 becomes 795.
  // Six chat-only helper tests removed; the Unix executable-replacement regression adds one.
  // Running task attempts as the pstack workflow agent (DPES-1044) adds four Unix-only executor tests: 820 becomes 824 (CI run 44356323793, measured on macOS).
  // DPES-1108 phase 4 worker status adds 5 Unix executor tests and 1 wake classification test: 710 becomes 716 (CI run 44494162204, measured on macOS).
  // DPES-1108 phase 5 authority model adds 12 daemon unit tests and 1 real-daemon test: 846 becomes 859 (CI run 44508913337).
  // DPES-1108 phase 6 preferences adds 7 daemon tests: 866 in the phase 9 integration, re-synced from CI.
  // DPES-1108 phase 7 coordinators adds 8 daemon unit tests: 874 in the phase 9 integration, re-synced from CI.
  // DPES-1108 phase 8 adds 4 daemon unit tests: 878 in the phase 9 integration, re-synced from CI.
  // DPES-1106 adds one portable daemon test and one unix-only executor test: 882 becomes 884, unit binary 750 becomes 752.
  // The wake brief adds one portable daemon test: 884 becomes 885, unit binary 752 becomes 753.
  daemon: { passed: 908, ignored: 1, sequence: [773, 98, 2, 5, 13, 1, 2, 2, 4, 4, 5] },
  coordinatorReview: { passed: 5, sequence: [5] },
  // The workspace gate excludes orchestrator-daemon, orchestrator-store and orchestrator-ipc: daemon-tests, store-tests and
  // ipc-tests already run those whole crates, and running them twice made the macOS job take over 30 minutes. Main measured
  // 2323 passed and 2 ignored for the whole workspace (CI run 44550404415); minus daemon 836+1, store 388+1 and ipc 121 that is
  // 978 passed and none ignored. The CLI help probe test of PR #229 (orchestrator-agents) makes 979, as the macOS
  // workspace-tests gate measured on this branch (CI run 44565723305). After PR #224 (DPES-1108), main's whole-workspace
  // pin is 2467 passed and 2 ignored; minus daemon 882+1, store 432+1 and ipc 121 that is 1032 passed and none ignored.
  // A cancelled waiting task no longer blocks the restart gate: one engine test, so 1054 becomes 1055.
  workspace: { passed: 1073, ignored: 1 },
  // DPES-1105 approver credential (CI run 44023050296, measured on macOS): 20 passed.
  tauri: { passed: 71, sequence: [71, 0, 0] },
  gitLib: { passed: 105, sequence: [105] },
  cliCleanupSafety: { passed: 4, sequence: [4] },
  privateFs: { passed: 30, ignored: 1, sequence: [18, 9, 4, 0] },
  runtimeCommandLine: { passed: 8, sequence: [8] },
  cliCommandLine: { passed: 1, sequence: [1] },
  // Measured on the DPES-1106 branch (macOS `cargo test -p orchestrator-store`): the lib has 67 tests and data_lock.rs 9 (8 passed, 1 ignored).
  // DPES-1105 permission levels (PR #80, CI run 43767887059, measured on macOS): the lib is 70 (3 new tests), and the store
  // totals 174 passed plus 1 ignored.
  // WI-DPES-736: Measured default-parallel macOS store run, 181 passed and 1 ignored, with seven tests in the audit_envelope binary.
  // DPES-1105 task overview (PR #108, CI run 43974417288, measured on macOS): the lib 70 becomes 71, so 181 becomes 182 passed.
  // DPES-1105 action proposals (PR #113, CI run 43981399384, measured on macOS): new tests/action_proposals.rs (7) after the lib, and
  // conversation_history 10 becomes 13, so 182 becomes 192 passed.
  // DPES-1105 action resolve (PR #116, CI run 43994386560, measured on macOS): action_proposals.rs is 14 tests, so 199 passed.
  // DPES-1105 task actions and settings (CI run 44023050296, measured on macOS): 220 passed.
  // Opt-in project coordinators add 2 conversation_history tests: 13 becomes 15, so 353 becomes 355 passed.
  // The home directory setting adds 1 store test (orchestrator_settings 12 becomes 13): 355 becomes 356.
  // Dispatch defaults add one orchestrator_settings test: 13 becomes 14, so 356 becomes 357 passed.
  // Workstream priority adds 2 workstreams.rs tests: 13 becomes 15, so 363 becomes 365 passed.
  // DPES-1108 phase 3 user decisions add the decision_questions binary (8 tests): 382 becomes 390 (CI run 44475972106).
  // DPES-1108 phase 4 adds the worker_status binary (11 tests): 390 becomes 401 (CI run 44494162204).
  // DPES-1108 phase 5 adds the action_authority binary (6 tests): 401 becomes 407 (CI run 44508913337).
  // DPES-1108 phase 6 replaces the 5 orchestrator_notes tests with the 7 memory_documents tests (CI run 44507155149): 409.
  // DPES-1108 phase 7 adds the coordinators binary (8 tests): 417.
  // DPES-1108 phase 8 adds one store lib test and the turn_links binary (4 tests): 422.
  // DPES-1106 adds two worker_status tests (the author split and migration 0073): 432 becomes 434, worker_status 11 becomes 13.
    // Chat times leave the store as RFC 3339 UTC (DPES-1044) adds two store lib tests and one conversations test: 421 becomes 424.
store: { passed: 424, ignored: 0, sequence: [109, 6, 7, 31, 9, 5, 9, 3, 7, 9, 13, 1, 15, 12, 9, 8, 11, 3, 7, 14, 2, 7, 14, 7, 20, 3, 11, 2, 5, 14, 3, 7, 5, 2, 5, 1, 13, 15, 0] },
  // Measured on the same run: the store lib has 70 tests, so 69 are filtered out.
  // PR #108 (CI run 43974417288): the macOS store lib has 71 tests, so 70 are filtered out.
  lockTakeover: { passed: 1, sequence: [1], filteredOut: 17 },
  diagnostics: { passed: 28, sequence: [0, 6, 7, 15, 0] },
  // DPES-1105 usage controls (PR #97) adds two real-daemon e2e files (3 and 1 tests): 8 files and 46 tests become 10 and 50,
  // as the macOS desktop-e2e gate measured them in CI run 43945842537.
  e2eFiles: 10,
};

// `node --test "scripts/**/*.test.mjs"`: no test is platform-gated, so both platforms run the same number. The 26
// tests of install-lib.test.mjs build paths with node:path (checked under scripts/release/win32-path-sim).
// 112 = 97 + the 13 tests of windows-jobs.test.mjs and the 2 of prune-target.test.mjs (DPES-1106). The 4 tests of the
// usage reader smoke test live in scripts/smoke/ (outside this glob) and run only when the smoke group is selected.
// 132 = 112 + the 13 security-gate regression tests of DPES-1100 (main: 125) + the 7 tests of publish.test.mjs
// (DPES-1106, release publish). The branch measured 119 = 112 + 7 in release dispatch run 43810534672; the merged total
// is the sum, and CI measures the real value.
// 137 = 132 + the 3 tests of release-please.test.mjs and the 2 added to publish.test.mjs (DPES-1106, release-please, PR #100).
// Measured by `node --test "scripts/**/*.test.mjs"` on main and by the macOS node-self-tests gate in CI run 43968335824.
// 146 = 137 + the 9 tests of sync-expected.test.mjs, counted by `node --test "scripts/**/*.test.mjs"`.
// 149 = 147 + the macOS job coverage and selection tests in windows-jobs.test.mjs.
export const NODE_SELF_TESTS = { passed: 167 };

// Platform independent. The target-picker selection fix adds 1 file and 7 tests (1549 becomes 1556, 68 files). The
// Copilot provider adds 4 test files and 15 tests: 72 files and 1571 tests. The Enter-in-search fix adds 2 tests
// (DPES-1105, PR #73): 1573, as the Windows desktop gate measured it in CI run 43726827533.
// The chat model choice (DPES-1105) adds 1 file (ModelPicker.test.tsx) and 7 tests: 73 files and 1580 tests, as the
// desktop gate measured them in CI run 43734333632.
// The permission level control (DPES-1105) adds 3 files (permissionLevel, PermissionLevelPicker and
// WorkspaceView.permission tests) and 46 tests. Counted from a full `vitest run --reporter=json` of apps/desktop: 76 files and 1626 tests.
// The ambient background is on by default (DPES-1105): 9 tests replace 5, so 1626 becomes 1630 (same 76 files), as a full
// `vitest run --reporter=json` of apps/desktop measured them.
// The composer permission cleanup (DPES-1105) moves the level into the model picker's chip and panel and adds 6 label-state
// tests: it adds 8 tests to that base with no file added or removed, so 1630 becomes 1638. CI measures the real total.
// DPES-1123 adds two files and 14 tests to main: 78 files and 1652 tests.
// WI-DPES-747: One additional boundary test file and ten frontend regression cases.
// DPES-1105 usage controls (PR #97) adds one file (UsagePill.headline.test.tsx, 5 tests) and renames two titles: 80 files and
// 1667 tests, as the desktop-vitest gate measured them on macOS and Windows in CI run 43945842537.
// The chat model choice for GitHub Copilot (DPES-1105, PR #99) adds 6 net tests to the existing ModelPicker and CopilotSignInCheck
// test files: 1673 tests in 80 files, as measured by `npx vitest run` on the merged tree (CI measured 1668 in 79 files before main's 5).
// DPES-1106 unverified agent versions (PR #101) adds 8 tests to conversationLive.test.ts: 1681 tests in 80 files, counted by
// `npx vitest run` on the merged tree and by the desktop-vitest gate in CI run 43965574766.
// The persisted model choice and its coordinator-note rule (DPES-1105) add one file (modelChoice.test.ts) and 36 tests, and
// remove one test (the old coordinator-note block): 81 files and 1716 tests, as the Windows desktop-vitest gate measured them
// in CI run 43974286338 (81 passed, 1716 passed). The name list holds the same 1716 titles.
// DPES-1105 action resolve and approval card (PRs #116 and #118, CI run 43994386560): 3 new files and 57 tests, 84 files 1773 tests.
// DPES-1105 task cards, the proposed actions queue and the approver notice (CI run 44023050296): 86 files, 1856 tests.
// DPES-1105 phases 2 to 6 (CI run 44084375506): 116 files, 2134 tests.
// Workstream plan editing (OX-W9) adds 20 tests to existing files: 12 fixtureGuard rows, one surface mapping test and seven
// view tests. The tree measured 123 files and 2230 tests before it (the pin had drifted), so `npx vitest run` counts 123 and 2250.
// Project replies extend running-turn and cancellation cases to project coordinators.
// Full local run after integrating main's notification tests: 124 files, 2264 tests.
// The home directory setting adds HomeDirectorySection.test.tsx (8 tests), 1 in screens1.test.tsx and 2 title tests in
// model.test.ts: 125 files, 2275 tests, as a full `npx vitest run` counted them.
// Dispatch defaults add five tests in existing files: 2275 becomes 2280.
// Queued-task feedback, with the reported disk hold and wait reasons in task lists, adds 23 tests in existing files. A full
// `npm --prefix apps/desktop test` after integrating main measured 125 files and 2303 tests.
// Popovers that stay open while WebKit moves focus during a press add two tests to ConversationActions.test.tsx: 2305.
// Workstream list actions replace the create-form tests and add prioritize and archive tests: 2313 becomes 2315.
// Repository Settings on main adds two tests; the capability picker replaces tests in place.
// Wake outcome reporting (PR #171) adds 5 more that the 2317 pin did not count.
// Removing the ResourceStrip color-scale note test drops one.
// The Cancel allowlist (DPES-1106) adds 5 tests across the agents, task detail and activity panel views, as the Windows desktop gate measured: 2326.
// Chat markdown and one-click copy add seven tests in existing files (primitives, WakeNotice, WorkspaceView): 2333.
// Light-mode ambient background adds four engine tests and the Global tasks target filter adds three: 2354.
// The top bar open task count adds one test in TopBar.test.tsx: 2355.
// The archived workstreams box and the settings layout (DPES-1044) add one file and six tests: 126 files, 2361 tests, as the
// Windows desktop-vitest gate measured them in CI run 44380025826.
// DPES-1108 phase 3 (Needs your decision list, decision.ask card) adds one file: 127 files, 2379 tests (CI run 44475972106).
// DPES-1108 phase 4 (the task.message card) adds one test in actionCardModel.test.ts: 2380 tests.
// DPES-1108 phase 5 (autonomy switch section, done-on-its-own cards, discard) adds one file: 128 files, 2394 tests.
// DPES-1108 phase 6 (Preferences section, memory.set card) adds 11 tests: 2405.
// DPES-1108 phase 7 (coordinator panel, route and report cards) adds one file and 28 tests: 129 files, 2433.
// DPES-1108 phase 8 (pull request links under replies) adds PullRequestLinks.test.tsx (2 tests) and 3 mapper tests in
// conversationLive.test.ts: 128 files, 2385 tests, as a full local `npx vitest run` counted them.
// Together in the phase 9 integration: 131 files, 2438 tests, as a full local `npx vitest run` counted them.
// With main (DPES-1106 agent thinking) merged in: 133 files, 2458 tests, as a full local `npx vitest run` counted them.
// DPES-1106 task window: 133 files, 2474 tests, as the desktop-vitest gate measured them on macOS and Windows in CI run 44640878437.
// DPES-1088 task window chat: two tests replace the old output and no-session tests with three, plus one Enter-key test: 133 files, 2476 tests, as the desktop-vitest gate measured them on macOS and Windows in CI run 44659055303.
// The Waiting and Finished tabs in Agents & Sessions (DPES-1044) add 4 tests in existing files: 2480, as a full `npx vitest run` counted them.
// DPES-1044 desktop update prompt adds DesktopUpdates.test.tsx (1 file, 15 tests): 134 files, 2495 tests.
// The two-screen Workstreams page (DPES-1044) replaces and adds 14 tests in existing files: 2509, as a full `npx vitest run` counted them.
// Live agent activity (DPES-1044) adds 3 files and 30 tests net: 137 files, 2539 tests, as a full `npx vitest run` counted them.
// Force start (DPES-1044) adds 2 tests in an existing file: 2543, as a full `npx vitest run` counted them.
// The calm shell (DPES-1044) drops the sidebar tree, four-count line and strip-while-healthy tests, and adds the need-you, nav badge, strip and Appearance tests (one new file): 138 files, 2537, as a full `npx vitest run` counted them.
// The Workspace redesign (DPES-1044) removes the activity popover tests and adds LimitsMachineLoadSection.test.tsx plus the workspace header, brief, card and picker tests: 138 files, 2507, as a full `npx vitest run` counted them.
// The Agents redesign (DPES-1044) replaces the Agents view tests and adds AgentsInventorySection.test.tsx and inventoryModel.test.ts (2 new files): 140 files, 2505, as a full `npx vitest run` counted them.
// The Review queue (DPES-1044) folds six Review view test files into three and adds Settings notifications tests (net 2 fewer files, 45 fewer tests): 138 files, 2460 tests, as a full `npx vitest run` counted them (the Boise timezone test fails only locally).
// Simplified Project and Task screens (DPES-1044) add AgentDetectionSection.test.tsx and net 16 tests in existing files: 139 files, 2476 tests, as a full `npx vitest run` counted them.
// The Settings redesign (DPES-1044) replaces the three switch section tests with AutonomySection.test.tsx and adds SettingsPages.test.tsx and page, dialog and Save tests: 138 files, 2490 tests, as a full `npx vitest run` counted them.
// The task Chat tab (DPES-1044) turns SessionActivity.test.tsx into useSessionActivity.test.tsx (one test fewer) and replaces two Activity tab tests in AgentsView.test.tsx with nine Chat tests: 138 files, 2496 tests, from the test count in the diff; CI evidence confirms it.
// Chat times (DPES-1044) add instant.test.ts and time.test.tsx and one mapEntries test: 140 files, 2505 tests, as a full `npx vitest run` counted them.
// Provider override pickers (DPES-1105) add two tests to SettingsView.test.tsx: 140 files, 2507 tests, as CI counted them.
export const VITEST = { files: 140, tests: 2507 };
// DPES-1106 moves the topbar usage popover test from this vitest gate to e2e/topbar-usage-viewport.spec.ts (Playwright).
// Main had 45 e2e tests; moving that one out leaves 44. The chat model choice (DPES-1105) adds one real-daemon e2e test: 45
// (CI run 43734333632). The permission level control (DPES-1105) adds one real-daemon e2e test: 46. CI run 43774378675 ran
// 46 e2e tests (`Tests 1 failed | 45 passed (46)`, before the fix in the next commit).
// DPES-1105 approval card (PR #118) adds one real-daemon e2e test (CI run 43994386560: 51 tests in 10 files): 51.
// DPES-1105 approver credential adds one real-daemon e2e test (CI run 44023050296: 52 tests): 52.
// Project coordinator replies add one real-daemon UI scenario.
// Live agent activity (DPES-1044) adds one real-daemon e2e test: 54.
export const E2E_TESTS = 54;

// The harness-free lifecycle test prints `STEP 1` .. `STEP n`, then `lifecycle: PASS`.
export const LIFECYCLE_STEPS = 8;

// The `single_match` warnings in orchestrator-runtime that clippy-scoped reports as known
// baseline. They are `#[cfg(windows)]` code, so only the Windows gate sees them. A change to
// supervisor.rs that moves them makes the gate report them as NEW (reported, not judged).
export const CLIPPY_BASELINE = {
  file: "crates/orchestrator-runtime/src/supervisor.rs",
  lines: [414, 437],
};

// Matches a clippy location under either path separator.
export const clippyBaselinePattern = ({ file, lines } = CLIPPY_BASELINE) =>
  `${file.split("/").join("[\\\\/]").replaceAll(".", "\\.")}:(?:${lines.join("|")}):\\d+`;
