import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";
import { buildChildEnv, resolveTargetDir, takeWindowsSecrets } from "./env.mjs";

const signed = { sign: true, notarize: false };
const unsigned = { sign: false, notarize: false };
const winSecrets = { WINDOWS_CERTIFICATE: "cert-value", WINDOWS_CERTIFICATE_PASSWORD: "pw-value" };

test("a Windows runner.temp path with mixed separators is accepted and normalized (win32 semantics)", () => {
  // release.yml sets CARGO_TARGET_DIR to `${{ runner.temp }}/target`.
  const value = "D:\\a\\_temp/target";
  assert.notEqual(path.win32.resolve(value), value, "the old resolve(x) === x check rejected this");
  assert.equal(resolveTargetDir(value, path.win32), "D:\\a\\_temp\\target");
  assert.equal(resolveTargetDir("D:\\a\\_temp\\target\\", path.win32), "D:\\a\\_temp\\target");
  assert.equal(resolveTargetDir("\\\\server\\share\\t", path.win32), "\\\\server\\share\\t");
});

test("relative, drive-relative, empty and unset values are rejected on both path flavours", () => {
  for (const bad of [undefined, "", "target", "..\\target", "D:target"]) {
    assert.throws(() => resolveTargetDir(bad, path.win32), /absolute path/, String(bad));
  }
  for (const bad of [undefined, "", "target", "../target"]) {
    assert.throws(() => resolveTargetDir(bad, path.posix), /absolute path/, String(bad));
  }
  assert.equal(resolveTargetDir("/var/t/./target/", path.posix), "/var/t/target");
});

test("the Windows child never inherits the signing certificate or password", () => {
  const env = { PATH: "p", ...winSecrets, REQUIRE_SIGNING: "true", ALLOW_UNSIGNED_RELEASE: "true" };
  const child = buildChildEnv("windows", env, signed);
  assert.deepEqual(child, { PATH: "p", CI: "true" });
  assert.deepEqual(buildChildEnv("windows", env, unsigned), { PATH: "p", CI: "true" });
  assert.equal(env.WINDOWS_CERTIFICATE, "cert-value", "the input is not modified");
});

test("the macOS child keeps non-empty signing variables only when signing, drops empty ones", () => {
  const env = { APPLE_CERTIFICATE: "c", APPLE_CERTIFICATE_PASSWORD: "p", APPLE_SIGNING_IDENTITY: "i", APPLE_ID: "", HOME: "h" };
  const child = buildChildEnv("macos", env, { sign: true, notarize: false });
  assert.deepEqual(Object.keys(child).sort(), ["APPLE_CERTIFICATE", "APPLE_CERTIFICATE_PASSWORD", "APPLE_SIGNING_IDENTITY", "CI", "HOME"]);
  assert.deepEqual(Object.keys(buildChildEnv("macos", env, unsigned)).sort(), ["CI", "HOME"]);
});

test("takeWindowsSecrets moves the secrets out of the environment object", () => {
  const env = { PATH: "p", ...winSecrets };
  assert.deepEqual(takeWindowsSecrets(env), winSecrets);
  assert.deepEqual(env, { PATH: "p" });
  assert.deepEqual(takeWindowsSecrets(env), { WINDOWS_CERTIFICATE: undefined, WINDOWS_CERTIFICATE_PASSWORD: undefined });
});

test("a process that took the secrets cannot pass them to a child it spawns afterwards", () => {
  const url = pathToFileURL(path.resolve(import.meta.dirname, "env.mjs")).href;
  const script = `
    import { spawnSync } from "node:child_process";
    import { takeWindowsSecrets } from ${JSON.stringify(url)};
    const taken = takeWindowsSecrets(process.env);
    if (taken.WINDOWS_CERTIFICATE !== "cert-value") throw new Error("not taken");
    const r = spawnSync(process.execPath, ["-e", "console.log(JSON.stringify(Object.keys(process.env).filter((k) => k.startsWith('WINDOWS_'))))"], { encoding: "utf8" });
    process.stdout.write(r.stdout);`;
  const r = spawnSync(process.execPath, ["--input-type=module", "-e", script], { encoding: "utf8", env: { ...process.env, ...winSecrets } });
  assert.equal(r.status, 0, r.stderr);
  assert.equal(r.stdout.trim(), "[]");
});
