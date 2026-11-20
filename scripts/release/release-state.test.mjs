import assert from "node:assert/strict";
import { test } from "node:test";
import { uploadAction } from "./release-state.mjs";

const staged = { "a.zip": "1".repeat(64), "latest.json": "2".repeat(64) };
const asset = (name, hex, over = {}) => ({ name, state: "uploaded", digest: `sha256:${hex}`, ...over });
const published = (assets, over = {}) => ({ tag_name: "v1.0.0", draft: false, assets, ...over });
const exact = [asset("a.zip", staged["a.zip"]), asset("latest.json", staged["latest.json"])];

test("no release for the tag means create one", () => {
  assert.equal(uploadAction([], "v1.0.0", staged), "create");
  assert.equal(uploadAction([published(exact, { tag_name: "v0.9.0" })], "v1.0.0", staged), "create");
});

test("a published release with exactly the staged names and digests is left alone", () => {
  assert.equal(uploadAction([published(exact)], "v1.0.0", staged), "noop");
});

test("a published release is refused when bytes changed, an asset is missing or extra, or the digest is unknown", () => {
  const cases = [
    [[asset("a.zip", "9".repeat(64)), exact[1]], /a\.zip differs from the published bytes/],
    [[exact[0]], /latest\.json is not in the published release/],
    [[...exact, asset("SHA256SUMS", "3".repeat(64))], /SHA256SUMS is published but not in this build/],
    [[{ name: "a.zip", state: "uploaded" }, exact[1]], /a\.zip has no digest evidence/],
    [[asset("a.zip", staged["a.zip"], { state: "starter" }), exact[1]], /a\.zip has no digest evidence/],
  ];
  for (const [assets, message] of cases) assert.throws(() => uploadAction([published(assets)], "v1.0.0", staged), message);
});

test("an old published tag never gains a latest.json", () => {
  const old = [asset("a.zip", staged["a.zip"])];
  assert.throws(() => uploadAction([published(old)], "v1.0.0", staged), /latest\.json is not in the published release/);
});

test("a draft is uploaded over unless it holds an asset this build lacks", () => {
  assert.equal(uploadAction([published([], { draft: true })], "v1.0.0", staged), "upload");
  assert.equal(uploadAction([published([asset("a.zip", "9".repeat(64))], { draft: true })], "v1.0.0", staged), "upload");
  assert.throws(() => uploadAction([published([asset("old.zip", "9".repeat(64))], { draft: true })], "v1.0.0", staged), /draft v1\.0\.0 has assets this build does not: old\.zip/);
});

test("two releases for one tag are refused", () => {
  assert.throws(() => uploadAction([published(exact), published([], { draft: true })], "v1.0.0", staged), /2 releases are named v1\.0\.0/);
});
