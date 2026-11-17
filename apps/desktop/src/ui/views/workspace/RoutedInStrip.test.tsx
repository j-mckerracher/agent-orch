import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { RoutedIn } from "../../../sources/surfaces/types";
import { RoutedInStrip } from "./RoutedInStrip";
import { ROUTED_IN_HEADING, ROUTED_IN_HELP, labelFor } from "./routedInModel";

afterEach(cleanup);

const row = (taskId: string, extra: Partial<RoutedIn> = {}): RoutedIn => ({
  taskId,
  title: "Fix the login bug",
  routedAt: "2026-11-16T21:04:11Z",
  state: "queued",
  ...extra,
});

describe("RoutedInStrip", () => {
  it("routed_in_strip_is_hidden_when_the_field_is_absent", () => {
    render(<RoutedInStrip snapshot={{}} workstream />);
    expect(screen.queryByRole("region", { name: ROUTED_IN_HEADING })).toBeNull();
  });

  it("routed_in_strip_is_hidden_when_the_list_is_empty", () => {
    render(<RoutedInStrip snapshot={{ routedIn: [] }} workstream />);
    expect(screen.queryByRole("region", { name: ROUTED_IN_HEADING })).toBeNull();
  });

  it("routed_in_strip_is_never_shown_for_the_global_conversation", () => {
    render(<RoutedInStrip snapshot={{ routedIn: [row("t-1")] }} workstream={false} />);
    expect(screen.queryByRole("region", { name: ROUTED_IN_HEADING })).toBeNull();
  });

  it("routed_in_strip_lists_rows_with_the_help_line", () => {
    render(<RoutedInStrip snapshot={{ routedIn: [row("t-12ab")] }} workstream />);
    const region = screen.getByRole("region", { name: ROUTED_IN_HEADING });
    expect(region.textContent).toContain(labelFor(row("t-12ab")));
    expect(region.textContent).toContain(ROUTED_IN_HELP);
  });

  it("routed_in_strip_speaks_a_row_that_arrives_later", () => {
    const { container, rerender } = render(<RoutedInStrip snapshot={{ routedIn: [row("t-1")] }} workstream />);
    const live = container.querySelector('[aria-live="polite"]');
    expect(live?.textContent).toBe("");

    rerender(<RoutedInStrip snapshot={{ routedIn: [row("t-2"), row("t-1")] }} workstream />);
    expect(live?.textContent).toBe(`Routed here: ${labelFor(row("t-2"))}`);
  });
});
