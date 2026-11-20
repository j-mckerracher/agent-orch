import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";
import {
  assemble, changelogSection, checkTagVersions, isPrerelease, releaseAssets, releaseNotes, sha256Sums, tomlSectionVersion,
  updaterManifest,
} from "./publish.mjs";

const commit = "0123456789abcdef0123456789abcdef01234567";
const build = (platform, overrides = {}) => ({
  version: "0.1.0",
  platform,
  signed: false,
  notarized: false,
  commit,
  artifacts: [{ file: `MCAO-0.1.0-${platform}.bin`, sha256: "a".repeat(64) }],
  ...overrides,
});

test("the workspace table's version is read, not a dependency's", () => {
  const text = '[dependencies]\nserde = { version = "1" }\n\n[workspace.package]\nversion = "0.1.0"\nedition = "2021"\n';
  assert.equal(tomlSectionVersion(text, "workspace.package"), "0.1.0");
  assert.throws(() => tomlSectionVersion('[dependencies]\nversion = "9.9.9"\n', "package"), /no \[package\] table/);
});

test("a tag must be v plus the version in every source, and the error names the mismatched files", () => {
  const sources = [
    { file: "tauri.conf.json", version: "0.1.0" },
    { file: "package.json", version: "0.1.0" },
  ];
  assert.doesNotThrow(() => checkTagVersions("v0.1.0", sources));
  assert.throws(() => checkTagVersions("0.1.0", sources), /must be v plus the version in every source/);
  assert.throws(
    () => checkTagVersions("v0.1.0", [...sources, { file: "Cargo.toml", version: "0.2.0" }]),
    /mismatched: Cargo\.toml = 0\.2\.0$/,
  );
});

test("a release needs exactly one macOS and one Windows build of the version, from one commit", () => {
  const ok = releaseAssets([build("windows"), build("macos")], "0.1.0");
  assert.equal(ok.commit, commit);
  assert.deepEqual(ok.assets.map((asset) => asset.platform), ["macos", "windows"]);

  assert.throws(() => releaseAssets([build("macos")], "0.1.0"), /expected 2 platform builds, found 1/);
  assert.throws(() => releaseAssets([build("macos"), build("macos")], "0.1.0"), /expected exactly one macos build, found 2/);
  assert.throws(() => releaseAssets([build("macos"), build("linux")], "0.1.0"), /expected exactly one windows build, found 0/);
  assert.throws(() => releaseAssets([build("macos"), build("windows", { version: "0.0.9" })], "0.1.0"), /is version 0\.0\.9, not 0\.1\.0/);
  assert.throws(
    () => releaseAssets([build("macos"), build("windows", { commit: "f".repeat(40) })], "0.1.0"),
    /different commits/,
  );
  assert.throws(
    () => releaseAssets([build("macos"), build("windows", { artifacts: [{ file: "../x.exe", sha256: "a" }] })], "0.1.0"),
    /not a bare file name/,
  );
});

test("SHA256SUMS uses the sha256sum format, one line per installer", () => {
  const assets = [{ file: "a.zip", sha256: "1".repeat(64) }, { file: "b-setup.exe", sha256: "2".repeat(64) }];
  assert.equal(sha256Sums(assets), `${"1".repeat(64)}  a.zip\n${"2".repeat(64)}  b-setup.exe\n`);
});

test("a release with any unsigned installer is a prerelease", () => {
  assert.equal(isPrerelease([{ signed: true }, { signed: false }]), true);
  assert.equal(isPrerelease([{ signed: true, notarized: true }, { signed: true }]), false);
});

test("unsigned notes open with the prerelease warning and name only the unsigned platform", () => {
  const assets = [
    { platform: "macos", file: "MCAO.zip", signed: true, notarized: true, sha256: "" },
    { platform: "windows", file: "MCAO-setup.exe", signed: false, notarized: false, sha256: "" },
  ];
  const notes = releaseNotes({ tag: "v0.1.0", commit, assets });
  assert.match(notes, /^> \*\*Unsigned build: this release is a prerelease\.\*\*/);
  assert.match(notes, /the app is unsigned and internal/);
  assert.match(notes, /Windows \(SmartScreen\):.*More info, then Run anyway/);
  assert.doesNotMatch(notes, /Gatekeeper/);
  const mac = releaseNotes({ tag: "v0.1.0", commit, assets: [{ platform: "macos", file: "MCAO.zip", signed: false, notarized: false, sha256: "" }] });
  assert.match(mac, /Open Anyway/);
  assert.ok(mac.includes("xattr -dr com.apple.quarantine /Applications/MCAO.app"));
  assert.doesNotMatch(mac, /SmartScreen/);
  assert.match(notes, /\| macOS \| `MCAO\.zip` \| Signed and notarized \|/);
  assert.match(notes, /\| Windows \| `MCAO-setup\.exe` \| UNSIGNED \|/);
  assert.ok(notes.includes(`commit \`${commit}\` for tag \`v0.1.0\``));
});

test("a version's changelog section is read under either heading form, and never a longer version's", () => {
  const text = [
    "# Changelog", "",
    "## [0.1.1](https://github.com/x/y/compare/v0.1.0...v0.1.1) (2026-11-17)", "",
    "### Bug Fixes", "", "* keep the tray open", "",
    "## [0.1.10](https://github.com/x/y/compare/v0.1.9...v0.1.10) (2026-12-01)", "", "### Updates", "",
    "## v0.1.0", "", "- Prevent the model selector.", "",
  ].join("\n");
  assert.equal(changelogSection(text, "0.1.1"), "### Bug Fixes\n\n* keep the tray open");
  assert.equal(changelogSection(text, "0.1.10"), "### Updates");
  assert.equal(changelogSection(text, "0.1.0"), "- Prevent the model selector.");
  assert.throws(() => changelogSection(text, "0.1.2"), /no section for 0\.1\.2/);
  assert.throws(() => changelogSection("## Unreleased\n\n- x\n", "0.1.0"), /no section for 0\.1\.0/);
});

test("the changelog section sits under the unsigned warning and above the installers", () => {
  const assets = [{ platform: "macos", file: "MCAO.zip", signed: false, notarized: false, sha256: "" }];
  const notes = releaseNotes({ tag: "v0.1.1", commit, assets, changelog: "### Updates\n\n* x" });
  const at = (text) => notes.indexOf(text);
  assert.ok(at("> **Unsigned build") < at("## Changes"));
  assert.ok(at("## Changes") < at("### Updates"));
  assert.ok(at("### Updates") < at("## Installers"));
  assert.doesNotMatch(releaseNotes({ tag: "v0.1.1", commit, assets }), /## Changes/);
});

test("fully signed notes carry no unsigned warning", () => {
  const assets = [
    { platform: "macos", file: "MCAO.zip", signed: true, notarized: false, sha256: "" },
    { platform: "windows", file: "MCAO-setup.exe", signed: true, notarized: false, sha256: "" },
  ];
  const notes = releaseNotes({ tag: "v0.1.0", commit, assets });
  assert.doesNotMatch(notes, /Unsigned build|UNSIGNED|Gatekeeper|SmartScreen/);
  assert.match(notes, /\| macOS \| `MCAO\.zip` \| Signed, not notarized \|/);
});

// ---- updater assets, against build directories laid out as package.mjs writes them ----

const scratch = mkdtempSync(join(tmpdir(), "publish-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));
const sha = (text) => createHash("sha256").update(text).digest("hex");
const MAC_ZIP = "MCAO-0.1.0-macos-arm64-unsigned.zip";
const MAC_TAR = "MCAO-0.1.0-macos-arm64-unsigned.app.tar.gz";
const WIN_EXE = "MCAO-0.1.0-windows-x64-unsigned-setup.exe";
const MAC_SIG = "bWFjLXNpZ25hdHVyZQ==";
const WIN_SIG = "d2luLXNpZ25hdHVyZQ==";
let fixtureCount = 0;

// `edit` receives the files (name to text) and release-info documents of each platform before they are written.
function fixture({ updater, edit = () => {} } = {}) {
  const root = join(scratch, `fixture-${fixtureCount += 1}`);
  const mac = { files: { [MAC_ZIP]: "mac zip", [MAC_TAR]: "mac archive", [`${MAC_TAR}.sig`]: `${MAC_SIG}\n` } };
  const win = { files: { [WIN_EXE]: "win installer", [`${WIN_EXE}.sig`]: `${WIN_SIG}\n` } };
  mac.info = build("macos", {
    artifacts: [{ file: MAC_ZIP, sha256: sha(mac.files[MAC_ZIP]) }],
    ...(updater && { updater: {
      platformKey: "darwin-aarch64",
      payload: { file: MAC_TAR, sha256: sha(mac.files[MAC_TAR]) },
      signature: { file: `${MAC_TAR}.sig`, sha256: sha(mac.files[`${MAC_TAR}.sig`]) },
    } }),
  });
  win.info = build("windows", {
    artifacts: [{ file: WIN_EXE, sha256: sha(win.files[WIN_EXE]) }],
    ...(updater && { updater: {
      platformKey: "windows-x86_64",
      payload: { file: WIN_EXE, sha256: sha(win.files[WIN_EXE]) },
      signature: { file: `${WIN_EXE}.sig`, sha256: sha(win.files[`${WIN_EXE}.sig`]) },
    } }),
  });
  edit({ mac, win });
  const downloads = join(root, "downloads");
  for (const [name, platform] of [["release-macos-unsigned", mac], ["release-windows-unsigned", win]]) {
    mkdirSync(join(downloads, name), { recursive: true });
    for (const [file, text] of Object.entries(platform.files)) writeFileSync(join(downloads, name, file), text);
    writeFileSync(join(downloads, name, "release-info.json"), JSON.stringify(platform.info));
  }
  return { downloads, out: join(root, "out"), root };
}

const run = ({ downloads, out }, updater) => assemble({
  tag: "v0.1.0", downloads, out, updater,
  sources: [{ file: "tauri.conf.json", version: "0.1.0" }],
  changelogText: "## v0.1.0\n\n- A change.\n",
});
const on = { enabled: true, publicKey: "key" };
const off = { enabled: false };

test("a disabled build publishes exactly the two manual installers and their SHA256SUMS", () => {
  const dirs = fixture();
  run(dirs, off);
  assert.deepEqual(readdirSync(join(dirs.out, "assets")).sort(), [MAC_ZIP, WIN_EXE, "SHA256SUMS"]);
});

test("an enabled pair publishes one macOS ZIP, one Windows EXE, one macOS archive, two signatures, SHA256SUMS and latest.json", () => {
  const dirs = fixture({ updater: true });
  const { prerelease } = run(dirs, on);
  const assets = join(dirs.out, "assets");
  assert.deepEqual(readdirSync(assets).sort(), [MAC_ZIP, MAC_TAR, `${MAC_TAR}.sig`, "SHA256SUMS", WIN_EXE, `${WIN_EXE}.sig`, "latest.json"].sort());
  assert.equal(prerelease, true);
  const base = "https://github.com/mc-innersource/motif-mcao/releases/download/v0.1.0";
  assert.deepEqual(JSON.parse(readFileSync(join(assets, "latest.json"), "utf8")), {
    version: "0.1.0",
    platforms: {
      "darwin-aarch64": { url: `${base}/${MAC_TAR}`, signature: MAC_SIG },
      "windows-x86_64": { url: `${base}/${WIN_EXE}`, signature: WIN_SIG },
    },
  });
  assert.equal(readFileSync(join(assets, "SHA256SUMS"), "utf8"), `${sha("mac zip")}  ${MAC_ZIP}\n${sha("win installer")}  ${WIN_EXE}\n`);
  assert.doesNotMatch(readFileSync(join(dirs.out, "release-notes.md"), "utf8"), /app\.tar\.gz|latest\.json/);
});

test("an enabled assembly refuses missing, partial and corrupt updater data and writes no release", () => {
  const cases = [
    ["a build without an updater record", fixture({ updater: true, edit: ({ win }) => delete win.info.updater }), /windows build has no updater record/],
    ["a payload that is not in the build", fixture({ updater: true, edit: ({ mac }) => delete mac.files[MAC_TAR] }), new RegExp(`${MAC_TAR} is missing from the macos build`)],
    ["a signature whose bytes changed", fixture({ updater: true, edit: ({ win }) => { win.files[`${WIN_EXE}.sig`] = "tampered"; } }), /does not match the sha256 its build recorded/],
    ["a wrong platform key", fixture({ updater: true, edit: ({ mac }) => { mac.info.updater.platformKey = "darwin-x86_64"; } }), /expected darwin-aarch64/],
    ["a payload named for the wrong signing state", fixture({ updater: true, edit: ({ mac }) => { mac.info.updater.payload.file = MAC_TAR.replace("unsigned", "signed"); } }), /payload is .*-signed\.app\.tar\.gz, expected/],
    ["a Windows payload that is not the manual installer", fixture({ updater: true, edit: ({ win }) => { win.info.updater.payload.sha256 = "b".repeat(64); } }), /payload is not the manual installer/],
    ["a payload outside the build directory", fixture({ updater: true, edit: ({ mac }) => { mac.info.updater.payload.file = "../x.app.tar.gz"; } }), /not a bare file name/],
    ["an extra updater field", fixture({ updater: true, edit: ({ mac }) => { mac.info.updater.notes = "x"; } }), /exactly platformKey, payload and signature/],
  ];
  for (const [name, dirs, message] of cases) {
    assert.throws(() => run(dirs, on), message, name);
    assert.equal(existsSync(join(dirs.out, "assets", "latest.json")), false, name);
  }
});

test("a disabled assembly refuses updater data", () => {
  assert.throws(() => run(fixture({ updater: true }), off), /macos build carries updater data but MCAO_UPDATES_ENABLED is not true/);
});

test("a signature that is not one base64 line cannot enter latest.json", () => {
  const entry = { platformKey: "darwin-aarch64", file: MAC_TAR };
  assert.throws(() => updaterManifest("0.1.0", [{ ...entry, signature: "two\nlines" }]), /not a single base64 line/);
  assert.throws(() => updaterManifest("0.1.0", [{ ...entry, signature: "  " }]), /not a single base64 line/);
  assert.equal(updaterManifest("0.1.0", [{ ...entry, signature: ` ${MAC_SIG}\n` }]).platforms["darwin-aarch64"].signature, MAC_SIG);
});
