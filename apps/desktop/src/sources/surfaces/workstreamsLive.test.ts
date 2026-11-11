import { afterEach, describe, expect, it } from "vitest";
import { ALL_CAPABILITIES } from "../../daemon/types";
import { FakeDaemon } from "../../test/fakeDaemon";
import { WORKSTREAM_STATE, connect, populate } from "./testkit";

const stops: Array<() => void> = [];
afterEach(() => {
  while (stops.length) stops.pop()!();
});

async function live(fake: FakeDaemon) {
  const connected = await connect(fake);
  stops.push(connected.stop);
  return connected.surfaces.workstreams;
}

const keyOf = (call: { params: unknown }) => (call.params as { idempotency_key: string }).idempotency_key;
const paramsOf = (fake: FakeDaemon, method: string) => fake.callsTo(method).map((call) => call.params as Record<string, unknown>);

describe("live workstreams adapter", () => {
  it("requests the workstreams capability", () => {
    expect(ALL_CAPABILITIES).toContain("workstreams");
  });

  it("reads through workstreams.list, workstreams.get per workstream and tasks.list", async () => {
    const fake = populate(new FakeDaemon());
    await (await live(fake)).workstreams();
    expect(fake.callsTo("workstreams.list")).toHaveLength(1);
    expect(paramsOf(fake, "workstreams.get").map((params) => params.workstream_id)).toEqual(["ws-1", "ws-2"]);
    expect(fake.callsTo("tasks.list")).toHaveLength(1);
  });

  it("is unsupported, with daemon provenance, when the daemon lacks the workstreams methods", async () => {
    const fake = new FakeDaemon();
    fake.projects = [];
    const result = await (await live(fake)).workstreams();
    expect(result).toMatchObject({ status: "unsupported", provenance: { kind: "daemon", id: "D9" } });
    expect(result).not.toHaveProperty("data");
  });

  it("does not call the daemon when the capability was not negotiated", async () => {
    const fake = populate(new FakeDaemon());
    fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "workstreams");
    const source = await live(fake);
    const result = await source.workstreams();
    expect(result.status).toBe("unsupported");
    expect(result.provenance.kind).toBe("daemon");
    expect(fake.callsTo("workstreams.list")).toHaveLength(0);
    await expect(source.create("x")).rejects.toMatchObject({ kind: "capability-not-negotiated" });
    expect(fake.callsTo("workstreams.create")).toHaveLength(0);
  });

  it("stays connected and shows queue, host and workstreams as unsupported on a daemon that refuses them", async () => {
    const fake = populate(new FakeDaemon());
    fake.unknownCapabilities = ["queue", "host", "workstreams"];
    const connected = await connect(fake);
    stops.push(connected.stop);
    expect(connected.connection.getSnapshot().status).toBe("connected");
    expect(fake.openRequests).toHaveLength(2);
    expect(await connected.surfaces.workstreams.workstreams()).toMatchObject({ status: "unsupported", provenance: { kind: "daemon" } });
    expect(await connected.surfaces.metrics.metrics()).toMatchObject({ status: "unsupported", provenance: { kind: "daemon" } });
    expect(await connected.surfaces.activity.activity()).toMatchObject({ status: "unsupported", provenance: { kind: "daemon" } });
    expect(fake.callsTo("workstreams.list")).toHaveLength(0);
    expect(fake.callsTo("host.metrics")).toHaveLength(0);
    expect(fake.callsTo("queue.snapshot")).toHaveLength(0);
  });

  it("creates a workstream and returns the daemon's id and revision", async () => {
    const fake = populate(new FakeDaemon());
    const receipt = await (await live(fake)).create("Gamma", "why");
    expect(receipt).toMatchObject({ revision: 1 });
    expect(receipt.id).toMatch(/^ws-/);
    expect(paramsOf(fake, "workstreams.create")).toEqual([{ title: "Gamma", summary: "why", idempotency_key: expect.any(String) }]);
  });

  it("passes the revision the caller read and maps camelCase fields to the wire", async () => {
    const fake = populate(new FakeDaemon());
    const source = await live(fake);
    await source.update({ id: "ws-1", revision: 2 }, { state: "suspended" });
    await source.addObjective("ws-1", { title: "New", description: "d", gates: ["g1"] });
    await source.reviseObjective({ id: "obj-1", revision: 3 }, { title: "T", addGates: ["g3"] });
    await source.setGate({ id: "obj-1", revision: 4 }, "gate-2", "verified", "t-run");
    await source.linkTask("ws-1", "t-free", "obj-1");
    await source.unlinkTask("ws-1", "t-free");
    const strip = ({ idempotency_key: _key, ...rest }: Record<string, unknown>) => rest;
    expect(paramsOf(fake, "workstreams.update").map(strip)).toEqual([{ workstream_id: "ws-1", expected_revision: 2, state: "suspended" }]);
    expect(paramsOf(fake, "workstreams.add_objective").map(strip)).toEqual([
      { workstream_id: "ws-1", title: "New", description: "d", gates: [{ description: "g1" }] },
    ]);
    expect(paramsOf(fake, "workstreams.revise_objective").map(strip)).toEqual([
      { objective_id: "obj-1", expected_revision: 3, title: "T", add_gates: [{ description: "g3" }] },
    ]);
    expect(paramsOf(fake, "workstreams.set_gate").map(strip)).toEqual([
      { objective_id: "obj-1", expected_revision: 4, gate_id: "gate-2", state: "verified", evidence_task_id: "t-run" },
    ]);
    expect(paramsOf(fake, "workstreams.link_task").map(strip)).toEqual([{ workstream_id: "ws-1", task_id: "t-free", objective_id: "obj-1" }]);
    expect(paramsOf(fake, "workstreams.unlink_task").map(strip)).toEqual([{ workstream_id: "ws-1", task_id: "t-free" }]);
    for (const method of ["update", "add_objective", "revise_objective", "set_gate", "link_task", "unlink_task"]) {
      expect(paramsOf(fake, `workstreams.${method}`)[0]!.idempotency_key).toEqual(expect.any(String));
    }
  });

  it("never sends a fixture id: it refuses before any request", async () => {
    const fake = populate(new FakeDaemon());
    const source = await live(fake);
    const refusals = [
      source.update({ id: "fixture:ws-auth", revision: 1 }, { state: "suspended" }),
      source.addObjective("fixture:ws-auth", { title: "x" }),
      source.reviseObjective({ id: "fixture:obj-1", revision: 1 }, { title: "x" }),
      source.setGate({ id: "obj-1", revision: 3 }, "gate-2", "verified", "fixture:task-1"),
      source.setGate({ id: "fixture:obj-1", revision: 3 }, "gate-2", "verified"),
      source.linkTask("ws-1", "fixture:task-1"),
      source.linkTask("fixture:ws-auth", "t-free"),
      source.linkTask("ws-1", "t-free", "fixture:obj-1"),
      source.unlinkTask("fixture:ws-auth", "t-run"),
    ];
    for (const refusal of refusals) await expect(refusal).rejects.toMatchObject({ kind: "invalid-params" });
    for (const call of fake.calls) expect(JSON.stringify(call.params ?? null)).not.toContain("fixture:");
    expect(fake.calls.filter((call) => /^workstreams\.(create|update|add_|revise_|set_|link_|unlink_)/.test(call.method))).toEqual([]);
  });

  it("reuses the idempotency key after an indeterminate failure and applies the write once", async () => {
    const fake = populate(new FakeDaemon());
    fake.failNext("workstreams.update", "hang");
    const source = await live(fake);
    const change = { state: "suspended" } as const;
    await expect(source.update({ id: "ws-1", revision: 2 }, change)).rejects.toMatchObject({ kind: "timeout" });
    await source.update({ id: "ws-1", revision: 2 }, change);
    const keys = fake.callsTo("workstreams.update").map(keyOf);
    expect(keys).toHaveLength(2);
    expect(keys[0]).toBe(keys[1]);
    expect(WORKSTREAM_STATE.get(fake)!.applied.filter((entry) => entry.method === "workstreams.update")).toHaveLength(1);
  });

  it("mints a fresh key after a definitive failure, and for different parameters", async () => {
    const fake = populate(new FakeDaemon());
    const source = await live(fake);
    // Stale revision: the daemon answers with a conflict, which is definitive.
    await expect(source.update({ id: "ws-1", revision: 1 }, { state: "suspended" })).rejects.toMatchObject({ kind: "conflict" });
    await source.update({ id: "ws-1", revision: 2 }, { state: "suspended" });
    await source.update({ id: "ws-1", revision: 3 }, { state: "active" });
    const keys = fake.callsTo("workstreams.update").map(keyOf);
    expect(new Set(keys).size).toBe(3);
  });

  it("maps daemon errors: conflict, not-found and invalid params", async () => {
    const fake = populate(new FakeDaemon());
    const source = await live(fake);
    await expect(source.setGate({ id: "obj-1", revision: 99 }, "gate-2", "verified")).rejects.toMatchObject({ kind: "conflict" });
    await expect(source.setGate({ id: "obj-missing", revision: 1 }, "gate-2", "verified")).rejects.toMatchObject({ kind: "not-found" });
    await expect(source.setGate({ id: "obj-1", revision: 3 }, "gate-1", "pending", "t-run")).rejects.toMatchObject({ kind: "invalid-params" });
    await expect(source.setGate({ id: "obj-1", revision: 3 }, "gate-1", "verified")).rejects.toMatchObject({ kind: "conflict" });
  });

  it("surfaces a task already linked elsewhere as a conflict and leaves the link alone", async () => {
    const fake = populate(new FakeDaemon());
    const source = await live(fake);
    await expect(source.linkTask("ws-2", "t-run")).rejects.toMatchObject({ kind: "conflict" });
    expect(WORKSTREAM_STATE.get(fake)!.links.find((link) => link.task_id === "t-run")?.workstream_id).toBe("ws-1");
  });

  it("does not report success when unlinking a task that was never linked to the workstream", async () => {
    const fake = populate(new FakeDaemon());
    const source = await live(fake);
    const before = [...WORKSTREAM_STATE.get(fake)!.links];
    await expect(source.unlinkTask("ws-1", "t-free")).rejects.toMatchObject({ kind: "not-found" });
    await expect(source.unlinkTask("ws-2", "t-run")).rejects.toMatchObject({ kind: "not-found" });
    expect(fake.callsTo("workstreams.unlink_task")).toHaveLength(2);
    expect(WORKSTREAM_STATE.get(fake)!.links).toEqual(before);
    // A real link still unlinks and reports success.
    await expect(source.unlinkTask("ws-1", "t-run")).resolves.toMatchObject({ id: "t-run" });
  });

  it("marks one workstream whose detail read failed as unknown and keeps the others", async () => {
    const fake = populate(new FakeDaemon());
    const state = WORKSTREAM_STATE.get(fake)!;
    const ghost = { id: "ws-ghost", title: "Unreadable", summary: "listed only", state: "active" as const, revision: 7 };
    fake.stub("workstreams.list", () => [...state.workstreams, ghost]);
    const result = await (await live(fake)).workstreams();
    if (result.status !== "ok") throw new Error(`expected ok, got ${result.status}`);
    const [first, , unread] = result.data;
    expect(result.data.map((item) => item.id)).toEqual(["ws-1", "ws-2", "ws-ghost"]);
    expect(first!.unreadReason).toBeUndefined();
    expect(first!.tasks.length).toBeGreaterThan(0);
    expect(unread).toMatchObject({ title: "Unreadable", revision: 7, tasks: [], objectives: { status: "unknown" } });
    expect(unread!.unreadReason).toMatch(/could not be read.*not-found/);
  });

  it("fails the read when every detail read fails or the failure is not about one record", async () => {
    const fake = populate(new FakeDaemon());
    fake.stub("workstreams.list", () => [{ id: "ws-ghost", title: "x", summary: "", state: "active", revision: 1 }]);
    expect(await (await live(fake)).workstreams()).toMatchObject({ status: "unavailable" });

    const second = populate(new FakeDaemon());
    second.failNext("workstreams.get", { code: -32602, message: "bad" });
    expect((await (await live(second)).workstreams()).status).toBe("unavailable");
  });

  it("refuses to unlink a task that a gate cites as evidence", async () => {
    const fake = populate(new FakeDaemon());
    const source = await live(fake);
    await expect(source.unlinkTask("ws-1", "t-done")).rejects.toMatchObject({ kind: "conflict" });
    await expect(source.linkTask("ws-1", "t-free", "obj-missing")).rejects.toMatchObject({ kind: "not-found" });
    expect(WORKSTREAM_STATE.get(fake)!.links.some((link) => link.task_id === "t-done")).toBe(true);
  });

  it("treats re-linking the identical link as a no-op success", async () => {
    const fake = populate(new FakeDaemon());
    const source = await live(fake);
    await source.linkTask("ws-1", "t-run", "obj-1");
    expect(WORKSTREAM_STATE.get(fake)!.links.filter((link) => link.task_id === "t-run")).toHaveLength(1);
  });

  it("never answers a failed live read with fixture data", async () => {
    const fake = populate(new FakeDaemon());
    fake.failNext("tasks.list", { code: -32603, message: "boom" });
    const result = await (await live(fake)).workstreams();
    expect(result).toMatchObject({ status: "unavailable", provenance: { kind: "daemon" } });
    expect(JSON.stringify(result)).not.toContain("fixture:");
  });
});
