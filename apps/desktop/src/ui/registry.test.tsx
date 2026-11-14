import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import App from "../App";
import { createServices } from "./createServices";
import { appGlobalViews, appShellSlots } from "./registry";
import type { GlobalView, Route } from "./router";

afterEach(() => {
  cleanup();
  window.localStorage.clear();
});

const VIEWS: GlobalView[] = ["workspace", "workstreams", "agents", "review"];
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
    expect(await screen.findByRole("heading", { level: 1 })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Device resources" })).toBeInTheDocument();
    expect(screen.queryByTestId("slot-top-usage")).not.toBeInTheDocument();
    expect(screen.queryByTestId("slot-resources")).not.toBeInTheDocument();
    expect(screen.queryByRole("complementary", { name: "Workspace activity" }) !== null).toBe(view === "workspace");
  });

  it("keeps the ambient background off by default and offers the opt-in control", async () => {
    const services = createServices({ initialRoute: route("workspace") });
    render(<App services={services} shellSlots={appShellSlots} globalViews={appGlobalViews} />);
    const slot = screen.getByTestId("slot-theme-toggle");
    const control = await within(slot).findByRole("button", { name: /^background/i });
    expect(control).toHaveAttribute("aria-pressed", "false");
    expect(document.querySelector("canvas")).toBeNull();
    await waitFor(() => expect(window.localStorage.length).toBe(0));
  });
});
