import { describe, expect, it } from "vitest";
import { agentDisplayName } from "./agentDisplayNames";

describe("agentDisplayName (DPES-1105)", () => {
  it("labels each daemon agent id with its product name", () => {
    expect(agentDisplayName("codex")).toBe("Codex");
    expect(agentDisplayName("claude-code")).toBe("Claude Code");
    expect(agentDisplayName("github-copilot")).toBe("GitHub Copilot");
    expect(agentDisplayName("pi")).toBe("Pi");
  });

  it("shows an unknown id as-is rather than inventing a label", () => {
    expect(agentDisplayName("some-future-agent")).toBe("some-future-agent");
    expect(agentDisplayName("fixture:codex")).toBe("fixture:codex");
  });

  it("does not resolve inherited object keys to a label", () => {
    expect(agentDisplayName("constructor")).toBe("constructor");
    expect(agentDisplayName("__proto__")).toBe("__proto__");
    expect(agentDisplayName("toString")).toBe("toString");
  });
});
