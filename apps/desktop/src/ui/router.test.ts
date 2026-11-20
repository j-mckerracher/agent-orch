import { describe, expect, it } from "vitest";
import { DEFAULT_ROUTE, GLOBAL_VIEWS, RouterStore, formatRoute, parseRoute, syncRouterToHash, type Route } from "./router";

const legacy: Route[] = [
  { name: "projects" },
  { name: "project", projectId: "/a b/repo" },
  { name: "tasks", projectId: "/repo", filter: "open" },
  { name: "tasks", projectId: "/repo", filter: "review" },
  { name: "tasks", projectId: "/repo", filter: "done" },
  { name: "task", projectId: "/repo", taskId: "task-1" },
  { name: "fixture-task", projectId: "/repo", id: "fixture:task-queued" },
  { name: "coordinator", projectId: "/repo" },
  { name: "defaults", projectId: "/repo" },
];

describe("global routes", () => {
  it("formats the five global views as #/<view>", () => {
    expect(GLOBAL_VIEWS.map((view) => formatRoute({ name: "global", view }))).toEqual([
      "#/workspace",
      "#/workstreams",
      "#/agents",
      "#/review",
      "#/settings",
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

  it("opens one candidate or one project on the review view", () => {
    const task: Route = { name: "global", view: "review", taskId: "t 1" };
    expect(formatRoute(task)).toBe("#/review?task=t%201");
    expect(parseRoute(formatRoute(task))).toEqual(task);
    const project: Route = { name: "global", view: "review", projectId: "/repo" };
    expect(formatRoute(project)).toBe("#/review?project=%2Frepo");
    expect(parseRoute(formatRoute(project))).toEqual(project);
    expect(parseRoute("#/review")).toEqual({ name: "global", view: "review" });
    expect(formatRoute({ name: "global", view: "workspace", taskId: "t" })).toBe("#/workspace");
  });

  it("redirects the old project-scoped review hashes to the review view", () => {
    expect(parseRoute("#/project/p/review/t1")).toEqual({ name: "global", view: "review", taskId: "t1" });
    expect(parseRoute("#/project/p/review/t1?fixture=1&evidence=missing")).toEqual({ name: "global", view: "review", taskId: "t1" });
    expect(parseRoute("#/project/p/review")).toEqual({ name: "global", view: "review", projectId: "p" });
    expect(parseRoute("#/project/p/review/")).toEqual({ name: "global", view: "review", projectId: "p" });
  });

  it("formats and parses a selected workstream on the workstreams view", () => {
    const route: Route = { name: "global", view: "workstreams", workstreamId: "ws-1 a" };
    expect(formatRoute(route)).toBe("#/workstreams?workstream=ws-1%20a");
    expect(parseRoute(formatRoute(route))).toEqual(route);
    expect(parseRoute("#/workstreams")).toEqual({ name: "global", view: "workstreams" });
    expect(formatRoute({ name: "global", view: "workspace", workstreamId: "ws-1" })).toBe("#/workspace");
  });

  it("formats and parses a settings page, and keeps the old bare link opening the first page", () => {
    const route: Route = { name: "global", view: "settings", settingsPage: "limits" };
    expect(formatRoute(route)).toBe("#/settings?page=limits");
    expect(parseRoute(formatRoute(route))).toEqual(route);
    expect(parseRoute("#/settings")).toEqual({ name: "global", view: "settings" });
    expect(parseRoute("#/settings?page=")).toEqual({ name: "global", view: "settings" });
    expect(formatRoute({ name: "global", view: "agents", settingsPage: "limits" })).toBe("#/agents");
  });

  it("formats and parses a selected task on the agents view, and keeps the old bare link", () => {
    const route: Route = { name: "global", view: "agents", taskId: "t 1" };
    expect(formatRoute(route)).toBe("#/agents?task=t%201");
    expect(parseRoute(formatRoute(route))).toEqual(route);
    expect(parseRoute("#/agents")).toEqual({ name: "global", view: "agents" });
    expect(formatRoute({ name: "global", view: "workspace", taskId: "t" })).toBe("#/workspace");
  });
});

describe("legacy routes are unchanged", () => {
  it.each(legacy)("round-trips %j", (route) => {
    expect(parseRoute(formatRoute(route))).toEqual(route);
  });

  it("keeps the exact legacy hash formats", () => {
    expect(formatRoute({ name: "projects" })).toBe("#/projects");
    expect(formatRoute({ name: "project", projectId: "/a b/repo" })).toBe("#/project/%2Fa%20b%2Frepo");
    expect(formatRoute({ name: "tasks", projectId: "/repo", filter: "done" })).toBe("#/project/%2Frepo/tasks/done");
    expect(formatRoute({ name: "task", projectId: "/repo", taskId: "t" })).toBe("#/project/%2Frepo/task/t");
    expect(formatRoute({ name: "fixture-task", projectId: "/repo", id: "fixture:x" })).toBe(
      "#/project/%2Frepo/preview/fixture%3Ax",
    );
    expect(formatRoute({ name: "coordinator", projectId: "/repo" })).toBe("#/project/%2Frepo/coordinator");
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

describe("hash synchronization", () => {
  it("follows back/forward (hashchange) and writes the hash on navigation", () => {
    const target = new EventTarget() as unknown as Window;
    (target as { location: { hash: string } }).location = { hash: "#/agents" };
    const router = new RouterStore();
    const off = syncRouterToHash(router, target);
    expect(router.getSnapshot()).toEqual({ name: "global", view: "agents" });

    router.navigate({ name: "global", view: "review" });
    expect(target.location.hash).toBe("#/review");

    target.location.hash = "#/project/%2Frepo/tasks/done";
    target.dispatchEvent(new Event("hashchange"));
    expect(router.getSnapshot()).toEqual({ name: "tasks", projectId: "/repo", filter: "done" });

    target.location.hash = "#/workstreams";
    target.dispatchEvent(new Event("hashchange"));
    expect(router.getSnapshot()).toEqual({ name: "global", view: "workstreams" });
    off();
  });
});


describe("non-canonical browser history", () => {
  it.each([
    ["#/project/fixture:project-alpha", "#/project/fixture%3Aproject-alpha"],
    ["#/unknown", "#/workspace"],
  ])("Back and Forward preserve the entry for %s", async (incoming, canonical) => {
    const original = location.href;
    history.replaceState(null, "", "#/agents");
    const router = new RouterStore();
    const off = syncRouterToHash(router);
    try {
      location.hash = incoming;
      await expect.poll(() => location.hash).toBe(canonical);
      expect(router.getSnapshot()).toEqual(parseRoute(canonical));
      history.back();
      await expect.poll(() => location.hash).toBe("#/agents");
      await expect.poll(() => router.getSnapshot()).toEqual({ name: "global", view: "agents" });
      history.forward();
      await expect.poll(() => location.hash).toBe(canonical);
      await expect.poll(() => router.getSnapshot()).toEqual(parseRoute(canonical));
    } finally {
      off();
      history.replaceState(null, "", original);
    }
  });
});
