/*
 * Live workstream controls (M7-23): every button here calls one real `workstreams.*` method
 * through the D9 source. Each write quotes the revision the operator last read, so a stale
 * edit is a daemon conflict that the runner answers by reloading, never by overwriting.
 */
import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import type { AppData } from "../../../daemon/appData";
import { asDaemonError, type DaemonError } from "../../../daemon/errors";
import type { GateState, WorkstreamGate, WorkstreamObjective } from "../../../sources/surfaces/types";
import { canArchive, stateLabel } from "../../format";
import { useGate, useServices, useStore } from "../../services";
import { Button, Pill } from "../../theme";
import { Dialog } from "../../Dialog";
import { ActionMenu, type MenuItem } from "./ActionMenu";
import { ErrorNotice, Notice } from "../../widgets";
import { reloadWorkstreams, taskLabelOf, type WsGroup, type WsTask } from "./model";

export type Run = <T>(action: string, send: () => Promise<T>) => Promise<T | null>;

interface Outcome {
  action: string;
  error: DaemonError | null;
  reloaded: boolean;
}

/** Serializes mutations: one in flight at a time, with the outcome shown until the next one. */
export function useMutationRunner(): { run: Run; busy: boolean; outcome: Outcome | null; clear: () => void } {
  const [busy, setBusy] = useState(false);
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const { connected } = useGate();
  const wasConnected = useRef(connected);
  // A failure banner describes a past attempt; once the connection is back it is no longer current.
  useEffect(() => {
    if (connected && !wasConnected.current) setOutcome((current) => (current?.error ? null : current));
    wasConnected.current = connected;
  }, [connected]);
  const run: Run = async (action, send) => {
    setBusy(true);
    setOutcome(null);
    try {
      const result = await send();
      reloadWorkstreams();
      setOutcome({ action, error: null, reloaded: true });
      return result;
    } catch (error) {
      const failure = asDaemonError(error);
      // A stale or vanished record is re-read so the operator sees the daemon's current state.
      const reloaded = failure.kind === "conflict" || failure.kind === "not-found";
      if (reloaded) reloadWorkstreams();
      setOutcome({ action, error: failure, reloaded });
      return null;
    } finally {
      setBusy(false);
    }
  };
  return { run, busy, outcome, clear: () => setOutcome(null) };
}

export function OutcomeNotice({ outcome }: { outcome: Outcome | null }) {
  if (outcome === null) return null;
  if (outcome.error === null) {
    return (
      <Notice tone="ok" role="status">
        {outcome.action}: recorded by the daemon.
      </Notice>
    );
  }
  return (
    <>
      <ErrorNotice error={outcome.error} action={outcome.action} />
      {outcome.reloaded && <p className="ws-subtle">The workstream data was reloaded from the daemon.</p>}
    </>
  );
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
  controls,
}: {
  objective: WorkstreamObjective;
  gate: WorkstreamGate;
  tasks: WsTask[];
  controls: Controls;
}) {
  const { surfaces } = useServices();
  const [target, setTarget] = useState<GateState>(GATE_TARGETS[gate.state][0]);
  const [evidence, setEvidence] = useState("");
  const evidenceAllowed = target !== "pending";
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const evidenceTaskId = evidenceAllowed && evidence !== "" ? evidence : undefined;
    void controls.run(`Set gate "${gate.label}" to ${target}`, () =>
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
          <select
            value={target}
            disabled={controls.busy || controls.locked !== null}
            onChange={(event) => setTarget(event.target.value as GateState)}
          >
            {GATE_TARGETS[gate.state].map((state) => (
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
            disabled={controls.busy || controls.locked !== null || !evidenceAllowed}
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
        <Button type="submit" disabled={controls.busy || controls.locked !== null}>
          Apply to {gate.label}
        </Button>
      </form>
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
      <Button type="submit" disabled={!changed || controls.busy || controls.locked !== null}>
        Revise objective
      </Button>
    </form>
  );
}

export function ObjectiveCard({
  objective,
  tasks,
  controls,
}: {
  objective: WorkstreamObjective;
  tasks: WsTask[];
  controls: Controls;
}) {
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
            <GateRow key={`${gate.id}:${gate.state}`} objective={objective} gate={gate} tasks={tasks} controls={controls} />
          ))}
        </ul>
      )}
      <details>
        <summary>Revise this objective</summary>
        {/* Remounts on every new revision so the fields always start from the daemon's values. */}
        <ReviseForm key={`${objective.id}@${objective.revision}`} objective={objective} controls={controls} />
      </details>
    </section>
  );
}

export function AddObjectiveForm({ group, controls }: { group: WsGroup; controls: Controls }) {
  const { surfaces } = useServices();
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [gates, setGates] = useState("");
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (title.trim() === "") return;
    const list = gates.split("\n").map((line) => line.trim()).filter((line) => line !== "");
    const done = await controls.run(`Add objective "${title.trim()}"`, () =>
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
  const off = controls.busy || controls.locked !== null;
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
      <Button type="submit" disabled={off || title.trim() === ""}>
        Add objective
      </Button>
    </form>
  );
}

/**
 * The detail header's Actions menu. Suspend and the list order are advisory (the scheduler ignores
 * both); archive is terminal and confirmed in a dialog. `fixtureReason` disables every item on placeholder data.
 */
export function WorkstreamActions({
  group,
  first,
  controls,
  fixtureReason,
}: {
  group: WsGroup;
  first: boolean;
  controls: Controls;
  fixtureReason: string | null;
}) {
  const { surfaces } = useServices();
  const [confirming, setConfirming] = useState(false);
  const reason = fixtureReason ?? controls.locked ?? (controls.busy ? "Another change is still being recorded." : null);
  const target = { id: group.id, revision: group.revision };
  const change = (action: string, state: "active" | "suspended" | "archived") =>
    void controls.run(action, () => surfaces.workstreams.update(target, { state }));
  const items: MenuItem[] = [
    group.state === "suspended"
      ? { label: "Resume workstream", onSelect: () => change("Resume workstream", "active"), reason }
      : { label: "Suspend workstream", onSelect: () => change("Suspend workstream", "suspended"), reason },
    {
      label: "Prioritize",
      onSelect: () => void controls.run(`Prioritize "${group.title}"`, () => surfaces.workstreams.prioritize(group.id)),
      reason: reason ?? (first ? `"${group.title}" is already first.` : null),
    },
    { label: "Archive workstream", onSelect: () => setConfirming(true), reason },
  ];
  return (
    <>
      <ActionMenu label="Workstream actions" trigger={<>Actions <span aria-hidden="true">⌄</span></>} items={items} />
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
                  change("Archive workstream", "archived");
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
  controls,
}: {
  group: WsGroup;
  unlinked: WsTask[];
  unlinkedKnown: boolean;
  controls: Controls;
}) {
  const { surfaces } = useServices();
  const [taskId, setTaskId] = useState("");
  const [objectiveId, setObjectiveId] = useState("");
  const objectives = group.objectives.status === "ok" ? group.objectives.value : [];
  const off = controls.busy || controls.locked !== null;
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (taskId === "") return;
    const done = await controls.run(`Link task ${taskId}`, () =>
      surfaces.workstreams.linkTask(group.id, taskId, objectiveId === "" ? undefined : objectiveId),
    );
    if (done) setTaskId("");
  };
  if (!unlinkedKnown) {
    return (
      <p className="ws-subtle" data-testid="ws-unlinked-unknown">
        Tasks outside every workstream are unknown: a workstream could not be read, and its tasks would look unlinked.
      </p>
    );
  }
  if (unlinked.length === 0) {
    return <p className="ws-subtle">Every daemon task is already in a workstream, so there is nothing to link.</p>;
  }
  return (
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
      <Button type="submit" disabled={off || taskId === ""}>
        Link task
      </Button>
    </form>
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
 * confirmation. The workstream-lifecycle actions live on the detail screen, not here.
 */
export function ListMenu({ run, busy, live, onRefresh }: { run: Run; busy: boolean; live: boolean; onRefresh: () => void }) {
  const { data, router } = useServices();
  const gate = useGate();
  const { tasks, archiveUnsupported } = useStore(data);
  const [confirming, setConfirming] = useState(false);
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
