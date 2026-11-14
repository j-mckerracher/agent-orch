import { describe, expect, it } from "vitest";
import { DaemonApi, isFixtureId } from "./api";
import { DaemonError } from "./errors";
import { isFixtureLikeId } from "../sources/surfaces/fixtureIds";
import { DaemonSession } from "./session";
import { ALL_CAPABILITIES } from "./types";
import { FakeDaemon } from "../test/fakeDaemon";

/**
 * M7-30: a placeholder id from fixture mode must never reach a daemon mutation. Every `DaemonApi`
 * method that takes an id, or a body containing one, is listed here with the argument that carries
 * it; the coverage test fails when a new method is added without a row.
 */
const FX = "fixture:placeholder-id";
const REAL = "real-id-1";

type Row = { method: keyof DaemonApi; wire: string; bad: (api: DaemonApi) => Promise<unknown>; good: (api: DaemonApi) => Promise<unknown> };

const row = (method: keyof DaemonApi, wire: string, bad: Row["bad"], good: Row["good"]): Row => ({ method, wire, bad, good });

const submission = (project: string, parent: string | null, deps: string[]) => ({
  project_id: project,
  parent_id: parent,
  objective: "o",
  kind: "Implement" as const,
  agent: "a",
  dependencies: deps,
  verification: "None" as const,
  delivery: "LocalReview" as const,
  autonomy: "ApprovalRequired" as const,
  max_retries: 1,
  max_repairs: 1,
});

const ROWS: Row[] = [
  row("tasksGet", "tasks.get", (a) => a.tasksGet(FX), (a) => a.tasksGet(REAL)),
  row("tasksSubmit", "tasks.submit", (a) => a.tasksSubmit("k", submission(FX, null, [])), (a) => a.tasksSubmit("k", submission("/p", null, []))),
  row("tasksCancel", "tasks.cancel", (a) => a.tasksCancel(FX, "k"), (a) => a.tasksCancel(REAL, "k")),
  row("tasksRetry", "tasks.retry", (a) => a.tasksRetry(FX, "k"), (a) => a.tasksRetry(REAL, "k")),
  row("sessionsList", "sessions.list", (a) => a.sessionsList(FX), (a) => a.sessionsList(REAL)),
  row("sessionsGet", "sessions.get", (a) => a.sessionsGet(REAL, FX), (a) => a.sessionsGet(REAL, "s1")),
  row("sessionsEvents", "sessions.events", (a) => a.sessionsEvents(REAL, FX), (a) => a.sessionsEvents(REAL, "s1")),
  row("approvalsResolve", "approvals.resolve", (a) => a.approvalsResolve(FX, "approve", "k"), (a) => a.approvalsResolve(REAL, "approve", "k")),
  row("workspacesDiff", "workspaces.diff", (a) => a.workspacesDiff({ task_id: FX }), (a) => a.workspacesDiff({ task_id: REAL })),
  row(
    "deliveryRequest",
    "delivery.request",
    (a) => a.deliveryRequest({ task_id: REAL, approval_id: FX, verification_run_id: "v", action: "local_review" }, "k"),
    (a) => a.deliveryRequest({ task_id: REAL, approval_id: "ap", verification_run_id: "v", action: "local_review" }, "k"),
  ),
  row("coordinatorSendMessage", "coordinator.send_message", (a) => a.coordinatorSendMessage(FX, "hi", "k"), (a) => a.coordinatorSendMessage("/p", "hi", "k")),
  row("coordinatorPropose", "coordinator.propose", (a) => a.coordinatorPropose(FX, "{}", "k"), (a) => a.coordinatorPropose("/p", "{}", "k")),
  row("coordinatorGet", "coordinator.get", (a) => a.coordinatorGet(FX), (a) => a.coordinatorGet("/p")),
  row(
    "reviewRequestApproval",
    "review.request_approval",
    (a) => a.reviewRequestApproval({ task_id: REAL, verification_run_id: FX, action: "local_review" }, "k"),
    (a) => a.reviewRequestApproval({ task_id: REAL, verification_run_id: "v", action: "local_review" }, "k"),
  ),
  row("reviewEvidence", "review.evidence", (a) => a.reviewEvidence(REAL, FX), (a) => a.reviewEvidence(REAL, "ap")),
  row("reviewRequestChanges", "review.request_changes", (a) => a.reviewRequestChanges(FX, "abc", "fix it", "k"), (a) => a.reviewRequestChanges(REAL, "abc", "fix it", "k")),
  row("workstreamsGet", "workstreams.get", (a) => a.workstreamsGet(FX), (a) => a.workstreamsGet(REAL)),
  row("workstreamsUpdate", "workstreams.update", (a) => a.workstreamsUpdate({ workstream_id: FX, expected_revision: 1, title: "t" }, "k"), (a) => a.workstreamsUpdate({ workstream_id: REAL, expected_revision: 1, title: "t" }, "k")),
  row("workstreamsAddObjective", "workstreams.add_objective", (a) => a.workstreamsAddObjective(FX, { title: "t" }, "k"), (a) => a.workstreamsAddObjective(REAL, { title: "t" }, "k")),
  row("workstreamsReviseObjective", "workstreams.revise_objective", (a) => a.workstreamsReviseObjective({ objective_id: FX, expected_revision: 1, title: "t" }, "k"), (a) => a.workstreamsReviseObjective({ objective_id: REAL, expected_revision: 1, title: "t" }, "k")),
  row(
    "workstreamsSetGate",
    "workstreams.set_gate",
    (a) => a.workstreamsSetGate({ objective_id: REAL, expected_revision: 1, gate_id: "g", state: "verified", evidence_task_id: FX }, "k"),
    (a) => a.workstreamsSetGate({ objective_id: REAL, expected_revision: 1, gate_id: "g", state: "verified", evidence_task_id: "t1" }, "k"),
  ),
  row(
    "workstreamsSetGate",
    "workstreams.set_gate",
    (a) => a.workstreamsSetGate({ objective_id: REAL, expected_revision: 1, gate_id: FX, state: "verified" }, "k"),
    (a) => a.workstreamsSetGate({ objective_id: REAL, expected_revision: 1, gate_id: "g", state: "verified" }, "k"),
  ),
  row("workstreamsLinkTask", "workstreams.link_task", (a) => a.workstreamsLinkTask(REAL, FX, undefined, "k"), (a) => a.workstreamsLinkTask(REAL, "t1", undefined, "k")),
  row("workstreamsUnlinkTask", "workstreams.unlink_task", (a) => a.workstreamsUnlinkTask(FX, REAL, "k"), (a) => a.workstreamsUnlinkTask(REAL, "t1", "k")),
  row("conversationsSend", "conversations.send", (a) => a.conversationsSend({ kind: "workstream", workstream_id: FX }, "hi", "k"), (a) => a.conversationsSend({ kind: "global" }, "hi", "k")),
  row("conversationsGet", "conversations.get", (a) => a.conversationsGet({ kind: "workstream", workstream_id: FX }), (a) => a.conversationsGet({ kind: "global" })),
  row("conversationsCancel", "conversations.cancel", (a) => a.conversationsCancel({ kind: "global" }, FX, "k"), (a) => a.conversationsCancel({ kind: "global" }, "turn-1", "k")),
  row("agentsValidateSelection", "agents.validate_selection", (a) => a.agentsValidateSelection(FX, { model: "m" }), (a) => a.agentsValidateSelection("codex", { model: "m" })),
];

/** Methods that take no id (or only free text / a path), so there is nothing to guard. */
const NO_ID_METHODS = new Set<string>([
  "health",
  "projectsList",
  "projectsRegister",
  "tasksList",
  "approvalsList",
  "queueSnapshot",
  "agentsCapabilities",
  "hostMetrics",
  "usageSummary",
  "workstreamsList",
  "workstreamsCreate",
  "agentsCatalog",
]);

async function api(fake: FakeDaemon) {
  return new DaemonApi(new DaemonSession(await fake.open(ALL_CAPABILITIES), { requestTimeoutMs: 1000 }));
}

function sentMethods(fake: FakeDaemon): string[] {
  return fake.calls.map((c) => c.method).filter((m) => m !== "events.subscribe");
}

describe("DaemonApi fixture-id mutation blocking (M7-30)", () => {
  it("has a row or an explicit no-id entry for every public DaemonApi method", () => {
    const methods = Object.getOwnPropertyNames(DaemonApi.prototype).filter((name) => name !== "constructor");
    const covered = new Set<string>([...ROWS.map((r) => r.method as string), ...NO_ID_METHODS]);
    expect(methods.filter((name) => !covered.has(name))).toEqual([]);
  });

  it.each(ROWS.map((r) => [r.method, r] as const))("%s rejects a fixture id as invalid-params and sends nothing", async (_name, r) => {
    const fake = new FakeDaemon();
    await expect(r.bad(await api(fake))).rejects.toMatchObject({ kind: "invalid-params" });
    expect(sentMethods(fake)).toEqual([]);
  });

  it.each(ROWS.map((r) => [r.method, r] as const))("%s still sends a real id (the guard is not a blanket block)", async (_name, r) => {
    const fake = new FakeDaemon();
    fake.stub(r.wire, {});
    await r.good(await api(fake)).catch(() => undefined);
    expect(sentMethods(fake)).toContain(r.wire);
  });

  it.each(["FIXTURE:x", "  fixture:x", "Fixture:Task-1"])("treats %j as a fixture id regardless of case or leading space", async (id) => {
    const fake = new FakeDaemon();
    await expect((await api(fake)).tasksCancel(id, "k")).rejects.toMatchObject({ kind: "invalid-params" });
    expect(sentMethods(fake)).toEqual([]);
  });

  it("checks every id in a multi-id body (task submission parent and dependencies)", async () => {
    const fake = new FakeDaemon();
    const target = await api(fake);
    await expect(target.tasksSubmit("k", submission("/p", FX, []))).rejects.toMatchObject({ kind: "invalid-params" });
    await expect(target.tasksSubmit("k", submission("/p", null, [REAL, FX]))).rejects.toMatchObject({ kind: "invalid-params" });
    expect(sentMethods(fake)).toEqual([]);
  });
});


const fixtureForms = ["fixture:placeholder-id", "fixture0001", "  FIXTURE0001  "];
const contractWithIds = () => ({
  ...submission("/p", "parent-1", ["dependency-1"]),
  id: "contract-1",
  agent: "codex",
  workstream_id: "ws-1",
  model_selection: { model: "model-1", effort: "high" },
  extension: { taskId: "task-1", nested: { session_ids: ["session-1"] } },
});
const idPaths = [
  ["id"], ["project_id"], ["parent_id"], ["dependencies", "0"], ["agent"],
  ["workstream_id"], ["model_selection", "model"], ["model_selection", "effort"],
  ["extension", "taskId"], ["extension", "nested", "session_ids", "0"],
];
function withIdAt(path: string[], id: string) {
  const contract = contractWithIds();
  let target: Record<string, unknown> = contract;
  for (const key of path.slice(0, -1)) target = target[key] as Record<string, unknown>;
  target[path.at(-1)!] = id;
  return contract;
}

describe("A1b shared mutation boundaries", () => {
  it("shares one fixture predicate with source adapters", () => {
    expect(isFixtureId).toBe(isFixtureLikeId);
    for (const id of fixtureForms) expect(isFixtureId(id)).toBe(true);
    expect(isFixtureId(REAL)).toBe(false);
  });

  it.each(idPaths.map((path) => [path.join("."), path] as const))(
    "tasksSubmit guards the spread field %s before transport", async (_name, path) => {
      const fake = new FakeDaemon();
      const target = await api(fake);
      for (const id of fixtureForms) {
        const failure = target.tasksSubmit("k", withIdAt(path, id));
        await expect(failure).rejects.toBeInstanceOf(DaemonError);
        await expect(failure).rejects.toMatchObject({ kind: "invalid-params" });
      }
      expect(sentMethods(fake)).toEqual([]);
    },
  );

  it.each(idPaths.map((path) => [path.join("."), path] as const))(
    "coordinatorPropose guards nested contract IDs at %s before transport", async (_name, path) => {
      const fake = new FakeDaemon();
      const target = await api(fake);
      for (const id of fixtureForms) {
        const proposal = { objective: "plan", tasks: [{ title: "task", contract: withIdAt(path, id) }] };
        const failure = target.coordinatorPropose("/p", JSON.stringify(proposal), "k");
        await expect(failure).rejects.toBeInstanceOf(DaemonError);
        await expect(failure).rejects.toMatchObject({ kind: "invalid-params" });
      }
      expect(sentMethods(fake)).toEqual([]);
    },
  );

  it("coordinatorPropose refuses malformed JSON without transport", async () => {
    const fake = new FakeDaemon();
    await expect((await api(fake)).coordinatorPropose("/p", "{", "k")).rejects.toMatchObject({ kind: "invalid-params" });
    expect(sentMethods(fake)).toEqual([]);
  });

  it.each(fixtureForms)("reviewRequestChanges refuses fixture candidate %j before transport", async (candidate) => {
    const fake = new FakeDaemon();
    const failure = (await api(fake)).reviewRequestChanges(REAL, candidate, "fix", "k");
    await expect(failure).rejects.toBeInstanceOf(DaemonError);
    await expect(failure).rejects.toMatchObject({ kind: "invalid-params" });
    expect(sentMethods(fake)).toEqual([]);
  });

  it.each(["approve", "reject"] as const)("approvalsResolve refuses prefixed and colon-less approval IDs for %s", async (decision) => {
    const fake = new FakeDaemon();
    const target = await api(fake);
    for (const id of fixtureForms) {
      const failure = target.approvalsResolve(id, decision, "k");
      await expect(failure).rejects.toBeInstanceOf(DaemonError);
      await expect(failure).rejects.toMatchObject({ kind: "invalid-params" });
    }
    expect(sentMethods(fake)).toEqual([]);
  });

  it("forwards real spread IDs and nested proposal IDs without blocking fixture-like free text", async () => {
    const fake = new FakeDaemon();
    fake.stub("coordinator.propose", {});
    const target = await api(fake);
    const contract = {
      ...contractWithIds(), objective: "fixture0001 is free text",
      verification: { Commands: { commands: [["echo", "fixture:free-text"]] } },
    };
    await target.tasksSubmit("k", contract);
    const proposal = JSON.stringify({ objective: "fixture:free-text", tasks: [{ title: "fixture0001", contract }] });
    await target.coordinatorPropose("/p", proposal, "proposal-key");
    expect(sentMethods(fake)).toEqual(["tasks.submit", "coordinator.propose"]);
    expect(fake.callsTo("tasks.submit")[0]?.params).toMatchObject({ contract });
    expect(fake.callsTo("coordinator.propose")[0]?.params).toEqual({ project_id: "/p", proposal, idempotency_key: "proposal-key" });
  });

  it("forwards real candidates and approval IDs for both decisions", async () => {
    const fake = new FakeDaemon();
    fake.stub("review.request_changes", {});
    fake.stub("approvals.resolve", {});
    const target = await api(fake);
    await target.reviewRequestChanges(REAL, "a".repeat(40), "fixture0001 is feedback", "k");
    await target.approvalsResolve(REAL, "approve", "a");
    await target.approvalsResolve(REAL, "reject", "r");
    expect(sentMethods(fake)).toEqual(["review.request_changes", "approvals.resolve", "approvals.resolve"]);
  });
});
