// The native gates, as data. Windows mirrors the published Priority 11r3 runbook
// (docs/evidence/priority-11r3-windows-acceptance-efa5e8a-runbook.md on branch
// docs/priority-11r3-windows-acceptance-efa5e8a): same commands, same order, same derived counts.
// macOS runs the same commands with counts measured on macOS (they differ by `cfg`), plus the
// two macOS-only gates the M5-05r3 owner measured: the whole workspace and the real-daemon e2e.
//
// Expected counts are never edited to fit a run. If a count changes, the change needs a reason in
// the commit that edits it.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

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
  const mac = win ? null : MACOS_COUNTS;

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
      expect: counts.ipc, passed: 45, ignored: 0, runningSequence: [45, 0],
    },
    {
      name: "daemon-tests", mode: "test", cwd: ".", exe: "cargo", isolated: true, timeout: 1800,
      args: ["test", "--locked", "-p", "orchestrator-daemon"],
      expect: counts.daemon, pattern: lifecyclePattern,
      ...(win
        ? { passed: 46, ignored: 0, runningSequence: [42, 4, 0] }
        : { passed: mac.daemonTests, ignored: 0, runningSequence: mac.daemonSequence }),
    },
    {
      name: "daemon-lifecycle", mode: "lifecycle", cwd: ".", exe: "cargo", isolated: true, timeout: 1800,
      args: ["test", "--locked", "-p", "orchestrator-daemon", "--test", "lifecycle", "--", "--nocapture"],
    },
    {
      name: "coordinator-review-tests", mode: "test", cwd: ".", exe: "cargo", isolated: true, timeout: 600,
      args: ["test", "--locked", "-p", "orchestrator-daemon", "--test", "coordinator_review"],
      expect: counts["coordinator-review"], passed: 4, ignored: 0, runningSequence: [4], filteredOut: 0,
    },
  ];

  if (win) {
    gates.push({
      name: "windows-transport-tests", mode: "test", cwd: ".", exe: "cargo", isolated: true, timeout: 1800,
      args: ["test", "--locked", "-p", "orchestrator-ipc", "windows_transport"],
      expect: counts["windows-transport"], passed: 13, ignored: 0, runningSequence: [13], filteredOut: 32,
    });
  }

  gates.push(
    {
      // The two `single_match` warnings in orchestrator-runtime are the known baseline; any other
      // diagnostic is reported in the summary and does not change the verdict.
      name: "clippy-scoped", mode: "lint", cwd: ".", exe: "cargo",
      args: ["clippy", "--locked", "-p", "orchestrator-ipc", "-p", "orchestrator-daemon", "--all-targets"],
      crates: ["orchestrator-ipc", "orchestrator-daemon"],
      baseline: "crates[\\\\/]orchestrator-runtime[\\\\/]src[\\\\/]supervisor\\.rs:(?:401|424):\\d+",
    },
  );

  if (!win) {
    gates.push({
      name: "workspace-tests", mode: "test", cwd: ".", exe: "cargo", isolated: true, timeout: 3600,
      args: ["test", "--locked", "--workspace"],
      passed: mac.workspacePassed, ignored: mac.workspaceIgnored,
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
      files: 15, tests: 287, needs: ["desktop-npm-ci"],
    },
  );

  if (!win) {
    gates.push({
      // Real daemon over a Unix socket: not runnable on Windows (see UNPROVEN_ROWS).
      name: "desktop-e2e", mode: "vitest", cwd: UI, exe: "npm", args: ["run", "test:e2e"], timeout: 1800,
      files: mac.e2eFiles, tests: 19, needs: ["desktop-npm-ci", "daemon-build"],
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
      expect: counts.tauri, passed: win ? 9 : mac.tauriTests, ignored: 0,
      runningSequence: win ? [9, 0, 0] : [mac.tauriTests, 0, 0],
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

// Measured on macOS at the source commit efa5e8af (see the M6-02 owner report). Not Windows evidence.
const MACOS_COUNTS = {
  daemonTests: 72,
  daemonSequence: [59, 4, 9],
  workspacePassed: 638,
  workspaceIgnored: 1,
  tauriTests: 10,
  e2eFiles: 1,
};
