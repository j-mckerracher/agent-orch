/**
 * Live D11 adapter (M7-20): workspace activity from `queue.snapshot` (capability `queue`),
 * joined with `tasks.list` and `approvals.list`.
 *
 * Rules:
 * - Never imports fixtures. A failed `queue.snapshot` is `unavailable`, or `unsupported` when the
 *   daemon or this connection lacks the method or capability (an older daemon without `queue`);
 *   it is never answered with fixture data. A failed `tasks.list` or `approvals.list` degrades
 *   only its own part, named in `degraded`, and is never shown as empty.
 * - `queue.snapshot` decides who runs, who waits, the order and the limits. `tasks.list` only adds
 *   text (objective, queued agent) and the `Verifying` tasks, which hold no slot and so appear in
 *   neither snapshot list. The two reads are not atomic: the pair is re-read when they disagree on
 *   who is `Running` or `Queued`, and the snapshot stays authoritative if they still disagree.
 * - `slots.used` counts snapshot `running` only, so `Verifying` rows do not use a slot.
 *   `slots.limit` is the global limit; the per-project limit is named in the wait detail.
 * - Unknown is never zero: there is no queue position without a snapshot position, and
 *   `dispatchPaused` is always `unsupported` because the daemon has no pause. The snapshot's
 *   `dispatch_blocked` is the restart gate, not a pause, and is reported as text only.
 * - `simulate*` calls reject: they exist for fixture adapters only. This adapter has no mutation.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { DaemonError, asDaemonError, isMissingFeature, presentError } from "../../daemon/errors";
import type { QueueQueuedEntry, QueueSnapshotWire } from "../../daemon/queue";
import type { Approval, TaskRecord } from "../../daemon/types";
import { measured, notOk, observation, okResult, unsupportedValue } from "./result";
import type {
  ActivitySnapshot,
  ActivitySource,
  AttentionItem,
  Measured,
  Observation,
  Provenance,
  QueueEntry,
  SurfaceResult,
  WaitingReason,
} from "./types";

const MAX_READ_ATTEMPTS = 3;
const OBJECTIVE_UNAVAILABLE = "Objective not available";
const AGENT_UNAVAILABLE = "agent not reported";

const PROVENANCE: Provenance = { kind: "daemon", id: "D11", methods: ["queue.snapshot", "tasks.list", "approvals.list"] };

const fixtureOnly = (what: string) => new DaemonError("method-unavailable", `Simulated ${what} exists only for fixture data.`);

function approvalKind(approval: Approval): string {
  const kind = approval.action.kind;
  return kind.type === "delivery" ? kind.kind : kind.type;
}

function reasonOf(failed: DaemonError): string {
  return `${presentError(failed).reason} (${failed.kind}: ${failed.message})`;
}

function failure<T>(error: unknown): SurfaceResult<T> {
  const failed = asDaemonError(error);
  const reason = reasonOf(failed);
  return notOk(PROVENANCE, isMissingFeature(failed) ? "unsupported" : "unavailable", reason);
}

/** A read that failed after the snapshot succeeded, kept as its reason so only that part degrades. */
type Part<T> = { ok: true; value: T } | { ok: false; reason: string };

interface Reading {
  snapshot: QueueSnapshotWire;
  tasks: Part<TaskRecord[]>;
  approvals: Part<Approval[]>;
}

function settle<T>(result: PromiseSettledResult<T>): Part<T> {
  return result.status === "fulfilled"
    ? { ok: true, value: result.value }
    : { ok: false, reason: reasonOf(asDaemonError(result.reason)) };
}

function agree(snapshot: QueueSnapshotWire, tasks: readonly TaskRecord[]): boolean {
  const running = new Set(snapshot.running.map((entry) => entry.task_id));
  const queued = new Set(snapshot.queued.map((entry) => entry.task_id));
  return tasks.every((task) => {
    const id = task.contract.id;
    return running.has(id) === (task.state === "Running") && queued.has(id) === (task.state === "Queued");
  });
}

async function read(connection: ConnectionManager): Promise<Reading> {
  const api = connection.api();
  let reading: Reading | null = null;
  for (let attempt = 0; attempt < MAX_READ_ATTEMPTS; attempt++) {
    const snapshot = await api.queueSnapshot();
    const [tasks, approvals] = await Promise.allSettled([api.tasksList(), api.approvalsList()]);
    reading = { snapshot, tasks: settle(tasks), approvals: settle(approvals) };
    // Without a task list there is nothing to compare, and a re-read would not repair it.
    if (!reading.tasks.ok || agree(snapshot, reading.tasks.value)) break;
  }
  return reading!;
}

function waitingOf(entry: QueueQueuedEntry, snapshot: QueueSnapshotWire): NonNullable<QueueEntry["waiting"]> {
  const on = entry.waiting_on;
  const wait = (reason: WaitingReason, detail: string) => ({ reason, detail });
  switch (entry.waiting_reason) {
    case "dependency":
      return wait("dependency", on ? `Waiting for ${on}.` : "Waiting for a dependency.");
    case "global_cap":
      return wait("capacity", `All ${snapshot.limits.max_concurrent_workers} worker slots are in use.`);
    case "project_cap":
      return wait("capacity", `This project already runs ${snapshot.limits.max_workers_per_project} workers, its per-project limit.`);
    case "approval":
      return wait("approval", entry.waiting_detail === "project_not_registered" ? "The project is not registered." : "Start authorization is withheld.");
    case "other":
      switch (entry.waiting_detail) {
        case "worktree_owned":
          return wait("unknown", on ? `Waiting for ${on} to release the shared worktree.` : "Waiting for a shared worktree to be released.");
        case "dispatch_pending":
          return wait("unknown", "A slot is free; the next scheduling pass starts this task.");
        case "dispatch_blocked":
          return wait("unknown", "The daemon's restart gate is blocking all launches.");
        case "not_queued":
          return wait("unknown", "The daemon no longer lists this task as queued.");
        default:
          return wait("unknown", "The daemon reports no further reason.");
      }
    default:
      return wait("unknown", `The daemon reported a wait reason this app does not recognise: ${entry.waiting_reason}.`);
  }
}

function snapshotOf(reading: Reading, at: Observation): ActivitySnapshot {
  const { snapshot } = reading;
  const tasks = reading.tasks.ok ? reading.tasks.value : [];
  const approvals = reading.approvals.ok ? reading.approvals.value : [];
  const byId = new Map(tasks.map((task) => [task.contract.id, task]));
  const projects = new Map(tasks.map((task) => [task.contract.id, task.contract.project_id]));

  const running: QueueEntry[] = snapshot.running.map((entry) => ({
    taskId: entry.task_id,
    projectId: entry.project_id,
    objective: byId.get(entry.task_id)?.contract.objective ?? OBJECTIVE_UNAVAILABLE,
    agent: entry.agent,
    state: "Running",
    waiting: null,
  }));
  // Verifying tasks hold no slot, so the snapshot omits them; they are still active work.
  for (const task of tasks) {
    if (task.state !== "Verifying") continue;
    running.push({
      taskId: task.contract.id,
      projectId: task.contract.project_id,
      objective: task.contract.objective,
      agent: task.contract.agent,
      state: "Verifying",
      waiting: null,
    });
  }
  const queued: QueueEntry[] = snapshot.queued.map((entry) => {
    const task = byId.get(entry.task_id);
    return {
      taskId: entry.task_id,
      projectId: entry.project_id,
      objective: task?.contract.objective ?? OBJECTIVE_UNAVAILABLE,
      agent: task?.contract.agent ?? AGENT_UNAVAILABLE,
      state: "Queued",
      waiting: waitingOf(entry, snapshot),
    };
  });

  const attention = approvals
    .filter((approval) => approval.state.state === "requested")
    .map<AttentionItem>((approval) => ({
      approvalId: approval.id,
      taskId: approval.action.task_id,
      projectId: projects.get(approval.action.task_id) ?? null,
      kind: approvalKind(approval),
      priority: "unknown",
    }));

  const next = snapshot.queued.find((entry) => entry.position === 1);
  let upNext: Measured<string>;
  if (snapshot.dispatch_blocked) {
    upNext = { status: "unknown", reason: "The daemon's restart gate is blocking all launches." };
  } else if (next) {
    upNext = measured(next.task_id, at);
  } else {
    upNext = { status: "unknown", reason: "No queued task has a dispatch position." };
  }

  return {
    running,
    queued,
    attention,
    slots: measured({ used: snapshot.running.length, limit: snapshot.limits.max_concurrent_workers }, at),
    upNext,
    dispatchPaused: unsupportedValue(
      snapshot.dispatch_blocked
        ? "The daemon has no pause contract. Its restart gate is blocking all launches, which is not a pause you can set."
        : "The daemon has no dispatch pause contract.",
    ),
    ...(reading.tasks.ok && reading.approvals.ok
      ? {}
      : {
          degraded: {
            ...(reading.tasks.ok ? {} : { tasks: reading.tasks.reason }),
            ...(reading.approvals.ok ? {} : { approvals: reading.approvals.reason }),
          },
        }),
  };
}

export function createDerivedActivity(connection: ConnectionManager, now: () => number): ActivitySource {
  return {
    provenance: PROVENANCE,
    async activity() {
      try {
        const reading = await read(connection);
        const at = observation(now());
        return okResult(PROVENANCE, snapshotOf(reading, at), at);
      } catch (error) {
        return failure(error);
      }
    },
    simulatePauseDispatch: () => Promise.reject(fixtureOnly("pause of dispatch")),
    simulateStop: () => Promise.reject(fixtureOnly("stop")),
  };
}
