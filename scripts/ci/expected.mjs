// Every expected count and baseline in the native gates, in one place.
//
// When a merge adds or removes tests, or moves the baseline clippy sites, edit THIS file and the
// matching name list (windows-test-names.json / macos-test-names.json), then run
// `node --test "scripts/**/*.test.mjs"`: expected.test.mjs checks that the numbers agree with each
// other and with the name lists. Never edit a value to fit a run without a reason in the commit.

// Windows: the published Priority 11r3 runbook's derived values (not yet measured on a runner).
// `sequence` is the `running N tests` series, one entry per test binary, in order.
export const WINDOWS = {
  // The M7 merge adds 9 portable ipc tests (host, queue, workstreams).
  // Measured on Windows (b44f88a): ipc 55 (13 of them in windows_transport); the M7-final-r1
  // large-write test makes 56. The daemon library measured 118; conversation_runtime ran 12
  // tests (10 passed, 1 failed, 1 ignored) and the run stopped there, so the daemon total is
  // still derived: 118 + 11 + 2 + 4 + 0 + 1 + 2 = 138 once M7-08 passes.
  // DPES-1105 adds 2 portable ipc tests (history capability), so 56 becomes 58.
  ipc: { passed: 58, sequence: [58, 0] },
  // Derived from cfg gates: the library loses executor::tests,
  // conversation_orphans, conversation_agent::launch and 4 other unix-only tests and gains one
  // cfg(windows) test; conversation_runtime loses 2 unix tests (11 passed + 1 #[ignore]);
  // tests/daemon.rs is cfg(unix), so it runs 0 tests.
  // DPES-1105 adds 8 portable daemon unit tests (conversation clear, removal and the capability), 118 becomes 126.
  // The AI usage module (api::usage_local, three portable tests) landed on main in PR #43 without this manifest, 146 becomes 149.
  // The cfg(windows) Copilot refusal test (api_tests.rs) adds one more: 149 becomes 150 and the unit binary 130.
  // The Copilot executor tests are unix-only (under executor::tests), so they do not count here.
  daemon: { passed: 150, ignored: 1, sequence: [130, 12, 2, 4, 0, 1, 2] },
  coordinatorReview: { passed: 4, sequence: [4] },
  // The ipc lib is 58 on Windows (56 plus the 2 portable history tests), so 58 - 14 = 44 are filtered out.
  windowsTransport: { passed: 14, sequence: [14], filteredOut: 44 },
  tauri: { passed: 14, sequence: [14, 0, 0] },
  // Derived from the sources' cfg gates and checked against the Priority 12r2 Windows run, which
  // ran git-lib at 46 (the 47 once derived counted the two cfg(unix) tests and missed the
  // cfg(windows) one) and cleanup_safety at 3 (two of its five tests are cfg(unix)).
  // git-lib: 58 = 56 portable tests + 2 cfg(windows) tests (macOS: 56 + 2 cfg(unix) = 58).
  gitLib: { passed: 58, sequence: [58] },
  cliCleanupSafety: { passed: 3, sequence: [3] },
  // 4 portable tests + 7 in the cfg(windows) module (the DACL tests; the inherited-ACE regression for
  // `.orchestrator.lock` is one of them).
  privateFs: { passed: 11, sequence: [11, 0] },
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
  store: { passed: 167, ignored: 1, sequence: [65, 9, 2, 13, 1, 10, 11, 9, 11, 3, 3, 3, 11, 4, 1, 12, 0] },
  // Windows lib: 65 tests compile (the 2 cfg(unix) tests are excluded), so 65 - 1 = 64 are filtered out.
  lockTakeover: { passed: 1, sequence: [1], filteredOut: 64 },
  // No cfg gate in the crate: the same 28 as the Priority 12r3 Windows run (6 + 7 + 15).
  diagnostics: { passed: 28, sequence: [0, 6, 7, 15, 0] },
};

// macOS: measured on the Priority 12 integration merge (M6-01 + M6-02 + M6-03 on the M5-05 round 5
// source). Not Windows evidence. daemon is unit, coordinator_review, daemon.rs, diagnostics_log.
export const MACOS = {
  // Measured on the M7 final integration merge (macOS, this branch).
  // Measured on the DPES-1105 branch: 55 plus the two history capability tests.
  ipc: { passed: 57, sequence: [57, 0] },
  // The new fast-exit executor test is Unix-only; Windows counts remain unchanged.
  // Measured on the DPES-1105 branch: the unit binary gains the 8 history tests (176 becomes 184), then the
  // Claude Code model alias argv test (184 becomes 185). The AI usage module (api::usage_local, three tests) landed on
  // main in PR #43: 218 becomes 221 and the unit binary 188. The three Copilot executor tests then make the unit binary
  // 191 and the total 224. These executor tests are unix-only, so Windows is unchanged by them.
  daemon: { passed: 224, ignored: 1, sequence: [191, 14, 2, 4, 11, 1, 2] },
  coordinatorReview: { passed: 4, sequence: [4] },
  // Measured on the DPES-1105 branch (`cargo test --locked --workspace`): 998 plus 21 history tests, then the
  // 2 model-picker tests (the Claude alias catalog and the rejected-model adapter failure): 1021.
  // DPES-1106 adds 2 store tests (the publish regression and the racing-acquires stress): 1021 becomes 1023.
  // Measured on the rebased DPES-1105 branch (`cargo test --locked --workspace --no-fail-fast`, every binary's `test
  // result` summed): 1115 passed, 2 ignored. That base already holds the AI usage tests from PR #43 (1050).
  workspace: { passed: 1115, ignored: 2 },
  tauri: { passed: 16, sequence: [16, 0, 0] },
  gitLib: { passed: 58, sequence: [58] },
  cliCleanupSafety: { passed: 4, sequence: [4] },
  privateFs: { passed: 8, sequence: [8, 0] },
  runtimeCommandLine: { passed: 8, sequence: [8] },
  cliCommandLine: { passed: 1, sequence: [1] },
  // Measured on the DPES-1106 branch (macOS `cargo test -p orchestrator-store`): the lib has 67 tests and data_lock.rs 9 (8 passed, 1 ignored).
  store: { passed: 171, ignored: 1, sequence: [67, 9, 2, 13, 1, 10, 11, 9, 11, 3, 4, 3, 11, 5, 1, 12, 0] },
  // Measured on the DPES-1106 branch: the store lib has 67 tests, so 66 are filtered out.
  lockTakeover: { passed: 1, sequence: [1], filteredOut: 66 },
  diagnostics: { passed: 28, sequence: [0, 6, 7, 15, 0] },
  e2eFiles: 8,
};

// `node --test "scripts/**/*.test.mjs"`: no test is platform-gated, so both platforms run the same number. The 26
// tests of install-lib.test.mjs build paths with node:path (checked under scripts/release/win32-path-sim).
// 116 = 97 + the 13 tests of windows-jobs.test.mjs and the 2 of prune-target.test.mjs (DPES-1106) + the 4 tests of
// usage-reader-smoke.test.mjs (DPES-1105).
export const NODE_SELF_TESTS = { passed: 116 };

// Platform independent. The target-picker selection fix adds 1 file and 7 tests (1549 becomes 1556, 68 files). The
// Copilot provider adds 4 test files and 15 tests: 72 files and 1571 tests.
export const VITEST = { files: 72, tests: 1571 };
// DPES-1106 moves the topbar usage popover test from this vitest gate to e2e/topbar-usage-viewport.spec.ts (Playwright).
// Main had 45 e2e tests; moving that one out leaves 44.
export const E2E_TESTS = 44;

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
