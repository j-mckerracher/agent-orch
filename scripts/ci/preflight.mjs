// Windows-only preflight: refuse to run where one directory has two spellings, or where the paths are long.
//
// A `SUBST` drive, a mapped network drive or a junction gives a directory two names. Vitest and Node see the
// name the run was started with (`Z:\run\src`), TypeScript and the file system API resolve the real one
// (`C:\Users\...\run\src`), and the declaration emitter of `tsc -b` then fails with TS2883 because the same
// module has two paths (Priority 12 round 4). Long roots also push `node_modules` and cargo output paths towards
// the 260-character limit. The check is cheap, so the harness fails before it creates anything.
import { realpathSync } from "node:fs";
import { win32 } from "node:path";

// Characters. The prescribed run root `C:\t\<8 hex>` is 12; a root under the temp directory is about 50.
export const MAX_ROOT_LENGTH = 64;

const parts = (path) => win32.resolve(path).split(/[\\/]+/).filter(Boolean);

// The real path of the nearest existing ancestor, with the part that does not exist yet appended unchanged.
const realOf = (path, { realpath, exists }) => {
  let head = win32.resolve(path);
  const tail = [];
  while (!exists(head)) {
    const parent = win32.dirname(head);
    if (parent === head) return win32.resolve(path);
    tail.unshift(win32.basename(head));
    head = parent;
  }
  return win32.join(realpath(head), ...tail);
};

// Case-insensitive, component by component. A component written as an 8.3 short name (`RUNNER~1`) may differ
// from its long form: that is one directory with two spellings in the file system, not an alias drive.
const sameDirectory = (given, real) => {
  const a = parts(given);
  const b = parts(real);
  if (a.length !== b.length) return false;
  return a.every((part, i) => part.toLowerCase() === b[i].toLowerCase() || /~\d/.test(part));
};

export const checkPreflight = ({
  roots, platform = process.platform, maxLength = MAX_ROOT_LENGTH,
  realpath = (p) => realpathSync.native(p), exists = (p) => { try { realpathSync.native(p); return true; } catch { return false; } },
}) => {
  if (platform !== "win32") return [];
  const problems = [];
  for (const [label, given] of Object.entries(roots)) {
    const path = win32.resolve(given);
    if (path.startsWith("\\\\")) {
      problems.push(`${label} is a network or device path (${path}); use a real local directory such as C:\\t\\<8 hex>`);
      continue;
    }
    let real;
    try {
      real = realOf(path, { realpath, exists });
    } catch (error) {
      problems.push(`${label} (${path}) could not be resolved: ${error.message}`);
      continue;
    }
    if (!sameDirectory(path, real)) {
      problems.push(
        `${label} ${path} is really ${real}: a SUBST drive, a mapped network drive or a junction. ` +
        "Use the real directory (for example C:\\t\\<8 hex>); never an alias.",
      );
    }
    if (path.length > maxLength) {
      problems.push(`${label} path is ${path.length} characters (limit ${maxLength}): ${path}. Use a shorter root such as C:\\t\\<8 hex>.`);
    }
  }
  return problems;
};
