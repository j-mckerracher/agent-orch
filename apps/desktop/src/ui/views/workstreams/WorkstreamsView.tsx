import { useId, useState, type FormEvent } from "react";
import type { ReactNode } from "react";
import type { Provenance } from "../../../sources/types";
import { TASK_STATES, basename } from "../../format";
import { useServices } from "../../services";
import { NavLink } from "../../shell/NavLink";
import { Badge, Button, EmptyState, Panel, Pill } from "../../theme";
import { PlaceholderBadge, SimulatedReceipts } from "../../widgets";
import { DependencyGraph } from "./DependencyGraph";
import { taskLabelOf, useWorkstreamModel, type WsGroup } from "./model";
import { STATE_TONE } from "./tone";
import "./workstreams.css";

interface Receipt {
  provenance: Provenance;
  notice: string;
}

const LIVE_REASON =
  "Disabled: the daemon has no workstream objectives, revisions or plan editing. Only tasks and their dependencies are real.";
const FIXTURE_REASON = "Disabled: this preview is not wired to anything. It would change nothing.";

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
    const { groups, mode } = model;
    const fixture = mode === "fixture";
    const reason = fixture ? FIXTURE_REASON : LIVE_REASON;
    const needle = filter.trim().toLowerCase();
    const visible = groups.filter((group) => group.title.toLowerCase().includes(needle));
    const current = groups.find((group) => group.id === selected) ?? visible[0] ?? groups[0];
    const marker = fixture ? <PlaceholderBadge provenance={model.provenance} /> : null;

    const create = async (event: FormEvent) => {
      event.preventDefault();
      if (!fixture || title.trim() === "") return;
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
            : "Interim grouping (decision D-4): one workstream per registered project. Tasks, states, workers and dependencies are live daemon data."}
          {model.stale && <strong> Stale: the connection was lost; this is the last data received.</strong>}
        </p>
        <form className="ws-create" onSubmit={create} aria-label="Create workstream">
          <label>
            New workstream title
            <input
              type="text"
              value={title}
              disabled={!fixture}
              onChange={(event) => setTitle(event.target.value)}
              aria-describedby={fixture ? undefined : reasonId}
            />
          </label>
          <Button type="submit" variant="primary" disabled={!fixture || title.trim() === ""}>
            {fixture ? "Create workstream (simulated)" : "Create workstream"}
          </Button>
          {!fixture && (
            <span className="ws-reason" id={reasonId}>
              Disabled: the daemon cannot create workstreams; a workstream is a registered project. Add a project instead.
            </span>
          )}
        </form>
        {createError && (
          <p role="alert" className="notice notice-error">
            {createError}
          </p>
        )}
        {receipts.length > 0 && <SimulatedReceipts label="Simulated workstream creations" receipts={receipts} />}

        {groups.length === 0 ? (
          <EmptyState
            title="No workstreams"
            description="No projects are registered. Registering a project creates its workstream."
          />
        ) : (
          <div className="ws-layout">
            <Panel
              title={
                <>
                  <span data-testid="ws-list-count">{groups.length}</span> {groups.length === 1 ? "initiative" : "initiatives"}
                </>
              }
              headingLevel={3}
              actions={marker}
              className="ws-list"
              aria-label="Workstream list"
            >
              <p className="ws-subtle">Grouped by project.</p>
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
                      <small>{group.tasks.length} tasks</small>
                    </button>
                  </li>
                ))}
              </ul>
              <div className="ws-row">
                <Button disabled aria-describedby={`${reasonId}-list`}>
                  Prioritize
                </Button>
                <Button disabled aria-describedby={`${reasonId}-list`}>
                  Archive
                </Button>
              </div>
              <p className="ws-reason" id={`${reasonId}-list`}>
                Disabled: no daemon method orders or archives workstreams.
              </p>
            </Panel>
            {current && <Detail group={current} fixture={fixture} reason={reason} marker={marker} reasonId={`${reasonId}-actions`} />}
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
      </header>
      {body}
    </section>
  );
}

function Detail({
  group,
  fixture,
  reason,
  marker,
  reasonId,
}: {
  group: WsGroup;
  fixture: boolean;
  reason: string;
  marker: ReactNode;
  reasonId: string;
}) {
  const decision = group.tasks.find((task) => task.pendingDecision);
  const objective = group.objective;
  const gates = objective.status === "ok" ? objective.value.gates : [];
  return (
    <section className="ws-detail" aria-label={`Workstream ${group.title}`} data-testid="ws-detail">
      <Panel title={group.title} headingLevel={3} actions={marker} className="ws-objective">
        {objective.status === "ok" ? (
          <>
            <p data-testid="ws-objective">{objective.value.text}</p>
            <p className="ws-subtle">
              Revision {objective.value.revision} ·{" "}
              {gates.length === 0
                ? "no acceptance gates"
                : `${gates.filter((gate) => gate.passed).length} of ${gates.length} gates passed`}
            </p>
            {gates.length > 0 && (
              <ul className="ws-gates" aria-label="Acceptance gates">
                {gates.map((gate) => (
                  <li key={gate.id}>
                    <Pill tone={gate.passed ? "ok" : "warn"}>{gate.passed ? "passed" : "open"}</Pill> {gate.label}
                  </li>
                ))}
              </ul>
            )}
          </>
        ) : (
          <p data-testid="ws-objective">
            Objective {objective.status}: {objective.reason}
          </p>
        )}
        <p className="ws-tags">
          <Badge>Project: {group.projectIds.map(basename).join(", ")}</Badge>{" "}
          <Badge>{group.tasks.length} tasks</Badge>{" "}
          {TASK_STATES.filter((state) => group.counts[state] > 0).map((state) => (
            <Pill key={state} tone={STATE_TONE[state]} data-testid={`count-${state}`}>
              {group.counts[state]} {state}
            </Pill>
          ))}
        </p>
        <div className="ws-row" role="group" aria-label="Workstream actions">
          <Button disabled aria-describedby={reasonId}>
            Revise objective
          </Button>
          <Button disabled aria-describedby={reasonId}>
            Add objective
          </Button>
          <Button disabled aria-describedby={reasonId}>
            Suspend new dispatch
          </Button>
          {decision ? (
            <NavLink
              className="ws-link-button"
              to={{ name: "review", projectId: decision.projectId, taskId: decision.id }}
            >
              Resolve contract decision
            </NavLink>
          ) : (
            <Button disabled aria-describedby={reasonId}>
              Resolve contract decision
            </Button>
          )}
        </div>
        <p className="ws-reason" id={reasonId}>
          {reason}
          {!decision && " Resolve contract decision becomes a link when a task has a pending approval."}
        </p>
      </Panel>

      <Panel title="Task dependencies" headingLevel={3} actions={marker}>
        <DependencyGraph tasks={group.tasks} />
      </Panel>

      <Panel title="Tasks" headingLevel={3} actions={marker} className="ws-tasks">
        <p className="ws-subtle">
          Other views of this workstream (Milestones, Context, Deliverables, Activity) are not available yet.
        </p>
        {group.tasks.length === 0 ? (
          <EmptyState title="No tasks" description="This workstream has no tasks yet." />
        ) : (
          <div className="ws-table-scroll" role="region" aria-label={`Tasks of ${group.title}`} tabIndex={0}>
            <table className="ws-table">
              <caption className="ws-sr">Tasks of {group.title}</caption>
              <thead>
                <tr>
                  <th scope="col">Task</th>
                  <th scope="col">Project</th>
                  <th scope="col">Task state</th>
                  <th scope="col">Assignment</th>
                  <th scope="col">Required next evidence</th>
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
                    <td>
                      <Pill tone={STATE_TONE[task.state]}>{task.state}</Pill>
                    </td>
                    <td>{assignment(task)}</td>
                    <td>{task.requiredEvidence}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        <div className="ws-row" role="group" aria-label="Plan editing">
          {["Add task", "Split task", "Remove task", "Reassign"].map((label) => (
            <Button key={label} disabled aria-describedby={`${reasonId}-plan`}>
              {label}
            </Button>
          ))}
        </div>
        <p className="ws-reason" id={`${reasonId}-plan`}>
          Disabled: {fixture ? "plan editing is not wired in this preview" : "the daemon has no plan-editing or bulk-action method"}
          .
        </p>
      </Panel>
    </section>
  );
}
