import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { realpathSync } from "node:fs";
import { ConnectionManager } from "../src/daemon/connection";
import { DaemonApi } from "../src/daemon/api";
import { DaemonSession } from "../src/daemon/session";
import { M5_03_CAPABILITIES, type TaskSubmission } from "../src/daemon/types";
import { createDerivedSurfaces } from "../src/sources/surfaces/derived";
import { NodeConnector, startDaemon, type RunningDaemon } from "./harness";

let daemon: RunningDaemon;
let connector: NodeConnector;
let projectId: string;

const submission = (objective: string, dependencies: string[] = []): TaskSubmission => ({
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

beforeAll(async () => {
  daemon = await startDaemon();
  connector = new NodeConnector(daemon.dataDir);
  projectId = realpathSync(daemon.repo);
});

afterAll(async () => {
  await daemon?.stop();
});

describe("real daemon: queue.snapshot and the live D11 adapter", () => {
  it("negotiates the queue capability and returns the documented shape with the default 2/2 limits", async () => {
    const session = new DaemonSession(await connector.open(M5_03_CAPABILITIES));
    try {
      expect(session.welcome.capabilities).toContain("queue");
      const api = new DaemonApi(session);
      await api.projectsRegister(daemon.repo);
      const snapshot = await api.queueSnapshot();
      expect(snapshot.limits).toEqual({ max_concurrent_workers: 2, max_workers_per_project: 2 });
      expect(typeof snapshot.dispatch_blocked).toBe("boolean");
      expect(typeof snapshot.epoch).toBe("string");
      expect(Array.isArray(snapshot.running)).toBe(true);
      expect(Array.isArray(snapshot.queued)).toBe(true);
    } finally {
      session.close();
    }
  });

  it("maps queued tasks and their waiting reasons from the real daemon, and never reports unknown values as zero", async () => {
    const session = new DaemonSession(await connector.open(M5_03_CAPABILITIES));
    const api = new DaemonApi(session);
    await api.projectsRegister(daemon.repo);
    const first = await api.tasksSubmit("q-key-1", submission("first queued"));
    const second = await api.tasksSubmit("q-key-2", submission("second queued"));
    const third = await api.tasksSubmit("q-key-3", submission("third queued"));
    const dependent = await api.tasksSubmit("q-key-4", submission("depends on first", [first.contract.id]));
    const ids = [first, second, third, dependent].map((task) => task.contract.id);
    session.close();

    const manager = new ConnectionManager({ connector, backoffMs: [50], requestTimeoutMs: 5000 });
    manager.start();
    try {
      await until(() => manager.getSnapshot().status === "connected", "connected");
      const activity = createDerivedSurfaces(manager).activity;
      const result = await activity.activity();
      expect(result.status).toBe("ok");
      if (result.status !== "ok") return;
      expect(result.provenance).toMatchObject({ kind: "daemon", id: "D11" });
      expect(result.data.slots.status).toBe("ok");
      if (result.data.slots.status === "ok") expect(result.data.slots.value.limit).toBe(2);

      const rows = [...result.data.running, ...result.data.queued].filter((row) => ids.includes(row.taskId));
      expect(rows.map((row) => row.taskId).sort()).toEqual([...ids].sort());
      for (const row of rows) {
        expect(row.objective).not.toBe("Objective not available");
        expect(row.projectId).toBe(projectId);
      }
      const dependentRow = result.data.queued.find((row) => row.taskId === dependent.contract.id);
      expect(dependentRow).toBeDefined();
      expect(dependentRow!.waiting).toMatchObject({ reason: "dependency", detail: `Waiting for ${first.contract.id}.` });
      for (const row of result.data.queued) expect(row.waiting).not.toBeNull();
      expect(result.data.dispatchPaused.status).toBe("unsupported");
      expect(JSON.stringify(result.data)).not.toMatch(/fixture:/);
    } finally {
      manager.stop();
    }
  });

  it("keeps the same objectives the task list reports, so queue rows join the right task", async () => {
    const session = new DaemonSession(await connector.open(M5_03_CAPABILITIES));
    try {
      const api = new DaemonApi(session);
      const tasks = await api.tasksList();
      const snapshot = await api.queueSnapshot();
      const known = new Set(tasks.map((task) => task.contract.id));
      for (const entry of [...snapshot.running, ...snapshot.queued]) expect(known.has(entry.task_id)).toBe(true);
      for (const entry of snapshot.queued) {
        expect(tasks.find((task) => task.contract.id === entry.task_id)?.state).toBe("Queued");
      }
    } finally {
      session.close();
    }
  });
});
