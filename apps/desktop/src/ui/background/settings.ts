/**
 * Persisted switch for the ambient background animation (decision D-9). On by default.
 *
 * Contract:
 * - Only the stored values `"on"` and `"off"` are a user choice and win over the default. A
 *   missing key, any other value or an unreadable store means the default (on).
 * - Storage may be missing or may throw (blocked profile, quota). The choice then applies
 *   for the current page only and `persisted` reports false.
 * - The `prefers-reduced-motion` media query is not part of this setting: the engine turns
 *   the animation into a static dot field when it matches.
 */
import { useSyncExternalStore } from "react";

export const AMBIENT_STORAGE_KEY = "agent-orch.ambient-background";

const DEFAULT_ENABLED = true;

/** The subset of `Storage` used here, so tests can pass a throwing fake. */
export interface AmbientStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export interface AmbientSnapshot {
  enabled: boolean;
  /** False after a failed write: the choice applies now but will not survive a reload. */
  persisted: boolean;
}

export interface AmbientSettings {
  getSnapshot(): AmbientSnapshot;
  subscribe(listener: () => void): () => void;
  setEnabled(enabled: boolean): void;
}

function defaultStorage(): AmbientStorage | null {
  try {
    return window.localStorage ?? null;
  } catch {
    return null;
  }
}

function read(storage: AmbientStorage | null): boolean {
  try {
    const stored = storage?.getItem(AMBIENT_STORAGE_KEY);
    if (stored === "on" || stored === "off") return stored === "on";
  } catch {
    // An unreadable store holds no choice, so the default applies.
  }
  return DEFAULT_ENABLED;
}

// Both values are written, so a chosen "off" survives while the default stays on.
function write(storage: AmbientStorage | null, enabled: boolean): boolean {
  if (!storage) return false;
  try {
    storage.setItem(AMBIENT_STORAGE_KEY, enabled ? "on" : "off");
    return true;
  } catch {
    return false;
  }
}

/** A settings store over `storage`; the toggle and the background layer share one instance. */
export function createAmbientSettings(storage: AmbientStorage | null = defaultStorage()): AmbientSettings {
  let snapshot: AmbientSnapshot = { enabled: read(storage), persisted: true };
  const listeners = new Set<() => void>();
  return {
    getSnapshot: () => snapshot,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    setEnabled(enabled) {
      snapshot = { enabled, persisted: write(storage, enabled) };
      listeners.forEach((listener) => listener());
    },
  };
}

let shared: AmbientSettings | undefined;

/** The application-wide instance, created on first use so importing this module stays side-effect free. */
export function ambientSettings(): AmbientSettings {
  shared ??= createAmbientSettings();
  return shared;
}

export function useAmbientSettings(settings: AmbientSettings = ambientSettings()): AmbientSnapshot & {
  setEnabled(enabled: boolean): void;
} {
  const snapshot = useSyncExternalStore(settings.subscribe, settings.getSnapshot, settings.getSnapshot);
  return { ...snapshot, setEnabled: settings.setEnabled };
}
