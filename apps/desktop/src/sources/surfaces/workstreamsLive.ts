/**
 * Live D9 adapter (M7-23): `workstreams.list` and `workstreams.get` joined with `tasks.list`,
 * plus the real `workstreams.*` mutations.
 *
 * Rules (shared with `derived.ts`, which wires this in):
 * - Never imports fixtures. A failed read is `unavailable` or `unsupported` with daemon
 *   provenance; it is never answered with fixture data or a project-grouped stand-in.
 * - Daemon shapes are mapped, not hidden: any number of objectives, gate states
 *   pending/verified/failed with evidence task ids, workstream state and revision.
 * - Every mutation refuses `fixture:` ids before a request and carries an idempotency key that
 *   is reused only while the previous outcome is unknown. Writes pass the revision the caller read.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { isFixtureId } from "../../daemon/api";
import { DaemonError, asDaemonError } from "../../daemon/errors";
import type { IdempotencyTracker } from "../../daemon/idempotency";
import type { ObjectiveWire, TaskRecord, WorkstreamDetailWire, WorkstreamWire } from "../../daemon/types";
import { countStates, measured, observation } from "./result";
import type { Provenance, SurfaceResult, Workstream, WorkstreamMutation, WorkstreamObjective, WorkstreamSource } from "./types";

type Guarded = <T>(provenance: Provenance, now: () => number, read: () => Promise<T>) => Promise<SurfaceResult<T>>;

/** Shown for a linked task that `tasks.list` did not return; the daemon's own fields are kept. */
const UNJOINED_AGENT = "unknown";

function objectiveOf(wire: ObjectiveWire): WorkstreamObjective {
  return {
    id: wire.id,
    title: wire.title,
    description: wire.description,
    revision: wire.revision,
    gates: wire.gates.map((gate) => ({
      id: gate.id,
      label: gate.description,
      state: gate.state,
      evidenceTaskId: gate.evidence_task_id,
    })),
  };
}

function workstreamOf(summary: WorkstreamWire, detail: WorkstreamDetailWire, tasks: Map<string, TaskRecord>, nowMs: number): Workstream {
  const joined = detail.tasks.map((link) => {
    const record = tasks.get(link.task_id);
    return {
      id: link.task_id,
      projectId: link.project_id,
      objectiveId: link.objective_id,
      objective: record?.contract.objective ?? "",
      state: record?.state ?? link.task_state,
      agent: record?.contract.agent ?? UNJOINED_AGENT,
      dependencies: record?.contract.dependencies ?? [],
    };
  });
  return {
    id: summary.id,
    title: detail.workstream.title,
    summary: detail.workstream.summary,
    state: detail.workstream.state,
    revision: detail.workstream.revision,
    projectIds: detail.project_ids,
    tasks: joined,
    counts: countStates(joined.map((task) => task.state)),
    objectives: measured(detail.objectives.map(objectiveOf), observation(nowMs)),
  };
}

export function createLiveWorkstreams(
  connection: ConnectionManager,
  now: () => number,
  keys: IdempotencyTracker,
  guarded: Guarded,
  provenance: Provenance,
): Pick<
  WorkstreamSource,
  "workstreams" | "create" | "update" | "addObjective" | "reviseObjective" | "setGate" | "linkTask" | "unlinkTask"
> {
  const receipt = (id: string, revision: number | null): WorkstreamMutation => ({
    id,
    revision,
    at: new Date(now()).toISOString(),
  });

  async function mutate<T>(
    scope: string,
    fingerprint: unknown,
    ids: readonly (string | undefined)[],
    send: (key: string) => Promise<T>,
  ): Promise<T> {
    if (ids.some((id) => id !== undefined && isFixtureId(id))) {
      throw new DaemonError("invalid-params", "Fixture ids are never sent to the daemon.");
    }
    const key = keys.keyFor(scope, JSON.stringify(fingerprint));
    try {
      const result = await send(key);
      keys.settle(scope, null);
      return result;
    } catch (error) {
      keys.settle(scope, asDaemonError(error));
      throw error;
    }
  }

  return {
    workstreams: () =>
      guarded(provenance, now, async () => {
        const api = connection.api();
        const list = await api.workstreamsList();
        const details = await Promise.all(list.map((item) => api.workstreamsGet(item.id)));
        // Tasks are read after the links so a linked task is never newer than the snapshot of it.
        const tasks = new Map((await api.tasksList()).map((task) => [task.contract.id, task]));
        const nowMs = now();
        return list.map((item, index) => workstreamOf(item, details[index]!, tasks, nowMs));
      }),

    create: (title, summary) =>
      mutate("ws:create", [title, summary], [], async (key) => {
        const created = await connection.api().workstreamsCreate(title, summary, key);
        return receipt(created.id, created.revision);
      }),

    update: (target, change) =>
      mutate("ws:update:" + target.id, [target.revision, change], [target.id], async (key) => {
        const updated = await connection.api().workstreamsUpdate(
          { workstream_id: target.id, expected_revision: target.revision, ...change },
          key,
        );
        return receipt(updated.id, updated.revision);
      }),

    addObjective: (workstreamId, objective) =>
      mutate("ws:add-objective:" + workstreamId, objective, [workstreamId], async (key) => {
        const added = await connection.api().workstreamsAddObjective(
          workstreamId,
          {
            title: objective.title,
            ...(objective.description === undefined ? {} : { description: objective.description }),
            ...(objective.gates === undefined ? {} : { gates: objective.gates.map((description) => ({ description })) }),
          },
          key,
        );
        return receipt(added.id, added.revision);
      }),

    reviseObjective: (target, change) =>
      mutate("ws:revise-objective:" + target.id, [target.revision, change], [target.id], async (key) => {
        const revised = await connection.api().workstreamsReviseObjective(
          {
            objective_id: target.id,
            expected_revision: target.revision,
            ...(change.title === undefined ? {} : { title: change.title }),
            ...(change.description === undefined ? {} : { description: change.description }),
            ...(change.addGates === undefined ? {} : { add_gates: change.addGates.map((description) => ({ description })) }),
          },
          key,
        );
        return receipt(revised.id, revised.revision);
      }),

    setGate: (target, gateId, state, evidenceTaskId) =>
      mutate(
        `ws:set-gate:${target.id}:${gateId}`,
        [target.revision, state, evidenceTaskId],
        [target.id, evidenceTaskId],
        async (key) => {
          const gated = await connection.api().workstreamsSetGate(
            {
              objective_id: target.id,
              expected_revision: target.revision,
              gate_id: gateId,
              state,
              ...(evidenceTaskId === undefined ? {} : { evidence_task_id: evidenceTaskId }),
            },
            key,
          );
          return receipt(gated.id, gated.revision);
        },
      ),

    linkTask: (workstreamId, taskId, objectiveId) =>
      mutate(`ws:link:${workstreamId}:${taskId}`, objectiveId ?? null, [workstreamId, taskId, objectiveId], async (key) => {
        const link = await connection.api().workstreamsLinkTask(workstreamId, taskId, objectiveId, key);
        return receipt(link.task_id, null);
      }),

    unlinkTask: (workstreamId, taskId) =>
      mutate(`ws:unlink:${workstreamId}:${taskId}`, null, [workstreamId, taskId], async (key) => {
        await connection.api().workstreamsUnlinkTask(workstreamId, taskId, key);
        return receipt(taskId, null);
      }),
  };
}
