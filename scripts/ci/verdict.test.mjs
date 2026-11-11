import assert from "node:assert/strict";
import { test } from "node:test";
import { EXIT_CODES, OVERALL, countVerdicts, overallVerdict } from "./verdict.mjs";

const v = (...verdicts) => verdicts.map((verdict) => ({ verdict }));

test("all PASS is PASS, and only that exits 0", () => {
  assert.equal(overallVerdict(v("PASS", "PASS"), v("PASS")), OVERALL.PASS);
  assert.equal(EXIT_CODES[OVERALL.PASS], 0);
});

test("a SKIPPED gate can never look like a pass: the verdict is INCONCLUSIVE and the exit is not 0", () => {
  assert.equal(overallVerdict(v("PASS", "SKIPPED", "PASS"), v("PASS")), OVERALL.INCONCLUSIVE);
  assert.equal(overallVerdict(v("SKIPPED"), []), OVERALL.INCONCLUSIVE);
  assert.notEqual(EXIT_CODES[OVERALL.INCONCLUSIVE], 0);
  assert.notEqual(EXIT_CODES[OVERALL.INCONCLUSIVE], EXIT_CODES[OVERALL.FAIL]);
});

test("a failure wins over a skip, in a gate or in a harness check", () => {
  assert.equal(overallVerdict(v("FAIL", "SKIPPED")), OVERALL.FAIL);
  assert.equal(overallVerdict(v("PASS", "SKIPPED"), v("FAIL")), OVERALL.FAIL);
  assert.equal(overallVerdict(v("PASS"), v("FAIL")), OVERALL.FAIL);
});

test("no gates is INCONCLUSIVE, and an unknown verdict is a FAIL", () => {
  assert.equal(overallVerdict([], v("PASS")), OVERALL.INCONCLUSIVE);
  assert.equal(overallVerdict(v("PASS", "MAYBE")), OVERALL.FAIL);
  assert.equal(overallVerdict(v("PASS", undefined)), OVERALL.FAIL);
});

test("verdicts are counted", () => {
  assert.deepEqual(countVerdicts(v("PASS", "PASS", "FAIL", "SKIPPED")), { PASS: 2, FAIL: 1, SKIPPED: 1 });
});
