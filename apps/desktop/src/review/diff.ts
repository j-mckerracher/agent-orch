// Pure helpers for the unified diff returned by `workspaces.diff`. The diff is
// untrusted text: it is only split and counted here, never interpreted as markup.

export interface DiffFile {
  path: string;
  oldPath: string | null;
  status: "added" | "deleted" | "modified" | "renamed";
  additions: number;
  deletions: number;
  /** The file's section of the unified diff, including its `diff --git` header. */
  text: string;
}

function unquote(token: string): string {
  if (!token.startsWith('"') || !token.endsWith('"') || token.length < 2) return token;
  const bytes: number[] = [];
  const body = token.slice(1, -1);
  const encoder = new TextEncoder();
  for (let i = 0; i < body.length; i++) {
    const ch = body[i];
    if (ch !== "\\") {
      bytes.push(...encoder.encode(ch));
      continue;
    }
    const next = body[++i];
    const octal = /^[0-7]{3}/.exec(body.slice(i, i + 3));
    if (octal) {
      bytes.push(parseInt(octal[0], 8));
      i += 2;
    } else if (next === "t") bytes.push(9);
    else if (next === "n") bytes.push(10);
    else if (next !== undefined) bytes.push(...encoder.encode(next));
  }
  return new TextDecoder().decode(new Uint8Array(bytes));
}

function stripPrefix(token: string, prefix: "a/" | "b/"): string {
  const value = unquote(token);
  return value.startsWith(prefix) ? value.slice(2) : value;
}

function pathsFromHeader(header: string): [string, string] | null {
  const rest = header.slice("diff --git ".length);
  const quoted = /^("(?:[^"\\]|\\.)*") ("(?:[^"\\]|\\.)*"|\S.*)$/.exec(rest);
  if (quoted) return [stripPrefix(quoted[1], "a/"), stripPrefix(quoted[2], "b/")];
  const half = (rest.length - 5) / 2;
  if (Number.isInteger(half) && rest.startsWith("a/") && rest.slice(2 + half, 5 + half) === " b/") {
    const left = rest.slice(2, 2 + half);
    if (left === rest.slice(5 + half)) return [left, left];
  }
  const split = /^a\/(.+) b\/(.+)$/.exec(rest);
  return split ? [split[1], split[2]] : null;
}

export function parseUnifiedDiff(diff: string): DiffFile[] {
  const files: DiffFile[] = [];
  const sections = diff.split(/^(?=diff --git )/m).filter((s) => s.startsWith("diff --git "));
  for (const text of sections) {
    const lines = text.split("\n");
    const header = pathsFromHeader(lines[0]);
    let oldPath = header?.[0] ?? null;
    let path = header?.[1] ?? null;
    let status: DiffFile["status"] = "modified";
    let additions = 0;
    let deletions = 0;
    let inHunk = false;
    for (const line of lines.slice(1)) {
      if (!inHunk) {
        if (line.startsWith("new file mode")) status = "added";
        else if (line.startsWith("deleted file mode")) status = "deleted";
        else if (line.startsWith("rename from ")) {
          status = "renamed";
          oldPath = unquote(line.slice("rename from ".length));
        } else if (line.startsWith("rename to ")) path = unquote(line.slice("rename to ".length));
        else if (line.startsWith("--- ")) {
          const p = line.slice(4);
          if (p !== "/dev/null") oldPath = stripPrefix(p, "a/");
        } else if (line.startsWith("+++ ")) {
          const p = line.slice(4);
          if (p !== "/dev/null") path = stripPrefix(p, "b/");
        } else if (line.startsWith("@@")) inHunk = true;
      } else if (line.startsWith("@@")) {
        continue;
      } else if (line.startsWith("+")) additions++;
      else if (line.startsWith("-")) deletions++;
    }
    if (status === "deleted") path = oldPath ?? path;
    if (path === null) continue;
    files.push({
      path,
      oldPath: status === "renamed" ? oldPath : null,
      status,
      additions,
      deletions,
      text,
    });
  }
  return files;
}

/**
 * Mirror of the daemon's `validate_workspace_diff_path` input rules, applied
 * before a path leaves the UI. The daemon remains the authority.
 */
export function isSafeRelativePath(path: string): boolean {
  if (path.length === 0 || path.includes("\0") || path.includes("\\")) return false;
  if (path.startsWith("-") || path.startsWith("/") || /^[A-Za-z]:/.test(path)) return false;
  return !path.split("/").some((segment) => segment === ".." || segment === ".");
}
