import { describe, expect, it } from "vitest";
import { mkApproval, mkSession, mkTask } from "../../../test/fakeDaemon";
import { layoutGraph, MAX_GRAPH_NODES, textDependencies } from "./graph";
import { deriveLiveGroups, requiredEvidence, type WsTask } from "./model";

const A = "/work/alpha";
const B = "/work/beta";

const ws = (id: string, dependencies: string[] = []): WsTask => ({
  id,
  projectId: A,
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

describe("deriveLiveGroups", () => {
  it("makes one workstream per project with its tasks, workers and counts", () => {
    const t1 = mkTask("t1", A, { state: "Running", sessions: [mkSession("s1", { adapter: "codex", state: "Running" })] });
    const t2 = mkTask("t2", B, { state: "Failed", attempts: [{ attempt_number: 1, state: "Failed" }, { attempt_number: 2, state: "Failed" }] as never });
    const groups = deriveLiveGroups([{ id: A }, { id: B }], [t1, t2], null, []);
    expect(groups.map((group) => group.title)).toEqual(["alpha", "beta"]);
    expect(groups[0]!.tasks[0]!.workers).toEqual([{ sessionId: "s1", adapter: "codex", state: "Running" }]);
    expect(groups[1]!.tasks[0]!.attempt).toBe(2);
    expect(groups[0]!.counts.Running).toBe(1);
    expect(groups[1]!.counts.Failed).toBe(1);
    expect(groups[0]!.objective.status).toBe("unsupported");
  });

  it("gives tasks of an unregistered project their own workstream and keeps empty projects", () => {
    const groups = deriveLiveGroups([{ id: A }], [mkTask("t9", "/work/other")], null, []);
    expect(groups.map((group) => group.id)).toEqual([A, "/work/other"]);
    expect(groups[0]!.tasks).toEqual([]);
  });

  it("marks a task with a pending approval and ignores decided ones", () => {
    const groups = deriveLiveGroups(
      [{ id: A }],
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
