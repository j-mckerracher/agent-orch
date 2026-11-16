import { describe, expect, it } from "vitest";
import { emptySubmitDraft } from "./drafts";
import { buildSubmission } from "./submission";

describe("buildSubmission agent identifiers", () => {
  it.each([
    ["codex", "codex"],
    ["Codex", "codex"],
    ["CLAUDE-CODE", "claude-code"],
    ["Claude Code", "claude-code"],
  ])("normalizes %s to %s", (input, expected) => {
    const result = buildSubmission("/repo", {
      ...emptySubmitDraft(),
      objective: "Implement the task",
      agent: input,
    });
    expect(result.ok && result.submission.agent).toBe(expected);
  });

  it("rejects an agent without a daemon launcher", () => {
    const result = buildSubmission("/repo", {
      ...emptySubmitDraft(),
      objective: "Implement the task",
      agent: "Pi",
    });
    expect(result).toEqual({ ok: false, errors: ["Agent must be codex or claude-code."] });
  });
});
