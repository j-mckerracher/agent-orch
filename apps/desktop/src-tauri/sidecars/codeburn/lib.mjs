// Shared, dependency-free helpers for the pinned codeburn reader sidecar (DPES-1105).
// Used by the packaging script (apps/desktop/scripts/prepare-codeburn-sidecar.mjs), the smoke
// test (scripts/smoke/usage-reader.smoke.mjs) and the record test (codeburn-sidecar.test.mjs).
// Pure apart from reading files; nothing here installs or downloads anything.
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const sidecarDir = dirname(fileURLToPath(import.meta.url));
export const repoRoot = resolve(sidecarDir, "..", "..", "..", "..", "..");

const readJson = (path) => JSON.parse(readFileSync(path, "utf8"));

/** crates/orchestrator-local-usage/codeburn-pin.json: the reader the daemon accepts. */
export function readPin() {
  return readJson(join(repoRoot, "crates", "orchestrator-local-usage", "codeburn-pin.json"));
}

/** node-runtime.json: the Node.js release that is bundled, and the pinned archive hashes. */
export function readNodeRuntime() {
  return readJson(join(sidecarDir, "node-runtime.json"));
}

export function readLockfile(path = join(sidecarDir, "package-lock.json")) {
  return readJson(path);
}

export function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

export function sha256OfFile(path) {
  return sha256(readFileSync(path));
}

/**
 * SHA-256 of a file, without its first line when that line is a `#!` interpreter line. Package
 * managers rewrite that line (Homebrew does), so the daemon pin ignores it for dist/cli.js.
 */
export function pinnedFileHash(path, stripShebang) {
  const bytes = readFileSync(path);
  if (stripShebang && bytes.subarray(0, 2).toString() === "#!") {
    const newline = bytes.indexOf(0x0a);
    return sha256(newline < 0 ? Buffer.alloc(0) : bytes.subarray(newline + 1));
  }
  return sha256(bytes);
}

/** The lockfile entry for an installed top-level package, e.g. "codeburn". */
export function lockEntry(lock, name) {
  return lock.packages?.[`node_modules/${name}`] ?? null;
}

/**
 * Problems with a lockfile that would weaken the pin: every package must carry a sha512
 * integrity value and be fetched from the public npm registry. Returns [] when clean.
 */
export function lockfileProblems(lock) {
  const problems = [];
  for (const [key, entry] of Object.entries(lock.packages ?? {})) {
    if (key === "") continue;
    if (!/^sha512-[A-Za-z0-9+/]+=*$/.test(entry.integrity ?? "")) problems.push(`${key}: no sha512 integrity`);
    if (!entry.resolved?.startsWith("https://registry.npmjs.org/")) problems.push(`${key}: resolved from ${entry.resolved ?? "nowhere"}`);
  }
  return problems;
}

/** `sha256  filename` lines (a SHASUMS256.txt file) to a Map of filename to digest. */
export function parseShasums(text) {
  const map = new Map();
  for (const line of text.split("\n")) {
    const match = /^([0-9a-f]{64})\s+\*?(\S+)$/.exec(line.trim());
    if (match) map.set(match[2], match[1]);
  }
  return map;
}

/** The bundled Node.js platform for this host, or null when no archive is pinned for it. */
export function hostPlatform(platform = process.platform, arch = process.arch) {
  if (platform === "darwin" && arch === "arm64") return "darwin-arm64";
  if (platform === "win32" && arch === "x64") return "win-x64";
  return null;
}

/** The pinned Node.js entry for a platform, with its archive name and hashes. */
export function runtimeEntry(runtime, platform) {
  const entry = runtime.platforms[platform];
  if (!entry) throw new Error(`no pinned Node.js ${runtime.version} archive for ${platform}`);
  return entry;
}

/** Problems with the pinned archive hashes against the committed SHASUMS256.txt. Returns [] when clean. */
export function runtimeProblems(runtime, shasumsText) {
  const shasums = parseShasums(shasumsText);
  const problems = [];
  for (const [platform, entry] of Object.entries(runtime.platforms)) {
    const official = shasums.get(entry.archive);
    if (!official) problems.push(`${platform}: ${entry.archive} is not listed in ${runtime.shasums}`);
    else if (official !== entry.archiveSha256) {
      problems.push(`${platform}: pinned ${entry.archiveSha256} but ${runtime.shasums} lists ${official}`);
    }
  }
  return problems;
}

/**
 * Path of the pinned Node.js archive in `cacheDir`, downloading it from nodejs.org when absent or
 * when its SHA-256 differs from the pin. A file that does not match is never used.
 */
export async function ensureArchive(runtime, platform, cacheDir) {
  const entry = runtimeEntry(runtime, platform);
  const path = join(cacheDir, entry.archive);
  mkdirSync(cacheDir, { recursive: true });
  if (existsSync(path) && sha256OfFile(path) === entry.archiveSha256) return path;
  rmSync(path, { force: true });
  const response = await fetch(`${runtime.distBase}${entry.archive}`);
  if (!response.ok) throw new Error(`download of ${entry.archive} failed: HTTP ${response.status}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  const actual = sha256(bytes);
  if (actual !== entry.archiveSha256) {
    throw new Error(`${entry.archive} has SHA-256 ${actual}, expected ${entry.archiveSha256}; not used`);
  }
  writeFileSync(path, bytes);
  return path;
}

/**
 * The tar that reads both the .tar.gz (macOS) and the .zip (Windows) archives. On Windows a bare
 * `tar` can be Git's GNU tar, which reads `C:` as a remote host and cannot read zip. The system
 * bsdtar is in System32 on Windows 10 and later.
 */
export function tarProgram(platform = process.platform) {
  if (platform !== "win32") return "tar";
  const bsdtar = join(process.env.SYSTEMROOT ?? "C:\\Windows", "System32", "tar.exe");
  return existsSync(bsdtar) ? bsdtar : "tar";
}

/**
 * Extracts only the pinned `node` binary and its LICENSE from the archive into `outDir`, then checks
 * the binary's SHA-256 against the pin.
 */
export function extractRuntime(runtime, platform, archivePath, outDir) {
  const entry = runtimeEntry(runtime, platform);
  mkdirSync(outDir, { recursive: true });
  const result = spawnSync(tarProgram(), ["-xf", archivePath, "-C", outDir, entry.member, entry.licenseMember], { encoding: "utf8" });
  if (result.error || result.status !== 0) {
    throw new Error(`tar could not extract ${entry.member}: ${result.error ?? result.stderr}`);
  }
  const binary = join(outDir, entry.member);
  const actual = sha256OfFile(binary);
  if (actual !== entry.binarySha256) {
    throw new Error(`extracted ${entry.member} has SHA-256 ${actual}, expected ${entry.binarySha256}`);
  }
  return { binary, license: join(outDir, entry.licenseMember), binarySha256: actual };
}

/** The environment the reader runs in, as the daemon sets it (allowlist, offline settings). */
export function readerEnv(home, cacheDir) {
  return {
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
}

/**
 * Runs `<program> <cli.js> --timezone UTC report --format json -p today --provider <provider>` on a
 * synthetic home and returns the parsed report. Throws with the step named on any failure.
 */
export function runReport(program, cliJs, home, cacheDir, provider) {
  const result = spawnSync(program, [cliJs, "--timezone", "UTC", "report", "--format", "json", "-p", "today", "--provider", provider], {
    env: readerEnv(home, cacheDir),
    encoding: "utf8",
    timeout: 120_000,
    maxBuffer: 32 * 1024 * 1024,
  });
  if (result.error) throw new Error(`reader: could not start ${program} for ${provider}: ${result.error.code ?? result.error.message}`);
  if (result.status !== 0) throw new Error(`reader: codeburn ${provider} exited with status ${result.status}: ${String(result.stderr).slice(-400)}`);
  try {
    return JSON.parse(result.stdout);
  } catch {
    throw new Error(`reader: codeburn ${provider} printed output that is not JSON`);
  }
}
