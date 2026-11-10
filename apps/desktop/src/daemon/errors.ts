/**
 * Daemon and transport failures as one typed error.
 *
 * Codes come from `crates/orchestrator-ipc` (handshake -32001..-32004,
 * -32601, -32600) and `RpcError` (-32602, -32010..-32018, -32603). Messages
 * from the daemon are sanitized and path-free; the UI still never renders a
 * token, which the webview does not hold.
 */

export type DaemonErrorKind =
  | "auth"
  | "version"
  | "capability-unavailable"
  | "capability-not-negotiated"
  | "method-unavailable"
  | "invalid-handshake"
  | "invalid-params"
  | "not-found"
  | "conflict"
  | "forbidden"
  | "idempotency-conflict"
  | "idempotency-in-progress"
  | "slow-consumer"
  | "result-too-large"
  | "replay-gap"
  | "epoch-mismatch"
  | "internal"
  | "timeout"
  | "disconnected"
  | "daemon-unavailable"
  | "protocol";

const KIND_BY_CODE: Record<number, DaemonErrorKind> = {
  [-32001]: "auth",
  [-32002]: "capability-unavailable",
  [-32003]: "version",
  [-32004]: "capability-not-negotiated",
  [-32600]: "invalid-handshake",
  [-32601]: "method-unavailable",
  [-32602]: "invalid-params",
  [-32010]: "not-found",
  [-32011]: "conflict",
  [-32012]: "forbidden",
  [-32013]: "idempotency-conflict",
  [-32014]: "idempotency-in-progress",
  [-32015]: "slow-consumer",
  [-32016]: "result-too-large",
  [-32017]: "replay-gap",
  [-32018]: "epoch-mismatch",
  [-32603]: "internal",
};

export class DaemonError extends Error {
  readonly kind: DaemonErrorKind;
  readonly code: number | null;

  constructor(kind: DaemonErrorKind, message: string, code: number | null = null) {
    super(message);
    this.name = "DaemonError";
    this.kind = kind;
    this.code = code;
  }

  static fromRpc(code: number, message: string): DaemonError {
    return new DaemonError(KIND_BY_CODE[code] ?? "protocol", message, code);
  }

  /**
   * True when the request may have been applied and the outcome is unknown,
   * so a retry of the same logical mutation must reuse its idempotency key.
   */
  get indeterminate(): boolean {
    return (
      this.kind === "timeout" ||
      this.kind === "disconnected" ||
      this.kind === "idempotency-in-progress" ||
      this.kind === "internal" ||
      this.kind === "protocol"
    );
  }

  /** Connection-level failures that make cached data stale. */
  get connectionLevel(): boolean {
    return (
      this.kind === "auth" ||
      this.kind === "version" ||
      this.kind === "capability-unavailable" ||
      this.kind === "disconnected" ||
      this.kind === "daemon-unavailable" ||
      this.kind === "invalid-handshake"
    );
  }
}

export function asDaemonError(error: unknown): DaemonError {
  if (error instanceof DaemonError) return error;
  return new DaemonError("protocol", error instanceof Error ? error.message : "unexpected failure");
}

export interface ErrorPresentation {
  reason: string;
  nextStep: string;
}

const PRESENTATION: Record<DaemonErrorKind, ErrorPresentation> = {
  auth: {
    reason: "The daemon rejected authentication.",
    nextStep: "Restart the daemon or reinstall the client, then reconnect.",
  },
  version: {
    reason: "No protocol version is shared with the daemon.",
    nextStep: "Update the desktop app or the daemon so their versions match.",
  },
  "capability-unavailable": {
    reason: "The daemon does not offer a capability this screen needs.",
    nextStep: "Update the daemon; the affected actions stay disabled.",
  },
  "capability-not-negotiated": {
    reason: "This connection did not negotiate the capability.",
    nextStep: "Reconnect to negotiate again.",
  },
  "method-unavailable": {
    reason: "The daemon does not implement this method.",
    nextStep: "No action will be taken; update the daemon.",
  },
  "invalid-handshake": {
    reason: "The daemon rejected the connection handshake.",
    nextStep: "Reconnect; if it persists, update the app and daemon together.",
  },
  "invalid-params": {
    reason: "The daemon rejected the request as invalid.",
    nextStep: "Check the entered values and try again.",
  },
  "not-found": {
    reason: "The record no longer exists.",
    nextStep: "Refresh the list.",
  },
  conflict: {
    reason: "The record is not in a state that allows this action.",
    nextStep: "Refresh to see the current state before trying again.",
  },
  forbidden: {
    reason: "The daemon refused this action.",
    nextStep: "Nothing was changed. Check the task state and policy.",
  },
  "idempotency-conflict": {
    reason: "This request key was already used for a different request.",
    nextStep: "Review the form and submit again as a new request.",
  },
  "idempotency-in-progress": {
    reason: "The same request is still being processed.",
    nextStep: "Wait, then refresh; retrying reuses the same request key.",
  },
  "slow-consumer": {
    reason: "The live event stream fell behind and was closed by the daemon.",
    nextStep: "History may be incomplete; the client resubscribes and refreshes.",
  },
  "result-too-large": {
    reason: "The result is too large to return.",
    nextStep: "Narrow the request.",
  },
  "replay-gap": {
    reason: "The saved event position is no longer valid.",
    nextStep: "History is incomplete; the client starts a fresh baseline.",
  },
  "epoch-mismatch": {
    reason: "The saved event position belongs to a different daemon store.",
    nextStep: "History is incomplete; the client starts a fresh baseline.",
  },
  internal: {
    reason: "The daemon reported an internal error.",
    nextStep: "The outcome may be unknown. Refresh before retrying.",
  },
  timeout: {
    reason: "The daemon did not answer in time.",
    nextStep: "The outcome may be unknown. Refresh before retrying.",
  },
  disconnected: {
    reason: "The connection to the daemon was lost.",
    nextStep: "Actions are paused until the client reconnects.",
  },
  "daemon-unavailable": {
    reason: "The daemon is not running and could not be started.",
    nextStep:
      "Nothing is known about running work. Check daemon.log in the data directory, then use Refresh / reconnect.",
  },
  protocol: {
    reason: "The daemon sent an unexpected response.",
    nextStep: "Reconnect; if it persists, update the app and daemon together.",
  },
};

export function presentError(error: DaemonError): ErrorPresentation {
  return PRESENTATION[error.kind];
}
