/**
 * Minimal in-house router. Route state is local UI state; navigation never
 * issues a daemon request. The hash form lets a reload return to a screen.
 *
 * `review` (S6) is owned by M5-04; this route carries the identifiers and an
 * optional query (`fixture=1&evidence=…&changes=…`) that selects explicit demo mode.
 */
import { Store } from "../daemon/store";

export type TaskFilter =
  | "all"
  | "queued"
  | "running"
  | "blocked"
  | "review"
  | "terminal"
  | `state:${string}`;

/** Global (not project-scoped) views of the app shell, in navigation order. Hash form: `#/<view>`. */
export const GLOBAL_VIEWS = ["workspace", "workstreams", "agents", "review"] as const;
export type GlobalView = (typeof GLOBAL_VIEWS)[number];

const isGlobalView = (segment: string | undefined): segment is GlobalView =>
  GLOBAL_VIEWS.includes(segment as GlobalView);

export type Route =
  | { name: "global"; view: GlobalView }
  | { name: "projects" }
  | { name: "project"; projectId: string }
  | { name: "tasks"; projectId: string; filter: TaskFilter }
  | { name: "task"; projectId: string; taskId: string }
  | { name: "fixture-task"; projectId: string; id: string }
  | { name: "coordinator"; projectId: string }
  | { name: "review"; projectId: string; taskId: string | null; query?: string };

const enc = encodeURIComponent;

/** Where an empty or unrecognised hash lands. `RouterStore`'s constructor default is unchanged (`projects`). */
export const DEFAULT_ROUTE: Route = { name: "global", view: "workspace" };

export function formatRoute(route: Route): string {
  switch (route.name) {
    case "global":
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
    case "review":
      return `#/project/${enc(route.projectId)}/review/${enc(route.taskId ?? "")}${route.query ? `?${route.query}` : ""}`;
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
  if (isGlobalView(parts[0])) return { name: "global", view: parts[0] };
  if (parts[0] !== "project") return DEFAULT_ROUTE;
  if (!parts[1]) return { name: "projects" };
  const projectId = parts[1];
  switch (parts[2]) {
    case "tasks":
      return { name: "tasks", projectId, filter: (parts[3] || "all") as TaskFilter };
    case "task":
      return parts[3] ? { name: "task", projectId, taskId: parts[3] } : { name: "project", projectId };
    case "preview":
      return parts[3] ? { name: "fixture-task", projectId, id: parts[3] } : { name: "project", projectId };
    case "coordinator":
      return { name: "coordinator", projectId };
    case "review":
      return query
        ? { name: "review", projectId, taskId: parts[3] || null, query }
        : { name: "review", projectId, taskId: parts[3] || null };
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
    // Normalise this entry before notifying subscribers. Pushing a second entry here
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
