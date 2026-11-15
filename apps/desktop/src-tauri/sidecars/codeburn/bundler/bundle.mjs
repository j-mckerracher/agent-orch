// Bundles the pinned codeburn reader into the two files the daemon runs, dist/main.js and
// dist/parse-worker.js, each as one self-contained ES module. The launcher dist/cli.js is not
// rebuilt: callers copy it from the installed package unchanged.
//
// Why: codeburn 0.9.28 installs about 10,000 files for these two entries. Measured on one machine
// (Node 22.23.5, claude provider, empty cache): the installed tree took 6-15 s cold, the bundle
// 0.5-1 s. One file per entry also lets the daemon pin hash all the code it executes.
//
// Reproducibility: esbuild is pinned exactly (bundler/package-lock.json) and reads the tree that
// `npm ci` installed from the sidecar lockfile. absWorkingDir is the install root, so the output
// carries no machine path. docs/third-party/codeburn.md records the rebuild check.
import { build } from "esbuild";
import { mkdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const ENTRIES = ["main.js", "parse-worker.js"];

// Modules that are never bundled. `tsx` is a development tool that codeburn loads only from its
// own dev path. `readline/promises` is a Node built-in that this esbuild target does not list.
const EXTERNAL = ["tsx", "readline/promises"];

// react-devtools-core is an optional development dependency of ink (codeburn's TUI library). ink
// imports it only when DEV=true, but esbuild would hoist the static import to the top of the
// bundle, where it fails to resolve. It is replaced by an empty module. The reader's environment
// never passes DEV, so the replacement is never reached.
const devtoolsStub = {
  name: "stub-react-devtools-core",
  setup(build) {
    build.onResolve({ filter: /^react-devtools-core$/ }, () => ({
      path: "react-devtools-core",
      namespace: "mcao-stub",
    }));
    build.onLoad({ filter: /.*/, namespace: "mcao-stub" }, () => ({
      contents: "export default undefined;",
      loader: "js",
    }));
  },
};

// Bundled CommonJS code calls require() for Node built-ins. An ES module has no require, so the
// bundle defines one with createRequire, the same way the upstream launcher's ESM entry works.
const REQUIRE_BANNER =
  'import { createRequire as __mcaoCreateRequire } from "node:module";\n' +
  "const require = __mcaoCreateRequire(import.meta.url);";

/**
 * Writes `<outDir>/main.js` and `<outDir>/parse-worker.js` from the codeburn package installed under
 * `<installRoot>/node_modules/codeburn`. Returns the two output paths.
 */
export async function bundleReader({ installRoot, outDir }) {
  const dist = join(resolve(installRoot), "node_modules", "codeburn", "dist");
  mkdirSync(outDir, { recursive: true });
  const outputs = [];
  for (const entry of ENTRIES) {
    const outfile = join(resolve(outDir), entry);
    await build({
      absWorkingDir: resolve(installRoot),
      entryPoints: [join(dist, entry)],
      outfile,
      bundle: true,
      platform: "node",
      format: "esm",
      target: "node22",
      external: EXTERNAL,
      banner: { js: REQUIRE_BANNER },
      // Keeps the license comments of bundled packages, at the end of the file.
      legalComments: "eof",
      sourcemap: false,
      minify: false,
      logLevel: "warning",
      plugins: [devtoolsStub],
    });
    outputs.push(outfile);
  }
  return outputs;
}

// node bundle.mjs <installRoot> <outDir>
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [installRoot, outDir] = process.argv.slice(2);
  if (!installRoot || !outDir) {
    console.error("usage: node bundle.mjs <installRoot> <outDir>");
    process.exit(2);
  }
  const outputs = await bundleReader({ installRoot, outDir });
  for (const file of outputs) console.log(`bundled ${file}`);
}
