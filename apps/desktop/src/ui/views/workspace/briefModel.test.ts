import { describe, expect, it } from "vitest";
import type { Brief, BriefCounts, BriefItem } from "../../../sources/surfaces/types";
import { briefHeading, briefLines, briefWindowText, itemText, plural } from "./briefModel";

const ZERO: BriefCounts = {
  tasksSucceeded: 0,
  tasksFailed: 0,
  tasksStalled: 0,
  tasksRunning: 0,
  proposalsApprovedByUser: 0,
  proposalsApprovedByGrant: 0,
  executedByGrant: 0,
  failedUnderGrant: 0,
  proposalsWaiting: 0,
  deliveriesWaiting: 0,
  decisionsHeld: 0,
  grantUses: 0,
  grantsExpired: 0,
  grantsRevoked: 0,
  quietUpdates: 0,
  repliesPausedByLimit: 0,
};

const brief = (counts: Partial<BriefCounts> = {}, open = false): Brief => ({
  fromMs: Date.parse("2026-11-16T10:00:00Z"),
  toMs: Date.parse("2026-11-16T14:00:00Z"),
  open,
  counts: { ...ZERO, ...counts },
  grants: [],
  waiting: [],
  attention: [],
});

describe("brief model", () => {
  it("brief_model_lines_come_from_counts_with_plural_text", () => {
    const lines = briefLines(
      brief({
        tasksSucceeded: 3,
        tasksFailed: 1,
        tasksRunning: 2,
        proposalsApprovedByGrant: 4,
        executedByGrant: 4,
        proposalsWaiting: 2,
        deliveriesWaiting: 1,
        quietUpdates: 5,
      }),
    );
    expect(lines).toEqual([
      "3 tasks finished, 1 failed, 2 still running",
      "4 actions approved by standing approvals, 4 completed",
      "2 decisions are waiting for you",
      "1 delivery is waiting for approval",
      "5 automatic updates were quiet",
    ]);
  });

  it("brief_model_uses_the_singular_for_one", () => {
    expect(briefLines(brief({ tasksSucceeded: 1, proposalsWaiting: 1, quietUpdates: 1, repliesPausedByLimit: 1 }))).toEqual([
      "1 task finished",
      "1 decision is waiting for you",
      "1 automatic update was quiet",
      "1 automatic reply was paused by a limit",
    ]);
    expect(plural(0, "task")).toBe("0 tasks");
    expect(plural(2, "delivery", "deliveries")).toBe("2 deliveries");
  });

  it("brief_model_an_empty_brief_says_nothing_happened", () => {
    expect(briefLines(brief())).toEqual(["Nothing happened while you were away."]);
    expect(briefLines(brief({}, true))).toEqual(["Nothing has happened so far."]);
  });

  it("brief_model_open_window_says_so_far_and_a_closed_one_names_both_ends", () => {
    expect(briefHeading(brief({}, true))).toBe("While you were away, so far");
    expect(briefHeading(brief())).toBe("While you were away");
    expect(briefWindowText(brief({}, true))).toMatch(/, so far$/);
    expect(briefWindowText(brief())).toMatch(/^From .+ to .+/);
    expect(briefWindowText(brief())).not.toMatch(/so far/);
  });

  it("brief_model_item_text_shows_the_kind_and_title_only", () => {
    const item: BriefItem = { kind: "task_failed", id: "t-1234abcd", title: "Fix login", state: "Failed", project: "p-00000000", atMs: 1 };
    expect(itemText(item)).toBe("Task failed: Fix login");
    expect(itemText({ ...item, kind: "something_new" })).toBe("Item: Fix login");
  });
});
