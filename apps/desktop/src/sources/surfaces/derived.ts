/**
 * Daemon-derived D9-D17 adapters. D9 uses workstreams.list/get and tasks.list;
 * D10 retains legacy project notes via coordinator.get and uses conversations.get/send/cancel
 * for Global, project and workstream turns. D11 prefers queue.snapshot with tasks.list and
 * approvals.list. D12 uses usage.summary and the usage.localTools methods; D13 uses host.metrics; D14 uses
 * agents.capabilities/catalog/validate_selection; D16 joins approvals.list and tasks.list and adds actions.pending;
 * D17 uses settings.get and settings.set. D15 indexes D9 and D16 locally. No daemon method is added here.
 *
 * Failed reads retain daemon provenance and never fall back to fixtures. Missing
 * methods are unsupported; other failures are unavailable. Unobserved metadata
 * stays unknown or explicitly nullable. Simulated actions reject with DaemonError.
 * Writes reject on failure and never return simulated receipts.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { isFixtureLikeId } from "./fixtureIds";
import { DaemonError, asDaemonError, isMissingFeature, presentError } from "../../daemon/errors";
import { IdempotencyTracker } from "../../daemon/idempotency";
import type { ActionsPendingWire, Approval, CoordinatorEntry, CoordinatorPage, TaskRecord } from "../../daemon/types";
import { createDerivedActivity } from "./activity";
import { mapProposal } from "./actionProposals";
import {
  createLiveConversation,
  HistoryChanged,
  LIVE_METHODS,
  restartOnHistoryChange,
} from "./conversationLive";
import { createLiveMetrics } from "./liveMetrics";
import { createLiveModels, MODELS_METHODS } from "./modelCatalogLive";
import { notOk, observation, okResult } from "./result";
import { createSearchSource } from "./search";
import { createLiveMonitoring, MONITORING_METHODS } from "./monitoringLive";
import { createLiveGrants, GRANTS_METHODS } from "./grants";
import { createLiveNotes, NOTES_METHODS } from "./notes";
import { createLivePulls, PULLS_METHODS } from "./pulls";
import { createLiveHolds, HOLDS_METHODS } from "./holds";
import { createLiveQuestions, QUESTIONS_METHODS } from "./questions";
import { createLiveProjectDefaults, PROJECT_DEFAULTS_METHODS } from "./projectDefaults";
import { createLiveSettings, SETTINGS_METHODS } from "./settingsLive";
import { createLiveLocalToolsSource } from "./localToolsLive";
import { createLiveUsageSource } from "./usageLive";
import { createLiveWorkstreams } from "./workstreamsLive";
import type {
  ClearReceipt,
  ConversationSource,
  ConversationTarget,
  CoordinatorChangeReceipt,
  DecisionItem,
  DecisionQueueSource,
  ModelCatalogSource,
  Provenance,
  ProposalItem,
  SurfaceId,
  SurfaceResult,
  SurfaceSources,
  SystemMetricsSource,
  LocalToolsSource,
  UsageSource,
  WorkstreamSource,
} from "./types";

const MAX_CONVERSATION_PAGES = 50;

const prov = (id: SurfaceId, methods: readonly string[]): Provenance => ({ kind: "daemon", id, methods });

function fixtureOnly(what: string): DaemonError {
  return new DaemonError("method-unavailable", `Simulated ${what} exists only for fixture data.`);
}

/** Maps a failed live call. Retry may help for `unavailable`; it will not for `unsupported`. */
function failure<T>(provenance: Provenance, error: unknown, now: () => number): SurfaceResult<T> {
  const failed = asDaemonError(error);
  const reason = `${presentError(failed).reason} (${failed.kind}: ${failed.message})`;
  return notOk(provenance, isMissingFeature(failed) ? "unsupported" : "unavailable", reason, observation(now()));
}

async function guarded<T>(provenance: Provenance, now: () => number, read: () => Promise<T>): Promise<SurfaceResult<T>> {
  try {
    const data = await read();
    return okResult(provenance, data, observation(now()));
  } catch (error) {
    return failure(provenance, error, now);
  }
}

function approvalKind(approval: Approval): string {
  const kind = approval.action.kind;
  return kind.type === "delivery" ? kind.kind : kind.type;
}

function projectOf(tasks: readonly TaskRecord[]): Map<string, string> {
  return new Map(tasks.map((task) => [task.contract.id, task.contract.project_id]));
}

/** A pending proposal and the conversation that holds its card, from `actions.pending`. */
function proposalItemOf(wire: ActionsPendingWire["proposals"][number]): ProposalItem {
  return {
    proposal: mapProposal(wire),
    scope: wire.scope.kind === "global" ? { kind: "global" } : { kind: "workstream", workstreamId: wire.scope.workstream_id },
  };
}

export function createDerivedSurfaces(connection: ConnectionManager, now: () => number = Date.now): SurfaceSources {
  const keys = new IdempotencyTracker();

  const wsProvenance = prov("D9", ["workstreams.list", "workstreams.get", "tasks.list"]);
  const workstreams: WorkstreamSource = {
    provenance: wsProvenance,
    ...createLiveWorkstreams(connection, now, keys, guarded, wsProvenance),
  };

  const conversationProvenance = prov("D10", ["coordinator.get", ...LIVE_METHODS]);
  const projectProvenance = prov("D10", [
    "coordinator.get",
    "coordinator.clear",
    "coordinator.remove",
    "coordinator.restore",
  ]);
  const liveConversation = createLiveConversation(connection, prov("D10", LIVE_METHODS), keys, now);
  const conversation: ConversationSource = {
    provenance: conversationProvenance,
    async transcript(target: ConversationTarget) {
      if (target.kind !== "project") return liveConversation.transcript(target);
      if (isFixtureLikeId(target.projectId)) {
        return notOk(projectProvenance, "unsupported", "Fixture ids have no daemon conversation.", observation(now()));
      }
      // A clear between pages starts the read over, so the notes shown never mix two logs.
      const readNotes = () => guarded(projectProvenance, now, () =>
        restartOnHistoryChange(async () => {
          const api = connection.api();
          const entries: CoordinatorEntry[] = [];
          let after: number | undefined;
          let more = false;
          let generation: number | undefined;
          for (let page = 0; page < MAX_CONVERSATION_PAGES; page++) {
            const result: CoordinatorPage = await api.coordinatorGet(target.projectId, after);
            generation ??= result.clear_generation;
            if (result.clear_generation !== generation) throw new HistoryChanged();
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
            clearGeneration: generation ?? 0,
            truncated: more,
            proposalsMore: false,
          };
        }),
      );
      if (!connection.getSnapshot().capabilities.includes("project_conversations")) return readNotes();
      try {
        return await restartOnHistoryChange(async () => {
          const notes = await readNotes();
          if (notes.status !== "ok") return notes;
          const replies = await liveConversation.transcript(target);
          if (replies.status !== "ok") return replies;
          if (notes.data.clearGeneration !== replies.data.clearGeneration) throw new HistoryChanged();
          return {
            ...replies,
            data: {
              ...replies.data,
              entries: [...notes.data.entries, ...replies.data.entries],
              truncated: notes.data.truncated || replies.data.truncated,
            },
          };
        });
      } catch (error) {
        return failure(conversationProvenance, error, now);
      }
    },
    send: (target, text, selection, permission) => liveConversation.send(target, text, selection, permission),
    cancel: (target, turnId) => liveConversation.cancel(target, turnId),
    resolve: (proposalId, decision, digest) => liveConversation.resolve(proposalId, decision, digest),
    async clear(target: ConversationTarget): Promise<ClearReceipt> {
      if (target.kind !== "project") return liveConversation.clear(target);
      if (isFixtureLikeId(target.projectId)) {
        throw new DaemonError("invalid-params", "Fixture ids are never sent to the daemon.");
      }
      const scope = `clear:${target.projectId}`;
      const key = keys.keyFor(scope, "clear");
      try {
        const result = await connection.api().coordinatorClear(target.projectId, key);
        keys.settle(scope, null);
        return {
          simulated: false,
          provenance: projectProvenance,
          cleared: result.cleared,
          notice: result.cleared
            ? "Coordinator history cleared. The deleted messages and replies cannot be recovered."
            : "There was no coordinator history to clear.",
        };
      } catch (error) {
        keys.settle(scope, asDaemonError(error));
        throw error;
      }
    },
    removeCoordinator: (projectId: string) => changeCoordinator(projectId, "remove"),
    restoreCoordinator: (projectId: string) => changeCoordinator(projectId, "restore"),
  };

  /** Removes or restores a project coordinator (DPES-1105). The project itself is never changed. */
  async function changeCoordinator(projectId: string, action: "remove" | "restore"): Promise<CoordinatorChangeReceipt> {
    if (isFixtureLikeId(projectId)) throw new DaemonError("invalid-params", "Fixture ids are never sent to the daemon.");
    const scope = `${action}:${projectId}`;
    const key = keys.keyFor(scope, action);
    try {
      const api = connection.api();
      const result =
        action === "remove"
          ? await api.coordinatorRemove(projectId, key)
          : await api.coordinatorRestore(projectId, key);
      keys.settle(scope, null);
      const removed = action === "remove";
      return {
        simulated: false,
        provenance: projectProvenance,
        changed: result.changed,
        removed,
        notice: removed
          ? result.changed
            ? "Coordinator removed. Its conversation was deleted; the project and its tasks are unchanged."
            : "That coordinator was already removed."
          : result.changed
            ? "Coordinator added. It starts with no conversation history."
            : "That coordinator is already active.",
      };
    } catch (error) {
      keys.settle(scope, asDaemonError(error));
      throw error;
    }
  }

  const activity = createDerivedActivity(connection, now);

  const usage: UsageSource = createLiveUsageSource(connection, now);

  const localTools: LocalToolsSource = createLiveLocalToolsSource(connection, now);

  const metrics: SystemMetricsSource = createLiveMetrics(connection, { now });

  const modelsProvenance = prov("D14", MODELS_METHODS);
  const models: ModelCatalogSource = createLiveModels(connection, modelsProvenance, now);

  const decisionProvenance = prov("D16", ["approvals.list", "tasks.list", "actions.pending"]);
  const decisions: DecisionQueueSource = {
    provenance: decisionProvenance,
    queue: () =>
      guarded(decisionProvenance, now, async () => {
        const api = connection.api();
        // `actionsPending` answers an older daemon with an empty list, so the queue never fails for the missing method.
        const [approvals, tasks, pending] = await Promise.all([api.approvalsList(), api.tasksList(), api.actionsPending()]);
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
          proposals: pending.proposals.map<ProposalItem>(proposalItemOf),
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

  const settings = createLiveSettings(connection, prov("D17", SETTINGS_METHODS), keys, now);
  // Monitoring belongs to the conversations, so it carries their id (D10) rather than a new surface id.
  const monitoring = createLiveMonitoring(connection, prov("D10", MONITORING_METHODS), keys, now);
  // Holds hide decisions, so they carry the decision queue's id (D16).
  const holds = createLiveHolds(connection, prov("D16", HOLDS_METHODS), keys, now);
  const questions = createLiveQuestions(connection, prov("D16", QUESTIONS_METHODS), keys, now);
  const pulls = createLivePulls(connection, prov("D16", PULLS_METHODS), now);
  const projectDefaults = createLiveProjectDefaults(connection, prov("D17", PROJECT_DEFAULTS_METHODS), keys, now);
  const grants = createLiveGrants(connection, prov("D17", GRANTS_METHODS), keys, now);
  const notes = createLiveNotes(connection, prov("D17", NOTES_METHODS), keys, now);

  return {
    workstreams,
    conversation,
    activity,
    usage,
    localTools,
    metrics,
    models,
    search: createSearchSource(workstreams, decisions, now),
    decisions,
    settings,
    monitoring,
    holds,
    questions,
    pulls,
    projectDefaults,
    grants,
    notes,
  };
}
