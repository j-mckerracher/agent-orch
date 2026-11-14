// Test doubles for the ReviewClient and ReviewEvidenceSource interfaces.
import { DaemonError, type DaemonErrorKind } from "../daemon/errors";
import type { Approval, TaskRecord } from "../daemon/types";
import { digestHex } from "./binding";
import type { ReviewClient, ReviewConnection, ReviewNotice } from "./client";
import type { ReviewEvidence, ReviewEvidenceResult, ReviewEvidenceSource } from "./sources";

export const COMMIT = "a".repeat(40);
export const BASE = "b".repeat(40);

export function approval(overrides: Partial<Approval> & { taskId?: string; commit?: string; digest?: number[] } = {}): Approval {
  return {
    id: overrides.id ?? "approval-1",
    action: {
      kind: { type: "delivery", kind: "LocalReviewMerge" },
      parameter_digest: overrides.digest ?? Array.from({ length: 32 }, (_, i) => i),
      candidate_commit: overrides.commit ?? COMMIT,
      task_id: overrides.taskId ?? "task-1",
    },
    state: overrides.state ?? { state: "requested" },
  };
}

export function taskRecord(id = "task-1", project = "project-1", delivery: "LocalReview" | "ReportOnly" = "LocalReview"): TaskRecord {
  return {
    contract: {
      id,
      project_id: project,
      parent_id: null,
      objective: `Objective of ${id}`,
      kind: "Implement",
      agent: "codex",
      dependencies: [],
      verification: "None",
      delivery,
      autonomy: "ApprovalRequired",
      max_retries: 0,
      max_repairs: 0,
    },
    state: "Succeeded",
    attempts: [],
    sessions: [],
    events: [],
    version: 2,
  };
}

export const DIFF = `diff --git a/src/a.rs b/src/a.rs
index 1..2 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -1 +1,2 @@
 keep
+added line
diff --git a/old.txt b/old.txt
deleted file mode 100644
index 3..0
--- a/old.txt
+++ /dev/null
@@ -1 +0,0 @@
-gone
`;

export function evidenceFor(a: Approval, overrides: Partial<ReviewEvidence> = {}): ReviewEvidence {
  return {
    provenance: "daemon",
    taskId: a.action.task_id,
    approvalId: a.id,
    workspaceId: "ws-1",
    candidate: { commit: a.action.candidate_commit, baseCommit: BASE },
    action: {
      kind: a.action.kind.type === "delivery" ? a.action.kind.kind : "",
      label: "Local review merge",
      target: "refs/heads/review/task-1",
      parameters: { target_ref: "refs/heads/review/task-1" },
      digestHex: digestHex(a.action.parameter_digest),
    },
    verification: {
      runId: "run-1",
      commit: a.action.candidate_commit,
      outcome: "passed",
      summary: "ok",
      checks: [{ name: "tests", outcome: "passed" }],
    },
    ...overrides,
  };
}

export function evidenceSource(
  provenance: "daemon" | "fixture",
  build: (a: Approval) => ReviewEvidenceResult | ((req: { approvalId: string }) => never),
  approvals: Approval[],
): ReviewEvidenceSource {
  return {
    provenance,
    async load(request) {
      const a = approvals.find((x) => x.id === request.approvalId) ?? approvals[0];
      return build(a) as ReviewEvidenceResult;
    },
  };
}

export type Handler = (params: Record<string, unknown>) => unknown;

export interface TestClient extends ReviewClient {
  calls: { method: string; params: Record<string, unknown> }[];
  callsOf(method: string): Record<string, unknown>[];
  setHandler(method: string, handler: Handler): void;
  emit(notice: ReviewNotice): void;
  setStatus(status: ReviewConnection["status"]): void;
}

export function makeClient(handlers: Record<string, Handler>, provenance: "daemon" | "fixture" = "daemon"): TestClient {
  const listeners = new Set<(n: ReviewNotice) => void>();
  let state: ReviewConnection = { status: "connected", historyIncomplete: false };
  const table = { ...handlers };
  const client: TestClient = {
    provenance,
    calls: [],
    callsOf: (method) => client.calls.filter((c) => c.method === method).map((c) => c.params),
    setHandler: (method, handler) => {
      table[method] = handler;
    },
    async call<T>(method: string, params: Record<string, unknown> = {}) {
      client.calls.push({ method, params });
      const handler = table[method];
      if (!handler) throw new DaemonError("capability-not-negotiated", `no handler for ${method}`, -32601);
      return (await handler(params)) as T;
    },
    connection: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    emit: (notice) => listeners.forEach((l) => l(notice)),
    setStatus(status) {
      state = { ...state, status };
      client.emit({ type: "connection", state });
    },
  };
  return client;
}

export function fail(kind: DaemonErrorKind, message = kind): never {
  throw new DaemonError(kind, message);
}
