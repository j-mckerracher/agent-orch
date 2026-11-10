/**
 * Daemon-derived adapters for D9-D16 (M7-03). They read only methods the
 * daemon already has: projects.list, tasks.list, approvals.list,
 * agents.capabilities, coordinator.get and coordinator.send_message. No
 * method is added. D11 is built in `activity.ts` from `queue.snapshot` (M7-20).
 *
 * Rules:
 * - This file never imports fixtures. A failed live call is `unavailable` (or
 *   `unsupported` when the daemon lacks the method) with daemon provenance; it
 *   is never answered with fixture data.
 * - Values the daemon does not record (spend, models, objectives, priorities,
 *   deadlines) are `unsupported` or `unknown`, never zero or an invented
 *   number. Slots and queue order are read live by `activity.ts`. Host metrics
 *   are not read here: D13 stays `unsupported` in this file.
 * - `simulate*` calls reject: they exist for fixture adapters only.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { isFixtureId } from "../../daemon/api";
import { DaemonError, asDaemonError, presentError } from "../../daemon/errors";
import { IdempotencyTracker } from "../../daemon/idempotency";
import type { Approval, CoordinatorEntry, CoordinatorPage, TaskRecord } from "../../daemon/types";
import { basename } from "../../ui/format";
import { createDerivedActivity } from "./activity";
import { countStates, notOk, observation, okResult, unsupportedValue } from "./result";
import { createSearchSource } from "./search";
import type {
  ConversationSource,
  ConversationTarget,
  DecisionItem,
  DecisionQueueSource,
  ModelCatalogSource,
  Provenance,
  SurfaceId,
  SurfaceResult,
  SurfaceSources,
  SystemMetricsSource,
  UsageSource,
  Workstream,
  WorkstreamSource,
} from "./types";

const MAX_CONVERSATION_PAGES = 50;

const prov = (id: SurfaceId, methods: readonly string[]): Provenance => ({ kind: "daemon", id, methods });

function fixtureOnly(what: string): DaemonError {
  return new DaemonError("method-unavailable", `Simulated ${what} exists only for fixture data.`);
}

/** Maps a failed live call. Retry may help for `unavailable`; it will not for `unsupported`. */
function failure<T>(provenance: Provenance, error: unknown): SurfaceResult<T> {
  const failed = asDaemonError(error);
  const reason = `${presentError(failed).reason} (${failed.kind}: ${failed.message})`;
  if (failed.kind === "method-unavailable" || failed.kind === "capability-unavailable") {
    return notOk(provenance, "unsupported", reason);
  }
  return notOk(provenance, "unavailable", reason);
}

async function guarded<T>(provenance: Provenance, now: () => number, read: () => Promise<T>): Promise<SurfaceResult<T>> {
  try {
    const data = await read();
    return okResult(provenance, data, observation(now()));
  } catch (error) {
    return failure(provenance, error);
  }
}

function approvalKind(approval: Approval): string {
  const kind = approval.action.kind;
  return kind.type === "delivery" ? kind.kind : kind.type;
}

function projectOf(tasks: readonly TaskRecord[]): Map<string, string> {
  return new Map(tasks.map((task) => [task.contract.id, task.contract.project_id]));
}

export function createDerivedSurfaces(connection: ConnectionManager, now: () => number = Date.now): SurfaceSources {
  const keys = new IdempotencyTracker();

  const wsProvenance = prov("D9", ["projects.list", "tasks.list"]);
  const workstreams: WorkstreamSource = {
    provenance: wsProvenance,
    workstreams: () =>
      guarded(wsProvenance, now, async () => {
        const api = connection.api();
        const [projects, tasks] = await Promise.all([api.projectsList(), api.tasksList()]);
        const ids = [...projects.map((project) => project.id)];
        for (const task of tasks) if (!ids.includes(task.contract.project_id)) ids.push(task.contract.project_id);
        return ids.map<Workstream>((id) => {
          const own = tasks.filter((task) => task.contract.project_id === id);
          return {
            id,
            title: basename(id),
            grouping: "project",
            projectIds: [id],
            tasks: own.map((task) => ({
              id: task.contract.id,
              objective: task.contract.objective,
              state: task.state,
              agent: task.contract.agent,
              dependencies: task.contract.dependencies,
            })),
            counts: countStates(own.map((task) => task.state)),
            objective: unsupportedValue("The daemon records no objective or revision for a project."),
          };
        });
      }),
    simulateCreate: () => Promise.reject(fixtureOnly("workstream creation")),
  };

  const conversationProvenance = prov("D10", ["coordinator.get", "coordinator.send_message"]);
  const conversation: ConversationSource = {
    provenance: conversationProvenance,
    async transcript(target: ConversationTarget) {
      if (target.kind === "global") {
        return notOk(conversationProvenance, "unsupported", "The daemon has a coordinator per project, not a global conversation.");
      }
      if (isFixtureId(target.projectId)) {
        return notOk(conversationProvenance, "unsupported", "Fixture ids have no daemon conversation.");
      }
      return guarded(conversationProvenance, now, async () => {
        const api = connection.api();
        const entries: CoordinatorEntry[] = [];
        let after: number | undefined;
        let more = false;
        for (let page = 0; page < MAX_CONVERSATION_PAGES; page++) {
          const result: CoordinatorPage = await api.coordinatorGet(target.projectId, after);
          entries.push(...result.entries);
          more = result.more;
          if (!more) break;
          after = result.next_after_sequence;
        }
        return {
          target,
          entries: entries.map((entry) => ({
            id: entry.id,
            kind: entry.kind,
            role: entry.author,
            text: entry.body,
            at: entry.created_at,
          })),
          repliesExpected: false,
          truncated: more,
        };
      });
    },
    async send(target, text) {
      if (target.kind === "global") throw new DaemonError("method-unavailable", "The daemon has no global conversation.");
      if (isFixtureId(target.projectId)) {
        throw new DaemonError("invalid-params", "Fixture ids are never sent to the daemon.");
      }
      const scope = `send:${target.projectId}`;
      const key = keys.keyFor(scope, text);
      try {
        const entry = await connection.api().coordinatorSendMessage(target.projectId, text, key);
        keys.settle(scope, null);
        return {
          simulated: false,
          provenance: conversationProvenance,
          notice: "Recorded by the daemon. The coordinator does not reply.",
          at: entry.created_at,
          entryId: entry.id,
        };
      } catch (error) {
        keys.settle(scope, asDaemonError(error));
        throw error;
      }
    },
  };

  const activity = createDerivedActivity(connection, now);

  const usageProvenance = prov("D12", []);
  const usage: UsageSource = {
    provenance: usageProvenance,
    usage: () => Promise.resolve(notOk(usageProvenance, "unsupported", "The daemon records no usage or spend.")),
  };

  const metricsProvenance = prov("D13", []);
  const metrics: SystemMetricsSource = {
    provenance: metricsProvenance,
    metrics: () => Promise.resolve(notOk(metricsProvenance, "unsupported", "The daemon reports no host metrics.")),
  };

  const modelsProvenance = prov("D14", ["agents.capabilities"]);
  const models: ModelCatalogSource = {
    provenance: modelsProvenance,
    catalog: () =>
      guarded(modelsProvenance, now, async () => {
        const entries = await connection.api().agentsCapabilities();
        return {
          providers: entries.map((entry) => ({
            id: entry.agent,
            name: entry.agent,
            installed: entry.installed,
            launchable: entry.launchable,
            modelSelection: entry.capabilities === null ? ("unknown" as const) : entry.capabilities.model_selection ? ("supported" as const) : ("unsupported" as const),
            models: unsupportedValue<{ id: string; label: string }[]>("The daemon lists agents, not their models."),
            efforts: unsupportedValue<string[]>("The daemon reports no effort settings."),
          })),
        };
      }),
  };

  const decisionProvenance = prov("D16", ["approvals.list", "tasks.list"]);
  const decisions: DecisionQueueSource = {
    provenance: decisionProvenance,
    queue: () =>
      guarded(decisionProvenance, now, async () => {
        const api = connection.api();
        const [approvals, tasks] = await Promise.all([api.approvalsList(), api.tasksList()]);
        const projects = projectOf(tasks);
        const reason = "The daemon has no decision for this action; use approve or reject on the review screen.";
        return {
          items: approvals.map<DecisionItem>((approval) => ({
            id: approval.id,
            taskId: approval.action.task_id,
            projectId: projects.get(approval.action.task_id) ?? null,
            kind: approvalKind(approval),
            candidateCommit: approval.action.candidate_commit,
            state: approval.state.state,
            priority: "unknown",
            latestResponseAt: null,
          })),
          actions: {
            postpone: { available: false, reason },
            dismiss: { available: false, reason },
            escalate: { available: false, reason },
          },
        };
      }),
    simulatePostpone: () => Promise.reject(fixtureOnly("postpone")),
    simulateDismiss: () => Promise.reject(fixtureOnly("dismissal")),
  };

  return {
    workstreams,
    conversation,
    activity,
    usage,
    metrics,
    models,
    search: createSearchSource(workstreams, decisions, now),
    decisions,
  };
}
