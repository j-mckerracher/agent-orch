/**
 * Typed wrapper over the accepted M5-02 methods used by M5-03.
 *
 * Mutation helpers refuse fixture-like ids, including historical colon-less ids, so a
 * placeholder id can never reach a real daemon mutation even if a caller wires
 * a fixture record into a real action by mistake.
 */
import { DaemonError, asDaemonError, isMissingFeature } from "./errors";
import { parseQueueSnapshot, type QueueSnapshotWire } from "./queue";
import type { DaemonSession } from "./session";
import type {
  AgentCapabilityEntry,
  AgentCatalogEntry,
  Approval,
  ConversationCancelResult,
  ConversationSelectionWire,
  ConversationSendResult,
  ConversationWirePage,
  PermissionLevelWire,
  ConversationWireTarget,
  IntakeNormalizeResult,
  ModelSelectionWire,
  SelectionCheck,
  WorkClassInput,
  WorkClassesSaveResult,
  WorkClassesWire,
  CoordinatorEntry,
  CoordinatorPage,
  DaemonHealth,
  DeliveryRequest,
  DeliveryResult,
  ProjectRow,
  ProjectDefaultsResultWire,
  ProjectDefaultsWire,
  ResolveDecision,
  ReviewApprovalRequest,
  ReviewApprovalResult,
  ReviewChangesResult,
  ReviewEvidenceWire,
  SessionEntry,
  SessionEventsPage,
  SessionOutputTail,
  TaskRecord,
  TaskSubmission,
  WorkspaceDiff,
  WorkspaceDiffRequest,
  GateDraft,
  ObjectiveWire,
  WorkstreamDetailWire,
  WorkstreamPlanEditWire,
  WorkstreamRemoveTaskWire,
  WorkstreamReviseObjectiveRequest,
  WorkstreamSetGateRequest,
  WorkstreamStateWire,
  WorkstreamTaskLinkWire,
  WorkstreamTaskPolicyPatch,
  WorkstreamUpdateRequest,
  WorkstreamWire,
  ActionsPendingWire,
  ActionsResolveWire,
  ClearedEntryReference,
  ConversationClearResult,
  CoordinatorChangeResult,
  CoordinatorClearResult,
  CopilotSignInRecord,
  LoadLimitsWire,
  SettingsWire,
  SettingsPatchWire,
  MonitoringWire,
  MonitoringAckWire,
  BriefAckWire,
  BriefResultWire,
  GrantCreateResultWire,
  GrantRevokeResultWire,
  GrantsCreateParams,
  GrantsListWire,
  NotesSetWire,
  NotesWire,
  HoldResultWire,
  HoldsListWire,
  QuestionResultWire,
  QuestionsListWire,
  PullsListWire,
  PullsRefreshWire,
} from "./types";
import type { HostMetricsWire } from "./hostMetrics";
import type { LocalToolsSetEnabledRequest, LocalToolsWindowRequest, UsageSummaryRequest } from "./usageWire";

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

/** A notes scope is `global` or `workstream:` plus an id; the id must not be a placeholder. */
function realScope(scope: string): string {
  if (scope.startsWith("workstream:")) realId(scope.slice("workstream:".length));
  return scope;
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

  /** DPES-1105. A project's delivery defaults; `local-only` when none are saved. A daemon without the method rejects as a missing feature. */
  async projectsGetDefaults(projectId: string): Promise<ProjectDefaultsResultWire> {
    return this.session.request("projects.get_defaults", { project_id: realId(projectId) });
  }

  /** DPES-1105. Replaces a project's delivery defaults. A user-only method: the daemon refuses it without the approver secret. */
  async projectsSetDefaults(
    projectId: string,
    defaults: ProjectDefaultsWire,
    idempotencyKey: string,
  ): Promise<ProjectDefaultsResultWire> {
    return this.session.request("projects.set_defaults", {
      project_id: realId(projectId),
      ...defaults,
      idempotency_key: idempotencyKey,
    });
  }

  tasksList(): Promise<TaskRecord[]> {
    return this.session.request("tasks.list");
  }

  async tasksGet(taskId: string): Promise<TaskRecord> {
    return this.session.request("tasks.get", { task_id: realId(taskId) });
  }

  /**
   * DPES-673 (capability `tasks`). Pure and read-only: turns pasted text, a Jira key or a GitHub issue
   * reference into a draft work item to show for confirmation. Nothing is fetched or stored; over-limit
   * input (20 000 bytes, 200-character title, 20 criteria, 500 characters each) is refused with `invalid-params`.
   */
  intakeNormalize(input: string): Promise<IntakeNormalizeResult> {
    return this.session.request("intake.normalize", { input });
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

  /** Archives finished tasks (DPES-1106). The result lists only the ids this call newly archived. */
  async tasksArchive(taskIds: string[], idempotencyKey: string): Promise<{ archived: string[] }> {
    return this.session.request("tasks.archive", {
      task_ids: taskIds.map(realId),
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

  /** Read-only; `limit` defaults to 40 and is clamped to 100 by the daemon. Output is not redacted. */
  async sessionsOutputTail(
    taskId: string,
    sessionId: string,
    options: { limit?: number; afterSequence?: number } = {},
  ): Promise<SessionOutputTail> {
    return this.session.request("sessions.output_tail", {
      task_id: realId(taskId),
      session_id: realId(sessionId),
      ...(options.limit === undefined ? {} : { limit: options.limit }),
      ...(options.afterSequence === undefined ? {} : { after_sequence: options.afterSequence }),
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

  /** Cached figures from other AI tools' local session files; the daemon never runs a reader on the call. */
  usageLocalTools(request: LocalToolsWindowRequest): Promise<unknown> {
    return this.session.request("usage.localTools", request);
  }

  /** Opts this machine in or out of reading those files. Off clears the daemon's cached figures. */
  usageLocalToolsSetEnabled(request: LocalToolsSetEnabledRequest): Promise<unknown> {
    return this.session.request("usage.localTools.setEnabled", request);
  }

  /** Asks the background collector to refresh now (rate limited); the snapshot comes back at once. */
  usageLocalToolsRefresh(request: LocalToolsWindowRequest): Promise<unknown> {
    return this.session.request("usage.localTools.refresh", request);
  }

  /** Records a user message; the daemon never acts on it. */
  async coordinatorSendMessage(
    projectId: string,
    text: string,
    idempotencyKey: string,
  ): Promise<CoordinatorEntry | ClearedEntryReference> {
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

  /** DPES-1105. Permanently deletes a project coordinator's log; the project and its tasks stay. */
  async coordinatorClear(projectId: string, idempotencyKey: string): Promise<CoordinatorClearResult> {
    return this.session.request("coordinator.clear", {
      project_id: realId(projectId),
      idempotency_key: idempotencyKey,
    });
  }

  /** DPES-1105. Removes a project's coordinator from the targets and deletes its log. Idempotent. */
  async coordinatorRemove(projectId: string, idempotencyKey: string): Promise<CoordinatorChangeResult> {
    return this.session.request("coordinator.remove", {
      project_id: realId(projectId),
      idempotency_key: idempotencyKey,
    });
  }

  /** DPES-1105. Restores a removed coordinator; it starts empty. Idempotent. */
  async coordinatorRestore(projectId: string, idempotencyKey: string): Promise<CoordinatorChangeResult> {
    return this.session.request("coordinator.restore", {
      project_id: realId(projectId),
      idempotency_key: idempotencyKey,
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

  async workstreamsPrioritize(workstreamId: string, idempotencyKey: string): Promise<WorkstreamWire> {
    return this.session.request("workstreams.prioritize", {
      workstream_id: realId(workstreamId),
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

  /** Queues a task and links it to the workstream (and objective) in one request; `contract` is `tasks.submit`'s. */
  async workstreamsAddTask(
    workstreamId: string,
    objectiveId: string | undefined,
    contract: TaskSubmission,
    idempotencyKey: string,
  ): Promise<WorkstreamPlanEditWire> {
    realIdFields(contract);
    return this.session.request("workstreams.add_task", {
      workstream_id: realId(workstreamId),
      ...(objectiveId === undefined ? {} : { objective_id: realId(objectiveId) }),
      contract: {
        ...contract,
        project_id: realId(contract.project_id),
        parent_id: contract.parent_id ? realId(contract.parent_id) : null,
        dependencies: contract.dependencies.map(realId),
      },
      idempotency_key: idempotencyKey,
    });
  }

  /** Replaces one task with 2 to 8 tasks, one per objective text. */
  async workstreamsSplitTask(
    workstreamId: string,
    taskId: string,
    objectives: string[],
    idempotencyKey: string,
  ): Promise<WorkstreamPlanEditWire> {
    return this.session.request("workstreams.split_task", {
      workstream_id: realId(workstreamId),
      task_id: realId(taskId),
      objectives,
      idempotency_key: idempotencyKey,
    });
  }

  /** Replaces 2 to 8 tasks of one project and kind with one task. */
  async workstreamsCombineTasks(
    workstreamId: string,
    taskIds: string[],
    objective: string,
    idempotencyKey: string,
  ): Promise<WorkstreamPlanEditWire> {
    return this.session.request("workstreams.combine_tasks", {
      workstream_id: realId(workstreamId),
      task_ids: taskIds.map(realId),
      objective,
      idempotency_key: idempotencyKey,
    });
  }

  /** Cancels a not-yet-started task, then unlinks it; a finished task is only unlinked. */
  async workstreamsRemoveTask(workstreamId: string, taskId: string, idempotencyKey: string): Promise<WorkstreamRemoveTaskWire> {
    return this.session.request("workstreams.remove_task", {
      workstream_id: realId(workstreamId),
      task_id: realId(taskId),
      idempotency_key: idempotencyKey,
    });
  }

  /** Replaces the task with a copy that runs on `agent`. */
  async workstreamsReassignTask(
    workstreamId: string,
    taskId: string,
    agent: string,
    modelSelection: ModelSelectionWire | undefined,
    idempotencyKey: string,
  ): Promise<WorkstreamPlanEditWire> {
    if (modelSelection !== undefined) realIdFields(modelSelection);
    return this.session.request("workstreams.reassign_task", {
      workstream_id: realId(workstreamId),
      task_id: realId(taskId),
      agent: realId(agent),
      ...(modelSelection === undefined ? {} : { model_selection: modelSelection }),
      idempotency_key: idempotencyKey,
    });
  }

  /** Replaces the task with a copy under the changed policy fields; the daemon refuses an empty patch. */
  async workstreamsSetTaskPolicy(
    workstreamId: string,
    taskId: string,
    patch: WorkstreamTaskPolicyPatch,
    idempotencyKey: string,
  ): Promise<WorkstreamPlanEditWire> {
    const fields = Object.fromEntries(Object.entries(patch).filter(([, value]) => value !== undefined));
    return this.session.request("workstreams.set_task_policy", {
      workstream_id: realId(workstreamId),
      task_id: realId(taskId),
      ...fields,
      idempotency_key: idempotencyKey,
    });
  }

  /**
   * M7-08. Records the message and starts a turn; the turn can still fail (e.g. `runtime_unavailable`).
   * `selection` is omitted unless the caller has one, so an older daemon sees the request it always did.
   */
  async conversationsSend(
    target: ConversationWireTarget,
    text: string,
    idempotencyKey: string,
    selection?: ConversationSelectionWire,
    permission?: PermissionLevelWire,
  ): Promise<ConversationSendResult> {
    return this.session.request("conversations.send", {
      target: realTarget(target),
      text,
      idempotency_key: idempotencyKey,
      ...(selection === undefined ? {} : { selection }),
      // Sent only when the daemon advertises `permission_levels`: an older daemon rejects unknown fields.
      ...(permission === undefined ? {} : { permission }),
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

  /** DPES-1105. Permanently deletes one conversation's history; refused with `conversation-busy` while a reply runs. */
  async conversationsClear(target: ConversationWireTarget, idempotencyKey: string): Promise<ConversationClearResult> {
    return this.session.request("conversations.clear", {
      target: realTarget(target),
      idempotency_key: idempotencyKey,
    });
  }

  /**
   * DPES-1105. Approves or rejects one proposed action. The daemon answers with the proposal as it now stands,
   * which may be `failed` with an `error_code` when the approved change could not be applied.
   */
  async actionsResolve(
    proposalId: string,
    decision: "approve" | "reject",
    digest: string,
    idempotencyKey: string,
  ): Promise<ActionsResolveWire> {
    return this.session.request("actions.resolve", {
      proposal_id: realId(proposalId),
      decision,
      digest,
      idempotency_key: idempotencyKey,
    });
  }

  /**
   * DPES-1105. Every pending proposal across all conversations, newest first. A read only: nothing is decided here.
   * An older daemon has no such method or capability, so that reads as nothing pending, never as a failure.
   */
  async actionsPending(): Promise<ActionsPendingWire> {
    try {
      return await this.session.request("actions.pending", {});
    } catch (error) {
      if (isMissingFeature(asDaemonError(error))) return { proposals: [], more: false };
      throw error;
    }
  }

  /**
   * DPES-1105. The machine load limits and the live state of the gate that holds new task starts. A read only.
   * A daemon without the `settings` capability rejects it as a missing feature, which callers read as "not available".
   */
  settingsGet(): Promise<SettingsWire> {
    return this.session.request("settings.get", {});
  }

  /**
   * DPES-1105. Changes some of the load limits and answers with `settings.get` as it now stands. A user-only method:
   * the daemon refuses it unless this session proved the approver secret. The new limits apply from the next sample.
   */
  settingsSet(patch: Partial<LoadLimitsWire>, idempotencyKey: string): Promise<SettingsWire> {
    return this.session.request("settings.set", { idempotency_key: idempotencyKey, load: patch });
  }

  /**
   * DPES-1105 Phase 3. Changes the supervision or wake-limit groups. Only the groups present are sent: a group
   * must name at least one field. User-only, like `settingsSet`.
   */
  settingsSetGroups(
    patch: Pick<SettingsPatchWire, "supervision" | "wake_limits" | "chat_pr_delivery" | "mcp" | "home_directory" | "dispatch_defaults">,
    idempotencyKey: string,
  ): Promise<SettingsWire> {
    return this.session.request("settings.set", { idempotency_key: idempotencyKey, ...patch });
  }

  /** DPES-1105 Phase 3. Monitoring mode, paused scopes and unseen counts. A read; needs the `monitoring` capability. */
  monitoringGet(): Promise<MonitoringWire> {
    return this.session.request("monitoring.get", {});
  }

  /** DPES-1105 Phase 3. Sets the monitoring mode and answers with `monitoring.get` as it now stands. User-only. */
  monitoringSet(mode: string, idempotencyKey: string): Promise<MonitoringWire> {
    return this.session.request("monitoring.set", { idempotency_key: idempotencyKey, mode });
  }

  /** DPES-1105 Phase 3. Marks a scope's wake records seen. Changes no decision, so it is not user-only. */
  async monitoringAck(target: ConversationWireTarget, idempotencyKey: string): Promise<MonitoringAckWire> {
    return this.session.request("monitoring.ack", { idempotency_key: idempotencyKey, target: realTarget(target) });
  }

  /** DPES-1105 Phase 5b. The return brief for the away window, or `brief: null`. A read; needs `standing_approvals`. */
  briefGet(): Promise<BriefResultWire> {
    return this.session.request("monitoring.brief", {});
  }

  /** DPES-1105 Phase 5b. Marks the closed brief seen. Changes no decision and deletes no data, so it is not user-only. */
  briefAck(idempotencyKey: string): Promise<BriefAckWire> {
    return this.session.request("monitoring.brief_ack", { idempotency_key: idempotencyKey });
  }

  /** DPES-1105 Phase 4. Hides one decision until a date (`YYYY-MM-DD`). User-only; needs the `holds` capability. */
  async holdsSet(subjectKind: string, subjectId: string, until: string, idempotencyKey: string): Promise<HoldResultWire> {
    return this.session.request("holds.set", {
      subject_kind: subjectKind,
      subject_id: realId(subjectId),
      until,
      idempotency_key: idempotencyKey,
    });
  }

  /** DPES-1105 Phase 4. Ends a hold before its date. User-only. */
  async holdsRelease(holdId: string, idempotencyKey: string): Promise<HoldResultWire> {
    return this.session.request("holds.release", { hold_id: realId(holdId), idempotency_key: idempotencyKey });
  }

  /** DPES-1105 Phase 4. The holds in force now. A read. */
  async holdsList(): Promise<HoldsListWire> {
    return this.session.request("holds.list", {});
  }

  /** DPES-1108. Every open question for the user, oldest first. A read; needs `holds`. */
  async decisionsList(): Promise<QuestionsListWire> {
    return this.session.request("decisions.list", {});
  }

  /** DPES-1108. Answers a question with the user's exact words, sent as typed. User-only. */
  async decisionsAnswer(questionId: string, answer: string, idempotencyKey: string): Promise<QuestionResultWire> {
    return this.session.request("decisions.answer", { question_id: realId(questionId), answer, idempotency_key: idempotencyKey });
  }

  /** DPES-1108. Defers a question until a date (`YYYY-MM-DD`); it stays open. User-only. */
  async decisionsDefer(questionId: string, until: string, idempotencyKey: string): Promise<QuestionResultWire> {
    return this.session.request("decisions.defer", { question_id: realId(questionId), until, idempotency_key: idempotencyKey });
  }

  /** DPES-1108. Closes a question that became moot, with evidence text. User-only. */
  async decisionsReconcile(questionId: string, evidence: string, idempotencyKey: string): Promise<QuestionResultWire> {
    return this.session.request("decisions.reconcile", { question_id: realId(questionId), evidence, idempotency_key: idempotencyKey });
  }

  /** DPES-1105 Phase 4c. The pull requests the daemon watches. A read; needs `landing`. */
  async pullsList(): Promise<PullsListWire> {
    return this.session.request("pulls.list", {});
  }

  /** DPES-1105 Phase 4c. Reads one task's pull request again. A read; the daemon allows it once per 10 seconds per task. */
  async pullsRefresh(taskId: string): Promise<PullsRefreshWire> {
    return this.session.request("pulls.refresh", { task_id: realId(taskId) });
  }

  /** DPES-1105 Phase 5. The standing approvals and the limits for a new one. A read; needs `standing_approvals`. */
  async grantsList(includeEnded = true): Promise<GrantsListWire> {
    return this.session.request("grants.list", { include_ended: includeEnded, limit: 50 });
  }

  /** DPES-1105 Phase 5. Creates one standing approval. User-only. */
  async grantsCreate(params: GrantsCreateParams, idempotencyKey: string): Promise<GrantCreateResultWire> {
    realId(params.scope.id);
    return this.session.request("grants.create", { ...params, idempotency_key: idempotencyKey });
  }

  /** DPES-1105 Phase 5. Ends a standing approval now. User-only. */
  async grantsRevoke(grantId: string, idempotencyKey: string): Promise<GrantRevokeResultWire> {
    return this.session.request("grants.revoke", { grant_id: realId(grantId), idempotency_key: idempotencyKey });
  }

  /** DPES-1105 5b. One scope's orchestrator notes and their limits. A read; needs `standing_approvals`. */
  async notesList(scope: string): Promise<NotesWire> {
    return this.session.request("notes.list", { scope: realScope(scope) });
  }

  /** DPES-1105 5b. Replaces one scope's whole list of notes. User-only. */
  async notesSet(scope: string, notes: string[], idempotencyKey: string): Promise<NotesSetWire> {
    return this.session.request("notes.set", { scope: realScope(scope), notes, idempotency_key: idempotencyKey });
  }

  /** M7-09. Per-model efforts; a cold catalog answers state `unknown` ("retry shortly"). */
  agentsCatalog(): Promise<AgentCatalogEntry[]> {
    return this.session.request("agents.catalog");
  }

  /**
   * DPES-1105. The one billable Copilot sign-in check: the daemon runs a single cheapest-model call
   * only when asked, caches the result for the session, and returns it as text. `force` runs it again.
   */
  agentsCopilotSignInCheck(force = false): Promise<CopilotSignInRecord> {
    return this.session.request("agents.check_copilot_sign_in", { force });
  }

  /** DPES-677 (capability `agents`). A broken settings file reads as state `invalid_configuration`, never an error. */
  settingsWorkClassesGet(): Promise<WorkClassesWire> {
    return this.session.request("settings.workClasses.get");
  }

  /** DPES-677. Replaces the whole set; checks every class against the live catalog and writes nothing when one is refused. */
  async settingsWorkClassesSet(classes: WorkClassInput[]): Promise<WorkClassesSaveResult> {
    realIdFields(classes);
    return this.session.request("settings.workClasses.set", { classes });
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
  if (target.kind === "project") return { kind: "project", project_id: realId(target.project_id) };
  return target.kind === "global" ? target : { kind: "workstream", workstream_id: realId(target.workstream_id) };
}
