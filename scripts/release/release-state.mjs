// Decides what the `Publish the GitHub release` step may do for a tag, from the releases that already exist.
//
//   gh api --paginate --slurp "repos/$GITHUB_REPOSITORY/releases?per_page=100" \
//     | node scripts/release/release-state.mjs <tag> <assets-dir>
//
// Prints one word: `create` (no release: create a draft, upload, publish), `upload` (an existing draft:
// upload over it, publish) or `noop` (already published with exactly these assets: touch nothing).
// Anything else throws, so a published release never changes and a draft never ends with a stray asset.
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

// `releases` is the flat list of release objects from the REST API; `staged` maps asset name to sha256.
export function uploadAction(releases, tag, staged) {
  const matches = releases.filter((release) => release.tag_name === tag);
  if (matches.length > 1) throw new Error(`${matches.length} releases are named ${tag}`);
  if (matches.length === 0) return "create";
  const [release] = matches;
  const existing = release.assets ?? [];
  const stagedNames = Object.keys(staged);
  if (release.draft) {
    const stray = existing.map((asset) => asset.name).filter((name) => !(name in staged));
    if (stray.length > 0) throw new Error(`draft ${tag} has assets this build does not: ${stray.join(", ")}`);
    return "upload";
  }
  const problems = [];
  for (const name of stagedNames) {
    const asset = existing.find((candidate) => candidate.name === name);
    if (!asset) problems.push(`${name} is not in the published release`);
    else if (asset.state !== "uploaded" || typeof asset.digest !== "string") problems.push(`${name} has no digest evidence`);
    else if (asset.digest !== `sha256:${staged[name]}`) problems.push(`${name} differs from the published bytes`);
  }
  for (const asset of existing) {
    if (!(asset.name in staged)) problems.push(`${asset.name} is published but not in this build`);
  }
  if (problems.length > 0) throw new Error(`${tag} is already published and this build does not match it: ${problems.join("; ")}`);
  return "noop";
}

export function stagedDigests(dir) {
  return Object.fromEntries(
    readdirSync(dir).map((name) => [name, createHash("sha256").update(readFileSync(join(dir, name))).digest("hex")]),
  );
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  try {
    const [tag, assetsDir] = process.argv.slice(2);
    if (!tag || !assetsDir) throw new Error("usage: node scripts/release/release-state.mjs <tag> <assets-dir> < releases.json");
    const releases = JSON.parse(readFileSync(0, "utf8")).flat();
    console.log(uploadAction(releases, tag, stagedDigests(resolve(assetsDir))));
  } catch (error) {
    console.error(`::error title=Release state::${error.message}`);
    process.exitCode = 1;
  }
}
