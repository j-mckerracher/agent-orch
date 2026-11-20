import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import { createServices } from "./createServices";
import { appGlobalViews, appShellSlots } from "./registry";
import type { GlobalView, Route } from "./router";

beforeEach(() => {
  // The ambient layer is on by default and jsdom has no 2D canvas; it draws nothing here.
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  window.localStorage.clear();
});

const VIEWS: GlobalView[] = ["workspace", "workstreams", "agents", "review", "settings"];
const route = (view: GlobalView): Route => ({ name: "global", view });

describe("M7 wiring", () => {
  it("registers a view for every global route and fills every shell slot", () => {
    expect(Object.keys(appGlobalViews).sort()).toEqual([...VIEWS].sort());
    for (const slot of ["activity", "resources", "topUsage", "topSearch", "topCounts", "themeToggle", "workstreamsTree", "workstreamsCount"] as const) {
      expect(appShellSlots[slot], slot).toBeTruthy();
    }
  });

  it.each(VIEWS)("renders %s instead of the placeholder, with the wired shell regions", async (view) => {
    const services = createServices({ initialRoute: route(view) });
    render(<App services={services} shellSlots={appShellSlots} globalViews={appGlobalViews} />);
    expect(screen.queryByTestId(`global-view-${view}`)).not.toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("heading", { level: 1 })).toBeInTheDocument());
    // System Usage lives inside the workspace activity popover, so only the workspace route renders it.
    expect(screen.queryByRole("region", { name: "Device resources" })).not.toBeInTheDocument();
    expect(document.querySelector('[data-testid="resource-strip"]') !== null).toBe(view === "workspace");
    expect(screen.queryByTestId("slot-top-usage")).not.toBeInTheDocument();
    expect(screen.queryByTestId("slot-resources")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Workspace activity/ }) !== null).toBe(view === "workspace");
  });

  it("turns the ambient background on by default, with the switch beside the theme toggle", async () => {
    const services = createServices({ initialRoute: route("workspace") });
    render(<App services={services} shellSlots={appShellSlots} globalViews={appGlobalViews} />);
    const slot = screen.getByTestId("slot-theme-toggle");
    const control = await within(slot).findByRole("button", { name: /^background/i });
    expect(control).toHaveAttribute("aria-pressed", "true");
    expect(document.querySelector('[data-testid="ambient-background"]')).not.toBeNull();
    await waitFor(() => expect(window.localStorage.length).toBe(0));
  });
});
