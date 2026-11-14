// Stages the daemon sidecar, then runs `tauri build` for one bundle type.
// Usage (from apps/desktop): node scripts/package.mjs <app|nsis>
//
// Tauri's `--ci` flag reads the CI environment variable and accepts only
// `true` or `false`; many CI systems and harnesses set `CI=1`, which aborts
// the build with "invalid value '1' for '--ci'". The variable is normalized
// here so packaging does not depend on how the caller set it.
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const bundles = new Set(["app", "nsis"]);
const bundle = process.argv[2];
if (!bundles.has(bundle)) {
  console.error(`usage: node scripts/package.mjs <${[...bundles].join("|")}>`);
  process.exit(2);
}

const here = dirname(fileURLToPath(import.meta.url));
const desktop = join(here, "..");

const normalizeCi = (env) => {
  const next = { ...env };
  const value = next.CI?.trim().toLowerCase();
  if (value === "1" || value === "true") next.CI = "true";
  else if (value === "0" || value === "false") next.CI = "false";
  else delete next.CI;
  return next;
};

const run = (command, args) => {
  const result = spawnSync(command, args, {
    cwd: desktop,
    stdio: "inherit",
    env: normalizeCi(process.env),
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
};

const tauri = createRequire(import.meta.url).resolve("@tauri-apps/cli/tauri.js");
run(process.execPath, [join(here, "prepare-daemon-sidecar.mjs")]);
run(process.execPath, [
  tauri,
  "build",
  "--config",
  "src-tauri/tauri.bundle.conf.json",
  "--bundles",
  bundle,
]);
