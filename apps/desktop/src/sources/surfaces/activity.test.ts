import { afterEach, describe, expect, it } from "vitest";
import { ALL_CAPABILITIES } from "../../daemon/types";
import { FakeDaemon, mkApproval, mkTask } from "../../test/fakeDaemon";
import { fakeQueueSnapshot, DEFAULT_QUEUE_SETTINGS } from "../../test/queueSnapshot";
import { ALPHA, BETA, connect } from "./testkit";
import type { ActivitySnapshot } from "./types";

const stops: Array<() => void> = [];
afterEach(() => {
  while (stops.length) stops.pop()!();
});

async function activityOf(fake: FakeDaemon) {
  const connected = await connect(fake);
  stops.push(connected.stop);
  return connected.surfaces.activity.activity();
}

async function okActivity(fake: FakeDaemon): Promise<ActivitySnapshot> {
  const result = await activityOf(fake);
  if (result.status !== "ok") throw new Error(`expected ok, got ${result.status}: ${"reason" in result ? result.reason : ""}`);
  return result.data;
}

function withSnapshot(fake: FakeDaemon, patch: (wire: ReturnType<typeof fakeQueueSnapshot>) => unknown) {
  fake.stub("queue.snapshot", () => patch(fakeQueueSnapshot(fake.tasks, fake.queue)));
  return fake;
}

describe("live D11 activity adapter", () => {
  it("reports real slot limits and capacity waits when more tasks are queued than slots", async () => {
    const fake = new FakeDaemon();
    fake.tasks = [
      mkTask("r1", ALPHA, { state: "Running" }),
      mkTask("r2", ALPHA, { state: "Running" }),
      mkTask("q1", ALPHA, { state: "Queued" }),
      mkTask("q2", BETA, { state: "Queued" }),
      mkTask("q3", BETA, { state: "Queued", contract: { dependencies: ["r1"] } }),
    ];
    const data = await okActivity(fake);
    expect(data.slots).toMatchObject({ status: "ok", value: { used: 2, limit: 2 } });
    expect(data.running.map((entry) => entry.taskId)).toEqual(["r1", "r2"]);
    const queued = Object.fromEntries(data.queued.map((entry) => [entry.taskId, entry.waiting]));
    expect(queued.q1).toMatchObject({ reason: "capacity", detail: "All 2 worker slots are in use." });
    expect(queued.q2).toMatchObject({ reason: "capacity" });
    expect(queued.q3).toMatchObject({ reason: "dependency", detail: "Waiting for r1." });
    expect(data.queued.map((entry) => entry.taskId)).toEqual(["q1", "q2", "q3"]);
    expect(data.upNext).toMatchObject({ status: "ok", value: "q1" });
  });

  it("names the per-project limit and keeps the global limit as the slot limit", async () => {
    const fake = new FakeDaemon();
    fake.queue = { ...DEFAULT_QUEUE_SETTINGS, max_concurrent_workers: 4, max_workers_per_project: 1 };
    fake.tasks = [mkTask("r1", ALPHA, { state: "Running" }), mkTask("q1", ALPHA, { state: "Queued" })];
    const data = await okActivity(fake);
    expect(data.slots).toMatchObject({ status: "ok", value: { used: 1, limit: 4 } });
    expect(data.queued[0]!.waiting).toMatchObject({ reason: "capacity" });
    expect(data.queued[0]!.waiting!.detail).toMatch(/per-project limit/);
  });

  it("maps every daemon waiting reason without guessing", async () => {
    const fake = new FakeDaemon();
    fake.tasks = ["a", "b", "c", "d", "e", "f", "g"].map((id) => mkTask(id, ALPHA, { state: "Queued" }));
    const reasons: Array<[string, string | null, string | null]> = [
      ["approval", null, null],
      ["approval", null, "project_not_registered"],
      ["other", null, "worktree_owned"],
      ["other", null, "dispatch_blocked"],
      ["other", null, "not_queued"],
      ["other", null, null],
      ["something_new", null, null],
    ];
    withSnapshot(fake, (wire) => ({
      ...wire,
      queued: fake.tasks.map((task, index) => ({
        task_id: task.contract.id,
        project_id: ALPHA,
        position: null,
        waiting_reason: reasons[index]![0],
        waiting_on: reasons[index]![1],
        waiting_detail: reasons[index]![2],
      })),
    }));
    const data = await okActivity(fake);
    const waiting = data.queued.map((entry) => entry.waiting!);
    expect(waiting.map((wait) => wait.reason)).toEqual(["approval", "approval", "unknown", "unknown", "unknown", "unknown", "unknown"]);
    expect(waiting[1]!.detail).toMatch(/not registered/);
    expect(waiting[2]!.detail).toMatch(/worktree/);
    expect(waiting[6]!.detail).toMatch(/something_new/);
    expect(data.upNext).toMatchObject({ status: "unknown" });
  });

  it("does not claim an up-next task or a pause when the restart gate blocks dispatch", async () => {
    const fake = new FakeDaemon();
    fake.queue = { ...DEFAULT_QUEUE_SETTINGS, dispatch_blocked: true };
    fake.tasks = [mkTask("q1", ALPHA, { state: "Queued" })];
    const data = await okActivity(fake);
    expect(data.upNext).toMatchObject({ status: "unknown" });
    expect(data.dispatchPaused).toMatchObject({ status: "unsupported" });
    expect(data.queued[0]!.waiting!.detail).toMatch(/restart gate/);
  });

  it("lists Verifying tasks as active work without counting them against the slots", async () => {
    const fake = new FakeDaemon();
    fake.tasks = [mkTask("r1", ALPHA, { state: "Running" }), mkTask("v1", ALPHA, { state: "Verifying" }), mkTask("c1", ALPHA, { state: "Created" })];
    const data = await okActivity(fake);
    expect(data.slots).toMatchObject({ status: "ok", value: { used: 1, limit: 2 } });
    expect(data.running.map((entry) => [entry.taskId, entry.state])).toEqual([
      ["r1", "Running"],
      ["v1", "Verifying"],
    ]);
    expect(data.queued).toEqual([]);
  });

  it("answers an empty queue as a real zero, not as unknown", async () => {
    const data = await okActivity(new FakeDaemon());
    expect(data.slots).toMatchObject({ status: "ok", value: { used: 0, limit: 2 } });
    expect(data.running).toEqual([]);
    expect(data.queued).toEqual([]);
    expect(data.upNext).toMatchObject({ status: "unknown" });
  });

  it("keeps rows the task list does not know, with explicit placeholders instead of invented text", async () => {
    const fake = new FakeDaemon();
    fake.tasks = [];
    const known = [mkTask("r1", ALPHA, { state: "Running" }), mkTask("q1", ALPHA, { state: "Queued" })];
    fake.stub("queue.snapshot", () => fakeQueueSnapshot(known, fake.queue));
    const data = await okActivity(fake);
    expect(data.running[0]).toMatchObject({ taskId: "r1", objective: "Objective not available" });
    expect(data.queued[0]).toMatchObject({ taskId: "q1", objective: "Objective not available", agent: "agent not reported" });
  });

  it("re-reads when the snapshot and the task list disagree, and then agrees", async () => {
    const fake = new FakeDaemon();
    fake.tasks = [mkTask("t1", ALPHA, { state: "Queued" })];
    const stale = fakeQueueSnapshot([mkTask("t1", ALPHA, { state: "Running" })], fake.queue);
    let calls = 0;
    fake.stub("queue.snapshot", () => (calls++ === 0 ? stale : fakeQueueSnapshot(fake.tasks, fake.queue)));
    const data = await okActivity(fake);
    expect(fake.callsTo("queue.snapshot")).toHaveLength(2);
    expect(data.running).toEqual([]);
    expect(data.queued.map((entry) => entry.taskId)).toEqual(["t1"]);
  });

  it("keeps the snapshot authoritative if the task list never agrees", async () => {
    const fake = new FakeDaemon();
    fake.tasks = [mkTask("t1", ALPHA, { state: "Succeeded" })];
    fake.stub("queue.snapshot", () => fakeQueueSnapshot([mkTask("t1", ALPHA, { state: "Running" })], fake.queue));
    const data = await okActivity(fake);
    expect(fake.callsTo("queue.snapshot")).toHaveLength(3);
    expect(data.running.map((entry) => entry.taskId)).toEqual(["t1"]);
    expect(data.slots).toMatchObject({ status: "ok", value: { used: 1 } });
  });

  it("lists only requested approvals as attention", async () => {
    const fake = new FakeDaemon();
    fake.tasks = [mkTask("r1", ALPHA, { state: "Running" })];
    fake.approvals = [mkApproval("a1", "r1"), mkApproval("a2", "r1", "approved")];
    const data = await okActivity(fake);
    expect(data.attention.map((item) => item.approvalId)).toEqual(["a1"]);
  });

  describe("failure states", () => {
    it("uses tasks and approvals when queue capability is unavailable", async () => {
      const fake = new FakeDaemon();
      fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "queue");
      fake.tasks = [
        mkTask("r1", ALPHA, { state: "Running" }),
        mkTask("v1", BETA, { state: "Verifying" }),
        mkTask("q1", ALPHA, { state: "Queued" }),
        mkTask("q2", BETA, { state: "Queued" }),
        mkTask("c1", ALPHA, { state: "Created" }),
      ];
      fake.approvals = [mkApproval("a1", "q1"), mkApproval("a2", "r1", "approved")];

      const result = await activityOf(fake);
      if (result.status !== "ok") throw new Error(`expected ok, got ${result.status}`);
      expect(result.provenance).toMatchObject({ methods: ["tasks.list", "approvals.list"] });
      const data = result.data;

      expect(data.running.map((entry) => entry.taskId).sort()).toEqual(["r1", "v1"]);
      expect(data.queued.map((entry) => entry.taskId).sort()).toEqual(["q1", "q2"]);
      expect(data.queued.every((entry) => entry.waiting?.reason === "unknown")).toBe(true);
      expect(data.attention.map((item) => [item.approvalId, item.taskId, item.projectId])).toEqual([["a1", "q1", ALPHA]]);
      expect(data.upNext).toMatchObject({ status: "unknown" });
      expect(data.slots).toMatchObject({ status: "unsupported" });
      expect(data.dispatchPaused).toMatchObject({ status: "unsupported" });
      expect(fake.callsTo("queue.snapshot")).toHaveLength(0);
      expect(fake.callsTo("tasks.list")).toHaveLength(1);
      expect(fake.callsTo("approvals.list")).toHaveLength(1);
    });

    it("uses task and approval lists when the daemon does not implement queue.snapshot", async () => {
      const fake = new FakeDaemon();
      fake.failNext("queue.snapshot", { code: -32601, message: "method unavailable: queue.snapshot" });
      fake.tasks = [mkTask("r1", ALPHA, { state: "Running" })];

      const result = await activityOf(fake);
      if (result.status !== "ok") throw new Error(`expected ok, got ${result.status}`);
      expect(result).toMatchObject({ provenance: { kind: "daemon", methods: ["tasks.list", "approvals.list"] } });
      const data = result.data;

      expect(data.running.map((entry) => entry.taskId)).toEqual(["r1"]);
      expect(fake.callsTo("queue.snapshot")).toHaveLength(1);
      expect(fake.callsTo("tasks.list")).toHaveLength(1);
      expect(fake.callsTo("approvals.list")).toHaveLength(1);
    });

    it("uses task and approval lists on an older daemon without the queue capability", async () => {
      const fake = new FakeDaemon();
      fake.unknownCapabilities = ["queue"];
      fake.tasks = [mkTask("r1", ALPHA, { state: "Running" })];
      const data = await okActivity(fake);

      expect(data.running.map((entry) => entry.taskId)).toEqual(["r1"]);
      expect(fake.callsTo("queue.snapshot")).toHaveLength(0);
      expect(fake.callsTo("tasks.list")).toHaveLength(1);
      expect(fake.callsTo("approvals.list")).toHaveLength(1);
    });

    it("is unavailable, not unsupported and not fixture data, when the daemon errors", async () => {
      const fake = new FakeDaemon();
      fake.failNext("queue.snapshot", { code: -32603, message: "boom" });
      const result = await activityOf(fake);
      expect(result).toMatchObject({ status: "unavailable", provenance: { kind: "daemon", id: "D11" } });
      expect(result).toMatchObject({ provenance: { kind: "daemon", methods: ["queue.snapshot"] } });
      expect(JSON.stringify(result)).toMatch(/boom/);
      expect(fake.callsTo("tasks.list")).toHaveLength(0);
      expect(fake.callsTo("approvals.list")).toHaveLength(0);
    });

    it("is unavailable when tasks.list fails during the task-list fallback", async () => {
      const fake = new FakeDaemon();
      fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "queue");
      fake.failNext("tasks.list", { code: -32603, message: "tasks boom" });
      const result = await activityOf(fake);
      expect(result).toMatchObject({ status: "unavailable", provenance: { kind: "daemon", id: "D11" } });
      expect(result).toMatchObject({ provenance: { kind: "daemon", methods: ["tasks.list", "approvals.list"] } });
      expect(JSON.stringify(result)).toMatch(/tasks boom/);
      expect(JSON.stringify(result)).not.toMatch(/fixture/i);
      expect(fake.callsTo("queue.snapshot")).toHaveLength(0);
      expect(fake.callsTo("tasks.list")).toHaveLength(1);
      expect(fake.callsTo("approvals.list")).toHaveLength(1);
    });

    it("marks fallback attention unavailable when approvals.list fails", async () => {
      const fake = new FakeDaemon();
      fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "queue");
      fake.tasks = [mkTask("r1", ALPHA, { state: "Running" })];
      fake.failNext("approvals.list", { code: -32603, message: "approvals boom" });
      const result = await activityOf(fake);
      if (result.status !== "ok") throw new Error(`expected ok, got ${result.status}`);
      expect(result).toMatchObject({ provenance: { kind: "daemon", methods: ["tasks.list", "approvals.list"] } });
      const data = result.data;
      expect(data.attention).toEqual([]);
      expect(data.degraded?.approvals).toMatch(/approvals boom/);
      expect(data.degraded?.tasks).toBeUndefined();
      expect(data.running.map((entry) => entry.taskId)).toEqual(["r1"]);
    });

    it("degrades only the task details when tasks.list fails after the snapshot succeeded", async () => {
      const fake = new FakeDaemon();
      fake.tasks = [
        mkTask("r1", ALPHA, { state: "Running" }),
        mkTask("q1", ALPHA, { state: "Queued" }),
        mkTask("v1", ALPHA, { state: "Verifying" }),
      ];
      fake.approvals = [mkApproval("a1", "r1")];
      fake.failNext("tasks.list", { code: -32603, message: "tasks boom" }, 10);
      const data = await okActivity(fake);
      expect(data.degraded?.tasks).toMatch(/tasks boom/);
      expect(data.degraded?.approvals).toBeUndefined();
      expect(data.slots).toMatchObject({ status: "ok", value: { used: 1, limit: 2 } });
      expect(data.running.map((entry) => entry.taskId)).toEqual(["r1"]);
      expect(data.running[0]!.objective).toBe("Objective not available");
      expect(data.queued.map((entry) => entry.taskId)).toEqual(["q1"]);
      expect(data.attention.map((item) => item.approvalId)).toEqual(["a1"]);
      expect(fake.callsTo("queue.snapshot")).toHaveLength(1);
    });

    it("degrades only the attention list when approvals.list fails after the snapshot succeeded", async () => {
      const fake = new FakeDaemon();
      fake.tasks = [mkTask("r1", ALPHA, { state: "Running" }), mkTask("q1", ALPHA, { state: "Queued" })];
      fake.approvals = [mkApproval("a1", "r1")];
      fake.failNext("approvals.list", { code: -32603, message: "approvals boom" }, 10);
      const data = await okActivity(fake);
      expect(data.degraded?.approvals).toMatch(/approvals boom/);
      expect(data.degraded?.tasks).toBeUndefined();
      expect(data.attention).toEqual([]);
      expect(data.slots).toMatchObject({ status: "ok", value: { used: 1, limit: 2 } });
      expect(data.running[0]!.objective).not.toBe("Objective not available");
      expect(data.queued.map((entry) => entry.taskId)).toEqual(["q1"]);
    });

    it("reports both degraded parts when both follow-up reads fail", async () => {
      const fake = new FakeDaemon();
      fake.tasks = [mkTask("r1", ALPHA, { state: "Running" })];
      fake.failNext("tasks.list", { code: -32603, message: "tasks boom" }, 10);
      fake.failNext("approvals.list", { code: -32603, message: "approvals boom" }, 10);
      const data = await okActivity(fake);
      expect(Object.keys(data.degraded ?? {}).sort()).toEqual(["approvals", "tasks"]);
      expect(data.running.map((entry) => entry.taskId)).toEqual(["r1"]);
    });

    it("has no degraded marker when every read succeeds", async () => {
      const fake = new FakeDaemon();
      fake.tasks = [mkTask("r1", ALPHA, { state: "Running" })];
      expect((await okActivity(fake)).degraded).toBeUndefined();
    });

    it("is unavailable when the snapshot is malformed", async () => {
      const fake = new FakeDaemon();
      fake.stub("queue.snapshot", { running: [], queued: [] });
      expect(await activityOf(fake)).toMatchObject({ status: "unavailable" });
    });

    it("recovers on the next read after a failure", async () => {
      const fake = new FakeDaemon();
      fake.tasks = [mkTask("r1", ALPHA, { state: "Running" })];
      fake.failNext("queue.snapshot", { code: -32603, message: "boom" });
      const connected = await connect(fake);
      stops.push(connected.stop);
      expect((await connected.surfaces.activity.activity()).status).toBe("unavailable");
      const second = await connected.surfaces.activity.activity();
      expect(second).toMatchObject({ status: "ok" });
    });

    it("is unavailable while disconnected and reads nothing", async () => {
      const fake = new FakeDaemon();
      const connected = await connect(fake, false);
      stops.push(connected.stop);
      const result = await connected.surfaces.activity.activity();
      expect(result.status).toBe("unavailable");
      expect(result).toMatchObject({ provenance: { kind: "daemon", methods: [] } });
      expect(fake.callsTo("queue.snapshot")).toHaveLength(0);
    });
  });

  it("rejects the fixture-only simulate calls", async () => {
    const connected = await connect(new FakeDaemon());
    stops.push(connected.stop);
    await expect(connected.surfaces.activity.simulatePauseDispatch()).rejects.toMatchObject({ kind: "method-unavailable" });
    await expect(connected.surfaces.activity.simulateStop("t1")).rejects.toMatchObject({ kind: "method-unavailable" });
  });
});
