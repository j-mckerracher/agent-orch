/** Small shared pieces of the Agents view and Settings > Agents. Tokens only; no color is the sole cue. */
import type { ReactNode } from "react";
import { stateLabel } from "../../format";
import { Button, EmptyState, Pill, type Tone } from "../../theme";
import type { Load } from "./useAgentsData";

const STATE_TONE: Record<string, Tone> = {
  Created: "neutral",
  Queued: "neutral",
  Pending: "neutral",
  Spawning: "info",
  Running: "info",
  Waiting: "warn",
  Verifying: "info",
  CandidateReady: "info",
  Succeeded: "ok",
  Exited: "neutral",
  Cancelled: "warn",
  Aborted: "bad",
  Failed: "bad",
  VerificationFailed: "bad",
};

export const stateTone = (state: string): Tone => STATE_TONE[state] ?? "neutral";

const SPACED: Record<string, string> = { VerificationFailed: "Verification failed", CandidateReady: "Candidate ready" };

export function StatePill({ state }: { state: string }) {
  return <Pill tone={stateTone(state)}>{SPACED[state] ?? stateLabel(state)}</Pill>;
}

export function LoadRegion<T>({
  what,
  load,
  emptyTitle,
  emptyDescription,
  onRetry,
  children,
}: {
  what: string;
  load: Load<T>;
  emptyTitle: string;
  emptyDescription?: string;
  onRetry?: () => void;
  children: (data: T) => ReactNode;
}) {
  const retry = onRetry && <Button onClick={onRetry}>Retry</Button>;
  switch (load.status) {
    case "loading":
      return <p role="status">Loading {what}…</p>;
    case "empty":
      return <EmptyState title={emptyTitle} description={emptyDescription} />;
    case "unavailable":
      return <EmptyState role="status" title={`${what} unavailable`} description={load.reason} action={retry} />;
    case "error":
      return <EmptyState role="alert" title={`${what} could not be loaded`} description={load.message} action={retry} />;
    case "ready":
      return (
        <>
          {load.stale && <p role="status">{load.stale}</p>}
          {children(load.data)}
        </>
      );
  }
}
