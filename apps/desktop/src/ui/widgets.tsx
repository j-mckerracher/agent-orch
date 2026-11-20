import { useEffect, useState, type HTMLAttributes, type ReactNode } from "react";
import { presentError } from "../daemon/errors";
import type { DaemonError } from "../daemon/errors";
import type { Loadable } from "../daemon/appData";
import type { Provenance } from "../sources/types";
import type { SourceState } from "./services";
import { Badge, Button, EmptyState, Pill, type Tone } from "./theme";
import { stateLabel } from "./format";
import "./projectScreens.css";

const NOTICE_MARK: Record<Tone, string> = { neutral: "–", info: "i", ok: "✓", warn: "!", bad: "✕" };

/** Inline notice. The glyph and the text carry the meaning; color only reinforces it. */
export function Notice({
  tone = "warn",
  className,
  children,
  ...rest
}: { tone?: Tone } & Omit<HTMLAttributes<HTMLParagraphElement>, "children"> & { children: ReactNode }) {
  // A paragraph, as before the restyle; children must stay phrasing content.
  return (
    <p className={`ps-notice ps-notice-${tone}${className ? ` ${className}` : ""}`} {...rest}>
      <span className="ps-notice-mark" aria-hidden="true">
        {NOTICE_MARK[tone]}
      </span>
      <span className="ps-notice-body">{children}</span>
    </p>
  );
}

const STATE_TONE: Record<string, Tone> = {
  Succeeded: "ok",
  Running: "info",
  Waiting: "warn",
  Verifying: "info",
  Spawning: "info",
  Failed: "bad",
  VerificationFailed: "bad",
  Aborted: "bad",
  Cancelled: "neutral",
  Exited: "neutral",
  Created: "neutral",
  Queued: "neutral",
};

/** Task, attempt or session state as text plus a glyph; unknown states render neutral. */
export function StateText({ state }: { state: string }) {
  return <Pill tone={STATE_TONE[state] ?? "neutral"}>{stateLabel(state)}</Pill>;
}

/** Wraps a table in a bordered, horizontally scrollable region. */
export function TableWrap({ children }: { children: ReactNode }) {
  return <div className="ps-table-wrap">{children}</div>;
}

/**
 * Loading / error / offline / empty / ready for a REAL snapshot. Empty is
 * distinct from offline and error; cached data is labeled stale when the
 * connection is not live.
 */
export function LoadableRegion<T>({
  what,
  loadable,
  connected,
  isEmpty,
  emptyText,
  onRetry,
  children,
}: {
  what: string;
  loadable: Loadable<T>;
  connected: boolean;
  isEmpty: (data: T) => boolean;
  emptyText: string;
  onRetry: () => void;
  children: (data: T) => ReactNode;
}) {
  const { data, status, error } = loadable;
  // Retry starts a read. While it runs, the error notice is out of the way, so a line says the read is in flight.
  const [asked, setAsked] = useState(false);
  useEffect(() => {
    if (status !== "loading") setAsked(false);
  }, [status]);
  const retry = (
    <Button
      onClick={() => {
        setAsked(true);
        onRetry();
      }}
      disabled={!connected}
    >
      Retry
    </Button>
  );
  if (data === null) {
    if (status === "error" && error) {
      return (
        <div>
          <ErrorNotice error={error} action={`Loading ${what}`} /> {retry}
        </div>
      );
    }
    if (!connected && status !== "loading") {
      return (
        <Notice tone="bad" role="status">
          Offline: no cached {what}. This is not an empty list.
        </Notice>
      );
    }
    return <p role="status">Loading {what}…</p>;
  }
  return (
    <div>
      {!connected && (
        <Notice tone="warn" role="status" data-testid={`stale-${what}`}>
          Stale: cached {what} from before the connection was lost.
        </Notice>
      )}
      {status === "error" && error && (
        <div>
          <ErrorNotice error={error} action={`Refreshing ${what}`} /> {retry}
        </div>
      )}
      {status === "loading" && asked && <p role="status">Refreshing {what}…</p>}
      {isEmpty(data) ? <EmptyState title={emptyText} /> : children(data)}
    </div>
  );
}

/** `compact` drops "DATA" for a narrow column; the source id and tone stay the same. */
export function PlaceholderBadge({ provenance, compact = false }: { provenance: Provenance; compact?: boolean }) {
  const fixture = provenance.kind === "fixture";
  const label = fixture ? (compact ? "PLACEHOLDER" : "PLACEHOLDER DATA") : "DAEMON";
  return (
    <Badge tone={fixture ? "placeholder" : "info"} data-testid={`badge-${provenance.id}`}>
      {label} ({provenance.id})
    </Badge>
  );
}

/** Receipts of simulated DUMMY actions: always labeled SIMULATED and with the source's PLACEHOLDER DATA badge. */
export function SimulatedReceipts({
  label,
  receipts,
}: {
  label: string;
  receipts: Array<{ provenance: Provenance; notice: string }>;
}) {
  return (
    <ul aria-label={label} className="ps-list">
      {receipts.map((receipt, index) => (
        <li key={index} data-testid="simulated-receipt">
          <Badge tone="placeholder">SIMULATED</Badge> <PlaceholderBadge provenance={receipt.provenance} />{" "}
          {receipt.notice}
        </li>
      ))}
    </ul>
  );
}

/** Inline error: reason, affected action, safe next step. */
/** The reason and next step ErrorNotice shows, as plain text for an action's Outcome or thrown Error (DPES-1170). */
export function failureText(action: string, error: DaemonError): string {
  const { reason, nextStep } = presentError(error);
  const unknown = error.indeterminate ? " The outcome is unknown; trying again reuses the same request key." : "";
  return `${action} failed. ${reason} ${nextStep}${unknown}`;
}

export function ErrorNotice({ error, action }: { error: DaemonError; action: string }) {
  const { reason, nextStep } = presentError(error);
  return (
    <Notice tone="bad" role="alert">
      <strong>{action} failed.</strong> {reason} <span>{nextStep}</span>
      <span className="ps-error-detail"> ({error.kind}{error.code !== null ? ` ${error.code}` : ""}: {error.message})</span>
    </Notice>
  );
}

/** Wraps a D1-D6 source: label, loading, empty, unavailable and error states. A daemon source drops the placeholder wording. */
export function SourceSection<T>({
  title,
  provenance,
  result,
  emptyText,
  children,
}: {
  title: string;
  provenance: Provenance;
  result: SourceState<T>;
  emptyText: string;
  children: (data: T) => ReactNode;
}) {
  let body: ReactNode;
  const live = provenance.kind === "daemon";
  switch (result.state) {
    case "loading":
      body = <p role="status">{live ? "Loading…" : "Loading placeholder data…"}</p>;
      break;
    case "empty":
      body = <p>{emptyText}</p>;
      break;
    case "unavailable":
      body = <p role="status">Unavailable: {result.reason}</p>;
      break;
    case "error":
      body = <p role="alert">{live ? "Source error" : "Placeholder source error"}: {result.message}</p>;
      break;
    case "ready":
      body = children(result.data);
      break;
  }
  return (
    <section
      className={live ? "ps-section" : "ps-section ps-section-placeholder"}
      aria-label={live ? title : `${title} (placeholder)`}
    >
      <h3>
        {title} <PlaceholderBadge provenance={provenance} />
      </h3>
      {body}
    </section>
  );
}
