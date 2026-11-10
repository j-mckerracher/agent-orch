// Runs the native gates of one platform, once each, and records evidence.
//
//   node scripts/ci/run-gates.mjs --evidence-dir D --target-dir T --test-tmp P [--only a,b] [--list]
//
// Evidence per gate: <name>.stdout.txt, <name>.stderr.txt (raw bytes), <name>.result.json. A gate
// is PASS only when the process exits 0 within its limit AND its output matches the expected
// shape (counts, sequences, names, artifacts) AND no process of the run survives it (a read-only
// scan for processes under the target or test-temp directory). Nothing is retried. A gate whose
// prerequisite did not pass is SKIPPED, which also fails the run. The evidence directory must not
// already exist.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  appendFileSync, existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync,
} from "node:fs";
import os from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { runBounded } from "./bounded.mjs";
import { UNPROVEN_ROWS, gatesFor } from "./gates.mjs";
import {
  checkBuild, checkLifecycle, checkLint, checkPattern, checkTests, checkVitest, diagnostics, normalize,
} from "./parse.mjs";
import { describeSurvivors, scanSurvivors } from "./survivors.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const win = process.platform === "win32";

const parseArgs = (argv) => {
  const out = { only: null, list: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--list") out.list = true;
    else if (["--evidence-dir", "--target-dir", "--test-tmp", "--only", "--source"].includes(a)) {
      const v = argv[++i];
      if (!v) throw new Error(`${a} needs a value`);
      out[a.slice(2).replace(/-(\w)/g, (_, c) => c.toUpperCase())] = v;
    } else throw new Error(`unknown argument ${a}`);
  }
  return out;
};

const args = parseArgs(process.argv.slice(2));
const source = resolve(args.source ?? join(here, "..", ".."));
let gates = gatesFor(process.platform);
if (args.only) {
  const wanted = args.only.split(",");
  const unknown = wanted.filter((n) => !gates.some((g) => g.name === n));
  if (unknown.length) throw new Error(`unknown gate(s): ${unknown}`);
  gates = gates.filter((g) => wanted.includes(g.name));
}
if (args.list) {
  for (const g of gates) console.log(`${g.name}\t${g.exe} ${g.args.join(" ")}\t(${g.cwd})`);
  process.exit(0);
}
for (const k of ["evidenceDir", "targetDir", "testTmp"]) {
  if (!args[k]) throw new Error(`--${k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)} is required`);
}
const evidence = resolve(args.evidenceDir);
const target = resolve(args.targetDir);
const testTmp = resolve(args.testTmp);
if (existsSync(evidence)) throw new Error(`evidence directory exists, refusing to reuse it: ${evidence}`);
if (existsSync(target) && readdirSync(target).length > 0) throw new Error(`target directory is not empty: ${target}`);
mkdirSync(evidence, { recursive: true });
mkdirSync(target, { recursive: true });
mkdirSync(testTmp, { recursive: true });

const baseEnv = { ...process.env };
delete baseEnv.AGENT_ORCH_DAEMON_BIN; // the src-tauri real-daemon tests must build the daemon of this source
delete baseEnv.AGENT_ORCH_SEED_BIN;
Object.assign(baseEnv, {
  CARGO_TARGET_DIR: target,
  CARGO_TERM_COLOR: "never",
  RUST_BACKTRACE: "1",
  NO_COLOR: "1",
  FORCE_COLOR: "0",
  CI: "true", // the Tauri CLI binds --ci to CI and accepts only true or false
});

const capture = (command, commandArgs, cwd = source) => {
  const r = spawnSync(command, commandArgs, { cwd, encoding: "utf8", env: baseEnv, shell: win && /^np[mx]$/.test(command) });
  return { code: r.status, out: (r.stdout ?? "").trim(), err: (r.stderr ?? "").trim() };
};

const sha256 = (file) => createHash("sha256").update(readFileSync(file)).digest("hex");

const results = [];
const writeJson = (name, value) => writeFileSync(join(evidence, name), `${JSON.stringify(value, null, 2)}\n`);

const git = (...gitArgs) => capture("git", gitArgs);

// ---- environment ----
const envLines = [
  `platform: ${process.platform} ${os.release()} ${os.arch()}`,
  `node: ${process.version}`,
  `npm: ${capture("npm", ["-v"]).out}`,
  `git: ${git("--version").out}`,
  `cargo: ${capture("cargo", ["-V"]).out}`,
  `rustc: ${capture("rustc", ["-vV"]).out.replace(/\n/g, " | ")}`,
  `HEAD: ${git("rev-parse", "HEAD").out}`,
  `tree: ${git("rev-parse", "HEAD^{tree}").out}`,
  `core.autocrlf: ${git("config", "--show-origin", "--get", "core.autocrlf").out || "(unset)"}`,
  `core.longpaths: ${git("config", "--show-origin", "--get", "core.longpaths").out || "(unset)"}`,
  `CARGO_TARGET_DIR: ${target}`,
  `TEST_TMP: ${testTmp}`,
  `CI: ${baseEnv.CI}`,
  `source: ${source}`,
];
writeFileSync(join(evidence, "environment.txt"), `${envLines.join("\n")}\n`);

// ---- harness checks (not gates) ----
const harness = [];
const harnessCheck = (name, problems, notes = "") => {
  harness.push({ name, verdict: problems.length ? "FAIL" : "PASS", problems, notes });
  writeJson(`${name}.result.json`, harness.at(-1));
};

const autocrlf = git("config", "--get", "core.autocrlf").out;
const longpaths = git("config", "--get", "core.longpaths").out;
harnessCheck("git-config", [
  ...(autocrlf === "true" ? ["core.autocrlf is true; CI must set it to false before checkout"] : []),
  ...(win && longpaths !== "true" ? ["core.longpaths is not true on Windows"] : []),
], `core.autocrlf=${autocrlf || "(unset)"} core.longpaths=${longpaths || "(unset)"}`);

const gitAttrFiles = [
  "apps/desktop/src-tauri/permissions/autogenerated/daemon_close.toml",
  "apps/desktop/src-tauri/permissions/autogenerated/daemon_open.toml",
  "apps/desktop/src-tauri/permissions/autogenerated/daemon_send.toml",
  "Cargo.lock",
  "apps/desktop/src-tauri/Cargo.lock",
  "apps/desktop/package-lock.json",
];
// These generated and lock files must be LF in the working tree under any core.autocrlf.
const checkGitAttributes = (label) => {
  const problems = [];
  const attr = git("check-attr", "text", "eol", "--", ...gitAttrFiles);
  if (attr.code !== 0) problems.push("git check-attr failed");
  const lines = attr.out.split(/\r?\n/);
  for (const file of gitAttrFiles) {
    for (const [key, value] of [["text", "set"], ["eol", "lf"]]) {
      if (!lines.includes(`${file}: ${key}: ${value}`)) problems.push(`missing '${file}: ${key}: ${value}'`);
    }
    const path = join(source, file);
    if (!existsSync(path)) problems.push(`${file} is absent`);
    else if (readFileSync(path).includes(13)) problems.push(`${file} holds a carriage return in the working tree`);
  }
  harnessCheck(`gitattributes-${label}`, problems);
};
checkGitAttributes("before");
const statusBefore = git("status", "--porcelain=v1", "--untracked-files=all");
harnessCheck("source-status-before", statusBefore.out ? [`checkout is not clean:\n${statusBefore.out}`] : []);

// ---- gates ----
const status = new Map();
const baselineNew = [];
const resolveEnv = (gate) => {
  const env = { ...baseEnv };
  if (gate.isolated) Object.assign(env, { TEMP: testTmp, TMP: testTmp, TMPDIR: testTmp });
  for (const [k, v] of Object.entries(gate.env ?? {})) {
    env[k] = typeof v === "string" ? v : join(target, v.target + (win ? ".exe" : ""));
  }
  return env;
};

for (const gate of gates) {
  const base = join(evidence, gate.name);
  const header = { gate: gate.name, command: [gate.exe, ...gate.args].join(" "), cwd: gate.cwd, platform: process.platform };
  const failedNeeds = (gate.needs ?? []).filter((n) => gates.some((g) => g.name === n) && status.get(n) !== "PASS");
  console.log(`::group::${gate.name}: ${header.command}`);
  let result;
  if (failedNeeds.length) {
    const problems = [`prerequisite gate(s) did not pass: ${failedNeeds.join(", ")}`];
    result = { ...header, verdict: "SKIPPED", problems };
    console.log(`SKIPPED ${gate.name}: ${problems[0]}`);
  } else {
    const run = await runBounded({
      exe: gate.exe, commandArgs: gate.args, cwd: resolve(source, gate.cwd), env: resolveEnv(gate),
      timeoutMs: (gate.timeout ?? 1800) * 1000, stdoutFile: `${base}.stdout.txt`, stderrFile: `${base}.stderr.txt`,
    });
    const problems = [];
    if (run.spawnError) problems.push(`could not start: ${run.spawnError}`);
    if (run.pipesAbandoned) problems.push("a descendant kept the output pipes open after the process exited; the pipes were abandoned");
    if (run.timedOut) problems.push(`timed out after ${gate.timeout ?? 1800} s`);
    else if (run.code !== 0) problems.push(`exit code ${run.code}${run.signal ? ` (signal ${run.signal})` : ""}`);
    const both = `${run.stdout}\n${run.stderr}`;
    let newDiagnostics = [];
    if (!problems.length) {
      if (gate.mode === "test") {
        problems.push(...checkTests({ stdout: run.stdout, ...gate }));
        if (gate.pattern) problems.push(...checkPattern(run.stdout, gate.pattern));
      } else if (gate.mode === "lifecycle") problems.push(...checkLifecycle(run.stdout));
      else if (gate.mode === "build") problems.push(...checkBuild(run.stderr, gate));
      else if (gate.mode === "lint") {
        problems.push(...checkLint(run.stderr, gate));
        const baseline = gate.baseline ? new RegExp(gate.baseline, "i") : null;
        newDiagnostics = diagnostics(run.stderr).filter((d) => !(baseline && baseline.test(d.location ?? "")));
        for (const d of newDiagnostics) baselineNew.push({ gate: gate.name, ...d });
      } else if (gate.mode === "vitest") problems.push(...checkVitest(both, gate));
      else if (gate.mode === "exit" && gate.pattern) problems.push(...checkPattern(both, gate.pattern));
      for (const artifact of gate.artifacts ?? []) {
        const path = join(target, artifact.target);
        if (!existsSync(path)) problems.push(`artifact missing: ${artifact.target}`);
        else {
          const size = statSync(path).size;
          writeFileSync(join(evidence, artifact.record), `${artifact.target}\nsize: ${size}\nsha256: ${sha256(path)}\n`);
        }
      }
    }
    // Read-only, after EVERY gate: nothing is stopped. A survivor fails the gate.
    try {
      const survivors = await scanSurvivors([target, testTmp]);
      writeFileSync(`${base}.survivors.txt`, survivors.length ? `${describeSurvivors(survivors)}\n` : "none\n");
      if (survivors.length) problems.push(`${survivors.length} process(es) of this run survived the gate; see ${gate.name}.survivors.txt`);
    } catch (error) {
      problems.push(`survivor scan failed: ${error.message}`);
    }
    result = {
      ...header, verdict: problems.length ? "FAIL" : "PASS", exitCode: run.code, durationSeconds: Math.round(run.durationMs / 1000),
      problems, newDiagnostics: newDiagnostics.length,
    };
    console.log(`${result.verdict} ${gate.name} (${result.durationSeconds} s)${problems.length ? `: ${problems.join("; ")}` : ""}`);
  }
  console.log("::endgroup::");
  if (result.verdict !== "PASS") console.log(`::error title=${gate.name} ${result.verdict}::${result.problems.join("; ").replace(/\n/g, " ")}`);
  status.set(gate.name, result.verdict);
  results.push(result);
  writeJson(`${gate.name}.result.json`, result);
}

// ---- after the gates ----
checkGitAttributes("after");
const statusAfter = git("status", "--porcelain=v1", "--untracked-files=all");
harnessCheck("source-status-after", statusAfter.out ? [`gates left the checkout dirty:\n${statusAfter.out}`] : []);
const diffAfter = git("diff");
writeFileSync(join(evidence, "source-diff-after.txt"), diffAfter.out);
harnessCheck("source-diff-after", diffAfter.out ? ["tracked files changed during the run (see source-diff-after.txt)"] : []);

// ---- summary ----
const rows = [...results, ...harness].map((r) => {
  const note = r.problems?.length ? r.problems.join("; ").replace(/\n/g, " ").replace(/\|/g, "/") : (r.notes ?? "");
  return `| ${r.gate ?? r.name} | ${r.verdict} | ${r.exitCode ?? ""} | ${r.durationSeconds ?? ""} | ${note} |`;
});
const overall = [...results, ...harness].every((r) => r.verdict === "PASS") && results.length > 0;
const summary = [
  `## Native gates: ${process.platform} (${overall ? "PASS" : "FAIL"})`,
  "",
  `Source ${git("rev-parse", "HEAD").out} (tree ${git("rev-parse", "HEAD^{tree}").out}).`,
  "",
  "| Gate | Verdict | Exit | Seconds | Notes |",
  "|---|---|---|---|---|",
  ...rows,
  "",
  ...(baselineNew.length
    ? [`### Clippy diagnostics outside the baseline (reported, not judged): ${baselineNew.length}`, "",
       ...baselineNew.slice(0, 40).map((d) => `- ${d.gate}: ${d.message} ${d.location ?? ""}`), ""]
    : []),
  ...(process.platform === "win32"
    ? ["### Not covered by this result (UNPROVEN by construction)", "", ...UNPROVEN_ROWS.map((r) => `- ${r}`), ""]
    : ["### macOS notes", "", "- Windows-only gates (`windows-transport-tests`, the installer) run on the Windows job; Windows runtime behaviour is not proven by this job.", ""]),
].join("\n");
writeFileSync(join(evidence, "summary.md"), summary);
if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${summary}\n`);
console.log(summary);
process.exit(overall ? 0 : 1);
