/** Polling of `sessions.thinking` for one session. Keeps every chunk: the UI never caps the text. */
import { useEffect, useRef, useState } from "react";
import { asDaemonError, isMissingFeature, type DaemonError } from "../../../daemon/errors";
import { useServices, useStore } from "../../services";

export const THINKING_POLL_MS = 2000;

export interface SessionThinkingState {
  /** False until the first answer (or failure) arrives. */
  loaded: boolean;
  /** Every chunk read so far, joined in order. */
  text: string;
  /** The daemon's size cap cut the log short. */
  truncated: boolean;
  error: DaemonError | null;
}

const INITIAL: SessionThinkingState = { loaded: false, text: "", truncated: false, error: null };

/** `poll` keeps refreshing while true; when it turns false the log is read once more, then left alone. */
export function useSessionThinking(sessionId: string, poll: boolean): SessionThinkingState {
  const { connection } = useServices();
  const connected = useStore(connection).status === "connected";
  const [state, setState] = useState<SessionThinkingState>(INITIAL);
  // Survives the effect re-running (connection or `poll` changes) so a re-read continues from the cursor.
  const read = useRef({ cursor: 0, text: "", truncated: false });

  useEffect(() => {
    if (!connected) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    const tick = async () => {
      let again = poll;
      try {
        // Drain every page the daemon has now; a page that does not move the cursor ends it.
        for (;;) {
          const page = await connection.api().sessionsThinking(sessionId, { after: read.current.cursor });
          if (cancelled) return;
          const truncated = read.current.truncated || page.truncated;
          // A page that did not move the cursor is one already read: keep none of its chunks.
          const moved = page.next_cursor > read.current.cursor;
          read.current = {
            cursor: moved ? page.next_cursor : read.current.cursor,
            text: moved ? read.current.text + page.chunks.map((chunk) => chunk.body).join("") : read.current.text,
            truncated,
          };
          if (!moved) break;
        }
        const { text, truncated } = read.current;
        setState({ loaded: true, text, truncated, error: null });
      } catch (error) {
        if (cancelled) return;
        const failure = asDaemonError(error);
        // An older daemon has no thinking log: show it as empty and stop asking.
        if (isMissingFeature(failure)) again = false;
        setState((prev) => ({ ...prev, loaded: true, error: isMissingFeature(failure) ? null : failure }));
      }
      if (again) timer = setTimeout(() => void tick(), THINKING_POLL_MS);
    };

    void tick();
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [connection, connected, sessionId, poll]);

  return state;
}
