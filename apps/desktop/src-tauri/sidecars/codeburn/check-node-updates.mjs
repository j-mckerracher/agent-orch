// Warns, never fails, when a newer Node.js 22.x release than the bundled one exists (DPES-1105).
// audit.yml runs it nightly. Security releases are marked in nodejs.org's release index.
//
//   node check-node-updates.mjs [index-url]
//
// Exit status is 0 in every case, so a warning cannot block a merge. A failed download is reported
// as a warning too.
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { join } from "node:path";
import { sidecarDir } from "./lib.mjs";

const parse = (version) => version.replace(/^v/, "").split(".").map(Number);

/** Release entries on the 22.x line that are newer than `pinned`, newest first. Pure. */
export function newerNode22(index, pinned) {
  const base = parse(pinned);
  return index
    .filter((release) => /^v22\./.test(release.version))
    .filter((release) => {
      const found = parse(release.version);
      for (let i = 0; i < 3; i += 1) {
        if (found[i] !== base[i]) return found[i] > base[i];
      }
      return false;
    })
    .sort((a, b) => {
      const x = parse(a.version);
      const y = parse(b.version);
      for (let i = 0; i < 3; i += 1) if (x[i] !== y[i]) return y[i] - x[i];
      return 0;
    });
}

async function main() {
  const url = process.argv[2] ?? "https://nodejs.org/dist/index.json";
  const runtime = JSON.parse(readFileSync(join(sidecarDir, "node-runtime.json"), "utf8"));
  let index;
  try {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    index = await response.json();
  } catch (error) {
    console.log(`::warning title=Node.js 22 update check::could not read ${url}: ${error.message}`);
    return;
  }
  const newer = newerNode22(index, runtime.version);
  if (newer.length === 0) {
    console.log(`bundled Node.js ${runtime.version} is the newest 22.x release`);
    return;
  }
  const security = newer.filter((release) => release.security);
  console.log(
    `::warning title=Node.js 22 update available::bundled ${runtime.version}; newer 22.x: ${newer.map((r) => r.version).join(", ")}` +
      (security.length ? `; security releases among them: ${security.map((r) => r.version).join(", ")}` : "") +
      `. Follow the upgrade procedure in docs/third-party/codeburn.md.`,
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
