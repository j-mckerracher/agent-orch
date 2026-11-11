/**
 * `DaemonConnector` backed by the Tauri host bridge (`src-tauri/src/bridge.rs`).
 *
 * The host owns the transport and the token: it connects to the daemon's
 * socket/pipe, authenticates, negotiates capabilities, and forwards JSON-RPC
 * frames. The webview only ever sees frames and the negotiated `welcome`.
 * Closing a channel closes this window's connection, never the daemon.
 */
import { Channel, invoke } from "@tauri-apps/api/core";
import { DaemonError, type DaemonErrorKind } from "../daemon/errors";
import type { DaemonConnector, OpenedChannel, RpcChannel } from "../daemon/session";
import type { Capability, Welcome } from "../daemon/types";

export interface TauriApi {
  invoke(command: string, args?: Record<string, unknown>): Promise<unknown>;
  createChannel(onMessage: (payload: unknown) => void): unknown;
}

const realApi: TauriApi = {
  invoke: (command, args) => invoke(command, args),
  createChannel: (onMessage) => new Channel<unknown>(onMessage),
};

const HOST_KIND: Record<string, DaemonErrorKind> = {
  "daemon-unavailable": "daemon-unavailable",
  auth: "auth",
  version: "version",
  "capability-unavailable": "capability-unavailable",
  protocol: "protocol",
  io: "disconnected",
  closed: "disconnected",
};

/** Maps a host `{code, message}` failure (or any rejection) to a typed error. */
export function hostError(failure: unknown): DaemonError {
  if (typeof failure === "object" && failure !== null) {
    const { code, message } = failure as { code?: unknown; message?: unknown };
    if (typeof code === "string") {
      return new DaemonError(HOST_KIND[code] ?? "protocol", typeof message === "string" ? message : code);
    }
  }
  return new DaemonError("protocol", typeof failure === "string" ? failure : "the native bridge failed");
}

const CLOSE_REASON: Record<string, DaemonError | null> = {
  local: null,
  remote: new DaemonError("disconnected", "the daemon closed the connection"),
  protocol: new DaemonError("protocol", "the daemon sent an invalid frame"),
  io: new DaemonError("disconnected", "the connection to the daemon failed"),
};

class BridgeChannel implements RpcChannel {
  private messageListener: ((message: unknown) => void) | null = null;
  private closeListener: ((reason: DaemonError | null) => void) | null = null;
  private readonly queued: unknown[] = [];
  private closed = false;
  private pendingClose: { reason: DaemonError | null } | null = null;
  connectionId = -1;

  constructor(private readonly api: TauriApi) {}

  /** Frames from the host Channel. */
  receive = (payload: unknown): void => {
    const frame = payload as { kind?: unknown; message?: unknown; reason?: unknown } | null;
    if (frame?.kind === "message") {
      if (this.messageListener) this.messageListener(frame.message);
      else this.queued.push(frame.message);
    } else if (frame?.kind === "closed") {
      const reason = CLOSE_REASON[String(frame.reason)] ?? new DaemonError("disconnected", "connection closed");
      this.finish(reason);
    }
  };

  private finish(reason: DaemonError | null): void {
    if (this.closed) return;
    this.closed = true;
    if (this.closeListener) this.closeListener(reason);
    else this.pendingClose = { reason };
  }

  send(message: unknown): void {
    if (this.closed) throw new DaemonError("disconnected", "channel closed");
    this.api
      .invoke("daemon_send", { connectionId: this.connectionId, message })
      .catch((failure) => this.finish(hostError(failure)));
  }

  onMessage(listener: (message: unknown) => void): void {
    this.messageListener = listener;
    for (const message of this.queued.splice(0)) listener(message);
  }

  onClose(listener: (reason: DaemonError | null) => void): void {
    this.closeListener = listener;
    if (this.pendingClose) {
      const { reason } = this.pendingClose;
      this.pendingClose = null;
      listener(reason);
    }
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;
    void this.api.invoke("daemon_close", { connectionId: this.connectionId }).catch(() => undefined);
    this.closeListener?.(null);
  }
}

function asWelcome(value: unknown): Welcome {
  const welcome = value as Partial<Welcome> | null;
  if (!welcome || typeof welcome.version !== "number" || !Array.isArray(welcome.capabilities)) {
    throw new DaemonError("protocol", "the native bridge returned a malformed welcome");
  }
  return welcome as Welcome;
}

export function createTauriConnector(api: TauriApi = realApi): DaemonConnector {
  return {
    async open(requested: readonly Capability[]): Promise<OpenedChannel> {
      const channel = new BridgeChannel(api);
      let opened: unknown;
      try {
        opened = await api.invoke("daemon_open", {
          requested: [...requested],
          onFrame: api.createChannel(channel.receive),
        });
      } catch (failure) {
        throw hostError(failure);
      }
      const { connectionId, welcome } = opened as { connectionId?: unknown; welcome?: unknown };
      if (typeof connectionId !== "number") {
        throw new DaemonError("protocol", "the native bridge returned a malformed connection");
      }
      channel.connectionId = connectionId;
      try {
        return { channel, welcome: asWelcome(welcome) };
      } catch (failure) {
        channel.close();
        throw failure;
      }
    },
  };
}

/** Whether this page runs inside the Tauri webview (the host injects this global). */
export function insideTauri(target: object = window): boolean {
  return "__TAURI_INTERNALS__" in target;
}

/** Used outside the desktop app: there is no bridge, so the app says so rather than showing sample data. */
export const noBridgeConnector: DaemonConnector = {
  open: () =>
    Promise.reject(
      new DaemonError(
        "daemon-unavailable",
        "this page is not running inside the desktop app, so there is no bridge to the daemon",
      ),
    ),
};
