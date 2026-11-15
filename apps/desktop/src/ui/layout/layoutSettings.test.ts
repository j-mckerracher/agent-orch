import { describe, expect, it, vi } from "vitest";
import {
  LAYOUT_STORAGE_KEY,
  SIDEBAR_MAX,
  SIDEBAR_MIN,
  createLayoutSettings,
  type LayoutStorage,
} from "./layoutSettings";

function memoryStorage(initial: Record<string, string> = {}): LayoutStorage & { data: Record<string, string> } {
  const data = { ...initial };
  return { data, getItem: (key) => data[key] ?? null, setItem: (key, value) => void (data[key] = value) };
}

const throwingStorage: LayoutStorage = {
  getItem: () => {
    throw new DOMException("denied", "SecurityError");
  },
  setItem: () => {
    throw new DOMException("quota", "QuotaExceededError");
  },
};

describe("layout settings", () => {
  it("starts at the defaults with nothing stored", () => {
    expect(createLayoutSettings(memoryStorage()).getSnapshot()).toEqual({
      sidebarWidth: null,
      sidebarCollapsed: false,
      connectionStripExpanded: true,
      persisted: true,
    });
  });

  it("round-trips width and collapsed state through storage", () => {
    const storage = memoryStorage();
    const settings = createLayoutSettings(storage);
    settings.setSidebarWidth(300);
    expect(JSON.parse(storage.data[LAYOUT_STORAGE_KEY]!)).toEqual({
      sidebarWidth: 300,
      sidebarCollapsed: false,
      connectionStripExpanded: true,
    });
    settings.setSidebarCollapsed(true);
    expect(createLayoutSettings(storage).getSnapshot()).toMatchObject({ sidebarWidth: 300, sidebarCollapsed: true });
  });

  it("clamps a requested width to the bounds and expands when resizing", () => {
    const settings = createLayoutSettings(memoryStorage());
    settings.setSidebarCollapsed(true);
    settings.setSidebarWidth(10);
    expect(settings.getSnapshot()).toMatchObject({ sidebarWidth: SIDEBAR_MIN, sidebarCollapsed: false });
    settings.setSidebarWidth(9999);
    expect(settings.getSnapshot().sidebarWidth).toBe(SIDEBAR_MAX);
  });

  it("resets to the defaults", () => {
    const settings = createLayoutSettings(memoryStorage());
    settings.setSidebarWidth(300);
    settings.setSidebarCollapsed(true);
    settings.reset();
    expect(settings.getSnapshot()).toMatchObject({ sidebarWidth: null, sidebarCollapsed: false });
  });

  it.each([
    ["corrupt JSON", "{nope"],
    ["a non-object", "42"],
    ["null", "null"],
    ["a string width", JSON.stringify({ sidebarWidth: "300" })],
    ["a width below the minimum", JSON.stringify({ sidebarWidth: SIDEBAR_MIN - 1 })],
    ["a width above the maximum", JSON.stringify({ sidebarWidth: SIDEBAR_MAX + 1 })],
  ])("ignores %s in storage", (_label, raw) => {
    const settings = createLayoutSettings(memoryStorage({ [LAYOUT_STORAGE_KEY]: raw }));
    expect(settings.getSnapshot()).toMatchObject({ sidebarWidth: null, sidebarCollapsed: false });
  });

  it("keeps a valid collapsed flag when the stored width is invalid", () => {
    const raw = JSON.stringify({ sidebarWidth: 5, sidebarCollapsed: true });
    expect(createLayoutSettings(memoryStorage({ [LAYOUT_STORAGE_KEY]: raw })).getSnapshot()).toMatchObject({
      sidebarWidth: null,
      sidebarCollapsed: true,
    });
  });

  it("applies the layout for this page but reports not persisted when storage throws", () => {
    const settings = createLayoutSettings(throwingStorage);
    expect(settings.getSnapshot()).toMatchObject({ sidebarWidth: null, persisted: true });
    settings.setSidebarWidth(320);
    expect(settings.getSnapshot()).toEqual({
      sidebarWidth: 320,
      sidebarCollapsed: false,
      connectionStripExpanded: true,
      persisted: false,
    });
  });

  it("keeps the connection strip choice expanded by default and across sidebar changes", () => {
    const storage = memoryStorage();
    const settings = createLayoutSettings(storage);
    expect(settings.getSnapshot().connectionStripExpanded).toBe(true);
    settings.setConnectionStripExpanded(false);
    settings.setSidebarWidth(260);
    settings.setSidebarCollapsed(true);
    settings.setSidebarCollapsed(false);
    expect(createLayoutSettings(storage).getSnapshot()).toMatchObject({
      sidebarWidth: 260,
      connectionStripExpanded: false,
    });
    settings.reset();
    expect(settings.getSnapshot()).toMatchObject({ sidebarWidth: null, connectionStripExpanded: false });
  });

  it("reads a store written before the connection strip choice existed as expanded", () => {
    const raw = JSON.stringify({ sidebarWidth: 300, sidebarCollapsed: false });
    expect(createLayoutSettings(memoryStorage({ [LAYOUT_STORAGE_KEY]: raw })).getSnapshot()).toMatchObject({
      sidebarWidth: 300,
      connectionStripExpanded: true,
    });
  });

  it("applies a collapsed connection strip for this page only when storage throws", () => {
    const settings = createLayoutSettings(throwingStorage);
    settings.setConnectionStripExpanded(false);
    expect(settings.getSnapshot()).toMatchObject({ connectionStripExpanded: false, persisted: false });
  });

  it("works without any storage", () => {
    const settings = createLayoutSettings(null);
    settings.setSidebarWidth(320);
    expect(settings.getSnapshot()).toMatchObject({ sidebarWidth: 320, persisted: false });
  });

  it("notifies subscribers until they unsubscribe", () => {
    const settings = createLayoutSettings(memoryStorage());
    const listener = vi.fn();
    const unsubscribe = settings.subscribe(listener);
    settings.setSidebarWidth(200);
    expect(listener).toHaveBeenCalledTimes(1);
    unsubscribe();
    settings.setSidebarWidth(220);
    expect(listener).toHaveBeenCalledTimes(1);
  });
});
