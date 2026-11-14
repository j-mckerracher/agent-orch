import type { Approval, Session, SessionEntry, TaskRecord, TaskState } from "../daemon/types";
import type { TaskFilter } from "./router";

/** Display name only; the full project id (canonical path) is always shown too. */
export function basename(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export const TASK_STATES: readonly TaskState[] = [
  "Created",
  "Queued",
  "Running",
  "Verifying",
  "Succeeded",
  "VerificationFailed",
  "Failed",
  "Cancelled",
];

export const TERMINAL_STATES: ReadonlySet<TaskState> = new Set([
  "Succeeded",
  "VerificationFailed",
  "Failed",
  "Cancelled",
]);

export const FILTER_LABELS: Array<{ filter: TaskFilter; label: string }> = [
  { filter: "all", label: "All" },
  { filter: "queued", label: "Queued" },
  { filter: "running", label: "Running" },
  { filter: "blocked", label: "Blocked" },
  { filter: "review", label: "Awaiting review" },
  { filter: "terminal", label: "Terminal" },
];

export function pendingApprovals(approvals: readonly Approval[]): Approval[] {
  return approvals.filter((approval) => approval.state.state === "requested");
}

/**
 * Local-only filter semantics (no server filter parameter exists).
 * - blocked: not yet running, and a dependency in this list has not succeeded
 *   (an unknown dependency counts as unmet);
 * - review: a returned approval for the task is still requested.
 */
export function matchesFilter(
  task: TaskRecord,
  filter: TaskFilter,
  all: readonly TaskRecord[],
  approvals: readonly Approval[],
): boolean {
  const state = task.state;
  switch (filter) {
    case "all":
      return true;
    case "queued":
      return state === "Created" || state === "Queued";
    case "running":
      return state === "Running" || state === "Verifying";
    case "terminal":
      return TERMINAL_STATES.has(state);
    case "blocked":
      return (
        (state === "Created" || state === "Queued") &&
        task.contract.dependencies.some(
          (dep) => all.find((other) => other.contract.id === dep)?.state !== "Succeeded",
        )
      );
    case "review":
      return pendingApprovals(approvals).some((a) => a.action.task_id === task.contract.id);
    default:
      return filter.startsWith("state:") && state === filter.slice("state:".length);
  }
}

export function latestSession(task: TaskRecord, entries: readonly SessionEntry[] | null): Session | null {
  const fromList = entries?.filter((entry) => entry.task_id === task.contract.id).map((e) => e.session);
  const sessions = fromList && fromList.length > 0 ? fromList : task.sessions;
  return sessions[sessions.length - 1] ?? null;
}

export function formatTime(ms: number | null): string {
  return ms === null ? "never" : new Date(ms).toISOString();
}
