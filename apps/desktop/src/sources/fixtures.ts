/**
 * Fixture adapters for D1-D6. They read only the JSON under `fixtures/ui/`
 * and never import the daemon client. Every result carries fixture
 * provenance so screens label it PLACEHOLDER DATA.
 */
import agentFixture from "../fixtures/ui/agent-capabilities.json";
import coordinatorFixture from "../fixtures/ui/coordinator.json";
import journalFixture from "../fixtures/ui/journal-identity.json";
import recoveryFixture from "../fixtures/ui/recovery-status.json";
import retryFixture from "../fixtures/ui/task-retry.json";
import executionFixture from "../fixtures/ui/task-execution.json";
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
  PreviewTask,
  Provenance,
  ProposedTask,
  RecoveryStatus,
  RecoveryStatusSource,
  RetryAvailability,
  SimulatedReceipt,
  SourceId,
  SourceResult,
  Sources,
  TaskExecutionSource,
  TaskRef,
  TaskRetrySource,
} from "./types";

export type FixtureScenario = "success" | "empty" | "loading" | "unavailable" | "error";

export interface FixtureOptions {
  scenario?: FixtureScenario;
  now?: () => string;
}

interface Envelope {
  fixture: string;
  scenarios: {
    unavailable: { unavailable: string };
    error: { error: string };
  };
}

function resolveScenario<T>(
  envelope: Envelope,
  scenario: FixtureScenario,
  build: () => SourceResult<T>,
): Promise<SourceResult<T>> {
  switch (scenario) {
    case "loading":
      return new Promise(() => undefined);
    case "empty":
      return Promise.resolve({ state: "empty" });
    case "unavailable":
      return Promise.resolve({ state: "unavailable", reason: envelope.scenarios.unavailable.unavailable });
    case "error":
      return Promise.resolve({ state: "error", message: envelope.scenarios.error.error });
    default:
      return Promise.resolve(build());
  }
}

function provenance(id: SourceId, envelope: Envelope): Provenance {
  return { kind: "fixture", id, fixture: envelope.fixture };
}

function stateOf(ref: TaskRef): string | null {
  return ref.origin === "real" ? ref.state : null;
}

function pick<T>(table: Record<string, T>, key: string | null): T {
  return (key !== null && table[key]) || (table["default"] as T);
}

function receipt(prov: Provenance, now: () => string, notice: string): SimulatedReceipt {
  return { simulated: true, provenance: prov, notice, at: now() };
}

const clock = () => new Date().toISOString();

export function fixtureAgents(options: FixtureOptions = {}): AgentCapabilitiesSource {
  const scenario = options.scenario ?? "success";
  const prov = provenance("D1", agentFixture);
  return {
    provenance: prov,
    listAgents: () =>
      resolveScenario<AgentCapability[]>(agentFixture, scenario, () => ({
        state: "ready",
        data: agentFixture.scenarios.success.data.agents as AgentCapability[],
      })),
  };
}

export function fixtureExecution(options: FixtureOptions = {}): TaskExecutionSource {
  const scenario = options.scenario ?? "success";
  const now = options.now ?? clock;
  const prov = provenance("D2", executionFixture);
  const data = executionFixture.scenarios.success.data;
  const tasks = data.tasks as PreviewTask[];
  return {
    provenance: prov,
    availability: () =>
      resolveScenario<ExecutionAvailability>(executionFixture, scenario, () => ({
        state: "ready",
        data: data.availability,
      })),
    listPreviewTasks: () =>
      resolveScenario<PreviewTask[]>(executionFixture, scenario, () => ({ state: "ready", data: tasks })),
    getPreviewTask: (id) =>
      resolveScenario<PreviewTask>(executionFixture, scenario, () => {
        const found = tasks.find((task) => task.id === id);
        return found ? { state: "ready", data: found } : { state: "empty" };
      }),
    outcomeFor: (ref) =>
      resolveScenario<ExecutionOutcome>(executionFixture, scenario, () => {
        if (ref.origin === "fixture") {
          const found = tasks.find((task) => task.id === ref.id);
          return found ? { state: "ready", data: found.outcome } : { state: "empty" };
        }
        const table = data.outcomeByState as Record<string, ExecutionOutcome>;
        return { state: "ready", data: pick(table, stateOf(ref)) };
      }),
    simulateLaunch: (id) =>
      Promise.resolve(receipt(prov, now, `Simulated launch of ${id}. No agent was started.`)),
    simulateCancel: (id) =>
      Promise.resolve(receipt(prov, now, `Simulated cancel of ${id}. No process was stopped.`)),
  };
}

export function fixtureRecovery(options: FixtureOptions = {}): RecoveryStatusSource {
  const scenario = options.scenario ?? "success";
  const prov = provenance("D3", recoveryFixture);
  const data = recoveryFixture.scenarios.success.data;
  return {
    provenance: prov,
    recoveryFor: (ref) =>
      resolveScenario<RecoveryStatus>(recoveryFixture, scenario, () => {
        const byId = data.byId as Record<string, RecoveryStatus>;
        if (ref.origin === "fixture" && byId[ref.id]) return { state: "ready", data: byId[ref.id] as RecoveryStatus };
        return {
          state: "ready",
          data: pick(data.byState as Record<string, RecoveryStatus>, stateOf(ref)),
        };
      }),
  };
}

export function fixtureRetry(options: FixtureOptions = {}): TaskRetrySource {
  const scenario = options.scenario ?? "success";
  const now = options.now ?? clock;
  const prov = provenance("D4", retryFixture);
  const data = retryFixture.scenarios.success.data;
  return {
    provenance: prov,
    retryFor: (ref) =>
      resolveScenario<RetryAvailability>(retryFixture, scenario, () => ({
        state: "ready",
        data: pick(data.byState as Record<string, RetryAvailability>, stateOf(ref) ?? "Failed"),
      })),
    simulateRetry: (ref) => {
      const id = ref.origin === "real" ? ref.taskId : ref.id;
      return Promise.resolve({
        ...receipt(prov, now, `Simulated retry of ${id}. No new attempt was created.`),
        newAttemptId: "fixture:attempt-simulated",
      });
    },
  };
}

export function fixtureCoordinator(options: FixtureOptions = {}): CoordinatorSource {
  const scenario = options.scenario ?? "success";
  const now = options.now ?? clock;
  const prov = provenance("D5", coordinatorFixture);
  const data = coordinatorFixture.scenarios.success.data;
  return {
    provenance: prov,
    conversation: (projectId) =>
      resolveScenario<CoordinatorConversation>(coordinatorFixture, scenario, () => ({
        state: "ready",
        data: {
          projectId,
          messages: data.messages as CoordinatorMessage[],
          proposals: data.proposals as ProposedTask[],
          outstandingDecisions: data.outstandingDecisions,
          relatedRealIds: [],
        },
      })),
    previewReply: (_projectId, draft) => {
      const at = now();
      return Promise.resolve({
        ...receipt(prov, now, "Simulated reply. Nothing was sent to a coordinator."),
        message: { id: "fixture:msg-simulated", role: "coordinator", text: `Preview reply to: ${draft}`, at },
      });
    },
    previewProposal: (_projectId, draft) =>
      Promise.resolve({
        ...receipt(prov, now, "Simulated proposal. No task was submitted."),
        proposal: {
          id: "fixture:prop-simulated",
          objective: draft,
          dependencies: [],
          validation: draft.trim() ? "valid" : "invalid",
          validationErrors: draft.trim() ? [] : ["objective is empty"],
          suggestedDelivery: "LocalReview",
        },
      }),
  };
}

export function fixtureJournal(options: FixtureOptions = {}): JournalIdentitySource {
  const scenario = options.scenario ?? "success";
  const prov = provenance("D6", journalFixture);
  return {
    provenance: prov,
    identity: () =>
      resolveScenario<JournalIdentity>(journalFixture, scenario, () => ({
        state: "ready",
        data: journalFixture.scenarios.success.data as JournalIdentity,
      })),
  };
}

export function createFixtureSources(options: FixtureOptions = {}): Sources {
  return {
    agents: fixtureAgents(options),
    execution: fixtureExecution(options),
    recovery: fixtureRecovery(options),
    retry: fixtureRetry(options),
    coordinator: fixtureCoordinator(options),
    journal: fixtureJournal(options),
  };
}
