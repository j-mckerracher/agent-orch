import { afterEach, describe, expect, it } from "vitest";
import { ConnectionManager } from "../daemon/connection";
import { FakeDaemon, mkTask, until } from "../test/fakeDaemon";
import { createDaemonSources, toAgentCapability } from "./daemon";

const disposables: Array<() => void> = [];
afterEach(() => {
  while (disposables.length) disposables.pop()!();
});

async function setup(fake: FakeDaemon, start = true) {
  const connection = new ConnectionManager({ connector: fake, backoffMs: [5], requestTimeoutMs: 100 });
  disposables.push(() => connection.stop());
  const sources = createDaemonSources(connection);
  if (start) {
    connection.start();
    await until(() => connection.getSnapshot().status === "connected", "connected");
  }
  return { connection, sources };
}

const FEATURES = {
  streaming: true,
  interactive_input: false,
  structured_output: true,
  cancellation: true,
  session_resumption: false,
  approvals: false,
  model_selection: false,
};

describe("daemon-backed D1-D6 adapters", () => {
  it("labels every adapter daemon except the D3 recovery fixture", async () => {
    const { sources } = await setup(new FakeDaemon());
    expect(sources.agents.provenance).toEqual({ kind: "daemon", id: "D1" });
    expect(sources.execution.provenance).toEqual({ kind: "daemon", id: "D2" });
    expect(sources.retry.provenance).toEqual({ kind: "daemon", id: "D4" });
    expect(sources.coordinator.provenance).toEqual({ kind: "daemon", id: "D5" });
    expect(sources.journal.provenance).toEqual({ kind: "daemon", id: "D6" });
    expect(sources.recovery.provenance.kind).toBe("fixture");
  });

  it("maps agents.capabilities for D1 and derives D2 availability from the same probe", async () => {
    const fake = new FakeDaemon();
    fake.stub("agents.capabilities", [
      { agent: "codex", installed: true, launchable: true, capabilities: FEATURES },
      { agent: "pi", installed: true, launchable: false, capabilities: null },
    ]);
    const { sources } = await setup(fake);
    const agents = await sources.agents.listAgents();
    expect(agents).toMatchObject({
      state: "ready",
      data: [
        { id: "codex", availability: "installed", features: ["streaming", "structured output", "cancellation"] },
        { id: "pi", availability: "unsupported" },
      ],
    });
    const availability = await sources.execution.availability();
    expect(availability).toMatchObject({ state: "ready", data: { launchAvailable: true } });
    expect(fake.callsTo("agents.capabilities")).toHaveLength(1);
  });

  it("reports no launchable agent honestly", async () => {
    const fake = new FakeDaemon();
    fake.stub("agents.capabilities", [{ agent: "codex", installed: false, launchable: true, capabilities: null }]);
    const { sources } = await setup(fake);
    expect(await sources.execution.availability()).toMatchObject({ state: "ready", data: { launchAvailable: false } });
    expect(await sources.agents.listAgents()).toMatchObject({
      state: "ready",
      data: [{ availability: "available" }],
    });
  });

  it("marks entries the daemon cannot launch as unsupported", () => {
    expect(toAgentCapability({ agent: "pi", installed: false, launchable: false, capabilities: null }, "t")).toMatchObject({
      availability: "unsupported",
    });
  });

  it("says unavailable, never empty, when disconnected", async () => {
    const { sources } = await setup(new FakeDaemon(), false);
    expect(await sources.agents.listAgents()).toMatchObject({ state: "unavailable" });
    expect(await sources.execution.availability()).toMatchObject({ state: "unavailable" });
    expect(await sources.coordinator.conversation("/repo")).toMatchObject({ state: "unavailable" });
    expect(await sources.journal.identity()).toMatchObject({ state: "unavailable" });
  });

  it("says unavailable when the daemon lacks the method", async () => {
    const { sources } = await setup(new FakeDaemon());
    expect(await sources.agents.listAgents()).toMatchObject({ state: "unavailable" });
  });

  it("offers retry only for Failed and VerificationFailed tasks", async () => {
    const fake = new FakeDaemon();
    fake.tasks = [
      mkTask("t-failed", "/repo", { state: "Failed" }),
      mkTask("t-queued", "/repo", { state: "Queued" }),
    ];
    const { sources } = await setup(fake);
    const failed = await sources.retry.retryFor({ origin: "real", taskId: "t-failed", state: "Failed" });
    expect(failed).toMatchObject({ state: "ready", data: { available: true } });
    const queued = await sources.retry.retryFor({ origin: "real", taskId: "t-queued", state: "Queued" });
    expect(queued).toMatchObject({ state: "ready", data: { available: false } });
    expect(JSON.stringify(failed)).not.toMatch(/simulated/i);
  });

  it("retries through tasks.retry and reuses the key only after an indeterminate failure", async () => {
    const fake = new FakeDaemon();
    fake.tasks = [mkTask("t-failed", "/repo", { state: "Failed" })];
    fake.stub("tasks.retry", (params: Record<string, unknown>) => ({ ...mkTask(String(params.task_id), "/repo"), state: "Queued" }));
    const { sources } = await setup(fake);
    const ref = { origin: "real", taskId: "t-failed", state: "Failed" } as const;
    expect(await sources.retry.retry?.(ref)).toEqual({ state: "Queued", attempts: 0 });
    await sources.retry.retry?.(ref);
    const keys = fake.callsTo("tasks.retry").map((c) => (c.params as { idempotency_key: string }).idempotency_key);
    expect(keys).toHaveLength(2);
    expect(keys[0]).not.toBe(keys[1]);
  });

  it("rejects simulated actions and fixture refs instead of fabricating receipts", async () => {
    const { sources } = await setup(new FakeDaemon());
    await expect(sources.retry.simulateRetry({ origin: "fixture", id: "x" })).rejects.toMatchObject({
      kind: "method-unavailable",
    });
    await expect(sources.retry.retry?.({ origin: "fixture", id: "x" })).rejects.toMatchObject({
      kind: "method-unavailable",
    });
    await expect(sources.execution.simulateLaunch("x")).rejects.toMatchObject({ kind: "method-unavailable" });
    await expect(sources.coordinator.previewReply("/repo", "x")).rejects.toMatchObject({ kind: "method-unavailable" });
  });

  it("reads D5 across pages and records messages and proposals", async () => {
    const fake = new FakeDaemon();
    const entry = (sequence: number, kind: "message" | "proposal", body: string, validation: unknown = null) => ({
      sequence,
      id: `e${sequence}`,
      project_id: "/repo",
      kind,
      author: "user",
      body,
      validation,
      created_at: "2026-01-01T00:00:00Z",
    });
    fake.stub("coordinator.get", (params: Record<string, unknown>) =>
      params.after_sequence === undefined
        ? { project_id: "/repo", entries: [entry(1, "message", "hello")], more: true, next_after_sequence: 1 }
        : {
            project_id: "/repo",
            entries: [
              entry(2, "proposal", "{}", {
                status: "invalid",
                authority: "none",
                errors: [{ code: "x", task_key: null, message: "bad shape" }],
              }),
            ],
            more: false,
            next_after_sequence: 2,
          },
    );
    fake.stub("coordinator.send_message", entry(3, "message", "hi"));
    fake.stub("coordinator.propose", entry(4, "proposal", "{}"));
    const { sources } = await setup(fake);
    const convo = await sources.coordinator.conversation("/repo");
    expect(convo).toMatchObject({
      state: "ready",
      data: {
        messages: [{ text: "hello" }],
        proposals: [{ validation: "invalid", validationErrors: ["bad shape"] }],
      },
    });
    await sources.coordinator.send?.("/repo", "hi");
    await sources.coordinator.propose?.("/repo", "{}");
    expect(fake.callsTo("coordinator.send_message")[0]?.params).toMatchObject({ project_id: "/repo", text: "hi" });
    expect(fake.callsTo("coordinator.propose")[0]?.params).toMatchObject({ proposal: "{}" });
  });

  it("is empty only when the daemon really returns no entries", async () => {
    const fake = new FakeDaemon();
    fake.stub("coordinator.get", { project_id: "/repo", entries: [], more: false, next_after_sequence: 0 });
    const { sources } = await setup(fake);
    expect(await sources.coordinator.conversation("/repo")).toEqual({ state: "empty" });
  });

  it("derives D6 from the Welcome epoch and never claims continuity it cannot prove", async () => {
    const fake = new FakeDaemon();
    const { sources } = await setup(fake);
    expect(await sources.journal.identity()).toEqual({
      state: "ready",
      data: { storeIdentity: "epoch-1", cursorRelationship: "unknown" },
    });
  });

  it("says D6 is unavailable when the daemon reports no epoch", async () => {
    const fake = new FakeDaemon();
    fake.journalEpoch = null;
    const { sources } = await setup(fake);
    expect(await sources.journal.identity()).toMatchObject({ state: "unavailable" });
  });
});
