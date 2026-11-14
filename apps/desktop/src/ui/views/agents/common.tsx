/** Small shared pieces of the Agents & Sessions view. Tokens only; no colour is the sole cue. */
import { useId, type ReactNode } from "react";
import type { SourceId } from "../../../sources/types";
import { Badge, Button, EmptyState, Pill, type Tone } from "../../theme";
import type { Flag } from "./inventoryModel";
import type { Load } from "./useAgentsData";

/** Same wording and test id as the shared PlaceholderBadge, drawn with the theme primitive. */
export function PlaceholderMark({ id }: { id: SourceId }) {
  return (
    <Badge tone="placeholder" data-testid={`badge-${id}`}>
      PLACEHOLDER DATA ({id})
    </Badge>
  );
}

const STATE_TONE: Record<string, Tone> = {
  Created: "neutral",
  Queued: "neutral",
  Pending: "neutral",
  Spawning: "info",
  Running: "info",
  Verifying: "info",
  CandidateReady: "info",
  Succeeded: "ok",
  Exited: "neutral",
  Cancelled: "warn",
  Aborted: "bad",
  Failed: "bad",
  VerificationFailed: "bad",
};

export function StatePill({ state }: { state: string }) {
  return <Pill tone={STATE_TONE[state] ?? "neutral"}>{state}</Pill>;
}

const FLAG_TEXT: Record<Flag, { tone: Tone; text: string }> = {
  yes: { tone: "ok", text: "Yes" },
  no: { tone: "neutral", text: "No" },
  unknown: { tone: "warn", text: "Unknown" },
};

export function FlagPill({ flag }: { flag: Flag }) {
  const { tone, text } = FLAG_TEXT[flag];
  return (
    <Pill tone={tone} className="agents-flag">
      {text}
    </Pill>
  );
}

/** A control the daemon cannot honour: disabled, with the reason always visible and linked. */
export function DisabledAction({ label, reason }: { label: string; reason: string }) {
  const id = useId();
  return (
    <div className="agents-action">
      <Button disabled aria-describedby={id}>
        {label}
      </Button>
      <span id={id} className="agents-reason">
        {reason}
      </span>
    </div>
  );
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
