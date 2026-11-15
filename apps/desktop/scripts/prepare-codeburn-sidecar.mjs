// Stages the pinned codeburn reader and the pinned Node.js runtime for the release bundle, where
// Tauri's `bundle.resources` expects them (src-tauri/resources/codeburn). Run from apps/desktop
// by scripts/package.mjs, after prepare-daemon-sidecar.mjs. Fails closed: any hash, signature or
// smoke-run mismatch stops the build, and nothing is staged from an unverified source.
//
// Staged layout (the seam the daemon reads, crates/orchestrator-local-usage ReaderLocations):
//   codeburn/node[.exe]                          official Node.js 22.23.5, unmodified
//   codeburn/pkg/package.json                    upstream codeburn manifest (name and version checked)
//   codeburn/pkg/dist/cli.js                     upstream launcher, unchanged
//   codeburn/pkg/dist/main.js, parse-worker.js   esbuild bundles of the upstream entries
//   codeburn/NOTICES/...                         licenses and the CycloneDX SBOM
import { spawnSync } from "node:child_process";
import { copyFileSync, chmodSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import {
  ensureArchive,
  extractRuntime,
  hostPlatform,
  lockEntry,
  lockfileProblems,
  pinnedFileHash,
  readLockfile,
  readNodeRuntime,
  readPin,
  runReport,
  runtimeEntry,
  runtimeProblems,
  sidecarDir,
} from "../src-tauri/sidecars/codeburn/lib.mjs";
import { writeSyntheticHome } from "../src-tauri/sidecars/codeburn/synthetic-home.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const desktop = resolve(here, "..");
const stage = join(desktop, "src-tauri", "resources", "codeburn");
const bundlerDir = join(sidecarDir, "bundler");
const win = process.platform === "win32";

const fail = (message) => {
  throw new Error(`prepare-codeburn-sidecar: ${message}`);
};

function npm(args, cwd) {
  const result = spawnSync("npm", args, { cwd, encoding: "utf8", shell: win, stdio: ["ignore", "inherit", "inherit"] });
  if (result.error || result.status !== 0) fail(`npm ${args.join(" ")} failed in ${cwd}`);
}

/** Signature check of the bundled node, without credentials. Throws unless it verifies as the pinned signer. */
function verifyNodeSignature(binary, entry) {
  if (process.platform === "darwin") {
    const verify = spawnSync("codesign", ["--verify", "--strict", "--verbose=2", binary], { encoding: "utf8" });
    if (verify.status !== 0) fail(`codesign --verify --strict failed for ${binary}: ${verify.stderr.trim()}`);
    const details = spawnSync("codesign", ["-dvv", binary], { encoding: "utf8" });
    if (!`${details.stderr}`.includes(`Authority=${entry.signerSubject}`)) {
      fail(`bundled node is not signed by "${entry.signerSubject}"`);
    }
    const entitlements = spawnSync("codesign", ["-d", "--entitlements", ":-", binary], { encoding: "utf8" });
    const jit = /com\.apple\.security\.cs\.allow-jit<\/key>\s*<true\/>/.test(entitlements.stdout);
    if (!jit) fail("bundled node lost its com.apple.security.cs.allow-jit entitlement; V8 cannot start under the hardened runtime");
    return `codesign verified; signer ${entry.signerSubject}; allow-jit present`;
  }
  if (win) {
    const script =
      "$s = Get-AuthenticodeSignature -LiteralPath $env:MCAO_NODE; " +
      "[pscustomobject]@{ Status = [string]$s.Status; Subject = [string]$s.SignerCertificate.Subject } | ConvertTo-Json -Compress";
    const result = spawnSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script], {
      encoding: "utf8",
      env: { ...process.env, MCAO_NODE: binary },
    });
    if (result.status !== 0) fail(`Get-AuthenticodeSignature failed: ${result.stderr.trim()}`);
    const { Status, Subject } = JSON.parse(result.stdout);
    if (Status !== "Valid") fail(`bundled node.exe Authenticode status is ${Status}, expected Valid`);
    if (!Subject.includes(entry.signerSubject)) fail(`bundled node.exe is signed by "${Subject}", expected "${entry.signerSubject}"`);
    return `Authenticode Valid; signer ${Subject}`;
  }
  fail(`no signature check for ${process.platform}`);
}

function fileBytes(dir) {
  let bytes = 0;
  let files = 0;
  for (const name of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, name.name);
    if (name.isDirectory()) {
      const inner = fileBytes(path);
      bytes += inner.bytes;
      files += inner.files;
    } else {
      bytes += statSync(path).size;
      files += 1;
    }
  }
  return { bytes, files };
}

const platform = hostPlatform();
if (!platform) fail(`no pinned Node.js archive for ${process.platform}-${process.arch}; packaging supports darwin-arm64 and win-x64`);
const runtime = readNodeRuntime();
const runtimeProblemList = runtimeProblems(runtime, readFileSync(join(sidecarDir, runtime.shasums), "utf8"));
if (runtimeProblemList.length) fail(`pinned Node.js hashes disagree with ${runtime.shasums}:\n  ${runtimeProblemList.join("\n  ")}`);
const pin = readPin();

// 1. Install exactly the lockfile trees. npm verifies every sha512 integrity value while installing.
npm(["ci", "--omit=dev", "--ignore-scripts", "--no-audit", "--no-fund", "--loglevel=error"], sidecarDir);
npm(["ci", "--ignore-scripts", "--no-audit", "--no-fund", "--loglevel=error"], bundlerDir);

const lock = readLockfile();
const problems = lockfileProblems(lock);
if (problems.length) fail(`lockfile is not fully pinned:\n  ${problems.join("\n  ")}`);
const codeburnLock = lockEntry(lock, "codeburn");
if (codeburnLock?.version !== pin.version || codeburnLock.integrity !== pin.tarball_integrity) {
  fail(`lockfile codeburn ${codeburnLock?.version} does not match the pin ${pin.version} / ${pin.tarball_integrity}`);
}
const installed = JSON.parse(readFileSync(join(sidecarDir, "node_modules", "codeburn", "package.json"), "utf8"));
if (installed.name !== pin.name || installed.version !== pin.version) {
  fail(`installed ${installed.name}@${installed.version}, expected ${pin.name}@${pin.version}`);
}
const upstreamDist = join(sidecarDir, "node_modules", "codeburn", "dist");
for (const [relative, expected] of Object.entries(pin.layouts.npm)) {
  const actual = pinnedFileHash(join(upstreamDist, relative.replace(/^dist\//, "")), relative === "dist/cli.js");
  if (actual !== expected) fail(`installed ${relative} does not match the npm layout pin`);
}

// 2. Stage: start clean (the folder is generated), bundle the two entries, copy the rest.
rmSync(stage, { recursive: true, force: true });
const pkg = join(stage, "pkg");
const { bundleReader } = await import(pathToFileURL(join(bundlerDir, "bundle.mjs")).href);
await bundleReader({ installRoot: sidecarDir, outDir: join(pkg, "dist") });
copyFileSync(join(upstreamDist, "cli.js"), join(pkg, "dist", "cli.js"));
copyFileSync(join(sidecarDir, "node_modules", "codeburn", "package.json"), join(pkg, "package.json"));
const notices = join(stage, "NOTICES");
mkdirSync(notices, { recursive: true });
copyFileSync(join(sidecarDir, "node_modules", "codeburn", "LICENSE"), join(notices, "codeburn-LICENSE"));
copyFileSync(join(sidecarDir, "node_modules", "codeburn", "THIRD_PARTY_NOTICES.md"), join(notices, "codeburn-THIRD_PARTY_NOTICES.md"));
copyFileSync(join(sidecarDir, "sbom.cdx.json"), join(notices, "codeburn-sbom.cdx.json"));

// 3. The shipped reader must be the pinned bundle, byte for byte.
for (const [relative, expected] of Object.entries(pin.layouts.bundled)) {
  const actual = pinnedFileHash(join(stage, "pkg", relative), relative === "dist/cli.js");
  if (actual !== expected) {
    fail(`staged ${relative} hashes to ${actual}, the pin says ${expected}. Rebuild is not reproducible, or the pin needs the upgrade procedure in docs/third-party/codeburn.md`);
  }
}

// 4. The bundled Node.js: pinned archive, pinned binary, signature, then copied out.
const cache = join(process.env.MCAO_NODE_CACHE ?? tmpdir(), "mcao-node-archives");
const archive = await ensureArchive(runtime, platform, cache);
const extractDir = mkdtempSync(join(tmpdir(), "mcao-node-extract-"));
const extracted = extractRuntime(runtime, platform, archive, extractDir);
const entry = runtimeEntry(runtime, platform);
const nodeName = win ? "node.exe" : "node";
copyFileSync(extracted.binary, join(stage, nodeName));
if (!win) chmodSync(join(stage, nodeName), 0o755);
copyFileSync(extracted.license, join(notices, "node-LICENSE"));
rmSync(extractDir, { recursive: true, force: true });
const signature = verifyNodeSignature(join(stage, nodeName), entry);

// 5. Smoke run with the staged runtime, on a synthetic home (no real data).
const version = spawnSync(join(stage, nodeName), ["--version"], { encoding: "utf8" });
if (version.status !== 0 || version.stdout.trim() !== `v${runtime.version}`) {
  fail(`staged node reports ${version.stdout.trim() || version.stderr.trim()}, expected v${runtime.version}`);
}
const home = mkdtempSync(join(tmpdir(), "mcao-codeburn-home-"));
const reportCache = join(home, "cache");
mkdirSync(reportCache, { recursive: true });
try {
  writeSyntheticHome(home);
  const claude = runReport(join(stage, nodeName), join(pkg, "dist", "cli.js"), home, reportCache, "claude");
  const codex = runReport(join(stage, nodeName), join(pkg, "dist", "cli.js"), home, reportCache, "codex");
  if (claude.overview.calls !== 2 || codex.overview.calls !== 1) {
    fail(`staged reader totals are claude ${claude.overview.calls} and codex ${codex.overview.calls} calls, expected 2 and 1`);
  }
} finally {
  rmSync(home, { recursive: true, force: true });
}

const size = fileBytes(stage);
console.log(`staged ${stage}`);
console.log(`  node ${runtime.version} ${platform}: ${signature}`);
console.log(`  node binary sha256 ${extracted.binarySha256}`);
console.log(`  ${size.files} files, ${size.bytes} bytes (apparent)`);
