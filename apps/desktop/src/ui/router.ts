/**
 * Minimal in-house router. Route state is local UI state; navigation never
 * issues a daemon request. The hash form lets a reload return to a screen.
 *
 * `review` (S6) is owned by M5-04; this route only carries the identifiers.
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

export type Route =
  | { name: "projects" }
  | { name: "project"; projectId: string }
  | { name: "tasks"; projectId: string; filter: TaskFilter }
  | { name: "task"; projectId: string; taskId: string }
  | { name: "fixture-task"; projectId: string; id: string }
  | { name: "coordinator"; projectId: string }
  | { name: "review"; projectId: string; taskId: string | null };

const enc = encodeURIComponent;

export function formatRoute(route: Route): string {
  switch (route.name) {
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
      return `#/project/${enc(route.projectId)}/review/${enc(route.taskId ?? "")}`;
  }
}

export function parseRoute(hash: string): Route {
  let parts: string[];
  try {
    parts = hash.replace(/^#\/?/, "").split("/").map(decodeURIComponent);
  } catch {
    return { name: "projects" };
  }
  if (parts[0] !== "project" || !parts[1]) return { name: "projects" };
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
      return { name: "review", projectId, taskId: parts[3] || null };
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
  const fromHash = () => router.navigate(parseRoute(target.location.hash));
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
