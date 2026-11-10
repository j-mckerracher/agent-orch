import { describe, expect, it } from "vitest";
import { mkSession, mkTask } from "../../../test/fakeDaemon";
import type { RecoveryStatus, SourceResult } from "../../../sources/types";
import { assignmentText, inventoryFromDaemon, inventoryFromPreview } from "./inventoryModel";
import { disconnected, headline, rowsFromDaemon, tabRows, type RecoveryMap } from "./model";

const REPO = "/work/repo";
const recovery = (classification: RecoveryStatus["classification"]): SourceResult<RecoveryStatus> => ({
  state: "ready",
  data: { phase: "p", checks: { process: "a", workspace: "b", delivery: "c" }, classification, reason: "r" },
});

const tasks = [
  mkTask("run", REPO, { state: "Running", sessions: [mkSession("s1"), mkSession("s2", { state: "Aborted" })] }),
  mkTask("ver", REPO, { state: "Verifying" }),
  mkTask("dep", REPO, { state: "Queued", contract: { dependencies: ["run"] } }),
  mkTask("q", REPO, { state: "Created" }),
  mkTask("fail", REPO, { state: "Failed" }),
  mkTask("vfail", REPO, { state: "VerificationFailed" }),
  mkTask("done", REPO, { state: "Succeeded" }),
];

describe("agents model", () => {
  const rows = rowsFromDaemon(tasks, null, []);

  it("derives tab lists that agree with the headline", () => {
    const stats = headline(rows, {});
    expect(tabRows("active", rows, {}).map((r) => r.id)).toEqual(["run", "ver"]);
    expect(stats.runningTasks).toBe(2);
    expect(stats.runningSessions).toBe(1);
    expect(tabRows("queued", rows, {}).map((r) => r.id)).toEqual(["dep", "q"]);
    expect(stats.readyQueued).toBe(1);
    expect(stats.queued).toBe(2);
    expect(stats.queued).toBe(tabRows("queued", rows, {}).length);
    expect(stats.blocked).toBe(1);
    expect(tabRows("failed", rows, {}).map((r) => r.id)).toEqual(["fail", "vfail"]);
  });

  it("counts disconnected only from resolved classifications and never as zero when unknown", () => {
    expect(disconnected(rows, {})).toEqual({ kind: "unknown", reason: "recovery classification is still loading" });
    const unavailable: RecoveryMap = { run: { state: "unavailable", reason: "no rpc" }, ver: recovery("completed") };
    expect(disconnected(rows, unavailable)).toEqual({ kind: "unknown", reason: "no rpc" });
    const known: RecoveryMap = { run: recovery("needs-inspection"), ver: recovery("recoverable") };
    expect(disconnected(rows, known)).toEqual({ kind: "known", value: 1 });
    expect(tabRows("disconnected", rows, known).map((r) => r.id)).toEqual(["run"]);
    expect(disconnected([], {})).toEqual({ kind: "known", value: 0 });
  });
});

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
