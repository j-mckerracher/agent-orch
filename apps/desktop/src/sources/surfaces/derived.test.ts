import { afterEach, describe, expect, it } from "vitest";
import { DaemonError } from "../../daemon/errors";
import { FakeDaemon, mkApproval, mkTask } from "../../test/fakeDaemon";
import { ALPHA, BETA, connect, populate } from "./testkit";

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
  it("groups tasks by project, counts every state, and says objectives are unsupported", async () => {
    const fake = populate(new FakeDaemon());
    fake.tasks.push(mkTask("t-orphan", "/repos/gamma", { state: "Failed" }));
    const s = await live(fake);
    const result = await s.workstreams.workstreams();
    expect(result).toMatchObject({
      status: "ok",
      provenance: { kind: "daemon", id: "D9", methods: ["projects.list", "tasks.list"] },
      ageMs: 0,
    });
    if (result.status !== "ok") return;
    expect(result.data.map((w) => [w.id, w.title, w.grouping])).toEqual([
      [ALPHA, "alpha", "project"],
      [BETA, "beta", "project"],
      ["/repos/gamma", "gamma", "project"],
    ]);
    expect(result.data[0]!.counts).toMatchObject({ Running: 1, Queued: 1, Succeeded: 0 });
    expect(result.data[1]!.counts).toMatchObject({ Queued: 1, Succeeded: 1 });
    expect(result.data[2]!.counts.Failed).toBe(1);
    expect(result.data[0]!.objective.status).toBe("unsupported");
  });

  it("lists a project with no tasks as a workstream with all-zero counts", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: ALPHA }];
    const result = await (await live(fake)).workstreams.workstreams();
    expect(result.status === "ok" && result.data).toMatchObject([{ id: ALPHA, tasks: [] }]);
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

  it("answers a global transcript as unsupported and never calls the daemon", async () => {
    const fake = populate(new FakeDaemon());
    const result = await (await live(fake)).conversation.transcript({ kind: "global" });
    expect(result.status).toBe("unsupported");
    expect(fake.callsTo("coordinator.get")).toHaveLength(0);
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

  it("rejects a global send without calling the daemon", async () => {
    const fake = populate(new FakeDaemon());
    const s = await live(fake);
    await expect(s.conversation.send({ kind: "global" }, "hi")).rejects.toMatchObject({ kind: "method-unavailable" });
    expect(fake.callsTo("coordinator.send_message")).toHaveLength(0);
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

describe("derived D12 (no daemon data)", () => {
  it("answers unsupported with daemon provenance that reads no method, even when connected", async () => {
    const fake = populate(new FakeDaemon());
    const s = await live(fake);
    const before = fake.calls.length;
    expect(await s.usage.usage()).toMatchObject({ status: "unsupported", provenance: { kind: "daemon", methods: [] } });
    expect(fake.calls.length).toBe(before);
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
