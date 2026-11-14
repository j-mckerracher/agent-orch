/**
 * Theme preference: persisted choice, system fallback, and application to `<html>`.
 *
 * Contract:
 * - A preference is `"light"`, `"dark"` or `null` (no explicit choice, follow the system).
 * - Only an explicit choice is stored. Any other stored value is ignored.
 * - Storage may be missing or may throw (blocked profile, quota, privacy mode). Reads then
 *   return `null` and writes return `false`; the choice still applies for the current page.
 * - `applyTheme` sets or removes `data-theme`; tokens.css does the rest.
 */
import { useCallback, useEffect, useState } from "react";

export type ThemeName = "light" | "dark";
export type ThemePreference = ThemeName | null;

export const THEME_STORAGE_KEY = "agent-orch.theme";

/** The subset of `Storage` used here, so tests can pass a throwing fake. */
export interface ThemeStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/** `window.localStorage`, or `null` when even reading the property throws. */
export function defaultStorage(win: Pick<Window, "localStorage"> = window): ThemeStorage | null {
  try {
    return win.localStorage ?? null;
  } catch {
    return null;
  }
}

export function readPreference(storage: ThemeStorage | null = defaultStorage()): ThemePreference {
  if (!storage) return null;
  try {
    const value = storage.getItem(THEME_STORAGE_KEY);
    return value === "light" || value === "dark" ? value : null;
  } catch {
    return null;
  }
}

/** Returns whether the preference reached storage. `null` clears the stored choice. */
export function writePreference(
  preference: ThemePreference,
  storage: ThemeStorage | null = defaultStorage(),
): boolean {
  if (!storage) return false;
  try {
    if (preference === null) storage.removeItem(THEME_STORAGE_KEY);
    else storage.setItem(THEME_STORAGE_KEY, preference);
    return true;
  } catch {
    return false;
  }
}

export function systemPrefersDark(win: Pick<Window, "matchMedia"> = window): boolean {
  try {
    return win.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
  } catch {
    return false;
  }
}

export function resolveTheme(preference: ThemePreference, systemDark: boolean): ThemeName {
  return preference ?? (systemDark ? "dark" : "light");
}

export function applyTheme(preference: ThemePreference, root: HTMLElement = document.documentElement): void {
  if (preference === null) root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", preference);
}

/** Applies the stored preference. `public/theme-init.js` does the same earlier, from index.html, before first paint. */
export function initTheme(win: Window = window): ThemePreference {
  const preference = readPreference(defaultStorage(win));
  applyTheme(preference, win.document.documentElement);
  return preference;
}

export interface ThemeState {
  preference: ThemePreference;
  resolved: ThemeName;
  /** False after a failed write: the choice applies now but will not survive a reload. */
  persisted: boolean;
  setPreference(preference: ThemePreference): void;
  toggle(): void;
}

export function useTheme(): ThemeState {
  const [preference, setPreferenceState] = useState<ThemePreference>(() => readPreference());
  const [systemDark, setSystemDark] = useState<boolean>(() => systemPrefersDark());
  const [persisted, setPersisted] = useState(true);

  useEffect(() => {
    applyTheme(preference);
  }, [preference]);

  useEffect(() => {
    const query = typeof window.matchMedia === "function" ? window.matchMedia("(prefers-color-scheme: dark)") : null;
    if (!query) return;
    const onChange = (event: MediaQueryListEvent) => setSystemDark(event.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);

  const resolved = resolveTheme(preference, systemDark);

  const setPreference = useCallback((next: ThemePreference) => {
    setPersisted(writePreference(next));
    setPreferenceState(next);
  }, []);

  const toggle = useCallback(() => setPreference(resolved === "dark" ? "light" : "dark"), [resolved, setPreference]);

  return { preference, resolved, persisted, setPreference, toggle };
}
