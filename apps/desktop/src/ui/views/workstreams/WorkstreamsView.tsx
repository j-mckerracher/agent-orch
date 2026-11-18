import { useEffect, useId, useState } from "react";
import type { ReactNode } from "react";
import type { QueueWait } from "../../../sources/surfaces/types";
import { TASK_STATES, basename, stateLabel } from "../../format";
import { QueueWaitNote, useQueueWaits } from "../../QueueWaitNote";
import { useServices, useStore } from "../../services";
import { NavLink } from "../../shell/NavLink";
import { formatAge } from "../../shell/ResourceStrip";
import { Badge, Button, EmptyState, Panel, Pill } from "../../theme";
import { PlaceholderBadge } from "../../widgets";
import { DependencyGraph } from "./DependencyGraph";
import { useReviewEvidence } from "./evidence";
import {
  ARCHIVED_REASON,
  AddObjectiveForm,
  ArchiveFinished,
  ArchiveTaskButton,
  LifecycleControls,
  LinkTaskForm,
  ListActions,
  ObjectiveCard,
  OutcomeNotice,
  UnlinkButton,
  useMutationRunner,
  type Controls,
} from "./Manage";
import {
  reloadWorkstreams,
  taskCount,
  taskLabelOf,
  taskListNotice,
  useWorkstreamModel,
  type WsGroup,
  type WsTask,
} from "./model";
import { PlanEditing } from "./PlanEditing";
import { STATE_TONE } from "./tone";
import { WsTabs } from "./WsTabs";
import "./workstreams.css";

const FIXTURE_REASON = "Disabled: this preview is not wired to anything. It would change nothing.";
const STALE_REASON = "Disabled: the connection was lost. Changes resume when the daemon is reachable again.";
const UNREAD_REASON = "Disabled: this workstream's details could not be read. Use Refresh to read them again.";
const DEPENDENCY_REASON =
  "Disabled: the daemon has no task-edit, resource-lease or regenerate method. This panel is read-only; a draft can be validated, never applied.";
const LIFECYCLE_TONE = { active: "ok", suspended: "warn", archived: "neutral" } as const;

function assignment(task: WsGroup["tasks"][number]): string {
  const latest = task.workers[task.workers.length - 1];
  if (!latest) return `${task.agent} · not dispatched`;
  return `${latest.adapter} · ${latest.state}${task.attempt === null ? "" : ` · attempt ${task.attempt}`}`;
}

/** The `#/workstreams` global view (OX-S4). Register it through `register.tsx`. */
export function WorkstreamsView() {
  const model = useWorkstreamModel();
  const { surfaces, router } = useServices();
  const headingId = useId();
  const reasonId = useId();
  const mutations = useMutationRunner();
  // A route that names a workstream selects it on first render and whenever navigation names one (DPES-1105).
  const route = useStore(router);
  const routeWorkstream = route.name === "global" ? route.workstreamId : undefined;
  const [selected, setSelected] = useState<string | null>(routeWorkstream ?? null);
  useEffect(() => {
    if (route.name === "global" && route.workstreamId !== undefined) setSelected(route.workstreamId);
  }, [route]);
  const [filter, setFilter] = useState("");

  let body;
  if (model.status === "loading") {
    body = <p role="status">Loading workstreams…</p>;
  } else if (model.status === "offline") {
    body = (
      <p role="status">
        Offline: no cached workstreams. This is not an empty list. · observed{" "}
        <time dateTime={model.observedAt}>{formatAge(model.observedAt, model.ageMs, Date.now())}</time>
      </p>
    );
  } else if (model.status === "error") {
    body = (
      <p role="alert" className="notice notice-error">
        Workstreams failed to load: {model.message} · observed{" "}
        <time dateTime={model.observedAt}>{formatAge(model.observedAt, model.ageMs, Date.now())}</time>
      </p>
    );
  } else if (model.status !== "ready") {
    body = (
      <p role="status">
        <PlaceholderBadge provenance={model.provenance} />{" "}
        {model.status === "unsupported" ? "Not supported" : model.status === "unknown" ? "Unknown" : "Unavailable"}:{" "}
        {model.reason} · observed <time dateTime={model.observedAt}>{formatAge(model.observedAt, model.ageMs, Date.now())}</time>
      </p>
    );
  } else {
    const { groups, mode, unlinked, unlinkedKnown } = model;
    const fixture = mode === "fixture";
    const needle = filter.trim().toLowerCase();
    // Archived workstreams move to their own box under the initiatives.
    const open = groups.filter((group) => group.state !== "archived");
    const archived = groups.filter((group) => group.state === "archived");
    const matches = (group: WsGroup) => group.title.toLowerCase().includes(needle);
    const visible = open.filter(matches);
    const visibleArchived = archived.filter(matches);
    const current = groups.find((group) => group.id === selected) ?? visible[0] ?? groups[0];
    const marker = fixture ? <PlaceholderBadge provenance={model.provenance} compact /> : null;
    const live = !fixture;
    const controls: Controls | null = current
      ? {
          // Pin the selection first: archiving moves the selected workstream out of the open list,
          // and an implicit "first open workstream" selection would jump to another one.
          run: (label, send) => {
            setSelected(current.id);
            return mutations.run(label, send);
          },
          busy: mutations.busy,
          locked: model.stale
            ? STALE_REASON
            : current.unreadReason !== null
              ? UNREAD_REASON
              : current.state === "archived"
                ? ARCHIVED_REASON
                : null,
        }
      : null;

    const item = (group: WsGroup, label: string) => (
      <li key={group.id}>
        <button
          type="button"
          className="ws-item"
          aria-pressed={group.id === current?.id}
          onClick={() => setSelected(group.id)}
          title={group.projectIds.join("\n")}
        >
          <span className="ws-item-title">{label}</span>
          <small>
            {group.projectIds.length > 0 && `${group.projectIds.length} ${group.projectIds.length === 1 ? "project" : "projects"} · `}
            {group.unreadReason === null ? taskCount(group.tasks.length) : "tasks unknown"}
            {group.state === "suspended" && " · suspended"}
          </small>
        </button>
      </li>
    );

    body = (
      <>
        <p className="ws-source" data-testid="ws-source">
          {fixture
            ? "Workstreams, objectives, revisions and gates are placeholder data. Nothing here is a daemon record."
            : "Workstreams, objectives, gates and task links are daemon records; task states, workers and dependencies are live daemon data."}
          {model.stale && <strong> Stale: the connection was lost; this is the last data received.</strong>}
        </p>
        {live && (
          <p
            className="ws-source"
            data-testid="ws-task-list-status"
            role={model.taskList.status === "error" ? "alert" : "status"}
          >
            {taskListNotice(model.taskList)}
          </p>
        )}
        {live && <OutcomeNotice outcome={mutations.outcome} />}

        {groups.length === 0 ? (
          <EmptyState
            title="No workstreams"
            description={
              fixture
                ? "There are no placeholder workstreams."
                : "No workstreams exist yet. Ask the global orchestrator to create one, then link tasks to it."
            }
          />
        ) : (
          <div className="ws-layout">
            <div className="ws-side">
            <Panel
              title={
                <>
                  <span data-testid="ws-list-count">{open.length}</span> {open.length === 1 ? "initiative" : "initiatives"}
                </>
              }
              headingLevel={2}
              className="ws-list"
              aria-label="Workstream list"
            >
              {marker && <div className="ws-list-marker">{marker}</div>}
              <label className="ws-filter">
                Filter workstreams
                <input type="search" value={filter} onChange={(event) => setFilter(event.target.value)} />
              </label>
              <p role="status" className="ws-subtle" data-testid="ws-showing">
                Showing {visible.length} of {open.length}
              </p>
              <ul className="ws-items" aria-label="Workstreams">
                {visible.map((group, index) => item(group, `${index + 1}. ${group.title}`))}
              </ul>
              {fixture ? (
                <>
                  <div className="ws-row">
                    <Button disabled aria-describedby={`${reasonId}-list`}>
                      Prioritize
                    </Button>
                    <Button disabled aria-describedby={`${reasonId}-list`}>
                      Archive
                    </Button>
                  </div>
                  <p className="ws-reason" id={`${reasonId}-list`}>
                    {FIXTURE_REASON}
                  </p>
                </>
              ) : (
                current &&
                controls && <ListActions group={current} first={current.id === open[0]?.id} controls={controls} />
              )}
              {live && !unlinkedKnown && (
                <p className="ws-subtle" data-testid="ws-unlinked">
                  Tasks not in any workstream: unknown.
                </p>
              )}
              {live && unlinked.length > 0 && (
                <p className="ws-subtle" data-testid="ws-unlinked">
                  {taskCount(unlinked.length)} not in any workstream.
                </p>
              )}
            </Panel>
            {archived.length > 0 && (
              <Panel
                title={
                  <>
                    <span data-testid="ws-archived-count">{archived.length}</span> archived
                  </>
                }
                headingLevel={2}
                className="ws-list"
                aria-label="Archived workstreams"
              >
                <ul className="ws-items" aria-label="Archived workstreams">
                  {visibleArchived.map((group) => item(group, group.title))}
                </ul>
              </Panel>
            )}
            </div>
            {current && controls && (
              <Detail
                group={current}
                fixture={fixture}
                marker={marker}
                unlinked={unlinked}
                unlinkedKnown={unlinkedKnown}
                reasonId={`${reasonId}-actions`}
                controls={controls}
              />
            )}
          </div>
        )}
      </>
    );
  }

  return (
    <section className="ws-view" aria-labelledby={headingId} data-testid="workstreams-view">
      <header className="ws-head">
        <div className="ws-head-title">
          <p className="ws-eyebrow">Continuing initiatives</p>
          <h1 id={headingId}>Workstreams</h1>
          <p className="ws-head-summary">Explicit objectives, linked projects, validated plans, and evidence.</p>
        </div>
        <div className="ws-head-actions">
          <NavLink className="ws-link-button" to={{ name: "projects" }}>
            Manage projects
          </NavLink>
          {surfaces.workstreams.provenance.kind !== "fixture" && (
            <ArchiveFinished run={mutations.run} busy={mutations.busy} />
          )}
          {surfaces.workstreams.provenance.kind !== "fixture" && (
            <Button
              disabled={mutations.busy}
              onClick={() => {
                mutations.clear();
                reloadWorkstreams();
              }}
            >
              Refresh
            </Button>
          )}
        </div>
      </header>
      {body}
    <p className="ws-purpose">Purpose and rationale: organize outcomes across repositories, expose parallelism, and change plans without losing context. Draft plan edits, bulk operations and coordinator recommendations must pass deterministic validation before application.</p>
    </section>
  );
}

/**
 * OX-W7 "Dependency and resource detail": read-only. It shows each task's predecessors and
 * their states from the loaded task records; edit, regenerate and lease actions are disabled
 * with a reason because the daemon has no method for them.
 */
function DependencyDetail({ group, reasonId }: { group: WsGroup; reasonId: string }) {
  const byId = new Map(group.tasks.map((task) => [task.id, task]));
  const rows = group.tasks.filter((task) => task.dependencies.length > 0);
  return (
    <details className="ws-dependency-detail" data-testid="ws-dependency-detail">
      <summary>Dependency and resource detail</summary>
      {rows.length === 0 ? (
        <p className="ws-subtle">No task in this workstream waits on a predecessor.</p>
      ) : (
        <ul className="ws-dependency-list" aria-label={`Predecessors of ${group.title}`}>
          {rows.map((task) => (
            <li key={task.id}>
              <strong>{taskLabelOf(task)}</strong> waits on{" "}
              {task.dependencies.map((dependency, index) => {
                const predecessor = byId.get(dependency);
                return (
                  <span key={dependency}>
                    {index > 0 && ", "}
                    {predecessor ? `${taskLabelOf(predecessor)} (${predecessor.state})` : `${dependency} (not in this workstream)`}
                  </span>
                );
              })}
            </li>
          ))}
        </ul>
      )}
      <div className="ws-row" role="group" aria-label="Dependency actions">
        {["Edit predecessors", "Ask coordinator to regenerate subplan", "Validate draft before apply"].map((label) => (
          <Button key={label} disabled aria-describedby={reasonId}>
            {label}
          </Button>
        ))}
      </div>
      <p className="ws-reason" id={reasonId}>
        {DEPENDENCY_REASON}
      </p>
    </details>
  );
}

function ActivityPane({ group, waits }: { group: WsGroup; waits: ReadonlyMap<string, QueueWait> }) {
  if (group.unreadReason !== null) return <p role="status">Activity unknown: tasks could not be read.</p>;
  if (group.tasks.length === 0) return <EmptyState title="No activity" description="This workstream has no tasks yet." />;
  return (
    <>
      <p className="ws-subtle">
        Derived from each task's state and recorded sessions. The session event stream is on the task detail.
      </p>
      <ul className="ws-activity" aria-label={`Activity of ${group.title}`}>
        {group.tasks.map((task) => (
          <li key={task.id} data-testid="ws-activity-row">
            <strong>{taskLabelOf(task)}</strong> <Pill tone={STATE_TONE[task.state]}>{task.state}</Pill>{" "}
            <span className="ws-subtle">{assignment(task)}</span>
            <QueueWaitNote state={task.state} wait={waits.get(task.id)} />
          </li>
        ))}
      </ul>
    </>
  );
}

function ReadOnlyObjectives({ group }: { group: WsGroup }) {
  const objectives = group.objectives;
  if (objectives.status !== "ok") {
    return (
      <p data-testid="ws-objective">
        Objectives {objectives.status}: {objectives.reason}
      </p>
    );
  }
  if (objectives.value.length === 0) return <p data-testid="ws-objective">No objectives.</p>;
  return (
    <>
      {objectives.value.map((objective) => (
        <section key={objective.id} className="ws-objective-card" aria-label={`Objective ${objective.title}`} data-testid="ws-objective">
          <h3>{objective.title}</h3>
          {objective.description !== "" && <p>{objective.description}</p>}
          <p className="ws-subtle">
            Revision {objective.revision} · {objective.gates.filter((gate) => gate.state === "verified").length} of{" "}
            {objective.gates.length} gates verified
          </p>
          <ul className="ws-gates" aria-label={`Acceptance gates of ${objective.title}`}>
            {objective.gates.map((gate) => (
              <li key={gate.id} data-testid="ws-gate">
                <Pill tone={gate.state === "verified" ? "ok" : gate.state === "failed" ? "bad" : "warn"}>{gate.state}</Pill>{" "}
                {gate.label}
                {gate.evidenceTaskId !== null && <small> · evidence: {gate.evidenceTaskId}</small>}
              </li>
            ))}
          </ul>
        </section>
      ))}
    </>
  );
}

function Detail({
  group,
  fixture,
  marker,
  unlinked,
  unlinkedKnown,
  reasonId,
  controls,
}: {
  group: WsGroup;
  fixture: boolean;
  marker: ReactNode;
  unlinked: WsTask[];
  unlinkedKnown: boolean;
  reasonId: string;
  controls: Controls;
}) {
  const decision = group.tasks.find((task) => task.pendingDecision);
  const evidence = useReviewEvidence(group.tasks, !fixture);
  const waits = useQueueWaits(!fixture && group.tasks.some((task) => task.state === "Queued"));
  const objectives = group.objectives.status === "ok" ? group.objectives.value : [];
  const counts = group.counts.status === "ok" ? group.counts.value : null;
  return (
    <section className="ws-detail" aria-label={`Workstream ${group.title}`} data-testid="ws-detail">
      <Panel title={group.title} headingLevel={2} actions={marker} className="ws-summary">
        <p className="ws-tags">
          <Pill tone={LIFECYCLE_TONE[group.state]} data-testid="ws-lifecycle">
            {group.state}
          </Pill>{" "}
          {group.projectIds.length > 0 && <Badge>Projects: {group.projectIds.map(basename).join(", ")}</Badge>}{" "}
          <Badge data-testid="ws-plan-revision">{fixture ? "Plan revision: PLACEHOLDER" : "Plan revision not available"}</Badge>{" "}
          {group.unreadReason !== null ? (
            <Badge data-testid="ws-unread">Tasks unknown</Badge>
          ) : (
            <>
              <Badge>{taskCount(group.tasks.length)}</Badge>{" "}
              {counts !== null &&
                TASK_STATES.filter((state) => counts[state] > 0).map((state) => (
                  <Pill key={state} tone={STATE_TONE[state]} data-testid={`count-${state}`}>
                    {counts[state]} {stateLabel(state)}
                  </Pill>
                ))}
            </>
          )}
        </p>
        {!fixture && (
          <p className="ws-subtle" data-testid="ws-count-observation">
            {group.counts.status === "ok" ? (
              <>
                D9 counts observed{" "}
                <time dateTime={group.counts.observedAt}>
                  {formatAge(group.counts.observedAt, group.counts.ageMs, Date.now())}
                </time>
              </>
            ) : (
              `D9 counts ${group.counts.status}: ${group.counts.reason}`
            )}
          </p>
        )}
        {group.unreadReason !== null && (
          <p role="status" data-testid="ws-unread-reason">
            {group.unreadReason} Its tasks and objectives are unknown, not empty.
          </p>
        )}
        {group.summary !== "" && <p data-testid="ws-summary">{group.summary}</p>}
        {fixture ? (
          <div className="ws-row" role="group" aria-label="Workstream actions">
            {["Revise objective", "Add objective", "Suspend new dispatch"].map((label) => (
              <Button key={label} disabled aria-describedby={reasonId}>
                {label}
              </Button>
            ))}
            <p className="ws-reason" id={reasonId}>
              {FIXTURE_REASON}
            </p>
          </div>
        ) : (
          <LifecycleControls group={group} controls={controls} />
        )}
        {decision ? (
          <NavLink className="ws-link-button" to={{ name: "review", projectId: decision.projectId, taskId: decision.id }}>
            Resolve contract decision
          </NavLink>
        ) : (
          <>
            <Button disabled aria-describedby={`${reasonId}-decision`}>
              Resolve contract decision
            </Button>
            <p className="ws-reason" id={`${reasonId}-decision`}>
              Resolve contract decision becomes a link when a task has a pending approval.
            </p>
          </>
        )}
      </Panel>

      <Panel title="Objectives" headingLevel={2} actions={marker} className="ws-objectives">
        {fixture ? (
          <ReadOnlyObjectives group={group} />
        ) : group.objectives.status !== "ok" ? (
          <p data-testid="ws-objective">
            Objectives {group.objectives.status}: {group.objectives.reason}
          </p>
        ) : (
          <>
            {objectives.length === 0 && <p data-testid="ws-objective">No objectives yet.</p>}
            {objectives.map((objective) => (
              <ObjectiveCard key={objective.id} objective={objective} tasks={group.tasks} controls={controls} />
            ))}
            <AddObjectiveForm group={group} controls={controls} />
          </>
        )}
      </Panel>

      <WsTabs
        label={`Views of ${group.title}`}
        actions={marker}
        tabs={[
          {
            id: "tasks",
            label: "Tasks",
            content: (
              <>
        {group.unreadReason !== null ? (
          <p role="status">Tasks unknown: {group.unreadReason}</p>
        ) : group.tasks.length === 0 ? (
          <EmptyState title="No tasks" description="This workstream has no tasks yet." />
        ) : (
          <div className="ws-table-scroll" role="region" aria-label={`Tasks of ${group.title}`} tabIndex={0}>
            <table className="ws-table">
              <caption className="ws-sr">Tasks of {group.title}</caption>
              <thead>
                <tr>
                  <th scope="col">Task</th>
                  <th scope="col">Objective / project</th>
                  <th scope="col">Task state</th>
                  <th scope="col">Assignment</th>
                  <th scope="col">Required next evidence</th>
                  {!fixture && <th scope="col">Link</th>}
                </tr>
              </thead>
              <tbody>
                {group.tasks.map((task) => (
                  <tr key={task.id} data-testid="task-row">
                    <th scope="row">
                      <span className="ws-task-label" title={taskLabelOf(task)}>
                        {task.openable ? (
                          <NavLink to={{ name: "task", projectId: task.projectId, taskId: task.id }}>
                            {taskLabelOf(task)}
                          </NavLink>
                        ) : (
                          taskLabelOf(task)
                        )}
                      </span>
                      {!task.openable && <small>(placeholder: no detail page)</small>}
                    </th>
                    <td title={task.projectId}>
                      {objectives.find((objective) => objective.id === task.objectiveId)?.title ?? "Workstream only"} /{" "}
                      {basename(task.projectId)}
                    </td>
                    <td>
                      <Pill tone={STATE_TONE[task.state]}>{task.state}</Pill>
                      <QueueWaitNote state={task.state} wait={waits.get(task.id)} />
                    </td>
                    <td>{assignment(task)}</td>
                    <td>{evidence.get(task.id) ?? task.requiredEvidence}</td>
                    {!fixture && (
                      <td className="ws-table-action">
                        <UnlinkButton group={group} task={task} controls={controls} />
                        <ArchiveTaskButton task={task} controls={controls} />
                      </td>
                    )}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {group.unreadReason === null && <DependencyDetail group={group} reasonId={`${reasonId}-dependency`} />}
        {!fixture && (
          <LinkTaskForm group={group} unlinked={unlinked} unlinkedKnown={unlinkedKnown} controls={controls} />
        )}
        {fixture ? (
          <>
            <div className="ws-row" role="group" aria-label="Plan editing">
              {["Add task", "Split task", "Combine tasks", "Remove task", "Reassign", "Change execution policy"].map((label) => (
                <Button key={label} disabled aria-describedby={`${reasonId}-plan`}>
                  {label}
                </Button>
              ))}
            </div>
            <p className="ws-reason" id={`${reasonId}-plan`}>
              {FIXTURE_REASON}
            </p>
          </>
        ) : (
          <PlanEditing group={group} controls={controls} />
        )}
              </>
            ),
          },
          {
            id: "dependencies",
            label: "Dependencies",
            content:
              group.unreadReason !== null ? (
                <p role="status">Dependencies unknown: tasks could not be read.</p>
              ) : (
                <DependencyGraph tasks={group.tasks} />
              ),
          },
          { id: "activity", label: "Activity", content: <ActivityPane group={group} waits={waits} /> },
        ]}
      />
    </section>
  );
}
