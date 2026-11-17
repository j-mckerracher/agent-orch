/** Bounded, read-only polling of `sessions.output_tail` for one session. Never overlaps requests. */
import { useEffect, useState } from "react";
import { asDaemonError, type DaemonError } from "../../../daemon/errors";
import type { OutputLine } from "../../../daemon/types";
import { useServices, useStore } from "../../services";

export const OUTPUT_POLL_MS = 2000;
export const OUTPUT_UI_LINES = 200;
const FETCH_LIMIT = 100;

export interface OutputTailState {
  /** False until the first answer (or failure) arrives. */
  loaded: boolean;
  available: boolean;
  ended: boolean;
  /** Newest lines, oldest first, never more than `OUTPUT_UI_LINES`. */
  lines: OutputLine[];
  /** Some older lines were never shown: dropped by the daemon ring, beyond one page, or beyond the UI cap. */
  olderDropped: boolean;
  error: DaemonError | null;
}

const INITIAL: OutputTailState = { loaded: false, available: false, ended: false, lines: [], olderDropped: false, error: null };

/** `poll` keeps refreshing while true; otherwise the tail is read once (and once more when `poll` turns false). */
export function useOutputTail(taskId: string, sessionId: string, poll: boolean): OutputTailState {
  const { connection } = useServices();
  const connected = useStore(connection).status === "connected";
  const [state, setState] = useState<OutputTailState>(INITIAL);

  useEffect(() => {
    if (!connected) return;
    let cancelled = false;
    let inFlight = false;
    let fetched = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let cursor = 0;
    let ended = false;
    let shown: OutputLine[] = [];
    let olderDropped = false;
    const hidden = () => typeof document !== "undefined" && document.visibilityState === "hidden";

    const schedule = () => {
      if (!cancelled && poll && !ended) timer = setTimeout(() => void tick(), OUTPUT_POLL_MS);
    };

    const tick = async () => {
      timer = undefined;
      if (cancelled || inFlight || hidden()) return;
      inFlight = true;
      try {
        const result = await connection
          .api()
          .sessionsOutputTail(taskId, sessionId, { limit: FETCH_LIMIT, ...(cursor > 0 ? { afterSequence: cursor } : {}) });
        if (cancelled) return;
        fetched = true;
        ended = result.ended;
        if (!result.available) {
          cursor = 0;
          shown = [];
          olderDropped = false;
          setState({ loaded: true, available: false, ended: result.ended, lines: [], olderDropped: false, error: null });
        } else {
          // A cursor ahead of the daemon's means its output restarted: drop what is shown.
          if (result.last_sequence < cursor) {
            shown = [];
            olderDropped = false;
          }
          const known = shown.length > 0 ? shown[shown.length - 1]!.sequence : 0;
          const merged = [...shown, ...result.lines.filter((line) => line.sequence > known)];
          shown = merged.slice(Math.max(0, merged.length - OUTPUT_UI_LINES));
          olderDropped = olderDropped || result.truncated_lines || shown.length < merged.length;
          cursor = result.last_sequence;
          setState({ loaded: true, available: true, ended: result.ended, lines: shown, olderDropped, error: null });
        }
      } catch (error) {
        if (cancelled) return;
        fetched = true;
        setState((prev) => ({ ...prev, loaded: true, error: asDaemonError(error) }));
      } finally {
        inFlight = false;
      }
      schedule();
    };

    const onVisibility = () => {
      if (cancelled || hidden() || inFlight || timer !== undefined) return;
      if (!fetched || (poll && !ended)) void tick();
    };

    document.addEventListener("visibilitychange", onVisibility);
    void tick();
    return () => {
      cancelled = true;
      if (timer !== undefined) clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [connection, connected, taskId, sessionId, poll]);

  return state;
}
