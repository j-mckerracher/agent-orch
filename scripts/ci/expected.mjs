// Every expected count and baseline in the native gates, in one place.
//
// When a merge adds or removes tests, or moves the baseline clippy sites, edit THIS file and the
// matching name list (windows-test-names.json / macos-test-names.json), then run
// `node --test "scripts/**/*.test.mjs"`: expected.test.mjs checks that the numbers agree with each
// other and with the name lists. Never edit a value to fit a run without a reason in the commit.

// Windows: the published Priority 11r3 runbook's derived values (not yet measured on a runner).
// `sequence` is the `running N tests` series, one entry per test binary, in order.
export const WINDOWS = {
  ipc: { passed: 45, sequence: [45, 0] },
  daemon: { passed: 46, sequence: [42, 4, 0] },
  coordinatorReview: { passed: 4, sequence: [4] },
  windowsTransport: { passed: 13, sequence: [13], filteredOut: 32 },
  tauri: { passed: 9, sequence: [9, 0, 0] },
};

// macOS: measured at source commit efa5e8af (M6-02 owner report). Not Windows evidence.
export const MACOS = {
  ipc: { passed: 45, sequence: [45, 0] },
  daemon: { passed: 72, sequence: [59, 4, 9] },
  coordinatorReview: { passed: 4, sequence: [4] },
  workspace: { passed: 638, ignored: 1 },
  tauri: { passed: 10, sequence: [10, 0, 0] },
  e2eFiles: 1,
};

// Platform independent.
export const VITEST = { files: 15, tests: 287 };
export const E2E_TESTS = 19;

// The `single_match` warnings in orchestrator-runtime that clippy-scoped reports as known
// baseline. They are `#[cfg(windows)]` code, so only the Windows gate sees them. A change to
// supervisor.rs that moves them makes the gate report them as NEW (reported, not judged).
export const CLIPPY_BASELINE = {
  file: "crates/orchestrator-runtime/src/supervisor.rs",
  lines: [401, 424],
};

// Matches a clippy location under either path separator.
export const clippyBaselinePattern = ({ file, lines } = CLIPPY_BASELINE) =>
  `${file.split("/").join("[\\\\/]").replaceAll(".", "\\.")}:(?:${lines.join("|")}):\\d+`;
