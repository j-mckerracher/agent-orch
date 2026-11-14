import assert from "node:assert/strict";
import { test } from "node:test";
import { signingDecision } from "./signing.mjs";

const mac = { APPLE_CERTIFICATE: "x", APPLE_CERTIFICATE_PASSWORD: "x", APPLE_SIGNING_IDENTITY: "x" };
const notary = { APPLE_ID: "x", APPLE_PASSWORD: "x", APPLE_TEAM_ID: "x" };

test("no secrets: a clean unsigned skip with a clear message", () => {
  const d = signingDecision("macos", {});
  assert.equal(d.mode, "unsigned");
  assert.match(d.message, /No macos signing secrets are set .*UNSIGNED/);
  assert.equal(signingDecision("windows", { WINDOWS_CERTIFICATE: "  " }).mode, "unsigned");
});

test("REQUIRE_SIGNING turns an absent configuration into an error", () => {
  assert.equal(signingDecision("windows", { REQUIRE_SIGNING: "true" }).mode, "error");
  assert.equal(signingDecision("windows", { REQUIRE_SIGNING: "false" }).mode, "unsigned");
});

test("a partial configuration is an error that names the missing variables only", () => {
  const d = signingDecision("macos", { APPLE_CERTIFICATE: "secret-value" });
  assert.equal(d.mode, "error");
  assert.match(d.message, /APPLE_CERTIFICATE_PASSWORD, APPLE_SIGNING_IDENTITY/);
  assert.doesNotMatch(d.message, /secret-value/);
  assert.equal(signingDecision("macos", { ...mac, APPLE_ID: "x" }).mode, "error");
});

test("signing without notarization is allowed unless required", () => {
  const d = signingDecision("macos", mac);
  assert.deepEqual([d.mode, d.sign, d.notarize], ["signed", true, false]);
  assert.match(d.message, /NOT notarized/);
  assert.equal(signingDecision("macos", { ...mac, REQUIRE_SIGNING: "true" }).mode, "error");
});

test("full macOS and Windows configurations", () => {
  const d = signingDecision("macos", { ...mac, ...notary, REQUIRE_SIGNING: "true" });
  assert.deepEqual([d.mode, d.sign, d.notarize], ["signed", true, true]);
  const w = signingDecision("windows", { WINDOWS_CERTIFICATE: "x", WINDOWS_CERTIFICATE_PASSWORD: "x", REQUIRE_SIGNING: "true" });
  assert.deepEqual([w.mode, w.sign, w.notarize], ["signed", true, false]);
});

test("a tag build without secrets is an error unless ALLOW_UNSIGNED_RELEASE=true opts in", () => {
  const tag = { GITHUB_REF_TYPE: "tag", GITHUB_REF_NAME: "v1.0.0" };
  for (const platform of ["macos", "windows"]) {
    const d = signingDecision(platform, tag);
    assert.equal(d.mode, "error");
    assert.match(d.message, /tag build.*ALLOW_UNSIGNED_RELEASE=true/);
    assert.deepEqual([signingDecision(platform, { ...tag, ALLOW_UNSIGNED_RELEASE: "true" }).mode], ["unsigned"]);
  }
  assert.equal(signingDecision("windows", { GITHUB_REF_TYPE: "branch" }).mode, "unsigned");
  assert.equal(signingDecision("windows", { ...tag, ALLOW_UNSIGNED_RELEASE: "yes" }).mode, "error");
});

test("a tag build with signing secrets signs; macOS also needs notarization unless opted out", () => {
  const tag = { GITHUB_REF_TYPE: "tag" };
  const w = signingDecision("windows", { ...tag, WINDOWS_CERTIFICATE: "x", WINDOWS_CERTIFICATE_PASSWORD: "x" });
  assert.deepEqual([w.mode, w.sign], ["signed", true]);
  assert.equal(signingDecision("macos", { ...tag, ...mac }).mode, "error");
  assert.equal(signingDecision("macos", { ...tag, ...mac, ALLOW_UNSIGNED_RELEASE: "true" }).mode, "signed");
  assert.deepEqual(signingDecision("macos", { ...tag, ...mac, ...notary }).notarize, true);
});

test("REQUIRE_SIGNING takes precedence over the unsigned opt-in", () => {
  const d = signingDecision("windows", { REQUIRE_SIGNING: "true", ALLOW_UNSIGNED_RELEASE: "true" });
  assert.equal(d.mode, "error");
  assert.match(d.message, /REQUIRE_SIGNING=true/);
});
