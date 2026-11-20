import { describe, expect, it } from "vitest";
import { mkSession, mkTask } from "../../../test/fakeDaemon";
import { TASK_STATES } from "../../format";
import { latestAttempt, rowsFromDaemon, summaryLine, tabRows, TABS } from "./model";

const REPO = "/work/repo";

const tasks = [
  mkTask("run", REPO, { state: "Running", sessions: [mkSession("s1"), mkSession("s2", { state: "Aborted" })] }),
  mkTask("ver", REPO, { state: "Verifying" }),
  mkTask("dep", REPO, { state: "Queued", contract: { dependencies: ["run"] } }),
  mkTask("q", REPO, { state: "Created" }),
  mkTask("fail", REPO, { state: "Failed" }),
  mkTask("vfail", REPO, { state: "VerificationFailed" }),
  mkTask("done", REPO, { state: "Succeeded" }),
  mkTask("gone", REPO, { state: "Cancelled" }),
];

describe("agents model", () => {
  const rows = rowsFromDaemon(tasks, [], []);

  it("has five tabs, and Running is Running and Verifying only", () => {
    expect(TABS.map((tab) => tab.label)).toEqual(["Running", "Waiting", "Queued", "Failed", "Finished"]);
    expect(tabRows("running", rows).map((r) => r.id)).toEqual(["run", "ver"]);
    expect(tabRows("queued", rows).map((r) => r.id)).toEqual(["dep", "q"]);
    expect(tabRows("failed", rows).map((r) => r.id)).toEqual(["fail", "vfail"]);
    expect(tabRows("finished", rows).map((r) => r.id)).toEqual(["done", "gone"]);
  });

  it("lists a Waiting task in the Waiting tab, not as running or queued", () => {
    const waiting = rowsFromDaemon([...tasks, mkTask("wait", REPO, { state: "Waiting" })], [], []);
    expect(tabRows("waiting", waiting).map((r) => r.id)).toEqual(["wait"]);
    expect(tabRows("running", waiting).map((r) => r.id)).toEqual(["run", "ver"]);
    expect(tabRows("queued", waiting).map((r) => r.id)).toEqual(["dep", "q"]);
    expect(summaryLine(waiting)).toBe("2 running · 2 queued · 1 waiting");
  });

  it("writes the one-line summary without a waiting part when nothing waits", () => {
    expect(summaryLine(rows)).toBe("2 running · 2 queued");
    expect(summaryLine([])).toBe("0 running · 0 queued");
  });

  it("lands every task state in exactly one tab", () => {
    for (const state of TASK_STATES) {
      const [row] = rowsFromDaemon([mkTask("t", REPO, { state })], [], []);
      expect({ state, tabs: TABS.filter((tab) => tabRows(tab.id, [row!]).length > 0).length }).toEqual({ state, tabs: 1 });
    }
  });

  it("names the repository and keeps sessions unknown until the session list has loaded", () => {
    expect(rows[0]).toMatchObject({ project: "repo" });
    expect(rowsFromDaemon(tasks, null, []).find((row) => row.id === "fail")?.sessions).toBeNull();
  });

  it("reads the level the task ran with, and the tools its worker lacked", () => {
    const [explicit, older] = rowsFromDaemon(
      [
        mkTask("t", REPO, {
          permission_level: "auto",
          tools_notice: { kind: "lacked_tools", tools: ["Bash"], reason: "denied 3×" },
        }),
        mkTask("o", REPO, { contract: { permission: "full" } }),
      ],
      [],
      [],
    );
    expect(explicit).toMatchObject({ permission: "auto", toolsNotice: { tools: ["Bash"] } });
    expect(older).toMatchObject({ permission: "full", toolsNotice: null });
  });

  it("names the highest attempt number", () => {
    const [row] = rowsFromDaemon(
      [
        mkTask("t", REPO, {
          state: "Failed",
          attempts: [
            { id: "a1", task_id: "t", state: "Failed", attempt_number: 1 },
            { id: "a2", task_id: "t", state: "Failed", attempt_number: 2 },
          ],
        }),
      ],
      [],
      [],
    );
    expect(latestAttempt(row!)).toEqual({ id: "a2", state: "Failed", number: 2 });
  });
});
