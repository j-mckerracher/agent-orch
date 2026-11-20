/** The task's Actions menu: Cancel attempt, Force start and Reassign. An action the daemon cannot take now is not offered. */
import type { ReactNode } from "react";
import { isFixtureId } from "../../../daemon/api";
import { asDaemonError } from "../../../daemon/errors";
import type { SourceResult } from "../../../sources/types";
import { canCancel, stateLabel } from "../../format";
import { ActionStatus, useAction, type Action, type Outcome } from "../../action";
import { useLastingAction } from "../../lastingAction";
import { useGate, useServices, useSource } from "../../services";
import type { MenuItem } from "../workstreams/ActionMenu";
import { failureText } from "../workstreams/Manage";
import { taskRef, type WorkRow } from "./model";

const BUSY_REASON = "Another action is still running for this task.";

/**
 * `items` feeds the menu; `statuses` must render beside the menu trigger, outside the menu, because the menu
 * closes when an item is chosen and can disappear once the task's state no longer offers any item.
 */
export function useTaskActions(row: WorkRow, live: boolean): { items: MenuItem[]; statuses: ReactNode } {
  const { data, sources } = useServices();
  const gate = useGate();
  // Cancel and force start end or move work that may be running, so their result stays until the next one.
  const stop = useLastingAction();
  // Reassign starts one more attempt and removes nothing, so its result clears itself.
  const reassign = useAction();
  const reason = stop.pending || reassign.pending ? BUSY_REASON : null;
  // Fixture ids can never reach a mutation: only a real task in live mode is sent.
  const real = live && row.origin === "real" && !isFixtureId(row.id);
  const linked = real && gate.can("tasks");
  const ref = taskRef(row);
  const retryInfo = useSource(
    () => (real ? sources.retry.retryFor(ref) : Promise.resolve({ state: "empty" } as SourceResult<never>)),
    `agents-retry:${real}:${row.id}:${row.state}`,
    sources.retry.provenance,
  );

  const write = (action: Action, text: string, request: () => Promise<string>) =>
    action.run(`${text}…`, async (): Promise<Outcome> => {
      try {
        return { kind: "saved", text: await request() };
      } catch (error) {
        const failure = asDaemonError(error);
        const unknown = failure.indeterminate ? " The outcome is unknown. Trying again reuses the same request key." : "";
        throw new Error(`${failureText(text, failure)}${unknown}`);
      }
    });

  const items: MenuItem[] = [];
  if (linked && canCancel(row.state)) {
    items.push({
      label: "Cancel attempt",
      reason,
      onSelect: () =>
        void write(stop, "Canceling the task", async () => {
          const result = await data.cancelTask(row.id);
          if (!result.ok) throw result.error;
          return `Cancel request acknowledged; the daemon now records the task as ${stateLabel(result.value.state)}. The cancel applied to the whole task, not only this attempt. This is not proof that any process has stopped.`;
        }),
    });
  }
  // `tasks.force_start` is user-only, like discard: only the app that started the daemon may send it.
  if (linked && row.state === "Queued" && gate.connected && gate.connection.approver) {
    items.push({
      label: "Force start",
      reason,
      onSelect: () =>
        void write(stop, "Forcing the task to start", async () => {
          const result = await data.forceStartTask(row.id);
          if (!result.ok) throw result.error;
          void data.watchTask(row.id);
          return "The daemon accepted the force start. The task leaves the queue when it launches.";
        }),
    });
  }
  if (linked && retryInfo.state === "ready" && retryInfo.data.available && sources.retry.retry) {
    const retry = sources.retry.retry.bind(sources.retry);
    items.push({
      label: "Reassign",
      reason,
      onSelect: () =>
        void write(reassign, "Reassigning the task", async () => {
          await retry(ref);
          void data.watchTask(row.id);
          void data.refreshAll();
          return "The request for a new attempt was accepted by the daemon.";
        }),
    });
  }

  const statuses = (
    <>
      <ActionStatus action={stop} />
      <ActionStatus action={reassign} />
    </>
  );
  return { items, statuses };
}
