// In-memory fixture daemon for explicit demo mode (never selected automatically).
// It serves PLACEHOLDER DATA behind the same ReviewClient interface and mimics
// the M5-02 behaviors the screen depends on, including the daemon's refusal to
// scope a diff to a path that no longer exists in the worktree.
import demo from "../fixtures/ui/review-demo-daemon.json";
import { DaemonError } from "../daemon/errors";
import type { Approval, TaskRecord } from "../daemon/types";
import type { ReviewClient, ReviewConnection, ReviewNotice } from "./client";
import { isSafeRelativePath, parseUnifiedDiff } from "./diff";

export function createFixtureDaemonClient(): ReviewClient {
  const approvals = structuredClone(demo.approvals) as Approval[];
  const tasks = demo.tasks as unknown as TaskRecord[];
  const receipts = new Map<string, unknown>();
  const connection: ReviewConnection = { status: "connected", historyIncomplete: false };

  function call(method: string, params: Record<string, unknown> = {}): unknown {
    switch (method) {
      case "tasks.list":
        return tasks;
      case "tasks.get": {
        const task = tasks.find((t) => t.contract.id === params.task_id);
        if (!task) throw new DaemonError("not-found", "task not found", -32010);
        return task;
      }
      case "approvals.list":
        return approvals.filter((a) => a.state.state === "requested");
      case "approvals.resolve": {
        const key = String(params.idempotency_key);
        if (receipts.has(key)) return receipts.get(key);
        const approval = approvals.find((a) => a.id === params.approval_id);
        if (!approval) throw new DaemonError("not-found", "approval not found", -32010);
        if (approval.state.state !== "requested") throw new DaemonError("conflict", "not pending", -32011);
        approval.state =
          params.decision === "approve"
            ? { state: "approved", by: { kind: "user", id: "fixture-user" } }
            : { state: "rejected" };
        const receipt = structuredClone(approval);
        receipts.set(key, receipt);
        return receipt;
      }
      case "workspaces.diff": {
        if (!tasks.some((t) => t.contract.id === params.task_id)) {
          throw new DaemonError("not-found", "task not found", -32010);
        }
        const path = params.path as string | undefined;
        if (path === undefined) {
          return { task_id: params.task_id, workspace_id: demo.workspaceId, diff: demo.diff };
        }
        const file = isSafeRelativePath(path)
          ? parseUnifiedDiff(demo.diff).find((f) => f.path === path && f.status !== "deleted")
          : undefined;
        if (!file) throw new DaemonError("invalid-params", "path is not valid", -32602);
        return { task_id: params.task_id, workspace_id: demo.workspaceId, diff: file.text };
      }
      default:
        throw new DaemonError("forbidden", "fixture daemon does not support this method", -32012);
    }
  }

  return {
    provenance: "fixture",
    async call<T>(method: string, params?: Record<string, unknown>) {
      return call(method, params) as T;
    },
    connection: () => connection,
    subscribe: (_listener: (notice: ReviewNotice) => void) => () => undefined,
  };
}
