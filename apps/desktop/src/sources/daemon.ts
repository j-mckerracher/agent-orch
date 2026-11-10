/**
 * Daemon-backed adapters for D1, D2, D4, D5 and D6 (M5-02b, M5-02c).
 *
 * D3 (recovery status) has no daemon RPC. It stays a labelled placeholder that
 * reports "not available" for every task: it never serves fixture recovery
 * facts for real tasks. Every other result here
 * carries `provenance.kind === "daemon"`; a disconnected or un-negotiated
 * capability is `unavailable`, never an empty list. Nothing in this file mints
 * a `fixture:` id or fabricates data.
 */
import type { ConnectionManager } from "../daemon/connection";
import { DaemonError, asDaemonError, presentError } from "../daemon/errors";
import { IdempotencyTracker } from "../daemon/idempotency";
import type {
  AgentCapabilityEntry,
  CoordinatorEntry,
  CoordinatorPage,
  TaskRecord,
} from "../daemon/types";
import type {
  AgentCapabilitiesSource,
  AgentCapability,
  CoordinatorConversation,
  CoordinatorMessage,
  CoordinatorSource,
  ExecutionAvailability,
  ExecutionOutcome,
  JournalIdentity,
  JournalIdentitySource,
  ProposedTask,
  Provenance,
  RecoveryStatusSource,
  RetryAvailability,
  SourceId,
  SourceResult,
  Sources,
  TaskExecutionSource,
  TaskRef,
  TaskRetrySource,
} from "./types";

const AGENT_CACHE_MS = 5_000;
const MAX_CONVERSATION_PAGES = 50;
const RETRYABLE = new Set(["Failed", "VerificationFailed"]);

const prov = (id: SourceId): Provenance => ({ kind: "daemon", id });

function unavailable<T>(error: unknown): SourceResult<T> {
  const failure = asDaemonError(error);
  if (
    failure.kind === "disconnected" ||
    failure.kind === "capability-not-negotiated" ||
    failure.kind === "capability-unavailable" ||
    failure.kind === "method-unavailable"
  ) {
    return { state: "unavailable", reason: presentError(failure).reason };
  }
  return { state: "error", message: `${presentError(failure).reason} (${failure.kind}: ${failure.message})` };
}

async function guarded<T>(load: () => Promise<SourceResult<T>>): Promise<SourceResult<T>> {
  try {
    return await load();
  } catch (error) {
    return unavailable<T>(error);
  }
}

const FEATURE_LABELS: Array<[keyof NonNullable<AgentCapabilityEntry["capabilities"]>, string]> = [
  ["streaming", "streaming"],
  ["interactive_input", "interactive input"],
  ["structured_output", "structured output"],
  ["cancellation", "cancellation"],
  ["session_resumption", "session resumption"],
  ["approvals", "approvals"],
  ["model_selection", "model selection"],
];

export function toAgentCapability(entry: AgentCapabilityEntry, discoveredAt: string): AgentCapability {
  const features = entry.capabilities
    ? FEATURE_LABELS.filter(([key]) => entry.capabilities?.[key]).map(([, label]) => label)
    : [];
  if (!entry.launchable) {
    return {
      id: entry.agent,
      name: entry.agent,
      availability: "unsupported",
      features,
      discoveredAt,
      note: entry.installed
        ? "Detected, but the daemon cannot launch this agent yet."
        : "Not detected, and the daemon cannot launch this agent yet.",
    };
  }
  return {
    id: entry.agent,
    name: entry.agent,
    availability: entry.installed ? "installed" : "available",
    features,
    discoveredAt,
    note: entry.installed ? undefined : "Launchable by the daemon but not detected on this machine.",
  };
}

function latest<T extends { id: string }>(items: T[]): T | null {
  return items.length > 0 ? (items[items.length - 1] ?? null) : null;
}

export function createDaemonSources(connection: ConnectionManager, now: () => number = Date.now): Sources {
  const keys = new IdempotencyTracker();
  let agentCache: { generation: number; at: number; load: Promise<AgentCapabilityEntry[]> } | null = null;

  const capabilities = (): Promise<AgentCapabilityEntry[]> => {
    const generation = connection.getSnapshot().generation;
    if (agentCache && agentCache.generation === generation && now() - agentCache.at < AGENT_CACHE_MS) {
      return agentCache.load;
    }
    const api = connection.api();
    const load = api.agentsCapabilities();
    const entry = { generation, at: now(), load };
    agentCache = entry;
    load.catch(() => {
      if (agentCache === entry) agentCache = null;
    });
    return load;
  };

  const agents: AgentCapabilitiesSource = {
    provenance: prov("D1"),
    listAgents: () =>
      guarded<AgentCapability[]>(async () => {
        const entries = await capabilities();
        const at = new Date(now()).toISOString();
        return entries.length === 0
          ? { state: "empty" }
          : { state: "ready", data: entries.map((entry) => toAgentCapability(entry, at)) };
      }),
  };

  const realTask = async (ref: TaskRef): Promise<TaskRecord | null> =>
    ref.origin === "real" ? connection.api().tasksGet(ref.taskId) : null;

  const execution: TaskExecutionSource = {
    provenance: prov("D2"),
    availability: () =>
      guarded<ExecutionAvailability>(async () => {
        const entries = await capabilities();
        const launchable = entries.filter((entry) => entry.launchable && entry.installed);
        return {
          state: "ready",
          data:
            launchable.length > 0
              ? {
                  launchAvailable: true,
                  reason: `Launchable agents detected: ${launchable.map((entry) => entry.agent).join(", ")}.`,
                }
              : {
                  launchAvailable: false,
                  reason: "No agent the daemon can launch was detected on this machine.",
                },
        };
      }),
    listPreviewTasks: () => Promise.resolve({ state: "empty" }),
    getPreviewTask: () =>
      Promise.resolve({ state: "unavailable", reason: "Preview tasks exist only in fixture mode." }),
    outcomeFor: (ref) =>
      guarded<ExecutionOutcome>(async () => {
        const record = await realTask(ref);
        if (!record) return { state: "unavailable", reason: "Fixture preview tasks have no daemon record." };
        const attempt = latest(record.attempts);
        const session = latest(record.sessions);
        return {
          state: "ready",
          data: {
            state: record.state,
            attemptId: attempt?.id ?? null,
            sessionId: session?.id ?? null,
            launchOutcome: session
              ? `a session is recorded (${session.state})`
              : "no session is recorded, so no agent launch is confirmed",
            cancelOutcome:
              record.state === "Cancelled"
                ? `the daemon records the task as ${record.state}; this is not proof a process stopped`
                : null,
          },
        };
      }),
    simulateLaunch: () => Promise.reject(fixtureOnly("launch")),
    simulateCancel: () => Promise.reject(fixtureOnly("cancel")),
  };

  const retry: TaskRetrySource = {
    provenance: prov("D4"),
    retryFor: (ref) =>
      guarded<RetryAvailability>(async () => {
        const record = await realTask(ref);
        if (!record) return { state: "unavailable", reason: "Fixture preview tasks have no daemon record." };
        const previous = latest(record.attempts);
        const available = RETRYABLE.has(record.state);
        return {
          state: "ready",
          data: {
            available,
            reason: available
              ? `The task is ${record.state}; the daemon allows a retry within max_retries (${record.contract.max_retries}) or max_repairs (${record.contract.max_repairs}) and refuses it otherwise.`
              : `The task is ${record.state}; only Failed or VerificationFailed tasks can be retried.`,
            previousAttemptId: previous?.id ?? null,
          },
        };
      }),
    simulateRetry: () => Promise.reject(fixtureOnly("retry")),
    async retry(ref) {
      if (ref.origin !== "real") throw fixtureOnly("retry");
      const scope = `retry:${ref.taskId}`;
      const key = keys.keyFor(scope, ref.taskId);
      try {
        const record = await connection.api().tasksRetry(ref.taskId, key);
        keys.settle(scope, null);
        return { state: record.state, attempts: record.attempts.length };
      } catch (error) {
        keys.settle(scope, asDaemonError(error));
        throw error;
      }
    },
  };

  const coordinator: CoordinatorSource = {
    provenance: prov("D5"),
    conversation: (projectId) =>
      guarded<CoordinatorConversation>(async () => {
        const api = connection.api();
        const entries: CoordinatorEntry[] = [];
        let after: number | undefined;
        for (let page = 0; page < MAX_CONVERSATION_PAGES; page++) {
          const result: CoordinatorPage = await api.coordinatorGet(projectId, after);
          entries.push(...result.entries);
          if (!result.more) break;
          after = result.next_after_sequence;
        }
        if (entries.length === 0) return { state: "empty" };
        return {
          state: "ready",
          data: {
            projectId,
            messages: entries.filter((e) => e.kind === "message").map(toMessage),
            proposals: entries.filter((e) => e.kind === "proposal").flatMap(toProposals),
            outstandingDecisions: [],
            relatedRealIds: [],
          },
        };
      }),
    previewReply: () => Promise.reject(fixtureOnly("reply")),
    previewProposal: () => Promise.reject(fixtureOnly("proposal")),
    async send(projectId, text) {
      const scope = `send:${projectId}`;
      const key = keys.keyFor(scope, text);
      try {
        await connection.api().coordinatorSendMessage(projectId, text, key);
        keys.settle(scope, null);
      } catch (error) {
        keys.settle(scope, asDaemonError(error));
        throw error;
      }
    },
    async propose(projectId, proposalJson) {
      const scope = `propose:${projectId}`;
      const key = keys.keyFor(scope, proposalJson);
      try {
        await connection.api().coordinatorPropose(projectId, proposalJson, key);
        keys.settle(scope, null);
      } catch (error) {
        keys.settle(scope, asDaemonError(error));
        throw error;
      }
    },
  };

  const journal: JournalIdentitySource = {
    provenance: prov("D6"),
    identity: () =>
      guarded<JournalIdentity>(async () => {
        const snapshot = connection.getSnapshot();
        if (snapshot.status !== "connected") {
          return { state: "unavailable", reason: "Not connected to the daemon." };
        }
        if (snapshot.journalEpoch === null) {
          return { state: "unavailable", reason: "The daemon did not report a journal epoch." };
        }
        const reset =
          snapshot.history.reason === "epoch-mismatch" || snapshot.history.reason === "replay-gap";
        return {
          state: "ready",
          data: {
            storeIdentity: snapshot.journalEpoch,
            cursorRelationship: reset ? "reset" : snapshot.resumedFrom !== null ? "same" : "unknown",
          },
        };
      }),
  };

  const recovery: RecoveryStatusSource = {
    provenance: { kind: "fixture", id: "D3", fixture: "recovery-unavailable" },
    recoveryFor: () =>
      Promise.resolve({
        state: "unavailable",
        reason:
          "the daemon exposes no recovery status yet. This task's process, workspace and delivery state are unknown; do not read the absence of a warning as safe.",
      }),
  };

  return { agents, execution, recovery, retry, coordinator, journal };
}

function fixtureOnly(what: string): DaemonError {
  return new DaemonError("method-unavailable", `Simulated ${what} exists only for fixture data.`);
}

function toMessage(entry: CoordinatorEntry): CoordinatorMessage {
  return { id: entry.id, role: entry.author, text: entry.body, at: entry.created_at };
}

function toProposals(entry: CoordinatorEntry): ProposedTask[] {
  const validation = entry.validation;
  if (validation?.status === "valid") {
    return validation.tasks.map((task) => ({
      id: `${entry.id}:${task.id}`,
      objective: task.title,
      dependencies: task.dependencies,
      validation: "valid",
      validationErrors: [],
      suggestedDelivery: String(task.contract.delivery ?? "not specified"),
    }));
  }
  return [
    {
      id: entry.id,
      objective: entry.body.length > 80 ? `${entry.body.slice(0, 77)}...` : entry.body,
      dependencies: [],
      validation: "invalid",
      validationErrors: validation?.status === "invalid" ? validation.errors.map((e) => e.message) : ["not validated"],
      suggestedDelivery: "not specified",
    },
  ];
}
