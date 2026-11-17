import { describe, expect, it } from "vitest";
import type { RoutedIn } from "../../../sources/surfaces/types";
import { ROUTED_IN_MAX_ROWS, announcement, labelFor, rowsFrom } from "./routedInModel";

const row = (taskId: string, extra: Partial<RoutedIn> = {}): RoutedIn => ({
  taskId,
  title: "Fix the login bug",
  routedAt: "2026-11-16T21:04:11Z",
  state: "queued",
  ...extra,
});

describe("routedInModel", () => {
  it("routed_in_rows_hide_when_the_field_is_absent_or_empty", () => {
    expect(rowsFrom(null)).toEqual([]);
    expect(rowsFrom(undefined)).toEqual([]);
    expect(rowsFrom({})).toEqual([]);
    expect(rowsFrom({ routedIn: [] })).toEqual([]);
  });

  it("routed_in_rows_keep_the_daemon_order_and_stop_at_three", () => {
    const rows = ["t-1", "t-2", "t-3", "t-4"].map((id) => row(id));
    const shown = rowsFrom({ routedIn: rows });
    expect(shown.map((item) => item.taskId)).toEqual(["t-1", "t-2", "t-3"]);
    expect(shown).toHaveLength(ROUTED_IN_MAX_ROWS);
  });

  it("routed_in_label_names_the_task_and_its_state_in_text", () => {
    const label = labelFor(row("t-12ab"));
    expect(label).toContain("t-12ab (Fix the login bug), queued, ");
  });

  it("routed_in_announcement_is_silent_on_the_first_read", () => {
    expect(announcement(null, [row("t-1")])).toBeNull();
  });

  it("routed_in_announcement_speaks_a_newly_arrived_row_only", () => {
    const before = [row("t-1")];
    const after = [row("t-2", { title: "Add retries" }), row("t-1")];
    expect(announcement(before, after)).toBe(`Routed here: ${labelFor(row("t-2", { title: "Add retries" }))}`);
  });

  it("routed_in_announcement_is_silent_when_nothing_arrived", () => {
    const rows = [row("t-1")];
    expect(announcement(rows, rows)).toBeNull();
    expect(announcement(rows, [])).toBeNull();
  });
});
