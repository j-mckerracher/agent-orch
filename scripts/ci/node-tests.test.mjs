import assert from "node:assert/strict";
import { test } from "node:test";
import { checkNodeTests, nodeTestSummary } from "./parse.mjs";

const tap = (o = {}) => {
  const c = { tests: 52, suites: 9, pass: 52, fail: 0, cancelled: 0, skipped: 0, todo: 0, ...o };
  return `ok 1 - x\n1..1\n# tests ${c.tests}\n# suites ${c.suites}\n# pass ${c.pass}\n# fail ${c.fail}\n# cancelled ${c.cancelled}\n# skipped ${c.skipped}\n# todo ${c.todo}\n# duration_ms 1234.5\n`;
};
const spec = (o = {}) => tap(o).replace(/^# /gm, "ℹ ");

test("both the TAP and the spec summary are read, with CRLF as well", () => {
  for (const text of [tap(), spec(), tap().replace(/\n/g, "\r\n"), spec().replace(/\n/g, "\r\n")]) {
    assert.deepEqual(nodeTestSummary(text), { tests: 52, pass: 52, fail: 0, cancelled: 0, skipped: 0, todo: 0 });
    assert.deepEqual(checkNodeTests(text, { passed: 52 }), []);
  }
});

test("a wrong count, a failure, a cancellation, a skip or a todo is a problem", () => {
  assert.match(checkNodeTests(tap(), { passed: 53 })[0], /pass is 52, expected 53/);
  assert.match(checkNodeTests(tap({ tests: 53, fail: 1 }), { passed: 52 }).join(";"), /tests is 53.*fail is 1/);
  for (const key of ["cancelled", "skipped", "todo"]) {
    assert.match(checkNodeTests(tap({ [key]: 1 }), { passed: 52 })[0], new RegExp(`${key} is 1, expected 0`));
  }
});

test("output without a summary proves nothing", () => {
  const problems = checkNodeTests("ok 1 - x\n", { passed: 52 });
  assert.equal(problems.length, 6);
  assert.match(problems[0], /no 'tests' summary line/);
  assert.equal(checkNodeTests("", { passed: 52 }).length, 6);
});

test("a counter in the middle of a line is not a summary line", () => {
  assert.equal(nodeTestSummary("some # pass 52\n").pass, null);
});
