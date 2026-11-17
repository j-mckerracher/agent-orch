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

/** Display text for a daemon state. The wire value stays `Cancelled`; only the label is US-spelled. */
const STATE_LABELS: Readonly<Record<string, string>> = { Cancelled: "Canceled", cancelled: "Canceled" };

export function stateLabel(state: string): string {
  return STATE_LABELS[state] ?? state;
}

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

/** Proposed actions (DPES-1105) still waiting for a decision. A decided one never counts, whatever list it is in. */
export function pendingProposals<T extends { state: string }>(proposals: readonly T[]): T[] {
  return proposals.filter((proposal) => proposal.state === "pending");
}

/** Pending approvals plus pending proposed actions: the one number the nav, the top bar and the activity badge show. */
export function pendingDecisionCount(approvals: readonly Approval[], proposals: readonly { state: string }[]): number {
  return pendingApprovals(approvals).length + pendingProposals(proposals).length;
}

/**
 * A pending count as text. `more` means the daemon held more pending proposed actions than it sent (it caps the list),
 * so the count is a lower bound and reads "100+"; every place that shows the count uses this, so they cannot disagree.
 */
export function decisionCountText(count: number, more: boolean): string {
  return more ? `${count}+` : String(count);
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
