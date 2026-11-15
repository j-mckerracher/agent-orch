/** C1: connection strip, history and continuity state, shared by every screen. */
import { presentError } from "../daemon/errors";
import type { ConnectionStatus } from "../daemon/connection";
import { formatTime } from "./format";
import { connectionAttention, type AttentionReason } from "./connectionAttention";
import { useLayoutSettings, type LayoutSettings } from "./layout/layoutSettings";
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

/** Shown beside the disabled toggle, so the locked strip says why it cannot fold. */
const ATTENTION_TEXT: Record<AttentionReason, string> = {
  // Neutral on purpose: the first connection is still opening, so this must not claim the daemon is not connected.
  "not-connected": "Kept open while the connection needs attention.",
  "daemon-error": "Kept open while a daemon error is shown.",
  "history-incomplete": "Kept open until history is refreshed.",
  "journal-reset": "Kept open after a journal reset.",
};

const DETAILS_ID = "connection-details";
const LOCK_ID = "connection-lock";

export interface ConnectionStripProps {
  /** Test seam; defaults to the application-wide persisted layout. */
  layout?: LayoutSettings;
}

/**
 * The connection strip. The status row, the Refresh / reconnect control, every live region and every problem
 * notice are always rendered. Only the healthy-state explanatory notes (connected note, recovery, journal
 * continuity) sit in the folding region, and the region is open whenever the connection needs attention.
 */
export function ConnectionStrip({ layout }: ConnectionStripProps = {}) {
  const { connection, sources, bridgeBound } = useServices();
  const { connection: snap, connected } = useGate();
  const journal = useSource(() => sources.journal.identity(), "journal", sources.journal.provenance);
  const { connectionStripExpanded, setConnectionStripExpanded } = useLayoutSettings(layout);

  const attention = connectionAttention({ bridgeBound, connection: snap, journal });
  // The saved choice applies only while the connection is healthy; it is restored when health returns.
  const expanded = attention !== null || connectionStripExpanded;

  const detail = snap.error ? presentError(snap.error) : null;
  // The host's launch-failure message carries the redacted end of the daemon
  // log; the generic presentation above does not. Only this kind is shown
  // verbatim, because the host redacts it before it reaches the webview.
  const launchDetail = snap.error?.kind === "daemon-unavailable" ? snap.error.message.trim() : "";
  return (
    <div className="strip conn-strip" data-expanded={expanded ? "true" : "false"}>
      <div className="conn-main">
        <button
          type="button"
          className="conn-toggle"
          aria-label={expanded ? "Collapse connection details" : "Expand connection details"}
          aria-expanded={expanded}
          aria-controls={DETAILS_ID}
          aria-describedby={attention ? LOCK_ID : undefined}
          disabled={attention !== null}
          data-testid="connection-toggle"
          onClick={() => setConnectionStripExpanded(!connectionStripExpanded)}
        >
          <svg className="conn-chevron" width="12" height="12" viewBox="0 0 12 12" aria-hidden="true" focusable="false">
            <path d="M2.5 4.5 6 8l3.5-3.5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </button>
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
        {attention && (
          <span id={LOCK_ID} className="conn-lock" data-testid="connection-lock">
            {ATTENTION_TEXT[attention]}
          </span>
        )}
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
      <div id={DETAILS_ID} className="conn-details" data-testid="connection-details" hidden={!expanded}>
        {bridgeBound && connected && (
          <p className="muted" data-testid="connected-note">
            Connected to the daemon. A connection does not prove any agent is running; process, workspace and delivery
            state come from each task.
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
      </div>
    </div>
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
