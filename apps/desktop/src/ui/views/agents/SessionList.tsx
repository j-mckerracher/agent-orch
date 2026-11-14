/** "Assigned work and sessions": tabs over the derived task lists. Counts in tab labels equal the list lengths. */
import { useRef, type KeyboardEvent } from "react";
import { EmptyState, Pill } from "../../theme";
import { LoadRegion, PlaceholderMark, StatePill } from "./common";
import type { InventoryRow } from "./inventoryModel";
import {
  isRunningSession,
  tabRows,
  TABS,
  disconnected,
  type RecoveryMap,
  type TabId,
  type WorkRow,
} from "./model";
import type { Load } from "./useAgentsData";

const EMPTY_TEXT: Record<Exclude<TabId, "available">, string> = {
  active: "No active tasks.",
  queued: "No queued tasks.",
  failed: "No failed tasks. Failed attempts of retried tasks stay in the task's attempt history.",
  disconnected: "No disconnected tasks.",
};

export function SessionList({
  work,
  inventory,
  recovery,
  live,
  tab,
  onTab,
  selectedId,
  onSelect,
  onRetry,
}: {
  work: Load<WorkRow[]>;
  inventory: Load<InventoryRow[]>;
  recovery: RecoveryMap;
  live: boolean;
  tab: TabId;
  onTab: (tab: TabId) => void;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onRetry: () => void;
}) {
  const refs = useRef<Record<string, HTMLButtonElement | null>>({});
  const rows = work.status === "ready" ? work.data : [];
  const assignable =
    inventory.status === "empty"
      ? []
      : inventory.status === "ready"
        ? inventory.data.filter((row) => row.assignable)
        : null;
  const unknownDisconnected = disconnected(rows, recovery);

  const label = (id: TabId, text: string): string => {
    if (id === "available") return `${text} (${assignable ? assignable.length : "unknown"})`;
    if (work.status !== "ready") return `${text} (…)`;
    if (id === "disconnected" && unknownDisconnected.kind === "unknown") return `${text} (unknown)`;
    return `${text} (${tabRows(id, rows, recovery).length})`;
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
          <Available inventory={inventory} />
        ) : (
          <LoadRegion
            what="Tasks"
            load={work}
            emptyTitle="No tasks"
            onRetry={live ? onRetry : undefined}
          >
            {() => (
              <Rows
                tab={tab}
                rows={rows}
                recovery={recovery}
                selectedId={selectedId}
                onSelect={onSelect}
              />
            )}
          </LoadRegion>
        )}
      </div>
    </section>
  );
}

function Available({ inventory }: { inventory: Load<InventoryRow[]> }) {
  return (
    <LoadRegion what="Adapter inventory" load={inventory} emptyTitle="No adapters reported">
      {(list) => {
        const ready = list.filter((row) => row.assignable);
        if (ready.length === 0) {
          return <EmptyState title="No adapter is available for assignment" description="An adapter must be installed and launchable." />;
        }
        return (
          <ul className="agents-list" aria-label="Available adapters">
            {ready.map((row) => (
              <li key={row.id} className="agents-row">
                <code>{row.name}</code>
                <Pill tone="ok">{row.assignment}</Pill>
              </li>
            ))}
          </ul>
        );
      }}
    </LoadRegion>
  );
}

function Rows({
  tab,
  rows,
  recovery,
  selectedId,
  onSelect,
}: {
  tab: Exclude<TabId, "available">;
  rows: WorkRow[];
  recovery: RecoveryMap;
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  if (tab === "disconnected") {
    const count = disconnected(rows, recovery);
    if (count.kind === "unknown") {
      return (
        <EmptyState
          role="status"
          title="Disconnected sessions are unknown"
          description={`They cannot be counted: ${count.reason}`}
        />
      );
    }
  }
  const list = tabRows(tab, rows, recovery);
  if (list.length === 0) return <EmptyState title={EMPTY_TEXT[tab]} />;
  return (
    <ul className="agents-list" aria-label={`${tab} tasks`}>
      {list.map((row) => {
        const running = row.sessions.filter((s) => isRunningSession(s.state)).length;
        return (
          <li key={row.id}>
            <button
              type="button"
              className="agents-row"
              aria-pressed={selectedId === row.id}
              onClick={() => onSelect(row.id)}
            >
              <span className="agents-row-main">
                <code>{row.id}</code>
                <span>{row.objective}</span>
                <span className="agents-sub">
                  Agent {row.agent}; {row.attempts.length} attempt(s); {row.sessions.length} session(s)
                  {running > 0 ? `, ${running} running` : ""}
                </span>
              </span>
              <span className="agents-row-state">
                <StatePill state={row.state} />
                {row.blocked && <Pill tone="warn">Blocked</Pill>}
              </span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}
