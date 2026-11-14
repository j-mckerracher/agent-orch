import { describe, expect, it } from "vitest";
import { isSafeRelativePath, parseUnifiedDiff } from "./diff";
import { DIFF } from "./testing";

describe("parseUnifiedDiff", () => {
  it("derives files, status and counts from a unified diff", () => {
    const files = parseUnifiedDiff(DIFF);
    expect(files.map((f) => [f.path, f.status, f.additions, f.deletions])).toEqual([
      ["src/a.rs", "modified", 1, 0],
      ["old.txt", "deleted", 0, 1],
    ]);
    expect(files[0].text.startsWith("diff --git a/src/a.rs")).toBe(true);
  });

  it("handles renames, quoted names and paths with spaces", () => {
    const diff = [
      "diff --git a/old name.txt b/new name.txt",
      "similarity index 100%",
      "rename from old name.txt",
      "rename to new name.txt",
      'diff --git "a/caf\\303\\251.txt" "b/caf\\303\\251.txt"',
      "new file mode 100644",
      "--- /dev/null",
      '+++ "b/caf\\303\\251.txt"',
      "@@ -0,0 +1 @@",
      "+hi",
      "diff --git a/bin b/bin",
      "Binary files a/bin and b/bin differ",
    ].join("\n");
    const files = parseUnifiedDiff(diff);
    expect(files.map((f) => [f.path, f.oldPath, f.status])).toEqual([
      ["new name.txt", "old name.txt", "renamed"],
      ["café.txt", null, "added"],
      ["bin", null, "modified"],
    ]);
  });

  it("does not count --- / +++ content lines inside hunks as headers", () => {
    const diff = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n--- old\n+++ new\n";
    const [file] = parseUnifiedDiff(diff);
    expect([file.path, file.additions, file.deletions]).toEqual(["x", 1, 1]);
  });

  it("returns no files for empty or non-diff text", () => {
    expect(parseUnifiedDiff("")).toEqual([]);
    expect(parseUnifiedDiff("not a diff")).toEqual([]);
  });
});

describe("isSafeRelativePath", () => {
  it.each(["src/a.rs", "a", "dir/sub/file.txt"])("accepts %s", (p) => expect(isSafeRelativePath(p)).toBe(true));
  it.each(["", "/etc/passwd", "../x", "a/../b", "-flag", "C:\\x", "C:/x", "a\\b", "./a", "a\0b"])(
    "rejects %j",
    (p) => expect(isSafeRelativePath(p)).toBe(false),
  );
});
