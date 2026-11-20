/** Agents (`#/agents`): the task list, and one task's own view at `#/agents?task=<id>`. Rendered inside the shell's `main` landmark, so it adds none. */
import { useServices, useStore } from "../../services";
import { PlaceholderBadge } from "../../widgets";
import { EmptyState } from "../../theme";
import { summaryLine } from "./model";
import { TaskDetail } from "./TaskDetail";
import { TaskList } from "./TaskList";
import { useAgentsData } from "./useAgentsData";
import "../workstreams/workstreams.css";
import "./agents.css";

export function AgentsView() {
  const model = useAgentsData();
  const { router, sources } = useServices();
  const route = useStore(router);
  const taskId = route.name === "global" ? route.taskId : undefined;
  const open = (id?: string) => router.navigate({ name: "global", view: "agents", ...(id === undefined ? {} : { taskId: id }) });
  const rows = model.work.status === "ready" ? model.work.data : null;
  const selected = taskId === undefined ? null : (rows?.find((row) => row.id === taskId) ?? null);

  let body;
  if (taskId !== undefined && selected) {
    body = <TaskDetail key={selected.id} initial={selected} live={model.live} onBack={() => open()} />;
  } else if (taskId !== undefined && rows) {
    body = (
      <>
        <button type="button" className="ws-back" onClick={() => open()}>
          <span aria-hidden="true">←</span> Agents
        </button>
        <EmptyState title="Task not found" description={`No task ${taskId} is in the task list.`} />
      </>
    );
  } else {
    body = (
      <>
        <header className="ws-head">
          <div className="ws-head-title">
            <div className="ws-title-row">
              <h1>Agents</h1>
              {rows && <span className="ws-subtle">{summaryLine(rows)}</span>}
              {!model.live && <PlaceholderBadge provenance={sources.execution.provenance} compact />}
            </div>
          </div>
        </header>
        <TaskList work={model.work} live={model.live} onOpen={open} onRetry={model.reloadWork} />
      </>
    );
  }

  return (
    <section className="ws-view agents-view" aria-label="Agents" data-testid="agents-view">
      {body}
    </section>
  );
}
