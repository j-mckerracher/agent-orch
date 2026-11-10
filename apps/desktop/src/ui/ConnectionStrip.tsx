/** C1: connection strip, history and continuity state, shared by every screen. */
import { presentError } from "../daemon/errors";
import type { ConnectionStatus } from "../daemon/connection";
import { formatTime } from "./format";
import { useGate, useServices, useSource } from "./services";
import { Badge, Button, Pill, type Tone } from "./theme";
import { PlaceholderBadge } from "./widgets";
import "./projectScreens.css";

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

/** Tone plus a per-state glyph, so each state differs by shape and text, never colour alone. */
const STATE_STYLE: Record<ConnectionStatus, { tone: Tone; glyph: string }> = {
  idle: { tone: "neutral", glyph: "○" },
  connecting: { tone: "info", glyph: "◌" },
  connected: { tone: "ok", glyph: "✓" },
  reconnecting: { tone: "warn", glyph: "↻" },
  disconnected: { tone: "bad", glyph: "⊘" },
  unavailable: { tone: "bad", glyph: "✕" },
  "auth-failed": { tone: "bad", glyph: "⚿" },
  incompatible: { tone: "bad", glyph: "≠" },
};

export function ConnectionStrip() {
  const { connection, sources, bridgeBound } = useServices();
  const { connection: snap, connected } = useGate();
  const journal = useSource(() => sources.journal.identity(), "journal", sources.journal.provenance);

  const detail = snap.error ? presentError(snap.error) : null;
  return (
    <header className="strip conn-strip" aria-label="Connection">
      <div className="conn-main">
        <Pill
          tone={STATE_STYLE[snap.status].tone}
          hideMark
          role="status"
          className="conn-pill"
          data-testid="connection-status"
        >
          <span className="conn-glyph" aria-hidden="true">
            {STATE_STYLE[snap.status].glyph}
          </span>
          {LABEL[snap.status]}
        </Pill>
        <span className="conn-seq" data-testid="last-sequence">
          Last durable sequence: {snap.lastSequence > 0 ? snap.lastSequence : "none"}
        </span>
        <Button onClick={() => connection.reconnect({ fresh: true })} disabled={!bridgeBound}>
          Refresh / reconnect
        </Button>
      </div>
      {!bridgeBound && (
        <p role="status" data-testid="fixture-banner" className="notice">
          <Badge tone="placeholder">FIXTURE DEMO MODE</Badge> Every screen shows built-in sample data. No daemon
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
    <nav aria-label="Breadcrumb" className="crumbs ps-crumbs">
      {items.map((item, index) => (
        <span key={`${index}-${item.label}`}>
          {index > 0 && <span className="ps-crumbs-sep" aria-hidden="true">/ </span>}
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
