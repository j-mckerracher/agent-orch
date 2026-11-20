/*
 * Live workstream controls (M7-23): every button here calls one real `workstreams.*` method
 * through the D9 source. Each write quotes the revision the operator last read, so a stale
 * edit is a daemon conflict that the runner answers by reloading, never by overwriting.
 */
import { useEffect, useId, useRef, useState, type FormEvent, type ReactNode } from "react";
import type { AppData } from "../../../daemon/appData";
import { asDaemonError, presentError, type DaemonError } from "../../../daemon/errors";
import type { GateState, WorkstreamGate, WorkstreamObjective } from "../../../sources/surfaces/types";
import { canArchive, stateLabel } from "../../format";
import { ActionStatus, useAction, type Action, type Outcome } from "../../action";
import { useLastingAction } from "../../lastingAction";
import { useGate, useServices, useStore } from "../../services";
import { Button, Pill } from "../../theme";
import { Dialog } from "../../Dialog";
import { ActionMenu, type MenuItem } from "./ActionMenu";
import { reloadWorkstreams, taskLabelOf, type WsGroup, type WsTask } from "./model";

export type Run = <T>(action: string, send: () => Promise<T>) => Promise<T | null>;

/** A failed daemon write in the person's words: what happened, what to do, then the daemon's own message. */
export function failureText(action: string, failure: DaemonError, reloaded = false): string {
  const { reason, nextStep } = presentError(failure);
  const detail = failure.message ? ` (${failure.message})` : "";
  const reread = reloaded ? " The workstream data was reloaded from the daemon." : "";
  return `${action} failed. ${reason} ${nextStep}${detail}${reread}`;
}

/**
 * One control group's daemon writes, shown by the group's own Action. A success re-reads the workstream data; a stale
 * or vanished record is re-read too, and the failure says so. Resolves to the write's value, or null when it failed.
 */
export function useGroupRunner(action: Action): Run {
  const { connected } = useGate();
  const wasConnected = useRef(connected);
  // A failure from before a lost connection is no longer current once the connection is back.
  useEffect(() => {
    const failed = action.state.phase === "done" && action.state.outcome.kind === "failed";
    if (connected && !wasConnected.current && failed) action.clear();
    wasConnected.current = connected;
  }, [connected]);
  return async <T,>(text: string, send: () => Promise<T>): Promise<T | null> => {
    let value: T | null = null;
    await action.run(`${text}…`, async (): Promise<Outcome> => {
      try {
        value = await send();
      } catch (error) {
        const failure = asDaemonError(error);
        // A stale or vanished record is re-read so the operator sees the daemon's current state.
        const reloaded = failure.kind === "conflict" || failure.kind === "not-found";
        if (reloaded) reloadWorkstreams();
        throw new Error(failureText(text, failure, reloaded));
      }
      reloadWorkstreams();
      return { kind: "saved", text: `${text}: recorded by the daemon.` };
    });
    return value;
  };
}

const GATE_TARGETS: Record<GateState, GateState[]> = {
  pending: ["verified", "failed"],
  failed: ["pending", "verified"],
  verified: ["pending"],
};
const GATE_TONE = { pending: "warn", verified: "ok", failed: "bad" } as const;
const GATE_LABEL = { pending: "pending", verified: "verified", failed: "failed" } as const;

export const ARCHIVED_REASON = "Disabled: an archived workstream is read-only.";

interface Controls {
  run: Run;
  busy: boolean;
  /** Disabled with this reason when set (archived workstream). */
  locked: string | null;
}

function GateRow({
  objective,
  gate,
  tasks,
  locked,
}: {
  objective: WorkstreamObjective;
  gate: WorkstreamGate;
  tasks: WsTask[];
  locked: string | null;
}) {
  const { surfaces } = useServices();
  // Its own action: the row is keyed by gate id, so the result survives the gate's state changing under it.
  const action = useAction();
  const run = useGroupRunner(action);
  const options = GATE_TARGETS[gate.state];
  const [picked, setPicked] = useState<GateState | null>(null);
  const target = picked !== null && options.includes(picked) ? picked : options[0]!;
  const [evidence, setEvidence] = useState("");
  const evidenceAllowed = target !== "pending";
  const off = action.pending || locked !== null;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const evidenceTaskId = evidenceAllowed && evidence !== "" ? evidence : undefined;
    void run(`Set gate "${gate.label}" to ${target}`, () =>
      surfaces.workstreams.setGate({ id: objective.id, revision: objective.revision }, gate.id, target, evidenceTaskId),
    );
  };
  return (
    <li data-testid="ws-gate">
      <Pill tone={GATE_TONE[gate.state]}>{GATE_LABEL[gate.state]}</Pill> {gate.label}
      {gate.evidenceTaskId !== null && <small> · evidence: {gate.evidenceTaskId}</small>}
      <form className="ws-inline-form" onSubmit={submit} aria-label={`Change gate ${gate.label}`}>
        <label>
          <span className="ws-sr">New state of gate {gate.label}</span>
          <select value={target} disabled={off} onChange={(event) => setPicked(event.target.value as GateState)}>
            {options.map((state) => (
              <option key={state} value={state}>
                Mark {state}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span className="ws-sr">Evidence task for gate {gate.label}</span>
          <select
            value={evidenceAllowed ? evidence : ""}
            disabled={off || !evidenceAllowed}
            onChange={(event) => setEvidence(event.target.value)}
          >
            <option value="">No evidence task</option>
            {tasks.map((task) => (
              <option key={task.id} value={task.id}>
                {taskLabelOf(task)}
              </option>
            ))}
          </select>
        </label>
        <Button type="submit" pending={action.pending} disabled={locked !== null}>
          Apply to {gate.label}
        </Button>
      </form>
      <ActionStatus action={action} />
    </li>
  );
}

function ReviseForm({ objective, controls }: { objective: WorkstreamObjective; controls: Controls }) {
  const { surfaces } = useServices();
  const [title, setTitle] = useState(objective.title);
  const [description, setDescription] = useState(objective.description);
  const [gate, setGate] = useState("");
  const change = {
    ...(title.trim() !== objective.title ? { title: title.trim() } : {}),
    ...(description !== objective.description ? { description } : {}),
    ...(gate.trim() !== "" ? { addGates: [gate.trim()] } : {}),
  };
  const changed = Object.keys(change).length > 0 && title.trim() !== "";
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!changed) return;
    const done = await controls.run(`Revise objective "${objective.title}"`, () =>
      surfaces.workstreams.reviseObjective({ id: objective.id, revision: objective.revision }, change),
    );
    if (done) setGate("");
  };
  return (
    <form className="ws-form" onSubmit={submit} aria-label={`Revise objective ${objective.title}`}>
      <label>
        Objective title
        <input type="text" value={title} disabled={controls.busy || controls.locked !== null} onChange={(e) => setTitle(e.target.value)} />
      </label>
      <label>
        Objective description
        <textarea value={description} rows={2} disabled={controls.busy || controls.locked !== null} onChange={(e) => setDescription(e.target.value)} />
      </label>
      <label>
        Add acceptance gate
        <input type="text" value={gate} disabled={controls.busy || controls.locked !== null} onChange={(e) => setGate(e.target.value)} />
      </label>
      <Button type="submit" pending={controls.busy} disabled={!changed || controls.locked !== null}>
        Revise objective
      </Button>
    </form>
  );
}

export function ObjectiveCard({
  objective,
  tasks,
  locked,
}: {
  objective: WorkstreamObjective;
  tasks: WsTask[];
  locked: string | null;
}) {
  // The card's revise form remounts on every revision, so its result is held here, above it.
  const revise = useAction();
  const run = useGroupRunner(revise);
  const controls: Controls = { run, busy: revise.pending, locked };
  const verified = objective.gates.filter((gate) => gate.state === "verified").length;
  return (
    <section className="ws-objective-card" aria-label={`Objective ${objective.title}`} data-testid="ws-objective">
      <h3>{objective.title}</h3>
      {objective.description !== "" && <p>{objective.description}</p>}
      <p className="ws-subtle">
        Revision {objective.revision} ·{" "}
        {objective.gates.length === 0
          ? "no acceptance gates"
          : `${verified} of ${objective.gates.length} gates verified`}
      </p>
      {objective.gates.length > 0 && (
        <ul className="ws-gates" aria-label={`Acceptance gates of ${objective.title}`}>
          {objective.gates.map((gate) => (
            <GateRow key={gate.id} objective={objective} gate={gate} tasks={tasks} locked={locked} />
          ))}
        </ul>
      )}
      <details>
        <summary>Revise this objective</summary>
        {/* Remounts on every new revision so the fields always start from the daemon's values. */}
        <ReviseForm key={`${objective.id}@${objective.revision}`} objective={objective} controls={controls} />
      </details>
      <ActionStatus action={revise} />
    </section>
  );
}

export function AddObjectiveForm({ group, locked }: { group: WsGroup; locked: string | null }) {
  const { surfaces } = useServices();
  const action = useAction();
  const run = useGroupRunner(action);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [gates, setGates] = useState("");
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (title.trim() === "") return;
    const list = gates.split("\n").map((line) => line.trim()).filter((line) => line !== "");
    const done = await run(`Add objective "${title.trim()}"`, () =>
      surfaces.workstreams.addObjective(group.id, {
        title: title.trim(),
        ...(description.trim() !== "" ? { description: description.trim() } : {}),
        ...(list.length > 0 ? { gates: list } : {}),
      }),
    );
    if (done) {
      setTitle("");
      setDescription("");
      setGates("");
    }
  };
  const off = action.pending || locked !== null;
  return (
    <form className="ws-form" onSubmit={submit} aria-label="Add objective">
      <label>
        New objective title
        <input type="text" value={title} disabled={off} onChange={(e) => setTitle(e.target.value)} />
      </label>
      <label>
        New objective description
        <textarea value={description} rows={2} disabled={off} onChange={(e) => setDescription(e.target.value)} />
      </label>
      <label>
        Acceptance gates, one per line
        <textarea value={gates} rows={2} disabled={off} onChange={(e) => setGates(e.target.value)} />
      </label>
      <Button type="submit" pending={action.pending} disabled={locked !== null || title.trim() === ""}>
        Add objective
      </Button>
      <ActionStatus action={action} />
    </form>
  );
}

/**
 * The detail header's Actions menu. Suspend and the list order are advisory (the scheduler ignores
 * both); archive is terminal and confirmed in a dialog. `fixtureReason` disables every item on placeholder data.
 * Suspend and Prioritize are minor and clear themselves; Archive stays until the next change.
 */
export function WorkstreamActions({
  group,
  first,
  locked,
  fixtureReason,
}: {
  group: WsGroup;
  first: boolean;
  locked: string | null;
  fixtureReason: string | null;
}) {
  const { surfaces } = useServices();
  const [confirming, setConfirming] = useState(false);
  const minor = useAction();
  const archive = useLastingAction();
  const runMinor = useGroupRunner(minor);
  const runArchive = useGroupRunner(archive);
  const busy = minor.pending || archive.pending;
  const reason = fixtureReason ?? locked ?? (busy ? "Another change is still being recorded." : null);
  const target = { id: group.id, revision: group.revision };
  const items: MenuItem[] = [
    group.state === "suspended"
      ? {
          label: "Resume workstream",
          onSelect: () => void runMinor("Resume workstream", () => surfaces.workstreams.update(target, { state: "active" })),
          reason,
        }
      : {
          label: "Suspend workstream",
          onSelect: () => void runMinor("Suspend workstream", () => surfaces.workstreams.update(target, { state: "suspended" })),
          reason,
        },
    {
      label: "Prioritize",
      onSelect: () => void runMinor(`Prioritize "${group.title}"`, () => surfaces.workstreams.prioritize(group.id)),
      reason: reason ?? (first ? `"${group.title}" is already first.` : null),
    },
    { label: "Archive workstream", onSelect: () => setConfirming(true), reason },
  ];
  return (
    <>
      <ActionMenu label="Workstream actions" trigger={<>Actions <span aria-hidden="true">⌄</span></>} items={items} />
      {/* Beside the trigger, outside the menu: the menu closes when an item is chosen. */}
      <ActionStatus action={minor} />
      <ActionStatus action={archive} />
      {confirming && (
        <Dialog
          title={`Archive "${group.title}"?`}
          onClose={() => setConfirming(false)}
          footer={
            <>
              <Button onClick={() => setConfirming(false)}>Keep workstream</Button>
              <Button
                variant="danger"
                onClick={() => {
                  setConfirming(false);
                  void runArchive("Archive workstream", () => surfaces.workstreams.update(target, { state: "archived" }));
                }}
              >
                Confirm archive (cannot be undone)
              </Button>
            </>
          }
        >
          <p>It becomes read-only, and this cannot be undone.</p>
        </Dialog>
      )}
    </>
  );
}

export function LinkTaskForm({
  group,
  unlinked,
  unlinkedKnown,
  locked,
}: {
  group: WsGroup;
  unlinked: WsTask[];
  unlinkedKnown: boolean;
  locked: string | null;
}) {
  const { surfaces } = useServices();
  const action = useAction();
  const run = useGroupRunner(action);
  const [taskId, setTaskId] = useState("");
  const [objectiveId, setObjectiveId] = useState("");
  const objectives = group.objectives.status === "ok" ? group.objectives.value : [];
  const off = action.pending || locked !== null;
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (taskId === "") return;
    const done = await run(`Link task ${taskId}`, () =>
      surfaces.workstreams.linkTask(group.id, taskId, objectiveId === "" ? undefined : objectiveId),
    );
    if (done) setTaskId("");
  };
  // The result sits outside these branches: linking the last unlinked task swaps the form for a note.
  let content: ReactNode;
  if (!unlinkedKnown) {
    content = (
      <p className="ws-subtle" data-testid="ws-unlinked-unknown">
        Tasks outside every workstream are unknown: a workstream could not be read, and its tasks would look unlinked.
      </p>
    );
  } else if (unlinked.length === 0) {
    content = <p className="ws-subtle">Every daemon task is already in a workstream, so there is nothing to link.</p>;
  } else {
    content = (
      <form className="ws-inline-form" onSubmit={submit} aria-label="Link task">
        <label>
          Task to link
          <select value={taskId} disabled={off} onChange={(e) => setTaskId(e.target.value)}>
            <option value="">Choose a task</option>
            {unlinked.map((task) => (
              <option key={task.id} value={task.id}>
                {taskLabelOf(task)} ({task.projectId.split(/[\\/]/).pop()})
              </option>
            ))}
          </select>
        </label>
        <label>
          Under objective
          <select value={objectiveId} disabled={off} onChange={(e) => setObjectiveId(e.target.value)}>
            <option value="">Workstream only</option>
            {objectives.map((objective) => (
              <option key={objective.id} value={objective.id}>
                {objective.title}
              </option>
            ))}
          </select>
        </label>
        <Button type="submit" pending={action.pending} disabled={locked !== null || taskId === ""}>
          Link task
        </Button>
      </form>
    );
  }
  return (
    <>
      {content}
      <ActionStatus action={action} />
    </>
  );
}

export function UnlinkButton({ group, task, controls }: { group: WsGroup; task: WsTask; controls: Controls }) {
  const { surfaces } = useServices();
  const reasonId = useId();
  // The daemon refuses to unlink a task that a gate cites as evidence, so say so up front.
  const cited =
    group.objectives.status === "ok" &&
    group.objectives.value.some((objective) => objective.gates.some((gate) => gate.evidenceTaskId === task.id));
  return (
    <>
      <Button
        disabled={controls.busy || controls.locked !== null || cited}
        aria-label={`Unlink ${taskLabelOf(task)}`}
        aria-describedby={cited ? reasonId : undefined}
        onClick={() => void controls.run(`Unlink task ${task.id}`, () => surfaces.workstreams.unlinkTask(group.id, task.id))}
      >
        Unlink
      </Button>
      {cited && (
        <span className="ws-reason" id={reasonId}>
          Disabled: a gate cites this task as evidence. Change that gate first.
        </span>
      )}
    </>
  );
}

/** `AppData.archiveTasks` as a runner send: a refused call throws its daemon error, so the runner shows it. */
export async function archiveSend(data: AppData, ids: string[]): Promise<string[]> {
  const result = await data.archiveTasks(ids);
  if (!result.ok) throw result.error;
  return result.value;
}

/** Hidden once the daemon answers method-unavailable (DPES-1106); otherwise disabled with a reason when not finished. */
export function ArchiveTaskButton({ task, controls }: { task: WsTask; controls: Controls }) {
  const { data } = useServices();
  const { archiveUnsupported } = useStore(data);
  const reasonId = useId();
  const finished = canArchive(task.state);
  if (archiveUnsupported) return null;
  return (
    <>
      <Button
        disabled={controls.busy || controls.locked !== null || !finished}
        aria-label={`Archive ${taskLabelOf(task)}`}
        aria-describedby={finished ? undefined : reasonId}
        onClick={() => void controls.run(`Archive task ${task.id}`, () => archiveSend(data, [task.id]))}
      >
        Archive
      </Button>
      {!finished && (
        <span className="ws-reason" id={reasonId}>
          Disabled: the task is {stateLabel(task.state)}. Only finished tasks can be archived.
        </span>
      )}
    </>
  );
}

/**
 * The list header's "⋯" menu. Archive finished archives every finished task, linked or not, after a
 * confirmation. The workstream-lifecycle actions live on the detail screen, not here. The caller owns the
 * actions, so their results stay in the header when this menu remounts.
 */
export function ListMenu({ controls, live, onRefresh }: { controls: Controls; live: boolean; onRefresh: () => void }) {
  const { data, router } = useServices();
  const gate = useGate();
  const { tasks, archiveUnsupported } = useStore(data);
  const [confirming, setConfirming] = useState(false);
  const { run, busy } = controls;
  const finished = (tasks.data ?? []).filter((task) => canArchive(task.state)).map((task) => task.contract.id);
  const busyReason = busy ? "Another change is still being recorded." : null;
  const items: MenuItem[] = [{ label: "Manage projects", onSelect: () => router.navigate({ name: "projects" }) }];
  if (live) {
    if (!archiveUnsupported) {
      items.push({
        label: `Archive finished (${finished.length})`,
        onSelect: () => setConfirming(true),
        reason: !gate.can("tasks") ? "Offline: reconnect to archive tasks." : finished.length === 0 ? "No finished task to archive." : busyReason,
      });
    }
    items.push({ label: "Refresh", onSelect: onRefresh, reason: busyReason });
  }
  return (
    <>
      <ActionMenu label="More actions" trigger={<span aria-hidden="true">⋯</span>} items={items} />
      {confirming && (
        <Dialog
          title={`Archive ${finished.length} finished ${finished.length === 1 ? "task" : "tasks"}?`}
          onClose={() => setConfirming(false)}
          footer={
            <>
              <Button onClick={() => setConfirming(false)}>Keep</Button>
              <Button
                variant="danger"
                onClick={() => {
                  setConfirming(false);
                  void run(`Archive ${finished.length} finished tasks`, () => archiveSend(data, finished));
                }}
              >
                Confirm archive
              </Button>
            </>
          }
        >
          <p>Archived tasks leave the lists; their history is kept.</p>
        </Dialog>
      )}
    </>
  );
}

export type { Controls };
