import { useId, useState } from "react";
import type { ReactNode } from "react";
import type { Loadable } from "../../../daemon/appData";
import type { TaskRecord } from "../../../daemon/types";
import type { QueueWait } from "../../../sources/surfaces/types";
import { TASK_STATES, basename, stateLabel } from "../../format";
import { ActionStatus, useAction } from "../../action";
import { useLastingAction } from "../../lastingAction";
import { QueueWaitNote, useQueueWaits } from "../../QueueWaitNote";
import { NavLink } from "../../shell/NavLink";
import { formatAge } from "../../shell/ResourceStrip";
import { Button, Disclosure, EmptyState, Panel, Pill } from "../../theme";
import { DependencyGraph } from "./DependencyGraph";
import { useReviewEvidence } from "./evidence";
import {
  AddObjectiveForm,
  ArchiveTaskButton,
  LinkTaskForm,
  ObjectiveCard,
  UnlinkButton,
  WorkstreamActions,
  type Controls,
  useGroupRunner,
} from "./Manage";
import { taskLabelOf, taskListNotice, workstreamStatus, type WsGroup, type WsTask } from "./model";
import { PlanEditing } from "./PlanEditing";
import { STATE_TONE } from "./tone";
import { WsTabs } from "./WsTabs";

export const FIXTURE_REASON = "Disabled: this preview is not wired to anything. It would change nothing.";
const DEPENDENCY_REASON =
  "Disabled: the daemon has no task-edit, resource-lease or regenerate method. This panel is read-only; a draft can be validated, never applied.";
const NO_OBJECTIVES = "No objectives yet. Add one to set acceptance gates for this workstream.";

function assignment(task: WsTask): string {
  const latest = task.workers[task.workers.length - 1];
  if (!latest) return `${task.agent} · not dispatched`;
  return `${latest.adapter} · ${latest.state}${task.attempt === null ? "" : ` · attempt ${task.attempt}`}`;
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
  if (objectives.status !== "ok") return null;
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

/**
 * Objectives, in two depths. `detailed` is the Objectives tab: every card with its gate controls.
 * The Overview block lists one line per objective. "Add objective" reveals the form in both.
 */
function ObjectivesPanel({
  group,
  locked,
  fixture,
  detailed,
  marker,
  reasonId,
}: {
  group: WsGroup;
  locked: string | null;
  fixture: boolean;
  detailed: boolean;
  marker: ReactNode;
  reasonId: string;
}) {
  const [adding, setAdding] = useState(false);
  const objectives = group.objectives;
  const list = objectives.status === "ok" ? objectives.value : [];
  const off = fixture || locked !== null;
  return (
    <Panel
      title="Objectives"
      headingLevel={2}
      actions={
        <>
          {marker}
          <Button disabled={off} aria-expanded={fixture ? undefined : adding} aria-describedby={off ? reasonId : undefined} onClick={() => setAdding((open) => !open)}>
            + Add objective
          </Button>
        </>
      }
      className="ws-objectives"
    >
      {off && (
        <p className="ws-reason" id={reasonId}>
          {fixture ? FIXTURE_REASON : locked}
        </p>
      )}
      {objectives.status !== "ok" ? (
        <p data-testid="ws-objective">
          Objectives {objectives.status}: {objectives.reason}
        </p>
      ) : (
        <>
          {adding && !off && <AddObjectiveForm group={group} locked={locked} />}
          {list.length === 0 && !detailed && <p className="ws-subtle">{NO_OBJECTIVES}</p>}
          {detailed &&
            (fixture ? (
              <ReadOnlyObjectives group={group} />
            ) : (
              <>
                {list.length === 0 && <p data-testid="ws-objective">No objectives yet.</p>}
                {list.map((objective) => (
                  <ObjectiveCard key={objective.id} objective={objective} tasks={group.tasks} locked={locked} />
                ))}
              </>
            ))}
          {!detailed && list.length > 0 && (
            <ul className="ws-objective-list" aria-label={`Objectives of ${group.title}`}>
              {list.map((objective) => (
                <li key={objective.id}>
                  <strong>{objective.title}</strong>{" "}
                  <span className="ws-subtle">
                    {objective.gates.length === 0
                      ? "no acceptance gates"
                      : `${objective.gates.filter((gate) => gate.state === "verified").length} of ${objective.gates.length} gates verified`}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </Panel>
  );
}

function Stat({ label, value }: { label: string; value: number | null }) {
  return (
    <div className="ws-stat">
      <dt>{label}</dt>
      <dd>{value ?? "—"}</dd>
    </div>
  );
}

/** The Overview's compact task list; the Tasks tab carries the evidence column and the row actions. */
function OverviewTasks({ group }: { group: WsGroup }) {
  if (group.unreadReason !== null) return <p role="status">Tasks unknown: {group.unreadReason}</p>;
  if (group.tasks.length === 0) return <EmptyState title="No tasks" description="This workstream has no tasks yet." />;
  return (
    <div className="ws-table-scroll" role="region" aria-label={`Task summary of ${group.title}`} tabIndex={0}>
      <table className="ws-table">
        <caption className="ws-sr">Task summary of {group.title}</caption>
        <thead>
          <tr>
            <th scope="col">Task</th>
            <th scope="col">Project</th>
            <th scope="col">State</th>
            <th scope="col">Assignee</th>
          </tr>
        </thead>
        <tbody>
          {group.tasks.map((task) => (
            <tr key={task.id} data-testid="ws-overview-task">
              <th scope="row">
                <span className="ws-task-label" title={taskLabelOf(task)}>
                  {taskLabelOf(task)}
                </span>
              </th>
              <td title={task.projectId}>{basename(task.projectId)}</td>
              <td>
                <Pill tone={STATE_TONE[task.state]}>{task.state}</Pill>
              </td>
              <td>{assignment(task)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** What the old page showed as sources, plan revision, observation times and scheduler notes. */
function TechnicalDetails({
  group,
  fixture,
  stale,
  taskList,
}: {
  group: WsGroup;
  fixture: boolean;
  stale: boolean;
  taskList: Loadable<TaskRecord[]>;
}) {
  const counts = group.counts.status === "ok" ? group.counts.value : null;
  return (
    <Disclosure summary="Technical details" count="plan revision, data observation times, scheduler notes" data-testid="ws-technical">
      <ul className="ws-tech">
        <li data-testid="ws-plan-revision">{fixture ? "Plan revision: PLACEHOLDER" : "Plan revision not available"}</li>
        <li data-testid="ws-source">
          {fixture
            ? "Workstreams, objectives, revisions and gates are placeholder data. Nothing here is a daemon record."
            : "Workstreams, objectives, gates and task links are daemon records; task states, workers and dependencies are live daemon data."}
          {stale && <strong> Stale: the connection was lost; this is the last data received.</strong>}
        </li>
        {!fixture && (
          <>
            <li data-testid="ws-task-list-status" role={taskList.status === "error" ? "alert" : "status"}>
              {taskListNotice(taskList)}
            </li>
            <li data-testid="ws-count-observation">
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
            </li>
          </>
        )}
        {counts !== null && (
          <li>
            {TASK_STATES.filter((state) => counts[state] > 0).map((state) => (
              <Pill key={state} tone={STATE_TONE[state]} data-testid={`count-${state}`}>
                {counts[state]} {stateLabel(state)}
              </Pill>
            ))}
          </li>
        )}
        <li>Suspend is advisory: the daemon scheduler does not consult it, so it does not stop dispatch.</li>
        <li>Prioritize moves a workstream to the top of the list. The order is advisory: the scheduler does not consult it.</li>
      </ul>
    </Disclosure>
  );
}

export function WorkstreamDetail({
  group,
  first,
  fixture,
  marker,
  unlinked,
  unlinkedKnown,
  locked,
  taskList,
  stale,
  onBack,
}: {
  group: WsGroup;
  /** The first open workstream: Prioritize has nothing left to do for it. */
  first: boolean;
  fixture: boolean;
  marker: ReactNode;
  unlinked: WsTask[];
  unlinkedKnown: boolean;
  /** Disabled with this reason when set: a stale connection, unread data, or an archived workstream. */
  locked: string | null;
  taskList: Loadable<TaskRecord[]>;
  stale: boolean;
  onBack: () => void;
}) {
  const reasonId = useId();
  const unlink = useAction();
  const archiveTask = useLastingAction();
  const runUnlink = useGroupRunner(unlink);
  const runArchiveTask = useGroupRunner(archiveTask);
  // Unlink and archive share the task table, so while either runs every row's buttons wait for it.
  const rowBusy = unlink.pending || archiveTask.pending;
  const unlinkControls: Controls = { run: runUnlink, busy: rowBusy, locked };
  const archiveControls: Controls = { run: runArchiveTask, busy: rowBusy, locked };
  const decision = group.tasks.find((task) => task.pendingDecision);
  const evidence = useReviewEvidence(group.tasks, !fixture);
  const waits = useQueueWaits(!fixture && group.tasks.some((task) => task.state === "Queued"));
  const objectives = group.objectives.status === "ok" ? group.objectives.value : [];
  const known = group.unreadReason === null;
  const status = workstreamStatus(group);
  const stateCount = (state: WsTask["state"]) => (known ? group.tasks.filter((task) => task.state === state).length : null);

  const tasksTab = (
    <>
      {/* Above the table: an archived or unlinked row leaves the table, and its result must stay. */}
      <ActionStatus action={unlink} />
      <ActionStatus action={archiveTask} />
      {!known ? (
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
                        <NavLink to={{ name: "task", projectId: task.projectId, taskId: task.id }}>{taskLabelOf(task)}</NavLink>
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
                      <UnlinkButton group={group} task={task} controls={unlinkControls} />
                      <ArchiveTaskButton task={task} controls={archiveControls} />
                    </td>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {!fixture && <LinkTaskForm group={group} unlinked={unlinked} unlinkedKnown={unlinkedKnown} locked={locked} />}
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
        <PlanEditing group={group} locked={locked} />
      )}
    </>
  );

  return (
    <section className="ws-detail" aria-label={`Workstream ${group.title}`} data-testid="ws-detail">
      <header className="ws-detail-head">
        <button type="button" className="ws-back" onClick={onBack}>
          <span aria-hidden="true">←</span> Workstreams
        </button>
        <div className="ws-title-row">
          <h1>{group.title}</h1>
          <Pill tone={status.tone} data-testid="ws-status">
            {status.label}
          </Pill>
          {marker}
          <span className="ws-spacer" />
          <WorkstreamActions group={group} first={first} locked={locked} fixtureReason={fixture ? FIXTURE_REASON : null} />
        </div>
        {(group.summary !== "" || group.projectIds.length > 0) && (
          <p className="ws-subtle" data-testid="ws-summary">
            {group.summary}
            {group.summary !== "" && group.projectIds.length > 0 && " "}
            {group.projectIds.length > 0 && `Projects: ${group.projectIds.map(basename).join(", ")}`}
          </p>
        )}
        {!known && (
          <p role="status" data-testid="ws-unread-reason">
            {group.unreadReason} Its tasks and objectives are unknown, not empty.
          </p>
        )}
      </header>

      <WsTabs
        label={`Views of ${group.title}`}
        tabs={[
          {
            id: "overview",
            label: "Overview",
            content: (
              <>
                {decision && (
                  <div className="ps-notice ps-notice-warn ws-banner" data-testid="ws-banner">
                    <span className="ps-notice-mark" aria-hidden="true">
                      !
                    </span>
                    <div className="ps-notice-body">
                      <strong>A task is waiting on your decision</strong>
                      <p className="ws-subtle">Review the pending approval to let work continue.</p>
                    </div>
                    <NavLink className="ws-link-button" to={{ name: "global", view: "review", taskId: decision.id }}>
                      Resolve decision
                    </NavLink>
                  </div>
                )}
                <dl className="ws-stats">
                  <Stat label="Tasks" value={known ? group.tasks.length : null} />
                  <Stat label="Succeeded" value={stateCount("Succeeded")} />
                  <Stat label="Running" value={stateCount("Running")} />
                </dl>
                <ObjectivesPanel group={group} locked={locked} fixture={fixture} detailed={false} marker={null} reasonId={`${reasonId}-add`} />
                <Panel title="Tasks" headingLevel={2} className="ws-overview-tasks">
                  <OverviewTasks group={group} />
                </Panel>
              </>
            ),
          },
          { id: "tasks", label: known ? `Tasks ${group.tasks.length}` : "Tasks", content: tasksTab },
          {
            id: "objectives",
            label: group.objectives.status === "ok" ? `Objectives ${objectives.length}` : "Objectives",
            content: (
              <ObjectivesPanel group={group} locked={locked} fixture={fixture} detailed marker={null} reasonId={`${reasonId}-add-tab`} />
            ),
          },
          {
            id: "dependencies",
            label: "Dependencies",
            content: !known ? (
              <p role="status">Dependencies unknown: tasks could not be read.</p>
            ) : (
              <>
                <DependencyGraph tasks={group.tasks} />
                <DependencyDetail group={group} reasonId={`${reasonId}-dependency`} />
              </>
            ),
          },
          { id: "activity", label: "Activity", content: <ActivityPane group={group} waits={waits} /> },
        ]}
      />
      <TechnicalDetails group={group} fixture={fixture} stale={stale} taskList={taskList} />
    </section>
  );
}
