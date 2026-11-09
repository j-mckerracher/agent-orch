import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { realpathSync } from "node:fs";
import { AppData } from "../src/daemon/appData";
import { ConnectionManager, type ConnectionStatus } from "../src/daemon/connection";
import { DaemonError } from "../src/daemon/errors";
import { DaemonSession } from "../src/daemon/session";
import { DaemonApi } from "../src/daemon/api";
import { M5_03_CAPABILITIES, type DaemonEvent, type TaskSubmission } from "../src/daemon/types";
import { NodeConnector, startDaemon, type RunningDaemon } from "./harness";

let daemon: RunningDaemon;
let connector: NodeConnector;
let projectId: string;

const submission = (overrides: Partial<TaskSubmission> = {}): TaskSubmission => ({
  project_id: projectId,
  parent_id: null,
  objective: "e2e objective",
  kind: "Implement",
  agent: "e2e-agent",
  dependencies: [],
  verification: "None",
  delivery: "LocalReview",
  autonomy: "ApprovalRequired",
  max_retries: 1,
  max_repairs: 1,
  ...overrides,
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

async function openSession() {
  return new DaemonSession(await connector.open(M5_03_CAPABILITIES));
}

beforeAll(async () => {
  daemon = await startDaemon();
  connector = new NodeConnector(daemon.dataDir);
  projectId = realpathSync(daemon.repo);
});

afterAll(async () => {
  await daemon?.stop();
});

describe("real M5-02 daemon: handshake", () => {
  it("negotiates exactly the requested capabilities", async () => {
    const session = await openSession();
    expect(session.welcome.version).toBe(1);
    expect([...session.welcome.capabilities].sort()).toEqual([...M5_03_CAPABILITIES].sort());
    session.close();
  });

  it("rejects a wrong token as an auth failure", async () => {
    const bad = new NodeConnector(daemon.dataDir);
    bad.tokenOverride = "0".repeat(64);
    await expect(bad.open(M5_03_CAPABILITIES)).rejects.toMatchObject({ kind: "auth" });
  });

  it("rejects an unsupported protocol version", async () => {
    const bad = new NodeConnector(daemon.dataDir);
    bad.versions = [99];
    await expect(bad.open(M5_03_CAPABILITIES)).rejects.toMatchObject({ kind: "version" });
  });

  it("refuses a method whose capability was not negotiated, locally", async () => {
    const session = new DaemonSession(await connector.open(["health"]));
    await expect(session.request("tasks.list")).rejects.toMatchObject({ kind: "capability-not-negotiated" });
    session.close();
  });
});

describe("real M5-02 daemon: projects, tasks and errors", () => {
  it("registers a project idempotently and lists it", async () => {
    const session = await openSession();
    const api = new DaemonApi(session);
    const first = await api.projectsRegister(daemon.repo);
    expect(first.id).toBe(projectId);
    const again = await api.projectsRegister(daemon.repo);
    expect(again.id).toBe(projectId);
    expect(await api.projectsList()).toEqual([{ id: projectId }]);
    session.close();
  });

  it("maps a relative project path to invalid-params", async () => {
    const session = await openSession();
    await expect(new DaemonApi(session).projectsRegister("relative/path")).rejects.toMatchObject({
      kind: "invalid-params",
    });
    session.close();
  });

  it("submits a Queued task, lists, gets, replays idempotently and cancels", async () => {
    const session = await openSession();
    const api = new DaemonApi(session);
    const record = await api.tasksSubmit("e2e-key-submit-1", submission());
    expect(record.state).toBe("Queued");
    expect(record.sessions).toEqual([]);
    expect(record.contract.agent).toBe("e2e-agent");

    const replay = await api.tasksSubmit("e2e-key-submit-1", submission());
    expect(replay.contract.id).toBe(record.contract.id);

    await expect(
      api.tasksSubmit("e2e-key-submit-1", submission({ objective: "different" })),
    ).rejects.toMatchObject({ kind: "idempotency-conflict" });

    expect((await api.tasksList()).map((t) => t.contract.id)).toContain(record.contract.id);
    expect((await api.tasksGet(record.contract.id)).state).toBe("Queued");
    expect(await api.sessionsList(record.contract.id)).toEqual([]);
    expect(await api.approvalsList()).toEqual([]);

    const cancelled = await api.tasksCancel(record.contract.id, "e2e-key-cancel-1");
    expect(cancelled.state).toBe("Cancelled");
    session.close();
  });

  it("maps definitive errors: unknown project, unknown task, invalid contract", async () => {
    const session = await openSession();
    const api = new DaemonApi(session);
    await expect(
      api.tasksSubmit("e2e-key-unknown-project", submission({ project_id: "/no/such/project" })),
    ).rejects.toMatchObject({ kind: "not-found" });
    await expect(api.tasksGet("task-does-not-exist")).rejects.toMatchObject({ kind: "not-found" });
    await expect(api.tasksSubmit("e2e-key-empty", submission({ objective: "" }))).rejects.toMatchObject({
      kind: "invalid-params",
    });
    await expect(
      api.tasksSubmit("e2e-key-dup-dep", submission({ dependencies: ["a", "a"] })),
    ).rejects.toMatchObject({ kind: "invalid-params" });
    await expect(
      api.tasksSubmit("e2e-key-no-cmds", submission({ verification: { Commands: { commands: [] } } })),
    ).rejects.toMatchObject({ kind: "invalid-params" });
    session.close();
  });

  it("accepts a Commands verification policy with tokenised commands", async () => {
    const session = await openSession();
    const api = new DaemonApi(session);
    const record = await api.tasksSubmit(
      "e2e-key-commands",
      submission({ objective: "with commands", verification: { Commands: { commands: [["cargo", "test"]] } } }),
    );
    expect(record.contract.verification).toEqual({ Commands: { commands: [["cargo", "test"]] } });
    session.close();
  });
});

describe("real M5-02 daemon: event stream", () => {
  it("replays from 0, is contiguous, and live events arrive after submit", async () => {
    const session = await openSession();
    const seen: DaemonEvent[] = [];
    await session.subscribe(0, { event: (e) => seen.push(e), error: (e) => { throw e; } });
    await until(() => seen.length > 0, "replayed events");
    const before = seen.length;
    const record = await new DaemonApi(session).tasksSubmit("e2e-key-live", submission({ objective: "live" }));
    await until(() => seen.length > before, "live events");
    for (let i = 1; i < seen.length; i++) expect(seen[i]!.sequence).toBe(seen[i - 1]!.sequence + 1);
    expect(seen.some((e) => e.task_id === record.contract.id)).toBe(true);
    session.close();
  });

  it("refuses a second subscribe on one connection", async () => {
    const session = await openSession();
    await session.subscribe(0, { event: () => undefined, error: () => undefined });
    await expect(session.subscribe(0, { event: () => undefined, error: () => undefined })).rejects.toMatchObject({
      kind: "conflict",
    });
    session.close();
  });
});

describe("real M5-02 daemon: ConnectionManager and AppData", () => {
  it("connects, loads snapshots, reflects a submit through live events, and reconnects after a drop (asserts PRE-EPOCH resume; flips with M5-02b)", async () => {
    const manager = new ConnectionManager({ connector, backoffMs: [50], requestTimeoutMs: 5000 });
    const data = new AppData(manager, { coalesceMs: 20 });
    const statuses: ConnectionStatus[] = [];
    manager.subscribe(() => {
      const status = manager.getSnapshot().status;
      if (statuses[statuses.length - 1] !== status) statuses.push(status);
    });
    manager.start();
    await until(() => manager.getSnapshot().status === "connected", "connected");
    await until(() => data.getSnapshot().tasks.status === "ready", "tasks loaded");
    expect(data.getSnapshot().projects.data).toEqual([{ id: projectId }]);

    const result = await data.submitTask(submission({ objective: "via app data" }));
    expect(result.ok).toBe(true);
    const id = result.ok ? result.value.contract.id : "";
    await until(
      () => data.getSnapshot().tasks.data?.some((t) => t.contract.id === id),
      "submitted task in the list",
    );
    expect(manager.getSnapshot().lastSequence).toBeGreaterThan(0);

    const generation = manager.getSnapshot().generation;
    const sequenceBefore = manager.getSnapshot().lastSequence;
    connector.opened[connector.opened.length - 1]!.sever();
    await until(() => manager.getSnapshot().generation > generation, "reconnected");
    expect(manager.getSnapshot().status).toBe("connected");
    // PRE-EPOCH BEHAVIOUR (M5-02 D3): the in-memory cursor is resumed silently because M5-02
    // has no store epoch. Against M5-02b (EpochMismatch -32018, commit 27181dbb) this must
    // flip at integration: a changed epoch has to force a fresh baseline and history.incomplete.
    expect(manager.getSnapshot().resumedFrom).toBe(sequenceBefore);
    expect(statuses).toContain("reconnecting");

    const cancel = await data.cancelTask(id);
    expect(cancel.ok && cancel.value.state).toBe("Cancelled");
    await until(
      () => data.getSnapshot().tasks.data?.find((t) => t.contract.id === id)?.state === "Cancelled",
      "cancel reflected",
    );
    const missing = await data.cancelTask("task-does-not-exist");
    expect(missing.ok).toBe(false);
    expect(!missing.ok && missing.error).toBeInstanceOf(DaemonError);
    manager.stop();
    data.dispose();
  });

  it("reports auth failure without retrying", async () => {
    const bad = new NodeConnector(daemon.dataDir);
    bad.tokenOverride = "f".repeat(64);
    const manager = new ConnectionManager({ connector: bad, backoffMs: [20] });
    manager.start();
    await until(() => manager.getSnapshot().status === "auth-failed", "auth-failed");
    await new Promise((r) => setTimeout(r, 150));
    expect(manager.getSnapshot().status).toBe("auth-failed");
    expect(bad.opened.length).toBe(1); // one attempt, never retried
    manager.stop();
  });
});
