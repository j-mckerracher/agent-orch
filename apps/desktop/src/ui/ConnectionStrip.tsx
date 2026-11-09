/** C1: connection strip, history and continuity state, shared by every screen. */
import { presentError } from "../daemon/errors";
import type { ConnectionStatus } from "../daemon/connection";
import { formatTime } from "./format";
import { useGate, useServices, useSource } from "./services";
import { PlaceholderBadge } from "./widgets";

const LABEL: Record<ConnectionStatus, string> = {
  idle: "Offline",
  connecting: "Connecting...",
  connected: "Connected",
  reconnecting: "Reconnecting...",
  disconnected: "Offline",
  "auth-failed": "Authentication failed",
  incompatible: "Incompatible version or capability",
};

export function ConnectionStrip() {
  const { connection, sources, bridgeBound } = useServices();
  const { connection: snap, connected } = useGate();
  const journal = useSource(() => sources.journal.identity(), "journal");

  const detail = snap.error ? presentError(snap.error) : null;
  return (
    <header className="strip" aria-label="Connection">
      <div>
        <span role="status" className={`pill pill-${snap.status}`} data-testid="connection-status">
          {LABEL[snap.status]}
        </span>{" "}
        <span data-testid="last-sequence">
          Last durable sequence: {snap.lastSequence > 0 ? snap.lastSequence : "none"}
        </span>{" "}
        <button type="button" onClick={() => connection.reconnect({ fresh: true })} disabled={!bridgeBound}>
          Refresh / reconnect
        </button>
      </div>
      {!bridgeBound && (
        <p role="status">Native bridge not bound: the daemon connector is supplied by the desktop host (M5-05).</p>
      )}
      {bridgeBound && !connected && (
        <p role="status" data-testid="offline-note">
          Showing cached data (stale). Last event received: {formatTime(snap.lastEventAt)}. Actions are unavailable
          until the connection returns.
        </p>
      )}
      {snap.history.incomplete && (
        <p role="alert" data-testid="history-incomplete">
          History incomplete ({snap.history.reason ?? "unknown"}): refresh required. Use “Refresh / reconnect” to start a
          fresh baseline.
        </p>
      )}
      {detail && snap.status !== "connected" && (
        <p role="alert" data-testid="connection-error">
          {detail.reason} {detail.nextStep}
        </p>
      )}
      <p className="muted" data-testid="journal-continuity">
        Journal continuity <PlaceholderBadge provenance={sources.journal.provenance} />:{" "}
        {journal.state === "ready"
          ? `${journal.data.cursorRelationship} (never proof that history is complete)`
          : journal.state === "loading"
            ? "loading"
            : "unknown (no store identity in M5-02)"}
      </p>
    </header>
  );
}

export function Crumbs({ items }: { items: Array<{ label: string; onClick?: () => void }> }) {
  return (
    <nav aria-label="Breadcrumb" className="crumbs">
      {items.map((item, index) => (
        <span key={`${index}-${item.label}`}>
          {index > 0 && " / "}
          {item.onClick ? (
            <a href="#" onClick={(event) => { event.preventDefault(); item.onClick?.(); }}>
              {item.label}
            </a>
          ) : (
            <span aria-current="page">{item.label}</span>
          )}
        </span>
      ))}
    </nav>
  );
}
