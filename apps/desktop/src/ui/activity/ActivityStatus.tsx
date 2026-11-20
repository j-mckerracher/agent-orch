/** What the agent is doing now, with a one-second elapsed time. Only this element re-renders on the tick. */
import { useEffect, useState } from "react";
import type { LiveActivity } from "../../sources/surfaces/types";
import "./activity.css";

/** No event for this long reads as a possibly hung agent. */
export const STALE_MS = 20_000;

/** The phase in words, without a time. A phase this app does not know is shown as the daemon sent it. */
export function phaseLabel(live: LiveActivity): string {
  switch (live.phase) {
    case "starting":
      return "Starting";
    case "thinking":
      return "Thinking";
    case "tool":
      return `Running ${live.tool?.name ?? "a tool"}`;
    case "replying":
      return "Replying";
    default:
      return live.phase;
  }
}

function useNow(): number {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);
  return now;
}

/** What to show from a daemon that sends no snapshot: a fixed line and when this app began waiting. */
export interface StatusFallback {
  text: string;
  since: number;
}

/**
 * `live` is the daemon's snapshot. The daemon's durations are measured when it answered, so the time since
 * `receivedAt` is added; the daemon's clock is never compared with this one. Without a snapshot `fallback` is
 * shown with the time since it began, and with neither nothing is shown.
 */
export function ActivityStatus({ live, fallback }: { live: LiveActivity | null; fallback?: StatusFallback }) {
  const now = useNow();
  if (live === null && !fallback) return null;
  const sinceReceived = live ? Math.max(0, now - live.receivedAt) : 0;
  const stale = live !== null && live.idleMs + sinceReceived >= STALE_MS;
  const elapsed = live ? (stale ? live.idleMs : live.phaseMs) + sinceReceived : Math.max(0, now - fallback!.since);
  const seconds = Math.floor(elapsed / 1000);
  return (
    <p className="act-status" data-stale={stale || undefined}>
      {/* The words are announced when the phase changes; the ticking number is hidden from readers. */}
      <span role="status" aria-live="polite">
        {live === null ? fallback!.text : stale ? "No activity" : phaseLabel(live)}
      </span>
      <span aria-hidden="true">{stale ? ` for ${seconds} s` : ` · ${seconds} s`}</span>
    </p>
  );
}
