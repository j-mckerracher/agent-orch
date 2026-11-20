/**
 * Polling of one session's activity: its thinking and tool calls in order, and what it is doing now. Reads
 * `sessions.activity` by cursor; a daemon without it gets `sessions.thinking`, which shows the thinking text alone.
 */
import { useEffect, useRef, useState } from "react";
import { addActivity, mapLive, type ToolSlot } from "../../../sources/surfaces/activityItems";
import type { ActivityItem, LiveActivity } from "../../../sources/surfaces/types";
import { asDaemonError, isMissingFeature, type DaemonError } from "../../../daemon/errors";
import type { SessionActivityPage } from "../../../daemon/types";
import { useServices, useStore } from "../../services";

export const ACTIVITY_POLL_MS = 1000;

export interface SessionActivityState {
  /** False until the first answer (or failure) arrives. */
  loaded: boolean;
  /** Every thinking and tool item read so far, in order. */
  items: readonly ActivityItem[];
  /** What the session is doing now; null once it is not running and from a daemon without `sessions.activity`. */
  live: LiveActivity | null;
  /** The daemon's thinking size cap cut the log short. */
  truncated: boolean;
  error: DaemonError | null;
}

const INITIAL: SessionActivityState = { loaded: false, items: [], live: null, truncated: false, error: null };

interface Read {
  cursor: number;
  /** Built in place: a tool's finish row updates its start's slot, so the array a slot points at must stay the same. */
  items: ActivityItem[];
  /** A copy of `items` taken when it last changed, so an unchanged read keeps the identity the view memoizes on. */
  shown: readonly ActivityItem[];
  tools: Map<string, ToolSlot>;
  truncated: boolean;
  legacy: boolean;
}

/** `poll` keeps refreshing while true; when it turns false the log is read once more, then left alone. */
export function useSessionActivity(sessionId: string, poll: boolean): SessionActivityState {
  const { connection } = useServices();
  const connected = useStore(connection).status === "connected";
  const [state, setState] = useState<SessionActivityState>(INITIAL);
  // Survives the effect re-running (connection or `poll` changes) so a re-read continues from the cursor.
  const read = useRef<Read>({ cursor: 0, items: [], shown: [], tools: new Map(), truncated: false, legacy: false });

  // Without a connection the snapshot is stale, and its idle time would keep climbing on screen.
  useEffect(() => {
    if (!connected) setState((prev) => (prev.live === null ? prev : { ...prev, live: null }));
  }, [connected]);

  useEffect(() => {
    if (!connected) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    const page = async (): Promise<SessionActivityPage> => {
      const api = connection.api();
      const after = read.current.cursor;
      if (!read.current.legacy) {
        try {
          return await api.sessionsActivity(sessionId, { after });
        } catch (error) {
          if (!isMissingFeature(asDaemonError(error))) throw error;
          read.current.legacy = true;
        }
      }
      const thinking = await api.sessionsThinking(sessionId, { after });
      return {
        entries: thinking.chunks.map((chunk) => ({ sequence: chunk.sequence, kind: "thinking", body: chunk.body })),
        next_cursor: thinking.next_cursor,
        truncated: thinking.truncated,
        live: null,
      };
    };

    const tick = async () => {
      let again = poll;
      try {
        let live: LiveActivity | null = null;
        // Drain every page the daemon has now; a page that does not move the cursor ends it.
        for (;;) {
          const result = await page();
          if (cancelled) return;
          const now = read.current;
          now.truncated ||= result.truncated;
          live = result.live ? mapLive(result.live, Date.now()) : null;
          // A page that did not move the cursor is one already read: keep none of its entries.
          if (result.next_cursor <= now.cursor) break;
          now.cursor = result.next_cursor;
          for (const entry of result.entries) addActivity(now.items, now.tools, entry.sequence, entry.kind, entry.body);
          now.shown = now.items.slice();
        }
        setState({ loaded: true, items: read.current.shown, live, truncated: read.current.truncated, error: null });
      } catch (error) {
        if (cancelled) return;
        const failure = asDaemonError(error);
        // A daemon with no thinking log either: show it as empty and stop asking.
        if (isMissingFeature(failure)) again = false;
        setState((prev) => ({ ...prev, loaded: true, error: isMissingFeature(failure) ? null : failure }));
      }
      if (again) timer = setTimeout(() => void tick(), ACTIVITY_POLL_MS);
    };

    void tick();
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [connection, connected, sessionId, poll]);

  return state;
}
