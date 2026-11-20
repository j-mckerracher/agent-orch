/** S2: a project's page: its task list first, with Coordinator and Defaults one tab away. */
import { useState } from "react";
import { Coordinator } from "./Coordinator";
import { Crumbs } from "./ConnectionStrip";
import { basename, pendingApprovals, taskBucket } from "./format";
import { NewTaskDialog } from "./NewTaskDialog";
import { ProjectDefaultsSection } from "./ProjectDefaultsSection";
import type { TaskFilter } from "./router";
import { useServices, useStore, useSurface } from "./services";
import { TaskList } from "./TaskList";
import { Button } from "./theme";
import { ActionMenu } from "./views/workstreams/ActionMenu";
import { Notice } from "./widgets";
import "./projectScreens.css";

export type ProjectTab = "tasks" | "coordinator" | "defaults";

const TAB_LABELS: Record<ProjectTab, string> = { tasks: "Tasks", coordinator: "Coordinator", defaults: "Defaults" };

export function ProjectDetail({ projectId, tab, filter = "open" }: { projectId: string; tab: ProjectTab; filter?: TaskFilter }) {
  const { data, router, surfaces } = useServices();
  const { projects, tasks, approvals } = useStore(data);
  const [creating, setCreating] = useState(false);
  const defaults = useSurface(
    () => surfaces.projectDefaults.get(projectId),
    `project-defaults:${projectId}`,
    surfaces.projectDefaults.provenance,
  );

  const row = projects.data?.find((project) => project.id === projectId);
  const known = projects.data ? row !== undefined : null;
  // A registered project has no coordinator until the user adds one in the Workspace (DPES-1105).
  const hasCoordinator = row?.coordinator_active !== false;
  // A task of an in-place project (the home directory) has nothing to review or land, so only a report is possible.
  const inPlace = row?.in_place === true;
  const own = tasks.data?.filter((task) => task.contract.project_id === projectId) ?? null;
  const ownIds = new Set(own?.map((task) => task.contract.id));
  // Pending review needs both lists: without the task list no approval can be matched to this project.
  const pending =
    tasks.data && approvals.data
      ? pendingApprovals(approvals.data).filter((approval) => ownIds.has(approval.action.task_id))
      : null;
  const awaiting = own && approvals.data ? own.filter((task) => taskBucket(task, approvals.data ?? []) === "review").length : 0;
  const summary = own
    ? `${own.length} ${own.length === 1 ? "task" : "tasks"}${awaiting > 0 ? ` · ${awaiting} awaiting review` : ""}`
    : null;

  const tabs: ProjectTab[] = [
    "tasks",
    ...(hasCoordinator ? (["coordinator"] as const) : []),
    ...(defaults.status === "unsupported" ? [] : (["defaults"] as const)),
  ];
  const show = (next: ProjectTab) =>
    router.navigate(next === "tasks" ? { name: "project", projectId } : { name: next, projectId });

  return (
    <main className="ps-page">
      <Crumbs
        items={[
          { label: "Global tasks", onClick: () => router.navigate({ name: "global", view: "workstreams" }) },
          { label: basename(projectId) },
        ]}
      />
      <header className="pt-head">
        <div className="pt-title-row">
          <h1>{basename(projectId)}</h1>
          {summary && <span className="muted" data-testid="task-total">{summary}</span>}
          <span className="pt-spacer" />
          <ActionMenu
            label="Project actions"
            trigger={<span aria-hidden="true">⋯</span>}
            items={[
              {
                label: pending ? `Pending review (${pending.length})` : "Pending review",
                onSelect: () => router.navigate({ name: "global", view: "review", projectId }),
              },
            ]}
          />
          <Button variant="primary" onClick={() => setCreating(true)}>
            New task
          </Button>
        </div>
        {known === false && (
          <Notice tone="bad" role="alert">
            This repository is not in the returned project list; it may have been removed. Refresh to confirm.
          </Notice>
        )}
      </header>
      <div role="tablist" aria-label="Project pages" className="pt-tabs">
        {tabs.map((id) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={id === tab}
            className="pt-tab"
            onClick={() => show(id)}
          >
            {TAB_LABELS[id]}
          </button>
        ))}
      </div>
      <div role="tabpanel" aria-label={TAB_LABELS[tab]}>
        {tab === "tasks" && <TaskList projectId={projectId} filter={filter} />}
        {tab === "coordinator" && <Coordinator projectId={projectId} />}
        {tab === "defaults" && (
          <>
            <p className="muted">
              Repository: <code>{projectId}</code>
            </p>
            <ProjectDefaultsSection key={projectId} projectId={projectId} loaded={defaults} />
          </>
        )}
      </div>
      {creating && <NewTaskDialog projectId={projectId} inPlace={inPlace} onClose={() => setCreating(false)} />}
    </main>
  );
}
