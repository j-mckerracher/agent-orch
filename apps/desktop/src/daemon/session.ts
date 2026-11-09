/**
 * Transport-neutral M5-02 JSON-RPC session.
 *
 * Host boundary: the native host (Tauri/Rust in M5-05, a Node harness in tests)
 * opens the local socket or pipe, reads the daemon token, performs the
 * `handshake`, and returns the negotiated {@link Welcome}. The webview never
 * holds the token. A host failure during open is thrown as a
 * {@link DaemonError} (`auth`, `version`, `capability-unavailable`, ...).
 *
 * Contract encoded here (see `crates/orchestrator-ipc/src/session.rs`):
 * - one `events.subscribe` per connection; a second returns `conflict`, and
 *   there is no unsubscribe, so resubscribing needs a new connection;
 * - `events.event` sequences are contiguous after the subscribe cursor;
 * - `events.error` ends the subscription.
 */
import { DaemonError, asDaemonError } from "./errors";
import type { Capability, DaemonEvent, Welcome } from "./types";

export interface RpcChannel {
  /** Sends one JSON-RPC message object; throws {@link DaemonError} when closed. */
  send(message: unknown): void;
  onMessage(listener: (message: unknown) => void): void;
  /** Called once when the channel ends for any reason. */
  onClose(listener: (reason: DaemonError | null) => void): void;
  close(): void;
}

export interface OpenedChannel {
  channel: RpcChannel;
  welcome: Welcome;
}

export interface DaemonConnector {
  /** Connects, authenticates and negotiates `requested` capabilities again. */
  open(requested: readonly Capability[]): Promise<OpenedChannel>;
}

export type StreamHandlers = {
  event(event: DaemonEvent): void;
  /** The subscription ended; `error` is the typed `events.error` reason. */
  error(error: DaemonError): void;
};

const METHOD_CAPABILITY: Record<string, Capability> = {
  health: "health",
  "projects.list": "projects",
  "projects.register": "projects",
  "tasks.submit": "tasks",
  "tasks.list": "tasks",
  "tasks.get": "tasks",
  "tasks.cancel": "tasks",
  "sessions.list": "sessions",
  "sessions.get": "sessions",
  "sessions.events": "sessions",
  "events.subscribe": "events",
  "approvals.list": "approvals",
  "approvals.resolve": "approvals",
  "workspaces.diff": "workspaces",
  "delivery.request": "delivery",
  "tasks.retry": "tasks",
  "agents.capabilities": "agents",
  "coordinator.send_message": "coordinator",
  "coordinator.propose": "coordinator",
  "coordinator.get": "coordinator",
  "review.request_approval": "review",
  "review.evidence": "review",
  "review.request_changes": "review",
};

interface Pending {
  resolve(value: unknown): void;
  reject(error: DaemonError): void;
  timer: ReturnType<typeof setTimeout>;
  onResult?: (result: unknown) => void;
}

export interface SessionOptions {
  requestTimeoutMs?: number;
}

export class DaemonSession {
  readonly welcome: Welcome;
  private readonly channel: RpcChannel;
  private readonly requestTimeoutMs: number;
  private readonly pending = new Map<number, Pending>();
  private nextId = 1;
  private closedReason: DaemonError | null | undefined;
  private closeListeners: Array<(reason: DaemonError | null) => void> = [];
  private subscription: { id: string; lastSequence: number } | null = null;
  private handlers: StreamHandlers | null = null;

  constructor(opened: OpenedChannel, options: SessionOptions = {}) {
    this.channel = opened.channel;
    this.welcome = opened.welcome;
    this.requestTimeoutMs = options.requestTimeoutMs ?? 10_000;
    this.channel.onMessage((message) => this.dispatch(message));
    this.channel.onClose((reason) => this.handleClose(reason));
  }

  get closed(): boolean {
    return this.closedReason !== undefined;
  }

  hasCapability(capability: Capability): boolean {
    return this.welcome.capabilities.includes(capability);
  }

  onClose(listener: (reason: DaemonError | null) => void): void {
    if (this.closedReason !== undefined) {
      listener(this.closedReason);
      return;
    }
    this.closeListeners.push(listener);
  }

  close(): void {
    if (this.closedReason === undefined) this.channel.close();
    this.handleClose(null);
  }

  /** Sends a request. Rejects locally when the capability was not negotiated. */
  request<T>(method: string, params?: unknown, timeoutMs = this.requestTimeoutMs): Promise<T> {
    return this.send(method, params, timeoutMs) as Promise<T>;
  }

  /**
   * Starts the event stream after `afterSequence` (0 replays everything). The
   * subscription id is recorded synchronously with the response so notifications
   * delivered in the same tick are not dropped.
   */
  async subscribe(afterSequence: number, handlers: StreamHandlers, epoch?: string): Promise<void> {
    if (this.subscription) throw new DaemonError("conflict", "already subscribed", -32011);
    this.handlers = handlers;
    const pendingCursor = afterSequence;
    const params: Record<string, unknown> = {};
    if (afterSequence > 0) params.after_sequence = afterSequence;
    if (afterSequence > 0 && epoch) params.epoch = epoch;
    await this.send("events.subscribe", params, this.requestTimeoutMs, (result) => {
      const id = (result as { subscription_id?: unknown } | null)?.subscription_id;
      if (typeof id !== "string") throw new DaemonError("protocol", "malformed subscribe result");
      this.subscription = { id, lastSequence: pendingCursor };
    });
  }

  private send(
    method: string,
    params: unknown,
    timeoutMs: number,
    onResult?: (result: unknown) => void,
  ): Promise<unknown> {
    if (this.closedReason !== undefined) {
      return Promise.reject(this.closedReason ?? new DaemonError("disconnected", "connection closed"));
    }
    const capability = METHOD_CAPABILITY[method];
    if (capability && !this.hasCapability(capability)) {
      return Promise.reject(
        new DaemonError("capability-not-negotiated", `capability ${capability} not negotiated`, -32004),
      );
    }
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new DaemonError("timeout", `${method} timed out`));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer, onResult });
      try {
        this.channel.send({
          jsonrpc: "2.0",
          id,
          method,
          ...(params === undefined ? {} : { params }),
        });
      } catch (error) {
        clearTimeout(timer);
        this.pending.delete(id);
        reject(asDaemonError(error));
      }
    });
  }

  private dispatch(message: unknown): void {
    if (typeof message !== "object" || message === null) return this.protocolFailure();
    const frame = message as Record<string, unknown>;
    if (frame.jsonrpc !== "2.0") return this.protocolFailure();
    if (typeof frame.method === "string") return this.notification(frame.method, frame.params);
    if (typeof frame.id !== "number") return this.protocolFailure();
    const pending = this.pending.get(frame.id);
    if (!pending) return;
    this.pending.delete(frame.id);
    clearTimeout(pending.timer);
    const failure = frame.error as { code?: unknown; message?: unknown } | undefined;
    if (failure) {
      pending.reject(
        DaemonError.fromRpc(Number(failure.code), typeof failure.message === "string" ? failure.message : ""),
      );
      return;
    }
    try {
      pending.onResult?.(frame.result);
      pending.resolve(frame.result);
    } catch (error) {
      pending.reject(asDaemonError(error));
    }
  }

  private notification(method: string, params: unknown): void {
    const body = params as { subscription_id?: unknown } | null;
    const subscription = this.subscription;
    if (!subscription || body?.subscription_id !== subscription.id) return;
    if (method === "events.event") {
      const event = (params as { event?: DaemonEvent }).event;
      if (!event || typeof event.sequence !== "number") return this.endStream(new DaemonError("protocol", "malformed event"));
      if (event.sequence <= subscription.lastSequence) return; // duplicate, applied once
      if (event.sequence !== subscription.lastSequence + 1) {
        return this.endStream(new DaemonError("replay-gap", "event sequence gap", -32017));
      }
      subscription.lastSequence = event.sequence;
      this.handlers?.event(event);
    } else if (method === "events.error") {
      const error = (params as { error?: { code?: number; message?: string } }).error;
      this.endStream(DaemonError.fromRpc(Number(error?.code), error?.message ?? ""));
    }
  }

  private endStream(error: DaemonError): void {
    const handlers = this.handlers;
    this.handlers = null;
    this.subscription = null;
    handlers?.error(error);
  }

  private protocolFailure(): void {
    const error = new DaemonError("protocol", "malformed frame");
    this.endStream(error);
    this.handleClose(error);
    this.channel.close();
  }

  private handleClose(reason: DaemonError | null): void {
    if (this.closedReason !== undefined) return;
    this.closedReason = reason;
    const error = reason ?? new DaemonError("disconnected", "connection closed");
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    this.pending.clear();
    for (const listener of this.closeListeners) listener(reason);
    this.closeListeners = [];
  }
}
