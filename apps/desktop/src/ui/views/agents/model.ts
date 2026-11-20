/** Pure derivations for the Agents view (M7-14). No React, no I/O. */
import type { Approval, SessionEntry, TaskRecord, TaskState, ToolsNoticeWire } from "../../../daemon/types";
import type { PreviewTask, TaskRef } from "../../../sources/types";
import type { PermissionLevel } from "../../../sources/surfaces/types";
import { basename, matchesFilter } from "../../format";
import { taskPermissionLevel } from "../workspace/permissionLevel";

/** Tabs whose rows are tasks, picked by task state alone. */
export type TaskTab = "running" | "waiting" | "queued" | "failed" | "finished";

export const TABS: ReadonlyArray<{ id: TaskTab; label: string }> = [
  { id: "running", label: "Running" },
  { id: "waiting", label: "Waiting" },
  { id: "queued", label: "Queued" },
  { id: "failed", label: "Failed" },
  { id: "finished", label: "Finished" },
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
  /** Repository name; null for a preview task, which has none. */
  project: string | null;
  state: TaskState;
  attempts: WorkAttempt[];
  /** Null until the daemon's session list has loaded: session counts and rows are then unknown, never zero. */
  sessions: WorkSession[] | null;
  blocked: boolean;
  /** The level the task ran with; null for a preview task, which has none. */
  permission: PermissionLevel | null;
  /** Tools the worker lacked; null when there were none, and always null for a preview task. */
  toolsNotice: ToolsNoticeWire | null;
  /** D2 preview events; empty for daemon tasks, whose feed comes from `sessions.events`. */
  timeline: Array<{ at: string; kind: string; detail: string }>;
}

const RUNNING_SESSION = new Set(["Spawning", "Running"]);
/** Every task state names its tab; a new TaskState does not compile until it is placed. */
const STATE_TAB: Readonly<Record<TaskState, TaskTab>> = {
  Created: "queued",
  Queued: "queued",
  Running: "running",
  Verifying: "running",
  Waiting: "waiting",
  Failed: "failed",
  VerificationFailed: "failed",
  Succeeded: "finished",
  Cancelled: "finished",
};

export const isRunningSession = (state: string): boolean => RUNNING_SESSION.has(state);

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
    project: basename(task.contract.project_id),
    state: task.state,
    attempts: task.attempts.map((a) => ({ id: a.id, state: a.state, number: a.attempt_number })),
    sessions: entries === null ? null : [...sessions.values()],
    blocked,
    permission: taskPermissionLevel(task),
    toolsNotice: task.tools_notice ?? null,
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
    project: null,
    state: task.state,
    attempts: task.attempts.map((a, index) => ({ id: a.id, state: a.state, number: index + 1 })),
    sessions: task.sessions.map((s) => ({ id: s.id, state: s.state, adapter: null, attemptId: null })),
    blocked: false,
    permission: null,
    toolsNotice: null,
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

/** The task list under one tab, in daemon order. */
export function tabRows(tab: TaskTab, rows: readonly WorkRow[]): WorkRow[] {
  return rows.filter((row) => STATE_TAB[row.state] === tab);
}

/** `1 running · 2 queued`, plus waiting tasks when there are any (they need the operator). */
export function summaryLine(rows: readonly WorkRow[]): string {
  const count = (tab: TaskTab) => tabRows(tab, rows).length;
  const waiting = count("waiting");
  return [`${count("running")} running`, `${count("queued")} queued`, ...(waiting > 0 ? [`${waiting} waiting`] : [])].join(" · ");
}
