/**
 * Test double for the daemon's `queue.snapshot`, computed from a fake task list.
 *
 * It is a small model of the scheduler, not the scheduler: `Running` tasks hold slots; a `Queued`
 * task waits on an unfinished dependency, then on the global cap, then on its project cap, and
 * otherwise starts on the next pass. Positions put starting tasks first, then capacity-blocked
 * ones, in task-id order; dependency-blocked tasks have none. The real contract is exercised
 * against the real daemon by `e2e/queue.e2e.test.ts`.
 */
import type { TaskRecord } from "../daemon/types";

export interface FakeQueueSettings {
  max_concurrent_workers: number;
  max_workers_per_project: number;
  dispatch_blocked: boolean;
}

export const DEFAULT_QUEUE_SETTINGS: FakeQueueSettings = {
  max_concurrent_workers: 2,
  max_workers_per_project: 2,
  dispatch_blocked: false,
};

const byId = (left: TaskRecord, right: TaskRecord) => left.contract.id.localeCompare(right.contract.id);

export function fakeQueueSnapshot(tasks: readonly TaskRecord[], settings: FakeQueueSettings, sequence = 1, epoch = "epoch-1") {
  const running = tasks.filter((task) => task.state === "Running").sort(byId);
  const perProject = new Map<string, number>();
  for (const task of running) perProject.set(task.contract.project_id, (perProject.get(task.contract.project_id) ?? 0) + 1);
  let busy = running.length;
  const state = new Map(tasks.map((task) => [task.contract.id, task.state]));

  const starting: string[] = [];
  const blocked: string[] = [];
  const queued = tasks
    .filter((task) => task.state === "Queued")
    .sort(byId)
    .map((task) => {
      const project = task.contract.project_id;
      const unmet = task.contract.dependencies.find((dependency) => state.get(dependency) !== "Succeeded");
      let reason = "other";
      let waitingOn: string | null = null;
      let detail: string | null = null;
      if (unmet) {
        reason = "dependency";
        waitingOn = unmet;
      } else if (busy >= settings.max_concurrent_workers) {
        reason = "global_cap";
        blocked.push(task.contract.id);
      } else if ((perProject.get(project) ?? 0) >= settings.max_workers_per_project) {
        reason = "project_cap";
        blocked.push(task.contract.id);
      } else {
        detail = settings.dispatch_blocked ? "dispatch_blocked" : "dispatch_pending";
        starting.push(task.contract.id);
        busy += 1;
        perProject.set(project, (perProject.get(project) ?? 0) + 1);
      }
      return { task_id: task.contract.id, project_id: project, waiting_reason: reason, waiting_on: waitingOn, waiting_detail: detail };
    });
  const order = [...starting, ...blocked];
  const withPositions = queued
    .map((entry) => ({ ...entry, position: order.includes(entry.task_id) ? order.indexOf(entry.task_id) + 1 : null }))
    .sort((left, right) => (left.position ?? Infinity) - (right.position ?? Infinity) || left.task_id.localeCompare(right.task_id));

  return {
    as_of_sequence: sequence,
    epoch,
    limits: {
      max_concurrent_workers: settings.max_concurrent_workers,
      max_workers_per_project: settings.max_workers_per_project,
    },
    dispatch_blocked: settings.dispatch_blocked,
    running: running.map((task) => ({ task_id: task.contract.id, project_id: task.contract.project_id, agent: task.contract.agent })),
    queued: withPositions,
  };
}
