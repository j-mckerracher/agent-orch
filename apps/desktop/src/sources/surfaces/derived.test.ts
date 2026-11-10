import { afterEach, describe, expect, it } from "vitest";
import { DaemonError } from "../../daemon/errors";
import { FakeDaemon, mkApproval, mkTask } from "../../test/fakeDaemon";
import { stubConversation } from "../../test/m724Stubs";
import { ALPHA, BETA, WORKSTREAM_STATE, connect, populate } from "./testkit";

const stops: Array<() => void> = [];
afterEach(() => {
  while (stops.length) stops.pop()!();
});

async function live(fake: FakeDaemon, start = true) {
  const connected = await connect(fake, start);
  stops.push(connected.stop);
  return connected.surfaces;
}

const asKind = (kind: string) => expect.objectContaining({ kind });

describe("derived D9 workstreams", () => {
  it("maps daemon workstreams, objectives, gates and links, joined with task records", async () => {
    const fake = populate(new FakeDaemon());
    const s = await live(fake);
    const result = await s.workstreams.workstreams();
    expect(result).toMatchObject({
      status: "ok",
      provenance: { kind: "daemon", id: "D9", methods: ["workstreams.list", "workstreams.get", "tasks.list"] },
      ageMs: 0,
    });
    if (result.status !== "ok") return;
    expect(result.data.map((w) => [w.id, w.title, w.state, w.revision])).toEqual([
      ["ws-1", "Session hardening", "active", 2],
      ["ws-2", "Docs refresh", "suspended", 1],
    ]);
    const first = result.data[0]!;
    expect(first.summary).toBe("Make sessions durable");
    expect(first.projectIds).toEqual([ALPHA, BETA]);
    expect(first.tasks.map((t) => [t.id, t.projectId, t.objectiveId, t.state])).toEqual([
      ["t-run", ALPHA, "obj-1", "Running"],
      ["t-wait", ALPHA, "obj-1", "Queued"],
      ["t-done", BETA, null, "Succeeded"],
    ]);
    expect(first.tasks[1]).toMatchObject({ objective: "Document the session model", dependencies: ["t-run"] });
    expect(first.counts).toMatchObject({ Running: 1, Queued: 1, Succeeded: 1, Failed: 0 });
    expect(first.objectives).toMatchObject({
      status: "ok",
      value: [
        {
          id: "obj-1",
          title: "Durable sessions",
          description: "Sessions survive restarts",
          revision: 3,
          gates: [
            { id: "gate-1", label: "Restart test passes", state: "verified", evidenceTaskId: "t-done" },
            { id: "gate-2", label: "Docs updated", state: "pending", evidenceTaskId: null },
          ],
        },
      ],
    });
    // The empty workstream is a real, known-empty answer, not an unsupported one.
    expect(result.data[1]).toMatchObject({ tasks: [], projectIds: [], objectives: { status: "ok", value: [] } });
    expect(result.data[1]!.counts.Running).toBe(0);
  });

  it("keeps the daemon's task state and marks a linked task that tasks.list no longer returns", async () => {
    const fake = populate(new FakeDaemon());
    fake.tasks = fake.tasks.filter((task) => task.contract.id !== "t-wait");
    WORKSTREAM_STATE.get(fake)!.links.push({ task_id: "t-ghost", workstream_id: "ws-2", objective_id: null });
    const s = await live(fake);
    const result = await s.workstreams.workstreams();
    const ws1 = result.status === "ok" ? result.data[0]! : null;
    expect(ws1?.tasks.map((t) => [t.id, t.objective, t.agent, t.state])).toEqual([
      ["t-run", "objective of t-run", "fake-agent", "Running"],
      ["t-wait", "", "unknown", "Created"],
      ["t-done", "objective of t-done", "fake-agent", "Succeeded"],
    ]);
    const ws2 = result.status === "ok" ? result.data[1]! : null;
    expect(ws2?.tasks).toEqual([
      { id: "t-ghost", projectId: "", objectiveId: null, objective: "", state: "Created", agent: "unknown", dependencies: [] },
    ]);
  });

  it("reports a failed read as unavailable with daemon provenance, never as fixture data", async () => {
    const fake = populate(new FakeDaemon());
    // Every detail read fails: nothing is known, so the read fails instead of listing unknown rows.
    fake.failNext("workstreams.get", { code: -32603, message: "boom" }, 2);
    const result = await (await live(fake)).workstreams.workstreams();
    expect(result).toMatchObject({ status: "unavailable", provenance: { kind: "daemon", id: "D9" } });
    expect(result).not.toHaveProperty("data");
  });

  it("keeps the readable workstreams when one detail read fails, and marks that row unknown", async () => {
    const fake = populate(new FakeDaemon());
    fake.failNext("workstreams.get", { code: -32603, message: "boom" });
    const result = await (await live(fake)).workstreams.workstreams();
    if (result.status !== "ok") throw new Error(`expected ok, got ${result.status}`);
    expect(result.data.map((item) => item.unreadReason !== undefined)).toEqual([true, false]);
    expect(result.data[0]!.objectives.status).toBe("unknown");
  });
});

describe("derived D10 conversation", () => {
  it("reads the project transcript from coordinator.get and expects no replies", async () => {
    const s = await live(populate(new FakeDaemon()));
    const result = await s.conversation.transcript({ kind: "project", projectId: ALPHA });
    expect(result).toMatchObject({
      status: "ok",
      data: { repliesExpected: false, truncated: false, entries: [{ id: "c-1", role: "user", text: "hello", kind: "message" }] },
    });
  });

  it("flags a transcript that hit the paging limit as truncated", async () => {
    const fake = new FakeDaemon();
    let sequence = 0;
    fake.stub("coordinator.get", (params: Record<string, unknown>) => {
      sequence += 1;
      return {
        project_id: params.project_id,
        entries: [{ sequence, id: `c-${sequence}`, project_id: ALPHA, kind: "message", author: "user", body: "x", validation: null, created_at: "2026-01-01T00:00:00Z" }],
        more: true,
        next_after_sequence: sequence,
      };
    });
    const result = await (await live(fake)).conversation.transcript({ kind: "project", projectId: ALPHA });
    expect(result.status === "ok" && result.data.truncated).toBe(true);
    expect(fake.callsTo("coordinator.get")).toHaveLength(50);
  });

  it("answers a global transcript from conversations.get and never touches the project coordinator", async () => {
    const fake = populate(new FakeDaemon());
    stubConversation(fake);
    const result = await (await live(fake)).conversation.transcript({ kind: "global" });
    expect(result).toMatchObject({ status: "ok", data: { entries: [], turn: null } });
    expect(fake.callsTo("conversations.get")).toHaveLength(1);
    expect(fake.callsTo("coordinator.get")).toHaveLength(0);
  });

  it("says a daemon without conversations.get does not support a global transcript", async () => {
    const result = await (await live(populate(new FakeDaemon()))).conversation.transcript({ kind: "global" });
    expect(result.status).toBe("unsupported");
  });

  it("records a note through coordinator.send_message and reuses the key after an indeterminate failure", async () => {
    const fake = populate(new FakeDaemon());
    fake.failNext("coordinator.send_message", "hang");
    const s = await live(fake);
    await expect(s.conversation.send({ kind: "project", projectId: ALPHA }, "ship it")).rejects.toMatchObject({ kind: "timeout" });
    const receipt = await s.conversation.send({ kind: "project", projectId: ALPHA }, "ship it");
    expect(receipt).toMatchObject({ simulated: false, entryId: "c-2", provenance: { kind: "daemon", id: "D10" } });
    const keys = fake.callsTo("coordinator.send_message").map((call) => (call.params as { idempotency_key: string }).idempotency_key);
    expect(keys).toHaveLength(2);
    expect(keys[0]).toBe(keys[1]);
  });

  it("sends a global message through conversations.send, not coordinator.send_message", async () => {
    const fake = populate(new FakeDaemon());
    stubConversation(fake);
    const s = await live(fake);
    const receipt = await s.conversation.send({ kind: "global" }, "hi");
    expect(receipt).toMatchObject({ simulated: false, turn: { state: "failed", errorCode: "runtime_unavailable" } });
    expect(receipt.notice).toMatch(/runtime unavailable/i);
    expect(fake.callsTo("coordinator.send_message")).toHaveLength(0);
    expect(fake.callsTo("conversations.send")).toHaveLength(1);
  });
});

describe("derived D11 activity", () => {
  it("splits running and queued, explains waiting honestly, and lists only requested approvals", async () => {
    const result = await (await live(populate(new FakeDaemon()))).activity.activity();
    expect(result).toMatchObject({ status: "ok", provenance: { methods: ["queue.snapshot", "tasks.list", "approvals.list"] } });
    if (result.status !== "ok") return;
    expect(result.data.running.map((e) => [e.taskId, e.waiting])).toEqual([["t-run", null]]);
    const queued = Object.fromEntries(result.data.queued.map((e) => [e.taskId, e.waiting]));
    expect(queued["t-wait"]).toMatchObject({ reason: "dependency" });
    expect(queued["t-free"]).toMatchObject({ reason: "unknown" });
    expect(result.data.attention).toEqual([
      { approvalId: "a-1", taskId: "t-run", projectId: ALPHA, kind: "LocalReviewMerge", priority: "unknown" },
    ]);
    expect(result.data.slots).toMatchObject({ status: "ok", value: { used: 1, limit: 2 } });
    expect(result.data.upNext).toMatchObject({ status: "ok", value: "t-free" });
    expect(result.data.dispatchPaused.status).toBe("unsupported");
  });

  it("treats a dependency on an unlisted task as unmet", async () => {
    const fake = new FakeDaemon();
    fake.tasks = [mkTask("t1", ALPHA, { state: "Queued", contract: { dependencies: ["ghost"] } })];
    const result = await (await live(fake)).activity.activity();
    expect(result.status === "ok" && result.data.queued[0]!.waiting).toMatchObject({ reason: "dependency" });
  });
});

describe("derived D14 models", () => {
  it("maps agents.capabilities and never claims a model list", async () => {
    const result = await (await live(populate(new FakeDaemon()))).models.catalog();
    expect(result.status === "ok" && result.data.providers).toMatchObject([
      { id: "codex", installed: true, launchable: true, modelSelection: "supported", models: { status: "unsupported" }, efforts: { status: "unsupported" } },
      { id: "pi", installed: false, modelSelection: "unknown", models: { status: "unsupported" } },
    ]);
  });
});

describe("derived D16 decisions", () => {
  it("joins approvals to projects and offers no extra actions", async () => {
    const result = await (await live(populate(new FakeDaemon()))).decisions.queue();
    expect(result.status === "ok" && result.data.items).toMatchObject([
      { id: "a-1", projectId: ALPHA, state: "requested", priority: "unknown", latestResponseAt: null },
      { id: "a-2", projectId: BETA, state: "approved" },
    ]);
    if (result.status !== "ok") return;
    for (const action of Object.values(result.data.actions)) expect(action.available).toBe(false);
  });

  it("keeps an approval whose task is unknown with a null project", async () => {
    const fake = new FakeDaemon();
    fake.approvals = [mkApproval("a-x", "ghost-task")];
    const result = await (await live(fake)).decisions.queue();
    expect(result.status === "ok" && result.data.items[0]!.projectId).toBeNull();
  });
});

describe("fixture ids cannot reach a real mutation", () => {
  it("refuses a fixture: project id before any request, connected or not", async () => {
    for (const start of [true, false]) {
      const fake = populate(new FakeDaemon());
      const s = await live(fake, start);
      await expect(s.conversation.send({ kind: "project", projectId: "fixture:project-alpha" }, "x")).rejects.toMatchObject({
        kind: "invalid-params",
      });
      expect(fake.callsTo("coordinator.send_message")).toHaveLength(0);
    }
  });

  it("answers a fixture: transcript as unsupported without calling coordinator.get", async () => {
    const fake = populate(new FakeDaemon());
    const result = await (await live(fake)).conversation.transcript({ kind: "project", projectId: "fixture:project-alpha" });
    expect(result.status).toBe("unsupported");
    expect(fake.callsTo("coordinator.get")).toHaveLength(0);
  });

  it("rejects every simulated action on derived adapters and sends nothing", async () => {
    const fake = populate(new FakeDaemon());
    const s = await live(fake);
    const before = fake.calls.length;
    const attempts = [
      s.workstreams.simulateCreate("x"),
      s.activity.simulatePauseDispatch(),
      s.activity.simulateStop("fixture:task-auth-2"),
      s.decisions.simulatePostpone("fixture:approval-1"),
      s.decisions.simulateDismiss("fixture:approval-1"),
    ];
    for (const attempt of attempts) {
      await expect(attempt).rejects.toBeInstanceOf(DaemonError);
      await expect(attempt).rejects.toMatchObject({ kind: "method-unavailable" });
    }
    expect(fake.calls.length).toBe(before);
  });
});

describe("a failed live call never falls back to fixtures", () => {
  it("returns non-ok daemon results with no data when the daemon errors", async () => {
    const fake = populate(new FakeDaemon());
    for (const method of ["projects.list", "tasks.list", "approvals.list", "queue.snapshot", "agents.capabilities", "coordinator.get"]) {
      fake.failNext(method, { code: -32603, message: "boom" }, 10);
    }
    const s = await live(fake);
    const results = [
      await s.workstreams.workstreams(),
      await s.conversation.transcript({ kind: "project", projectId: ALPHA }),
      await s.activity.activity(),
      await s.models.catalog(),
      await s.search.search("task"),
      await s.decisions.queue(),
    ];
    for (const result of results) {
      expect(result.status).toBe("unavailable");
      expect(result.provenance.kind).toBe("daemon");
      expect(result).not.toHaveProperty("data");
      expect(JSON.stringify(result)).not.toMatch(/fixture/i);
    }
  });

  it("says unavailable when disconnected", async () => {
    const s = await live(populate(new FakeDaemon()), false);
    const result = await s.workstreams.workstreams();
    expect(result).toMatchObject({ status: "unavailable", provenance: { kind: "daemon", id: "D9" } });
  });

  it("recovers to real data after a failure, still without fixtures", async () => {
    const fake = populate(new FakeDaemon());
    fake.failNext("tasks.list", { code: -32603, message: "boom" });
    const s = await live(fake);
    expect((await s.workstreams.workstreams()).status).toBe("unavailable");
    const second = await s.workstreams.workstreams();
    expect(second.status).toBe("ok");
    expect(JSON.stringify(second)).not.toMatch(/fixture/i);
  });

  it("maps a missing method to unsupported, not to fixtures or empty", async () => {
    const fake = new FakeDaemon();
    const result = await (await live(fake)).models.catalog();
    expect(result).toMatchObject({ status: "unsupported", provenance: asKind("daemon") });
  });
});
