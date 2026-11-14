/**
 * Persisted shell layout: the sidebar width the user dragged to and whether it is collapsed (DPES-1102).
 *
 * Contract:
 * - Stored as one versioned JSON value. A missing key, unreadable store, corrupt JSON or a width outside
 *   `SIDEBAR_MIN`-`SIDEBAR_MAX` falls back to the defaults (`sidebarWidth: null` means "use the CSS default").
 * - Storage may be missing or may throw (blocked profile, quota). The layout then applies for the current
 *   page only and `persisted` reports false.
 * - The saved width is kept raw. `shell.css` clamps it against the live viewport, so shrinking the window
 *   never rewrites the preference.
 */
import { useSyncExternalStore } from "react";

export const LAYOUT_STORAGE_KEY = "agent-orch.layout.v1";

export const SIDEBAR_MIN = 160;
export const SIDEBAR_MAX = 480;

/** The subset of `Storage` used here, so tests can pass a throwing fake. */
export interface LayoutStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export interface LayoutSnapshot {
  /** Pixels, or null until the user resizes. */
  sidebarWidth: number | null;
  sidebarCollapsed: boolean;
  /** False after a failed write: the layout applies now but will not survive a reload. */
  persisted: boolean;
}

export interface LayoutSettings {
  getSnapshot(): LayoutSnapshot;
  subscribe(listener: () => void): () => void;
  setSidebarWidth(width: number): void;
  setSidebarCollapsed(collapsed: boolean): void;
  /** Back to the CSS default width, expanded. */
  reset(): void;
}

export function clampSidebarWidth(width: number): number {
  return Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, Math.round(width)));
}

function defaultStorage(): LayoutStorage | null {
  try {
    return window.localStorage ?? null;
  } catch {
    return null;
  }
}

function read(storage: LayoutStorage | null): Pick<LayoutSnapshot, "sidebarWidth" | "sidebarCollapsed"> {
  const fallback = { sidebarWidth: null, sidebarCollapsed: false };
  if (!storage) return fallback;
  try {
    const raw = storage.getItem(LAYOUT_STORAGE_KEY);
    if (raw === null) return fallback;
    const value: unknown = JSON.parse(raw);
    if (typeof value !== "object" || value === null) return fallback;
    const { sidebarWidth, sidebarCollapsed } = value as Record<string, unknown>;
    return {
      sidebarWidth:
        typeof sidebarWidth === "number" && sidebarWidth >= SIDEBAR_MIN && sidebarWidth <= SIDEBAR_MAX ? sidebarWidth : null,
      sidebarCollapsed: sidebarCollapsed === true,
    };
  } catch {
    return fallback;
  }
}

function write(storage: LayoutStorage | null, value: Pick<LayoutSnapshot, "sidebarWidth" | "sidebarCollapsed">): boolean {
  if (!storage) return false;
  try {
    storage.setItem(LAYOUT_STORAGE_KEY, JSON.stringify(value));
    return true;
  } catch {
    return false;
  }
}

/** A layout store over `storage`; the shell and its splitter share one instance. */
export function createLayoutSettings(storage: LayoutStorage | null = defaultStorage()): LayoutSettings {
  let snapshot: LayoutSnapshot = { ...read(storage), persisted: true };
  const listeners = new Set<() => void>();
  const update = (next: Pick<LayoutSnapshot, "sidebarWidth" | "sidebarCollapsed">) => {
    snapshot = { ...next, persisted: write(storage, next) };
    listeners.forEach((listener) => listener());
  };
  return {
    getSnapshot: () => snapshot,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    setSidebarWidth: (width) => update({ sidebarWidth: clampSidebarWidth(width), sidebarCollapsed: false }),
    setSidebarCollapsed: (collapsed) => update({ sidebarWidth: snapshot.sidebarWidth, sidebarCollapsed: collapsed }),
    reset: () => update({ sidebarWidth: null, sidebarCollapsed: false }),
  };
}

let shared: LayoutSettings | undefined;

/** The application-wide instance, created on first use so importing this module stays side-effect free. */
export function layoutSettings(): LayoutSettings {
  shared ??= createLayoutSettings();
  return shared;
}

export function useLayoutSettings(settings: LayoutSettings = layoutSettings()): LayoutSnapshot & Omit<LayoutSettings, "getSnapshot" | "subscribe"> {
  const snapshot = useSyncExternalStore(settings.subscribe, settings.getSnapshot, settings.getSnapshot);
  return { ...snapshot, setSidebarWidth: settings.setSidebarWidth, setSidebarCollapsed: settings.setSidebarCollapsed, reset: settings.reset };
}
