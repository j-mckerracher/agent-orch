/**
 * "Assigned work and sessions": tabs over the derived lists, with local filters. Counts in tab labels
 * state what they count: tasks for Active and Queued, sessions for Failed. Unavailable data is never a number.
 */
import { useId, useRef, useState, type KeyboardEvent } from "react";
import { Button, EmptyState, Pill } from "../../theme";
import { LoadRegion, PlaceholderMark, StatePill } from "./common";
import { agentDisplayName } from "../../../daemon/agentDisplayNames";
import {
failedSessions,
  failedWithoutSession,
  isRunningSession,
  latestAttempt,
  sessionsKnown,
  tabRows,
  TABS,
  type FailedSession,
  type TabId,
  type WorkRow,
} from "./model";
import type { Load } from "./useAgentsData";

const EMPTY_TEXT: Record<"active" | "queued", string> = {
  active: "No active tasks.",
  queued: "No queued tasks.",
};

/** Local filter over task id, objective and agent. Nothing is sent to the daemon. */
function matchesQuery(row: WorkRow, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (needle === "") return true;
  return [row.id, row.objective, row.agent].some((text) => text.toLowerCase().includes(needle));
}

function attemptText(row: WorkRow): string {
  const latest = latestAttempt(row);
  if (latest === null) return "No attempt recorded";
  return latest.number === null ? "Attempt" : `Attempt ${latest.number}`;
}

function sessionsText(row: WorkRow): string {
  if (row.sessions === null) return "sessions unknown";
  const running = row.sessions.filter((s) => isRunningSession(s.state)).length;
  return `${row.sessions.length} session(s)${running > 0 ? `, ${running} running` : ""}`;
}

export function SessionList({
  work,
  live,
  tab,
  onTab,
  selectedId,
  onSelect,
  onRetry,
}: {
  work: Load<WorkRow[]>;
  live: boolean;
  tab: TabId;
  onTab: (tab: TabId) => void;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onRetry: () => void;
}) {
  const refs = useRef<Record<string, HTMLButtonElement | null>>({});
  const filterId = useId();
  const [query, setQuery] = useState("");
  const rows = work.status === "ready" ? work.data : [];

  const label = (id: TabId, text: string): string => {
    if (id === "available") return text;
    if (id === "disconnected") return `${text} (not available)`;
    // Loading is the only state that may still become a count; any other unread list is unknown, never "…".
    if (work.status === "loading") return `${text} (…)`;
    if (work.status !== "ready") return `${text} (unknown)`;
    if (id === "failed") return sessionsKnown(rows) ? `${text} (${failedSessions(rows).length})` : `${text} (unknown)`;
    return `${text} (${tabRows(id, rows).length})`;
  };

  const onKey = (event: KeyboardEvent, index: number) => {
    const last = TABS.length - 1;
    const next =
      event.key === "ArrowRight" ? (index + 1) % TABS.length
      : event.key === "ArrowLeft" ? (index + last) % TABS.length
      : event.key === "Home" ? 0
      : event.key === "End" ? last
      : null;
    if (next === null) return;
    event.preventDefault();
    onTab(TABS[next]!.id);
    refs.current[TABS[next]!.id]?.focus();
  };

  return (
    <section className="agents-card" aria-labelledby="agents-work-title">
      <header className="agents-card-head">
        <h2 id="agents-work-title">Assigned work and sessions</h2>
        {!live && <PlaceholderMark id="D2" />}
      </header>
      <div role="tablist" aria-label="Task and session lists" className="agents-tabs">
        {TABS.map(({ id, label: text }, index) => (
          <button
            key={id}
            ref={(el) => {
              refs.current[id] = el;
            }}
            type="button"
            role="tab"
            id={`agents-tab-${id}`}
            aria-selected={tab === id}
            aria-controls="agents-tabpanel"
            tabIndex={tab === id ? 0 : -1}
            onClick={() => onTab(id)}
            onKeyDown={(event) => onKey(event, index)}
          >
            {label(id, text)}
          </button>
        ))}
      </div>
      <div role="tabpanel" id="agents-tabpanel" aria-labelledby={`agents-tab-${tab}`} className="agents-panel">
        {tab === "available" ? (
          <EmptyState
            role="status"
            title="Available / idle workers are not available"
            description="The daemon reports no idle-worker count. Adapter launchability is not an idle-worker count; see Configured inventory."
          />
        ) : tab === "disconnected" ? (
          <EmptyState
            role="status"
            title="Disconnected sessions are not available"
            description="The daemon has no method that reports them (D3), so they are not counted as zero."
          />
        ) : (
          <>
            <div className="agents-filter">
              <label htmlFor={filterId}>Filter by task, objective or agent</label>
              <input id={filterId} type="search" value={query} onChange={(event) => setQuery(event.target.value)} />
            </div>
            <p className="agents-sub">
              Provider, model and progress-age filters are not available: the daemon does not report them.
            </p>
            <LoadRegion
              what="Tasks"
              load={work}
              emptyTitle="No tasks"
              onRetry={live ? onRetry : undefined}
            >
              {() =>
                tab === "failed" ? (
                  <FailedRows
                    rows={rows}
                    query={query}
                    selectedId={selectedId}
                    onSelect={onSelect}
                    onRetry={onRetry}
                  />
                ) : (
                  <TaskRows
                    tab={tab}
                    rows={rows}
                    query={query}
                    selectedId={selectedId}
                    onSelect={onSelect}
                  />
                )
              }
            </LoadRegion>
          </>
        )}
      </div>
    </section>
  );
}

function TaskRows({
  tab,
  rows,
  query,
  selectedId,
  onSelect,
}: {
  tab: "active" | "queued";
  rows: WorkRow[];
  query: string;
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const all = tabRows(tab, rows);
  const list = all.filter((row) => matchesQuery(row, query));
  if (all.length === 0) return <EmptyState title={EMPTY_TEXT[tab]} />;
  if (list.length === 0) return <EmptyState title="No tasks match the filter." />;
  return (
    <>
      {list.length !== all.length && <p className="agents-sub">Showing {list.length} of {all.length}.</p>}
      <ul className="agents-list" aria-label={`${tab} tasks`}>
        {list.map((row) => (
          <TaskButton
            key={row.id}
            row={row}
            selected={selectedId === row.id}
            onSelect={onSelect}
            detail={`Agent ${agentDisplayName(row.agent)}; ${attemptText(row)}; ${sessionsText(row)}; provider and model: not available`}
          />
        ))}
      </ul>
    </>
  );
}

function FailedRows({
  rows,
  query,
  selectedId,
  onSelect,
  onRetry,
}: {
  rows: WorkRow[];
  query: string;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onRetry: () => void;
}) {
  if (!sessionsKnown(rows)) {
    return (
      <EmptyState
        role="status"
        title="Failed sessions are unknown."
        description="The daemon's session list has not loaded, so failed sessions cannot be counted or listed."
        action={<Button onClick={onRetry}>Retry</Button>}
      />
    );
  }
  const sessions = failedSessions(rows);
  const orphans = failedWithoutSession(rows);
  const shownSessions = sessions.filter((failed) => matchesQuery(failed.row, query));
  const shownOrphans = orphans.filter((row) => matchesQuery(row, query));
  if (sessions.length === 0 && orphans.length === 0) {
    return (
      <EmptyState
        title="No failed sessions."
        description="Failed attempts of retried tasks stay in the task's attempt history."
      />
    );
  }
  if (shownSessions.length === 0 && shownOrphans.length === 0) return <EmptyState title="No sessions match the filter." />;
  return (
    <>
      {shownSessions.length > 0 && (
        <ul className="agents-list" aria-label="failed sessions">
          {shownSessions.map((failed) => (
            <FailedSessionButton
              key={`${failed.row.id}:${failed.session.id}`}
              failed={failed}
              selected={selectedId === failed.row.id}
              onSelect={onSelect}
            />
          ))}
        </ul>
      )}
      {shownOrphans.length > 0 && (
        <>
          <p className="agents-sub">
            Failed tasks with no session on record ({shownOrphans.length}). They are not sessions, so they are not counted
            above.
          </p>
          <ul className="agents-list" aria-label="failed tasks without a session">
            {shownOrphans.map((row) => (
              <TaskButton
                key={row.id}
                row={row}
                selected={selectedId === row.id}
                onSelect={onSelect}
                detail={`Agent ${agentDisplayName(row.agent)}; ${attemptText(row)}; no session recorded`}
              />
            ))}
          </ul>
        </>
      )}
    </>
  );
}

function TaskButton({
  row,
  selected,
  onSelect,
  detail,
}: {
  row: WorkRow;
  selected: boolean;
  onSelect: (id: string) => void;
  detail: string;
}) {
  return (
    <li>
      <button type="button" className="agents-row" aria-pressed={selected} onClick={() => onSelect(row.id)}>
        <span className="agents-row-main">
          <code>{row.id}</code>
          <span>{row.objective}</span>
          <span className="agents-sub">{detail}</span>
        </span>
        <span className="agents-row-state">
          <StatePill state={row.state} />
          {row.blocked && <Pill tone="warn">Blocked</Pill>}
        </span>
      </button>
    </li>
  );
}

function FailedSessionButton({
  failed,
  selected,
  onSelect,
}: {
  failed: FailedSession;
  selected: boolean;
  onSelect: (id: string) => void;
}) {
  const { row, session, attempt } = failed;
  const attemptLabel = attempt?.number != null ? `Attempt ${attempt.number}; ` : "";
  return (
    <li>
      <button type="button" className="agents-row" aria-pressed={selected} onClick={() => onSelect(row.id)}>
        <span className="agents-row-main">
          <code>{row.id}</code>
          <span>{row.objective}</span>
          <span className="agents-sub">
            {attemptLabel}session <code>{session.id}</code>
          </span>
        </span>
        <span className="agents-row-state">
          <StatePill state={session.state} />
        </span>
      </button>
    </li>
  );
}
