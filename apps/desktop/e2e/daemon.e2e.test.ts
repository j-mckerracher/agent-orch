import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { realpathSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { AppData } from "../src/daemon/appData";
import { ConnectionManager, type ConnectionStatus } from "../src/daemon/connection";
import { DaemonError } from "../src/daemon/errors";
import { DaemonSession } from "../src/daemon/session";
import { DaemonApi } from "../src/daemon/api";
import { toReviewEvidence } from "../src/review/daemonSources";
import { M5_03_CAPABILITIES, type DaemonEvent, type TaskSubmission } from "../src/daemon/types";
import { NodeConnector, startDaemon, type RunningDaemon, type SeededReview } from "./harness";

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
  it("connects, loads snapshots, reflects a submit through live events, and resumes after a drop on the same journal", async () => {
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

    // An unsupported agent cannot succeed, so its dependent remains cancellable.
    const blockerSession = new DaemonSession(await new NodeConnector(daemon.dataDir).open(M5_03_CAPABILITIES));
    const blocker = await new DaemonApi(blockerSession).tasksSubmit("app-data-blocker", submission({ objective: "blocker" }));
    blockerSession.close();
    const result = await data.submitTask(submission({ objective: "via app data", dependencies: [blocker.contract.id] }));
    expect(result.ok).toBe(true);
    const id = result.ok ? result.value.contract.id : "";
    await until(
      () => data.getSnapshot().tasks.data?.some((t) => t.contract.id === id),
      "submitted task in the list",
    );
    expect(manager.getSnapshot().lastSequence).toBeGreaterThan(0);

    const generation = manager.getSnapshot().generation;
    const sequenceBefore = manager.getSnapshot().lastSequence;
    const epochBefore = manager.getSnapshot().journalEpoch;
    expect(epochBefore).toMatch(/\S/);
    connector.opened[connector.opened.length - 1]!.sever();
    await until(() => manager.getSnapshot().generation > generation, "reconnected");
    expect(manager.getSnapshot().status).toBe("connected");
    // The Welcome names the journal, so the cursor is resumed with its epoch and accepted.
    expect(manager.getSnapshot().journalEpoch).toBe(epochBefore);
    expect(manager.getSnapshot().resumedFrom).toBe(sequenceBefore);
    expect(manager.getSnapshot().history).toEqual({ incomplete: false, reason: null });
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

describe("real daemon: journal epoch", () => {
  it("names the journal in the Welcome and rejects a stale cursor against a replaced store (-32018)", async () => {
    const own = await startDaemon();
    try {
      const own_connector = new NodeConnector(own.dataDir);
      const first = new DaemonSession(await own_connector.open(M5_03_CAPABILITIES));
      const epoch = first.welcome.journal_epoch;
      expect(epoch).toMatch(/\S/);
      const api = new DaemonApi(first);
      await api.projectsRegister(own.repo);
      const seen: DaemonEvent[] = [];
      await first.subscribe(0, { event: (e) => seen.push(e), error: () => undefined });
      await api.tasksSubmit("epoch-key-1", { ...submission(), project_id: realpathSync(own.repo) });
      await until(() => seen.length > 0, "events");
      const cursor = seen[seen.length - 1]!.sequence;
      first.close();

      // The same store keeps its epoch, so the cursor resumes.
      await own.restart();
      const same = new DaemonSession(await own_connector.open(M5_03_CAPABILITIES));
      expect(same.welcome.journal_epoch).toBe(epoch);
      await same.subscribe(cursor, { event: () => undefined, error: () => undefined }, epoch);
      same.close();

      // A replaced store is a different journal: the old cursor must not resume silently.
      await own.restart({ resetStore: true });
      const replaced = new DaemonSession(await own_connector.open(M5_03_CAPABILITIES));
      expect(replaced.welcome.journal_epoch).toMatch(/\S/);
      expect(replaced.welcome.journal_epoch).not.toBe(epoch);
      await expect(
        replaced.subscribe(cursor, { event: () => undefined, error: () => undefined }, epoch),
      ).rejects.toMatchObject({ kind: "epoch-mismatch", code: -32018 });
      replaced.close();
    } finally {
      await own.stop();
    }
  });

  it("ConnectionManager drops its cursor, reports incomplete history and re-baselines on a replaced store", async () => {
    const own = await startDaemon();
    const own_connector = new NodeConnector(own.dataDir);
    // Restart may take longer than five short retries on CI.
    const manager = new ConnectionManager({ connector: own_connector, backoffMs: [50], maxAutoAttempts: 400, requestTimeoutMs: 5000 });
    try {
      const session = new DaemonSession(await own_connector.open(M5_03_CAPABILITIES));
      const seedApi = new DaemonApi(session);
      await seedApi.projectsRegister(own.repo);
      await seedApi.tasksSubmit("epoch-key-2", { ...submission(), project_id: realpathSync(own.repo) });
      session.close();
      manager.start();
      await until(() => manager.getSnapshot().status === "connected", "connected");
      await until(() => manager.getSnapshot().lastSequence > 0, "a cursor");
      const before = manager.getSnapshot();
      expect(before.history).toEqual({ incomplete: false, reason: null });

      await own.restart({ resetStore: true });
      await until(
        () => manager.getSnapshot().history.reason === "epoch-mismatch" && manager.getSnapshot().status === "connected",
        "epoch mismatch re-baseline",
        15_000,
      );
      const after = manager.getSnapshot();
      expect(after.history).toEqual({ incomplete: true, reason: "epoch-mismatch" });
      expect(after.journalEpoch).toMatch(/\S/);
      expect(after.journalEpoch).not.toBe(before.journalEpoch);
      expect(after.resumedFrom).toBeNull();
    } finally {
      manager.stop();
      await own.stop();
    }
  });
});

describe("real daemon: M5-02b/M5-02c methods", () => {
  it("answers agents.capabilities, tasks.retry (conflict outside Failed) and the coordinator methods", async () => {
    const session = await openSession();
    const api = new DaemonApi(session);
    await api.projectsRegister(daemon.repo);

    const agents = await api.agentsCapabilities();
    expect(agents.map((a) => a.agent)).toEqual(expect.arrayContaining(["codex", "claude-code", "pi"]));
    expect(typeof agents[0]!.installed).toBe("boolean");

    const queued = await api.tasksSubmit("m502b-key-retry", submission({ objective: "retry me" }));
    await expect(api.tasksRetry(queued.contract.id, "m502b-key-retry-1")).rejects.toMatchObject({ kind: "conflict" });
    await expect(api.tasksRetry("task-does-not-exist", "m502b-key-retry-2")).rejects.toMatchObject({
      kind: "not-found",
    });

    const message = await api.coordinatorSendMessage(projectId, "hello coordinator", "m502b-key-msg");
    expect(message).toMatchObject({ kind: "message", author: "user", body: "hello coordinator" });
    const replay = await api.coordinatorSendMessage(projectId, "hello coordinator", "m502b-key-msg");
    expect(replay.id).toBe(message.id);
    const proposal = await api.coordinatorPropose(projectId, "{}", "m502b-key-prop");
    expect(proposal).toMatchObject({ kind: "proposal", validation: { status: "invalid" } });
    const page = await api.coordinatorGet(projectId);
    expect(page.entries.map((e) => e.id)).toEqual([message.id, proposal.id]);
    await expect(api.coordinatorSendMessage("/no/such/project", "x", "m502b-key-msg-2")).rejects.toMatchObject({
      kind: "not-found",
    });
    session.close();
  });
});

describe("real daemon: review flow on a seeded task (workspaces.diff, review.*, approvals.resolve, delivery.request)", () => {
  let own: RunningDaemon;
  let seeded: SeededReview;
  let own_connector: NodeConnector;

  beforeAll(async () => {
    own = await startDaemon();
    seeded = await own.seedReview();
    own_connector = new NodeConnector(own.dataDir);
  });

  afterAll(async () => {
    await own?.stop();
  });

  const targetRef = () => `refs/heads/orchestrator/review/${seeded.task_id}`;

  it("returns the real worktree diff and refuses delivery before an approval exists", async () => {
    const session = new DaemonSession(await own_connector.open(M5_03_CAPABILITIES));
    const api = new DaemonApi(session);
    const diff = await api.workspacesDiff({ task_id: seeded.task_id, workspace_id: seeded.workspace_id });
    expect(diff.workspace_id).toBe(seeded.workspace_id);
    expect(diff.diff).toContain("candidate.txt");
    await expect(api.workspacesDiff({ task_id: "task-does-not-exist" })).rejects.toMatchObject({ kind: "not-found" });
    await expect(
      api.deliveryRequest(
        {
          task_id: seeded.task_id,
          workspace_id: seeded.workspace_id,
          approval_id: "no-such-approval",
          verification_run_id: seeded.verification_run_id,
          action: "local_review",
          target_ref: targetRef(),
        },
        "review-key-early",
      ),
    ).rejects.toBeInstanceOf(DaemonError);
    session.close();
  });

  it("requests approval, resolves it, delivers a local review ref, and replays idempotently", async () => {
    const session = new DaemonSession(await own_connector.open(M5_03_CAPABILITIES));
    const api = new DaemonApi(session);
    const events: DaemonEvent[] = [];
    await session.subscribe(0, { event: (e) => events.push(e), error: () => undefined });

    const requested = await api.reviewRequestApproval(
      {
        task_id: seeded.task_id,
        workspace_id: seeded.workspace_id,
        verification_run_id: seeded.verification_run_id,
        action: "local_review",
        target_ref: targetRef(),
      },
      "review-key-request",
    );
    expect(requested.approval.state).toBe("requested");
    expect(requested.approval.candidate_commit).toBe(seeded.candidate_commit);
    expect(requested.digest).toMatch(/^[0-9a-f]{64}$/);

    const evidence = await api.reviewEvidence(seeded.task_id, requested.approval.id);
    expect(evidence).toMatchObject({
      state: "current",
      review_state: "awaiting_decision",
      decision_ready: true,
      digest_relationship: "matches",
      candidate: { commit: seeded.candidate_commit },
      workspace: { id: seeded.workspace_id, base_commit: seeded.base_commit },
    });
    expect(evidence.verification[0]).toMatchObject({ id: seeded.verification_run_id, status: "passed" });
    // The UI adapter must accept the daemon's real wire shape, not just the fake's.
    expect(toReviewEvidence(evidence, {
      approvalId: requested.approval.id,
      digestHex: requested.digest,
    })).toMatchObject({
      state: "available",
      evidence: {
        provenance: "daemon",
        workspaceId: seeded.workspace_id,
        candidate: { commit: seeded.candidate_commit, baseCommit: seeded.base_commit },
        action: { kind: "LocalReviewMerge", target: targetRef(), digestHex: requested.digest },
        verification: { runId: seeded.verification_run_id, commit: seeded.candidate_commit, outcome: "passed" },
      },
    });

    const pending = await api.approvalsList();
    expect(pending.map((a) => a.id)).toContain(requested.approval.id);
    const resolved = await api.approvalsResolve(requested.approval.id, "approve", "review-key-resolve");
    expect(resolved).toMatchObject({ id: requested.approval.id, state: { state: "approved" } });
    expect((await api.approvalsResolve(requested.approval.id, "approve", "review-key-resolve")).state).toMatchObject({
      state: "approved",
    });

    const delivery = {
      task_id: seeded.task_id,
      workspace_id: seeded.workspace_id,
      approval_id: requested.approval.id,
      verification_run_id: seeded.verification_run_id,
      action: "local_review" as const,
      target_ref: targetRef(),
    };
    const result = await api.deliveryRequest(delivery, "review-key-deliver");
    expect(result).toMatchObject({ task_id: seeded.task_id, outcome: "review_ref_created" });
    expect(execFileSync("git", ["rev-parse", targetRef()], { cwd: own.repo, encoding: "utf8" }).trim()).toBe(
      seeded.candidate_commit,
    );
    expect(await api.deliveryRequest(delivery, "review-key-deliver")).toEqual(result);

    // The durable event is what the UI treats as its own delivery and must not mistake for staleness.
    await until(
      () => events.some((e) => e.task_id === seeded.task_id && e.kind === "delivery.review_ref_created"),
      "delivery event",
    );
    session.close();
  });

  it("rejects request-changes for an unknown approval", async () => {
    const session = new DaemonSession(await own_connector.open(M5_03_CAPABILITIES));
    const api = new DaemonApi(session);
    await expect(
      api.reviewRequestChanges("no-such-approval", seeded.candidate_commit, "x", "review-key-changes"),
    ).rejects.toBeInstanceOf(DaemonError);
    session.close();
  });
});
