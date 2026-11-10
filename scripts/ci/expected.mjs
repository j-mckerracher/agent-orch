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
  tauri: { passed: 13, sequence: [13, 0, 0] },
};

// macOS: measured on the Priority 12 integration merge (M6-01 + M6-02 + M6-03 on the M5-05 round 5
// source). Not Windows evidence. daemon is unit, coordinator_review, daemon.rs, diagnostics_log.
export const MACOS = {
  ipc: { passed: 46, sequence: [46, 0] },
  daemon: { passed: 88, sequence: [73, 4, 10, 1] },
  coordinatorReview: { passed: 4, sequence: [4] },
  workspace: { passed: 721, ignored: 1 },
  tauri: { passed: 15, sequence: [15, 0, 0] },
  e2eFiles: 1,
};

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
