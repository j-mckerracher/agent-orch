import assert from "node:assert/strict";
import { test } from "node:test";
import {
  checkTagVersions, isPrerelease, releaseAssets, releaseNotes, sha256Sums, tomlSectionVersion,
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

test("fully signed notes carry no unsigned warning", () => {
  const assets = [
    { platform: "macos", file: "MCAO.zip", signed: true, notarized: false, sha256: "" },
    { platform: "windows", file: "MCAO-setup.exe", signed: true, notarized: false, sha256: "" },
  ];
  const notes = releaseNotes({ tag: "v0.1.0", commit, assets });
  assert.doesNotMatch(notes, /Unsigned build|UNSIGNED|Gatekeeper|SmartScreen/);
  assert.match(notes, /\| macOS \| `MCAO\.zip` \| Signed, not notarized \|/);
});
