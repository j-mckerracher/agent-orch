// Pure parsers and verdict checks for the native gates (no I/O, no process spawning).
//
// Every check returns a list of problems; an empty list means the gate output matches its
// expectation. A count that differs from the expectation is a problem in either direction:
// expected counts are never edited to fit a measured run.

import { LIFECYCLE_STEPS } from "./expected.mjs";

export const normalize = (text) => text.replace(/\r\n/g, "\n").replace(/\x1b\[[0-9;]*m/g, "");

// Every `running N test(s)` line, in order. A filtered cargo run prints one block, an
// unfiltered library crate also prints a doc-test block.
export const runningSequence = (text) =>
  [...normalize(text).matchAll(/^running (\d+) tests?\s*$/gm)].map((m) => Number(m[1]));

// Sum of every libtest `test result:` line.
export const testTotals = (text) => {
  const totals = { passed: 0, failed: 0, ignored: 0, filteredOut: 0, results: 0 };
  const re =
    /^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; \d+ measured; (\d+) filtered out/gm;
  for (const m of normalize(text).matchAll(re)) {
    totals.passed += Number(m[1]);
    totals.failed += Number(m[2]);
    totals.ignored += Number(m[3]);
    totals.filteredOut += Number(m[4]);
    totals.results += 1;
  }
  return totals;
};

// Number of `test name ... outcome` lines (duplicate names across binaries still count).
export const testLineCount = (text) => [...normalize(text).matchAll(/^test (\S+) \.\.\. (ok|FAILED|ignored)\b/gm)].length;

// Outcome of each named test: `test a::b::name ... ok`.
export const testOutcomes = (text) => {
  const outcomes = new Map();
  for (const m of normalize(text).matchAll(/^test (\S+) \.\.\. (ok|FAILED|ignored)\b/gm)) {
    outcomes.set(m[1], m[2]);
  }
  return outcomes;
};

const isNamed = (full, expected) => full === expected || full.endsWith(`::${expected}`);

export const checkTests = ({ stdout, expect = [], passed, ignored, runningSequence: sequence, filteredOut }) => {
  const problems = [];
  const totals = testTotals(stdout);
  const seen = runningSequence(stdout);
  if (sequence && seen.join(",") !== sequence.join(",")) {
    problems.push(`running sequence is [${seen}], expected [${sequence}]`);
  }
  if (passed !== undefined && totals.passed !== passed) {
    problems.push(`passed is ${totals.passed}, expected ${passed}`);
  }
  if (totals.failed !== 0) problems.push(`failed is ${totals.failed}, expected 0`);
  if (ignored !== undefined && totals.ignored !== ignored) {
    problems.push(`ignored is ${totals.ignored}, expected ${ignored}`);
  }
  if (filteredOut !== undefined && totals.filteredOut !== filteredOut) {
    problems.push(`filtered out is ${totals.filteredOut}, expected ${filteredOut}`);
  }
  const outcomes = testOutcomes(stdout);
  const names = [...outcomes.keys()];
  for (const name of expect) {
    const hits = names.filter((n) => isNamed(n, name));
    if (hits.length === 0) problems.push(`test not reported: ${name}`);
    else if (!hits.some((n) => outcomes.get(n) === "ok")) problems.push(`test did not report ok: ${name}`);
  }
  if (expect.length > 0 && passed !== undefined && testLineCount(stdout) !== passed + (ignored ?? 0)) {
    problems.push(`${testLineCount(stdout)} test lines, expected ${passed + (ignored ?? 0)}`);
  }
  return problems;
};

// The harness-free lifecycle test: exit 0, STEP 1..LIFECYCLE_STEPS in order, exactly one
// `lifecycle: PASS`, and no libtest `running` or `test result` line (a different binary shape).
export const checkLifecycle = (stdout, steps = LIFECYCLE_STEPS) => {
  const problems = [];
  const text = normalize(stdout);
  const passes = [...text.matchAll(/^lifecycle: PASS[ \t]*$/gm)].length;
  if (passes !== 1) problems.push(`'lifecycle: PASS' appears ${passes} times, expected 1`);
  const seen = [...text.matchAll(/^STEP (\d+)\b/gm)].map((m) => Number(m[1]));
  const wanted = Array.from({ length: steps }, (_, i) => i + 1);
  if (seen.join(",") !== wanted.join(",")) problems.push(`STEP lines are [${seen}], expected [${wanted}]`);
  if (runningSequence(text).length > 0 || testTotals(text).results > 0) {
    problems.push("lifecycle printed libtest lines; its shape is not the harness-free binary");
  }
  return problems;
};

export const checkBuild = (stderr, { crates = [] }) => {
  const problems = [];
  const text = normalize(stderr);
  for (const crate of crates) {
    if (!new RegExp(`^\\s*Compiling ${crate.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")} v`, "m").test(text)) {
      problems.push(`no 'Compiling ${crate}' line (stale or cached build?)`);
    }
  }
  if (!/^\s*Finished\b/m.test(text)) problems.push("no 'Finished' line");
  return problems;
};

// Clippy: the run must finish and lint the named crates. Diagnostics are returned, not judged:
// the plan records them as new findings and lets the reviewer decide.
export const checkLint = (stderr, { crates = [], crateNames = [] }) => {
  const problems = [];
  const text = normalize(stderr);
  for (const crate of crates) {
    if (!new RegExp(`^\\s*Checking ${crate} v`, "m").test(text)) problems.push(`no 'Checking ${crate}' line`);
  }
  for (const name of crateNames) {
    if (!new RegExp(`Running .*--crate-name ${name}\\b`).test(text)) {
      problems.push(`no '--crate-name ${name}' Running line`);
    }
  }
  if (!/^\s*Finished\b/m.test(text)) problems.push("no 'Finished' line");
  return problems;
};

export const diagnostics = (stderr) => {
  const lines = normalize(stderr).split("\n");
  const found = [];
  lines.forEach((line, i) => {
    if (/^(warning|error)(\[\S+\])?: /.test(line) && !/generated \d+ warnings?/.test(line)) {
      const where = /^\s*--> (.+)$/.exec(lines[i + 1] ?? "");
      found.push({ message: line, location: where ? where[1] : null });
    }
  });
  return found;
};

export const checkVitest = (stdout, { files, tests }) => {
  const problems = [];
  const text = normalize(stdout);
  const f = /^\s*Test Files\s+(\d+) passed \((\d+)\)\s*$/m.exec(text);
  const t = /^\s*Tests\s+(\d+) passed \((\d+)\)\s*$/m.exec(text);
  if (!f || Number(f[1]) !== files || Number(f[2]) !== files) {
    problems.push(`Test Files line is '${f ? f[0].trim() : "absent"}', expected ${files} passed (${files})`);
  }
  if (!t || Number(t[1]) !== tests || Number(t[2]) !== tests) {
    problems.push(`Tests line is '${t ? t[0].trim() : "absent"}', expected ${tests} passed (${tests})`);
  }
  return problems;
};

export const checkPattern = (stdout, pattern) =>
  new RegExp(pattern, "m").test(normalize(stdout)) ? [] : [`output does not match /${pattern}/`];

// `node --test` summary lines: `# pass 52` (TAP reporter, Node 22 without a terminal) or `ℹ pass 52` (spec
// reporter). Every counter must be present: a run that printed no summary proves nothing.
export const nodeTestSummary = (stdout) => {
  const text = normalize(stdout);
  const read = (key) => {
    const m = new RegExp(`^(?:#|ℹ)[ \\t]+${key}[ \\t]+(\\d+)[ \\t]*$`, "m").exec(text);
    return m ? Number(m[1]) : null;
  };
  return Object.fromEntries(["tests", "pass", "fail", "cancelled", "skipped", "todo"].map((k) => [k, read(k)]));
};

export const checkNodeTests = (stdout, { passed }) => {
  const problems = [];
  const s = nodeTestSummary(stdout);
  for (const [key, value] of Object.entries(s)) if (value === null) problems.push(`no '${key}' summary line`);
  if (problems.length) return problems;
  if (s.pass !== passed) problems.push(`pass is ${s.pass}, expected ${passed}`);
  if (s.tests !== passed) problems.push(`tests is ${s.tests}, expected ${passed}`);
  for (const key of ["fail", "cancelled", "skipped", "todo"]) if (s[key] !== 0) problems.push(`${key} is ${s[key]}, expected 0`);
  return problems;
};

// The Priority 12 by-hand checks (scripts/ci/p12-windows-checks.ps1) end with one verdict line:
// `P12-WINDOWS-CHECKS: PASS | FAIL (n of m failed) | INCOMPLETE (...)`. The match is case-sensitive:
// the script's own banner lines start with the lower-case `p12-windows-checks:` and a
// case-insensitive filter (PowerShell's `-match`) counted them as verdicts, so the Priority 12
// round 3 harness read an empty verdict and reported INCONCLUSIVE.
export const parseP12Verdict = (text) => {
  const lines = [...normalize(text).matchAll(/^P12-WINDOWS-CHECKS: (.*\S)[ \t]*$/gm)].map((m) => m[1]);
  const last = lines.at(-1) ?? "";
  const m = /^(PASS|FAIL|INCOMPLETE)\b[ \t]*(.*)$/.exec(last);
  return { verdict: m ? m[1] : "", detail: m ? m[2] : last, lines: lines.length };
};

export const checkP12 = (stdout) => {
  const problems = [];
  const { verdict, detail, lines } = parseP12Verdict(stdout);
  const checks = [...normalize(stdout).matchAll(/^CHECK (\S+): (PASS|FAIL|NOT RUN) - /gm)].map((m) => ({ name: m[1], status: m[2] }));
  const failing = checks.filter((c) => c.status === "FAIL").map((c) => c.name);
  if (lines === 0) problems.push("no 'P12-WINDOWS-CHECKS:' verdict line");
  else if (lines > 1) problems.push(`'P12-WINDOWS-CHECKS:' verdict appears ${lines} times, expected 1`);
  if (lines > 0 && verdict === "") problems.push(`unrecognised verdict '${detail}'`);
  if (verdict === "FAIL") problems.push(`P12-WINDOWS-CHECKS: FAIL ${detail}${failing.length ? `; failing: ${failing.join(", ")}` : ""}`);
  if (verdict === "INCOMPLETE") problems.push(`P12-WINDOWS-CHECKS: INCOMPLETE ${detail}`);
  if (verdict === "PASS") {
    if (failing.length) problems.push(`verdict is PASS but these checks failed: ${failing.join(", ")}`);
    if (checks.every((c) => c.status === "NOT RUN")) problems.push("verdict is PASS but no check ran");
  }
  return problems;
};
