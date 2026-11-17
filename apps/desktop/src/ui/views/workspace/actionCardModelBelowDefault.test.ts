import { describe, expect, it } from "vitest";
import type { ActionProposal } from "../../../sources/surfaces/types";
import { fieldsOf } from "./actionCardModel";

const PARAMS = {
  project: "p-3fa9c1d2",
  objective: "Add unit tests for the retry module.",
  agent: "codex",
  task_kind: "Implement",
  delivery: "LocalReview",
};

function propose(target: ActionProposal["target"], delivery = "LocalReview"): ActionProposal {
  return {
    id: "proposal-bd",
    turnId: "turn-1",
    index: 0,
    kind: "task.propose",
    params: { ...PARAMS, delivery },
    digest: "digest-bd",
    dependsOn: null,
    state: "pending",
    resultRef: null,
    errorCode: null,
    denial: null,
    target,
    updatedAt: "2026-11-10T20:00:00.000Z",
  };
}

const below = (card: ActionProposal) => fieldsOf(card, new Map(), null).find((field) => field.label === "Below this project's default");

describe("below the project default card line", () => {
  it("card_below_default_row_states_the_default_and_the_task_in_words", () => {
    const card = propose({ projectName: "alpha", belowProjectDefault: true, projectMode: "direct-pr" });
    expect(below(card)).toEqual({ label: "Below this project's default", value: "Project default: pull request. This task: local review." });
    expect(below(propose({ belowProjectDefault: true, projectMode: "no-mistakes" }, "ReportOnly"))?.value).toBe(
      "Project default: pull request. This task: report only.",
    );
  });

  it("card_below_default_row_is_absent_without_the_flag_and_names_an_unknown_mode_as_sent", () => {
    expect(below(propose({ projectName: "alpha" }))).toBeUndefined();
    expect(below(propose({ projectName: "alpha", belowProjectDefault: false, projectMode: "direct-pr" }))).toBeUndefined();
    expect(below(propose(null))).toBeUndefined();
    expect(below(propose({ belowProjectDefault: true, projectMode: "future-mode" }))?.value).toBe(
      "Project default: future-mode. This task: local review.",
    );
  });
});
