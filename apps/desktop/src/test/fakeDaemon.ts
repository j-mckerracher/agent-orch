/**
 * In-memory stand-in for the M5-02 daemon, used by unit and component tests.
 * It speaks the same JSON-RPC shapes as the real daemon (verified against the
 * real binary by the e2e suite) so the real client layer runs unchanged.
 */
import { DaemonError } from "../daemon/errors";
import type { DaemonConnector, OpenedChannel, RpcChannel } from "../daemon/session";
import type {
  Approval,
  Capability,
  DaemonEvent,
  OutputLine,
  ProjectRow,
  Session,
  SessionEntry,
  TaskRecord,
  TaskState,
} from "../daemon/types";
import { DEFAULT_QUEUE_SETTINGS, fakeQueueSnapshot, type FakeQueueSettings } from "./queueSnapshot";

export type Failure = { code: number; message: string } | "hang";

/** States the daemon accepts `tasks.cancel` from; mirrors the daemon, so the fake does not import UI code. */
const CANCELLABLE_STATES: ReadonlySet<TaskState> = new Set(["Created", "Queued", "Running", "Verifying"]);

/** States the daemon accepts `tasks.archive` for (the finished ones); mirrors the daemon, as above. */
const ARCHIVABLE_STATES: ReadonlySet<TaskState> = new Set(["Succeeded", "VerificationFailed", "Failed", "Cancelled"]);

export class FakeChannel implements RpcChannel {
  private messageListeners: Array<(message: unknown) => void> = [];
  private closeListeners: Array<(reason: DaemonError | null) => void> = [];
  closed = false;
  subscriptionId: string | null = null;

  constructor(private readonly daemon: FakeDaemon) {}

  send(message: unknown): void {
    if (this.closed) throw new DaemonError("disconnected", "channel closed");
    queueMicrotask(() => {
      if (!this.closed) this.daemon.handle(this, message as { id: number; method: string; params?: unknown });
    });
  }

  onMessage(listener: (message: unknown) => void): void {
    this.messageListeners.push(listener);
  }

  onClose(listener: (reason: DaemonError | null) => void): void {
    this.closeListeners.push(listener);
  }

  close(): void {
    this.finish(null);
  }

  deliver(message: unknown): void {
    if (this.closed) return;
    for (const listener of this.messageListeners) listener(message);
  }

  finish(reason: DaemonError | null): void {
    if (this.closed) return;
    this.closed = true;
    for (const listener of this.closeListeners) listener(reason);
  }
}

export const mkSession = (id: string, overrides: Partial<Session> = {}): Session => ({
  id,
  attempt_id: `attempt-${id}`,
  adapter: "fake-adapter",
  state: "Running",
  ...overrides,
});

export const mkTask = (
  id: string,
  projectId: string,
  overrides: Omit<Partial<TaskRecord>, "contract"> & { contract?: Partial<TaskRecord["contract"]> } = {},
): TaskRecord => {
  const { contract, ...rest } = overrides;
  return {
    contract: {
      id,
      project_id: projectId,
      parent_id: null,
      objective: `objective of ${id}`,
      kind: "Implement",
      agent: "fake-agent",
      dependencies: [],
      verification: "None",
      delivery: "LocalReview",
      autonomy: "ApprovalRequired",
      max_retries: 1,
      max_repairs: 1,
      ...contract,
    },
    state: "Queued",
    attempts: [],
    sessions: [],
    events: ["TaskAdmitted"],
    version: 1,
    ...rest,
  };
};

export const mkEvent = (sequence: number, taskId: string, sessionId: string | null, kind = "SessionOutput"): DaemonEvent => ({
  sequence,
  task_id: taskId,
  session_id: sessionId,
  kind,
  payload: { n: sequence },
  created_at: `2026-01-01T00:00:${String(sequence).padStart(2, "0")}Z`,
});

export const mkApproval = (id: string, taskId: string, state: "requested" | "rejected" | "approved" = "requested"): Approval => ({
  id,
  action: {
    kind: { type: "delivery", kind: "LocalReviewMerge" },
    parameter_digest: new Array<number>(32).fill(7),
    candidate_commit: "abc123",
    task_id: taskId,
  },
  state: state === "approved" ? { state, by: { kind: "user", id: "u1" } } : { state },
});

export class FakeDaemon implements DaemonConnector {
  projects: ProjectRow[] = [];
  tasks: TaskRecord[] = [];
  /** Ids archived by `tasks.archive`: `tasks.list` and workstream links leave them out; `tasks.get` still returns them. */
  archived = new Set<string>();
  /** Ids with committed work that was not delivered: `tasks.archive` refuses them with Conflict until discarded. */
  undelivered = new Set<string>();
  /** Ids `tasks.discard` recorded. */
  discarded = new Set<string>();
  events: DaemonEvent[] = [];
  approvals: Approval[] = [];
  /** Scheduler limits and restart gate reported by `queue.snapshot`. */
  queue: FakeQueueSettings = { ...DEFAULT_QUEUE_SETTINGS };
  pageSize = 256;
  /** Retained worker output by session id for `sessions.output_tail`; a missing entry is `available: false`. */
  outputs = new Map<string, { lines: OutputLine[]; ended?: boolean; lastSequence?: number }>();
  /** Makes `sessions.events` ignore `after_sequence`, as an overlapping page would. */
  ignoreAfterSequence = false;
  capabilitiesOverride: Capability[] | null = null;
  /** Capability names an older daemon does not know: a handshake naming one is rejected, as `auth.rs` does. */
  unknownCapabilities: Capability[] = [];
  openRequests: Capability[][] = [];
  /** Journal identity advertised in the Welcome; null omits it, as an older daemon would. */
  journalEpoch: string | null = "epoch-1";
  /** Approver flag advertised in the Welcome; null omits it, as an older daemon would. */
  approver: boolean | null = true;
  openFailures: DaemonError[] = [];
  openError: DaemonError | null = null;
  calls: Array<{ method: string; params: unknown }> = [];
  channels: FakeChannel[] = [];
  private stubs = new Map<string, (params: Record<string, unknown>) => unknown>();
  private failures = new Map<string, Failure[]>();
  private delays = new Map<string, number[]>();
  private submitted = new Map<string, TaskRecord>();
  private nextSub = 1;
  private nextTask = 1;

  /** Answers a method with a fixed result or a computed one; use failNext for error replies. */
  stub(method: string, result: unknown | ((params: Record<string, unknown>) => unknown)): void {
    this.stubs.set(method, typeof result === "function" ? (result as (p: Record<string, unknown>) => unknown) : () => result);
  }

  failNext(method: string, failure: Failure, times = 1): void {
    const list = this.failures.get(method) ?? [];
    for (let i = 0; i < times; i++) list.push(failure);
    this.failures.set(method, list);
  }

  /** Replies to the next calls of a method are computed immediately but delivered after `ms`. */
  delayNext(method: string, ms: number, times = 1): void {
    const list = this.delays.get(method) ?? [];
    for (let i = 0; i < times; i++) list.push(ms);
    this.delays.set(method, list);
  }

  callsTo(method: string): Array<{ method: string; params: unknown }> {
    return this.calls.filter((call) => call.method === method);
  }

  async open(requested: readonly Capability[]): Promise<OpenedChannel> {
    this.openRequests.push([...requested]);
    const failure = this.openFailures.shift() ?? this.openError;
    if (failure) throw failure;
    if (requested.some((capability) => this.unknownCapabilities.includes(capability))) {
      throw new DaemonError("capability-unavailable", "the daemon does not offer a requested capability", -32002);
    }
    const channel = new FakeChannel(this);
    this.channels.push(channel);
    return {
      channel,
      welcome: {
        version: 1,
        capabilities: this.capabilitiesOverride ?? [...requested],
        ...(this.journalEpoch === null ? {} : { journal_epoch: this.journalEpoch }),
        ...(this.approver === null ? {} : { approver: this.approver }),
      },
    };
  }

  /** Appends a journal event and pushes it to every subscribed connection. */
  emit(event: DaemonEvent): void {
    this.events.push(event);
    for (const channel of this.channels) {
      if (channel.subscriptionId) {
        channel.deliver({
          jsonrpc: "2.0",
          method: "events.event",
          params: { subscription_id: channel.subscriptionId, event },
        });
      }
    }
  }

  pushStreamError(code: number, message: string): void {
    for (const channel of this.channels) {
      if (channel.subscriptionId) {
        channel.deliver({
          jsonrpc: "2.0",
          method: "events.error",
          params: { subscription_id: channel.subscriptionId, error: { code, message } },
        });
      }
    }
  }

  /** Drops every live connection as a crash or restart would. */
  drop(): void {
    for (const channel of this.channels) channel.finish(new DaemonError("disconnected", "connection lost"));
  }

  get live(): FakeChannel[] {
    return this.channels.filter((channel) => !channel.closed);
  }

  handle(channel: FakeChannel, message: { id: number; method: string; params?: unknown }): void {
    this.calls.push({ method: message.method, params: message.params });
    const failure = this.failures.get(message.method)?.shift();
    if (failure === "hang") return;
    const delay = this.delays.get(message.method)?.shift() ?? 0;
    const send = (frame: unknown) => {
      if (delay > 0) setTimeout(() => channel.deliver(frame), delay);
      else channel.deliver(frame);
    };
    const reply = (result: unknown) => send({ jsonrpc: "2.0", id: message.id, result });
    if (failure) {
      send({ jsonrpc: "2.0", id: message.id, error: failure });
      return;
    }
    const params = (message.params ?? {}) as Record<string, unknown>;
    const fail = (code: number, text: string) =>
      send({ jsonrpc: "2.0", id: message.id, error: { code, message: text } });
    switch (message.method) {
      case "health":
        return reply({ package_version: "0.0.0-fake", uptime_seconds: 1, data_directory: "/fake" });
      case "projects.list":
        return reply(this.projects);
      case "projects.register": {
        const path = String(params.path);
        if (!path.startsWith("/")) return fail(-32602, "path must be absolute");
        const row = { id: path };
        if (!this.projects.some((p) => p.id === path)) this.projects.push({ ...row, coordinator_active: false });
        return reply(row);
      }
      case "tasks.list":
        return reply(this.tasks.filter((t) => !this.archived.has(t.contract.id)));
      case "tasks.get": {
        const task = this.tasks.find((t) => t.contract.id === params.task_id);
        return task ? reply(task) : fail(-32010, "task not found");
      }
      case "tasks.submit": {
        const key = String(params.idempotency_key);
        const existing = this.submitted.get(key);
        if (existing) return reply(existing);
        const contract = params.contract as Record<string, unknown>;
        const task = mkTask(`task-${this.nextTask++}`, String(contract.project_id), {
          contract: contract as Partial<TaskRecord["contract"]>,
        });
        this.tasks.push(task);
        this.submitted.set(key, task);
        return reply(task);
      }
      case "tasks.cancel": {
        const task = this.tasks.find((t) => t.contract.id === params.task_id);
        if (!task) return fail(-32010, "task not found");
        if (!CANCELLABLE_STATES.has(task.state)) return fail(-32011, `task is ${task.state}; it cannot be canceled`);
        task.state = "Cancelled";
        for (const attempt of task.attempts) {
          if (attempt.state === "CandidateReady") attempt.state = "Cancelled";
        }
        task.version += 1;
        return reply(task);
      }
      case "tasks.archive": {
        const ids = params.task_ids as string[];
        const found: TaskRecord[] = [];
        for (const id of ids) {
          const task = this.tasks.find((t) => t.contract.id === id);
          if (!task) return fail(-32010, "task not found");
          found.push(task);
        }
        if (found.some((task) => !ARCHIVABLE_STATES.has(task.state))) return fail(-32011, "task is not finished; it cannot be archived");
        if (found.some((task) => this.undelivered.has(task.contract.id) && !this.discarded.has(task.contract.id))) {
          return fail(-32011, "task has committed work that was not delivered");
        }
        const archived = ids.filter((id) => !this.archived.has(id));
        for (const id of archived) this.archived.add(id);
        return reply({ archived });
      }
      case "tasks.discard": {
        const task = this.tasks.find((t) => t.contract.id === params.task_id);
        if (!task) return fail(-32010, "task not found");
        if (!ARCHIVABLE_STATES.has(task.state)) return fail(-32011, "task is not finished; it cannot be discarded");
        this.discarded.add(task.contract.id);
        return reply({ task_id: task.contract.id, discarded: true });
      }
      case "sessions.list": {
        const entries: SessionEntry[] = this.tasks
          .filter((t) => params.task_id == null || t.contract.id === params.task_id)
          .flatMap((t) => t.sessions.map((session) => ({ task_id: t.contract.id, session })));
        return reply(entries);
      }
      case "sessions.get": {
        const task = this.tasks.find((t) => t.contract.id === params.task_id);
        const session = task?.sessions.find((s) => s.id === params.session_id);
        return task && session ? reply({ task_id: task.contract.id, session }) : fail(-32010, "session not found");
      }
      case "sessions.events": {
        const after = !this.ignoreAfterSequence && typeof params.after_sequence === "number" ? params.after_sequence : 0;
        const all = this.events.filter(
          (e) => e.task_id === params.task_id && e.session_id === params.session_id && e.sequence > after,
        );
        const page = all.slice(0, this.pageSize);
        return reply({
          task_id: params.task_id,
          session_id: params.session_id,
          events: page,
          more: all.length > page.length,
          next_after_sequence: page.length > 0 ? page[page.length - 1]!.sequence : after,
        });
      }
      case "sessions.output_tail": {
        const task = this.tasks.find((t) => t.contract.id === params.task_id);
        const session = task?.sessions.find((x) => x.id === params.session_id);
        if (!task || !session) return fail(-32010, "session not found");
        const limit = typeof params.limit === "number" ? Math.min(params.limit, 100) : 40;
        const after = typeof params.after_sequence === "number" ? params.after_sequence : 0;
        const output = this.outputs.get(session.id);
        const over = session.state === "Exited" || session.state === "Aborted";
        const base = { task_id: task.contract.id, session_id: session.id, limit };
        if (!output) {
          return reply({ ...base, available: false, ended: over, lines: [], truncated_lines: false, last_sequence: 0 });
        }
        const newer = output.lines.filter((l) => l.sequence > after);
        const lines = newer.slice(Math.max(0, newer.length - limit));
        const last = output.lastSequence ?? output.lines.reduce((max, l) => Math.max(max, l.sequence), 0);
        return reply({
          ...base,
          available: true,
          ended: (output.ended ?? false) || over,
          lines,
          truncated_lines: Math.max(0, last - after) > lines.length,
          last_sequence: last,
        });
      }
      case "approvals.list":
        return reply(this.approvals);
      case "queue.snapshot": {
        const stub = this.stubs.get("queue.snapshot");
        return reply(stub ? stub(params) : fakeQueueSnapshot(this.tasks, this.queue, this.events.length, this.journalEpoch ?? "epoch-1"));
      }
      case "events.subscribe": {
        if (channel.subscriptionId) return fail(-32011, "already subscribed");
        if (typeof params.epoch === "string" && this.journalEpoch !== null && params.epoch !== this.journalEpoch) {
          return fail(-32018, "journal epoch mismatch");
        }
        const after = typeof params.after_sequence === "number" ? params.after_sequence : 0;
        const id = `sub-${this.nextSub++}`;
        channel.subscriptionId = id;
        reply({
          subscription_id: id,
          after_sequence: after,
          ...(this.journalEpoch === null ? {} : { epoch: this.journalEpoch }),
        });
        for (const event of this.events.filter((e) => e.sequence > after)) {
          channel.deliver({ jsonrpc: "2.0", method: "events.event", params: { subscription_id: id, event } });
        }
        return;
      }
      default: {
        const stub = this.stubs.get(message.method);
        if (stub) {
          try {
            return reply(stub(params));
          } catch (error) {
            if (error instanceof StubRpcError) return fail(error.code, error.message);
            throw error;
          }
        }
        return fail(-32601, `method unavailable: ${message.method}`);
      }
    }
  }
}

/** Thrown from a stub to answer with a JSON-RPC error (for example -32011 conflict). */
export class StubRpcError extends Error {
  constructor(
    readonly code: number,
    message: string,
  ) {
    super(message);
  }
}

export async function until(check: () => boolean, label = "condition", ms = 3000): Promise<void> {
  const deadline = Date.now() + ms;
  while (!check()) {
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${label}`);
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
}
