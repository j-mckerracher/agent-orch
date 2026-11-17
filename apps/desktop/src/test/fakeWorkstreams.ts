/*
 * A stateful in-memory `workstreams.*` daemon for tests. It follows the real daemon's rules
 * (crates/orchestrator-daemon/src/api_workstreams.rs): optimistic revisions, append-only gates
 * with legal transitions, one workstream per task link, archived is terminal, and a replayed
 * idempotency key returns the first result. It is test-only and never imported by app code.
 */
import type {
  ObjectiveWire,
  TaskContract,
  TaskRecord,
  WorkstreamStateWire,
  WorkstreamTaskLinkWire,
  WorkstreamWire,
} from "../daemon/types";
import { StubRpcError, mkTask, type FakeDaemon } from "./fakeDaemon";

const CONFLICT = -32011;
const NOT_FOUND = -32010;
const INVALID = -32602;
const IDEMPOTENCY_CONFLICT = -32013;

/** States a plan edit may replace; Remove refuses only these two. */
const REPLACEABLE = new Set(["Created", "Queued", "Failed", "VerificationFailed"]);
const IN_FLIGHT = new Set(["Running", "Verifying"]);
const PLAN_AGENTS = new Set(["codex", "claude-code", "github-copilot", "pi"]);

const GATE_MOVES: Record<string, string[]> = {
  pending: ["verified", "failed"],
  failed: ["pending", "verified"],
  verified: ["pending"],
};

export interface FakeWorkstreams {
  workstreams: WorkstreamWire[];
  objectives: ObjectiveWire[];
  links: Array<{ task_id: string; workstream_id: string; objective_id: string | null }>;
  /** Mutation requests applied (replays excluded), in order. */
  applied: Array<{ method: string; params: Record<string, unknown> }>;
}

type Params = Record<string, unknown>;

export function installFakeWorkstreams(
  fake: FakeDaemon,
  seed: Partial<Pick<FakeWorkstreams, "workstreams" | "objectives" | "links">> = {},
): FakeWorkstreams {
  const state: FakeWorkstreams = {
    workstreams: seed.workstreams ?? [],
    objectives: seed.objectives ?? [],
    links: seed.links ?? [],
    applied: [],
  };
  const seen = new Map<string, { fingerprint: string; result: unknown }>();
  let counter = 100;

  const workstream = (id: unknown) => {
    const found = state.workstreams.find((item) => item.id === id);
    if (!found) throw new StubRpcError(NOT_FOUND, "workstream not found");
    return found;
  };
  const objective = (id: unknown) => {
    const found = state.objectives.find((item) => item.id === id);
    if (!found) throw new StubRpcError(NOT_FOUND, "objective not found");
    return found;
  };
  const revision = (target: { revision: number }, expected: unknown) => {
    if (target.revision !== expected) throw new StubRpcError(CONFLICT, "stale revision: record changed since it was read");
  };
  const writable = (ws: WorkstreamWire) => {
    if (ws.state === "archived") throw new StubRpcError(CONFLICT, "workstream is archived");
  };

  /** Applies a mutation once per idempotency key; a replay returns the first result. */
  const mutation = (method: string, apply: (params: Params) => unknown) => {
    fake.stub(method, (params: Params) => {
      const { idempotency_key: key, ...rest } = params;
      if (typeof key !== "string" || key === "") throw new StubRpcError(INVALID, "idempotency_key is required");
      const fingerprint = JSON.stringify([method, rest]);
      const prior = seen.get(key);
      if (prior) {
        if (prior.fingerprint !== fingerprint) throw new StubRpcError(IDEMPOTENCY_CONFLICT, "idempotency key reused with different parameters");
        return prior.result;
      }
      const result = apply(rest);
      state.applied.push({ method, params: rest });
      seen.set(key, { fingerprint, result });
      return result;
    });
  };

  fake.stub("workstreams.list", (params: Params) =>
    state.workstreams.filter((item) => params.state === undefined || item.state === params.state),
  );

  fake.stub("workstreams.get", (params: Params) => {
    const ws = workstream(params.workstream_id);
    const tasks: WorkstreamTaskLinkWire[] = state.links
      .filter((link) => link.workstream_id === ws.id)
      .map((link) => {
        const record = fake.tasks.find((task) => task.contract.id === link.task_id);
        return {
          ...link,
          project_id: record?.contract.project_id ?? "",
          task_state: record?.state ?? "Created",
        };
      });
    return {
      workstream: ws,
      objectives: state.objectives.filter((item) => item.workstream_id === ws.id),
      tasks,
      project_ids: [...new Set(tasks.map((task) => task.project_id))].sort(),
    };
  });

  mutation("workstreams.create", (params) => {
    const title = String(params.title ?? "").trim();
    if (title === "") throw new StubRpcError(INVALID, "title must not be empty");
    const created: WorkstreamWire = { id: `ws-${counter++}`, title, summary: String(params.summary ?? ""), state: "active", revision: 1 };
    state.workstreams.push(created);
    return created;
  });

  mutation("workstreams.update", (params) => {
    const ws = workstream(params.workstream_id);
    revision(ws, params.expected_revision);
    if (params.title === undefined && params.summary === undefined && params.state === undefined) {
      throw new StubRpcError(INVALID, "update changes nothing");
    }
    if (ws.state === "archived") throw new StubRpcError(CONFLICT, "workstream is archived");
    if (params.title !== undefined) ws.title = String(params.title);
    if (params.summary !== undefined) ws.summary = String(params.summary);
    if (params.state !== undefined) ws.state = params.state as WorkstreamStateWire;
    ws.revision += 1;
    return { ...ws };
  });

  mutation("workstreams.add_objective", (params) => {
    const ws = workstream(params.workstream_id);
    writable(ws);
    const gates = ((params.gates as Array<{ description: string }> | undefined) ?? []).map((gate, index) => ({
      id: `gate-${index + 1}`,
      description: gate.description,
      state: "pending" as const,
      evidence_task_id: null,
    }));
    const added: ObjectiveWire = {
      id: `obj-${counter++}`,
      workstream_id: ws.id,
      title: String(params.title),
      description: String(params.description ?? ""),
      gates,
      revision: 1,
    };
    state.objectives.push(added);
    return added;
  });

  mutation("workstreams.revise_objective", (params) => {
    const target = objective(params.objective_id);
    writable(workstream(target.workstream_id));
    revision(target, params.expected_revision);
    const add = (params.add_gates as Array<{ description: string }> | undefined) ?? [];
    if (params.title === undefined && params.description === undefined && add.length === 0) {
      throw new StubRpcError(INVALID, "revision changes nothing");
    }
    if (params.title !== undefined) target.title = String(params.title);
    if (params.description !== undefined) target.description = String(params.description);
    for (const gate of add) {
      target.gates.push({ id: `gate-${target.gates.length + 1}`, description: gate.description, state: "pending", evidence_task_id: null });
    }
    target.revision += 1;
    return { ...target };
  });

  mutation("workstreams.set_gate", (params) => {
    const target = objective(params.objective_id);
    writable(workstream(target.workstream_id));
    revision(target, params.expected_revision);
    const gate = target.gates.find((item) => item.id === params.gate_id);
    if (!gate) throw new StubRpcError(NOT_FOUND, "gate not found");
    const next = String(params.state);
    if (!GATE_MOVES[gate.state]?.includes(next)) throw new StubRpcError(CONFLICT, `gate cannot move from ${gate.state} to ${next}`);
    const evidence = params.evidence_task_id;
    if (evidence !== undefined) {
      if (next === "pending") throw new StubRpcError(INVALID, "a pending gate has no evidence");
      if (!state.links.some((link) => link.task_id === evidence && link.workstream_id === target.workstream_id)) {
        throw new StubRpcError(INVALID, "evidence task is not linked to this workstream");
      }
    }
    gate.state = next as typeof gate.state;
    gate.evidence_task_id = typeof evidence === "string" ? evidence : null;
    target.revision += 1;
    return { ...target };
  });

  mutation("workstreams.link_task", (params) => {
    const ws = workstream(params.workstream_id);
    writable(ws);
    const taskId = String(params.task_id);
    const record = fake.tasks.find((task) => task.contract.id === taskId);
    if (!record) throw new StubRpcError(NOT_FOUND, "task not found");
    const objectiveId = typeof params.objective_id === "string" ? params.objective_id : null;
    if (objectiveId !== null && objective(objectiveId).workstream_id !== ws.id) {
      throw new StubRpcError(INVALID, "objective belongs to another workstream");
    }
    const existing = state.links.find((link) => link.task_id === taskId);
    if (existing && (existing.workstream_id !== ws.id || existing.objective_id !== objectiveId)) {
      throw new StubRpcError(CONFLICT, "task is already linked; unlink it first");
    }
    if (!existing) state.links.push({ task_id: taskId, workstream_id: ws.id, objective_id: objectiveId });
    return { task_id: taskId, workstream_id: ws.id, objective_id: objectiveId, project_id: record.contract.project_id, task_state: record.state };
  });

  mutation("workstreams.unlink_task", (params) => {
    const ws = workstream(params.workstream_id);
    writable(ws);
    const cited = state.objectives.some(
      (item) => item.workstream_id === ws.id && item.gates.some((gate) => gate.evidence_task_id === params.task_id),
    );
    if (cited && state.links.some((link) => link.task_id === params.task_id && link.workstream_id === ws.id)) {
      throw new StubRpcError(CONFLICT, "task is cited as gate evidence");
    }
    const before = state.links.length;
    state.links = state.links.filter((link) => !(link.task_id === params.task_id && link.workstream_id === ws.id));
    return { removed: state.links.length < before };
  });

  // Plan edits (crates/orchestrator-daemon/src/api_workstream_plan.rs): new tasks are queued and linked under the
  // replaced task's objective; a replaced task that had not started is cancelled and stays linked as history.
  const replaced = new Set<string>();
  const citedIn = (ws: WorkstreamWire, taskId: unknown) =>
    state.objectives.some((item) => item.workstream_id === ws.id && item.gates.some((gate) => gate.evidence_task_id === taskId));
  const linked = (ws: WorkstreamWire, taskId: unknown) => {
    const link = state.links.find((item) => item.task_id === taskId && item.workstream_id === ws.id);
    const record = fake.tasks.find((task) => task.contract.id === taskId);
    if (!link || !record) throw new StubRpcError(NOT_FOUND, "task is not linked to this workstream");
    return { link, record };
  };
  const replaceable = (ws: WorkstreamWire, taskId: unknown) => {
    const found = linked(ws, taskId);
    if (!REPLACEABLE.has(found.record.state)) throw new StubRpcError(CONFLICT, `task is ${found.record.state}`);
    if (replaced.has(found.record.contract.id)) throw new StubRpcError(CONFLICT, "task was already replaced");
    if (citedIn(ws, taskId)) throw new StubRpcError(CONFLICT, "task is cited as gate evidence");
    return found;
  };
  const queue = (ws: WorkstreamWire, objectiveId: string | null, contract: Partial<TaskContract> & { project_id: string }) => {
    const id = `ws-task-${counter++}`;
    fake.tasks.push(mkTask(id, contract.project_id, { state: "Queued", contract: { ...contract, id } }));
    state.links.push({ task_id: id, workstream_id: ws.id, objective_id: objectiveId });
    return id;
  };
  const retire = (records: TaskRecord[]) => {
    for (const record of records) {
      replaced.add(record.contract.id);
      if (record.state === "Created" || record.state === "Queued") {
        record.state = "Cancelled";
        record.version += 1;
      }
    }
    return records.map((record) => record.contract.id);
  };
  const text = (value: unknown, what: string) => {
    const trimmed = typeof value === "string" ? value.trim() : "";
    if (trimmed === "") throw new StubRpcError(INVALID, `${what} must not be empty`);
    return trimmed;
  };

  mutation("workstreams.add_task", (params) => {
    const ws = workstream(params.workstream_id);
    writable(ws);
    const objectiveId = typeof params.objective_id === "string" ? params.objective_id : null;
    if (objectiveId !== null && objective(objectiveId).workstream_id !== ws.id) {
      throw new StubRpcError(INVALID, "objective belongs to another workstream");
    }
    const contract = params.contract as TaskContract;
    if (!PLAN_AGENTS.has(contract.agent)) throw new StubRpcError(INVALID, "unknown agent");
    const created = queue(ws, objectiveId, { ...contract, objective: text(contract.objective, "objective") });
    return { created: [created], replaced: [] };
  });

  mutation("workstreams.split_task", (params) => {
    const ws = workstream(params.workstream_id);
    writable(ws);
    const parts = (params.objectives as unknown[]).map((part) => text(part, "objective"));
    if (parts.length < 2 || parts.length > 8) throw new StubRpcError(INVALID, "a split needs 2 to 8 objectives");
    const { link, record } = replaceable(ws, params.task_id);
    const created = parts.map((part) => queue(ws, link.objective_id, { ...record.contract, objective: part }));
    return { created, replaced: retire([record]) };
  });

  mutation("workstreams.combine_tasks", (params) => {
    const ws = workstream(params.workstream_id);
    writable(ws);
    const ids = params.task_ids as string[];
    if (ids.length < 2 || ids.length > 8 || new Set(ids).size !== ids.length) {
      throw new StubRpcError(INVALID, "combine needs 2 to 8 distinct tasks");
    }
    const sources = ids.map((id) => replaceable(ws, id));
    const first = sources[0]!;
    if (sources.some(({ record }) => record.contract.project_id !== first.record.contract.project_id || record.contract.kind !== first.record.contract.kind)) {
      throw new StubRpcError(INVALID, "combined tasks must share one project and kind");
    }
    const objectiveId = sources.every(({ link }) => link.objective_id === first.link.objective_id) ? first.link.objective_id : null;
    const created = queue(ws, objectiveId, { ...first.record.contract, objective: text(params.objective, "objective") });
    return { created: [created], replaced: retire(sources.map(({ record }) => record)) };
  });

  mutation("workstreams.remove_task", (params) => {
    const ws = workstream(params.workstream_id);
    writable(ws);
    const { record } = linked(ws, params.task_id);
    if (IN_FLIGHT.has(record.state)) throw new StubRpcError(CONFLICT, `task is ${record.state}`);
    if (citedIn(ws, params.task_id)) throw new StubRpcError(CONFLICT, "task is cited as gate evidence");
    const cancelled = record.state === "Created" || record.state === "Queued";
    if (cancelled) {
      record.state = "Cancelled";
      record.version += 1;
    }
    state.links = state.links.filter((link) => !(link.task_id === params.task_id && link.workstream_id === ws.id));
    return { task_id: record.contract.id, cancelled, removed: true };
  });

  mutation("workstreams.reassign_task", (params) => {
    const ws = workstream(params.workstream_id);
    writable(ws);
    const agent = String(params.agent);
    if (!PLAN_AGENTS.has(agent)) throw new StubRpcError(INVALID, "unknown agent");
    const { link, record } = replaceable(ws, params.task_id);
    if (record.contract.agent === agent) throw new StubRpcError(INVALID, `task already runs on ${agent}`);
    const created = queue(ws, link.objective_id, { ...record.contract, agent });
    return { created: [created], replaced: retire([record]) };
  });

  mutation("workstreams.set_task_policy", (params) => {
    const ws = workstream(params.workstream_id);
    writable(ws);
    const { workstream_id: _ws, task_id: _task, ...patch } = params;
    if (Object.keys(patch).length === 0) throw new StubRpcError(INVALID, "the policy change is empty");
    const { link, record } = replaceable(ws, params.task_id);
    const { permission, ...rest } = { ...record.contract, ...(patch as Partial<TaskContract>) };
    const contract = permission === undefined || permission === "restricted" ? rest : { ...rest, permission };
    const created = queue(ws, link.objective_id, contract);
    return { created: [created], replaced: retire([record]) };
  });

  return state;
}
