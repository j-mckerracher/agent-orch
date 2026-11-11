import { describe, expect, it } from "vitest";
import { DEFAULT_ROUTE, GLOBAL_VIEWS, RouterStore, formatRoute, parseRoute, syncRouterToHash, type Route } from "./router";

const legacy: Route[] = [
  { name: "projects" },
  { name: "project", projectId: "/a b/repo" },
  { name: "tasks", projectId: "/repo", filter: "all" },
  { name: "tasks", projectId: "/repo", filter: "state:Queued" },
  { name: "task", projectId: "/repo", taskId: "task-1" },
  { name: "fixture-task", projectId: "/repo", id: "fixture:task-queued" },
  { name: "coordinator", projectId: "/repo" },
  { name: "review", projectId: "/repo", taskId: "task-1" },
  { name: "review", projectId: "/repo", taskId: null },
  { name: "review", projectId: "/repo", taskId: "fixture-task-1", query: "fixture=1&evidence=missing" },
];

describe("global routes", () => {
  it("formats the four global views as #/<view>", () => {
    expect(GLOBAL_VIEWS.map((view) => formatRoute({ name: "global", view }))).toEqual([
      "#/workspace",
      "#/workstreams",
      "#/agents",
      "#/review",
    ]);
  });

  it.each(GLOBAL_VIEWS)("round-trips %s", (view) => {
    const route: Route = { name: "global", view };
    expect(parseRoute(formatRoute(route))).toEqual(route);
  });

  it("lands on the workspace for an empty, bare or unknown hash", () => {
    for (const hash of ["", "#", "#/", "#/nonsense", "#/workspace/extra"]) {
      expect(parseRoute(hash), hash).toEqual(DEFAULT_ROUTE);
    }
    expect(DEFAULT_ROUTE).toEqual({ name: "global", view: "workspace" });
  });

  it("keeps the global review view distinct from the project-scoped review route", () => {
    expect(parseRoute("#/review")).toEqual({ name: "global", view: "review" });
    expect(parseRoute("#/project/p/review/t1")).toEqual({ name: "review", projectId: "p", taskId: "t1" });
    expect(parseRoute("#/project/p/review")).toEqual({ name: "review", projectId: "p", taskId: null });
  });
});

describe("legacy routes are unchanged", () => {
  it.each(legacy)("round-trips %j", (route) => {
    expect(parseRoute(formatRoute(route))).toEqual(route);
  });

  it("keeps the exact legacy hash formats", () => {
    expect(formatRoute({ name: "projects" })).toBe("#/projects");
    expect(formatRoute({ name: "project", projectId: "/a b/repo" })).toBe("#/project/%2Fa%20b%2Frepo");
    expect(formatRoute({ name: "tasks", projectId: "/repo", filter: "state:Queued" })).toBe(
      "#/project/%2Frepo/tasks/state%3AQueued",
    );
    expect(formatRoute({ name: "task", projectId: "/repo", taskId: "t" })).toBe("#/project/%2Frepo/task/t");
    expect(formatRoute({ name: "fixture-task", projectId: "/repo", id: "fixture:x" })).toBe(
      "#/project/%2Frepo/preview/fixture%3Ax",
    );
    expect(formatRoute({ name: "coordinator", projectId: "/repo" })).toBe("#/project/%2Frepo/coordinator");
    expect(formatRoute({ name: "review", projectId: "/repo", taskId: "t", query: "fixture=1" })).toBe(
      "#/project/%2Frepo/review/t?fixture=1",
    );
  });

  it("still resolves the project list, incomplete project hashes and malformed escapes", () => {
    expect(parseRoute("#/projects")).toEqual({ name: "projects" });
    expect(parseRoute("#/project")).toEqual({ name: "projects" });
    expect(parseRoute("#/project/")).toEqual({ name: "projects" });
    expect(parseRoute("#/project/%E0%A4%A")).toEqual({ name: "projects" });
    expect(parseRoute("#/project/p/task")).toEqual({ name: "project", projectId: "p" });
    expect(parseRoute("#/project/p/unknown")).toEqual({ name: "project", projectId: "p" });
  });

  it("keeps RouterStore's own default as the project list", () => {
    expect(new RouterStore().getSnapshot()).toEqual({ name: "projects" });
  });
});

describe("hash synchronisation", () => {
  it("follows back/forward (hashchange) and writes the hash on navigation", () => {
    const target = new EventTarget() as unknown as Window;
    (target as { location: { hash: string } }).location = { hash: "#/agents" };
    const router = new RouterStore();
    const off = syncRouterToHash(router, target);
    expect(router.getSnapshot()).toEqual({ name: "global", view: "agents" });

    router.navigate({ name: "global", view: "review" });
    expect(target.location.hash).toBe("#/review");

    target.location.hash = "#/project/%2Frepo/tasks/running";
    target.dispatchEvent(new Event("hashchange"));
    expect(router.getSnapshot()).toEqual({ name: "tasks", projectId: "/repo", filter: "running" });

    target.location.hash = "#/workstreams";
    target.dispatchEvent(new Event("hashchange"));
    expect(router.getSnapshot()).toEqual({ name: "global", view: "workstreams" });
    off();
  });
});
