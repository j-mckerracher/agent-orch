/**
 * Persisted opt-in for the ambient background animation (decision D-9).
 *
 * Contract:
 * - Off unless the stored value is exactly `"on"`; any other value, a missing key or an
 *   unreadable store means off.
 * - Storage may be missing or may throw (blocked profile, quota). The choice then applies
 *   for the current page only and `persisted` reports false.
 * - The `prefers-reduced-motion` media query is not part of this setting: the engine turns
 *   the animation into a static dot field when it matches.
 */
import { useSyncExternalStore } from "react";

export const AMBIENT_STORAGE_KEY = "agent-orch.ambient-background";

/** The subset of `Storage` used here, so tests can pass a throwing fake. */
export interface AmbientStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
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
  if (!storage) return false;
  try {
    return storage.getItem(AMBIENT_STORAGE_KEY) === "on";
  } catch {
    return false;
  }
}

function write(storage: AmbientStorage | null, enabled: boolean): boolean {
  if (!storage) return false;
  try {
    if (enabled) storage.setItem(AMBIENT_STORAGE_KEY, "on");
    else storage.removeItem(AMBIENT_STORAGE_KEY);
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
