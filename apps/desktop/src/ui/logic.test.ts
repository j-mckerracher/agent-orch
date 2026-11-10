import { describe, expect, it } from "vitest";
import { emptySubmitDraft, type SubmitDraft } from "./drafts";
import { matchesFilter } from "./format";
import { RouterStore, formatRoute, parseRoute, type Route } from "./router";
import { buildSubmission } from "./submission";
import { mkApproval, mkTask } from "../test/fakeDaemon";

const draft = (patch: Partial<SubmitDraft> = {}): SubmitDraft => ({
  ...emptySubmitDraft(),
  objective: "  Fix it  ",
  agent: " codex ",
  ...patch,
});

describe("buildSubmission", () => {
  it("trims, applies defaults and builds a contract without an id", () => {
    const result = buildSubmission("/repo", draft());
    expect(result).toEqual({
      ok: true,
      submission: {
        project_id: "/repo",
        parent_id: null,
        objective: "Fix it",
        kind: "Implement",
        agent: "codex",
        dependencies: [],
        verification: "None",
        delivery: "LocalReview",
        autonomy: "ApprovalRequired",
        max_retries: 1,
        max_repairs: 1,
      },
    });
  });

  it("requires an objective and an agent; no agent is chosen for the user", () => {
    const result = buildSubmission("/repo", draft({ objective: " ", agent: "" }));
    expect(result.ok).toBe(false);
    expect(!result.ok && result.errors.join(" ")).toMatch(/Objective is required.*Agent is required/);
  });

  it("splits verification commands on whitespace, one argv per line", () => {
    const result = buildSubmission("/repo", draft({ verification: "Commands", commands: "cargo test\n\n npm  run build " }));
    expect(result.ok && result.submission.verification).toEqual({
      Commands: { commands: [["cargo", "test"], ["npm", "run", "build"]] },
    });
  });

  it("rejects Commands with no command", () => {
    expect(buildSubmission("/repo", draft({ verification: "Commands", commands: "  \n" })).ok).toBe(false);
  });

  it("rejects bad limits, duplicate dependencies and placeholder ids", () => {
    for (const patch of [
      { maxRetries: "-1" },
      { maxRepairs: "1.5" },
      { maxRetries: "" },
      { maxRepairs: "101" },
      { dependencies: "a, a" },
      { dependencies: "fixture:task-queued" },
      { parentId: "fixture:task-queued" },
    ]) {
      expect(buildSubmission("/repo", draft(patch)).ok, JSON.stringify(patch)).toBe(false);
    }
  });

  it("accepts comma or space separated dependencies and a parent", () => {
    const result = buildSubmission("/repo", draft({ dependencies: "a, b  c", parentId: " p " }));
    expect(result.ok && result.submission.dependencies).toEqual(["a", "b", "c"]);
    expect(result.ok && result.submission.parent_id).toBe("p");
  });
});

describe("router", () => {
  const routes: Route[] = [
    { name: "projects" },
    { name: "project", projectId: "/a b/repo" },
    { name: "tasks", projectId: "/repo", filter: "state:Queued" },
    { name: "task", projectId: "/repo", taskId: "task-1" },
    { name: "fixture-task", projectId: "/repo", id: "fixture:task-queued" },
    { name: "coordinator", projectId: "/repo" },
    { name: "review", projectId: "/repo", taskId: "task-1" },
    { name: "review", projectId: "/repo", taskId: null },
    { name: "review", projectId: "/repo", taskId: "fixture-task-1", query: "fixture=1&evidence=missing" },
  ];

  it.each(routes)("round-trips %j through the hash", (route) => {
    expect(parseRoute(formatRoute(route))).toEqual(route);
  });

  it("falls back to the workspace for an empty or unknown hash", () => {
    expect(parseRoute("#/nonsense")).toEqual({ name: "global", view: "workspace" });
    expect(parseRoute("")).toEqual({ name: "global", view: "workspace" });
  });

  it("falls back to the project list for a malformed percent-escape instead of throwing", () => {
    expect(parseRoute("#/project/%E0%A4%A")).toEqual({ name: "projects" });
    expect(parseRoute("#/project/%E0%A4%A/tasks/all")).toEqual({ name: "projects" });
  });

  it("navigation only changes local state", () => {
    const router = new RouterStore();
    let notified = 0;
    router.subscribe(() => notified++);
    router.navigate({ name: "project", projectId: "/repo" });
    expect(router.getSnapshot()).toEqual({ name: "project", projectId: "/repo" });
    expect(notified).toBe(1);
  });
});

describe("matchesFilter", () => {
  const dep = mkTask("dep", "/r", { state: "Running" });
  const done = mkTask("done", "/r", { state: "Succeeded" });
  const blocked = mkTask("blocked", "/r", { contract: { dependencies: ["dep"] } });
  const unblocked = mkTask("ok", "/r", { contract: { dependencies: ["done"] } });
  const unknownDep = mkTask("unk", "/r", { contract: { dependencies: ["missing"] } });
  const all = [dep, done, blocked, unblocked, unknownDep];
  const approvals = [mkApproval("a1", "ok"), mkApproval("a2", "done", "approved")];
  const ids = (filter: Parameters<typeof matchesFilter>[1]) =>
    all.filter((t) => matchesFilter(t, filter, all, approvals)).map((t) => t.contract.id);

  it("applies the documented local semantics", () => {
    expect(ids("queued")).toEqual(["blocked", "ok", "unk"]);
    expect(ids("running")).toEqual(["dep"]);
    expect(ids("terminal")).toEqual(["done"]);
    expect(ids("blocked")).toEqual(["blocked", "unk"]);
    expect(ids("review")).toEqual(["ok"]);
    expect(ids("state:Succeeded")).toEqual(["done"]);
    expect(ids("all")).toHaveLength(5);
  });
});
