import { afterEach, describe, expect, it, vi } from "vitest";
import { AppData, PROPOSALS_REFRESH_MS, timelineKey } from "./appData";
import { ConnectionManager } from "./connection";
import { DaemonError } from "./errors";
import { OPTIONAL_CAPABILITIES, type TaskSubmission } from "./types";
import { FakeDaemon, mkEvent, mkSession, mkTask, until } from "../test/fakeDaemon";

const submission: TaskSubmission = {
  project_id: "/repo",
  parent_id: null,
  objective: "do it",
  kind: "Implement",
  agent: "fake-agent",
  dependencies: [],
  verification: "None",
  delivery: "LocalReview",
  autonomy: "ApprovalRequired",
  max_retries: 1,
  max_repairs: 1,
};

const disposables: Array<() => void> = [];
afterEach(() => {
  while (disposables.length) disposables.pop()!();
});

function setup(fake: FakeDaemon, options: { maxAutoAttempts?: number } = {}) {
  const connection = new ConnectionManager({
    connector: fake,
    backoffMs: [5],
    requestTimeoutMs: 100,
    ...options,
  });
  const data = new AppData(connection, { coalesceMs: 10 });
  disposables.push(() => {
    connection.stop();
    data.dispose();
  });
  return { connection, data };
}

const status = (c: ConnectionManager) => c.getSnapshot().status;

describe("ConnectionManager", () => {
  it("connects, records negotiated capabilities, and tracks the last sequence", async () => {
    const fake = new FakeDaemon();
    fake.events = [mkEvent(1, "t", null), mkEvent(2, "t", null)];
    const { connection } = setup(fake);
    connection.start();
    await until(() => status(connection) === "connected");
    await until(() => connection.getSnapshot().lastSequence === 2);
    expect(connection.getSnapshot().capabilities).toContain("tasks");
    expect(connection.getSnapshot().lastEventAt).not.toBeNull();
  });

  it("does not retry an authentication failure", async () => {
    const fake = new FakeDaemon();
    fake.openError = new DaemonError("auth", "bad token", -32001);
    const { connection } = setup(fake);
    connection.start();
    await until(() => status(connection) === "auth-failed");
    await new Promise((r) => setTimeout(r, 40));
    expect(status(connection)).toBe("auth-failed");
    expect(fake.channels).toHaveLength(0);
  });

  it.each([
    ["version", -32003],
    ["capability-unavailable", -32002],
  ] as const)("reports %s as incompatible and does not retry", async (kind, code) => {
    const fake = new FakeDaemon();
    fake.openError = new DaemonError(kind, "nope", code);
    const { connection } = setup(fake);
    connection.start();
    await until(() => status(connection) === "incompatible");
    await new Promise((r) => setTimeout(r, 40));
    expect(status(connection)).toBe("incompatible");
  });

  it("requests every capability first and makes one handshake when the daemon offers them all", async () => {
    const fake = new FakeDaemon();
    const { connection } = setup(fake);
    connection.start();
    await until(() => status(connection) === "connected");
    expect(fake.openRequests).toHaveLength(1);
    for (const capability of OPTIONAL_CAPABILITIES) {
      expect(fake.openRequests[0]).toContain(capability);
      expect(connection.getSnapshot().capabilities).toContain(capability);
    }
  });

  it("stays connected without the optional capabilities when an older daemon refuses them", async () => {
    const fake = new FakeDaemon();
    fake.unknownCapabilities = ["queue", "host", "workstreams"];
    const { connection } = setup(fake);
    connection.start();
    await until(() => status(connection) === "connected");
    expect(fake.openRequests).toHaveLength(2);
    for (const capability of OPTIONAL_CAPABILITIES) {
      expect(fake.openRequests[0]).toContain(capability);
      expect(fake.openRequests[1]).not.toContain(capability);
      expect(connection.getSnapshot().capabilities).not.toContain(capability);
    }
    expect(fake.openRequests[1]).toContain("tasks");
    expect(connection.getSnapshot().capabilities).toContain("tasks");
  });

  it("asks for the optional capabilities again on the next connection, so an upgraded daemon is used in full", async () => {
    const fake = new FakeDaemon();
    fake.unknownCapabilities = ["host"];
    const { connection } = setup(fake);
    connection.start();
    await until(() => status(connection) === "connected");
    fake.unknownCapabilities = [];
    const generation = connection.getSnapshot().generation;
    connection.reconnect();
    await until(() => connection.getSnapshot().generation > generation);
    expect(connection.getSnapshot().capabilities).toContain("host");
  });

  it("is still incompatible when a required capability is refused", async () => {
    const fake = new FakeDaemon();
    fake.unknownCapabilities = ["queue", "tasks"];
    const { connection } = setup(fake);
    connection.start();
    await until(() => status(connection) === "incompatible");
    expect(fake.openRequests).toHaveLength(2);
    expect(fake.channels).toHaveLength(0);
  });

  it("does not retry without optional capabilities when none were requested", async () => {
    const fake = new FakeDaemon();
    fake.unknownCapabilities = ["tasks"];
    const connection = new ConnectionManager({ connector: fake, backoffMs: [5], capabilities: ["health", "tasks"] });
    disposables.push(() => connection.stop());
    connection.start();
    await until(() => status(connection) === "incompatible");
    expect(fake.openRequests).toHaveLength(1);
  });

  it("does not repeat the handshake without optional capabilities for other failures", async () => {
    const fake = new FakeDaemon();
    fake.openError = new DaemonError("version", "nope", -32003);
    const { connection } = setup(fake);
    connection.start();
    await until(() => status(connection) === "incompatible");
    expect(fake.openRequests).toHaveLength(1);
  });

  it("reconnects after a drop and resumes after the last applied sequence", async () => {
    const fake = new FakeDaemon();
    fake.events = [mkEvent(1, "t", null), mkEvent(2, "t", null)];
    const { connection } = setup(fake);
    connection.start();
    await until(() => connection.getSnapshot().lastSequence === 2);
    const generation = connection.getSnapshot().generation;
    fake.drop();
    await until(() => connection.getSnapshot().generation > generation);
    expect(status(connection)).toBe("connected");
    expect(connection.getSnapshot().resumedFrom).toBe(2);
    expect(fake.callsTo("events.subscribe").at(-1)!.params).toEqual({ after_sequence: 2, epoch: "epoch-1" });
    fake.emit(mkEvent(3, "t", null));
    await until(() => connection.getSnapshot().lastSequence === 3);
  });

  it("keeps the saved cursor on a manual reconnect and drops it only for a fresh one", async () => {
    const fake = new FakeDaemon();
    fake.events = [mkEvent(1, "t", null), mkEvent(2, "t", null)];
    const { connection } = setup(fake);
    connection.start();
    await until(() => connection.getSnapshot().lastSequence === 2);

    let generation = connection.getSnapshot().generation;
    connection.reconnect();
    await until(() => connection.getSnapshot().generation > generation && status(connection) === "connected");
    expect(connection.getSnapshot().resumedFrom).toBe(2);
    expect(fake.callsTo("events.subscribe").at(-1)!.params).toEqual({ after_sequence: 2, epoch: "epoch-1" });

    generation = connection.getSnapshot().generation;
    connection.reconnect({ fresh: true });
    await until(() => connection.getSnapshot().generation > generation && status(connection) === "connected");
    expect(connection.getSnapshot().resumedFrom).toBeNull();
    expect(fake.callsTo("events.subscribe").at(-1)!.params).toEqual({});
  });

  it("backs off, then stops auto-retry after repeated failures and offers a manual reconnect", async () => {
    const fake = new FakeDaemon();
    fake.openError = new DaemonError("disconnected", "down");
    const { connection } = setup(fake, { maxAutoAttempts: 2 });
    connection.start();
    await until(() => status(connection) === "disconnected");
    expect(connection.getSnapshot().failedAttempts).toBeGreaterThan(2);
    fake.openError = null;
    connection.reconnect();
    await until(() => status(connection) === "connected");
  });

  it("restarts from a fresh baseline on replay-gap and marks history incomplete", async () => {
    const fake = new FakeDaemon();
    fake.events = [mkEvent(1, "t", null)];
    const { connection } = setup(fake);
    connection.start();
    await until(() => connection.getSnapshot().lastSequence === 1);
    fake.failNext("events.subscribe", { code: -32017, message: "gap" });
    fake.drop();
    await until(() => connection.getSnapshot().history.incomplete);
    await until(() => status(connection) === "connected");
    expect(connection.getSnapshot().history.reason).toBe("replay-gap");
    expect(fake.callsTo("events.subscribe").at(-1)!.params).toEqual({});
  });

  it("resumes with the journal epoch and the cursor", async () => {
    const fake = new FakeDaemon();
    fake.events = [mkEvent(1, "t", null)];
    const { connection } = setup(fake);
    connection.start();
    await until(() => connection.getSnapshot().lastSequence === 1);
    expect(connection.getSnapshot().journalEpoch).toBe("epoch-1");
    fake.drop();
    await until(() => fake.callsTo("events.subscribe").length === 2);
    await until(() => status(connection) === "connected");
    expect(fake.callsTo("events.subscribe").at(-1)!.params).toEqual({ after_sequence: 1, epoch: "epoch-1" });
    expect(connection.getSnapshot().history.incomplete).toBe(false);
  });

  it("restarts from a fresh baseline on epoch-mismatch instead of resuming silently", async () => {
    const fake = new FakeDaemon();
    fake.events = [mkEvent(1, "t", null)];
    const { connection } = setup(fake);
    connection.start();
    await until(() => connection.getSnapshot().lastSequence === 1);
    // The daemon restarts on a replaced store: new epoch, same-looking cursor.
    fake.journalEpoch = "epoch-2";
    fake.drop();
    await until(() => connection.getSnapshot().history.reason === "epoch-mismatch");
    await until(() => status(connection) === "connected");
    expect(fake.callsTo("events.subscribe").at(-1)!.params).toEqual({});
    expect(connection.getSnapshot().journalEpoch).toBe("epoch-2");
    expect(connection.getSnapshot().history.incomplete).toBe(true);
  });

  it("treats a stream-level epoch-mismatch like a replay gap", async () => {
    const fake = new FakeDaemon();
    const { connection } = setup(fake);
    connection.start();
    await until(() => status(connection) === "connected");
    fake.pushStreamError(-32018, "epoch");
    await until(() => connection.getSnapshot().history.reason === "epoch-mismatch");
    await until(() => status(connection) === "connected");
    expect(connection.getSnapshot().lastSequence).toBe(0);
  });

  it("works against a daemon that does not advertise an epoch", async () => {
    const fake = new FakeDaemon();
    fake.journalEpoch = null;
    fake.events = [mkEvent(1, "t", null)];
    const { connection } = setup(fake);
    connection.start();
    await until(() => connection.getSnapshot().lastSequence === 1);
    expect(connection.getSnapshot().journalEpoch).toBeNull();
    fake.drop();
    await until(() => fake.callsTo("events.subscribe").length === 2);
    expect(fake.callsTo("events.subscribe").at(-1)!.params).toEqual({ after_sequence: 1 });
  });

  it("marks history incomplete on a slow-consumer stream error and reconnects", async () => {
    const fake = new FakeDaemon();
    const { connection } = setup(fake);
    connection.start();
    await until(() => status(connection) === "connected");
    fake.pushStreamError(-32015, "slow");
    await until(() => connection.getSnapshot().history.incomplete);
    expect(connection.getSnapshot().history.reason).toBe("slow-consumer");
    await until(() => status(connection) === "connected");
    // Only a fresh manual baseline clears the mark.
    expect(connection.getSnapshot().history.incomplete).toBe(true);
    connection.reconnect({ fresh: true });
    await until(() => status(connection) === "connected" && !connection.getSnapshot().history.incomplete);
    expect(connection.getSnapshot().lastSequence).toBe(0);
  });

  it("api() throws while not connected", () => {
    const { connection } = setup(new FakeDaemon());
    expect(() => connection.api()).toThrowError(/not connected/);
  });

  it("stop() returns to idle and drops the connection", async () => {
    const fake = new FakeDaemon();
    const { connection } = setup(fake);
    connection.start();
    await until(() => status(connection) === "connected");
    connection.stop();
    expect(status(connection)).toBe("idle");
    expect(fake.live).toHaveLength(0);
  });
});

describe("AppData", () => {
  it("loads snapshots on connect and refreshes after a live event", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: "/repo" }];
    const { connection, data } = setup(fake);
    connection.start();
    await until(() => data.getSnapshot().tasks.status === "ready");
    expect(data.getSnapshot().projects.data).toEqual([{ id: "/repo" }]);
    fake.tasks.push(mkTask("task-9", "/repo"));
    fake.emit(mkEvent(1, "task-9", null, "TaskAdmitted"));
    await until(() => data.getSnapshot().tasks.data?.length === 1);
  });

  it("issues a new tasks.list refresh when a live event arrives", async () => {
    const fake = new FakeDaemon();
    const { connection, data } = setup(fake);
    connection.start();
    await until(() => data.getSnapshot().tasks.status === "ready");
    await new Promise((r) => setTimeout(r, 40));
    const before = fake.callsTo("tasks.list").length;
    fake.emit(mkEvent(1, "t", null));
    await until(() => fake.callsTo("tasks.list").length > before);
  });

  it("coalesces a burst of events into few refreshes", async () => {
    const fake = new FakeDaemon();
    const { connection, data } = setup(fake);
    connection.start();
    await until(() => data.getSnapshot().tasks.status === "ready");
    const before = fake.callsTo("tasks.list").length;
    for (let i = 1; i <= 20; i++) fake.emit(mkEvent(i, "t", null));
    await until(() => connection.getSnapshot().lastSequence === 20);
    await new Promise((r) => setTimeout(r, 80));
    expect(fake.callsTo("tasks.list").length - before).toBeLessThanOrEqual(3);
  });

  it("keeps previous data and records an error when a refresh fails", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: "/repo" }];
    const { connection, data } = setup(fake);
    connection.start();
    await until(() => data.getSnapshot().projects.status === "ready");
    fake.failNext("projects.list", { code: -32603, message: "boom" });
    await data.refreshAll();
    const projects = data.getSnapshot().projects;
    expect(projects.status).toBe("error");
    expect(projects.data).toEqual([{ id: "/repo" }]);
    expect(projects.error?.kind).toBe("internal");
  });

  it("submits, and reuses the same idempotency key only after an indeterminate failure", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: "/repo" }];
    const { connection, data } = setup(fake);
    connection.start();
    await until(() => status(connection) === "connected");
    const keys = () => fake.callsTo("tasks.submit").map((c) => (c.params as { idempotency_key: string }).idempotency_key);

    fake.failNext("tasks.submit", "hang");
    const first = await data.submitTask(submission);
    expect(first.ok).toBe(false);
    expect(!first.ok && first.error.kind).toBe("timeout");
    const second = await data.submitTask(submission);
    expect(second.ok).toBe(true);
    expect(keys()[0]).toBe(keys()[1]);

    await data.submitTask(submission);
    expect(keys()[2]).not.toBe(keys()[1]);
  });

  it("mints a new key after a definitive failure or when the form changes", async () => {
    const fake = new FakeDaemon();
    const { connection, data } = setup(fake);
    connection.start();
    await until(() => status(connection) === "connected");
    const keys = () => fake.callsTo("tasks.submit").map((c) => (c.params as { idempotency_key: string }).idempotency_key);

    fake.failNext("tasks.submit", { code: -32602, message: "invalid" });
    expect((await data.submitTask(submission)).ok).toBe(false);
    await data.submitTask(submission);
    expect(keys()[0]).not.toBe(keys()[1]);

    fake.failNext("tasks.submit", "hang");
    await data.submitTask({ ...submission, objective: "one" });
    await data.submitTask({ ...submission, objective: "two" });
    expect(keys()[2]).not.toBe(keys()[3]);
  });

  it("scopes the submit idempotency key per project", async () => {
    const fake = new FakeDaemon();
    const { connection, data } = setup(fake);
    connection.start();
    await until(() => status(connection) === "connected");
    const keys = () => fake.callsTo("tasks.submit").map((c) => (c.params as { idempotency_key: string }).idempotency_key);

    fake.failNext("tasks.submit", "hang");
    expect((await data.submitTask({ ...submission, project_id: "/a" })).ok).toBe(false);
    expect(data.tracker.pending("submit:/a")).toBe(true);
    expect((await data.submitTask({ ...submission, project_id: "/b" })).ok).toBe(true);
    expect(data.tracker.pending("submit:/a")).toBe(true);
    expect((await data.submitTask({ ...submission, project_id: "/a" })).ok).toBe(true);
    expect(keys()[2]).toBe(keys()[0]);
    expect(keys()[1]).not.toBe(keys()[0]);
  });

  it("reports a failed mutation while disconnected without throwing", async () => {
    const { data } = setup(new FakeDaemon());
    const result = await data.submitTask(submission);
    expect(result.ok).toBe(false);
    expect(!result.ok && result.error.kind).toBe("disconnected");
  });

  it("cancels a task with an idempotency key and refreshes", async () => {
    const fake = new FakeDaemon();
    fake.tasks = [mkTask("task-1", "/repo")];
    const { connection, data } = setup(fake);
    connection.start();
    await until(() => status(connection) === "connected");
    const result = await data.cancelTask("task-1");
    expect(result.ok && result.value.state).toBe("Cancelled");
    expect(fake.callsTo("tasks.cancel")[0]!.params).toMatchObject({ task_id: "task-1" });
    expect((fake.callsTo("tasks.cancel")[0]!.params as { idempotency_key?: string }).idempotency_key).toEqual(
      expect.any(String),
    );
    await until(() => data.getSnapshot().tasks.data?.[0]?.state === "Cancelled");
  });

  describe("timelines", () => {
    function seeded() {
      const fake = new FakeDaemon();
      fake.pageSize = 2;
      fake.tasks = [mkTask("t1", "/repo", { sessions: [mkSession("s1")] })];
      fake.events = [1, 2, 3, 4, 5].map((n) => mkEvent(n, "t1", "s1"));
      return fake;
    }

    it("pages with a cursor and never duplicates a sequence", async () => {
      const fake = seeded();
      const { connection, data } = setup(fake);
      connection.start();
      await until(() => status(connection) === "connected");
      const key = timelineKey("t1", "s1");
      await data.loadTimeline("t1", "s1", "reset");
      let timeline = data.getSnapshot().timelines[key]!;
      expect(timeline.events.map((e) => e.sequence)).toEqual([1, 2]);
      expect(timeline.more).toBe(true);
      await data.loadTimeline("t1", "s1", "more");
      await data.loadTimeline("t1", "s1", "more");
      timeline = data.getSnapshot().timelines[key]!;
      expect(timeline.events.map((e) => e.sequence)).toEqual([1, 2, 3, 4, 5]);
      expect(timeline.more).toBe(false);
      expect(fake.callsTo("sessions.events").map((c) => (c.params as { after_sequence?: number }).after_sequence)).toEqual(
        [undefined, 2, 4],
      );
    });

    it("reloads a loaded timeline from the start after a reconnect, not from a saved cursor", async () => {
      const fake = seeded();
      fake.pageSize = 256;
      const { connection, data } = setup(fake);
      connection.start();
      await until(() => status(connection) === "connected");
      await data.loadTimeline("t1", "s1", "reset");
      const generation = connection.getSnapshot().generation;
      fake.events = [mkEvent(1, "t1", "s1"), mkEvent(2, "t1", "s1")]; // store reset: journal replaced
      fake.drop();
      await until(() => connection.getSnapshot().generation > generation);
      await until(() => data.getSnapshot().timelines[timelineKey("t1", "s1")]?.events.length === 2);
      expect(fake.callsTo("sessions.events").at(-1)!.params).not.toHaveProperty("after_sequence");
    });

    it("appends a live event for a fully loaded timeline", async () => {
      const fake = seeded();
      fake.pageSize = 256;
      const { connection, data } = setup(fake);
      connection.start();
      await until(() => status(connection) === "connected");
      await data.loadTimeline("t1", "s1", "reset");
      fake.emit(mkEvent(6, "t1", "s1"));
      await until(() => data.getSnapshot().timelines[timelineKey("t1", "s1")]!.events.length === 6);
    });

    it("drops sequences the daemon returns again in an overlapping page", async () => {
      const fake = seeded();
      fake.ignoreAfterSequence = true;
      const { connection, data } = setup(fake);
      connection.start();
      await until(() => status(connection) === "connected");
      await data.loadTimeline("t1", "s1", "reset");
      await data.loadTimeline("t1", "s1", "more");
      const timeline = data.getSnapshot().timelines[timelineKey("t1", "s1")]!;
      expect(timeline.events.map((e) => e.sequence)).toEqual([1, 2]);
    });

    it("discards the result of a superseded load", async () => {
      const fake = seeded();
      const { connection, data } = setup(fake);
      connection.start();
      await until(() => status(connection) === "connected");
      await data.loadTimeline("t1", "s1", "reset");

      fake.delayNext("sessions.events", 60);
      const slow = data.loadTimeline("t1", "s1", "more");
      fake.events = [mkEvent(7, "t1", "s1"), mkEvent(8, "t1", "s1")];
      await data.loadTimeline("t1", "s1", "reset");
      await slow;
      const timeline = data.getSnapshot().timelines[timelineKey("t1", "s1")]!;
      expect(timeline.events.map((e) => e.sequence)).toEqual([7, 8]);
    });

    it("records a load error without losing already loaded events", async () => {
      const fake = seeded();
      const { connection, data } = setup(fake);
      connection.start();
      await until(() => status(connection) === "connected");
      await data.loadTimeline("t1", "s1", "reset");
      fake.failNext("sessions.events", { code: -32603, message: "boom" });
      await data.loadTimeline("t1", "s1", "more");
      const timeline = data.getSnapshot().timelines[timelineKey("t1", "s1")]!;
      expect(timeline.events).toHaveLength(2);
      expect(timeline.error?.kind).toBe("internal");
    });
  });
});

describe("AppData proposals", () => {
  const pendingWire = (id: string) => ({
    id,
    turn_id: "turn-1",
    index: 0,
    kind: "task.propose",
    params: { kind: "task.propose", project: "p-1", objective: "Add unit tests", agent: "codex" },
    digest: "d".repeat(64),
    depends_on: null,
    state: "pending",
    result_ref: null,
    error_code: null,
    denial: null,
    target: { project_name: "alpha" },
    created_at: "2026-11-10T21:00:00Z",
    updated_at: "2026-11-10T21:00:00Z",
    scope: { kind: "global" },
  });
  const stubPending = (fake: FakeDaemon, ids: string[] = ["proposal-1"]) =>
    fake.stub("actions.pending", () => ({ proposals: ids.map(pendingWire), more: false }));
  const every = (data: AppData) =>
    (["projects", "tasks", "sessions", "approvals", "proposals"] as const).every((key) => data.getSnapshot()[key].status === "ready");
  const setHidden = (hidden: boolean) => {
    Object.defineProperty(document, "hidden", { configurable: true, get: () => hidden });
    document.dispatchEvent(new Event("visibilitychange"));
  };
  const idleFor = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

  afterEach(() => {
    Reflect.deleteProperty(document, "hidden");
    vi.useRealTimers();
  });

  it("proposals_slice_loads_the_pending_list_on_connect", async () => {
    const fake = new FakeDaemon();
    stubPending(fake, ["proposal-1", "proposal-2"]);
    const { connection, data } = setup(fake);
    expect(data.getSnapshot().proposals).toEqual({ status: "idle", data: null, error: null });
    connection.start();
    await until(() => data.getSnapshot().proposals.status === "ready");
    expect(data.getSnapshot().proposals.data?.map((item) => [item.id, item.scope])).toEqual([
      ["proposal-1", { kind: "global" }],
      ["proposal-2", { kind: "global" }],
    ]);
  });

  it("proposals_slice_is_an_empty_loaded_list_for_a_daemon_without_actions_pending", async () => {
    const fake = new FakeDaemon();
    const { connection, data } = setup(fake);
    connection.start();
    await until(() => data.getSnapshot().proposals.status === "ready");
    expect(data.getSnapshot().proposals).toEqual({ status: "ready", data: [], error: null });
    expect(data.getSnapshot().approvals.status).toBe("ready");
  });

  it("refresh_all_loads_the_proposals_again_with_the_other_lists", async () => {
    const fake = new FakeDaemon();
    stubPending(fake);
    const { connection, data } = setup(fake);
    connection.start();
    await until(() => every(data));
    const before = fake.callsTo("actions.pending").length;
    await data.refreshAll();
    expect(fake.callsTo("actions.pending").length).toBeGreaterThan(before);
  });

  it("refresh_proposals_sends_nothing_while_disconnected", async () => {
    const fake = new FakeDaemon();
    const { data } = setup(fake);
    await data.refreshProposals();
    expect(fake.callsTo("actions.pending")).toHaveLength(0);
    expect(data.getSnapshot().proposals.status).toBe("idle");
  });

  it("a_failed_proposals_read_keeps_the_previous_list_and_records_the_error", async () => {
    const fake = new FakeDaemon();
    stubPending(fake);
    const { connection, data } = setup(fake);
    connection.start();
    await until(() => every(data));
    fake.failNext("actions.pending", { code: -32603, message: "store busy" });
    await data.refreshProposals();
    const slice = data.getSnapshot().proposals;
    expect(slice.status).toBe("error");
    expect(slice.error?.kind).toBe("internal");
    expect(slice.data?.map((item) => item.id)).toEqual(["proposal-1"]);
  });

  it("proposals_are_read_every_30_seconds_while_the_window_is_visible_and_not_while_it_is_hidden", async () => {
    // Only the intervals are faked: the connection and `until` keep their real timers.
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    const fake = new FakeDaemon();
    stubPending(fake);
    const { connection, data } = setup(fake);
    setHidden(false);
    connection.start();
    await until(() => every(data));
    await idleFor(30);
    const reads = () => fake.callsTo("actions.pending").length;
    const first = reads();

    vi.advanceTimersByTime(PROPOSALS_REFRESH_MS - 1);
    await idleFor(30);
    expect(reads()).toBe(first);
    vi.advanceTimersByTime(1);
    await until(() => reads() === first + 1);

    setHidden(true);
    vi.advanceTimersByTime(3 * PROPOSALS_REFRESH_MS);
    await idleFor(30);
    expect(reads()).toBe(first + 1);

    setHidden(false);
    vi.advanceTimersByTime(PROPOSALS_REFRESH_MS);
    await until(() => reads() === first + 2);
  });

  it("each_app_data_starts_one_proposals_interval_and_dispose_clears_it", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    const fake = new FakeDaemon();
    stubPending(fake);
    const connection = new ConnectionManager({ connector: fake, backoffMs: [5], requestTimeoutMs: 100 });
    expect(vi.getTimerCount()).toBe(0);
    const first = new AppData(connection, { coalesceMs: 10 });
    expect(vi.getTimerCount()).toBe(1);
    const second = new AppData(connection, { coalesceMs: 10 });
    expect(vi.getTimerCount()).toBe(2);
    try {
      setHidden(false);
      connection.start();
      await until(() => every(second));
      await idleFor(30);
      first.dispose();
      expect(vi.getTimerCount()).toBe(1);
      const reads = fake.callsTo("actions.pending").length;
      second.dispose();
      second.dispose();
      expect(vi.getTimerCount()).toBe(0);
      vi.advanceTimersByTime(5 * PROPOSALS_REFRESH_MS);
      await idleFor(30);
      expect(fake.callsTo("actions.pending")).toHaveLength(reads);
    } finally {
      connection.stop();
      first.dispose();
      second.dispose();
    }
  });
});
