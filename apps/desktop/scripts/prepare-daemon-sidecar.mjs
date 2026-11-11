// Builds the release daemon and stages it where Tauri's `externalBin` expects
// it: src-tauri/binaries/orchestrator-daemon-<target-triple>[.exe].
// Run from apps/desktop (npm run package:mac / package:win does this first).
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "../../..");
const run = (command, args, options = {}) => {
  const result = spawnSync(command, args, { encoding: "utf8", ...options });
  if (result.error || result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed: ${result.error ?? result.stderr}`);
  }
  return result.stdout;
};

const triple = (/host: (\S+)/.exec(run("rustc", ["-vV"])) ?? [])[1];
if (!triple) throw new Error("cannot read the host target triple from rustc -vV");
const exe = triple.includes("windows") ? ".exe" : "";

const targetDir = resolve(process.env.CARGO_TARGET_DIR ?? join(repo, "target"));
run("cargo", ["build", "--locked", "--release", "-p", "orchestrator-daemon"], {
  cwd: repo,
  stdio: "inherit",
  env: { ...process.env, CARGO_TARGET_DIR: targetDir },
});

const out = join(here, "../src-tauri/binaries");
mkdirSync(out, { recursive: true });
const destination = join(out, `orchestrator-daemon-${triple}${exe}`);
copyFileSync(join(targetDir, "release", `orchestrator-daemon${exe}`), destination);
console.log(`staged ${destination}`);
