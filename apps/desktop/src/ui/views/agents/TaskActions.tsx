/** The task's Actions menu: Cancel attempt and Reassign. An action the daemon cannot take now is not offered. */
import { useState, type ReactNode } from "react";
import { isFixtureId } from "../../../daemon/api";
import { asDaemonError, type DaemonError } from "../../../daemon/errors";
import type { SourceResult } from "../../../sources/types";
import { canCancel, stateLabel } from "../../format";
import { useGate, useServices, useSource } from "../../services";
import { ErrorNotice } from "../../widgets";
import type { MenuItem } from "../workstreams/ActionMenu";
import { taskRef, type WorkRow } from "./model";

export function useTaskActions(row: WorkRow, live: boolean): { items: MenuItem[]; notices: ReactNode } {
  const { data, sources } = useServices();
  const gate = useGate();
  const [note, setNote] = useState<string | null>(null);
  const [failure, setFailure] = useState<{ error: DaemonError; action: string } | null>(null);
  const [busy, setBusy] = useState(false);
  // Fixture ids can never reach a mutation: only a real task in live mode is sent.
  const real = live && row.origin === "real" && !isFixtureId(row.id);
  const linked = real && gate.can("tasks");
  const ref = taskRef(row);
  const retryInfo = useSource(
    () => (real ? sources.retry.retryFor(ref) : Promise.resolve({ state: "empty" } as SourceResult<never>)),
    `agents-retry:${real}:${row.id}:${row.state}`,
    sources.retry.provenance,
  );

  const run = async (action: string, request: () => Promise<string>) => {
    if (busy) return;
    setNote(null);
    setFailure(null);
    setBusy(true);
    try {
      setNote(await request());
    } catch (error) {
      setFailure({ error: asDaemonError(error), action });
    } finally {
      setBusy(false);
    }
  };

  const items: MenuItem[] = [];
  if (linked && canCancel(row.state)) {
    items.push({
      label: "Cancel attempt",
      onSelect: () =>
        void run("Canceling the task", async () => {
          const result = await data.cancelTask(row.id);
          if (!result.ok) throw result.error;
          return `Cancel request acknowledged; the daemon now records the task as ${stateLabel(result.value.state)}. The cancel applied to the whole task, not only this attempt. This is not proof that any process has stopped.`;
        }),
    });
  }
  if (linked && retryInfo.state === "ready" && retryInfo.data.available && sources.retry.retry) {
    const retry = sources.retry.retry.bind(sources.retry);
    items.push({
      label: "Reassign",
      onSelect: () =>
        void run("Reassigning the task", async () => {
          await retry(ref);
          void data.watchTask(row.id);
          void data.refreshAll();
          return "The request for a new attempt was accepted by the daemon.";
        }),
    });
  }

  const notices = (
    <>
      {note && <p role="status">{note}</p>}
      {failure && <ErrorNotice error={failure.error} action={failure.action} />}
      {failure?.error.indeterminate && <p role="status">The outcome is unknown. Trying again reuses the same request key.</p>}
    </>
  );
  return { items, notices };
}
