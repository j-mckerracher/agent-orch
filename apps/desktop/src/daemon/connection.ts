/**
 * Connection lifecycle for the M5-02 session (C1 data source).
 *
 * Behavior:
 * - every (re)connect goes through {@link DaemonConnector.open}, which
 *   authenticates and negotiates capabilities again;
 * - optional capabilities (`host`) are dropped from the handshake when the
 *   daemon refuses it with `capability-unavailable`, so an older daemon
 *   degrades that feature instead of making the app incompatible;
 * - the event stream resumes after the last applied sequence and is
 *   deduplicated by {@link DaemonSession}; `events.error` (slow consumer,
 *   gap, internal) marks history incomplete and reconnects;
 * - `replay-gap` restarts from a fresh baseline (cursor 0) because the daemon
 *   journal is behind the saved cursor;
 * - the Welcome names the journal (`journal_epoch`, M5-02b). A resume sends
 *   that epoch with the cursor, and the daemon answers `epoch-mismatch`
 *   (-32018) when the store was replaced; the client then restarts from a
 *   fresh baseline instead of silently resuming against another journal;
 * - the cursor and epoch are held in memory only.
 */
import { DaemonApi } from "./api";
import { DaemonError, asDaemonError, type DaemonErrorKind } from "./errors";
import { DaemonSession, type DaemonConnector, type OpenedChannel } from "./session";
import { Store } from "./store";
import { M5_03_CAPABILITIES, OPTIONAL_CAPABILITIES, type Capability, type DaemonEvent } from "./types";

export type ConnectionStatus =
  | "idle"
  | "connecting"
  | "connected"
  | "reconnecting"
  | "disconnected"
  | "unavailable"
  | "auth-failed"
  | "incompatible";

export interface ConnectionSnapshot {
  status: ConnectionStatus;
  error: DaemonError | null;
  capabilities: Capability[];
  /** Last event sequence applied in this process; 0 means none. */
  lastSequence: number;
  /** Time the last event was received, not inferred from heartbeats. */
  lastEventAt: number | null;
  lastConnectedAt: number | null;
  /** Set when replay may have missed events; cleared by a fresh baseline. */
  history: { incomplete: boolean; reason: DaemonErrorKind | null };
  /** Cursor used to resume the latest subscription; null for a fresh baseline. */
  resumedFrom: number | null;
  /** Increments each time a connection is fully established. */
  generation: number;
  failedAttempts: number;
  /** Journal identity from the daemon Welcome; null before the first connection or when absent. */
  journalEpoch: string | null;
}

export interface ConnectionOptions {
  connector: DaemonConnector;
  capabilities?: readonly Capability[];
  /**
   * Requested capabilities the app can run without. When the daemon refuses the handshake with
   * `capability-unavailable`, it is repeated once without them. Defaults to {@link OPTIONAL_CAPABILITIES}.
   */
  optionalCapabilities?: readonly Capability[];
  now?: () => number;
  requestTimeoutMs?: number;
  backoffMs?: readonly number[];
  /** Consecutive failures before automatic retry stops. */
  maxAutoAttempts?: number;
}

const DEFAULT_BACKOFF = [500, 1000, 2000, 4000, 8000];

export class ConnectionManager extends Store<ConnectionSnapshot> {
  private readonly connector: DaemonConnector;
  private readonly requested: readonly Capability[];
  private readonly optional: readonly Capability[];
  private readonly now: () => number;
  private readonly requestTimeoutMs: number | undefined;
  private readonly backoff: readonly number[];
  private readonly maxAutoAttempts: number;
  private session: DaemonSession | null = null;
  private epoch = 0;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private stopped = true;
  private eventListeners = new Set<(event: DaemonEvent) => void>();
  private connectedListeners = new Set<() => void>();

  constructor(options: ConnectionOptions) {
    super({
      status: "idle",
      error: null,
      capabilities: [],
      lastSequence: 0,
      lastEventAt: null,
      lastConnectedAt: null,
      history: { incomplete: false, reason: null },
      resumedFrom: null,
      generation: 0,
      failedAttempts: 0,
      journalEpoch: null,
    });
    this.connector = options.connector;
    this.requested = options.capabilities ?? M5_03_CAPABILITIES;
    this.optional = options.optionalCapabilities ?? OPTIONAL_CAPABILITIES;
    this.now = options.now ?? Date.now;
    this.requestTimeoutMs = options.requestTimeoutMs;
    this.backoff = options.backoffMs ?? DEFAULT_BACKOFF;
    this.maxAutoAttempts = options.maxAutoAttempts ?? 5;
  }

  start(): void {
    if (!this.stopped) return;
    this.stopped = false;
    void this.establish();
  }

  stop(): void {
    this.stopped = true;
    this.epoch++;
    this.clearTimer();
    this.dropSession();
    this.update({ status: "idle" });
  }

  /** Authenticated API for the current connection; throws when not connected. */
  api(): DaemonApi {
    if (!this.session || this.state_().status !== "connected") {
      throw new DaemonError("disconnected", "not connected to the daemon");
    }
    return new DaemonApi(this.session);
  }

  onEvent(listener: (event: DaemonEvent) => void): () => void {
    this.eventListeners.add(listener);
    return () => this.eventListeners.delete(listener);
  }

  /** Fires after each fully established connection, once the stream is live. */
  onConnected(listener: () => void): () => void {
    this.connectedListeners.add(listener);
    return () => this.connectedListeners.delete(listener);
  }

  /**
   * Manual reconnect. `fresh` discards the saved cursor, replays from the
   * start of the journal and clears the incomplete-history mark.
   */
  reconnect(options: { fresh?: boolean } = {}): void {
    this.stopped = false;
    this.clearTimer();
    this.dropSession();
    this.update({
      failedAttempts: 0,
      ...(options.fresh
        ? { lastSequence: 0, journalEpoch: null, history: { incomplete: false, reason: null } }
        : {}),
    });
    void this.establish();
  }

  private state_(): ConnectionSnapshot {
    return this.getSnapshot();
  }

  /**
   * Asks for every requested capability first, so a daemon that has them is always used in full. A daemon
   * that refuses (it rejects capability names it does not know) is asked again without the optional ones.
   */
  private async openNegotiated(): Promise<OpenedChannel> {
    try {
      return await this.connector.open(this.requested);
    } catch (error) {
      const required = this.requested.filter((capability) => !this.optional.includes(capability));
      if (asDaemonError(error).kind !== "capability-unavailable" || required.length === this.requested.length) throw error;
      return this.connector.open(required);
    }
  }

  private async establish(): Promise<void> {
    const epoch = ++this.epoch;
    this.clearTimer();
    const first = this.state_().generation === 0;
    this.update({ status: first ? "connecting" : "reconnecting" });

    let session: DaemonSession;
    try {
      const opened = await this.openNegotiated();
      if (epoch !== this.epoch) {
        opened.channel.close();
        return;
      }
      session = new DaemonSession(opened, { requestTimeoutMs: this.requestTimeoutMs });
    } catch (error) {
      if (epoch === this.epoch) this.fail(asDaemonError(error));
      return;
    }

    this.session = session;
    const cursor = this.state_().lastSequence;
    const savedEpoch = this.state_().journalEpoch ?? undefined;
    session.onClose((reason) => {
      if (epoch !== this.epoch || this.session !== session) return;
      this.session = null;
      this.fail(reason ?? new DaemonError("disconnected", "connection closed"));
    });
    try {
      await session.subscribe(
        cursor,
        {
          event: (event) => this.applyEvent(epoch, event),
          error: (error) => this.streamEnded(epoch, session, error),
        },
        savedEpoch,
      );
    } catch (error) {
      if (epoch !== this.epoch || this.session !== session) return;
      const failure = asDaemonError(error);
      this.session = null;
      session.close();
      if (failure.kind === "replay-gap" || failure.kind === "epoch-mismatch") {
        this.update({
          lastSequence: 0,
          journalEpoch: null,
          history: { incomplete: true, reason: failure.kind },
        });
        void this.establish();
      } else {
        this.fail(failure);
      }
      return;
    }
    if (epoch !== this.epoch || this.session !== session) return;
    if (session.closed) return;
    this.update({
      status: "connected",
      error: null,
      capabilities: session.welcome.capabilities,
      lastConnectedAt: this.now(),
      resumedFrom: cursor > 0 ? cursor : null,
      generation: this.state_().generation + 1,
      failedAttempts: 0,
      journalEpoch: session.welcome.journal_epoch ?? null,
    });
    for (const listener of [...this.connectedListeners]) listener();
  }

  private applyEvent(epoch: number, event: DaemonEvent): void {
    if (epoch !== this.epoch) return;
    this.update({ lastSequence: event.sequence, lastEventAt: this.now() });
    for (const listener of [...this.eventListeners]) listener(event);
  }

  private streamEnded(epoch: number, session: DaemonSession, error: DaemonError): void {
    if (epoch !== this.epoch || this.session !== session) return;
    this.session = null;
    session.close();
    this.update({
      history: { incomplete: true, reason: error.kind },
      ...(error.kind === "replay-gap" || error.kind === "epoch-mismatch"
        ? { lastSequence: 0, journalEpoch: null }
        : {}),
    });
    this.fail(error);
  }

  private fail(error: DaemonError): void {
    this.dropSession();
    if (this.stopped) return;
    if (error.kind === "auth") {
      this.update({ status: "auth-failed", error });
      return;
    }
    if (
      error.kind === "version" ||
      error.kind === "capability-unavailable" ||
      error.kind === "invalid-handshake"
    ) {
      this.update({ status: "incompatible", error });
      return;
    }
    const failedAttempts = this.state_().failedAttempts + 1;
    if (failedAttempts > this.maxAutoAttempts) {
      this.update({
        status: error.kind === "daemon-unavailable" ? "unavailable" : "disconnected",
        error,
        failedAttempts,
      });
      return;
    }
    this.update({ status: "reconnecting", error, failedAttempts });
    const delay = this.backoff[Math.min(failedAttempts - 1, this.backoff.length - 1)] ?? 0;
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.establish();
    }, delay);
  }

  private dropSession(): void {
    const session = this.session;
    this.session = null;
    session?.close();
  }

  private clearTimer(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
}
