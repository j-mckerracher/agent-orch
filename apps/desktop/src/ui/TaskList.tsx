/** S3: task list with local filters and the separate D2 execution preview. */
import { Crumbs } from "./ConnectionStrip";
import { FILTER_LABELS, basename, latestSession, matchesFilter } from "./format";
import type { TaskFilter } from "./router";
import { useGate, useServices, useSource, useStore } from "./services";
import { Button } from "./theme";
import { LoadableRegion, SourceSection, StateText, TableWrap } from "./widgets";

export function TaskList({ projectId, filter }: { projectId: string; filter: TaskFilter }) {
  const { data, router, sources } = useServices();
  const { tasks, sessions, approvals } = useStore(data);
  const gate = useGate();
  const preview = useSource(() => sources.execution.listPreviewTasks(), "preview", sources.execution.provenance);
  const go = (next: TaskFilter) => router.navigate({ name: "tasks", projectId, filter: next });
  const own = (tasks.data ?? []).filter((task) => task.contract.project_id === projectId);
  const approvalList = approvals.data ?? [];

  return (
    <main className="ps-page">
      <Crumbs
        items={[
          { label: "Projects", onClick: () => router.navigate({ name: "projects" }) },
          { label: basename(projectId), onClick: () => router.navigate({ name: "project", projectId }) },
          { label: "Tasks" },
        ]}
      />
      <h1>Tasks</h1>
      <div className="ps-actions">
        <Button onClick={() => router.navigate({ name: "project", projectId })}>Back to project</Button>
        <Button variant="primary" onClick={() => router.navigate({ name: "project", projectId })}>
          New task
        </Button>
      </div>
      <div role="group" aria-label="Task filters" className="ps-filters">
        {FILTER_LABELS.map(({ filter: value, label }) => {
          const count = own.filter((task) => matchesFilter(task, value, own, approvalList)).length;
          return (
            <Button key={value} aria-pressed={filter === value} onClick={() => go(value)}>
              {label} ({count}/{own.length})
            </Button>
          );
        })}
        {filter.startsWith("state:") && (
          <Button aria-pressed onClick={() => go("all")}>
            {filter.slice("state:".length)} (clear)
          </Button>
        )}
      </div>
      <LoadableRegion
        what="tasks"
        loadable={tasks}
        connected={gate.connected}
        isEmpty={() => own.length === 0}
        emptyText="No tasks in this project yet."
        onRetry={() => void data.refreshAll()}
      >
        {() => {
          const rows = own.filter((task) => matchesFilter(task, filter, own, approvalList));
          if (rows.length === 0) return <p>No tasks match this filter.</p>;
          return (
            <TableWrap>
              <table aria-label="Tasks">
                <thead>
                  <tr>
                    <th>Task ID</th>
                    <th>Objective</th>
                    <th>State</th>
                    <th>Agent</th>
                    <th>Recorded session</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {rows.map((task) => {
                    const session = latestSession(task, sessions.data);
                    return (
                      <tr key={task.contract.id}>
                        <td>
                          <code>{task.contract.id}</code>
                        </td>
                        <td>{task.contract.objective}</td>
                        <td>
                          <StateText state={task.state} />
                        </td>
                        <td>{task.contract.agent}</td>
                        <td>{session ? `${session.id} (${session.state})` : "none recorded"}</td>
                        <td>
                          <Button
                            onClick={() => router.navigate({ name: "task", projectId, taskId: task.contract.id })}
                          >
                            Open
                          </Button>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </TableWrap>
          );
        }}
      </LoadableRegion>

      {sources.execution.provenance.kind === "fixture" && (
      <SourceSection
        title="Execution preview"
        provenance={sources.execution.provenance}
        result={preview}
        emptyText="No preview tasks."
      >
        {(list) => (
          <TableWrap>
            <table aria-label="Execution preview">
              <thead>
                <tr>
                  <th>Preview ID</th>
                  <th>Objective</th>
                  <th>State</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {list.map((task) => (
                  <tr key={task.id}>
                    <td>
                      <code>{task.id}</code>
                    </td>
                    <td>{task.objective}</td>
                    <td>
                      <StateText state={task.state} />
                    </td>
                    <td>
                      <Button onClick={() => router.navigate({ name: "fixture-task", projectId, id: task.id })}>
                        Open preview
                      </Button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TableWrap>
        )}
      </SourceSection>
      )}
    </main>
  );
}
