// Fails unless a node:test TAP report has exactly the expected number of passing tests and no
// failed, cancelled, skipped or todo tests. A skipped test must never count as a pass.
//
//   node require-tap-totals.mjs <tap-file> <expected-pass>
//
// Reads the summary lines that `node --test --test-reporter=tap` prints at the end
// (`# tests N`, `# pass N`, `# fail N`, `# cancelled N`, `# skipped N`, `# todo N`).
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

export function checkTapTotals(text, expectedPass) {
  const totals = {};
  for (const line of text.split("\n")) {
    const match = /^# (tests|pass|fail|cancelled|skipped|todo) (\d+)\s*$/.exec(line);
    if (match) totals[match[1]] = Number(match[2]);
  }
  const problems = [];
  for (const key of ["tests", "pass", "fail", "cancelled", "skipped", "todo"]) {
    if (totals[key] === undefined) problems.push(`no "# ${key}" line in the TAP report`);
  }
  if (totals.pass !== expectedPass) problems.push(`pass is ${totals.pass}, expected ${expectedPass}`);
  if (totals.tests !== expectedPass) problems.push(`tests is ${totals.tests}, expected ${expectedPass}`);
  for (const key of ["fail", "cancelled", "skipped", "todo"]) {
    if (totals[key] !== 0) problems.push(`${key} is ${totals[key]}, expected 0`);
  }
  return problems;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [file, expected] = process.argv.slice(2);
  const expectedPass = Number(expected);
  if (!file || !Number.isInteger(expectedPass)) {
    console.error("usage: node require-tap-totals.mjs <tap-file> <expected-pass>");
    process.exit(2);
  }
  const problems = checkTapTotals(readFileSync(file, "utf8"), expectedPass);
  if (problems.length) {
    console.error(`TAP totals differ from the expected ${expectedPass} passes:\n  ${problems.join("\n  ")}`);
    process.exit(1);
  }
  console.log(`TAP totals: ${expectedPass} passed, 0 failed, 0 cancelled, 0 skipped, 0 todo`);
}
