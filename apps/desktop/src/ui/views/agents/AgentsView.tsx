/** M7-14 Agents & Sessions (`#/agents`). Rendered inside the shell's `main` landmark, so it adds none. */
import { useState } from "react";
import { Pill } from "../../theme";
import { Inventory } from "./Inventory";
import { LaterScope } from "./LaterScope";
import { SessionDetail } from "./SessionDetail";
import { SessionList } from "./SessionList";
import { DisabledAction, PlaceholderMark } from "./common";
import { headline, type TabId } from "./model";
import { useAgentsData } from "./useAgentsData";
import "./agents.css";

export function AgentsView() {
  const model = useAgentsData();
  const [tab, setTab] = useState<TabId>("active");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const rows = model.work.status === "ready" ? model.work.data : null;
  const selected = rows?.find((row) => row.id === selectedId) ?? null;
  const stats = rows ? headline(rows) : null;

  return (
    <div className="agents-view" data-testid="agents-view">
      <header className="agents-head agents-head-row">
        <div>
          <p className="agents-eyebrow">Execution inventory / S5</p>
          <h1>Agents &amp; Sessions</h1>
          <p>A task persists through failed attempts, new assignments, and changed strategies.</p>
        </div>
        <DisabledAction
          label="Configure providers"
          reason="Provider configuration is not available: the daemon has no method to change adapters."
        />
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
              <Pill tone="warn">Disconnected sessions not available</Pill>
              <span className="agents-sub"> Not counted: the daemon has no method that reports them (D3).</span>
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
