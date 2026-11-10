import { describe, expect, it } from "vitest";
import { createFixtureSurfaces } from "./fixtures";
import { notOk, observation, okResult } from "./result";
import { createSearchSource } from "./search";
import { NOW, clock } from "./testkit";
import type {
  DecisionQueue,
  DecisionQueueSource,
  Provenance,
  SurfaceResult,
  Workstream,
  WorkstreamSource,
} from "./types";
import { countStates } from "./result";

const daemon = (id: "D9" | "D16", methods: string[]): Provenance => ({ kind: "daemon", id, methods });

function wsSource(result: SurfaceResult<Workstream[]>): WorkstreamSource {
  return {
    provenance: result.provenance,
    workstreams: () => Promise.resolve(result),
    simulateCreate: () => Promise.reject(new Error("unused")),
    create: () => Promise.reject(new Error("unused")),
    update: () => Promise.reject(new Error("unused")),
    addObjective: () => Promise.reject(new Error("unused")),
    reviseObjective: () => Promise.reject(new Error("unused")),
    setGate: () => Promise.reject(new Error("unused")),
    linkTask: () => Promise.reject(new Error("unused")),
    unlinkTask: () => Promise.reject(new Error("unused")),
  };
}
function dqSource(result: SurfaceResult<DecisionQueue>): DecisionQueueSource {
  return {
    provenance: result.provenance,
    queue: () => Promise.resolve(result),
    simulatePostpone: () => Promise.reject(new Error("unused")),
    simulateDismiss: () => Promise.reject(new Error("unused")),
  };
}
const workstream = (id: string, title: string, tasks: Array<[string, string]>): Workstream => ({
  id,
  title,
  summary: "",
  state: "active",
  revision: 1,
  projectIds: [id],
  tasks: tasks.map(([taskId, objective]) => ({ id: taskId, projectId: id, objectiveId: null, objective, state: "Queued", agent: "codex", dependencies: [] })),
  counts: countStates(tasks.map(() => "Queued")),
  objectives: { status: "ok", value: [], observedAt: new Date(NOW).toISOString(), ageMs: 0 },
});
const emptyQueue: DecisionQueue = { items: [], actions: { postpone: { available: false, reason: "r" }, dismiss: { available: false, reason: "r" }, escalate: { available: false, reason: "r" } } };

describe("D15 local search", () => {
  const ws = okResult(daemon("D9", ["projects.list", "tasks.list"]), [workstream("/r/alpha", "alpha", [["t1", "Fix Login Bug"], ["t2", "Write docs"]])], observation(NOW, 5_000));
  const dq = okResult(daemon("D16", ["approvals.list", "tasks.list"]), { ...emptyQueue, items: [{ id: "a1", taskId: "t1", projectId: "/r/alpha", kind: "LocalReviewMerge", candidateCommit: "c", state: "requested" as const, priority: "unknown" as const, latestResponseAt: null }] }, observation(NOW, 9_000));

  it("matches case-insensitively, requires every term, and labels origin real", async () => {
    const search = createSearchSource(wsSource(ws), dqSource(dq), clock);
    const result = await search.search("login FIX");
    expect(result.status === "ok" && result.data.hits.map((h) => [h.kind, h.id, h.origin])).toEqual([["task", "t1", "real"]]);
    const mixed = await search.search("alpha");
    expect(mixed.status === "ok" && mixed.data.hits.map((h) => h.kind)).toEqual(["workstream"]);
    const decision = await search.search("localreviewmerge requested");
    expect(decision.status === "ok" && decision.data.hits.map((h) => h.kind)).toEqual(["decision"]);
  });

  it("mirrors input provenance, takes the oldest observation, and returns no hits for a blank query", async () => {
    const result = await createSearchSource(wsSource(ws), dqSource(dq), clock).search("   ");
    expect(result).toMatchObject({
      status: "ok",
      provenance: { kind: "daemon", id: "D15", methods: ["approvals.list", "projects.list", "tasks.list"] },
      ageMs: 9_000,
      data: { hits: [], truncated: false, skipped: [] },
    });
  });

  it("caps hits and says it truncated", async () => {
    const many = workstream("/r/big", "big", Array.from({ length: 60 }, (_, i) => [`t${i}`, "same objective"] as [string, string]));
    const search = createSearchSource(wsSource(okResult(ws.provenance, [many], observation(NOW))), dqSource(dq), clock);
    const result = await search.search("same");
    expect(result.status === "ok" && [result.data.hits.length, result.data.truncated]).toEqual([50, true]);
  });

  it("lists a failed input as skipped and still searches the other one", async () => {
    const down = notOk<Workstream[]>(ws.provenance, "unavailable", "daemon down");
    const result = await createSearchSource(wsSource(down), dqSource(dq), clock).search("requested");
    expect(result).toMatchObject({
      status: "ok",
      data: { hits: [{ kind: "decision" }], skipped: [{ source: "D9", status: "unavailable", reason: "daemon down" }] },
    });
  });

  it("takes the most actionable status when every input fails, and never reports ok", async () => {
    const mk = (a: "unavailable" | "unsupported" | "unknown", b: "unavailable" | "unsupported" | "unknown") =>
      createSearchSource(
        wsSource(notOk(ws.provenance, a, "ws reason")),
        dqSource(notOk(dq.provenance, b, "dq reason")),
        clock,
      ).search("anything");
    expect(await mk("unsupported", "unavailable")).toMatchObject({ status: "unavailable", reason: "D9: ws reason; D16: dq reason" });
    expect(await mk("unsupported", "unknown")).toMatchObject({ status: "unknown" });
    expect(await mk("unsupported", "unsupported")).toMatchObject({ status: "unsupported" });
  });

  it("is labelled fixture whenever a fixture input feeds it", async () => {
    const fixtures = createFixtureSurfaces({ now: clock });
    expect(fixtures.search.provenance).toMatchObject({ kind: "fixture", id: "D15" });
    const result = await fixtures.search.search("auth");
    expect(result.status === "ok" && result.data.hits.length).toBeGreaterThan(0);
    expect(result.status === "ok" && result.data.hits.every((h) => h.origin === "fixture" && h.id.startsWith("fixture:"))).toBe(true);
  });
});
