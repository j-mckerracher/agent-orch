import type { Approval, Session, SessionEntry, TaskRecord, TaskState } from "../daemon/types";
import { NO_HELD, type HeldSets } from "../sources/surfaces/holds";
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

/** Tasks still active or waiting (Created, Queued, Running, Verifying): the top-bar task count. */
export function openTaskCount(tasks: readonly TaskRecord[]): number {
  return tasks.filter((task) => !TERMINAL_STATES.has(task.state)).length;
}

/** States the daemon accepts `tasks.cancel` from; any other state is refused. */
export const CANCELLABLE_STATES: ReadonlySet<TaskState> = new Set(["Created", "Queued", "Running", "Verifying"]);

export function canCancel(state: TaskState): boolean {
  return CANCELLABLE_STATES.has(state);
}

/** `tasks.archive` takes only finished tasks; any other state is refused with Conflict. */
export function canArchive(state: TaskState): boolean {
  return TERMINAL_STATES.has(state);
}

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

/**
 * Pending approvals plus pending proposed actions: one number nav, top bar activity badge show. A held decision
 * (the user postponed it) is not counted, so it matches the Actionable tab; with no `held` every decision counts.
 */
export function pendingDecisionCount(
  approvals: readonly Approval[],
  proposals: readonly { id?: string; state: string }[],
  held: HeldSets = NO_HELD,
): number {
  const approvalCount = pendingApprovals(approvals).filter((approval) => !held.tasks.has(approval.action.task_id)).length;
  const proposalCount = pendingProposals(proposals).filter((proposal) => proposal.id === undefined || !held.proposals.has(proposal.id)).length;
  return approvalCount + proposalCount;
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
