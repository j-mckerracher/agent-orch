/**
 * Application data over the M5-02 contract.
 *
 * Snapshots are refetched after every (re)connect and, coalesced, after each
 * live event. Failed loads keep the previous data (shown as stale by the UI).
 * Session timelines page with `sessions.events`; after a reconnect they are
 * refetched from sequence 0, because resume across a store reset is not
 * trusted (M5-02 D3).
 */
import type { ConnectionManager } from "./connection";
import { DaemonError, asDaemonError } from "./errors";
import { IdempotencyTracker } from "./idempotency";
import { Store } from "./store";
import type {
  Approval,
  DaemonEvent,
  ProjectRow,
  SessionEntry,
  TaskRecord,
  TaskSubmission,
} from "./types";

export interface Loadable<T> {
  status: "idle" | "loading" | "ready" | "error";
  data: T | null;
  error: DaemonError | null;
}

export interface Timeline {
  taskId: string;
  sessionId: string;
  status: "loading" | "ready" | "error";
  events: DaemonEvent[];
  more: boolean;
  /** Cursor for the next page of this session. */
  next: number;
  loadingMore: boolean;
  error: DaemonError | null;
}

export interface AppDataSnapshot {
  projects: Loadable<ProjectRow[]>;
  tasks: Loadable<TaskRecord[]>;
  sessions: Loadable<SessionEntry[]>;
  approvals: Loadable<Approval[]>;
  timelines: Record<string, Timeline>;
  /** `tasks.get` per watched task id. */
  taskDetails: Record<string, Loadable<TaskRecord>>;
  /** `sessions.list({task_id})` per watched task id. */
  taskSessions: Record<string, Loadable<SessionEntry[]>>;
  /** `sessions.get` keyed by `timelineKey`. */
  sessionDetails: Record<string, Loadable<SessionEntry>>;
}

type Api = ReturnType<ConnectionManager["api"]>;
type KeyedName = "taskDetails" | "taskSessions" | "sessionDetails";

export type MutationResult<T> = { ok: true; value: T } | { ok: false; error: DaemonError };

const idle = <T>(): Loadable<T> => ({ status: "idle", data: null, error: null });

export const timelineKey = (taskId: string, sessionId: string): string => `${taskId}/${sessionId}`;

export interface AppDataOptions {
  coalesceMs?: number;
  tracker?: IdempotencyTracker;
}

export class AppData extends Store<AppDataSnapshot> {
  readonly tracker: IdempotencyTracker;
  private readonly coalesceMs: number;
  private refreshing = false;
  private dirty = false;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private readonly detach: Array<() => void> = [];
  private readonly watched = new Map<string, () => Promise<void>>();
  private readonly timelineLoads = new Map<string, number>();

  constructor(
    private readonly connection: ConnectionManager,
    options: AppDataOptions = {},
  ) {
    super({
      projects: idle(),
      tasks: idle(),
      sessions: idle(),
      approvals: idle(),
      timelines: {},
      taskDetails: {},
      taskSessions: {},
      sessionDetails: {},
    });
    this.coalesceMs = options.coalesceMs ?? 150;
    this.tracker = options.tracker ?? new IdempotencyTracker();
    this.detach.push(
      connection.onConnected(() => {
        void this.refreshAll();
        void this.reloadTimelines();
      }),
      connection.onEvent((event) => this.onEvent(event)),
    );
  }

  dispose(): void {
    for (const off of this.detach) off();
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }

  /** Refetches every snapshot; overlapping calls collapse into one rerun. */
  async refreshAll(): Promise<void> {
    if (this.refreshing) {
      this.dirty = true;
      return;
    }
    this.refreshing = true;
    try {
      do {
        this.dirty = false;
        await Promise.all([
          this.load("projects", (api) => api.projectsList()),
          this.load("tasks", (api) => api.tasksList()),
          this.load("sessions", (api) => api.sessionsList(null)),
          this.load("approvals", (api) => api.approvalsList()),
          ...[...this.watched.values()].map((reload) => reload()),
        ]);
      } while (this.dirty);
    } finally {
      this.refreshing = false;
    }
  }

  private async load<K extends "projects" | "tasks" | "sessions" | "approvals">(
    key: K,
    fetcher: (api: ReturnType<ConnectionManager["api"]>) => Promise<NonNullable<AppDataSnapshot[K]["data"]>>,
  ): Promise<void> {
    const previous = this.getSnapshot()[key];
    this.update({ [key]: { ...previous, status: "loading", error: null } } as Partial<AppDataSnapshot>);
    try {
      const data = await fetcher(this.connection.api());
      this.update({ [key]: { status: "ready", data, error: null } } as Partial<AppDataSnapshot>);
    } catch (error) {
      const failure = asDaemonError(error);
      this.update({ [key]: { status: "error", data: previous.data, error: failure } } as Partial<AppDataSnapshot>);
    }
  }

  /** Loads `tasks.get` and `sessions.list` for a task and keeps them fresh. */
  watchTask(taskId: string): Promise<void> {
    const reload = () =>
      Promise.all([
        this.loadKeyed("taskDetails", taskId, (api) => api.tasksGet(taskId)),
        this.loadKeyed("taskSessions", taskId, (api) => api.sessionsList(taskId)),
      ]).then(() => undefined);
    this.watched.set(`task:${taskId}`, reload);
    return reload();
  }

  /** Loads `sessions.get` for a recorded session and keeps it fresh. */
  watchSession(taskId: string, sessionId: string): Promise<void> {
    const key = timelineKey(taskId, sessionId);
    const reload = () => this.loadKeyed("sessionDetails", key, (api) => api.sessionsGet(taskId, sessionId));
    this.watched.set(`session:${key}`, reload);
    return reload();
  }

  private async loadKeyed<N extends KeyedName>(
    name: N,
    key: string,
    fetcher: (api: Api) => Promise<NonNullable<AppDataSnapshot[N][string]["data"]>>,
  ): Promise<void> {
    const write = (value: Loadable<unknown>) =>
      this.update({ [name]: { ...this.getSnapshot()[name], [key]: value } } as Partial<AppDataSnapshot>);
    const previous = this.getSnapshot()[name][key] ?? idle<unknown>();
    write({ ...previous, status: "loading", error: null });
    try {
      write({ status: "ready", data: await fetcher(this.connection.api()), error: null });
    } catch (error) {
      write({ status: "error", data: previous.data, error: asDaemonError(error) });
    }
  }

  private onEvent(event: DaemonEvent): void {
    if (this.timer === null) {
      this.timer = setTimeout(() => {
        this.timer = null;
        void this.refreshAll();
      }, this.coalesceMs);
    }
    if (event.session_id) {
      const key = timelineKey(event.task_id, event.session_id);
      const timeline = this.getSnapshot().timelines[key];
      if (timeline && timeline.status === "ready" && !timeline.more) {
        void this.loadTimeline(event.task_id, event.session_id, "more");
      }
    }
  }

  /**
   * `reset` fetches from the start; `more` continues from the saved cursor.
   * Only the latest load for a timeline may write, so an overlapping reset
   * and "more" cannot overwrite each other with a stale base.
   */
  async loadTimeline(taskId: string, sessionId: string, mode: "reset" | "more" = "reset"): Promise<void> {
    const key = timelineKey(taskId, sessionId);
    const current = this.getSnapshot().timelines[key];
    const continuing = mode === "more" && current !== undefined && current.status !== "loading";
    if (mode === "more" && current?.loadingMore) return;
    const generation = (this.timelineLoads.get(key) ?? 0) + 1;
    this.timelineLoads.set(key, generation);
    const stale = () => this.timelineLoads.get(key) !== generation;
    const base: Timeline = continuing
      ? { ...current, loadingMore: true, error: null }
      : { taskId, sessionId, status: "loading", events: [], more: false, next: 0, loadingMore: false, error: null };
    this.setTimeline(key, base);
    try {
      const after = continuing ? current.next : undefined;
      const page = await this.connection.api().sessionsEvents(taskId, sessionId, after);
      if (stale()) return;
      const seen = new Set(base.events.map((event) => event.sequence));
      const fresh = page.events.filter((event) => !seen.has(event.sequence));
      this.setTimeline(key, {
        taskId,
        sessionId,
        status: "ready",
        events: [...base.events, ...fresh],
        more: page.more,
        next: page.next_after_sequence,
        loadingMore: false,
        error: null,
      });
    } catch (error) {
      if (stale()) return;
      this.setTimeline(key, {
        ...base,
        status: base.events.length > 0 || continuing ? "ready" : "error",
        loadingMore: false,
        error: asDaemonError(error),
      });
    }
  }

  private async reloadTimelines(): Promise<void> {
    const loaded = Object.values(this.getSnapshot().timelines);
    await Promise.all(loaded.map((t) => this.loadTimeline(t.taskId, t.sessionId, "reset")));
  }

  private setTimeline(key: string, timeline: Timeline): void {
    this.update({ timelines: { ...this.getSnapshot().timelines, [key]: timeline } });
  }

  async registerProject(path: string): Promise<MutationResult<ProjectRow>> {
    try {
      const value = await this.connection.api().projectsRegister(path);
      void this.refreshAll();
      return { ok: true, value };
    } catch (error) {
      return { ok: false, error: asDaemonError(error) };
    }
  }

  async submitTask(submission: TaskSubmission): Promise<MutationResult<TaskRecord>> {
    const scope = `submit:${submission.project_id}`;
    const key = this.tracker.keyFor(scope, JSON.stringify(submission));
    try {
      const value = await this.connection.api().tasksSubmit(key, submission);
      this.tracker.settle(scope, null);
      void this.refreshAll();
      return { ok: true, value };
    } catch (error) {
      const failure = asDaemonError(error);
      this.tracker.settle(scope, failure);
      return { ok: false, error: failure };
    }
  }

  /** The daemon acknowledges the cancel; it does not report process termination. */
  async cancelTask(taskId: string): Promise<MutationResult<TaskRecord>> {
    const scope = `cancel:${taskId}`;
    const key = this.tracker.keyFor(scope, taskId);
    try {
      const value = await this.connection.api().tasksCancel(taskId, key);
      this.tracker.settle(scope, null);
      void this.refreshAll();
      return { ok: true, value };
    } catch (error) {
      const failure = asDaemonError(error);
      this.tracker.settle(scope, failure);
      return { ok: false, error: failure };
    }
  }
}
