// The approvals behind the Review queue: the pending ones from `approvals.list` split into candidates this view can
// decide (delivery) and the rest, plus the decisions this client already resolved (the daemon lists pending only).
import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { asDaemonError, type DaemonError, type DaemonErrorKind } from "../../../daemon/errors";
import type { Approval } from "../../../daemon/types";
import type { ReviewClient } from "../../../review/client";
import type { ReviewSession } from "../../../review/reviewSession";

export type ApprovalQueue =
  | { status: "loading" }
  | { status: "error"; kind: DaemonErrorKind; message: string }
  | { status: "ready"; candidates: Approval[]; other: Approval[] };

const LOADING: ApprovalQueue = { status: "loading" };

/**
 * `refreshKey` changes when the shell's own lists change, so the queue is read again with them. `refresh` reads again
 * and resolves when the read ends; it rejects with the daemon's error, and the queue keeps showing that error too.
 */
export function useApprovalQueue(client: ReviewClient, session: ReviewSession, refreshKey: string) {
  const { resolved } = useSyncExternalStore(session.subscribe, session.state);
  const [queue, setQueue] = useState<ApprovalQueue>(LOADING);
  // Only the newest read may set the queue, so an older answer that arrives late is dropped.
  const reads = useRef(0);

  const read = useCallback(async () => {
    const current = ++reads.current;
    try {
      const all = await client.call<Approval[]>("approvals.list", {});
      if (current === reads.current) {
        setQueue({
          status: "ready",
          candidates: all.filter((a) => a.action.kind.type === "delivery"),
          other: all.filter((a) => a.action.kind.type !== "delivery"),
        });
      }
    } catch (error) {
      const e: DaemonError = asDaemonError(error);
      if (current === reads.current) setQueue({ status: "error", kind: e.kind, message: e.message });
      throw e;
    }
  }, [client]);

  useEffect(() => {
    read().catch(() => undefined);
  }, [read, refreshKey]);

  return { queue, resolved: Object.values(resolved), refresh: read };
}
