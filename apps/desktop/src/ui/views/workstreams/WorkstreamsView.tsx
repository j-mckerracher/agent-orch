import { useId, useState, type FormEvent } from "react";
import type { ReactNode } from "react";
import type { Provenance } from "../../../sources/types";
import { TASK_STATES, basename } from "../../format";
import { useServices } from "../../services";
import { NavLink } from "../../shell/NavLink";
import { Badge, Button, EmptyState, Panel, Pill } from "../../theme";
import { PlaceholderBadge, SimulatedReceipts } from "../../widgets";
import { DependencyGraph } from "./DependencyGraph";
import { useReviewEvidence } from "./evidence";
import {
  ARCHIVED_REASON,
  AddObjectiveForm,
  LifecycleControls,
  LinkTaskForm,
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
import { STATE_TONE } from "./tone";
import "./workstreams.css";

interface Receipt {
  provenance: Provenance;
  notice: string;
}

const FIXTURE_REASON = "Disabled: this preview is not wired to anything. It would change nothing.";
const STALE_REASON = "Disabled: the connection was lost. Changes resume when the daemon is reachable again.";
const UNREAD_REASON = "Disabled: this workstream's details could not be read. Use Refresh to read them again.";
const LIST_REASON = "Disabled: no daemon method orders workstreams, so Prioritize has nothing to call.";
const PLAN_REASON =
  "Disabled: the daemon has no method to split a task or reassign it. Moving a task needs an unlink then a link, which is not atomic, so it is not offered as one action.";
const LIFECYCLE_TONE = { active: "ok", suspended: "warn", archived: "neutral" } as const;

function assignment(task: WsGroup["tasks"][number]): string {
  const latest = task.workers[task.workers.length - 1];
  if (!latest) return `${task.agent} · not dispatched`;
  return `${latest.adapter} · ${latest.state}${task.attempt === null ? "" : ` · attempt ${task.attempt}`}`;
}

/** The `#/workstreams` global view (OX-S4). Register it through `register.tsx`. */
export function WorkstreamsView() {
  const model = useWorkstreamModel();
  const { surfaces } = useServices();
  const headingId = useId();
  const reasonId = useId();
  const mutations = useMutationRunner();
  const [selected, setSelected] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [title, setTitle] = useState("");
  const [receipts, setReceipts] = useState<Receipt[]>([]);
  const [createError, setCreateError] = useState<string | null>(null);

  let body;
  if (model.status === "loading") {
    body = <p role="status">Loading workstreams…</p>;
  } else if (model.status === "offline") {
    body = <p role="status">Offline: no cached workstreams. This is not an empty list.</p>;
  } else if (model.status === "error") {
    body = (
      <p role="alert" className="notice notice-error">
        Workstreams failed to load: {model.message}
      </p>
    );
  } else if (model.status !== "ready") {
    body = (
      <p role="status">
        <PlaceholderBadge provenance={model.provenance} />{" "}
        {model.status === "unsupported" ? "Not supported" : model.status === "unknown" ? "Unknown" : "Unavailable"}:{" "}
        {model.reason}
      </p>
    );
  } else {
    const { groups, mode, unlinked, unlinkedKnown } = model;
    const fixture = mode === "fixture";
    const needle = filter.trim().toLowerCase();
    const visible = groups.filter((group) => group.title.toLowerCase().includes(needle));
    const current = groups.find((group) => group.id === selected) ?? visible[0] ?? groups[0];
    const marker = fixture ? <PlaceholderBadge provenance={model.provenance} /> : null;
    const live = !fixture;
    const writeOff = mutations.busy || model.stale;

    const create = async (event: FormEvent) => {
      event.preventDefault();
      if (title.trim() === "") return;
      if (live) {
        const made = await mutations.run(`Create workstream "${title.trim()}"`, () =>
          surfaces.workstreams.create(title.trim()),
        );
        if (made) {
          setTitle("");
          setSelected(made.id);
          setFilter("");
        }
        return;
      }
      setCreateError(null);
      try {
        const receipt = await surfaces.workstreams.simulateCreate(title.trim());
        setReceipts((all) => [{ provenance: receipt.provenance, notice: receipt.notice }, ...all]);
        setTitle("");
      } catch (error) {
        setCreateError(error instanceof Error ? error.message : "Simulation failed.");
      }
    };

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
        <form className="ws-create" onSubmit={create} aria-label="Create workstream">
          <label>
            New workstream title
            <input
              type="text"
              value={title}
              disabled={writeOff}
              onChange={(event) => setTitle(event.target.value)}
              aria-describedby={model.stale ? reasonId : undefined}
            />
          </label>
          <Button
            type="submit"
            variant="primary"
            disabled={writeOff || title.trim() === ""}
            aria-describedby={model.stale ? reasonId : undefined}
          >
            {fixture ? "Create workstream (simulated)" : "Create workstream"}
          </Button>
          {model.stale && (
            <span className="ws-reason" id={reasonId}>
              {STALE_REASON}
            </span>
          )}
        </form>
        {createError && (
          <p role="alert" className="notice notice-error">
            {createError}
          </p>
        )}
        {receipts.length > 0 && <SimulatedReceipts label="Simulated workstream creations" receipts={receipts} />}
        {live && <OutcomeNotice outcome={mutations.outcome} />}

        {groups.length === 0 ? (
          <EmptyState
            title="No workstreams"
            description={
              fixture
                ? "There are no placeholder workstreams."
                : "No workstreams exist yet. Create one above, then link tasks to it."
            }
          />
        ) : (
          <div className="ws-layout">
            <Panel
              title={
                <>
                  <span data-testid="ws-list-count">{groups.length}</span> {groups.length === 1 ? "workstream" : "workstreams"}
                </>
              }
              headingLevel={2}
              actions={marker}
              className="ws-list"
              aria-label="Workstream list"
            >
              <label className="ws-filter">
                Filter workstreams
                <input type="search" value={filter} onChange={(event) => setFilter(event.target.value)} />
              </label>
              <p role="status" className="ws-subtle" data-testid="ws-showing">
                Showing {visible.length} of {groups.length}
              </p>
              <ul className="ws-items" aria-label="Workstreams">
                {visible.map((group, index) => (
                  <li key={group.id}>
                    <button
                      type="button"
                      className="ws-item"
                      aria-pressed={group.id === current?.id}
                      onClick={() => setSelected(group.id)}
                      title={group.projectIds.join("\n")}
                    >
                      <span className="ws-item-title">
                        {index + 1}. {group.title}
                      </span>
                      <small>
                        {group.unreadReason === null ? taskCount(group.tasks.length) : "tasks unknown"}
                        {group.state !== "active" && ` · ${group.state}`}
                      </small>
                    </button>
                  </li>
                ))}
              </ul>
              <div className="ws-row">
                <Button disabled aria-describedby={`${reasonId}-list`}>
                  Prioritize
                </Button>
              </div>
              <p className="ws-reason" id={`${reasonId}-list`}>
                {LIST_REASON}
              </p>
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
            {current && (
              <Detail
                group={current}
                fixture={fixture}
                marker={marker}
                unlinked={unlinked}
                unlinkedKnown={unlinkedKnown}
                reasonId={`${reasonId}-actions`}
                controls={{
                  run: mutations.run,
                  busy: mutations.busy,
                  locked: model.stale
                    ? STALE_REASON
                    : current.unreadReason !== null
                      ? UNREAD_REASON
                      : current.state === "archived"
                        ? ARCHIVED_REASON
                        : null,
                }}
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
        <p className="ws-eyebrow">Orchestration</p>
        <h1 id={headingId}>Workstreams</h1>
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
      </header>
      {body}
    </section>
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
          {group.unreadReason !== null ? (
            <Badge data-testid="ws-unread">Tasks unknown</Badge>
          ) : (
            <>
              <Badge>{taskCount(group.tasks.length)}</Badge>{" "}
              {counts !== null &&
                TASK_STATES.filter((state) => counts[state] > 0).map((state) => (
                  <Pill key={state} tone={STATE_TONE[state]} data-testid={`count-${state}`}>
                    {counts[state]} {state}
                  </Pill>
                ))}
            </>
          )}
        </p>
        {!fixture && (
          <p className="ws-subtle" data-testid="ws-count-observation">
            {group.counts.status === "ok" ? (
              <>
                D9 counts observed at{" "}
                <time dateTime={group.counts.observedAt}>{group.counts.observedAt}</time>
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

      <Panel title="Task dependencies" headingLevel={2} actions={marker}>
        {group.unreadReason !== null ? <p role="status">Dependencies unknown: tasks could not be read.</p> : <DependencyGraph tasks={group.tasks} />}
      </Panel>

      <Panel title="Tasks" headingLevel={2} actions={marker} className="ws-tasks">
        <p className="ws-subtle">
          Other views of this workstream (Milestones, Context, Deliverables, Activity) are not available yet.
        </p>
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
                  <th scope="col">Project</th>
                  <th scope="col">Objective</th>
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
                      {task.openable ? (
                        <NavLink to={{ name: "task", projectId: task.projectId, taskId: task.id }}>
                          {taskLabelOf(task)}
                        </NavLink>
                      ) : (
                        <>
                          {taskLabelOf(task)} <small>(placeholder: no detail page)</small>
                        </>
                      )}
                    </th>
                    <td title={task.projectId}>{basename(task.projectId)}</td>
                    <td>{objectives.find((objective) => objective.id === task.objectiveId)?.title ?? "Workstream only"}</td>
                    <td>
                      <Pill tone={STATE_TONE[task.state]}>{task.state}</Pill>
                    </td>
                    <td>{assignment(task)}</td>
                    <td>{evidence.get(task.id) ?? task.requiredEvidence}</td>
                    {!fixture && (
                      <td>
                        <UnlinkButton group={group} task={task} controls={controls} />
                      </td>
                    )}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {!fixture && (
          <LinkTaskForm group={group} unlinked={unlinked} unlinkedKnown={unlinkedKnown} controls={controls} />
        )}
        <div className="ws-row" role="group" aria-label="Plan editing">
          {(fixture ? ["Add task", "Split task", "Remove task", "Reassign"] : ["Split task", "Reassign"]).map((label) => (
            <Button key={label} disabled aria-describedby={`${reasonId}-plan`}>
              {label}
            </Button>
          ))}
        </div>
        <p className="ws-reason" id={`${reasonId}-plan`}>
          {fixture ? FIXTURE_REASON : PLAN_REASON}
        </p>
      </Panel>
    </section>
  );
}
