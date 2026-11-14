/**
 * Pure derivations for the Agents & Sessions view (M7-14). No React, no I/O.
 *
 * Failed counts are sessions, not tasks. Disconnected has no daemon source (D3), so it is never
 * counted: the view says "not available" instead of a number.
 */
import type { Approval, SessionEntry, TaskRecord, TaskState } from "../../../daemon/types";
import type { PreviewTask, TaskRef } from "../../../sources/types";
import { matchesFilter } from "../../format";

export type TabId = "active" | "available" | "queued" | "failed" | "disconnected";

export const TABS: ReadonlyArray<{ id: TabId; label: string }> = [
  { id: "active", label: "Active" },
  { id: "available", label: "Available / idle" },
  { id: "queued", label: "Queued" },
  { id: "failed", label: "Failed sessions" },
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

/** One task as view lists it, from either daemon (`real`) or D2 preview (`fixture`). */
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

/** A session that ended unsuccessfully, with the attempt it belonged to when the daemon links one. */
export interface FailedSession {
  row: WorkRow;
  session: WorkSession;
  attempt: WorkAttempt | null;
}

export interface Headline {
  runningTasks: number;
  runningSessions: number;
  /** Every Created or Queued task; equals the Queued tab length. */
  queued: number;
  /** The queued tasks whose dependencies are met (`queued - blocked`). */
  readyQueued: number;
  blocked: number;
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

/** The attempt with the highest number, the one the row's attempt label names. */
export function latestAttempt(row: WorkRow): WorkAttempt | null {
  let best: WorkAttempt | null = null;
  for (const attempt of row.attempts) {
    if (best === null || (attempt.number ?? 0) >= (best.number ?? 0)) best = attempt;
  }
  return best;
}

/** Sessions that ended unsuccessfully: aborted, or belonging to an attempt that failed. */
export function failedSessions(rows: readonly WorkRow[]): FailedSession[] {
  const found: FailedSession[] = [];
  for (const row of rows) {
    for (const session of row.sessions) {
      const attempt = row.attempts.find((a) => a.id === session.attemptId) ?? null;
      if (session.state === "Aborted" || attempt?.state === "Failed") found.push({ row, session, attempt });
    }
  }
  return found;
}

/** Failed tasks with no failed session on record. They are still failures, so the Failed tab lists them apart. */
export function failedWithoutSession(rows: readonly WorkRow[]): WorkRow[] {
  const counted = new Set(failedSessions(rows).map((failed) => failed.row.id));
  return rows.filter((row) => isFailed(row) && !counted.has(row.id));
}

export function headline(rows: readonly WorkRow[]): Headline {
  const queued = rows.filter(isQueued);
  return {
    runningTasks: rows.filter(isActive).length,
    runningSessions: rows.reduce((n, row) => n + row.sessions.filter((s) => isRunningSession(s.state)).length, 0),
    queued: queued.length,
    readyQueued: queued.filter((row) => !row.blocked).length,
    blocked: queued.filter((row) => row.blocked).length,
  };
}

/** Task lists for the Active and Queued tabs. The other tabs derive their own rows. */
export function tabRows(tab: "active" | "queued", rows: readonly WorkRow[]): WorkRow[] {
  return tab === "active" ? rows.filter(isActive) : rows.filter(isQueued);
}
