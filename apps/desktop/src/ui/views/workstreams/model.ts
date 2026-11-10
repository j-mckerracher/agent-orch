/**
 * Workstream view model (M7-11, OX-S4, decision D-4).
 *
 * Interim model: one workstream per registered project, tasks as its children. Two
 * origins feed the same shape and the UI never mixes them:
 * - live (`D9` provenance `daemon`): derived here from the AppData snapshots
 *   (`projects.list`, `tasks.list`, `sessions.list`, `approvals.list`), so the tree and
 *   the view follow live events. Objectives and revisions have no daemon record and are
 *   `unsupported`; nothing is invented.
 * - fixture (`D9` provenance `fixture`): read from the injected `WorkstreamSource`; every
 *   value is PLACEHOLDER DATA and `fixture:` ids are never routed or sent anywhere.
 */
import { useMemo } from "react";
import { isFixtureId } from "../../../daemon/api";
import type { Approval, ProjectRow, SessionEntry, TaskRecord, TaskState } from "../../../daemon/types";
import { countStates, unsupportedValue } from "../../../sources/surfaces/result";
import type { Measured, Workstream, WorkstreamObjective } from "../../../sources/surfaces/types";
import type { Provenance } from "../../../sources/types";
import { basename, pendingApprovals } from "../../format";
import { useGate, useServices, useStore, useSurface } from "../../services";

export interface WsWorker {
  sessionId: string;
  adapter: string;
  state: string;
}

export interface WsTask {
  id: string;
  projectId: string;
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
  projectIds: string[];
  tasks: WsTask[];
  counts: Record<TaskState, number>;
  objective: Measured<WorkstreamObjective>;
}

export type WsModel =
  | { status: "loading" }
  | { status: "offline" }
  | { status: "error"; message: string }
  | { status: "unavailable" | "unsupported" | "unknown"; reason: string; provenance: Provenance }
  | { status: "ready"; mode: "live" | "fixture"; provenance: Provenance; groups: WsGroup[]; stale: boolean };

const NO_OBJECTIVE = "The daemon records no objective or revision for a project (interim grouping, decision D-4).";

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
 * pending approvals. `review.evidence` is not fetched per task, so a candidate's evidence
 * detail stays on the review screen. `approvals === null` means the list is not loaded.
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

/** Pure derivation of the live model. Tasks of unregistered projects get their own group, as D9 does. */
export function deriveLiveGroups(
  projects: readonly ProjectRow[],
  tasks: readonly TaskRecord[],
  sessions: readonly SessionEntry[] | null,
  approvals: readonly Approval[] | null,
): WsGroup[] {
  const ids = projects.map((project) => project.id);
  for (const task of tasks) if (!ids.includes(task.contract.project_id)) ids.push(task.contract.project_id);
  const byId = new Map(tasks.map((task) => [task.contract.id, task]));
  return ids.map<WsGroup>((id) => {
    const own = tasks.filter((task) => task.contract.project_id === id);
    return {
      id,
      title: basename(id),
      projectIds: [id],
      tasks: own.map<WsTask>((task) => ({
        id: task.contract.id,
        projectId: id,
        objective: task.contract.objective,
        state: task.state,
        agent: task.contract.agent,
        dependencies: task.contract.dependencies,
        workers: workersOf(task, sessions),
        attempt: task.attempts.reduce<number | null>(
          (latest, current) => (latest === null || current.attempt_number > latest ? current.attempt_number : latest),
          null,
        ),
        requiredEvidence: requiredEvidence(
          { id: task.contract.id, state: task.state, dependencies: task.contract.dependencies },
          byId,
          approvals,
        ),
        pendingDecision: pendingApprovals(approvals ?? []).some((approval) => approval.action.task_id === task.contract.id),
        openable: !isFixtureId(task.contract.id),
      })),
      counts: countStates(own.map((task) => task.state)),
      objective: unsupportedValue(NO_OBJECTIVE),
    };
  });
}

/** Maps the fixture D9 workstreams to the same shape. Placeholder tasks have no detail route. */
export function groupsFromSource(workstreams: readonly Workstream[]): WsGroup[] {
  const byId = new Map(workstreams.flatMap((ws) => ws.tasks.map((task) => [task.id, task] as const)));
  return workstreams.map<WsGroup>((ws) => ({
    id: ws.id,
    title: ws.title,
    projectIds: ws.projectIds,
    tasks: ws.tasks.map<WsTask>((task) => ({
      id: task.id,
      projectId: ws.projectIds[0] ?? ws.id,
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
    objective: ws.objective,
  }));
}

/**
 * Loads the workstream model for the view and the sidebar tree. The mode is chosen by the
 * D9 source's provenance and never changes at runtime: a failed live load is shown as
 * such, never replaced by fixture data.
 */
export function useWorkstreamModel(): WsModel {
  const { data, surfaces } = useServices();
  const gate = useGate();
  const { projects, tasks, sessions, approvals } = useStore(data);
  const source = surfaces.workstreams;
  const provenance = source.provenance;
  const fixture = provenance.kind === "fixture";
  const fixtureState = useSurface(
    () => (fixture ? source.workstreams() : new Promise<never>(() => {})),
    "ws-model",
    provenance,
  );

  const live = useMemo(
    () =>
      projects.data && tasks.data ? deriveLiveGroups(projects.data, tasks.data, sessions.data, approvals.data) : null,
    [projects.data, tasks.data, sessions.data, approvals.data],
  );
  const fixtureGroups = useMemo(
    () => (fixtureState.status === "ok" ? groupsFromSource(fixtureState.data) : null),
    [fixtureState],
  );

  if (fixture) {
    if (fixtureState.status === "loading") return { status: "loading" };
    if (fixtureState.status !== "ok") {
      return { status: fixtureState.status, reason: fixtureState.reason, provenance: fixtureState.provenance };
    }
    return { status: "ready", mode: "fixture", provenance, groups: fixtureGroups ?? [], stale: false };
  }
  if (live) return { status: "ready", mode: "live", provenance, groups: live, stale: !gate.connected };
  const failed = [projects, tasks].find((loadable) => loadable.status === "error" && loadable.error);
  if (failed?.error) return { status: "error", message: failed.error.message };
  const settling = ["idle", "connecting", "reconnecting"].includes(gate.connection.status);
  if (!gate.connected && !settling && projects.status !== "loading" && tasks.status !== "loading") {
    return { status: "offline" };
  }
  return { status: "loading" };
}
