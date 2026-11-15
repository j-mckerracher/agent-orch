// Smoke test for the pinned local-usage reader (DPES-1105), run by the
// "Harness self-tests" step on every OS, Windows included.
//
// It installs the exact codeburn version in crates/orchestrator-local-usage/codeburn-pin.json
// into a temporary folder with scripts disabled, checks the registry's tarball
// integrity and the installed files against the pin, then runs the real reader
// against a SYNTHETIC home folder (no real user data is read) and asserts the
// totals, the estimate labels and the unpriced-model handling.
//
// Every failure names its step: "registry", "install" or "reader". A test that
// cannot run fails; nothing here is skipped, because a skipped test would
// still be counted as a pass by the gate's expected totals.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const pinPath = resolve(here, "..", "crates", "orchestrator-local-usage", "codeburn-pin.json");
const pin = JSON.parse(readFileSync(pinPath, "utf8"));
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

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

/** SHA-256 of a file, without its first line when that line is a #! interpreter line. */
function pinnedHash(path, stripShebang) {
  const bytes = readFileSync(path);
  if (stripShebang && bytes.subarray(0, 2).toString() === "#!") {
    const newline = bytes.indexOf(0x0a);
    return sha256(newline < 0 ? Buffer.alloc(0) : bytes.subarray(newline + 1));
  }
  return sha256(bytes);
}

/** Runs npm with fixed arguments. On Windows npm is a .cmd shim, which needs a shell. */
function runNpm(args, options) {
  return spawnSync(isWindows ? "npm.cmd" : "npm", args, { ...options, encoding: "utf8", shell: isWindows });
}

function tail(text, limit = 400) {
  const clean = String(text ?? "").trim();
  return clean.length > limit ? `...${clean.slice(-limit)}` : clean;
}

function todayUtc() {
  return new Date().toISOString().slice(0, 10);
}

let installed;
/** Installs the pinned reader once for every test in this file. */
function install() {
  installed ??= (() => {
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
    const result = runNpm(
      ["install", `${pin.name}@${pin.version}`, "--prefix", prefix, "--ignore-scripts", "--no-audit", "--no-fund", "--loglevel=error"],
      { timeout: 300_000, maxBuffer: 32 * 1024 * 1024 },
    );
    if (result.status !== 0) {
      throw new Error(`install: npm install failed (status ${result.status}): ${tail(result.stderr || result.stdout)}`);
    }
    const root = join(prefix, "node_modules", pin.name);
    const manifest = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
    if (manifest.name !== pin.name || manifest.version !== pin.version) {
      throw new Error(`install: installed ${manifest.name}@${manifest.version}, expected ${pin.name}@${pin.version}`);
    }
    for (const [relative, expected] of Object.entries(pin.files_sha256)) {
      const actual = pinnedHash(join(root, relative), relative === "dist/cli.js");
      if (actual !== expected) {
        throw new Error(`install: installed ${relative} does not match the pinned SHA-256 (${actual} vs ${expected})`);
      }
    }
    return { prefix, root, entry: join(root, "dist", "cli.js"), hashesChecked: Object.keys(pin.files_sha256).length };
  })();
  return installed;
}

/**
 * Synthetic Claude Code and Codex session files stamped for today (UTC). The reader drops
 * entries stamped after the current time, so the stamp is a second before now, but never
 * before the start of the UTC day.
 */
function writeSyntheticHome(home) {
  const startOfDay = Date.parse(`${todayUtc()}T00:00:00.000Z`);
  const noon = new Date(Math.max(Date.now() - 1000, startOfDay + 1000)).toISOString();
  const claudeDir = join(home, ".claude", "projects", "-synthetic-demo");
  mkdirSync(claudeDir, { recursive: true });
  const claudeLines = [
    {
      type: "assistant",
      timestamp: noon,
      sessionId: "sess-claude-1",
      cwd: "/synthetic/demo",
      requestId: "req_1",
      uuid: "u1",
      message: {
        id: "msg_1",
        model: "claude-sonnet-4-5-20250929",
        role: "assistant",
        content: [{ type: "text", text: "synthetic" }],
        usage: { input_tokens: 1200, output_tokens: 300, cache_read_input_tokens: 5000, cache_creation_input_tokens: 800 },
      },
    },
    {
      type: "assistant",
      timestamp: noon,
      sessionId: "sess-claude-2",
      cwd: "/synthetic/demo",
      requestId: "req_2",
      uuid: "u2",
      message: {
        id: "msg_2",
        model: "synthetic-unpriced-model-9",
        role: "assistant",
        content: [{ type: "text", text: "synthetic" }],
        usage: { input_tokens: 700, output_tokens: 70, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 },
      },
    },
  ];
  writeFileSync(join(claudeDir, "sess-claude-1.jsonl"), claudeLines.map((line) => JSON.stringify(line)).join("\n") + "\n");

  const [year, month, day] = todayUtc().split("-");
  const codexDir = join(home, ".codex", "sessions", year, month, day);
  mkdirSync(codexDir, { recursive: true });
  const codexLines = [
    { timestamp: noon, type: "session_meta", payload: { id: "synth-codex-1", cwd: "/synthetic/demo" } },
    { timestamp: noon, type: "turn_context", payload: { model: "gpt-5-codex" } },
    {
      timestamp: noon,
      type: "event_msg",
      payload: {
        type: "token_count",
        info: {
          total_token_usage: { input_tokens: 1000, cached_input_tokens: 400, output_tokens: 50, reasoning_output_tokens: 20, total_tokens: 1050 },
          last_token_usage: { input_tokens: 1000, cached_input_tokens: 400, output_tokens: 50, reasoning_output_tokens: 20, total_tokens: 1050 },
        },
      },
    },
  ];
  writeFileSync(join(codexDir, "rollout-synthetic.jsonl"), codexLines.map((line) => JSON.stringify(line)).join("\n") + "\n");
}

/** Runs the reader for one provider on the synthetic home. */
function readReport(entry, home, cacheDir, provider) {
  const env = {
    PATH: process.env.PATH ?? "",
    HOME: home,
    USERPROFILE: home,
    APPDATA: join(home, "AppData", "Roaming"),
    LOCALAPPDATA: join(home, "AppData", "Local"),
    SYSTEMROOT: process.env.SYSTEMROOT ?? "",
    TEMP: process.env.TEMP ?? tmpdir(),
    TMP: process.env.TMP ?? tmpdir(),
    TMPDIR: tmpdir(),
    CODEBURN_PRICING_SNAPSHOT_ONLY: "1",
    CODEBURN_FX_NO_FETCH: "1",
    CODEBURN_CACHE_DIR: cacheDir,
  };
  const result = spawnSync(process.execPath, [entry, "--timezone", "UTC", "report", "--format", "json", "-p", "today", "--provider", provider], {
    env,
    encoding: "utf8",
    timeout: 120_000,
    maxBuffer: 32 * 1024 * 1024,
  });
  if (result.error) throw new Error(`reader: could not start codeburn for ${provider}: ${result.error.code ?? result.error.message}`);
  if (result.status !== 0) {
    throw new Error(`reader: codeburn ${provider} exited with status ${result.status}: ${tail(result.stderr)}`);
  }
  try {
    return JSON.parse(result.stdout);
  } catch {
    throw new Error(`reader: codeburn ${provider} printed output that is not JSON: ${tail(result.stdout, 200)}`);
  }
}

test("the pin names the supported reader and a complete set of file hashes", () => {
  assert.equal(pin.name, "codeburn");
  assert.match(pin.version, /^\d+\.\d+\.\d+$/);
  assert.ok(atLeast(parseVersion(pin.node_minimum), [22, 15, 0]), "codeburn needs Node.js 22.15 or newer");
  assert.match(pin.tarball_integrity, /^sha512-[A-Za-z0-9+/=]+$/);
  for (const file of ["dist/cli.js", "dist/main.js", "dist/parse-worker.js"]) {
    assert.match(pin.files_sha256[file] ?? "", /^[0-9a-f]{64}$/, `pinned hash for ${file}`);
  }
});

test("the registry serves the pinned tarball and the install matches the pinned files", () => {
  const { entry, hashesChecked } = install();
  assert.ok(existsSync(entry), "install: dist/cli.js is present");
  assert.equal(hashesChecked, 3, "install: every pinned file was hashed");
});

function requireSupportedNode() {
  const minimum = parseVersion(pin.node_minimum);
  if (!atLeast(parseVersion(process.version), minimum)) {
    throw new Error(`reader: this Node.js ${process.version} is older than codeburn's minimum ${pin.node_minimum}`);
  }
}

test("the pinned reader reports the synthetic claude totals, with unpriced usage counted apart", () => {
  requireSupportedNode();
  const { entry } = install();
  const home = mkdtempSync(join(tmpdir(), "mcao-codeburn-home-"));
  const cache = join(home, "cache");
  mkdirSync(cache, { recursive: true });
  try {
    writeSyntheticHome(home);
    const claude = readReport(entry, home, cache, "claude");
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
  } finally {
    rmSync(home, { recursive: true, force: true });
  }
});

test("the pinned reader reports codex as an estimate and copilot with no calls on a synthetic home", () => {
  requireSupportedNode();
  const { entry } = install();
  const home = mkdtempSync(join(tmpdir(), "mcao-codeburn-home-"));
  const cache = join(home, "cache");
  mkdirSync(cache, { recursive: true });
  try {
    writeSyntheticHome(home);
    const codex = readReport(entry, home, cache, "codex");
    assert.equal(codex.overview.calls, 1, "reader: one Codex call");
    assert.ok(codex.overview.cost > 0, "reader: the priced Codex call has an estimated cost");
    assert.equal(codex.unpricedModels.length, 0, "reader: the Codex model has a price");
    const copilot = readReport(entry, home, cache, "copilot");
    assert.equal(copilot.overview.calls, 0, "reader: no Copilot calls on a machine without Copilot data");
    assert.equal(copilot.overview.cost, 0, "reader: no calls means no cost figure");
  } finally {
    rmSync(home, { recursive: true, force: true });
  }
});
