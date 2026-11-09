import type { ReactNode } from "react";
import { presentError } from "../daemon/errors";
import type { DaemonError } from "../daemon/errors";
import type { Loadable } from "../daemon/appData";
import type { Provenance } from "../sources/types";
import type { SourceState } from "./services";

/**
 * Loading / error / offline / empty / ready for a REAL snapshot. Empty is
 * distinct from offline and error; cached data is labelled stale when the
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
  const retry = (
    <button type="button" onClick={onRetry} disabled={!connected}>
      Retry
    </button>
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
      return <p role="status">Offline: no cached {what}. This is not an empty list.</p>;
    }
    return <p role="status">Loading {what}…</p>;
  }
  return (
    <div>
      {!connected && (
        <p role="status" className="notice" data-testid={`stale-${what}`}>
          Stale: cached {what} from before the connection was lost.
        </p>
      )}
      {status === "error" && error && (
        <div>
          <ErrorNotice error={error} action={`Refreshing ${what}`} /> {retry}
        </div>
      )}
      {isEmpty(data) ? <p>{emptyText}</p> : children(data)}
    </div>
  );
}

export function PlaceholderBadge({ provenance }: { provenance: Provenance }) {
  const label = provenance.kind === "fixture" ? "PLACEHOLDER DATA" : "DAEMON";
  return (
    <span className={`badge badge-${provenance.kind}`} data-testid={`badge-${provenance.id}`}>
      {label} ({provenance.id})
    </span>
  );
}

/** Receipts of simulated DUMMY actions: always labelled SIMULATED and with the source's PLACEHOLDER DATA badge. */
export function SimulatedReceipts({
  label,
  receipts,
}: {
  label: string;
  receipts: Array<{ provenance: Provenance; notice: string }>;
}) {
  return (
    <ul aria-label={label}>
      {receipts.map((receipt, index) => (
        <li key={index} data-testid="simulated-receipt">
          <span className="badge badge-fixture">SIMULATED</span> <PlaceholderBadge provenance={receipt.provenance} />{" "}
          {receipt.notice}
        </li>
      ))}
    </ul>
  );
}

/** Inline error: reason, affected action, safe next step. */
export function ErrorNotice({ error, action }: { error: DaemonError; action: string }) {
  const { reason, nextStep } = presentError(error);
  return (
    <div role="alert" className="notice notice-error">
      <strong>{action} failed.</strong> {reason} <span>{nextStep}</span>
      <span className="muted"> ({error.kind}{error.code !== null ? ` ${error.code}` : ""}: {error.message})</span>
    </div>
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
    <section className={live ? undefined : "placeholder"} aria-label={live ? title : `${title} (placeholder)`}>
      <h3>
        {title} <PlaceholderBadge provenance={provenance} />
      </h3>
      {body}
    </section>
  );
}
