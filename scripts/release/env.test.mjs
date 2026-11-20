import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";
import { buildChildEnv, resolveTargetDir, takeUpdaterSecrets, takeWindowsSecrets, tauriConfigOverride, updaterDecision } from "./env.mjs";

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

test("relative, drive-relative, empty and unset values are rejected on both path flavors", () => {
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

const updaterSecrets = { TAURI_SIGNING_PRIVATE_KEY: "key-value", TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "pw-value" };

test("MCAO_UPDATES_ENABLED is blank, false or true and nothing else", () => {
  assert.deepEqual(updaterDecision({}, "package"), { enabled: false });
  assert.deepEqual(updaterDecision({ MCAO_UPDATES_ENABLED: "" }, "publish"), { enabled: false });
  assert.deepEqual(updaterDecision({ MCAO_UPDATES_ENABLED: "false", ...updaterSecrets }, "package"), { enabled: false });
  for (const bad of ["TRUE", "1", "yes", "True"]) {
    assert.throws(() => updaterDecision({ MCAO_UPDATES_ENABLED: bad }, "package"), /must be blank, false or true/, bad);
  }
});

test("enabled updates need the public key, and packaging also the private key and password", () => {
  const env = { MCAO_UPDATES_ENABLED: "true", MCAO_UPDATER_PUBLIC_KEY: " pub \n", ...updaterSecrets };
  assert.deepEqual(updaterDecision(env, "package"), { enabled: true, publicKey: "pub" });
  assert.deepEqual(updaterDecision({ MCAO_UPDATES_ENABLED: "true", MCAO_UPDATER_PUBLIC_KEY: "pub" }, "publish"), { enabled: true, publicKey: "pub" });
  assert.throws(() => updaterDecision({ ...env, MCAO_UPDATER_PUBLIC_KEY: "" }, "publish"), /requires MCAO_UPDATER_PUBLIC_KEY$/);
  assert.throws(
    () => updaterDecision({ MCAO_UPDATES_ENABLED: "true", MCAO_UPDATER_PUBLIC_KEY: "pub", TAURI_SIGNING_PRIVATE_KEY: "  " }, "package"),
    /requires TAURI_SIGNING_PRIVATE_KEY, TAURI_SIGNING_PRIVATE_KEY_PASSWORD$/,
  );
});

test("no packaging child inherits the updater key or password, on either platform", () => {
  const env = { PATH: "p", MCAO_UPDATER_PUBLIC_KEY: "pub", ...updaterSecrets };
  for (const platform of ["macos", "windows"]) {
    assert.deepEqual(buildChildEnv(platform, env, unsigned), { PATH: "p", MCAO_UPDATER_PUBLIC_KEY: "pub", CI: "true" }, platform);
  }
  assert.equal(env.TAURI_SIGNING_PRIVATE_KEY, "key-value", "the input is not modified");
  assert.deepEqual(takeUpdaterSecrets(env), updaterSecrets);
  assert.deepEqual(env, { PATH: "p", MCAO_UPDATER_PUBLIC_KEY: "pub" });
});

test("one Tauri config merges the Windows signing fields and the updater switch, and is absent when neither applies", () => {
  const windows = { certificateThumbprint: "A".repeat(40), digestAlgorithm: "sha256", timestampUrl: "http://t" };
  assert.equal(tauriConfigOverride({}), null);
  assert.deepEqual(JSON.parse(tauriConfigOverride({ windows })), { bundle: { windows } });
  assert.deepEqual(JSON.parse(tauriConfigOverride({ updaterPublicKey: "pub" })), {
    bundle: { createUpdaterArtifacts: true }, plugins: { updater: { pubkey: "pub" } },
  });
  assert.deepEqual(JSON.parse(tauriConfigOverride({ windows, updaterPublicKey: "pub" })), {
    bundle: { windows, createUpdaterArtifacts: true }, plugins: { updater: { pubkey: "pub" } },
  });
});
