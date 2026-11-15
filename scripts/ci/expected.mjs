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
  // The chat model choice (DPES-1105) adds 1 portable ipc test (selection capability): 58 becomes 59 (CI run 43734333632).
  ipc: { passed: 59, sequence: [59, 0] },
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
  daemon: { passed: 170, ignored: 1, sequence: [138, 24, 2, 4, 0, 1, 2] },
  coordinatorReview: { passed: 4, sequence: [4] },
  // The ipc lib is 59 on Windows (58 plus the selection capability test), so 59 - 14 = 45 are filtered out (CI run 43734333632).
  windowsTransport: { passed: 14, sequence: [14], filteredOut: 45 },
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
  // DPES-1105 permission levels (PR #80, CI run 43767887059, measured on Windows): the store lib is 68 (up from 65) and
  // the store totals 170 passed plus 1 ignored. The lib gains 3 portable tests (permission level and migration checks).
  store: { passed: 170, ignored: 1, sequence: [68, 9, 2, 13, 1, 10, 11, 9, 11, 3, 3, 3, 11, 4, 1, 12, 0] },
  // Windows lib: 68 tests compile (the 2 cfg(unix) tests are excluded), so 68 - 1 = 67 are filtered out.
  lockTakeover: { passed: 1, sequence: [1], filteredOut: 67 },
  // No cfg gate in the crate: the same 28 as the Priority 12r3 Windows run (6 + 7 + 15).
  diagnostics: { passed: 28, sequence: [0, 6, 7, 15, 0] },
};

// macOS: measured on the Priority 12 integration merge (M6-01 + M6-02 + M6-03 on the M5-05 round 5
// source). Not Windows evidence. daemon is unit, coordinator_review, daemon.rs, diagnostics_log.
export const MACOS = {
  // Measured on the M7 final integration merge (macOS, this branch).
  // Measured on the DPES-1105 branch: 55 plus the two history capability tests.
  // The chat model choice (DPES-1105) adds the selection capability test: 57 becomes 58 (CI run 43734333632).
  ipc: { passed: 58, sequence: [58, 0] },
  // The new fast-exit executor test is Unix-only; Windows counts remain unchanged.
  // Measured on the DPES-1105 branch: the unit binary gains the 8 history tests (176 becomes 184), then the
  // Claude Code model alias argv test (184 becomes 185). The AI usage module (api::usage_local, three tests) landed on
  // main in PR #43: 218 becomes 221 and the unit binary 188. The three Copilot executor tests then make the unit binary
  // 191 and the total 224. These executor tests are unix-only, so Windows is unchanged by them.
  // The chat model choice (DPES-1105) adds 6 unit tests and 8 process tests in conversation_runtime (fake agent): 224
  // becomes 238, the unit binary 191 becomes 197 and conversation_runtime 14 becomes 22 (CI run 43734333632).
  // DPES-1105 permission levels (PR #80, CI run 43767887059, measured on macOS): the unit binary 197 becomes 200 (3 in-process
  // daemon tests) and conversation_runtime 22 becomes 26 (4 process tests), so 238 becomes 245 passed plus 1 ignored.
  daemon: { passed: 245, ignored: 1, sequence: [200, 26, 2, 4, 11, 1, 2] },
  coordinatorReview: { passed: 4, sequence: [4] },
  // Measured on the DPES-1105 branch (`cargo test --locked --workspace`): 998 plus 21 history tests, then the
  // 2 model-picker tests (the Claude alias catalog and the rejected-model adapter failure): 1021.
  // DPES-1106 adds 2 store tests (the publish regression and the racing-acquires stress): 1021 becomes 1023.
  // Measured on the rebased DPES-1105 branch (`cargo test --locked --workspace --no-fail-fast`, every binary's `test
  // result` summed): 1115 passed, 2 ignored. That base already holds the AI usage tests from PR #43 (1050).
  // The bundled usage reader (PR #62) adds 4 tests in orchestrator-local-usage (pin and service): 1119. Measured on a real
  // macOS run of the DPES-1105 fixture-identifier PR.
  // The chat model choice (DPES-1105) adds 15 tests (14 daemon, 1 ipc): 1119 becomes 1134 (CI run 43734333632).
  // The Copilot sandbox gate removal (PR #75) deletes 10 tests (the sandbox probes and the sandbox-gated catalog and launch
  // tests) and adds 1 sign-in test: 1134 becomes 1125 (CI run 43757529505).
  // DPES-1105 permission levels (PR #80, CI run 43767887059, measured on macOS, `cargo test --locked --workspace`): 18 new
  // tests across orchestrator-agents, orchestrator-core, orchestrator-daemon and orchestrator-store, so 1125 becomes 1143.
  workspace: { passed: 1143, ignored: 2 },
  tauri: { passed: 16, sequence: [16, 0, 0] },
  gitLib: { passed: 58, sequence: [58] },
  cliCleanupSafety: { passed: 4, sequence: [4] },
  privateFs: { passed: 8, sequence: [8, 0] },
  runtimeCommandLine: { passed: 8, sequence: [8] },
  cliCommandLine: { passed: 1, sequence: [1] },
  // Measured on the DPES-1106 branch (macOS `cargo test -p orchestrator-store`): the lib has 67 tests and data_lock.rs 9 (8 passed, 1 ignored).
  // DPES-1105 permission levels (PR #80, CI run 43767887059, measured on macOS): the lib is 70 (3 new tests), and the store
  // totals 174 passed plus 1 ignored.
  store: { passed: 174, ignored: 1, sequence: [70, 9, 2, 13, 1, 10, 11, 9, 11, 3, 4, 3, 11, 5, 1, 12, 0] },
  // Measured on the same run: the store lib has 70 tests, so 69 are filtered out.
  lockTakeover: { passed: 1, sequence: [1], filteredOut: 69 },
  diagnostics: { passed: 28, sequence: [0, 6, 7, 15, 0] },
  e2eFiles: 8,
};

// `node --test "scripts/**/*.test.mjs"`: no test is platform-gated, so both platforms run the same number. The 26
// tests of install-lib.test.mjs build paths with node:path (checked under scripts/release/win32-path-sim).
// 112 = 97 + the 13 tests of windows-jobs.test.mjs and the 2 of prune-target.test.mjs (DPES-1106). The 4 tests of the
// usage reader smoke test live in scripts/smoke/ (outside this glob) and run only when the smoke group is selected.
export const NODE_SELF_TESTS = { passed: 112 };

// Platform independent. The target-picker selection fix adds 1 file and 7 tests (1549 becomes 1556, 68 files). The
// Copilot provider adds 4 test files and 15 tests: 72 files and 1571 tests. The Enter-in-search fix adds 2 tests
// (DPES-1105, PR #73): 1573, as the Windows desktop gate measured it in CI run 43726827533.
// The chat model choice (DPES-1105) adds 1 file (ModelPicker.test.tsx) and 7 tests: 73 files and 1580 tests, as the
// desktop gate measured them in CI run 43734333632.
// The permission level control (DPES-1105) adds 3 files (permissionLevel, PermissionLevelPicker and
// WorkspaceView.permission tests) and 46 tests. Counted from a full `vitest run --reporter=json` of apps/desktop: 76 files and 1626 tests.
export const VITEST = { files: 76, tests: 1626 };
// DPES-1106 moves the topbar usage popover test from this vitest gate to e2e/topbar-usage-viewport.spec.ts (Playwright).
// Main had 45 e2e tests; moving that one out leaves 44. The chat model choice (DPES-1105) adds one real-daemon e2e test: 45
// (CI run 43734333632).
export const E2E_TESTS = 45;

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
