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
  disconnected: "Disconnected (daemon may still be running)",
  unavailable: "Daemon unavailable",
  "auth-failed": "Authentication failed",
  incompatible: "Incompatible version or capability",
};

export function ConnectionStrip() {
  const { connection, sources, bridgeBound } = useServices();
  const { connection: snap, connected } = useGate();
  const journal = useSource(() => sources.journal.identity(), "journal", sources.journal.provenance);

  const detail = snap.error ? presentError(snap.error) : null;
  // The host's launch-failure message carries the redacted end of the daemon
  // log; the generic presentation above does not. Only this kind is shown
  // verbatim, because the host redacts it before it reaches the webview.
  const launchDetail = snap.error?.kind === "daemon-unavailable" ? snap.error.message.trim() : "";
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
        <p role="status" data-testid="fixture-banner" className="notice">
          <span className="placeholder-tag">FIXTURE DEMO MODE</span> Every screen shows built-in sample data. No daemon
          is contacted, so nothing here reflects real projects, tasks or agents.
        </p>
      )}
      {bridgeBound && connected && (
        <p className="muted" data-testid="connected-note">
          Connected to the daemon. A connection does not prove any agent is running; process, workspace and delivery
          state come from each task.
        </p>
      )}
      {bridgeBound && !connected && (
        <p role="status" data-testid="offline-note">
          Showing cached data (stale). Last event received: {formatTime(snap.lastEventAt)}. Actions are unavailable
          until the connection returns. Closing or losing this connection never stops the daemon or its agents.
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
      {launchDetail && snap.status !== "connected" && (
        <pre data-testid="connection-error-detail" aria-label="Why the daemon could not start">
          {launchDetail}
        </pre>
      )}
      <p className="muted" data-testid="recovery-unknown">
        Recovery: unknown <PlaceholderBadge provenance={sources.recovery.provenance} /> The daemon does not expose
        recovery status yet; no process is assumed safe from a PID or from a successful connection.
      </p>
      <p className="muted" data-testid="journal-continuity">
        Journal continuity <PlaceholderBadge provenance={sources.journal.provenance} />:{" "}
        {journal.state === "ready"
          ? `${journal.data.cursorRelationship} (never proof that history is complete)`
          : journal.state === "loading"
            ? "loading"
            : journal.state === "unavailable"
              ? `unknown (${journal.reason})`
              : "unknown (journal identity could not be read)"}
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
