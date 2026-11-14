import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import {
  checkBuild, checkLifecycle, checkLint, checkP12, checkTests, checkVitest, diagnostics, parseP12Verdict, runningSequence, testTotals,
} from "./parse.mjs";

// The stdout of the Priority 12 round 3 native Windows run (user name already replaced by `<user>`).
const windowsR3 = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "fixtures", "p12-windows-checks.windows-r3.stdout.txt"), "utf8");

test("the real Windows output is read as FAIL, not as an empty verdict: the banner lines are not verdicts", () => {
  for (const text of [windowsR3, windowsR3.replace(/\n/g, "\r\n")]) {
    assert.match(windowsR3, /^p12-windows-checks: platform windows$/m);
    assert.deepEqual(parseP12Verdict(text), { verdict: "FAIL", detail: "(2 of 8 failed)", lines: 1 });
    const problems = checkP12(text);
    assert.equal(problems.length, 1);
    assert.match(problems[0], /FAIL \(2 of 8 failed\); failing: acl-files-present, acl-\.orchestrator\.lock/);
  }
});

test("the verdict is case-sensitive, taken from a line start, and must be unique and known", () => {
  const checks = "CHECK a: PASS - x\nCHECK b: NOT RUN - y\n";
  assert.deepEqual(checkP12(`${checks}P12-WINDOWS-CHECKS: PASS\n`), []);
  assert.deepEqual(checkP12(`p12-windows-checks: platform windows\n${checks}P12-WINDOWS-CHECKS: PASS\r\n`), []);
  assert.match(checkP12(`${checks}p12-windows-checks: PASS\n`)[0], /no 'P12-WINDOWS-CHECKS:' verdict line/);
  assert.match(checkP12(`${checks}  P12-WINDOWS-CHECKS: PASS\n`)[0], /no 'P12-WINDOWS-CHECKS:'/);
  assert.match(checkP12("")[0], /no 'P12-WINDOWS-CHECKS:'/);
  assert.match(checkP12(`${checks}P12-WINDOWS-CHECKS: PASS\nP12-WINDOWS-CHECKS: PASS\n`)[0], /appears 2 times/);
  assert.match(checkP12(`${checks}P12-WINDOWS-CHECKS: MAYBE\n`)[0], /unrecognised verdict 'MAYBE'/);
  assert.match(checkP12(`${checks}P12-WINDOWS-CHECKS: INCOMPLETE (1 check(s) could not run)\n`)[0], /INCOMPLETE \(1 check/);
});

test("a PASS verdict that contradicts its CHECK lines is a problem", () => {
  assert.match(checkP12("CHECK a: FAIL - x\nP12-WINDOWS-CHECKS: PASS\n")[0], /PASS but these checks failed: a/);
  assert.match(checkP12("CHECK a: NOT RUN - x\nP12-WINDOWS-CHECKS: PASS\n")[0], /PASS but no check ran/);
});

const block = (n, names, extra = "") =>
  `running ${n} test${n === 1 ? "" : "s"}\n${names.map((x) => `test ${x} ... ok`).join("\n")}\n\ntest result: ok. ${n} passed; 0 failed; ${extra || "0 ignored"}; 0 measured; 0 filtered out; finished in 0.10s\n`;

test("sequence and totals are read across binaries, with CRLF output", () => {
  const out = (block(2, ["a::one", "a::two"]) + block(0, []) ).replace(/\n/g, "\r\n");
  assert.deepEqual(runningSequence(out), [2, 0]);
  assert.equal(testTotals(out).passed, 2);
  assert.deepEqual(checkTests({ stdout: out, expect: ["one", "two"], passed: 2, ignored: 0, runningSequence: [2, 0] }), []);
});

test("a wrong count, sequence or missing name is a problem in either direction", () => {
  const out = block(2, ["a::one", "a::two"]);
  assert.match(checkTests({ stdout: out, passed: 3 })[0], /passed is 2, expected 3/);
  assert.match(checkTests({ stdout: out, passed: 1 })[0], /passed is 2, expected 1/);
  assert.match(checkTests({ stdout: out, runningSequence: [2, 0] })[0], /running sequence/);
  assert.match(checkTests({ stdout: out, expect: ["three"] })[0], /test not reported: three/);
});

test("a failed or ignored test is not ok", () => {
  const out = "running 1 test\ntest a::one ... FAILED\n\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n";
  const problems = checkTests({ stdout: out, expect: ["one"], passed: 1, ignored: 0 });
  assert.ok(problems.some((p) => /failed is 1/.test(p)));
  assert.ok(problems.some((p) => /did not report ok: one/.test(p)));
});

test("name matching is by whole path segment, not substring", () => {
  const out = block(1, ["a::not_one"]);
  assert.match(checkTests({ stdout: out, expect: ["one"] })[0], /test not reported: one/);
});

test("lifecycle needs its eight ordered STEP lines, one PASS and no libtest shape", () => {
  const good = Array.from({ length: 8 }, (_, i) => `STEP ${i + 1} something`).join("\n") + "\nlifecycle: PASS\n";
  assert.deepEqual(checkLifecycle(good), []);
  assert.match(checkLifecycle(good.replace("STEP 3", "STEP 9"))[0], /STEP lines/);
  assert.match(checkLifecycle(`${good}${good}`)[0], /appears 2 times/);
  assert.ok(checkLifecycle(`${good}running 0 tests\n`).some((p) => /libtest/.test(p)));
});

test("build and lint outputs must show real work and a Finished line", () => {
  const err = "   Compiling orchestrator-ipc v0.1.0 (/x)\n    Finished `dev` profile in 3s\n";
  assert.deepEqual(checkBuild(err, { crates: ["orchestrator-ipc"] }), []);
  assert.match(checkBuild("    Finished `dev`\n", { crates: ["orchestrator-ipc"] })[0], /stale or cached/);
  assert.match(checkLint("    Checking a v1\n", { crates: ["a"] })[0], /Finished/);
  const verbose = "     Running `clippy-driver rustc --crate-name agent_orch_desktop_lib x`\n    Finished `dev`\n";
  assert.match(checkLint(verbose, { crateNames: ["agent_orch_desktop_lib", "agent_orch_desktop"] })[0], /agent_orch_desktop\b/);
});

test("diagnostics keep the message and location", () => {
  const err = "warning: you seem to be trying to use `match`\n  --> crates/x/src/a.rs:401:9\n   |\nwarning: `x` (lib) generated 2 warnings\n";
  assert.deepEqual(diagnostics(err), [{ message: "warning: you seem to be trying to use `match`", location: "crates/x/src/a.rs:401:9" }]);
});

test("vitest summary lines", () => {
  const out = " Test Files  15 passed (15)\n      Tests  287 passed (287)\n";
  assert.deepEqual(checkVitest(out, { files: 15, tests: 287 }), []);
  assert.equal(checkVitest(out, { files: 15, tests: 288 }).length, 1);
  assert.equal(checkVitest(" Test Files  1 failed | 14 passed (15)\n", { files: 15, tests: 287 }).length, 2);
});
