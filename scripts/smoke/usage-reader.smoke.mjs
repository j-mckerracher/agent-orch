// Smoke test for the pinned local-usage reader (DPES-1105), run by the "Usage reader smoke test" step
// in ci.yml (Windows and macOS, when the reader's inputs change) and by audit.yml (Linux). It lives
// outside scripts/**/*.test.mjs so the Harness self-tests glob does not install the reader.
//
// It checks the pin, then installs the LOCKED codeburn tree from
// apps/desktop/src-tauri/sidecars/codeburn/package-lock.json (npm verifies every sha512 value),
// builds the release bundle from it with the locked esbuild, and checks two layouts against the one
// pin table (crates/orchestrator-local-usage/codeburn-pin.json): the npm layout (development builds)
// and the bundled layout (release builds). Then it runs the reader on a SYNTHETIC home. On hosts with
// a pinned Node.js archive (darwin-arm64, win-x64) the reader runs under the BUNDLED node, downloaded
// and checked against sidecars/codeburn/node-runtime.json; other hosts use this test's own node.
//
// Every failure names its step: "registry", "install", "bundle" or "reader". Nothing here is skipped:
// a test that cannot run fails, because a skipped test would still count as a pass.
import { after, test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
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
  sidecarDir,
} from "../../apps/desktop/src-tauri/sidecars/codeburn/lib.mjs";
import { todayUtc, writeSyntheticHome } from "../../apps/desktop/src-tauri/sidecars/codeburn/synthetic-home.mjs";

const bundlerSource = join(sidecarDir, "bundler", "bundle.mjs");
const pin = readPin();
const isWindows = process.platform === "win32";

function parseVersion(text) {
  const [major, minor = "0", patch = "0"] = text.replace(/^v/, "").trim().split(".");
  return [Number(major), Number(minor), Number(patch)];
}

function atLeast(found, minimum) {
  for (let index = 0; index < 3; index += 1) {
    if (found[index] !== minimum[index]) return found[index] > minimum[index];
  }
  return true;
}

/** Runs npm with fixed arguments. On Windows npm is a .cmd shim, which needs a shell. */
function runNpm(args, options) {
  return spawnSync(isWindows ? "npm.cmd" : "npm", args, { ...options, encoding: "utf8", shell: isWindows });
}

function tail(text, limit = 400) {
  const clean = String(text ?? "").trim();
  return clean.length > limit ? `...${clean.slice(-limit)}` : clean;
}

function expectedHashes(files, root, layout) {
  for (const [relative, expected] of Object.entries(files)) {
    const actual = pinnedFileHash(join(root, relative), relative === "dist/cli.js");
    if (actual !== expected) {
      throw new Error(`${step(layout)}: ${relative} does not match the ${layout} layout pin (${actual} vs ${expected})`);
    }
  }
  return Object.keys(files).length;
}

function step(layout) {
  return layout === "bundled" ? "bundle" : "install";
}

let installed;
/** Installs the locked tree and builds the bundle once for every test in this file. */
function install() {
  installed ??= (async () => {
    const registry = runNpm(["view", `${pin.name}@${pin.version}`, "dist.integrity", "--json"], { timeout: 60_000 });
    if (registry.status !== 0) {
      throw new Error(`registry: npm view failed (status ${registry.status}): ${tail(registry.stderr)}`);
    }
    const served = JSON.parse(registry.stdout || "null");
    if (served !== pin.tarball_integrity) {
      throw new Error(
        `registry: npm served integrity ${served} for ${pin.name}@${pin.version}, but the pin says ${pin.tarball_integrity}. ` +
          "Either the pin is stale or the registry served a different tarball; do not update the pin without checking the release.",
      );
    }

    const prefix = mkdtempSync(join(tmpdir(), "mcao-codeburn-pin-"));
    const sidecar = join(prefix, "sidecar");
    mkdirSync(join(sidecar, "bundler"), { recursive: true });
    for (const file of ["package.json", "package-lock.json"]) {
      copyFileSync(join(sidecarDir, file), join(sidecar, file));
      copyFileSync(join(sidecarDir, "bundler", file), join(sidecar, "bundler", file));
    }
    const lock = readLockfile(join(sidecar, "package-lock.json"));
    const problems = lockfileProblems(lock);
    if (problems.length) throw new Error(`install: the lockfile is not fully pinned: ${problems.join("; ")}`);
    const entry = lockEntry(lock, pin.name);
    if (entry?.version !== pin.version || entry.integrity !== pin.tarball_integrity) {
      throw new Error(`install: the lockfile's ${pin.name} is ${entry?.version} ${entry?.integrity}, the pin says ${pin.version} ${pin.tarball_integrity}`);
    }
    // Both trees come from their lockfiles. `npm ci` checks each sha512 value as it installs.
    const ci = (cwd, args, label) => {
      const result = spawnSync(isWindows ? "npm.cmd" : "npm", ["ci", ...args, "--no-audit", "--no-fund", "--loglevel=error"], {
        cwd,
        encoding: "utf8",
        shell: isWindows,
        timeout: 300_000,
        maxBuffer: 32 * 1024 * 1024,
      });
      if (result.status !== 0) throw new Error(`install: ${label} npm ci failed (status ${result.status}): ${tail(result.stderr || result.stdout)}`);
    };
    ci(sidecar, ["--omit=dev", "--ignore-scripts"], "runtime tree");
    ci(join(sidecar, "bundler"), ["--ignore-scripts"], "bundler");

    const root = join(sidecar, "node_modules", pin.name);
    const manifest = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
    if (manifest.name !== pin.name || manifest.version !== pin.version) {
      throw new Error(`install: installed ${manifest.name}@${manifest.version}, expected ${pin.name}@${pin.version}`);
    }
    const npmHashes = expectedHashes(pin.layouts.npm, join(root), "npm");

    // The bundler is copied next to the locked esbuild it must use, then run from there.
    const bundlerCopy = join(sidecar, "bundler", "bundle.mjs");
    copyFileSync(bundlerSource, bundlerCopy);
    const { bundleReader } = await import(pathToFileURL(bundlerCopy).href);
    const pkg = join(prefix, "pkg");
    await bundleReader({ installRoot: sidecar, outDir: join(pkg, "dist") }).catch((error) => {
      throw new Error(`bundle: building the bundle failed: ${error.message}`);
    });
    copyFileSync(join(root, "package.json"), join(pkg, "package.json"));
    copyFileSync(join(root, "dist", "cli.js"), join(pkg, "dist", "cli.js"));
    const bundledHashes = expectedHashes(pin.layouts.bundled, pkg, "bundled");
    return { prefix, root, pkg, entry: join(pkg, "dist", "cli.js"), hashesChecked: npmHashes + bundledHashes };
  })();
  return installed;
}

let runner;
/** The reader program: the bundled Node.js for this host when one is pinned, else this test's own node. */
function readerProgram() {
  runner ??= (async () => {
    const platform = hostPlatform();
    if (!platform) {
      requireSupportedNode();
      return { program: process.execPath, label: `node ${process.version} (no pinned archive for ${process.platform}-${process.arch})` };
    }
    const runtime = readNodeRuntime();
    const archive = await ensureArchive(runtime, platform, join(tmpdir(), "mcao-node-archives")).catch((error) => {
      throw new Error(`reader: could not get the pinned Node.js ${runtime.version} archive: ${error.message}`);
    });
    const extractDir = mkdtempSync(join(tmpdir(), "mcao-node-extract-"));
    extracted.push(extractDir);
    const { binary, binarySha256 } = extractRuntime(runtime, platform, archive, extractDir);
    const version = spawnSync(binary, ["--version"], { encoding: "utf8" });
    if (version.status !== 0 || version.stdout.trim() !== `v${runtime.version}`) {
      throw new Error(`reader: bundled node reports ${version.stdout.trim() || version.stderr.trim()}, expected v${runtime.version}`);
    }
    return { program: binary, label: `bundled node ${runtime.version} ${platform} (sha256 ${binarySha256})` };
  })();
  return runner;
}

const extracted = [];
after(() => {
  for (const dir of extracted) rmSync(dir, { recursive: true, force: true });
});

function requireSupportedNode() {
  const minimum = parseVersion(pin.node_minimum);
  if (!atLeast(parseVersion(process.version), minimum)) {
    throw new Error(`reader: this Node.js ${process.version} is older than codeburn's minimum ${pin.node_minimum}`);
  }
}

test("the pin names the supported reader and a complete set of file hashes for both layouts", () => {
  assert.equal(pin.name, "codeburn");
  assert.match(pin.version, /^\d+\.\d+\.\d+$/);
  assert.ok(atLeast(parseVersion(pin.node_minimum), [22, 15, 0]), "codeburn needs Node.js 22.15 or newer");
  assert.match(pin.tarball_integrity, /^sha512-[A-Za-z0-9+/=]+$/);
  for (const layout of ["npm", "bundled"]) {
    for (const file of ["dist/cli.js", "dist/main.js", "dist/parse-worker.js"]) {
      assert.match(pin.layouts[layout][file] ?? "", /^[0-9a-f]{64}$/, `pinned ${layout} hash for ${file}`);
    }
  }
  assert.equal(pin.layouts.npm["dist/cli.js"], pin.layouts.bundled["dist/cli.js"], "the launcher is shipped unchanged");
});

test("the registry serves the pinned tarball, the locked install builds the pinned bundle, and both layouts match", async () => {
  const { entry, hashesChecked } = await install();
  assert.ok(existsSync(entry), "install: dist/cli.js is present");
  assert.equal(hashesChecked, 6, "install and bundle: every pinned file of both layouts was hashed");
});

test("the pinned reader reports the synthetic claude totals, with unpriced usage counted apart", async () => {
  const { entry } = await install();
  const { program, label } = await readerProgram();
  const home = mkdtempSync(join(tmpdir(), "mcao-codeburn-home-"));
  const cache = join(home, "cache");
  mkdirSync(cache, { recursive: true });
  try {
    writeSyntheticHome(home);
    const claude = runReport(program, entry, home, cache, "claude");
    assert.equal(claude.currency, "USD", "reader: the report is in USD");
    assert.match(claude.period, new RegExp(`\\(${todayUtc()}\\)`), "reader: the report covers today");
    assert.equal(claude.overview.calls, 2, "reader: two Claude calls");
    assert.equal(claude.overview.tokens.input, 1900, "reader: fresh input tokens summed");
    assert.equal(claude.overview.tokens.output, 370, "reader: output tokens summed");
    assert.equal(claude.overview.tokens.cacheRead, 5000, "reader: cache read tokens summed");
    assert.equal(claude.overview.tokens.cacheWrite, 800, "reader: cache write tokens summed");
    // Only the priced call contributes to the cost. The unpriced call is listed, not priced at zero.
    assert.ok(Math.abs(claude.overview.cost - 0.0126) < 1e-9, `reader: priced cost is 0.0126 (got ${claude.overview.cost})`);
    assert.equal(claude.unpricedModels.length, 1, "reader: one unpriced model");
    assert.equal(claude.unpricedModels[0].tokens, 770, "reader: its tokens are reported");
    assert.ok(label.length > 0);
  } finally {
    rmSync(home, { recursive: true, force: true });
  }
});

test("the pinned reader reports codex as an estimate and copilot with no calls on a synthetic home", async () => {
  const { entry } = await install();
  const { program } = await readerProgram();
  const home = mkdtempSync(join(tmpdir(), "mcao-codeburn-home-"));
  const cache = join(home, "cache");
  mkdirSync(cache, { recursive: true });
  try {
    writeSyntheticHome(home);
    const codex = runReport(program, entry, home, cache, "codex");
    assert.equal(codex.overview.calls, 1, "reader: one Codex call");
    assert.ok(codex.overview.cost > 0, "reader: the priced Codex call has an estimated cost");
    assert.equal(codex.unpricedModels.length, 0, "reader: the Codex model has a price");
    const copilot = runReport(program, entry, home, cache, "copilot");
    assert.equal(copilot.overview.calls, 0, "reader: no Copilot calls on a machine without Copilot data");
    assert.equal(copilot.overview.cost, 0, "reader: no calls means no cost figure");
  } finally {
    rmSync(home, { recursive: true, force: true });
  }
});

