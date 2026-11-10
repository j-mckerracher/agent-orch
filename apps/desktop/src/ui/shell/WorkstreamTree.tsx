import { useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import { formatRoute } from "../router";
import { useServices, useStore } from "../services";
import { taskLabelOf, useWorkstreamModel, type WsGroup, type WsTask } from "../views/workstreams/model";
import { Badge } from "../theme";
import { PlaceholderBadge } from "../widgets";
import "../views/workstreams/workstreams.css";

/** Sessions shown per task in the tree; older ones are summarised as "+N earlier". */
const MAX_WORKER_ROWS = 3;

interface TreeItem {
  key: string;
  kind: "group" | "task";
  /** Key of the owning group (a group is its own parent key). */
  groupKey: string;
}

const groupKey = (group: WsGroup) => `g:${group.id}`;
const taskKey = (task: WsTask) => `t:${task.projectId}:${task.id}`;

/** Count for the "Workstreams · N" heading; "—" while no list has loaded. */
export function WorkstreamsCount() {
  const model = useWorkstreamModel();
  if (model.status !== "ready") return <span aria-label="workstreams: not available">—</span>;
  return (
    <>
      <span data-testid="workstreams-count">{model.groups.length}</span>
      {model.mode === "fixture" && (
        <Badge tone="placeholder" title={`PLACEHOLDER DATA (${model.provenance.id}): fixture workstream count`}>
          Placeholder
        </Badge>
      )}
    </>
  );
}

function WorkerRows({ task }: { task: WsTask }) {
  if (task.workers.length === 0) {
    return (
      <span className="ws-tree-worker">
        {task.agent} · no session
      </span>
    );
  }
  const shown = task.workers.slice(-MAX_WORKER_ROWS);
  return (
    <>
      {task.workers.length > shown.length && (
        <span className="ws-tree-worker">+{task.workers.length - shown.length} earlier</span>
      )}
      {shown.map((worker) => (
        <span key={worker.sessionId} className="ws-tree-worker" data-testid="tree-worker">
          {worker.adapter} · {worker.state}
        </span>
      ))}
    </>
  );
}

/**
 * Sidebar tree for the `workstreamsTree` shell slot (OX-A4). Groups are workstreams, which
 * under decision D-4 are registered projects; leaves are their tasks with one worker row
 * ("adapter · state") per recorded session. It follows the ARIA tree pattern: one tab stop
 * (roving tabindex), arrows move, Right/Left expand and collapse, Home/End jump, and
 * Enter/Space toggles a group or opens a task. Expanded state lives here, so groups the
 * operator collapsed or opened stay that way across data refreshes. The shell's tree body
 * scrolls independently of the page.
 */
export function WorkstreamTree() {
  const model = useWorkstreamModel();
  const { router } = useServices();
  const route = useStore(router);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [active, setActive] = useState<string | null>(null);
  const refs = useRef(new Map<string, HTMLElement>());

  if (model.status === "loading") return <p className="shell-placeholder">Loading workstreams…</p>;
  if (model.status === "offline") {
    return <p className="shell-placeholder">Offline: no cached workstreams. This is not an empty list.</p>;
  }
  if (model.status === "error") return <p className="shell-placeholder">Workstreams failed to load: {model.message}</p>;
  if (model.status !== "ready") {
    return (
      <p className="shell-placeholder">
        {model.status === "unsupported" ? "Not supported" : "Unavailable"}: {model.reason}
      </p>
    );
  }

  const { groups } = model;
  if (groups.length === 0) {
    return <p className="shell-placeholder">No projects are registered, so there are no workstreams.</p>;
  }
  const isOpen = (group: WsGroup, index: number) => expanded[group.id] ?? index === 0;
  const items: TreeItem[] = groups.flatMap((group, index) => [
    { key: groupKey(group), kind: "group" as const, groupKey: groupKey(group) },
    ...(isOpen(group, index)
      ? group.tasks.map((task) => ({ key: taskKey(task), kind: "task" as const, groupKey: groupKey(group) }))
      : []),
  ]);
  const tabKey = items.some((item) => item.key === active) ? active : items[0]?.key;
  const focusKey = (key: string | undefined) => {
    if (!key) return;
    setActive(key);
    refs.current.get(key)?.focus();
  };
  const setOpen = (group: WsGroup, open: boolean) => setExpanded((current) => ({ ...current, [group.id]: open }));

  const onKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    if (event.ctrlKey || event.altKey || event.metaKey) return;
    const element = (event.target as HTMLElement).closest<HTMLElement>('[role="treeitem"]');
    const key = element?.dataset.key;
    const at = items.findIndex((item) => item.key === key);
    if (!element || at < 0) return;
    const item = items[at]!;
    const groupIndex = groups.findIndex((group) => groupKey(group) === item.groupKey);
    const group = groups[groupIndex]!;
    const handled = () => event.preventDefault();
    switch (event.key) {
      case "ArrowDown":
        handled();
        focusKey(items[Math.min(at + 1, items.length - 1)]?.key);
        break;
      case "ArrowUp":
        handled();
        focusKey(items[Math.max(at - 1, 0)]?.key);
        break;
      case "Home":
        handled();
        focusKey(items[0]?.key);
        break;
      case "End":
        handled();
        focusKey(items[items.length - 1]?.key);
        break;
      case "ArrowRight":
        if (item.kind !== "group") break;
        handled();
        if (!isOpen(group, groupIndex)) setOpen(group, true);
        else if (items[at + 1]?.groupKey === item.groupKey) focusKey(items[at + 1]?.key);
        break;
      case "ArrowLeft":
        handled();
        if (item.kind === "task") focusKey(item.groupKey);
        else if (isOpen(group, groupIndex)) setOpen(group, false);
        break;
      case "Enter":
      case " ":
        if (item.kind === "group") {
          handled();
          setOpen(group, !isOpen(group, groupIndex));
        } else if (event.key === " ") {
          handled();
          element.click();
        }
        break;
    }
  };

  const open = (event: MouseEvent<HTMLAnchorElement>, task: WsTask) => {
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    router.navigate({ name: "task", projectId: task.projectId, taskId: task.id });
  };

  return (
    <div className="ws-tree" data-testid="workstream-tree" data-mode={model.mode}>
      <p className="ws-tree-caption">
        Grouped by project {model.mode === "fixture" && <PlaceholderBadge provenance={model.provenance} />}
        {model.stale && <span> · stale: connection lost</span>}
      </p>
      <ul role="tree" aria-label="Workstreams grouped by project" className="ws-tree-root" onKeyDown={onKeyDown}>
        {groups.map((group, index) => {
          const key = groupKey(group);
          const groupOpen = isOpen(group, index);
          return (
            <li role="none" key={key} className="ws-tree-group">
              <div
                role="treeitem"
                aria-level={1}
                aria-setsize={groups.length}
                aria-posinset={index + 1}
                aria-expanded={groupOpen}
                aria-label={`Workstream ${index + 1} of ${groups.length}: ${group.title}, ${group.tasks.length} tasks`}
                tabIndex={tabKey === key ? 0 : -1}
                data-key={key}
                ref={(node) => {
                  if (node) refs.current.set(key, node);
                  else refs.current.delete(key);
                }}
                className="ws-tree-summary"
                title={group.projectIds.join("\n")}
                onFocus={() => setActive(key)}
                onClick={() => setOpen(group, !groupOpen)}
              >
                <span className="ws-tree-chevron" aria-hidden="true">
                  {groupOpen ? "▾" : "▸"}
                </span>
                <span className="ws-tree-number" aria-hidden="true">
                  {index + 1} / {groups.length}
                </span>
                <span className="ws-tree-title">{group.title}</span>
                <small className="ws-tree-count" aria-hidden="true">
                  {group.tasks.length}
                </small>
              </div>
              {groupOpen && (
                <ul role="group" className="ws-tree-tasks">
                  {group.tasks.length === 0 && <li role="none" className="ws-tree-none">No tasks.</li>}
                  {group.tasks.map((task, position) => {
                    const leafKey = taskKey(task);
                    const common = {
                      role: "treeitem",
                      "aria-level": 2,
                      "aria-setsize": group.tasks.length,
                      "aria-posinset": position + 1,
                      tabIndex: tabKey === leafKey ? 0 : -1,
                      "data-key": leafKey,
                      className: "ws-tree-task",
                      onFocus: () => setActive(leafKey),
                      ref: (node: HTMLElement | null) => {
                        if (node) refs.current.set(leafKey, node);
                        else refs.current.delete(leafKey);
                      },
                    };
                    const body = (
                      <>
                        <span className="ws-tree-task-head">
                          <span className="ws-tree-task-name">{taskLabelOf(task)}</span>
                          <small>{task.state}</small>
                        </span>
                        <WorkerRows task={task} />
                      </>
                    );
                    const current =
                      route.name === "task" && route.taskId === task.id && route.projectId === task.projectId;
                    return (
                      <li role="none" key={leafKey}>
                        {task.openable ? (
                          <a
                            {...common}
                            href={formatRoute({ name: "task", projectId: task.projectId, taskId: task.id })}
                            aria-current={current ? "page" : undefined}
                            onClick={(event) => open(event, task)}
                          >
                            {body}
                          </a>
                        ) : (
                          <span {...common} aria-disabled="true" title="Placeholder task: it has no detail page.">
                            {body}
                          </span>
                        )}
                      </li>
                    );
                  })}
                </ul>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
