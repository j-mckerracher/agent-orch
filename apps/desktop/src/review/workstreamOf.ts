// Workstream title for a task, from the daemon workstream list (D9). Returns null when the
// list is not loaded or the task belongs to no workstream, so the identity panel shows
// "not available" rather than a guess.
import { useMemo } from "react";
import { useServices, useSurface } from "../ui/services";

export function useWorkstreamOf(): (taskId: string) => string | null {
  const { surfaces } = useServices();
  const workstreams = useSurface(
    () => surfaces.workstreams.workstreams(),
    "review-workstreams",
    surfaces.workstreams.provenance,
  );
  return useMemo(() => {
    const byTask = new Map<string, string>();
    if (workstreams.status === "ok") {
      for (const workstream of workstreams.data) {
        for (const task of workstream.tasks) byTask.set(task.id, workstream.title);
      }
    }
    return (taskId: string) => byTask.get(taskId) ?? null;
  }, [workstreams]);
}
