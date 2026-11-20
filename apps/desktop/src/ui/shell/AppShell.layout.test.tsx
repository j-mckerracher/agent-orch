import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { createLayoutSettings, LAYOUT_STORAGE_KEY, type LayoutStorage } from "../layout/layoutSettings";
import { createServices } from "../createServices";
import { DEFAULT_ROUTE } from "../router";
import { ServicesProvider } from "../services";
import { AppShell } from "./AppShell";
import type { ShellSlots } from "./slots";

const slots: ShellSlots = {
  activity: null,
  agentsCount: null,
  reviewCount: null,
  topUsage: null,
  topSearch: null,
  topNeedYou: null,
};

function memoryStorage(initial: Record<string, string> = {}): LayoutStorage & { data: Record<string, string> } {
  const data = { ...initial };
  return { data, getItem: (key) => data[key] ?? null, setItem: (key, value) => void (data[key] = value) };
}

/** Same `matchMedia` stub as `AppShell.test.tsx`: parses `max-width` only. */
const realMatchMedia = window.matchMedia;
function setViewport(width: number): void {
  window.matchMedia = ((query: string) => {
    const max = /max-width:\s*(\d+)px/.exec(query);
    return {
      matches: max ? width <= Number(max[1]) : false,
      media: query,
      addEventListener: () => {},
      removeEventListener: () => {},
    } as unknown as MediaQueryList;
  }) as typeof window.matchMedia;
}

function mount(storage: LayoutStorage) {
  // No connector: the nav only needs the router and counts from a fixture-mode services bundle.
  return render(
    <ServicesProvider services={createServices({})}>
      <AppShell route={DEFAULT_ROUTE} connection={null} slots={slots} mainLandmark layout={createLayoutSettings(storage)}>
        content
      </AppShell>
    </ServicesProvider>,
  );
}

afterEach(() => {
  window.matchMedia = realMatchMedia;
});

describe("AppShell sidebar layout", () => {
  it("applies and persists a keyboard resize, then restores it on remount", async () => {
    setViewport(1280);
    const storage = memoryStorage();
    const first = mount(storage);
    const body = document.querySelector<HTMLElement>(".shell-body")!;
    expect(body.style.getPropertyValue("--shell-sidebar-width")).toBe("");
    screen.getByRole("separator", { name: "Resize sidebar" }).focus();
    await userEvent.setup().keyboard("{End}");
    expect(body.style.getPropertyValue("--shell-sidebar-width")).toBe("480px");
    expect(JSON.parse(storage.data[LAYOUT_STORAGE_KEY]!)).toMatchObject({ sidebarWidth: 480 });
    first.unmount();

    mount(storage);
    expect(document.querySelector<HTMLElement>(".shell-body")!.style.getPropertyValue("--shell-sidebar-width")).toBe("480px");
  });

  it("collapses and expands with the toggle, keeping the controls reachable", async () => {
    setViewport(1280);
    const storage = memoryStorage();
    mount(storage);
    const user = userEvent.setup();
    const body = document.querySelector<HTMLElement>(".shell-body")!;
    expect(body).not.toHaveAttribute("data-sidebar-collapsed");
    await user.click(screen.getByRole("button", { name: "Collapse sidebar" }));
    expect(body).toHaveAttribute("data-sidebar-collapsed");
    expect(screen.getByRole("separator", { name: "Resize sidebar" })).toHaveAttribute("aria-valuenow", "0");
    await user.click(screen.getByRole("button", { name: "Expand sidebar" }));
    expect(body).not.toHaveAttribute("data-sidebar-collapsed");
  });

  it("restores a collapsed sidebar from storage", () => {
    setViewport(1280);
    mount(memoryStorage({ [LAYOUT_STORAGE_KEY]: JSON.stringify({ sidebarWidth: 300, sidebarCollapsed: true }) }));
    expect(document.querySelector(".shell-body")).toHaveAttribute("data-sidebar-collapsed");
  });

  it.each([1280, 1150, 960, 761])("offers the splitter at %i px", (width) => {
    setViewport(width);
    mount(memoryStorage());
    expect(screen.getByRole("separator", { name: "Resize sidebar" })).toBeInTheDocument();
  });

  it.each([760, 390])("has no splitter at %i px, where the sidebar is a full-width block", (width) => {
    setViewport(width);
    mount(memoryStorage({ [LAYOUT_STORAGE_KEY]: JSON.stringify({ sidebarWidth: 300, sidebarCollapsed: true }) }));
    expect(screen.queryByRole("separator")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /sidebar/i })).not.toBeInTheDocument();
    // A saved collapsed state must not hide the navigation when the splitter cannot bring it back.
    expect(document.querySelector(".shell-body")).not.toHaveAttribute("data-sidebar-collapsed");
  });
});
