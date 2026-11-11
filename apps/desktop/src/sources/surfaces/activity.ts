/**
 * Live D11 adapter (M7-20): workspace activity from `queue.snapshot` (capability `queue`),
 * joined with `tasks.list` and `approvals.list`.
 * Updated: 2026-11-11 03:46:08 -0700 MST.
 *
 * Rules:
 * - Never imports fixtures. `queue.snapshot` is authoritative when supported. Only missing-feature
 *   errors fall back to `tasks.list` and `approvals.list`; other snapshot failures stay unavailable.
 * - The task-list fallback reports `Running`, `Verifying`, and `Queued` task states plus requested
 *   approvals. It cannot establish wait reasons, queue order, slot limits, or a pause state, so
 *   those remain unknown or unsupported. A task-list failure fails the fallback; approval failure
 *   degrades only attention.
 * - In the snapshot path, failed `tasks.list` or `approvals.list` reads degrade only their own
 *   part, named in `degraded`, and are never shown as empty. The two reads are not atomic: the pair
 *   is re-read when they disagree on `Running` or `Queued`, and the snapshot stays authoritative.
 * - Snapshot `slots.used` counts `running` only, so `Verifying` rows do not use a slot. Its limit
 *   is global; the per-project limit is named in the wait detail.
 * - `dispatchPaused` is unsupported because the daemon has no pause. The snapshot's
 *   `dispatch_blocked` is a restart gate, not a pause, and is reported as text only.
 * - `simulate*` calls reject: they exist for fixture adapters only. This adapter has no mutation.
 */
import type { DaemonApi } from "../../daemon/api";
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

const daemonProvenance = (methods: readonly string[]): Provenance => ({ kind: "daemon", id: "D11", methods });
const PROVENANCE = daemonProvenance(["queue.snapshot", "tasks.list", "approvals.list"]);
const QUEUE_PROVENANCE = daemonProvenance(["queue.snapshot"]);
const TASK_LIST_PROVENANCE = daemonProvenance(["tasks.list", "approvals.list"]);
const EMPTY_PROVENANCE = daemonProvenance([]);

const fixtureOnly = (what: string) => new DaemonError("method-unavailable", `Simulated ${what} exists only for fixture data.`);

function approvalKind(approval: Approval): string {
  const kind = approval.action.kind;
  return kind.type === "delivery" ? kind.kind : kind.type;
}

function attentionOf(approvals: readonly Approval[], projects: ReadonlyMap<string, string>): AttentionItem[] {
  return approvals
    .filter((approval) => approval.state.state === "requested")
    .map<AttentionItem>((approval) => ({
      approvalId: approval.id,
      taskId: approval.action.task_id,
      projectId: projects.get(approval.action.task_id) ?? null,
      kind: approvalKind(approval),
      priority: "unknown",
    }));
}

function reasonOf(failed: DaemonError): string {
  return `${presentError(failed).reason} (${failed.kind}: ${failed.message})`;
}

function failure<T>(error: unknown, provenance: Provenance): SurfaceResult<T> {
  const failed = asDaemonError(error);
  const reason = reasonOf(failed);
  return notOk(provenance, isMissingFeature(failed) ? "unsupported" : "unavailable", reason);
}

/** A failed read retained when the remaining data can still form a partial snapshot. */
type Part<T> = { ok: true; value: T } | { ok: false; reason: string };

type Reading =
  | {
      source: "queue.snapshot";
      snapshot: QueueSnapshotWire;
      tasks: Part<TaskRecord[]>;
      approvals: Part<Approval[]>;
    }
  | { source: "tasks.list"; tasks: TaskRecord[]; approvals: Part<Approval[]> };
type ReadResult = Reading | { source: "failure"; error: unknown; provenance: Provenance };

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

async function read(connection: ConnectionManager): Promise<ReadResult> {
  let api: DaemonApi;
  try {
    api = connection.api();
  } catch (error) {
    return { source: "failure", error, provenance: EMPTY_PROVENANCE };
  }

  let reading: Extract<Reading, { source: "queue.snapshot" }> | null = null;
  for (let attempt = 0; attempt < MAX_READ_ATTEMPTS; attempt++) {
    let snapshot: QueueSnapshotWire;
    try {
      snapshot = await api.queueSnapshot();
    } catch (error) {
      if (!isMissingFeature(asDaemonError(error))) {
        return { source: "failure", error, provenance: QUEUE_PROVENANCE };
      }
      const [tasks, approvals] = await Promise.allSettled([api.tasksList(), api.approvalsList()]);
      if (tasks.status === "rejected") {
        return { source: "failure", error: tasks.reason, provenance: TASK_LIST_PROVENANCE };
      }
      return { source: "tasks.list", tasks: tasks.value, approvals: settle(approvals) };
    }

    const [tasks, approvals] = await Promise.allSettled([api.tasksList(), api.approvalsList()]);
    reading = { source: "queue.snapshot", snapshot, tasks: settle(tasks), approvals: settle(approvals) };
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
  if (reading.source === "tasks.list") return taskListSnapshotOf(reading);
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

  const attention = attentionOf(approvals, projects);

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

function taskListSnapshotOf(reading: Extract<Reading, { source: "tasks.list" }>): ActivitySnapshot {
  const projects = new Map(reading.tasks.map((task) => [task.contract.id, task.contract.project_id]));
  const running: QueueEntry[] = reading.tasks
    .filter((task) => task.state === "Running" || task.state === "Verifying")
    .map((task): QueueEntry => ({
      taskId: task.contract.id,
      projectId: task.contract.project_id,
      objective: task.contract.objective,
      agent: task.contract.agent,
      state: task.state,
      waiting: null,
    }));
  const queued: QueueEntry[] = reading.tasks
    .filter((task) => task.state === "Queued")
    .map((task): QueueEntry => ({
      taskId: task.contract.id,
      projectId: task.contract.project_id,
      objective: task.contract.objective,
      agent: task.contract.agent,
      state: "Queued",
      waiting: { reason: "unknown", detail: "Queue order and wait reason are unavailable without queue.snapshot." },
    }));

  return {
    running,
    queued,
    attention: attentionOf(reading.approvals.ok ? reading.approvals.value : [], projects),
    slots: unsupportedValue("Scheduler slot limits are unavailable without queue.snapshot."),
    upNext: { status: "unknown", reason: "Dispatch order is unavailable without queue.snapshot." },
    dispatchPaused: unsupportedValue("The daemon has no dispatch pause contract."),
    ...(reading.approvals.ok ? {} : { degraded: { approvals: reading.approvals.reason } }),
  };
}

export function createDerivedActivity(connection: ConnectionManager, now: () => number): ActivitySource {
  return {
    provenance: PROVENANCE,
    async activity() {
      let provenance = EMPTY_PROVENANCE;
      try {
        const reading = await read(connection);
        if (reading.source === "failure") return failure(reading.error, reading.provenance);
        provenance = reading.source === "tasks.list" ? TASK_LIST_PROVENANCE : PROVENANCE;
        const at = observation(now());
        return okResult(provenance, snapshotOf(reading, at), at);
      } catch (error) {
        return failure(error, provenance);
      }
    },
    simulatePauseDispatch: () => Promise.reject(fixtureOnly("pause of dispatch")),
    simulateStop: () => Promise.reject(fixtureOnly("stop")),
  };
}
