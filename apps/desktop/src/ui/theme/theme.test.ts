import { afterEach, describe, expect, it, vi } from "vitest";
import {
  THEME_STORAGE_KEY,
  applyTheme,
  defaultStorage,
  initTheme,
  readPreference,
  resolveTheme,
  systemPrefersDark,
  writePreference,
  type ThemeStorage,
} from "./theme";

function memoryStorage(initial: Record<string, string> = {}): ThemeStorage & { data: Record<string, string> } {
  const data = { ...initial };
  return {
    data,
    getItem: (key) => data[key] ?? null,
    setItem: (key, value) => void (data[key] = value),
    removeItem: (key) => void delete data[key],
  };
}

const throwingStorage: ThemeStorage = {
  getItem: () => {
    throw new DOMException("denied", "SecurityError");
  },
  setItem: () => {
    throw new DOMException("quota", "QuotaExceededError");
  },
  removeItem: () => {
    throw new DOMException("denied", "SecurityError");
  },
};

afterEach(() => {
  document.documentElement.removeAttribute("data-theme");
  vi.restoreAllMocks();
});

describe("theme preference storage", () => {
  it("round-trips an explicit choice", () => {
    const storage = memoryStorage();
    expect(readPreference(storage)).toBeNull();
    expect(writePreference("dark", storage)).toBe(true);
    expect(storage.data[THEME_STORAGE_KEY]).toBe("dark");
    expect(readPreference(storage)).toBe("dark");
    expect(writePreference("light", storage)).toBe(true);
    expect(readPreference(storage)).toBe("light");
  });

  it("clears the stored choice for a null preference", () => {
    const storage = memoryStorage({ [THEME_STORAGE_KEY]: "dark" });
    expect(writePreference(null, storage)).toBe(true);
    expect(readPreference(storage)).toBeNull();
  });

  it("ignores stored values that are not a theme", () => {
    for (const value of ["", "auto", "DARK", "{}", "1"]) {
      expect(readPreference(memoryStorage({ [THEME_STORAGE_KEY]: value }))).toBeNull();
    }
  });

  it("falls back safely when storage throws", () => {
    expect(readPreference(throwingStorage)).toBeNull();
    expect(writePreference("dark", throwingStorage)).toBe(false);
    expect(writePreference(null, throwingStorage)).toBe(false);
  });

  it("falls back safely when storage is unavailable", () => {
    expect(readPreference(null)).toBeNull();
    expect(writePreference("dark", null)).toBe(false);
  });

  it("returns null when reading window.localStorage itself throws", () => {
    const win = {
      get localStorage(): Storage {
        throw new DOMException("blocked", "SecurityError");
      },
    };
    expect(defaultStorage(win)).toBeNull();
  });
});

describe("theme resolution and application", () => {
  it("honours the system setting until an explicit choice exists", () => {
    expect(resolveTheme(null, true)).toBe("dark");
    expect(resolveTheme(null, false)).toBe("light");
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });

  it("sets data-theme for an explicit choice and removes it for none", () => {
    applyTheme("dark");
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    applyTheme("light");
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
    applyTheme(null);
    expect(document.documentElement.hasAttribute("data-theme")).toBe(false);
  });

  it("reads the system preference defensively", () => {
    expect(systemPrefersDark({ matchMedia: undefined as unknown as Window["matchMedia"] })).toBe(false);
    expect(
      systemPrefersDark({ matchMedia: (() => ({ matches: true })) as unknown as Window["matchMedia"] }),
    ).toBe(true);
    expect(
      systemPrefersDark({
        matchMedia: (() => {
          throw new Error("nope");
        }) as unknown as Window["matchMedia"],
      }),
    ).toBe(false);
  });

  it("initTheme applies the stored choice before the app renders", () => {
    window.localStorage.setItem(THEME_STORAGE_KEY, "dark");
    try {
      expect(initTheme(window)).toBe("dark");
      expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    } finally {
      window.localStorage.removeItem(THEME_STORAGE_KEY);
    }
  });

  it("initTheme leaves data-theme unset when nothing is stored or storage throws", () => {
    expect(initTheme(window)).toBeNull();
    expect(document.documentElement.hasAttribute("data-theme")).toBe(false);

    const blocked = {
      document,
      get localStorage(): Storage {
        throw new DOMException("blocked", "SecurityError");
      },
    } as unknown as Window;
    expect(initTheme(blocked)).toBeNull();
    expect(document.documentElement.hasAttribute("data-theme")).toBe(false);
  });
});
