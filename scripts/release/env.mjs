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
  return take(env, signingNames("windows"));
}

const take = (env, names) => {
  const taken = {};
  for (const name of names) {
    taken[name] = env[name];
    delete env[name];
  }
  return taken;
};

// Tauri reads these two names to sign the updater payloads. They are not OS code-signing secrets.
export const UPDATER_SECRET_NAMES = ["TAURI_SIGNING_PRIVATE_KEY", "TAURI_SIGNING_PRIVATE_KEY_PASSWORD"];

// Same contract as takeWindowsSecrets: the caller reattaches them to the Tauri build and nothing else.
export function takeUpdaterSecrets(env) {
  return take(env, UPDATER_SECRET_NAMES);
}

// Whether this run ships updater artifacts. MCAO_UPDATES_ENABLED is blank or `false` (off) or exactly
// `true` (on); anything else is a configuration error, so a typo never silently ships without updates.
// When on, the public key is required by both roles, and the packaging role also needs the private key.
export function updaterDecision(env, role) {
  const value = (env.MCAO_UPDATES_ENABLED ?? "").trim();
  if (value === "" || value === "false") return { enabled: false };
  if (value !== "true") throw new Error(`MCAO_UPDATES_ENABLED must be blank, false or true, found '${value}'`);
  const required = ["MCAO_UPDATER_PUBLIC_KEY", ...(role === "package" ? UPDATER_SECRET_NAMES : [])];
  const missing = required.filter((name) => !env[name]?.trim());
  if (missing.length > 0) throw new Error(`MCAO_UPDATES_ENABLED=true requires ${missing.join(", ")}`);
  return { enabled: true, publicKey: env.MCAO_UPDATER_PUBLIC_KEY.trim() };
}

// The one `--config` JSON for the Tauri build, or null when nothing overrides tauri.bundle.conf.json:
// the Windows signing fields (the thumbprint is not a secret) and the updater switch plus public key.
export function tauriConfigOverride({ windows, updaterPublicKey }) {
  const config = {};
  if (windows) config.bundle = { windows };
  if (updaterPublicKey) {
    config.bundle = { ...config.bundle, createUpdaterArtifacts: true };
    config.plugins = { updater: { pubkey: updaterPublicKey } };
  }
  return Object.keys(config).length > 0 ? JSON.stringify(config) : null;
}

// Environment of the packaging child. Signing variables are inherited only when signing and
// only when non-empty (Tauri treats a variable that is set but empty as configured). Windows
// signing secrets are never inherited: the thumbprint reaches Tauri through --config. The updater key
// and password are stripped too and reattached by the caller to the Tauri build alone.
export function buildChildEnv(platform, env, decision) {
  const child = { ...env, CI: "true" };
  for (const name of signingNames(platform)) {
    if (!decision.sign || !child[name]?.trim()) delete child[name];
  }
  if (platform === "windows") takeWindowsSecrets(child);
  takeUpdaterSecrets(child);
  delete child.REQUIRE_SIGNING;
  delete child.ALLOW_UNSIGNED_RELEASE;
  return child;
}
