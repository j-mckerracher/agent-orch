import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { CLIPPY_BASELINE, MACOS, NODE_SELF_TESTS, WINDOWS, clippyBaselinePattern } from "./expected.mjs";
import { gatesFor } from "./gates.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const names = (file) => JSON.parse(readFileSync(join(here, file), "utf8"));
const sum = (xs) => xs.reduce((a, b) => a + b, 0);

test("each running-sequence sum equals passed plus ignored (filtered tests excluded)", () => {
  for (const [platform, table] of [["windows", WINDOWS], ["macos", MACOS]]) {
    for (const [key, value] of Object.entries(table)) {
      if (!value.sequence) continue;
      assert.equal(sum(value.sequence), value.passed + (value.ignored ?? 0), `${platform}.${key}`);
    }
  }
});

test("each expected total equals the length of its name list, so a merge updates both", () => {
  const win = names("windows-test-names.json").windows;
  assert.equal(win.ipc.length, WINDOWS.ipc.passed);
  assert.equal(win.daemon.length, WINDOWS.daemon.passed);
  assert.equal(win["coordinator-review"].length, WINDOWS.coordinatorReview.passed);
  assert.equal(win["windows-transport"].length, WINDOWS.windowsTransport.passed);
  assert.equal(win.tauri.length, WINDOWS.tauri.passed);
  const mac = names("macos-test-names.json").macos;
  const suites = [
    ["git-lib", "gitLib"],
    ["cli-cleanup-safety", "cliCleanupSafety"],
    ["private-fs", "privateFs"],
    ["runtime-command-line", "runtimeCommandLine"],
    ["cli-command-line", "cliCommandLine"],
    ["store", "store"],
    ["diagnostics", "diagnostics"],
    ["lock-takeover", "lockTakeover"],
  ];
  for (const [list, key] of suites) {
    assert.equal(win[list].length, WINDOWS[key].passed, `windows ${list}`);
    assert.equal(mac[list].length, MACOS[key].passed, `macos ${list}`);
    assert.equal(new Set(win[list]).size, win[list].length, `windows ${list} has duplicate names`);
  }
  assert.equal(mac.ipc.length, MACOS.ipc.passed);
  assert.equal(mac.daemon.length, MACOS.daemon.passed);
  assert.equal(mac["coordinator-review"].length, MACOS.coordinatorReview.passed);
  assert.equal(mac.tauri.length, MACOS.tauri.passed);
});

test("gates take their counts from expected.mjs", () => {
  const byName = (platform) => Object.fromEntries(gatesFor(platform).map((g) => [g.name, g]));
  const win = byName("win32");
  assert.deepEqual([win["daemon-tests"].passed, win["daemon-tests"].runningSequence], [WINDOWS.daemon.passed, WINDOWS.daemon.sequence]);
  assert.equal(win["windows-transport-tests"].filteredOut, WINDOWS.windowsTransport.filteredOut);
  for (const [gate, key] of [
    ["git-lib-tests", "gitLib"],
    ["cli-cleanup-safety-tests", "cliCleanupSafety"],
    ["private-fs-tests", "privateFs"],
    ["runtime-command-line-tests", "runtimeCommandLine"],
    ["cli-command-line-tests", "cliCommandLine"],
    ["store-tests", "store"],
    ["lock-takeover-test", "lockTakeover"],
    ["diagnostics-tests", "diagnostics"],
  ]) {
    assert.deepEqual([win[gate].passed, win[gate].runningSequence], [WINDOWS[key].passed, WINDOWS[key].sequence], gate);
  }
  const mac = byName("darwin");
  for (const [gate, key] of [["store-tests", "store"], ["lock-takeover-test", "lockTakeover"], ["diagnostics-tests", "diagnostics"]]) {
    assert.deepEqual([mac[gate].passed, mac[gate].runningSequence], [MACOS[key].passed, MACOS[key].sequence], gate);
  }
  for (const table of [win, mac]) {
    assert.equal(table["store-tests"].ignored, 1);
    assert.equal(table["node-self-tests"].passed, NODE_SELF_TESTS.passed);
    assert.equal(table["node-self-tests"].mode, "node-tests");
  }
  assert.equal(gatesFor("win32")[0].name, "node-self-tests");
  assert.equal(gatesFor("darwin")[0].name, "node-self-tests");
  assert.equal(win["lock-takeover-test"].filteredOut, WINDOWS.lockTakeover.filteredOut);
  assert.equal(mac["lock-takeover-test"].filteredOut, MACOS.lockTakeover.filteredOut);
  assert.equal(mac["workspace-tests"].passed, MACOS.workspace.passed);
  assert.equal(mac["tauri-tests"].passed, MACOS.tauri.passed);
  assert.equal(win["tauri-tests"].passed, WINDOWS.tauri.passed);
});

test("the Windows-only p12 gate needs both binaries and takes its paths from the harness", () => {
  const win = gatesFor("win32");
  const p12 = win.find((g) => g.name === "p12-windows-checks");
  assert.equal(p12.mode, "p12");
  assert.deepEqual(p12.needs, ["daemon-build", "cli-build"]);
  for (const placeholder of ["{target}", "{testTmp}", "{evidence}"]) assert.ok(p12.args.includes(placeholder) || p12.args.some((a) => a.includes(placeholder)), placeholder);
  assert.ok(win.findIndex((g) => g.name === "cli-build") < win.indexOf(p12));
  assert.equal(win.length, 27);
  assert.equal(gatesFor("darwin").length, 26);
  assert.ok(!gatesFor("darwin").some((g) => g.name === "p12-windows-checks" || g.name === "cli-build"));
});

test("the clippy baseline pattern matches only the listed lines, under either separator", () => {
  const re = new RegExp(clippyBaselinePattern(CLIPPY_BASELINE), "i");
  for (const line of CLIPPY_BASELINE.lines) {
    assert.ok(re.test(`crates/orchestrator-runtime/src/supervisor.rs:${line}:9`));
    assert.ok(re.test(`crates\\orchestrator-runtime\\src\\supervisor.rs:${line}:9`));
  }
  assert.ok(!re.test("crates/orchestrator-runtime/src/supervisor.rs:402:9"));
  assert.ok(!re.test("crates/orchestrator-runtime/src/supervisorXrs:401:9"));
  const moved = new RegExp(clippyBaselinePattern({ file: "a/b.rs", lines: [7, 80] }));
  assert.ok(moved.test("a/b.rs:80:1") && !moved.test("a/b.rs:401:1"));
});
