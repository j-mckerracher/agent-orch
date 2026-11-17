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
import { isNamed, nodeTestSummary, normalize, runningSequence, testOutcomes, testTotals } from "./parse.mjs";

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

const vitestTotals = (text) => {
  const f = /^\s*Test Files\s+(\d+) passed \((\d+)\)\s*$/m.exec(text);
  const t = /^\s*Tests\s+(\d+) passed \((\d+)\)\s*$/m.exec(text);
  return f && t && f[1] === f[2] && t[1] === t[2] ? { files: Number(f[1]), tests: Number(t[1]) } : null;
};

// Existing entries that still match a passing test stay (so the diff is only what changed); passing tests no
// entry matches are appended under their full name.
const syncNames = (existing, passing) => {
  const covered = new Set();
  const kept = existing.filter((name) => {
    const hits = passing.filter((n) => isNamed(n, name));
    hits.forEach((n) => covered.add(n));
    return hits.length > 0;
  });
  const added = passing.filter((n) => !covered.has(n));
  return { list: [...kept, ...added], added, removed: existing.filter((n) => !kept.includes(n)) };
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
      const other = ["workspace-tests", "desktop-vitest", "desktop-e2e", "node-self-tests"].includes(gate);
      if (!isTest && !other) continue;
      const stdoutFile = join(dir, `${gate}.stdout.txt`);
      if (result.exitCode !== 0 || !existsSync(stdoutFile)) {
        report.skipped.push(`${gate} (${result.platform}): exit ${result.exitCode}, not a clean run; left unchanged`);
        continue;
      }
      const stdout = normalize(readFileSync(stdoutFile, "utf8"));
      const win = result.platform === "win32";
      const table = win ? "WINDOWS" : "MACOS";

      if (gate === "node-self-tests" || gate === "desktop-vitest" || gate === "desktop-e2e") {
        const before = src;
        let measured;
        if (gate === "node-self-tests") {
          const { pass } = nodeTestSummary(stdout);
          if (pass === null) { report.skipped.push(`${gate}: no summary line`); continue; }
          measured = `${pass} tests`;
          setShared(gate, pass, () => { src = rewriteConst(src, /export const NODE_SELF_TESTS = \{ passed: \d+ \};/, `export const NODE_SELF_TESTS = { passed: ${pass} };`); });
        } else {
          const totals = vitestTotals(stdout);
          if (!totals) { report.skipped.push(`${gate}: no passing vitest summary`); continue; }
          measured = `${totals.files} files, ${totals.tests} tests`;
          setShared(gate, `${totals.files}/${totals.tests}`, () => {
            if (gate === "desktop-vitest") {
              src = rewriteConst(src, /export const VITEST = \{ files: \d+, tests: \d+ \};/, `export const VITEST = { files: ${totals.files}, tests: ${totals.tests} };`);
            } else {
              src = rewriteConst(src, /export const E2E_TESTS = \d+;/, `export const E2E_TESTS = ${totals.tests};`);
              src = rewriteConst(src, /^  e2eFiles: \d+,$/m, `  e2eFiles: ${totals.files},`);
            }
          });
        }
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
      const shrinks = [];
      if (observed.passed < rewritten.oldPassed) shrinks.push(`passed ${rewritten.oldPassed} -> ${observed.passed}`);

      let names = null;
      if (isTest) {
        const [file, rootKey] = nameFiles[result.platform];
        const passing = [...testOutcomes(stdout)].filter(([, o]) => o === "ok").map(([n]) => n);
        names = syncNames(loaded[result.platform][rootKey][TEST_GATES[gate][1]], passing);
        if (names.removed.length) shrinks.push(`removed: ${names.removed.join(", ")}`);
        if (names.list.length !== observed.passed) {
          throw new Error(`${gate}: ${names.list.length} names for ${observed.passed} passing tests; an existing name matches several tests, fix it by hand in ${file}`);
        }
      }
      if (shrinks.length && !allowRemovals) {
        throw new Error(`${gate} (${result.platform}) has fewer tests than the pins (${shrinks.join("; ")}); pass --allow-removals if that is intended`);
      }
      src = rewritten.src;
      if (names) loaded[result.platform][nameFiles[result.platform][1]][TEST_GATES[gate][1]] = names.list;
      if (rewritten.changed || names?.added.length || names?.removed.length) {
        report.changes.push(`${table}.${key}: ${rewritten.old} -> ${rewritten.next}${names ? ` (+${names.added.length} -${names.removed.length} names)` : ""}`);
      }
    }
  }

  if (write) {
    writeFileSync(join(scriptsDir, "expected.mjs"), src);
    for (const [platform, [file]] of Object.entries(nameFiles)) writeFileSync(join(scriptsDir, file), `${JSON.stringify(loaded[platform], null, 2)}\n`);
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
