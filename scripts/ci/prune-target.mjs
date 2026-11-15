// Removes the build output of this repository's own packages (workspace members and path dependencies)
// from a Cargo target directory, so that the CI cache holds third-party artifacts only.
//
//   node scripts/ci/prune-target.mjs --target-dir T --dir D [--dir E ...]
//
// Why: a checkout gives every local file a fresh timestamp, so Cargo rebuilds local packages anyway, and
// the build and lint gates must show `Compiling` and `Checking` lines for them. A cached copy would hide
// nothing and would fail those assertions. Registry and git packages are not touched.
import { execFileSync } from "node:child_process";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

// The names of the local packages in a `cargo metadata` document: source is null for workspace members
// and path dependencies, and set for registry and git packages.
export const localPackageNames = (metadata) =>
  [...new Set(metadata.packages.filter((p) => p.source === null).map((p) => p.name))].sort();

const optionValues = (argv, name) => argv.flatMap((a, i) => (a === name ? [argv[i + 1]] : []));

function prune(argv) {
  const targetDir = optionValues(argv, "--target-dir")[0];
  const dirs = optionValues(argv, "--dir");
  if (!targetDir || dirs.length === 0) throw new Error("usage: prune-target.mjs --target-dir T --dir D [--dir E ...]");
  const env = { ...process.env, CARGO_TARGET_DIR: resolve(targetDir) };
  for (const dir of dirs) {
    const cwd = resolve(dir);
    const metadata = JSON.parse(
      execFileSync("cargo", ["metadata", "--format-version", "1", "--locked"], {
        cwd, env, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"], maxBuffer: 256 * 1024 * 1024,
      }),
    );
    const names = localPackageNames(metadata);
    if (names.length === 0) continue;
    // `cargo clean -p` removes the debug and test artifacts only; the release profile needs its own call.
    for (const profile of [[], ["--release"]]) {
      execFileSync("cargo", ["clean", ...profile, ...names.flatMap((name) => ["-p", name])], {
        cwd, env, stdio: ["ignore", "inherit", "inherit"],
      });
    }
    console.log(`${dir}: pruned ${names.length} local package(s) in the debug and release profiles: ${names.join(", ")}`);
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  try {
    prune(process.argv.slice(2));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
