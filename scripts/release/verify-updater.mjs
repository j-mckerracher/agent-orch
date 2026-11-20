// Checks the staged updater assets before anything is uploaded: latest.json is well formed and canonical, each
// payload has its .sig beside it, and `minisign` verifies each payload against the configured public key with a
// trusted comment that carries the manifest's version.
//
//   MCAO_UPDATER_PUBLIC_KEY=<base64 Tauri public key> node scripts/release/verify-updater.mjs <assets-dir>
//
// Tauri stores both the public key and the signature as base64 of a minisign file; they are decoded to a
// temporary directory outside the checkout. A key that does not match the one that signed only produces a warning
// in the Tauri CLI, so this is the check that stops the mismatch.
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { DOWNLOAD_BASE } from "./publish.mjs";

const PLATFORM_KEYS = ["darwin-aarch64", "windows-x86_64"];
const SEMVER = /^\d+\.\d+\.\d+$/;

const isObject = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
const hasKeys = (value, keys) => isObject(value) && JSON.stringify(Object.keys(value).sort()) === JSON.stringify([...keys].sort());

// Decodes a Tauri base64 wrapper to the minisign text it holds.
function decode(what, base64) {
  if (!/^[A-Za-z0-9+/]+={0,2}$/.test(base64)) throw new Error(`${what} is not base64`);
  const text = Buffer.from(base64, "base64").toString("utf8");
  if (!text.startsWith("untrusted comment:")) throw new Error(`${what} does not decode to a minisign file`);
  return text;
}

// `minisign` is the command and its leading arguments. Throws on the first problem.
export function verifyUpdater(assetsDir, publicKey, minisign = ["minisign"]) {
  const manifestPath = join(assetsDir, "latest.json");
  if (!existsSync(manifestPath)) throw new Error("latest.json is missing");
  let manifest;
  try {
    manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  } catch {
    throw new Error("latest.json is not valid JSON");
  }
  if (!hasKeys(manifest, ["version", "platforms"]) || typeof manifest.version !== "string" || !SEMVER.test(manifest.version)) {
    throw new Error("latest.json must be {version, platforms} with a MAJOR.MINOR.PATCH version");
  }
  if (!hasKeys(manifest.platforms, PLATFORM_KEYS)) throw new Error(`latest.json platforms must be exactly ${PLATFORM_KEYS.join(" and ")}`);

  const keyText = decode("the public key", publicKey.trim());
  const scratch = mkdtempSync(join(tmpdir(), "mcao-verify-updater-"));
  try {
    const keyFile = join(scratch, "key.pub");
    writeFileSync(keyFile, keyText);
    for (const platformKey of PLATFORM_KEYS) {
      const entry = manifest.platforms[platformKey];
      if (!hasKeys(entry, ["url", "signature"]) || typeof entry.url !== "string" || typeof entry.signature !== "string") {
        throw new Error(`${platformKey} must be {url, signature}`);
      }
      const prefix = `${DOWNLOAD_BASE}/v${manifest.version}/`;
      const file = entry.url.startsWith(prefix) ? entry.url.slice(prefix.length) : "";
      if (file === "" || basename(file) !== file) throw new Error(`${platformKey} url is not ${prefix}<asset-name>: ${entry.url}`);
      const payload = join(assetsDir, file);
      if (!existsSync(payload)) throw new Error(`${platformKey} payload ${file} is missing`);
      if (!existsSync(`${payload}.sig`)) throw new Error(`${file}.sig is missing`);
      if (readFileSync(`${payload}.sig`, "utf8").trim() !== entry.signature.trim()) throw new Error(`${file}.sig differs from the ${platformKey} signature in latest.json`);

      const sigFile = join(scratch, `${platformKey}.minisig`);
      writeFileSync(sigFile, decode(`the ${platformKey} signature`, entry.signature.trim()));
      const [command, ...lead] = minisign;
      const result = spawnSync(command, [...lead, "-V", "-Q", "-p", keyFile, "-m", payload, "-x", sigFile], { encoding: "utf8" });
      if (result.error) throw new Error(`could not run minisign: ${result.error.message}`);
      if (result.status !== 0) throw new Error(`${file} failed signature verification: ${result.stderr.trim() || result.stdout.trim()}`);
      if (!result.stdout.split(/\t|\r?\n/).includes(`version:${manifest.version}`)) {
        throw new Error(`${file} was signed for another version than ${manifest.version}; trusted comment: ${result.stdout.trim()}`);
      }
    }
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
  return manifest.version;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  try {
    const [dir] = process.argv.slice(2);
    if (!dir) throw new Error("usage: node scripts/release/verify-updater.mjs <assets-dir>");
    if (!process.env.MCAO_UPDATER_PUBLIC_KEY?.trim()) throw new Error("MCAO_UPDATER_PUBLIC_KEY is not set");
    const version = verifyUpdater(resolve(dir), process.env.MCAO_UPDATER_PUBLIC_KEY);
    console.log(`updater payloads for ${version} verify against the configured public key`);
  } catch (error) {
    console.error(`::error title=Updater verification::${error.message}`);
    process.exitCode = 1;
  }
}
