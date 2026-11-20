// The watched pull requests behind the Review queue's PR rows (DPES-1105 Phase 4c). Nothing here merges: a merge is
// proposed by the orchestrator and approved on its card. Titles, branches and check names are remote text and are
// shown as plain text only: no markup, no link, no attribute built from them.
import { useCallback, useEffect, useRef, useState } from "react";
import { asDaemonError } from "../../../daemon/errors";
import { notOk, observation } from "../../../sources/surfaces/result";
import { PULL_REFRESH_COOLDOWN_MS } from "../../../sources/surfaces/pulls";
import type { PullItem } from "../../../sources/surfaces/types";
import { type Action, type Outcome, useAction } from "../../action";
import { useGate, useServices, useSurface } from "../../services";
import { useRetained } from "../workspace/useRetained";
import { checksSummary, pullsErrorText } from "./pullsModel";

/** Read again while the Review view is open. */
export const PULLS_POLL_MS = 30_000;

export interface PullsModel {
  /** False for a live daemon without the `landing` capability: no PR rows, no error. */
  supported: boolean;
  loading: boolean;
  /** Why the list could not be read; null while it can. */
  unavailable: string | null;
  rows: PullItem[];
  fixture: boolean;
  /** The task whose pull request is being read now; null when idle. */
  busy: string | null;
  /** Tasks whose pull request was read a moment ago: the daemon allows one fresh read per task every 10 seconds. */
  cooling: ReadonlySet<string>;
  /** The last refresh: pending, then what the daemon read or why it failed. Render it with ActionStatus. */
  action: Action;
  refresh: (pull: PullItem) => Promise<void>;
}

const NO_ROWS: PullItem[] = [];

export function usePulls(): PullsModel {
  const { surfaces } = useServices();
  const gate = useGate();
  const source = surfaces.pulls;
  const live = source.provenance.kind === "daemon";
  // Read from the negotiated list, not `gate.can`, so the section does not flicker while the connection restarts.
  const supported = !live || gate.connection.capabilities.includes("landing");
  const [nonce, setNonce] = useState(0);
  const [busy, setBusy] = useState<string | null>(null);
  const [cooling, setCooling] = useState<ReadonlySet<string>>(new Set());
  const action = useAction();
  const [fresh, setFresh] = useState<{ base: unknown; rows: Readonly<Record<string, PullItem>> } | null>(null);
  const timers = useRef(new Set<ReturnType<typeof setTimeout>>());
  useEffect(
    () => () => {
      for (const timer of timers.current) clearTimeout(timer);
    },
    [],
  );

  const loaded = useSurface(
    () =>
      supported
        ? source.list()
        : Promise.resolve(notOk<PullItem[]>(source.provenance, "unsupported", "This daemon does not watch pull requests.", observation(Date.now()))),
    `pulls#${nonce}`,
    source.provenance,
    supported ? { pollMs: PULLS_POLL_MS, onFocus: true } : undefined,
  );
  const result = useRetained(loaded, "pulls");

  const refresh = useCallback(
    async (pull: PullItem) => {
      if (busy !== null) return;
      setBusy(pull.taskId);
      // The daemon allows one fresh read per task every 10 seconds, so the button waits as long, whatever the answer.
      setCooling((current) => new Set(current).add(pull.taskId));
      const timer = setTimeout(() => {
        timers.current.delete(timer);
        setCooling((current) => {
          const next = new Set(current);
          next.delete(pull.taskId);
          return next;
        });
      }, PULL_REFRESH_COOLDOWN_MS);
      timers.current.add(timer);
      await action.run("Reading the pull request…", async (): Promise<Outcome> => {
        try {
          const row = await source.refresh(pull.taskId);
          setFresh((current) => ({
            base: result,
            rows: { ...(current !== null && current.base === result ? current.rows : {}), [row.taskId]: row },
          }));
          setNonce((value) => value + 1);
          return { kind: "saved", text: `Read pull request ${row.number} again: ${checksSummary(row)}.` };
        } catch (error) {
          throw new Error(pullsErrorText(asDaemonError(error)));
        } finally {
          setBusy(null);
        }
      });
    },
    [busy, result, source, action.run],
  );

  const rows =
    supported && result.status === "ok"
      ? result.data.map((pull) => (fresh !== null && fresh.base === result ? fresh.rows[pull.taskId] : undefined) ?? pull)
      : NO_ROWS;
  return {
    supported: supported && result.status !== "unsupported",
    loading: result.status === "loading",
    unavailable: result.status === "unavailable" || result.status === "unknown" ? result.reason : null,
    rows,
    fixture: !live,
    busy,
    cooling,
    action,
    refresh,
  };
}
