import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ThemeToggle } from "./ThemeToggle";
import { THEME_STORAGE_KEY } from "./theme";

type Listener = (event: MediaQueryListEvent) => void;

function mockSystemTheme(dark: boolean) {
  const listeners = new Set<Listener>();
  const query = {
    matches: dark,
    addEventListener: (_: string, fn: Listener) => listeners.add(fn),
    removeEventListener: (_: string, fn: Listener) => listeners.delete(fn),
  };
  window.matchMedia = vi.fn().mockReturnValue(query) as unknown as typeof window.matchMedia;
  return {
    change(next: boolean) {
      query.matches = next;
      act(() => listeners.forEach((fn) => fn({ matches: next } as MediaQueryListEvent)));
    },
  };
}

const originalMatchMedia = window.matchMedia;

beforeEach(() => {
  window.localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
});

afterEach(() => {
  window.matchMedia = originalMatchMedia;
  vi.restoreAllMocks();
  window.localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
});

describe("ThemeToggle", () => {
  it("starts light, switches to dark, persists, and survives a reload", async () => {
    mockSystemTheme(false);
    const user = userEvent.setup();
    const first = render(<ThemeToggle />);
    const button = screen.getByRole("button", { name: /dark mode/i });
    expect(button).toHaveAttribute("aria-pressed", "false");

    await user.click(button);
    expect(button).toHaveAttribute("aria-pressed", "true");
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe("dark");

    first.unmount();
    document.documentElement.removeAttribute("data-theme"); // a reload starts with a fresh document
    render(<ThemeToggle />);
    expect(screen.getByRole("button", { name: /dark mode/i })).toHaveAttribute("aria-pressed", "true");
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
  });

  it("is operable from the keyboard", async () => {
    mockSystemTheme(false);
    const user = userEvent.setup();
    render(<ThemeToggle />);
    await user.tab();
    expect(screen.getByRole("button", { name: /dark mode/i })).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    await user.keyboard(" ");
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
  });

  it("follows prefers-color-scheme until the user chooses explicitly", async () => {
    const system = mockSystemTheme(true);
    const user = userEvent.setup();
    render(<ThemeToggle />);
    const button = screen.getByRole("button", { name: /dark mode/i });
    expect(button).toHaveAttribute("aria-pressed", "true");
    expect(document.documentElement.hasAttribute("data-theme")).toBe(false);

    system.change(false);
    expect(button).toHaveAttribute("aria-pressed", "false");

    await user.click(button); // explicit dark while the system is light
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    system.change(true);
    system.change(false);
    expect(button).toHaveAttribute("aria-pressed", "true");
  });

  it("an explicit light choice beats a dark system preference", async () => {
    mockSystemTheme(true);
    const user = userEvent.setup();
    render(<ThemeToggle />);
    await user.click(screen.getByRole("button", { name: /dark mode/i }));
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe("light");
  });

  it("still switches, and says so, when localStorage throws", async () => {
    mockSystemTheme(false);
    const denied = () => {
      throw new DOMException("denied", "SecurityError");
    };
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(denied);
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(denied);
    vi.spyOn(Storage.prototype, "removeItem").mockImplementation(denied);

    const user = userEvent.setup();
    render(<ThemeToggle />);
    const button = screen.getByRole("button", { name: /dark mode/i });
    expect(button).toHaveAttribute("aria-pressed", "false");
    await user.click(button);
    expect(button).toHaveAttribute("aria-pressed", "true");
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    expect(button).toHaveTextContent("this session");
  });

  it("works when window.localStorage cannot even be read", async () => {
    mockSystemTheme(false);
    vi.spyOn(window, "localStorage", "get").mockImplementation(() => {
      throw new DOMException("blocked", "SecurityError");
    });
    const user = userEvent.setup();
    render(<ThemeToggle />);
    await user.click(screen.getByRole("button", { name: /dark mode/i }));
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
  });

  it("works when matchMedia is missing", async () => {
    window.matchMedia = undefined as unknown as typeof window.matchMedia;
    const user = userEvent.setup();
    render(<ThemeToggle />);
    const button = screen.getByRole("button", { name: /dark mode/i });
    expect(button).toHaveAttribute("aria-pressed", "false");
    await user.click(button);
    expect(button).toHaveAttribute("aria-pressed", "true");
  });
});
