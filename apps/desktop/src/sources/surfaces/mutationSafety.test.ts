import { afterEach, describe, expect, it } from "vitest";
import { DaemonError } from "../../daemon/errors";
import { FakeDaemon } from "../../test/fakeDaemon";
import { createFixtureSurfaces } from "./fixtures";
import { connect, populate } from "./testkit";
import type { ConversationTarget, WorkstreamSource } from "./types";

const stops: Array<() => void> = [];
afterEach(() => { while (stops.length) stops.pop()!(); });
async function setup(start = true) {
  const fake = populate(new FakeDaemon());
  const built = await connect(fake, start);
  stops.push(built.stop);
  return { fake, ...built };
}

type Write = { method: string; run: (source: WorkstreamSource, id: string) => Promise<unknown> };
// Every ID position, including objective, gate and evidence IDs.
const idWrites: Write[] = [
  { method: "update", run: (s, id) => s.update({ id, revision: 2 }, { title: "x" }) },
  { method: "add_objective", run: (s, id) => s.addObjective(id, { title: "x" }) },
  { method: "revise_objective", run: (s, id) => s.reviseObjective({ id, revision: 3 }, { title: "x" }) },
  { method: "set_gate", run: (s, id) => s.setGate({ id, revision: 3 }, "gate-1", "verified", "t-run") },
  { method: "set_gate", run: (s, id) => s.setGate({ id: "obj-1", revision: 3 }, id, "verified", "t-run") },
  { method: "set_gate", run: (s, id) => s.setGate({ id: "obj-1", revision: 3 }, "gate-1", "verified", id) },
  { method: "link_task", run: (s, id) => s.linkTask(id, "t-free") },
  { method: "link_task", run: (s, id) => s.linkTask("ws-1", id) },
  { method: "link_task", run: (s, id) => s.linkTask("ws-1", "t-free", id) },
  { method: "unlink_task", run: (s, id) => s.unlinkTask(id, "t-run") },
  { method: "unlink_task", run: (s, id) => s.unlinkTask("ws-1", id) },
];
const writes: Write[] = [
  { method: "create", run: (s) => s.create("New workstream") },
  ...idWrites.filter((_, i) => [0, 1, 2, 3, 6, 9].includes(i)),
];

describe("D9 mutation safety", () => {
  it.each(["fixture:item", "fixture0001", " FIXTURE0001 "])("refuses every ID position for %s, connected or disconnected", async (id) => {
    for (const start of [true, false]) {
      const { surfaces, fake } = await setup(start);
      const before = fake.calls.length;
      for (const write of idWrites) await expect(write.run(surfaces.workstreams, id)).rejects.toMatchObject({ kind: "invalid-params" });
      expect(fake.calls.length).toBe(before);
    }
  });

  it.each(writes)("rejects a failed workstreams.$method write without a simulated receipt", async (write) => {
    const { surfaces, fake } = await setup();
    const method = `workstreams.${write.method}`;
    fake.failNext(method, { code: -32603, message: "write failed" });
    await expect(write.run(surfaces.workstreams, ["revise_objective", "set_gate"].includes(write.method) ? "obj-1" : "ws-1"))
      .rejects.toBeInstanceOf(DaemonError);
    expect(fake.callsTo(method)).toHaveLength(1);
  });

  it.each(writes)("rejects real $method writes on the fixture adapter with DaemonError", async (write) => {
    const source = createFixtureSurfaces().workstreams;
    await expect(write.run(source, "fixture:item")).rejects.toBeInstanceOf(DaemonError);
    await expect(write.run(source, "fixture:item")).rejects.toMatchObject({ kind: "method-unavailable" });
  });
});

const targets: ConversationTarget[] = [{ kind: "global" }, { kind: "workstream", workstreamId: "ws-1" }, { kind: "project", projectId: "/repos/alpha" }];
describe("D10 mutation safety", () => {
  it.each(targets)("rejects failed sends for $kind without fixture fallback", async (target) => {
    const { surfaces, fake } = await setup();
    const method = target.kind === "project" ? "coordinator.send_message" : "conversations.send";
    fake.failNext(method, { code: -32603, message: "send failed" });
    await expect(surfaces.conversation.send(target, "hello")).rejects.toBeInstanceOf(DaemonError);
    expect(fake.callsTo(method)).toHaveLength(1);
  });
  it.each(targets.slice(0, 2))("rejects failed cancels for $kind without fixture fallback", async (target) => {
    const { surfaces, fake } = await setup();
    fake.failNext("conversations.cancel", { code: -32603, message: "cancel failed" });
    await expect(surfaces.conversation.cancel!(target, "turn-1")).rejects.toBeInstanceOf(DaemonError);
    expect(fake.callsTo("conversations.cancel")).toHaveLength(1);
  });
  it.each(["fixture:item", "fixture0001", " FIXTURE0001 "])("refuses fixture targets and turn IDs %s before requests", async (id) => {
    for (const start of [true, false]) {
      const { surfaces, fake } = await setup(start);
      const before = fake.calls.length;
      const badTargets: ConversationTarget[] = [{ kind: "project", projectId: id }, { kind: "workstream", workstreamId: id }];
      for (const target of badTargets) {
        await expect(surfaces.conversation.send(target, "hello")).rejects.toMatchObject({ kind: "invalid-params" });
        if (target.kind === "workstream") await expect(surfaces.conversation.cancel!(target, "turn-1")).rejects.toMatchObject({ kind: "invalid-params" });
      }
      for (const target of targets.slice(0, 2)) await expect(surfaces.conversation.cancel!(target, id)).rejects.toMatchObject({ kind: "invalid-params" });
      expect(fake.calls.length).toBe(before);
    }
  });
});

describe("review approve/reject API used by the review screen", () => {
  it.each(["approve", "reject"] as const)("refuses fixture approval IDs for %s before any request", async (decision) => {
    const { connection, fake } = await setup();
    await expect(connection.api().approvalsResolve("fixture:approval-1", decision, "key")).rejects.toMatchObject({ kind: "invalid-params" });
    expect(fake.callsTo("approvals.resolve")).toHaveLength(0);
  });
});
