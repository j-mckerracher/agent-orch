// Environment handling for release packaging, pure so it can be tested on any host.
import path from "node:path";
import { signingNames } from "./signing.mjs";

// `pathApi` is injectable so tests can exercise win32 semantics on any host. On Windows a value
// such as `D:\a\_temp/target` (runner.temp joined with a forward slash) is absolute but is not
// equal to its own `resolve()` result, so absoluteness is tested with isAbsolute and the
// normalized value is what callers must use afterwards.
export function resolveTargetDir(value, pathApi = path) {
  if (!value || !pathApi.isAbsolute(value)) throw new Error("CARGO_TARGET_DIR must be set to an absolute path");
  return pathApi.resolve(value);
}

// Removes the Windows signing certificate and password from `env` and returns them. The build
// child, Tauri and every cargo build script run after this and never inherit them; the caller
// hands them only to the one PowerShell step that imports the certificate.
export function takeWindowsSecrets(env) {
  const taken = {};
  for (const name of signingNames("windows")) {
    taken[name] = env[name];
    delete env[name];
  }
  return taken;
}

// Environment of the packaging child. Signing variables are inherited only when signing and
// only when non-empty (Tauri treats a variable that is set but empty as configured). Windows
// signing secrets are never inherited: the thumbprint reaches Tauri through TAURI_CONFIG.
export function buildChildEnv(platform, env, decision) {
  const child = { ...env, CI: "true" };
  for (const name of signingNames(platform)) {
    if (!decision.sign || !child[name]?.trim()) delete child[name];
  }
  if (platform === "windows") takeWindowsSecrets(child);
  delete child.REQUIRE_SIGNING;
  delete child.ALLOW_UNSIGNED_RELEASE;
  return child;
}
