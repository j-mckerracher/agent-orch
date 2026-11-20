/** S3: a project's task list: three filters, and Task | State | Agent. Id and session are in the task page. */
import { FILTER_LABELS, taskBucket } from "./format";
import type { TaskFilter } from "./router";
import { agentDisplayName } from "../daemon/agentDisplayNames";
import { QueueWaitNote, useQueueWaits } from "./QueueWaitNote";
import { TaskPermissionBadge, ToolsNotice } from "./TaskPermission";
import { useGate, useServices, useSource, useStore } from "./services";
import { taskPermissionLevel } from "./views/workspace/permissionLevel";
import { NavLink } from "./shell/NavLink";
import { Button, Disclosure, Pill } from "./theme";
import { LoadableRegion, SourceSection, StateText, TableWrap } from "./widgets";

export function TaskList({ projectId, filter }: { projectId: string; filter: TaskFilter }) {
  const { data, router, sources } = useServices();
  const { tasks, approvals } = useStore(data);
  const gate = useGate();
  const preview = useSource(() => sources.execution.listPreviewTasks(), "preview", sources.execution.provenance);
  const go = (next: TaskFilter) => router.navigate({ name: "tasks", projectId, filter: next });
  const own = (tasks.data ?? []).filter((task) => task.contract.project_id === projectId);
  const approvalList = approvals.data ?? [];
  // Unread counts are not zero: "…" while a daemon answer can still arrive, "unknown" once it cannot.
  const unread = (status: string): string => (status !== "error" && (gate.connected || gate.settling) ? "…" : "unknown");
  const unreadCount = tasks.data === null ? unread(tasks.status) : null;
  const approvalsUnread = approvals.data === null ? unread(approvals.status) : null;
  // Without approvals, review membership is unknown: no filter narrows the rows by it.
  const unfiltered = approvals.data === null && filter === "review";
  const rows = unfiltered ? own : own.filter((task) => taskBucket(task, approvalList) === filter);
  const waits = useQueueWaits(rows.some((task) => task.state === "Queued"));

  return (
    <div>
      <div role="group" aria-label="Task filters" className="ps-filters">
        {FILTER_LABELS.map(({ filter: value, label }) => {
          const shown =
            unreadCount ?? (value === "review" ? approvalsUnread : null) ?? own.filter((task) => taskBucket(task, approvalList) === value).length;
          return (
            <Button key={value} aria-pressed={filter === value} onClick={() => go(value)}>
              {label} {shown}
            </Button>
          );
        })}
      </div>
      {unfiltered && (
        <p role="status" className="muted">
          Awaiting review is unknown: the approvals list has not been read. Rows are not filtered by review.
        </p>
      )}
      <LoadableRegion
        what="tasks"
        loadable={tasks}
        connected={gate.connected}
        isEmpty={() => own.length === 0}
        emptyText="No tasks in this project yet."
        onRetry={() => void data.refreshAll()}
      >
        {() => {
          if (rows.length === 0) return <p>No tasks match this filter.</p>;
          return (
            <TableWrap>
              <table aria-label="Tasks">
                <thead>
                  <tr>
                    <th>Task</th>
                    <th>State</th>
                    <th>Agent</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((task) => (
                    <tr key={task.contract.id}>
                      <td>
                        <NavLink to={{ name: "task", projectId, taskId: task.contract.id }}>{task.contract.objective}</NavLink>
                      </td>
                      <td>
                        {taskBucket(task, approvalList) === "review" ? <Pill tone="warn">Awaiting review</Pill> : <StateText state={task.state} />}
                        <QueueWaitNote state={task.state} wait={waits.get(task.contract.id)} />
                        <ToolsNotice notice={task.tools_notice} />
                      </td>
                      <td>
                        {agentDisplayName(task.contract.agent)} <TaskPermissionBadge level={taskPermissionLevel(task)} />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </TableWrap>
          );
        }}
      </LoadableRegion>

      {sources.execution.provenance.kind === "fixture" && (
        <Disclosure summary="Execution preview" count="sample data">
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
        </Disclosure>
      )}
    </div>
  );
}
