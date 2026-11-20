/**
 * Minimal in-house router. Route state is local UI state; navigation never
 * issues a daemon request. The hash form lets a reload return to a screen.
 *
 * The review view (`#/review`) opens one candidate with `?task=<id>` and can start filtered to one project with
 * `?project=<id>`. The old project-scoped form `#/project/<id>/review/<task>` is read as that route.
 */
import { Store } from "../daemon/store";

/** The project task list's three filters. */
export type TaskFilter = "open" | "review" | "done";

/**
 * Hash values from before the three-filter list: queued, running, blocked and a non-final state are Open,
 * `terminal` and a final state are Done. Anything unknown opens the default.
 */
export function normalizeFilter(value: string): TaskFilter {
  if (value === "review" || value === "done") return value;
  if (value === "terminal") return "done";
  const state = value.startsWith("state:") ? value.slice("state:".length) : "";
  return ["Succeeded", "Failed", "VerificationFailed", "Cancelled"].includes(state) ? "done" : "open";
}

/** Global (not project-scoped) views of the app shell, in navigation order. Hash form: `#/<view>`. */
export const GLOBAL_VIEWS = ["workspace", "workstreams", "agents", "review", "settings"] as const;
export type GlobalView = (typeof GLOBAL_VIEWS)[number];

const isGlobalView = (segment: string | undefined): segment is GlobalView =>
  GLOBAL_VIEWS.includes(segment as GlobalView);

export type Route =
  | { name: "global"; view: GlobalView; workstreamId?: string; taskId?: string; projectId?: string }
  | { name: "projects" }
  | { name: "project"; projectId: string }
  | { name: "tasks"; projectId: string; filter: TaskFilter }
  | { name: "task"; projectId: string; taskId: string }
  | { name: "fixture-task"; projectId: string; id: string }
  | { name: "coordinator"; projectId: string }
  | { name: "defaults"; projectId: string };

const enc = encodeURIComponent;

/** Where an empty or unrecognized hash lands. `RouterStore`'s constructor default is unchanged (`projects`). */
export const DEFAULT_ROUTE: Route = { name: "global", view: "workspace" };

export function formatRoute(route: Route): string {
  switch (route.name) {
    case "global":
      // The selected workstream and task are only meaningful on their own views (DPES-1105, DPES-1044).
      if (route.workstreamId !== undefined && route.view === "workstreams") return `#/workstreams?workstream=${enc(route.workstreamId)}`;
      if (route.taskId !== undefined && (route.view === "agents" || route.view === "review")) return `#/${route.view}?task=${enc(route.taskId)}`;
      if (route.view === "review" && route.projectId !== undefined) return `#/review?project=${enc(route.projectId)}`;
      return `#/${route.view}`;
    case "projects":
      return "#/projects";
    case "project":
      return `#/project/${enc(route.projectId)}`;
    case "tasks":
      return `#/project/${enc(route.projectId)}/tasks/${enc(route.filter)}`;
    case "task":
      return `#/project/${enc(route.projectId)}/task/${enc(route.taskId)}`;
    case "fixture-task":
      return `#/project/${enc(route.projectId)}/preview/${enc(route.id)}`;
    case "coordinator":
      return `#/project/${enc(route.projectId)}/coordinator`;
    case "defaults":
      return `#/project/${enc(route.projectId)}/defaults`;
  }
}

export function parseRoute(hash: string): Route {
  let parts: string[];
  const queryAt = hash.indexOf("?");
  const query = queryAt >= 0 ? hash.slice(queryAt + 1) : "";
  try {
    parts = (queryAt >= 0 ? hash.slice(0, queryAt) : hash).replace(/^#\/?/, "").split("/").map(decodeURIComponent);
  } catch {
    return { name: "projects" };
  }
  if (parts[0] === "projects") return { name: "projects" };
  if (isGlobalView(parts[0])) {
    const params = new URLSearchParams(query);
    if (parts[0] === "review") {
      const taskId = params.get("task");
      const projectId = params.get("project");
      if (taskId) return { name: "global", view: "review", taskId };
      return projectId ? { name: "global", view: "review", projectId } : { name: "global", view: "review" };
    }
    const workstreamId = parts[0] === "workstreams" ? params.get("workstream") : null;
    if (workstreamId) return { name: "global", view: parts[0], workstreamId };
    const taskId = parts[0] === "agents" ? params.get("task") : null;
    return taskId ? { name: "global", view: parts[0], taskId } : { name: "global", view: parts[0] };
  }
  if (parts[0] !== "project") return DEFAULT_ROUTE;
  if (!parts[1]) return { name: "projects" };
  const projectId = parts[1];
  switch (parts[2]) {
    case "tasks":
      return { name: "tasks", projectId, filter: normalizeFilter(parts[3] ?? "") };
    case "task":
      return parts[3] ? { name: "task", projectId, taskId: parts[3] } : { name: "project", projectId };
    case "preview":
      return parts[3] ? { name: "fixture-task", projectId, id: parts[3] } : { name: "project", projectId };
    case "coordinator":
      return { name: "coordinator", projectId };
    case "defaults":
      return { name: "defaults", projectId };
    case "review":
      // The project-scoped review page moved into the Review view.
      return parts[3] ? { name: "global", view: "review", taskId: parts[3] } : { name: "global", view: "review", projectId };
    default:
      return { name: "project", projectId };
  }
}

export class RouterStore extends Store<Route> {
  constructor(initial: Route = { name: "projects" }) {
    super(initial);
  }

  readonly navigate = (route: Route): void => {
    this.set(route);
  };
}

/** Mirrors a router to `location.hash` in a browser; returns a disposer. */
export function syncRouterToHash(router: RouterStore, target: Window = window): () => void {
  const fromHash = () => {
    const route = parseRoute(target.location.hash);
    const canonical = formatRoute(route);
    // Normalize this entry before notifying subscribers. Pushing a second entry here
    // would trap Back on the original non-canonical hash and discard Forward history.
    if (target.location.hash !== canonical) target.history.replaceState(target.history.state, "", canonical);
    router.navigate(route);
  };
  if (target.location.hash) fromHash();
  const off = router.subscribe(() => {
    const next = formatRoute(router.getSnapshot());
    if (target.location.hash !== next) target.location.hash = next;
  });
  target.addEventListener("hashchange", fromHash);
  return () => {
    off();
    target.removeEventListener("hashchange", fromHash);
  };
}
