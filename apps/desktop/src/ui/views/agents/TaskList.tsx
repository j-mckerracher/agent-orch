/** The task list: five status tabs with counts and a local filter. A row opens the task's own view. */
import { useState } from "react";
import { agentDisplayName } from "../../../daemon/agentDisplayNames";
import type { QueueWait } from "../../../sources/surfaces/types";
import { QueueWaitNote, useQueueWaits } from "../../QueueWaitNote";
import { EmptyState, Pill } from "../../theme";
import { WsTabs } from "../workstreams/WsTabs";
import { LoadRegion, stateTone } from "./common";
import { latestAttempt, tabRows, TABS, type TaskTab, type WorkRow } from "./model";
import type { Load } from "./useAgentsData";

const EMPTY_TEXT: Record<TaskTab, string> = {
  running: "No tasks are running.",
  waiting: "No tasks are waiting for a reply.",
  queued: "No tasks are queued.",
  failed: "No tasks have failed.",
  finished: "No tasks have finished.",
};

/** Local filter over task id, objective, project and agent. Nothing is sent to the daemon. */
function matchesQuery(row: WorkRow, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (needle === "") return true;
  return [row.id, row.objective, row.project ?? "", row.agent, agentDisplayName(row.agent)].some((text) =>
    text.toLowerCase().includes(needle),
  );
}

/** `project · agent · attempt N`: the second line of a row; the detail header adds `of M`. */
export function taskSubtitle(row: WorkRow, ofTotal = false): string {
  const attempt = latestAttempt(row);
  return [
    row.project,
    agentDisplayName(row.agent),
    attempt?.number != null ? `attempt ${attempt.number}${ofTotal ? ` of ${row.attempts.length}` : ""}` : null,
  ]
    .filter((part) => part !== null)
    .join(" · ");
}

function TaskRow({ row, wait, onOpen }: { row: WorkRow; wait?: QueueWait; onOpen: (id: string) => void }) {
  return (
    <li>
      <button type="button" className="ws-row-card" data-testid="agents-row" onClick={() => onOpen(row.id)}>
        <span className="ws-dot" data-tone={stateTone(row.state)} aria-hidden="true" />
        <span className="ws-row-main">
          <span className="ws-row-name">{row.objective}</span>
          <small>{taskSubtitle(row)}</small>
          <QueueWaitNote state={row.state} wait={wait} />
        </span>
        {row.blocked && <Pill tone="warn">Blocked</Pill>}
        <span aria-hidden="true">›</span>
      </button>
    </li>
  );
}

function TaskRows({ tab, rows, live, query, onOpen }: { tab: TaskTab; rows: WorkRow[]; live: boolean; query: string; onOpen: (id: string) => void }) {
  const all = tabRows(tab, rows);
  const list = all.filter((row) => matchesQuery(row, query));
  const waits = useQueueWaits(live && list.some((row) => row.origin === "real" && row.state === "Queued"));
  if (all.length === 0) return <EmptyState title={EMPTY_TEXT[tab]} />;
  if (list.length === 0) return <EmptyState title="No tasks match the filter." />;
  return (
    <ul className="ws-items" aria-label={`${tab} tasks`}>
      {list.map((row) => (
        <TaskRow key={row.id} row={row} wait={row.origin === "real" ? waits.get(row.id) : undefined} onOpen={onOpen} />
      ))}
    </ul>
  );
}

export function TaskList({
  work,
  live,
  onOpen,
  onRetry,
}: {
  work: Load<WorkRow[]>;
  live: boolean;
  onOpen: (id: string) => void;
  onRetry: () => void;
}) {
  const [query, setQuery] = useState("");
  return (
    <LoadRegion what="Tasks" load={work} emptyTitle="No tasks" onRetry={live ? onRetry : undefined}>
      {(rows) =>
        rows.length === 0 ? (
          <EmptyState title="No tasks" />
        ) : (
        <WsTabs
          label="Task status"
          actions={
            <input
              type="search"
              className="ws-search"
              aria-label="Filter tasks or agents"
              placeholder="Filter tasks or agents"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          }
          tabs={TABS.map(({ id, label }) => ({
            id,
            label: `${label} ${tabRows(id, rows).length}`,
            content: <TaskRows tab={id} rows={rows} live={live} query={query} onOpen={onOpen} />,
          }))}
        />
        )
      }
    </LoadRegion>
  );
}
