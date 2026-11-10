// The native gates, as data. Windows mirrors the published Priority 11r3 runbook
// (docs/evidence/priority-11r3-windows-acceptance-efa5e8a-runbook.md on branch
// docs/priority-11r3-windows-acceptance-efa5e8a): same commands, same order, same derived counts.
// macOS runs the same commands with counts measured on macOS (they differ by `cfg`), plus the
// two macOS-only gates the M5-05r3 owner measured: the whole workspace and the real-daemon e2e.
//
// Every expected count and the clippy baseline live in expected.mjs; this file holds none. They
// are never edited to fit a run. If a count changes, the change needs a reason in the commit that
// edits it.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { CLIPPY_BASELINE, E2E_TESTS, MACOS, VITEST, WINDOWS, clippyBaselinePattern } from "./expected.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const names = (file) => JSON.parse(readFileSync(join(here, file), "utf8"));

const UI = "apps/desktop";
const TAURI = "apps/desktop/src-tauri";
const lifecyclePattern = "^lifecycle: PASS[ \\t]*\\r?$";

export const UNPROVEN_ROWS = [
  "unproven-second-user: named-pipe denial to a second local user",
  "unproven-remote-client: rejection of a genuinely remote client",
  "unproven-slow-reader-64kib: a daemon write above the 64 KiB pipe buffer to a slow reader",
  "unproven-e2e-windows: the desktop e2e suite needs a Unix socket and is not run on Windows",
  "unproven-tauri-webview-click-path: webview and React click path through the approval flow",
  "unproven-gui-install-launch: installing the installer and launching the app",
  "unproven-gui-task-survives-close: a daemon-owned task surviving window close and reopen",
  "unproven-gui-event-reconnect: event-stream reconnect at the saved cursor after reopen",
  "unproven-stale-token-relaunch-timing: stale-token relaunch timing on Windows",
  "unproven-daemon-idle-behaviour: 60 s idle exit and subscription keep-alive on the real app",
  "unproven-daemon-kill-and-sidecar-failure: daemon kill and sidecar start failure",
];

export function gatesFor(platform) {
  const win = platform === "win32";
  if (!win && platform !== "darwin") throw new Error(`unsupported platform ${platform}`);
  const exe = win ? ".exe" : "";
  const counts = win ? names("windows-test-names.json").windows : names("macos-test-names.json").macos;
  const want = win ? WINDOWS : MACOS;

  const gates = [
    {
      name: "daemon-build", mode: "build", cwd: ".", exe: "cargo",
      args: ["build", "--locked", "-p", "orchestrator-daemon"],
      crates: ["orchestrator-ipc", "orchestrator-store", "orchestrator-engine", "orchestrator-daemon"],
      artifacts: [{ target: `debug/orchestrator-daemon${exe}`, record: "daemon-binary.txt" }],
    },
    {
      name: "ipc-tests", mode: "test", cwd: ".", exe: "cargo", isolated: true, timeout: 1800,
      args: ["test", "--locked", "-p", "orchestrator-ipc"],
      expect: counts.ipc, passed: want.ipc.passed, ignored: 0, runningSequence: want.ipc.sequence,
    },
    {
      name: "daemon-tests", mode: "test", cwd: ".", exe: "cargo", isolated: true, timeout: 1800,
      args: ["test", "--locked", "-p", "orchestrator-daemon"],
      expect: counts.daemon, pattern: lifecyclePattern,
      passed: want.daemon.passed, ignored: 0, runningSequence: want.daemon.sequence,
    },
    {
      name: "daemon-lifecycle", mode: "lifecycle", cwd: ".", exe: "cargo", isolated: true, timeout: 1800,
      args: ["test", "--locked", "-p", "orchestrator-daemon", "--test", "lifecycle", "--", "--nocapture"],
    },
    {
      name: "coordinator-review-tests", mode: "test", cwd: ".", exe: "cargo", isolated: true, timeout: 600,
      args: ["test", "--locked", "-p", "orchestrator-daemon", "--test", "coordinator_review"],
      expect: counts["coordinator-review"], passed: want.coordinatorReview.passed, ignored: 0,
      runningSequence: want.coordinatorReview.sequence, filteredOut: 0,
    },
  ];

  // Per-crate suites the Priority 12r2 runbook ran by hand or never ran. Windows counts are derived
  // from the sources' cfg gates (see expected.mjs); the filters name tests, not test binaries.
  const suite = (name, args, key, list, timeout = 1800) => ({
    name, mode: "test", cwd: ".", exe: "cargo", isolated: true, timeout, args: ["test", "--locked", ...args],
    expect: counts[list], passed: want[key].passed, ignored: 0, runningSequence: want[key].sequence,
  });
  gates.push(
    suite("git-lib-tests", ["-p", "orchestrator-git", "--lib"], "gitLib", "git-lib"),
    suite("cli-cleanup-safety-tests", ["-p", "orchestrator-cli", "--test", "cleanup_safety"], "cliCleanupSafety", "cli-cleanup-safety"),
    suite("private-fs-tests", ["-p", "orchestrator-private-fs"], "privateFs", "private-fs", 600),
    suite("runtime-command-line-tests", ["-p", "orchestrator-runtime", "--lib", "command_line"], "runtimeCommandLine", "runtime-command-line", 600),
    suite(
      "cli-command-line-tests",
      ["-p", "orchestrator-cli", "--lib", "--", "os_command_line_limit", "65000_character_objective"],
      "cliCommandLine", "cli-command-line", 600,
    ),
  );

  if (win) {
    gates.push({
      name: "windows-transport-tests", mode: "test", cwd: ".", exe: "cargo", isolated: true, timeout: 1800,
      args: ["test", "--locked", "-p", "orchestrator-ipc", "windows_transport"],
      expect: counts["windows-transport"], passed: WINDOWS.windowsTransport.passed, ignored: 0,
      runningSequence: WINDOWS.windowsTransport.sequence, filteredOut: WINDOWS.windowsTransport.filteredOut,
    });
  }

  gates.push(
    {
      // The `single_match` warnings in orchestrator-runtime (CLIPPY_BASELINE) are the known baseline; any other
      // diagnostic is reported in the summary and does not change the verdict.
      name: "clippy-scoped", mode: "lint", cwd: ".", exe: "cargo",
      args: ["clippy", "--locked", "-p", "orchestrator-ipc", "-p", "orchestrator-daemon", "--all-targets"],
      crates: ["orchestrator-ipc", "orchestrator-daemon"],
      baseline: clippyBaselinePattern(CLIPPY_BASELINE),
    },
  );

  if (!win) {
    gates.push({
      name: "workspace-tests", mode: "test", cwd: ".", exe: "cargo", isolated: true, timeout: 3600,
      args: ["test", "--locked", "--workspace"],
      passed: MACOS.workspace.passed, ignored: MACOS.workspace.ignored,
    });
  }

  gates.push(
    { name: "desktop-npm-ci", mode: "exit", cwd: UI, exe: "npm", args: ["ci"], timeout: 1200 },
    { name: "desktop-tsc", mode: "exit", cwd: UI, exe: "npx", args: ["tsc", "-b"], timeout: 900, needs: ["desktop-npm-ci"] },
    {
      name: "desktop-build", mode: "exit", cwd: UI, exe: "npm", args: ["run", "build"], timeout: 900,
      pattern: "\\bbuilt in \\d", needs: ["desktop-npm-ci"],
    },
    {
      name: "desktop-vitest", mode: "vitest", cwd: UI, exe: "npx", args: ["vitest", "run"], timeout: 1200,
      files: VITEST.files, tests: VITEST.tests, needs: ["desktop-npm-ci"],
    },
  );

  if (!win) {
    gates.push({
      // Real daemon over a Unix socket: not runnable on Windows (see UNPROVEN_ROWS).
      name: "desktop-e2e", mode: "vitest", cwd: UI, exe: "npm", args: ["run", "test:e2e"], timeout: 1800,
      files: MACOS.e2eFiles, tests: E2E_TESTS, needs: ["desktop-npm-ci", "daemon-build"],
      env: { ORCH_DAEMON_BIN: { target: "debug/orchestrator-daemon" } },
    });
  }

  gates.push(
    {
      name: "tauri-build", mode: "build", cwd: TAURI, exe: "cargo", args: ["build", "--locked"], timeout: 3600,
      crates: ["agent-orch-desktop"], needs: ["desktop-build"],
      artifacts: [{ target: `debug/agent-orch-desktop${exe}`, record: "tauri-binary.txt" }],
    },
    {
      name: "tauri-tests", mode: "test", cwd: TAURI, exe: "cargo", isolated: true, timeout: 3600,
      args: ["test", "--locked"], needs: ["desktop-build"],
      expect: counts.tauri, passed: want.tauri.passed, ignored: 0, runningSequence: want.tauri.sequence,
    },
    // Removes only this package's cached output so the next gate really lints it.
    { name: "tauri-clippy-clean", mode: "exit", cwd: TAURI, exe: "cargo", args: ["clean", "-p", "agent-orch-desktop"], timeout: 900 },
    {
      name: "tauri-clippy", mode: "lint", cwd: TAURI, exe: "cargo", timeout: 3600, needs: ["desktop-build"],
      args: ["clippy", "--locked", "--all-targets", "--verbose"],
      crateNames: ["agent_orch_desktop_lib", "agent_orch_desktop"],
    },
  );

  gates.push(
    win
      ? {
          name: "desktop-package-win", mode: "exit", cwd: UI, exe: "npm", args: ["run", "package:win"], timeout: 3600,
          needs: ["desktop-npm-ci"],
          artifacts: [{ target: "release/bundle/nsis/Agent Orchestrator_0.1.0_x64-setup.exe", record: "installer.txt" }],
        }
      : {
          name: "desktop-package-mac", mode: "exit", cwd: UI, exe: "npm", args: ["run", "package:mac"], timeout: 3600,
          needs: ["desktop-npm-ci"],
          artifacts: [
            { target: "release/bundle/macos/Agent Orchestrator.app/Contents/MacOS/agent-orch-desktop", record: "app-binary.txt" },
            { target: "release/bundle/macos/Agent Orchestrator.app/Contents/MacOS/orchestrator-daemon", record: "app-sidecar.txt" },
          ],
        },
  );
  return gates;
}
