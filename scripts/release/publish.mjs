// Prepares one GitHub release from the platform builds of a `v*` tag: checks the tag against every
// version source, collects the installers, verifies each against the digest its build recorded, and
// writes the assets (installers and SHA256SUMS) and the release notes, which carry the version's section
// of docs/changelog.md. The release itself is created and published by the `Publish the GitHub release`
// step of .github/workflows/release.yml, the only step that holds the token.
//
//   node scripts/release/publish.mjs <tag> --downloads DIR --out DIR
//
// --downloads  the downloaded `release-*` artifacts: one directory per platform build, each with release-info.json.
// --out        a new directory: assets/ (installers and SHA256SUMS) and release-notes.md.
// MCAO_UPDATES_ENABLED=true (with MCAO_UPDATER_PUBLIC_KEY) also requires each build's `updater` record and adds the
// updater payloads, their signatures and latest.json to assets/; any other value of the variable must see no
// updater data. SHA256SUMS and the notes cover the two manual installers only.
// GITHUB_OUTPUT gets prerelease=true|false (a release with any unsigned installer is a prerelease) and
// updater=true|false.
import { createHash } from "node:crypto";
import { appendFileSync, copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { updaterDecision } from "./env.mjs";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const PLATFORMS = ["macos", "windows"];
const PLATFORM_NAMES = { macos: "macOS", windows: "Windows" };
// Tauri updater platform keys; the platform builds are macOS arm64 and Windows x64 only.
const UPDATER_KEYS = { macos: "darwin-aarch64", windows: "windows-x86_64" };
const UPDATER_PAYLOADS = {
  macos: (version, label) => `MCAO-${version}-macos-arm64-${label}.app.tar.gz`,
  windows: (version, label) => `MCAO-${version}-windows-x64-${label}-setup.exe`,
};
const RESERVED_NAMES = ["SHA256SUMS", "latest.json", "release-info.json"];
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
export const DOWNLOAD_BASE = "https://github.com/mc-innersource/motif-mcao/releases/download";
// The workarounds apply because the app is unsigned and internal. On macOS 15 and later, Control-click Open
// no longer bypasses Gatekeeper, so the Privacy & Security step is the one that works.
const UNSIGNED_WARNING = {
  macos: "**macOS (Gatekeeper):** the first launch is blocked. Try to open MCAO once, then open System Settings, Privacy & Security, and click Open Anyway. Or, after moving MCAO.app to /Applications, run `xattr -dr com.apple.quarantine /Applications/MCAO.app` in Terminal.",
  windows: "**Windows (SmartScreen):** Windows protected your PC. Choose More info, then Run anyway.",
};

// The version of one `[section]` of a TOML document. Only that table is searched, so a dependency's
// `version = ...` line is never read as the package version.
export function tomlSectionVersion(text, section) {
  const lines = text.split(/\r?\n/);
  const start = lines.findIndex((line) => line.trim() === `[${section}]`);
  if (start < 0) throw new Error(`no [${section}] table`);
  for (const line of lines.slice(start + 1)) {
    if (/^\s*\[/.test(line)) break;
    const match = /^version\s*=\s*"([^"]+)"/.exec(line);
    if (match) return match[1];
  }
  throw new Error(`no version in [${section}]`);
}

// `sources` is [{ file, version }]. Every source must equal the tag without its `v` prefix.
export function checkTagVersions(tag, sources) {
  const mismatched = sources.filter(({ version }) => `v${version}` !== tag);
  if (mismatched.length > 0) {
    const found = mismatched.map(({ file, version }) => `${file} = ${version}`).join("; ");
    throw new Error(`tag ${tag} must be v plus the version in every source; mismatched: ${found}`);
  }
}

const sameKeys = (value, keys) =>
  value !== null && typeof value === "object" && JSON.stringify(Object.keys(value).sort()) === JSON.stringify(keys);

// The validated updater record of one platform build, or null when updater releases are off. When on, the
// record must be complete and its names and digests must match the build's own manual installer.
function checkUpdater(build, version, enabled) {
  const { platform, updater } = build;
  if (!enabled) {
    if (updater !== undefined) throw new Error(`the ${platform} build carries updater data but MCAO_UPDATES_ENABLED is not true`);
    return null;
  }
  if (updater === undefined) throw new Error(`the ${platform} build has no updater record`);
  const bad = (why) => new Error(`the ${platform} updater record ${why}`);
  if (!sameKeys(updater, ["payload", "platformKey", "signature"])) throw bad("must have exactly platformKey, payload and signature");
  if (updater.platformKey !== UPDATER_KEYS[platform]) throw bad(`has platformKey ${updater.platformKey}, expected ${UPDATER_KEYS[platform]}`);
  const { payload, signature } = updater;
  for (const [what, entry] of [["payload", payload], ["signature", signature]]) {
    if (!sameKeys(entry, ["file", "sha256"])) throw bad(`${what} must have exactly file and sha256`);
    if (typeof entry.file !== "string" || basename(entry.file) !== entry.file || entry.file === "") {
      throw bad(`${what} name is not a bare file name: ${entry.file}`);
    }
    if (!SHA256_PATTERN.test(entry.sha256)) throw bad(`${what} sha256 is not a lowercase hex digest`);
  }
  const label = build.signed ? "signed" : "unsigned";
  const expected = UPDATER_PAYLOADS[platform](version, label);
  if (payload.file !== expected) throw bad(`payload is ${payload.file}, expected ${expected}`);
  if (signature.file !== `${payload.file}.sig`) throw bad(`signature is ${signature.file}, expected ${payload.file}.sig`);
  const [manual] = build.artifacts;
  if (platform === "windows" && (payload.file !== manual.file || payload.sha256 !== manual.sha256)) {
    throw bad("payload is not the manual installer");
  }
  return { platform, platformKey: updater.platformKey, payload, signature };
}

// `infos` are the parsed release-info.json documents of the platform builds. Returns the commit they were
// built from, one asset per platform in PLATFORMS order, the updater records (null when `updaterEnabled` is
// false) and `files`: every file to publish except SHA256SUMS and latest.json, as { platform, file, sha256 }.
// Throws unless the set is exactly one macOS and one Windows build of `version`, from one commit, each with one
// installer whose name is a bare file name, and (when enabled) a complete updater pair for it.
export function releaseAssets(infos, version, updaterEnabled = false) {
  if (infos.length !== PLATFORMS.length) throw new Error(`expected ${PLATFORMS.length} platform builds, found ${infos.length}`);
  const builds = PLATFORMS.map((platform) => {
    const matches = infos.filter((info) => info.platform === platform);
    if (matches.length !== 1) throw new Error(`expected exactly one ${platform} build, found ${matches.length}`);
    return matches[0];
  });
  for (const build of builds) {
    if (build.version !== version) throw new Error(`the ${build.platform} build is version ${build.version}, not ${version}`);
    if (build.artifacts.length !== 1) throw new Error(`the ${build.platform} build has ${build.artifacts.length} installers, expected 1`);
    const { file } = build.artifacts[0];
    if (basename(file) !== file) throw new Error(`the ${build.platform} installer name is not a bare file name: ${file}`);
  }
  const commits = new Set(builds.map((build) => build.commit));
  if (commits.size !== 1) throw new Error(`the platform builds come from different commits: ${[...commits].join(", ")}`);
  const updaters = builds.map((build) => checkUpdater(build, version, updaterEnabled));
  const files = builds.flatMap((build, index) => {
    const { file, sha256 } = build.artifacts[0];
    const updater = updaters[index];
    const list = [{ platform: build.platform, file, sha256 }];
    if (updater) {
      if (updater.payload.file !== file) list.push({ platform: build.platform, ...updater.payload });
      list.push({ platform: build.platform, ...updater.signature });
    }
    return list;
  });
  const names = files.map((entry) => entry.file);
  const clash = names.find((name, index) => RESERVED_NAMES.includes(name) || names.indexOf(name) !== index);
  if (clash) throw new Error(`conflicting release asset name: ${clash}`);
  return {
    commit: builds[0].commit,
    assets: builds.map((build) => ({
      platform: build.platform,
      file: build.artifacts[0].file,
      sha256: build.artifacts[0].sha256,
      signed: build.signed,
      notarized: build.notarized,
    })),
    updaters: updaterEnabled ? updaters : null,
    files,
  };
}

// The Tauri updater manifest. `entries` are { platformKey, file, signature } with `signature` the .sig file's
// contents. No notes or date: the app shows neither.
export function updaterManifest(version, entries) {
  const platforms = {};
  for (const { platformKey, file, signature } of entries) {
    const trimmed = signature.trim();
    if (!/^[A-Za-z0-9+/]+={0,2}$/.test(trimmed)) throw new Error(`the ${platformKey} signature is not a single base64 line`);
    platforms[platformKey] = { url: `${DOWNLOAD_BASE}/v${version}/${file}`, signature: trimmed };
  }
  return { version, platforms };
}

export const isPrerelease = (assets) => assets.some((asset) => !asset.signed);

// The `sha256sum -c` format: "<digest>  <file>" per line.
export const sha256Sums = (assets) => assets.map((asset) => `${asset.sha256}  ${asset.file}\n`).join("");

// The body of the `## ` section of docs/changelog.md for `version`, without its heading. release-please
// writes `## [X.Y.Z](link) (date)`, and a hand-written section reads `## vX.Y.Z`; `(?![\d.])` keeps 0.1.1
// from matching 0.1.10. Throws when there is no section: a release has to say what changed.
export function changelogSection(text, version) {
  const lines = text.split(/\r?\n/);
  const escaped = version.replace(/\./g, "\\.");
  const heading = new RegExp(`^## \\[?v?${escaped}(?![\\d.])`);
  const start = lines.findIndex((line) => heading.test(line));
  if (start < 0) throw new Error(`docs/changelog.md has no section for ${version}`);
  const rest = lines.slice(start + 1);
  const next = rest.findIndex((line) => line.startsWith("## "));
  return (next < 0 ? rest : rest.slice(0, next)).join("\n").trim();
}

const signingLabel = (asset) => {
  if (!asset.signed) return "UNSIGNED";
  if (asset.platform === "macos") return asset.notarized ? "Signed and notarized" : "Signed, not notarized";
  return "Signed";
};

// The notes name only the unsigned platforms in the warning, so a signed platform never reads as unsigned.
// `changelog` is the version's changelog section, placed under the warning and above the installers.
export function releaseNotes({ tag, commit, assets, changelog }) {
  const unsigned = assets.filter((asset) => !asset.signed);
  const lines = [];
  if (unsigned.length > 0) {
    lines.push(
      "> **Unsigned build: this release is a prerelease.** Its installers are not code-signed or notarized. That is expected: the app is unsigned and internal.",
    );
    for (const asset of unsigned) lines.push(">", `> ${UNSIGNED_WARNING[asset.platform]}`);
    lines.push("");
  }
  if (changelog) lines.push("## Changes", "", changelog, "");
  lines.push(
    "## Installers",
    "",
    "| Platform | File | Signing |",
    "| --- | --- | --- |",
    ...assets.map((asset) => `| ${PLATFORM_NAMES[asset.platform]} | \`${asset.file}\` | ${signingLabel(asset)} |`),
    "",
    `Built from commit \`${commit}\` for tag \`${tag}\`. Digests are in \`SHA256SUMS\`.`,
    "",
    "## Install",
    "",
    "- macOS: unzip the archive and move `MCAO.app` to `/Applications`.",
    "- Windows: run the setup `.exe` and follow the installer.",
    "",
    "To check a download, run `shasum -a 256 <file>` (macOS) or `Get-FileHash <file>` (Windows) and compare the digest with its line in `SHA256SUMS`.",
    "",
  );
  return lines.join("\n");
}

function parseArgs(argv) {
  const [tag, ...rest] = argv;
  const value = (name) => {
    const index = rest.indexOf(name);
    return index >= 0 ? rest[index + 1] : undefined;
  };
  const downloads = value("--downloads");
  const out = value("--out");
  if (!tag || !downloads || !out) throw new Error("usage: node scripts/release/publish.mjs <tag> --downloads DIR --out DIR");
  return { tag, downloads: resolve(downloads), out: resolve(out) };
}

// Every version source of the app, read from the working tree the tag was checked out to.
function readVersions() {
  const desktop = join(repo, "apps", "desktop");
  const jsonVersion = (path) => JSON.parse(readFileSync(path, "utf8")).version;
  const tomlVersion = (path, section) => tomlSectionVersion(readFileSync(path, "utf8"), section);
  return [
    { file: "apps/desktop/src-tauri/tauri.conf.json", version: jsonVersion(join(desktop, "src-tauri", "tauri.conf.json")) },
    { file: "apps/desktop/package.json", version: jsonVersion(join(desktop, "package.json")) },
    { file: "Cargo.toml", version: tomlVersion(join(repo, "Cargo.toml"), "workspace.package") },
    { file: "apps/desktop/src-tauri/Cargo.toml", version: tomlVersion(join(desktop, "src-tauri", "Cargo.toml"), "package") },
  ];
}

const sha256 = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");

function readBuilds(downloads) {
  return readdirSync(downloads, { recursive: true })
    .filter((path) => basename(path) === "release-info.json")
    .map((path) => {
      const file = join(downloads, path);
      return { dir: dirname(file), info: JSON.parse(readFileSync(file, "utf8")) };
    });
}

// Writes `out/assets` and `out/release-notes.md` for `tag` and returns the assets and the prerelease flag. Throws,
// leaving nothing to publish, on any mismatch between the tag, the version sources and the builds.
// `sources` are the version sources, `changelogText` is docs/changelog.md and `updater` is the updaterDecision.
export function assemble({ tag, downloads, out, sources, changelogText, updater }) {
  if (existsSync(out)) throw new Error(`output directory exists, refusing to reuse it: ${out}`);
  checkTagVersions(tag, sources);
  const builds = readBuilds(downloads);
  // The check above makes every source equal the tag, so the first one is the app version.
  const version = sources[0].version;
  const { commit, assets, updaters, files } = releaseAssets(builds.map(({ info }) => info), version, updater.enabled);

  const assetsDir = join(out, "assets");
  mkdirSync(assetsDir, { recursive: true });
  for (const { platform, file, sha256: recorded } of files) {
    const path = join(builds.find(({ info }) => info.platform === platform).dir, file);
    if (!existsSync(path)) throw new Error(`${file} is missing from the ${platform} build`);
    if (sha256(path) !== recorded) throw new Error(`${file} does not match the sha256 its build recorded`);
    copyFileSync(path, join(assetsDir, file));
  }
  writeFileSync(join(assetsDir, "SHA256SUMS"), sha256Sums(assets));
  if (updaters) {
    const entries = updaters.map(({ platformKey, payload, signature }) => ({
      platformKey, file: payload.file, signature: readFileSync(join(assetsDir, signature.file), "utf8"),
    }));
    writeFileSync(join(assetsDir, "latest.json"), `${JSON.stringify(updaterManifest(version, entries), null, 2)}\n`);
  }
  const changelog = changelogSection(changelogText, version);
  writeFileSync(join(out, "release-notes.md"), releaseNotes({ tag, commit, assets, changelog }));
  return { assets, prerelease: isPrerelease(assets) };
}

function publish(argv) {
  const { tag, downloads, out } = parseArgs(argv);
  const updater = updaterDecision(process.env, "publish");
  const { assets, prerelease } = assemble({
    tag, downloads, out, updater,
    sources: readVersions(),
    changelogText: readFileSync(join(repo, "docs", "changelog.md"), "utf8"),
  });
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `prerelease=${prerelease}\nupdater=${updater.enabled}\n`);
  const listing = assets.map((asset) => `${asset.file} (${asset.platform}, ${asset.signed ? "signed" : "UNSIGNED"})`);
  console.log(`${tag}: ${listing.join("; ")}; prerelease=${prerelease}; updater=${updater.enabled}`);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  try {
    publish(process.argv.slice(2));
  } catch (error) {
    console.error(`::error title=Release publish::${error.message}`);
    process.exitCode = 1;
  }
}
