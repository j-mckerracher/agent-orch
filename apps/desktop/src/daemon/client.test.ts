import { describe, expect, it } from "vitest";
import { DaemonApi, isFixtureId } from "./api";
import { DaemonError } from "./errors";
import { IdempotencyTracker } from "./idempotency";
import { DaemonSession } from "./session";
import { M5_03_CAPABILITIES, type DaemonEvent } from "./types";
import { FakeDaemon, mkEvent, until } from "../test/fakeDaemon";

async function open(fake: FakeDaemon, requestTimeoutMs = 1000) {
  return new DaemonSession(await fake.open(M5_03_CAPABILITIES), { requestTimeoutMs });
}

describe("DaemonSession", () => {
  it("correlates concurrent requests by id", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: "/p" }];
    const session = await open(fake);
    const [projects, tasks] = await Promise.all([session.request("projects.list"), session.request("tasks.list")]);
    expect(projects).toEqual([{ id: "/p" }]);
    expect(tasks).toEqual([]);
  });

  it("maps an RPC error code to a typed error", async () => {
    const session = await open(new FakeDaemon());
    await expect(session.request("tasks.get", { task_id: "nope" })).rejects.toMatchObject({
      kind: "not-found",
      code: -32010,
    });
  });

  it("rejects locally, without sending, a method whose capability was not negotiated", async () => {
    const fake = new FakeDaemon();
    const session = new DaemonSession(await fake.open(["health"]));
    await expect(session.request("tasks.list")).rejects.toMatchObject({ kind: "capability-not-negotiated" });
    expect(fake.callsTo("tasks.list")).toHaveLength(0);
  });

  it("times out an unanswered request", async () => {
    const fake = new FakeDaemon();
    fake.failNext("tasks.list", "hang");
    const session = await open(fake, 20);
    await expect(session.request("tasks.list")).rejects.toMatchObject({ kind: "timeout" });
  });

  it("fails pending requests and later requests when the channel closes", async () => {
    const fake = new FakeDaemon();
    fake.failNext("tasks.list", "hang");
    const session = await open(fake);
    const pending = session.request("tasks.list");
    fake.drop();
    await expect(pending).rejects.toMatchObject({ kind: "disconnected" });
    await expect(session.request("projects.list")).rejects.toMatchObject({ kind: "disconnected" });
    expect(session.closed).toBe(true);
  });

  it("delivers replayed then live events in order and applies a duplicate once", async () => {
    const fake = new FakeDaemon();
    fake.events = [mkEvent(1, "t", "s"), mkEvent(2, "t", "s")];
    const session = await open(fake);
    const seen: number[] = [];
    await session.subscribe(0, { event: (e) => seen.push(e.sequence), error: () => undefined });
    await until(() => seen.length === 2);
    fake.emit(mkEvent(3, "t", "s"));
    const channel = fake.live[0]!;
    channel.deliver({
      jsonrpc: "2.0",
      method: "events.event",
      params: { subscription_id: channel.subscriptionId, event: mkEvent(3, "t", "s") },
    });
    await until(() => seen.length === 3);
    expect(seen).toEqual([1, 2, 3]);
  });

  it("resumes after the given cursor", async () => {
    const fake = new FakeDaemon();
    fake.events = [mkEvent(1, "t", "s"), mkEvent(2, "t", "s"), mkEvent(3, "t", "s")];
    const session = await open(fake);
    const seen: number[] = [];
    await session.subscribe(2, { event: (e) => seen.push(e.sequence), error: () => undefined });
    await until(() => seen.length === 1);
    expect(seen).toEqual([3]);
    expect(fake.callsTo("events.subscribe")[0]!.params).toEqual({ after_sequence: 2 });
  });

  it("ends the stream with replay-gap when a sequence is skipped", async () => {
    const fake = new FakeDaemon();
    const session = await open(fake);
    const errors: DaemonError[] = [];
    await session.subscribe(0, { event: () => undefined, error: (e) => errors.push(e) });
    fake.emit(mkEvent(1, "t", "s"));
    fake.emit(mkEvent(3, "t", "s"));
    expect(errors.map((e) => e.kind)).toEqual(["replay-gap"]);
  });

  it("surfaces events.error as a typed stream failure", async () => {
    const fake = new FakeDaemon();
    const session = await open(fake);
    const errors: DaemonError[] = [];
    await session.subscribe(0, { event: () => undefined, error: (e) => errors.push(e) });
    fake.pushStreamError(-32015, "slow consumer");
    expect(errors.map((e) => e.kind)).toEqual(["slow-consumer"]);
  });

  it("refuses a second subscription on one connection", async () => {
    const session = await open(new FakeDaemon());
    await session.subscribe(0, { event: () => undefined, error: () => undefined });
    await expect(session.subscribe(0, { event: () => undefined, error: () => undefined })).rejects.toMatchObject({
      kind: "conflict",
    });
  });

  it("treats a malformed frame as a protocol failure and closes", async () => {
    const fake = new FakeDaemon();
    const session = await open(fake);
    const closes: Array<DaemonError | null> = [];
    session.onClose((reason) => closes.push(reason));
    fake.live[0]!.deliver("not an object");
    expect(closes[0]).toMatchObject({ kind: "protocol" });
    expect(session.closed).toBe(true);
  });

  it("ignores notifications for an unknown subscription id", async () => {
    const fake = new FakeDaemon();
    const session = await open(fake);
    const seen: DaemonEvent[] = [];
    await session.subscribe(0, { event: (e) => seen.push(e), error: () => undefined });
    fake.live[0]!.deliver({
      jsonrpc: "2.0",
      method: "events.event",
      params: { subscription_id: "other", event: mkEvent(1, "t", "s") },
    });
    expect(seen).toEqual([]);
  });
});

describe("DaemonApi placeholder-id guard", () => {
  it("refuses fixture ids on every id-taking method without sending a request", async () => {
    const fake = new FakeDaemon();
    const api = new DaemonApi(await open(fake));
    const calls = [
      () => api.tasksGet("fixture:task-queued"),
      () => api.tasksCancel("fixture:task-queued", "key"),
      () => api.sessionsList("fixture:task-queued"),
      () => api.sessionsGet("t", "fixture:session-1"),
      () => api.sessionsEvents("fixture:task-queued", "s"),
    ];
    for (const call of calls) await expect(call()).rejects.toMatchObject({ kind: "invalid-params" });
    await expect(
      api.tasksSubmit("k", {
        project_id: "/p",
        objective: "o",
        kind: "Implement",
        agent: "a",
        dependencies: ["fixture:task-queued"],
        verification: "None",
        delivery: "LocalReview",
        autonomy: "ApprovalRequired",
        max_retries: 1,
        max_repairs: 1,
      }),
    ).rejects.toMatchObject({ kind: "invalid-params" });
    expect(fake.calls.filter((c) => c.method !== "events.subscribe")).toEqual([]);
    expect(isFixtureId("fixture:x")).toBe(true);
    expect(isFixtureId("task-1")).toBe(false);
  });
});

describe("IdempotencyTracker", () => {
  const timeout = new DaemonError("timeout", "t");
  const invalid = new DaemonError("invalid-params", "i", -32602);
  const make = () => {
    let n = 0;
    return new IdempotencyTracker(() => `key-${++n}`);
  };

  it("reuses a key after an indeterminate failure with an unchanged fingerprint", () => {
    const tracker = make();
    expect(tracker.keyFor("submit", "a")).toBe("key-1");
    tracker.settle("submit", timeout);
    expect(tracker.pending("submit")).toBe(true);
    expect(tracker.keyFor("submit", "a")).toBe("key-1");
  });

  it("mints a new key after a definitive failure, a success, or a changed fingerprint", () => {
    const tracker = make();
    tracker.keyFor("submit", "a");
    tracker.settle("submit", invalid);
    expect(tracker.keyFor("submit", "a")).toBe("key-2");
    tracker.settle("submit", null);
    expect(tracker.keyFor("submit", "a")).toBe("key-3");
    tracker.settle("submit", timeout);
    expect(tracker.keyFor("submit", "b")).toBe("key-4");
  });

  it("keeps scopes independent", () => {
    const tracker = make();
    expect(tracker.keyFor("cancel:1", "x")).not.toBe(tracker.keyFor("cancel:2", "x"));
  });
});
