import { afterEach, describe, expect, it, vi } from "vitest";
import { DaemonError } from "../daemon/errors";
import { ConnectionManager } from "../daemon/connection";
import type { ReviewEvidenceWire } from "../daemon/types";
import { FakeDaemon, until } from "../test/fakeDaemon";
import { createDaemonChangeRequestSource, createDaemonReviewEvidenceSource, toReviewEvidence } from "./daemonSources";

const COMMIT = "c".repeat(40);
const BASE = "b".repeat(40);
const request = { taskId: "task-1", approvalId: "approval-1", candidateCommit: COMMIT, digestHex: "ab".repeat(32) };

const wire = (overrides: Partial<ReviewEvidenceWire> = {}): ReviewEvidenceWire => ({
  task_id: "task-1",
  project_id: "/repo",
  state: "current",
  reasons: [],
  review_state: "awaiting_decision",
  decision_ready: true,
  approval: { id: "approval-1", state: "requested", kind: "delivery", task_id: "task-1", candidate_commit: COMMIT },
  action: {
    action: "local_review",
    destination: { project_id: "/repo" },
    base_sha: BASE,
    target_ref: "refs/heads/review/task-1",
    candidate_commit: COMMIT,
  },
  digest_relationship: "matches",
  candidate: { commit: COMMIT, source: "verification", workspace_head: COMMIT },
  workspace: { id: "ws-1", branch: "task/task-1", base_commit: BASE },
  verification: [
    {
      id: "run-1",
      status: "passed",
      commit: COMMIT,
      started_at_ms: 1,
      finished_at_ms: 2,
      matches_candidate: true,
      details: { checks: [{ name: "cargo test", status: "passed" }, { nope: true }] },
      details_truncated: false,
    },
  ],
  artifacts: [],
  snapshot: null,
  change_request: null,
  ...overrides,
});

const disposables: Array<() => void> = [];
afterEach(() => {
  while (disposables.length) disposables.pop()!();
});

async function connected(fake: FakeDaemon) {
  const connection = new ConnectionManager({ connector: fake, backoffMs: [5], requestTimeoutMs: 100 });
  disposables.push(() => connection.stop());
  connection.start();
  await until(() => connection.getSnapshot().status === "connected", "connected");
  return connection;
}

describe("toReviewEvidence (D7)", () => {
  it("maps a current review into bound daemon evidence", () => {
    const result = toReviewEvidence(wire(), request);
    expect(result).toMatchObject({
      state: "available",
      evidence: {
        provenance: "daemon",
        taskId: "task-1",
        approvalId: "approval-1",
        workspaceId: "ws-1",
        candidate: { commit: COMMIT, baseCommit: BASE },
        action: { kind: "LocalReviewMerge", target: "refs/heads/review/task-1", digestHex: request.digestHex },
        verification: { runId: "run-1", commit: COMMIT, outcome: "passed", checks: [{ name: "cargo test", outcome: "passed" }] },
      },
    });
  });

  it("withholds the digest unless the daemon says it matches", () => {
    const result = toReviewEvidence(wire({ digest_relationship: "mismatch" }), request);
    expect(result).toMatchObject({ state: "available", evidence: { action: { digestHex: "" } } });
  });

  it("is unavailable for stale or changes-requested evidence and missing when absent", () => {
    expect(toReviewEvidence(wire({ state: "stale", reasons: ["candidate_changed"] }), request)).toMatchObject({
      state: "unavailable",
      reason: expect.stringContaining("candidate_changed"),
    });
    expect(toReviewEvidence(wire({ review_state: "changes_requested" }), request)).toMatchObject({
      state: "unavailable",
    });
    expect(toReviewEvidence(wire({ state: "missing", approval: null }), request)).toMatchObject({ state: "missing" });
    expect(toReviewEvidence(wire({ verification: [] }), request)).toMatchObject({ state: "missing" });
  });

  it("reports a failed verification run as such so binding refuses it", () => {
    const failed = wire({
      verification: [{ ...wire().verification[0]!, status: "failed", details: "unexpected shape" }],
    });
    const result = toReviewEvidence(failed, request);
    expect(result).toMatchObject({ state: "available", evidence: { verification: { outcome: "failed", checks: [] } } });
  });
});

describe("daemon D7 and D8 sources", () => {
  it("D7 calls review.evidence with the task and approval", async () => {
    const fake = new FakeDaemon();
    fake.stub("review.evidence", wire());
    const connection = await connected(fake);
    const source = createDaemonReviewEvidenceSource(() => connection.api());
    expect(source.provenance).toBe("daemon");
    expect(await source.load(request)).toMatchObject({ state: "available" });
    expect(fake.callsTo("review.evidence")[0]?.params).toEqual({ task_id: "task-1", approval_id: "approval-1" });
  });

  it("D7 reports unavailable when disconnected and an error for a daemon failure", async () => {
    const fake = new FakeDaemon();
    const connection = await connected(fake);
    const source = createDaemonReviewEvidenceSource(() => connection.api());
    // The fake has no review.evidence method, which a daemon without the capability reports as unavailable.
    expect(await source.load(request)).toMatchObject({ state: "unavailable", provenance: "daemon" });
    connection.stop();
    expect(await source.load(request)).toMatchObject({ state: "unavailable", provenance: "daemon" });
  });

  it("D8 reads history from review.evidence and submits through review.request_changes", async () => {
    const fake = new FakeDaemon();
    const record = {
      id: "cr-1",
      task_id: "task-1",
      approval_id: "approval-1",
      candidate_commit: COMMIT,
      feedback: "add tests",
      requested_by: "user",
      created_at: "2026-01-01T00:00:00Z",
    };
    fake.stub("review.evidence", wire({ change_request: null }));
    fake.stub("review.request_changes", { change_request: record, review_state: "changes_requested", approval_state: "requested" });
    const connection = await connected(fake);
    const source = createDaemonChangeRequestSource(() => connection.api());
    expect(source.labels.receiptNote).toBe("");
    expect(JSON.stringify(source.labels)).not.toMatch(/simulated|preview|no daemon/i);
    expect(await source.loadHistory("task-1")).toMatchObject({ state: "empty" });
    const receipt = await source.submit({ taskId: "task-1", approvalId: "approval-1", candidateCommit: COMMIT, feedback: "add tests" });
    expect(receipt).toMatchObject({ provenance: "daemon", requestId: "cr-1", resultingState: "changes requested" });
    expect(fake.callsTo("review.request_changes")[0]?.params).toMatchObject({
      approval_id: "approval-1",
      candidate_commit: COMMIT,
      feedback: "add tests",
    });
    fake.stub("review.evidence", wire({ change_request: record }));
    expect(await source.loadHistory("task-1")).toMatchObject({ state: "available", items: [{ requestId: "cr-1" }] });
  });

  it.each(["fixture:commit", "fixture0001", "  FIXTURE0001  "])("D8 refuses bound candidate %j before API access or transport", async (candidateCommit) => {
    const fake = new FakeDaemon();
    const connection = await connected(fake);
    const api = vi.fn(() => connection.api());
    const source = createDaemonChangeRequestSource(api);
    const before = fake.calls.length;
    const failure = source.submit({ taskId: "task-1", approvalId: "approval-1", candidateCommit, feedback: "fix" });
    await expect(failure).rejects.toBeInstanceOf(DaemonError);
    await expect(failure).rejects.toMatchObject({ kind: "invalid-params" });
    expect(api).not.toHaveBeenCalled();
    expect(fake.calls.length).toBe(before);
  });

  it("D8 rejects with the daemon's error", async () => {
    const connection = await connected(new FakeDaemon());
    const source = createDaemonChangeRequestSource(() => connection.api());
    await expect(
      source.submit({ taskId: "task-1", approvalId: "approval-1", candidateCommit: COMMIT, feedback: "x" }),
    ).rejects.toThrow(/method-unavailable/);
  });
});
