import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const repo = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const read = (file) => JSON.parse(readFileSync(join(repo, file), "utf8"));
const config = read("release-please-config.json");
const bumped = config.packages["."]["extra-files"].map((file) => file.path);
const appVersion = read("apps/desktop/src-tauri/tauri.conf.json").version;

test("release-please bumps every version source that publish.mjs checks", () => {
  for (const file of [
    "Cargo.toml",
    "apps/desktop/src-tauri/Cargo.toml",
    "apps/desktop/src-tauri/tauri.conf.json",
    "apps/desktop/package.json",
  ]) {
    assert.ok(bumped.includes(file), `${file} is not in the release-please extra-files`);
  }
});

test("release-please bumps both lockfiles and the root entry of the npm lockfile", () => {
  for (const file of ["Cargo.lock", "apps/desktop/src-tauri/Cargo.lock", "apps/desktop/package-lock.json"]) {
    assert.ok(bumped.includes(file), `${file} is not in the release-please extra-files`);
  }
  const npmLock = config.packages["."]["extra-files"].filter((file) => file.path === "apps/desktop/package-lock.json");
  assert.deepEqual(npmLock.map((file) => file.jsonpath).sort(), ["$.packages[''].version", "$.version"]);
});

test("the release manifest starts at the app version, so the first release PR bumps from it", () => {
  assert.equal(read(".release-please-manifest.json")["."], appVersion);
});
