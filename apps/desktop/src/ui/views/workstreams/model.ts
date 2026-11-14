/**
 * Workstream view model (M7-11, OX-S4; live binding M7-23).
 *
 * Two origins feed the same shape and the UI never mixes them:
 * - live (`D9` provenance `daemon`): the workstreams, objectives, gates and task links come
 *   from `workstreams.list` / `workstreams.get`. Task state, workers, attempts and pending
 *   decisions are joined from the AppData snapshots so the tree follows live events. Tasks the
 *   daemon has not linked to any workstream are reported separately, never hidden.
 * - fixture (`D9` provenance `fixture`): read from the injected `WorkstreamSource`; every
 *   value is PLACEHOLDER DATA and `fixture:` ids are never routed or sent anywhere.
 * A failed live load is shown as such; it never falls back to fixture data.
 */
import { useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { isFixtureId } from "../../../daemon/api";
import type { Loadable } from "../../../daemon/appData";
import { presentError } from "../../../daemon/errors";
import type { Approval, SessionEntry, TaskRecord, TaskState } from "../../../daemon/types";
import type {
  Measured,
  Workstream,
  WorkstreamLifecycle,
  WorkstreamObjective,
  WorkstreamTask,
} from "../../../sources/surfaces/types";
import type { Provenance } from "../../../sources/types";
import { pendingApprovals } from "../../format";
import { useGate, useServices, useStore, useSurface } from "../../services";

export interface WsWorker {
  sessionId: string;
  adapter: string;
  state: string;
}

export interface WsTask {
  id: string;
  projectId: string;
  /** The objective this task is linked under; null when linked to the workstream only. */
  objectiveId: string | null;
  objective: string;
  state: TaskState;
  /** The adapter the contract asks for; sessions record the adapter that actually ran. */
  agent: string;
  dependencies: string[];
  /** Recorded sessions, oldest first. Empty means none recorded. */
  workers: WsWorker[];
  /** Latest attempt number, or null when no attempt was recorded. */
  attempt: number | null;
  requiredEvidence: string;
  /** A pending approval waits on the operator for this task (live only). */
  pendingDecision: boolean;
  /** True only for a real daemon task: the one kind that has a detail route. */
  openable: boolean;
}

export interface WsGroup {
  id: string;
  title: string;
  summary: string;
  /** Advisory only: the scheduler does not consult it. */
  state: WorkstreamLifecycle;
  /** The revision the next `workstreams.update` must quote. */
  revision: number;
  projectIds: string[];
  tasks: WsTask[];
  counts: Measured<Record<TaskState, number>>;
  objectives: Measured<WorkstreamObjective[]>;
  /** Set when this workstream's detail read failed; its tasks, counts and objectives are unknown. */
  unreadReason: string | null;
}

export type WsModel =
  | { status: "loading" }
  | { status: "offline"; observedAt: string; ageMs: number }
  | { status: "error"; message: string; observedAt: string; ageMs: number }
  | { status: "unavailable" | "unsupported" | "unknown"; reason: string; provenance: Provenance; observedAt: string; ageMs: number }
  | {
      status: "ready";
      mode: "live" | "fixture";
      provenance: Provenance;
      taskList: Loadable<TaskRecord[]>;
      groups: WsGroup[];
      /** Daemon tasks linked to no workstream (live only); candidates for `workstreams.link_task`. */
      unlinked: WsTask[];
      /** False when a workstream could not be read: a task linked there would wrongly look unlinked. */
      unlinkedKnown: boolean;
      stale: boolean;
    };

export function taskListNotice(taskList: Loadable<TaskRecord[]>): string {
  switch (taskList.status) {
    case "idle":
      return "AppData task list has not loaded; no task snapshot is available.";
    case "loading":
      return taskList.data === null
        ? "AppData task list is loading; no task snapshot is available yet."
        : "AppData task list is refreshing; task rows use retained data with no recorded observation time.";
    case "ready":
      return "AppData task list is ready as a separate snapshot; its observation time is not tracked. D9 counts have their own observation time.";
    case "error": {
      const reason = taskList.error ? presentError(taskList.error).reason : "The failure reason is unavailable.";
      return taskList.data === null
        ? `AppData task list refresh failed; no task snapshot is available. ${reason}`
        : `AppData task list refresh failed; task rows use retained data that may be stale. ${reason}`;
    }
    default: {
      const exhaustive: never = taskList.status;
      return exhaustive;
    }
  }
}

/** "1 task", "2 tasks". */
export function taskCount(count: number): string {
  return `${count} ${count === 1 ? "task" : "tasks"}`;
}

/** A task's display name: its objective, or its id when the objective is blank. */
export function taskLabelOf(task: Pick<WsTask, "id" | "objective">): string {
  return task.objective.trim() === "" ? task.id : task.objective;
}

function approvalKind(approval: Approval): string {
  const kind = approval.action.kind;
  return kind.type === "delivery" ? kind.kind : kind.type;
}

/**
 * The next evidence a task needs, derived only from its state, its predecessors and its
 * pending approvals. It is the fallback text: the view refines it with `review.evidence`
 * for tasks that may hold a candidate (see `evidence.ts`). `approvals === null` means the
 * list is not loaded.
 */
export function requiredEvidence(
  task: { id: string; state: TaskState; dependencies: readonly string[] },
  byId: ReadonlyMap<string, { state: TaskState }>,
  approvals: readonly Approval[] | null,
): string {
  const pending = approvals ? pendingApprovals(approvals).filter((approval) => approval.action.task_id === task.id) : [];
  const first = pending[0];
  if (first) {
    return `Operator decision on ${approvalKind(first)} approval for candidate ${first.action.candidate_commit.slice(0, 8)}`;
  }
  const unloaded = approvals === null ? " (approval state not loaded)" : "";
  switch (task.state) {
    case "Created":
    case "Queued": {
      const unmet = task.dependencies.filter((dependency) => byId.get(dependency)?.state !== "Succeeded");
      return unmet.length > 0 ? `Predecessors to succeed: ${unmet.join(", ")}` : "None until dispatched";
    }
    case "Running":
      return `Candidate from the running attempt${unloaded}`;
    case "Verifying":
      return `Verification result${unloaded}`;
    case "Succeeded":
      return `None: task succeeded${unloaded}`;
    case "VerificationFailed":
      return "Repair or retry decision";
    case "Failed":
      return "Retry or investigation decision";
    case "Cancelled":
      return "None: task cancelled";
  }
}

function workersOf(task: TaskRecord, sessions: readonly SessionEntry[] | null): WsWorker[] {
  const listed = sessions?.filter((entry) => entry.task_id === task.contract.id).map((entry) => entry.session);
  const own = listed && listed.length > 0 ? listed : task.sessions;
  return own.map((session) => ({ sessionId: session.id, adapter: session.adapter, state: session.state }));
}

/**
 * One task row. The D9 link supplies membership; a loaded AppData record, when present, is
 * fresher and supplies state, contract fields, workers and attempts.
 */
function wsTaskOf(
  base: WorkstreamTask,
  record: TaskRecord | undefined,
  sessions: readonly SessionEntry[] | null,
  approvals: readonly Approval[] | null,
  states: ReadonlyMap<string, { state: TaskState }>,
): WsTask {
  const state = record?.state ?? base.state;
  const dependencies = record?.contract.dependencies ?? base.dependencies;
  return {
    id: base.id,
    projectId: base.projectId,
    objectiveId: base.objectiveId,
    objective: record?.contract.objective ?? base.objective,
    state,
    agent: record?.contract.agent ?? base.agent,
    dependencies,
    workers: record ? workersOf(record, sessions) : [],
    attempt:
      record?.attempts.reduce<number | null>(
        (latest, current) => (latest === null || current.attempt_number > latest ? current.attempt_number : latest),
        null,
      ) ?? null,
    requiredEvidence: requiredEvidence({ id: base.id, state, dependencies }, states, approvals),
    pendingDecision: pendingApprovals(approvals ?? []).some((approval) => approval.action.task_id === base.id),
    openable: !isFixtureId(base.id),
  };
}

/** Pure derivation of the live model from the D9 workstreams and the AppData snapshots. */
export function deriveLiveGroups(
  workstreams: readonly Workstream[],
  tasks: readonly TaskRecord[] | null,
  sessions: readonly SessionEntry[] | null,
  approvals: readonly Approval[] | null,
): { groups: WsGroup[]; unlinked: WsTask[]; unlinkedKnown: boolean } {
  const records = new Map((tasks ?? []).map((task) => [task.contract.id, task]));
  const states = new Map<string, { state: TaskState }>(
    workstreams.flatMap((ws) => ws.tasks.map((task) => [task.id, { state: task.state }] as const)),
  );
  for (const [id, record] of records) states.set(id, { state: record.state });

  const groups = workstreams.map<WsGroup>((ws) => {
    const own = ws.tasks.map((task) => wsTaskOf(task, records.get(task.id), sessions, approvals, states));
    return {
      id: ws.id,
      title: ws.title,
      summary: ws.summary,
      state: ws.state,
      revision: ws.revision,
      projectIds: ws.projectIds,
      tasks: own,
      counts: ws.counts,
      objectives: ws.objectives,
      unreadReason: ws.unreadReason ?? null,
    };
  });

  const unlinkedKnown = groups.every((group) => group.unreadReason === null);
  const linked = new Set(workstreams.flatMap((ws) => ws.tasks.map((task) => task.id)));
  const unlinked = (unlinkedKnown ? tasks ?? [] : [])
    .filter((record) => !linked.has(record.contract.id))
    .map((record) =>
      wsTaskOf(
        {
          id: record.contract.id,
          projectId: record.contract.project_id,
          objectiveId: null,
          objective: record.contract.objective,
          state: record.state,
          agent: record.contract.agent,
          dependencies: record.contract.dependencies,
        },
        record,
        sessions,
        approvals,
        states,
      ),
    );
  return { groups, unlinked, unlinkedKnown };
}

/** Maps the fixture D9 workstreams to the same shape. Placeholder tasks have no detail route. */
export function groupsFromSource(workstreams: readonly Workstream[]): WsGroup[] {
  const byId = new Map(workstreams.flatMap((ws) => ws.tasks.map((task) => [task.id, task] as const)));
  return workstreams.map<WsGroup>((ws) => ({
    id: ws.id,
    title: ws.title,
    summary: ws.summary,
    state: ws.state,
    revision: ws.revision,
    projectIds: ws.projectIds,
    tasks: ws.tasks.map<WsTask>((task) => ({
      id: task.id,
      projectId: task.projectId,
      objectiveId: task.objectiveId,
      objective: task.objective,
      state: task.state,
      agent: task.agent,
      dependencies: task.dependencies,
      workers: [],
      attempt: null,
      requiredEvidence: requiredEvidence(task, byId, []),
      pendingDecision: false,
      openable: false,
    })),
    counts: ws.counts,
    objectives: ws.objectives,
    unreadReason: null,
  }));
}

/*
 * Every consumer of D9 (this view, the sidebar tree, the top-bar counts) reloads when a
 * workstream mutation lands, so none of them keeps showing the pre-mutation revision.
 */
let version = 0;
const listeners = new Set<() => void>();

export function reloadWorkstreams(): void {
  version += 1;
  for (const listener of listeners) listener();
}

export function useWorkstreamsVersion(): number {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => version,
  );
}

/**
 * Loads the workstream model for the view and the sidebar tree. The mode is chosen by the
 * D9 source's provenance and never changes at runtime: a failed live load is shown as
 * such, never replaced by fixture data.
 */
export function useWorkstreamModel(): WsModel {
  const { data, surfaces } = useServices();
  const gate = useGate();
  const { tasks, sessions, approvals } = useStore(data);
  const source = surfaces.workstreams;
  const provenance = source.provenance;
  const fixture = provenance.kind === "fixture";
  const reloads = useWorkstreamsVersion();
  // The daemon emits no workstream events: re-read on a mutation or when the task set changes.
  const membership = tasks.data?.map((task) => task.contract.id).join("\n") ?? "-";
  const state = useSurface(() => source.workstreams(), `ws-model:${reloads}:${membership}`, provenance);

  // The last good live answer stays on screen, labelled stale, while the connection is down.
  const [kept, setKept] = useState<Workstream[] | null>(null);
  useEffect(() => {
    if (!fixture && state.status === "ok") setKept(state.data);
  }, [fixture, state]);

  const loaded =
    state.status === "ok" ? state.data : !fixture && (state.status === "loading" || !gate.connected) ? kept : null;
  const derived = useMemo(() => {
    if (!loaded) return null;
    return fixture
      ? { groups: groupsFromSource(loaded), unlinked: [] as WsTask[], unlinkedKnown: true }
      : deriveLiveGroups(loaded, tasks.data, sessions.data, approvals.data);
  }, [loaded, fixture, tasks.data, sessions.data, approvals.data]);

  if (derived) {
    return {
      status: "ready",
      mode: fixture ? "fixture" : "live",
      provenance,
      taskList: tasks,
      groups: derived.groups,
      unlinked: derived.unlinked,
      unlinkedKnown: derived.unlinkedKnown,
      stale: !fixture && !gate.connected,
    };
  }
  if (state.status === "loading" || state.status === "ok") return { status: "loading" };
  if (fixture || state.status === "unsupported") {
    return { status: state.status, reason: state.reason, provenance: state.provenance, observedAt: state.observedAt, ageMs: state.ageMs };
  }
  const settling = ["idle", "connecting", "reconnecting"].includes(gate.connection.status);
  if (!gate.connected) return settling ? { status: "loading" } : { status: "offline", observedAt: state.observedAt, ageMs: state.ageMs };
  return state.status === "unknown"
    ? { status: "unknown", reason: state.reason, provenance: state.provenance, observedAt: state.observedAt, ageMs: state.ageMs }
    : { status: "error", message: state.reason, observedAt: state.observedAt, ageMs: state.ageMs };
}
