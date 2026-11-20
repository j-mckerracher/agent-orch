/** New task form in a dialog: kind, agent and delivery up front; the rest under "More options". */
import { useId, useState } from "react";
import type { DaemonError } from "../daemon/errors";
import type { AutonomyPolicy, DeliveryPolicy, TaskKind } from "../daemon/types";
import type { PermissionLevel } from "../sources/surfaces/types";
import { Dialog } from "./Dialog";
import { buildSubmission } from "./submission";
import { useGate, useServices, useStore } from "./services";
import { Button, Disclosure } from "./theme";
import { ErrorNotice } from "./widgets";
import { PermissionLevelNotes, PermissionLevelPicker } from "./views/workspace/PermissionLevelPicker";
import { TASK_PREFERENCE_KEY, browserStorage, permissionForSend, readSavedLevel, saveLevel } from "./views/workspace/permissionLevel";

export function NewTaskDialog({ projectId, inPlace, onClose }: { projectId: string; inPlace: boolean; onClose: () => void }) {
  const { data, router, drafts } = useServices();
  const draft = useStore(drafts).submit[projectId] ?? drafts.submitDraft(projectId);
  const gate = useGate();
  const formId = useId();
  const [errors, setErrors] = useState<string[]>([]);
  const [failure, setFailure] = useState<DaemonError | null>(null);
  const [busy, setBusy] = useState(false);
  const [permission, setPermission] = useState<PermissionLevel>(() => readSavedLevel(browserStorage(), TASK_PREFERENCE_KEY));
  // An older daemon predates permission levels and runs a task at Restricted, so the form reads Restricted (DPES-1105).
  const olderDaemon = gate.connected && !gate.can("permission_levels");
  const shownLevel: PermissionLevel = olderDaemon ? "restricted" : permission;
  const permissionOnWire = permissionForSend(permission, !olderDaemon);
  const choosePermission = (next: PermissionLevel) => {
    saveLevel(next, browserStorage(), TASK_PREFERENCE_KEY);
    setPermission(next);
  };
  const set = (patch: Parameters<typeof drafts.patchSubmit>[1]) => drafts.patchSubmit(projectId, patch);

  const submit = async () => {
    setFailure(null);
    const built = buildSubmission(projectId, inPlace ? { ...draft, delivery: "ReportOnly" } : draft, permissionOnWire);
    if (!built.ok) {
      setErrors(built.errors);
      return;
    }
    setErrors([]);
    setBusy(true);
    const result = await data.submitTask(built.submission);
    setBusy(false);
    if (result.ok) {
      drafts.clearSubmit(projectId);
      onClose();
      router.navigate({ name: "task", projectId, taskId: result.value.contract.id });
    } else {
      setFailure(result.error);
    }
  };

  return (
    <Dialog
      title="New task"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button type="submit" form={formId} variant="primary" disabled={!gate.can("tasks") || busy}>
            Create task
          </Button>
        </>
      }
    >
      <form
        id={formId}
        className="ps-form"
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <label className="ps-wide">
          Objective
          <textarea value={draft.objective} onChange={(e) => set({ objective: e.target.value })} />
        </label>
        <label>
          Kind
          <select value={draft.kind} onChange={(e) => set({ kind: e.target.value as TaskKind })}>
            <option>Implement</option>
            <option>Investigate</option>
          </select>
        </label>
        <label>
          Agent
          <input
            value={draft.agent}
            placeholder="Settings default"
            onChange={(e) => set({ agent: e.target.value })}
          />
        </label>
        {inPlace ? (
          <p className="muted ps-wide">This folder is the home directory: a task runs in it directly and only reports.</p>
        ) : (
          <label>
            Delivery
            <select value={draft.delivery} onChange={(e) => set({ delivery: e.target.value as DeliveryPolicy })}>
              <option>ReportOnly</option>
              <option>LocalReview</option>
              <option>PullRequest</option>
            </select>
          </label>
        )}
        <Disclosure summary="More options" className="ps-wide">
          <div className="ps-form">
            <PermissionLevelPicker
              level={shownLevel}
              daemonAdvertises={!olderDaemon}
              agent={draft.agent.trim() || undefined}
              surface="task"
              onChange={choosePermission}
            />
            <PermissionLevelNotes olderDaemon={olderDaemon} lastTurn={null} />
            <label>
              Autonomy
              <select value={draft.autonomy} onChange={(e) => set({ autonomy: e.target.value as AutonomyPolicy })}>
                <option>WorkspaceWrites</option>
                <option>ApprovalRequired</option>
              </select>
            </label>
            <label>
              Verification
              <select
                value={draft.verification}
                onChange={(e) => set({ verification: e.target.value as "None" | "Commands" })}
              >
                <option>None</option>
                <option>Commands</option>
              </select>
            </label>
            {draft.verification === "Commands" && (
              <label className="ps-wide">
                Verification commands (one per line; split on spaces, no shell quoting)
                <textarea value={draft.commands} onChange={(e) => set({ commands: e.target.value })} />
              </label>
            )}
            <label>
              Parent task ID (optional)
              <input value={draft.parentId} onChange={(e) => set({ parentId: e.target.value })} />
            </label>
            <label>
              Dependencies (task IDs, comma or space separated)
              <input value={draft.dependencies} onChange={(e) => set({ dependencies: e.target.value })} />
            </label>
            <label>
              Max retries
              <input value={draft.maxRetries} onChange={(e) => set({ maxRetries: e.target.value })} />
            </label>
            <label>
              Max repairs
              <input value={draft.maxRepairs} onChange={(e) => set({ maxRepairs: e.target.value })} />
            </label>
          </div>
        </Disclosure>
        {!gate.connected && <p className="muted ps-wide">Offline: the draft is kept until you can create the task.</p>}
        {errors.length > 0 && (
          <ul role="alert" className="ps-steps ps-wide">
            {errors.map((message) => (
              <li key={message}>{message}</li>
            ))}
          </ul>
        )}
        {failure && <ErrorNotice error={failure} action="Submitting the task" />}
        {failure?.indeterminate && (
          <p role="status" className="ps-wide">
            The outcome is unknown. Submitting the same form again reuses the same request key.
          </p>
        )}
      </form>
    </Dialog>
  );
}
