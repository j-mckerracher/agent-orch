/**
 * Wire types for `queue.snapshot` (capability `queue`, M7-05).
 *
 * Mirrors `crates/orchestrator-daemon/src/api_queue.rs` and `docs/queue-snapshot.md`.
 * The method takes no parameters. `Verifying` and `Created` tasks appear in neither list.
 */
import { DaemonError } from "./errors";

export interface QueueLimits {
  max_concurrent_workers: number;
  max_workers_per_project: number;
}

export interface QueueRunningEntry {
  task_id: string;
  project_id: string;
  agent: string;
}

export interface QueueQueuedEntry {
  task_id: string;
  project_id: string;
  /** 1-based up-next order; null when the task cannot start for a non-capacity reason. */
  position: number | null;
  /** `dependency | global_cap | project_cap | approval | decision | load | other`; kept a string so new values degrade. */
  waiting_reason: string;
  waiting_on: string | null;
  /** For `other`: `worktree_owned | dispatch_pending | dispatch_blocked | not_queued`. */
  waiting_detail: string | null;
  /** Sent with `waiting_reason: "load"` only (DPES-1105): the meter that holds new starts back. */
  load?: QueueLoadWait;
}

/** The machine load gate's hold. Each figure is a whole percent; the meter is `cpu`, `memory` or `disk`. */
export interface QueueLoadWait {
  meter: string;
  mean_percent: number;
  limit_percent: number;
  reopen_percent: number;
}

export interface QueueSnapshotWire {
  as_of_sequence: number;
  epoch: string;
  limits: QueueLimits;
  /** The restart gate blocks every launch. This is not a user pause. */
  dispatch_blocked: boolean;
  running: QueueRunningEntry[];
  queued: QueueQueuedEntry[];
}

const malformed = (what: string) => new DaemonError("protocol", `malformed queue.snapshot: ${what}`);

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);
const text = (value: unknown, what: string): string => {
  if (typeof value !== "string" || value === "") throw malformed(what);
  return value;
};
const count = (value: unknown, what: string): number => {
  if (typeof value !== "number" || !Number.isInteger(value) || value < 0) throw malformed(what);
  return value;
};
const optionalText = (value: unknown, what: string): string | null => {
  if (value === null || value === undefined) return null;
  if (typeof value !== "string") throw malformed(what);
  return value;
};
const loadWait = (value: unknown): QueueLoadWait | undefined => {
  if (value === null || value === undefined) return undefined;
  if (!isRecord(value)) throw malformed("queued.load");
  return {
    meter: text(value.meter, "queued.load.meter"),
    mean_percent: count(value.mean_percent, "queued.load.mean_percent"),
    limit_percent: count(value.limit_percent, "queued.load.limit_percent"),
    reopen_percent: count(value.reopen_percent, "queued.load.reopen_percent"),
  };
};
const list = (value: unknown, what: string): Record<string, unknown>[] => {
  if (!Array.isArray(value) || !value.every(isRecord)) throw malformed(what);
  return value;
};

/** Validates a `queue.snapshot` result; a missing or mistyped field is a protocol error, never a default. */
export function parseQueueSnapshot(value: unknown): QueueSnapshotWire {
  if (!isRecord(value) || !isRecord(value.limits)) throw malformed("result");
  if (typeof value.dispatch_blocked !== "boolean") throw malformed("dispatch_blocked");
  return {
    as_of_sequence: count(value.as_of_sequence, "as_of_sequence"),
    epoch: text(value.epoch, "epoch"),
    limits: {
      max_concurrent_workers: count(value.limits.max_concurrent_workers, "limits.max_concurrent_workers"),
      max_workers_per_project: count(value.limits.max_workers_per_project, "limits.max_workers_per_project"),
    },
    dispatch_blocked: value.dispatch_blocked,
    running: list(value.running, "running").map((entry) => ({
      task_id: text(entry.task_id, "running.task_id"),
      project_id: text(entry.project_id, "running.project_id"),
      agent: text(entry.agent, "running.agent"),
    })),
    queued: list(value.queued, "queued").map((entry) => {
      const load = loadWait(entry.load);
      return {
        task_id: text(entry.task_id, "queued.task_id"),
        project_id: text(entry.project_id, "queued.project_id"),
        position: entry.position === null || entry.position === undefined ? null : count(entry.position, "queued.position"),
        waiting_reason: text(entry.waiting_reason, "queued.waiting_reason"),
        waiting_on: optionalText(entry.waiting_on, "queued.waiting_on"),
        waiting_detail: optionalText(entry.waiting_detail, "queued.waiting_detail"),
        ...(load ? { load } : {}),
      };
    }),
  };
}
