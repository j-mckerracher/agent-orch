// Stages the pinned treehouse worktree tool (github.com/kunchenguid/treehouse, MIT) for the release bundle, or
// into a directory for CI and tests. Run from apps/desktop by scripts/package.mjs, or from the repository root
// with --dest.
//
// Default: src-tauri/binaries/treehouse-<triple>[.exe], where Tauri's externalBin expects it. Tauri then places
// it beside the daemon: Contents/MacOS in the app, the install folder on Windows. --dest DIR stages a plain
// treehouse[.exe] into DIR instead.
//
// Fails closed: the archive's SHA-256 and the extracted binary's SHA-256 must match sidecars/treehouse/treehouse-pin.json,
// and the binary must report the pinned version, before anything is staged. Upstream publishes checksums but no
// signature, so this pin is the only check that these bytes are the upstream release. Tauri re-signs the staged file
// when it bundles it, so the pin is enforced here, at staging, and not again later.
import { spawnSync } from "node:child_process";
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { ensureArchive, extractRuntime, hostPlatform, runtimeEntry } from "../src-tauri/sidecars/codeburn/lib.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const desktop = resolve(here, "..");
const pinPath = join(desktop, "src-tauri", "sidecars", "treehouse", "treehouse-pin.json");
const win = process.platform === "win32";
const exe = win ? ".exe" : "";

const fail = (message) => {
  throw new Error(`prepare-treehouse-sidecar: ${message}`);
};

const destFlag = process.argv.indexOf("--dest");
const destDir = destFlag === -1 ? join(desktop, "src-tauri", "binaries") : resolve(process.argv[destFlag + 1] ?? fail("--dest needs a directory"));

const platform = hostPlatform();
if (!platform) fail(`no pinned treehouse for ${process.platform}-${process.arch}; packaging supports darwin-arm64 and win-x64`);
const pin = JSON.parse(readFileSync(pinPath, "utf8"));
const entry = runtimeEntry(pin, platform);
const stagedName = destFlag === -1 ? `treehouse-${entry.triple}${exe}` : `treehouse${exe}`;
const staged = join(destDir, stagedName);

const archive = await ensureArchive(pin, platform, join(tmpdir(), "mcao-treehouse-archives"));
const extractDir = mkdtempSync(join(tmpdir(), "mcao-treehouse-extract-"));
try {
  const { binary } = extractRuntime(pin, platform, archive, extractDir);
  const version = spawnSync(binary, ["--version"], { encoding: "utf8", env: { ...process.env, TREEHOUSE_NO_UPDATE_CHECK: "1" } });
  if (version.status !== 0 || version.stdout.trim() !== `v${pin.version}`) {
    fail(`staged treehouse reports ${version.stdout.trim() || version.stderr.trim()}, expected v${pin.version}`);
  }
  mkdirSync(destDir, { recursive: true });
  copyFileSync(binary, staged);
  chmodSync(staged, 0o755);
} finally {
  // Windows can still hold treehouse.exe after `--version` (EBUSY); Node retries the removal.
  rmSync(extractDir, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
}

console.log(`staged ${staged}`);
console.log(`  treehouse ${pin.version} ${platform}: archive ${entry.archiveSha256}`);
console.log(`  binary sha256 ${entry.binarySha256}`);
