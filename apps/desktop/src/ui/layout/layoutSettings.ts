/**
 * Persisted shell layout: the sidebar width the user dragged to, whether it is collapsed (DPES-1102), and
 * whether the connection strip shows its explanatory notes (DPES-1105).
 *
 * Contract:
 * - Stored as one versioned JSON value. A missing key, unreadable store, corrupt JSON or a width outside
 *   `SIDEBAR_MIN`-`SIDEBAR_MAX` falls back to the defaults (`sidebarWidth: null` means "use the CSS default").
 *   A missing `connectionStripExpanded` reads as expanded, so values stored before DPES-1105 keep working.
 * - Storage may be missing or may throw (blocked profile, quota). The layout then applies for the current
 *   page only and `persisted` reports false.
 * - The saved width is kept raw. `shell.css` clamps it against the live viewport, so shrinking the window
 *   never rewrites the preference.
 * - `connectionStripExpanded` is the user's choice only. The connection strip overrides it while the
 *   connection needs attention; that override is never written here.
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

/** The part of the layout that is saved. */
interface StoredLayout {
  /** Pixels, or null until the user resizes. */
  sidebarWidth: number | null;
  sidebarCollapsed: boolean;
  /** False when the user folded the connection strip's explanatory notes. */
  connectionStripExpanded: boolean;
}

export interface LayoutSnapshot extends StoredLayout {
  /** False after a failed write: the layout applies now but will not survive a reload. */
  persisted: boolean;
}

export interface LayoutSettings {
  getSnapshot(): LayoutSnapshot;
  subscribe(listener: () => void): () => void;
  setSidebarWidth(width: number): void;
  setSidebarCollapsed(collapsed: boolean): void;
  setConnectionStripExpanded(expanded: boolean): void;
  /** Back to the CSS default width, expanded. The connection strip choice is kept. */
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

function read(storage: LayoutStorage | null): StoredLayout {
  const fallback: StoredLayout = { sidebarWidth: null, sidebarCollapsed: false, connectionStripExpanded: true };
  if (!storage) return fallback;
  try {
    const raw = storage.getItem(LAYOUT_STORAGE_KEY);
    if (raw === null) return fallback;
    const value: unknown = JSON.parse(raw);
    if (typeof value !== "object" || value === null) return fallback;
    const { sidebarWidth, sidebarCollapsed, connectionStripExpanded } = value as Record<string, unknown>;
    return {
      sidebarWidth:
        typeof sidebarWidth === "number" && sidebarWidth >= SIDEBAR_MIN && sidebarWidth <= SIDEBAR_MAX ? sidebarWidth : null,
      sidebarCollapsed: sidebarCollapsed === true,
      connectionStripExpanded: connectionStripExpanded !== false,
    };
  } catch {
    return fallback;
  }
}

function write(storage: LayoutStorage | null, value: StoredLayout): boolean {
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
  const current = (): StoredLayout => ({
    sidebarWidth: snapshot.sidebarWidth,
    sidebarCollapsed: snapshot.sidebarCollapsed,
    connectionStripExpanded: snapshot.connectionStripExpanded,
  });
  const update = (next: StoredLayout) => {
    snapshot = { ...next, persisted: write(storage, next) };
    listeners.forEach((listener) => listener());
  };
  return {
    getSnapshot: () => snapshot,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    setSidebarWidth: (width) => update({ ...current(), sidebarWidth: clampSidebarWidth(width), sidebarCollapsed: false }),
    setSidebarCollapsed: (collapsed) => update({ ...current(), sidebarCollapsed: collapsed }),
    setConnectionStripExpanded: (expanded) => update({ ...current(), connectionStripExpanded: expanded }),
    reset: () => update({ ...current(), sidebarWidth: null, sidebarCollapsed: false }),
  };
}

let shared: LayoutSettings | undefined;

/** The application-wide instance, created on first use so importing this module stays side-effect free. */
export function layoutSettings(): LayoutSettings {
  shared ??= createLayoutSettings();
  return shared;
}

export function useLayoutSettings(
  settings: LayoutSettings = layoutSettings(),
): LayoutSnapshot & Omit<LayoutSettings, "getSnapshot" | "subscribe"> {
  const snapshot = useSyncExternalStore(settings.subscribe, settings.getSnapshot, settings.getSnapshot);
  return {
    ...snapshot,
    setSidebarWidth: settings.setSidebarWidth,
    setSidebarCollapsed: settings.setSidebarCollapsed,
    setConnectionStripExpanded: settings.setConnectionStripExpanded,
    reset: settings.reset,
  };
}
