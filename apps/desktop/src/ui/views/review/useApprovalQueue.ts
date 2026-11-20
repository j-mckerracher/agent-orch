// The approvals behind the Review queue: the pending ones from `approvals.list` split into candidates this view can
// decide (delivery) and the rest, plus the decisions this client already resolved (the daemon lists pending only).
import { useCallback, useEffect, useState, useSyncExternalStore } from "react";
import { asDaemonError, type DaemonErrorKind } from "../../../daemon/errors";
import type { Approval } from "../../../daemon/types";
import type { ReviewClient } from "../../../review/client";
import type { ReviewSession } from "../../../review/reviewSession";

export type ApprovalQueue =
  | { status: "loading" }
  | { status: "error"; kind: DaemonErrorKind; message: string }
  | { status: "ready"; candidates: Approval[]; other: Approval[] };

const LOADING: ApprovalQueue = { status: "loading" };

/** `refreshKey` changes when the shell's own lists change, so the queue is read again with them. */
export function useApprovalQueue(client: ReviewClient, session: ReviewSession, refreshKey: string) {
  const { resolved } = useSyncExternalStore(session.subscribe, session.state);
  const [queue, setQueue] = useState<ApprovalQueue>(LOADING);
  const [nonce, setNonce] = useState(0);
  const refresh = useCallback(() => setNonce((value) => value + 1), []);

  useEffect(() => {
    let active = true;
    client.call<Approval[]>("approvals.list", {}).then(
      (all) => {
        if (!active) return;
        setQueue({
          status: "ready",
          candidates: all.filter((a) => a.action.kind.type === "delivery"),
          other: all.filter((a) => a.action.kind.type !== "delivery"),
        });
      },
      (error: unknown) => {
        if (!active) return;
        const e = asDaemonError(error);
        setQueue({ status: "error", kind: e.kind, message: e.message });
      },
    );
    return () => {
      active = false;
    };
  }, [client, refreshKey, nonce]);

  return { queue, resolved: Object.values(resolved), refresh };
}
