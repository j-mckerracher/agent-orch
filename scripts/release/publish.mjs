// Prepares one GitHub release from the platform builds of a `v*` tag: checks the tag against every
// version source, collects the installers, verifies each against the digest its build recorded, and
// writes the assets (installers and SHA256SUMS) and the release notes. The release itself is created by
// the `gh release create` step of .github/workflows/release.yml, the only step that holds the token.
//
//   node scripts/release/publish.mjs <tag> --downloads DIR --out DIR
//
// --downloads  the downloaded `release-*` artifacts: one directory per platform build, each with release-info.json.
// --out        a new directory: assets/ (installers and SHA256SUMS) and release-notes.md.
// GITHUB_OUTPUT gets prerelease=true|false: a release with any unsigned installer is a prerelease.
import { createHash } from "node:crypto";
import { appendFileSync, copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const PLATFORMS = ["macos", "windows"];
const PLATFORM_NAMES = { macos: "macOS", windows: "Windows" };
const UNSIGNED_WARNING = {
  macos: "**macOS:** Gatekeeper will block or warn on the first launch of an unsigned app. Allow it under System Settings, Privacy & Security only if you trust this build.",
  windows: "**Windows:** SmartScreen will warn that Windows protected your PC. Choose More info, then Run anyway, only if you trust this build.",
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

// `infos` are the parsed release-info.json documents of the platform builds. Returns the commit they were
// built from and one asset per platform, in PLATFORMS order. Throws unless the set is exactly one macOS and
// one Windows build of `version`, from one commit, each with one installer whose name is a bare file name.
export function releaseAssets(infos, version) {
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
  return {
    commit: builds[0].commit,
    assets: builds.map((build) => ({
      platform: build.platform,
      file: build.artifacts[0].file,
      sha256: build.artifacts[0].sha256,
      signed: build.signed,
      notarized: build.notarized,
    })),
  };
}

export const isPrerelease = (assets) => assets.some((asset) => !asset.signed);

// The `sha256sum -c` format: "<digest>  <file>" per line.
export const sha256Sums = (assets) => assets.map((asset) => `${asset.sha256}  ${asset.file}\n`).join("");

const signingLabel = (asset) => {
  if (!asset.signed) return "UNSIGNED";
  if (asset.platform === "macos") return asset.notarized ? "Signed and notarized" : "Signed, not notarized";
  return "Signed";
};

// The notes name only the unsigned platforms in the warning, so a signed platform never reads as unsigned.
export function releaseNotes({ tag, commit, assets }) {
  const unsigned = assets.filter((asset) => !asset.signed);
  const lines = [];
  if (unsigned.length > 0) {
    lines.push("> **Unsigned build: this release is a prerelease.** Its installers are not code-signed.");
    for (const asset of unsigned) lines.push(">", `> ${UNSIGNED_WARNING[asset.platform]}`);
    lines.push("");
  }
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

function publish(argv) {
  const { tag, downloads, out } = parseArgs(argv);
  if (existsSync(out)) throw new Error(`output directory exists, refusing to reuse it: ${out}`);
  const sources = readVersions();
  checkTagVersions(tag, sources);
  const builds = readBuilds(downloads);
  // The check above makes every source equal the tag, so the first one is the app version.
  const { commit, assets } = releaseAssets(builds.map(({ info }) => info), sources[0].version);

  const assetsDir = join(out, "assets");
  mkdirSync(assetsDir, { recursive: true });
  for (const { dir, info } of builds) {
    for (const artifact of info.artifacts) {
      const path = join(dir, artifact.file);
      if (sha256(path) !== artifact.sha256) throw new Error(`${artifact.file} does not match the sha256 its build recorded`);
      copyFileSync(path, join(assetsDir, artifact.file));
    }
  }
  writeFileSync(join(assetsDir, "SHA256SUMS"), sha256Sums(assets));
  writeFileSync(join(out, "release-notes.md"), releaseNotes({ tag, commit, assets }));

  const prerelease = isPrerelease(assets);
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `prerelease=${prerelease}\n`);
  const listing = assets.map((asset) => `${asset.file} (${asset.platform}, ${asset.signed ? "signed" : "UNSIGNED"})`);
  console.log(`${tag}: ${listing.join("; ")}; prerelease=${prerelease}`);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  try {
    publish(process.argv.slice(2));
  } catch (error) {
    console.error(`::error title=Release publish::${error.message}`);
    process.exitCode = 1;
  }
}
