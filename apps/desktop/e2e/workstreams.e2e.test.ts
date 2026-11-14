import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { realpathSync } from "node:fs";
import { DaemonApi } from "../src/daemon/api";
import { ConnectionManager } from "../src/daemon/connection";
import { DaemonSession } from "../src/daemon/session";
import { M5_03_CAPABILITIES, type TaskSubmission } from "../src/daemon/types";
import { createDerivedSurfaces } from "../src/sources/surfaces/derived";
import type { WorkstreamSource } from "../src/sources/surfaces/types";
import { NodeConnector, startDaemon, type RunningDaemon } from "./harness";

let daemon: RunningDaemon;
let connector: NodeConnector;
let connection: ConnectionManager;
let source: WorkstreamSource;
let api: DaemonApi;
let session: DaemonSession;
const taskIds: string[] = [];

const submission = (projectId: string, objective: string, dependencies: string[] = []): TaskSubmission => ({
  project_id: projectId,
  parent_id: null,
  objective,
  kind: "Implement",
  agent: "e2e-agent",
  dependencies,
  verification: "None",
  delivery: "LocalReview",
  autonomy: "ApprovalRequired",
  max_retries: 1,
  max_repairs: 1,
});

async function until<T>(read: () => T | undefined | false, label: string, ms = 8000): Promise<T> {
  const deadline = Date.now() + ms;
  for (;;) {
    const value = read();
    if (value) return value;
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${label}`);
    await new Promise((r) => setTimeout(r, 25));
  }
}

const read = async () => {
  const result = await source.workstreams();
  if (result.status !== "ok") throw new Error(`workstreams read was ${result.status}`);
  return result.data;
};

beforeAll(async () => {
  daemon = await startDaemon();
  connector = new NodeConnector(daemon.dataDir);
  const projectId = realpathSync(daemon.repo);
  session = new DaemonSession(await connector.open(M5_03_CAPABILITIES));
  api = new DaemonApi(session);
  await api.projectsRegister(daemon.repo);
  // The unsupported blocker cannot succeed; linked tasks remain queued.
  const blocker = await api.tasksSubmit("ws-blocker", submission(projectId, "workstreams blocker"));
  for (const [index, objective] of ["first task", "second task", "third task"].entries()) {
    taskIds.push((await api.tasksSubmit(`ws-e2e-submit-${index}`, submission(projectId, objective, [blocker.contract.id]))).contract.id);
  }
  connection = new ConnectionManager({ connector, backoffMs: [50], requestTimeoutMs: 8000 });
  connection.start();
  await until(() => connection.getSnapshot().status === "connected", "connected");
  source = createDerivedSurfaces(connection, Date.now).workstreams;
});

afterAll(async () => {
  connection?.stop();
  session?.close();
  await daemon?.stop();
});

describe("live workstreams adapter over the real daemon", () => {
  it("negotiates the workstreams capability and reads an empty, known list", async () => {
    expect(session.welcome.capabilities).toContain("workstreams");
    expect(await read()).toEqual([]);
  });

  it("runs the full lifecycle through the real workstreams.* methods", async () => {
    const [t1, t2, t3] = taskIds as [string, string, string];
    const created = await source.create("E2E stream", "Proves the adapter");
    expect(created.revision).toBe(1);

    const withObjective = await source.addObjective(created.id, {
      title: "Ship it",
      description: "all gates",
      gates: ["Builds", "Tests pass"],
    });
    expect(withObjective.revision).toBe(1);

    await source.linkTask(created.id, t1, withObjective.id);
    await source.linkTask(created.id, t2);
    // Linking the identical link again is a daemon no-op, not an error.
    await source.linkTask(created.id, t1, withObjective.id);

    let [stream] = await read();
    expect(stream).toMatchObject({ id: created.id, title: "E2E stream", summary: "Proves the adapter", state: "active", revision: 1 });
    expect(stream!.tasks.map((t) => [t.id, t.objectiveId, t.state, t.agent, t.objective]).sort()).toEqual(
      [
        [t1, withObjective.id, "Queued", "e2e-agent", "first task"],
        [t2, null, "Queued", "e2e-agent", "second task"],
      ].sort(),
    );
    expect(stream!.projectIds).toEqual([realpathSync(daemon.repo)]);
    let objective = stream!.objectives.status === "ok" ? stream!.objectives.value[0]! : null;
    expect(objective?.gates.map((g) => [g.id, g.label, g.state])).toEqual([
      ["gate-1", "Builds", "pending"],
      ["gate-2", "Tests pass", "pending"],
    ]);

    // set_gate at the revision read, with evidence from a task linked to this workstream.
    const gated = await source.setGate({ id: objective!.id, revision: objective!.revision }, "gate-1", "verified", t1);
    expect(gated.revision).toBe(objective!.revision + 1);
    const revised = await source.reviseObjective({ id: objective!.id, revision: gated.revision! }, { title: "Ship it now", addGates: ["Docs"] });
    expect(revised.revision).toBe(gated.revision! + 1);

    [stream] = await read();
    objective = stream!.objectives.status === "ok" ? stream!.objectives.value[0]! : null;
    expect(objective).toMatchObject({ title: "Ship it now", revision: revised.revision });
    expect(objective!.gates.map((g) => [g.id, g.state, g.evidenceTaskId])).toEqual([
      ["gate-1", "verified", t1],
      ["gate-2", "pending", null],
      ["gate-3", "pending", null],
    ]);

    // A stale revision is a conflict and changes nothing.
    await expect(source.setGate({ id: objective!.id, revision: 1 }, "gate-2", "verified")).rejects.toMatchObject({ kind: "conflict" });
    // An evidence task from outside the workstream is refused.
    await expect(
      source.setGate({ id: objective!.id, revision: objective!.revision }, "gate-2", "verified", t3),
    ).rejects.toMatchObject({ kind: "invalid-params" });
    // A task cited as gate evidence cannot be unlinked.
    await expect(source.unlinkTask(created.id, t1)).rejects.toMatchObject({ kind: "conflict" });
    // A task already linked elsewhere cannot be linked to a second workstream.
    const other = await source.create("Other stream");
    await expect(source.linkTask(other.id, t2)).rejects.toMatchObject({ kind: "conflict" });

    await source.unlinkTask(created.id, t2);
    [stream] = await read();
    expect(stream!.tasks.map((t) => t.id)).toEqual([t1]);

    const suspended = await source.update({ id: created.id, revision: stream!.revision }, { state: "suspended" });
    expect(suspended.revision).toBe(stream!.revision + 1);
    await expect(source.update({ id: created.id, revision: stream!.revision }, { state: "active" })).rejects.toMatchObject({ kind: "conflict" });
    const archived = await source.update({ id: created.id, revision: suspended.revision! }, { state: "archived" });
    expect(archived.revision).toBe(suspended.revision! + 1);

    // Archived is terminal: every write is refused and the data stays readable.
    await expect(source.addObjective(created.id, { title: "late" })).rejects.toMatchObject({ kind: "conflict" });
    await expect(source.linkTask(created.id, t3)).rejects.toMatchObject({ kind: "conflict" });
    const final = (await read()).find((item) => item.id === created.id);
    expect(final).toMatchObject({ state: "archived", revision: archived.revision });
  });

  it("replays an idempotency key to the first result and rejects it for different parameters", async () => {
    const first = await api.workstreamsCreate("Replayed", undefined, "ws-e2e-replay-1");
    const again = await api.workstreamsCreate("Replayed", undefined, "ws-e2e-replay-1");
    expect(again).toEqual(first);
    expect((await api.workstreamsList()).filter((item) => item.title === "Replayed")).toHaveLength(1);
    await expect(api.workstreamsCreate("Different", undefined, "ws-e2e-replay-1")).rejects.toMatchObject({ kind: "idempotency-conflict" });
  });

  it("reports a missing record as not-found and refuses fixture ids before any request", async () => {
    await expect(source.update({ id: "ws-does-not-exist", revision: 1 }, { state: "suspended" })).rejects.toMatchObject({ kind: "not-found" });
    await expect(source.update({ id: "fixture:ws-auth", revision: 1 }, { state: "suspended" })).rejects.toMatchObject({ kind: "invalid-params" });
    await expect(source.linkTask("fixture:ws-auth", taskIds[0]!)).rejects.toMatchObject({ kind: "invalid-params" });
    await expect(api.workstreamsGet("fixture:ws-auth")).rejects.toBeDefined();
    expect((await api.workstreamsList()).some((item) => item.id.startsWith("fixture:"))).toBe(false);
  });
});
