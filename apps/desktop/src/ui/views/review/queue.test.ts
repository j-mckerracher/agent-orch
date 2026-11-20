import { describe, expect, it } from "vitest";
import { approval } from "../../../review/testing";
import { heldSets } from "../../../sources/surfaces/holds";
import type { HoldItem, PullItem, QuestionItem } from "../../../sources/surfaces/types";
import { TABS, buildQueue, filterQueue, projectsOf, type QueueInput } from "./queue";

const question = (id: string, over: Partial<QuestionItem> = {}): QuestionItem => ({
  id,
  taskId: `t-${id}`,
  taskTitle: "",
  question: `Question ${id}?`,
  options: [],
  deferredUntil: null,
  state: "open",
  ...over,
});

const pull = (taskId: string, state = "open"): PullItem =>
  ({ taskId, number: 7, state, headSha: "a".repeat(40), baseBranch: "main", branch: "b", mergeable: "yes", requiredTotal: 0, requiredPassed: 0, requiredFailed: 0, checks: [], title: "T", observedAgeSecs: 1, merge: null }) as PullItem;

const hold = (subjectKind: string, subjectId: string): HoldItem => ({ id: `h-${subjectId}`, subjectKind, subjectId, untilDate: "2026-11-21", state: "active" });

function input(over: Partial<QueueInput> = {}): QueueInput {
  const holds = over.holds ?? [];
  return {
    proposals: [],
    questions: [],
    candidates: [],
    resolved: [],
    other: [],
    pulls: [],
    holds,
    held: heldSets(holds),
    projectOfTask: (taskId) => (taskId === "t1" ? "/p/one" : taskId === "t2" ? "/p/two" : null),
    ...over,
  };
}

describe("review queue model", () => {
  it("has_exactly_the_three_tabs_needs_you_held_resolved", () => {
    expect(TABS.map((tab) => tab.label)).toEqual(["Needs you", "Held", "Resolved"]);
  });

  it("puts_every_type_in_one_list_newest_first_within_a_type", () => {
    const items = buildQueue(
      input({
        questions: [question("old"), question("new")],
        candidates: [approval({ id: "a-old", taskId: "t1" }), approval({ id: "a-new", taskId: "t2" })],
        pulls: [pull("t1")],
      }),
    );
    expect(items.map((item) => `${item.type}:${item.key}`)).toEqual([
      "question:question:new",
      "question:question:old",
      "candidate:approval:a-new",
      "candidate:approval:a-old",
      "pr:pr:t1",
    ]);
    expect(items.every((item) => item.tab === "needs")).toBe(true);
  });

  it("moves_a_deferred_question_and_a_held_candidate_to_held_and_a_decided_one_to_resolved", () => {
    const items = buildQueue(
      input({
        questions: [question("q", { deferredUntil: "2026-11-21" })],
        candidates: [approval({ id: "a1", taskId: "t1" }), approval({ id: "a2", taskId: "t2" })],
        resolved: [approval({ id: "a3", taskId: "t1", state: { state: "rejected" } })],
        pulls: [pull("t2", "merged")],
        holds: [hold("approval", "t1")],
      }),
    );
    const tabOf = (key: string) => items.find((item) => item.key === key)?.tab;
    expect(tabOf("question:q")).toBe("held");
    expect(tabOf("approval:a1")).toBe("held");
    expect(tabOf("approval:a2")).toBe("needs");
    expect(tabOf("approval:a3")).toBe("resolved");
    expect(tabOf("pr:t2")).toBe("resolved");
  });

  it("filters_by_tab_project_and_type_and_lists_only_projects_that_have_an_item", () => {
    const items = buildQueue(
      input({ questions: [question("1", { taskId: "t1" })], candidates: [approval({ id: "a", taskId: "t2" })], pulls: [pull("t9")] }),
    );
    expect(projectsOf(items)).toEqual(["/p/one", "/p/two"]);
    expect(filterQueue(items, { tab: "needs", project: null, type: null })).toHaveLength(3);
    expect(filterQueue(items, { tab: "needs", project: "/p/one", type: null }).map((item) => item.type)).toEqual(["question"]);
    expect(filterQueue(items, { tab: "needs", project: null, type: "candidate" }).map((item) => item.key)).toEqual(["approval:a"]);
    expect(filterQueue(items, { tab: "held", project: null, type: null })).toEqual([]);
  });
});
