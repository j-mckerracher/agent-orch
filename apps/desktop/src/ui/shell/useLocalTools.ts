import { useCallback, useEffect, useRef, useState } from "react";
import type { LocalToolsSnapshot, LocalToolsSource, SurfaceResult } from "../../sources/surfaces/types";
import { useServices, useStore } from "../services";

/** The daemon answers from its cache, so re-reading this often shows a first answer or a finished refresh soon. */
export const LOCAL_TOOLS_POLL_MS = 15_000;

export interface LocalToolsState {
  /** Null until the first read settles. */
  result: SurfaceResult<LocalToolsSnapshot> | null;
  /** The opt-in value being saved. While it is off, every figure is hidden at once. */
  pending: boolean | null;
  /** Why the last change or refresh did not take effect; null when it did. */
  note: string | null;
  refreshing: boolean;
  setEnabled: (enabled: boolean) => void;
  refresh: () => void;
}

/**
 * Reads the other-AI-tools snapshot and runs its opt-in and refresh. It re-reads on focus, on
 * connection change, and on a short interval. Replies are applied only when they answer a newer request
 * than the one already shown, so a slow earlier read never overwrites a later answer.
 */
export function useLocalTools(source: LocalToolsSource): LocalToolsState {
  const { connection } = useServices();
  const { generation, status } = useStore(connection);
  const [result, setResult] = useState<SurfaceResult<LocalToolsSnapshot> | null>(null);
  const [pending, setPending] = useState<boolean | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [refreshCalls, setRefreshCalls] = useState(0);
  const requests = useRef({ started: 0, shown: 0 });

  const settle = useCallback((seq: number, value: SurfaceResult<LocalToolsSnapshot>) => {
    if (seq <= requests.current.shown) return;
    requests.current.shown = seq;
    setResult(value);
  }, []);

  useEffect(() => {
    const load = () => {
      const seq = ++requests.current.started;
      void source.local().then((value) => settle(seq, value));
    };
    load();
    const timer = setInterval(load, LOCAL_TOOLS_POLL_MS);
    window.addEventListener("focus", load);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", load);
    };
  }, [source, generation, status, settle]);

  /** A failed change or refresh is reported in words, and the figures shown are the daemon's current answer. */
  const exchange = useCallback(
    async (call: () => Promise<SurfaceResult<LocalToolsSnapshot>>, failure: string) => {
      const seq = ++requests.current.started;
      const value = await call();
      if (value.status === "ok") {
        setNote(null);
        settle(seq, value);
        return;
      }
      setNote(`${failure}: ${value.reason.replace(/[.\s]+$/, "")}.`);
      const reread = ++requests.current.started;
      settle(reread, await source.local());
    },
    [source, settle],
  );

  const setEnabled = useCallback(
    (enabled: boolean) => {
      setPending(enabled);
      setNote(null);
      const failure = enabled ? "The setting did not turn on" : "The setting did not turn off";
      void exchange(() => source.setEnabled(enabled), failure).finally(() => setPending(null));
    },
    [source, exchange],
  );

  const refresh = useCallback(() => {
    setRefreshCalls((calls) => calls + 1);
    setNote(null);
    void exchange(() => source.refresh(), "Refresh did not start").finally(() => setRefreshCalls((calls) => calls - 1));
  }, [source, exchange]);

  return {
    result,
    pending,
    note,
    refreshing: refreshCalls > 0 || (result?.status === "ok" && result.data.refreshing),
    setEnabled,
    refresh,
  };
}
