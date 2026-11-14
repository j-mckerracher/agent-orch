import { describe, expect, it } from "vitest";
import { FakeDaemon, mkTask } from "../test/fakeDaemon";
import { fakeQueueSnapshot, DEFAULT_QUEUE_SETTINGS } from "../test/queueSnapshot";
import { DaemonApi } from "./api";
import { parseQueueSnapshot } from "./queue";
import { DaemonSession } from "./session";
import { ALL_CAPABILITIES, M5_03_CAPABILITIES } from "./types";

const valid = () => fakeQueueSnapshot([mkTask("r1", "/p", { state: "Running" }), mkTask("q1", "/p", { state: "Queued" })], DEFAULT_QUEUE_SETTINGS);

describe("parseQueueSnapshot", () => {
  it("accepts the daemon shape and keeps unknown reasons as text", () => {
    const wire = valid();
    wire.queued[0]!.waiting_reason = "future_reason";
    const parsed = parseQueueSnapshot(wire);
    expect(parsed.limits).toEqual({ max_concurrent_workers: 2, max_workers_per_project: 2 });
    expect(parsed.running).toEqual([{ task_id: "r1", project_id: "/p", agent: "fake-agent" }]);
    expect(parsed.queued[0]).toMatchObject({ task_id: "q1", waiting_reason: "future_reason", position: 1 });
  });

  it("treats a missing position or waiting detail as null, never as a number", () => {
    const wire = valid() as unknown as { queued: Array<Record<string, unknown>> };
    delete wire.queued[0]!.position;
    delete wire.queued[0]!.waiting_detail;
    expect(parseQueueSnapshot(wire).queued[0]).toMatchObject({ position: null, waiting_detail: null });
  });

  it.each([
    ["a non-object", null],
    ["a missing limits block", { ...valid(), limits: undefined }],
    ["a string limit", { ...valid(), limits: { max_concurrent_workers: "2", max_workers_per_project: 2 } }],
    ["a negative limit", { ...valid(), limits: { max_concurrent_workers: -1, max_workers_per_project: 2 } }],
    ["a missing dispatch_blocked", { ...valid(), dispatch_blocked: undefined }],
    ["a missing running list", { ...valid(), running: undefined }],
    ["a running entry without an agent", { ...valid(), running: [{ task_id: "r1", project_id: "/p" }] }],
    ["a queued entry without a reason", { ...valid(), queued: [{ task_id: "q1", project_id: "/p", position: 1 }] }],
  ])("rejects %s as a protocol error", (_label, value) => {
    expect(() => parseQueueSnapshot(value)).toThrowError(expect.objectContaining({ kind: "protocol" }));
  });
});

describe("queue capability", () => {
  it("is part of the capability request", () => {
    expect(ALL_CAPABILITIES).toContain("queue");
    expect(M5_03_CAPABILITIES).toContain("queue");
  });

  it("requests queue.snapshot through the queue capability and parses the result", async () => {
    const fake = new FakeDaemon();
    fake.tasks = [mkTask("r1", "/p", { state: "Running" })];
    const api = new DaemonApi(new DaemonSession(await fake.open(M5_03_CAPABILITIES), { requestTimeoutMs: 200 }));
    const snapshot = await api.queueSnapshot();
    expect(snapshot.running.map((entry) => entry.task_id)).toEqual(["r1"]);
    expect(fake.callsTo("queue.snapshot")[0]?.params ?? {}).toEqual({});
  });

  it("refuses to call queue.snapshot when the capability was not negotiated", async () => {
    const fake = new FakeDaemon();
    fake.capabilitiesOverride = ["tasks"];
    const api = new DaemonApi(new DaemonSession(await fake.open(M5_03_CAPABILITIES), { requestTimeoutMs: 200 }));
    await expect(api.queueSnapshot()).rejects.toMatchObject({ kind: "capability-not-negotiated" });
    expect(fake.callsTo("queue.snapshot")).toHaveLength(0);
  });
});
