/**
 * Pure derivations for the Agents & Sessions view (M7-14). No React, no I/O.
 *
 * Unknown is never zero: a count that cannot be derived is `{ kind: "unknown" }`
 * with the reason, and the UI says so.
 */
import type { Approval, SessionEntry, TaskRecord, TaskState } from "../../../daemon/types";
import type { PreviewTask, RecoveryStatus, SourceResult, TaskRef } from "../../../sources/types";
import { matchesFilter } from "../../format";

export type TabId = "active" | "available" | "queued" | "failed" | "disconnected";

export const TABS: ReadonlyArray<{ id: TabId; label: string }> = [
  { id: "active", label: "Active" },
  { id: "available", label: "Available" },
  { id: "queued", label: "Queued" },
  { id: "failed", label: "Failed" },
  { id: "disconnected", label: "Disconnected" },
];

export interface WorkSession {
  id: string;
  state: string;
  adapter: string | null;
  attemptId: string | null;
}

export interface WorkAttempt {
  id: string;
  state: string;
  number: number | null;
}

/** One task as the view lists it, from either the daemon (`real`) or the D2 preview (`fixture`). */
export interface WorkRow {
  id: string;
  origin: "real" | "fixture";
  objective: string;
  agent: string;
  state: TaskState;
  attempts: WorkAttempt[];
  sessions: WorkSession[];
  blocked: boolean;
  /** D2 preview events; empty for daemon tasks, whose feed comes from `sessions.events`. */
  timeline: Array<{ at: string; kind: string; detail: string }>;
}

export type RecoveryMap = Record<string, SourceResult<RecoveryStatus> | undefined>;

export type Count = { kind: "known"; value: number } | { kind: "unknown"; reason: string };

export interface Headline {
  runningTasks: number;
  runningSessions: number;
  /** Every Created or Queued task; equals the Queued tab length. */
  queued: number;
  /** The queued tasks whose dependencies are met (`queued - blocked`). */
  readyQueued: number;
  blocked: number;
  disconnected: Count;
}

const RUNNING_SESSION = new Set(["Spawning", "Running"]);
const ACTIVE_STATES: ReadonlySet<TaskState> = new Set(["Running", "Verifying"]);
const QUEUED_STATES: ReadonlySet<TaskState> = new Set(["Created", "Queued"]);
const FAILED_STATES: ReadonlySet<TaskState> = new Set(["Failed", "VerificationFailed"]);

export const isRunningSession = (state: string): boolean => RUNNING_SESSION.has(state);
export const isActive = (row: WorkRow): boolean => ACTIVE_STATES.has(row.state);
export const isQueued = (row: WorkRow): boolean => QUEUED_STATES.has(row.state);
export const isFailed = (row: WorkRow): boolean => FAILED_STATES.has(row.state);

export function taskRef(row: WorkRow): TaskRef {
  return row.origin === "real" ? { origin: "real", taskId: row.id, state: row.state } : { origin: "fixture", id: row.id };
}

export function rowFromTask(task: TaskRecord, entries: readonly SessionEntry[] | null, blocked: boolean): WorkRow {
  const sessions = new Map<string, WorkSession>();
  const add = ({ session }: { session: SessionEntry["session"] }) =>
    sessions.set(session.id, { id: session.id, state: session.state, adapter: session.adapter, attemptId: session.attempt_id });
  for (const session of task.sessions) add({ session });
  for (const entry of entries ?? []) if (entry.task_id === task.contract.id) add(entry);
  return {
    id: task.contract.id,
    origin: "real",
    objective: task.contract.objective,
    agent: task.contract.agent,
    state: task.state,
    attempts: task.attempts.map((a) => ({ id: a.id, state: a.state, number: a.attempt_number })),
    sessions: [...sessions.values()],
    blocked,
    timeline: [],
  };
}

export function rowsFromDaemon(
  tasks: readonly TaskRecord[],
  sessions: readonly SessionEntry[] | null,
  approvals: readonly Approval[],
): WorkRow[] {
  return tasks.map((task) => rowFromTask(task, sessions, matchesFilter(task, "blocked", tasks, approvals)));
}

export function rowsFromPreview(previews: readonly PreviewTask[]): WorkRow[] {
  return previews.map((task) => ({
    id: task.id,
    origin: "fixture",
    objective: task.objective,
    agent: task.agent,
    state: task.state,
    attempts: task.attempts.map((a, index) => ({ id: a.id, state: a.state, number: index + 1 })),
    sessions: task.sessions.map((s) => ({ id: s.id, state: s.state, adapter: null, attemptId: null })),
    blocked: false,
    timeline: task.timeline,
  }));
}

/** Active tasks whose D3 classification is `needs-inspection`. Any unresolved lookup makes the count unknown. */
export function disconnected(rows: readonly WorkRow[], recovery: RecoveryMap): Count {
  let found = 0;
  for (const row of rows.filter(isActive)) {
    const result = recovery[row.id];
    if (!result) return { kind: "unknown", reason: "recovery classification is still loading" };
    if (result.state === "unavailable") return { kind: "unknown", reason: result.reason };
    if (result.state === "error") return { kind: "unknown", reason: result.message };
    if (result.state === "empty") return { kind: "unknown", reason: "no recovery classification was returned" };
    if (result.data.classification === "needs-inspection") found += 1;
  }
  return { kind: "known", value: found };
}

export function disconnectedRows(rows: readonly WorkRow[], recovery: RecoveryMap): WorkRow[] {
  return rows.filter((row) => {
    const result = isActive(row) ? recovery[row.id] : undefined;
    return result?.state === "ready" && result.data.classification === "needs-inspection";
  });
}

export function headline(rows: readonly WorkRow[], recovery: RecoveryMap): Headline {
  const queued = rows.filter(isQueued);
  return {
    runningTasks: rows.filter(isActive).length,
    runningSessions: rows.reduce((n, row) => n + row.sessions.filter((s) => isRunningSession(s.state)).length, 0),
    queued: queued.length,
    readyQueued: queued.filter((row) => !row.blocked).length,
    blocked: queued.filter((row) => row.blocked).length,
    disconnected: disconnected(rows, recovery),
  };
}

export function tabRows(tab: Exclude<TabId, "available">, rows: readonly WorkRow[], recovery: RecoveryMap): WorkRow[] {
  switch (tab) {
    case "active":
      return rows.filter(isActive);
    case "queued":
      return rows.filter(isQueued);
    case "failed":
      return rows.filter(isFailed);
    case "disconnected":
      return disconnectedRows(rows, recovery);
  }
}
