import { describe, expect, it } from "vitest";
import { DaemonError } from "../../../daemon/errors";
import { formatHoldDay, heldUntilText, holdErrorText, holdRange } from "./holdDates";
import { filterByTab, tabsFor } from "./queue";
import { approval } from "../../../review/testing";
import { NO_HELD } from "../../../sources/surfaces/holds";

describe("hold dates", () => {
  it("hold_dates_read_the_day_from_its_text_without_a_time_zone_shift", () => {
    expect(formatHoldDay("2026-11-21", true)).toBe("21 November 2026");
    expect(formatHoldDay("2026-01-09", false)).toBe("9 January");
    expect(heldUntilText("2026-11-21")).toBe("Held until 21 November");
    expect(formatHoldDay("not a day", true)).toBe("not a day");
    expect(formatHoldDay("2026-13-01", true)).toBe("2026-13-01");
  });

  it("hold_dates_range_runs_from_today_to_sixty_days_on_the_local_date", () => {
    expect(holdRange(new Date(2026, 10, 17, 23, 30))).toEqual({ min: "2026-11-17", max: "2027-01-16" });
    expect(holdRange(new Date(2026, 0, 1, 0, 5))).toEqual({ min: "2026-01-01", max: "2026-03-02" });
  });

  it("hold_dates_error_words_say_nothing_was_changed_and_name_the_range", () => {
    expect(holdErrorText(new DaemonError("invalid-params", "x"), "hold")).toMatch(/60 days from today\. Nothing was changed\.$/);
    expect(holdErrorText(new DaemonError("not-found", "x"), "hold")).toMatch(/Nothing was changed\.$/);
    expect(holdErrorText(new DaemonError("not-found", "x"), "release")).toBe("That hold has already ended. Nothing was changed.");
    expect(holdErrorText(new DaemonError("conflict", "x"), "release")).toMatch(/Nothing was changed\.$/);
  });
});

describe("held tab selection", () => {
  it("held_tab_is_listed_only_when_the_daemon_supports_holds", () => {
    expect(tabsFor(true).map((tab) => tab.id)).toContain("held");
    expect(tabsFor(false).map((tab) => tab.id)).not.toContain("held");
    expect(tabsFor(true).find((tab) => tab.id === "held")).toMatchObject({
      label: "Held",
      description: "Decisions you postponed. Each returns on its date.",
    });
  });

  it("held_tab_moves_a_held_tasks_pending_approvals_out_of_actionable_and_leaves_resolved_ones_alone", () => {
    const pending = approval({ id: "a1", taskId: "t1" });
    const other = approval({ id: "a2", taskId: "t2" });
    const done = approval({ id: "a3", taskId: "t1", state: { state: "rejected" } });
    const held = { tasks: new Set(["t1"]), proposals: new Set<string>() };
    expect(filterByTab("actionable", [pending, other, done], held).map((a) => a.id)).toEqual(["a2"]);
    expect(filterByTab("held", [pending, other, done], held).map((a) => a.id)).toEqual(["a1"]);
    expect(filterByTab("actionable", [pending, other], NO_HELD).map((a) => a.id)).toEqual(["a1", "a2"]);
    expect(filterByTab("approvals", [pending, other, done], held)).toHaveLength(3);
  });
});
