/** Live queue wait reasons (D11 `queue.snapshot`) shared by the task page and the task lists. */
import { useMemo } from "react";
import type { TaskState } from "../daemon/types";
import { notOk } from "../sources/surfaces/result";
import type { ActivitySnapshot, QueueWait } from "../sources/surfaces/types";
import { useServices, useStore, useSurface, type SurfaceState } from "./services";
import "./projectScreens.css";

/** Machine load and disk use change without a daemon event, so queue diagnostics are also re-read on this interval. */
export const QUEUE_RECHECK_MS = 15_000;

const NO_WAITS: ReadonlyMap<string, QueueWait> = new Map();

/**
 * Reads queue diagnostics from a daemon-backed activity source. The answer re-loads in place, with no
 * loading state, when any task changes state, every `QUEUE_RECHECK_MS` and when the window regains focus.
 * Nothing is requested while `enabled` is false or the source is not daemon-backed.
 */
export function useQueueActivity(key: string, enabled = true): SurfaceState<ActivitySnapshot> {
  const { surfaces, data } = useServices();
  const { tasks } = useStore(data);
  const source = surfaces.activity;
  const read = enabled && source.provenance.kind === "daemon";
  const states = useMemo(
    () => (tasks.data ?? []).map((task) => `${task.contract.id}=${task.state}`).join(" "),
    [tasks.data],
  );
  return useSurface(
    () =>
      read
        ? source.activity()
        : Promise.resolve(notOk<ActivitySnapshot>(source.provenance, "unsupported", "Live queue diagnostics are not read here.")),
    `${key}:${read}`,
    source.provenance,
    read ? { pollMs: QUEUE_RECHECK_MS, onFocus: true, reloadOn: states } : undefined,
  );
}

/**
 * Wait reasons by task id, read only while `enabled` (a list shows a Queued row). Empty while loading,
 * after a failed read, while disconnected and with fixture data, so a list never shows a stale reason.
 */
export function useQueueWaits(enabled: boolean): ReadonlyMap<string, QueueWait> {
  const activity = useQueueActivity("queue-waits", enabled);
  return useMemo(() => {
    if (activity.status !== "ok" || activity.provenance.kind !== "daemon") return NO_WAITS;
    const waits = new Map<string, QueueWait>();
    for (const entry of activity.data.queued) if (entry.waiting) waits.set(entry.taskId, entry.waiting);
    return waits;
  }, [activity]);
}

/**
 * The short reason a queued task has not started, shown beside its state in a task list. Renders nothing
 * unless the row itself is Queued. It is not a live region; the task page announces changes and next steps.
 */
export function QueueWaitNote({ state, wait }: { state: TaskState; wait: QueueWait | undefined }) {
  if (state !== "Queued" || !wait) return null;
  return (
    <span className="queue-wait-note" data-testid="queue-wait">
      {wait.summary}
    </span>
  );
}
