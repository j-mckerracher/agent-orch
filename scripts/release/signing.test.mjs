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
