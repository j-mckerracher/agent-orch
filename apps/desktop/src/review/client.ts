// The S6 view of the daemon: a narrow port over the shared M5-03 connection
// (ConnectionManager + DaemonApi). There is one daemon client layer; this adapter
// only restricts it to the six methods the review screen uses. The "fixture"
// provenance is served by fixtureDaemon.ts and never reaches a daemon.
import type { ConnectionManager, ConnectionSnapshot, ConnectionStatus } from "../daemon/connection";
import { DaemonError, type DaemonErrorKind } from "../daemon/errors";
import type { DaemonEvent, DeliveryRequest, ResolveDecision, WorkspaceDiffRequest } from "../daemon/types";

export interface ReviewConnection {
  status: ConnectionStatus;
  /** Set after ReplayGap/SlowConsumer until a fresh baseline is established. */
  historyIncomplete: boolean;
}

export type ReviewNotice =
  | { type: "connection"; state: ReviewConnection }
  | { type: "event"; event: DaemonEvent }
  | { type: "events_error"; kind: DaemonErrorKind };

export interface ReviewClient {
  /** "fixture" clients serve in-memory PLACEHOLDER DATA and never reach a daemon. */
  readonly provenance: "daemon" | "fixture";
  /** Rejects with DaemonError. Mutations must pass `idempotency_key` in params. */
  call<T>(method: string, params?: Record<string, unknown>): Promise<T>;
  /** Snapshot; the same object is returned until the state changes. */
  connection(): ReviewConnection;
  subscribe(listener: (notice: ReviewNotice) => void): () => void;
}

const str = (value: unknown): string => String(value);

export function createReviewClient(manager: ConnectionManager): ReviewClient {
  let last: { snapshot: ConnectionSnapshot; state: ReviewConnection } | null = null;
  const connection = (): ReviewConnection => {
    const snapshot = manager.getSnapshot();
    if (!last || last.snapshot !== snapshot) {
      last = { snapshot, state: { status: snapshot.status, historyIncomplete: snapshot.history.incomplete } };
    }
    return last.state;
  };

  return {
    provenance: "daemon",
    async call<T>(method: string, params: Record<string, unknown> = {}): Promise<T> {
      const api = manager.api();
      switch (method) {
        case "approvals.list":
          return (await api.approvalsList()) as T;
        case "tasks.list":
          return (await api.tasksList()) as T;
        case "tasks.get":
          return (await api.tasksGet(str(params.task_id))) as T;
        case "workspaces.diff":
          return (await api.workspacesDiff(params as unknown as WorkspaceDiffRequest)) as T;
        case "approvals.resolve":
          return (await api.approvalsResolve(
            str(params.approval_id),
            params.decision as ResolveDecision,
            str(params.idempotency_key),
          )) as T;
        case "delivery.request": {
          const { idempotency_key, ...request } = params;
          return (await api.deliveryRequest(request as unknown as DeliveryRequest, str(idempotency_key))) as T;
        }
        default:
          throw new DaemonError("method-unavailable", `${method} is not used by the review screen`);
      }
    },
    connection,
    subscribe(listener) {
      let incomplete = manager.getSnapshot().history.incomplete;
      const offState = manager.subscribe(() => {
        const state = connection();
        listener({ type: "connection", state });
        const reason = manager.getSnapshot().history.reason;
        if (state.historyIncomplete && !incomplete && reason) listener({ type: "events_error", kind: reason });
        incomplete = state.historyIncomplete;
      });
      const offEvent = manager.onEvent((event) => listener({ type: "event", event }));
      return () => {
        offState();
        offEvent();
      };
    },
  };
}
