import { describe, expect, it } from "vitest";
import { mkSession, mkTask } from "../../../test/fakeDaemon";
import { assignmentText, inventoryFromDaemon, inventoryFromPreview } from "./inventoryModel";
import { TASK_STATES } from "../../format";
import { failedSessions, failedWithoutSession, headline, isFailed, latestAttempt, rowsFromDaemon, sessionsKnown, tabRows } from "./model";

const REPO = "/work/repo";

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
  const rows = rowsFromDaemon(tasks, [], []);

  it("reports sessions as unknown, not zero or none, until the session list has loaded", () => {
    const unknown = rowsFromDaemon(tasks, null, []);
    expect(sessionsKnown(unknown)).toBe(false);
    expect(sessionsKnown(rows)).toBe(true);
    expect(headline(unknown).runningSessions).toBeNull();
    expect(headline(unknown).runningTasks).toBe(2);
    expect(failedWithoutSession(unknown)).toEqual([]);
    expect(unknown.find((row) => row.id === "fail")?.sessions).toBeNull();
  });

  it("derives tab lists that agree with the headline", () => {
    const stats = headline(rows);
    expect(tabRows("active", rows).map((r) => r.id)).toEqual(["run", "ver"]);
    expect(stats.runningTasks).toBe(2);
    expect(stats.runningSessions).toBe(1);
    expect(tabRows("queued", rows).map((r) => r.id)).toEqual(["dep", "q"]);
    expect(stats.readyQueued).toBe(1);
    expect(stats.queued).toBe(2);
    expect(stats.queued).toBe(tabRows("queued", rows).length);
    expect(stats.blocked).toBe(1);
  });

  it("lists a Waiting task in the Waiting tab and counts it in the headline, not as running or queued", () => {
    const waiting = rowsFromDaemon([...tasks, mkTask("wait", REPO, { state: "Waiting" })], [], []);
    expect(tabRows("waiting", waiting).map((r) => r.id)).toEqual(["wait"]);
    expect(tabRows("active", waiting).map((r) => r.id)).toEqual(["run", "ver"]);
    expect(tabRows("queued", waiting).map((r) => r.id)).toEqual(["dep", "q"]);
    expect(headline(waiting).waiting).toBe(1);
    expect(headline(waiting).runningTasks).toBe(2);
    expect(headline(waiting).queued).toBe(2);
  });

  it("lands every task state in at least one tab", () => {
    for (const state of TASK_STATES) {
      const [row] = rowsFromDaemon([mkTask("t", REPO, { state })], [], []);
      const listed = (["active", "waiting", "queued", "finished"] as const).some((tab) => tabRows(tab, [row!]).length > 0);
      expect({ state, listed: listed || isFailed(row!) }).toEqual({ state, listed: true });
    }
  });

  it("counts failed sessions, not tasks, and keeps failed tasks with no session apart", () => {
    expect(failedSessions(rows).map((f) => [f.row.id, f.session.id])).toEqual([["run", "s2"]]);
    expect(failedWithoutSession(rows).map((r) => r.id)).toEqual(["fail", "vfail"]);
  });

  it("links a failed session to the attempt the daemon names for it", () => {
    const linked = rowsFromDaemon(
      [
        mkTask("t", REPO, {
          state: "Failed",
          attempts: [{ id: "a1", task_id: "t", state: "Failed", attempt_number: 2 }],
          sessions: [mkSession("s9", { state: "Exited", attempt_id: "a1" })],
        }),
      ],
      [],
      [],
    );
    expect(failedSessions(linked).map((f) => [f.session.id, f.attempt])).toEqual([
      ["s9", { id: "a1", state: "Failed", number: 2 }],
    ]);
    expect(failedWithoutSession(linked)).toEqual([]);
    expect(latestAttempt(linked[0]!)).toEqual({ id: "a1", state: "Failed", number: 2 });
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
