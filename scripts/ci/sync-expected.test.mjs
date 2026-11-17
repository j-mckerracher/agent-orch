import assert from "node:assert/strict";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { gatesFor } from "./gates.mjs";
import { TEST_GATES, syncExpected } from "./sync-expected.mjs";

const here = dirname(fileURLToPath(import.meta.url));

// A scripts directory holding the real expected.mjs and name lists, and an evidence directory per call.
const sandbox = () => {
  const root = mkdtempSync(join(tmpdir(), "sync-expected-"));
  const scriptsDir = join(root, "scripts");
  mkdirSync(scriptsDir);
  for (const f of ["expected.mjs", "windows-test-names.json", "macos-test-names.json"]) cpSync(join(here, f), join(scriptsDir, f));
  return { root, scriptsDir };
};

const libtest = (names, { ignored = 0, filteredOut = 0 } = {}) =>
  [
    `running ${names.length + ignored} tests`,
    ...names.map((n) => `test ${n} ... ok`),
    `\ntest result: ok. ${names.length} passed; 0 failed; ${ignored} ignored; 0 measured; ${filteredOut} filtered out; finished in 0.1s\n`,
  ].join("\n");

const evidence = (root, gates, platform = "darwin") => {
  const dir = mkdtempSync(join(root, "evidence-"));
  for (const [gate, { stdout, exitCode = 0 }] of Object.entries(gates)) {
    writeFileSync(join(dir, `${gate}.stdout.txt`), stdout);
    writeFileSync(join(dir, `${gate}.result.json`), JSON.stringify({ gate, platform, exitCode, verdict: "FAIL", problems: ["count"] }));
  }
  return dir;
};

test("a measured count and a new test name are written to the pins and the name list", () => {
  const { root, scriptsDir } = sandbox();
  try {
    const names = JSON.parse(readFileSync(join(scriptsDir, "macos-test-names.json"), "utf8")).macos["coordinator-review"];
    const dir = evidence(root, { "coordinator-review-tests": { stdout: libtest([...names, "api::a_new_test"]) } });
    const report = syncExpected({ evidenceDirs: [dir], scriptsDir });
    assert.equal(report.changes.length, 1);
    assert.match(readFileSync(join(scriptsDir, "expected.mjs"), "utf8"), /^ {2}coordinatorReview: \{ passed: 6, sequence: \[6\] \},$/m);
    const after = JSON.parse(readFileSync(join(scriptsDir, "macos-test-names.json"), "utf8")).macos["coordinator-review"];
    assert.deepEqual(after, [...names, "api::a_new_test"]);
    assert.equal(syncExpected({ evidenceDirs: [dir], scriptsDir }).changes.length, 0, "a second run changes nothing");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a run with fewer tests than the pins is refused unless removals are allowed", () => {
  const { root, scriptsDir } = sandbox();
  try {
    const names = JSON.parse(readFileSync(join(scriptsDir, "macos-test-names.json"), "utf8")).macos["coordinator-review"];
    const dir = evidence(root, { "coordinator-review-tests": { stdout: libtest(names.slice(1)) } });
    assert.throws(() => syncExpected({ evidenceDirs: [dir], scriptsDir }), /fewer tests than the pins/);
    syncExpected({ evidenceDirs: [dir], scriptsDir, allowRemovals: true });
    assert.match(readFileSync(join(scriptsDir, "expected.mjs"), "utf8"), /^ {2}coordinatorReview: \{ passed: 4, sequence: \[4\] \},$/m);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a gate that did not exit 0 is reported and its pins stay", () => {
  const { root, scriptsDir } = sandbox();
  try {
    const before = readFileSync(join(scriptsDir, "expected.mjs"), "utf8");
    const dir = evidence(root, { "coordinator-review-tests": { stdout: libtest(["a"]), exitCode: 101 } });
    const report = syncExpected({ evidenceDirs: [dir], scriptsDir });
    assert.equal(report.skipped.length, 1);
    assert.equal(readFileSync(join(scriptsDir, "expected.mjs"), "utf8"), before);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a filtered gate keeps its filteredOut pin in step with the measured value", () => {
  const { root, scriptsDir } = sandbox();
  try {
    const names = JSON.parse(readFileSync(join(scriptsDir, "windows-test-names.json"), "utf8")).windows["windows-transport"];
    const dir = evidence(root, { "windows-transport-tests": { stdout: libtest(names, { filteredOut: 120 }) } }, "win32");
    syncExpected({ evidenceDirs: [dir], scriptsDir });
    assert.match(readFileSync(join(scriptsDir, "expected.mjs"), "utf8"), /windowsTransport: \{ passed: 14, sequence: \[14\], filteredOut: 120 \}/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the same test path in two binaries of one gate gets two entries, and a second run changes nothing", () => {
  const { root, scriptsDir } = sandbox();
  try {
    const names = JSON.parse(readFileSync(join(scriptsDir, "macos-test-names.json"), "utf8")).macos["coordinator-review"];
    const dir = evidence(root, { "coordinator-review-tests": { stdout: libtest([...names, "tests::dup", "tests::dup"]) } });
    syncExpected({ evidenceDirs: [dir], scriptsDir });
    const after = JSON.parse(readFileSync(join(scriptsDir, "macos-test-names.json"), "utf8")).macos["coordinator-review"];
    assert.deepEqual(after, [...names, "tests::dup", "tests::dup"]);
    assert.match(readFileSync(join(scriptsDir, "expected.mjs"), "utf8"), /^ {2}coordinatorReview: \{ passed: 7, sequence: \[7\] \},$/m);
    assert.equal(syncExpected({ evidenceDirs: [dir], scriptsDir }).changes.length, 0);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a sync that changes no name leaves both name files byte for byte as committed", () => {
  const { root, scriptsDir } = sandbox();
  try {
    const names = JSON.parse(readFileSync(join(scriptsDir, "macos-test-names.json"), "utf8")).macos["coordinator-review"];
    const before = ["windows-test-names.json", "macos-test-names.json"].map((f) => readFileSync(join(scriptsDir, f), "utf8"));
    syncExpected({ evidenceDirs: [evidence(root, { "coordinator-review-tests": { stdout: libtest(names) } })], scriptsDir });
    assert.deepEqual(["windows-test-names.json", "macos-test-names.json"].map((f) => readFileSync(join(scriptsDir, f), "utf8")), before);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a renamed test (same count) is synced and reported, not refused", () => {
  const { root, scriptsDir } = sandbox();
  try {
    const names = JSON.parse(readFileSync(join(scriptsDir, "macos-test-names.json"), "utf8")).macos["coordinator-review"];
    const report = syncExpected({ evidenceDirs: [evidence(root, { "coordinator-review-tests": { stdout: libtest([...names.slice(1), "api::renamed"]) } })], scriptsDir });
    assert.match(report.changes[0], new RegExp(`names removed: ${names[0]}`));
    const after = JSON.parse(readFileSync(join(scriptsDir, "macos-test-names.json"), "utf8")).macos["coordinator-review"];
    assert.deepEqual(after, [...names.slice(1), "api::renamed"]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a node self-test count below the pin is refused unless removals are allowed", () => {
  const { root, scriptsDir } = sandbox();
  try {
    const tap = "# tests 3\n# pass 3\n# fail 0\n# cancelled 0\n# skipped 0\n# todo 0\n";
    const dir = evidence(root, { "node-self-tests": { stdout: tap } });
    assert.throws(() => syncExpected({ evidenceDirs: [dir], scriptsDir }), /fewer than the pins/);
    syncExpected({ evidenceDirs: [dir], scriptsDir, allowRemovals: true });
    assert.match(readFileSync(join(scriptsDir, "expected.mjs"), "utf8"), /NODE_SELF_TESTS = \{ passed: 3 \};/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("every counted cargo gate has a row, so a new gate cannot be left out of the sync", () => {
  for (const platform of ["win32", "darwin"]) {
    for (const gate of gatesFor(platform)) {
      if (gate.runningSequence && gate.name !== "workspace-tests") assert.ok(gate.name in TEST_GATES, `${platform} ${gate.name}`);
    }
  }
});
