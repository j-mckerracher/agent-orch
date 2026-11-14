/**
 * Typed wrapper over the accepted M5-02 methods used by M5-03.
 *
 * Mutation helpers refuse fixture-like ids, including historical colon-less ids, so a
 * placeholder id can never reach a real daemon mutation even if a caller wires
 * a fixture record into a real action by mistake.
 */
import { DaemonError } from "./errors";
import { parseQueueSnapshot, type QueueSnapshotWire } from "./queue";
import type { DaemonSession } from "./session";
import type {
  AgentCapabilityEntry,
  AgentCatalogEntry,
  Approval,
  ConversationCancelResult,
  ConversationSendResult,
  ConversationWirePage,
  ConversationWireTarget,
  ModelSelectionWire,
  SelectionCheck,
  CoordinatorEntry,
  CoordinatorPage,
  DaemonHealth,
  DeliveryRequest,
  DeliveryResult,
  ProjectRow,
  ResolveDecision,
  ReviewApprovalRequest,
  ReviewApprovalResult,
  ReviewChangesResult,
  ReviewEvidenceWire,
  SessionEntry,
  SessionEventsPage,
  TaskRecord,
  TaskSubmission,
  WorkspaceDiff,
  WorkspaceDiffRequest,
  GateDraft,
  ObjectiveWire,
  WorkstreamDetailWire,
  WorkstreamReviseObjectiveRequest,
  WorkstreamSetGateRequest,
  WorkstreamStateWire,
  WorkstreamTaskLinkWire,
  WorkstreamUpdateRequest,
  WorkstreamWire,
} from "./types";
import type { HostMetricsWire } from "./hostMetrics";
import type { UsageSummaryRequest } from "./usageWire";

export const FIXTURE_ID_PREFIX = "fixture:";

export function isFixtureId(id: string): boolean {
  return id.trim().toLowerCase().startsWith("fixture");
}

/** Guard failures reject asynchronously, like every other API failure. */
function realId(id: string): string {
  if (isFixtureId(id)) {
    throw new DaemonError("invalid-params", "placeholder ids cannot be sent to the daemon");
  }
  return id;
}

/**
 * Check ID-bearing fields before forwarding spread contracts or proposal JSON.
 * Walk nested records and ID arrays, including unknown extension fields, so a spread
 * cannot bypass the guard. Free text and verification commands stay unchanged.
 * Schema validation and authorization remain the daemon's responsibility.
 */
function realIdFields(value: unknown, idField = false): void {
  if (typeof value === "string") {
    if (idField) realId(value);
  } else if (Array.isArray(value)) {
    for (const item of value) realIdFields(item, idField);
  } else if (value !== null && typeof value === "object") {
    for (const [field, item] of Object.entries(value)) {
      const isId = /^(id|ids|dependencies|agent|model|effort|candidate_commit|candidateCommit)$/.test(field)
        || /(_ids?$|[a-z]Ids?$)/.test(field);
      realIdFields(item, isId);
    }
  }
}

export class DaemonApi {
  constructor(private readonly session: DaemonSession) {}

  health(): Promise<DaemonHealth> {
    return this.session.request("health");
  }

  projectsList(): Promise<ProjectRow[]> {
    return this.session.request("projects.list");
  }

  projectsRegister(path: string): Promise<ProjectRow> {
    return this.session.request("projects.register", { path });
  }

  tasksList(): Promise<TaskRecord[]> {
    return this.session.request("tasks.list");
  }

  async tasksGet(taskId: string): Promise<TaskRecord> {
    return this.session.request("tasks.get", { task_id: realId(taskId) });
  }

  /** `tasks.submit` queues work; it does not prove an agent launched. */
  async tasksSubmit(idempotencyKey: string, contract: TaskSubmission): Promise<TaskRecord> {
    realIdFields(contract);
    return this.session.request("tasks.submit", {
      idempotency_key: idempotencyKey,
      contract: {
        ...contract,
        project_id: realId(contract.project_id),
        parent_id: contract.parent_id ? realId(contract.parent_id) : null,
        dependencies: contract.dependencies.map(realId),
      },
    });
  }

  /** The daemon acknowledges the request; process termination is separate. */
  async tasksCancel(taskId: string, idempotencyKey: string): Promise<TaskRecord> {
    return this.session.request("tasks.cancel", {
      task_id: realId(taskId),
      idempotency_key: idempotencyKey,
    });
  }

  async sessionsList(taskId: string | null): Promise<SessionEntry[]> {
    return this.session.request("sessions.list", { task_id: taskId === null ? null : realId(taskId) });
  }

  async sessionsGet(taskId: string, sessionId: string): Promise<SessionEntry> {
    return this.session.request("sessions.get", {
      task_id: realId(taskId),
      session_id: realId(sessionId),
    });
  }

  async sessionsEvents(taskId: string, sessionId: string, afterSequence?: number): Promise<SessionEventsPage> {
    return this.session.request("sessions.events", {
      task_id: realId(taskId),
      session_id: realId(sessionId),
      ...(afterSequence === undefined ? {} : { after_sequence: afterSequence }),
    });
  }

  approvalsList(): Promise<Approval[]> {
    return this.session.request("approvals.list");
  }

  /** Only approve/reject exist; there is no request-changes decision. */
  async approvalsResolve(approvalId: string, decision: ResolveDecision, idempotencyKey: string): Promise<Approval> {
    return this.session.request("approvals.resolve", {
      approval_id: realId(approvalId),
      decision,
      idempotency_key: idempotencyKey,
    });
  }

  async workspacesDiff(request: WorkspaceDiffRequest): Promise<WorkspaceDiff> {
    return this.session.request("workspaces.diff", {
      ...request,
      task_id: realId(request.task_id),
      ...(request.workspace_id === undefined ? {} : { workspace_id: realId(request.workspace_id) }),
    });
  }

  /** Approval alone never delivers; this is the separate delivery request. */
  async deliveryRequest(request: DeliveryRequest, idempotencyKey: string): Promise<DeliveryResult> {
    return this.session.request("delivery.request", {
      ...request,
      task_id: realId(request.task_id),
      approval_id: realId(request.approval_id),
      verification_run_id: realId(request.verification_run_id),
      ...(request.workspace_id === undefined ? {} : { workspace_id: realId(request.workspace_id) }),
      idempotency_key: idempotencyKey,
    });
  }

  /** Read-only scheduler capacity and queue view (`queue` capability); takes no parameters. */
  async queueSnapshot(): Promise<QueueSnapshotWire> {
    return parseQueueSnapshot(await this.session.request("queue.snapshot"));
  }

  /** Legal only for a Failed or VerificationFailed task within its retry or repair budget. */
  async tasksRetry(taskId: string, idempotencyKey: string): Promise<TaskRecord> {
    return this.session.request("tasks.retry", {
      task_id: realId(taskId),
      idempotency_key: idempotencyKey,
    });
  }

  agentsCapabilities(): Promise<AgentCapabilityEntry[]> {
    return this.session.request("agents.capabilities");
  }

  /** Cached host meters (capability `host`); the daemon never probes on the call. */
  hostMetrics(): Promise<HostMetricsWire> {
    return this.session.request("host.metrics");
  }

  /** The result is untyped wire JSON; the usage adapter validates it. */
  usageSummary(request: UsageSummaryRequest): Promise<unknown> {
    return this.session.request("usage.summary", request);
  }

  /** Records a user message; the daemon never acts on it. */
  async coordinatorSendMessage(projectId: string, text: string, idempotencyKey: string): Promise<CoordinatorEntry> {
    return this.session.request("coordinator.send_message", {
      project_id: realId(projectId),
      text,
      idempotency_key: idempotencyKey,
    });
  }

  /** Validates proposal JSON without creating tasks or approvals; malformed JSON and fixture IDs reject locally. */
  async coordinatorPropose(projectId: string, proposal: string, idempotencyKey: string): Promise<CoordinatorEntry> {
    let payload: unknown;
    try {
      payload = JSON.parse(proposal);
    } catch {
      throw new DaemonError("invalid-params", "The coordinator proposal must be valid JSON.");
    }
    realIdFields(payload);
    return this.session.request("coordinator.propose", {
      project_id: realId(projectId),
      proposal,
      idempotency_key: idempotencyKey,
    });
  }

  async coordinatorGet(projectId: string, afterSequence?: number): Promise<CoordinatorPage> {
    return this.session.request("coordinator.get", {
      project_id: realId(projectId),
      ...(afterSequence === undefined ? {} : { after_sequence: afterSequence }),
    });
  }

  async reviewRequestApproval(request: ReviewApprovalRequest, idempotencyKey: string): Promise<ReviewApprovalResult> {
    return this.session.request("review.request_approval", {
      ...request,
      task_id: realId(request.task_id),
      verification_run_id: realId(request.verification_run_id),
      ...(request.workspace_id === undefined ? {} : { workspace_id: realId(request.workspace_id) }),
      idempotency_key: idempotencyKey,
    });
  }

  async reviewEvidence(taskId: string, approvalId?: string, path?: string): Promise<ReviewEvidenceWire> {
    return this.session.request("review.evidence", {
      task_id: realId(taskId),
      ...(approvalId === undefined ? {} : { approval_id: realId(approvalId) }),
      ...(path === undefined ? {} : { path }),
    });
  }

  async reviewRequestChanges(
    approvalId: string,
    candidateCommit: string,
    feedback: string,
    idempotencyKey: string,
  ): Promise<ReviewChangesResult> {
    return this.session.request("review.request_changes", {
      approval_id: realId(approvalId),
      candidate_commit: realId(candidateCommit),
      feedback,
      idempotency_key: idempotencyKey,
    });
  }

  workstreamsList(state?: WorkstreamStateWire): Promise<WorkstreamWire[]> {
    return this.session.request("workstreams.list", state === undefined ? {} : { state });
  }

  async workstreamsGet(workstreamId: string): Promise<WorkstreamDetailWire> {
    return this.session.request("workstreams.get", { workstream_id: realId(workstreamId) });
  }

  workstreamsCreate(title: string, summary: string | undefined, idempotencyKey: string): Promise<WorkstreamWire> {
    return this.session.request("workstreams.create", {
      title,
      ...(summary === undefined ? {} : { summary }),
      idempotency_key: idempotencyKey,
    });
  }

  async workstreamsUpdate(request: WorkstreamUpdateRequest, idempotencyKey: string): Promise<WorkstreamWire> {
    return this.session.request("workstreams.update", {
      ...request,
      workstream_id: realId(request.workstream_id),
      idempotency_key: idempotencyKey,
    });
  }

  async workstreamsAddObjective(
    workstreamId: string,
    objective: { title: string; description?: string; gates?: GateDraft[] },
    idempotencyKey: string,
  ): Promise<ObjectiveWire> {
    return this.session.request("workstreams.add_objective", {
      ...objective,
      workstream_id: realId(workstreamId),
      idempotency_key: idempotencyKey,
    });
  }

  async workstreamsReviseObjective(request: WorkstreamReviseObjectiveRequest, idempotencyKey: string): Promise<ObjectiveWire> {
    return this.session.request("workstreams.revise_objective", {
      ...request,
      objective_id: realId(request.objective_id),
      idempotency_key: idempotencyKey,
    });
  }

  async workstreamsSetGate(request: WorkstreamSetGateRequest, idempotencyKey: string): Promise<ObjectiveWire> {
    return this.session.request("workstreams.set_gate", {
      ...request,
      objective_id: realId(request.objective_id),
      gate_id: realId(request.gate_id),
      ...(request.evidence_task_id === undefined ? {} : { evidence_task_id: realId(request.evidence_task_id) }),
      idempotency_key: idempotencyKey,
    });
  }

  async workstreamsLinkTask(
    workstreamId: string,
    taskId: string,
    objectiveId: string | undefined,
    idempotencyKey: string,
  ): Promise<WorkstreamTaskLinkWire> {
    return this.session.request("workstreams.link_task", {
      workstream_id: realId(workstreamId),
      task_id: realId(taskId),
      ...(objectiveId === undefined ? {} : { objective_id: realId(objectiveId) }),
      idempotency_key: idempotencyKey,
    });
  }

  async workstreamsUnlinkTask(workstreamId: string, taskId: string, idempotencyKey: string): Promise<{ removed: boolean }> {
    return this.session.request("workstreams.unlink_task", {
      workstream_id: realId(workstreamId),
      task_id: realId(taskId),
      idempotency_key: idempotencyKey,
    });
  }

  /** M7-08. Records the message and starts a turn; the turn can still fail (e.g. `runtime_unavailable`). */
  async conversationsSend(
    target: ConversationWireTarget,
    text: string,
    idempotencyKey: string,
  ): Promise<ConversationSendResult> {
    return this.session.request("conversations.send", {
      target: realTarget(target),
      text,
      idempotency_key: idempotencyKey,
    });
  }

  /** Cursor read; there is no push stream. Omit `afterSequence` to read from the start. */
  async conversationsGet(
    target: ConversationWireTarget,
    afterSequence?: number,
    epoch?: string,
  ): Promise<ConversationWirePage> {
    return this.session.request("conversations.get", {
      target: realTarget(target),
      ...(afterSequence === undefined ? {} : { after_sequence: afterSequence }),
      ...(epoch === undefined ? {} : { epoch }),
    });
  }

  async conversationsCancel(
    target: ConversationWireTarget,
    turnId: string,
    idempotencyKey: string,
  ): Promise<ConversationCancelResult> {
    return this.session.request("conversations.cancel", {
      target: realTarget(target),
      turn_id: realId(turnId),
      idempotency_key: idempotencyKey,
    });
  }

  /** M7-09. Per-model efforts; a cold catalog answers state `unknown` ("retry shortly"). */
  agentsCatalog(): Promise<AgentCatalogEntry[]> {
    return this.session.request("agents.catalog");
  }

  /** At least one of model and effort is required by the daemon. */
  async agentsValidateSelection(agent: string, selection: ModelSelectionWire): Promise<SelectionCheck> {
    return this.session.request("agents.validate_selection", {
      agent: realId(agent),
      ...(selection.model === undefined ? {} : { model: selection.model }),
      ...(selection.effort === undefined ? {} : { effort: selection.effort }),
    });
  }
}

function realTarget(target: ConversationWireTarget): ConversationWireTarget {
  return target.kind === "global" ? target : { kind: "workstream", workstream_id: realId(target.workstream_id) };
}
