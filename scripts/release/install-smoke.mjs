// Install and upgrade smoke test of a packaged release artifact.
//
//   node scripts/release/install-smoke.mjs <macos|windows> --artifact-dir DIR --work DIR
//        [--old-artifact-dir DIR] [--smoke-bin PATH] [--smoke-work DIR] [--require-signed] [--out FILE]
//
// --artifact-dir      the --out directory of `package.mjs` (artifact, SHA256SUMS, release-info.json).
// --old-artifact-dir  the same for a PREVIOUS release; without it the run is a reinstall, not an upgrade.
// --smoke-bin         the `release_smoke` example built from `crates/orchestrator-daemon`.
// --smoke-work        a new, SHORT directory for the smoke's repositories and data directory (a Unix
//                     socket path is limited to about 100 bytes). Default: --work/s.
// --require-signed    fail unless the NEW artifact is really signed (and notarized when release-info
//                     says so). Without it an unsigned artifact is reported, not failed.
// --out               write the step results as JSON.
// Exit 0 when no step is FAIL. UNPROVEN steps are listed and never count as a pass.
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { readArtifactDir, runMacos, runWindows } from "./install-lib.mjs";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const [platform, ...rest] = process.argv.slice(2);
const flags = { requireSigned: false };
const valued = { "--artifact-dir": "artifactDir", "--old-artifact-dir": "oldArtifactDir", "--work": "work", "--smoke-bin": "smokeBin", "--smoke-work": "smokeWork", "--out": "out" };
for (let i = 0; i < rest.length; i++) {
  if (rest[i] === "--require-signed") flags.requireSigned = true;
  else if (valued[rest[i]] && rest[i + 1]) flags[valued[rest[i]]] = resolve(rest[++i]);
  else fail(`unknown or incomplete argument ${rest[i]}`);
}
function fail(message) {
  console.error(`${message}\nusage: node scripts/release/install-smoke.mjs <macos|windows> --artifact-dir DIR --work DIR [--old-artifact-dir DIR] [--smoke-bin PATH] [--smoke-work DIR] [--require-signed] [--out FILE]`);
  process.exit(2);
}
if (!["macos", "windows"].includes(platform) || !flags.artifactDir || !flags.work) fail("platform, --artifact-dir and --work are required");
if ((platform === "macos") !== (process.platform === "darwin") || (platform === "windows") !== (process.platform === "win32")) fail(`cannot test a ${platform} install on ${process.platform}`);
if (existsSync(flags.work)) fail(`--work exists, refusing to reuse it: ${flags.work}`);
mkdirSync(flags.work, { recursive: true });
flags.smokeWork ??= join(flags.work, "s");
if (existsSync(flags.smokeWork)) fail(`--smoke-work exists, refusing to reuse it: ${flags.smokeWork}`);

const version = JSON.parse(readFileSync(join(repo, "apps", "desktop", "src-tauri", "tauri.conf.json"), "utf8")).version;
const run = (command, args, options = {}) => {
  const result = spawnSync(command, args, {
    encoding: "utf8", env: { ...process.env, ...options.env }, stdio: options.stream ? ["ignore", "inherit", "inherit"] : "pipe",
  });
  return { code: result.status ?? -1, out: result.stdout ?? "", err: result.stderr ?? (result.error?.message ?? "") };
};

const ctx = { ...flags, platform, version, run, exists: existsSync, readDir: readArtifactDir };
const steps = (platform === "macos" ? runMacos : runWindows)(ctx);
const counts = Object.fromEntries(["PASS", "FAIL", "INFO", "UNPROVEN"].map((v) => [v, steps.items.filter((s) => s.verdict === v).length]));
console.log(`\ninstall-smoke ${platform}: ${steps.failed ? "FAIL" : "no failed step"} ${JSON.stringify(counts)}`);
if (counts.UNPROVEN) console.log("UNPROVEN steps above are NOT passes.");
if (flags.out) writeFileSync(flags.out, `${JSON.stringify({ platform, version, requireSigned: flags.requireSigned, counts, steps: steps.items }, null, 2)}\n`);
process.exit(steps.failed ? 1 : 0);
