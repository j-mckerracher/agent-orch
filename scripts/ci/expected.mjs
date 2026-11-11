// Every expected count and baseline in the native gates, in one place.
//
// When a merge adds or removes tests, or moves the baseline clippy sites, edit THIS file and the
// matching name list (windows-test-names.json / macos-test-names.json), then run
// `node --test "scripts/**/*.test.mjs"`: expected.test.mjs checks that the numbers agree with each
// other and with the name lists. Never edit a value to fit a run without a reason in the commit.

// Windows: the published Priority 11r3 runbook's derived values (not yet measured on a runner).
// `sequence` is the `running N tests` series, one entry per test binary, in order.
export const WINDOWS = {
  ipc: { passed: 46, sequence: [46, 0] },
  daemon: { passed: 57, sequence: [52, 4, 0, 1] },
  coordinatorReview: { passed: 4, sequence: [4] },
  windowsTransport: { passed: 13, sequence: [13], filteredOut: 33 },
  tauri: { passed: 14, sequence: [14, 0, 0] },
  // Derived from the sources' cfg gates and checked against the Priority 12r2 Windows run, which
  // ran git-lib at 46 (the 47 once derived counted the two cfg(unix) tests and missed the
  // cfg(windows) one) and cleanup_safety at 3 (two of its five tests are cfg(unix)).
  // git-lib: 49 = 47 portable tests + 2 cfg(windows) tests (macOS: 47 + 2 cfg(unix) = 49).
  gitLib: { passed: 49, sequence: [49] },
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
  store: { passed: 100, ignored: 1, sequence: [33, 9, 2, 13, 8, 11, 3, 3, 3, 11, 4, 1, 0] },
  lockTakeover: { passed: 1, sequence: [1], filteredOut: 32 },
  // No cfg gate in the crate: the same 28 as the Priority 12r3 Windows run (6 + 7 + 15).
  diagnostics: { passed: 28, sequence: [0, 6, 7, 15, 0] },
};

// macOS: measured on the Priority 12 integration merge (M6-01 + M6-02 + M6-03 on the M5-05 round 5
// source). Not Windows evidence. daemon is unit, coordinator_review, daemon.rs, diagnostics_log.
export const MACOS = {
  ipc: { passed: 46, sequence: [46, 0] },
  daemon: { passed: 88, sequence: [73, 4, 10, 1] },
  coordinatorReview: { passed: 4, sequence: [4] },
  workspace: { passed: 726, ignored: 1 },
  tauri: { passed: 16, sequence: [16, 0, 0] },
  gitLib: { passed: 49, sequence: [49] },
  cliCleanupSafety: { passed: 4, sequence: [4] },
  privateFs: { passed: 8, sequence: [8, 0] },
  runtimeCommandLine: { passed: 8, sequence: [8] },
  cliCommandLine: { passed: 1, sequence: [1] },
  store: { passed: 104, ignored: 1, sequence: [35, 9, 2, 13, 8, 11, 3, 4, 3, 11, 5, 1, 0] },
  lockTakeover: { passed: 1, sequence: [1], filteredOut: 34 },
  diagnostics: { passed: 28, sequence: [0, 6, 7, 15, 0] },
  e2eFiles: 1,
};

// `node --test "scripts/**/*.test.mjs"`: no test is platform-gated, so both platforms run the same number.
export const NODE_SELF_TESTS = { passed: 95 };

// Platform independent.
export const VITEST = { files: 15, tests: 290 };
export const E2E_TESTS = 19;

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
