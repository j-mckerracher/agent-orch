// The Windows native gates, split into parallel jobs, and the rules that decide which of those jobs a
// change needs. The functions are pure and tested in windows-jobs.test.mjs. The CLI is what
// .github/workflows/ci.yml calls:
//
//   node scripts/ci/windows-jobs.mjs select --event <github.event_name>
//       key=value lines for $GITHUB_OUTPUT (the groups, and `macos`: a JSON list of the macOS jobs to run);
//       the reason goes to stderr
//   node scripts/ci/windows-jobs.mjs gates <job-id>
//       the gate names one job runs, for run-gates --only
//   node scripts/ci/windows-jobs.mjs verdict
//       reads the needs context from NEEDS (JSON) and fails unless every job agrees with the selection
//   node scripts/ci/windows-jobs.mjs macos-gates <job-id>
//       the gate names one macOS job runs
//
// Only a pull request is selected by its changed files. Every other event (a push to main, a manual run,
// a release call) runs every Windows gate. The harness job always runs. A job that is not selected is
// skipped, and the aggregator accepts a skipped job only when its group was not selected; a selected job
// must succeed.
import { execFileSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

// Each job runs its gates in gates.mjs order. A gate that needs another gate must share its job, or
// run-gates would run it without the prerequisite; windows-jobs.test.mjs checks that.
export const JOBS = [
  { id: "harness", group: "harness", gates: ["node-self-tests"] },
  {
    id: "engine-build",
    group: "engine",
    gates: ["daemon-build", "windows-transport-tests", "cli-build", "p12-windows-checks", "clippy-scoped"],
  },
  {
    id: "engine-tests-a",
    group: "engine",
    gates: [
      "ipc-tests", "daemon-tests", "daemon-lifecycle", "coordinator-review-tests",
      "cli-cleanup-safety-tests", "runtime-command-line-tests", "cli-command-line-tests",
    ],
  },
  {
    id: "engine-tests-b",
    group: "engine",
    gates: ["git-lib-tests", "private-fs-tests", "store-tests", "lock-takeover-test", "diagnostics-tests"],
  },
  {
    id: "tauri-build",
    group: "tauri",
    gates: ["desktop-npm-ci", "desktop-build", "tauri-build", "tauri-clippy-clean", "tauri-clippy"],
  },
  { id: "tauri-tests", group: "tauri", gates: ["desktop-npm-ci", "desktop-build", "tauri-tests"] },
  { id: "desktop", group: "desktop", gates: ["desktop-npm-ci", "desktop-tsc", "desktop-build", "desktop-vitest"] },
  { id: "desktop-package", group: "package", gates: ["desktop-npm-ci", "desktop-package-win"] },
];

// The macOS native gates, split the same way so that no job runs longer than about ten minutes. `daemon-tests`
// alone takes up to seven minutes, so it has the engine job to itself. A job runs when any of its groups is selected,
// by the same rules as the Windows jobs (every event but a pull request selects every group).
export const MACOS_JOBS = [
  { id: "engine", groups: ["engine"], gates: ["daemon-tests", "daemon-lifecycle", "coordinator-review-tests"] },
  {
    id: "crates",
    groups: ["engine", "smoke"],
    gates: [
      "node-self-tests", "ipc-tests", "git-lib-tests", "cli-cleanup-safety-tests", "private-fs-tests", "store-tests",
      "lock-takeover-test", "diagnostics-tests", "runtime-command-line-tests", "cli-command-line-tests", "clippy-scoped",
      "workspace-tests",
    ],
  },
  {
    id: "desktop",
    groups: ["desktop", "tauri", "package"],
    gates: [
      "daemon-build", "desktop-npm-ci", "desktop-tsc", "desktop-build", "desktop-vitest", "desktop-e2e", "desktop-geometry",
      "tauri-build", "tauri-tests", "tauri-clippy-clean", "tauri-clippy", "desktop-package-mac",
    ],
  },
];

export const GROUPS = ["engine", "tauri", "desktop", "package", "smoke"];

// The usage smoke step (AI usage, DPES-1105) runs only when what it exercises changes: the pinned reader crate, the
// smoke scripts, the bundled reader sidecar and the script that stages it, and the daemon's and store's usage code.
// `smoke` is selected by these paths on a pull request and by every other event. The step itself is wired in ci.yml
// and runs only when USAGE_SMOKE.enabled is true (see docs/release-ci.md).
const SMOKE_INPUTS = [
  /^crates\/orchestrator-local-usage\//,
  /^scripts\/smoke\//,
  /^apps\/desktop\/src-tauri\/sidecars\/codeburn\//,
  /^apps\/desktop\/scripts\/prepare-codeburn-sidecar\.mjs$/,
  /^crates\/orchestrator-daemon\/.*usage/i,
  /^crates\/orchestrator-agents\/.*usage/i,
  /^crates\/orchestrator-store\/.*usage/i,
];

// The pinned treehouse binary (DPES-1044) is what the engine's worktree tests lease from, and what the installer ships,
// so a change to its pin or staging script selects the engine, Tauri and package groups.
const TREEHOUSE_INPUTS = [
  /^apps\/desktop\/src-tauri\/sidecars\/treehouse\//,
  /^apps\/desktop\/scripts\/prepare-treehouse-sidecar\.mjs$/,
];

// A file that no Windows gate reads: anything under docs/, and a Markdown file at the repository root.
const isDocumentation = (file) => file.startsWith("docs/") || (!file.includes("/") && file.endsWith(".md"));

// Changes to the gate harness, the toolchain, a manifest or a lockfile, or the Node tooling run every gate:
// they can change what is built or how it is judged.
const RUNS_EVERYTHING = [
  /^scripts\//,
  /^\.github\//,
  /^\.cargo\//,
  /^deny\.toml$/,
  /(^|\/)Cargo\.(toml|lock)$/,
  /(^|\/)rust-toolchain(\.toml)?$/,
  /(^|\/)package(-lock)?\.json$/,
];

// The usage smoke step runs in the harness and macOS jobs when `smoke` is selected. Setting this to false turns it off.
export const USAGE_SMOKE = { enabled: true };

const ALL = { engine: true, tauri: true, desktop: true, package: true, smoke: true };
const NONE = { engine: false, tauri: false, desktop: false, package: false, smoke: false };

// The groups a change set needs. Conservative: an input this table does not know runs every gate.
// `crates/` feeds the engine, the Tauri host (it path-depends on three crates) and the installer (it bundles
// the daemon). `apps/desktop/src-tauri/` feeds the host and the installer. Frontend source under
// `apps/desktop/src/` feeds only the desktop gates: tsc, vitest and the vite build cover it, and the installer
// packages whatever those produce. Any other `apps/desktop/` file (vite config, index.html, public assets,
// package scripts) can change the installer, so it feeds the desktop gates and the installer.
export function selectGroups({ eventName, changedFiles }) {
  if (eventName !== "pull_request") return { ...ALL, reason: `${eventName || "this event"} runs every Windows gate` };
  if (!Array.isArray(changedFiles)) return { ...ALL, reason: "the change set is unknown, so every Windows gate runs" };
  const selected = { ...NONE };
  for (const file of changedFiles) {
    if (isDocumentation(file)) continue;
    if (RUNS_EVERYTHING.some((re) => re.test(file))) return { ...ALL, reason: `${file} runs every Windows gate` };
    if (SMOKE_INPUTS.some((re) => re.test(file))) selected.smoke = true;
    if (TREEHOUSE_INPUTS.some((re) => re.test(file))) Object.assign(selected, { engine: true, tauri: true, package: true });
    if (file.startsWith("crates/")) Object.assign(selected, { engine: true, tauri: true, package: true });
    else if (file.startsWith("apps/desktop/src-tauri/")) Object.assign(selected, { tauri: true, package: true });
    else if (file.startsWith("apps/desktop/src/")) selected.desktop = true;
    else if (file.startsWith("apps/desktop/")) Object.assign(selected, { desktop: true, package: true });
    else return { ...ALL, reason: `${file} is not a known input, so every Windows gate runs` };
  }
  return { ...selected, reason: "only the groups the changed files need" };
}

// The outputs the `changes` job reports: a group is true only when it is selected and something will run for it.
export function groupOutputs(picked, smokeEnabled = USAGE_SMOKE.enabled) {
  return Object.fromEntries(GROUPS.map((group) => [group, group === "smoke" ? picked.smoke && smokeEnabled : picked[group]]));
}

// The macOS jobs that the group outputs select, in table order.
export const macosJobs = (outputs) => MACOS_JOBS.filter((job) => job.groups.some((g) => outputs[g])).map((job) => job.id);

// The verdict over the needs context of the `gates (windows)` aggregator.
export function windowsVerdict(needs) {
  const outputs = needs.changes?.outputs ?? {};
  const changesResult = needs.changes?.result ?? "missing";
  const rows = [{ job: "changes", group: "-", selected: true, result: changesResult, pass: changesResult === "success" }];
  for (const job of JOBS) {
    const selected = job.group === "harness" || outputs[job.group] === "true";
    const result = needs[job.id]?.result ?? "missing";
    const pass = selected ? result === "success" : result === "skipped" || result === "success";
    rows.push({ job: job.id, group: job.group, selected, result, pass });
  }
  return { ok: rows.every((row) => row.pass), rows };
}

// The files a pull request changes against its base. GitHub's merge commit has the base as its first
// parent. Null when the change set cannot be read: not a merge commit, or git fails.
export function changedFilesFromGit(cwd = process.cwd()) {
  try {
    execFileSync("git", ["rev-parse", "--verify", "--quiet", "HEAD^2"], { cwd, stdio: "ignore" });
  } catch {
    return null;
  }
  try {
    const out = execFileSync("git", ["diff", "-z", "--name-only", "--no-renames", "HEAD^1", "HEAD"], {
      cwd, encoding: "utf8", maxBuffer: 64 * 1024 * 1024,
    });
    return out.split("\0").filter(Boolean);
  } catch {
    return null;
  }
}

const summaryTable = (verdict) => [
  `## Windows gates: ${verdict.ok ? "PASS" : "FAIL"}`,
  "",
  "| Job | Group | Selected | Result | Verdict |",
  "|---|---|---|---|---|",
  ...verdict.rows.map((r) => `| ${r.job} | ${r.group} | ${r.selected} | ${r.result} | ${r.pass ? "ok" : "FAIL"} |`),
  "",
].join("\n");

const optionValue = (argv, name) => {
  const i = argv.indexOf(name);
  if (i === -1 || !argv[i + 1]) throw new Error(`${name} needs a value`);
  return argv[i + 1];
};

function cli(argv) {
  const [command, ...rest] = argv;
  if (command === "select") {
    const eventName = optionValue(rest, "--event");
    const picked = selectGroups({
      eventName,
      changedFiles: eventName === "pull_request" ? changedFilesFromGit() : null,
    });
    process.stderr.write(`${picked.reason}\n`);
    const outputs = groupOutputs(picked);
    for (const group of GROUPS) process.stdout.write(`${group}=${outputs[group]}\n`);
    process.stdout.write(`macos=${JSON.stringify(macosJobs(outputs))}\n`);
    return 0;
  }
  if (command === "gates") {
    const job = JOBS.find((j) => j.id === rest[0]);
    if (!job) throw new Error(`unknown Windows job '${rest[0]}'`);
    process.stdout.write(`${job.gates.join(",")}\n`);
    return 0;
  }
  if (command === "macos-gates") {
    const job = MACOS_JOBS.find((j) => j.id === rest[0]);
    if (!job) throw new Error(`unknown macOS job '${rest[0]}'`);
    process.stdout.write(`${job.gates.join(",")}\n`);
    return 0;
  }
  if (command === "verdict") {
    const verdict = windowsVerdict(JSON.parse(process.env.NEEDS ?? "{}"));
    const table = summaryTable(verdict);
    process.stdout.write(table);
    if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, table);
    return verdict.ok ? 0 : 1;
  }
  throw new Error("usage: windows-jobs.mjs select --event <name> | gates <job-id> | verdict | macos-gates <job-id>");
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  try {
    process.exitCode = cli(process.argv.slice(2));
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 2;
  }
}
