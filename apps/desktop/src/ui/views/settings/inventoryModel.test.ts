import { describe, expect, it } from "vitest";
import { assignmentText, inventoryFromDaemon, inventoryFromPreview } from "./inventoryModel";

describe("inventory mapping", () => {
  it("keeps installed and launchable exactly as reported", () => {
    expect(assignmentText(true, true)).toBe("Installed and launchable");
    expect(assignmentText(true, false)).toBe("Installed, not launchable");
    expect(assignmentText(false, true)).toBe("Launchable, not detected");
    expect(assignmentText(false, false)).toBe("Not detected");
    const [codex, pi, ghost] = inventoryFromDaemon([
      {
        agent: "codex",
        installed: true,
        launchable: true,
        capabilities: {
          streaming: true,
          interactive_input: false,
          structured_output: true,
          cancellation: true,
          session_resumption: true,
          approvals: false,
          model_selection: false,
        },
      },
      { agent: "pi", installed: true, launchable: false, capabilities: null },
      { agent: "ghost", installed: false, launchable: false, capabilities: null },
    ]);
    expect(codex).toMatchObject({ assignable: true, resume: "yes", interactive: "no" });
    expect(pi).toMatchObject({ assignable: false, resume: "unknown", capabilities: null });
    expect(ghost!.assignable).toBe(false);
  });

  it("does not turn preview features into negatives", () => {
    const [row] = inventoryFromPreview([
      { id: "fixture:x", name: "X", availability: "installed", features: ["structured events"], discoveredAt: "t" },
    ]);
    expect(row).toMatchObject({ resume: "unknown", interactive: "unknown", assignable: true });
  });
});
