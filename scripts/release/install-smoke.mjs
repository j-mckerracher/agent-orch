// Install and upgrade smoke test of a packaged release artifact.
//
//   node scripts/release/install-smoke.mjs <macos|windows> --artifact-dir DIR --work DIR
//        [--old-artifact-dir DIR] [--smoke-bin PATH] [--smoke-work DIR] (--require-signed | --allow-unsigned) [--out FILE]
//
// --artifact-dir      the --out directory of `package.mjs` (artifact, SHA256SUMS, release-info.json).
// --old-artifact-dir  the same for a PREVIOUS release; without it the run is a reinstall, not an upgrade. An old
//                     build with the same version or the same sidecar bytes is a labelled same-version rerun
//                     and the N-1 upgrade stays UNPROVEN.
// --smoke-bin         the `release_smoke` example built from `crates/orchestrator-daemon`.
// --smoke-work        a new, SHORT directory for the smoke's repositories and data directory (a Unix
//                     socket path is limited to about 100 bytes). Default: --work/s.
// --require-signed    fail unless the NEW artifact is really signed (and notarized when release-info
//                     says so).
// --allow-unsigned    accept an unsigned NEW artifact: it is reported (INFO), not failed, and nothing about
//                     signing is proven. Exactly one of --require-signed and --allow-unsigned is required, so
//                     an unsigned build can never pass this gate by omission.
// --out               write the step results as JSON, with the source commit, tree and checkout status under `source`.
// Exit 0 when no step is FAIL. UNPROVEN steps are listed and never count as a pass. The smoke binary is
// accepted only when it exits 0 AND writes a valid structured result (see validateSmokeResult).
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, rmdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describeSource, parsePorcelain, readArtifactDir, runMacos, runWindows } from "./install-lib.mjs";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const [platform, ...rest] = process.argv.slice(2);
const flags = { requireSigned: false, allowUnsigned: false };
const valued = { "--artifact-dir": "artifactDir", "--old-artifact-dir": "oldArtifactDir", "--work": "work", "--smoke-bin": "smokeBin", "--smoke-work": "smokeWork", "--out": "out" };
for (let i = 0; i < rest.length; i++) {
  if (rest[i] === "--require-signed") flags.requireSigned = true;
  else if (rest[i] === "--allow-unsigned") flags.allowUnsigned = true;
  else if (valued[rest[i]] && rest[i + 1]) flags[valued[rest[i]]] = resolve(rest[++i]);
  else fail(`unknown or incomplete argument ${rest[i]}`);
}
function fail(message) {
  console.error(`${message}\nusage: node scripts/release/install-smoke.mjs <macos|windows> --artifact-dir DIR --work DIR [--old-artifact-dir DIR] [--smoke-bin PATH] [--smoke-work DIR] (--require-signed | --allow-unsigned) [--out FILE]`);
  process.exit(2);
}
if (!["macos", "windows"].includes(platform) || !flags.artifactDir || !flags.work) fail("platform, --artifact-dir and --work are required");
if (flags.requireSigned === flags.allowUnsigned) fail("give exactly one of --require-signed and --allow-unsigned");
if ((platform === "macos") !== (process.platform === "darwin") || (platform === "windows") !== (process.platform === "win32")) fail(`cannot test a ${platform} install on ${process.platform}`);
if (existsSync(flags.work)) fail(`--work exists, refusing to reuse it: ${flags.work}`);
mkdirSync(flags.work, { recursive: true });
flags.smokeWork ??= join(flags.work, "s");
if (existsSync(flags.smokeWork)) fail(`--smoke-work exists, refusing to reuse it: ${flags.smokeWork}`);

const version = JSON.parse(readFileSync(join(repo, "apps", "desktop", "src-tauri", "tauri.conf.json"), "utf8")).version;
// An override of undefined removes the variable, whatever its case (Windows environment names are case-insensitive).
const childEnv = (overrides = {}) => {
  const env = { ...process.env };
  for (const [key, value] of Object.entries(overrides)) {
    for (const existing of Object.keys(env)) if (existing.toUpperCase() === key.toUpperCase()) delete env[existing];
    if (value !== undefined) env[key] = value;
  }
  return env;
};
const run = (command, args, options = {}) => {
  const result = spawnSync(command, args, {
    encoding: "utf8", env: childEnv(options.env), stdio: options.stream ? ["ignore", "inherit", "inherit"] : "pipe",
  });
  return { code: result.status ?? -1, signal: result.signal, error: result.error?.message, out: result.stdout ?? "", err: result.stderr ?? (result.error?.message ?? "") };
};

// The source checkout this run was started from: commit, tree and `git status` before and after, so a dirty
// checkout is explained by the paths that changed during the run instead of being an unexplained flag.
const gitInRepo = (...args) => {
  const result = run("git", ["-C", repo, ...args]);
  return result.code === 0 ? result.out.trimEnd() : null;
};
const sourceSnapshot = () => {
  const porcelain = gitInRepo("status", "--porcelain=v1", "--untracked-files=all");
  return porcelain === null ? null : parsePorcelain(porcelain);
};
const sourceHead = gitInRepo("rev-parse", "HEAD");
const sourceTree = gitInRepo("rev-parse", "HEAD^{tree}");
const statusBefore = sourceSnapshot();

// The daemon's default Windows data directory: %LOCALAPPDATA%\agent-orch (crates/orchestrator-daemon/src/main.rs).
// The directory is removed again only if this run created it and nothing else is in it.
const userData = (() => {
  const base = process.env.LOCALAPPDATA;
  if (platform !== "windows" || !base) return { root: null };
  const root = join(base, "agent-orch");
  const created = !existsSync(root);
  return {
    root,
    seed: (name, text) => { mkdirSync(root, { recursive: true }); writeFileSync(join(root, name), text, { flag: "wx" }); },
    read: (name) => (existsSync(join(root, name)) ? readFileSync(join(root, name), "utf8") : null),
    remove: (name) => {
      rmSync(join(root, name), { force: true });
      if (created && readdirSync(root).length === 0) rmdirSync(root);
    },
  };
})();

const ctx = {
  ...flags, platform, version, run, exists: existsSync, readDir: readArtifactDir,
  smokeOut: join(flags.work, "release-smoke-result.json"),
  readText: (path) => (existsSync(path) ? readFileSync(path, "utf8") : null),
  fileHash: (path) => (existsSync(path) ? createHash("sha256").update(readFileSync(path)).digest("hex") : null),
  userData, nonce: `${process.pid}-${Date.now()}`,
};
const steps = (platform === "macos" ? runMacos : runWindows)(ctx);
const source = describeSource({ head: sourceHead, tree: sourceTree, before: statusBefore, after: sourceSnapshot() });
const counts = Object.fromEntries(["PASS", "FAIL", "INFO", "UNPROVEN"].map((v) => [v, steps.items.filter((s) => s.verdict === v).length]));
console.log(`\ninstall-smoke ${platform}: ${steps.failed ? "FAIL" : "no failed step"} ${JSON.stringify(counts)}`);
console.log(`upgrade: ${steps.upgrade === "upgrade" ? "a different older build ran (N-1 to N)" : `NOT proven (${steps.upgrade})`}`);
if (counts.UNPROVEN) console.log("UNPROVEN steps above are NOT passes.");
for (const line of source.lines) console.log(line);
if (flags.out) {
  const signedPolicy = flags.requireSigned ? "require-signed" : "allow-unsigned";
  const { lines, ...sourceFacts } = source;
  writeFileSync(flags.out, `${JSON.stringify({ platform, version, signedPolicy, upgrade: { kind: steps.upgrade, proven: steps.upgrade === "upgrade" }, source: sourceFacts, counts, steps: steps.items }, null, 2)}\n`);
}
process.exit(steps.failed ? 1 : 0);
