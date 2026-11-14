/** M7-14 Agents & Sessions (`#/agents`). Rendered inside the shell's `main` landmark, so it adds none. */
import { useState } from "react";
import { Pill } from "../../theme";
import { Inventory } from "./Inventory";
import { LaterScope } from "./LaterScope";
import { SessionDetail } from "./SessionDetail";
import { SessionList } from "./SessionList";
import { PlaceholderMark } from "./common";
import { headline, type Count, type TabId } from "./model";
import { useAgentsData } from "./useAgentsData";
import "./agents.css";

const countText = (count: Count): string => (count.kind === "known" ? String(count.value) : "unknown");

export function AgentsView() {
  const model = useAgentsData();
  const [tab, setTab] = useState<TabId>("active");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const rows = model.work.status === "ready" ? model.work.data : null;
  const selected = rows?.find((row) => row.id === selectedId) ?? null;
  const stats = rows ? headline(rows, model.recovery) : null;

  return (
    <div className="agents-view" data-testid="agents-view">
      <header className="agents-head">
        <p className="agents-eyebrow">Execution inventory / S5</p>
        <h1>Agents &amp; Sessions</h1>
        <p>A task persists through failed attempts, new assignments, and changed strategies.</p>
      </header>

      <section aria-label="Execution status" className="agents-status">
        {stats ? (
          <ul className="agents-plain agents-status-list">
            <li>
              <Pill tone="info">
                {stats.runningTasks} running task(s), {stats.runningSessions} running session(s)
              </Pill>
            </li>
            <li>
              <Pill>
                {stats.queued} tasks queued ({stats.readyQueued} ready)
              </Pill>
            </li>
            <li>
              <Pill tone={stats.blocked > 0 ? "warn" : "neutral"}>{stats.blocked} tasks blocked</Pill>
            </li>
            <li>
              <Pill tone={stats.disconnected.kind === "known" ? "neutral" : "warn"}>
                {countText(stats.disconnected)} disconnected sessions
              </Pill>
              {stats.disconnected.kind === "unknown" && (
                <span className="agents-sub"> Not counted: {stats.disconnected.reason}</span>
              )}
            </li>
            <li>
              <span className="agents-sub">Historical failures retained separately</span>
            </li>
            {!model.live && (
              <li>
                <PlaceholderMark id="D2" /> <PlaceholderMark id="D3" />
              </li>
            )}
          </ul>
        ) : (
          <p role="status">
            {model.work.status === "loading"
              ? "Loading execution status…"
              : "Execution status is unknown because the task list is not available."}
          </p>
        )}
        {stats && <p className="agents-sub">Counts cover {rows?.length ?? 0} task(s). The Queued tab lists every queued task, blocked ones included.</p>}
      </section>

      <Inventory inventory={model.inventory} live={model.live} onRetry={model.reloadInventory} />

      <div className="agents-layout">
        <SessionList
          work={model.work}
          inventory={model.inventory}
          recovery={model.recovery}
          live={model.live}
          tab={tab}
          onTab={setTab}
          selectedId={selectedId}
          onSelect={setSelectedId}
          onRetry={model.reloadWork}
        />
        <SessionDetail row={selected} live={model.live} />
      </div>

      <LaterScope />
    </div>
  );
}
