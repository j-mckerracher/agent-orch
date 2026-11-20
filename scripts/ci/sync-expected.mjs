// Rewrites the expected counts in expected.mjs and the cargo name lists from the evidence a native-gates
// run uploaded (the `gate-evidence-*` artifacts). Counts drifting from the tree is the usual reason CI goes
// red; this turns the hand edit into one command whose diff the pull request shows.
//
//   gh run download <run-id> -D /tmp/evidence
//   node scripts/ci/sync-expected.mjs /tmp/evidence/gate-evidence-* [--allow-removals] [--dry-run]
//
// Only gates whose process exited 0 with no failed test are read; anything else is reported and left alone.
// A run that has fewer tests than the pins is refused without --allow-removals: a vanished test is a finding.

import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { isNamed, nodeTestSummary, normalize, runningSequence, testTotals } from "./parse.mjs";

const here = dirname(fileURLToPath(import.meta.url));

// gate name -> [key in the WINDOWS / MACOS table, list in the name file]. A new counted cargo gate needs a row.
export const TEST_GATES = {
  "ipc-tests": ["ipc", "ipc"],
  "daemon-tests": ["daemon", "daemon"],
  "coordinator-review-tests": ["coordinatorReview", "coordinator-review"],
  "windows-transport-tests": ["windowsTransport", "windows-transport"],
  "git-lib-tests": ["gitLib", "git-lib"],
  "cli-cleanup-safety-tests": ["cliCleanupSafety", "cli-cleanup-safety"],
  "private-fs-tests": ["privateFs", "private-fs"],
  "store-tests": ["store", "store"],
  "lock-takeover-test": ["lockTakeover", "lock-takeover"],
  "diagnostics-tests": ["diagnostics", "diagnostics"],
  "runtime-command-line-tests": ["runtimeCommandLine", "runtime-command-line"],
  "cli-command-line-tests": ["cliCommandLine", "cli-command-line"],
  "tauri-tests": ["tauri", "tauri"],
};

// One entry per passing test, duplicates included: two test binaries of one gate can both report `tests::foo`.
// An existing entry stays when it claims a passing test no earlier entry claimed (so the diff is only what
// changed); passing tests nobody claims are appended under their full name.
const syncNames = (existing, passing) => {
  const claimed = new Array(passing.length).fill(false);
  const kept = [];
  const removed = [];
  for (const name of existing) {
    const i = passing.findIndex((n, j) => !claimed[j] && isNamed(n, name));
    if (i < 0) removed.push(name);
    else { claimed[i] = true; kept.push(name); }
  }
  const added = passing.filter((_, j) => !claimed[j]);
  return { list: [...kept, ...added], added, removed };
};

const entry = ({ passed, ignored, sequence, filteredOut }) =>
  `{ passed: ${passed}${ignored === undefined ? "" : `, ignored: ${ignored}`}${sequence ? `, sequence: [${sequence.join(", ")}]` : ""}${filteredOut === undefined ? "" : `, filteredOut: ${filteredOut}`} }`;

const tableBounds = (src, table) => {
  const start = src.indexOf(`export const ${table} = {`);
  const end = src.indexOf("\n};", start);
  if (start < 0 || end < 0) throw new Error(`expected.mjs has no ${table} table`);
  return [start, end];
};

const rewriteEntry = (src, table, key, observed) => {
  const [start, end] = tableBounds(src, table);
  const re = new RegExp(`^(  ${key}: )(\\{.*\\})(,)$`, "m");
  const body = src.slice(start, end);
  const m = re.exec(body);
  if (!m) throw new Error(`expected.mjs ${table} has no single-line entry for ${key}`);
  const old = m[2];
  const next = entry({
    passed: observed.passed,
    ignored: observed.ignored > 0 || /ignored:/.test(old) ? observed.ignored : undefined,
    sequence: /sequence:/.test(old) ? observed.sequence : undefined,
    filteredOut: /filteredOut:/.test(old) ? observed.filteredOut : undefined,
  });
  const oldPassed = Number(/passed: (\d+)/.exec(old)[1]);
  return { src: src.slice(0, start) + body.replace(re, `$1${next}$3`) + src.slice(end), oldPassed, changed: old !== next, old, next };
};

const rewriteConst = (src, pattern, text) => {
  if (!pattern.test(src)) throw new Error(`expected.mjs has no ${pattern}`);
  return src.replace(pattern, text);
};

export const syncExpected = ({ evidenceDirs, scriptsDir = here, allowRemovals = false, write = true }) => {
  let src = readFileSync(join(scriptsDir, "expected.mjs"), "utf8");
  const nameFiles = { win32: ["windows-test-names.json", "windows"], darwin: ["macos-test-names.json", "macos"] };
  const loaded = {};
  for (const [platform, [file]] of Object.entries(nameFiles)) loaded[platform] = JSON.parse(readFileSync(join(scriptsDir, file), "utf8"));
  const report = { changes: [], skipped: [], heads: new Set() };
  const shared = new Map();
  const setShared = (label, value, apply) => {
    if (shared.has(label) && shared.get(label) !== value) throw new Error(`${label} differs between evidence directories: ${shared.get(label)} and ${value}`);
    shared.set(label, value);
    apply();
  };

  for (const dir of evidenceDirs) {
    const env = join(dir, "environment.txt");
    if (existsSync(env)) {
      const head = /^HEAD: (\w+)/m.exec(readFileSync(env, "utf8"));
      if (head) report.heads.add(head[1]);
    }
    for (const file of readdirSync(dir).filter((f) => f.endsWith(".result.json")).sort()) {
      const result = JSON.parse(readFileSync(join(dir, file), "utf8"));
      const gate = result.gate;
      const isTest = gate in TEST_GATES;
      const other = ["workspace-tests", "node-self-tests"].includes(gate);
      if (!isTest && !other) continue;
      const stdoutFile = join(dir, `${gate}.stdout.txt`);
      if (result.exitCode !== 0 || !existsSync(stdoutFile)) {
        report.skipped.push(`${gate} (${result.platform}): exit ${result.exitCode}, not a clean run; left unchanged`);
        continue;
      }
      const stdout = normalize(readFileSync(stdoutFile, "utf8"));
      const win = result.platform === "win32";
      const table = win ? "WINDOWS" : "MACOS";

      if (gate === "node-self-tests") {
        const before = src;
        const { pass } = nodeTestSummary(stdout);
        if (pass === null) { report.skipped.push(`${gate}: no summary line`); continue; }
        const measured = `${pass} tests`;
        const shrank = pass < Number(/export const NODE_SELF_TESTS = \{ passed: (\d+) \};/.exec(src)?.[1]);
        if (!shrank || allowRemovals) setShared(gate, pass, () => { src = rewriteConst(src, /export const NODE_SELF_TESTS = \{ passed: \d+ \};/, `export const NODE_SELF_TESTS = { passed: ${pass} };`); });
        if (shrank && !allowRemovals) throw new Error(`${gate} measured ${measured}, fewer than the pins; pass --allow-removals if that is intended`);
        if (src !== before) report.changes.push(`${gate}: ${measured}`);
        continue;
      }

      const totals = testTotals(stdout);
      if (totals.failed !== 0 || totals.results === 0) {
        report.skipped.push(`${gate} (${result.platform}): failed or missing test results; left unchanged`);
        continue;
      }
      const observed = { passed: totals.passed, ignored: totals.ignored, sequence: runningSequence(stdout), filteredOut: totals.filteredOut };
      const key = gate === "workspace-tests" ? "workspace" : TEST_GATES[gate][0];
      const rewritten = rewriteEntry(src, table, key, observed);
      // Only a lower count is a shrink: a renamed test (same count, one name out, one in) is reported, not refused.
      const shrinks = [];
      if (observed.passed < rewritten.oldPassed) shrinks.push(`passed ${rewritten.oldPassed} -> ${observed.passed}`);

      let names = null;
      if (isTest) {
        const rootKey = nameFiles[result.platform][1];
        const passing = [...stdout.matchAll(/^test (\S+) \.\.\. ok\b/gm)].map((m) => m[1]);
        if (passing.length !== observed.passed) {
          throw new Error(`${gate}: ${passing.length} passing test lines for ${observed.passed} passed in the summary; the output is not the expected libtest shape`);
        }
        names = syncNames(loaded[result.platform][rootKey][TEST_GATES[gate][1]], passing);
        if (names.removed.length && observed.passed < rewritten.oldPassed) shrinks.push(`removed: ${names.removed.join(", ")}`);
      }
      if (shrinks.length && !allowRemovals) {
        throw new Error(`${gate} (${result.platform}) has fewer tests than the pins (${shrinks.join("; ")}); pass --allow-removals if that is intended`);
      }
      src = rewritten.src;
      if (names) loaded[result.platform][nameFiles[result.platform][1]][TEST_GATES[gate][1]] = names.list;
      if (rewritten.changed || names?.added.length || names?.removed.length) {
        const gone = names?.removed.length ? `; names removed: ${names.removed.join(", ")}` : "";
        report.changes.push(`${table}.${key}: ${rewritten.old} -> ${rewritten.next}${names ? ` (+${names.added.length} -${names.removed.length} names)` : ""}${gone}`);
      }
    }
  }

  if (write) {
    const put = (path, text) => { if (!existsSync(path) || readFileSync(path, "utf8") !== text) writeFileSync(path, text); };
    put(join(scriptsDir, "expected.mjs"), src);
    // One-space indent: the files are committed that way, so an unchanged list writes no diff.
    for (const [platform, [file]] of Object.entries(nameFiles)) put(join(scriptsDir, file), `${JSON.stringify(loaded[platform], null, 1)}\n`);
  }
  return report;
};

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const evidenceDirs = args.filter((a) => !a.startsWith("--"));
  if (!evidenceDirs.length) {
    console.error("usage: node scripts/ci/sync-expected.mjs <gate-evidence-dir>... [--allow-removals] [--dry-run]");
    process.exit(64);
  }
  try {
    const report = syncExpected({ evidenceDirs, allowRemovals: args.includes("--allow-removals"), write: !args.includes("--dry-run") });
    console.log(`measured at ${[...report.heads].join(", ") || "unknown commit"}`);
    for (const line of report.changes) console.log(`updated ${line}`);
    for (const line of report.skipped) console.log(`skipped ${line}`);
    if (!report.changes.length) console.log("no count changed");
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}
