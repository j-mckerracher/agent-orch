/*
 * Plan editing (OX-W9): add, split, combine, remove, reassign and re-policy the tasks of one
 * live workstream. Each form calls one `workstreams.*` plan method through the D9 source. A
 * replace queues new tasks under the same objective; the daemon cancels a replaced task that
 * had not started and keeps it linked as history. The eligibility rules below mirror the
 * daemon's so a control is disabled with its reason instead of failing; the daemon decides.
 */
import { useId, useState, type FormEvent } from "react";
import { agentDisplayName } from "../../../daemon/agentDisplayNames";
import type { AutonomyPolicy, DeliveryPolicy, TaskKind, TaskState } from "../../../daemon/types";
import type { PermissionLevel, WorkstreamMutation, WorkstreamTaskPolicy } from "../../../sources/surfaces/types";
import { basename } from "../../format";
import { useServices, useStore } from "../../services";
import { ActionStatus, useAction } from "../../action";
import { useLastingAction } from "../../lastingAction";
import { Button } from "../../theme";
import { PERMISSION_LEVELS, permissionLabel } from "../workspace/permissionLevel";
import { useGroupRunner, type Controls, type Run } from "./Manage";
import { taskLabelOf, type WsGroup, type WsTask } from "./model";

type Form = "add" | "split" | "combine" | "remove" | "reassign" | "policy";

const FORMS: readonly Form[] = ["add", "split", "combine", "remove", "reassign", "policy"];
const LABELS: Record<Form, string> = {
  add: "Add task",
  split: "Split task",
  combine: "Combine tasks",
  remove: "Remove task",
  reassign: "Reassign",
  policy: "Change execution policy",
};

const AGENTS = ["codex", "claude-code", "github-copilot", "pi"] as const;
const KINDS: readonly TaskKind[] = ["Implement", "Investigate"];
const DELIVERIES: readonly DeliveryPolicy[] = ["ReportOnly", "LocalReview", "PullRequest"];
/** ReadOnly is refused for a new contract, so it is never offered. */
const AUTONOMIES = ["WorkspaceWrites", "ApprovalRequired"] as const;
/** Mirrors the daemon's split and combine bounds and its `MAX_RETRIES` / `MAX_REPAIRS`. */
const MIN_PARTS = 2;
const MAX_PARTS = 8;
const MAX_ATTEMPTS = 10;

const REPLACEABLE: ReadonlySet<TaskState> = new Set<TaskState>(["Created", "Queued", "Failed", "VerificationFailed"]);
const NOT_STARTED: ReadonlySet<TaskState> = new Set<TaskState>(["Created", "Queued"]);
const IN_FLIGHT: ReadonlySet<TaskState> = new Set<TaskState>(["Running", "Verifying"]);
/** The daemon's terminal states: a task in any other state still waits on its dependencies. */
const FINISHED: ReadonlySet<TaskState> = new Set<TaskState>(["Succeeded", "Cancelled"]);

const NO_REPLACEABLE =
  "no task here can be replaced. A task must be created, queued, failed or verification failed, not cited as gate evidence, and not needed by an unfinished task.";
const NO_COMBINABLE = "it needs two tasks that can be replaced, in one project and of one kind.";
const NO_REMOVABLE = "every task here is running, verifying, cited as gate evidence, or needed by an unfinished task.";
const NO_PROJECT = "no project is registered, so a new task has nowhere to run.";

interface Eligible {
  replaceable: WsTask[];
  combinable: WsTask[];
  removable: WsTask[];
}

const citedIds = (group: WsGroup): Set<string> =>
  new Set(
    group.objectives.status === "ok"
      ? group.objectives.value.flatMap((objective) => objective.gates.flatMap((gate) => (gate.evidenceTaskId ? [gate.evidenceTaskId] : [])))
      : [],
  );

/** True when an unfinished task of this workstream, outside `ids`, depends on one of `ids`. */
function needed(group: WsGroup, ids: readonly string[]): boolean {
  return group.tasks.some(
    (task) => !FINISHED.has(task.state) && !ids.includes(task.id) && task.dependencies.some((id) => ids.includes(id)),
  );
}

/** Tasks of one project and kind can be combined; a task whose contract is not loaded groups by project alone. */
const combineKey = (task: WsTask) => `${task.projectId}\n${task.contract?.kind ?? ""}`;

function eligibility(group: WsGroup): Eligible {
  const cited = citedIds(group);
  const combinable = group.tasks.filter((task) => REPLACEABLE.has(task.state) && !cited.has(task.id));
  return {
    replaceable: combinable.filter((task) => !needed(group, [task.id])),
    combinable,
    removable: group.tasks.filter(
      (task) => !IN_FLIGHT.has(task.state) && !cited.has(task.id) && !(NOT_STARTED.has(task.state) && needed(group, [task.id])),
    ),
  };
}

function hasPair(tasks: readonly WsTask[]): boolean {
  const seen = new Set<string>();
  for (const task of tasks) {
    const key = combineKey(task);
    if (seen.has(key)) return true;
    seen.add(key);
  }
  return false;
}

const optionText = (task: WsTask) => `${taskLabelOf(task)} (${task.state}, ${agentDisplayName(task.agent)})`;

interface FormProps {
  group: WsGroup;
  controls: Controls;
  /** Runs one plan edit; on success the form closes and every snapshot is read again. */
  submit: (action: string, send: () => Promise<WorkstreamMutation>) => Promise<void>;
}

/** The six plan-editing entry points; each opens one inline form. */
export function PlanEditing({ group, locked }: { group: WsGroup; locked: string | null }) {
  const { data } = useServices();
  const { projects } = useStore(data);
  const baseId = useId();
  const [open, setOpen] = useState<Form | null>(null);
  // Adding a task is minor and clears itself; the other forms cancel or replace tasks, so their result stays.
  const add = useAction();
  const plan = useLastingAction();
  const runAdd = useGroupRunner(add);
  const runPlan = useGroupRunner(plan);
  const controls: Controls = { run: runPlan, busy: add.pending || plan.pending, locked };
  const eligible = eligibility(group);
  const projectIds = [
    ...new Set([...group.projectIds, ...(projects.data ?? []).filter((project) => !project.archived).map((project) => project.id)]),
  ];
  const why: Record<Form, string | null> = {
    add: projectIds.length === 0 ? NO_PROJECT : null,
    split: eligible.replaceable.length === 0 ? NO_REPLACEABLE : null,
    combine: hasPair(eligible.combinable) ? null : NO_COMBINABLE,
    remove: eligible.removable.length === 0 ? NO_REMOVABLE : null,
    reassign: eligible.replaceable.length === 0 ? NO_REPLACEABLE : null,
    policy: eligible.replaceable.length === 0 ? NO_REPLACEABLE : null,
  };
  // One note per distinct reason, naming the buttons it covers; a lock covers all six.
  const notes = new Map<string, Form[]>();
  for (const form of FORMS) {
    const text = controls.locked ?? why[form];
    if (text !== null) notes.set(text, [...(notes.get(text) ?? []), form]);
  }
  const noteIds = new Map([...notes.keys()].map((text, index) => [text, `${baseId}-note-${index}`]));
  const reasonOf = (form: Form) => controls.locked ?? why[form];
  const shown = open !== null && reasonOf(open) === null ? open : null;
  const panelId = `${baseId}-panel`;

  const submitWith =
    (run: Run): FormProps["submit"] =>
    async (action, send) => {
      const done = await run(action, send);
      if (done) {
        setOpen(null);
        // A plan edit queues and cancels tasks, so the task snapshot is read again as after a submit or cancel.
        void data.refreshAll();
      }
    };
  const props = { group, controls, submit: submitWith(runPlan) };

  return (
    <div className="ws-plan" data-testid="ws-plan">
      <div className="ws-row" role="group" aria-label="Plan editing">
        {FORMS.map((form) => {
          const reason = reasonOf(form);
          return (
            <Button
              key={form}
              aria-expanded={shown === form}
              aria-controls={shown === form ? panelId : undefined}
              aria-describedby={reason !== null ? noteIds.get(reason) : undefined}
              disabled={controls.busy || reason !== null}
              onClick={() => setOpen(shown === form ? null : form)}
            >
              {LABELS[form]}
            </Button>
          );
        })}
      </div>
      <ActionStatus action={add} />
      <ActionStatus action={plan} />
      {[...notes].map(([text, forms]) => (
        <p key={text} className="ws-reason" id={noteIds.get(text)}>
          {controls.locked !== null ? text : `Disabled (${forms.map((form) => LABELS[form]).join(", ")}): ${text}`}
        </p>
      ))}
      {shown !== null && (
        <div id={panelId} className="ws-plan-panel">
          {shown === "add" && <AddTaskForm {...props} submit={submitWith(runAdd)} projectIds={projectIds} />}
          {shown === "split" && <SplitTaskForm {...props} tasks={eligible.replaceable} />}
          {shown === "combine" && <CombineTasksForm {...props} tasks={eligible.combinable} />}
          {shown === "remove" && <RemoveTaskForm {...props} tasks={eligible.removable} />}
          {shown === "reassign" && <ReassignForm {...props} tasks={eligible.replaceable} />}
          {shown === "policy" && <PolicyForm {...props} tasks={eligible.replaceable} />}
        </div>
      )}
    </div>
  );
}

function TaskSelect({
  label,
  tasks,
  value,
  disabled,
  onChange,
}: {
  label: string;
  tasks: readonly WsTask[];
  value: string;
  disabled: boolean;
  onChange: (id: string) => void;
}) {
  return (
    <label>
      {label}
      <select value={tasks.some((task) => task.id === value) ? value : ""} disabled={disabled} onChange={(e) => onChange(e.target.value)}>
        <option value="">Choose a task</option>
        {tasks.map((task) => (
          <option key={task.id} value={task.id}>
            {optionText(task)}
          </option>
        ))}
      </select>
    </label>
  );
}

function AddTaskForm({ group, controls, submit, projectIds }: FormProps & { projectIds: string[] }) {
  const { surfaces } = useServices();
  const [projectId, setProjectId] = useState(projectIds[0] ?? "");
  const [objective, setObjective] = useState("");
  const [kind, setKind] = useState<TaskKind>("Implement");
  const [agent, setAgent] = useState("");
  const [objectiveId, setObjectiveId] = useState("");
  const objectives = group.objectives.status === "ok" ? group.objectives.value : [];
  const off = controls.busy;
  const ready = projectIds.includes(projectId) && objective.trim() !== "";
  const send = (event: FormEvent) => {
    event.preventDefault();
    if (!ready) return;
    void submit(`Add task "${objective.trim()}"`, () =>
      surfaces.workstreams.addTask(group.id, {
        projectId,
        objective: objective.trim(),
        kind,
        agent,
        ...(objectiveId !== "" ? { objectiveId } : {}),
      }),
    );
  };
  return (
    <form className="ws-form" onSubmit={send} aria-label="Add task">
      <label>
        Project
        <select value={projectId} disabled={off} onChange={(e) => setProjectId(e.target.value)}>
          {projectIds.map((id) => (
            <option key={id} value={id} title={id}>
              {basename(id)}
            </option>
          ))}
        </select>
      </label>
      <label>
        Task objective
        <textarea value={objective} rows={2} disabled={off} onChange={(e) => setObjective(e.target.value)} />
      </label>
      <label>
        Kind
        <select value={kind} disabled={off} onChange={(e) => setKind(e.target.value as TaskKind)}>
          {KINDS.map((value) => (
            <option key={value}>{value}</option>
          ))}
        </select>
      </label>
      <AgentSelect value={agent} disabled={off} onChange={setAgent} allowDefault />
      <label>
        Under objective
        <select value={objectiveId} disabled={off} onChange={(e) => setObjectiveId(e.target.value)}>
          <option value="">Workstream only</option>
          {objectives.map((item) => (
            <option key={item.id} value={item.id}>
              {item.title}
            </option>
          ))}
        </select>
      </label>
      <Button type="submit" variant="primary" pending={off} disabled={!ready}>
        Queue task
      </Button>
      <p className="ws-reason">The task is queued with local review, approval required, and one retry and repair.</p>
    </form>
  );
}

function AgentSelect({
  value,
  current,
  disabled,
  onChange,
  allowDefault = false,
}: {
  value: string;
  current?: string;
  disabled: boolean;
  onChange: (agent: string) => void;
  allowDefault?: boolean;
}) {
  return (
    <label>
      Agent
      <select value={value} disabled={disabled} onChange={(e) => onChange(e.target.value)}>
        <option value="">{allowDefault ? "Use Settings default provider" : "Choose an agent"}</option>
        {AGENTS.map((id) => (
          <option key={id} value={id} disabled={id === current}>
            {agentDisplayName(id)}
            {id === current ? " (current)" : ""}
          </option>
        ))}
      </select>
    </label>
  );
}

const lines = (text: string) =>
  text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "");

function SplitTaskForm({ group, controls, submit, tasks }: FormProps & { tasks: WsTask[] }) {
  const { surfaces } = useServices();
  const [taskId, setTaskId] = useState("");
  const [text, setText] = useState("");
  const parts = lines(text);
  const chosen = tasks.find((task) => task.id === taskId);
  const ready = chosen !== undefined && parts.length >= MIN_PARTS && parts.length <= MAX_PARTS;
  const send = (event: FormEvent) => {
    event.preventDefault();
    if (!ready) return;
    void submit(`Split task ${chosen.id} into ${parts.length} tasks`, () => surfaces.workstreams.splitTask(group.id, chosen.id, parts));
  };
  return (
    <form className="ws-form" onSubmit={send} aria-label="Split task">
      <TaskSelect label="Task to split" tasks={tasks} value={taskId} disabled={controls.busy} onChange={setTaskId} />
      <label>
        New task objectives, one per line
        <textarea value={text} rows={3} disabled={controls.busy} onChange={(e) => setText(e.target.value)} />
      </label>
      <Button type="submit" variant="primary" pending={controls.busy} disabled={!ready}>
        Split into {parts.length} tasks
      </Button>
      <p className="ws-reason">
        {MIN_PARTS} to {MAX_PARTS} objectives. Each new task copies the original's contract; the original is cancelled if it has not
        started.
      </p>
    </form>
  );
}

function CombineTasksForm({ group, controls, submit, tasks }: FormProps & { tasks: WsTask[] }) {
  const { surfaces } = useServices();
  const [ids, setIds] = useState<string[]>([]);
  const [draft, setDraft] = useState<string | null>(null);
  const chosen = tasks.filter((task) => ids.includes(task.id));
  const objective = draft ?? chosen.map((task) => task.objective.trim()).filter((text) => text !== "").join("\n");
  const problem =
    chosen.length < MIN_PARTS || chosen.length > MAX_PARTS
      ? `Choose ${MIN_PARTS} to ${MAX_PARTS} tasks.`
      : new Set(chosen.map(combineKey)).size > 1
        ? "The chosen tasks must share one project and one kind."
        : needed(group, chosen.map((task) => task.id))
          ? "An unfinished task outside this choice depends on one of them."
          : objective.trim() === ""
            ? "Write the combined objective."
            : null;
  const toggle = (id: string, on: boolean) => setIds((current) => (on ? [...current, id] : current.filter((item) => item !== id)));
  const send = (event: FormEvent) => {
    event.preventDefault();
    if (problem !== null) return;
    void submit(`Combine ${chosen.length} tasks`, () =>
      surfaces.workstreams.combineTasks(
        group.id,
        chosen.map((task) => task.id),
        objective.trim(),
      ),
    );
  };
  return (
    <form className="ws-form" onSubmit={send} aria-label="Combine tasks">
      <fieldset className="ws-checks" disabled={controls.busy}>
        <legend>Tasks to combine</legend>
        {tasks.map((task) => (
          <label key={task.id}>
            <input type="checkbox" checked={ids.includes(task.id)} onChange={(e) => toggle(task.id, e.target.checked)} />{" "}
            {optionText(task)} · {basename(task.projectId)}
          </label>
        ))}
      </fieldset>
      <label>
        Combined objective
        <textarea value={objective} rows={3} disabled={controls.busy} onChange={(e) => setDraft(e.target.value)} />
      </label>
      <Button type="submit" variant="primary" pending={controls.busy} disabled={problem !== null}>
        Combine into one task
      </Button>
      <p className="ws-reason">{problem ?? "One new task replaces the chosen ones; those that have not started are cancelled."}</p>
    </form>
  );
}

function RemoveTaskForm({ group, controls, submit, tasks }: FormProps & { tasks: WsTask[] }) {
  const { surfaces } = useServices();
  const [taskId, setTaskId] = useState("");
  const chosen = tasks.find((task) => task.id === taskId);
  const send = (event: FormEvent) => {
    event.preventDefault();
    if (!chosen) return;
    void submit(`Remove task ${chosen.id}`, () => surfaces.workstreams.removeTask(group.id, chosen.id));
  };
  return (
    <form className="ws-form" onSubmit={send} aria-label="Remove task">
      <TaskSelect label="Task to remove" tasks={tasks} value={taskId} disabled={controls.busy} onChange={setTaskId} />
      <Button type="submit" variant="danger" pending={controls.busy} disabled={!chosen}>
        Confirm remove
      </Button>
      <p className="ws-reason">
        {chosen === undefined
          ? "Choose the task to take out of this workstream."
          : NOT_STARTED.has(chosen.state)
            ? "It has not started, so the daemon cancels it and then unlinks it."
            : "It has finished, so the daemon only unlinks it. Its record and results are kept."}
      </p>
    </form>
  );
}

function ReassignForm({ group, controls, submit, tasks }: FormProps & { tasks: WsTask[] }) {
  const { surfaces } = useServices();
  const [taskId, setTaskId] = useState("");
  const [agent, setAgent] = useState("");
  const chosen = tasks.find((task) => task.id === taskId);
  const ready = chosen !== undefined && agent !== "" && agent !== chosen.agent;
  const send = (event: FormEvent) => {
    event.preventDefault();
    if (!ready) return;
    void submit(`Reassign task ${chosen.id} to ${agentDisplayName(agent)}`, () =>
      surfaces.workstreams.reassignTask(group.id, chosen.id, agent),
    );
  };
  return (
    <form className="ws-form" onSubmit={send} aria-label="Reassign task">
      <TaskSelect label="Task to reassign" tasks={tasks} value={taskId} disabled={controls.busy} onChange={setTaskId} />
      <AgentSelect value={agent} current={chosen?.agent} disabled={controls.busy} onChange={setAgent} />
      <Button type="submit" variant="primary" pending={controls.busy} disabled={!ready}>
        Reassign task
      </Button>
      <p className="ws-reason">A copy of the task runs on the new agent with that agent's default model.</p>
    </form>
  );
}

function PolicyForm({ group, controls, submit, tasks }: FormProps & { tasks: WsTask[] }) {
  const [taskId, setTaskId] = useState("");
  const chosen = tasks.find((task) => task.id === taskId);
  return (
    <div className="ws-form" role="group" aria-label="Change execution policy">
      <TaskSelect label="Task to change" tasks={tasks} value={taskId} disabled={controls.busy} onChange={setTaskId} />
      {/* Remounts per task so the fields always start from that task's contract. */}
      {chosen && <PolicyFields key={chosen.id} group={group} controls={controls} submit={submit} task={chosen} />}
    </div>
  );
}

const KEEP = "";

function PolicyFields({ group, controls, submit, task }: FormProps & { task: WsTask }) {
  const { surfaces } = useServices();
  const contract = task.contract;
  // Unknown fields (no loaded contract) start at "keep current" and are sent only when chosen.
  const initial = {
    permission: task.permission ?? KEEP,
    autonomy: contract?.autonomy ?? KEEP,
    delivery: contract?.delivery ?? KEEP,
    maxRetries: contract ? String(contract.max_retries) : KEEP,
    maxRepairs: contract ? String(contract.max_repairs) : KEEP,
  };
  const [fields, setFields] = useState(initial);
  const set = (change: Partial<typeof fields>) => setFields((current) => ({ ...current, ...change }));
  const count = (text: string) => {
    const value = Number(text);
    return text.trim() !== "" && Number.isInteger(value) && value >= 0 && value <= MAX_ATTEMPTS ? value : null;
  };
  const changed = (key: keyof typeof fields) => fields[key] !== initial[key] && fields[key] !== KEEP;
  const retries = count(fields.maxRetries);
  const repairs = count(fields.maxRepairs);
  const invalid = (changed("maxRetries") && retries === null) || (changed("maxRepairs") && repairs === null);
  const policy: WorkstreamTaskPolicy = {
    ...(changed("permission") ? { permission: fields.permission as PermissionLevel } : {}),
    ...(changed("autonomy") ? { autonomy: fields.autonomy as (typeof AUTONOMIES)[number] } : {}),
    ...(changed("delivery") ? { delivery: fields.delivery as DeliveryPolicy } : {}),
    ...(changed("maxRetries") && retries !== null ? { maxRetries: retries } : {}),
    ...(changed("maxRepairs") && repairs !== null ? { maxRepairs: repairs } : {}),
  };
  const ready = !invalid && Object.keys(policy).length > 0;
  const send = (event: FormEvent) => {
    event.preventDefault();
    if (!ready) return;
    void submit(`Change execution policy of task ${task.id}`, () => surfaces.workstreams.setTaskPolicy(group.id, task.id, policy));
  };
  const off = controls.busy;
  const keep = <option value={KEEP}>Keep current</option>;
  return (
    <form className="ws-form" onSubmit={send} aria-label={`Execution policy of ${taskLabelOf(task)}`}>
      <label>
        Permission
        <select value={fields.permission} disabled={off} onChange={(e) => set({ permission: e.target.value })}>
          {contract === null && keep}
          {PERMISSION_LEVELS.map((level) => (
            <option key={level} value={level}>
              {permissionLabel(level)}
            </option>
          ))}
        </select>
      </label>
      <label>
        Autonomy
        <select value={fields.autonomy} disabled={off} onChange={(e) => set({ autonomy: e.target.value })}>
          {contract === null && keep}
          {/* A ReadOnly contract shows its value; it cannot be chosen again. */}
          {contract?.autonomy === "ReadOnly" && <option value={"ReadOnly" satisfies AutonomyPolicy}>ReadOnly (current)</option>}
          {AUTONOMIES.map((value) => (
            <option key={value}>{value}</option>
          ))}
        </select>
      </label>
      <label>
        Delivery
        <select value={fields.delivery} disabled={off} onChange={(e) => set({ delivery: e.target.value })}>
          {contract === null && keep}
          {DELIVERIES.map((value) => (
            <option key={value}>{value}</option>
          ))}
        </select>
      </label>
      <label>
        Max retries
        <input
          type="number"
          min={0}
          max={MAX_ATTEMPTS}
          placeholder={contract === null ? "Keep current" : undefined}
          value={fields.maxRetries}
          disabled={off}
          onChange={(e) => set({ maxRetries: e.target.value })}
        />
      </label>
      <label>
        Max repairs
        <input
          type="number"
          min={0}
          max={MAX_ATTEMPTS}
          placeholder={contract === null ? "Keep current" : undefined}
          value={fields.maxRepairs}
          disabled={off}
          onChange={(e) => set({ maxRepairs: e.target.value })}
        />
      </label>
      <Button type="submit" variant="primary" pending={off} disabled={!ready}>
        Apply policy
      </Button>
      <p className="ws-reason">
        {invalid
          ? `Max retries and max repairs must be whole numbers from 0 to ${MAX_ATTEMPTS}.`
          : "A copy of the task with the new policy replaces it. Only the fields you change are sent."}
      </p>
    </form>
  );
}
