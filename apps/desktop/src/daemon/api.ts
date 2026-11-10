/**
 * Typed wrapper over the accepted M5-02 methods used by M5-03.
 *
 * Mutation helpers refuse ids minted by UI fixtures (prefix `fixture:`), so a
 * placeholder id can never reach a real daemon mutation even if a caller wires
 * a fixture record into a real action by mistake.
 */
import { DaemonError } from "./errors";
import { parseQueueSnapshot, type QueueSnapshotWire } from "./queue";
import type { DaemonSession } from "./session";
import type {
  AgentCapabilityEntry,
  Approval,
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
} from "./types";

export const FIXTURE_ID_PREFIX = "fixture:";

export function isFixtureId(id: string): boolean {
  return id.trim().toLowerCase().startsWith(FIXTURE_ID_PREFIX);
}

/** Guard failures reject asynchronously, like every other API failure. */
function realId(id: string): string {
  if (isFixtureId(id)) {
    throw new DaemonError("invalid-params", "placeholder ids cannot be sent to the daemon");
  }
  return id;
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

  /** Records a user message; the daemon never acts on it. */
  async coordinatorSendMessage(projectId: string, text: string, idempotencyKey: string): Promise<CoordinatorEntry> {
    return this.session.request("coordinator.send_message", {
      project_id: realId(projectId),
      text,
      idempotency_key: idempotencyKey,
    });
  }

  /** Validates a proposal (a JSON string) without creating tasks or approvals. */
  async coordinatorPropose(projectId: string, proposal: string, idempotencyKey: string): Promise<CoordinatorEntry> {
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
      candidate_commit: candidateCommit,
      feedback,
      idempotency_key: idempotencyKey,
    });
  }
}
