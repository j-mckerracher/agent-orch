import { describe, expect, it } from "vitest";
import { mkApproval, mkSession, mkTask } from "../../../test/fakeDaemon";
import { layoutGraph, MAX_GRAPH_NODES, textDependencies } from "./graph";
import { countStates, measured } from "../../../sources/surfaces/result";
import type { Workstream, WorkstreamTask } from "../../../sources/surfaces/types";
import { deriveLiveGroups, requiredEvidence, type WsTask } from "./model";

const A = "/work/alpha";
const B = "/work/beta";

const ws = (id: string, dependencies: string[] = []): WsTask => ({
  id,
  projectId: A,
  objectiveId: null,
  objective: `do ${id}`,
  state: "Queued",
  agent: "codex",
  dependencies,
  workers: [],
  attempt: null,
  requiredEvidence: "",
  pendingDecision: false,
  openable: true,
});

const link = (id: string, projectId: string, objectiveId: string | null = null, state: WorkstreamTask["state"] = "Queued"): WorkstreamTask => ({
  id,
  projectId,
  objectiveId,
  objective: "",
  state,
  agent: "unknown",
  dependencies: [],
});

const stream = (id: string, tasks: WorkstreamTask[], extra: Partial<Workstream> = {}): Workstream => ({
  id,
  title: `stream ${id}`,
  summary: "",
  state: "active",
  revision: 1,
  projectIds: [...new Set(tasks.map((task) => task.projectId))],
  tasks,
  counts: measured(countStates(tasks.map((task) => task.state)), { observedAt: "2026-01-01T00:00:00.000Z", ageMs: 0 }),
  objectives: { status: "ok", value: [], observedAt: "2026-01-01T00:00:00.000Z", ageMs: 0 },
  ...extra,
});

describe("deriveLiveGroups", () => {
  it("keeps D9 count measurements while joining fresher task records", () => {
    const t1 = mkTask("t1", A, { state: "Running", sessions: [mkSession("s1", { adapter: "codex", state: "Running" })] });
    const t2 = mkTask("t2", B, { state: "Failed", attempts: [{ attempt_number: 1, state: "Failed" }, { attempt_number: 2, state: "Failed" }] as never });
    const { groups, unlinked } = deriveLiveGroups(
      [stream("w1", [link("t1", A, "o1")], { revision: 4, state: "suspended" }), stream("w2", [link("t2", B, null, "Queued")])],
      [t1, t2],
      null,
      [],
    );
    expect(groups.map((group) => [group.id, group.state, group.revision])).toEqual([
      ["w1", "suspended", 4],
      ["w2", "active", 1],
    ]);
    expect(groups[0]!.tasks[0]).toMatchObject({ objectiveId: "o1", state: "Running", objective: "objective of t1" });
    expect(groups[0]!.tasks[0]!.workers).toEqual([{ sessionId: "s1", adapter: "codex", state: "Running" }]);
    expect(groups[1]!.tasks[0]!.attempt).toBe(2);
    expect(groups[1]!.counts).toMatchObject({ status: "ok", value: { Queued: 1, Failed: 0 } });
    expect(groups[0]!.counts).toMatchObject({ status: "ok", value: { Queued: 1, Running: 0 } });
    expect(unlinked).toEqual([]);
  });

  it("keeps D9 counts paired with their observation when AppData retains old tasks", () => {
    const sourceCounts = {
      status: "ok" as const,
      value: countStates(["Running"]),
      observedAt: "2026-01-01T11:59:55.000Z",
      ageMs: 5_000,
    };
    const source = stream("w1", [link("t1", A, null, "Running")], { counts: sourceCounts });
    const retainedTask = mkTask("t1", A, { state: "Failed" });
    const { groups } = deriveLiveGroups([source], [retainedTask], null, []);
    expect(groups[0]!.tasks[0]!.state).toBe("Failed");
    expect(groups[0]!.counts).toEqual(sourceCounts);
  });

  it("falls back to the daemon link when AppData has no record, and reports tasks no workstream links", () => {
    const { groups, unlinked } = deriveLiveGroups(
      [stream("w1", [link("t1", A, null, "Succeeded")]), stream("w2", [])],
      [mkTask("t9", B)],
      null,
      [],
    );
    expect(groups[0]!.tasks[0]).toMatchObject({ id: "t1", state: "Succeeded", workers: [], attempt: null, openable: true });
    expect(groups[1]!.tasks).toEqual([]);
    expect(unlinked.map((task) => [task.id, task.projectId, task.objectiveId])).toEqual([["t9", B, null]]);
  });

  it("does not guess unlinked tasks while a workstream is unread, since its tasks would look unlinked", () => {
    const unread = stream("w2", [], {
      counts: { status: "unknown", reason: "Details could not be read." },
      unreadReason: "Details could not be read.",
      objectives: { status: "unknown", reason: "Details could not be read." },
    });
    const { groups, unlinked, unlinkedKnown } = deriveLiveGroups(
      [stream("w1", [link("t1", A)]), unread],
      [mkTask("t1", A), mkTask("t2", B)],
      null,
      [],
    );
    expect(groups.map((group) => group.unreadReason)).toEqual([null, "Details could not be read."]);
    expect(unlinkedKnown).toBe(false);
    expect(groups[1]!.counts).toMatchObject({ status: "unknown", reason: "Details could not be read." });
    expect(groups[1]!.counts).not.toHaveProperty("value");
    expect(unlinked).toEqual([]);
  });

  it("reports no unlinked tasks while the task list is unloaded, and passes objectives through unchanged", () => {
    const objectives = { status: "unknown" as const, reason: "later" };
    const { groups, unlinked } = deriveLiveGroups([stream("w1", [], { objectives })], null, null, null);
    expect(unlinked).toEqual([]);
    expect(groups[0]!.objectives).toBe(objectives);
  });

  it("marks a task with a pending approval and ignores decided ones", () => {
    const { groups } = deriveLiveGroups(
      [stream("w1", [link("t1", A), link("t2", A)])],
      [mkTask("t1", A), mkTask("t2", A)],
      null,
      [mkApproval("a1", "t1"), mkApproval("a2", "t2", "approved")],
    );
    expect(groups[0]!.tasks.map((task) => task.pendingDecision)).toEqual([true, false]);
  });
});

describe("requiredEvidence", () => {
  const byId = new Map<string, { state: "Succeeded" | "Queued" }>([
    ["done", { state: "Succeeded" }],
    ["waiting", { state: "Queued" }],
  ]);
  const evidence = (state: Parameters<typeof requiredEvidence>[0]["state"], dependencies: string[] = [], approvals = []) =>
    requiredEvidence({ id: "t", state, dependencies }, byId, approvals);

  it("names unmet predecessors, treating unknown ones as unmet", () => {
    expect(evidence("Queued", ["done", "waiting", "ghost"])).toBe("Predecessors to succeed: waiting, ghost");
    expect(evidence("Created", ["done"])).toBe("None until dispatched");
  });

  it("derives from the state otherwise", () => {
    expect(evidence("Running")).toMatch(/running attempt/);
    expect(evidence("Verifying")).toMatch(/Verification result/);
    expect(evidence("Succeeded")).toMatch(/None: task succeeded/);
    expect(evidence("VerificationFailed")).toMatch(/Repair or retry/);
    expect(evidence("Failed")).toMatch(/Retry or investigation/);
    expect(evidence("Cancelled")).toMatch(/cancelled/);
  });

  it("lets a pending approval take precedence and says when approvals are not loaded", () => {
    expect(requiredEvidence({ id: "t1", state: "Verifying", dependencies: [] }, byId, [mkApproval("a", "t1")])).toBe(
      "Operator decision on LocalReviewMerge approval for candidate abc123",
    );
    expect(requiredEvidence({ id: "t1", state: "Running", dependencies: [] }, byId, null)).toMatch(/approval state not loaded/);
  });
});

describe("layoutGraph", () => {
  it("layers predecessors above their consumers and draws one edge per dependency", () => {
    const layout = layoutGraph([ws("a"), ws("b", ["a"]), ws("c", ["a", "b"]), ws("lonely")]);
    const layer = (id: string) => layout.nodes.find((node) => node.task.id === id)!.layer;
    expect([layer("a"), layer("b"), layer("c")]).toEqual([0, 1, 2]);
    expect(layout.edges).toHaveLength(3);
    expect(layout.nodes.some((node) => node.task.id === "lonely")).toBe(false);
    expect(layout.connected).toBe(3);
  });

  it("draws nothing when no dependency is known and ignores dependencies outside the workstream", () => {
    const layout = layoutGraph([ws("a", ["elsewhere"]), ws("b")]);
    expect(layout.nodes).toEqual([]);
    expect(layout.height).toBe(0);
  });

  it("survives a cycle", () => {
    const layout = layoutGraph([ws("a", ["b"]), ws("b", ["a"])]);
    expect(layout.cyclic).toBe(true);
    expect(layout.nodes).toHaveLength(2);
  });

  it("caps the drawing and reports what it left out", () => {
    const tasks = [ws("root"), ...Array.from({ length: 40 }, (_, index) => ws(`n${index}`, ["root"]))];
    const layout = layoutGraph(tasks);
    expect(layout.nodes).toHaveLength(MAX_GRAPH_NODES);
    expect(layout.omitted).toBe(41 - MAX_GRAPH_NODES);
  });
});

describe("textDependencies", () => {
  it("lists in-workstream predecessors, external ones and consumers", () => {
    const rows = textDependencies([ws("a"), ws("b", ["a", "ghost"])]);
    expect(rows[1]!.predecessors.map((task) => task.id)).toEqual(["a"]);
    expect(rows[1]!.external).toEqual(["ghost"]);
    expect(rows[0]!.consumers.map((task) => task.id)).toEqual(["b"]);
  });
});
